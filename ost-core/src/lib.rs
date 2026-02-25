//! OpenSimTelemetry Core Library
//!
//! This crate provides the core data model and adapter trait for unified
//! telemetry access across multiple racing simulators.

pub mod adapter;
pub mod model;
pub mod units;

pub use adapter::TelemetryAdapter;
pub use model::{FieldMask, TelemetryFrame};
