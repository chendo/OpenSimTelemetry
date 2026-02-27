//! REST API and SSE routes

use crate::replay::ReplayState;
use crate::state::{AppState, SinkConfig};
use crate::web_ui;
use axum::{
    extract::{DefaultBodyLimit, Multipart, Query, State},
    http::StatusCode,
    response::{
        sse::{Event, KeepAlive, Sse},
        IntoResponse,
    },
    routing::{delete, get, post},
    Json, Router,
};
use futures::stream::{Stream, StreamExt as FuturesStreamExt};
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
        .route("/api/telemetry/stream", get(telemetry_stream))
        .route("/api/sinks", get(list_sinks).post(create_sink))
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
    name: String,
    detected: bool,
    active: bool,
}

async fn list_adapters(State(state): State<AppState>) -> Json<Vec<AdapterInfo>> {
    let adapters = state.adapters.read().await;
    let active_name = state.active_adapter.read().await;

    let info: Vec<AdapterInfo> = adapters
        .iter()
        .map(|adapter| AdapterInfo {
            name: adapter.name().to_string(),
            detected: adapter.detect(),
            active: adapter.is_active()
                || active_name
                    .as_ref()
                    .map(|n| n == adapter.name())
                    .unwrap_or(false),
        })
        .collect();

    Json(info)
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
    let mut sinks = state.sinks.write().await;

    // Generate ID if not provided
    let mut config = request.config;
    if config.id.is_empty() {
        config.id = format!("sink-{}", sinks.len() + 1);
    }

    sinks.push(config.clone());

    (StatusCode::CREATED, Json(config))
}

async fn delete_sink(
    State(state): State<AppState>,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> impl IntoResponse {
    let mut sinks = state.sinks.write().await;

    if let Some(pos) = sinks.iter().position(|s| s.id == id) {
        sinks.remove(pos);
        StatusCode::NO_CONTENT
    } else {
        StatusCode::NOT_FOUND
    }
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

    let temp_dir = std::env::temp_dir().join("ost-replay");
    std::fs::create_dir_all(&temp_dir)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to create temp dir: {}", e)))?;

    let temp_path = temp_dir.join(&file_name);
    std::fs::write(&temp_path, &data)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to write temp file: {}", e)))?;

    let replay_state = ReplayState::from_file(&temp_path).map_err(|e| {
        let _ = std::fs::remove_file(&temp_path);
        (StatusCode::BAD_REQUEST, format!("Failed to parse .ibt file: {}", e))
    })?;

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
}

async fn replay_frames(
    State(state): State<AppState>,
    Query(params): Query<ReplayFramesQuery>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let mut replay = state.replay.write().await;
    let rs = replay
        .as_mut()
        .ok_or((StatusCode::NOT_FOUND, "No active replay".to_string()))?;

    let frames = rs
        .get_frames_range(params.start, params.count)
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to read frames: {}", e),
            )
        })?;

    let json_frames: Vec<serde_json::Value> = frames
        .into_iter()
        .map(|(idx, frame)| {
            serde_json::json!({
                "i": idx,
                "f": frame
            })
        })
        .collect();

    Ok(Json(serde_json::json!(json_frames)))
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
