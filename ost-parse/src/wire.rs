//! Wire types for the NDJSON parse format.
//!
//! The first line of any parse stream is a [`SessionHeader`] (one JSON
//! object). Each subsequent line is a JSON object of channel-name → value
//! pairs (see top-level crate docs).

use serde::{Deserialize, Serialize};

/// Header line of an `ost-parse` NDJSON stream.
///
/// Always emitted as the first line of any parse output. Carries all
/// session metadata, the lap index, the track outline, and the union of
/// channels discovered during parsing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionHeader {
    /// Constant `"ost-parse"`.
    pub format: String,
    /// Wire format version. Bumped on incompatible changes.
    pub version: u32,
    /// Source format identifier (`"ibt"` for v1).
    pub source_format: String,
    /// Whether each frame line includes every channel (`dense`) or only
    /// the channels actually present at that tick (`sparse`).
    pub mode: String,
    pub metadata: ReplayMetadata,
    pub laps: Vec<LapInfo>,
    /// Track outline as `[lat, lng]` pairs (on-track points only).
    pub track_outline: Vec<[f64; 2]>,
    /// Union of all channel names discovered during parsing, in stable
    /// (sorted) order. Informational in sparse mode; defines the per-frame
    /// shape in dense mode.
    pub channels: Vec<String>,
    pub total_frames: u64,
}

/// Per-session metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplayMetadata {
    pub track_name: String,
    pub car_name: String,
    pub tick_rate: f64,
    pub duration_secs: f64,
    pub file_size: u64,
    /// Stable hash of (file_size, total_frames, track_name, car_name) —
    /// matches the existing `ReplayState::replay_id` so callers keep
    /// stable IDs across both code paths.
    pub replay_id: String,
}

/// Lap boundary info, mirroring the existing `IbtFile::build_lap_index`
/// output.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LapInfo {
    pub lap_number: i32,
    pub lap_index: u32,
    pub start_frame: u64,
    pub lap_time_secs: Option<f64>,
    pub incomplete: bool,
    pub invalid_reason: Option<String>,
}

impl From<&ost_adapters::ibt_parser::LapInfo> for LapInfo {
    fn from(src: &ost_adapters::ibt_parser::LapInfo) -> Self {
        LapInfo {
            lap_number: src.lap_number,
            lap_index: src.lap_index,
            start_frame: src.start_frame as u64,
            lap_time_secs: src.lap_time_secs,
            incomplete: src.incomplete,
            invalid_reason: src.invalid_reason.clone(),
        }
    }
}

/// Compute the stable replay ID matching `ReplayState::from_file`.
///
/// Hashes `(file_size, total_frames, track_name, car_name)` with the
/// stdlib `DefaultHasher` and formats as a 16-char hex string. Both this
/// crate and `ost-server::replay::ReplayState` produce the same ID for
/// the same file.
pub fn compute_replay_id(
    file_size: u64,
    total_frames: usize,
    track_name: &str,
    car_name: &str,
) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    file_size.hash(&mut hasher);
    total_frames.hash(&mut hasher);
    track_name.hash(&mut hasher);
    car_name.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replay_id_matches_default_hasher_format() {
        // Same input → same output, deterministic across runs.
        let id1 = compute_replay_id(12345, 67890, "Daytona", "F1 Demo");
        let id2 = compute_replay_id(12345, 67890, "Daytona", "F1 Demo");
        assert_eq!(id1, id2);
        assert_eq!(id1.len(), 16);
        // Hex characters only.
        assert!(id1.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn replay_id_changes_with_inputs() {
        let base = compute_replay_id(100, 1000, "Daytona", "F1");
        assert_ne!(base, compute_replay_id(101, 1000, "Daytona", "F1"));
        assert_ne!(base, compute_replay_id(100, 1001, "Daytona", "F1"));
        assert_ne!(base, compute_replay_id(100, 1000, "Spa", "F1"));
        assert_ne!(base, compute_replay_id(100, 1000, "Daytona", "F2"));
    }

    #[test]
    fn header_round_trips_through_json() {
        let header = SessionHeader {
            format: "ost-parse".to_string(),
            version: 1,
            source_format: "ibt".to_string(),
            mode: "sparse".to_string(),
            metadata: ReplayMetadata {
                track_name: "Daytona".to_string(),
                car_name: "F1".to_string(),
                tick_rate: 60.0,
                duration_secs: 600.0,
                file_size: 12345,
                replay_id: compute_replay_id(12345, 36000, "Daytona", "F1"),
            },
            laps: vec![LapInfo {
                lap_number: 1,
                lap_index: 0,
                start_frame: 0,
                lap_time_secs: Some(83.21),
                incomplete: false,
                invalid_reason: None,
            }],
            track_outline: vec![[28.1, -81.4], [28.2, -81.5]],
            channels: vec!["meta.tick".to_string(), "vehicle.speed".to_string()],
            total_frames: 36000,
        };
        let json = serde_json::to_string(&header).unwrap();
        let back: SessionHeader = serde_json::from_str(&json).unwrap();
        assert_eq!(back.metadata.replay_id, header.metadata.replay_id);
        assert_eq!(back.channels, header.channels);
        assert_eq!(back.laps.len(), 1);
        assert_eq!(back.track_outline.len(), 2);
    }
}
