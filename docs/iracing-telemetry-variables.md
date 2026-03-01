# iRacing .ibt Telemetry Variables

Reference of all telemetry variables available in iRacing .ibt files (267 total).
Variables are either **mapped** to the normalized TelemetryFrame model or forwarded
as **extras** with an `iracing/` prefix.

## Mapped Variables (203)

These are mapped to the standard TelemetryFrame model via `MAPPED_VARS` in
`ost-adapters/src/iracing.rs`.

### Motion (16)
| iRacing Variable | Model Path | Notes |
|---|---|---|
| VelocityX | motion.velocity.x | m/s |
| VelocityY | motion.velocity.y | m/s |
| VelocityZ | motion.velocity.z | m/s |
| LatAccel | motion.g_force.x | lateral G |
| LongAccel | motion.g_force.y | longitudinal G |
| VertAccel | motion.g_force.z | vertical G |
| Pitch | motion.orientation.pitch | rad |
| Yaw | motion.orientation.yaw | rad |
| Roll | motion.orientation.roll | rad |
| PitchRate | motion.pitch_rate | deg/s |
| YawRate | motion.yaw_rate | deg/s |
| RollRate | motion.roll_rate | deg/s |
| Speed | vehicle.speed | m/s |
| Lat | motion.latitude | degrees, WGS84 |
| Lon | motion.longitude | degrees, WGS84 |
| Alt | motion.altitude | meters ASL |

### Vehicle (14)
| iRacing Variable | Model Path | Notes |
|---|---|---|
| RPM | vehicle.rpm | |
| Gear | vehicle.gear | -1=R, 0=N, 1+=forward |
| Throttle | vehicle.throttle | 0.0-1.0 |
| Brake | vehicle.brake | 0.0-1.0 |
| Clutch | vehicle.clutch | 0.0=engaged, 1.0=disengaged |
| SteeringWheelAngle | vehicle.steering_angle | rad |
| SteeringWheelTorque | vehicle.steering_torque | Nm |
| SteeringWheelPctTorque | vehicle.steering_torque_pct | 0.0-1.0 |
| HandbrakeRaw | vehicle.handbrake | 0.0-1.0 |
| ShiftIndicatorPct | vehicle.shift_indicator | 0.0-1.0 shift light |
| SteeringWheelAngleMax | vehicle.steering_angle_max | rad, max steering lock |
| IsOnTrack | vehicle.on_track | bool |
| IsInGarage | vehicle.in_garage | bool |
| PlayerTrackSurface | vehicle.track_surface | enum |

### Engine (12)
| iRacing Variable | Model Path | Notes |
|---|---|---|
| WaterTemp | engine.water_temp | C |
| OilTemp | engine.oil_temp | C |
| OilPress | engine.oil_pressure | kPa |
| OilLevel | engine.oil_level | L |
| FuelLevel | engine.fuel_level | L |
| FuelLevelPct | engine.fuel_level_pct | 0.0-1.0 |
| FuelPress | engine.fuel_pressure | kPa |
| FuelUsePerHour | engine.fuel_use_per_hour | L/hr |
| Voltage | engine.voltage | V |
| ManifoldPress | engine.manifold_pressure | kPa |
| WaterLevel | engine.water_level | L, coolant level |
| EngineWarnings | engine.warnings | bitfield |

### Wheels — Front Left (19)
| iRacing Variable | Model Path | Notes |
|---|---|---|
| LFshockDefl | wheels.front_left.suspension_deflection | m |
| LFshockDeflST | wheels.front_left.suspension_deflection_st | m (short-term) |
| LFshockVel | wheels.front_left.suspension_velocity | m/s |
| LFshockVelST | wheels.front_left.suspension_velocity_st | m/s (short-term) |
| LFrideHeight | wheels.front_left.ride_height | m |
| LFairPressure | wheels.front_left.tyre_pressure | kPa |
| LFcoldPressure | wheels.front_left.tyre_cold_pressure | kPa |
| LFtempCL | wheels.front_left.tyre_temp_left | C (carcass) |
| LFtempCC | wheels.front_left.tyre_temp_center | C (carcass) |
| LFtempCR | wheels.front_left.tyre_temp_right | C (carcass) |
| LFtempL | wheels.front_left.tyre_surface_temp_left | C (surface) |
| LFtempM | wheels.front_left.tyre_surface_temp_center | C (surface) |
| LFtempR | wheels.front_left.tyre_surface_temp_right | C (surface) |
| LFwear | wheels.front_left.tyre_wear | 0.0-1.0 |
| LFwearL | wheels.front_left.tyre_wear_outer | 0.0-1.0 (L=outer for left) |
| LFwearM | wheels.front_left.tyre_wear_middle | 0.0-1.0 |
| LFwearR | wheels.front_left.tyre_wear_inner | 0.0-1.0 (R=inner for left) |
| LFspeed | wheels.front_left.wheel_speed | rad/s → rpm |
| LFbrakeLinePress | wheels.front_left.brake_line_pressure | kPa |

### Wheels — Front Right (19)
| iRacing Variable | Model Path | Notes |
|---|---|---|
| RFshockDefl | wheels.front_right.suspension_deflection | |
| RFshockDeflST | wheels.front_right.suspension_deflection_st | |
| RFshockVel | wheels.front_right.suspension_velocity | |
| RFshockVelST | wheels.front_right.suspension_velocity_st | |
| RFrideHeight | wheels.front_right.ride_height | |
| RFairPressure | wheels.front_right.tyre_pressure | |
| RFcoldPressure | wheels.front_right.tyre_cold_pressure | |
| RFtempCL | wheels.front_right.tyre_temp_left | |
| RFtempCC | wheels.front_right.tyre_temp_center | |
| RFtempCR | wheels.front_right.tyre_temp_right | |
| RFtempL | wheels.front_right.tyre_surface_temp_left | |
| RFtempM | wheels.front_right.tyre_surface_temp_center | |
| RFtempR | wheels.front_right.tyre_surface_temp_right | |
| RFwear | wheels.front_right.tyre_wear | |
| RFwearL | wheels.front_right.tyre_wear_inner | L=inner for right |
| RFwearM | wheels.front_right.tyre_wear_middle | |
| RFwearR | wheels.front_right.tyre_wear_outer | R=outer for right |
| RFspeed | wheels.front_right.wheel_speed | |
| RFbrakeLinePress | wheels.front_right.brake_line_pressure | |

### Wheels — Rear Left (19)
| iRacing Variable | Model Path | Notes |
|---|---|---|
| LRshockDefl | wheels.rear_left.suspension_deflection | |
| LRshockDeflST | wheels.rear_left.suspension_deflection_st | |
| LRshockVel | wheels.rear_left.suspension_velocity | |
| LRshockVelST | wheels.rear_left.suspension_velocity_st | |
| LRrideHeight | wheels.rear_left.ride_height | |
| LRairPressure | wheels.rear_left.tyre_pressure | |
| LRcoldPressure | wheels.rear_left.tyre_cold_pressure | |
| LRtempCL | wheels.rear_left.tyre_temp_left | |
| LRtempCC | wheels.rear_left.tyre_temp_center | |
| LRtempCR | wheels.rear_left.tyre_temp_right | |
| LRtempL | wheels.rear_left.tyre_surface_temp_left | |
| LRtempM | wheels.rear_left.tyre_surface_temp_center | |
| LRtempR | wheels.rear_left.tyre_surface_temp_right | |
| LRwear | wheels.rear_left.tyre_wear | |
| LRwearL | wheels.rear_left.tyre_wear_outer | L=outer for left |
| LRwearM | wheels.rear_left.tyre_wear_middle | |
| LRwearR | wheels.rear_left.tyre_wear_inner | R=inner for left |
| LRspeed | wheels.rear_left.wheel_speed | |
| LRbrakeLinePress | wheels.rear_left.brake_line_pressure | |

### Wheels — Rear Right (19)
| iRacing Variable | Model Path | Notes |
|---|---|---|
| RRshockDefl | wheels.rear_right.suspension_deflection | |
| RRshockDeflST | wheels.rear_right.suspension_deflection_st | |
| RRshockVel | wheels.rear_right.suspension_velocity | |
| RRshockVelST | wheels.rear_right.suspension_velocity_st | |
| RRrideHeight | wheels.rear_right.ride_height | |
| RRairPressure | wheels.rear_right.tyre_pressure | |
| RRcoldPressure | wheels.rear_right.tyre_cold_pressure | |
| RRtempCL | wheels.rear_right.tyre_temp_left | |
| RRtempCC | wheels.rear_right.tyre_temp_center | |
| RRtempCR | wheels.rear_right.tyre_temp_right | |
| RRtempL | wheels.rear_right.tyre_surface_temp_left | |
| RRtempM | wheels.rear_right.tyre_surface_temp_center | |
| RRtempR | wheels.rear_right.tyre_surface_temp_right | |
| RRwear | wheels.rear_right.tyre_wear | |
| RRwearL | wheels.rear_right.tyre_wear_inner | L=inner for right |
| RRwearM | wheels.rear_right.tyre_wear_middle | |
| RRwearR | wheels.rear_right.tyre_wear_outer | R=outer for right |
| RRspeed | wheels.rear_right.wheel_speed | |
| RRbrakeLinePress | wheels.rear_right.brake_line_pressure | |

### Timing (18)
| iRacing Variable | Model Path | Notes |
|---|---|---|
| LapCurrentLapTime | timing.current_lap_time | s |
| LapLastLapTime | timing.last_lap_time | s |
| LapBestLapTime | timing.best_lap_time | s |
| LapBestNLapTime | timing.best_n_lap_time | s |
| LapBestNLapLap | timing.best_n_lap_lap | lap number |
| Lap | timing.current_lap | |
| LapCompleted | timing.laps_completed | |
| LapDist | timing.lap_distance | m |
| LapDistPct | timing.lap_distance_pct | 0.0-1.0 |
| PlayerCarPosition | timing.position | overall |
| PlayerCarClassPosition | timing.class_position | in-class |
| LapDeltaToBestLap | timing.delta_best | s |
| LapDeltaToBestLap_OK | timing.delta_best_ok | bool |
| LapDeltaToSessionBestLap | timing.delta_session_best | s |
| LapDeltaToSessionBestLap_OK | timing.delta_session_best_ok | bool |
| LapDeltaToOptimalLap | timing.delta_optimal | s |
| LapDeltaToOptimalLap_OK | timing.delta_optimal_ok | bool |
| RaceLaps | timing.race_laps | |

### Session (7)
| iRacing Variable | Model Path | Notes |
|---|---|---|
| SessionState | session.state | enum |
| SessionTime | session.time | s |
| SessionTimeRemain | session.time_remaining | s |
| SessionTimeOfDay | session.time_of_day | s since midnight |
| SessionLapsRemainEx | session.laps_remaining | |
| SessionFlags | session.flags | bitfield |
| SessionNum | session.number | |

### Weather (13)
| iRacing Variable | Model Path | Notes |
|---|---|---|
| AirTemp | weather.air_temp | C |
| TrackTempCrew | weather.track_temp | C, crew-reported |
| TrackTemp | weather.track_surface_temp | C, measured surface |
| AirPressure | weather.air_pressure | kPa |
| AirDensity | weather.air_density | kg/m3 |
| RelativeHumidity | weather.humidity | 0.0-1.0 |
| WindVel | weather.wind_speed | m/s |
| WindDir | weather.wind_direction | rad |
| FogLevel | weather.fog_level | 0.0-1.0 |
| Precipitation | weather.precipitation | 0.0-1.0 |
| TrackWetness | weather.track_wetness | enum |
| Skies | weather.sky_condition | enum |
| WeatherDeclaredWet | weather.declared_wet | bool |

### Pit (19)
| iRacing Variable | Model Path | Notes |
|---|---|---|
| OnPitRoad | pit.on_pit_road | bool |
| PitstopActive | pit.pitstop_active | bool |
| PlayerCarPitSvStatus | pit.service_status | bitfield |
| PitRepairLeft | pit.repair_time_left | s |
| PitOptRepairLeft | pit.optional_repair_left | s |
| FastRepairAvailable | pit.fast_repair_available | count |
| FastRepairUsed | pit.fast_repair_used | count |
| dpFuelFill | pit_request.fuel_fill | L |
| dpFuelAddKg | pit_request.fuel_add_kg | kg |
| dpLFTireChange | pit_request.tyre_change_fl | bool |
| dpRFTireChange | pit_request.tyre_change_fr | bool |
| dpLRTireChange | pit_request.tyre_change_rl | bool |
| dpRRTireChange | pit_request.tyre_change_rr | bool |
| dpLFTireColdPress | pit_request.tyre_pressure_fl | kPa |
| dpRFTireColdPress | pit_request.tyre_pressure_fr | kPa |
| dpLRTireColdPress | pit_request.tyre_pressure_rl | kPa |
| dpRRTireColdPress | pit_request.tyre_pressure_rr | kPa |
| dpWindshieldTearoff | pit_request.windshield_tearoff | bool |
| dpFastRepair | pit_request.fast_repair | bool |

### Electronics (11)
| iRacing Variable | Model Path | Notes |
|---|---|---|
| dcABS | electronics.abs | setting level |
| BrakeABSactive | electronics.abs_active | bool, ABS currently firing |
| dcTractionControl | electronics.traction_control | |
| dcTractionControl2 | electronics.traction_control_2 | |
| dcBrakeBias | electronics.brake_bias | % front |
| dcAntiRollFront | electronics.anti_roll_front | |
| dcAntiRollRear | electronics.anti_roll_rear | |
| DRS_Status | electronics.drs_status | |
| dcThrottleShape | electronics.throttle_shape | |
| PushToPass | electronics.push_to_pass_status | |

### Per-Car Arrays (13)
| iRacing Variable | Model Path | Notes |
|---|---|---|
| CarIdxLap | competitors[].current_lap | |
| CarIdxLapCompleted | competitors[].laps_completed | |
| CarIdxLapDistPct | competitors[].lap_distance_pct | |
| CarIdxPosition | competitors[].position | |
| CarIdxClassPosition | competitors[].class_position | |
| CarIdxOnPitRoad | competitors[].on_pit_road | |
| CarIdxTrackSurface | competitors[].track_surface | |
| CarIdxBestLapTime | competitors[].best_lap_time | |
| CarIdxLastLapTime | competitors[].last_lap_time | |
| CarIdxEstTime | competitors[].est_time | |
| CarIdxGear | competitors[].gear | |
| CarIdxRPM | competitors[].rpm | |
| CarIdxSteer | competitors[].steering_angle | |

### Other (1)
| iRacing Variable | Model Path | Notes |
|---|---|---|
| SessionTick | tick | server tick counter |

## Unmapped Variables — Extras (108)

These are forwarded to `extras` with an `iracing/` prefix. Categorized by
likely purpose for future model promotion decisions.

### Steering Wheel Feedback (7)
| Variable | Type | Description |
|---|---|---|
| SteeringWheelMaxForceNm | f32 | Maximum force feedback force (Nm) |
| SteeringWheelTorque_ST | f32 | Steering torque short-term |
| SteeringWheelPctTorqueSign | f32 | Signed torque percentage |
| SteeringWheelPctTorqueSignStops | f32 | Signed torque % with stops |
| SteeringWheelPctDamper | f32 | Damper effect percentage |
| SteeringWheelPctSmoothing | f32 | Smoothing percentage |
| SteeringWheelLimiter | bool | Force feedback limiter active |
| SteeringWheelUseLinear | bool | Linear mode enabled |

### Raw/Unfiltered Pedal Inputs (3)
| Variable | Type | Description |
|---|---|---|
| BrakeRaw | f32 | Raw brake pedal position (pre-calibration) |
| ThrottleRaw | f32 | Raw throttle pedal position |
| ClutchRaw | f32 | Raw clutch pedal position |

### Extended Tire Data (per-wheel) (8)
| Variable | Type | Description |
|---|---|---|
| LFpressure / RFpressure / LRpressure / RRpressure | f32 | Current tire pressure (alternate reading) |
| LFtempCM / RFtempCM / LRtempCM / RRtempCM | f32 | Carcass middle temperature |

### Tire Sets Management (12)
| Variable | Type | Description |
|---|---|---|
| TireSetsAvailable / TireSetsUsed | i32 | Total tire set counts |
| FrontTireSetsAvailable / FrontTireSetsUsed | i32 | Front tire sets |
| RearTireSetsAvailable / RearTireSetsUsed | i32 | Rear tire sets |
| LeftTireSetsAvailable / LeftTireSetsUsed | i32 | Left tire sets |
| RightTireSetsAvailable / RightTireSetsUsed | i32 | Right tire sets |
| LFTiresAvailable / LFTiresUsed | i32 | LF individual |
| RFTiresAvailable / RFTiresUsed | i32 | RF individual |
| LRTiresAvailable / LRTiresUsed | i32 | LR individual |
| RRTiresAvailable / RRTiresUsed | i32 | RR individual |

### Tire Rumble (4)
| Variable | Type | Description |
|---|---|---|
| TireLF_RumblePitch | f32 | LF tire rumble strip vibration pitch |
| TireRF_RumblePitch | f32 | RF tire rumble strip vibration pitch |
| TireLR_RumblePitch | f32 | LR tire rumble strip vibration pitch |
| TireRR_RumblePitch | f32 | RR tire rumble strip vibration pitch |

### Lap Timing Extended (12)
| Variable | Type | Description |
|---|---|---|
| LapDeltaToBestLap_DD | f32 | Delta to best lap (rate of change) |
| LapDeltaToOptimalLap_DD | f32 | Delta to optimal (rate of change) |
| LapDeltaToSessionBestLap_DD | f32 | Delta to session best (rate of change) |
| LapDeltaToSessionLastlLap | f32 | Delta to session last lap |
| LapDeltaToSessionLastlLap_DD | f32 | Delta to session last (rate of change) |
| LapDeltaToSessionLastlLap_OK | bool | Session last delta valid |
| LapDeltaToSessionOptimalLap | f32 | Delta to session optimal |
| LapDeltaToSessionOptimalLap_DD | f32 | Delta to session optimal (rate) |
| LapDeltaToSessionOptimalLap_OK | bool | Session optimal delta valid |
| LapBestLap | i32 | Lap number of best lap |
| LapLastNLapTime | f32 | Last N-lap average time |
| LapLasNLapSeq | i32 | Last N-lap sequence number |

### Network / Performance (10)
| Variable | Type | Description |
|---|---|---|
| CpuUsageFG | f32 | CPU usage foreground thread |
| CpuUsageBG | f32 | CPU usage background thread |
| GpuUsage | f32 | GPU usage |
| FrameRate | f32 | Render frame rate |
| MemPageFaultSec | f32 | Memory page faults/sec |
| MemSoftPageFaultSec | f32 | Soft page faults/sec |
| ChanLatency | f32 | Network channel latency |
| ChanAvgLatency | f32 | Network average latency |
| ChanClockSkew | f32 | Network clock skew |
| ChanQuality | f32 | Network quality |
| ChanPartnerQuality | f32 | Partner connection quality |

### Player Car Metadata (13)
| Variable | Type | Description |
|---|---|---|
| PlayerCarIdx | i32 | Player's car index in session |
| PlayerCarClass | i32 | Player car class ID |
| PlayerCarDriverIncidentCount | i32 | Driver incident count |
| PlayerCarMyIncidentCount | i32 | Player's incident count |
| PlayerCarTeamIncidentCount | i32 | Team incident count |
| PlayerCarWeightPenalty | f32 | BOP weight penalty (kg) |
| PlayerCarPowerAdjust | f32 | BOP power adjustment (%) |
| PlayerCarDryTireSetLimit | i32 | Dry tire set allocation |
| PlayerCarTowTime | f32 | Tow time remaining (s) |
| PlayerCarInPitStall | bool | Car is in pit stall |
| PlayerTireCompound | i32 | Current tire compound |
| PlayerFastRepairsUsed | i32 | Fast repairs used |
| PlayerTrackSurfaceMaterial | i32 | Track surface material enum |

### Pit Service Details (8)
| Variable | Type | Description |
|---|---|---|
| PitSvFlags | i32 | Pit service flags bitfield |
| PitSvFuel | f32 | Pit fuel to add (L) |
| PitSvLFP / PitSvRFP / PitSvLRP / PitSvRRP | f32 | Pit tire pressures (kPa) |
| PitSvTireCompound | i32 | Pit tire compound selection |
| PitsOpen | bool | Pit lane is open |

### Location / Misc Spatial (4)
| Variable | Type | Description |
|---|---|---|
| YawNorth | f32 | Yaw relative to true north (rad) |
| SolarAltitude | f32 | Sun altitude angle (rad) |
| SolarAzimuth | f32 | Sun azimuth angle (rad) |
| CFSRrideHeight | f32 | Center front splitter ride height |

### Session State (7)
| Variable | Type | Description |
|---|---|---|
| SessionUniqueID | i32 | Unique session ID |
| SessionLapsTotal | i32 | Total session laps |
| SessionLapsRemain | i32 | Remaining laps |
| SessionTimeTotal | f64 | Total session time (s) |
| SessionOnJokerLap | bool | On joker/shortcut lap |
| SessionJokerLapsRemain | i32 | Joker laps remaining |
| PaceMode | i32 | Pace car mode |

### Performance / Shift Indicators (2)
| Variable | Type | Description |
|---|---|---|
| ShiftPowerPct | f32 | Shift for max power percentage |
| ShiftGrindRPM | f32 | RPM at which gear grind occurs |

### Controls / Miscellaneous (8)
| Variable | Type | Description |
|---|---|---|
| Engine0_RPM | f32 | Engine 0 RPM (alternate reading) |
| DriverMarker | bool | Driver marker/flag button active |
| ManualBoost | bool | Manual boost override |
| ManualNoBoost | bool | Manual no-boost override |
| EnterExitReset | i32 | Enter/exit/reset state |
| IsOnTrackCar | bool | Car is on track (alternate) |
| PushToTalk | bool | Push-to-talk radio active |
| dcDashPage | i32 | Dashboard page selection |
| dcHeadlightFlash | bool | Headlight flash activated |
| dcPitSpeedLimiterToggle | bool | Pit speed limiter toggled |
| dcStarter | bool | Engine starter activated |
| dcTractionControlToggle | bool | TC toggle button pressed |
| WeatherType | i32 | Weather type enum |

## Session Info Extras (7)

These are extracted from session info YAML (not telemetry tick data):

| Variable | Description |
|---|---|
| iRating | Player's iRating |
| LicenseLevel | License level (Rookie, D, C, B, A, Pro) |
| LicenseSubLevel | License sub-level (safety rating) |
| LicenseString | Full license string |
| SeriesID | Series identifier |
| SessionID | Session identifier |
| SubSessionID | Sub-session identifier |
