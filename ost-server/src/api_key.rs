//! API key generation and persistence.
//!
//! On first boot, generates a random API key and stores it on disk.
//! On subsequent boots, reads the existing key. Can be overridden
//! via `OST_AUTH_TOKEN` environment variable.

use std::path::PathBuf;
use tracing::info;

/// Directory for persistent config files.
fn config_dir() -> PathBuf {
    #[cfg(target_os = "windows")]
    {
        let base =
            dirs::document_dir().unwrap_or_else(|| dirs::home_dir().unwrap_or_else(|| ".".into()));
        base.join("OpenSimTelemetry")
    }
    #[cfg(not(target_os = "windows"))]
    {
        let base = dirs::home_dir().unwrap_or_else(|| ".".into());
        base.join(".opensimtelemetry")
    }
}

fn key_file_path() -> PathBuf {
    config_dir().join("api_key")
}

/// Generate a random hex string (using OS randomness).
fn generate_key() -> String {
    let mut buf = [0u8; 24]; // 48 hex chars
    #[cfg(unix)]
    {
        use std::io::Read;
        if let Ok(mut f) = std::fs::File::open("/dev/urandom") {
            let _ = f.read_exact(&mut buf);
        }
    }
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        // Fallback: use thread RNG from system time + thread id
        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        for (i, byte) in buf.iter_mut().enumerate() {
            *byte = ((seed >> (i % 16)) ^ (i as u128 * 0x9e3779b97f4a7c15)) as u8;
        }
    }
    buf.iter().map(|b| format!("{b:02x}")).collect()
}

/// Load or generate the API key.
///
/// Priority:
/// 1. `OST_AUTH_TOKEN` env var (if set and non-empty)
/// 2. Existing key file on disk
/// 3. Generate new key and persist to disk
pub fn load_or_generate() -> String {
    // Check env var first
    if let Ok(token) = std::env::var("OST_AUTH_TOKEN") {
        if !token.is_empty() {
            info!("Using API key from OST_AUTH_TOKEN environment variable");
            return token;
        }
    }

    let path = key_file_path();

    // Try to read existing key
    if let Ok(key) = std::fs::read_to_string(&path) {
        let key = key.trim().to_string();
        if !key.is_empty() {
            info!("Loaded API key from {}", path.display());
            return key;
        }
    }

    // Generate new key
    let key = generate_key();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    match std::fs::write(&path, &key) {
        Ok(()) => info!("Generated new API key, saved to {}", path.display()),
        Err(e) => tracing::warn!("Could not save API key to {}: {e}", path.display()),
    }
    key
}

/// Generate a new key and persist it, returning the new key.
pub fn regenerate() -> String {
    let key = generate_key();
    let path = key_file_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(&path, &key);
    info!("Regenerated API key");
    key
}
