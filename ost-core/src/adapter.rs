//! Telemetry adapter trait definition

use crate::model::TelemetryFrame;
use anyhow::Result;

/// Trait for game-specific telemetry adapters
///
/// Each adapter is responsible for:
/// - Detecting if the game is currently running
/// - Reading telemetry data from the game
/// - Converting game-specific data to the unified TelemetryFrame format
pub trait TelemetryAdapter: Send + Sync {
    /// Get the name of this adapter (e.g., "Assetto Corsa", "iRacing")
    fn name(&self) -> &str;

    /// Check if the game is currently running and accessible
    ///
    /// This should be a lightweight check (e.g., process name, shared memory existence)
    fn detect(&self) -> bool;

    /// Start reading telemetry data
    ///
    /// Called when the game is detected. Initialize any connections or resources.
    fn start(&mut self) -> Result<()>;

    /// Stop reading telemetry data
    ///
    /// Called when the game exits or adapter is being shut down.
    fn stop(&mut self) -> Result<()>;

    /// Read the next telemetry frame
    ///
    /// Returns:
    /// - `Ok(Some(frame))` if a new frame is available
    /// - `Ok(None)` if no new data (non-blocking)
    /// - `Err(_)` if an error occurred
    ///
    /// This should be non-blocking or have a short timeout.
    fn read_frame(&mut self) -> Result<Option<TelemetryFrame>>;

    /// Get whether the adapter is currently active
    fn is_active(&self) -> bool;
}
