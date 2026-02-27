//! Adapter lifecycle manager
//!
//! This module handles:
//! - Polling adapters for game detection
//! - Starting/stopping adapters when games are detected/exit
//! - Reading frames from active adapters
//! - Broadcasting frames to subscribers

use crate::api::broadcast_adapter_status;
use crate::state::AppState;
use anyhow::Result;
use ost_adapters::{DemoAdapter, IRacingAdapter};
use std::time::Duration;
use tokio::time::sleep;
use tracing::{error, info, warn};

const DETECTION_INTERVAL: Duration = Duration::from_secs(1);
const FRAME_INTERVAL: Duration = Duration::from_millis(16); // ~60Hz

/// Main manager loop
pub async fn run(state: AppState) {
    // Register adapters
    state
        .register_adapter(Box::new(IRacingAdapter::new()))
        .await;
    state.register_adapter(Box::new(DemoAdapter::new())).await;
    broadcast_adapter_status(&state).await;

    info!("Adapter manager started");

    loop {
        // Check for game detection
        if let Err(e) = detection_cycle(&state).await {
            error!("Error in detection cycle: {}", e);
        }

        // Read frames from active adapter
        if let Err(e) = frame_read_cycle(&state).await {
            error!("Error reading frames: {}", e);
        }

        sleep(FRAME_INTERVAL).await;
    }
}

/// Check all adapters for game detection
async fn detection_cycle(state: &AppState) -> Result<()> {
    static mut LAST_CHECK: Option<std::time::Instant> = None;

    // Rate limit detection checks to once per second
    unsafe {
        if let Some(last) = LAST_CHECK {
            if last.elapsed() < DETECTION_INTERVAL {
                return Ok(());
            }
        }
        LAST_CHECK = Some(std::time::Instant::now());
    }

    let mut changed = false;

    {
        let mut adapters = state.adapters.write().await;
        let mut active_adapter = state.active_adapter.write().await;

        // If we have an active adapter, check if it's still detected
        if let Some(ref active_key) = *active_adapter {
            if let Some(adapter) = adapters.iter_mut().find(|a| a.key() == active_key) {
                if !adapter.detect() {
                    info!(
                        "Game {} no longer detected, stopping adapter",
                        adapter.name()
                    );
                    if let Err(e) = adapter.stop() {
                        error!("Error stopping adapter {}: {}", adapter.name(), e);
                    }
                    *active_adapter = None;
                    changed = true;
                }
                if changed {
                    drop(adapters);
                    drop(active_adapter);
                    broadcast_adapter_status(state).await;
                }
                return Ok(());
            }
        }

        // No active adapter, look for detected games (skip disabled adapters)
        let disabled = state.disabled_adapters.read().await;
        for adapter in adapters.iter_mut() {
            if disabled.contains(adapter.key()) {
                continue;
            }
            if adapter.detect() && !adapter.is_active() {
                info!("Game {} detected, starting adapter", adapter.name());
                match adapter.start() {
                    Ok(_) => {
                        *active_adapter = Some(adapter.key().to_string());
                        info!("Adapter {} started successfully", adapter.name());
                        changed = true;
                        break;
                    }
                    Err(e) => {
                        error!("Failed to start adapter {}: {}", adapter.name(), e);
                    }
                }
            }
        }
    }

    if changed {
        broadcast_adapter_status(state).await;
    }

    Ok(())
}

/// Read frames from the active adapter and broadcast them
async fn frame_read_cycle(state: &AppState) -> Result<()> {
    // Don't send adapter frames while a replay is active
    {
        let replay = state.replay.read().await;
        if replay.is_some() {
            return Ok(());
        }
    }

    let active_key = {
        let active = state.active_adapter.read().await;
        active.clone()
    };

    let Some(active_key) = active_key else {
        return Ok(());
    };

    let mut adapters = state.adapters.write().await;

    if let Some(adapter) = adapters.iter_mut().find(|a| a.key() == active_key) {
        match adapter.read_frame() {
            Ok(Some(frame)) => {
                // Broadcast to all subscribers
                // Ignore error if no receivers (they'll get the next frame)
                let _ = state.telemetry_tx.send(frame);
            }
            Ok(None) => {
                // No data available, that's fine
            }
            Err(e) => {
                warn!("Error reading frame from {}: {}", active_key, e);
            }
        }
    }

    Ok(())
}
