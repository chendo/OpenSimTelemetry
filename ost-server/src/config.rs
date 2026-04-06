//! Persistent server configuration.
//!
//! Stores settings in a `config.toml` file in the platform config directory.
//! Migrates from the legacy flat `api_key` file on first load.
//! Environment variables (`OST_AUTH_TOKEN`, `OST_CORS_ORIGINS`) override file values.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tracing::info;

/// Platform-specific config directory.
///
/// - macOS/Linux: `~/.opensimtelemetry/`
/// - Windows: `Documents/OpenSimTelemetry/`
pub fn config_dir() -> PathBuf {
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

fn config_file_path() -> PathBuf {
    config_dir().join("config.toml")
}

fn legacy_key_file_path() -> PathBuf {
    config_dir().join("api_key")
}

/// Persistent server configuration stored as TOML.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ServerConfig {
    /// API key for authenticating requests.
    #[serde(default)]
    pub api_key: String,

    /// Allowed CORS origins (e.g. `["https://example.com"]`).
    #[serde(default)]
    pub cors_origins: Vec<String>,
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
        // Fallback: use system time + thread id
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

/// Load config from disk. Returns defaults if the file is missing or unparseable.
/// Performs one-time migration from the legacy `api_key` flat file.
fn load_from_disk() -> ServerConfig {
    let path = config_file_path();

    // Try reading existing config.toml
    if let Ok(contents) = std::fs::read_to_string(&path) {
        if let Ok(config) = toml::from_str::<ServerConfig>(&contents) {
            return config;
        }
        tracing::warn!("Failed to parse {}, using defaults", path.display());
    }

    // Migrate from legacy api_key file if it exists
    let legacy_path = legacy_key_file_path();
    if let Ok(key) = std::fs::read_to_string(&legacy_path) {
        let key = key.trim().to_string();
        if !key.is_empty() {
            info!(
                "Migrating API key from legacy {} into config.toml",
                legacy_path.display()
            );
            let config = ServerConfig {
                api_key: key,
                ..Default::default()
            };
            let _ = save_to_disk(&config);
            return config;
        }
    }

    ServerConfig::default()
}

/// Save config to disk.
pub fn save_to_disk(config: &ServerConfig) -> anyhow::Result<()> {
    let path = config_file_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let contents = toml::to_string_pretty(config)?;
    std::fs::write(&path, contents)?;
    Ok(())
}

/// Load config from disk, generate API key if missing, apply env var overrides.
/// This is the main entry point called at server startup.
pub fn load_effective_config() -> ServerConfig {
    let mut config = load_from_disk();

    // Generate API key if empty
    if config.api_key.is_empty() {
        config.api_key = generate_key();
        let _ = save_to_disk(&config);
        info!(
            "Generated new API key, saved to {}",
            config_file_path().display()
        );
    } else {
        info!("Loaded config from {}", config_file_path().display());
    }

    // Env var overrides
    if let Ok(token) = std::env::var("OST_AUTH_TOKEN") {
        if !token.is_empty() {
            info!("API key overridden by OST_AUTH_TOKEN");
            config.api_key = token;
        }
    }

    if let Ok(origins) = std::env::var("OST_CORS_ORIGINS") {
        if !origins.is_empty() {
            info!("CORS origins overridden by OST_CORS_ORIGINS: {origins}");
            config.cors_origins = origins
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
        }
    }

    config
}

/// Regenerate the API key, persist it, and return the new key.
pub fn regenerate_key() -> String {
    let mut config = load_from_disk();
    config.api_key = generate_key();
    let _ = save_to_disk(&config);
    info!("Regenerated API key");
    config.api_key
}

/// Update CORS origins in the persisted config file.
/// Loads the current config, replaces cors_origins, and saves.
pub fn save_cors_origins(origins: &[String]) -> anyhow::Result<()> {
    let mut config = load_from_disk();
    config.cors_origins = origins.to_vec();
    save_to_disk(&config)
}
