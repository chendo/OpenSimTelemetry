//! Cross-platform .ibt file parser for iRacing telemetry replay
//!
//! Parses iRacing binary telemetry (.ibt) files and converts samples
//! to TelemetryFrame for replay. Works on all platforms.

use anyhow::{bail, Context, Result};
use chrono::Utc;
use ost_core::{model::*, units::*};
use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
#[cfg(unix)]
use std::os::unix::fs::FileExt;
#[cfg(windows)]
use std::os::windows::fs::FileExt;
use std::path::Path;

// ============================================================================
// Binary format types
// ============================================================================

/// Variable data types in .ibt files
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VarType {
    Char = 0,
    Bool = 1,
    Int = 2,
    BitField = 3,
    Float = 4,
    Double = 5,
}

impl VarType {
    fn from_i32(val: i32) -> Result<Self> {
        match val {
            0 => Ok(VarType::Char),
            1 => Ok(VarType::Bool),
            2 => Ok(VarType::Int),
            3 => Ok(VarType::BitField),
            4 => Ok(VarType::Float),
            5 => Ok(VarType::Double),
            _ => bail!("Unknown variable type: {}", val),
        }
    }

    /// Size in bytes for a single element of this type
    fn element_size(&self) -> usize {
        match self {
            VarType::Char => 1,
            VarType::Bool => 1,
            VarType::Int => 4,
            VarType::BitField => 4,
            VarType::Float => 4,
            VarType::Double => 8,
        }
    }
}

/// A parsed variable value from a sample
#[derive(Debug, Clone)]
pub enum VarValue {
    Char(u8),
    Bool(bool),
    Int(i32),
    BitField(u32),
    Float(f32),
    Double(f64),
    CharArray(Vec<u8>),
    IntArray(Vec<i32>),
    FloatArray(Vec<f32>),
    DoubleArray(Vec<f64>),
}

impl VarValue {
    pub fn as_f32(&self) -> Option<f32> {
        match self {
            VarValue::Float(v) => Some(*v),
            VarValue::Double(v) => Some(*v as f32),
            VarValue::Int(v) => Some(*v as f32),
            VarValue::Bool(v) => Some(if *v { 1.0 } else { 0.0 }),
            _ => None,
        }
    }

    pub fn as_f64(&self) -> Option<f64> {
        match self {
            VarValue::Double(v) => Some(*v),
            VarValue::Float(v) => Some(*v as f64),
            VarValue::Int(v) => Some(*v as f64),
            _ => None,
        }
    }

    pub fn as_i32(&self) -> Option<i32> {
        match self {
            VarValue::Int(v) => Some(*v),
            VarValue::BitField(v) => Some(*v as i32),
            VarValue::Float(v) => Some(*v as i32),
            VarValue::Bool(v) => Some(if *v { 1 } else { 0 }),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            VarValue::Bool(v) => Some(*v),
            VarValue::Int(v) => Some(*v != 0),
            _ => None,
        }
    }

    pub fn as_u32(&self) -> Option<u32> {
        match self {
            VarValue::BitField(v) => Some(*v),
            VarValue::Int(v) => Some(*v as u32),
            _ => None,
        }
    }
}

/// Lap boundary info for replay seeking
#[derive(Debug, Clone, serde::Serialize)]
pub struct LapInfo {
    pub lap_number: i32,
    /// 0-based index within the same lap number, increments on each reset
    pub lap_index: u32,
    pub start_frame: usize,
    pub lap_time_secs: Option<f64>,
    /// True if the lap was interrupted by a reset checkpoint
    pub incomplete: bool,
    /// Reason the lap is invalid (e.g., "Off track"), None if valid
    pub invalid_reason: Option<String>,
}

/// Main .ibt file header (48 bytes at offset 0)
#[derive(Debug, Clone)]
pub struct IbtHeader {
    pub ver: i32,
    pub status: i32,
    pub tick_rate: i32,
    pub session_info_update: i32,
    pub session_info_len: i32,
    pub session_info_offset: i32,
    pub num_vars: i32,
    pub var_header_offset: i32,
    pub num_buf: i32,
    pub buf_len: i32,
}

/// Variable buffer descriptor (one of 4, 16 bytes each)
#[derive(Debug, Clone)]
pub struct VarBuf {
    pub tick_count: i32,
    pub buf_offset: i32,
}

/// Disk sub-header (32 bytes at offset 112)
#[derive(Debug, Clone)]
pub struct DiskSubHeader {
    pub session_start_date: i64,
    pub session_start_time: f64,
    pub session_end_time: f64,
    pub session_lap_count: i32,
    pub session_record_count: i32,
}

/// A single variable header (144 bytes each)
#[derive(Debug, Clone)]
pub struct VarHeader {
    pub var_type: VarType,
    pub offset: i32,
    pub count: i32,
    pub count_as_time: bool,
    pub name: String,
    pub desc: String,
    pub unit: String,
}

// ============================================================================
// Session info parsed from YAML
// ============================================================================

/// Key session info extracted from the YAML string in the .ibt file
#[derive(Debug, Clone, Default)]
pub struct IbtSessionInfo {
    pub track_name: String,
    pub track_display_name: String,
    pub track_config_name: String,
    pub track_length: String,
    pub car_name: String,
    pub car_screen_name: String,
    pub driver_name: String,
    pub driver_car_idx: i32,
    pub session_type: String,
}

impl IbtSessionInfo {
    /// Parse session info from the YAML string.
    /// Uses simple line-based parsing to avoid adding a YAML dependency.
    pub fn from_yaml(yaml: &str) -> Result<Self> {
        let mut info = IbtSessionInfo::default();

        for line in yaml.lines() {
            let trimmed = line.trim();

            if let Some(val) = try_extract_yaml_value(trimmed, "TrackName:") {
                info.track_name = val;
            } else if let Some(val) = try_extract_yaml_value(trimmed, "TrackDisplayName:") {
                info.track_display_name = val;
            } else if let Some(val) = try_extract_yaml_value(trimmed, "TrackConfigName:") {
                info.track_config_name = val;
            } else if let Some(val) = try_extract_yaml_value(trimmed, "TrackLength:") {
                info.track_length = val;
            } else if let Some(val) = try_extract_yaml_value(trimmed, "CarScreenName:") {
                if info.car_screen_name.is_empty() {
                    info.car_screen_name = val;
                }
            } else if let Some(val) = try_extract_yaml_value(trimmed, "UserName:") {
                if info.driver_name.is_empty() {
                    info.driver_name = val;
                }
            } else if let Some(val) = try_extract_yaml_value(trimmed, "DriverCarIdx:") {
                if let Ok(idx) = val.parse::<i32>() {
                    info.driver_car_idx = idx;
                }
            } else if let Some(val) = try_extract_yaml_value(trimmed, "SessionType:") {
                if info.session_type.is_empty() {
                    info.session_type = val;
                }
            }
        }

        if info.track_display_name.is_empty() {
            info.track_display_name = info.track_name.clone();
        }

        info.car_name = info.car_screen_name.clone();

        Ok(info)
    }
}

fn try_extract_yaml_value(line: &str, key: &str) -> Option<String> {
    line.strip_prefix(key).map(|rest| rest.trim().to_string())
}

// ============================================================================
// IbtFile: main parser
// ============================================================================

/// Parsed .ibt file handle for reading telemetry samples
pub struct IbtFile {
    file: File,
    pub header: IbtHeader,
    pub disk_sub_header: DiskSubHeader,
    pub var_headers: Vec<VarHeader>,
    pub session_info_yaml: String,
    pub session_info: IbtSessionInfo,
    sample_data_offset: u64,
    file_size: u64,
    #[allow(dead_code)]
    var_index: HashMap<String, usize>,
}

impl IbtFile {
    /// Positional read: reads `buf.len()` bytes at the given offset without
    /// mutating the file cursor. Uses pread on Unix and seek_read on Windows.
    #[cfg(unix)]
    fn read_at(&self, buf: &mut [u8], offset: u64) -> Result<()> {
        self.file
            .read_exact_at(buf, offset)
            .context("positional read failed")?;
        Ok(())
    }

    #[cfg(windows)]
    fn read_at(&self, buf: &mut [u8], offset: u64) -> Result<()> {
        let mut pos = 0;
        while pos < buf.len() {
            let n = self
                .file
                .seek_read(&mut buf[pos..], offset + pos as u64)
                .context("positional read failed")?;
            if n == 0 {
                bail!("unexpected EOF during positional read");
            }
            pos += n;
        }
        Ok(())
    }

    /// Open and parse an .ibt file from disk.
    /// Reads headers and session info, but does NOT load sample data into memory.
    pub fn open(path: &Path) -> Result<Self> {
        let mut file = File::open(path)
            .with_context(|| format!("Failed to open .ibt file: {}", path.display()))?;

        let file_size = file.metadata()?.len();

        let header = Self::read_header(&mut file)?;

        file.seek(SeekFrom::Start(48))?;
        let var_buf = Self::read_var_buf(&mut file)?;

        file.seek(SeekFrom::Start(112))?;
        let disk_sub_header = Self::read_disk_sub_header(&mut file)?;

        file.seek(SeekFrom::Start(header.var_header_offset as u64))?;
        let var_headers = Self::read_var_headers(&mut file, header.num_vars as usize)?;

        let var_index: HashMap<String, usize> = var_headers
            .iter()
            .enumerate()
            .map(|(i, vh)| (vh.name.clone(), i))
            .collect();

        file.seek(SeekFrom::Start(header.session_info_offset as u64))?;
        let mut yaml_buf = vec![0u8; header.session_info_len as usize];
        file.read_exact(&mut yaml_buf)?;
        let yaml_end = yaml_buf
            .iter()
            .position(|&b| b == 0)
            .unwrap_or(yaml_buf.len());
        let session_info_yaml = String::from_utf8_lossy(&yaml_buf[..yaml_end]).to_string();

        let session_info = IbtSessionInfo::from_yaml(&session_info_yaml).unwrap_or_default();

        let sample_data_offset = var_buf.buf_offset as u64;

        Ok(IbtFile {
            file,
            header,
            disk_sub_header,
            var_headers,
            session_info_yaml,
            session_info,
            sample_data_offset,
            file_size,
            var_index,
        })
    }

    fn read_header(file: &mut File) -> Result<IbtHeader> {
        file.seek(SeekFrom::Start(0))?;
        let mut buf = [0u8; 48];
        file.read_exact(&mut buf)?;

        Ok(IbtHeader {
            ver: i32::from_le_bytes(buf[0..4].try_into()?),
            status: i32::from_le_bytes(buf[4..8].try_into()?),
            tick_rate: i32::from_le_bytes(buf[8..12].try_into()?),
            session_info_update: i32::from_le_bytes(buf[12..16].try_into()?),
            session_info_len: i32::from_le_bytes(buf[16..20].try_into()?),
            session_info_offset: i32::from_le_bytes(buf[20..24].try_into()?),
            num_vars: i32::from_le_bytes(buf[24..28].try_into()?),
            var_header_offset: i32::from_le_bytes(buf[28..32].try_into()?),
            num_buf: i32::from_le_bytes(buf[32..36].try_into()?),
            buf_len: i32::from_le_bytes(buf[36..40].try_into()?),
        })
    }

    fn read_var_buf(file: &mut File) -> Result<VarBuf> {
        let mut buf = [0u8; 16];
        file.read_exact(&mut buf)?;

        Ok(VarBuf {
            tick_count: i32::from_le_bytes(buf[0..4].try_into()?),
            buf_offset: i32::from_le_bytes(buf[4..8].try_into()?),
        })
    }

    fn read_disk_sub_header(file: &mut File) -> Result<DiskSubHeader> {
        let mut buf = [0u8; 32];
        file.read_exact(&mut buf)?;

        Ok(DiskSubHeader {
            session_start_date: i64::from_le_bytes(buf[0..8].try_into()?),
            session_start_time: f64::from_le_bytes(buf[8..16].try_into()?),
            session_end_time: f64::from_le_bytes(buf[16..24].try_into()?),
            session_lap_count: i32::from_le_bytes(buf[24..28].try_into()?),
            session_record_count: i32::from_le_bytes(buf[28..32].try_into()?),
        })
    }

    fn read_var_headers(file: &mut File, count: usize) -> Result<Vec<VarHeader>> {
        let mut headers = Vec::with_capacity(count);

        for i in 0..count {
            let mut buf = [0u8; 144];
            file.read_exact(&mut buf)
                .with_context(|| format!("Failed to read variable header {}", i))?;

            let var_type = VarType::from_i32(i32::from_le_bytes(buf[0..4].try_into()?))?;
            let offset = i32::from_le_bytes(buf[4..8].try_into()?);
            let count = i32::from_le_bytes(buf[8..12].try_into()?);
            let count_as_time = buf[12] != 0;

            let name = read_null_terminated_string(&buf[16..48]);
            let desc = read_null_terminated_string(&buf[48..112]);
            let unit = read_null_terminated_string(&buf[112..144]);

            headers.push(VarHeader {
                var_type,
                offset,
                count,
                count_as_time,
                name,
                desc,
                unit,
            });
        }

        Ok(headers)
    }

    pub fn record_count(&self) -> usize {
        self.disk_sub_header.session_record_count as usize
    }

    pub fn tick_rate(&self) -> u32 {
        self.header.tick_rate as u32
    }

    pub fn duration_secs(&self) -> f64 {
        self.disk_sub_header.session_end_time - self.disk_sub_header.session_start_time
    }

    pub fn session_info_yaml(&self) -> &str {
        &self.session_info_yaml
    }

    pub fn session_info(&self) -> &IbtSessionInfo {
        &self.session_info
    }

    pub fn var_headers_ref(&self) -> &[VarHeader] {
        &self.var_headers
    }

    pub fn header_ref(&self) -> &IbtHeader {
        &self.header
    }

    pub fn sample_data_offset(&self) -> u64 {
        self.sample_data_offset
    }

    pub fn file_size(&self) -> u64 {
        self.file_size
    }

    /// Scan all frames to build a lap index for replay seeking.
    /// Uses GPS proximity (start ≈ end within 10m) plus `LapLastLapTime`
    /// to identify genuinely completed laps vs out-laps and session resets.
    pub fn build_lap_index(&mut self) -> Result<Vec<LapInfo>> {
        let record_count = self.record_count();
        if record_count == 0 {
            return Ok(Vec::new());
        }

        let lap_vh = match self.var_index.get("Lap").map(|&i| &self.var_headers[i]) {
            Some(vh) => vh.clone(),
            None => return Ok(Vec::new()),
        };

        // Optional vars for lap validation
        let last_lap_time_vh = self
            .var_index
            .get("LapLastLapTime")
            .map(|&i| self.var_headers[i].clone());
        let session_time_vh = self
            .var_index
            .get("SessionTime")
            .map(|&i| self.var_headers[i].clone());
        let lap_dist_pct_vh = self
            .var_index
            .get("LapDistPct")
            .map(|&i| self.var_headers[i].clone());
        let on_track_vh = self
            .var_index
            .get("IsOnTrack")
            .map(|&i| self.var_headers[i].clone());

        // Bulk read all sample buffers
        let buf_len = self.header.buf_len as usize;
        let total_bytes = buf_len * record_count;
        self.file.seek(SeekFrom::Start(self.sample_data_offset))?;
        let mut bulk_buf = vec![0u8; total_bytes];
        self.file.read_exact(&mut bulk_buf)?;

        // Helpers to read typed values from a frame buffer
        let read_f32 = |frame_buf: &[u8], vh: &VarHeader| -> Option<f32> {
            let off = vh.offset as usize;
            if off + 4 <= frame_buf.len() {
                Some(f32::from_le_bytes(
                    frame_buf[off..off + 4].try_into().unwrap(),
                ))
            } else {
                None
            }
        };
        let read_f64 = |frame_buf: &[u8], vh: &VarHeader| -> Option<f64> {
            let off = vh.offset as usize;
            match vh.var_type {
                VarType::Double if off + 8 <= frame_buf.len() => Some(f64::from_le_bytes(
                    frame_buf[off..off + 8].try_into().unwrap(),
                )),
                VarType::Float if off + 4 <= frame_buf.len() => {
                    Some(f32::from_le_bytes(frame_buf[off..off + 4].try_into().unwrap()) as f64)
                }
                _ => None,
            }
        };

        // Find lap transitions
        let mut laps: Vec<LapInfo> = Vec::new();
        let mut prev_lap: Option<i32> = None;
        let mut prev_pct: Option<f32> = None;
        let mut sum_pct: f64 = 0.0;
        let mut sum_pct_sq: f64 = 0.0;
        let mut pct_count: usize = 0;
        let mut lap_index: u32 = 0;
        let mut consecutive_off_track: usize = 0;
        let mut went_off_track = false;
        let mut had_pct_discontinuity = false;

        for i in 0..record_count {
            let frame_buf = &bulk_buf[i * buf_len..(i + 1) * buf_len];
            let lap_offset = lap_vh.offset as usize;
            if lap_offset + 4 > frame_buf.len() {
                continue;
            }
            let lap_num =
                i32::from_le_bytes(frame_buf[lap_offset..lap_offset + 4].try_into().unwrap());

            if prev_lap.is_none() || prev_lap != Some(lap_num) {
                // Lap transition detected — validate the PREVIOUS lap
                if !laps.is_empty() {
                    let prev_idx = laps.len() - 1;

                    // A complete lap has avg LapDistPct ~0.5 (car drove the full track).
                    // Partial/reset laps have skewed avg (e.g., 0.17 for resets near S/F).
                    // A complete lap has avg LapDistPct ~0.5 and stddev ~0.289
                    // (uniform distribution from driving the full track).
                    // Partial/reset laps have skewed avg and abnormal stddev.
                    let avg_pct = if pct_count > 0 {
                        sum_pct / pct_count as f64
                    } else {
                        0.0
                    };
                    let std_dev = if pct_count > 1 {
                        let variance = (sum_pct_sq - sum_pct * sum_pct / pct_count as f64)
                            / (pct_count - 1) as f64;
                        variance.max(0.0).sqrt()
                    } else {
                        0.0
                    };
                    let lap_complete =
                        avg_pct > 0.45 && avg_pct < 0.55 && std_dev > 0.25 && std_dev < 0.32;

                    if lap_complete {
                        // Use iRacing's official LapLastLapTime (set at start of new lap)
                        let official_time = last_lap_time_vh
                            .as_ref()
                            .and_then(|vh| read_f32(frame_buf, vh))
                            .filter(|&t| t > 0.0)
                            .map(|t| t as f64);

                        // Verify frame range duration matches the reported lap time.
                        // After session resets, LapLastLapTime can be stale from a
                        // previous session while the frame range is much shorter.
                        let t_end = session_time_vh
                            .as_ref()
                            .and_then(|vh| read_f64(frame_buf, vh));
                        let prev_frame =
                            &bulk_buf[laps[prev_idx].start_frame * buf_len..][..buf_len];
                        let t_start = session_time_vh
                            .as_ref()
                            .and_then(|vh| read_f64(prev_frame, vh));
                        let frame_duration = match (t_start, t_end) {
                            (Some(ts), Some(te)) if te > ts => Some(te - ts),
                            _ => None,
                        };

                        if let Some(t) = official_time {
                            // Only accept if frame duration is within 10% of reported time
                            let duration_ok =
                                frame_duration.is_some_and(|fd| fd > t * 0.9 && fd < t * 1.1);
                            if duration_ok {
                                laps[prev_idx].lap_time_secs = Some(t);
                            }
                        } else if let Some(fd) = frame_duration {
                            // Fallback: use frame duration directly
                            if fd > 0.0 && fd < 3600.0 {
                                laps[prev_idx].lap_time_secs = Some(fd);
                            }
                        }
                    }

                    // Mark invalid laps
                    if went_off_track {
                        laps[prev_idx].invalid_reason = Some("Off track".to_string());
                    } else if had_pct_discontinuity {
                        laps[prev_idx].invalid_reason = Some("Lap % discontinuity".to_string());
                    }
                }

                // Start tracking new lap
                lap_index = 0;
                consecutive_off_track = 0;
                went_off_track = false;
                had_pct_discontinuity = false;
                sum_pct = 0.0;
                sum_pct_sq = 0.0;
                pct_count = 0;
                laps.push(LapInfo {
                    lap_number: lap_num,
                    lap_index: 0,
                    start_frame: i,
                    lap_time_secs: None,
                    incomplete: false,
                    invalid_reason: None,
                });
                prev_lap = Some(lap_num);
                prev_pct = None; // Reset pct tracking on lap transition
            } else {
                // Same lap number — check for reset checkpoint (pct going backwards)
                let current_pct = lap_dist_pct_vh
                    .as_ref()
                    .and_then(|vh| read_f32(frame_buf, vh));
                if let (Some(cur), Some(prev_p)) = (current_pct, prev_pct) {
                    if prev_p - cur > 0.01 && prev_p < 0.99 {
                        // Reset detected — mark current segment as incomplete with elapsed time
                        if let Some(last) = laps.last_mut() {
                            let t_end = session_time_vh
                                .as_ref()
                                .and_then(|vh| read_f64(frame_buf, vh));
                            let prev_frame = &bulk_buf[last.start_frame * buf_len..][..buf_len];
                            let t_start = session_time_vh
                                .as_ref()
                                .and_then(|vh| read_f64(prev_frame, vh));
                            last.lap_time_secs = match (t_start, t_end) {
                                (Some(ts), Some(te)) if te > ts => Some(te - ts),
                                _ => None,
                            };
                            last.incomplete = true;
                        }
                        // Start new segment with incremented lap_index
                        lap_index += 1;
                        consecutive_off_track = 0;
                        went_off_track = false;
                        had_pct_discontinuity = false;
                        sum_pct = 0.0;
                        sum_pct_sq = 0.0;
                        pct_count = 0;
                        laps.push(LapInfo {
                            lap_number: lap_num,
                            lap_index,
                            start_frame: i,
                            lap_time_secs: None,
                            incomplete: false,
                            invalid_reason: None,
                        });
                    }
                }
            }

            // Track lap_distance_pct for reset detection and lap completion (avg pct)
            let current_pct = lap_dist_pct_vh
                .as_ref()
                .and_then(|vh| read_f32(frame_buf, vh));
            if let Some(p) = current_pct {
                // Detect forward pct discontinuity (>1% jump in a single frame = teleport)
                if let Some(pp) = prev_pct {
                    let delta = p - pp;
                    if delta > 0.01 {
                        had_pct_discontinuity = true;
                    }
                }
                let pd = p as f64;
                sum_pct += pd;
                sum_pct_sq += pd * pd;
                pct_count += 1;
                prev_pct = current_pct;
            }

            // Track off-track status (>5 consecutive frames = significant excursion)
            if let Some(ref vh) = on_track_vh {
                let off = vh.offset as usize;
                if off < frame_buf.len() && frame_buf[off] == 0 {
                    consecutive_off_track += 1;
                    if consecutive_off_track > 5 {
                        went_off_track = true;
                    }
                } else {
                    consecutive_off_track = 0;
                }
            }
        }

        Ok(laps)
    }

    /// Extract the track outline as lat/lng pairs.
    /// Uses the fastest complete lap when available for a clean single-lap outline.
    /// Falls back to binning all frames by `LapDistPct` to handle resets and
    /// incomplete laps without teleport lines.
    /// Only includes points where the car is on-track (`IsOnTrack == true`).
    pub fn build_track_outline(&mut self, laps: &[LapInfo]) -> Result<Vec<[f64; 2]>> {
        let record_count = self.record_count();
        if record_count == 0 {
            return Ok(Vec::new());
        }

        // Find best lap for outline: prefer clean (no off-track), then any complete
        let time_cmp = |a: &&LapInfo, b: &&LapInfo| {
            a.lap_time_secs
                .unwrap()
                .partial_cmp(&b.lap_time_secs.unwrap())
                .unwrap_or(std::cmp::Ordering::Equal)
        };
        let complete_laps: Vec<_> = laps
            .iter()
            .enumerate()
            .filter(|(_i, lap)| lap.lap_time_secs.is_some() && !lap.incomplete)
            .collect();
        // Tier 1: fastest clean lap (no off-track, no invalid reason)
        let best_lap = complete_laps
            .iter()
            .filter(|(_i, lap)| lap.invalid_reason.is_none())
            .min_by(|(_, a), (_, b)| time_cmp(a, b))
            .copied()
            // Tier 2: fastest complete lap (even if off-track)
            .or_else(|| {
                complete_laps
                    .iter()
                    .min_by(|(_, a), (_, b)| time_cmp(a, b))
                    .copied()
            });

        let lat_vh = self.var_index.get("Lat").map(|&i| &self.var_headers[i]);
        let lon_vh = self.var_index.get("Lon").map(|&i| &self.var_headers[i]);
        let (lat_vh, lon_vh) = match (lat_vh, lon_vh) {
            (Some(a), Some(b)) => (a.clone(), b.clone()),
            _ => return Ok(Vec::new()),
        };
        let on_track_vh = self
            .var_index
            .get("IsOnTrack")
            .map(|&i| self.var_headers[i].clone());
        let lap_dist_pct_vh = self
            .var_index
            .get("LapDistPct")
            .map(|&i| self.var_headers[i].clone());

        // Determine frame range and whether to use binning
        let use_binning = best_lap.is_none() && lap_dist_pct_vh.is_some();
        let (start_frame, end_frame) = if let Some((idx, _lap)) = best_lap {
            let start = laps[idx].start_frame;
            let end = if idx + 1 < laps.len() {
                laps[idx + 1].start_frame
            } else {
                record_count
            };
            (start, end)
        } else {
            (0, record_count)
        };

        let buf_len = self.header.buf_len as usize;
        let range_bytes = buf_len * (end_frame - start_frame);
        let seek_offset = self.sample_data_offset + (start_frame * buf_len) as u64;
        self.file.seek(SeekFrom::Start(seek_offset))?;
        let mut bulk_buf = vec![0u8; range_bytes];
        self.file.read_exact(&mut bulk_buf)?;

        let read_f32 = |frame_buf: &[u8], vh: &VarHeader| -> Option<f32> {
            let off = vh.offset as usize;
            if off + 4 <= frame_buf.len() {
                Some(f32::from_le_bytes(
                    frame_buf[off..off + 4].try_into().unwrap(),
                ))
            } else {
                None
            }
        };

        let frame_count = end_frame - start_frame;

        if use_binning {
            // Bin GPS points by LapDistPct — handles resets without teleport lines
            const NUM_BINS: usize = 5000;
            let pct_vh = lap_dist_pct_vh.as_ref().unwrap();
            let mut bins: Vec<Option<[f64; 2]>> = vec![None; NUM_BINS];

            for i in 0..frame_count {
                let frame_buf = &bulk_buf[i * buf_len..(i + 1) * buf_len];

                if let Some(ref vh) = on_track_vh {
                    let off = vh.offset as usize;
                    if off < frame_buf.len() && frame_buf[off] == 0 {
                        continue;
                    }
                }

                let pct = match read_f32(frame_buf, pct_vh) {
                    Some(p) if p > 0.0 && p <= 1.0 => p,
                    _ => continue,
                };

                let lat_off = lat_vh.offset as usize;
                if lat_off + 8 > frame_buf.len() {
                    continue;
                }
                let lat = f64::from_le_bytes(frame_buf[lat_off..lat_off + 8].try_into().unwrap());

                let lon_off = lon_vh.offset as usize;
                if lon_off + 8 > frame_buf.len() {
                    continue;
                }
                let lng = f64::from_le_bytes(frame_buf[lon_off..lon_off + 8].try_into().unwrap());

                if lat == 0.0 && lng == 0.0 {
                    continue;
                }

                let bin = ((pct * NUM_BINS as f32) as usize).min(NUM_BINS - 1);
                bins[bin] = Some([lat, lng]);
            }

            // Collect non-empty bins with MIN_DELTA dedup
            let mut points = Vec::new();
            let mut last_lat = f64::NAN;
            let mut last_lng = f64::NAN;
            const MIN_DELTA: f64 = 0.000005;
            for bin in bins.into_iter().flatten() {
                if (bin[0] - last_lat).abs() < MIN_DELTA && (bin[1] - last_lng).abs() < MIN_DELTA {
                    continue;
                }
                points.push(bin);
                last_lat = bin[0];
                last_lng = bin[1];
            }
            Ok(points)
        } else {
            // Sequential scan of a single complete lap (or all frames if no pct available)
            let mut points = Vec::new();
            let mut last_lat = f64::NAN;
            let mut last_lng = f64::NAN;
            const MIN_DELTA: f64 = 0.000005;

            for i in 0..frame_count {
                let frame_buf = &bulk_buf[i * buf_len..(i + 1) * buf_len];

                if let Some(ref vh) = on_track_vh {
                    let off = vh.offset as usize;
                    if off < frame_buf.len() && frame_buf[off] == 0 {
                        continue;
                    }
                }

                let lat_off = lat_vh.offset as usize;
                if lat_off + 8 > frame_buf.len() {
                    continue;
                }
                let lat = f64::from_le_bytes(frame_buf[lat_off..lat_off + 8].try_into().unwrap());

                let lon_off = lon_vh.offset as usize;
                if lon_off + 8 > frame_buf.len() {
                    continue;
                }
                let lng = f64::from_le_bytes(frame_buf[lon_off..lon_off + 8].try_into().unwrap());

                if lat == 0.0 && lng == 0.0 {
                    continue;
                }

                if (lat - last_lat).abs() < MIN_DELTA && (lng - last_lng).abs() < MIN_DELTA {
                    continue;
                }

                points.push([lat, lng]);
                last_lat = lat;
                last_lng = lng;
            }
            Ok(points)
        }
    }

    /// Read a contiguous range of samples in a single disk operation.
    /// Much faster than calling `read_sample()` in a loop because it avoids
    /// per-frame seek overhead.
    pub fn read_samples_range(
        &self,
        start: usize,
        count: usize,
    ) -> Result<Vec<HashMap<String, VarValue>>> {
        let record_count = self.record_count();
        if start >= record_count {
            bail!("Start index {} out of range (0..{})", start, record_count);
        }
        let clamped_count = count.min(record_count - start);
        if clamped_count == 0 {
            return Ok(Vec::new());
        }

        let buf_len = self.header.buf_len as usize;
        let offset = self.sample_data_offset + (start as u64) * (buf_len as u64);
        let total_bytes = buf_len * clamped_count;

        // Positional read for the entire range (no seek needed, supports concurrent reads)
        let mut bulk_buf = vec![0u8; total_bytes];
        self.read_at(&mut bulk_buf, offset)?;

        // Parse each frame from the in-memory buffer
        let mut results = Vec::with_capacity(clamped_count);
        for i in 0..clamped_count {
            let frame_buf = &bulk_buf[i * buf_len..(i + 1) * buf_len];
            let mut sample = HashMap::with_capacity(self.var_headers.len());
            for vh in &self.var_headers {
                let var_offset = vh.offset as usize;
                let count = vh.count as usize;
                let end = var_offset + count * vh.var_type.element_size();
                if end > frame_buf.len() {
                    continue;
                }
                let value = if count == 1 {
                    read_scalar_value(frame_buf, var_offset, vh.var_type)
                } else {
                    read_array_value(frame_buf, var_offset, vh.var_type, count)
                };
                if let Some(val) = value {
                    sample.insert(vh.name.clone(), val);
                }
            }
            results.push(sample);
        }
        Ok(results)
    }

    /// Read a single sample by index, returning a HashMap of variable name -> VarValue
    pub fn read_sample(&self, index: usize) -> Result<HashMap<String, VarValue>> {
        let record_count = self.record_count();
        if index >= record_count {
            bail!("Sample index {} out of range (0..{})", index, record_count);
        }

        let buf_len = self.header.buf_len as u64;
        let offset = self.sample_data_offset + (index as u64) * buf_len;

        let mut sample_buf = vec![0u8; buf_len as usize];
        self.read_at(&mut sample_buf, offset)?;

        let mut result = HashMap::with_capacity(self.var_headers.len());

        for vh in &self.var_headers {
            let var_offset = vh.offset as usize;
            let count = vh.count as usize;

            let end = var_offset + count * vh.var_type.element_size();
            if end > sample_buf.len() {
                continue;
            }

            let value = if count == 1 {
                read_scalar_value(&sample_buf, var_offset, vh.var_type)
            } else {
                read_array_value(&sample_buf, var_offset, vh.var_type, count)
            };

            if let Some(val) = value {
                result.insert(vh.name.clone(), val);
            }
        }

        Ok(result)
    }

    /// Convert a VarValue to a serde_json::Value for extras.
    fn var_value_to_json(value: &VarValue) -> serde_json::Value {
        match value {
            VarValue::Char(c) => serde_json::json!(*c),
            VarValue::Bool(b) => serde_json::json!(*b),
            VarValue::Int(i) => serde_json::json!(*i),
            VarValue::BitField(u) => serde_json::json!(*u),
            VarValue::Float(f) => serde_json::json!((*f * 10000.0).round() / 10000.0),
            VarValue::Double(d) => serde_json::json!((*d * 10000.0).round() / 10000.0),
            VarValue::CharArray(v) => {
                let s = String::from_utf8_lossy(v)
                    .trim_end_matches('\0')
                    .to_string();
                serde_json::json!(s)
            }
            VarValue::IntArray(v) => serde_json::json!(v),
            VarValue::FloatArray(v) => {
                let rounded: Vec<f32> = v.iter().map(|x| (x * 10000.0).round() / 10000.0).collect();
                serde_json::json!(rounded)
            }
            VarValue::DoubleArray(v) => {
                let rounded: Vec<f64> = v.iter().map(|x| (x * 10000.0).round() / 10000.0).collect();
                serde_json::json!(rounded)
            }
        }
    }

    /// Convert a raw sample HashMap to a TelemetryFrame.
    /// Mirrors the conversion logic from IRacingAdapter::convert_sample(),
    /// producing the nested sub-struct model.
    pub fn sample_to_frame(&self, sample: &HashMap<String, VarValue>) -> TelemetryFrame {
        let get_f32 = |name: &str| -> Option<f32> { sample.get(name).and_then(|v| v.as_f32()) };
        let get_f64 = |name: &str| -> Option<f64> { sample.get(name).and_then(|v| v.as_f64()) };
        let get_i32 = |name: &str| -> Option<i32> { sample.get(name).and_then(|v| v.as_i32()) };
        let get_u32 = |name: &str| -> Option<u32> { sample.get(name).and_then(|v| v.as_u32()) };
        let get_bool = |name: &str| -> Option<bool> { sample.get(name).and_then(|v| v.as_bool()) };

        let tick = get_i32("SessionTick").map(|t| t as u32);

        // =================================================================
        // Motion
        // =================================================================
        let velocity = match (
            get_f32("VelocityX"),
            get_f32("VelocityY"),
            get_f32("VelocityZ"),
        ) {
            (Some(vx), Some(vy), Some(vz)) => Some(Vector3::new(
                MetersPerSecond(vx),
                MetersPerSecond(vy),
                MetersPerSecond(vz),
            )),
            _ => None,
        };

        let acceleration = match (
            get_f32("LatAccel"),
            get_f32("LongAccel"),
            get_f32("VertAccel"),
        ) {
            (Some(lat), Some(long), Some(vert)) => Some(Vector3::new(
                MetersPerSecondSquared(lat),
                MetersPerSecondSquared(vert),
                MetersPerSecondSquared(long),
            )),
            _ => None,
        };

        let g_force = acceleration.as_ref().map(|a| {
            Vector3::new(
                GForce::from_acceleration(a.x),
                GForce::from_acceleration(a.y),
                GForce::from_acceleration(a.z),
            )
        });

        // Pitch/Yaw/Roll are car orientation in track coordinates (radians).
        // Just convert to degrees — compass bearing is handled by `heading` (from YawNorth).
        let pitch_val = get_f32("Pitch").map(Degrees::from_radians);
        let yaw_val = get_f32("Yaw").map(Degrees::from_radians);
        let roll_val = get_f32("Roll").map(Degrees::from_radians);

        // YawNorth: compass heading in radians (0=north, CW-positive).
        // Just convert to degrees.
        let heading = get_f32("YawNorth").map(|yn| {
            let deg = yn * (180.0 / std::f32::consts::PI);
            Degrees(deg.rem_euclid(360.0))
        });

        let motion = Some(MotionData {
            position: None,
            velocity,
            acceleration,
            g_force,
            pitch: pitch_val,
            roll: roll_val,
            yaw: yaw_val,
            pitch_rate: get_f32("PitchRate").map(DegreesPerSecond::from_radians),
            yaw_rate: get_f32("YawRate").map(DegreesPerSecond::from_radians),
            roll_rate: get_f32("RollRate").map(DegreesPerSecond::from_radians),
            latitude: get_f64("Lat"),
            longitude: get_f64("Lon"),
            altitude: get_f32("Alt").map(Meters),
            heading,
        });

        // =================================================================
        // Vehicle
        // =================================================================
        let speed = get_f32("Speed").map(MetersPerSecond).or_else(|| {
            velocity
                .as_ref()
                .map(|v| MetersPerSecond((v.x.0.powi(2) + v.y.0.powi(2) + v.z.0.powi(2)).sqrt()))
        });

        // PlayerTrackSurface is the irsdk_TrkLoc *location* enum
        // (OffTrack/OnTrack/pits), NOT the surface material — decode it as
        // a location. The raw code is still exposed under
        // `iracing.PlayerTrackSurface`.
        let track_surface =
            get_i32("PlayerTrackSurface").map(crate::iracing::iracing_track_location);

        let vehicle = Some(VehicleData {
            speed,
            rpm: get_f32("RPM").map(Rpm),
            max_rpm: None,
            idle_rpm: None,
            gear: get_i32("Gear").map(|g| g as i8),
            max_gears: None,
            throttle: get_f32("Throttle").map(Percentage::new),
            brake: get_f32("Brake").map(Percentage::new),
            clutch: get_f32("Clutch").map(Percentage::new),
            steering_angle: get_f32("SteeringWheelAngle").map(Degrees::from_radians),
            steering_torque: get_f32("SteeringWheelTorque").map(NewtonMeters),
            steering_torque_pct: get_f32("SteeringWheelPctTorque").map(Percentage::new),
            handbrake: get_f32("HandbrakeRaw").map(Percentage::new),
            shift_indicator: get_f32("ShiftIndicatorPct").map(Percentage::new),
            steering_angle_max: get_f32("SteeringWheelAngleMax").map(Degrees::from_radians),
            on_track: get_bool("IsOnTrack"),
            in_garage: get_bool("IsInGarage"),
            track_surface,
            car_name: Some(self.session_info.car_name.clone()).filter(|s| !s.is_empty()),
            car_class: None,
            setup_name: None,
            abs: get_f32("dcABS"),
            abs_active: get_bool("BrakeABSactive"),
            traction_control: get_f32("dcTractionControl"),
            traction_control_2: None,
            brake_bias: get_f32("dcBrakeBias").map(Percentage::new),
            anti_roll_front: None,
            anti_roll_rear: None,
            drs_status: get_i32("DRS_Status").map(|v| v as u32),
            push_to_pass_status: None,
            push_to_pass_count: None,
            throttle_shape: None,
            shift_light_first_rpm: None,
            shift_light_shift_rpm: None,
            shift_light_last_rpm: None,
            shift_light_blink_rpm: None,
        });

        // =================================================================
        // Engine
        // =================================================================
        let engine_warnings = get_u32("EngineWarnings").map(EngineWarnings::from_iracing_bits);

        let engine = Some(EngineData {
            water_temp: get_f32("WaterTemp").map(Celsius),
            oil_temp: get_f32("OilTemp").map(Celsius),
            oil_pressure: get_f32("OilPress").map(Kilopascals),
            oil_level: get_f32("OilLevel").map(Percentage::new),
            fuel_level: get_f32("FuelLevel").map(Liters),
            fuel_level_pct: get_f32("FuelLevelPct").map(Percentage::new),
            fuel_capacity: None,
            fuel_pressure: get_f32("FuelPress").map(Kilopascals),
            fuel_use_per_hour: get_f32("FuelUsePerHour").map(LitersPerHour),
            voltage: get_f32("Voltage").map(Volts),
            manifold_pressure: get_f32("ManifoldPress").map(Bar),
            water_level: get_f32("WaterLevel").map(Liters),
            warnings: engine_warnings,
        });

        // =================================================================
        // Wheels
        // =================================================================
        let wheels = Some(WheelData {
            front_left: self.extract_wheel(sample, "LF", true),
            front_right: self.extract_wheel(sample, "RF", false),
            rear_left: self.extract_wheel(sample, "LR", true),
            rear_right: self.extract_wheel(sample, "RR", false),
        });

        // =================================================================
        // Timing
        // =================================================================
        let timing = Some(TimingData {
            current_lap_time: get_f64("LapCurrentLapTime").map(|t| Seconds(t as f32)),
            last_lap_time: get_f64("LapLastLapTime").map(|t| Seconds(t as f32)),
            best_lap_time: get_f64("LapBestLapTime").map(|t| Seconds(t as f32)),
            best_n_lap_time: get_f64("LapBestNLapTime").map(|t| Seconds(t as f32)),
            best_n_lap_num: get_i32("LapBestNLapLap").map(|v| v as u32),
            sector_times: None,
            lap_number: get_i32("Lap").map(|l| l as u32),
            laps_completed: get_i32("LapCompleted").map(|l| l as u32),
            lap_distance: get_f32("LapDist").map(Meters),
            lap_distance_pct: get_f32("LapDistPct").map(Percentage::new),
            race_position: get_i32("PlayerCarPosition").map(|p| p as u32),
            class_position: get_i32("PlayerCarClassPosition").map(|p| p as u32),
            num_cars: None,
            delta_best: get_f32("LapDeltaToBestLap").map(Seconds),
            delta_best_ok: get_bool("LapDeltaToBestLap_OK"),
            delta_session_best: get_f32("LapDeltaToSessionBestLap").map(Seconds),
            delta_session_best_ok: get_bool("LapDeltaToSessionBestLap_OK"),
            delta_optimal: get_f32("LapDeltaToOptimalLap").map(Seconds),
            delta_optimal_ok: get_bool("LapDeltaToOptimalLap_OK"),
            estimated_lap_time: None,
            race_laps: None,
        });

        // =================================================================
        // Session
        // =================================================================
        let session_state = get_i32("SessionState").map(SessionState::from_iracing);
        let flags = get_u32("SessionFlags").map(FlagState::from_iracing_bits);
        let session_type = self.parse_session_type();

        let track_length = self
            .session_info
            .track_length
            .trim_end_matches(" km")
            .replace(',', ".")
            .parse::<f32>()
            .ok()
            .map(|km| Meters(km * 1000.0));

        let session = Some(SessionData {
            session_type,
            session_state,
            session_time: get_f64("SessionTime").map(|t| Seconds(t as f32)),
            session_time_remaining: get_f64("SessionTimeRemain").map(|t| Seconds(t as f32)),
            session_time_of_day: get_f32("SessionTimeOfDay").map(Seconds),
            session_laps: None,
            session_laps_remaining: get_i32("SessionLapsRemainEx").map(|l| l as u32),
            flags,
            track_name: Some(self.session_info.track_display_name.clone())
                .filter(|s| !s.is_empty()),
            track_config: Some(self.session_info.track_config_name.clone())
                .filter(|s| !s.is_empty()),
            track_length,
            track_type: None,
        });

        // =================================================================
        // Weather
        // =================================================================
        let weather = Some(WeatherData {
            air_temp: get_f32("AirTemp").map(Celsius),
            track_temp: get_f32("TrackTempCrew").map(Celsius),
            track_surface_temp: get_f32("TrackTemp").map(Celsius),
            air_pressure: get_f32("AirPressure").map(|v| Kilopascals(v / 1000.0)),
            air_density: get_f32("AirDensity").map(KilogramsPerCubicMeter),
            humidity: get_f32("RelativeHumidity").map(|h| Percentage::new(h / 100.0)),
            wind_speed: get_f32("WindVel").map(MetersPerSecond),
            wind_direction: get_f32("WindDir").map(Degrees::from_radians),
            fog_level: get_f32("FogLevel").map(Percentage::new),
            precipitation: None,
            track_wetness: None,
            skies: get_i32("Skies").map(|s| match s {
                0 => "Clear".to_string(),
                1 => "Partly Cloudy".to_string(),
                2 => "Mostly Cloudy".to_string(),
                3 => "Overcast".to_string(),
                _ => format!("Unknown({})", s),
            }),
            declared_wet: None,
        });

        // =================================================================
        // Pit
        // =================================================================
        let requested_services = Some(PitServices {
            fuel_to_add: get_f32("dpFuelFill").map(Liters),
            change_tyre_fl: get_f32("dpLFTireChange").is_some_and(|v| v > 0.0),
            change_tyre_fr: get_f32("dpRFTireChange").is_some_and(|v| v > 0.0),
            change_tyre_rl: get_f32("dpLRTireChange").is_some_and(|v| v > 0.0),
            change_tyre_rr: get_f32("dpRRTireChange").is_some_and(|v| v > 0.0),
            windshield_tearoff: get_f32("dpWindshieldTearoff").is_some_and(|v| v > 0.0),
            fast_repair: get_f32("dpFastRepair").is_some_and(|v| v > 0.0),
            tyre_pressure_fl: get_f32("dpLFTireColdPress").map(Kilopascals),
            tyre_pressure_fr: get_f32("dpRFTireColdPress").map(Kilopascals),
            tyre_pressure_rl: get_f32("dpLRTireColdPress").map(Kilopascals),
            tyre_pressure_rr: get_f32("dpRRTireColdPress").map(Kilopascals),
        });

        let pit = Some(PitData {
            on_pit_road: get_bool("OnPitRoad"),
            pit_active: get_bool("PitstopActive"),
            pit_service_status: get_i32("PlayerCarPitSvStatus").map(|v| v as u32),
            repair_time_left: get_f32("PitRepairLeft").map(Seconds),
            optional_repair_time_left: get_f32("PitOptRepairLeft").map(Seconds),
            fast_repair_available: get_i32("FastRepairAvailable").map(|v| v as u32),
            fast_repair_used: get_i32("FastRepairUsed").map(|v| v as u32),
            pit_speed_limit: None,
            requested_services,
        });

        // =================================================================
        // Game-specific namespace: all iRacing variables under "iracing"
        // =================================================================
        let mut iracing_data = serde_json::Map::new();

        for (name, value) in sample {
            // Skip CarIdx arrays (large per-car arrays, already in competitors)
            if name.starts_with("CarIdx") {
                continue;
            }
            iracing_data.insert(name.clone(), Self::var_value_to_json(value));
        }

        let mut extras = HashMap::new();
        extras.insert(
            "iracing".to_string(),
            serde_json::Value::Object(iracing_data),
        );

        TelemetryFrame {
            meta: MetaData {
                timestamp: Utc::now(),
                game: "iRacing Replay".to_string(),
                tick,
            },
            motion,
            vehicle,
            engine,
            wheels,
            timing,
            session,
            weather,
            pit,
            damage: None,
            drivers: if !self.session_info.driver_name.is_empty() {
                Some(DriversData {
                    current: Some(CurrentDriver {
                        name: Some(self.session_info.driver_name.clone()),
                        car_index: Some(self.session_info.driver_car_idx as u32),
                        car_number: None,
                        team_name: None,
                        estimated_lap_time: None,
                    }),
                    competitors: None,
                })
            } else {
                None
            },
            extras,
        }
    }

    /// Extract per-wheel data.
    /// `prefix` is "LF", "RF", "LR", or "RR".
    /// `is_left_side` determines inner/outer mapping for temperatures.
    fn extract_wheel(
        &self,
        sample: &HashMap<String, VarValue>,
        prefix: &str,
        is_left_side: bool,
    ) -> WheelInfo {
        let get_f32 = |suffix: &str| -> Option<f32> {
            let key = format!("{}{}", prefix, suffix);
            sample.get(&key).and_then(|v| v.as_f32())
        };

        // Inner/outer mapping: for left wheels, CL=outer edge, CR=inner edge.
        // For right wheels, CL=inner edge, CR=outer edge.
        let (surface_temp_inner, surface_temp_outer) = if is_left_side {
            (
                get_f32("tempCR").map(Celsius),
                get_f32("tempCL").map(Celsius),
            )
        } else {
            (
                get_f32("tempCL").map(Celsius),
                get_f32("tempCR").map(Celsius),
            )
        };

        let (carcass_temp_inner, carcass_temp_outer) = if is_left_side {
            (get_f32("tempR").map(Celsius), get_f32("tempL").map(Celsius))
        } else {
            (get_f32("tempL").map(Celsius), get_f32("tempR").map(Celsius))
        };

        WheelInfo {
            suspension_travel: get_f32("shockDefl").map(|v| Millimeters(v * 1000.0)),
            suspension_travel_avg: None, // shockDefl_ST is an array in iRacing, not a scalar
            shock_velocity: get_f32("shockVel").map(|v| MillimetersPerSecond(v * 1000.0)),
            shock_velocity_avg: None, // shockVel_ST is an array in iRacing, not a scalar
            ride_height: get_f32("rideHeight").map(|v| Millimeters(v * 1000.0)),
            tyre_pressure: get_f32("pressure").map(Kilopascals),
            tyre_cold_pressure: get_f32("coldPressure").map(Kilopascals),
            surface_temp_inner,
            surface_temp_middle: get_f32("tempCM").map(Celsius),
            surface_temp_outer,
            carcass_temp_inner,
            carcass_temp_middle: get_f32("tempM").map(Celsius),
            carcass_temp_outer,
            tyre_wear: None, // iRacing only has per-zone wearL/M/R, no overall wear
            tyre_wear_inner: if is_left_side {
                get_f32("wearR").map(Percentage::new)
            } else {
                get_f32("wearL").map(Percentage::new)
            },
            tyre_wear_middle: get_f32("wearM").map(Percentage::new),
            tyre_wear_outer: if is_left_side {
                get_f32("wearL").map(Percentage::new)
            } else {
                get_f32("wearR").map(Percentage::new)
            },
            wheel_speed: get_f32("speed").map(Rpm::from_radians_per_sec),
            slip_ratio: None,
            slip_angle: None,
            load: None,
            brake_line_pressure: get_f32("brakeLinePress").map(Kilopascals),
            brake_temp: None,
            tyre_compound: None,
        }
    }

    fn parse_session_type(&self) -> Option<SessionType> {
        let st = self.session_info.session_type.to_lowercase();
        if st.contains("race") {
            Some(SessionType::Race)
        } else if st.contains("qualify") || st.contains("qual") {
            Some(SessionType::Qualifying)
        } else if st.contains("practice") {
            Some(SessionType::Practice)
        } else if st.contains("time trial") || st.contains("timetrial") {
            Some(SessionType::TimeTrial)
        } else if st.contains("hotlap") {
            Some(SessionType::Hotlap)
        } else if st.contains("warmup") || st.contains("warm up") {
            Some(SessionType::Warmup)
        } else if !st.is_empty() {
            Some(SessionType::Other)
        } else {
            None
        }
    }
}

// ============================================================================
// Binary reading helpers
// ============================================================================

fn read_null_terminated_string(buf: &[u8]) -> String {
    let end = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
    String::from_utf8_lossy(&buf[..end]).to_string()
}

fn read_scalar_value(buf: &[u8], offset: usize, var_type: VarType) -> Option<VarValue> {
    match var_type {
        VarType::Char => {
            if offset < buf.len() {
                Some(VarValue::Char(buf[offset]))
            } else {
                None
            }
        }
        VarType::Bool => {
            if offset < buf.len() {
                Some(VarValue::Bool(buf[offset] != 0))
            } else {
                None
            }
        }
        VarType::Int => {
            if offset + 4 <= buf.len() {
                Some(VarValue::Int(i32::from_le_bytes(
                    buf[offset..offset + 4].try_into().ok()?,
                )))
            } else {
                None
            }
        }
        VarType::BitField => {
            if offset + 4 <= buf.len() {
                Some(VarValue::BitField(u32::from_le_bytes(
                    buf[offset..offset + 4].try_into().ok()?,
                )))
            } else {
                None
            }
        }
        VarType::Float => {
            if offset + 4 <= buf.len() {
                Some(VarValue::Float(f32::from_le_bytes(
                    buf[offset..offset + 4].try_into().ok()?,
                )))
            } else {
                None
            }
        }
        VarType::Double => {
            if offset + 8 <= buf.len() {
                Some(VarValue::Double(f64::from_le_bytes(
                    buf[offset..offset + 8].try_into().ok()?,
                )))
            } else {
                None
            }
        }
    }
}

fn read_array_value(
    buf: &[u8],
    offset: usize,
    var_type: VarType,
    count: usize,
) -> Option<VarValue> {
    match var_type {
        VarType::Char | VarType::Bool => {
            if offset + count <= buf.len() {
                Some(VarValue::CharArray(buf[offset..offset + count].to_vec()))
            } else {
                None
            }
        }
        VarType::Int | VarType::BitField => {
            let mut vals = Vec::with_capacity(count);
            for i in 0..count {
                let off = offset + i * 4;
                if off + 4 <= buf.len() {
                    vals.push(i32::from_le_bytes(buf[off..off + 4].try_into().ok()?));
                }
            }
            Some(VarValue::IntArray(vals))
        }
        VarType::Float => {
            let mut vals = Vec::with_capacity(count);
            for i in 0..count {
                let off = offset + i * 4;
                if off + 4 <= buf.len() {
                    vals.push(f32::from_le_bytes(buf[off..off + 4].try_into().ok()?));
                }
            }
            Some(VarValue::FloatArray(vals))
        }
        VarType::Double => {
            let mut vals = Vec::with_capacity(count);
            for i in 0..count {
                let off = offset + i * 8;
                if off + 8 <= buf.len() {
                    vals.push(f64::from_le_bytes(buf[off..off + 8].try_into().ok()?));
                }
            }
            Some(VarValue::DoubleArray(vals))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_var_type_from_i32() {
        assert_eq!(VarType::from_i32(0).unwrap(), VarType::Char);
        assert_eq!(VarType::from_i32(1).unwrap(), VarType::Bool);
        assert_eq!(VarType::from_i32(2).unwrap(), VarType::Int);
        assert_eq!(VarType::from_i32(3).unwrap(), VarType::BitField);
        assert_eq!(VarType::from_i32(4).unwrap(), VarType::Float);
        assert_eq!(VarType::from_i32(5).unwrap(), VarType::Double);
        assert!(VarType::from_i32(6).is_err());
    }

    #[test]
    fn test_var_value_conversions() {
        assert_eq!(VarValue::Float(1.5).as_f32(), Some(1.5));
        assert_eq!(VarValue::Double(2.5).as_f64(), Some(2.5));
        assert_eq!(VarValue::Int(42).as_i32(), Some(42));
        assert_eq!(VarValue::Bool(true).as_bool(), Some(true));
        assert_eq!(VarValue::BitField(0xFF).as_u32(), Some(0xFF));
    }

    #[test]
    fn test_read_null_terminated_string() {
        let buf = b"hello\0\0\0\0\0";
        assert_eq!(read_null_terminated_string(buf), "hello");

        let buf2 = b"no null here!!!!";
        assert_eq!(read_null_terminated_string(buf2), "no null here!!!!");
    }

    #[test]
    fn test_session_info_from_yaml() {
        let yaml = r#"---
WeekendInfo:
 TrackName: spielberg gp
 TrackDisplayName: Red Bull Ring
 TrackConfigName: Grand Prix
 TrackLength: 4.28 km
 DriverInfo:
 DriverCarIdx: 5
 Drivers:
 - CarIdx: 0
   UserName: Test Driver
   CarScreenName: Formula Test
SessionInfo:
 Sessions:
 - SessionNum: 0
   SessionType: Lone Qualify
"#;
        let info = IbtSessionInfo::from_yaml(yaml).unwrap();
        assert_eq!(info.track_name, "spielberg gp");
        assert_eq!(info.track_display_name, "Red Bull Ring");
        assert_eq!(info.driver_car_idx, 5);
        assert_eq!(info.driver_name, "Test Driver");
        assert_eq!(info.car_screen_name, "Formula Test");
        assert_eq!(info.session_type, "Lone Qualify");
    }

    #[test]
    fn test_read_scalar_values() {
        let mut buf = vec![0u8; 32];
        let val: f32 = 42.5;
        buf[0..4].copy_from_slice(&val.to_le_bytes());
        let ival: i32 = -123;
        buf[4..8].copy_from_slice(&ival.to_le_bytes());
        let dval: f64 = 99.99;
        buf[8..16].copy_from_slice(&dval.to_le_bytes());
        buf[16] = 1;

        match read_scalar_value(&buf, 0, VarType::Float) {
            Some(VarValue::Float(v)) => assert!((v - 42.5).abs() < 0.001),
            _ => panic!("Expected Float"),
        }
        match read_scalar_value(&buf, 4, VarType::Int) {
            Some(VarValue::Int(v)) => assert_eq!(v, -123),
            _ => panic!("Expected Int"),
        }
        match read_scalar_value(&buf, 8, VarType::Double) {
            Some(VarValue::Double(v)) => assert!((v - 99.99).abs() < 0.001),
            _ => panic!("Expected Double"),
        }
        match read_scalar_value(&buf, 16, VarType::Bool) {
            Some(VarValue::Bool(v)) => assert!(v),
            _ => panic!("Expected Bool"),
        }
    }

    // ========================================================================
    // Integration test: load the real fixtures/race.ibt file
    // ========================================================================

    fn fixture_path() -> std::path::PathBuf {
        let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        manifest_dir.join("../fixtures/race.ibt")
    }

    fn has_fixture() -> bool {
        fixture_path().exists()
    }

    #[test]
    fn test_ibt_open_and_header() {
        if !has_fixture() {
            return;
        }
        let ibt = IbtFile::open(&fixture_path()).expect("Failed to open .ibt file");

        assert_eq!(ibt.header.ver, 2);
        assert_eq!(ibt.header.tick_rate, 60);
        assert_eq!(ibt.header.num_vars, 268);
        assert_eq!(ibt.disk_sub_header.session_record_count, 73225);
        assert_eq!(ibt.record_count(), 73225);
        let duration = ibt.duration_secs();
        assert!(
            duration > 1220.0 && duration < 1221.0,
            "Expected ~1220s duration, got {duration}"
        );
    }

    #[test]
    fn test_ibt_session_info_yaml() {
        if !has_fixture() {
            return;
        }
        let ibt = IbtFile::open(&fixture_path()).expect("Failed to open .ibt file");
        let info = &ibt.session_info;
        assert_eq!(info.track_display_name, "Tsukuba Circuit 2k Full");
        assert_eq!(info.session_type, "Practice");
    }

    /// Flatten a JSON value into sorted key-value pairs, skipping nulls.
    fn flatten_json(val: &serde_json::Value) -> Vec<(String, String)> {
        fn recurse(prefix: &str, val: &serde_json::Value, out: &mut Vec<(String, String)>) {
            match val {
                serde_json::Value::Object(map) => {
                    for (k, v) in map {
                        let key = if prefix.is_empty() {
                            k.clone()
                        } else {
                            format!("{prefix}.{k}")
                        };
                        recurse(&key, v, out);
                    }
                }
                serde_json::Value::Array(arr) => {
                    for (i, v) in arr.iter().enumerate() {
                        recurse(&format!("{prefix}[{i}]"), v, out);
                    }
                }
                serde_json::Value::Null => {}
                _ => {
                    let display = match val {
                        serde_json::Value::String(s) => s.clone(),
                        other => other.to_string(),
                    };
                    out.push((prefix.to_string(), display));
                }
            }
        }
        let mut out = Vec::new();
        recurse("", val, &mut out);
        out.sort_by(|a, b| a.0.cmp(&b.0));
        out
    }

    /// Print flattened channels to stderr.
    fn print_channels(label: &str, channels: &[(String, String)]) {
        let max_key = channels.iter().map(|(k, _)| k.len()).max().unwrap_or(0);
        eprintln!("\n=== {label} ===");
        for (key, val) in channels {
            eprintln!("{key:<max_key$}\t\t{val}");
        }
    }

    #[test]
    fn test_ibt_read_and_convert_frame() {
        if !has_fixture() {
            return;
        }
        let ibt = IbtFile::open(&fixture_path()).expect("Failed to open .ibt file");

        // Read a frame ~30s in where car is likely on track
        let idx = 1800.min(ibt.record_count() - 1);
        let sample = ibt.read_sample(idx).expect("Failed to read sample");
        let frame = ibt.sample_to_frame(&sample);

        assert_eq!(frame.meta.game, "iRacing Replay");

        // Nested vehicle data
        let vehicle = frame.vehicle.as_ref().expect("vehicle should be populated");
        assert!(vehicle.speed.is_some());
        assert!(vehicle.rpm.is_some());
        assert!(vehicle.gear.is_some());
        assert!(vehicle.throttle.is_some());
        assert!(vehicle.brake.is_some());

        // Nested motion data
        let motion = frame.motion.as_ref().expect("motion should be populated");
        assert!(motion.velocity.is_some());
        assert!(motion.g_force.is_some());

        // Nested engine data
        let engine = frame.engine.as_ref().expect("engine should be populated");
        assert!(engine.water_temp.is_some());

        // Nested timing data
        let timing = frame.timing.as_ref().expect("timing should be populated");
        assert!(timing.lap_number.is_some());

        // Session data
        let session = frame.session.as_ref().expect("session should be populated");
        assert_eq!(
            session.track_name.as_deref(),
            Some("Tsukuba Circuit 2k Full")
        );
        assert_eq!(session.session_type, Some(SessionType::Practice));

        // Wheels
        let wheels = frame.wheels.as_ref().expect("wheels should be populated");
        assert!(wheels.front_left.suspension_travel.is_some());
        assert!(wheels.front_right.tyre_pressure.is_some());

        // Dump all channels for frame 1800
        let json = serde_json::to_value(&frame).expect("serialize");
        let channels = flatten_json(&json);
        print_channels(&format!("ALL CHANNELS (frame {idx})"), &channels);

        // =====================================================================
        // Sample at 10s intervals and print + assert exact parsed values
        // =====================================================================
        let tick_rate = ibt.header.tick_rate as usize;
        let step = tick_rate * 10; // 600 frames = 10s

        // Helper to get a channel value from flattened list
        let get = |channels: &[(String, String)], key: &str| -> Option<String> {
            channels
                .iter()
                .find(|(k, _)| k == key)
                .map(|(_, v)| v.clone())
        };

        eprintln!("\n=== CHANNELS AT 10s INTERVALS ===");
        eprintln!(
            "{:>6} {:>8} {:>7} {:>4} {:>6} {:>6} {:>8} {:>8} {:>8} {:>8} {:>8} {:>4} {:>8} {:>8}",
            "frame",
            "sess_t",
            "speed",
            "gear",
            "throt",
            "brake",
            "rot.x",
            "rot.y",
            "rot.z",
            "heading",
            "lat",
            "lap",
            "lap_dst",
            "lap_time"
        );

        let mut interval_snapshots: Vec<(usize, Vec<(String, String)>)> = Vec::new();
        for frame_idx in (0..ibt.record_count()).step_by(step) {
            let sample = ibt.read_sample(frame_idx).unwrap();
            let frame = ibt.sample_to_frame(&sample);
            let json = serde_json::to_value(&frame).expect("serialize");
            let m = flatten_json(&json);

            let sess_t = get(&m, "session.session_time").unwrap_or_default();
            let speed = get(&m, "vehicle.speed").unwrap_or_default();
            let gear = get(&m, "vehicle.gear").unwrap_or_default();
            let throttle = get(&m, "vehicle.throttle").unwrap_or_default();
            let brake = get(&m, "vehicle.brake").unwrap_or_default();
            let rot_x = get(&m, "motion.pitch").unwrap_or_default();
            let rot_y = get(&m, "motion.yaw").unwrap_or_default();
            let rot_z = get(&m, "motion.roll").unwrap_or_default();
            let heading = get(&m, "motion.heading").unwrap_or_default();
            let lat = get(&m, "motion.latitude").unwrap_or_default();
            let lap = get(&m, "timing.lap_number").unwrap_or_default();
            let lap_dst = get(&m, "timing.lap_distance").unwrap_or_default();
            let lap_time = get(&m, "timing.current_lap_time").unwrap_or_default();

            eprintln!(
                "{:6} {:>8} {:>7} {:>4} {:>6} {:>6} {:>8} {:>8} {:>8} {:>8} {:>8} {:>4} {:>8} {:>8}",
                frame_idx, sess_t, speed, gear, throttle, brake,
                rot_x, rot_y, rot_z, heading,
                lat, lap, lap_dst, lap_time
            );

            interval_snapshots.push((frame_idx, m));
        }

        // Assert exact values at specific 10s interval frames to detect parsing regressions.
        // These are hardcoded from a known-good parse of fixtures/race.ibt.
        // If any assertion fails, the parsing logic has changed.
        let assert_channel = |snapshots: &[(usize, Vec<(String, String)>)],
                              frame_idx: usize,
                              key: &str,
                              expected: &str| {
            let (_, channels) = snapshots
                .iter()
                .find(|(idx, _)| *idx == frame_idx)
                .unwrap_or_else(|| panic!("No snapshot for frame {frame_idx}"));
            let actual = get(channels, key);
            assert_eq!(
                actual.as_deref(),
                Some(expected),
                "frame {frame_idx} channel {key}"
            );
        };

        // Frame 0: stationary in pit
        assert_channel(&interval_snapshots, 0, "vehicle.speed", "0.0");
        assert_channel(&interval_snapshots, 0, "vehicle.gear", "0");
        assert_channel(&interval_snapshots, 0, "vehicle.brake", "1.0");
        assert_channel(&interval_snapshots, 0, "timing.lap_number", "0");

        // Frame 600 (~10s): leaving pit
        assert_channel(&interval_snapshots, 600, "vehicle.gear", "2");
        assert_channel(
            &interval_snapshots,
            600,
            "vehicle.speed",
            "24.08989906311035",
        );
        assert_channel(
            &interval_snapshots,
            600,
            "motion.heading",
            "30.579999923706055",
        );
        assert_channel(&interval_snapshots, 600, "motion.yaw", "5.3719000816345215");
        assert_channel(&interval_snapshots, 600, "timing.lap_number", "0");

        // Frame 6000 (~100s): on-track mid-lap
        assert_channel(&interval_snapshots, 6000, "vehicle.gear", "4");
        assert_channel(
            &interval_snapshots,
            6000,
            "vehicle.speed",
            "44.3286018371582",
        );
        assert_channel(&interval_snapshots, 6000, "vehicle.throttle", "1.0");
        assert_channel(&interval_snapshots, 6000, "timing.lap_number", "1");
        assert_channel(
            &interval_snapshots,
            6000,
            "timing.current_lap_time",
            "28.792400360107422",
        );

        // Frame 12000 (~200s): further into the session
        assert_channel(&interval_snapshots, 12000, "vehicle.gear", "2");
        assert_channel(
            &interval_snapshots,
            12000,
            "vehicle.speed",
            "21.92620086669922",
        );
        assert_channel(
            &interval_snapshots,
            12000,
            "motion.heading",
            "118.48809814453125",
        );
        assert_channel(&interval_snapshots, 12000, "timing.lap_number", "3");

        // Frame 30000 (~500s): well into session
        assert_channel(
            &interval_snapshots,
            30000,
            "vehicle.speed",
            "22.12459945678711",
        );
        assert_channel(&interval_snapshots, 30000, "vehicle.gear", "2");
        assert_channel(&interval_snapshots, 30000, "timing.lap_number", "8");
        assert_channel(
            &interval_snapshots,
            30000,
            "motion.latitude",
            "36.15249059429738",
        );
        assert_channel(
            &interval_snapshots,
            30000,
            "timing.current_lap_time",
            "33.53770065307617",
        );

        // Frame 60000 (~1000s): near end of session, stationary
        assert_channel(&interval_snapshots, 60000, "vehicle.gear", "0");
        assert_channel(&interval_snapshots, 60000, "vehicle.speed", "0.0");
        assert_channel(&interval_snapshots, 60000, "timing.lap_number", "13");
        assert_channel(
            &interval_snapshots,
            60000,
            "motion.heading",
            "12.827899932861328",
        );
        assert_channel(
            &interval_snapshots,
            60000,
            "motion.yaw",
            "23.124000549316406",
        );
    }

    #[test]
    fn test_ibt_frame_values_are_sane() {
        if !has_fixture() {
            return;
        }
        let ibt = IbtFile::open(&fixture_path()).expect("Failed to open .ibt file");

        for &idx in &[0, 1000, 5000, 10000, ibt.record_count() - 1] {
            if idx >= ibt.record_count() {
                continue;
            }
            let sample = ibt.read_sample(idx).unwrap();
            let frame = ibt.sample_to_frame(&sample);

            if let Some(ref v) = frame.vehicle {
                if let Some(speed) = v.speed {
                    assert!(
                        speed.0 >= 0.0 && speed.0 < 120.0,
                        "Frame {idx}: Speed {:.1} m/s out of range",
                        speed.0
                    );
                }
                if let Some(rpm) = v.rpm {
                    assert!(rpm.0 >= 0.0 && rpm.0 < 20000.0);
                }
            }
            if let Some(ref m) = frame.motion {
                if let Some(ref g) = m.g_force {
                    assert!(g.x.0.abs() < 10.0);
                    assert!(g.z.0.abs() < 10.0);
                }
            }
        }
    }

    #[test]
    fn test_ibt_lap_index() {
        if !has_fixture() {
            return;
        }
        let mut ibt = IbtFile::open(&fixture_path()).expect("Failed to open .ibt file");
        let laps = ibt.build_lap_index().unwrap();
        assert_eq!(laps.len(), 18);
        // Lap 0 is the out-lap (pct reaches ~1.0 so it gets a time now)
        assert_eq!(laps[0].lap_number, 0);
        assert!(laps[0].lap_time_secs.is_some());
        // Lap 1 has a timed lap (~55.5s around Tsukuba)
        assert_eq!(laps[1].lap_number, 1);
        let t1 = laps[1].lap_time_secs.expect("Lap 1 should have a time");
        assert!(
            t1 > 54.0 && t1 < 57.0,
            "Lap 1 time {t1} out of range for Tsukuba"
        );
        // 13 timed laps total (out-lap + laps 1-12)
        let timed_count = laps.iter().filter(|l| l.lap_time_secs.is_some()).count();
        assert_eq!(timed_count, 13);
    }

    #[test]
    fn test_ibt_frame_snapshot_values() {
        if !has_fixture() {
            return;
        }
        let ibt = IbtFile::open(&fixture_path()).expect("Failed to open .ibt file");

        // Frame 1800 (~30s in) — car should be on track with stable values
        let sample = ibt.read_sample(1800).unwrap();
        let frame = ibt.sample_to_frame(&sample);

        // Vehicle data assertions
        let vehicle = frame.vehicle.as_ref().expect("vehicle");
        let speed_ms = vehicle.speed.expect("speed").0;
        assert!(
            speed_ms > 20.0 && speed_ms < 100.0,
            "Speed at frame 1800 should be 20-100 m/s, got {speed_ms}"
        );
        let rpm = vehicle.rpm.expect("rpm").0;
        assert!(
            rpm > 3000.0 && rpm < 12000.0,
            "RPM should be 3000-12000, got {rpm}"
        );
        assert!(
            vehicle.gear.expect("gear") > 0,
            "Should be in a forward gear"
        );
        let throttle = vehicle.throttle.expect("throttle").0;
        assert!(
            (0.0..=1.0).contains(&throttle),
            "Throttle should be 0-1, got {throttle}"
        );
        let brake = vehicle.brake.expect("brake").0;
        assert!(
            (0.0..=1.0).contains(&brake),
            "Brake should be 0-1, got {brake}"
        );

        // Motion data
        let motion = frame.motion.as_ref().expect("motion");
        let g = motion.g_force.as_ref().expect("g_force");
        assert!(
            g.x.0.abs() < 5.0 && g.y.0.abs() < 5.0 && g.z.0.abs() < 5.0,
            "G-forces should be within ±5g"
        );
        // Wheels — all four corners should have tyre pressure
        let wheels = frame.wheels.as_ref().expect("wheels");
        for (name, corner) in [
            ("FL", &wheels.front_left),
            ("FR", &wheels.front_right),
            ("RL", &wheels.rear_left),
            ("RR", &wheels.rear_right),
        ] {
            let pressure = corner
                .tyre_pressure
                .unwrap_or_else(|| panic!("{name} pressure"));
            assert!(
                pressure.0 > 100.0 && pressure.0 < 300.0,
                "{name} tyre pressure {:.1} kPa out of range",
                pressure.0
            );
        }

        // Session data should match fixture
        let session = frame.session.as_ref().expect("session");
        assert_eq!(
            session.track_name.as_deref(),
            Some("Tsukuba Circuit 2k Full")
        );
        assert_eq!(session.session_type, Some(SessionType::Practice));

        // Should have iracing namespace with raw variables
        let iracing_ns = frame
            .extras
            .get("iracing")
            .expect("Should have iracing namespace");
        assert!(
            iracing_ns.is_object(),
            "iracing namespace should be an object"
        );
    }

    #[test]
    fn test_ibt_sequential_read_consistency() {
        if !has_fixture() {
            return;
        }
        let ibt = IbtFile::open(&fixture_path()).expect("Failed to open .ibt file");

        let first = ibt
            .read_sample(0)
            .unwrap()
            .get("SessionTime")
            .unwrap()
            .as_f64()
            .unwrap();
        let sixtieth = ibt
            .read_sample(59)
            .unwrap()
            .get("SessionTime")
            .unwrap()
            .as_f64()
            .unwrap();
        let elapsed = sixtieth - first;
        assert!(
            (elapsed - 1.0).abs() < 0.1,
            "60 frames at 60Hz should span ~1 second, got {elapsed:.3}s"
        );
    }
}
