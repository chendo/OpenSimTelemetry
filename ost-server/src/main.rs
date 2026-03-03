//! OpenSimTelemetry Server
//!
//! Main server application with web UI and REST API

use anyhow::Result;
use ost_server::{api, manager, persistence, sessions, state};
use std::net::SocketAddr;
use std::sync::Arc;
use tracing::info;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let serve_mode = std::env::args().any(|a| a == "--serve");

    // Create application state
    let mut state = state::AppState::new();

    if serve_mode {
        info!("Starting OpenSimTelemetry Server in SERVE mode");

        let admin_user = std::env::var("OST_ADMIN_USER")
            .ok()
            .filter(|s| !s.is_empty());
        let admin_pass = std::env::var("OST_ADMIN_PASS")
            .ok()
            .filter(|s| !s.is_empty());

        if admin_user.is_none() || admin_pass.is_none() {
            tracing::warn!(
                "Serve mode: OST_ADMIN_USER and/or OST_ADMIN_PASS not set — admin endpoints will be unauthenticated"
            );
        }

        // Session storage directory
        let sessions_dir = std::env::var("OST_SESSIONS_DIR")
            .ok()
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| {
                #[cfg(target_os = "windows")]
                {
                    let base = dirs::document_dir()
                        .unwrap_or_else(|| dirs::home_dir().unwrap_or_else(|| ".".into()));
                    base.join("OpenSimTelemetry").join("sessions")
                }
                #[cfg(not(target_os = "windows"))]
                {
                    let base = dirs::home_dir().unwrap_or_else(|| ".".into());
                    base.join(".opensimtelemetry").join("sessions")
                }
            });

        // Max storage: OST_MAX_STORAGE_GB env var, default 10 GB
        let max_storage_bytes = std::env::var("OST_MAX_STORAGE_GB")
            .ok()
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(10)
            * 1024
            * 1024
            * 1024;

        let session_store = sessions::SessionStore::new(sessions_dir, max_storage_bytes)?;

        state.serve_mode = true;
        state.session_store = Some(Arc::new(session_store));
        state.admin_user = admin_user;
        state.admin_pass = admin_pass;
    } else {
        info!("Starting OpenSimTelemetry Server");
    }

    // Build the router
    let app = api::create_router(state.clone());

    if !serve_mode {
        // Start adapter manager in background (not needed in serve mode)
        tokio::spawn(manager::run(state.clone()));

        // Start persistence background task
        let persistence_rx = state.subscribe();
        tokio::spawn(persistence::run(
            state.persistence_config.clone(),
            persistence_rx,
        ));
    }

    // Start server
    let addr = SocketAddr::from(([0, 0, 0, 0], 9100));
    info!("Server listening on http://{}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
