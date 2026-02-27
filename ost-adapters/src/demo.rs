//! Demo adapter that generates synthetic telemetry for testing
//!
//! Simulates laps around a circuit with straights, braking zones, corners,
//! and acceleration phases. Produces realistic-looking telemetry at 60Hz
//! without requiring an actual game.

use anyhow::Result;
use chrono::Utc;
use ost_core::{adapter::TelemetryAdapter, model::*, units::*};
use std::collections::HashMap;
use std::time::Instant;

// =============================================================================
// Track definition — a sequence of segments that form a lap
// =============================================================================

#[derive(Clone, Copy)]
enum SegmentKind {
    Straight,   // Full throttle, top speed
    Braking,    // Heavy braking into a corner
    Corner,     // Constant-ish speed cornering
    Accel,      // Accelerating out of a corner
}

#[derive(Clone, Copy)]
struct TrackSegment {
    kind: SegmentKind,
    duration: f32,       // seconds to traverse at representative pace
    target_speed: f32,   // m/s at end of segment
    steering: f32,       // peak steering angle in radians (signed: + = right)
    lateral_g: f32,      // peak lateral G
}

/// A simple circuit: ~85s lap, mix of corners and straights
fn demo_track() -> Vec<TrackSegment> {
    vec![
        // Start/finish straight
        TrackSegment { kind: SegmentKind::Straight, duration: 8.0,  target_speed: 75.0, steering: 0.0,   lateral_g: 0.0 },
        // T1: heavy braking into slow right-hander
        TrackSegment { kind: SegmentKind::Braking,  duration: 3.0,  target_speed: 28.0, steering: 0.02,  lateral_g: 0.1 },
        TrackSegment { kind: SegmentKind::Corner,   duration: 4.0,  target_speed: 25.0, steering: 0.35,  lateral_g: 1.8 },
        TrackSegment { kind: SegmentKind::Accel,    duration: 3.5,  target_speed: 55.0, steering: 0.1,   lateral_g: 0.4 },
        // Short straight
        TrackSegment { kind: SegmentKind::Straight, duration: 4.0,  target_speed: 62.0, steering: 0.0,   lateral_g: 0.0 },
        // T2: medium braking into fast left-hander
        TrackSegment { kind: SegmentKind::Braking,  duration: 2.0,  target_speed: 45.0, steering: -0.02, lateral_g: -0.1 },
        TrackSegment { kind: SegmentKind::Corner,   duration: 3.5,  target_speed: 42.0, steering: -0.22, lateral_g: -1.5 },
        TrackSegment { kind: SegmentKind::Accel,    duration: 3.0,  target_speed: 58.0, steering: -0.05, lateral_g: -0.3 },
        // Back straight
        TrackSegment { kind: SegmentKind::Straight, duration: 10.0, target_speed: 80.0, steering: 0.0,   lateral_g: 0.0 },
        // T3: chicane — quick right-left
        TrackSegment { kind: SegmentKind::Braking,  duration: 2.5,  target_speed: 35.0, steering: 0.05,  lateral_g: 0.2 },
        TrackSegment { kind: SegmentKind::Corner,   duration: 2.0,  target_speed: 32.0, steering: 0.30,  lateral_g: 1.6 },
        TrackSegment { kind: SegmentKind::Corner,   duration: 2.0,  target_speed: 30.0, steering: -0.32, lateral_g: -1.7 },
        TrackSegment { kind: SegmentKind::Accel,    duration: 3.0,  target_speed: 50.0, steering: -0.05, lateral_g: -0.2 },
        // Medium straight
        TrackSegment { kind: SegmentKind::Straight, duration: 6.0,  target_speed: 68.0, steering: 0.0,   lateral_g: 0.0 },
        // T4: long sweeping right
        TrackSegment { kind: SegmentKind::Braking,  duration: 1.5,  target_speed: 52.0, steering: 0.03,  lateral_g: 0.1 },
        TrackSegment { kind: SegmentKind::Corner,   duration: 5.0,  target_speed: 50.0, steering: 0.18,  lateral_g: 1.3 },
        TrackSegment { kind: SegmentKind::Accel,    duration: 3.0,  target_speed: 60.0, steering: 0.05,  lateral_g: 0.3 },
        // T5: tight hairpin left
        TrackSegment { kind: SegmentKind::Braking,  duration: 3.5,  target_speed: 22.0, steering: -0.03, lateral_g: -0.1 },
        TrackSegment { kind: SegmentKind::Corner,   duration: 4.5,  target_speed: 20.0, steering: -0.42, lateral_g: -1.2 },
        TrackSegment { kind: SegmentKind::Accel,    duration: 4.0,  target_speed: 55.0, steering: -0.1,  lateral_g: -0.3 },
        // Run to start/finish
        TrackSegment { kind: SegmentKind::Straight, duration: 6.0,  target_speed: 72.0, steering: 0.0,   lateral_g: 0.0 },
    ]
}

// =============================================================================
// Interpolation state — derived from track position
// =============================================================================

struct LapState {
    seg_idx: usize,
    speed: f32,
    throttle: f32,
    brake: f32,
    steering: f32,
    lateral_g: f32,
    longitudinal_g: f32,
    gear: i8,
    rpm: f32,
}

fn compute_lap_state(track: &[TrackSegment], lap_time: f32) -> LapState {
    let lap_duration: f32 = track.iter().map(|s| s.duration).sum();
    let t = lap_time % lap_duration;

    // Find current segment
    let mut elapsed = 0.0_f32;
    let mut seg_idx = 0;
    for (i, seg) in track.iter().enumerate() {
        if elapsed + seg.duration > t {
            seg_idx = i;
            break;
        }
        elapsed += seg.duration;
        if i == track.len() - 1 {
            seg_idx = i;
        }
    }

    let seg = track[seg_idx];
    let seg_t = ((t - elapsed) / seg.duration).clamp(0.0, 1.0);

    // Previous segment's target speed (for interpolation start)
    let prev_target_speed = if seg_idx > 0 {
        track[seg_idx - 1].target_speed
    } else {
        track.last().unwrap().target_speed
    };

    // Smooth interpolation of speed through the segment
    let smooth_t = smoothstep(seg_t);
    let speed = lerp(prev_target_speed, seg.target_speed, smooth_t);

    // Inputs based on segment kind
    let (throttle, brake) = match seg.kind {
        SegmentKind::Straight => (0.95 + 0.05 * (1.0 - seg_t), 0.0), // slight lift approaching end
        SegmentKind::Braking => {
            let brake_force = 1.0 - smooth_t * 0.3; // starts heavy, eases off
            (0.0, brake_force.clamp(0.0, 1.0))
        }
        SegmentKind::Corner => {
            // Maintenance throttle through the corner, more toward exit
            let thr = 0.2 + 0.3 * seg_t;
            (thr, 0.0)
        }
        SegmentKind::Accel => {
            // Progressive throttle application
            let thr = 0.5 + 0.5 * smooth_t;
            (thr, 0.0)
        }
    };

    // Steering: ramp in during first half, ramp out during second half
    let steer_envelope = if seg_t < 0.5 {
        smoothstep(seg_t * 2.0)
    } else {
        smoothstep((1.0 - seg_t) * 2.0)
    };
    let steering = seg.steering * steer_envelope;

    // Lateral G follows steering with slight lag (approximated)
    let lateral_g = seg.lateral_g * steer_envelope;

    // Longitudinal G from speed change
    let speed_rate = (seg.target_speed - prev_target_speed) / seg.duration;
    let longitudinal_g = speed_rate / 9.81;

    // Gear from speed
    let gear = speed_to_gear(speed);

    // RPM from speed and gear
    let rpm = speed_to_rpm(speed, gear);

    LapState {
        seg_idx,
        speed,
        throttle,
        brake,
        steering,
        lateral_g,
        longitudinal_g,
        gear,
        rpm,
    }
}

fn smoothstep(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

fn speed_to_gear(speed_ms: f32) -> i8 {
    let kph = speed_ms * 3.6;
    match kph {
        x if x < 40.0 => 1,
        x if x < 80.0 => 2,
        x if x < 120.0 => 3,
        x if x < 170.0 => 4,
        x if x < 230.0 => 5,
        _ => 6,
    }
}

fn speed_to_rpm(speed_ms: f32, gear: i8) -> f32 {
    // Approximate RPM curve per gear: lower gear = higher RPM for same speed
    let base_ratio = match gear {
        1 => 130.0,
        2 => 85.0,
        3 => 60.0,
        4 => 45.0,
        5 => 36.0,
        _ => 30.0,
    };
    let rpm = speed_ms * base_ratio + 1200.0; // idle offset
    rpm.clamp(1200.0, 8000.0)
}

/// Simple deterministic noise from a seed
fn noise(seed: f32) -> f32 {
    let x = (seed * 12.9898 + 78.233).sin() * 43_758.547;
    x - x.floor()
}

/// Small jitter centered around 0
fn jitter(seed: f32, amplitude: f32) -> f32 {
    (noise(seed) - 0.5) * 2.0 * amplitude
}

// =============================================================================
// DemoAdapter
// =============================================================================

pub struct DemoAdapter {
    active: bool,
    start_time: Option<Instant>,
    frame_count: u64,
    track: Vec<TrackSegment>,
    lap_duration: f32,
    laps_completed: u32,
    best_lap: f32,
    last_lap: f32,
}

impl DemoAdapter {
    pub fn new() -> Self {
        let track = demo_track();
        let lap_duration: f32 = track.iter().map(|s| s.duration).sum();
        Self {
            active: false,
            start_time: None,
            frame_count: 0,
            track,
            lap_duration,
            laps_completed: 0,
            best_lap: 85.1,
            last_lap: 87.3,
        }
    }

    fn generate_frame(&mut self) -> TelemetryFrame {
        let elapsed = self
            .start_time
            .map(|t| t.elapsed().as_secs_f32())
            .unwrap_or(0.0);

        self.frame_count += 1;
        let t = elapsed; // shorthand
        let n = self.frame_count as f32; // noise seed

        // Track position
        let lap_time = t % self.lap_duration;
        let current_lap_num = (t / self.lap_duration) as u32 + 1;
        if current_lap_num > self.laps_completed + 1 {
            self.laps_completed = current_lap_num - 1;
            // Vary lap times slightly
            self.last_lap = self.lap_duration + jitter(n, 1.5);
            if self.last_lap < self.best_lap {
                self.best_lap = self.last_lap;
            }
        }

        let state = compute_lap_state(&self.track, lap_time);

        // Add small noise to all values for realism
        let speed = (state.speed + jitter(n, 0.3)).max(0.0);
        let rpm = (state.rpm + jitter(n * 1.1, 30.0)).clamp(1200.0, 8000.0);
        let throttle = (state.throttle + jitter(n * 1.2, 0.02)).clamp(0.0, 1.0);
        let brake = (state.brake + jitter(n * 1.3, 0.02)).clamp(0.0, 1.0);
        let steering = state.steering + jitter(n * 1.4, 0.005);
        let lat_g = state.lateral_g + jitter(n * 1.5, 0.05);
        let long_g = state.longitudinal_g + jitter(n * 1.6, 0.03);

        // Roll and pitch from G-forces
        let roll = lat_g * 0.015; // slight body roll
        let pitch = -long_g * 0.02; // nose dip under braking

        // Suspension: base travel + load transfer from G-forces
        let base_travel = 0.05;
        let make_wheel = |is_left: bool, is_front: bool| {
            let lat_load = if is_left { -lat_g } else { lat_g } * 0.008;
            let long_load = if is_front { long_g } else { -long_g } * 0.006;
            let travel = (base_travel + lat_load + long_load + jitter(n + if is_left { 0.0 } else { 1.0 } + if is_front { 0.0 } else { 2.0 }, 0.001)).max(0.01);
            let heat_base = if is_front { 85.0 } else { 78.0 };
            let heat_offset = speed * 0.15 + lat_g.abs() * 3.0;

            WheelInfo {
                suspension_travel: Some(Meters(travel)),
                suspension_travel_avg: Some(Meters(base_travel)),
                shock_velocity: Some(MetersPerSecond(jitter(n + travel, 0.02))),
                shock_velocity_avg: Some(MetersPerSecond(0.005)),
                ride_height: Some(Meters(0.06 - travel * 0.2)),
                tyre_pressure: Some(Kilopascals(178.0 + heat_offset * 0.3 + jitter(n, 0.5))),
                tyre_cold_pressure: Some(Kilopascals(172.0)),
                surface_temp_inner: Some(Celsius(heat_base + heat_offset + 5.0 + jitter(n * 2.1, 0.5))),
                surface_temp_middle: Some(Celsius(heat_base + heat_offset + jitter(n * 2.2, 0.5))),
                surface_temp_outer: Some(Celsius(heat_base + heat_offset - 3.0 + jitter(n * 2.3, 0.5))),
                carcass_temp_inner: Some(Celsius(heat_base + heat_offset * 0.7 + 8.0)),
                carcass_temp_middle: Some(Celsius(heat_base + heat_offset * 0.7 + 5.0)),
                carcass_temp_outer: Some(Celsius(heat_base + heat_offset * 0.7 + 2.0)),
                tyre_wear: Some(Percentage::new((0.02 + elapsed * 0.0001).min(0.3))),
                wheel_speed: Some(RadiansPerSecond(speed / 0.33 + jitter(n, 0.5))), // ~0.33m tyre radius
                slip_ratio: Some(jitter(n * 3.0, 0.03 + brake * 0.05)),
                slip_angle: Some(Radians(steering.abs() * 0.1 + jitter(n * 3.1, 0.005))),
                load: Some(Newtons(2500.0 + (lat_load + long_load) * 800.0 + jitter(n * 3.2, 50.0))),
                brake_line_pressure: Some(Kilopascals(brake * 3500.0 + jitter(n * 3.3, 10.0))),
                brake_temp: Some(Celsius(200.0 + brake * 300.0 + speed * 1.5 + jitter(n * 3.4, 5.0))),
                tyre_compound: Some("Soft".to_string()),
            }
        };

        // --- Motion ---
        let motion = Some(MotionData {
            position: Some(Vector3::new(
                Meters(elapsed * 10.0),
                Meters(0.5),
                Meters(elapsed * 5.0),
            )),
            velocity: Some(Vector3::new(
                MetersPerSecond(lat_g * 2.0),
                MetersPerSecond(0.0),
                MetersPerSecond(speed),
            )),
            acceleration: Some(Vector3::new(
                MetersPerSecondSquared(lat_g * 9.81),
                MetersPerSecondSquared(-9.81),
                MetersPerSecondSquared(long_g * 9.81),
            )),
            g_force: Some(Vector3::new(
                GForce(lat_g),
                GForce(-1.0 + jitter(n * 4.0, 0.02)),
                GForce(long_g),
            )),
            rotation: Some(Vector3::new(
                Radians(pitch),
                Radians(0.0), // yaw is absolute, not useful for demo
                Radians(roll),
            )),
            angular_velocity: Some(Vector3::new(
                RadiansPerSecond(jitter(n * 4.1, 0.01)),
                RadiansPerSecond(steering * speed * 0.02), // yaw rate from steering + speed
                RadiansPerSecond(jitter(n * 4.2, 0.01)),
            )),
            angular_acceleration: Some(Vector3::new(
                RadiansPerSecondSquared(0.0),
                RadiansPerSecondSquared(0.0),
                RadiansPerSecondSquared(0.0),
            )),
        });

        // --- Vehicle ---
        let vehicle = Some(VehicleData {
            speed: Some(MetersPerSecond(speed)),
            rpm: Some(Rpm(rpm)),
            max_rpm: Some(Rpm(8000.0)),
            idle_rpm: Some(Rpm(1200.0)),
            gear: Some(state.gear),
            max_gears: Some(6),
            throttle: Some(Percentage::new(throttle)),
            brake: Some(Percentage::new(brake)),
            clutch: Some(Percentage::new(0.0)),
            steering_angle: Some(Radians(steering)),
            steering_torque: Some(NewtonMeters(steering * 15.0 + lat_g * 3.0)),
            steering_torque_pct: Some(Percentage::new((steering.abs() * 2.0).min(1.0))),
            handbrake: None,
            on_track: Some(true),
            in_garage: Some(false),
            track_surface: Some(TrackSurface::Asphalt),
        });

        // --- Engine ---
        let fuel_remaining = (60.0 * (1.0 - elapsed * 0.00015)).max(0.0);
        let engine = Some(EngineData {
            water_temp: Some(Celsius(88.0 + rpm * 0.0005 + speed * 0.02 + jitter(n * 5.0, 0.3))),
            oil_temp: Some(Celsius(102.0 + rpm * 0.0004 + jitter(n * 5.1, 0.2))),
            oil_pressure: Some(Kilopascals(320.0 + rpm * 0.005 + jitter(n * 5.2, 3.0))),
            oil_level: Some(Percentage::new(0.95)),
            fuel_level: Some(Liters(fuel_remaining)),
            fuel_level_pct: Some(Percentage::new(fuel_remaining / 60.0)),
            fuel_capacity: Some(Liters(60.0)),
            fuel_pressure: Some(Kilopascals(400.0 + jitter(n * 5.3, 2.0))),
            fuel_use_per_hour: Some(LitersPerHour(30.0 + throttle * 15.0)),
            voltage: Some(Volts(13.8 + jitter(n * 5.4, 0.1))),
            manifold_pressure: Some(Bar(0.8 + throttle * 0.6)),
            warnings: Some(EngineWarnings {
                water_temp_high: false,
                fuel_pressure_low: false,
                oil_pressure_low: false,
                engine_stalled: false,
                pit_speed_limiter: false,
                rev_limiter: rpm > 7800.0,
            }),
        });

        // --- Wheels ---
        let wheels = Some(WheelData {
            front_left: make_wheel(true, true),
            front_right: make_wheel(false, true),
            rear_left: make_wheel(true, false),
            rear_right: make_wheel(false, false),
        });

        // --- Timing ---
        let lap_dist_pct = lap_time / self.lap_duration;
        let timing = Some(TimingData {
            current_lap_time: Some(Seconds(lap_time)),
            last_lap_time: Some(Seconds(self.last_lap)),
            best_lap_time: Some(Seconds(self.best_lap)),
            best_n_lap_time: Some(Seconds(self.best_lap + 1.1)),
            best_n_lap_num: Some(3),
            sector_times: Some(vec![Seconds(28.4), Seconds(29.1), Seconds(27.6)]),
            lap_number: Some(current_lap_num),
            laps_completed: Some(self.laps_completed),
            lap_distance: Some(Meters(lap_dist_pct * 4500.0)),
            lap_distance_pct: Some(Percentage::new(lap_dist_pct)),
            race_position: Some(3),
            class_position: Some(2),
            num_cars: Some(20),
            delta_best: Some(Seconds(jitter(n * 6.0, 0.8))),
            delta_best_ok: Some(true),
            delta_session_best: Some(Seconds(0.5 + jitter(n * 6.1, 0.6))),
            delta_session_best_ok: Some(true),
            delta_optimal: Some(Seconds(0.2 + jitter(n * 6.2, 0.4))),
            delta_optimal_ok: Some(true),
            estimated_lap_time: Some(Seconds(self.lap_duration)),
            race_laps: Some(current_lap_num),
        });

        // --- Session ---
        let session = Some(SessionData {
            session_type: Some(SessionType::Race),
            session_state: Some(SessionState::Racing),
            session_time: Some(Seconds(elapsed)),
            session_time_remaining: Some(Seconds((1800.0 - elapsed).max(0.0))),
            session_time_of_day: Some(Seconds(43200.0 + elapsed)),
            session_laps: Some(30),
            session_laps_remaining: Some(30u32.saturating_sub(self.laps_completed)),
            flags: Some(FlagState {
                green: true,
                ..Default::default()
            }),
            track_name: Some("Demo Circuit".to_string()),
            track_config: Some("Grand Prix".to_string()),
            track_length: Some(Meters(4500.0)),
            track_type: Some("Road".to_string()),
            car_name: Some("Formula Demo".to_string()),
            car_class: Some("Open Wheel".to_string()),
        });

        // --- Weather ---
        let weather = Some(WeatherData {
            air_temp: Some(Celsius(22.0 + jitter(n * 7.0, 0.1))),
            track_temp: Some(Celsius(28.0 + jitter(n * 7.1, 0.2))),
            air_pressure: Some(Pascals(101325.0)),
            air_density: Some(KilogramsPerCubicMeter(1.225)),
            humidity: Some(Percentage::new(0.55)),
            wind_speed: Some(MetersPerSecond(3.5 + jitter(n * 7.2, 0.3))),
            wind_direction: Some(Radians(1.2 + jitter(n * 7.3, 0.05))),
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
            left: Some(Percentage::new(0.0)),
            right: Some(Percentage::new(0.0)),
            engine: Some(Percentage::new(0.0)),
            transmission: Some(Percentage::new(0.0)),
        });

        // --- Competitors (simulate two other cars on track) ---
        let comp_a_pct = ((elapsed + 10.0) % (self.lap_duration - 1.5)) / (self.lap_duration - 1.5);
        let comp_b_pct = ((elapsed + 25.0) % (self.lap_duration + 1.0)) / (self.lap_duration + 1.0);
        let competitors = Some(vec![
            CompetitorData {
                car_index: 1,
                driver_name: Some("Alex Rivera".to_string()),
                car_name: Some("Formula Demo".to_string()),
                car_class: Some("Open Wheel".to_string()),
                team_name: Some("Apex Racing".to_string()),
                car_number: Some("7".to_string()),
                lap: Some((elapsed / (self.lap_duration - 1.5)) as u32 + 1),
                laps_completed: Some((elapsed / (self.lap_duration - 1.5)) as u32),
                lap_distance_pct: Some(Percentage::new(comp_a_pct)),
                position: Some(1),
                class_position: Some(1),
                on_pit_road: Some(false),
                track_surface: Some(TrackSurface::Asphalt),
                best_lap_time: Some(Seconds(self.best_lap - 0.8)),
                last_lap_time: Some(Seconds(self.lap_duration - 1.2)),
                estimated_time: Some(Seconds(self.lap_duration - 1.0)),
                gear: Some(4),
                rpm: Some(Rpm(6200.0)),
                steering: Some(Radians(0.05)),
            },
            CompetitorData {
                car_index: 2,
                driver_name: Some("Sam Chen".to_string()),
                car_name: Some("Formula Demo".to_string()),
                car_class: Some("Open Wheel".to_string()),
                team_name: Some("Velocity Motorsport".to_string()),
                car_number: Some("22".to_string()),
                lap: Some((elapsed / (self.lap_duration + 1.0)) as u32 + 1),
                laps_completed: Some((elapsed / (self.lap_duration + 1.0)) as u32),
                lap_distance_pct: Some(Percentage::new(comp_b_pct)),
                position: Some(2),
                class_position: Some(2),
                on_pit_road: Some(false),
                track_surface: Some(TrackSurface::Asphalt),
                best_lap_time: Some(Seconds(self.best_lap + 0.3)),
                last_lap_time: Some(Seconds(self.lap_duration + 0.8)),
                estimated_time: Some(Seconds(self.lap_duration + 0.5)),
                gear: Some(5),
                rpm: Some(Rpm(5800.0)),
                steering: Some(Radians(-0.03)),
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
            estimated_lap_time: Some(Seconds(self.lap_duration)),
            setup_name: Some("baseline".to_string()),
        });

        // --- Extras ---
        let mut extras = HashMap::new();
        extras.insert(
            "demo/frame_count".to_string(),
            serde_json::json!(self.frame_count),
        );
        extras.insert(
            "demo/segment_index".to_string(),
            serde_json::json!(state.seg_idx),
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
    fn key(&self) -> &str {
        "demo"
    }

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
        self.laps_completed = 0;
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
