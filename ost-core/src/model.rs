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
// TelemetryFrame — top-level container
// =============================================================================

/// Complete telemetry frame with all available data, organized by domain.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelemetryFrame {
    /// Timestamp when this frame was captured
    pub timestamp: DateTime<Utc>,

    /// Game/simulator name
    pub game: String,

    /// Sample tick/frame number from the sim
    pub tick: Option<u32>,

    // === Domain sections ===
    pub motion: Option<MotionData>,
    pub vehicle: Option<VehicleData>,
    pub engine: Option<EngineData>,
    pub wheels: Option<WheelData>,
    pub timing: Option<TimingData>,
    pub session: Option<SessionData>,
    pub weather: Option<WeatherData>,
    pub pit: Option<PitData>,
    pub electronics: Option<ElectronicsData>,
    pub damage: Option<DamageData>,
    pub competitors: Option<Vec<CompetitorData>>,
    pub driver: Option<DriverData>,

    /// Game-specific data that doesn't fit the normalized model.
    /// Keys use slash-separated game prefix: "iracing/SessionTick", "acc/RealTimeCarUpdate"
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
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
    pub position: Option<Vector3<Meters>>,

    /// Linear velocity in car-local space (m/s)
    pub velocity: Option<Vector3<MetersPerSecond>>,

    /// Linear acceleration in car-local space (m/s²)
    pub acceleration: Option<Vector3<MetersPerSecondSquared>>,

    /// G-forces experienced (derived from acceleration)
    pub g_force: Option<Vector3<GForce>>,

    /// Rotation (pitch, yaw, roll) in radians
    pub rotation: Option<Vector3<Radians>>,

    /// Angular velocity (rad/s)
    pub angular_velocity: Option<Vector3<RadiansPerSecond>>,

    /// Angular acceleration (rad/s²)
    pub angular_acceleration: Option<Vector3<RadiansPerSecondSquared>>,
}

// =============================================================================
// VehicleData
// =============================================================================

/// Driver inputs and basic vehicle state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VehicleData {
    /// Speed magnitude (m/s)
    pub speed: Option<MetersPerSecond>,

    /// Engine RPM
    pub rpm: Option<Rpm>,

    /// Redline RPM (from session info)
    pub max_rpm: Option<Rpm>,

    /// Idle RPM (from session info)
    pub idle_rpm: Option<Rpm>,

    /// Current gear (-1 = reverse, 0 = neutral, 1+ = forward gears)
    pub gear: Option<i8>,

    /// Maximum gears available
    pub max_gears: Option<u8>,

    /// Throttle input (0.0 to 1.0)
    pub throttle: Option<Percentage>,

    /// Brake input (0.0 to 1.0)
    pub brake: Option<Percentage>,

    /// Clutch input (0.0 = engaged, 1.0 = disengaged)
    pub clutch: Option<Percentage>,

    /// Steering wheel angle in radians
    pub steering_angle: Option<Radians>,

    /// Steering wheel torque
    pub steering_torque: Option<NewtonMeters>,

    /// Steering wheel torque as percentage of max
    pub steering_torque_pct: Option<Percentage>,

    /// Handbrake input (0.0 to 1.0) — not available in iRacing
    pub handbrake: Option<Percentage>,

    /// Whether the car is on the track
    pub on_track: Option<bool>,

    /// Whether the car is in the garage
    pub in_garage: Option<bool>,

    /// What surface the player's car is currently on
    pub track_surface: Option<TrackSurface>,
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
    pub water_temp: Option<Celsius>,

    /// Oil temperature
    pub oil_temp: Option<Celsius>,

    /// Oil pressure
    pub oil_pressure: Option<Kilopascals>,

    /// Oil level (0.0 to 1.0)
    pub oil_level: Option<Percentage>,

    /// Fuel level in liters
    pub fuel_level: Option<Liters>,

    /// Fuel level as percentage of capacity
    pub fuel_level_pct: Option<Percentage>,

    /// Fuel tank capacity in liters (from session info)
    pub fuel_capacity: Option<Liters>,

    /// Fuel pressure
    pub fuel_pressure: Option<Kilopascals>,

    /// Fuel consumption rate
    pub fuel_use_per_hour: Option<LitersPerHour>,

    /// Battery/alternator voltage
    pub voltage: Option<Volts>,

    /// Manifold pressure
    pub manifold_pressure: Option<Bar>,

    /// Engine warning flags
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
    /// Suspension/shock deflection (meters)
    pub suspension_travel: Option<Meters>,

    /// Short-term averaged suspension deflection (meters)
    pub suspension_travel_avg: Option<Meters>,

    /// Shock velocity (m/s)
    pub shock_velocity: Option<MetersPerSecond>,

    /// Short-term averaged shock velocity (m/s)
    pub shock_velocity_avg: Option<MetersPerSecond>,

    /// Ride height at this corner (meters)
    pub ride_height: Option<Meters>,

    // --- Tyre pressure ---
    /// Current tyre air pressure (kPa)
    pub tyre_pressure: Option<Kilopascals>,

    /// Cold tyre pressure from setup (kPa)
    pub tyre_cold_pressure: Option<Kilopascals>,

    // --- Tyre surface temperatures (inner/middle/outer relative to car center) ---
    /// Surface temp at inner edge (toward car center)
    pub surface_temp_inner: Option<Celsius>,

    /// Surface temp at middle of tread
    pub surface_temp_middle: Option<Celsius>,

    /// Surface temp at outer edge (away from car center)
    pub surface_temp_outer: Option<Celsius>,

    // --- Tyre carcass temperatures ---
    /// Carcass temp at inner position
    pub carcass_temp_inner: Option<Celsius>,

    /// Carcass temp at middle position
    pub carcass_temp_middle: Option<Celsius>,

    /// Carcass temp at outer position
    pub carcass_temp_outer: Option<Celsius>,

    // --- Wear & dynamics ---
    /// Tyre wear (0.0 = new, 1.0 = worn out)
    pub tyre_wear: Option<Percentage>,

    /// Wheel rotation speed (rad/s)
    pub wheel_speed: Option<RadiansPerSecond>,

    /// Longitudinal slip ratio
    pub slip_ratio: Option<f32>,

    /// Lateral slip angle (radians)
    pub slip_angle: Option<Radians>,

    /// Vertical load on tyre (Newtons)
    pub load: Option<Newtons>,

    // --- Brakes ---
    /// Brake line pressure (kPa)
    pub brake_line_pressure: Option<Kilopascals>,

    /// Brake disc/rotor temperature
    pub brake_temp: Option<Celsius>,

    // --- Compound ---
    /// Tyre compound name or index
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
    pub current_lap_time: Option<Seconds>,

    /// Last completed lap time
    pub last_lap_time: Option<Seconds>,

    /// Personal best lap time
    pub best_lap_time: Option<Seconds>,

    /// Best N-lap average time
    pub best_n_lap_time: Option<Seconds>,

    /// Lap number of best N-lap average
    pub best_n_lap_num: Option<u32>,

    /// Sector times for current/last lap
    pub sector_times: Option<Vec<Seconds>>,

    /// Current lap number
    pub lap_number: Option<u32>,

    /// Laps completed
    pub laps_completed: Option<u32>,

    /// Distance around track (meters)
    pub lap_distance: Option<Meters>,

    /// Distance around track as percentage (0.0 to 1.0)
    pub lap_distance_pct: Option<Percentage>,

    /// Overall race position
    pub race_position: Option<u32>,

    /// Position within class
    pub class_position: Option<u32>,

    /// Total number of cars in session
    pub num_cars: Option<u32>,

    /// Delta to personal best lap (seconds, negative = ahead)
    pub delta_best: Option<Seconds>,

    /// Whether delta_best is valid/usable
    pub delta_best_ok: Option<bool>,

    /// Delta to session best lap
    pub delta_session_best: Option<Seconds>,

    /// Whether delta_session_best is valid
    pub delta_session_best_ok: Option<bool>,

    /// Delta to optimal lap (theoretical best from best sectors)
    pub delta_optimal: Option<Seconds>,

    /// Whether delta_optimal is valid
    pub delta_optimal_ok: Option<bool>,

    /// Estimated lap time (from session info)
    pub estimated_lap_time: Option<Seconds>,

    /// Total race laps completed by leader
    pub race_laps: Option<u32>,
}

// =============================================================================
// SessionData
// =============================================================================

/// Session state, identity, and metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionData {
    /// Session type (practice, qualifying, race, etc.)
    pub session_type: Option<SessionType>,

    /// Current session state (warmup, racing, checkered, etc.)
    pub session_state: Option<SessionState>,

    /// Elapsed session time
    pub session_time: Option<Seconds>,

    /// Time remaining in session
    pub session_time_remaining: Option<Seconds>,

    /// In-sim time of day
    pub session_time_of_day: Option<Seconds>,

    /// Total laps for this session (None = unlimited)
    pub session_laps: Option<u32>,

    /// Laps remaining in session
    pub session_laps_remaining: Option<u32>,

    /// Comprehensive flag state (multiple flags can be active)
    pub flags: Option<FlagState>,

    /// Track display name
    pub track_name: Option<String>,

    /// Track configuration/layout name
    pub track_config: Option<String>,

    /// Track length
    pub track_length: Option<Meters>,

    /// Track type (Road, Oval, Dirt, etc.)
    pub track_type: Option<String>,

    /// Player's car name
    pub car_name: Option<String>,

    /// Player's car class
    pub car_class: Option<String>,
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
    pub air_temp: Option<Celsius>,

    /// Track surface temperature
    pub track_temp: Option<Celsius>,

    /// Atmospheric pressure
    pub air_pressure: Option<Pascals>,

    /// Air density
    pub air_density: Option<KilogramsPerCubicMeter>,

    /// Relative humidity (0.0 to 1.0)
    pub humidity: Option<Percentage>,

    /// Wind speed
    pub wind_speed: Option<MetersPerSecond>,

    /// Wind direction (radians, relative to north)
    pub wind_direction: Option<Radians>,

    /// Fog level (0.0 to 1.0)
    pub fog_level: Option<Percentage>,

    /// Precipitation amount (0.0 to 1.0)
    pub precipitation: Option<Percentage>,

    /// Track wetness level
    pub track_wetness: Option<TrackWetness>,

    /// Sky condition description
    pub skies: Option<String>,

    /// Whether the race has been declared wet
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
    pub on_pit_road: Option<bool>,

    /// Whether a pit stop is currently active
    pub pit_active: Option<bool>,

    /// Pit service status code
    pub pit_service_status: Option<u32>,

    /// Mandatory repair time remaining (seconds)
    pub repair_time_left: Option<Seconds>,

    /// Optional repair time remaining (seconds)
    pub optional_repair_time_left: Option<Seconds>,

    /// Number of fast repairs available
    pub fast_repair_available: Option<u32>,

    /// Number of fast repairs used
    pub fast_repair_used: Option<u32>,

    /// Pit lane speed limit
    pub pit_speed_limit: Option<MetersPerSecond>,

    /// Requested pit services for next stop
    pub requested_services: Option<PitServices>,
}

/// Detailed pit service request state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PitServices {
    /// Fuel to add (liters)
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
    pub tyre_pressure_fl: Option<Kilopascals>,

    /// Requested cold pressure for front-right
    pub tyre_pressure_fr: Option<Kilopascals>,

    /// Requested cold pressure for rear-left
    pub tyre_pressure_rl: Option<Kilopascals>,

    /// Requested cold pressure for rear-right
    pub tyre_pressure_rr: Option<Kilopascals>,
}

// =============================================================================
// ElectronicsData
// =============================================================================

/// Driver aids and electronic systems
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ElectronicsData {
    /// ABS setting level
    pub abs: Option<f32>,

    /// Traction control setting
    pub traction_control: Option<f32>,

    /// Secondary traction control setting
    pub traction_control_2: Option<f32>,

    /// Brake bias (percentage front)
    pub brake_bias: Option<Percentage>,

    /// Front anti-roll bar setting
    pub anti_roll_front: Option<f32>,

    /// Rear anti-roll bar setting
    pub anti_roll_rear: Option<f32>,

    /// DRS (drag reduction system) status
    pub drs_status: Option<u32>,

    /// Push-to-pass status
    pub push_to_pass_status: Option<u32>,

    /// Push-to-pass remaining count
    pub push_to_pass_count: Option<u32>,

    /// Throttle shape/map setting
    pub throttle_shape: Option<f32>,
}

// =============================================================================
// DamageData
// =============================================================================

/// Vehicle damage information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DamageData {
    /// Front damage (0.0 to 1.0)
    pub front: Option<Percentage>,

    /// Rear damage
    pub rear: Option<Percentage>,

    /// Left side damage
    pub left: Option<Percentage>,

    /// Right side damage
    pub right: Option<Percentage>,

    /// Engine damage
    pub engine: Option<Percentage>,

    /// Transmission/gearbox damage
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
    pub driver_name: Option<String>,
    pub car_name: Option<String>,
    pub car_class: Option<String>,
    pub team_name: Option<String>,
    pub car_number: Option<String>,

    // --- From live telemetry (per-tick CarIdx arrays) ---
    /// Current lap
    pub lap: Option<u32>,

    /// Laps completed
    pub laps_completed: Option<u32>,

    /// Track position as percentage (0.0 to 1.0)
    pub lap_distance_pct: Option<Percentage>,

    /// Overall position
    pub position: Option<u32>,

    /// Position within class
    pub class_position: Option<u32>,

    /// Whether this car is on pit road
    pub on_pit_road: Option<bool>,

    /// Surface this car is on
    pub track_surface: Option<TrackSurface>,

    /// Best lap time
    pub best_lap_time: Option<Seconds>,

    /// Last lap time
    pub last_lap_time: Option<Seconds>,

    /// Estimated time around track
    pub estimated_time: Option<Seconds>,

    /// Current gear
    pub gear: Option<i8>,

    /// Current RPM
    pub rpm: Option<Rpm>,

    /// Steering angle
    pub steering: Option<Radians>,
}

// =============================================================================
// DriverData
// =============================================================================

/// Player driver metadata (mostly from session info, relatively static)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DriverData {
    pub name: Option<String>,
    pub car_index: Option<u32>,
    pub car_name: Option<String>,
    pub car_class: Option<String>,
    pub car_number: Option<String>,
    pub team_name: Option<String>,
    pub fuel_capacity: Option<Liters>,
    pub shift_light_first_rpm: Option<Rpm>,
    pub shift_light_shift_rpm: Option<Rpm>,
    pub shift_light_last_rpm: Option<Rpm>,
    pub shift_light_blink_rpm: Option<Rpm>,
    pub estimated_lap_time: Option<Seconds>,
    pub setup_name: Option<String>,
}

// =============================================================================
// Field Masking for Selective Output
// =============================================================================

/// Specifies which fields to include in serialized output.
///
/// Supports both section-level filtering (`vehicle`, `timing`) and
/// dotted sub-field filtering (`vehicle.speed`, `timing.best_lap_time`).
#[derive(Debug, Clone, Default)]
pub struct FieldMask {
    fields: HashSet<String>,
    include_all: bool,
}

impl FieldMask {
    /// Create a mask that includes all fields
    pub fn all() -> Self {
        Self {
            fields: HashSet::new(),
            include_all: true,
        }
    }

    /// Create a mask from a comma-separated list of field names
    pub fn parse(fields: &str) -> Self {
        let fields: HashSet<String> = fields
            .split(',')
            .map(|s| s.trim().to_lowercase())
            .filter(|s| !s.is_empty())
            .collect();

        Self {
            fields,
            include_all: false,
        }
    }

    /// Check if a field should be included.
    ///
    /// Returns true if:
    /// - All fields are included (no mask)
    /// - The exact field name matches (e.g. "vehicle")
    /// - A parent section matches (e.g. "vehicle" includes "vehicle.speed")
    /// - The specific dotted path matches (e.g. "vehicle.speed")
    pub fn includes(&self, field: &str) -> bool {
        if self.include_all {
            return true;
        }

        let field_lower = field.to_lowercase();

        // Exact match
        if self.fields.contains(&field_lower) {
            return true;
        }

        // Check if any requested field is a parent section of this field
        // e.g. if mask has "vehicle" and field is "vehicle.speed"
        if let Some(dot_pos) = field_lower.find('.') {
            let section = &field_lower[..dot_pos];
            if self.fields.contains(section) {
                return true;
            }
        }

        // Check if any requested field is a child of this section
        // e.g. if mask has "vehicle.speed" and field is "vehicle" (the section)
        for f in &self.fields {
            if f.starts_with(&field_lower) && f.as_bytes().get(field_lower.len()) == Some(&b'.') {
                return true;
            }
        }

        false
    }

    /// Check if all fields should be included
    pub fn is_all(&self) -> bool {
        self.include_all
    }
}

impl FromStr for FieldMask {
    type Err = std::convert::Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self::parse(s))
    }
}

/// Builder for FieldMask
#[derive(Debug, Default)]
pub struct FieldMaskBuilder {
    fields: HashSet<String>,
}

impl FieldMaskBuilder {
    pub fn with_field(mut self, field: &str) -> Self {
        self.fields.insert(field.to_lowercase());
        self
    }

    pub fn motion(self) -> Self {
        self.with_field("motion")
    }

    pub fn vehicle(self) -> Self {
        self.with_field("vehicle")
    }

    pub fn engine(self) -> Self {
        self.with_field("engine")
    }

    pub fn wheels(self) -> Self {
        self.with_field("wheels")
    }

    pub fn timing(self) -> Self {
        self.with_field("timing")
    }

    pub fn session(self) -> Self {
        self.with_field("session")
    }

    pub fn weather(self) -> Self {
        self.with_field("weather")
    }

    pub fn pit(self) -> Self {
        self.with_field("pit")
    }

    pub fn electronics(self) -> Self {
        self.with_field("electronics")
    }

    pub fn damage(self) -> Self {
        self.with_field("damage")
    }

    pub fn competitors(self) -> Self {
        self.with_field("competitors")
    }

    pub fn driver(self) -> Self {
        self.with_field("driver")
    }

    pub fn build(self) -> FieldMask {
        FieldMask {
            fields: self.fields,
            include_all: false,
        }
    }
}

// =============================================================================
// Filtered serialization
// =============================================================================

impl TelemetryFrame {
    /// Serialize this frame respecting the given field mask.
    ///
    /// If mask is None or includes all fields, serialize everything.
    /// Otherwise, only include specified sections/fields.
    pub fn to_json_filtered(&self, mask: Option<&FieldMask>) -> serde_json::Result<String> {
        if mask.is_none() || mask.map(|m| m.is_all()).unwrap_or(true) {
            return serde_json::to_string(self);
        }

        let mask = mask.unwrap();
        let mut map = serde_json::Map::new();

        // Always include timestamp and game
        map.insert(
            "timestamp".to_string(),
            serde_json::to_value(self.timestamp)?,
        );
        map.insert("game".to_string(), serde_json::to_value(&self.game)?);

        if let Some(ref v) = self.tick {
            map.insert("tick".to_string(), serde_json::to_value(v)?);
        }

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
        if mask.includes("electronics") {
            if let Some(ref v) = self.electronics {
                map.insert("electronics".to_string(), serde_json::to_value(v)?);
            }
        }
        if mask.includes("damage") {
            if let Some(ref v) = self.damage {
                map.insert("damage".to_string(), serde_json::to_value(v)?);
            }
        }
        if mask.includes("competitors") {
            if let Some(ref v) = self.competitors {
                map.insert("competitors".to_string(), serde_json::to_value(v)?);
            }
        }
        if mask.includes("driver") {
            if let Some(ref v) = self.driver {
                map.insert("driver".to_string(), serde_json::to_value(v)?);
            }
        }
        if mask.includes("extras") && !self.extras.is_empty() {
            map.insert("extras".to_string(), serde_json::to_value(&self.extras)?);
        }

        serde_json::to_string(&map)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper to construct a minimal TelemetryFrame for testing
    fn make_test_frame() -> TelemetryFrame {
        TelemetryFrame {
            timestamp: Utc::now(),
            game: "TestGame".to_string(),
            tick: Some(42),
            motion: Some(MotionData {
                position: None,
                velocity: None,
                acceleration: None,
                g_force: Some(Vector3::new(GForce(0.3), GForce(1.0), GForce(-0.5))),
                rotation: None,
                angular_velocity: None,
                angular_acceleration: None,
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
                steering_angle: Some(Radians(0.1)),
                steering_torque: None,
                steering_torque_pct: None,
                on_track: None,
                in_garage: None,
                track_surface: None,
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
                car_name: Some("Test Car".to_string()),
                car_class: None,
            }),
            weather: None,
            pit: None,
            electronics: None,
            damage: None,
            competitors: None,
            driver: None,
            extras: HashMap::new(),
        }
    }

    #[test]
    fn test_field_mask_parse_comma_separated() {
        let mask = FieldMask::parse("vehicle,timing,motion");
        assert!(mask.includes("vehicle"));
        assert!(mask.includes("timing"));
        assert!(mask.includes("motion"));
        assert!(!mask.includes("weather"));
        assert!(!mask.is_all());
    }

    #[test]
    fn test_field_mask_parse_with_whitespace() {
        let mask = FieldMask::parse(" vehicle , timing , motion ");
        assert!(mask.includes("vehicle"));
        assert!(mask.includes("timing"));
        assert!(mask.includes("motion"));
    }

    #[test]
    fn test_field_mask_parse_case_insensitive() {
        let mask = FieldMask::parse("Vehicle,TIMING,Motion");
        assert!(mask.includes("vehicle"));
        assert!(mask.includes("timing"));
        assert!(mask.includes("motion"));
    }

    #[test]
    fn test_field_mask_parse_empty_string() {
        let mask = FieldMask::parse("");
        assert!(!mask.is_all());
        assert!(!mask.includes("vehicle"));
    }

    #[test]
    fn test_field_mask_all() {
        let mask = FieldMask::all();
        assert!(mask.is_all());
        assert!(mask.includes("vehicle"));
        assert!(mask.includes("anything"));
    }

    #[test]
    fn test_field_mask_from_str() {
        let mask: FieldMask = "vehicle,timing".parse().unwrap();
        assert!(mask.includes("vehicle"));
        assert!(mask.includes("timing"));
        assert!(!mask.includes("engine"));
    }

    #[test]
    fn test_field_mask_builder() {
        let mask = FieldMaskBuilder::default()
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
    fn test_field_mask_section_includes_subfields() {
        let mask = FieldMask::parse("vehicle");
        assert!(mask.includes("vehicle"));
        assert!(mask.includes("vehicle.speed"));
        assert!(!mask.includes("timing"));
    }

    #[test]
    fn test_to_json_filtered_with_none_returns_full_frame() {
        let frame = make_test_frame();
        let json = frame.to_json_filtered(None).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

        assert!(parsed.get("timestamp").is_some());
        assert!(parsed.get("game").is_some());
        assert!(parsed.get("vehicle").is_some());
        assert!(parsed.get("timing").is_some());
        assert!(parsed.get("session").is_some());
    }

    #[test]
    fn test_to_json_filtered_with_all_mask_returns_full_frame() {
        let frame = make_test_frame();
        let mask = FieldMask::all();
        let json = frame.to_json_filtered(Some(&mask)).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

        assert!(parsed.get("vehicle").is_some());
        assert!(parsed.get("timing").is_some());
        assert!(parsed.get("session").is_some());
    }

    #[test]
    fn test_to_json_filtered_with_mask_returns_only_requested_sections() {
        let frame = make_test_frame();
        let mask = FieldMask::parse("vehicle,timing");
        let json = frame.to_json_filtered(Some(&mask)).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

        // Always-included fields
        assert!(parsed.get("timestamp").is_some());
        assert!(parsed.get("game").is_some());

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
        let mask = FieldMask::parse("weather,vehicle");
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

        assert_eq!(deserialized.game, "TestGame");
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
}
