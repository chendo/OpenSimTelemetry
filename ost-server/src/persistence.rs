//! Telemetry persistence — writes NDJSON+ZSTD files to disk
//!
//! Subscribes to the telemetry broadcast channel and writes frames
//! to compressed NDJSON files at a configurable frequency.

use ost_core::model::TelemetryFrame;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};
use tracing::{error, info, warn};

/// Persistence configuration
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct PersistenceConfig {
    pub enabled: bool,
    pub frequency_hz: u32,
    pub auto_save: bool,
}

impl Default for PersistenceConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            frequency_hz: 60,
            auto_save: false,
        }
    }
}

/// Get the default telemetry storage directory
pub fn telemetry_dir() -> PathBuf {
    #[cfg(target_os = "windows")]
    {
        let base = dirs::document_dir()
            .unwrap_or_else(|| dirs::home_dir().unwrap_or_else(|| PathBuf::from(".")));
        base.join("OpenSimTelemetry").join("telemetry")
    }
    #[cfg(not(target_os = "windows"))]
    {
        let base = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
        base.join(".opensimtelemetry").join("telemetry")
    }
}

/// Sanitize a string for use in a filename
fn sanitize_filename(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect::<String>()
        .trim_matches('_')
        .to_string()
}

/// Generate a filename from session info
/// Format: YYYY-MM-DD_track_car.ost.ndjson.zstd
fn generate_filename(track: &str, car: &str) -> String {
    let now = chrono::Local::now();
    let date = now.format("%Y-%m-%d_%H-%M-%S").to_string();
    let track_clean = if track.is_empty() {
        "unknown_track"
    } else {
        &sanitize_filename(track)
    };
    let car_clean = if car.is_empty() {
        "unknown_car"
    } else {
        &sanitize_filename(car)
    };
    format!("{}_{track_clean}_{car_clean}.ost.ndjson.zstd", date)
}

/// Active file writer state
struct ActiveWriter {
    encoder: zstd::Encoder<'static, std::fs::File>,
    path: PathBuf,
    frame_count: u64,
    track: String,
    car: String,
}

impl ActiveWriter {
    fn new(track: &str, car: &str) -> Result<Self, std::io::Error> {
        let dir = telemetry_dir();
        std::fs::create_dir_all(&dir)?;
        let filename = generate_filename(track, car);
        let path = dir.join(&filename);
        let file = std::fs::File::create(&path)?;
        let encoder = zstd::Encoder::new(file, 3)?; // Level 3 = good balance
        info!("Persistence: writing to {}", path.display());
        Ok(Self {
            encoder,
            path,
            frame_count: 0,
            track: track.to_string(),
            car: car.to_string(),
        })
    }

    fn write_frame(
        &mut self,
        frame: &TelemetryFrame,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let json = serde_json::to_string(frame)?;
        writeln!(self.encoder, "{}", json)?;
        self.frame_count += 1;
        // Flush every 60 frames (~1 second at 60Hz)
        if self.frame_count.is_multiple_of(60) {
            self.encoder.flush()?;
        }
        Ok(())
    }

    fn finish(mut self) -> Result<PathBuf, std::io::Error> {
        self.encoder.flush()?;
        self.encoder.finish()?;
        info!(
            "Persistence: finished {} ({} frames)",
            self.path.display(),
            self.frame_count
        );
        Ok(self.path)
    }
}

/// Run the persistence background task
pub async fn run(
    config: Arc<RwLock<PersistenceConfig>>,
    mut rx: broadcast::Receiver<TelemetryFrame>,
) {
    let mut writer: Option<ActiveWriter> = None;
    let mut frame_counter: u64 = 0;

    loop {
        let frame = match rx.recv().await {
            Ok(f) => f,
            Err(broadcast::error::RecvError::Lagged(n)) => {
                warn!("Persistence: skipped {} frames (lagged)", n);
                continue;
            }
            Err(broadcast::error::RecvError::Closed) => break,
        };

        let cfg = config.read().await.clone();
        if !cfg.auto_save {
            // If auto-save was just disabled, finish any open file
            if let Some(w) = writer.take() {
                if let Err(e) = w.finish() {
                    error!("Persistence: failed to finish file: {}", e);
                }
            }
            frame_counter = 0;
            continue;
        }

        // Compute skip interval from frequency and tick rate
        let tick_rate = frame.tick.unwrap_or(60) as u64;
        let freq = cfg.frequency_hz.max(1) as u64;
        let skip_interval = if tick_rate > freq {
            tick_rate / freq
        } else {
            1
        };

        // Extract session info
        let track = frame
            .session
            .as_ref()
            .and_then(|s| s.track_name.as_deref())
            .unwrap_or("")
            .to_string();
        let car = frame
            .session
            .as_ref()
            .and_then(|s| s.car_name.as_deref())
            .unwrap_or("")
            .to_string();

        // Check if session changed — start a new file
        let session_changed = writer
            .as_ref()
            .map(|w| (!track.is_empty() && w.track != track) || (!car.is_empty() && w.car != car))
            .unwrap_or(false);

        if session_changed {
            if let Some(w) = writer.take() {
                if let Err(e) = w.finish() {
                    error!("Persistence: failed to finish file: {}", e);
                }
            }
            frame_counter = 0;
        }

        // Open writer if needed
        if writer.is_none() {
            match ActiveWriter::new(&track, &car) {
                Ok(w) => writer = Some(w),
                Err(e) => {
                    error!("Persistence: failed to create file: {}", e);
                    continue;
                }
            }
        }

        // Write frame at configured frequency
        frame_counter += 1;
        if frame_counter.is_multiple_of(skip_interval) {
            if let Some(ref mut w) = writer {
                if let Err(e) = w.write_frame(&frame) {
                    error!("Persistence: failed to write frame: {}", e);
                }
            }
        }
    }

    // Clean up
    if let Some(w) = writer {
        if let Err(e) = w.finish() {
            error!("Persistence: failed to finish file on shutdown: {}", e);
        }
    }
}

/// Download the history buffer as NDJSON+ZSTD bytes
pub fn compress_frames(
    frames: &[TelemetryFrame],
) -> Result<Vec<u8>, Box<dyn std::error::Error + Send + Sync>> {
    let mut encoder = zstd::Encoder::new(Vec::new(), 3)?;
    for frame in frames {
        let json = serde_json::to_string(frame)?;
        writeln!(encoder, "{}", json)?;
    }
    let compressed = encoder.finish()?;
    Ok(compressed)
}
