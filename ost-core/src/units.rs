//! Type-safe wrappers for physical units
//!
//! This module provides newtype wrappers around f32/f64 to ensure
//! type safety and prevent unit confusion.

use serde::{Deserialize, Serialize};

/// Meters
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Meters(pub f32);

/// Meters per second
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct MetersPerSecond(pub f32);

/// Meters per second squared (acceleration)
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct MetersPerSecondSquared(pub f32);

/// Radians
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Radians(pub f32);

/// Radians per second
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct RadiansPerSecond(pub f32);

/// Radians per second squared
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct RadiansPerSecondSquared(pub f32);

/// Revolutions per minute
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Rpm(pub f32);

/// Kilograms
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Kilograms(pub f32);

/// Newtons
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Newtons(pub f32);

/// Celsius
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Celsius(pub f32);

/// Pascals (pressure)
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Pascals(pub f32);

/// Kilopascals (pressure)
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Kilopascals(pub f32);

/// Percentage (0.0 to 1.0)
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Percentage(pub f32);

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
pub struct Seconds(pub f32);

/// G-force (multiples of gravitational acceleration)
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct GForce(pub f32);

impl GForce {
    pub fn from_acceleration(accel: MetersPerSecondSquared) -> Self {
        const G: f32 = 9.81; // m/s^2
        Self(accel.0 / G)
    }
}
