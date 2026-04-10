//! Unified telemetry data model
//!
//! Defines the TelemetryFrame structure that all adapters convert to.
//! Uses Option<T> for fields that not all games provide.
//! Organized into domain sub-structs for clarity and scalability.
//!
//! Coordinate system: Right-handed, car-local
//! - X: Right (positive = right side)
//! - Y: Up (positive = up)
//! - Z: Forward (positive = forward)

use crate::units::*;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::str::FromStr;

// =============================================================================
// MetaData — frame metadata
// =============================================================================

/// Frame metadata: timestamp, game identity, and tick counter.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetaData {
    /// Timestamp when this frame was captured
    pub timestamp: DateTime<Utc>,

    /// Game/simulator name
    pub game: String,

    /// Sample tick/frame number from the sim
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tick: Option<u32>,
}

// =============================================================================
// TelemetryFrame — top-level container
// =============================================================================

/// Complete telemetry frame with all available data, organized by domain.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelemetryFrame {
    /// Frame metadata (timestamp, game, tick)
    pub meta: MetaData,

    // === Domain sections ===
    #[serde(skip_serializing_if = "Option::is_none")]
    pub motion: Option<MotionData>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vehicle: Option<VehicleData>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub engine: Option<EngineData>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wheels: Option<WheelData>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timing: Option<TimingData>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session: Option<SessionData>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub weather: Option<WeatherData>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pit: Option<PitData>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub damage: Option<DamageData>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub drivers: Option<DriversData>,

    /// Game-specific telemetry data that doesn't fit the normalized model.
    /// Keyed by lowercase game namespace (e.g., "iracing"), value is a JSON object
    /// of raw variable names. Flattened into the top-level JSON during serialization.
    #[serde(flatten, default, skip_serializing_if = "HashMap::is_empty")]
    pub extras: HashMap<String, serde_json::Value>,
}

// =============================================================================
// 3D Vector
// =============================================================================

/// 3D vector with typed components
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Vector3<T> {
    pub x: T,
    pub y: T,
    pub z: T,
}

impl<T> Vector3<T> {
    pub fn new(x: T, y: T, z: T) -> Self {
        Self { x, y, z }
    }
}

// =============================================================================
// MotionData
// =============================================================================

/// Physics/motion state of the player's car
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MotionData {
    /// Position in world space (meters)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub position: Option<Vector3<Meters>>,

    /// Linear velocity in car-local space (m/s)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub velocity: Option<Vector3<MetersPerSecond>>,

    /// Linear acceleration in car-local space (m/s²)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub acceleration: Option<Vector3<MetersPerSecondSquared>>,

    /// G-forces experienced (derived from acceleration)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub g_force: Option<Vector3<GForce>>,

    /// Pitch angle (degrees)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pitch: Option<Degrees>,

    /// Roll angle (degrees)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub roll: Option<Degrees>,

    /// Yaw angle (degrees, track-relative)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub yaw: Option<Degrees>,

    /// Pitch rate (deg/s) — rotation around lateral axis
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pitch_rate: Option<DegreesPerSecond>,

    /// Yaw rate (deg/s) — rotation around vertical axis
    #[serde(skip_serializing_if = "Option::is_none")]
    pub yaw_rate: Option<DegreesPerSecond>,

    /// Roll rate (deg/s) — rotation around longitudinal axis
    #[serde(skip_serializing_if = "Option::is_none")]
    pub roll_rate: Option<DegreesPerSecond>,

    /// GPS latitude (degrees, WGS84)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latitude: Option<f64>,

    /// GPS longitude (degrees, WGS84)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub longitude: Option<f64>,

    /// Altitude above sea level (meters)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub altitude: Option<Meters>,

    /// Compass heading (degrees, clockwise from true north: 0=N, 90=E, 180=S, 270=W)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub heading: Option<Degrees>,
}

// =============================================================================
// VehicleData
// =============================================================================

/// Driver inputs and basic vehicle state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VehicleData {
    /// Speed magnitude (m/s)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub speed: Option<MetersPerSecond>,

    /// Engine RPM
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rpm: Option<Rpm>,

    /// Redline RPM (from session info)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_rpm: Option<Rpm>,

    /// Idle RPM (from session info)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub idle_rpm: Option<Rpm>,

    /// Current gear (-1 = reverse, 0 = neutral, 1+ = forward gears)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gear: Option<i8>,

    /// Maximum gears available
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_gears: Option<u8>,

    /// Throttle input (0.0 to 1.0)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub throttle: Option<Percentage>,

    /// Brake input (0.0 to 1.0)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub brake: Option<Percentage>,

    /// Clutch input (0.0 = engaged, 1.0 = disengaged)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub clutch: Option<Percentage>,

    /// Steering wheel angle in degrees
    #[serde(skip_serializing_if = "Option::is_none")]
    pub steering_angle: Option<Degrees>,

    /// Steering wheel torque
    #[serde(skip_serializing_if = "Option::is_none")]
    pub steering_torque: Option<NewtonMeters>,

    /// Steering wheel torque as percentage of max
    #[serde(skip_serializing_if = "Option::is_none")]
    pub steering_torque_pct: Option<Percentage>,

    /// Handbrake input (0.0 to 1.0)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub handbrake: Option<Percentage>,

    /// Shift indicator / shift light percentage (0.0 = off, 1.0 = full)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shift_indicator: Option<Percentage>,

    /// Maximum steering lock angle (for scaling wheel visualizations)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub steering_angle_max: Option<Degrees>,

    /// Whether the car is on the track
    #[serde(skip_serializing_if = "Option::is_none")]
    pub on_track: Option<bool>,

    /// Whether the car is in the garage
    #[serde(skip_serializing_if = "Option::is_none")]
    pub in_garage: Option<bool>,

    /// What surface the player's car is currently on
    #[serde(skip_serializing_if = "Option::is_none")]
    pub track_surface: Option<TrackSurface>,

    /// Player's car name
    #[serde(skip_serializing_if = "Option::is_none")]
    pub car_name: Option<String>,

    /// Player's car class
    #[serde(skip_serializing_if = "Option::is_none")]
    pub car_class: Option<String>,

    /// Setup name
    #[serde(skip_serializing_if = "Option::is_none")]
    pub setup_name: Option<String>,

    // === Electronics / driver aids (merged from ElectronicsData) ===
    /// ABS setting level
    #[serde(skip_serializing_if = "Option::is_none")]
    pub abs: Option<f32>,

    /// ABS currently active (firing)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub abs_active: Option<bool>,

    /// Traction control setting
    #[serde(skip_serializing_if = "Option::is_none")]
    pub traction_control: Option<f32>,

    /// Secondary traction control setting
    #[serde(skip_serializing_if = "Option::is_none")]
    pub traction_control_2: Option<f32>,

    /// Brake bias (percentage front)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub brake_bias: Option<Percentage>,

    /// Front anti-roll bar setting
    #[serde(skip_serializing_if = "Option::is_none")]
    pub anti_roll_front: Option<f32>,

    /// Rear anti-roll bar setting
    #[serde(skip_serializing_if = "Option::is_none")]
    pub anti_roll_rear: Option<f32>,

    /// DRS (drag reduction system) status
    #[serde(skip_serializing_if = "Option::is_none")]
    pub drs_status: Option<u32>,

    /// Push-to-pass status
    #[serde(skip_serializing_if = "Option::is_none")]
    pub push_to_pass_status: Option<u32>,

    /// Push-to-pass remaining count
    #[serde(skip_serializing_if = "Option::is_none")]
    pub push_to_pass_count: Option<u32>,

    /// Throttle shape/map setting
    #[serde(skip_serializing_if = "Option::is_none")]
    pub throttle_shape: Option<f32>,

    /// Shift light: first RPM (begin illumination)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shift_light_first_rpm: Option<Rpm>,

    /// Shift light: optimal shift RPM
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shift_light_shift_rpm: Option<Rpm>,

    /// Shift light: last RPM (full illumination)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shift_light_last_rpm: Option<Rpm>,

    /// Shift light: blink RPM (over-rev warning)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shift_light_blink_rpm: Option<Rpm>,
}

// =============================================================================
// TrackSurface enum (normalized)
// =============================================================================

/// Type of surface the car is on (normalized across games)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TrackSurface {
    NotInWorld,
    Undefined,
    Asphalt,
    Concrete,
    RacingDirt,
    Paint,
    Rumble,
    Grass,
    Dirt,
    Sand,
    Gravel,
    Grasscrete,
    Astroturf,
    Unknown,
}

// =============================================================================
// EngineData
// =============================================================================

/// Engine and drivetrain diagnostics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineData {
    /// Coolant/water temperature
    #[serde(skip_serializing_if = "Option::is_none")]
    pub water_temp: Option<Celsius>,

    /// Oil temperature
    #[serde(skip_serializing_if = "Option::is_none")]
    pub oil_temp: Option<Celsius>,

    /// Oil pressure
    #[serde(skip_serializing_if = "Option::is_none")]
    pub oil_pressure: Option<Kilopascals>,

    /// Oil level (0.0 to 1.0)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub oil_level: Option<Percentage>,

    /// Fuel level in liters
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fuel_level: Option<Liters>,

    /// Fuel level as percentage of capacity
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fuel_level_pct: Option<Percentage>,

    /// Fuel tank capacity in liters (from session info)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fuel_capacity: Option<Liters>,

    /// Fuel pressure
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fuel_pressure: Option<Kilopascals>,

    /// Fuel consumption rate
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fuel_use_per_hour: Option<LitersPerHour>,

    /// Battery/alternator voltage
    #[serde(skip_serializing_if = "Option::is_none")]
    pub voltage: Option<Volts>,

    /// Manifold pressure
    #[serde(skip_serializing_if = "Option::is_none")]
    pub manifold_pressure: Option<Bar>,

    /// Coolant/water level
    #[serde(skip_serializing_if = "Option::is_none")]
    pub water_level: Option<Liters>,

    /// Engine warning flags
    #[serde(skip_serializing_if = "Option::is_none")]
    pub warnings: Option<EngineWarnings>,
}

// =============================================================================
// EngineWarnings
// =============================================================================

/// Decoded engine warning/status flags
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct EngineWarnings {
    pub water_temp_high: bool,
    pub fuel_pressure_low: bool,
    pub oil_pressure_low: bool,
    pub engine_stalled: bool,
    pub pit_speed_limiter: bool,
    pub rev_limiter: bool,
}

impl EngineWarnings {
    /// Decode from iRacing bitfield
    pub fn from_iracing_bits(bits: u32) -> Self {
        Self {
            water_temp_high: bits & 0x01 != 0,
            fuel_pressure_low: bits & 0x02 != 0,
            oil_pressure_low: bits & 0x04 != 0,
            engine_stalled: bits & 0x08 != 0,
            pit_speed_limiter: bits & 0x10 != 0,
            rev_limiter: bits & 0x20 != 0,
        }
    }
}

// =============================================================================
// WheelData / WheelInfo
// =============================================================================

/// Per-wheel telemetry data (Front-Left, Front-Right, Rear-Left, Rear-Right)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WheelData {
    pub front_left: WheelInfo,
    pub front_right: WheelInfo,
    pub rear_left: WheelInfo,
    pub rear_right: WheelInfo,
}

impl WheelData {
    pub fn all_wheels(&self) -> [&WheelInfo; 4] {
        [
            &self.front_left,
            &self.front_right,
            &self.rear_left,
            &self.rear_right,
        ]
    }

    pub fn all_wheels_mut(&mut self) -> [&mut WheelInfo; 4] {
        [
            &mut self.front_left,
            &mut self.front_right,
            &mut self.rear_left,
            &mut self.rear_right,
        ]
    }
}

/// Comprehensive information for a single wheel/tyre
///
/// Temperature naming convention: "inner" = toward car center, "outer" = away from car center.
/// Adapters handle the mapping from game-specific naming (e.g. iRacing CL/CR) to this
/// car-relative convention.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WheelInfo {
    // --- Suspension ---
    /// Suspension/shock deflection (mm)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suspension_travel: Option<Millimeters>,

    /// Short-term averaged suspension deflection (mm)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suspension_travel_avg: Option<Millimeters>,

    /// Shock velocity (mm/s)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shock_velocity: Option<MillimetersPerSecond>,

    /// Short-term averaged shock velocity (mm/s)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shock_velocity_avg: Option<MillimetersPerSecond>,

    /// Ride height at this corner (mm)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ride_height: Option<Millimeters>,

    // --- Tyre pressure ---
    /// Current tyre air pressure (kPa)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tyre_pressure: Option<Kilopascals>,

    /// Cold tyre pressure from setup (kPa)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tyre_cold_pressure: Option<Kilopascals>,

    // --- Tyre surface temperatures (inner/middle/outer relative to car center) ---
    /// Surface temp at inner edge (toward car center)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub surface_temp_inner: Option<Celsius>,

    /// Surface temp at middle of tread
    #[serde(skip_serializing_if = "Option::is_none")]
    pub surface_temp_middle: Option<Celsius>,

    /// Surface temp at outer edge (away from car center)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub surface_temp_outer: Option<Celsius>,

    // --- Tyre carcass temperatures ---
    /// Carcass temp at inner position
    #[serde(skip_serializing_if = "Option::is_none")]
    pub carcass_temp_inner: Option<Celsius>,

    /// Carcass temp at middle position
    #[serde(skip_serializing_if = "Option::is_none")]
    pub carcass_temp_middle: Option<Celsius>,

    /// Carcass temp at outer position
    #[serde(skip_serializing_if = "Option::is_none")]
    pub carcass_temp_outer: Option<Celsius>,

    // --- Wear & dynamics ---
    /// Tyre wear (0.0 = new, 1.0 = worn out)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tyre_wear: Option<Percentage>,

    /// Tyre wear at inner edge (toward car center)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tyre_wear_inner: Option<Percentage>,

    /// Tyre wear at middle of tread
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tyre_wear_middle: Option<Percentage>,

    /// Tyre wear at outer edge (away from car center)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tyre_wear_outer: Option<Percentage>,

    /// Wheel rotation speed (RPM)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wheel_speed: Option<Rpm>,

    /// Longitudinal slip ratio
    #[serde(skip_serializing_if = "Option::is_none")]
    pub slip_ratio: Option<f32>,

    /// Lateral slip angle (degrees)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub slip_angle: Option<Degrees>,

    /// Vertical load on tyre (Newtons)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub load: Option<Newtons>,

    // --- Brakes ---
    /// Brake line pressure (kPa)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub brake_line_pressure: Option<Kilopascals>,

    /// Brake disc/rotor temperature
    #[serde(skip_serializing_if = "Option::is_none")]
    pub brake_temp: Option<Celsius>,

    // --- Compound ---
    /// Tyre compound name or index
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tyre_compound: Option<String>,
}

impl WheelInfo {
    pub fn new() -> Self {
        Self {
            suspension_travel: None,
            suspension_travel_avg: None,
            shock_velocity: None,
            shock_velocity_avg: None,
            ride_height: None,
            tyre_pressure: None,
            tyre_cold_pressure: None,
            surface_temp_inner: None,
            surface_temp_middle: None,
            surface_temp_outer: None,
            carcass_temp_inner: None,
            carcass_temp_middle: None,
            carcass_temp_outer: None,
            tyre_wear: None,
            tyre_wear_inner: None,
            tyre_wear_middle: None,
            tyre_wear_outer: None,
            wheel_speed: None,
            slip_ratio: None,
            slip_angle: None,
            load: None,
            brake_line_pressure: None,
            brake_temp: None,
            tyre_compound: None,
        }
    }
}

impl Default for WheelInfo {
    fn default() -> Self {
        Self::new()
    }
}

// =============================================================================
// TimingData
// =============================================================================

/// Lap timing, position, and delta information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimingData {
    /// Current lap time in seconds
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_lap_time: Option<Seconds>,

    /// Last completed lap time
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_lap_time: Option<Seconds>,

    /// Personal best lap time
    #[serde(skip_serializing_if = "Option::is_none")]
    pub best_lap_time: Option<Seconds>,

    /// Best N-lap average time
    #[serde(skip_serializing_if = "Option::is_none")]
    pub best_n_lap_time: Option<Seconds>,

    /// Lap number of best N-lap average
    #[serde(skip_serializing_if = "Option::is_none")]
    pub best_n_lap_num: Option<u32>,

    /// Sector times for current/last lap
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sector_times: Option<Vec<Seconds>>,

    /// Current lap number
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lap_number: Option<u32>,

    /// Laps completed
    #[serde(skip_serializing_if = "Option::is_none")]
    pub laps_completed: Option<u32>,

    /// Distance around track (meters)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lap_distance: Option<Meters>,

    /// Distance around track as percentage (0.0 to 1.0)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lap_distance_pct: Option<Percentage>,

    /// Overall race position
    #[serde(skip_serializing_if = "Option::is_none")]
    pub race_position: Option<u32>,

    /// Position within class
    #[serde(skip_serializing_if = "Option::is_none")]
    pub class_position: Option<u32>,

    /// Total number of cars in session
    #[serde(skip_serializing_if = "Option::is_none")]
    pub num_cars: Option<u32>,

    /// Delta to personal best lap (seconds, negative = ahead)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delta_best: Option<Seconds>,

    /// Whether delta_best is valid/usable
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delta_best_ok: Option<bool>,

    /// Delta to session best lap
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delta_session_best: Option<Seconds>,

    /// Whether delta_session_best is valid
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delta_session_best_ok: Option<bool>,

    /// Delta to optimal lap (theoretical best from best sectors)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delta_optimal: Option<Seconds>,

    /// Whether delta_optimal is valid
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delta_optimal_ok: Option<bool>,

    /// Estimated lap time (from session info)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub estimated_lap_time: Option<Seconds>,

    /// Total race laps completed by leader
    #[serde(skip_serializing_if = "Option::is_none")]
    pub race_laps: Option<u32>,
}

// =============================================================================
// SessionData
// =============================================================================

/// Session state, identity, and metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionData {
    /// Session type (practice, qualifying, race, etc.)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_type: Option<SessionType>,

    /// Current session state (warmup, racing, checkered, etc.)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_state: Option<SessionState>,

    /// Elapsed session time
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_time: Option<Seconds>,

    /// Time remaining in session
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_time_remaining: Option<Seconds>,

    /// In-sim time of day
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_time_of_day: Option<Seconds>,

    /// Total laps for this session (None = unlimited)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_laps: Option<u32>,

    /// Laps remaining in session
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_laps_remaining: Option<u32>,

    /// Comprehensive flag state (multiple flags can be active)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub flags: Option<FlagState>,

    /// Track display name
    #[serde(skip_serializing_if = "Option::is_none")]
    pub track_name: Option<String>,

    /// Track configuration/layout name
    #[serde(skip_serializing_if = "Option::is_none")]
    pub track_config: Option<String>,

    /// Track length
    #[serde(skip_serializing_if = "Option::is_none")]
    pub track_length: Option<Meters>,

    /// Track type (Road, Oval, Dirt, etc.)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub track_type: Option<String>,
}

// =============================================================================
// Session enums
// =============================================================================

/// Session type enumeration
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SessionType {
    Practice,
    Qualifying,
    Race,
    Hotlap,
    TimeTrial,
    Drift,
    Warmup,
    Other,
}

/// Session state (progression through a session)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SessionState {
    Invalid,
    GetInCar,
    Warmup,
    ParadeLaps,
    Racing,
    Checkered,
    Cooldown,
}

impl SessionState {
    /// Convert from iRacing SessionState integer
    pub fn from_iracing(value: i32) -> Self {
        match value {
            1 => Self::GetInCar,
            2 => Self::Warmup,
            3 => Self::ParadeLaps,
            4 => Self::Racing,
            5 => Self::Checkered,
            6 => Self::Cooldown,
            _ => Self::Invalid,
        }
    }
}

// =============================================================================
// FlagState
// =============================================================================

/// Comprehensive flag state — multiple flags can be active simultaneously.
/// Replaces the simple FlagType enum. Games that only report a single flag
/// just set one field to true.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct FlagState {
    pub green: bool,
    pub yellow: bool,
    pub yellow_waving: bool,
    pub caution: bool,
    pub caution_waving: bool,
    pub red: bool,
    pub blue: bool,
    pub white: bool,
    pub checkered: bool,
    pub black: bool,
    pub disqualified: bool,
    pub debris: bool,
    pub crossed: bool,
    pub one_lap_to_green: bool,
    pub green_held: bool,
    pub ten_to_go: bool,
    pub five_to_go: bool,
    pub can_service: bool,
    pub furled: bool,
    pub repair: bool,
    pub start_hidden: bool,
    pub start_ready: bool,
    pub start_set: bool,
    pub start_go: bool,
}

impl FlagState {
    /// Decode from iRacing SessionFlags bitfield
    pub fn from_iracing_bits(bits: u32) -> Self {
        Self {
            checkered: bits & 0x01 != 0,
            white: bits & (1 << 1) != 0,
            green: bits & 0x04 != 0,
            yellow: bits & (1 << 3) != 0,
            red: bits & (1 << 4) != 0,
            blue: bits & (1 << 5) != 0,
            debris: bits & (1 << 6) != 0,
            crossed: bits & (1 << 7) != 0,
            yellow_waving: bits & (1 << 8) != 0,
            one_lap_to_green: bits & (1 << 9) != 0,
            green_held: bits & (1 << 10) != 0,
            ten_to_go: bits & (1 << 11) != 0,
            five_to_go: bits & (1 << 12) != 0,
            caution: bits & (1 << 14) != 0,
            caution_waving: bits & (1 << 15) != 0,
            black: bits & (1 << 16) != 0,
            disqualified: bits & (1 << 17) != 0,
            can_service: bits & (1 << 18) != 0,
            furled: bits & (1 << 19) != 0,
            repair: bits & (1 << 20) != 0,
            start_hidden: bits & (1 << 21) != 0,
            start_ready: bits & (1 << 22) != 0,
            start_set: bits & (1 << 23) != 0,
            start_go: bits & (1 << 24) != 0,
        }
    }

    /// Check if any flag is active
    pub fn any_active(&self) -> bool {
        self.green
            || self.yellow
            || self.yellow_waving
            || self.caution
            || self.caution_waving
            || self.red
            || self.blue
            || self.white
            || self.checkered
            || self.black
            || self.disqualified
            || self.debris
            || self.crossed
    }
}

// =============================================================================
// WeatherData
// =============================================================================

/// Environmental/weather conditions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WeatherData {
    /// Air temperature
    #[serde(skip_serializing_if = "Option::is_none")]
    pub air_temp: Option<Celsius>,

    /// Track surface temperature (crew-reported or estimated)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub track_temp: Option<Celsius>,

    /// Measured track surface temperature (direct sensor reading)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub track_surface_temp: Option<Celsius>,

    /// Atmospheric pressure (kPa)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub air_pressure: Option<Kilopascals>,

    /// Air density
    #[serde(skip_serializing_if = "Option::is_none")]
    pub air_density: Option<KilogramsPerCubicMeter>,

    /// Relative humidity (0.0 to 1.0)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub humidity: Option<Percentage>,

    /// Wind speed
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wind_speed: Option<MetersPerSecond>,

    /// Wind direction (degrees, relative to north)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wind_direction: Option<Degrees>,

    /// Fog level (0.0 to 1.0)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fog_level: Option<Percentage>,

    /// Precipitation amount (0.0 to 1.0)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub precipitation: Option<Percentage>,

    /// Track wetness level
    #[serde(skip_serializing_if = "Option::is_none")]
    pub track_wetness: Option<TrackWetness>,

    /// Sky condition description
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skies: Option<String>,

    /// Whether the race has been declared wet
    #[serde(skip_serializing_if = "Option::is_none")]
    pub declared_wet: Option<bool>,
}

/// Track wetness level
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TrackWetness {
    Dry,
    SlightlyWet,
    Wet,
    VeryWet,
    Flooded,
    Unknown,
}

// =============================================================================
// PitData
// =============================================================================

/// Pit road state and service information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PitData {
    /// Whether the player's car is on pit road
    #[serde(skip_serializing_if = "Option::is_none")]
    pub on_pit_road: Option<bool>,

    /// Whether a pit stop is currently active
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pit_active: Option<bool>,

    /// Pit service status code
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pit_service_status: Option<u32>,

    /// Mandatory repair time remaining (seconds)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repair_time_left: Option<Seconds>,

    /// Optional repair time remaining (seconds)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub optional_repair_time_left: Option<Seconds>,

    /// Number of fast repairs available
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fast_repair_available: Option<u32>,

    /// Number of fast repairs used
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fast_repair_used: Option<u32>,

    /// Pit lane speed limit
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pit_speed_limit: Option<MetersPerSecond>,

    /// Requested pit services for next stop
    #[serde(skip_serializing_if = "Option::is_none")]
    pub requested_services: Option<PitServices>,
}

/// Detailed pit service request state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PitServices {
    /// Fuel to add (liters)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fuel_to_add: Option<Liters>,

    /// Change front-left tyre
    pub change_tyre_fl: bool,

    /// Change front-right tyre
    pub change_tyre_fr: bool,

    /// Change rear-left tyre
    pub change_tyre_rl: bool,

    /// Change rear-right tyre
    pub change_tyre_rr: bool,

    /// Windshield tearoff
    pub windshield_tearoff: bool,

    /// Use fast repair
    pub fast_repair: bool,

    /// Requested cold pressure for front-left
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tyre_pressure_fl: Option<Kilopascals>,

    /// Requested cold pressure for front-right
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tyre_pressure_fr: Option<Kilopascals>,

    /// Requested cold pressure for rear-left
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tyre_pressure_rl: Option<Kilopascals>,

    /// Requested cold pressure for rear-right
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tyre_pressure_rr: Option<Kilopascals>,
}

// =============================================================================
// DamageData
// =============================================================================

/// Vehicle damage information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DamageData {
    /// Front damage (0.0 to 1.0)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub front: Option<Percentage>,

    /// Rear damage
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rear: Option<Percentage>,

    /// Left side damage
    #[serde(skip_serializing_if = "Option::is_none")]
    pub left: Option<Percentage>,

    /// Right side damage
    #[serde(skip_serializing_if = "Option::is_none")]
    pub right: Option<Percentage>,

    /// Engine damage
    #[serde(skip_serializing_if = "Option::is_none")]
    pub engine: Option<Percentage>,

    /// Transmission/gearbox damage
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transmission: Option<Percentage>,
}

// =============================================================================
// CompetitorData
// =============================================================================

/// Data for a single competitor car (from per-car arrays + session info)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompetitorData {
    /// Car index in the session
    pub car_index: u32,

    // --- From session info (relatively static) ---
    #[serde(skip_serializing_if = "Option::is_none")]
    pub driver_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub car_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub car_class: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub team_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub car_number: Option<String>,

    // --- From live telemetry (per-tick CarIdx arrays) ---
    /// Current lap
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lap: Option<u32>,

    /// Laps completed
    #[serde(skip_serializing_if = "Option::is_none")]
    pub laps_completed: Option<u32>,

    /// Track position as percentage (0.0 to 1.0)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lap_distance_pct: Option<Percentage>,

    /// Overall position
    #[serde(skip_serializing_if = "Option::is_none")]
    pub position: Option<u32>,

    /// Position within class
    #[serde(skip_serializing_if = "Option::is_none")]
    pub class_position: Option<u32>,

    /// Whether this car is on pit road
    #[serde(skip_serializing_if = "Option::is_none")]
    pub on_pit_road: Option<bool>,

    /// Surface this car is on
    #[serde(skip_serializing_if = "Option::is_none")]
    pub track_surface: Option<TrackSurface>,

    /// Best lap time
    #[serde(skip_serializing_if = "Option::is_none")]
    pub best_lap_time: Option<Seconds>,

    /// Last lap time
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_lap_time: Option<Seconds>,

    /// Estimated time around track
    #[serde(skip_serializing_if = "Option::is_none")]
    pub estimated_time: Option<Seconds>,

    /// Current gear
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gear: Option<i8>,

    /// Current RPM
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rpm: Option<Rpm>,

    /// Steering angle
    #[serde(skip_serializing_if = "Option::is_none")]
    pub steering: Option<Degrees>,
}

// =============================================================================
// DriversData
// =============================================================================

/// Container for driver-related data (current player + competitors)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DriversData {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current: Option<CurrentDriver>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub competitors: Option<Vec<CompetitorData>>,
}

// =============================================================================
// CurrentDriver (formerly DriverData)
// =============================================================================

/// Player driver metadata (mostly from session info, relatively static)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CurrentDriver {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub car_index: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub car_number: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub team_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub estimated_lap_time: Option<Seconds>,
}

// =============================================================================
// Channel Masking for Selective Output
// =============================================================================

/// Specifies which channels to include in serialized output.
///
/// Supports both section-level filtering (`vehicle`, `timing`) and
/// dotted sub-field filtering (`vehicle.speed`, `timing.best_lap_time`).
#[derive(Debug, Clone, Default)]
pub struct ChannelMask {
    channels: HashSet<String>,
    include_all: bool,
}

impl ChannelMask {
    /// Create a mask that includes all channels
    pub fn all() -> Self {
        Self {
            channels: HashSet::new(),
            include_all: true,
        }
    }

    /// Create a mask from a comma-separated list of channel names
    pub fn parse(channels: &str) -> Self {
        let channels: HashSet<String> = channels
            .split(',')
            .map(|s| s.trim().to_lowercase())
            .filter(|s| !s.is_empty())
            .collect();

        Self {
            channels,
            include_all: false,
        }
    }

    /// Check if a channel should be included.
    ///
    /// Returns true if:
    /// - All channels are included (no mask)
    /// - The exact channel name matches (e.g. "vehicle")
    /// - A parent section matches (e.g. "vehicle" includes "vehicle.speed")
    /// - The specific dotted path matches (e.g. "vehicle.speed")
    pub fn includes(&self, channel: &str) -> bool {
        if self.include_all {
            return true;
        }

        let channel_lower = channel.to_lowercase();

        // Exact match
        if self.channels.contains(&channel_lower) {
            return true;
        }

        // Check if any requested channel is a parent section of this channel
        // e.g. if mask has "vehicle" and channel is "vehicle.speed"
        if let Some(dot_pos) = channel_lower.find('.') {
            let section = &channel_lower[..dot_pos];
            if self.channels.contains(section) {
                return true;
            }
        }

        // Check if any requested channel is a child of this section
        // e.g. if mask has "vehicle.speed" and channel is "vehicle" (the section)
        for f in &self.channels {
            if f.starts_with(&channel_lower) && f.as_bytes().get(channel_lower.len()) == Some(&b'.')
            {
                return true;
            }
        }

        false
    }

    /// Return the set of child keys requested under a section.
    ///
    /// For example, if the mask contains `extras.iracing/Foo` and `extras.iracing/Bar`,
    /// calling `child_keys("extras")` returns `Some({"iracing/Foo", "iracing/Bar"})`.
    /// Returns `None` if the bare section name is in the mask (meaning include all).
    pub fn child_keys(&self, section: &str) -> Option<Vec<&str>> {
        if self.include_all {
            return None;
        }
        let section_lower = section.to_lowercase();
        // If the bare section is requested, include everything
        if self.channels.contains(&section_lower) {
            return None;
        }
        let prefix = format!("{}.", section_lower);
        let keys: Vec<&str> = self
            .channels
            .iter()
            .filter_map(|f| f.strip_prefix(&prefix))
            .collect();
        if keys.is_empty() {
            None
        } else {
            Some(keys)
        }
    }

    /// Check if all channels should be included
    pub fn is_all(&self) -> bool {
        self.include_all
    }
}

impl FromStr for ChannelMask {
    type Err = std::convert::Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self::parse(s))
    }
}

/// Builder for ChannelMask
#[derive(Debug, Default)]
pub struct ChannelMaskBuilder {
    channels: HashSet<String>,
}

impl ChannelMaskBuilder {
    pub fn with_channel(mut self, channel: &str) -> Self {
        self.channels.insert(channel.to_lowercase());
        self
    }

    pub fn motion(self) -> Self {
        self.with_channel("motion")
    }

    pub fn vehicle(self) -> Self {
        self.with_channel("vehicle")
    }

    pub fn engine(self) -> Self {
        self.with_channel("engine")
    }

    pub fn wheels(self) -> Self {
        self.with_channel("wheels")
    }

    pub fn timing(self) -> Self {
        self.with_channel("timing")
    }

    pub fn session(self) -> Self {
        self.with_channel("session")
    }

    pub fn weather(self) -> Self {
        self.with_channel("weather")
    }

    pub fn pit(self) -> Self {
        self.with_channel("pit")
    }

    pub fn damage(self) -> Self {
        self.with_channel("damage")
    }

    pub fn drivers(self) -> Self {
        self.with_channel("drivers")
    }

    pub fn build(self) -> ChannelMask {
        ChannelMask {
            channels: self.channels,
            include_all: false,
        }
    }
}

// =============================================================================
// Filtered serialization
// =============================================================================

impl TelemetryFrame {
    /// Serialize this frame respecting the given channel mask.
    ///
    /// If mask is None or includes all channels, serialize everything.
    /// Otherwise, only include specified sections/channels.
    pub fn to_json_filtered(&self, mask: Option<&ChannelMask>) -> serde_json::Result<String> {
        if mask.is_none() || mask.map(|m| m.is_all()).unwrap_or(true) {
            return serde_json::to_string(self);
        }
        let value = self.to_json_value_filtered(mask)?;
        serde_json::to_string(&value)
    }

    /// Serialize this frame to a JSON Value respecting the given channel mask.
    /// Like `to_json_filtered` but returns a Value for programmatic use (e.g. delta computation).
    pub fn to_json_value_filtered(
        &self,
        mask: Option<&ChannelMask>,
    ) -> serde_json::Result<serde_json::Value> {
        if mask.is_none() || mask.map(|m| m.is_all()).unwrap_or(true) {
            return serde_json::to_value(self);
        }

        let mask = mask.unwrap();
        let mut map = serde_json::Map::new();

        // Always include meta
        map.insert("meta".to_string(), serde_json::to_value(&self.meta)?);

        // Conditionally include domain sections
        if mask.includes("motion") {
            if let Some(ref v) = self.motion {
                map.insert("motion".to_string(), serde_json::to_value(v)?);
            }
        }
        if mask.includes("vehicle") {
            if let Some(ref v) = self.vehicle {
                map.insert("vehicle".to_string(), serde_json::to_value(v)?);
            }
        }
        if mask.includes("engine") {
            if let Some(ref v) = self.engine {
                map.insert("engine".to_string(), serde_json::to_value(v)?);
            }
        }
        if mask.includes("wheels") {
            if let Some(ref v) = self.wheels {
                map.insert("wheels".to_string(), serde_json::to_value(v)?);
            }
        }
        if mask.includes("timing") {
            if let Some(ref v) = self.timing {
                map.insert("timing".to_string(), serde_json::to_value(v)?);
            }
        }
        if mask.includes("session") {
            if let Some(ref v) = self.session {
                map.insert("session".to_string(), serde_json::to_value(v)?);
            }
        }
        if mask.includes("weather") {
            if let Some(ref v) = self.weather {
                map.insert("weather".to_string(), serde_json::to_value(v)?);
            }
        }
        if mask.includes("pit") {
            if let Some(ref v) = self.pit {
                map.insert("pit".to_string(), serde_json::to_value(v)?);
            }
        }
        if mask.includes("damage") {
            if let Some(ref v) = self.damage {
                map.insert("damage".to_string(), serde_json::to_value(v)?);
            }
        }
        if mask.includes("drivers") {
            if let Some(ref v) = self.drivers {
                map.insert("drivers".to_string(), serde_json::to_value(v)?);
            }
        }
        // Game-specific namespaces (flattened into top level)
        for (ns, data) in &self.extras {
            if mask.includes(ns) {
                map.insert(ns.clone(), data.clone());
            }
        }

        Ok(serde_json::Value::Object(map))
    }
}

/// Flatten a nested JSON object into a flat map keyed by dot-separated paths.
///
/// Walks `value` recursively, joining keys with `.`. Anything that isn't a JSON
/// object (numbers, strings, bools, arrays, null) is treated as a leaf.
///
/// e.g. `{"vehicle": {"speed": 45.2}}` → `{"vehicle.speed": 45.2}`.
///
/// If `value` is not an object at the top level, returns an empty map.
pub fn flatten_to_channels(value: serde_json::Value) -> serde_json::Map<String, serde_json::Value> {
    let mut out = serde_json::Map::new();
    if let serde_json::Value::Object(map) = value {
        for (k, v) in map {
            flatten_into(&mut out, k, v);
        }
    }
    out
}

fn flatten_into(
    out: &mut serde_json::Map<String, serde_json::Value>,
    prefix: String,
    value: serde_json::Value,
) {
    match value {
        serde_json::Value::Object(map) => {
            for (k, v) in map {
                let path = format!("{}.{}", prefix, k);
                flatten_into(out, path, v);
            }
        }
        leaf => {
            out.insert(prefix, leaf);
        }
    }
}

/// Compute a per-channel delta between two flat frame maps.
///
/// Returns a JSON object containing only channels whose value differs between
/// `prev` and `curr`, plus a `_delta: true` marker. `meta.timestamp` is always
/// included (it changes every frame), so clients can timestamp delta frames.
/// Channels present in `prev` but missing from `curr` are set to `null`.
pub fn compute_channel_delta(
    prev: &serde_json::Map<String, serde_json::Value>,
    curr: &serde_json::Map<String, serde_json::Value>,
) -> serde_json::Value {
    let mut delta = serde_json::Map::new();
    delta.insert("_delta".to_string(), serde_json::Value::Bool(true));

    // Always include meta.timestamp so delta frames carry a time
    if let Some(ts) = curr.get("meta.timestamp") {
        delta.insert("meta.timestamp".to_string(), ts.clone());
    }

    // Include changed or new channels
    for (key, curr_val) in curr {
        if key == "meta.timestamp" {
            continue;
        }
        match prev.get(key) {
            Some(prev_val) if prev_val == curr_val => {
                // Channel unchanged — omit
            }
            _ => {
                delta.insert(key.clone(), curr_val.clone());
            }
        }
    }

    // Channels removed (present in prev but not in curr)
    for key in prev.keys() {
        if key == "_delta" {
            continue;
        }
        if !curr.contains_key(key) {
            delta.insert(key.clone(), serde_json::Value::Null);
        }
    }

    serde_json::Value::Object(delta)
}

// =============================================================================
// Columnar data utilities — path enumeration, value extraction, channel selector
// =============================================================================

/// Recursively enumerate all leaf (non-object) paths in a JSON Value.
/// Returns dotted paths like "vehicle.speed", "wheels.fl.tyre.pressure".
pub fn enumerate_leaf_paths(value: &serde_json::Value, prefix: &str) -> Vec<String> {
    let mut paths = Vec::new();
    if let Some(obj) = value.as_object() {
        for (key, val) in obj {
            let path = if prefix.is_empty() {
                key.clone()
            } else {
                format!("{}.{}", prefix, key)
            };
            if val.is_object() {
                paths.extend(enumerate_leaf_paths(val, &path));
            } else {
                paths.push(path);
            }
        }
    }
    paths.sort();
    paths
}

/// Extract a value at a dot-separated path from a JSON Value.
/// e.g. "vehicle.speed" walks into {"vehicle": {"speed": 45.2}} → Some(45.2)
pub fn extract_value_at_path(value: &serde_json::Value, path: &str) -> Option<serde_json::Value> {
    let mut current = value;
    for part in path.split('.') {
        current = current.get(part)?;
    }
    Some(current.clone())
}

/// A parsed channel selector supporting literal, glob, and regex patterns.
#[derive(Debug)]
pub struct ChannelSelector {
    patterns: Vec<ChannelPattern>,
}

#[derive(Debug)]
enum ChannelPattern {
    Literal(String),
    Glob(regex::Regex),
    Regex(regex::Regex),
}

impl ChannelSelector {
    /// Parse a comma-separated channel selector string.
    ///
    /// Pattern types:
    /// - Literal: `vehicle.speed` (no wildcards)
    /// - Glob: `vehicle.*`, `wheels.**` (`*` = one segment, `**` = any depth)
    /// - Regex: `/pattern/` (slash-delimited)
    pub fn parse(input: &str) -> Result<Self, String> {
        let mut patterns = Vec::new();
        for part in input.split(',') {
            let part = part.trim();
            if part.is_empty() {
                continue;
            }
            if part.starts_with('/') && part.ends_with('/') && part.len() > 2 {
                // Regex pattern
                let re_str = &part[1..part.len() - 1];
                let re = regex::Regex::new(re_str)
                    .map_err(|e| format!("invalid regex '{}': {}", re_str, e))?;
                patterns.push(ChannelPattern::Regex(re));
            } else if part.contains('*') || part.contains('?') {
                // Glob pattern — convert to regex
                let mut re_str = String::from("^");
                let mut chars = part.chars().peekable();
                while let Some(ch) = chars.next() {
                    match ch {
                        '*' => {
                            if chars.peek() == Some(&'*') {
                                chars.next(); // consume second *
                                              // Skip trailing dot if present (e.g., "wheels.**" or "wheels.**.")
                                if chars.peek() == Some(&'.') {
                                    chars.next();
                                }
                                re_str.push_str(".+");
                            } else {
                                re_str.push_str("[^.]+");
                            }
                        }
                        '?' => re_str.push_str("[^.]"),
                        '.' => re_str.push_str("\\."),
                        c => {
                            if regex::escape(&c.to_string()) != c.to_string() {
                                re_str.push_str(&regex::escape(&c.to_string()));
                            } else {
                                re_str.push(c);
                            }
                        }
                    }
                }
                re_str.push('$');
                let re = regex::Regex::new(&re_str)
                    .map_err(|e| format!("invalid glob '{}': {}", part, e))?;
                patterns.push(ChannelPattern::Glob(re));
            } else {
                patterns.push(ChannelPattern::Literal(part.to_string()));
            }
        }
        Ok(ChannelSelector { patterns })
    }

    /// Resolve patterns against available paths, returning sorted deduplicated matches.
    pub fn resolve(&self, available_paths: &[String]) -> Vec<String> {
        let mut matched = Vec::new();
        let mut seen = HashSet::new();
        for pattern in &self.patterns {
            match pattern {
                ChannelPattern::Literal(lit) => {
                    // Exact match or prefix match (e.g., "vehicle" matches "vehicle.speed")
                    for path in available_paths {
                        if (path == lit
                            || path.starts_with(lit)
                                && path.as_bytes().get(lit.len()) == Some(&b'.'))
                            && seen.insert(path.clone())
                        {
                            matched.push(path.clone());
                        }
                    }
                }
                ChannelPattern::Glob(re) | ChannelPattern::Regex(re) => {
                    for path in available_paths {
                        if re.is_match(path) && seen.insert(path.clone()) {
                            matched.push(path.clone());
                        }
                    }
                }
            }
        }
        matched.sort();
        matched
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper to construct a minimal TelemetryFrame for testing
    fn make_test_frame() -> TelemetryFrame {
        TelemetryFrame {
            meta: MetaData {
                timestamp: Utc::now(),
                game: "TestGame".to_string(),
                tick: Some(42),
            },
            motion: Some(MotionData {
                position: None,
                velocity: None,
                acceleration: None,
                g_force: Some(Vector3::new(GForce(0.3), GForce(1.0), GForce(-0.5))),
                pitch: None,
                roll: None,
                yaw: None,
                pitch_rate: None,
                yaw_rate: None,
                roll_rate: None,
                latitude: None,
                longitude: None,
                altitude: None,
                heading: None,
            }),
            vehicle: Some(VehicleData {
                speed: Some(MetersPerSecond(30.0)),
                rpm: Some(Rpm(5000.0)),
                max_rpm: None,
                idle_rpm: None,
                gear: Some(3),
                max_gears: Some(6),
                throttle: Some(Percentage::new(0.75)),
                brake: Some(Percentage::new(0.0)),
                clutch: Some(Percentage::new(0.0)),
                handbrake: None,
                shift_indicator: None,
                steering_angle_max: None,
                steering_angle: Some(Degrees(0.1)),
                steering_torque: None,
                steering_torque_pct: None,
                on_track: None,
                in_garage: None,
                track_surface: None,
                car_name: Some("Test Car".to_string()),
                car_class: None,
                setup_name: None,
                abs: None,
                abs_active: None,
                traction_control: None,
                traction_control_2: None,
                brake_bias: None,
                anti_roll_front: None,
                anti_roll_rear: None,
                drs_status: None,
                push_to_pass_status: None,
                push_to_pass_count: None,
                throttle_shape: None,
                shift_light_first_rpm: None,
                shift_light_shift_rpm: None,
                shift_light_last_rpm: None,
                shift_light_blink_rpm: None,
            }),
            engine: Some(EngineData {
                water_temp: Some(Celsius(90.0)),
                oil_temp: None,
                oil_pressure: None,
                oil_level: None,
                fuel_level: None,
                fuel_level_pct: None,
                fuel_capacity: None,
                fuel_pressure: None,
                fuel_use_per_hour: None,
                manifold_pressure: None,
                water_level: None,
                voltage: None,
                warnings: None,
            }),
            wheels: None,
            timing: Some(TimingData {
                current_lap_time: Some(Seconds(45.2)),
                last_lap_time: Some(Seconds(87.3)),
                best_lap_time: Some(Seconds(85.1)),
                best_n_lap_time: None,
                best_n_lap_num: None,
                sector_times: None,
                lap_number: Some(5),
                laps_completed: None,
                lap_distance: None,
                lap_distance_pct: None,
                race_position: None,
                class_position: None,
                num_cars: None,
                delta_best: None,
                delta_best_ok: None,
                delta_session_best: None,
                delta_session_best_ok: None,
                delta_optimal: None,
                delta_optimal_ok: None,
                estimated_lap_time: None,
                race_laps: None,
            }),
            session: Some(SessionData {
                session_type: Some(SessionType::Race),
                session_state: None,
                session_time: None,
                session_time_remaining: Some(Seconds(1200.0)),
                session_time_of_day: None,
                session_laps: None,
                session_laps_remaining: None,
                flags: None,
                track_name: Some("Test Track".to_string()),
                track_config: None,
                track_length: None,
                track_type: None,
            }),
            weather: None,
            pit: None,
            damage: None,
            drivers: None,
            extras: HashMap::new(),
        }
    }

    #[test]
    fn test_channel_mask_parse_comma_separated() {
        let mask = ChannelMask::parse("vehicle,timing,motion");
        assert!(mask.includes("vehicle"));
        assert!(mask.includes("timing"));
        assert!(mask.includes("motion"));
        assert!(!mask.includes("weather"));
        assert!(!mask.is_all());
    }

    #[test]
    fn test_channel_mask_parse_with_whitespace() {
        let mask = ChannelMask::parse(" vehicle , timing , motion ");
        assert!(mask.includes("vehicle"));
        assert!(mask.includes("timing"));
        assert!(mask.includes("motion"));
    }

    #[test]
    fn test_channel_mask_parse_case_insensitive() {
        let mask = ChannelMask::parse("Vehicle,TIMING,Motion");
        assert!(mask.includes("vehicle"));
        assert!(mask.includes("timing"));
        assert!(mask.includes("motion"));
    }

    #[test]
    fn test_channel_mask_parse_empty_string() {
        let mask = ChannelMask::parse("");
        assert!(!mask.is_all());
        assert!(!mask.includes("vehicle"));
    }

    #[test]
    fn test_channel_mask_all() {
        let mask = ChannelMask::all();
        assert!(mask.is_all());
        assert!(mask.includes("vehicle"));
        assert!(mask.includes("anything"));
    }

    #[test]
    fn test_channel_mask_from_str() {
        let mask: ChannelMask = "vehicle,timing".parse().unwrap();
        assert!(mask.includes("vehicle"));
        assert!(mask.includes("timing"));
        assert!(!mask.includes("engine"));
    }

    #[test]
    fn test_channel_mask_builder() {
        let mask = ChannelMaskBuilder::default()
            .vehicle()
            .timing()
            .engine()
            .build();
        assert!(mask.includes("vehicle"));
        assert!(mask.includes("timing"));
        assert!(mask.includes("engine"));
        assert!(!mask.includes("weather"));
    }

    #[test]
    fn test_channel_mask_section_includes_subfields() {
        let mask = ChannelMask::parse("vehicle");
        assert!(mask.includes("vehicle"));
        assert!(mask.includes("vehicle.speed"));
        assert!(!mask.includes("timing"));
    }

    #[test]
    fn test_to_json_filtered_with_none_returns_full_frame() {
        let frame = make_test_frame();
        let json = frame.to_json_filtered(None).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

        assert!(parsed.get("meta").is_some());
        assert!(parsed.get("vehicle").is_some());
        assert!(parsed.get("timing").is_some());
        assert!(parsed.get("session").is_some());
    }

    #[test]
    fn test_to_json_filtered_with_all_mask_returns_full_frame() {
        let frame = make_test_frame();
        let mask = ChannelMask::all();
        let json = frame.to_json_filtered(Some(&mask)).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

        assert!(parsed.get("vehicle").is_some());
        assert!(parsed.get("timing").is_some());
        assert!(parsed.get("session").is_some());
    }

    #[test]
    fn test_to_json_filtered_with_mask_returns_only_requested_sections() {
        let frame = make_test_frame();
        let mask = ChannelMask::parse("vehicle,timing");
        let json = frame.to_json_filtered(Some(&mask)).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

        // Always-included fields
        assert!(parsed.get("meta").is_some());

        // Requested sections
        assert!(parsed.get("vehicle").is_some());
        assert!(parsed.get("timing").is_some());

        // Sections NOT requested should be absent
        assert!(parsed.get("session").is_none());
        assert!(parsed.get("weather").is_none());
        assert!(parsed.get("engine").is_none());
    }

    #[test]
    fn test_to_json_filtered_with_mask_for_none_section() {
        let frame = make_test_frame();
        // weather is None in our test frame
        let mask = ChannelMask::parse("weather,vehicle");
        let json = frame.to_json_filtered(Some(&mask)).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

        assert!(parsed.get("weather").is_none());
        assert!(parsed.get("vehicle").is_some());
    }

    #[test]
    fn test_telemetry_frame_serialization_roundtrip() {
        let frame = make_test_frame();
        let json = serde_json::to_string(&frame).unwrap();
        let deserialized: TelemetryFrame = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.meta.game, "TestGame");
        let vehicle = deserialized.vehicle.unwrap();
        assert_eq!(vehicle.gear, Some(3));
        assert_eq!(vehicle.max_gears, Some(6));
        let session = deserialized.session.unwrap();
        assert_eq!(session.session_type, Some(SessionType::Race));
        assert_eq!(session.track_name, Some("Test Track".to_string()));
    }

    #[test]
    fn test_vector3_new() {
        let v = Vector3::new(Meters(1.0), Meters(2.0), Meters(3.0));
        assert_eq!(v.x, Meters(1.0));
        assert_eq!(v.y, Meters(2.0));
        assert_eq!(v.z, Meters(3.0));
    }

    #[test]
    fn test_session_type_serialization() {
        let st = SessionType::Race;
        let json = serde_json::to_string(&st).unwrap();
        assert_eq!(json, "\"Race\"");

        let deserialized: SessionType = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, SessionType::Race);
    }

    #[test]
    fn test_percentage_clamp() {
        let p = Percentage::new(1.5);
        assert_eq!(p.0, 1.0);

        let p = Percentage::new(-0.5);
        assert_eq!(p.0, 0.0);

        let p = Percentage::new(0.5);
        assert_eq!(p.0, 0.5);
    }

    #[test]
    fn test_percentage_as_percent() {
        let p = Percentage::new(0.75);
        assert!((p.as_percent() - 75.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_game_namespace_channel_mask() {
        // Game-specific namespaces are flattened to top level.
        // channel_mask=iracing includes the whole iracing namespace.
        let mut frame = make_test_frame();
        let mut iracing_data = serde_json::Map::new();
        iracing_data.insert("brakeABSactive".to_string(), serde_json::Value::Bool(true));
        iracing_data.insert("dcBrakeBias".to_string(), serde_json::json!(56.5));
        frame.extras.insert(
            "iracing".to_string(),
            serde_json::Value::Object(iracing_data),
        );

        let mask = ChannelMask::parse("iracing");
        let json = frame.to_json_filtered(Some(&mask)).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

        let iracing = parsed
            .get("iracing")
            .expect("iracing namespace should be present");
        assert_eq!(
            iracing.get("brakeABSactive"),
            Some(&serde_json::Value::Bool(true))
        );
        assert_eq!(iracing.get("dcBrakeBias"), Some(&serde_json::json!(56.5)));
    }

    #[test]
    fn test_to_json_value_filtered_matches_string() {
        let frame = make_test_frame();
        let mask = ChannelMask::parse("vehicle,timing");
        let value = frame.to_json_value_filtered(Some(&mask)).unwrap();
        let from_value: String = serde_json::to_string(&value).unwrap();
        let from_string = frame.to_json_filtered(Some(&mask)).unwrap();
        // Parse both to Value for comparison (key order may differ)
        let v1: serde_json::Value = serde_json::from_str(&from_value).unwrap();
        let v2: serde_json::Value = serde_json::from_str(&from_string).unwrap();
        assert_eq!(v1, v2);
    }

    #[test]
    fn test_flatten_to_channels_basic() {
        let v = serde_json::json!({
            "vehicle": { "speed": 45.2, "rpm": 5000 },
            "motion": { "g_force": { "x": 0.4, "y": -0.1, "z": 1.2 } }
        });
        let flat = flatten_to_channels(v);
        assert_eq!(flat.get("vehicle.speed"), Some(&serde_json::json!(45.2)));
        assert_eq!(flat.get("vehicle.rpm"), Some(&serde_json::json!(5000)));
        assert_eq!(flat.get("motion.g_force.x"), Some(&serde_json::json!(0.4)));
        assert_eq!(flat.get("motion.g_force.y"), Some(&serde_json::json!(-0.1)));
        assert_eq!(flat.get("motion.g_force.z"), Some(&serde_json::json!(1.2)));
        // No nested objects in the output
        for (_, val) in &flat {
            assert!(!val.is_object(), "flat map should contain no objects");
        }
    }

    #[test]
    fn test_flatten_to_channels_arrays_are_leaves() {
        let v = serde_json::json!({ "iracing": { "tags": ["a", "b"] } });
        let flat = flatten_to_channels(v);
        assert_eq!(
            flat.get("iracing.tags"),
            Some(&serde_json::json!(["a", "b"]))
        );
    }

    #[test]
    fn test_flatten_to_channels_empty_and_non_object() {
        assert!(flatten_to_channels(serde_json::json!({})).is_empty());
        assert!(flatten_to_channels(serde_json::json!(42)).is_empty());
        assert!(flatten_to_channels(serde_json::json!("hi")).is_empty());
    }

    #[test]
    fn test_flatten_full_test_frame_has_no_nested_objects() {
        let frame = make_test_frame();
        let v = serde_json::to_value(&frame).unwrap();
        let flat = flatten_to_channels(v);
        // Spot-check a few expected dot-paths
        assert!(flat.contains_key("meta.timestamp"));
        assert!(flat.contains_key("meta.game"));
        assert!(flat.contains_key("vehicle.speed"));
        assert!(flat.contains_key("motion.g_force.x"));
        // No values should be objects
        for (k, val) in &flat {
            assert!(!val.is_object(), "{} should not be an object", k);
        }
    }

    #[test]
    fn test_compute_channel_delta_unchanged() {
        let frame = make_test_frame();
        let v = serde_json::to_value(&frame).unwrap();
        let prev = flatten_to_channels(v.clone());
        let curr = flatten_to_channels(v);
        let delta = compute_channel_delta(&prev, &curr);
        let map = delta.as_object().unwrap();
        assert_eq!(map.get("_delta"), Some(&serde_json::Value::Bool(true)));
        // meta.timestamp is always included
        assert!(map.get("meta.timestamp").is_some());
        // Nothing else should change
        assert!(map.get("vehicle.speed").is_none());
        assert!(map.get("motion.g_force.x").is_none());
    }

    #[test]
    fn test_compute_channel_delta_changed() {
        let frame = make_test_frame();
        let prev = flatten_to_channels(serde_json::to_value(&frame).unwrap());
        let mut frame2 = make_test_frame();
        frame2.vehicle.as_mut().unwrap().speed = Some(MetersPerSecond(99.0));
        let curr = flatten_to_channels(serde_json::to_value(&frame2).unwrap());
        let delta = compute_channel_delta(&prev, &curr);
        let map = delta.as_object().unwrap();
        assert_eq!(map.get("_delta"), Some(&serde_json::Value::Bool(true)));
        assert!(map.get("meta.timestamp").is_some());
        assert_eq!(map.get("vehicle.speed"), Some(&serde_json::json!(99.0)));
        // unchanged channel should not appear
        assert!(map.get("vehicle.rpm").is_none());
    }

    #[test]
    fn test_compute_channel_delta_removed_channel() {
        let frame = make_test_frame();
        let prev = flatten_to_channels(serde_json::to_value(&frame).unwrap());
        let mut frame2 = make_test_frame();
        frame2.vehicle.as_mut().unwrap().speed = None;
        let curr = flatten_to_channels(serde_json::to_value(&frame2).unwrap());
        let delta = compute_channel_delta(&prev, &curr);
        let map = delta.as_object().unwrap();
        assert_eq!(map.get("vehicle.speed"), Some(&serde_json::Value::Null));
    }

    #[test]
    fn test_compute_channel_delta_added_channel() {
        let frame = make_test_frame();
        let mut prev = flatten_to_channels(serde_json::to_value(&frame).unwrap());
        prev.remove("vehicle.speed");
        let curr = flatten_to_channels(serde_json::to_value(&frame).unwrap());
        let delta = compute_channel_delta(&prev, &curr);
        let map = delta.as_object().unwrap();
        assert!(map.get("vehicle.speed").is_some());
        assert!(!map["vehicle.speed"].is_null());
    }

    // =========================================================================
    // Columnar utility tests
    // =========================================================================

    #[test]
    fn test_enumerate_leaf_paths() {
        let val = serde_json::json!({
            "vehicle": { "speed": 45.0, "rpm": 6500 },
            "meta": { "tick": 1, "game": "test" }
        });
        let paths = enumerate_leaf_paths(&val, "");
        assert!(paths.contains(&"vehicle.speed".to_string()));
        assert!(paths.contains(&"vehicle.rpm".to_string()));
        assert!(paths.contains(&"meta.tick".to_string()));
        assert!(paths.contains(&"meta.game".to_string()));
        // Should not contain intermediate objects
        assert!(!paths.contains(&"vehicle".to_string()));
    }

    #[test]
    fn test_extract_value_at_path() {
        let val = serde_json::json!({
            "vehicle": { "speed": 45.2 },
            "meta": { "game": "iracing" }
        });
        assert_eq!(
            extract_value_at_path(&val, "vehicle.speed"),
            Some(serde_json::json!(45.2))
        );
        assert_eq!(
            extract_value_at_path(&val, "meta.game"),
            Some(serde_json::json!("iracing"))
        );
        assert_eq!(extract_value_at_path(&val, "nonexistent.path"), None);
    }

    #[test]
    fn test_channel_selector_literal() {
        let sel = ChannelSelector::parse("vehicle.speed,meta.tick").unwrap();
        let available = vec![
            "meta.game".to_string(),
            "meta.tick".to_string(),
            "vehicle.rpm".to_string(),
            "vehicle.speed".to_string(),
        ];
        let matched = sel.resolve(&available);
        assert_eq!(matched, vec!["meta.tick", "vehicle.speed"]);
    }

    #[test]
    fn test_channel_selector_literal_prefix() {
        let sel = ChannelSelector::parse("vehicle").unwrap();
        let available = vec![
            "meta.tick".to_string(),
            "vehicle.rpm".to_string(),
            "vehicle.speed".to_string(),
        ];
        let matched = sel.resolve(&available);
        assert_eq!(matched, vec!["vehicle.rpm", "vehicle.speed"]);
    }

    #[test]
    fn test_channel_selector_glob() {
        let sel = ChannelSelector::parse("vehicle.*").unwrap();
        let available = vec![
            "meta.tick".to_string(),
            "vehicle.rpm".to_string(),
            "vehicle.speed".to_string(),
            "vehicle.electronics.abs".to_string(),
        ];
        let matched = sel.resolve(&available);
        // * matches one segment only, so vehicle.electronics.abs should NOT match
        assert_eq!(matched, vec!["vehicle.rpm", "vehicle.speed"]);
    }

    #[test]
    fn test_channel_selector_double_glob() {
        let sel = ChannelSelector::parse("wheels.**").unwrap();
        let available = vec![
            "meta.tick".to_string(),
            "wheels.fl.tyre.pressure".to_string(),
            "wheels.fr.tyre.pressure".to_string(),
        ];
        let matched = sel.resolve(&available);
        assert_eq!(
            matched,
            vec!["wheels.fl.tyre.pressure", "wheels.fr.tyre.pressure"]
        );
    }

    #[test]
    fn test_channel_selector_regex() {
        let sel = ChannelSelector::parse("/engine\\..*/").unwrap();
        let available = vec![
            "engine.rpm".to_string(),
            "engine.water_temp".to_string(),
            "vehicle.speed".to_string(),
        ];
        let matched = sel.resolve(&available);
        assert_eq!(matched, vec!["engine.rpm", "engine.water_temp"]);
    }

    #[test]
    fn test_channel_selector_mixed() {
        let sel = ChannelSelector::parse("meta.tick,vehicle.*,/engine\\..*/").unwrap();
        let available = vec![
            "engine.rpm".to_string(),
            "engine.water_temp".to_string(),
            "meta.tick".to_string(),
            "vehicle.rpm".to_string(),
            "vehicle.speed".to_string(),
        ];
        let matched = sel.resolve(&available);
        assert_eq!(
            matched,
            vec![
                "engine.rpm",
                "engine.water_temp",
                "meta.tick",
                "vehicle.rpm",
                "vehicle.speed"
            ]
        );
    }
}
