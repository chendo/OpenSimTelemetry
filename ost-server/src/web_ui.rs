//! Embedded web UI

use axum::extract::State;
use axum::response::Html;

use crate::state::AppState;

static UI_HTML: &str = include_str!("ui.html");

/// Serve the embedded web UI with API key injected.
/// The key is embedded in the HTML so same-origin JS can use it.
/// No CORS headers are set on this route, so remote pages cannot read it.
pub async fn serve_ui(State(state): State<AppState>) -> Html<String> {
    let key = state.api_key.read().unwrap().clone();
    Html(inject_config(UI_HTML, &key))
}

/// Get UI HTML with API key injected (for session pages in serve mode).
pub fn get_ui_html_with_key(api_key: &str) -> String {
    inject_config(UI_HTML, api_key)
}

/// Inject server config into HTML by replacing the placeholder.
fn inject_config(html: &str, api_key: &str) -> String {
    let config_script = format!(r#"<script>window.__OST_API_KEY__="{api_key}";</script>"#);
    html.replace("</head>", &format!("{config_script}</head>"))
}
