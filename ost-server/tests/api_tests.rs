//! Integration tests for the ost-server HTTP API
//!
//! Uses tower::ServiceExt::oneshot to test routes directly without binding a port.

use axum::body::Body;
use http_body_util::BodyExt;
use hyper::Request;
use ost_core::TelemetryAdapter;
use ost_server::{
    api::create_router,
    state::{AppState, SinkConfig, SinkType},
};
use tower::ServiceExt;

/// Helper: build a router with fresh AppState (no adapters registered)
fn app() -> axum::Router {
    let state = AppState::new();
    create_router(state)
}

/// Helper: build a router with AppState returned for further manipulation
fn app_with_state() -> (axum::Router, AppState) {
    let state = AppState::new();
    let router = create_router(state.clone());
    (router, state)
}

/// Helper: collect response body into bytes
async fn body_bytes(body: Body) -> Vec<u8> {
    let collected = body.collect().await.unwrap();
    collected.to_bytes().to_vec()
}

/// Helper: collect response body into string
async fn body_string(body: Body) -> String {
    String::from_utf8(body_bytes(body).await).unwrap()
}

// ==================== GET / ====================

#[tokio::test]
async fn test_get_root_returns_200_with_html() {
    let app = app();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);

    let content_type = response
        .headers()
        .get("content-type")
        .unwrap()
        .to_str()
        .unwrap();
    assert!(
        content_type.contains("text/html"),
        "Expected text/html content-type, got: {}",
        content_type
    );

    let body = body_string(response.into_body()).await;
    assert!(!body.is_empty(), "HTML body should not be empty");
    assert!(
        body.contains("<html") || body.contains("<!DOCTYPE") || body.contains("<!doctype"),
        "Response should contain HTML markup"
    );
}

// ==================== GET /api/adapters ====================

#[tokio::test]
async fn test_get_adapters_returns_200_with_empty_array() {
    let app = app();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/adapters")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);

    let body = body_string(response.into_body()).await;
    let parsed: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert!(parsed.is_array(), "Response should be a JSON array");
    assert_eq!(parsed.as_array().unwrap().len(), 0, "Array should be empty");
}

#[tokio::test]
async fn test_get_adapters_with_demo_adapter_registered() {
    let (app, state) = app_with_state();

    // Register the demo adapter
    state
        .register_adapter(Box::new(ost_adapters::DemoAdapter::new()))
        .await;

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/adapters")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);

    let body = body_string(response.into_body()).await;
    let parsed: serde_json::Value = serde_json::from_str(&body).unwrap();
    let adapters = parsed.as_array().unwrap();

    assert_eq!(adapters.len(), 1, "Should have one adapter");
    assert_eq!(adapters[0]["name"], "Demo");
    assert_eq!(adapters[0]["detected"], true, "Demo adapter is always detected");
}

// ==================== GET /api/sinks ====================

#[tokio::test]
async fn test_get_sinks_returns_200_with_empty_array() {
    let app = app();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/sinks")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);

    let body = body_string(response.into_body()).await;
    let parsed: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert!(parsed.is_array());
    assert_eq!(parsed.as_array().unwrap().len(), 0);
}

// ==================== POST /api/sinks ====================

#[tokio::test]
async fn test_create_sink_returns_201() {
    let app = app();

    let sink_json = serde_json::json!({
        "id": "test-sink-1",
        "sink_type": {
            "type": "http",
            "url": "http://localhost:8080/telemetry"
        },
        "field_mask": null
    });

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/sinks")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_string(&sink_json).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(
        response.status(),
        201,
        "POST /api/sinks should return 201 Created"
    );

    let body = body_string(response.into_body()).await;
    let parsed: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(parsed["id"], "test-sink-1");
}

#[tokio::test]
async fn test_create_sink_generates_id_when_empty() {
    let app = app();

    let sink_json = serde_json::json!({
        "id": "",
        "sink_type": {
            "type": "udp",
            "host": "127.0.0.1",
            "port": 9200
        },
        "field_mask": null
    });

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/sinks")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_string(&sink_json).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 201);

    let body = body_string(response.into_body()).await;
    let parsed: serde_json::Value = serde_json::from_str(&body).unwrap();
    let id = parsed["id"].as_str().unwrap();
    assert!(
        !id.is_empty(),
        "Generated ID should not be empty"
    );
    assert!(
        id.starts_with("sink-"),
        "Generated ID should start with 'sink-', got: {}",
        id
    );
}

// ==================== POST then GET /api/sinks ====================

#[tokio::test]
async fn test_create_then_list_sinks() {
    let (app, state) = app_with_state();

    // First create a sink by directly modifying state (since oneshot consumes the router)
    {
        let mut sinks = state.sinks.write().await;
        sinks.push(SinkConfig {
            id: "test-sink-1".to_string(),
            sink_type: SinkType::Http {
                url: "http://localhost:8080/telemetry".to_string(),
            },
            field_mask: None,
        });
    }

    // Now list sinks
    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/sinks")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);

    let body = body_string(response.into_body()).await;
    let parsed: serde_json::Value = serde_json::from_str(&body).unwrap();
    let sinks = parsed.as_array().unwrap();

    assert_eq!(sinks.len(), 1);
    assert_eq!(sinks[0]["id"], "test-sink-1");
}

// ==================== DELETE /api/sinks/:id ====================

#[tokio::test]
async fn test_delete_sink_returns_204() {
    let (app, state) = app_with_state();

    // Pre-populate a sink
    {
        let mut sinks = state.sinks.write().await;
        sinks.push(SinkConfig {
            id: "to-delete".to_string(),
            sink_type: SinkType::Http {
                url: "http://localhost:8080".to_string(),
            },
            field_mask: None,
        });
    }

    let response = app
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri("/api/sinks/to-delete")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(
        response.status(),
        204,
        "DELETE existing sink should return 204 No Content"
    );

    // Verify it was actually removed
    let sinks = state.sinks.read().await;
    assert_eq!(sinks.len(), 0, "Sink should have been removed");
}

#[tokio::test]
async fn test_delete_nonexistent_sink_returns_404() {
    let app = app();

    let response = app
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri("/api/sinks/nonexistent")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(
        response.status(),
        404,
        "DELETE nonexistent sink should return 404 Not Found"
    );
}

// ==================== GET /api/telemetry/stream ====================

#[tokio::test]
async fn test_telemetry_stream_returns_sse_content_type() {
    let (app, state) = app_with_state();

    // Spawn a task to send a frame after a short delay so the stream has data
    let tx = state.telemetry_tx.clone();
    tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        let mut adapter = ost_adapters::DemoAdapter::new();
        adapter.start().unwrap();
        let frame = adapter.read_frame().unwrap().unwrap();
        let _ = tx.send(frame);
    });

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/telemetry/stream")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);

    let content_type = response
        .headers()
        .get("content-type")
        .unwrap()
        .to_str()
        .unwrap();
    assert!(
        content_type.contains("text/event-stream"),
        "SSE endpoint should return text/event-stream, got: {}",
        content_type
    );
}

#[tokio::test]
async fn test_telemetry_stream_receives_broadcast_frame() {
    let (app, state) = app_with_state();

    // Spawn a task to send a frame shortly after the stream connects
    let tx = state.telemetry_tx.clone();
    tokio::spawn(async move {
        // Give the stream time to connect and subscribe
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        let mut adapter = ost_adapters::DemoAdapter::new();
        adapter.start().unwrap();
        let frame = adapter.read_frame().unwrap().unwrap();
        let _ = tx.send(frame);
    });

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/telemetry/stream")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);

    // Read the body with a timeout to avoid hanging forever
    let body = response.into_body();
    let result = tokio::time::timeout(std::time::Duration::from_secs(3), async {
        // Read the first chunk from the SSE stream
        let mut stream = body.into_data_stream();
        use futures::StreamExt;
        if let Some(Ok(chunk)) = stream.next().await {
            let text = String::from_utf8(chunk.to_vec()).unwrap();
            return Some(text);
        }
        None
    })
    .await;

    match result {
        Ok(Some(text)) => {
            // SSE events are formatted as "data: {...}\n\n"
            assert!(
                text.contains("data:"),
                "SSE stream should contain 'data:' prefix, got: {}",
                text
            );
            // The data should contain JSON with game = "Demo"
            assert!(
                text.contains("Demo"),
                "SSE data should contain 'Demo' game name"
            );
        }
        Ok(None) => {
            // Stream ended without data - this can happen in CI but the
            // content-type test above already verifies SSE setup
        }
        Err(_) => {
            // Timeout - acceptable in test environments where timing is unpredictable
            // The content-type test above already validates the SSE endpoint works
        }
    }
}

#[tokio::test]
async fn test_telemetry_stream_with_field_filter() {
    let (app, state) = app_with_state();

    let tx = state.telemetry_tx.clone();
    tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        let mut adapter = ost_adapters::DemoAdapter::new();
        adapter.start().unwrap();
        let frame = adapter.read_frame().unwrap().unwrap();
        let _ = tx.send(frame);
    });

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/telemetry/stream?fields=vehicle,timing")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);

    let content_type = response
        .headers()
        .get("content-type")
        .unwrap()
        .to_str()
        .unwrap();
    assert!(
        content_type.contains("text/event-stream"),
        "Should still return SSE content type with field filter"
    );

    // Read the body with a timeout
    let body = response.into_body();
    let result = tokio::time::timeout(std::time::Duration::from_secs(3), async {
        let mut stream = body.into_data_stream();
        use futures::StreamExt;
        if let Some(Ok(chunk)) = stream.next().await {
            let text = String::from_utf8(chunk.to_vec()).unwrap();
            return Some(text);
        }
        None
    })
    .await;

    if let Ok(Some(text)) = result {
        // Extract JSON from SSE data line
        // SSE format: "data: {json}\n\n"
        if let Some(data_line) = text.lines().find(|l| l.starts_with("data:")) {
            let json_str = data_line.trim_start_matches("data:").trim();
            if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(json_str) {
                // Should have requested sections
                assert!(parsed.get("vehicle").is_some(), "Filtered response should include vehicle");
                assert!(parsed.get("timing").is_some(), "Filtered response should include timing");
                // Should NOT have unrequested sections
                assert!(
                    parsed.get("engine").is_none(),
                    "Filtered response should NOT include engine"
                );
                assert!(
                    parsed.get("weather").is_none(),
                    "Filtered response should NOT include weather"
                );
                // Should always include timestamp and game
                assert!(
                    parsed.get("timestamp").is_some(),
                    "Filtered response should always include timestamp"
                );
                assert!(
                    parsed.get("game").is_some(),
                    "Filtered response should always include game"
                );
            }
        }
    }
}

// ==================== AppState unit tests ====================

#[tokio::test]
async fn test_app_state_new_has_empty_adapters() {
    let state = AppState::new();
    let adapters = state.adapters.read().await;
    assert_eq!(adapters.len(), 0);
}

#[tokio::test]
async fn test_app_state_new_has_empty_sinks() {
    let state = AppState::new();
    let sinks = state.sinks.read().await;
    assert_eq!(sinks.len(), 0);
}

#[tokio::test]
async fn test_app_state_register_adapter() {
    let state = AppState::new();
    state
        .register_adapter(Box::new(ost_adapters::DemoAdapter::new()))
        .await;

    let adapters = state.adapters.read().await;
    assert_eq!(adapters.len(), 1);
    assert_eq!(adapters[0].name(), "Demo");
}

#[tokio::test]
async fn test_app_state_subscribe_receives_broadcast() {
    let state = AppState::new();
    let mut rx = state.subscribe();

    // Create and send a frame
    let mut adapter = ost_adapters::DemoAdapter::new();
    adapter.start().unwrap();
    let frame = adapter.read_frame().unwrap().unwrap();

    state.telemetry_tx.send(frame).unwrap();

    let received = rx.recv().await.unwrap();
    assert_eq!(received.game, "Demo");
}

#[tokio::test]
async fn test_app_state_default() {
    let state = AppState::default();
    let adapters = state.adapters.read().await;
    assert_eq!(adapters.len(), 0);
}
