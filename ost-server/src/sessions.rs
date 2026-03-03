//! Session store for serve mode — persistent .ibt file storage with token-based access

use crate::replay::ReplayState;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tracing::{info, warn};

/// Metadata for a stored session
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SessionInfo {
    pub id: String,
    pub token: String,
    pub track_name: String,
    pub car_name: String,
    pub file_name: String,
    pub file_size: u64,
    pub total_frames: usize,
    pub duration_secs: f64,
    pub created_at: String,
}

/// Manages persistent session storage on disk.
///
/// Directory layout:
/// ```text
/// sessions_dir/
///   {id}/
///     meta.json     # SessionInfo
///     data.ibt      # Original uploaded .ibt file
/// ```
pub struct SessionStore {
    sessions_dir: PathBuf,
    max_storage_bytes: u64,
}

impl SessionStore {
    pub fn new(sessions_dir: PathBuf, max_storage_bytes: u64) -> std::io::Result<Self> {
        std::fs::create_dir_all(&sessions_dir)?;
        Ok(Self {
            sessions_dir,
            max_storage_bytes,
        })
    }

    /// Create a new session from an uploaded .ibt file.
    /// Writes the file to disk, parses metadata, and returns session info.
    pub fn create_session(&self, file_name: &str, data: &[u8]) -> Result<SessionInfo, String> {
        let id = random_hex(6); // 12 hex chars
        let token = random_hex(16); // 32 hex chars

        let session_dir = self.sessions_dir.join(&id);
        std::fs::create_dir_all(&session_dir)
            .map_err(|e| format!("Failed to create session dir: {}", e))?;

        // Write .ibt file
        let ibt_path = session_dir.join("data.ibt");
        std::fs::write(&ibt_path, data).map_err(|e| format!("Failed to write .ibt file: {}", e))?;

        // Parse to extract metadata
        let replay = ReplayState::from_file(&ibt_path).map_err(|e| {
            // Clean up on parse failure
            let _ = std::fs::remove_dir_all(&session_dir);
            format!("Failed to parse .ibt file: {}", e)
        })?;

        let info = replay.info();
        let session_info = SessionInfo {
            id: id.clone(),
            token,
            track_name: info.track_name,
            car_name: info.car_name,
            file_name: file_name.to_string(),
            file_size: data.len() as u64,
            total_frames: info.total_frames,
            duration_secs: info.duration_secs,
            created_at: chrono::Utc::now().to_rfc3339(),
        };

        // Write metadata
        let meta_path = session_dir.join("meta.json");
        let meta_json = serde_json::to_string_pretty(&session_info)
            .map_err(|e| format!("Failed to serialize session info: {}", e))?;
        std::fs::write(&meta_path, meta_json)
            .map_err(|e| format!("Failed to write meta.json: {}", e))?;

        info!(
            "Session created: {} ({}, {})",
            id, session_info.track_name, session_info.car_name
        );

        // Enforce disk cap (delete oldest sessions if over limit)
        self.enforce_disk_cap(&id);

        Ok(session_info)
    }

    /// Get session metadata by ID
    pub fn get_session(&self, id: &str) -> Option<SessionInfo> {
        let meta_path = self.sessions_dir.join(id).join("meta.json");
        let data = std::fs::read_to_string(&meta_path).ok()?;
        serde_json::from_str(&data).ok()
    }

    /// Get the path to a session's .ibt file
    pub fn get_session_file(&self, id: &str) -> Option<PathBuf> {
        let path = self.sessions_dir.join(id).join("data.ibt");
        if path.exists() {
            Some(path)
        } else {
            None
        }
    }

    /// List all sessions, sorted by creation time (newest first)
    pub fn list_sessions(&self) -> Vec<SessionInfo> {
        let mut sessions = Vec::new();
        if let Ok(entries) = std::fs::read_dir(&self.sessions_dir) {
            for entry in entries.flatten() {
                if !entry.path().is_dir() {
                    continue;
                }
                let meta_path = entry.path().join("meta.json");
                if let Ok(data) = std::fs::read_to_string(&meta_path) {
                    if let Ok(info) = serde_json::from_str::<SessionInfo>(&data) {
                        sessions.push(info);
                    }
                }
            }
        }
        // Sort newest first
        sessions.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        sessions
    }

    /// Delete a session by ID. Returns true if it existed.
    pub fn delete_session(&self, id: &str) -> bool {
        let session_dir = self.sessions_dir.join(id);
        if session_dir.exists() {
            info!("Deleting session: {}", id);
            let _ = std::fs::remove_dir_all(&session_dir);
            true
        } else {
            false
        }
    }

    /// Total disk usage of all sessions in bytes
    pub fn total_storage_bytes(&self) -> u64 {
        let mut total: u64 = 0;
        if let Ok(entries) = std::fs::read_dir(&self.sessions_dir) {
            for entry in entries.flatten() {
                if entry.path().is_dir() {
                    total += dir_size(&entry.path());
                }
            }
        }
        total
    }

    /// Storage stats as JSON
    pub fn storage_stats(&self) -> serde_json::Value {
        let total = self.total_storage_bytes();
        let sessions = self.list_sessions();
        serde_json::json!({
            "session_count": sessions.len(),
            "total_size_bytes": total,
            "total_size_mb": (total as f64 / 1_048_576.0 * 100.0).round() / 100.0,
            "max_storage_mb": (self.max_storage_bytes as f64 / 1_048_576.0).round(),
            "directory": self.sessions_dir.to_string_lossy(),
        })
    }

    /// Delete oldest sessions until total storage is under the cap.
    /// Skips the session with the given `keep_id` (the one just created).
    fn enforce_disk_cap(&self, keep_id: &str) {
        let mut total = self.total_storage_bytes();
        if total <= self.max_storage_bytes {
            return;
        }

        // Get sessions sorted oldest first
        let mut sessions = self.list_sessions();
        sessions.reverse(); // oldest first

        for session in sessions {
            if total <= self.max_storage_bytes {
                break;
            }
            if session.id == keep_id {
                continue; // Don't delete the one we just created
            }
            let session_dir = self.sessions_dir.join(&session.id);
            let size = dir_size(&session_dir);
            info!(
                "Disk cap: deleting session {} ({:.1} MB)",
                session.id,
                size as f64 / 1_048_576.0
            );
            let _ = std::fs::remove_dir_all(&session_dir);
            total = total.saturating_sub(size);
        }

        if total > self.max_storage_bytes {
            warn!(
                "Disk cap: still over limit after cleanup ({:.1} MB / {:.1} MB)",
                total as f64 / 1_048_576.0,
                self.max_storage_bytes as f64 / 1_048_576.0
            );
        }
    }
}

/// Compute the total size of a directory and its contents
fn dir_size(path: &Path) -> u64 {
    let mut total: u64 = 0;
    if let Ok(entries) = std::fs::read_dir(path) {
        for entry in entries.flatten() {
            let meta = entry.metadata();
            if let Ok(m) = meta {
                if m.is_file() {
                    total += m.len();
                } else if m.is_dir() {
                    total += dir_size(&entry.path());
                }
            }
        }
    }
    total
}

/// Generate a random hex string of `n_bytes` length (produces 2*n_bytes hex chars).
fn random_hex(n_bytes: usize) -> String {
    let mut buf = vec![0u8; n_bytes];
    #[cfg(unix)]
    {
        use std::io::Read;
        if let Ok(mut f) = std::fs::File::open("/dev/urandom") {
            let _ = f.read_exact(&mut buf);
        }
    }
    #[cfg(not(unix))]
    {
        use std::hash::{Hash, Hasher};
        // Fallback: hash time + thread ID (not cryptographically secure)
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        std::time::SystemTime::now().hash(&mut hasher);
        std::thread::current().id().hash(&mut hasher);
        let h = hasher.finish().to_ne_bytes();
        for (i, b) in buf.iter_mut().enumerate() {
            *b = h[i % h.len()];
        }
    }
    buf.iter().map(|b| format!("{:02x}", b)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_random_hex_length() {
        assert_eq!(random_hex(6).len(), 12);
        assert_eq!(random_hex(16).len(), 32);
    }

    #[test]
    fn test_random_hex_uniqueness() {
        let a = random_hex(16);
        let b = random_hex(16);
        assert_ne!(a, b);
    }

    #[test]
    fn test_session_store_empty() {
        let dir = std::env::temp_dir().join("ost-test-sessions-empty");
        let _ = std::fs::remove_dir_all(&dir);
        let store = SessionStore::new(dir.clone(), 10 * 1024 * 1024 * 1024).unwrap();
        assert!(store.list_sessions().is_empty());
        assert_eq!(store.total_storage_bytes(), 0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_dir_size() {
        let dir = std::env::temp_dir().join("ost-test-dir-size");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("a.txt"), "hello").unwrap();
        std::fs::write(dir.join("b.txt"), "world!").unwrap();
        assert_eq!(dir_size(&dir), 11);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
