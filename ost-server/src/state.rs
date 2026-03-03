//! Application state management

use crate::history::HistoryBuffer;
use crate::persistence::PersistenceConfig;
use crate::replay::ReplayState;
use crate::sessions::SessionStore;
use ost_core::{adapter::TelemetryAdapter, model::TelemetryFrame};
use std::collections::{BTreeMap, HashMap, HashSet};
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

    /// History buffer for seek-back through recent live telemetry
    pub history: Arc<RwLock<HistoryBuffer>>,

    /// Persistence configuration for auto-saving telemetry to disk
    pub persistence_config: Arc<RwLock<PersistenceConfig>>,

    /// Optional API authentication token (from OST_AUTH_TOKEN env var)
    pub auth_token: Option<String>,

    /// User-submitted custom metrics (std RwLock for sync access in SSE filter_map)
    pub custom_metrics: Arc<std::sync::RwLock<CustomMetrics>>,

    /// Annotations on the telemetry timeline
    pub annotations: Arc<std::sync::RwLock<Vec<Annotation>>>,

    /// Broadcast channel for annotation updates (serialized JSON strings)
    pub annotations_tx: broadcast::Sender<String>,

    /// Whether the server is running in serve mode (--serve flag)
    pub serve_mode: bool,

    /// Session store for serve mode (None when not in serve mode)
    pub session_store: Option<Arc<SessionStore>>,

    /// Admin credentials for serve mode (from OST_ADMIN_USER / OST_ADMIN_PASS)
    pub admin_user: Option<String>,
    pub admin_pass: Option<String>,
}

/// Storage for user-submitted custom metrics.
///
/// Sticky metrics (no tick) are merged into every frame.
/// Tick-specific metrics are merged only into frames with a matching tick number.
#[derive(Default)]
pub struct CustomMetrics {
    /// Namespace → JSON object, merged into every frame
    pub sticky: HashMap<String, serde_json::Value>,
    /// Tick → Namespace → JSON object, merged into frames with matching tick
    pub by_tick: BTreeMap<u32, HashMap<String, serde_json::Value>>,
}

impl CustomMetrics {
    /// Merge custom metrics into a frame JSON value.
    /// `tick` is the frame's tick number (from meta.tick), used for tick-specific lookups.
    pub fn merge_into(&self, frame_json: &mut serde_json::Value, tick: Option<u32>) {
        let obj = match frame_json.as_object_mut() {
            Some(o) => o,
            None => return,
        };
        // Merge sticky metrics
        for (ns, data) in &self.sticky {
            obj.insert(ns.clone(), data.clone());
        }
        // Merge tick-specific metrics
        if let Some(tick) = tick {
            if let Some(tick_metrics) = self.by_tick.get(&tick) {
                for (ns, data) in tick_metrics {
                    // If namespace already exists from sticky, merge fields
                    if let Some(existing) = obj.get_mut(ns) {
                        if let (Some(existing_obj), Some(new_obj)) =
                            (existing.as_object_mut(), data.as_object())
                        {
                            for (k, v) in new_obj {
                                existing_obj.insert(k.clone(), v.clone());
                            }
                            continue;
                        }
                    }
                    obj.insert(ns.clone(), data.clone());
                }
            }
        }
    }

    pub fn is_empty(&self) -> bool {
        self.sticky.is_empty() && self.by_tick.is_empty()
    }
}

/// A time-range annotation on the telemetry buffer.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Annotation {
    pub id: String,
    pub title: String,
    /// CSS color string (e.g., "#ff6b6b", "rgba(255,0,0,0.5)")
    pub color: String,
    /// Start/end as tick numbers (from meta.tick)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_tick: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_tick: Option<u32>,
    /// Start/end as session time in seconds (from meta.timestamp offset)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_time_s: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_time_s: Option<f64>,
}

/// Configuration for an output sink
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct SinkConfig {
    pub id: String,
    pub host: String,
    pub port: u16,
    pub update_rate_hz: Option<f64>,
    pub metric_mask: Option<String>, // Comma-separated metric names
}

impl AppState {
    pub fn new() -> Self {
        // Create broadcast channel with capacity for 100 frames
        let (telemetry_tx, _) = broadcast::channel(100);
        let (status_tx, _) = broadcast::channel(16);
        let (sinks_tx, _) = broadcast::channel(16);
        let (annotations_tx, _) = broadcast::channel(16);

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
            history: Arc::new(RwLock::new(HistoryBuffer::new(600))),
            persistence_config: Arc::new(RwLock::new(PersistenceConfig::default())),
            auth_token: std::env::var("OST_AUTH_TOKEN")
                .ok()
                .filter(|s| !s.is_empty()),
            custom_metrics: Arc::new(std::sync::RwLock::new(CustomMetrics::default())),
            annotations: Arc::new(std::sync::RwLock::new(Vec::new())),
            annotations_tx,
            serve_mode: false,
            session_store: None,
            admin_user: None,
            admin_pass: None,
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
