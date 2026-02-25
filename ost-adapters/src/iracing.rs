//! iRacing adapter using the iracing.rs library
//!
//! This adapter connects to iRacing via shared memory and reads telemetry data.
//! Only available on Windows.

#[cfg(target_os = "windows")]
mod windows_impl {
    use anyhow::Result;
    use chrono::Utc;
    use iracing::telemetry::{Connection, Sample as IRacingSample};
    use ost_core::{adapter::TelemetryAdapter, model::*, units::*};
    use std::collections::HashMap;
    use std::convert::TryInto;
    use std::time::Duration;

    pub struct IRacingAdapter {
        connection: Option<Connection>,
        blocking: Option<iracing::telemetry::Blocking>,
        active: bool,
    }

    // SAFETY: iRacing's shared memory is thread-safe for reading.
    // The Connection and Blocking types contain raw pointers to memory-mapped files,
    // which are safe to access from multiple threads (Windows handles the synchronization).
    // We never mutate the shared state, only read from it.
    unsafe impl Send for IRacingAdapter {}
    unsafe impl Sync for IRacingAdapter {}

    impl IRacingAdapter {
        pub fn new() -> Self {
            Self {
                connection: None,
                blocking: None,
                active: false,
            }
        }

        /// Convert iRacing telemetry sample to unified TelemetryFrame
        fn convert_sample(&self, sample: &IRacingSample) -> TelemetryFrame {
            // Helper to safely get values
            let get_f32 = |name: &'static str| -> Option<f32> {
                sample.get(name).ok().and_then(|v| v.try_into().ok())
            };

            let get_f64 = |name: &'static str| -> Option<f64> {
                sample.get(name).ok().and_then(|v| v.try_into().ok())
            };

            let get_i32 = |name: &'static str| -> Option<i32> {
                sample.get(name).ok().and_then(|v| v.try_into().ok())
            };

            let _get_bool = |name: &'static str| -> Option<bool> {
                sample.get(name).ok().and_then(|v| v.try_into().ok())
            };

            // Motion data - iRacing uses right-handed coordinate system
            // X = right, Y = up, Z = forward (same as our model)
            let velocity = if let (Some(vx), Some(vy), Some(vz)) = (
                get_f32("VelocityX"),
                get_f32("VelocityY"),
                get_f32("VelocityZ"),
            ) {
                Some(Vector3::new(
                    MetersPerSecond(vx),
                    MetersPerSecond(vy),
                    MetersPerSecond(vz),
                ))
            } else {
                None
            };

            // Calculate speed from velocity if available
            let speed = velocity
                .as_ref()
                .map(|v| MetersPerSecond((v.x.0.powi(2) + v.y.0.powi(2) + v.z.0.powi(2)).sqrt()));

            // Acceleration (lateral, longitudinal, vertical)
            let acceleration = if let (Some(lat), Some(long), Some(vert)) = (
                get_f32("LatAccel"),
                get_f32("LongAccel"),
                get_f32("VertAccel"),
            ) {
                Some(Vector3::new(
                    MetersPerSecondSquared(lat),
                    MetersPerSecondSquared(vert),
                    MetersPerSecondSquared(long),
                ))
            } else {
                None
            };

            // G-forces (derived from acceleration)
            let g_force = acceleration.as_ref().map(|a| {
                Vector3::new(
                    GForce::from_acceleration(a.x),
                    GForce::from_acceleration(a.y),
                    GForce::from_acceleration(a.z),
                )
            });

            // Rotation (pitch, yaw, roll)
            let rotation = if let (Some(pitch), Some(yaw), Some(roll)) =
                (get_f32("Pitch"), get_f32("Yaw"), get_f32("Roll"))
            {
                Some(Vector3::new(Radians(pitch), Radians(yaw), Radians(roll)))
            } else {
                None
            };

            // Angular velocity
            let angular_velocity = if let (Some(pitch_rate), Some(yaw_rate), Some(roll_rate)) = (
                get_f32("PitchRate"),
                get_f32("YawRate"),
                get_f32("RollRate"),
            ) {
                Some(Vector3::new(
                    RadiansPerSecond(pitch_rate),
                    RadiansPerSecond(yaw_rate),
                    RadiansPerSecond(roll_rate),
                ))
            } else {
                None
            };

            // Vehicle state
            let rpm = get_f32("RPM").map(Rpm);
            let gear = get_i32("Gear").map(|g| g as i8);
            let throttle = get_f32("Throttle").map(Percentage::new);
            let brake = get_f32("Brake").map(Percentage::new);
            let clutch = get_f32("Clutch").map(Percentage::new);
            let steering = get_f32("SteeringWheelAngle");

            // Engine and fuel
            let engine_temp = get_f32("WaterTemp").map(Celsius);
            let fuel_level = get_f32("FuelLevel");
            let fuel_capacity =
                get_f32("FuelLevelPct").and_then(|pct| fuel_level.map(|level| level / pct));

            // Wheel data - iRacing provides per-wheel data
            let wheels = Self::extract_wheel_data(sample);

            // Lap timing
            let current_lap_time = get_f64("LapCurrentLapTime").map(|t| Seconds(t as f32));
            let last_lap_time = get_f64("LapLastLapTime").map(|t| Seconds(t as f32));
            let best_lap_time = get_f64("LapBestLapTime").map(|t| Seconds(t as f32));
            let lap_number = get_i32("Lap").map(|l| l as u32);

            // Session info
            let session_time_remaining = get_f64("SessionTimeRemain").map(|t| Seconds(t as f32));
            let track_temp = get_f32("TrackTempCrew").map(Celsius);
            let air_temp = get_f32("AirTemp").map(Celsius);

            // Race position
            let race_position = get_i32("PlayerCarPosition").map(|p| p as u32);
            let num_cars = get_i32("SessionNum").map(|n| n as u32);

            // Flags - simplified for now
            let flag = Self::extract_flag_status(sample);

            TelemetryFrame {
                timestamp: Utc::now(),
                game: "iRacing".to_string(),

                // Motion
                position: None, // iRacing doesn't expose world position directly
                velocity,
                acceleration,
                g_force,
                rotation,
                angular_velocity,
                angular_acceleration: None, // Not directly available

                // Vehicle state
                speed,
                rpm,
                gear,
                max_gears: None, // Would need to read from session info
                throttle,
                brake,
                clutch,
                steering,
                engine_temp,
                fuel_level: fuel_level.map(|l| Percentage::new(l / fuel_capacity.unwrap_or(1.0))),
                fuel_capacity: fuel_capacity.map(|c| c as f32),

                // Wheels
                wheels,

                // Lap timing
                current_lap_time,
                last_lap_time,
                best_lap_time,
                sector_times: None, // Could be extracted from session info
                lap_number,
                race_position,
                num_cars,

                // Session
                session_type: None, // Could be determined from SessionState
                session_time_remaining,
                track_temp,
                air_temp,
                track_name: None, // Would need session info
                car_name: None,   // Would need session info
                flag,

                // Damage
                damage: None, // iRacing has LF/RF/LR/RR wear, could map to damage

                // Extras
                extras: HashMap::new(),
            }
        }

        /// Extract per-wheel telemetry data
        fn extract_wheel_data(sample: &IRacingSample) -> Option<WheelData> {
            // Helper to get f32 from sample (takes static str as required by iracing.rs API)
            let get_f32 = |name: &'static str| -> Option<f32> {
                sample.get(name).ok().and_then(|v| v.try_into().ok())
            };

            // Extract each wheel's data directly using static field names
            let front_left = WheelInfo {
                suspension_travel: get_f32("LFshockDefl").map(Meters),
                tyre_pressure: get_f32("LFairPressure").map(Kilopascals),
                tyre_temp_surface: get_f32("LFtempCL")
                    .or_else(|| {
                        let l = get_f32("LFtempCL");
                        let c = get_f32("LFtempCC");
                        let r = get_f32("LFtempCR");
                        match (l, c, r) {
                            (Some(l), Some(c), Some(r)) => Some((l + c + r) / 3.0),
                            _ => None,
                        }
                    })
                    .map(Celsius),
                tyre_temp_inner: get_f32("LFtempCC").map(Celsius),
                tyre_wear: get_f32("LFwear").map(Percentage::new),
                slip_ratio: None,
                slip_angle: None,
                load: None,
                rotation_speed: get_f32("LFspeed").map(RadiansPerSecond),
            };

            let front_right = WheelInfo {
                suspension_travel: get_f32("RFshockDefl").map(Meters),
                tyre_pressure: get_f32("RFairPressure").map(Kilopascals),
                tyre_temp_surface: get_f32("RFtempCL")
                    .or_else(|| {
                        let l = get_f32("RFtempCL");
                        let c = get_f32("RFtempCC");
                        let r = get_f32("RFtempCR");
                        match (l, c, r) {
                            (Some(l), Some(c), Some(r)) => Some((l + c + r) / 3.0),
                            _ => None,
                        }
                    })
                    .map(Celsius),
                tyre_temp_inner: get_f32("RFtempCC").map(Celsius),
                tyre_wear: get_f32("RFwear").map(Percentage::new),
                slip_ratio: None,
                slip_angle: None,
                load: None,
                rotation_speed: get_f32("RFspeed").map(RadiansPerSecond),
            };

            let rear_left = WheelInfo {
                suspension_travel: get_f32("LRshockDefl").map(Meters),
                tyre_pressure: get_f32("LRairPressure").map(Kilopascals),
                tyre_temp_surface: get_f32("LRtempCL")
                    .or_else(|| {
                        let l = get_f32("LRtempCL");
                        let c = get_f32("LRtempCC");
                        let r = get_f32("LRtempCR");
                        match (l, c, r) {
                            (Some(l), Some(c), Some(r)) => Some((l + c + r) / 3.0),
                            _ => None,
                        }
                    })
                    .map(Celsius),
                tyre_temp_inner: get_f32("LRtempCC").map(Celsius),
                tyre_wear: get_f32("LRwear").map(Percentage::new),
                slip_ratio: None,
                slip_angle: None,
                load: None,
                rotation_speed: get_f32("LRspeed").map(RadiansPerSecond),
            };

            let rear_right = WheelInfo {
                suspension_travel: get_f32("RRshockDefl").map(Meters),
                tyre_pressure: get_f32("RRairPressure").map(Kilopascals),
                tyre_temp_surface: get_f32("RRtempCL")
                    .or_else(|| {
                        let l = get_f32("RRtempCL");
                        let c = get_f32("RRtempCC");
                        let r = get_f32("RRtempCR");
                        match (l, c, r) {
                            (Some(l), Some(c), Some(r)) => Some((l + c + r) / 3.0),
                            _ => None,
                        }
                    })
                    .map(Celsius),
                tyre_temp_inner: get_f32("RRtempCC").map(Celsius),
                tyre_wear: get_f32("RRwear").map(Percentage::new),
                slip_ratio: None,
                slip_angle: None,
                load: None,
                rotation_speed: get_f32("RRspeed").map(RadiansPerSecond),
            };

            Some(WheelData {
                front_left,
                front_right,
                rear_left,
                rear_right,
            })
        }

        /// Extract flag status from session state
        fn extract_flag_status(sample: &IRacingSample) -> Option<FlagType> {
            // SessionFlags is a bitfield, but for simplicity we'll check SessionState
            let state: Option<i32> = sample
                .get("SessionState")
                .ok()
                .and_then(|v| v.try_into().ok());

            match state {
                Some(4) => Some(FlagType::Green),     // Racing
                Some(5) => Some(FlagType::Checkered), // Checkered
                Some(6) => Some(FlagType::Yellow),    // CoolDown/Yellow
                _ => Some(FlagType::None),
            }
        }
    }

    impl TelemetryAdapter for IRacingAdapter {
        fn name(&self) -> &str {
            "iRacing"
        }

        fn detect(&self) -> bool {
            // Try to open the connection - if it succeeds, iRacing is running
            Connection::new().is_ok()
        }

        fn start(&mut self) -> Result<()> {
            let connection = Connection::new()?;
            let blocking = connection.blocking()?;

            self.connection = Some(connection);
            self.blocking = Some(blocking);
            self.active = true;

            Ok(())
        }

        fn stop(&mut self) -> Result<()> {
            self.blocking = None;
            self.connection = None;
            self.active = false;
            Ok(())
        }

        fn read_frame(&mut self) -> Result<Option<TelemetryFrame>> {
            if !self.active {
                return Ok(None);
            }

            let blocking = match &self.blocking {
                Some(b) => b,
                None => return Ok(None),
            };

            // Try to get a sample with a short timeout (non-blocking)
            match blocking.sample(Duration::from_millis(1)) {
                Ok(sample) => {
                    let frame = self.convert_sample(&sample);
                    Ok(Some(frame))
                }
                Err(_) => Ok(None), // Timeout or no data available
            }
        }

        fn is_active(&self) -> bool {
            self.active
        }
    }
}

// Re-export for Windows
#[cfg(target_os = "windows")]
pub use windows_impl::IRacingAdapter;

// Stub implementation for non-Windows platforms
#[cfg(not(target_os = "windows"))]
#[derive(Default)]
pub struct IRacingAdapter;

#[cfg(not(target_os = "windows"))]
impl IRacingAdapter {
    pub fn new() -> Self {
        Self
    }
}

#[cfg(not(target_os = "windows"))]
impl ost_core::adapter::TelemetryAdapter for IRacingAdapter {
    fn name(&self) -> &str {
        "iRacing (Windows only)"
    }

    fn detect(&self) -> bool {
        false
    }

    fn start(&mut self) -> anyhow::Result<()> {
        anyhow::bail!("iRacing adapter only available on Windows")
    }

    fn stop(&mut self) -> anyhow::Result<()> {
        Ok(())
    }

    fn read_frame(&mut self) -> anyhow::Result<Option<ost_core::model::TelemetryFrame>> {
        Ok(None)
    }

    fn is_active(&self) -> bool {
        false
    }
}
