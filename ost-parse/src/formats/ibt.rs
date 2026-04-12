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

use ost_adapters::ibt_parser::IbtFile;
use serde_json::{Map, Value};

use crate::flatten::{flatten_frame, is_numeric};
use crate::formats::{FrameMode, ParseError, ParseOptions, ReplayParser};
use crate::wire::{compute_replay_id, LapInfo, ReplayMetadata, SessionHeader};

/// How often we sample frames during channel discovery. Walking every
/// 100th sample catches conditional channels (pit-only vars, off-track
/// flags) cheaply without doing a second full pass.
const CHANNEL_DISCOVERY_STRIDE: usize = 100;

/// Internal batch size for `IbtFile::read_samples_range`. Trades RAM for
/// fewer disk reads. ~1000 frames * ~1KB/frame ≈ 1 MB resident per
/// in-flight batch.
const FRAME_BATCH_SIZE: usize = 1000;

pub struct IbtReplayParser;

impl ReplayParser for IbtReplayParser {
    fn parse_to_ndjson(
        &self,
        path: &Path,
        writer: &mut dyn Write,
        options: &ParseOptions,
    ) -> Result<(), ParseError> {
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

        // In compact mode the per-frame array is positional, so the
        // header's `channels` list IS the column index — and it must be
        // numeric-only because positional arrays can't carry strings.
        // In sparse / dense mode `channels` is the full union.
        let header_channels = match options.mode {
            FrameMode::Compact => numeric_channels.clone(),
            _ => all_channels.clone(),
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
            channels: header_channels,
            total_frames: total_frames as u64,
        };

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
        // `numeric_channels`, plus a name→index lookup table for fast
        // per-frame updates. Same carry-forward semantics as dense.
        let (mut compact_state, compact_index): (
            Vec<Value>,
            std::collections::HashMap<String, usize>,
        ) = if options.mode == FrameMode::Compact {
            let row = vec![Value::from(0); numeric_channels.len()];
            let idx = numeric_channels
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
                        // Positional carry-forward: walk frame_obj and
                        // update slots indexed by compact_index. Strings
                        // are dropped (they're not in compact_index).
                        for (k, v) in frame_obj.iter() {
                            if is_numeric(v) {
                                if let Some(&i) = compact_index.get(k) {
                                    compact_state[i] = v.clone();
                                }
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
