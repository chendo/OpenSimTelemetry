//! Game-specific telemetry adapters for OpenSimTelemetry

pub mod demo;
pub mod iracing;

pub use demo::DemoAdapter;
pub use iracing::IRacingAdapter;
