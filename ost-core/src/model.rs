//! Unified telemetry data model
//!
//! Defines the TelemetryFrame structure that all adapters convert to.
//! Uses Option<T> for fields that not all games provide.
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

/// Complete telemetry frame with all available data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelemetryFrame {
    /// Timestamp when this frame was captured
    pub timestamp: DateTime<Utc>,

    /// Game/simulator name
    pub game: String,

    // === Motion Data ===
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

    // === Vehicle State ===
    /// Speed (magnitude of velocity) in m/s
    pub speed: Option<MetersPerSecond>,

    /// Engine RPM
    pub rpm: Option<Rpm>,

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

    /// Steering input (-1.0 = full left, 1.0 = full right)
    pub steering: Option<f32>,

    /// Engine temperature
    pub engine_temp: Option<Celsius>,

    /// Fuel level (0.0 to 1.0)
    pub fuel_level: Option<Percentage>,

    /// Fuel capacity in liters
    pub fuel_capacity: Option<f32>,

    // === Wheel Data (FL, FR, RL, RR) ===
    pub wheels: Option<WheelData>,

    // === Lap Timing ===
    /// Current lap time in seconds
    pub current_lap_time: Option<Seconds>,

    /// Last lap time
    pub last_lap_time: Option<Seconds>,

    /// Best lap time
    pub best_lap_time: Option<Seconds>,

    /// Sector times (typically 3 sectors)
    pub sector_times: Option<Vec<Seconds>>,

    /// Current lap number
    pub lap_number: Option<u32>,

    /// Position in race
    pub race_position: Option<u32>,

    /// Total number of cars
    pub num_cars: Option<u32>,

    // === Session Info ===
    /// Session type (practice, qualifying, race, etc.)
    pub session_type: Option<SessionType>,

    /// Session time remaining in seconds
    pub session_time_remaining: Option<Seconds>,

    /// Track temperature
    pub track_temp: Option<Celsius>,

    /// Air temperature
    pub air_temp: Option<Celsius>,

    /// Track name
    pub track_name: Option<String>,

    /// Car name/model
    pub car_name: Option<String>,

    /// Current flag status
    pub flag: Option<FlagType>,

    // === Damage ===
    /// Damage levels per body panel (0.0 = no damage, 1.0 = destroyed)
    pub damage: Option<DamageData>,

    // === Game-Specific Extras ===
    /// Game-specific data that doesn't fit the common model
    pub extras: HashMap<String, serde_json::Value>,
}

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

/// Information for a single wheel
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WheelInfo {
    /// Suspension travel (meters)
    pub suspension_travel: Option<Meters>,

    /// Tyre pressure (kPa)
    pub tyre_pressure: Option<Kilopascals>,

    /// Tyre surface temperature
    pub tyre_temp_surface: Option<Celsius>,

    /// Tyre inner temperature
    pub tyre_temp_inner: Option<Celsius>,

    /// Tyre wear (0.0 = new, 1.0 = worn out)
    pub tyre_wear: Option<Percentage>,

    /// Slip ratio (longitudinal slip)
    pub slip_ratio: Option<f32>,

    /// Slip angle (lateral slip) in radians
    pub slip_angle: Option<Radians>,

    /// Vertical load on tyre (Newtons)
    pub load: Option<Newtons>,

    /// Wheel rotation speed (rad/s)
    pub rotation_speed: Option<RadiansPerSecond>,
}

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

    /// Transmission damage
    pub transmission: Option<Percentage>,
}

/// Session type enumeration
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SessionType {
    Practice,
    Qualifying,
    Race,
    Hotlap,
    TimeTrial,
    Drift,
    Other,
}

/// Race flag types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FlagType {
    None,
    Green,
    Yellow,
    Blue,
    White,
    Checkered,
    Red,
    Black,
}

// === Field Masking for Selective Output ===

/// Specifies which fields to include in serialized output
///
/// This is used to reduce bandwidth and latency by only transmitting
/// the fields that a client needs.
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

    /// Create a builder for constructing masks
    pub fn builder() -> FieldMaskBuilder {
        FieldMaskBuilder::default()
    }

    /// Check if a field should be included
    pub fn includes(&self, field: &str) -> bool {
        self.include_all || self.fields.contains(&field.to_lowercase())
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

    pub fn rpm(self) -> Self {
        self.with_field("rpm")
    }

    pub fn speed(self) -> Self {
        self.with_field("speed")
    }

    pub fn gear(self) -> Self {
        self.with_field("gear")
    }

    pub fn throttle(self) -> Self {
        self.with_field("throttle")
    }

    pub fn brake(self) -> Self {
        self.with_field("brake")
    }

    pub fn steering(self) -> Self {
        self.with_field("steering")
    }

    pub fn g_force(self) -> Self {
        self.with_field("g_force")
    }

    pub fn position(self) -> Self {
        self.with_field("position")
    }

    pub fn velocity(self) -> Self {
        self.with_field("velocity")
    }

    pub fn build(self) -> FieldMask {
        FieldMask {
            fields: self.fields,
            include_all: false,
        }
    }
}

impl TelemetryFrame {
    /// Serialize this frame respecting the given field mask
    ///
    /// If mask is None or includes all fields, serialize everything.
    /// Otherwise, only include specified fields.
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

        // Conditionally include other fields
        if mask.includes("position") {
            if let Some(ref v) = self.position {
                map.insert("position".to_string(), serde_json::to_value(v)?);
            }
        }
        if mask.includes("velocity") {
            if let Some(ref v) = self.velocity {
                map.insert("velocity".to_string(), serde_json::to_value(v)?);
            }
        }
        if mask.includes("acceleration") {
            if let Some(ref v) = self.acceleration {
                map.insert("acceleration".to_string(), serde_json::to_value(v)?);
            }
        }
        if mask.includes("g_force") {
            if let Some(ref v) = self.g_force {
                map.insert("g_force".to_string(), serde_json::to_value(v)?);
            }
        }
        if mask.includes("rotation") {
            if let Some(ref v) = self.rotation {
                map.insert("rotation".to_string(), serde_json::to_value(v)?);
            }
        }
        if mask.includes("angular_velocity") {
            if let Some(ref v) = self.angular_velocity {
                map.insert("angular_velocity".to_string(), serde_json::to_value(v)?);
            }
        }
        if mask.includes("angular_acceleration") {
            if let Some(ref v) = self.angular_acceleration {
                map.insert("angular_acceleration".to_string(), serde_json::to_value(v)?);
            }
        }
        if mask.includes("speed") {
            if let Some(ref v) = self.speed {
                map.insert("speed".to_string(), serde_json::to_value(v)?);
            }
        }
        if mask.includes("rpm") {
            if let Some(ref v) = self.rpm {
                map.insert("rpm".to_string(), serde_json::to_value(v)?);
            }
        }
        if mask.includes("gear") {
            if let Some(ref v) = self.gear {
                map.insert("gear".to_string(), serde_json::to_value(v)?);
            }
        }
        if mask.includes("max_gears") {
            if let Some(ref v) = self.max_gears {
                map.insert("max_gears".to_string(), serde_json::to_value(v)?);
            }
        }
        if mask.includes("throttle") {
            if let Some(ref v) = self.throttle {
                map.insert("throttle".to_string(), serde_json::to_value(v)?);
            }
        }
        if mask.includes("brake") {
            if let Some(ref v) = self.brake {
                map.insert("brake".to_string(), serde_json::to_value(v)?);
            }
        }
        if mask.includes("clutch") {
            if let Some(ref v) = self.clutch {
                map.insert("clutch".to_string(), serde_json::to_value(v)?);
            }
        }
        if mask.includes("steering") {
            if let Some(ref v) = self.steering {
                map.insert("steering".to_string(), serde_json::to_value(v)?);
            }
        }
        if mask.includes("engine_temp") {
            if let Some(ref v) = self.engine_temp {
                map.insert("engine_temp".to_string(), serde_json::to_value(v)?);
            }
        }
        if mask.includes("fuel_level") {
            if let Some(ref v) = self.fuel_level {
                map.insert("fuel_level".to_string(), serde_json::to_value(v)?);
            }
        }
        if mask.includes("fuel_capacity") {
            if let Some(ref v) = self.fuel_capacity {
                map.insert("fuel_capacity".to_string(), serde_json::to_value(v)?);
            }
        }
        if mask.includes("wheels") {
            if let Some(ref v) = self.wheels {
                map.insert("wheels".to_string(), serde_json::to_value(v)?);
            }
        }
        if mask.includes("current_lap_time") {
            if let Some(ref v) = self.current_lap_time {
                map.insert("current_lap_time".to_string(), serde_json::to_value(v)?);
            }
        }
        if mask.includes("last_lap_time") {
            if let Some(ref v) = self.last_lap_time {
                map.insert("last_lap_time".to_string(), serde_json::to_value(v)?);
            }
        }
        if mask.includes("best_lap_time") {
            if let Some(ref v) = self.best_lap_time {
                map.insert("best_lap_time".to_string(), serde_json::to_value(v)?);
            }
        }
        if mask.includes("sector_times") {
            if let Some(ref v) = self.sector_times {
                map.insert("sector_times".to_string(), serde_json::to_value(v)?);
            }
        }
        if mask.includes("lap_number") {
            if let Some(ref v) = self.lap_number {
                map.insert("lap_number".to_string(), serde_json::to_value(v)?);
            }
        }
        if mask.includes("race_position") {
            if let Some(ref v) = self.race_position {
                map.insert("race_position".to_string(), serde_json::to_value(v)?);
            }
        }
        if mask.includes("num_cars") {
            if let Some(ref v) = self.num_cars {
                map.insert("num_cars".to_string(), serde_json::to_value(v)?);
            }
        }
        if mask.includes("session_type") {
            if let Some(ref v) = self.session_type {
                map.insert("session_type".to_string(), serde_json::to_value(v)?);
            }
        }
        if mask.includes("session_time_remaining") {
            if let Some(ref v) = self.session_time_remaining {
                map.insert(
                    "session_time_remaining".to_string(),
                    serde_json::to_value(v)?,
                );
            }
        }
        if mask.includes("track_temp") {
            if let Some(ref v) = self.track_temp {
                map.insert("track_temp".to_string(), serde_json::to_value(v)?);
            }
        }
        if mask.includes("air_temp") {
            if let Some(ref v) = self.air_temp {
                map.insert("air_temp".to_string(), serde_json::to_value(v)?);
            }
        }
        if mask.includes("track_name") {
            if let Some(ref v) = self.track_name {
                map.insert("track_name".to_string(), serde_json::to_value(v)?);
            }
        }
        if mask.includes("car_name") {
            if let Some(ref v) = self.car_name {
                map.insert("car_name".to_string(), serde_json::to_value(v)?);
            }
        }
        if mask.includes("flag") {
            if let Some(ref v) = self.flag {
                map.insert("flag".to_string(), serde_json::to_value(v)?);
            }
        }
        if mask.includes("damage") {
            if let Some(ref v) = self.damage {
                map.insert("damage".to_string(), serde_json::to_value(v)?);
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
    use crate::units::*;

    /// Helper to construct a minimal TelemetryFrame for testing
    fn make_test_frame() -> TelemetryFrame {
        TelemetryFrame {
            timestamp: Utc::now(),
            game: "TestGame".to_string(),
            position: None,
            velocity: None,
            acceleration: None,
            g_force: None,
            rotation: None,
            angular_velocity: None,
            angular_acceleration: None,
            speed: Some(MetersPerSecond(30.0)),
            rpm: Some(Rpm(5000.0)),
            gear: Some(3),
            max_gears: Some(6),
            throttle: Some(Percentage::new(0.75)),
            brake: Some(Percentage::new(0.0)),
            clutch: Some(Percentage::new(0.0)),
            steering: Some(0.1),
            engine_temp: Some(Celsius(90.0)),
            fuel_level: Some(Percentage::new(0.8)),
            fuel_capacity: Some(60.0),
            wheels: None,
            current_lap_time: Some(Seconds(45.2)),
            last_lap_time: Some(Seconds(87.3)),
            best_lap_time: Some(Seconds(85.1)),
            sector_times: None,
            lap_number: Some(5),
            race_position: Some(3),
            num_cars: Some(20),
            session_type: Some(SessionType::Race),
            session_time_remaining: Some(Seconds(1200.0)),
            track_temp: Some(Celsius(28.0)),
            air_temp: Some(Celsius(22.0)),
            track_name: Some("Test Track".to_string()),
            car_name: Some("Test Car".to_string()),
            flag: Some(FlagType::Green),
            damage: None,
            extras: HashMap::new(),
        }
    }

    #[test]
    fn test_field_mask_parse_comma_separated() {
        let mask = FieldMask::parse("speed,rpm,gear");
        assert!(mask.includes("speed"));
        assert!(mask.includes("rpm"));
        assert!(mask.includes("gear"));
        assert!(!mask.includes("throttle"));
        assert!(!mask.is_all());
    }

    #[test]
    fn test_field_mask_parse_with_whitespace() {
        let mask = FieldMask::parse(" speed , rpm , gear ");
        assert!(mask.includes("speed"));
        assert!(mask.includes("rpm"));
        assert!(mask.includes("gear"));
    }

    #[test]
    fn test_field_mask_parse_case_insensitive() {
        let mask = FieldMask::parse("Speed,RPM,Gear");
        assert!(mask.includes("speed"));
        assert!(mask.includes("rpm"));
        assert!(mask.includes("gear"));
    }

    #[test]
    fn test_field_mask_parse_empty_string() {
        let mask = FieldMask::parse("");
        assert!(!mask.is_all());
        // Empty string should produce no fields (the empty token is filtered out)
        assert!(!mask.includes("speed"));
    }

    #[test]
    fn test_field_mask_all() {
        let mask = FieldMask::all();
        assert!(mask.is_all());
        assert!(mask.includes("speed"));
        assert!(mask.includes("anything"));
    }

    #[test]
    fn test_field_mask_from_str() {
        let mask: FieldMask = "speed,rpm".parse().unwrap();
        assert!(mask.includes("speed"));
        assert!(mask.includes("rpm"));
        assert!(!mask.includes("gear"));
    }

    #[test]
    fn test_field_mask_builder() {
        let mask = FieldMask::builder()
            .speed()
            .rpm()
            .gear()
            .build();
        assert!(mask.includes("speed"));
        assert!(mask.includes("rpm"));
        assert!(mask.includes("gear"));
        assert!(!mask.includes("throttle"));
    }

    #[test]
    fn test_to_json_filtered_with_none_returns_full_frame() {
        let frame = make_test_frame();
        let json = frame.to_json_filtered(None).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

        // Full frame should contain all populated fields
        assert!(parsed.get("timestamp").is_some());
        assert!(parsed.get("game").is_some());
        assert!(parsed.get("speed").is_some());
        assert!(parsed.get("rpm").is_some());
        assert!(parsed.get("gear").is_some());
        assert!(parsed.get("throttle").is_some());
        assert!(parsed.get("track_name").is_some());
        assert!(parsed.get("car_name").is_some());
    }

    #[test]
    fn test_to_json_filtered_with_all_mask_returns_full_frame() {
        let frame = make_test_frame();
        let mask = FieldMask::all();
        let json = frame.to_json_filtered(Some(&mask)).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

        assert!(parsed.get("speed").is_some());
        assert!(parsed.get("rpm").is_some());
        assert!(parsed.get("gear").is_some());
        assert!(parsed.get("track_name").is_some());
    }

    #[test]
    fn test_to_json_filtered_with_mask_returns_only_requested_fields() {
        let frame = make_test_frame();
        let mask = FieldMask::parse("speed,rpm");
        let json = frame.to_json_filtered(Some(&mask)).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

        // Always-included fields
        assert!(parsed.get("timestamp").is_some());
        assert!(parsed.get("game").is_some());

        // Requested fields
        assert!(parsed.get("speed").is_some());
        assert!(parsed.get("rpm").is_some());

        // Fields NOT requested should be absent
        assert!(parsed.get("gear").is_none());
        assert!(parsed.get("throttle").is_none());
        assert!(parsed.get("track_name").is_none());
        assert!(parsed.get("car_name").is_none());
        assert!(parsed.get("flag").is_none());
    }

    #[test]
    fn test_to_json_filtered_with_mask_for_none_field() {
        let frame = make_test_frame();
        // position is None in our test frame
        let mask = FieldMask::parse("position,speed");
        let json = frame.to_json_filtered(Some(&mask)).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

        // position is None, so it should not appear even if requested
        assert!(parsed.get("position").is_none());
        // speed is Some, so it should appear
        assert!(parsed.get("speed").is_some());
    }

    #[test]
    fn test_telemetry_frame_serialization_roundtrip() {
        let frame = make_test_frame();
        let json = serde_json::to_string(&frame).unwrap();
        let deserialized: TelemetryFrame = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.game, "TestGame");
        assert_eq!(deserialized.gear, Some(3));
        assert_eq!(deserialized.max_gears, Some(6));
        assert_eq!(deserialized.race_position, Some(3));
        assert_eq!(deserialized.session_type, Some(SessionType::Race));
        assert_eq!(deserialized.flag, Some(FlagType::Green));
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
    fn test_flag_type_serialization() {
        let flag = FlagType::Yellow;
        let json = serde_json::to_string(&flag).unwrap();
        assert_eq!(json, "\"Yellow\"");

        let deserialized: FlagType = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, FlagType::Yellow);
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
