//! `.ibt` format adapter.
//!
//! Wraps `ost_adapters::ibt_parser::IbtFile` and emits the streaming
//! NDJSON wire format. The flow is:
//!
//! 1. Open the file → header, var defs, total samples, session info.
//! 2. Build the lap index and track outline (delegated to `IbtFile`).
//! 3. Discover the channel set by walking every Nth sample (see
//!    [`CHANNEL_DISCOVERY_STRIDE`]) and unioning the keys, plus tracking
//!    which channels are numeric (for dense carry-forward).
//! 4. Emit the [`SessionHeader`] line.
//! 5. Stream frames in batches via `read_samples_range`, walking each
//!    sample's `TelemetryFrame` with [`crate::flatten::flatten_frame`]
//!    and writing one JSON-object line per frame.

use std::io::Write;
use std::path::Path;
use std::sync::Arc;

use arrow::array::{ArrayRef, Float32Array, Float64Array, StringArray};
use arrow::datatypes::{DataType, Field, Schema};
use arrow::ipc::writer::FileWriter;
use arrow::record_batch::RecordBatch;
use ost_adapters::ibt_parser::IbtFile;
use serde_json::{Map, Value};

use crate::flatten::{flatten_frame, is_numeric};
use crate::formats::{FrameMode, ParseError, ParseOptions, ReplayParser};
use crate::wire::{
    compute_replay_id, LapInfo, QualifyResult, ReplayMetadata, Roster, RosterEntry, SessionHeader,
};

/// How often we sample frames during channel discovery. Walking every
/// 100th sample catches conditional channels (pit-only vars, off-track
/// flags) cheaply without doing a second full pass.
const CHANNEL_DISCOVERY_STRIDE: usize = 100;

/// Internal batch size for `IbtFile::read_samples_range`. Trades RAM for
/// fewer disk reads. ~1000 frames * ~1KB/frame ≈ 1 MB resident per
/// in-flight batch.
const FRAME_BATCH_SIZE: usize = 1000;

/// Rows per Arrow `RecordBatch` in Feather output. Bounds peak builder
/// memory (rows * channels * 4 bytes) and lets the IPC writer flush
/// incrementally rather than buffering the whole session.
const FEATHER_BATCH_SIZE: usize = 8192;

/// Frame interval between `@progress` stderr lines in Feather mode (when
/// `ParseOptions::progress` is set). Matches the NDJSON consumer's cadence.
const FEATHER_PROGRESS_INTERVAL: usize = 1000;

/// Channels emitted as `Float64` rather than `Float32`.
///
/// An IBT stores exactly five variables as doubles — `Lat`, `Lon`,
/// `SessionTime`, `SessionTimeRemain` and `SessionTimeTotal` — and iRacing
/// chose that width because f32 cannot hold them. They are absolute
/// quantities with small meaningful increments, which is the case f32 is
/// worst at: the mantissa is spent on the magnitude, leaving nothing for the
/// detail.
///
/// The session clock is here for the same reason the coordinates are, one
/// magnitude down: it climbs to 86,400 in a twenty-four hour race, where
/// consecutive f32 values sit 7.8ms apart. It is carried through the model as
/// `SessionSeconds(f64)` rather than the `Seconds(f32)` used for lap times and
/// deltas, which are small enough that f32 still resolves microseconds.
///
/// `SessionTimeTotal` is the IBT's fifth double and is not here: it has no
/// channel of its own in the frame model, so there is no column to widen.
///
/// Measured on a real session at The Bend (latitude 35.3, longitude 139.5),
/// rounding each to the nearest f32 costs:
///
/// - longitude  7.6e-6 deg  =  85 cm
/// - latitude   1.9e-6 deg  =  21 cm
/// - session time            0.03 ms over 790 s, and it grows with the clock
///
/// At 60Hz a car at racing speed covers about half a metre per frame, so an
/// 85cm error is larger than the step between samples: the recorded path
/// becomes a staircase, and half the frames repeat the previous position
/// exactly. Zoomed to a whole lap that is sub-pixel. Zoomed to one corner it
/// is the shape of the line.
///
/// Every other IBT variable is natively f32 — `Speed`, `Alt`, `LapDist`, the
/// pedals, the g-forces — so widening those would double the bytes to store
/// the same numbers.
const WIDE_CHANNELS: &[&str] = &[
    "motion.latitude",
    "motion.longitude",
    "session.session_time",
    "session.session_time_remaining",
    "session.session_time_of_day",
];

/// Everything needed to emit frames after the up-front scans: the open
/// file handle, the assembled [`SessionHeader`], and the discovered
/// channel sets (full union + numeric subset).
struct SessionPrep {
    ibt: IbtFile,
    header: SessionHeader,
    all_channels: Vec<String>,
    numeric_channels: Vec<String>,
}

/// Open the file and run the up-front work shared by every output path:
/// metadata, lap index, track outline, and channel discovery. The
/// returned `header.channels` is the full union (numeric AND string) in
/// stable discovery order — the positional index used by compact frames
/// and the column order used by Feather.
fn prepare_session(path: &Path, options: &ParseOptions) -> Result<SessionPrep, ParseError> {
    if !path.exists() {
        return Err(ParseError::NotFound(path.display().to_string()));
    }

    let mut ibt =
        IbtFile::open(path).map_err(|e| ParseError::Parse(format!("IbtFile::open: {e}")))?;

    let total_frames = ibt.record_count();
    let tick_rate = ibt.tick_rate() as f64;
    let duration_secs = ibt.duration_secs();
    let file_size = ibt.file_size();
    let track_name = ibt.session_info().track_display_name.clone();
    let car_name = ibt.session_info().car_name.clone();
    let replay_id = compute_replay_id(file_size, total_frames, &track_name, &car_name);

    // Both blocks come from the session YAML, which is read when the file is
    // opened, so this costs nothing extra even in stream mode. Empty means the
    // file did not carry them — far and away the common case for qualifying.
    let roster = {
        let entries: Vec<RosterEntry> =
            ibt.session_info().drivers.iter().map(Into::into).collect();
        (!entries.is_empty()).then(|| Roster {
            driver_car_idx: ibt.session_info().driver_car_idx,
            entries,
        })
    };
    let qualifying: Option<Vec<QualifyResult>> = {
        let results: Vec<QualifyResult> = ibt
            .session_info()
            .qualify_results
            .iter()
            .map(Into::into)
            .collect();
        (!results.is_empty()).then_some(results)
    };

    // In stream mode, skip full-file scans (lap index, track outline)
    // and derive channels from the first frame instead of sampling
    // every Nth frame.
    let (laps, track_outline, all_channels, numeric_channels) = if options.stream {
        let (all_ch, num_ch) = if total_frames > 0 {
            discover_channels_from_frame(&ibt, 0)?
        } else {
            (Vec::new(), Vec::new())
        };
        (None, None, all_ch, num_ch)
    } else {
        let laps_raw = ibt.build_lap_index().unwrap_or_default();
        let outline = ibt.build_track_outline(&laps_raw).unwrap_or_else(|e| {
            eprintln!("warning: build_track_outline failed: {e}");
            Vec::new()
        });
        let laps: Vec<LapInfo> = laps_raw.iter().map(LapInfo::from).collect();
        let (all_ch, num_ch) = discover_channels(&ibt, total_frames)?;
        (Some(laps), Some(outline), all_ch, num_ch)
    };

    let header = SessionHeader {
        format: "ost-parse".to_string(),
        version: 1,
        source_format: "ibt".to_string(),
        mode: options.mode.as_wire_str().to_string(),
        metadata: ReplayMetadata {
            track_name,
            car_name,
            tick_rate,
            duration_secs,
            file_size,
            replay_id,
        },
        laps,
        track_outline,
        channels: all_channels.clone(),
        total_frames: total_frames as u64,
        roster,
        qualifying,
    };

    Ok(SessionPrep {
        ibt,
        header,
        all_channels,
        numeric_channels,
    })
}

pub struct IbtReplayParser;

impl ReplayParser for IbtReplayParser {
    fn parse_to_ndjson(
        &self,
        path: &Path,
        writer: &mut dyn Write,
        options: &ParseOptions,
    ) -> Result<(), ParseError> {
        let SessionPrep {
            ibt,
            header,
            all_channels,
            numeric_channels,
        } = prepare_session(path, options)?;
        let total_frames = header.total_frames as usize;

        // Header line.
        serde_json::to_writer(&mut *writer, &header)
            .map_err(|e| ParseError::Parse(format!("serialize header: {e}")))?;
        writer.write_all(b"\n")?;

        // Frame stream. We reuse a single Map allocation across frames.
        let mut frame_obj = Map::with_capacity(all_channels.len().max(64));

        // Dense-mode carry-forward state: previous emitted value for
        // every numeric channel. Initialised to 0 so the first frame's
        // missing channels still serialize. We mutate this in-place per
        // frame and roll back any temporarily-added string channels
        // afterwards, so the per-frame cost is just the keys touched.
        let mut dense_state: Map<String, Value> = if options.mode == FrameMode::Dense {
            let mut m = Map::with_capacity(numeric_channels.len());
            for ch in &numeric_channels {
                m.insert(ch.clone(), Value::from(0));
            }
            m
        } else {
            Map::new()
        };
        // Reused buffer of string-keys we added to dense_state this
        // frame so we can remove them after emit.
        let mut transient_string_keys: Vec<String> = Vec::new();

        // Compact-mode state: positional Vec<Value> aligned with
        // `all_channels` (numeric AND string), plus a name→index lookup
        // table for fast per-frame updates. Every column carries forward:
        // numeric columns start at 0, string columns start at null until
        // first seen.
        let (mut compact_state, compact_index): (
            Vec<Value>,
            std::collections::HashMap<String, usize>,
        ) = if options.mode == FrameMode::Compact {
            let numeric_set: std::collections::HashSet<&String> = numeric_channels.iter().collect();
            let row: Vec<Value> = all_channels
                .iter()
                .map(|ch| {
                    if numeric_set.contains(ch) {
                        Value::from(0)
                    } else {
                        Value::Null
                    }
                })
                .collect();
            let idx = all_channels
                .iter()
                .enumerate()
                .map(|(i, k)| (k.clone(), i))
                .collect();
            (row, idx)
        } else {
            (Vec::new(), std::collections::HashMap::new())
        };

        let mut start = 0;
        while start < total_frames {
            let count = FRAME_BATCH_SIZE.min(total_frames - start);
            let samples = ibt
                .read_samples_range(start, count)
                .map_err(|e| ParseError::Parse(format!("read_samples_range: {e}")))?;

            for sample in &samples {
                let frame = ibt.sample_to_frame(sample);
                let value = serde_json::to_value(&frame)
                    .map_err(|e| ParseError::Parse(format!("serialize frame: {e}")))?;

                frame_obj.clear();
                flatten_frame(&value, &mut frame_obj);

                match options.mode {
                    FrameMode::Sparse => {
                        serde_json::to_writer(&mut *writer, &frame_obj)
                            .map_err(|e| ParseError::Parse(format!("serialize frame: {e}")))?;
                    }
                    FrameMode::Dense => {
                        // Update numeric carry-forward in place; stage
                        // string channels temporarily so the emitted
                        // object is dense_state ∪ this frame's strings.
                        transient_string_keys.clear();
                        for (k, v) in frame_obj.iter() {
                            if is_numeric(v) {
                                if dense_state.contains_key(k) {
                                    dense_state.insert(k.clone(), v.clone());
                                }
                            } else {
                                dense_state.insert(k.clone(), v.clone());
                                transient_string_keys.push(k.clone());
                            }
                        }
                        serde_json::to_writer(&mut *writer, &dense_state)
                            .map_err(|e| ParseError::Parse(format!("serialize frame: {e}")))?;
                        for k in transient_string_keys.drain(..) {
                            dense_state.remove(&k);
                        }
                    }
                    FrameMode::Compact => {
                        // Positional carry-forward across all columns: walk
                        // frame_obj and update slots indexed by
                        // compact_index. Both numeric and string values are
                        // carried forward; a JSON array holds them side by
                        // side.
                        for (k, v) in frame_obj.iter() {
                            if let Some(&i) = compact_index.get(k) {
                                compact_state[i] = v.clone();
                            }
                        }
                        serde_json::to_writer(&mut *writer, &compact_state)
                            .map_err(|e| ParseError::Parse(format!("serialize frame: {e}")))?;
                    }
                }
                writer.write_all(b"\n")?;
            }

            start += samples.len();
            if samples.is_empty() {
                break; // safety: avoid infinite loop on misbehaving reader
            }
        }

        writer.flush()?;
        Ok(())
    }

    fn parse_to_feather(
        &self,
        path: &Path,
        writer: &mut dyn Write,
        options: &ParseOptions,
    ) -> Result<(), ParseError> {
        let SessionPrep {
            ibt,
            mut header,
            all_channels,
            numeric_channels,
        } = prepare_session(path, options)?;
        // Mode is meaningless for a columnar table; label it for clarity.
        header.mode = "feather".to_string();
        let total_frames = header.total_frames as usize;

        // Per-column type: numeric → Float32 (or Float64, see WIDE_CHANNELS),
        // everything else → Utf8.
        let numeric_set: std::collections::HashSet<&String> = numeric_channels.iter().collect();
        let col_is_numeric: Vec<bool> = all_channels
            .iter()
            .map(|c| numeric_set.contains(c))
            .collect();
        let col_is_wide: Vec<bool> = all_channels
            .iter()
            .map(|c| WIDE_CHANNELS.contains(&c.as_str()))
            .collect();
        // name → column index, for fast per-frame updates.
        let col_index: std::collections::HashMap<&str, usize> = all_channels
            .iter()
            .enumerate()
            .map(|(i, c)| (c.as_str(), i))
            .collect();
        // Column index of the lap-number channel, for the `current_lap` field
        // of progress lines (carried-forward in num_state like any other).
        let lap_idx = col_index.get("timing.lap_number").copied();

        // Arrow schema: one field per channel + the full SessionHeader
        // JSON-encoded in schema metadata, so the Feather file is
        // self-describing (laps, outline, tick_rate, channel order …).
        let fields: Vec<Field> = all_channels
            .iter()
            .zip(&col_is_numeric)
            .map(|(name, &is_num)| {
                let dt = if is_num {
                    if WIDE_CHANNELS.contains(&name.as_str()) {
                        DataType::Float64
                    } else {
                        DataType::Float32
                    }
                } else {
                    DataType::Utf8
                };
                Field::new(name, dt, true)
            })
            .collect();
        let mut metadata = std::collections::HashMap::new();
        metadata.insert(
            "ost_header".to_string(),
            serde_json::to_string(&header)
                .map_err(|e| ParseError::Parse(format!("serialize header: {e}")))?,
        );
        let schema = Arc::new(Schema::new(fields).with_metadata(metadata));

        let mut fw = FileWriter::try_new(&mut *writer, &schema)
            .map_err(|e| ParseError::Parse(format!("arrow FileWriter: {e}")))?;

        // Running carry-forward state, one slot per column. Numeric slots
        // start at 0; string slots start null until first seen. Each frame
        // updates only the columns present, then appends the full row.
        let mut num_state: Vec<f64> = vec![0.0; all_channels.len()];
        let mut str_state: Vec<Option<String>> = vec![None; all_channels.len()];

        // Per-batch column builders, reset after each flush.
        let mut num_builders: Vec<Vec<f64>> = all_channels
            .iter()
            .map(|_| Vec::with_capacity(FEATHER_BATCH_SIZE))
            .collect();
        let mut str_builders: Vec<Vec<Option<String>>> = all_channels
            .iter()
            .map(|_| Vec::with_capacity(FEATHER_BATCH_SIZE))
            .collect();
        let mut rows_in_batch = 0usize;
        let mut frames_done = 0usize;

        let mut frame_obj = Map::with_capacity(all_channels.len().max(64));

        let mut start = 0;
        while start < total_frames {
            let count = FRAME_BATCH_SIZE.min(total_frames - start);
            let samples = ibt
                .read_samples_range(start, count)
                .map_err(|e| ParseError::Parse(format!("read_samples_range: {e}")))?;

            for sample in &samples {
                let frame = ibt.sample_to_frame(sample);
                let value = serde_json::to_value(&frame)
                    .map_err(|e| ParseError::Parse(format!("serialize frame: {e}")))?;
                frame_obj.clear();
                flatten_frame(&value, &mut frame_obj);

                // Update carry-forward state for columns present this frame.
                for (k, v) in frame_obj.iter() {
                    if let Some(&i) = col_index.get(k.as_str()) {
                        if col_is_numeric[i] {
                            if let Some(n) = v.as_f64() {
                                num_state[i] = n;
                            }
                        } else if let Some(s) = v.as_str() {
                            str_state[i] = Some(s.to_string());
                        }
                    }
                }

                // Append the (carried-forward) row to the batch builders.
                for i in 0..all_channels.len() {
                    if col_is_numeric[i] {
                        num_builders[i].push(num_state[i]);
                    } else {
                        str_builders[i].push(str_state[i].clone());
                    }
                }
                rows_in_batch += 1;
                frames_done += 1;

                if options.progress && frames_done.is_multiple_of(FEATHER_PROGRESS_INTERVAL) {
                    let cur_lap = lap_idx.map(|i| num_state[i].round() as i64).unwrap_or(0);
                    eprintln!(
                        "@progress {frames_done} {total_frames} {} {cur_lap}",
                        all_channels.len()
                    );
                }

                if rows_in_batch >= FEATHER_BATCH_SIZE {
                    flush_feather_batch(
                        &mut fw,
                        &schema,
                        &col_is_numeric,
                        &col_is_wide,
                        &mut num_builders,
                        &mut str_builders,
                    )?;
                    rows_in_batch = 0;
                }
            }

            start += samples.len();
            if samples.is_empty() {
                break; // safety: avoid infinite loop on misbehaving reader
            }
        }

        // Final progress line so the consumer always sees the true total
        // (the last partial 1000-frame chunk would otherwise be missed).
        if options.progress && frames_done > 0 {
            let cur_lap = lap_idx.map(|i| num_state[i].round() as i64).unwrap_or(0);
            eprintln!(
                "@progress {frames_done} {total_frames} {} {cur_lap}",
                all_channels.len()
            );
        }

        if rows_in_batch > 0 {
            flush_feather_batch(
                &mut fw,
                &schema,
                &col_is_numeric,
                &col_is_wide,
                &mut num_builders,
                &mut str_builders,
            )?;
        }

        fw.finish()
            .map_err(|e| ParseError::Parse(format!("arrow finish: {e}")))?;
        drop(fw);
        writer.flush()?;
        Ok(())
    }
}

/// Build one Arrow `RecordBatch` from the per-column builders and write it
/// to the Feather stream, draining the builders for reuse.
fn flush_feather_batch<W: Write>(
    fw: &mut FileWriter<W>,
    schema: &Arc<Schema>,
    col_is_numeric: &[bool],
    col_is_wide: &[bool],
    num_builders: &mut [Vec<f64>],
    str_builders: &mut [Vec<Option<String>>],
) -> Result<(), ParseError> {
    let columns: Vec<ArrayRef> = (0..col_is_numeric.len())
        .map(|i| {
            if col_is_numeric[i] {
                let vals = std::mem::take(&mut num_builders[i]);
                if col_is_wide[i] {
                    Arc::new(Float64Array::from(vals)) as ArrayRef
                } else {
                    Arc::new(Float32Array::from(
                        vals.into_iter().map(|v| v as f32).collect::<Vec<f32>>(),
                    )) as ArrayRef
                }
            } else {
                let vals = std::mem::take(&mut str_builders[i]);
                Arc::new(StringArray::from_iter(vals)) as ArrayRef
            }
        })
        .collect();
    let batch = RecordBatch::try_new(schema.clone(), columns)
        .map_err(|e| ParseError::Parse(format!("arrow RecordBatch: {e}")))?;
    fw.write(&batch)
        .map_err(|e| ParseError::Parse(format!("arrow write batch: {e}")))?;
    Ok(())
}

/// Walk every `CHANNEL_DISCOVERY_STRIDE`-th sample, union the keys, and
/// classify each channel as numeric or non-numeric. Returns
/// `(sorted_channel_list, sorted_numeric_subset)`.
fn discover_channels(
    ibt: &IbtFile,
    total_frames: usize,
) -> Result<(Vec<String>, Vec<String>), ParseError> {
    use std::collections::{BTreeMap, BTreeSet};

    if total_frames == 0 {
        return Ok((Vec::new(), Vec::new()));
    }

    let mut all: BTreeSet<String> = BTreeSet::new();
    // Use a BTreeMap so the type-classification is deterministic.
    // Once a channel is seen as non-numeric anywhere, it stays that way.
    let mut numeric: BTreeMap<String, bool> = BTreeMap::new();

    let mut frame_obj = Map::new();
    let mut idx = 0;
    while idx < total_frames {
        // read_samples_range(idx, 1) → exactly one sample, positional
        // read, no cursor mutation.
        let samples = ibt
            .read_samples_range(idx, 1)
            .map_err(|e| ParseError::Parse(format!("discover_channels read: {e}")))?;
        if let Some(sample) = samples.first() {
            let frame = ibt.sample_to_frame(sample);
            let value = serde_json::to_value(&frame)
                .map_err(|e| ParseError::Parse(format!("discover_channels serialize: {e}")))?;
            frame_obj.clear();
            flatten_frame(&value, &mut frame_obj);
            for (k, v) in frame_obj.iter() {
                all.insert(k.clone());
                let is_num = is_numeric(v);
                // Once-string-always-string: if any sample shows this
                // channel as non-numeric, downgrade it.
                numeric
                    .entry(k.clone())
                    .and_modify(|prev| *prev = *prev && is_num)
                    .or_insert(is_num);
            }
        }
        idx += CHANNEL_DISCOVERY_STRIDE;
    }

    // Always include the very last frame to catch end-of-session channels.
    let last = total_frames - 1;
    if !last.is_multiple_of(CHANNEL_DISCOVERY_STRIDE) {
        let samples = ibt
            .read_samples_range(last, 1)
            .map_err(|e| ParseError::Parse(format!("discover_channels read: {e}")))?;
        if let Some(sample) = samples.first() {
            let frame = ibt.sample_to_frame(sample);
            let value = serde_json::to_value(&frame)
                .map_err(|e| ParseError::Parse(format!("discover_channels serialize: {e}")))?;
            frame_obj.clear();
            flatten_frame(&value, &mut frame_obj);
            for (k, v) in frame_obj.iter() {
                all.insert(k.clone());
                let is_num = is_numeric(v);
                numeric
                    .entry(k.clone())
                    .and_modify(|prev| *prev = *prev && is_num)
                    .or_insert(is_num);
            }
        }
    }

    let channels: Vec<String> = all.into_iter().collect();
    let numeric_channels: Vec<String> = numeric
        .into_iter()
        .filter_map(|(k, is_num)| if is_num { Some(k) } else { None })
        .collect();
    Ok((channels, numeric_channels))
}

/// Discover channels from a single frame. Used in stream mode to avoid
/// scanning the full file. The IBT var header table declares all
/// variables upfront, so the first frame's channel set is nearly
/// identical to the full union — only truly conditional extras (e.g.
/// pit-only vars) might be missing.
fn discover_channels_from_frame(
    ibt: &IbtFile,
    frame_idx: usize,
) -> Result<(Vec<String>, Vec<String>), ParseError> {
    use std::collections::BTreeMap;

    let samples = ibt
        .read_samples_range(frame_idx, 1)
        .map_err(|e| ParseError::Parse(format!("discover_channels_from_frame: {e}")))?;
    let sample = match samples.first() {
        Some(s) => s,
        None => return Ok((Vec::new(), Vec::new())),
    };

    let frame = ibt.sample_to_frame(sample);
    let value = serde_json::to_value(&frame)
        .map_err(|e| ParseError::Parse(format!("discover_channels_from_frame: {e}")))?;
    let mut frame_obj = Map::new();
    flatten_frame(&value, &mut frame_obj);

    let mut numeric: BTreeMap<String, bool> = BTreeMap::new();
    for (k, v) in frame_obj.iter() {
        numeric.insert(k.clone(), is_numeric(v));
    }

    let all: Vec<String> = numeric.keys().cloned().collect();
    let num: Vec<String> = numeric
        .into_iter()
        .filter_map(|(k, is_num)| if is_num { Some(k) } else { None })
        .collect();
    Ok((all, num))
}
