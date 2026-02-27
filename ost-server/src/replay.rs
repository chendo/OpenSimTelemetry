//! Replay state and playback engine for .ibt file replay
//!
//! Manages the state of an active replay session including playback control
//! (play/pause/seek/speed) and frame-by-frame reading from parsed .ibt files.

use anyhow::Result;
use ost_adapters::ibt_parser::IbtFile;
use ost_core::model::TelemetryFrame;
use serde::Serialize;
use std::path::{Path, PathBuf};

/// State for an active replay session
pub struct ReplayState {
    ibt: IbtFile,
    current_frame: usize,
    total_frames: usize,
    tick_rate: u32,
    playing: bool,
    playback_speed: f64,
    file_size: u64,
    temp_path: PathBuf,
    track_name: String,
    car_name: String,
    duration_secs: f64,
}

impl ReplayState {
    pub fn from_file(path: &Path) -> Result<Self> {
        let ibt = IbtFile::open(path)?;

        let total_frames = ibt.record_count();
        let tick_rate = ibt.tick_rate();
        let file_size = ibt.file_size();
        let track_name = ibt.session_info().track_display_name.clone();
        let car_name = ibt.session_info().car_name.clone();
        let duration_secs = ibt.duration_secs();

        Ok(ReplayState {
            ibt,
            current_frame: 0,
            total_frames,
            tick_rate,
            playing: true,
            playback_speed: 1.0,
            file_size,
            temp_path: path.to_path_buf(),
            track_name,
            car_name,
            duration_secs,
        })
    }

    pub fn get_frame(&mut self, index: usize) -> Result<TelemetryFrame> {
        let sample = self.ibt.read_sample(index)?;
        Ok(self.ibt.sample_to_frame(&sample))
    }

    /// Read a range of frames for batch delivery to the client.
    /// Returns Vec of (frame_index, TelemetryFrame) pairs.
    /// Uses bulk disk read for performance (single seek instead of per-frame seeks).
    pub fn get_frames_range(
        &mut self,
        start: usize,
        count: usize,
    ) -> Result<Vec<(usize, TelemetryFrame)>> {
        let max_count = 7200; // Cap at 2 minutes at 60fps
        let clamped_start = start.min(self.total_frames.saturating_sub(1));
        let clamped_count = count
            .min(max_count)
            .min(self.total_frames.saturating_sub(clamped_start));

        let samples = self.ibt.read_samples_range(clamped_start, clamped_count)?;
        let frames = samples
            .iter()
            .enumerate()
            .map(|(i, sample)| (clamped_start + i, self.ibt.sample_to_frame(sample)))
            .collect();
        Ok(frames)
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
        if self.temp_path.exists() {
            let _ = std::fs::remove_file(&self.temp_path);
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
}
