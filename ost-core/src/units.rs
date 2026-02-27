//! Type-safe wrappers for physical units
//!
//! This module provides newtype wrappers around f32/f64 to ensure
//! type safety and prevent unit confusion.
//!
//! All unit types serialize with 4 decimal places to reduce JSON payload size.

use serde::{Deserialize, Serialize};

/// Round f32 to 4 decimal places for compact JSON serialization
fn round4<S: serde::Serializer>(val: &f32, s: S) -> Result<S::Ok, S::Error> {
    s.serialize_f32((*val * 10000.0).round() / 10000.0)
}

/// Meters
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Meters(#[serde(serialize_with = "round4")] pub f32);

/// Meters per second
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct MetersPerSecond(#[serde(serialize_with = "round4")] pub f32);

/// Meters per second squared (acceleration)
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct MetersPerSecondSquared(#[serde(serialize_with = "round4")] pub f32);

/// Radians
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Radians(#[serde(serialize_with = "round4")] pub f32);

/// Radians per second
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct RadiansPerSecond(#[serde(serialize_with = "round4")] pub f32);

/// Radians per second squared
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct RadiansPerSecondSquared(#[serde(serialize_with = "round4")] pub f32);

/// Revolutions per minute
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Rpm(#[serde(serialize_with = "round4")] pub f32);

/// Kilograms
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Kilograms(#[serde(serialize_with = "round4")] pub f32);

/// Newtons
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Newtons(#[serde(serialize_with = "round4")] pub f32);

/// Celsius
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Celsius(#[serde(serialize_with = "round4")] pub f32);

/// Pascals (pressure)
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Pascals(#[serde(serialize_with = "round4")] pub f32);

/// Kilopascals (pressure)
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Kilopascals(#[serde(serialize_with = "round4")] pub f32);

/// Percentage (0.0 to 1.0)
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Percentage(#[serde(serialize_with = "round4")] pub f32);

impl Percentage {
    /// Create a new percentage, clamping to [0.0, 1.0]
    pub fn new(value: f32) -> Self {
        Self(value.clamp(0.0, 1.0))
    }

    /// Get as percentage (0-100)
    pub fn as_percent(&self) -> f32 {
        self.0 * 100.0
    }
}

/// Seconds (timestamps, durations)
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Seconds(#[serde(serialize_with = "round4")] pub f32);

/// G-force (multiples of gravitational acceleration)
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct GForce(#[serde(serialize_with = "round4")] pub f32);

impl GForce {
    pub fn from_acceleration(accel: MetersPerSecondSquared) -> Self {
        const G: f32 = 9.81; // m/s^2
        Self(accel.0 / G)
    }
}

/// Liters (volume, primarily for fuel)
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Liters(#[serde(serialize_with = "round4")] pub f32);

/// Liters per hour (fuel consumption rate)
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct LitersPerHour(#[serde(serialize_with = "round4")] pub f32);

/// Volts (electrical)
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Volts(#[serde(serialize_with = "round4")] pub f32);

/// Bar (pressure, typically manifold pressure)
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Bar(#[serde(serialize_with = "round4")] pub f32);

/// Newton-meters (torque)
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct NewtonMeters(#[serde(serialize_with = "round4")] pub f32);

/// Kilograms per cubic meter (density)
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct KilogramsPerCubicMeter(#[serde(serialize_with = "round4")] pub f32);
