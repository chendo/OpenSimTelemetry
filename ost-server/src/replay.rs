//! Replay state and playback engine for .ibt file replay
//!
//! Manages the state of an active replay session including playback control
//! (play/pause/seek/speed) and frame-by-frame reading from parsed .ibt files.

use anyhow::Result;
use ost_adapters::ibt_parser::{IbtFile, LapInfo};
use ost_core::model::TelemetryFrame;
use serde::Serialize;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

/// The data source backing a replay session
enum ReplaySource {
    /// .ibt file with random-access reads
    Ibt(Box<IbtFile>),
    /// In-memory frames from NDJSON+ZSTD file
    Ndjson(Vec<TelemetryFrame>),
}

/// State for an active replay session
pub struct ReplayState {
    source: ReplaySource,
    current_frame: usize,
    total_frames: usize,
    tick_rate: u32,
    playing: bool,
    playback_speed: f64,
    file_size: u64,
    temp_path: Option<PathBuf>,
    track_name: String,
    car_name: String,
    duration_secs: f64,
    laps: Vec<LapInfo>,
    replay_id: String,
    /// Pre-computed track outline as [[lat, lng], ...] for the track map widget
    track_outline: Vec<[f64; 2]>,
}

impl ReplayState {
    pub fn from_file(path: &Path) -> Result<Self> {
        let mut ibt = IbtFile::open(path)?;

        let total_frames = ibt.record_count();
        let tick_rate = ibt.tick_rate();
        let file_size = ibt.file_size();
        let track_name = ibt.session_info().track_display_name.clone();
        let car_name = ibt.session_info().car_name.clone();
        let duration_secs = ibt.duration_secs();
        let laps = ibt.build_lap_index().unwrap_or_default();
        let track_outline = ibt.build_track_outline().unwrap_or_default();

        // Compute a stable replay ID from file metadata
        let mut hasher = DefaultHasher::new();
        file_size.hash(&mut hasher);
        total_frames.hash(&mut hasher);
        track_name.hash(&mut hasher);
        car_name.hash(&mut hasher);
        let replay_id = format!("{:016x}", hasher.finish());

        Ok(ReplayState {
            source: ReplaySource::Ibt(Box::new(ibt)),
            current_frame: 0,
            total_frames,
            tick_rate,
            playing: false,
            playback_speed: 1.0,
            file_size,
            temp_path: Some(path.to_path_buf()),
            track_name,
            car_name,
            duration_secs,
            laps,
            replay_id,
            track_outline,
        })
    }

    /// Load an NDJSON+ZSTD telemetry file
    pub fn from_ndjson_zstd(path: &Path) -> Result<Self> {
        let file = std::fs::File::open(path)?;
        let file_size = file.metadata()?.len();
        let decoder = zstd::Decoder::new(file)?;
        let reader = std::io::BufReader::new(decoder);

        let mut frames = Vec::new();
        use std::io::BufRead;
        for line in reader.lines() {
            let line = line?;
            if line.is_empty() {
                continue;
            }
            match serde_json::from_str::<TelemetryFrame>(&line) {
                Ok(frame) => frames.push(frame),
                Err(e) => {
                    tracing::warn!("Skipping malformed NDJSON line: {}", e);
                }
            }
        }

        let total_frames = frames.len();
        if total_frames == 0 {
            anyhow::bail!("No valid frames in file");
        }

        // Estimate tick rate from timestamps
        let tick_rate = if total_frames >= 2 {
            let diff = frames.last().unwrap().meta.timestamp - frames[0].meta.timestamp;
            let secs = diff.num_milliseconds() as f64 / 1000.0;
            if secs > 0.0 {
                ((total_frames - 1) as f64 / secs).round() as u32
            } else {
                60
            }
        } else {
            60
        };

        let duration_secs = total_frames as f64 / tick_rate as f64;

        let track_name = frames[0]
            .session
            .as_ref()
            .and_then(|s| s.track_name.clone())
            .unwrap_or_default();
        let car_name = frames[0]
            .vehicle
            .as_ref()
            .and_then(|v| v.car_name.clone())
            .unwrap_or_default();

        // Build lap index from timing data
        let mut laps = Vec::new();
        let mut last_lap: Option<u32> = None;
        for (i, f) in frames.iter().enumerate() {
            if let Some(lap_num) = f.timing.as_ref().and_then(|t| t.lap_number) {
                if last_lap.is_some_and(|prev| prev != lap_num) {
                    let lap_time = f
                        .timing
                        .as_ref()
                        .and_then(|t| t.last_lap_time)
                        .map(|s| s.0 as f64);
                    laps.push(LapInfo {
                        lap_number: lap_num as i32,
                        start_frame: i,
                        lap_time_secs: lap_time,
                    });
                }
                last_lap = Some(lap_num);
            }
        }

        // Build track outline from GPS data
        let mut track_outline = Vec::new();
        let mut last_lat = f64::NAN;
        let mut last_lng = f64::NAN;
        const MIN_DELTA: f64 = 0.000005;
        for f in &frames {
            let on_track = f.vehicle.as_ref().and_then(|v| v.on_track);
            if on_track == Some(false) {
                continue;
            }
            if let Some(m) = &f.motion {
                if let (Some(lat), Some(lng)) = (m.latitude, m.longitude) {
                    if lat == 0.0 && lng == 0.0 {
                        continue;
                    }
                    if (lat - last_lat).abs() < MIN_DELTA && (lng - last_lng).abs() < MIN_DELTA {
                        continue;
                    }
                    track_outline.push([lat, lng]);
                    last_lat = lat;
                    last_lng = lng;
                }
            }
        }

        let mut hasher = DefaultHasher::new();
        file_size.hash(&mut hasher);
        total_frames.hash(&mut hasher);
        track_name.hash(&mut hasher);
        car_name.hash(&mut hasher);
        let replay_id = format!("{:016x}", hasher.finish());

        Ok(ReplayState {
            source: ReplaySource::Ndjson(frames),
            current_frame: 0,
            total_frames,
            tick_rate,
            playing: false,
            playback_speed: 1.0,
            file_size,
            temp_path: None, // Don't delete on drop — it's the user's saved file
            track_name,
            car_name,
            duration_secs,
            laps,
            replay_id,
            track_outline,
        })
    }

    pub fn get_frame(&self, index: usize) -> Result<TelemetryFrame> {
        match &self.source {
            ReplaySource::Ibt(ibt) => {
                let sample = ibt.read_sample(index)?;
                Ok(ibt.sample_to_frame(&sample))
            }
            ReplaySource::Ndjson(frames) => frames
                .get(index)
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("Frame index {} out of range", index)),
        }
    }

    /// Read a range of frames for batch delivery to the client.
    pub fn get_frames_range(
        &self,
        start: usize,
        count: usize,
    ) -> Result<Vec<(usize, TelemetryFrame)>> {
        let max_count = 7200; // Cap at 2 minutes at 60fps
        let clamped_start = start.min(self.total_frames.saturating_sub(1));
        let clamped_count = count
            .min(max_count)
            .min(self.total_frames.saturating_sub(clamped_start));

        match &self.source {
            ReplaySource::Ibt(ibt) => {
                let samples = ibt.read_samples_range(clamped_start, clamped_count)?;
                let frames = samples
                    .iter()
                    .enumerate()
                    .map(|(i, sample)| (clamped_start + i, ibt.sample_to_frame(sample)))
                    .collect();
                Ok(frames)
            }
            ReplaySource::Ndjson(frames) => {
                let result = (clamped_start..clamped_start + clamped_count)
                    .map(|i| (i, frames[i].clone()))
                    .collect();
                Ok(result)
            }
        }
    }

    pub fn total_frames(&self) -> usize {
        self.total_frames
    }

    pub fn info(&self) -> ReplayInfo {
        ReplayInfo {
            total_frames: self.total_frames,
            tick_rate: self.tick_rate,
            duration_secs: self.duration_secs,
            current_frame: self.current_frame,
            playing: self.playing,
            playback_speed: self.playback_speed,
            track_name: self.track_name.clone(),
            car_name: self.car_name.clone(),
            file_size: self.file_size,
            laps: self.laps.clone(),
            replay_id: self.replay_id.clone(),
        }
    }

    pub fn current_frame(&self) -> usize {
        self.current_frame
    }

    pub fn tick_rate(&self) -> u32 {
        self.tick_rate
    }

    pub fn playback_speed(&self) -> f64 {
        self.playback_speed
    }

    pub fn is_playing(&self) -> bool {
        self.playing
    }

    pub fn track_outline(&self) -> &[[f64; 2]] {
        &self.track_outline
    }

    /// Clear the temp path so the file is NOT deleted on drop.
    /// Used for session files that should persist.
    pub fn set_persistent(&mut self) {
        self.temp_path = None;
    }

    pub fn play(&mut self) {
        self.playing = true;
    }

    pub fn pause(&mut self) {
        self.playing = false;
    }

    pub fn seek(&mut self, frame: usize) {
        self.current_frame = frame.min(self.total_frames.saturating_sub(1));
    }

    pub fn set_speed(&mut self, speed: f64) {
        self.playback_speed = speed.clamp(0.1, 16.0);
    }

    pub fn advance(&mut self) -> Option<usize> {
        if !self.playing {
            return None;
        }

        if self.current_frame >= self.total_frames.saturating_sub(1) {
            self.playing = false;
            return None;
        }

        self.current_frame += 1;
        Some(self.current_frame)
    }
}

impl Drop for ReplayState {
    fn drop(&mut self) {
        if let Some(ref path) = self.temp_path {
            if path.exists() {
                let _ = std::fs::remove_file(path);
            }
        }
    }
}

/// Serializable replay info for the API
#[derive(Debug, Clone, Serialize)]
pub struct ReplayInfo {
    pub total_frames: usize,
    pub tick_rate: u32,
    pub duration_secs: f64,
    pub current_frame: usize,
    pub playing: bool,
    pub playback_speed: f64,
    pub track_name: String,
    pub car_name: String,
    pub file_size: u64,
    pub laps: Vec<LapInfo>,
    pub replay_id: String,
}
