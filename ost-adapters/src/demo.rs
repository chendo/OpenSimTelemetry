//! Demo adapter that generates synthetic telemetry for testing
//!
//! This adapter always reports as "detected" and generates realistic-looking
//! telemetry data at 60Hz without requiring an actual game.

use anyhow::Result;
use chrono::Utc;
use ost_core::{adapter::TelemetryAdapter, model::*, units::*};
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

        // Simulate G-forces
        let lateral_g = 1.5 * (elapsed * 0.7).sin();
        let longitudinal_g = 0.5 * (elapsed * 0.9).cos();
        let vertical_g = -1.0;

        // --- Wheel helper ---
        let make_wheel = |offset: f32| WheelInfo {
            suspension_travel: Some(Meters(0.05 + 0.02 * (elapsed + offset).sin())),
            suspension_travel_avg: Some(Meters(0.05)),
            shock_velocity: Some(MetersPerSecond(0.01 * (elapsed * 2.0 + offset).cos())),
            shock_velocity_avg: Some(MetersPerSecond(0.005)),
            ride_height: Some(Meters(0.06 + 0.005 * (elapsed + offset).sin())),
            tyre_pressure: Some(Kilopascals(180.0 + 5.0 * (elapsed * 0.1 + offset).sin())),
            tyre_cold_pressure: Some(Kilopascals(172.0)),
            surface_temp_inner: Some(Celsius(82.0 + 12.0 * (elapsed * 0.05 + offset).cos())),
            surface_temp_middle: Some(Celsius(80.0 + 15.0 * (elapsed * 0.05 + offset).cos())),
            surface_temp_outer: Some(Celsius(78.0 + 10.0 * (elapsed * 0.05 + offset).cos())),
            carcass_temp_inner: Some(Celsius(88.0 + 8.0 * (elapsed * 0.03 + offset).cos())),
            carcass_temp_middle: Some(Celsius(85.0 + 10.0 * (elapsed * 0.03 + offset).cos())),
            carcass_temp_outer: Some(Celsius(82.0 + 7.0 * (elapsed * 0.03 + offset).cos())),
            tyre_wear: Some(Percentage::new(0.1 + 0.02 * (elapsed * 0.01))),
            wheel_speed: Some(RadiansPerSecond(speed * 10.0)),
            slip_ratio: Some(0.05 * (elapsed * 2.0 + offset).sin()),
            slip_angle: Some(Radians(0.02 * (elapsed + offset).cos())),
            load: Some(Newtons(2500.0 + 500.0 * (elapsed * 0.5 + offset).sin())),
            brake_line_pressure: Some(Kilopascals(200.0 * ((elapsed * 0.3).sin() * 0.5 + 0.5).max(0.0) * 0.2)),
            brake_temp: Some(Celsius(350.0 + 50.0 * (elapsed * 0.1 + offset).sin())),
            tyre_compound: Some("Soft".to_string()),
        };

        // --- Motion ---
        let motion = Some(MotionData {
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
                Radians(0.0),
                Radians(elapsed * 0.1),
                Radians(0.05 * (elapsed * 0.5).sin()),
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
        });

        // --- Vehicle ---
        let vehicle = Some(VehicleData {
            speed: Some(MetersPerSecond(speed)),
            rpm: Some(Rpm(rpm)),
            max_rpm: Some(Rpm(8000.0)),
            idle_rpm: Some(Rpm(1200.0)),
            gear: Some(gear),
            max_gears: Some(6),
            throttle: Some(Percentage::new(0.6 + 0.3 * (elapsed * 0.8).sin())),
            brake: Some(Percentage::new(
                ((elapsed * 0.3).sin() * 0.5 + 0.5).max(0.0) * 0.2,
            )),
            clutch: Some(Percentage::new(0.0)),
            steering_angle: Some(Radians(0.3 * (elapsed * 0.7).sin())),
            steering_torque: Some(NewtonMeters(5.0 * (elapsed * 0.7).sin())),
            steering_torque_pct: Some(Percentage::new(0.3 * (elapsed * 0.7).sin().abs())),
            handbrake: None,
            on_track: Some(true),
            in_garage: Some(false),
            track_surface: Some(TrackSurface::Asphalt),
        });

        // --- Engine ---
        let engine = Some(EngineData {
            water_temp: Some(Celsius(90.0 + 5.0 * (elapsed * 0.01).min(1.0))),
            oil_temp: Some(Celsius(105.0 + 3.0 * (elapsed * 0.01).min(1.0))),
            oil_pressure: Some(Kilopascals(350.0 + 20.0 * (elapsed * 0.05).sin())),
            oil_level: Some(Percentage::new(0.95)),
            fuel_level: Some(Liters(60.0 * (1.0 - (elapsed * 0.001).min(0.5)))),
            fuel_level_pct: Some(Percentage::new(1.0 - (elapsed * 0.001).min(0.5))),
            fuel_capacity: Some(Liters(60.0)),
            fuel_pressure: Some(Kilopascals(400.0)),
            fuel_use_per_hour: Some(LitersPerHour(35.0)),
            voltage: Some(Volts(13.8)),
            manifold_pressure: Some(Bar(1.2 + 0.3 * (elapsed * 0.8).sin())),
            warnings: Some(EngineWarnings {
                water_temp_high: false,
                fuel_pressure_low: false,
                oil_pressure_low: false,
                engine_stalled: false,
                pit_speed_limiter: false,
                rev_limiter: rpm > 7500.0,
            }),
        });

        // --- Wheels ---
        let wheels = Some(WheelData {
            front_left: make_wheel(0.0),
            front_right: make_wheel(PI / 2.0),
            rear_left: make_wheel(PI),
            rear_right: make_wheel(3.0 * PI / 2.0),
        });

        // --- Timing ---
        let timing = Some(TimingData {
            current_lap_time: Some(Seconds(elapsed % 90.0)),
            last_lap_time: Some(Seconds(87.3)),
            best_lap_time: Some(Seconds(85.1)),
            best_n_lap_time: Some(Seconds(86.2)),
            best_n_lap_num: Some(3),
            sector_times: Some(vec![Seconds(28.4), Seconds(29.1), Seconds(27.6)]),
            lap_number: Some((elapsed / 90.0) as u32 + 1),
            laps_completed: Some((elapsed / 90.0) as u32),
            lap_distance: Some(Meters((elapsed % 90.0) / 90.0 * 4500.0)),
            lap_distance_pct: Some(Percentage::new((elapsed % 90.0) / 90.0)),
            race_position: Some(3),
            class_position: Some(2),
            num_cars: Some(20),
            delta_best: Some(Seconds(0.3 * (elapsed * 0.2).sin())),
            delta_best_ok: Some(true),
            delta_session_best: Some(Seconds(0.5 + 0.4 * (elapsed * 0.15).sin())),
            delta_session_best_ok: Some(true),
            delta_optimal: Some(Seconds(0.1 + 0.2 * (elapsed * 0.18).sin())),
            delta_optimal_ok: Some(true),
            estimated_lap_time: Some(Seconds(86.0)),
            race_laps: Some((elapsed / 85.0) as u32 + 1),
        });

        // --- Session ---
        let session = Some(SessionData {
            session_type: Some(SessionType::Race),
            session_state: Some(SessionState::Racing),
            session_time: Some(Seconds(elapsed)),
            session_time_remaining: Some(Seconds(1800.0 - elapsed)),
            session_time_of_day: Some(Seconds(43200.0 + elapsed)),
            session_laps: Some(30),
            session_laps_remaining: Some(30 - (elapsed / 85.0) as u32),
            flags: Some(FlagState { green: true, ..Default::default() }),
            track_name: Some("Demo Circuit".to_string()),
            track_config: Some("Grand Prix".to_string()),
            track_length: Some(Meters(4500.0)),
            track_type: Some("Road".to_string()),
            car_name: Some("Formula Demo".to_string()),
            car_class: Some("Open Wheel".to_string()),
        });

        // --- Weather ---
        let weather = Some(WeatherData {
            air_temp: Some(Celsius(22.0)),
            track_temp: Some(Celsius(28.0)),
            air_pressure: Some(Pascals(101325.0)),
            air_density: Some(KilogramsPerCubicMeter(1.225)),
            humidity: Some(Percentage::new(0.55)),
            wind_speed: Some(MetersPerSecond(3.5)),
            wind_direction: Some(Radians(1.2)),
            fog_level: Some(Percentage::new(0.0)),
            precipitation: Some(Percentage::new(0.0)),
            track_wetness: Some(TrackWetness::Dry),
            skies: Some("Clear".to_string()),
            declared_wet: Some(false),
        });

        // --- Pit ---
        let pit = Some(PitData {
            on_pit_road: Some(false),
            pit_active: Some(false),
            pit_service_status: Some(0),
            repair_time_left: Some(Seconds(0.0)),
            optional_repair_time_left: Some(Seconds(0.0)),
            fast_repair_available: Some(1),
            fast_repair_used: Some(0),
            pit_speed_limit: Some(MetersPerSecond(80.0 / 3.6)),
            requested_services: Some(PitServices {
                fuel_to_add: Some(Liters(40.0)),
                change_tyre_fl: true,
                change_tyre_fr: true,
                change_tyre_rl: true,
                change_tyre_rr: true,
                windshield_tearoff: false,
                fast_repair: false,
                tyre_pressure_fl: Some(Kilopascals(172.0)),
                tyre_pressure_fr: Some(Kilopascals(172.0)),
                tyre_pressure_rl: Some(Kilopascals(165.0)),
                tyre_pressure_rr: Some(Kilopascals(165.0)),
            }),
        });

        // --- Electronics ---
        let electronics = Some(ElectronicsData {
            abs: Some(2.0),
            traction_control: Some(3.0),
            traction_control_2: None,
            brake_bias: Some(Percentage::new(0.56)),
            anti_roll_front: None,
            anti_roll_rear: None,
            drs_status: None,
            push_to_pass_status: None,
            push_to_pass_count: None,
            throttle_shape: None,
        });

        // --- Damage ---
        let damage = Some(DamageData {
            front: Some(Percentage::new(0.0)),
            rear: Some(Percentage::new(0.0)),
            left: Some(Percentage::new(0.05)),
            right: Some(Percentage::new(0.0)),
            engine: Some(Percentage::new(0.0)),
            transmission: Some(Percentage::new(0.0)),
        });

        // --- Competitors ---
        let competitors = Some(vec![
            CompetitorData {
                car_index: 1,
                driver_name: Some("Demo Driver A".to_string()),
                car_name: Some("Formula Demo".to_string()),
                car_class: Some("Open Wheel".to_string()),
                team_name: Some("Team Alpha".to_string()),
                car_number: Some("7".to_string()),
                lap: Some((elapsed / 84.0) as u32 + 1),
                laps_completed: Some((elapsed / 84.0) as u32),
                lap_distance_pct: Some(Percentage::new(((elapsed + 10.0) % 84.0) / 84.0)),
                position: Some(1),
                class_position: Some(1),
                on_pit_road: Some(false),
                track_surface: Some(TrackSurface::Asphalt),
                best_lap_time: Some(Seconds(83.5)),
                last_lap_time: Some(Seconds(84.1)),
                estimated_time: Some(Seconds(84.0)),
                gear: Some(4),
                rpm: Some(Rpm(6200.0)),
                steering: Some(Radians(0.1)),
            },
            CompetitorData {
                car_index: 2,
                driver_name: Some("Demo Driver B".to_string()),
                car_name: Some("Formula Demo".to_string()),
                car_class: Some("Open Wheel".to_string()),
                team_name: Some("Team Beta".to_string()),
                car_number: Some("22".to_string()),
                lap: Some((elapsed / 86.0) as u32 + 1),
                laps_completed: Some((elapsed / 86.0) as u32),
                lap_distance_pct: Some(Percentage::new(((elapsed + 5.0) % 86.0) / 86.0)),
                position: Some(2),
                class_position: Some(2),
                on_pit_road: Some(false),
                track_surface: Some(TrackSurface::Asphalt),
                best_lap_time: Some(Seconds(84.8)),
                last_lap_time: Some(Seconds(85.5)),
                estimated_time: Some(Seconds(85.0)),
                gear: Some(5),
                rpm: Some(Rpm(5800.0)),
                steering: Some(Radians(-0.05)),
            },
        ]);

        // --- Driver ---
        let driver = Some(DriverData {
            name: Some("Demo Player".to_string()),
            car_index: Some(0),
            car_name: Some("Formula Demo".to_string()),
            car_class: Some("Open Wheel".to_string()),
            car_number: Some("42".to_string()),
            team_name: Some("Team Demo".to_string()),
            fuel_capacity: Some(Liters(60.0)),
            shift_light_first_rpm: Some(Rpm(6500.0)),
            shift_light_shift_rpm: Some(Rpm(7500.0)),
            shift_light_last_rpm: Some(Rpm(7800.0)),
            shift_light_blink_rpm: Some(Rpm(7900.0)),
            estimated_lap_time: Some(Seconds(86.0)),
            setup_name: Some("baseline".to_string()),
        });

        // --- Extras ---
        let mut extras = HashMap::new();
        extras.insert(
            "demo/frame_count".to_string(),
            serde_json::json!(self.frame_count),
        );

        TelemetryFrame {
            timestamp: Utc::now(),
            game: "Demo".to_string(),
            tick: Some(self.frame_count as u32),
            motion,
            vehicle,
            engine,
            wheels,
            timing,
            session,
            weather,
            pit,
            electronics,
            damage,
            competitors,
            driver,
            extras,
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

        Ok(Some(self.generate_frame()))
    }

    fn is_active(&self) -> bool {
        self.active
    }
}
