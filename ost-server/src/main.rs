//! OpenSimTelemetry Server
//!
//! Main server application with web UI and REST API

use anyhow::Result;
use ost_server::{api, manager, persistence, state};
use std::net::SocketAddr;
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

    info!("Starting OpenSimTelemetry Server");

    // Create application state
    let state = state::AppState::new();

    // Build the router
    let app = api::create_router(state.clone());

    // Start adapter manager in background
    tokio::spawn(manager::run(state.clone()));

    // Start persistence background task
    let persistence_rx = state.subscribe();
    tokio::spawn(persistence::run(
        state.persistence_config.clone(),
        persistence_rx,
    ));

    // Start server
    let addr = SocketAddr::from(([0, 0, 0, 0], 9100));
    info!("Server listening on http://{}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
