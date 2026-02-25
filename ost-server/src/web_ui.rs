//! Embedded web UI

use axum::response::Html;

/// Serve the embedded web UI
pub async fn serve_ui() -> Html<&'static str> {
    Html(include_str!("ui.html"))
}
