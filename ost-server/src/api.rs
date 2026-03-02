//! REST API and SSE routes

use crate::replay::ReplayState;
use crate::state::{AppState, SinkConfig};
use crate::web_ui;
use axum::{
    extract::{DefaultBodyLimit, Multipart, Query, State},
    http::{header, StatusCode},
    middleware::Next,
    response::{
        sse::{Event, KeepAlive, Sse},
        IntoResponse,
    },
    routing::{delete, get, post},
    Json, Router,
};
use futures::stream::{self, Stream, StreamExt as FuturesStreamExt};
use ost_core::model::{MetricMask, TelemetryFrame};
use serde::{Deserialize, Serialize};
use std::convert::Infallible;
use std::time::Duration;
use tokio_stream::wrappers::BroadcastStream;
use tokio_util::sync::CancellationToken;
use tower_http::cors::CorsLayer;

/// Convert a rate (frames per second) query param to a minimum interval between emissions.
/// Returns None for rates >= 60 (no throttling needed).
fn rate_to_interval(rate: Option<f64>) -> Option<Duration> {
    let hz = rate.unwrap_or(60.0).clamp(0.01, 60.0);
    if hz >= 60.0 {
        None
    } else {
        Some(Duration::from_secs_f64(1.0 / hz))
    }
}

/// Adaptive throttle state for SSE streams.
/// Tracks client lag and dynamically adjusts the frame skip interval.
struct AdaptiveThrottle {
    base_interval: Option<Duration>,
    /// Extra skip multiplier (1 = no throttle, 2 = skip every other, etc.)
    skip_multiplier: u32,
    /// Frames received since last lag event (used to relax throttle)
    frames_since_lag: u64,
}

impl AdaptiveThrottle {
    fn new(base_interval: Option<Duration>) -> Self {
        Self {
            base_interval,
            skip_multiplier: 1,
            frames_since_lag: 0,
        }
    }

    /// Called when a lag event is detected (client fell behind by `n` frames).
    /// Returns the new effective FPS for logging.
    fn on_lag(&mut self, _dropped: u64) -> u32 {
        self.skip_multiplier = (self.skip_multiplier * 2).min(8);
        self.frames_since_lag = 0;
        self.effective_fps()
    }

    /// Called on every successfully received frame.
    fn on_frame_received(&mut self) {
        self.frames_since_lag += 1;
        // Relax throttle after 300 consecutive frames (~5 seconds at 60fps)
        if self.skip_multiplier > 1 && self.frames_since_lag > 300 {
            self.skip_multiplier = (self.skip_multiplier / 2).max(1);
            self.frames_since_lag = 0;
        }
    }

    /// Get the effective minimum interval between emitted frames.
    fn effective_interval(&self) -> Option<Duration> {
        if self.skip_multiplier <= 1 {
            self.base_interval
        } else {
            let base = self.base_interval.unwrap_or(Duration::from_millis(16));
            Some(base * self.skip_multiplier)
        }
    }

    fn effective_fps(&self) -> u32 {
        match self.effective_interval() {
            Some(d) => (1.0 / d.as_secs_f64()).round() as u32,
            None => 60,
        }
    }
}

/// Round all floating-point numbers in a JSON value tree to 5 decimal places.
fn round_json_floats(val: &mut serde_json::Value) {
    match val {
        serde_json::Value::Number(n) => {
            if let Some(f) = n.as_f64() {
                // Only round actual floats (skip integers)
                if n.is_f64() {
                    let rounded = (f * 100_000.0).round() / 100_000.0;
                    *val = serde_json::json!(rounded);
                }
            }
        }
        serde_json::Value::Array(arr) => {
            for item in arr.iter_mut() {
                round_json_floats(item);
            }
        }
        serde_json::Value::Object(map) => {
            for (_, v) in map.iter_mut() {
                round_json_floats(v);
            }
        }
        _ => {}
    }
}

/// Check if a Basic auth header matches the token (password field).
fn check_basic_auth(auth_header: &str, token: &str) -> bool {
    if let Some(encoded) = auth_header.strip_prefix("Basic ") {
        use base64::Engine;
        if let Ok(decoded) = base64::engine::general_purpose::STANDARD.decode(encoded) {
            if let Ok(credentials) = std::str::from_utf8(&decoded) {
                // Format is "username:password" — we only check the password
                if let Some(password) = credentials.split_once(':').map(|(_, p)| p) {
                    return password == token;
                }
            }
        }
    }
    false
}

/// Auth middleware: checks token on all routes when auth is configured.
/// Supports Bearer token, Basic auth, and ?token= query parameter.
/// The UI page (/) triggers a browser Basic auth prompt on 401.
async fn auth_middleware(
    State(state): State<AppState>,
    req: axum::http::Request<axum::body::Body>,
    next: Next,
) -> Result<axum::response::Response, axum::response::Response> {
    let token = match &state.auth_token {
        Some(t) => t,
        None => return Ok(next.run(req).await),
    };

    // Check Authorization header (Bearer or Basic)
    if let Some(auth) = req.headers().get(header::AUTHORIZATION) {
        if let Ok(val) = auth.to_str() {
            if val.strip_prefix("Bearer ").is_some_and(|t| t == token)
                || check_basic_auth(val, token)
            {
                return Ok(next.run(req).await);
            }
        }
    }

    // Check ?token= query param
    if let Some(query) = req.uri().query() {
        for pair in query.split('&') {
            if let Some(val) = pair.strip_prefix("token=") {
                if val == token {
                    return Ok(next.run(req).await);
                }
            }
        }
    }

    // Return 401 with WWW-Authenticate header to trigger browser Basic auth prompt
    Err((
        StatusCode::UNAUTHORIZED,
        [(header::WWW_AUTHENTICATE, "Basic realm=\"OpenSimTelemetry\"")],
    )
        .into_response())
}

/// Create the main application router
pub fn create_router(state: AppState) -> Router {
    Router::new()
        .route("/", get(web_ui::serve_ui))
        .route("/api/docs", get(api_docs))
        .route("/api/adapters", get(list_adapters))
        .route("/api/adapters/:name/toggle", post(toggle_adapter))
        .route("/api/stream", get(unified_stream))
        .route("/api/telemetry/stream", get(telemetry_stream))
        .route("/api/status/stream", get(status_stream))
        .route("/api/metrics", get(get_metrics))
        .route("/api/sinks", get(list_sinks).post(create_sink))
        .route("/api/sinks/stream", get(sinks_stream))
        .route("/api/sinks/:id", delete(delete_sink))
        // Replay endpoints
        .route(
            "/api/replay/upload",
            post(replay_upload).layer(DefaultBodyLimit::max(1024 * 1024 * 1024)),
        )
        .route("/api/replay/info", get(replay_info))
        .route("/api/replay/frames", get(replay_frames))
        .route("/api/replay/control", post(replay_control))
        .route("/api/replay", delete(replay_delete))
        // History buffer config & aggregation
        .route("/api/history/config", post(history_config))
        .route("/api/history/aggregate", get(history_aggregate))
        // Conversion endpoints
        .route(
            "/api/convert/ibt",
            post(convert_ibt).layer(DefaultBodyLimit::max(1024 * 1024 * 1024)),
        )
        // Persistence endpoints
        .route(
            "/api/persistence/config",
            get(persistence_get_config).post(persistence_set_config),
        )
        .route("/api/persistence/download", get(persistence_download))
        .route("/api/persistence/stats", get(persistence_stats))
        .route("/api/persistence/files", get(persistence_list_files))
        .route("/api/persistence/load", post(persistence_load_file))
        .route(
            "/api/persistence/files/:name",
            delete(persistence_delete_file),
        )
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            auth_middleware,
        ))
        .layer(CorsLayer::permissive())
        .with_state(state)
}

// === Adapter Endpoints ===

#[derive(Serialize)]
struct AdapterInfo {
    key: String,
    name: String,
    detected: bool,
    active: bool,
    enabled: bool,
}

async fn list_adapters(State(state): State<AppState>) -> Json<Vec<AdapterInfo>> {
    let adapters = state.adapters.read().await;
    let active_name = state.active_adapter.read().await;
    let disabled = state.disabled_adapters.read().await;

    let info: Vec<AdapterInfo> = adapters
        .iter()
        .map(|adapter| AdapterInfo {
            key: adapter.key().to_string(),
            name: adapter.name().to_string(),
            detected: adapter.detect(),
            active: adapter.is_active()
                || active_name
                    .as_ref()
                    .map(|n| n == adapter.key())
                    .unwrap_or(false),
            enabled: !disabled.contains(adapter.key()),
        })
        .collect();

    Json(info)
}

async fn toggle_adapter(
    State(state): State<AppState>,
    axum::extract::Path(key): axum::extract::Path<String>,
) -> Result<Json<AdapterInfo>, (StatusCode, String)> {
    let result = {
        let mut adapters = state.adapters.write().await;
        let mut active_adapter = state.active_adapter.write().await;
        let mut disabled = state.disabled_adapters.write().await;

        let adapter = adapters.iter_mut().find(|a| a.key() == key).ok_or((
            StatusCode::NOT_FOUND,
            format!("Adapter '{}' not found", key),
        ))?;

        let is_enabled = !disabled.contains(adapter.key());

        if is_enabled {
            // Disable: stop if active, add to disabled set
            let is_active = adapter.is_active()
                || active_adapter
                    .as_ref()
                    .map(|n| n == adapter.key())
                    .unwrap_or(false);
            if is_active {
                let _ = adapter.stop();
                *active_adapter = None;
            }
            disabled.insert(key.clone());
            Ok(Json(AdapterInfo {
                key: adapter.key().to_string(),
                name: adapter.name().to_string(),
                detected: adapter.detect(),
                active: false,
                enabled: false,
            }))
        } else {
            // Enable: remove from disabled set, let detection loop handle starting
            disabled.remove(&key);
            Ok(Json(AdapterInfo {
                key: adapter.key().to_string(),
                name: adapter.name().to_string(),
                detected: adapter.detect(),
                active: false,
                enabled: true,
            }))
        }
    };
    // Broadcast status update after locks are released
    broadcast_adapter_status(&state).await;
    result
}

/// Build the current adapter status JSON and broadcast it to all status SSE subscribers.
/// Called after any adapter state change (toggle, auto-detect start/stop).
pub async fn broadcast_adapter_status(state: &AppState) {
    let adapters = state.adapters.read().await;
    let active_name = state.active_adapter.read().await;
    let disabled = state.disabled_adapters.read().await;

    let info: Vec<AdapterInfo> = adapters
        .iter()
        .map(|adapter| AdapterInfo {
            key: adapter.key().to_string(),
            name: adapter.name().to_string(),
            detected: adapter.detect(),
            active: adapter.is_active()
                || active_name
                    .as_ref()
                    .map(|n| n == adapter.key())
                    .unwrap_or(false),
            enabled: !disabled.contains(adapter.key()),
        })
        .collect();

    if let Ok(json) = serde_json::to_string(&info) {
        let _ = state.status_tx.send(json);
    }
}

/// GET /api/metrics — returns the latest telemetry frame as JSON.
/// Accepts optional `metric_mask` query param to filter top-level sections.
#[derive(Deserialize)]
struct MetricsQuery {
    metric_mask: Option<String>,
}

async fn get_metrics(
    State(state): State<AppState>,
    Query(query): Query<MetricsQuery>,
) -> impl IntoResponse {
    let history = state.history.read().await;
    match history.latest_frame() {
        Some(frame) => {
            let mask = query.metric_mask.as_deref().map(MetricMask::parse);
            let json = frame
                .to_json_filtered(mask.as_ref())
                .unwrap_or_else(|_| "{}".to_string());
            (
                StatusCode::OK,
                [(header::CONTENT_TYPE, "application/json")],
                json,
            )
        }
        None => (
            StatusCode::OK,
            [(header::CONTENT_TYPE, "application/json")],
            "null".to_string(),
        ),
    }
}

/// Broadcast the current sink config list to all sink SSE subscribers.
async fn broadcast_sinks(state: &AppState) {
    let sinks = state.sinks.read().await;
    if let Ok(json) = serde_json::to_string(&*sinks) {
        let _ = state.sinks_tx.send(json);
    }
}

/// Unified SSE endpoint that multiplexes telemetry, status, and sinks events
/// over a single connection. Uses named events: "frame", "status", "sinks".
/// This avoids consuming multiple HTTP/1.1 connection slots (browsers limit to 6).
async fn unified_stream(
    State(state): State<AppState>,
    Query(query): Query<StreamQuery>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    // Build initial status
    let initial_status_json = {
        let adapters = state.adapters.read().await;
        let active_name = state.active_adapter.read().await;
        let disabled = state.disabled_adapters.read().await;
        let info: Vec<AdapterInfo> = adapters
            .iter()
            .map(|adapter| AdapterInfo {
                key: adapter.key().to_string(),
                name: adapter.name().to_string(),
                detected: adapter.detect(),
                active: adapter.is_active()
                    || active_name
                        .as_ref()
                        .map(|n| n == adapter.key())
                        .unwrap_or(false),
                enabled: !disabled.contains(adapter.key()),
            })
            .collect();
        serde_json::to_string(&info).unwrap_or_default()
    };

    // Build initial sinks
    let initial_sinks_json = {
        let sinks = state.sinks.read().await;
        serde_json::to_string(&*sinks).unwrap_or_default()
    };

    // Subscribe to all channels
    let telemetry_rx = state.subscribe();
    let status_rx = state.status_tx.subscribe();
    let sinks_rx = state.sinks_tx.subscribe();

    // Initial events
    let initial = stream::iter(vec![
        Ok(Event::default().event("status").data(initial_status_json)),
        Ok(Event::default().event("sinks").data(initial_sinks_json)),
    ]);

    // Telemetry frames (with optional metric mask filtering and rate limiting)
    let metric_mask = query.metric_mask.map(|f| MetricMask::parse(&f));
    let min_interval = rate_to_interval(query.rate);
    let use_msgpack = query
        .format
        .as_deref()
        .is_some_and(|f| f.eq_ignore_ascii_case("msgpack"));
    // Adaptive throttling state: tracks lag and dynamically adjusts skip rate
    let throttle_state =
        std::sync::Arc::new(std::sync::Mutex::new(AdaptiveThrottle::new(min_interval)));
    let last_emit = std::sync::Arc::new(std::sync::Mutex::new(tokio::time::Instant::now()));
    let telemetry = BroadcastStream::new(telemetry_rx).filter_map(move |result| {
        let mask = metric_mask.clone();
        let last = last_emit.clone();
        let throttle = throttle_state.clone();
        async move {
            match result {
                Ok(frame) => {
                    let mut ts = throttle.lock().unwrap();
                    ts.on_frame_received();
                    let effective_interval = ts.effective_interval();
                    drop(ts);

                    if let Some(interval) = effective_interval {
                        let mut guard = last.lock().unwrap();
                        if guard.elapsed() < interval {
                            return None;
                        }
                        *guard = tokio::time::Instant::now();
                    }
                    if use_msgpack {
                        serialize_frame_msgpack(&frame, mask.as_ref())
                    } else {
                        match frame.to_json_filtered(mask.as_ref()) {
                            Ok(json) => Some(Ok(Event::default().event("frame").data(json))),
                            Err(_) => None,
                        }
                    }
                }
                Err(tokio_stream::wrappers::errors::BroadcastStreamRecvError::Lagged(n)) => {
                    let mut ts = throttle.lock().unwrap();
                    let effective_fps = ts.on_lag(n);
                    tracing::debug!(
                        "Client lagged {} frames, throttled to ~{} fps",
                        n,
                        effective_fps
                    );
                    Some(Ok(
                        Event::default().comment(format!("throttled to {}fps", effective_fps))
                    ))
                }
            }
        }
    });

    // Status updates
    let status = BroadcastStream::new(status_rx).filter_map(|result| async move {
        match result {
            Ok(json) => Some(Ok(Event::default().event("status").data(json))),
            Err(_) => None,
        }
    });

    // Sinks updates
    let sinks = BroadcastStream::new(sinks_rx).filter_map(|result| async move {
        match result {
            Ok(json) => Some(Ok(Event::default().event("sinks").data(json))),
            Err(_) => None,
        }
    });

    // Merge all streams using select (round-robin polling)
    let merged = futures::stream::select(
        futures::stream::select(initial.chain(telemetry), status),
        sinks,
    );

    Sse::new(merged).keep_alive(KeepAlive::default())
}

/// SSE endpoint that pushes sink config updates in real-time.
/// Sends the current state immediately on connect, then on every change.
async fn sinks_stream(
    State(state): State<AppState>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let sinks = state.sinks.read().await;
    let initial_json = serde_json::to_string(&*sinks).unwrap_or_default();
    drop(sinks);

    let rx = state.sinks_tx.subscribe();
    let updates = BroadcastStream::new(rx).filter_map(|result| async move {
        match result {
            Ok(json) => Some(Ok(Event::default().data(json))),
            Err(_) => None,
        }
    });

    let initial_event = stream::once(async move { Ok(Event::default().data(initial_json)) });

    Sse::new(initial_event.chain(updates)).keep_alive(KeepAlive::default())
}

/// SSE endpoint that pushes adapter status updates in real-time.
/// Sends the current state immediately on connect, then on every change.
async fn status_stream(
    State(state): State<AppState>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    // Build initial status to send immediately
    let adapters = state.adapters.read().await;
    let active_name = state.active_adapter.read().await;
    let disabled = state.disabled_adapters.read().await;

    let initial: Vec<AdapterInfo> = adapters
        .iter()
        .map(|adapter| AdapterInfo {
            key: adapter.key().to_string(),
            name: adapter.name().to_string(),
            detected: adapter.detect(),
            active: adapter.is_active()
                || active_name
                    .as_ref()
                    .map(|n| n == adapter.key())
                    .unwrap_or(false),
            enabled: !disabled.contains(adapter.key()),
        })
        .collect();
    drop(adapters);
    drop(active_name);
    drop(disabled);

    let initial_json = serde_json::to_string(&initial).unwrap_or_default();

    let rx = state.status_tx.subscribe();
    let updates = BroadcastStream::new(rx).filter_map(|result| async move {
        match result {
            Ok(json) => Some(Ok(Event::default().data(json))),
            Err(_) => None,
        }
    });

    // Prepend the initial state event
    let initial_event = stream::once(async move { Ok(Event::default().data(initial_json)) });

    Sse::new(initial_event.chain(updates)).keep_alive(KeepAlive::default())
}

// === Telemetry Stream Endpoint ===

#[derive(Deserialize)]
struct StreamQuery {
    metric_mask: Option<String>,
    /// Frames per second (0.0–60.0). Defaults to 60.
    rate: Option<f64>,
    /// Wire format: "json" (default) or "msgpack" (base64-encoded MessagePack)
    format: Option<String>,
}

async fn telemetry_stream(
    State(state): State<AppState>,
    Query(query): Query<StreamQuery>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let rx = state.subscribe();
    let metric_mask = query.metric_mask.map(|f| MetricMask::parse(&f));
    let min_interval = rate_to_interval(query.rate);
    let use_msgpack = query
        .format
        .as_deref()
        .is_some_and(|f| f.eq_ignore_ascii_case("msgpack"));

    let throttle_state =
        std::sync::Arc::new(std::sync::Mutex::new(AdaptiveThrottle::new(min_interval)));
    let last_emit = std::sync::Arc::new(std::sync::Mutex::new(tokio::time::Instant::now()));
    let stream = BroadcastStream::new(rx).filter_map(move |result| {
        let mask = metric_mask.clone();
        let last = last_emit.clone();
        let throttle = throttle_state.clone();
        async move {
            match result {
                Ok(frame) => {
                    let mut ts = throttle.lock().unwrap();
                    ts.on_frame_received();
                    let effective_interval = ts.effective_interval();
                    drop(ts);

                    if let Some(interval) = effective_interval {
                        let mut guard = last.lock().unwrap();
                        if guard.elapsed() < interval {
                            return None;
                        }
                        *guard = tokio::time::Instant::now();
                    }
                    if use_msgpack {
                        serialize_frame_msgpack(&frame, mask.as_ref())
                    } else {
                        match frame.to_json_filtered(mask.as_ref()) {
                            Ok(json) => Some(Ok(Event::default().data(json))),
                            Err(e) => {
                                tracing::error!("Failed to serialize frame: {}", e);
                                None
                            }
                        }
                    }
                }
                Err(tokio_stream::wrappers::errors::BroadcastStreamRecvError::Lagged(n)) => {
                    let mut ts = throttle.lock().unwrap();
                    ts.on_lag(n);
                    None
                }
            }
        }
    });

    Sse::new(stream).keep_alive(KeepAlive::default())
}

/// Serialize a frame to base64-encoded MessagePack for SSE transport.
fn serialize_frame_msgpack(
    frame: &TelemetryFrame,
    mask: Option<&MetricMask>,
) -> Option<Result<Event, Infallible>> {
    // If there's a metric mask, filter through JSON first then serialize the filtered value
    let bytes = if let Some(mask) = mask {
        let json_str = frame.to_json_filtered(Some(mask)).ok()?;
        let val: serde_json::Value = serde_json::from_str(&json_str).ok()?;
        rmp_serde::to_vec(&val).ok()?
    } else {
        rmp_serde::to_vec(frame).ok()?
    };
    use base64::Engine;
    let encoded = base64::engine::general_purpose::STANDARD.encode(&bytes);
    Some(Ok(Event::default().event("msgpack").data(encoded)))
}

// === Sink Management Endpoints ===

async fn list_sinks(State(state): State<AppState>) -> Json<Vec<SinkConfig>> {
    let sinks = state.sinks.read().await;
    Json(sinks.clone())
}

#[derive(Deserialize)]
struct CreateSinkRequest {
    #[serde(flatten)]
    config: SinkConfig,
}

async fn create_sink(
    State(state): State<AppState>,
    Json(request): Json<CreateSinkRequest>,
) -> impl IntoResponse {
    let config = {
        let mut sinks = state.sinks.write().await;

        // Generate ID if not provided
        let mut config = request.config;
        if config.id.is_empty() {
            config.id = format!("sink-{}", sinks.len() + 1);
        }

        sinks.push(config.clone());
        config
    };
    broadcast_sinks(&state).await;

    (StatusCode::CREATED, Json(config))
}

async fn delete_sink(
    State(state): State<AppState>,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Result<StatusCode, StatusCode> {
    {
        let mut sinks = state.sinks.write().await;
        if let Some(pos) = sinks.iter().position(|s| s.id == id) {
            sinks.remove(pos);
        } else {
            return Err(StatusCode::NOT_FOUND);
        }
    }
    broadcast_sinks(&state).await;
    Ok(StatusCode::NO_CONTENT)
}

// === Replay Endpoints ===

/// Handle .ibt file upload, create replay state, and start playback
async fn replay_upload(
    State(state): State<AppState>,
    mut multipart: Multipart,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    {
        let replay = state.replay.read().await;
        if replay.is_some() {
            return Err((
                StatusCode::CONFLICT,
                "A replay is already active. Delete it first.".to_string(),
            ));
        }
    }

    let field = multipart
        .next_field()
        .await
        .map_err(|e| {
            (
                StatusCode::BAD_REQUEST,
                format!("Failed to read upload: {}", e),
            )
        })?
        .ok_or((StatusCode::BAD_REQUEST, "No file provided".to_string()))?;

    let file_name = field.file_name().unwrap_or("upload.ibt").to_string();

    if !file_name.to_lowercase().ends_with(".ibt") {
        return Err((
            StatusCode::BAD_REQUEST,
            "Only .ibt files are supported".to_string(),
        ));
    }

    let data = field.bytes().await.map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            format!("Failed to read file data: {}", e),
        )
    })?;

    tracing::info!("Received .ibt file: {} ({} bytes)", file_name, data.len());

    // Move blocking file I/O off the async runtime to avoid starving
    // SSE keep-alive events and other async tasks
    let replay_state = tokio::task::spawn_blocking(move || {
        let temp_dir = std::env::temp_dir().join("ost-replay");
        std::fs::create_dir_all(&temp_dir).map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to create temp dir: {}", e),
            )
        })?;

        let temp_path = temp_dir.join(&file_name);
        std::fs::write(&temp_path, &data).map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to write temp file: {}", e),
            )
        })?;

        ReplayState::from_file(&temp_path).map_err(|e| {
            let _ = std::fs::remove_file(&temp_path);
            (
                StatusCode::BAD_REQUEST,
                format!("Failed to parse .ibt file: {}", e),
            )
        })
    })
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("File processing failed: {}", e),
        )
    })??;

    let info = replay_state.info();

    {
        let mut replay = state.replay.write().await;
        *replay = Some(replay_state);
    }

    start_playback_task(state.clone()).await;

    Ok(Json(serde_json::json!({
        "status": "ok",
        "info": info
    })))
}

async fn replay_info(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let replay = state.replay.read().await;
    if let Some(rs) = &*replay {
        let mut info = serde_json::to_value(rs.info()).unwrap();
        info.as_object_mut()
            .unwrap()
            .insert("mode".into(), "replay".into());
        Ok(Json(info))
    } else {
        drop(replay);
        let history = state.history.read().await;
        Ok(Json(serde_json::json!({
            "mode": "history",
            "total_frames": history.frame_count(),
            "tick_rate": history.tick_rate(),
            "duration_secs": history.duration_secs(),
            "current_frame": history.frame_count().saturating_sub(1),
            "playing": false,
            "playback_speed": 1.0,
            "track_name": history.track_name(),
            "car_name": history.car_name(),
            "file_size": 0,
            "laps": history.laps(),
            "replay_id": "",
            "paused": history.is_paused(),
            "estimated_memory_mb": history.estimated_memory_mb(),
            "max_duration_secs": history.max_duration_secs(),
        })))
    }
}

#[derive(Deserialize)]
struct ReplayFramesQuery {
    start: usize,
    count: usize,
    metric_mask: Option<String>,
    /// Replay ID for cache-busting; when present, response is immutable-cached
    rid: Option<String>,
}

async fn replay_frames(
    State(state): State<AppState>,
    Query(params): Query<ReplayFramesQuery>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let replay = state.replay.read().await;
    if let Some(rs) = replay.as_ref() {
        // Serve from replay file
        let frames = rs
            .get_frames_range(params.start, params.count)
            .map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Failed to read frames: {}", e),
                )
            })?;

        let metric_mask = params.metric_mask.map(|f| MetricMask::parse(&f));
        let json_frames = serialize_frames(frames.into_iter(), &metric_mask);

        // When a replay_id is in the URL, the response is content-addressed and immutable
        let cache_header = if params.rid.is_some() {
            "public, max-age=31536000, immutable"
        } else {
            "no-cache"
        };

        Ok((
            [(header::CACHE_CONTROL, cache_header)],
            Json(serde_json::json!(json_frames)),
        ))
    } else {
        // Serve from history buffer
        drop(replay);
        let history = state.history.read().await;
        let frames = history.get_frames_range(params.start, params.count);

        let metric_mask = params.metric_mask.map(|f| MetricMask::parse(&f));
        let json_frames = serialize_frames(
            frames.into_iter().map(|(i, f)| (i, f.clone())),
            &metric_mask,
        );

        Ok((
            [(header::CACHE_CONTROL, "no-cache")],
            Json(serde_json::json!(json_frames)),
        ))
    }
}

/// Serialize frames with optional metric mask filtering, shared by replay and history.
fn serialize_frames(
    frames: impl Iterator<Item = (usize, TelemetryFrame)>,
    metric_mask: &Option<MetricMask>,
) -> Vec<serde_json::Value> {
    frames
        .map(|(idx, frame)| {
            let mut f_val = if let Some(ref mask) = metric_mask {
                let json_str = frame.to_json_filtered(Some(mask)).unwrap_or_default();
                serde_json::from_str(&json_str).unwrap_or(serde_json::Value::Null)
            } else {
                serde_json::to_value(&frame).unwrap_or(serde_json::Value::Null)
            };
            round_json_floats(&mut f_val);
            serde_json::json!({
                "i": idx,
                "f": f_val
            })
        })
        .collect()
}

#[derive(Deserialize)]
struct ReplayControlRequest {
    action: String,
    value: Option<f64>,
}

async fn replay_control(
    State(state): State<AppState>,
    Json(request): Json<ReplayControlRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let mut replay = state.replay.write().await;
    if let Some(rs) = replay.as_mut() {
        // Control active replay
        match request.action.as_str() {
            "play" => {
                rs.play();
                drop(replay);
                start_playback_task(state.clone()).await;
                Ok(Json(serde_json::json!({"status": "playing"})))
            }
            "pause" => {
                rs.pause();
                Ok(Json(serde_json::json!({"status": "paused"})))
            }
            "seek" => {
                let frame = request.value.ok_or((
                    StatusCode::BAD_REQUEST,
                    "Missing 'value' for seek".to_string(),
                ))? as usize;
                rs.seek(frame);
                Ok(Json(
                    serde_json::json!({"status": "seeked", "frame": rs.current_frame()}),
                ))
            }
            "speed" => {
                let speed = request.value.ok_or((
                    StatusCode::BAD_REQUEST,
                    "Missing 'value' for speed".to_string(),
                ))?;
                rs.set_speed(speed);
                Ok(Json(
                    serde_json::json!({"status": "speed_set", "speed": rs.playback_speed()}),
                ))
            }
            _ => Err((
                StatusCode::BAD_REQUEST,
                format!("Unknown action: {}", request.action),
            )),
        }
    } else {
        // Control history buffer (pause/resume buffering)
        drop(replay);
        let mut history = state.history.write().await;
        match request.action.as_str() {
            "pause" => {
                history.set_paused(true);
                Ok(Json(serde_json::json!({"status": "paused"})))
            }
            "play" | "resume" => {
                history.set_paused(false);
                Ok(Json(serde_json::json!({"status": "buffering"})))
            }
            _ => Ok(Json(serde_json::json!({"status": "ok"}))),
        }
    }
}

async fn replay_delete(State(state): State<AppState>) -> Result<StatusCode, (StatusCode, String)> {
    {
        let mut cancel = state.replay_cancel.write().await;
        if let Some(token) = cancel.take() {
            token.cancel();
        }
    }

    {
        let mut replay = state.replay.write().await;
        if replay.is_none() {
            return Err((StatusCode::NOT_FOUND, "No active replay".to_string()));
        }
        *replay = None;
    }

    tracing::info!("Replay stopped and cleaned up");
    Ok(StatusCode::NO_CONTENT)
}

// === History Config ===

#[derive(Deserialize)]
struct HistoryConfigRequest {
    max_duration_secs: u32,
}

async fn history_config(
    State(state): State<AppState>,
    Json(req): Json<HistoryConfigRequest>,
) -> Json<serde_json::Value> {
    let clamped = req.max_duration_secs.clamp(60, 3600);
    let mut history = state.history.write().await;
    history.resize(clamped);
    Json(serde_json::json!({"status": "ok", "max_duration_secs": clamped}))
}

// === History Aggregation ===

#[derive(Deserialize)]
struct AggregateQuery {
    /// Duration to aggregate over, e.g. "60s", "5m", "1h". Defaults to 60s.
    duration: Option<String>,
    /// Comma-separated metric paths, e.g. "vehicle.speed,engine.rpm"
    metrics: String,
}

/// Parse a human-readable duration string into seconds.
/// Supports "60s", "5m", "1h", or bare numbers (treated as seconds).
fn parse_duration_str(s: &str) -> f64 {
    let s = s.trim();
    if let Some(secs) = s.strip_suffix('s') {
        secs.parse().unwrap_or(60.0)
    } else if let Some(mins) = s.strip_suffix('m') {
        mins.parse::<f64>().unwrap_or(1.0) * 60.0
    } else if let Some(hours) = s.strip_suffix('h') {
        hours.parse::<f64>().unwrap_or(1.0) * 3600.0
    } else {
        s.parse().unwrap_or(60.0)
    }
}

/// Extract a numeric value from a TelemetryFrame by dot-separated path.
/// e.g. "vehicle.speed" → frame.vehicle.speed, "engine.rpm" → frame.engine.rpm
fn extract_metric_value(frame: &TelemetryFrame, path: &str) -> Option<f64> {
    let json = serde_json::to_value(frame).ok()?;
    let mut current = &json;
    for part in path.split('.') {
        current = current.get(part)?;
    }
    current.as_f64()
}

async fn history_aggregate(
    State(state): State<AppState>,
    Query(params): Query<AggregateQuery>,
) -> Json<serde_json::Value> {
    let duration_secs = parse_duration_str(&params.duration.unwrap_or_else(|| "60s".to_string()));
    let history = state.history.read().await;
    let frames = history.get_frames_since_secs(duration_secs);

    let metrics: Vec<&str> = params.metrics.split(',').map(|s| s.trim()).collect();
    let mut result = serde_json::Map::new();

    for metric_path in &metrics {
        let values: Vec<f64> = frames
            .iter()
            .filter_map(|f| extract_metric_value(f, metric_path))
            .collect();

        if values.is_empty() {
            continue;
        }

        let count = values.len();
        let min = values.iter().copied().fold(f64::INFINITY, f64::min);
        let max = values.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        let sum: f64 = values.iter().sum();
        let avg = sum / count as f64;
        let variance = values.iter().map(|v| (v - avg).powi(2)).sum::<f64>() / count as f64;
        let stddev = variance.sqrt();

        result.insert(
            metric_path.to_string(),
            serde_json::json!({
                "min": (min * 100_000.0).round() / 100_000.0,
                "max": (max * 100_000.0).round() / 100_000.0,
                "avg": (avg * 100_000.0).round() / 100_000.0,
                "stddev": (stddev * 100_000.0).round() / 100_000.0,
                "count": count,
            }),
        );
    }

    Json(serde_json::Value::Object(result))
}

/// Start the playback background task that pushes frames through the broadcast channel
async fn start_playback_task(state: AppState) {
    {
        let mut cancel = state.replay_cancel.write().await;
        if let Some(token) = cancel.take() {
            token.cancel();
        }
        let new_token = CancellationToken::new();
        *cancel = Some(new_token);
    }

    let cancel_token = {
        let cancel = state.replay_cancel.read().await;
        cancel.as_ref().unwrap().clone()
    };

    let tx = state.telemetry_tx.clone();
    let replay = state.replay.clone();

    tokio::spawn(async move {
        tracing::info!("Playback task started");

        let mut interval = {
            let rs = replay.read().await;
            let (tick_rate, playback_speed) = match &*rs {
                Some(rs) => (rs.tick_rate(), rs.playback_speed()),
                None => return,
            };
            let period_us = (1_000_000.0 / (tick_rate as f64 * playback_speed)).max(1000.0);
            tokio::time::interval(Duration::from_micros(period_us as u64))
        };
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        // First tick completes immediately
        interval.tick().await;
        let mut last_send = tokio::time::Instant::now();

        loop {
            tokio::select! {
                _ = cancel_token.cancelled() => break,
                _ = interval.tick() => {},
            }

            let (should_advance, tick_rate, playback_speed) = {
                let rs = replay.read().await;
                match &*rs {
                    Some(rs) => (rs.is_playing(), rs.tick_rate(), rs.playback_speed()),
                    None => break,
                }
            };

            if !should_advance {
                // Reset so we don't burst frames on resume
                last_send = tokio::time::Instant::now();
                continue;
            }

            // Recalculate interval if speed changed
            let new_period_us =
                (1_000_000.0 / (tick_rate as f64 * playback_speed)).max(1000.0) as u64;
            let current_period = interval.period();
            if current_period != Duration::from_micros(new_period_us) {
                interval = tokio::time::interval(Duration::from_micros(new_period_us));
                interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
                interval.tick().await;
                last_send = tokio::time::Instant::now();
            }

            // Calculate how many frames are due based on elapsed wall time
            let now = tokio::time::Instant::now();
            let elapsed = (now - last_send).as_secs_f64();
            let frames_due = (elapsed * tick_rate as f64 * playback_speed)
                .round()
                .max(1.0) as usize;
            last_send = now;

            let frame = {
                let mut rs = replay.write().await;
                match rs.as_mut() {
                    Some(rs) => {
                        // Skip frames if behind schedule
                        if frames_due > 1 {
                            let target = rs.current_frame() + frames_due - 1;
                            rs.seek(target);
                        }
                        let idx = rs.current_frame();
                        match rs.get_frame(idx) {
                            Ok(frame) => {
                                rs.advance();
                                Some(frame)
                            }
                            Err(e) => {
                                tracing::error!("Failed to read frame {}: {}", idx, e);
                                rs.advance();
                                None
                            }
                        }
                    }
                    None => break,
                }
            };

            if let Some(frame) = frame {
                let _ = tx.send(frame);
            }
        }

        tracing::info!("Playback task ended");
    });
}

// === Persistence Endpoints ===

async fn persistence_get_config(State(state): State<AppState>) -> Json<serde_json::Value> {
    let config = state.persistence_config.read().await;
    let dir = crate::persistence::telemetry_dir();
    Json(serde_json::json!({
        "enabled": config.enabled,
        "frequency_hz": config.frequency_hz,
        "auto_save": config.auto_save,
        "retention": config.retention,
        "directory": dir.to_string_lossy(),
    }))
}

#[derive(Deserialize)]
struct PersistenceConfigRequest {
    enabled: Option<bool>,
    frequency_hz: Option<u32>,
    auto_save: Option<bool>,
    max_sessions: Option<Option<usize>>,
    max_age_days: Option<Option<u32>>,
}

async fn persistence_set_config(
    State(state): State<AppState>,
    Json(req): Json<PersistenceConfigRequest>,
) -> Json<serde_json::Value> {
    let mut config = state.persistence_config.write().await;
    if let Some(enabled) = req.enabled {
        config.enabled = enabled;
    }
    if let Some(freq) = req.frequency_hz {
        config.frequency_hz = freq.clamp(1, 60);
    }
    if let Some(auto_save) = req.auto_save {
        config.auto_save = auto_save;
    }
    if let Some(max_sessions) = req.max_sessions {
        config.retention.max_sessions = max_sessions;
    }
    if let Some(max_age_days) = req.max_age_days {
        config.retention.max_age_days = max_age_days;
    }

    // Run cleanup after config change
    let retention = config.retention.clone();
    drop(config);
    tokio::task::spawn_blocking(move || {
        crate::persistence::cleanup_old_sessions(&retention);
    });

    let config = state.persistence_config.read().await;
    Json(serde_json::json!({
        "status": "ok",
        "enabled": config.enabled,
        "frequency_hz": config.frequency_hz,
        "auto_save": config.auto_save,
        "retention": config.retention,
    }))
}

async fn persistence_stats() -> Json<serde_json::Value> {
    Json(crate::persistence::storage_stats())
}

// === Conversion Endpoints ===

async fn convert_ibt(mut multipart: Multipart) -> Result<impl IntoResponse, (StatusCode, String)> {
    // Extract uploaded .ibt file
    let field = multipart
        .next_field()
        .await
        .map_err(|e| {
            (
                StatusCode::BAD_REQUEST,
                format!("Failed to read upload: {}", e),
            )
        })?
        .ok_or((StatusCode::BAD_REQUEST, "No file provided".to_string()))?;

    let file_name = field.file_name().unwrap_or("upload.ibt").to_string();

    if !file_name.to_lowercase().ends_with(".ibt") {
        return Err((
            StatusCode::BAD_REQUEST,
            "Only .ibt files are supported".to_string(),
        ));
    }

    let data = field.bytes().await.map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            format!("Failed to read file data: {}", e),
        )
    })?;

    tracing::info!("Converting .ibt file: {} ({} bytes)", file_name, data.len());

    // Write to temp file and parse header (blocking I/O)
    let (ibt, temp_path) = tokio::task::spawn_blocking({
        let file_name = file_name.clone();
        move || {
            use ost_adapters::ibt_parser::IbtFile;

            let temp_dir = std::env::temp_dir().join("ost-convert");
            std::fs::create_dir_all(&temp_dir).map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Failed to create temp dir: {}", e),
                )
            })?;

            let temp_path = temp_dir.join(&file_name);
            std::fs::write(&temp_path, &data).map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Failed to write temp file: {}", e),
                )
            })?;

            let ibt = IbtFile::open(&temp_path).map_err(|e| {
                let _ = std::fs::remove_file(&temp_path);
                (
                    StatusCode::BAD_REQUEST,
                    format!("Failed to parse .ibt file: {}", e),
                )
            })?;

            Ok::<_, (StatusCode, String)>((ibt, temp_path))
        }
    })
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Task failed: {}", e),
        )
    })??;

    // Build output filename from session metadata
    let session = ibt.session_info();
    let track = if session.track_display_name.is_empty() {
        "unknown"
    } else {
        &session.track_display_name
    };
    let car = if session.car_screen_name.is_empty() {
        "unknown"
    } else {
        &session.car_screen_name
    };
    let out_filename = format!(
        "{}_{}.ost.ndjson.zstd",
        track.replace(' ', "_"),
        car.replace(' ', "_")
    );

    // Set up streaming pipeline: duplex pipe bridges blocking writes to async reads
    let (write_half, read_half) = tokio::io::duplex(65536);
    let sync_write = tokio_util::io::SyncIoBridge::new(write_half);

    // Spawn blocking conversion task that streams compressed NDJSON through the pipe
    tokio::task::spawn_blocking(move || {
        use std::io::Write;

        let total = ibt.record_count();
        let batch_size = 1000;

        let result: Result<(), Box<dyn std::error::Error + Send + Sync>> = (|| {
            let mut encoder = zstd::Encoder::new(sync_write, 3)?;
            for start in (0..total).step_by(batch_size) {
                let count = batch_size.min(total - start);
                let samples = ibt.read_samples_range(start, count)?;
                for sample in &samples {
                    let frame = ibt.sample_to_frame(sample);
                    let json = serde_json::to_string(&frame)?;
                    writeln!(encoder, "{}", json)?;
                }
            }
            encoder.finish()?;
            Ok(())
        })();

        if let Err(e) = &result {
            tracing::error!("IBT conversion failed: {}", e);
        }

        // Clean up temp file
        let _ = std::fs::remove_file(&temp_path);
    });

    // Build streaming response
    let stream = tokio_util::io::ReaderStream::new(read_half);
    let body = axum::body::Body::from_stream(stream);

    let mut headers = axum::http::HeaderMap::new();
    headers.insert(header::CONTENT_TYPE, "application/zstd".parse().unwrap());
    headers.insert(
        header::CONTENT_DISPOSITION,
        format!("attachment; filename=\"{}\"", out_filename)
            .parse()
            .unwrap(),
    );

    Ok((headers, body))
}

async fn persistence_download(
    State(state): State<AppState>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let history = state.history.read().await;
    let frame_count = history.frame_count();
    if frame_count == 0 {
        return Err((
            StatusCode::NOT_FOUND,
            "No telemetry data in buffer".to_string(),
        ));
    }

    let track = history.track_name().to_string();
    let car = history.car_name().to_string();
    let frames: Vec<_> = history
        .get_frames_range(0, frame_count)
        .into_iter()
        .map(|(_, f)| f.clone())
        .collect();
    drop(history);

    let compressed =
        tokio::task::spawn_blocking(move || crate::persistence::compress_frames(&frames))
            .await
            .map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Task failed: {}", e),
                )
            })?
            .map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Compression failed: {}", e),
                )
            })?;

    let now = chrono::Local::now();
    let track_clean = if track.is_empty() { "unknown" } else { &track };
    let car_clean = if car.is_empty() { "unknown" } else { &car };
    let filename = format!(
        "{}_{track_clean}_{car_clean}.ost.ndjson.zstd",
        now.format("%Y-%m-%d_%H-%M-%S")
    );

    let mut headers = axum::http::HeaderMap::new();
    headers.insert(header::CONTENT_TYPE, "application/zstd".parse().unwrap());
    headers.insert(
        header::CONTENT_DISPOSITION,
        format!("attachment; filename=\"{}\"", filename)
            .parse()
            .unwrap(),
    );

    Ok((headers, compressed))
}

async fn persistence_list_files() -> Json<Vec<serde_json::Value>> {
    let dir = crate::persistence::telemetry_dir();
    let mut files = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            let name = path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            if !name.ends_with(".ost.ndjson.zstd") {
                continue;
            }
            let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
            let modified = entry
                .metadata()
                .ok()
                .and_then(|m| m.modified().ok())
                .map(|t| {
                    chrono::DateTime::<chrono::Utc>::from(t)
                        .format("%Y-%m-%d %H:%M:%S")
                        .to_string()
                })
                .unwrap_or_default();
            files.push(serde_json::json!({
                "name": name,
                "size": size,
                "modified": modified,
            }));
        }
    }
    // Sort by name descending (newest first since names are date-prefixed)
    files.sort_by(|a, b| {
        b.get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .cmp(a.get("name").and_then(|v| v.as_str()).unwrap_or(""))
    });
    Json(files)
}

#[derive(Deserialize)]
struct LoadFileRequest {
    filename: String,
}

async fn persistence_load_file(
    State(state): State<AppState>,
    Json(req): Json<LoadFileRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    {
        let replay = state.replay.read().await;
        if replay.is_some() {
            return Err((
                StatusCode::CONFLICT,
                "A replay is already active. Delete it first.".to_string(),
            ));
        }
    }

    // Validate filename to prevent path traversal
    if req.filename.contains('/') || req.filename.contains('\\') || req.filename.contains("..") {
        return Err((StatusCode::BAD_REQUEST, "Invalid filename".to_string()));
    }

    let dir = crate::persistence::telemetry_dir();
    let path = dir.join(&req.filename);
    if !path.exists() {
        return Err((StatusCode::NOT_FOUND, "File not found".to_string()));
    }

    let replay_state = tokio::task::spawn_blocking(move || {
        ReplayState::from_ndjson_zstd(&path).map_err(|e| {
            (
                StatusCode::BAD_REQUEST,
                format!("Failed to load file: {}", e),
            )
        })
    })
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Task failed: {}", e),
        )
    })??;

    let info = replay_state.info();

    {
        let mut replay = state.replay.write().await;
        *replay = Some(replay_state);
    }

    start_playback_task(state.clone()).await;

    Ok(Json(serde_json::json!({
        "status": "ok",
        "info": info,
    })))
}

async fn persistence_delete_file(
    axum::extract::Path(name): axum::extract::Path<String>,
) -> Result<StatusCode, (StatusCode, String)> {
    // Validate filename to prevent path traversal
    if name.contains('/') || name.contains('\\') || name.contains("..") {
        return Err((StatusCode::BAD_REQUEST, "Invalid filename".to_string()));
    }
    if !name.ends_with(".ost.ndjson.zstd") {
        return Err((
            StatusCode::BAD_REQUEST,
            "Only .ost.ndjson.zstd files can be deleted".to_string(),
        ));
    }

    let dir = crate::persistence::telemetry_dir();
    let path = dir.join(&name);
    if !path.exists() {
        return Err((StatusCode::NOT_FOUND, "File not found".to_string()));
    }

    std::fs::remove_file(&path).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to delete file: {}", e),
        )
    })?;

    tracing::info!("Deleted telemetry file: {}", name);
    Ok(StatusCode::NO_CONTENT)
}

// === Interactive API Docs ===

async fn api_docs() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "text/html; charset=utf-8")],
        include_str!("api_docs.html"),
    )
}
