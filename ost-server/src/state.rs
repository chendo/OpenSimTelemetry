//! Application state management

use crate::replay::ReplayState;
use ost_core::{adapter::TelemetryAdapter, model::TelemetryFrame};
use std::collections::HashSet;
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};
use tokio_util::sync::CancellationToken;

/// Shared application state
#[derive(Clone)]
pub struct AppState {
    /// All registered adapters
    pub adapters: Arc<RwLock<Vec<Box<dyn TelemetryAdapter>>>>,

    /// Key of the currently active adapter
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

    /// Adapter keys that should not auto-start (e.g. "demo")
    pub disabled_adapters: Arc<RwLock<HashSet<String>>>,

    /// Broadcast channel for status updates (serialized JSON strings)
    pub status_tx: broadcast::Sender<String>,

    /// Broadcast channel for sink config updates (serialized JSON strings)
    pub sinks_tx: broadcast::Sender<String>,
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
        let (status_tx, _) = broadcast::channel(16);
        let (sinks_tx, _) = broadcast::channel(16);

        let mut disabled = HashSet::new();
        disabled.insert("demo".to_string());

        Self {
            adapters: Arc::new(RwLock::new(Vec::new())),
            active_adapter: Arc::new(RwLock::new(None)),
            telemetry_tx,
            sinks: Arc::new(RwLock::new(Vec::new())),
            replay: Arc::new(RwLock::new(None)),
            replay_cancel: Arc::new(RwLock::new(None)),
            disabled_adapters: Arc::new(RwLock::new(disabled)),
            status_tx,
            sinks_tx,
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
