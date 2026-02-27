# OpenSimTelemetry

A unified telemetry server for sim racing. Single Rust binary that connects to your sim, serves a real-time web dashboard, and streams data via HTTP/SSE/UDP for motion platforms, overlays, and analysis tools.

## Getting Started

Download the latest binary for your platform from [Releases](https://github.com/chendo/OpenSimTelemetry/releases), then run it:

```bash
./ost-server
```

Open `http://localhost:9100` in your browser to access the dashboard.

On Windows with iRacing running, telemetry is detected and streamed automatically. On other platforms, enable the Demo adapter from the Sources menu to see synthetic data.

## Features

### Web Dashboard
- Configurable grid of widgets: vehicle data, G-force visualisation, wheels, lap timing, session info
- Graph widgets with preset and custom metrics, time windowing, crosshair tooltips
- Custom metric plotting from any field including game-specific extras (boolean and numeric)
- Drag-and-drop layout with persistent save/restore

### Replay System
- Load iRacing `.ibt` telemetry files via drag-and-drop or file picker
- Frame-accurate playback at variable speeds (0.25x to 4x)
- Seek slider with lap markers and lap navigation dropdown
- Loop markers: set start/end points and loop playback between them
- Graph prefetching with loading indicators for unloaded regions
- Real-time throughput indicator showing playback health

### Streaming & Output
- SSE endpoint (`/api/stream`) for real-time telemetry frames
- Metric filtering to reduce bandwidth (request only the sections you need)
- Output sinks: HTTP POST, UDP, or file (NDJSON) forwarding
- Per-sink metric masks for efficient data routing

### Adapters
- **iRacing** (Windows) — shared memory adapter with full telemetry + all unmapped vars forwarded as extras
- **Demo** — synthetic telemetry generator for testing (enable via Sources menu)

## Supported Games

| Game | Status |
|------|--------|
| iRacing | Supported (Windows) |
| Assetto Corsa | Planned |
| Assetto Corsa Competizione | Planned |
| F1 series | Planned |
| rFactor 2 | Planned |
| Automobilista 2 | Planned |
| BeamNG.drive | Planned |

## API

### Stream Telemetry (SSE)

```bash
# Full telemetry stream
curl http://localhost:9100/api/stream

# Filtered to specific sections
curl "http://localhost:9100/api/stream?fields=vehicle,timing"
```

### Output Sinks

```bash
# UDP sink for motion platform
curl -X POST http://localhost:9100/api/sinks \
  -H "Content-Type: application/json" \
  -d '{
    "id": "motion-platform",
    "sink_type": {"type": "udp", "host": "192.168.1.100", "port": 9200},
    "field_mask": "motion,vehicle"
  }'

# File logger (NDJSON)
curl -X POST http://localhost:9100/api/sinks \
  -H "Content-Type: application/json" \
  -d '{
    "id": "logger",
    "sink_type": {"type": "file", "path": "/tmp/telemetry.ndjson"}
  }'

# List / delete sinks
curl http://localhost:9100/api/sinks
curl -X DELETE http://localhost:9100/api/sinks/motion-platform
```

## Data Model

The unified telemetry frame includes sections for: **motion** (position, velocity, G-forces, rotation), **vehicle** (speed, RPM, gear, pedal inputs), **engine** (temps, fuel, pressure), **wheels** (per-corner: suspension, tyre pressure/temp/wear, slip), **timing** (lap times, sectors, position), **session** (type, track, car, flags), **weather**, **pit**, **electronics**, **damage**, **driver**, and **extras** (game-specific fields passed through as-is).

### Coordinate System

Right-handed, car-local: **X** = right, **Y** = up, **Z** = forward.

G-forces: lateral (+right), vertical (-compression), longitudinal (+acceleration).

## Development

Uses `just` for all build tasks:

```
just check    # Fast compilation check
just build    # Debug build
just test     # Run all workspace tests
just lint     # Clippy lints
just fmt      # Format code
just ci       # Full CI check
just run      # Run server (debug, port 9100)
```

### Architecture

```
ost-core        Data model, adapter trait, units
ost-adapters    Game-specific adapters (iRacing, demo)
ost-server      Axum HTTP/SSE server + embedded web UI
```

The web UI source lives in `ost-server/src/ui/` as separate JS/CSS/HTML files. `build.rs` concatenates them into `src/ui.html` which is embedded at compile time via `include_str!`.

## License

Licensed under the Apache License, Version 2.0. See [LICENSE](LICENSE) for details.
