//! Server-side telemetry history buffer
//!
//! Stores recent TelemetryFrames in a ring buffer (VecDeque) for client-side
//! seek-back functionality. Reuses the same API shape as replay endpoints
//! (0-based frame indexing, chunk-based fetching, metric mask filtering).

use ost_core::model::TelemetryFrame;
use serde::Serialize;
use std::collections::VecDeque;

/// Lap transition marker detected from live telemetry
#[derive(Clone, Debug, Serialize)]
pub struct LapMarker {
    pub lap_number: u32,
    /// 0-based index into the buffer (shifts when old frames are evicted)
    pub start_frame: usize,
    /// Lap time of the just-completed lap (from timing.last_lap_time)
    pub lap_time_secs: Option<f64>,
}

/// Ring buffer of recent TelemetryFrames
pub struct HistoryBuffer {
    frames: VecDeque<TelemetryFrame>,
    max_frames: usize,
    max_duration_secs: u32,
    paused: bool,
    // Lap detection
    laps: Vec<LapMarker>,
    last_lap_number: Option<u32>,
    // Session info captured from frames
    track_name: String,
    car_name: String,
}

impl HistoryBuffer {
    pub fn new(max_duration_secs: u32) -> Self {
        let max_frames = max_duration_secs as usize * 60;
        Self {
            frames: VecDeque::with_capacity(max_frames.min(36000)), // Don't pre-alloc more than 10min
            max_frames,
            max_duration_secs,
            paused: false,
            laps: Vec::new(),
            last_lap_number: None,
            track_name: String::new(),
            car_name: String::new(),
        }
    }

    /// Push a frame into the buffer. Detects lap transitions and captures session info.
    pub fn push(&mut self, frame: TelemetryFrame) {
        if self.paused {
            return;
        }

        // Capture session info
        if let Some(ref session) = frame.session {
            if let Some(ref name) = session.track_name {
                if !name.is_empty() {
                    self.track_name = name.clone();
                }
            }
        }
        if let Some(ref vehicle) = frame.vehicle {
            if let Some(ref name) = vehicle.car_name {
                if !name.is_empty() {
                    self.car_name = name.clone();
                }
            }
        }

        // Detect lap transitions (ignore telemetry dropouts like 4→0→4)
        let current_lap = frame.timing.as_ref().and_then(|t| t.lap_number);
        if let Some(lap_num) = current_lap {
            if let Some(prev) = self.last_lap_number {
                // Only count as a new lap if the number increased (skip drops to 0 or backwards)
                if lap_num > prev {
                    let lap_time = frame
                        .timing
                        .as_ref()
                        .and_then(|t| t.last_lap_time)
                        .map(|s| s.0 as f64);
                    self.laps.push(LapMarker {
                        lap_number: lap_num,
                        start_frame: self.frames.len(),
                        lap_time_secs: lap_time,
                    });
                    self.last_lap_number = Some(lap_num);
                } else if lap_num == prev {
                    // Same lap, no change needed
                } else {
                    // lap_num < prev: telemetry dropout, don't update last_lap_number
                    // so when it recovers back to `prev`, no spurious transition is recorded
                }
            } else {
                self.last_lap_number = Some(lap_num);
            }
        }

        // Push frame
        self.frames.push_back(frame);

        // Evict oldest if at capacity
        if self.frames.len() > self.max_frames {
            self.frames.pop_front();
            // Shift all lap markers down by 1, remove any that fall off
            self.laps.retain_mut(|lap| {
                if lap.start_frame == 0 {
                    false
                } else {
                    lap.start_frame -= 1;
                    true
                }
            });
        }
    }

    /// Get a range of frames by 0-based index.
    /// Returns (index, &TelemetryFrame) pairs, clamped to valid range.
    pub fn get_frames_range(&self, start: usize, count: usize) -> Vec<(usize, &TelemetryFrame)> {
        let max_count = 7200; // Cap at 2 minutes at 60fps (same as replay)
        let len = self.frames.len();
        if len == 0 || start >= len {
            return Vec::new();
        }
        let clamped_count = count.min(max_count).min(len - start);
        (start..start + clamped_count)
            .map(|i| (i, &self.frames[i]))
            .collect()
    }

    /// Number of frames currently in the buffer
    pub fn frame_count(&self) -> usize {
        self.frames.len()
    }

    /// Duration in seconds covered by the buffer (from oldest to newest frame)
    pub fn duration_secs(&self) -> f64 {
        if self.frames.len() < 2 {
            return 0.0;
        }
        let oldest = &self.frames[0];
        let newest = self.frames.back().unwrap();
        let diff = newest.meta.timestamp - oldest.meta.timestamp;
        diff.num_milliseconds() as f64 / 1000.0
    }

    /// Estimate tick rate from frame timestamps
    pub fn tick_rate(&self) -> u32 {
        let len = self.frames.len();
        if len < 2 {
            return 60;
        }
        let duration = self.duration_secs();
        if duration <= 0.0 {
            return 60;
        }
        ((len - 1) as f64 / duration).round() as u32
    }

    /// Estimated memory usage in MB (~3KB per frame)
    pub fn estimated_memory_mb(&self) -> f64 {
        (self.frames.len() as f64 * 3072.0) / 1_048_576.0
    }

    /// Resize the buffer capacity
    pub fn resize(&mut self, max_duration_secs: u32) {
        self.max_duration_secs = max_duration_secs;
        self.max_frames = max_duration_secs as usize * 60;
        // Trim from front if over new capacity
        while self.frames.len() > self.max_frames {
            self.frames.pop_front();
            self.laps.retain_mut(|lap| {
                if lap.start_frame == 0 {
                    false
                } else {
                    lap.start_frame -= 1;
                    true
                }
            });
        }
    }

    pub fn set_paused(&mut self, paused: bool) {
        self.paused = paused;
    }

    pub fn is_paused(&self) -> bool {
        self.paused
    }

    pub fn laps(&self) -> &[LapMarker] {
        &self.laps
    }

    pub fn track_name(&self) -> &str {
        &self.track_name
    }

    pub fn car_name(&self) -> &str {
        &self.car_name
    }

    pub fn max_duration_secs(&self) -> u32 {
        self.max_duration_secs
    }

    /// Get the most recent frame in the buffer
    pub fn latest_frame(&self) -> Option<&TelemetryFrame> {
        self.frames.back()
    }

    /// Get all frames from the last N seconds (based on timestamps)
    pub fn get_frames_since_secs(&self, duration_secs: f64) -> Vec<&TelemetryFrame> {
        if self.frames.is_empty() {
            return Vec::new();
        }
        let newest = self.frames.back().unwrap();
        let cutoff =
            newest.meta.timestamp - chrono::Duration::milliseconds((duration_secs * 1000.0) as i64);
        self.frames
            .iter()
            .filter(|f| f.meta.timestamp >= cutoff)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use ost_core::model::{MetaData, SessionData, TimingData};
    use ost_core::units::Seconds;

    fn make_frame(lap: Option<u32>, last_lap_time: Option<f64>) -> TelemetryFrame {
        TelemetryFrame {
            meta: MetaData {
                timestamp: Utc::now(),
                game: "test".to_string(),
                tick: None,
            },
            motion: None,
            vehicle: None,
            engine: None,
            wheels: None,
            timing: Some(TimingData {
                current_lap_time: None,
                last_lap_time: last_lap_time.map(|v| Seconds(v as f32)),
                best_lap_time: None,
                best_n_lap_time: None,
                best_n_lap_num: None,
                sector_times: None,
                lap_number: lap,
                laps_completed: None,
                lap_distance: None,
                lap_distance_pct: None,
                race_position: None,
                class_position: None,
                num_cars: None,
                delta_best: None,
                delta_best_ok: None,
                delta_session_best: None,
                delta_session_best_ok: None,
                delta_optimal: None,
                delta_optimal_ok: None,
                estimated_lap_time: None,
                race_laps: None,
            }),
            session: None,
            weather: None,
            pit: None,
            electronics: None,
            damage: None,
            competitors: None,
            driver: None,
            extras: Default::default(),
        }
    }

    #[test]
    fn test_push_and_frame_count() {
        let mut buf = HistoryBuffer::new(1); // 1 second = 60 frames
        assert_eq!(buf.frame_count(), 0);
        for _ in 0..10 {
            buf.push(make_frame(Some(1), None));
        }
        assert_eq!(buf.frame_count(), 10);
    }

    #[test]
    fn test_eviction() {
        let mut buf = HistoryBuffer::new(1); // max 60 frames
        for i in 0..100 {
            buf.push(make_frame(Some(1), None));
            assert!(
                buf.frame_count() <= 60,
                "frame {} count {}",
                i,
                buf.frame_count()
            );
        }
        assert_eq!(buf.frame_count(), 60);
    }

    #[test]
    fn test_lap_detection() {
        let mut buf = HistoryBuffer::new(10);
        // Push frames for lap 1
        for _ in 0..10 {
            buf.push(make_frame(Some(1), None));
        }
        assert!(buf.laps().is_empty()); // No transition yet

        // Transition to lap 2
        buf.push(make_frame(Some(2), Some(85.5)));
        assert_eq!(buf.laps().len(), 1);
        assert_eq!(buf.laps()[0].lap_number, 2);
        assert_eq!(buf.laps()[0].start_frame, 10);
        assert_eq!(buf.laps()[0].lap_time_secs, Some(85.5));
    }

    #[test]
    fn test_lap_detection_ignores_dropout() {
        let mut buf = HistoryBuffer::new(10);
        // Push frames for lap 4
        for _ in 0..10 {
            buf.push(make_frame(Some(4), None));
        }
        assert!(buf.laps().is_empty());

        // Telemetry drops to 0 — should NOT create a new lap
        buf.push(make_frame(Some(0), None));
        assert!(buf.laps().is_empty());

        // Telemetry recovers back to 4 — should NOT create a new lap
        buf.push(make_frame(Some(4), None));
        assert!(buf.laps().is_empty());

        // Real transition to lap 5
        buf.push(make_frame(Some(5), Some(90.0)));
        assert_eq!(buf.laps().len(), 1);
        assert_eq!(buf.laps()[0].lap_number, 5);
    }

    #[test]
    fn test_lap_markers_shift_on_eviction() {
        let mut buf = HistoryBuffer::new(1); // max 60 frames
                                             // Fill buffer halfway, then transition
        for _ in 0..30 {
            buf.push(make_frame(Some(1), None));
        }
        buf.push(make_frame(Some(2), Some(30.0)));
        assert_eq!(buf.laps()[0].start_frame, 30);

        // Push more to trigger eviction
        for _ in 0..40 {
            buf.push(make_frame(Some(2), None));
        }
        // Lap marker should have shifted
        assert_eq!(buf.laps().len(), 1);
        assert!(buf.laps()[0].start_frame < 30);
    }

    #[test]
    fn test_get_frames_range() {
        let mut buf = HistoryBuffer::new(10);
        for _ in 0..20 {
            buf.push(make_frame(Some(1), None));
        }
        let range = buf.get_frames_range(5, 10);
        assert_eq!(range.len(), 10);
        assert_eq!(range[0].0, 5);
        assert_eq!(range[9].0, 14);
    }

    #[test]
    fn test_get_frames_range_clamped() {
        let mut buf = HistoryBuffer::new(10);
        for _ in 0..5 {
            buf.push(make_frame(Some(1), None));
        }
        // Request beyond buffer
        let range = buf.get_frames_range(3, 100);
        assert_eq!(range.len(), 2);
    }

    #[test]
    fn test_paused() {
        let mut buf = HistoryBuffer::new(10);
        buf.push(make_frame(Some(1), None));
        assert_eq!(buf.frame_count(), 1);

        buf.set_paused(true);
        buf.push(make_frame(Some(1), None));
        assert_eq!(buf.frame_count(), 1); // Not added

        buf.set_paused(false);
        buf.push(make_frame(Some(1), None));
        assert_eq!(buf.frame_count(), 2);
    }

    #[test]
    fn test_resize() {
        let mut buf = HistoryBuffer::new(10);
        for _ in 0..100 {
            buf.push(make_frame(Some(1), None));
        }
        assert_eq!(buf.frame_count(), 100);

        buf.resize(1); // Shrink to 60 frames
        assert_eq!(buf.frame_count(), 60);
    }

    #[test]
    fn test_get_frames_since_secs() {
        let mut buf = HistoryBuffer::new(60);
        // Push frames with staggered timestamps
        let base = Utc::now();
        for i in 0..10 {
            let mut frame = make_frame(Some(1), None);
            frame.meta.timestamp = base + chrono::Duration::seconds(i);
            buf.push(frame);
        }
        // All 10 frames are within the last 60 seconds
        let recent = buf.get_frames_since_secs(60.0);
        assert_eq!(recent.len(), 10);

        // Only the last 5 seconds should give us frames from second 5..9
        let recent5 = buf.get_frames_since_secs(5.0);
        assert!(
            recent5.len() >= 5 && recent5.len() <= 6,
            "Expected 5-6 frames, got {}",
            recent5.len()
        );
    }

    #[test]
    fn test_get_frames_since_empty() {
        let buf = HistoryBuffer::new(10);
        let recent = buf.get_frames_since_secs(60.0);
        assert!(recent.is_empty());
    }

    #[test]
    fn test_session_capture() {
        let mut buf = HistoryBuffer::new(10);
        let mut frame = make_frame(Some(1), None);
        frame.session = Some(SessionData {
            session_type: None,
            session_state: None,
            session_time: None,
            session_time_remaining: None,
            session_time_of_day: None,
            session_laps: None,
            session_laps_remaining: None,
            flags: None,
            track_name: Some("Spa".to_string()),
            track_config: None,
            track_length: None,
            track_type: None,
        });
        frame.vehicle = Some(ost_core::model::VehicleData {
            speed: None,
            rpm: None,
            max_rpm: None,
            idle_rpm: None,
            gear: None,
            max_gears: None,
            throttle: None,
            brake: None,
            clutch: None,
            steering_angle: None,
            steering_torque: None,
            steering_torque_pct: None,
            handbrake: None,
            shift_indicator: None,
            steering_angle_max: None,
            on_track: None,
            in_garage: None,
            track_surface: None,
            car_name: Some("McLaren".to_string()),
            car_class: None,
            setup_name: None,
        });
        buf.push(frame);
        assert_eq!(buf.track_name(), "Spa");
        assert_eq!(buf.car_name(), "McLaren");
    }
}
