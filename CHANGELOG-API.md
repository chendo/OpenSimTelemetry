# API & Data Model Changelog

Breaking changes and migration notes for consumers of the OpenSimTelemetry API (SSE, REST, UDP sinks).

## 0.4.0 — 2026-06-24

### `ost-parse` Wire Format — `--mode compact` now carries string channels (BREAKING)

- **Compact frames are no longer numeric-only.** `--mode compact` previously
  dropped every string channel: `SessionHeader.channels` listed only numeric
  channels and the positional frame arrays contained only numbers. A JSON array
  can hold mixed types, so string columns now ride along positionally.
  - `SessionHeader.channels` in compact mode is now the **full** channel union
    (numeric **and** string), in the same sorted discovery order as
    sparse/dense. It is identical to `channels` in the other modes.
  - Each compact frame array is positionally aligned with `header.channels`.
    String columns occupy their slot like any other; a column not yet seen is
    `null` (numeric columns still default to `0`), and both carry their last
    value forward when absent from a tick.
  - **Migration:** consumers that index compact arrays by `header.channels`
    order keep working, but **must stop assuming every slot is a number** —
    some slots are now JSON strings (or `null`). Readers that coerce
    non-numeric positions to `0` must be updated to pass strings through.

### `vehicle.track_surface` is now a track *location*, not a material (BREAKING)

- **`vehicle.track_surface` / `competitors[].track_surface` changed meaning and
  values.** The field was sourced from iRacing's `PlayerTrackSurface` (an
  `irsdk_TrkLoc` *location*) but decoded through the *material* table, so values
  were wrong (e.g. `OnTrack` rendered as `"Asphalt"`, `OffTrack` as
  `"Undefined"`). It now serializes as a normalized location enum **string**:
  `"NotInWorld"`, `"OffTrack"`, `"InPitStall"`, `"ApproachingPits"`,
  `"OnTrack"`, or `"Unknown"`.
  - **Migration:** consumers should read the location strings above. For
    off-track detection, iRacing's raw numeric code remains available under the
    game namespace at `iracing.PlayerTrackSurface` (`0` = OffTrack … `3` =
    OnTrack), which — being numeric — also survives `--mode compact`. The
    surface *material* (Asphalt/Grass/…) is likewise available raw at
    `iracing.PlayerTrackSurfaceMaterial`.

> Note: iRacing-specific signals such as the running incident count
> (`PlayerCarMyIncidentCount`) are not normalized into the cross-sim model;
> they remain available raw under the `iracing.` namespace (e.g.
> `iracing.PlayerCarMyIncidentCount`, which a consumer can edge-detect).

## 0.3.0 — 2026-04-12

### `ost-parse` Wire Format

- **`SessionHeader.laps` and `track_outline` are now nullable.** When `--stream` is used (or the `stream` parse option is set), these fields are omitted from the JSON header (`null`). Consumers that previously assumed arrays should handle the absent case. `channels` is always populated.
- **`ost-cli parse --stream`** — new flag that skips lap index and track outline computation. Channels are derived from the first frame and always included in the header.
- **`ParseOptions.stream`** — new boolean field (default `false`) available to library consumers and the `POST /api/parse` endpoint.

## 0.2.0 — 2026-04-11

First tagged release. The sections below describe the API & data model
surface as of 0.2.0; "Breaking" tags refer to changes from the
pre-release codebase. Subsequent releases will list deltas from 0.2.0.

### Columnar Telemetry Endpoint (New)

New endpoint for fetching telemetry data in columnar (channel-per-array) format, optimized for charting, analysis, and data export.

**Endpoint:** `GET /api/telemetry/columns`

**Query parameters:**

| Parameter | Required | Default | Description |
|-----------|----------|---------|-------------|
| `channels` | Yes | — | Comma-separated channel patterns (see below) |
| `source` | No | `history` | Data source: `history` or `replay` |
| `duration` | No | `60s` | Time window for history source (e.g., `60s`, `5m`, `1h`) |
| `start` | No | — | 0-based frame index (overrides `duration` for history) |
| `count` | No | 7200 | Max frames to return (capped at 7200) |

**Channel pattern syntax:**

- **Literal:** `vehicle.speed` — exact path match
- **Prefix:** `vehicle` — matches all paths under `vehicle.*`
- **Glob:** `vehicle.*` (one segment), `wheels.**` (any depth)
- **Regex:** `/engine\..*/` — slash-delimited regex against full path

**Response format:**

```json
{
  "meta": {
    "frame_count": 3600,
    "channels": ["vehicle.speed", "engine.rpm"],
    "first_tick": 1000,
    "last_tick": 4600
  },
  "columns": {
    "meta.tick": [1000, 1001, 1002, ...],
    "meta.timestamp": ["2026-04-04T12:00:00Z", ...],
    "vehicle.speed": [45.2, 45.8, 46.1, ...],
    "engine.rpm": [6500, 6800, 7100, ...]
  }
}
```

- `meta.tick` and `meta.timestamp` are always included as built-in columns
- Missing values are `null`
- Values preserve their original type (number, string, boolean)

### "Metrics" renamed to "Channels" (Breaking)

All API endpoints, query parameters, and request/response fields using "metrics" have been renamed to "channels" to align with standard telemetry software terminology.

**Endpoint renames:**

| Before | After |
|--------|-------|
| `GET /api/metrics` | `GET /api/channels` |
| `POST /api/metrics` | `POST /api/channels` |
| `GET /api/metrics/custom` | `GET /api/channels/custom` |
| `DELETE /api/metrics/custom` | `DELETE /api/channels/custom` |
| `DELETE /api/metrics/custom/:namespace` | `DELETE /api/channels/custom/:namespace` |

**Query parameter renames:**

| Before | After | Affected endpoints |
|--------|-------|--------------------|
| `metric_mask` | `channel_mask` | `/api/stream`, `/api/telemetry/stream`, `/api/replay/frames` |
| `metrics` | `channels` | `/api/history/aggregate` |

**Request body field renames:**

| Before | After | Endpoint |
|--------|-------|----------|
| `"metrics": {...}` | `"channels": {...}` | `POST /api/channels` |

**Rust type renames** (for library consumers):

| Before | After |
|--------|-------|
| `MetricMask` | `ChannelMask` |
| `MetricMaskBuilder` | `ChannelMaskBuilder` |
| `CustomMetrics` | `CustomChannels` |

**UDP sink config:** The `metric_mask` field in sink configuration is now `channel_mask`.

### Namespace Refactor (Breaking)

Three structural changes to the telemetry frame layout:

**1. `motion.rotation` flattened to individual fields**

| Before | After |
|--------|-------|
| `motion.rotation.x` | `motion.pitch` |
| `motion.rotation.y` | `motion.yaw` |
| `motion.rotation.z` | `motion.roll` |
| `motion.angular_acceleration.*` | Removed (was always null) |

`motion.pitch` and `motion.roll` are car body tilt angles in degrees. `motion.yaw` is the raw track-relative yaw (degrees) — for compass heading use `motion.heading` (0-360°, 0=N).

**2. `electronics` merged into `vehicle`**

| Before | After |
|--------|-------|
| `electronics.abs` | `vehicle.abs` |
| `electronics.abs_active` | `vehicle.abs_active` |
| `electronics.traction_control` | `vehicle.traction_control` |
| `electronics.brake_bias` | `vehicle.brake_bias` |
| `electronics.drs_status` | `vehicle.drs_status` |
| `electronics.shift_light_*` | `vehicle.shift_light_*` |
| ... (all other electronics fields) | `vehicle.*` |

The `electronics` namespace no longer exists. All fields are now under `vehicle`.

**3. `driver` and `competitors` merged into `drivers`**

| Before | After |
|--------|-------|
| `driver.name` | `drivers.current.name` |
| `driver.car_index` | `drivers.current.car_index` |
| `driver.car_number` | `drivers.current.car_number` |
| `driver.team_name` | `drivers.current.team_name` |
| `driver.estimated_lap_time` | `drivers.current.estimated_lap_time` |
| `competitors[n].*` | `drivers.competitors[n].*` |

**Updated frame structure:**

```
TelemetryFrame
  motion.*        — velocity, acceleration, g_force, pitch/roll/yaw, heading, pitch/yaw/roll_rate,
                    latitude, longitude, altitude
  vehicle.*       — speed, rpm, gear, inputs, steering, ABS, TC, brake bias, DRS, shift lights
  engine.*        — oil/water temp & pressure, fuel, voltage, warnings
  wheels.*        — per-corner: tire temp/pressure/wear, suspension, brake, slip
  timing.*        — lap times, positions, deltas
  session.*       — track/car names, session type, flags
  weather.*       — air/track temp, wind, humidity
  pit.*           — pit status, services, speed limit
  damage.*        — body/engine/transmission damage
  drivers.*       — drivers.current (player info), drivers.competitors[] (other cars)
  extras.*        — adapter-specific fields (flattened to top level in JSON)
```

### API Key Authentication (New)

All `/api/*` endpoints now require authentication. An API key is auto-generated on first boot and persisted to `~/.opensimtelemetry/api_key`.

**Authentication methods** (any one is sufficient):
- `Authorization: Bearer <key>` header
- HTTP Basic auth: `ost:<key>` (username `ost`, password is the API key)
- `?key=<key>` query parameter (for EventSource/SSE which can't set headers)

**Override:** Set `OST_AUTH_TOKEN` env var to use a custom key instead of the auto-generated one.

**UI pages** (`/`, `/s/:id`) are not gated — the API key is injected server-side into the HTML.

**Settings UI:** The API key is shown in Settings with a Copy button and a Regenerate option.

**Key management endpoint:**
- `POST /api/key/reset` — regenerate the API key (returns the new key)

### CORS Policy (Breaking)

`CorsLayer::permissive()` has been replaced with a restrictive CORS policy:
- **Default:** No CORS headers (same-origin only)
- **With `OST_CORS_ORIGINS` env var:** Allows specified origins for API endpoints. Set to a comma-separated list of origins (e.g., `http://localhost:3000,https://myapp.com`)

### Heading Fix (Breaking)

`motion.heading` was incorrectly negated, mirroring east↔west (e.g. NNE appeared as NNW). Now correctly outputs 0-360° compass bearing (0=N, 90=E, 180=S, 270=W) from iRacing's `YawNorth`.

### Performance: IBT Conversion

The `/api/convert/ibt` endpoint is significantly faster:
- JSON serialization skips null fields (`skip_serializing_if`) — reduces output size dramatically
- Direct `serde_json::to_writer` instead of `to_string` + copy
- 256KB buffered writes to the compression stream
- Zstd compression level reduced from 3 to 1 (minimal ratio difference on repetitive JSONL)

---

### Data Model Redesign

The telemetry data model was completely redesigned for comprehensive iRacing coverage. If you were consuming the original model, all field paths have changed. The current model is structured as above.

### Field Renames

| Before | After | Notes |
|--------|-------|-------|
| `FieldMask` | `MetricMask` → `ChannelMask` | Rust type rename |
| `field_mask` | `metric_mask` → `channel_mask` | JSON field and query parameter |
| `?fields=` | `?metric_mask=` → `?channel_mask=` | Query parameter on all endpoints returning frames |
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

Several iRacing-specific channels were promoted from `extras.*` to the standard data model:

- `motion.position` — world-space position (x, y, z)
- `wheels.*.tyre_wear_inner/middle/outer` — per-tread wear zones
- `weather.track_surface_temp` — track surface temperature
- Various engine channels previously in extras

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

The `channel_mask` filter for `extras.*` fields is now **case-insensitive**, fixing issues where iRacing variable names with mixed casing were not matched.
