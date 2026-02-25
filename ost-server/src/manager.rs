//! Adapter lifecycle manager
//!
//! This module handles:
//! - Polling adapters for game detection
//! - Starting/stopping adapters when games are detected/exit
//! - Reading frames from active adapters
//! - Broadcasting frames to subscribers

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

    let mut adapters = state.adapters.write().await;
    let mut active_adapter = state.active_adapter.write().await;

    // If we have an active adapter, check if it's still detected
    if let Some(ref active_name) = *active_adapter {
        if let Some(adapter) = adapters.iter_mut().find(|a| a.name() == active_name) {
            if !adapter.detect() {
                info!("Game {} no longer detected, stopping adapter", active_name);
                if let Err(e) = adapter.stop() {
                    error!("Error stopping adapter {}: {}", active_name, e);
                }
                *active_adapter = None;
            }
            return Ok(());
        }
    }

    // No active adapter, look for detected games
    for adapter in adapters.iter_mut() {
        if adapter.detect() && !adapter.is_active() {
            info!("Game {} detected, starting adapter", adapter.name());
            match adapter.start() {
                Ok(_) => {
                    *active_adapter = Some(adapter.name().to_string());
                    info!("Adapter {} started successfully", adapter.name());
                    break;
                }
                Err(e) => {
                    error!("Failed to start adapter {}: {}", adapter.name(), e);
                }
            }
        }
    }

    Ok(())
}

/// Read frames from the active adapter and broadcast them
async fn frame_read_cycle(state: &AppState) -> Result<()> {
    let active_name = {
        let active = state.active_adapter.read().await;
        active.clone()
    };

    if active_name.is_none() {
        return Ok(());
    }

    let active_name = active_name.unwrap();
    let mut adapters = state.adapters.write().await;

    if let Some(adapter) = adapters.iter_mut().find(|a| a.name() == active_name) {
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
                warn!("Error reading frame from {}: {}", active_name, e);
            }
        }
    }

    Ok(())
}
