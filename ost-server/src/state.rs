//! Application state management

use crate::replay::ReplayState;
use ost_core::{adapter::TelemetryAdapter, model::TelemetryFrame};
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};
use tokio_util::sync::CancellationToken;

/// Shared application state
#[derive(Clone)]
pub struct AppState {
    /// All registered adapters
    pub adapters: Arc<RwLock<Vec<Box<dyn TelemetryAdapter>>>>,

    /// Name of the currently active adapter
    pub active_adapter: Arc<RwLock<Option<String>>>,

    /// Broadcast channel for telemetry frames
    /// Multiple consumers can subscribe to receive frames
    pub telemetry_tx: broadcast::Sender<TelemetryFrame>,

    /// Sinks for forwarding telemetry data
    pub sinks: Arc<RwLock<Vec<SinkConfig>>>,

    /// Active replay state (None when not in replay mode)
    pub replay: Arc<RwLock<Option<ReplayState>>>,

    /// Cancellation token for the replay playback task
    pub replay_cancel: Arc<RwLock<Option<CancellationToken>>>,
}

/// Configuration for an output sink
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct SinkConfig {
    pub id: String,
    pub sink_type: SinkType,
    pub field_mask: Option<String>, // Comma-separated field names
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SinkType {
    Http { url: String },
    Udp { host: String, port: u16 },
    File { path: String },
}

impl AppState {
    pub fn new() -> Self {
        // Create broadcast channel with capacity for 100 frames
        let (telemetry_tx, _) = broadcast::channel(100);

        Self {
            adapters: Arc::new(RwLock::new(Vec::new())),
            active_adapter: Arc::new(RwLock::new(None)),
            telemetry_tx,
            sinks: Arc::new(RwLock::new(Vec::new())),
            replay: Arc::new(RwLock::new(None)),
            replay_cancel: Arc::new(RwLock::new(None)),
        }
    }

    /// Register an adapter
    pub async fn register_adapter(&self, adapter: Box<dyn TelemetryAdapter>) {
        let mut adapters = self.adapters.write().await;
        adapters.push(adapter);
    }

    /// Subscribe to telemetry frames
    pub fn subscribe(&self) -> broadcast::Receiver<TelemetryFrame> {
        self.telemetry_tx.subscribe()
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}
