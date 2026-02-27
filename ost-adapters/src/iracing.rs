//! iRacing adapter using the iracing.rs library
//!
//! This adapter connects to iRacing via shared memory and reads telemetry data.
//! It enumerates ALL available telemetry variables each tick via `sample.all()`,
//! maps known variables to the normalized data model, and dumps everything else
//! to `extras` with an "iracing/" prefix.
//!
//! Only available on Windows.

#[cfg(target_os = "windows")]
mod windows_impl {
    use anyhow::Result;
    use chrono::Utc;
    use iracing::session::SessionDetails;
    use iracing::telemetry::{Connection, Sample as IRacingSample, Value, ValueDescription};
    use ost_core::{adapter::TelemetryAdapter, model::*, units::*};
    use std::collections::HashMap;
    use std::convert::TryInto;
    use std::time::Duration;

    pub struct IRacingAdapter {
        connection: Option<Connection>,
        blocking: Option<iracing::telemetry::Blocking>,
        active: bool,
        /// Cached session info
        session_details: Option<SessionDetails>,
        /// Whether session info changed on last read
        session_changed: bool,
    }

    // SAFETY: iRacing's shared memory is thread-safe for reading.
    // The Connection and Blocking types contain raw pointers to memory-mapped files,
    // which are safe to access from multiple threads (Windows handles the synchronization).
    // We never mutate the shared state, only read from it.
    unsafe impl Send for IRacingAdapter {}
    unsafe impl Sync for IRacingAdapter {}

    /// Set of iRacing variable names that are mapped to normalized fields.
    /// Variables NOT in this set will be forwarded to extras.
    const MAPPED_VARS: &[&str] = &[
        // Motion
        "VelocityX",
        "VelocityY",
        "VelocityZ",
        "LatAccel",
        "LongAccel",
        "VertAccel",
        "Pitch",
        "Yaw",
        "Roll",
        "PitchRate",
        "YawRate",
        "RollRate",
        "Speed",
        // Vehicle
        "RPM",
        "Gear",
        "Throttle",
        "Brake",
        "Clutch",
        "SteeringWheelAngle",
        "SteeringWheelTorque",
        "SteeringWheelPctTorque",
        "IsOnTrack",
        "IsInGarage",
        "PlayerTrackSurface",
        // Engine
        "WaterTemp",
        "OilTemp",
        "OilPress",
        "OilLevel",
        "FuelLevel",
        "FuelLevelPct",
        "FuelPress",
        "FuelUsePerHour",
        "Voltage",
        "ManifoldPress",
        "EngineWarnings",
        // Wheels - LF
        "LFshockDefl",
        "LFshockDeflST",
        "LFshockVel",
        "LFshockVelST",
        "LFrideHeight",
        "LFairPressure",
        "LFcoldPressure",
        "LFtempCL",
        "LFtempCC",
        "LFtempCR",
        "LFtempL",
        "LFtempM",
        "LFtempR",
        "LFwear",
        "LFspeed",
        "LFbrakeLinePress",
        // Wheels - RF
        "RFshockDefl",
        "RFshockDeflST",
        "RFshockVel",
        "RFshockVelST",
        "RFrideHeight",
        "RFairPressure",
        "RFcoldPressure",
        "RFtempCL",
        "RFtempCC",
        "RFtempCR",
        "RFtempL",
        "RFtempM",
        "RFtempR",
        "RFwear",
        "RFspeed",
        "RFbrakeLinePress",
        // Wheels - LR
        "LRshockDefl",
        "LRshockDeflST",
        "LRshockVel",
        "LRshockVelST",
        "LRrideHeight",
        "LRairPressure",
        "LRcoldPressure",
        "LRtempCL",
        "LRtempCC",
        "LRtempCR",
        "LRtempL",
        "LRtempM",
        "LRtempR",
        "LRwear",
        "LRspeed",
        "LRbrakeLinePress",
        // Wheels - RR
        "RRshockDefl",
        "RRshockDeflST",
        "RRshockVel",
        "RRshockVelST",
        "RRrideHeight",
        "RRairPressure",
        "RRcoldPressure",
        "RRtempCL",
        "RRtempCC",
        "RRtempCR",
        "RRtempL",
        "RRtempM",
        "RRtempR",
        "RRwear",
        "RRspeed",
        "RRbrakeLinePress",
        // Timing
        "LapCurrentLapTime",
        "LapLastLapTime",
        "LapBestLapTime",
        "LapBestNLapTime",
        "LapBestNLapLap",
        "Lap",
        "LapCompleted",
        "LapDist",
        "LapDistPct",
        "PlayerCarPosition",
        "PlayerCarClassPosition",
        "LapDeltaToBestLap",
        "LapDeltaToBestLap_OK",
        "LapDeltaToSessionBestLap",
        "LapDeltaToSessionBestLap_OK",
        "LapDeltaToOptimalLap",
        "LapDeltaToOptimalLap_OK",
        "RaceLaps",
        // Session
        "SessionState",
        "SessionTime",
        "SessionTimeRemain",
        "SessionTimeOfDay",
        "SessionLapsRemainEx",
        "SessionFlags",
        "SessionNum",
        // Weather
        "AirTemp",
        "TrackTempCrew",
        "AirPressure",
        "AirDensity",
        "RelativeHumidity",
        "WindVel",
        "WindDir",
        "FogLevel",
        "Precipitation",
        "TrackWetness",
        "Skies",
        "WeatherDeclaredWet",
        // Pit
        "OnPitRoad",
        "PitstopActive",
        "PlayerCarPitSvStatus",
        "PitRepairLeft",
        "PitOptRepairLeft",
        "FastRepairAvailable",
        "FastRepairUsed",
        "dpFuelFill",
        "dpFuelAddKg",
        "dpLFTireChange",
        "dpRFTireChange",
        "dpLRTireChange",
        "dpRRTireChange",
        "dpLFTireColdPress",
        "dpRFTireColdPress",
        "dpLRTireColdPress",
        "dpRRTireColdPress",
        "dpWindshieldTearoff",
        "dpFastRepair",
        // Electronics
        "dcABS",
        "dcTractionControl",
        "dcTractionControl2",
        "dcBrakeBias",
        "dcAntiRollFront",
        "dcAntiRollRear",
        "DRS_Status",
        "dcThrottleShape",
        "PushToPass",
        // Per-car arrays
        "CarIdxLap",
        "CarIdxLapCompleted",
        "CarIdxLapDistPct",
        "CarIdxPosition",
        "CarIdxClassPosition",
        "CarIdxOnPitRoad",
        "CarIdxTrackSurface",
        "CarIdxBestLapTime",
        "CarIdxLastLapTime",
        "CarIdxEstTime",
        "CarIdxGear",
        "CarIdxRPM",
        "CarIdxSteer",
        // Tick
        "SessionTick",
    ];

    impl IRacingAdapter {
        pub fn new() -> Self {
            Self {
                connection: None,
                blocking: None,
                active: false,
                session_details: None,
                session_changed: false,
            }
        }

        /// Refresh session info from the connection, if available.
        fn refresh_session_info(&mut self) {
            if let Some(ref mut conn) = self.connection {
                match conn.session_info() {
                    Ok(details) => {
                        self.session_details = Some(details);
                        self.session_changed = true;
                    }
                    Err(_) => {}
                }
            }
        }

        /// Convert iRacing telemetry sample to unified TelemetryFrame.
        /// Uses `sample.all()` to enumerate every variable, maps known ones
        /// to normalized fields, and puts the rest into extras.
        fn convert_sample(&self, sample: &IRacingSample) -> TelemetryFrame {
            // Get all variables in one pass
            let all_vars = sample.all();
            let vars: HashMap<&str, &ValueDescription> =
                all_vars.iter().map(|v| (v.name.as_str(), v)).collect();

            // Helper closures for type conversion
            let get_f32 = |name: &str| -> Option<f32> {
                vars.get(name).and_then(|v| v.value.clone().try_into().ok())
            };
            let get_f64 = |name: &str| -> Option<f64> {
                vars.get(name).and_then(|v| v.value.clone().try_into().ok())
            };
            let get_i32 = |name: &str| -> Option<i32> {
                vars.get(name).and_then(|v| v.value.clone().try_into().ok())
            };
            let get_u32 = |name: &str| -> Option<u32> {
                vars.get(name).and_then(|v| v.value.clone().try_into().ok())
            };
            let get_bool =
                |name: &str| -> Option<bool> { vars.get(name).map(|v| v.value.clone().into()) };

            let tick = get_i32("SessionTick").map(|t| t as u32);

            // =================================================================
            // Motion
            // =================================================================
            let velocity = match (
                get_f32("VelocityX"),
                get_f32("VelocityY"),
                get_f32("VelocityZ"),
            ) {
                (Some(vx), Some(vy), Some(vz)) => Some(Vector3::new(
                    MetersPerSecond(vx),
                    MetersPerSecond(vy),
                    MetersPerSecond(vz),
                )),
                _ => None,
            };

            let acceleration = match (
                get_f32("LatAccel"),
                get_f32("LongAccel"),
                get_f32("VertAccel"),
            ) {
                (Some(lat), Some(long), Some(vert)) => Some(Vector3::new(
                    MetersPerSecondSquared(lat),
                    MetersPerSecondSquared(vert),
                    MetersPerSecondSquared(long),
                )),
                _ => None,
            };

            let g_force = acceleration.as_ref().map(|a| {
                Vector3::new(
                    GForce::from_acceleration(a.x),
                    GForce::from_acceleration(a.y),
                    GForce::from_acceleration(a.z),
                )
            });

            let rotation = match (get_f32("Pitch"), get_f32("Yaw"), get_f32("Roll")) {
                (Some(p), Some(y), Some(r)) => {
                    Some(Vector3::new(Radians(p), Radians(y), Radians(r)))
                }
                _ => None,
            };

            let angular_velocity = match (
                get_f32("PitchRate"),
                get_f32("YawRate"),
                get_f32("RollRate"),
            ) {
                (Some(p), Some(y), Some(r)) => Some(Vector3::new(
                    RadiansPerSecond(p),
                    RadiansPerSecond(y),
                    RadiansPerSecond(r),
                )),
                _ => None,
            };

            let motion = Some(MotionData {
                position: None,
                velocity,
                acceleration,
                g_force,
                rotation,
                angular_velocity,
                angular_acceleration: None,
            });

            // =================================================================
            // Vehicle
            // =================================================================
            let speed = get_f32("Speed").map(MetersPerSecond).or_else(|| {
                velocity.as_ref().map(|v| {
                    MetersPerSecond((v.x.0.powi(2) + v.y.0.powi(2) + v.z.0.powi(2)).sqrt())
                })
            });

            let track_surface = get_i32("PlayerTrackSurface").map(|idx| match idx {
                -1 => TrackSurface::NotInWorld,
                0 => TrackSurface::Undefined,
                1..=4 => TrackSurface::Asphalt,
                6 | 7 => TrackSurface::Concrete,
                8 | 9 => TrackSurface::RacingDirt,
                10 | 11 => TrackSurface::Paint,
                12..=15 => TrackSurface::Rumble,
                16..=19 => TrackSurface::Grass,
                20..=23 => TrackSurface::Dirt,
                24 => TrackSurface::Sand,
                25..=28 => TrackSurface::Gravel,
                29 => TrackSurface::Grasscrete,
                30 => TrackSurface::Astroturf,
                _ => TrackSurface::Unknown,
            });

            // Session info for RPM limits
            let (max_rpm, idle_rpm) = self.session_details.as_ref().map_or((None, None), |s| {
                (
                    Some(Rpm(s.drivers.red_line_rpm)),
                    Some(Rpm(s.drivers.idle_rpm)),
                )
            });

            let vehicle = Some(VehicleData {
                speed,
                rpm: get_f32("RPM").map(Rpm),
                max_rpm,
                idle_rpm,
                gear: get_i32("Gear").map(|g| g as i8),
                max_gears: None,
                throttle: get_f32("Throttle").map(Percentage::new),
                brake: get_f32("Brake").map(Percentage::new),
                clutch: get_f32("Clutch").map(Percentage::new),
                steering_angle: get_f32("SteeringWheelAngle").map(Radians),
                steering_torque: get_f32("SteeringWheelTorque").map(NewtonMeters),
                steering_torque_pct: get_f32("SteeringWheelPctTorque").map(Percentage::new),
                handbrake: None,
                on_track: get_bool("IsOnTrack"),
                in_garage: get_bool("IsInGarage"),
                track_surface,
            });

            // =================================================================
            // Engine
            // =================================================================
            let fuel_capacity = self
                .session_details
                .as_ref()
                .map(|s| Liters(s.drivers.fuel_capacity));

            let engine_warnings = get_u32("EngineWarnings").map(EngineWarnings::from_iracing_bits);

            let engine = Some(EngineData {
                water_temp: get_f32("WaterTemp").map(Celsius),
                oil_temp: get_f32("OilTemp").map(Celsius),
                oil_pressure: get_f32("OilPress").map(Kilopascals),
                oil_level: get_f32("OilLevel").map(Percentage::new),
                fuel_level: get_f32("FuelLevel").map(Liters),
                fuel_level_pct: get_f32("FuelLevelPct").map(Percentage::new),
                fuel_capacity,
                fuel_pressure: get_f32("FuelPress").map(Kilopascals),
                fuel_use_per_hour: get_f32("FuelUsePerHour").map(LitersPerHour),
                voltage: get_f32("Voltage").map(Volts),
                manifold_pressure: get_f32("ManifoldPress").map(Bar),
                warnings: engine_warnings,
            });

            // =================================================================
            // Wheels
            // =================================================================
            let wheels = Some(WheelData {
                front_left: self.extract_wheel(&vars, "LF", true),
                front_right: self.extract_wheel(&vars, "RF", false),
                rear_left: self.extract_wheel(&vars, "LR", true),
                rear_right: self.extract_wheel(&vars, "RR", false),
            });

            // =================================================================
            // Timing
            // =================================================================
            let estimated_lap_time = self
                .session_details
                .as_ref()
                .map(|s| Seconds(s.drivers.estimated_lap_time));

            let num_cars = self
                .session_details
                .as_ref()
                .map(|s| s.drivers.other_drivers.len() as u32);

            let timing = Some(TimingData {
                current_lap_time: get_f64("LapCurrentLapTime").map(|t| Seconds(t as f32)),
                last_lap_time: get_f64("LapLastLapTime").map(|t| Seconds(t as f32)),
                best_lap_time: get_f64("LapBestLapTime").map(|t| Seconds(t as f32)),
                best_n_lap_time: get_f64("LapBestNLapTime").map(|t| Seconds(t as f32)),
                best_n_lap_num: get_i32("LapBestNLapLap").map(|v| v as u32),
                sector_times: None,
                lap_number: get_i32("Lap").map(|l| l as u32),
                laps_completed: get_i32("LapCompleted").map(|l| l as u32),
                lap_distance: get_f32("LapDist").map(Meters),
                lap_distance_pct: get_f32("LapDistPct").map(Percentage::new),
                race_position: get_i32("PlayerCarPosition").map(|p| p as u32),
                class_position: get_i32("PlayerCarClassPosition").map(|p| p as u32),
                num_cars,
                delta_best: get_f32("LapDeltaToBestLap").map(Seconds),
                delta_best_ok: get_bool("LapDeltaToBestLap_OK"),
                delta_session_best: get_f32("LapDeltaToSessionBestLap").map(Seconds),
                delta_session_best_ok: get_bool("LapDeltaToSessionBestLap_OK"),
                delta_optimal: get_f32("LapDeltaToOptimalLap").map(Seconds),
                delta_optimal_ok: get_bool("LapDeltaToOptimalLap_OK"),
                estimated_lap_time,
                race_laps: get_i32("RaceLaps").map(|l| l as u32),
            });

            // =================================================================
            // Session
            // =================================================================
            let session_state = get_i32("SessionState").map(SessionState::from_iracing);

            let flags = get_u32("SessionFlags").map(FlagState::from_iracing_bits);

            let (track_name, track_config, track_length_m, track_type_str, car_name, car_class) =
                self.session_details
                    .as_ref()
                    .map_or((None, None, None, None, None, None), |s| {
                        let player_idx = s.drivers.car_index;
                        let driver = s
                            .drivers
                            .other_drivers
                            .iter()
                            .find(|d| d.index == player_idx);
                        let track_len = s
                            .weekend
                            .track_length
                            .trim_end_matches(" km")
                            .replace(',', ".")
                            .parse::<f32>()
                            .ok()
                            .map(|km| Meters(km * 1000.0));

                        (
                            Some(s.weekend.track_display_name.clone()),
                            if s.weekend.track_config_name.is_empty() {
                                None
                            } else {
                                Some(s.weekend.track_config_name.clone())
                            },
                            track_len,
                            Some(s.weekend.track_type.clone()),
                            driver.map(|d| d.car_screen_name.clone()),
                            driver.map(|d| d.car_class_short_name.clone()),
                        )
                    });

            // Determine session type from session info
            let session_type = self.session_details.as_ref().and_then(|s| {
                let sess_num = get_i32("SessionNum").unwrap_or(0) as u64;
                s.session
                    .sessions
                    .iter()
                    .find(|sess| sess.session_number == sess_num)
                    .map(|sess| match sess.session_type.to_lowercase().as_str() {
                        "practice" | "open practice" | "lone practice" => SessionType::Practice,
                        "qualify" | "open qualify" | "lone qualify" | "qualifying" => {
                            SessionType::Qualifying
                        }
                        "race" => SessionType::Race,
                        "warmup" | "warm up" => SessionType::Warmup,
                        "time trial" => SessionType::TimeTrial,
                        _ => SessionType::Other,
                    })
            });

            let session_laps = self.session_details.as_ref().and_then(|s| {
                let sess_num = get_i32("SessionNum").unwrap_or(0) as u64;
                s.session
                    .sessions
                    .iter()
                    .find(|sess| sess.session_number == sess_num)
                    .and_then(|sess| sess.max_laps().map(|l| l as u32))
            });

            let session = Some(SessionData {
                session_type,
                session_state,
                session_time: get_f64("SessionTime").map(|t| Seconds(t as f32)),
                session_time_remaining: get_f64("SessionTimeRemain").map(|t| Seconds(t as f32)),
                session_time_of_day: get_f32("SessionTimeOfDay").map(Seconds),
                session_laps,
                session_laps_remaining: get_i32("SessionLapsRemainEx").map(|l| l as u32),
                flags,
                track_name,
                track_config,
                track_length: track_length_m,
                track_type: track_type_str,
                car_name,
                car_class,
            });

            // =================================================================
            // Weather
            // =================================================================
            let weather = Some(WeatherData {
                air_temp: get_f32("AirTemp").map(Celsius),
                track_temp: get_f32("TrackTempCrew").map(Celsius),
                air_pressure: get_f32("AirPressure").map(Pascals),
                air_density: get_f32("AirDensity").map(KilogramsPerCubicMeter),
                humidity: get_f32("RelativeHumidity").map(|h| Percentage::new(h / 100.0)),
                wind_speed: get_f32("WindVel").map(MetersPerSecond),
                wind_direction: get_f32("WindDir").map(Radians),
                fog_level: get_f32("FogLevel").map(Percentage::new),
                precipitation: get_f32("Precipitation").map(Percentage::new),
                track_wetness: get_i32("TrackWetness").map(|w| match w {
                    0 => TrackWetness::Dry,
                    1 => TrackWetness::SlightlyWet,
                    2 => TrackWetness::Wet,
                    3 => TrackWetness::VeryWet,
                    4 => TrackWetness::Flooded,
                    _ => TrackWetness::Unknown,
                }),
                skies: get_i32("Skies").map(|s| match s {
                    0 => "Clear".to_string(),
                    1 => "Partly Cloudy".to_string(),
                    2 => "Mostly Cloudy".to_string(),
                    3 => "Overcast".to_string(),
                    _ => format!("Unknown({})", s),
                }),
                declared_wet: get_bool("WeatherDeclaredWet"),
            });

            // =================================================================
            // Pit
            // =================================================================
            let requested_services = Some(PitServices {
                fuel_to_add: get_f32("dpFuelFill").map(Liters),
                change_tyre_fl: get_f32("dpLFTireChange").map_or(false, |v| v > 0.0),
                change_tyre_fr: get_f32("dpRFTireChange").map_or(false, |v| v > 0.0),
                change_tyre_rl: get_f32("dpLRTireChange").map_or(false, |v| v > 0.0),
                change_tyre_rr: get_f32("dpRRTireChange").map_or(false, |v| v > 0.0),
                windshield_tearoff: get_f32("dpWindshieldTearoff").map_or(false, |v| v > 0.0),
                fast_repair: get_f32("dpFastRepair").map_or(false, |v| v > 0.0),
                tyre_pressure_fl: get_f32("dpLFTireColdPress").map(Kilopascals),
                tyre_pressure_fr: get_f32("dpRFTireColdPress").map(Kilopascals),
                tyre_pressure_rl: get_f32("dpLRTireColdPress").map(Kilopascals),
                tyre_pressure_rr: get_f32("dpRRTireColdPress").map(Kilopascals),
            });

            let pit_speed_limit = self.session_details.as_ref().and_then(|s| {
                s.weekend
                    .track_pit_speed_limit
                    .trim_end_matches(" kph")
                    .replace(',', ".")
                    .parse::<f32>()
                    .ok()
                    .map(|kph| MetersPerSecond(kph / 3.6))
            });

            let pit = Some(PitData {
                on_pit_road: get_bool("OnPitRoad"),
                pit_active: get_bool("PitstopActive"),
                pit_service_status: get_i32("PlayerCarPitSvStatus").map(|v| v as u32),
                repair_time_left: get_f32("PitRepairLeft").map(Seconds),
                optional_repair_time_left: get_f32("PitOptRepairLeft").map(Seconds),
                fast_repair_available: get_i32("FastRepairAvailable").map(|v| v as u32),
                fast_repair_used: get_i32("FastRepairUsed").map(|v| v as u32),
                pit_speed_limit,
                requested_services,
            });

            // =================================================================
            // Electronics
            // =================================================================
            let electronics = Some(ElectronicsData {
                abs: get_f32("dcABS"),
                traction_control: get_f32("dcTractionControl"),
                traction_control_2: get_f32("dcTractionControl2"),
                brake_bias: get_f32("dcBrakeBias").map(Percentage::new),
                anti_roll_front: get_f32("dcAntiRollFront"),
                anti_roll_rear: get_f32("dcAntiRollRear"),
                drs_status: get_i32("DRS_Status").map(|v| v as u32),
                push_to_pass_status: None,
                push_to_pass_count: None,
                throttle_shape: get_f32("dcThrottleShape"),
            });

            // =================================================================
            // Competitors (from CarIdx arrays)
            // =================================================================
            let competitors = self.extract_competitors(&all_vars);

            // =================================================================
            // Driver (from session info)
            // =================================================================
            let driver = self.session_details.as_ref().map(|s| {
                let player_idx = s.drivers.car_index;
                let driver_info = s
                    .drivers
                    .other_drivers
                    .iter()
                    .find(|d| d.index == player_idx);

                DriverData {
                    name: driver_info.map(|d| d.user_name.clone()),
                    car_index: Some(player_idx as u32),
                    car_name: driver_info.map(|d| d.car_screen_name.clone()),
                    car_class: driver_info.map(|d| d.car_class_short_name.clone()),
                    car_number: driver_info.map(|d| d.car_number.to_string()),
                    team_name: driver_info.map(|d| d.team_name.clone()),
                    fuel_capacity: Some(Liters(s.drivers.fuel_capacity)),
                    shift_light_first_rpm: Some(Rpm(s.drivers.shift_light_first_rpm)),
                    shift_light_shift_rpm: Some(Rpm(s.drivers.shift_light_shift_rpm)),
                    shift_light_last_rpm: Some(Rpm(s.drivers.shift_light_last_rpm)),
                    shift_light_blink_rpm: Some(Rpm(s.drivers.shift_light_blink_rpm)),
                    estimated_lap_time: Some(Seconds(s.drivers.estimated_lap_time)),
                    setup_name: Some(s.drivers.setup_name.clone()),
                }
            });

            // =================================================================
            // Extras: every unmapped variable with iracing/ prefix
            // =================================================================
            let mut extras = HashMap::new();

            // Build a set of mapped names for fast lookup
            let mapped_set: std::collections::HashSet<&str> = MAPPED_VARS.iter().copied().collect();

            for vd in &all_vars {
                if mapped_set.contains(vd.name.as_str()) {
                    continue;
                }
                // Skip CarIdx arrays (already processed into competitors)
                if vd.name.starts_with("CarIdx") {
                    continue;
                }

                let key = format!("iracing/{}", vd.name);
                let json_val = value_to_json(&vd.value);
                extras.insert(key, json_val);
            }

            // Also add game-specific driver metadata from session info
            if let Some(ref s) = self.session_details {
                let player_idx = s.drivers.car_index;
                if let Some(driver_info) = s
                    .drivers
                    .other_drivers
                    .iter()
                    .find(|d| d.index == player_idx)
                {
                    extras.insert(
                        "iracing/iRating".to_string(),
                        serde_json::json!(driver_info.i_rating),
                    );
                    extras.insert(
                        "iracing/LicenseLevel".to_string(),
                        serde_json::json!(driver_info.license_level),
                    );
                    extras.insert(
                        "iracing/LicenseSubLevel".to_string(),
                        serde_json::json!(driver_info.license_sub_level),
                    );
                    extras.insert(
                        "iracing/LicenseString".to_string(),
                        serde_json::json!(driver_info.license.clone()),
                    );
                    extras.insert(
                        "iracing/SeriesID".to_string(),
                        serde_json::json!(s.weekend.series_id),
                    );
                    extras.insert(
                        "iracing/SessionID".to_string(),
                        serde_json::json!(s.weekend.session_id),
                    );
                    extras.insert(
                        "iracing/SubSessionID".to_string(),
                        serde_json::json!(s.weekend.sub_session_id),
                    );
                }
            }

            TelemetryFrame {
                timestamp: Utc::now(),
                game: "iRacing".to_string(),
                tick,
                motion,
                vehicle,
                engine,
                wheels,
                timing,
                session,
                weather,
                pit,
                electronics,
                damage: None,
                competitors,
                driver,
                extras,
            }
        }

        /// Extract per-wheel data.
        /// `prefix` is "LF", "RF", "LR", or "RR".
        /// `is_left_side` determines inner/outer mapping for temperatures.
        fn extract_wheel(
            &self,
            vars: &HashMap<&str, &ValueDescription>,
            prefix: &str,
            is_left_side: bool,
        ) -> WheelInfo {
            let get_f32 = |suffix: &str| -> Option<f32> {
                let key = format!("{}{}", prefix, suffix);
                vars.get(key.as_str())
                    .and_then(|v| v.value.clone().try_into().ok())
            };

            // Inner/outer mapping: for left wheels, CL=outer edge, CR=inner edge.
            // For right wheels, CL=inner edge, CR=outer edge.
            let (surface_temp_inner, surface_temp_outer) = if is_left_side {
                (
                    get_f32("tempCR").map(Celsius),
                    get_f32("tempCL").map(Celsius),
                )
            } else {
                (
                    get_f32("tempCL").map(Celsius),
                    get_f32("tempCR").map(Celsius),
                )
            };

            let (carcass_temp_inner, carcass_temp_outer) = if is_left_side {
                (get_f32("tempR").map(Celsius), get_f32("tempL").map(Celsius))
            } else {
                (get_f32("tempL").map(Celsius), get_f32("tempR").map(Celsius))
            };

            WheelInfo {
                suspension_travel: get_f32("shockDefl").map(Meters),
                suspension_travel_avg: get_f32("shockDeflST").map(Meters),
                shock_velocity: get_f32("shockVel").map(MetersPerSecond),
                shock_velocity_avg: get_f32("shockVelST").map(MetersPerSecond),
                ride_height: get_f32("rideHeight").map(Meters),
                tyre_pressure: get_f32("airPressure").map(Kilopascals),
                tyre_cold_pressure: get_f32("coldPressure").map(Kilopascals),
                surface_temp_inner,
                surface_temp_middle: get_f32("tempCC").map(Celsius),
                surface_temp_outer,
                carcass_temp_inner,
                carcass_temp_middle: get_f32("tempM").map(Celsius),
                carcass_temp_outer,
                tyre_wear: get_f32("wear").map(Percentage::new),
                wheel_speed: get_f32("speed").map(RadiansPerSecond),
                slip_ratio: None,
                slip_angle: None,
                load: None,
                brake_line_pressure: get_f32("brakeLinePress").map(Kilopascals),
                brake_temp: None,
                tyre_compound: None,
            }
        }

        /// Extract competitor data from CarIdx* arrays and merge with session info.
        fn extract_competitors(
            &self,
            all_vars: &[ValueDescription],
        ) -> Option<Vec<CompetitorData>> {
            // Find the CarIdx arrays
            let find_int_vec = |name: &str| -> Option<&Vec<i32>> {
                all_vars.iter().find(|v| v.name == name).and_then(|v| {
                    if let Value::IntVec(ref vec) = v.value {
                        Some(vec)
                    } else {
                        None
                    }
                })
            };
            let find_float_vec = |name: &str| -> Option<&Vec<f32>> {
                all_vars.iter().find(|v| v.name == name).and_then(|v| {
                    if let Value::FloatVec(ref vec) = v.value {
                        Some(vec)
                    } else {
                        None
                    }
                })
            };
            let find_bool_vec = |name: &str| -> Option<&Vec<bool>> {
                all_vars.iter().find(|v| v.name == name).and_then(|v| {
                    if let Value::BoolVec(ref vec) = v.value {
                        Some(vec)
                    } else {
                        None
                    }
                })
            };

            let laps = find_int_vec("CarIdxLap");
            let laps_completed = find_int_vec("CarIdxLapCompleted");
            let lap_dist_pct = find_float_vec("CarIdxLapDistPct");
            let positions = find_int_vec("CarIdxPosition");
            let class_positions = find_int_vec("CarIdxClassPosition");
            let on_pit_road = find_bool_vec("CarIdxOnPitRoad");
            let track_surfaces = find_int_vec("CarIdxTrackSurface");
            let best_lap_times = find_float_vec("CarIdxBestLapTime");
            let last_lap_times = find_float_vec("CarIdxLastLapTime");
            let est_times = find_float_vec("CarIdxEstTime");
            let gears = find_int_vec("CarIdxGear");
            let rpms = find_float_vec("CarIdxRPM");
            let steers = find_float_vec("CarIdxSteer");

            // Determine number of entries from any available array
            let count = laps
                .map(|v| v.len())
                .or_else(|| positions.map(|v| v.len()))
                .or_else(|| lap_dist_pct.map(|v| v.len()));

            let count = match count {
                Some(c) => c,
                None => return None,
            };

            // Player car index to skip
            let player_idx = self.session_details.as_ref().map(|s| s.drivers.car_index);

            let mut competitors = Vec::new();

            for i in 0..count {
                // Skip invalid entries (lap == -1 means not in session)
                let lap_val = laps.and_then(|v| v.get(i).copied());
                if lap_val == Some(-1) {
                    continue;
                }

                // Skip the player's own car
                if player_idx == Some(i) {
                    continue;
                }

                // Get session info for this driver
                let (driver_name, car_name_str, car_class_str, team_name_str, car_number_str) =
                    self.session_details
                        .as_ref()
                        .map_or((None, None, None, None, None), |s| {
                            s.drivers
                                .other_drivers
                                .iter()
                                .find(|d| d.index == i)
                                .map_or((None, None, None, None, None), |d| {
                                    (
                                        Some(d.user_name.clone()),
                                        Some(d.car_screen_name.clone()),
                                        Some(d.car_class_short_name.clone()),
                                        Some(d.team_name.clone()),
                                        Some(d.car_number.to_string()),
                                    )
                                })
                        });

                let track_surface_val =
                    track_surfaces
                        .and_then(|v| v.get(i).copied())
                        .map(|idx| match idx {
                            -1 => TrackSurface::NotInWorld,
                            0 => TrackSurface::Undefined,
                            1..=4 => TrackSurface::Asphalt,
                            6 | 7 => TrackSurface::Concrete,
                            8 | 9 => TrackSurface::RacingDirt,
                            10 | 11 => TrackSurface::Paint,
                            12..=15 => TrackSurface::Rumble,
                            16..=19 => TrackSurface::Grass,
                            20..=23 => TrackSurface::Dirt,
                            24 => TrackSurface::Sand,
                            25..=28 => TrackSurface::Gravel,
                            29 => TrackSurface::Grasscrete,
                            30 => TrackSurface::Astroturf,
                            _ => TrackSurface::Unknown,
                        });

                competitors.push(CompetitorData {
                    car_index: i as u32,
                    driver_name,
                    car_name: car_name_str,
                    car_class: car_class_str,
                    team_name: team_name_str,
                    car_number: car_number_str,
                    lap: lap_val.map(|l| l as u32),
                    laps_completed: laps_completed
                        .and_then(|v| v.get(i).copied())
                        .map(|l| l as u32),
                    lap_distance_pct: lap_dist_pct
                        .and_then(|v| v.get(i).copied())
                        .map(Percentage::new),
                    position: positions.and_then(|v| v.get(i).copied()).map(|p| p as u32),
                    class_position: class_positions
                        .and_then(|v| v.get(i).copied())
                        .map(|p| p as u32),
                    on_pit_road: on_pit_road.and_then(|v| v.get(i).copied()),
                    track_surface: track_surface_val,
                    best_lap_time: best_lap_times
                        .and_then(|v| v.get(i).copied())
                        .and_then(|t| if t > 0.0 { Some(Seconds(t)) } else { None }),
                    last_lap_time: last_lap_times
                        .and_then(|v| v.get(i).copied())
                        .and_then(|t| if t > 0.0 { Some(Seconds(t)) } else { None }),
                    estimated_time: est_times.and_then(|v| v.get(i).copied()).and_then(|t| {
                        if t > 0.0 {
                            Some(Seconds(t))
                        } else {
                            None
                        }
                    }),
                    gear: gears.and_then(|v| v.get(i).copied()).map(|g| g as i8),
                    rpm: rpms.and_then(|v| v.get(i).copied()).map(Rpm),
                    steering: steers.and_then(|v| v.get(i).copied()).map(Radians),
                });
            }

            if competitors.is_empty() {
                None
            } else {
                Some(competitors)
            }
        }
    }

    /// Convert an iracing Value to serde_json::Value
    fn value_to_json(value: &Value) -> serde_json::Value {
        match value {
            Value::CHAR(c) => serde_json::json!(*c),
            Value::BOOL(b) => serde_json::json!(*b),
            Value::INT(i) => serde_json::json!(*i),
            Value::BITS(u) => serde_json::json!(*u),
            Value::FLOAT(f) => serde_json::json!((*f * 10000.0).round() / 10000.0),
            Value::DOUBLE(d) => serde_json::json!((*d * 10000.0).round() / 10000.0),
            Value::IntVec(v) => serde_json::json!(v),
            Value::FloatVec(v) => {
                let rounded: Vec<f32> = v.iter().map(|x| (x * 10000.0).round() / 10000.0).collect();
                serde_json::json!(rounded)
            }
            Value::BoolVec(v) => serde_json::json!(v),
            Value::UNKNOWN(_) => serde_json::Value::Null,
        }
    }

    impl TelemetryAdapter for IRacingAdapter {
        fn key(&self) -> &str {
            "iracing"
        }

        fn name(&self) -> &str {
            "iRacing"
        }

        fn detect(&self) -> bool {
            Connection::new().is_ok()
        }

        fn start(&mut self) -> Result<()> {
            let mut connection = Connection::new()?;

            // Read initial session info
            if let Ok(details) = connection.session_info() {
                self.session_details = Some(details);
                self.session_changed = true;
            }

            let blocking = connection.blocking()?;

            self.connection = Some(connection);
            self.blocking = Some(blocking);
            self.active = true;

            Ok(())
        }

        fn stop(&mut self) -> Result<()> {
            self.blocking = None;
            self.connection = None;
            self.session_details = None;
            self.active = false;
            self.session_changed = false;
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

            match blocking.sample(Duration::from_millis(1)) {
                Ok(sample) => {
                    // Periodically refresh session info
                    // (iRacing updates session info version when it changes)
                    self.refresh_session_info();

                    let frame = self.convert_sample(&sample);
                    Ok(Some(frame))
                }
                Err(_) => Ok(None),
            }
        }

        fn is_active(&self) -> bool {
            self.active
        }

        fn session_info_changed(&self) -> bool {
            self.session_changed
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
    fn key(&self) -> &str {
        "iracing"
    }

    fn name(&self) -> &str {
        "iRacing"
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
