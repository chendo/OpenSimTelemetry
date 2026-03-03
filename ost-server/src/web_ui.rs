//! Embedded web UI

use axum::response::Html;

/// Serve the embedded web UI
pub async fn serve_ui() -> Html<&'static str> {
    Html(include_str!("ui.html"))
}

/// Get the raw UI HTML string (for embedding in session pages)
pub fn get_ui_html() -> &'static str {
    include_str!("ui.html")
}
