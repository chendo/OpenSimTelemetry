//! REST API and SSE routes

use crate::state::{AppState, SinkConfig};
use crate::web_ui;
use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::{
        sse::{Event, KeepAlive, Sse},
        IntoResponse,
    },
    routing::{delete, get, post},
    Json, Router,
};
use futures::stream::{Stream, StreamExt as FuturesStreamExt};
use ost_core::model::{FieldMask, TelemetryFrame};
use serde::{Deserialize, Serialize};
use std::convert::Infallible;
use tokio_stream::wrappers::BroadcastStream;
use tower_http::cors::CorsLayer;

/// Create the main application router
pub fn create_router(state: AppState) -> Router {
    Router::new()
        .route("/", get(web_ui::serve_ui))
        .route("/api/adapters", get(list_adapters))
        .route("/api/telemetry/stream", get(telemetry_stream))
        .route("/api/sinks", get(list_sinks).post(create_sink))
        .route("/api/sinks/:id", delete(delete_sink))
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
    let field_mask = query.fields.map(|f| FieldMask::from_str(&f));

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
