//! Demo adapter that generates synthetic telemetry for testing
//!
//! This adapter always reports as "detected" and generates realistic-looking
//! telemetry data at 60Hz without requiring an actual game.

use anyhow::Result;
use chrono::Utc;
use ost_core::{
    adapter::TelemetryAdapter,
    model::*,
    units::*,
};
use std::collections::HashMap;
use std::f32::consts::PI;
use std::time::Instant;

pub struct DemoAdapter {
    active: bool,
    start_time: Option<Instant>,
    frame_count: u64,
}

impl DemoAdapter {
    pub fn new() -> Self {
        Self {
            active: false,
            start_time: None,
            frame_count: 0,
        }
    }

    /// Generate synthetic telemetry data
    fn generate_frame(&mut self) -> TelemetryFrame {
        let elapsed = self
            .start_time
            .map(|t| t.elapsed().as_secs_f32())
            .unwrap_or(0.0);

        self.frame_count += 1;

        // Simulate oscillating RPM (3000-7000 RPM, 2 second cycle)
        let rpm_base = 5000.0;
        let rpm_amplitude = 2000.0;
        let rpm = rpm_base + rpm_amplitude * (elapsed * PI).sin();

        // Simulate speed increasing/decreasing
        let speed = 30.0 + 20.0 * (elapsed * 0.5).sin();

        // Simulate gear based on speed
        let gear = ((speed / 10.0).floor() as i8).clamp(1, 6);

        // Simulate G-forces (lateral and longitudinal)
        let lateral_g = 1.5 * (elapsed * 0.7).sin();
        let longitudinal_g = 0.5 * (elapsed * 0.9).cos();
        let vertical_g = -1.0; // Gravity

        // Create wheel data
        let wheel_base = |offset: f32| WheelInfo {
            suspension_travel: Some(Meters(0.05 + 0.02 * (elapsed + offset).sin())),
            tyre_pressure: Some(Kilopascals(180.0 + 5.0 * (elapsed * 0.1 + offset).sin())),
            tyre_temp_surface: Some(Celsius(80.0 + 15.0 * (elapsed * 0.05 + offset).cos())),
            tyre_temp_inner: Some(Celsius(85.0 + 10.0 * (elapsed * 0.05 + offset).cos())),
            tyre_wear: Some(Percentage::new(0.1 + 0.02 * (elapsed * 0.01))),
            slip_ratio: Some(0.05 * (elapsed * 2.0 + offset).sin()),
            slip_angle: Some(Radians(0.02 * (elapsed + offset).cos())),
            load: Some(Newtons(2500.0 + 500.0 * (elapsed * 0.5 + offset).sin())),
            rotation_speed: Some(RadiansPerSecond(speed * 10.0)),
        };

        TelemetryFrame {
            timestamp: Utc::now(),
            game: "Demo".to_string(),

            // Motion
            position: Some(Vector3::new(
                Meters(elapsed * 10.0),
                Meters(0.5),
                Meters(elapsed * 5.0),
            )),
            velocity: Some(Vector3::new(
                MetersPerSecond(lateral_g * 2.0),
                MetersPerSecond(0.0),
                MetersPerSecond(speed),
            )),
            acceleration: Some(Vector3::new(
                MetersPerSecondSquared(lateral_g * 9.81),
                MetersPerSecondSquared(vertical_g * 9.81),
                MetersPerSecondSquared(longitudinal_g * 9.81),
            )),
            g_force: Some(Vector3::new(
                GForce(lateral_g),
                GForce(vertical_g),
                GForce(longitudinal_g),
            )),
            rotation: Some(Vector3::new(
                Radians(0.0),                         // pitch
                Radians(elapsed * 0.1),               // yaw (slowly rotating)
                Radians(0.05 * (elapsed * 0.5).sin()), // roll
            )),
            angular_velocity: Some(Vector3::new(
                RadiansPerSecond(0.0),
                RadiansPerSecond(0.1),
                RadiansPerSecond(0.05 * (elapsed * 0.5).cos() * 0.5),
            )),
            angular_acceleration: Some(Vector3::new(
                RadiansPerSecondSquared(0.0),
                RadiansPerSecondSquared(0.0),
                RadiansPerSecondSquared(-0.025 * (elapsed * 0.5).sin() * 0.25),
            )),

            // Vehicle state
            speed: Some(MetersPerSecond(speed)),
            rpm: Some(Rpm(rpm)),
            gear: Some(gear),
            max_gears: Some(6),
            throttle: Some(Percentage::new(0.6 + 0.3 * (elapsed * 0.8).sin())),
            brake: Some(Percentage::new(
                ((elapsed * 0.3).sin() * 0.5 + 0.5).max(0.0) * 0.2,
            )),
            clutch: Some(Percentage::new(0.0)),
            steering: Some(0.3 * (elapsed * 0.7).sin()),
            engine_temp: Some(Celsius(90.0 + 5.0 * (elapsed * 0.01).min(1.0))),
            fuel_level: Some(Percentage::new(1.0 - (elapsed * 0.001).min(0.5))),
            fuel_capacity: Some(60.0),

            // Wheels
            wheels: Some(WheelData {
                front_left: wheel_base(0.0),
                front_right: wheel_base(PI / 2.0),
                rear_left: wheel_base(PI),
                rear_right: wheel_base(3.0 * PI / 2.0),
            }),

            // Lap timing
            current_lap_time: Some(Seconds(elapsed % 90.0)),
            last_lap_time: Some(Seconds(87.3)),
            best_lap_time: Some(Seconds(85.1)),
            sector_times: Some(vec![Seconds(28.4), Seconds(29.1), Seconds(27.6)]),
            lap_number: Some((elapsed / 90.0) as u32 + 1),
            race_position: Some(3),
            num_cars: Some(20),

            // Session
            session_type: Some(SessionType::Race),
            session_time_remaining: Some(Seconds(1800.0 - elapsed)),
            track_temp: Some(Celsius(28.0)),
            air_temp: Some(Celsius(22.0)),
            track_name: Some("Demo Circuit".to_string()),
            car_name: Some("Formula Demo".to_string()),
            flag: Some(FlagType::Green),

            // Damage
            damage: Some(DamageData {
                front: Some(Percentage::new(0.0)),
                rear: Some(Percentage::new(0.0)),
                left: Some(Percentage::new(0.05)),
                right: Some(Percentage::new(0.0)),
                engine: Some(Percentage::new(0.0)),
                transmission: Some(Percentage::new(0.0)),
            }),

            // Extras
            extras: {
                let mut map = HashMap::new();
                map.insert(
                    "demo_frame_count".to_string(),
                    serde_json::json!(self.frame_count),
                );
                map
            },
        }
    }
}

impl Default for DemoAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl TelemetryAdapter for DemoAdapter {
    fn name(&self) -> &str {
        "Demo"
    }

    fn detect(&self) -> bool {
        // Demo adapter is always "detected"
        true
    }

    fn start(&mut self) -> Result<()> {
        self.active = true;
        self.start_time = Some(Instant::now());
        self.frame_count = 0;
        Ok(())
    }

    fn stop(&mut self) -> Result<()> {
        self.active = false;
        self.start_time = None;
        Ok(())
    }

    fn read_frame(&mut self) -> Result<Option<TelemetryFrame>> {
        if !self.active {
            return Ok(None);
        }

        // Simulate 60Hz update rate
        // In a real adapter, this would read from shared memory or UDP
        // For demo, we just generate a frame every time
        Ok(Some(self.generate_frame()))
    }

    fn is_active(&self) -> bool {
        self.active
    }
}
