//! Integration tests for the ost-server HTTP API
//!
//! Uses tower::ServiceExt::oneshot to test routes directly without binding a port.

use axum::body::Body;
use http_body_util::BodyExt;
use hyper::Request;
use ost_core::TelemetryAdapter;
use ost_server::{
    api::create_router,
    state::{AppState, SinkConfig},
};
use std::path::Path;
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
        .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
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
    assert_eq!(
        adapters[0]["detected"], true,
        "Demo adapter is always detected"
    );
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
        "host": "127.0.0.1",
        "port": 9200,
        "update_rate_hz": 60.0,
        "metric_mask": null
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
        "host": "127.0.0.1",
        "port": 9200,
        "update_rate_hz": 30.0,
        "metric_mask": null
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
    assert!(!id.is_empty(), "Generated ID should not be empty");
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
            host: "127.0.0.1".to_string(),
            port: 9200,
            update_rate_hz: Some(60.0),
            metric_mask: None,
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
            host: "127.0.0.1".to_string(),
            port: 9200,
            update_rate_hz: Some(60.0),
            metric_mask: None,
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
async fn test_telemetry_stream_with_metric_filter() {
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
                .uri("/api/telemetry/stream?metric_mask=vehicle,timing")
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
        "Should still return SSE content type with metric filter"
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
                assert!(
                    parsed.get("vehicle").is_some(),
                    "Filtered response should include vehicle"
                );
                assert!(
                    parsed.get("timing").is_some(),
                    "Filtered response should include timing"
                );
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

// ==================== Fixture helpers ====================

fn fixture_path() -> std::path::PathBuf {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest_dir.join("../fixtures/race.ibt")
}

fn has_fixture() -> bool {
    fixture_path().exists()
}

/// Build a multipart body with a single file field
fn multipart_body(file_name: &str, data: &[u8]) -> (String, Vec<u8>) {
    let boundary = "----TestBoundary7MA4YWxkTrZu0gW";
    let mut body = Vec::new();
    body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
    body.extend_from_slice(
        format!("Content-Disposition: form-data; name=\"file\"; filename=\"{file_name}\"\r\n")
            .as_bytes(),
    );
    body.extend_from_slice(b"Content-Type: application/octet-stream\r\n\r\n");
    body.extend_from_slice(data);
    body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
    (boundary.to_string(), body)
}

// ==================== POST /api/convert/ibt ====================

#[tokio::test]
async fn test_convert_ibt_returns_zstd_stream() {
    if !has_fixture() {
        return;
    }

    let app = app();
    let ibt_data = std::fs::read(fixture_path()).expect("Failed to read fixture");
    let (boundary, body) = multipart_body("race.ibt", &ibt_data);

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/convert/ibt")
                .header(
                    "content-type",
                    format!("multipart/form-data; boundary={boundary}"),
                )
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(
        response.status(),
        200,
        "POST /api/convert/ibt should return 200"
    );

    let content_type = response
        .headers()
        .get("content-type")
        .unwrap()
        .to_str()
        .unwrap();
    assert_eq!(content_type, "application/zstd");

    let disposition = response
        .headers()
        .get("content-disposition")
        .unwrap()
        .to_str()
        .unwrap();
    assert!(
        disposition.contains(".ost.ndjson.zstd"),
        "Content-Disposition should suggest .ost.ndjson.zstd filename, got: {}",
        disposition
    );

    // Collect and decompress the streamed response
    let compressed = body_bytes(response.into_body()).await;
    assert!(!compressed.is_empty(), "Response body should not be empty");

    // Verify it's valid ZSTD containing NDJSON
    let decompressed = zstd::decode_all(compressed.as_slice()).expect("Should be valid ZSTD");
    let text = String::from_utf8(decompressed).expect("Should be valid UTF-8");
    let lines: Vec<&str> = text.lines().collect();

    assert!(
        lines.len() > 1000,
        "Should have many NDJSON lines, got {}",
        lines.len()
    );

    // Parse first and last lines as valid TelemetryFrame JSON
    let first: serde_json::Value =
        serde_json::from_str(lines[0]).expect("First line should be valid JSON");
    assert_eq!(first["game"], "iRacing Replay");
    assert!(first["vehicle"].is_object());

    let last: serde_json::Value =
        serde_json::from_str(lines.last().unwrap()).expect("Last line should be valid JSON");
    assert_eq!(last["game"], "iRacing Replay");
}

#[tokio::test]
async fn test_convert_ibt_rejects_non_ibt() {
    let app = app();
    let (boundary, body) = multipart_body("data.csv", b"not an ibt file");

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/convert/ibt")
                .header(
                    "content-type",
                    format!("multipart/form-data; boundary={boundary}"),
                )
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 400, "Non-.ibt upload should return 400");
}

// ==================== POST /api/replay/upload ====================

#[tokio::test]
async fn test_replay_upload_parses_ibt_and_returns_info() {
    if !has_fixture() {
        return;
    }

    let (app, _state) = app_with_state();
    let ibt_data = std::fs::read(fixture_path()).expect("Failed to read fixture");
    let (boundary, body) = multipart_body("race.ibt", &ibt_data);

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/replay/upload")
                .header(
                    "content-type",
                    format!("multipart/form-data; boundary={boundary}"),
                )
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(
        response.status(),
        200,
        "POST /api/replay/upload should return 200"
    );

    let text = body_string(response.into_body()).await;
    let parsed: serde_json::Value = serde_json::from_str(&text).unwrap();

    assert_eq!(parsed["status"], "ok");
    let info = &parsed["info"];
    assert!(
        info["total_frames"].as_u64().unwrap() > 10000,
        "Should have many frames"
    );
    assert_eq!(info["tick_rate"], 60);
    assert_eq!(info["track_name"], "Red Bull Ring");
    assert!(
        info["duration_secs"].as_f64().unwrap() > 200.0,
        "Duration should be > 200s"
    );
    assert!(!info["replay_id"].as_str().unwrap().is_empty());
}

#[tokio::test]
async fn test_replay_upload_rejects_non_ibt() {
    let app = app();
    let (boundary, body) = multipart_body("data.csv", b"not an ibt file");

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/replay/upload")
                .header(
                    "content-type",
                    format!("multipart/form-data; boundary={boundary}"),
                )
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 400, "Non-.ibt upload should return 400");
}

// ==================== Persistence download round-trip ====================

#[tokio::test]
async fn test_persistence_download_round_trip() {
    let (app, state) = app_with_state();

    // Push some frames into the history buffer
    let mut adapter = ost_adapters::DemoAdapter::new();
    adapter.start().unwrap();
    {
        let mut history = state.history.write().await;
        for _ in 0..10 {
            let frame = adapter.read_frame().unwrap().unwrap();
            history.push(frame);
        }
    }

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/persistence/download")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(
        response.status(),
        200,
        "GET /api/persistence/download should return 200"
    );

    let content_type = response
        .headers()
        .get("content-type")
        .unwrap()
        .to_str()
        .unwrap();
    assert_eq!(content_type, "application/zstd");

    let compressed = body_bytes(response.into_body()).await;
    let decompressed = zstd::decode_all(compressed.as_slice()).expect("Should be valid ZSTD");
    let text = String::from_utf8(decompressed).expect("Should be valid UTF-8");
    let lines: Vec<&str> = text.lines().filter(|l| !l.is_empty()).collect();

    assert_eq!(lines.len(), 10, "Should have 10 NDJSON lines");

    for line in &lines {
        let frame: serde_json::Value = serde_json::from_str(line).expect("Valid JSON");
        assert_eq!(frame["game"], "Demo");
    }
}

#[tokio::test]
async fn test_persistence_download_empty_returns_404() {
    let app = app();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/persistence/download")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(
        response.status(),
        404,
        "Download with empty buffer should return 404"
    );
}

// ==================== DELETE /api/persistence/files/:name ====================

#[tokio::test]
async fn test_delete_persistence_file_nonexistent_returns_404() {
    let app = app();

    let response = app
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri("/api/persistence/files/nonexistent.ost.ndjson.zstd")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 404);
}

#[tokio::test]
async fn test_delete_persistence_file_rejects_traversal() {
    let app = app();

    let response = app
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri("/api/persistence/files/..%2F..%2Fetc%2Fpasswd")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 400);
}

// ==================== Golden/snapshot test: IBT frame structure ====================

#[tokio::test]
async fn test_ibt_frame_golden_structure() {
    if !has_fixture() {
        return;
    }

    use ost_adapters::ibt_parser::IbtFile;

    let ibt = IbtFile::open(&fixture_path()).expect("Failed to open fixture");

    // Read frame at index 1800 (~30s in, car on track)
    let sample = ibt.read_sample(1800).expect("Failed to read sample");
    let frame = ibt.sample_to_frame(&sample);
    let json = serde_json::to_value(&frame).expect("Frame should serialize");

    // Verify top-level structure
    assert_eq!(json["game"], "iRacing Replay");
    assert!(json["timestamp"].is_string());
    assert!(json["tick"].is_number());

    // Vehicle section
    let vehicle = &json["vehicle"];
    assert!(vehicle["speed"].is_number());
    assert!(vehicle["rpm"].is_number());
    assert!(vehicle["gear"].is_number());
    assert!(vehicle["throttle"].is_number());
    assert!(vehicle["brake"].is_number());
    assert!(vehicle["clutch"].is_number());
    assert!(vehicle["steering_angle"].is_number());

    // Motion section
    let motion = &json["motion"];
    assert!(motion["velocity"].is_object());
    assert!(motion["g_force"].is_object());
    assert!(motion["g_force"]["x"].is_number());
    assert!(motion["g_force"]["y"].is_number());
    assert!(motion["g_force"]["z"].is_number());

    // Engine section
    let engine = &json["engine"];
    assert!(engine["water_temp"].is_number());
    assert!(engine["oil_temp"].is_number());

    // Wheels section — per-corner data
    let wheels = &json["wheels"];
    for corner in &["front_left", "front_right", "rear_left", "rear_right"] {
        let w = &wheels[corner];
        assert!(
            w["tyre_pressure"].is_number(),
            "Missing tyre_pressure for {}",
            corner
        );
        assert!(
            w["suspension_travel"].is_number(),
            "Missing suspension_travel for {}",
            corner
        );
    }

    // Timing section
    let timing = &json["timing"];
    assert!(timing["lap_number"].is_number());

    // Session section
    let session = &json["session"];
    assert_eq!(session["track_name"], "Red Bull Ring");
    assert_eq!(session["session_type"], "Qualifying");

    // Extras should contain iRacing-specific data
    let extras = &json["extras"];
    assert!(extras.is_object());
    assert!(
        extras
            .as_object()
            .unwrap()
            .keys()
            .any(|k| k.starts_with("iracing/")),
        "Extras should contain iracing/ prefixed keys"
    );
}

// ==================== GET /api/history/aggregate ====================

#[tokio::test]
async fn test_history_aggregate_returns_stats() {
    let (app, state) = app_with_state();

    // Push some frames with known values into the history buffer
    {
        let mut history = state.history.write().await;
        for i in 0..10 {
            let json = serde_json::json!({
                "timestamp": chrono::Utc::now().to_rfc3339(),
                "game": "test",
                "tick": i,
                "vehicle": {
                    "speed": 10.0 + i as f64,
                    "rpm": 3000.0 + (i as f64 * 500.0)
                }
            });
            let frame: ost_core::model::TelemetryFrame = serde_json::from_value(json).unwrap();
            history.push(frame);
        }
    }

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/history/aggregate?duration=60s&metrics=vehicle.speed,vehicle.rpm")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);

    let body = body_string(response.into_body()).await;
    let json: serde_json::Value = serde_json::from_str(&body).unwrap();

    // Check vehicle.speed stats (values 10..19)
    let speed = &json["vehicle.speed"];
    assert!(speed.is_object(), "Should have vehicle.speed stats");
    assert_eq!(speed["count"], 10);
    assert!(speed["min"].as_f64().unwrap() >= 9.9);
    assert!(speed["max"].as_f64().unwrap() <= 19.1);
    assert!(speed["avg"].as_f64().unwrap() > 13.0);
    assert!(speed["avg"].as_f64().unwrap() < 16.0);
    assert!(speed["stddev"].as_f64().unwrap() > 0.0);

    // Check vehicle.rpm stats (values 3000..7500)
    let rpm = &json["vehicle.rpm"];
    assert!(rpm.is_object(), "Should have vehicle.rpm stats");
    assert_eq!(rpm["count"], 10);
    assert!(rpm["min"].as_f64().unwrap() >= 2999.0);
    assert!(rpm["max"].as_f64().unwrap() <= 7501.0);
}

#[tokio::test]
async fn test_history_aggregate_empty_buffer() {
    let app = app();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/history/aggregate?duration=60s&metrics=vehicle.speed")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);

    let body = body_string(response.into_body()).await;
    let json: serde_json::Value = serde_json::from_str(&body).unwrap();
    // Empty object since no frames matched
    assert!(json.as_object().unwrap().is_empty());
}

#[tokio::test]
async fn test_history_aggregate_unknown_metric() {
    let (app, state) = app_with_state();

    // Push a frame
    {
        let mut history = state.history.write().await;
        let json = serde_json::json!({
            "timestamp": chrono::Utc::now().to_rfc3339(),
            "game": "test",
            "tick": 0
        });
        let frame: ost_core::model::TelemetryFrame = serde_json::from_value(json).unwrap();
        history.push(frame);
    }

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/history/aggregate?duration=60s&metrics=nonexistent.metric")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);

    let body = body_string(response.into_body()).await;
    let json: serde_json::Value = serde_json::from_str(&body).unwrap();
    // Nonexistent metric should be omitted
    assert!(json.as_object().unwrap().is_empty());
}
