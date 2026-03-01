# API & Data Model Changelog

Breaking changes and migration notes for consumers of the OpenSimTelemetry API (SSE, REST, UDP sinks).

## Unreleased

### Data Model Redesign

The telemetry data model was completely redesigned for comprehensive iRacing coverage. If you were consuming the original model, all field paths have changed. The current model is structured as:

```
TelemetryFrame
  motion.*        — position, velocity, acceleration, g_force, rotation, pitch/yaw/roll_rate
  vehicle.*       — speed, rpm, gear, throttle, brake, clutch, steering_angle, fuel
  engine.*        — oil/water temp & pressure, fuel pressure, manifold pressure, warnings
  wheels.*        — per-corner (front_left, front_right, rear_left, rear_right):
                    tire temp/pressure/wear, shock velocity, wheel_speed, brake_temp, slip
  timing.*        — lap times, lap number, deltas
  session.*       — track/car names, session type, flags
  weather.*       — air/track temp, wind, humidity
  pit.*           — pit status, services, speed limit
  electronics.*   — ABS, traction control, DRS
  damage.*        — body/engine/suspension damage
  extras.*        — adapter-specific fields not in the standard model
```

### Field Renames

| Before | After | Notes |
|--------|-------|-------|
| `FieldMask` | `MetricMask` | Rust type rename |
| `field_mask` | `metric_mask` | JSON field and query parameter |
| `?fields=` | `?metric_mask=` | Query parameter on all endpoints returning frames |
| `angular_velocity.x` | `pitch_rate` | Was a `Vector3`, now individual top-level fields |
| `angular_velocity.y` | `yaw_rate` | |
| `angular_velocity.z` | `roll_rate` | |
| `Suspension` (widget) | `Wheels` | Widget renamed; data path unchanged |

### Unit Changes

| Field | Before | After | Scope |
|-------|--------|-------|-------|
| All angle fields (yaw, pitch, roll, steering_angle, etc.) | Radians | Degrees | API responses |
| `pitch_rate`, `yaw_rate`, `roll_rate` | rad/s (as `angular_velocity` Vector3) | deg/s | API responses |
| `wheels.*.wheel_speed` | deg/s (`DegreesPerSecond`) | RPM (`Rpm`) | API responses |
| `vehicle.speed` | m/s | m/s (unchanged) | UI displays as km/h but API remains m/s |

### Promoted Fields (extras to standard model)

Several iRacing-specific metrics were promoted from `extras.*` to the standard data model:

- `motion.position` — world-space position (x, y, z)
- `wheels.*.tyre_wear_inner/middle/outer` — per-tread wear zones
- `weather.track_surface_temp` — track surface temperature
- Various engine metrics previously in extras

If you were reading these from `extras.*`, update your paths to the standard model fields. The IBT parser now also forwards **all** unmapped iRacing variables to `extras`, so any iRacing var not in the standard model is accessible via `extras.<varName>`.

### API Endpoint Changes

#### SSE Consolidation

Three separate SSE connections were consolidated into one:

| Before | After |
|--------|-------|
| `/api/telemetry/stream` | `/api/stream` (unified) |
| `/api/status/stream` | `/api/stream` (unified) |
| `/api/sinks/stream` | `/api/stream` (unified) |

The unified stream sends typed events (telemetry frames, status updates, sink config changes) over a single connection.

#### New Parameter: `rate`

All SSE telemetry endpoints now accept a `rate` query parameter for client-side throttling:

```
GET /api/stream?rate=10    # 10 updates per second
```

#### New Endpoints

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/api/history/config` | GET/POST | Server-side history buffer configuration |
| `/api/persistence/*` | Various | Saved replay management (NDJSON+ZSTD files) |
| `/api/replay/info` | GET | Replay metadata including history mode info |
| `/api/convert/ibt` | POST | Upload .ibt file, streams back ZSTD-compressed NDJSON |

#### Removed Endpoints

HTTP and file sink endpoints were removed. Only UDP sinks remain, configurable with an update rate option.

#### Replay Upload

The replay upload endpoint body size limit was raised to 512MB. Blocking I/O was moved off the async runtime, so uploads no longer hang under load.

### Authentication (New)

Authentication is optional and off by default. When enabled:

- **Bearer token**: `Authorization: Bearer <token>` header or `?token=<token>` query parameter
- **HTTP Basic**: Standard browser login prompt, useful for accessing the web UI in a browser

### Float Precision

All float values in API responses are rounded to 5 decimal places to reduce payload size and avoid floating-point noise.

### Extras Field Matching

The `metric_mask` filter for `extras.*` fields is now **case-insensitive**, fixing issues where iRacing variable names with mixed casing were not matched.
