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

/// Four decimal places, kept at double width.
///
/// The f32 version rounds and then narrows, which is fine while the value is
/// small: a lap time of 93.3 has f32 steps of 7 microseconds, far below the
/// 0.1ms grid. A session clock at 86,400 has f32 steps of 7.8ms, so narrowing
/// there would quantise well above the grid the rounding asks for.
fn round4_f64<S: serde::Serializer>(val: &f64, s: S) -> Result<S::Ok, S::Error> {
    s.serialize_f64((*val * 10000.0).round() / 10000.0)
}

/// Meters
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Meters(#[serde(serialize_with = "round4")] pub f32);

/// Millimeters
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Millimeters(#[serde(serialize_with = "round4")] pub f32);

/// Meters per second
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct MetersPerSecond(#[serde(serialize_with = "round4")] pub f32);

/// Millimeters per second
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct MillimetersPerSecond(#[serde(serialize_with = "round4")] pub f32);

/// Meters per second squared (acceleration)
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct MetersPerSecondSquared(#[serde(serialize_with = "round4")] pub f32);

/// Degrees
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Degrees(#[serde(serialize_with = "round4")] pub f32);

impl Degrees {
    pub fn from_radians(rad: f32) -> Self {
        Self(rad * (180.0 / std::f32::consts::PI))
    }
}

/// Degrees per second
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct DegreesPerSecond(#[serde(serialize_with = "round4")] pub f32);

impl DegreesPerSecond {
    pub fn from_radians(rad: f32) -> Self {
        Self(rad * (180.0 / std::f32::consts::PI))
    }
}

/// Degrees per second squared
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct DegreesPerSecondSquared(#[serde(serialize_with = "round4")] pub f32);

impl DegreesPerSecondSquared {
    pub fn from_radians(rad: f32) -> Self {
        Self(rad * (180.0 / std::f32::consts::PI))
    }
}

/// Revolutions per minute
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Rpm(#[serde(serialize_with = "round4")] pub f32);

impl Rpm {
    pub fn from_radians_per_sec(rad_s: f32) -> Self {
        Self(rad_s * 60.0 / (2.0 * std::f32::consts::PI))
    }
}

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

/// Position around a lap, 0.0 to 1.0.
///
/// Deliberately NOT a [`Percentage`], despite having the same range and units,
/// because it is not a reading — it is an axis. Every chart of a lap is plotted
/// against it and every comparison between laps is aligned by it, so its
/// resolution has to be finer than the distance a car covers between samples.
///
/// The 1e-4 grid the other units round to cannot do that. A ten-thousandth of a
/// 5.75km lap is 0.575m, while a car at 60Hz covers `v/60` metres per sample —
/// the two are equal at 124 km/h, so below that speed consecutive samples land
/// on the same value. Plotted, that draws a staircase: a flat tread wherever
/// the value has not yet ticked over and a vertical riser where it does. It is
/// invisible on a straight and unmistakable through a slow corner, which is
/// what made it look like a rendering bug for as long as it did.
///
/// Serialised with no rounding at all. An f32 near 1.0 steps by about 6e-8,
/// which is a third of a millimetre on a 5.75km lap — already far finer than
/// anything needs, so there is nothing to gain by putting a grid under it and
/// a staircase to lose. serde emits the shortest decimal that round-trips the
/// f32, so the payload cost over the old four-decimal form is a few characters
/// on one channel.
///
/// Rounding here was tried and removed: doing it as `(v * 1e7).round() / 1e7`
/// in f32 is itself lossy near 1.0, where the intermediate lands in a range
/// whose representable values are a whole integer apart.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct LapFraction(pub f32);

impl LapFraction {
    /// Create a lap fraction, clamping to [0.0, 1.0].
    pub fn new(value: f32) -> Self {
        Self(value.clamp(0.0, 1.0))
    }

    /// Get as percentage (0-100)
    pub fn as_percent(&self) -> f32 {
        self.0 * 100.0
    }
}

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

/// Seconds carried at double precision, for clocks rather than durations.
///
/// [`Seconds`] is f32, which is the right width for a lap time or a delta:
/// those live in the tens to hundreds, where f32 still resolves microseconds.
/// A session clock is the same unit with a different lifetime — it climbs to
/// 86,400 in a twenty-four hour race, and by then consecutive f32 values sit
/// 7.8ms apart. iRacing stores these as doubles for exactly that reason, so
/// narrowing them here would discard something the file went to the trouble
/// of keeping.
///
/// The magnitude is also why this cannot round-trip through
/// [`round4`]: that serialises as f32, which would undo the width on the way
/// out.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SessionSeconds(#[serde(serialize_with = "round4_f64")] pub f64);

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
