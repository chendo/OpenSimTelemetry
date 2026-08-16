//! Wire types for the NDJSON parse format.
//!
//! The first line of any parse stream is a [`SessionHeader`] (one JSON
//! object). Each subsequent line is a JSON object of channel-name → value
//! pairs (see top-level crate docs).

use ost_adapters::ibt_parser::{IbtDriverEntry, IbtQualifyResult, LapTimeSource};
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
    /// Lap index. `None` in stream mode (full-file scan skipped).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub laps: Option<Vec<LapInfo>>,
    /// Track outline as `[lat, lng]` pairs (on-track points only).
    /// `None` in stream mode (full-file scan skipped).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub track_outline: Option<Vec<[f64; 2]>>,
    /// Union of all channel names discovered during parsing, in stable
    /// (sorted) order. Informational in sparse mode; defines the per-frame
    /// shape in dense mode.
    pub channels: Vec<String>,
    pub total_frames: u64,
    /// The field that was entered, and which of them recorded the file.
    /// `None` when the source carries no roster.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub roster: Option<Roster>,
    /// Qualifying results, when the file has them. Absent far more often than
    /// present — of 73 iRacing files, 14 carried results, 10 carried an empty
    /// list and 49 had no such block at all.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub qualifying: Option<Vec<QualifyResult>>,
}

/// The session's entry list.
///
/// A list of who was in the session and in which car — it holds no positions
/// and no times, so it can say who was out there but never who was where.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Roster {
    /// Index into `entries` by `car_idx`: which entry recorded this file.
    pub driver_car_idx: i32,
    pub entries: Vec<RosterEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RosterEntry {
    pub car_idx: i32,
    pub user_name: String,
    /// The number as displayed, so `"07"` stays distinct from `"7"`.
    pub car_number: String,
    /// Entries sharing a class are the ones whose times compare.
    pub car_class_id: i32,
    pub car_name: String,
}

/// One qualifying result, with the file's sentinels left in place.
///
/// `fastest_time` is -1 when the car set no time, and the whole block is not
/// necessarily lap times: in a heat-racing event iRacing populates it from the
/// grid-setting race instead, where the numbers are finishing gaps in seconds.
/// `fastest_lap` of 0 alongside a leader time of 0 is the tell, so both fields
/// travel rather than being collapsed into one "the pole time" number here.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QualifyResult {
    /// Zero-based: position 0 is pole.
    pub position: i32,
    pub class_position: i32,
    pub car_idx: i32,
    pub fastest_lap: i32,
    pub fastest_time: f64,
}

impl From<&IbtDriverEntry> for RosterEntry {
    fn from(src: &IbtDriverEntry) -> Self {
        RosterEntry {
            car_idx: src.car_idx,
            user_name: src.user_name.clone(),
            car_number: src.car_number.clone(),
            car_class_id: src.car_class_id,
            car_name: src.car_screen_name.clone(),
        }
    }
}

impl From<&IbtQualifyResult> for QualifyResult {
    fn from(src: &IbtQualifyResult) -> Self {
        QualifyResult {
            position: src.position,
            class_position: src.class_position,
            car_idx: src.car_idx,
            fastest_lap: src.fastest_lap,
            fastest_time: src.fastest_time,
        }
    }
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
    /// The car drove the whole circuit. Pick representative laps with this;
    /// it is never a reason for `lap_time_secs` to be absent.
    #[serde(default)]
    pub full_lap: bool,
    /// Where the time came from, for callers deciding how far to trust it.
    #[serde(default)]
    pub time_source: Option<LapTimeSource>,
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
            full_lap: src.full_lap,
            time_source: src.time_source,
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
            laps: Some(vec![LapInfo {
                lap_number: 1,
                lap_index: 0,
                start_frame: 0,
                lap_time_secs: Some(83.21),
                incomplete: false,
                invalid_reason: None,
                full_lap: true,
                time_source: Some(LapTimeSource::Official),
            }]),
            track_outline: Some(vec![[28.1, -81.4], [28.2, -81.5]]),
            channels: vec!["meta.tick".to_string(), "vehicle.speed".to_string()],
            total_frames: 36000,
            roster: Some(Roster {
                driver_car_idx: 0,
                entries: vec![RosterEntry {
                    car_idx: 0,
                    user_name: "Test Driver".to_string(),
                    car_number: "07".to_string(),
                    car_class_id: 0,
                    car_name: "F1".to_string(),
                }],
            }),
            qualifying: Some(vec![QualifyResult {
                position: 0,
                class_position: 0,
                car_idx: 0,
                fastest_lap: 1,
                fastest_time: 83.21,
            }]),
        };
        let json = serde_json::to_string(&header).unwrap();
        let back: SessionHeader = serde_json::from_str(&json).unwrap();
        assert_eq!(back.metadata.replay_id, header.metadata.replay_id);
        assert_eq!(back.channels, header.channels);
        assert_eq!(back.laps.as_ref().unwrap().len(), 1);
        assert_eq!(back.track_outline.as_ref().unwrap().len(), 2);
        // The car number stays a string: "07" must not come back as 7.
        assert_eq!(back.roster.as_ref().unwrap().entries[0].car_number, "07");
        assert_eq!(back.qualifying.as_ref().unwrap()[0].fastest_time, 83.21);
    }

    /// A header written before rosters existed still reads.
    #[test]
    fn header_without_roster_deserializes() {
        let json = r#"{
            "format": "ost-parse", "version": 1, "source_format": "ibt",
            "mode": "sparse",
            "metadata": {"track_name": "Daytona", "car_name": "F1",
                "tick_rate": 60.0, "duration_secs": 1.0, "file_size": 1,
                "replay_id": "abc"},
            "channels": [], "total_frames": 0
        }"#;
        let header: SessionHeader = serde_json::from_str(json).unwrap();
        assert!(header.roster.is_none());
        assert!(header.qualifying.is_none());
    }
}
