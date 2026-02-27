//! REST API and SSE routes

use crate::replay::ReplayState;
use crate::state::{AppState, SinkConfig};
use crate::web_ui;
use axum::{
    extract::{DefaultBodyLimit, Multipart, Query, State},
    http::{header, StatusCode},
    response::{
        sse::{Event, KeepAlive, Sse},
        IntoResponse,
    },
    routing::{delete, get, post},
    Json, Router,
};
use futures::stream::{self, Stream, StreamExt as FuturesStreamExt};
use ost_core::model::FieldMask;
use serde::{Deserialize, Serialize};
use std::convert::Infallible;
use std::time::Duration;
use tokio_stream::wrappers::BroadcastStream;
use tokio_util::sync::CancellationToken;
use tower_http::cors::CorsLayer;

/// Create the main application router
pub fn create_router(state: AppState) -> Router {
    Router::new()
        .route("/", get(web_ui::serve_ui))
        .route("/api/adapters", get(list_adapters))
        .route("/api/adapters/:name/toggle", post(toggle_adapter))
        .route("/api/stream", get(unified_stream))
        .route("/api/telemetry/stream", get(telemetry_stream))
        .route("/api/status/stream", get(status_stream))
        .route("/api/sinks", get(list_sinks).post(create_sink))
        .route("/api/sinks/stream", get(sinks_stream))
        .route("/api/sinks/:id", delete(delete_sink))
        // Replay endpoints
        .route("/api/replay/upload", post(replay_upload)
            .layer(DefaultBodyLimit::max(512 * 1024 * 1024)))
        .route("/api/replay/info", get(replay_info))
        .route("/api/replay/frames", get(replay_frames))
        .route("/api/replay/control", post(replay_control))
        .route("/api/replay", delete(replay_delete))
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

        let adapter = adapters
            .iter_mut()
            .find(|a| a.key() == key)
            .ok_or((StatusCode::NOT_FOUND, format!("Adapter '{}' not found", key)))?;

        let is_enabled = !disabled.contains(adapter.key());

        if is_enabled {
            // Disable: stop if active, add to disabled set
            let is_active = adapter.is_active()
                || active_adapter.as_ref().map(|n| n == adapter.key()).unwrap_or(false);
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

    // Telemetry frames
    let telemetry = BroadcastStream::new(telemetry_rx).filter_map(|result| async move {
        match result {
            Ok(frame) => match frame.to_json_filtered(None) {
                Ok(json) => Some(Ok(Event::default().event("frame").data(json))),
                Err(_) => None,
            },
            Err(_) => None,
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

    let initial_event = stream::once(async move {
        Ok(Event::default().data(initial_json))
    });

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
    let initial_event = stream::once(async move {
        Ok(Event::default().data(initial_json))
    });

    Sse::new(initial_event.chain(updates)).keep_alive(KeepAlive::default())
}

// === Telemetry Stream Endpoint ===

#[derive(Deserialize)]
struct StreamQuery {
    fields: Option<String>,
}

async fn telemetry_stream(
    State(state): State<AppState>,
    Query(query): Query<StreamQuery>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let rx = state.subscribe();
    let field_mask = query.fields.map(|f| FieldMask::parse(&f));

    let stream = BroadcastStream::new(rx).filter_map(move |result| {
        let mask = field_mask.clone();
        async move {
            match result {
                Ok(frame) => {
                    // Serialize with field mask
                    match frame.to_json_filtered(mask.as_ref()) {
                        Ok(json) => Some(Ok(Event::default().data(json))),
                        Err(e) => {
                            tracing::error!("Failed to serialize frame: {}", e);
                            None
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!("Broadcast stream error: {}", e);
                    None
                }
            }
        }
    });

    Sse::new(stream).keep_alive(KeepAlive::default())
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
        .map_err(|e| (StatusCode::BAD_REQUEST, format!("Failed to read upload: {}", e)))?
        .ok_or((StatusCode::BAD_REQUEST, "No file provided".to_string()))?;

    let file_name = field
        .file_name()
        .unwrap_or("upload.ibt")
        .to_string();

    if !file_name.to_lowercase().ends_with(".ibt") {
        return Err((
            StatusCode::BAD_REQUEST,
            "Only .ibt files are supported".to_string(),
        ));
    }

    let data = field
        .bytes()
        .await
        .map_err(|e| (StatusCode::BAD_REQUEST, format!("Failed to read file data: {}", e)))?;

    tracing::info!("Received .ibt file: {} ({} bytes)", file_name, data.len());

    // Move blocking file I/O off the async runtime to avoid starving
    // SSE keep-alive events and other async tasks
    let replay_state = tokio::task::spawn_blocking(move || {
        let temp_dir = std::env::temp_dir().join("ost-replay");
        std::fs::create_dir_all(&temp_dir)
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to create temp dir: {}", e)))?;

        let temp_path = temp_dir.join(&file_name);
        std::fs::write(&temp_path, &data)
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to write temp file: {}", e)))?;

        ReplayState::from_file(&temp_path).map_err(|e| {
            let _ = std::fs::remove_file(&temp_path);
            (StatusCode::BAD_REQUEST, format!("Failed to parse .ibt file: {}", e))
        })
    })
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("File processing failed: {}", e)))??;

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
    match &*replay {
        Some(rs) => Ok(Json(serde_json::to_value(rs.info()).unwrap())),
        None => Err((StatusCode::NOT_FOUND, "No active replay".to_string())),
    }
}

#[derive(Deserialize)]
struct ReplayFramesQuery {
    start: usize,
    count: usize,
    fields: Option<String>,
    /// Replay ID for cache-busting; when present, response is immutable-cached
    rid: Option<String>,
}

async fn replay_frames(
    State(state): State<AppState>,
    Query(params): Query<ReplayFramesQuery>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    // Read lock: get_frames_range uses pread, no &mut self needed
    let replay = state.replay.read().await;
    let rs = replay
        .as_ref()
        .ok_or((StatusCode::NOT_FOUND, "No active replay".to_string()))?;

    let frames = rs
        .get_frames_range(params.start, params.count)
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to read frames: {}", e),
            )
        })?;

    let field_mask = params.fields.map(|f| FieldMask::parse(&f));

    let json_frames: Vec<serde_json::Value> = frames
        .into_iter()
        .map(|(idx, frame)| {
            let f_val = if let Some(ref mask) = field_mask {
                let json_str = frame.to_json_filtered(Some(mask)).unwrap_or_default();
                serde_json::from_str(&json_str).unwrap_or(serde_json::Value::Null)
            } else {
                serde_json::to_value(&frame).unwrap_or(serde_json::Value::Null)
            };
            serde_json::json!({
                "i": idx,
                "f": f_val
            })
        })
        .collect();

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
    let rs = replay
        .as_mut()
        .ok_or((StatusCode::NOT_FOUND, "No active replay".to_string()))?;

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
            let frame = request
                .value
                .ok_or((StatusCode::BAD_REQUEST, "Missing 'value' for seek".to_string()))?
                as usize;
            rs.seek(frame);
            Ok(Json(serde_json::json!({"status": "seeked", "frame": rs.current_frame()})))
        }
        "speed" => {
            let speed = request
                .value
                .ok_or((StatusCode::BAD_REQUEST, "Missing 'value' for speed".to_string()))?;
            rs.set_speed(speed);
            Ok(Json(serde_json::json!({"status": "speed_set", "speed": rs.playback_speed()})))
        }
        _ => Err((
            StatusCode::BAD_REQUEST,
            format!("Unknown action: {}", request.action),
        )),
    }
}

async fn replay_delete(
    State(state): State<AppState>,
) -> Result<StatusCode, (StatusCode, String)> {
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

        loop {
            if cancel_token.is_cancelled() {
                break;
            }

            let (should_advance, tick_rate, playback_speed) = {
                let rs = replay.read().await;
                match &*rs {
                    Some(rs) => (rs.is_playing(), rs.tick_rate(), rs.playback_speed()),
                    None => break,
                }
            };

            if !should_advance {
                tokio::select! {
                    _ = cancel_token.cancelled() => break,
                    _ = tokio::time::sleep(Duration::from_millis(50)) => continue,
                }
            }

            let frame = {
                let mut rs = replay.write().await;
                match rs.as_mut() {
                    Some(rs) => {
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

            let interval_ms = (1000.0 / (tick_rate as f64 * playback_speed)).max(1.0);
            tokio::select! {
                _ = cancel_token.cancelled() => break,
                _ = tokio::time::sleep(Duration::from_micros((interval_ms * 1000.0) as u64)) => {},
            }
        }

        tracing::info!("Playback task ended");
    });
}
