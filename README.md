# 🏎️ OpenSimTelemetry

The goal of this project is to enable easier prototyping and experimentation with telemetry data from simulators, by providing easy access to real-time telemetry via HTTP/UDP. Single Rust binary that runs on Windows, macOS, and Linux, however onl

## State

This project is in the prototype phase, and only Windows / iRacing is being tested at the moment.

## Features

- **🔌 Auto-Detection** - Automatically detects running games and starts collecting data
- **🌐 Web UI** - Dashboard for monitoring live telemetry and configuration
- **📡 Multiple Outputs** - Stream data via HTTP, UDP
- **🎯 Field Selection** - Choose which fields to transmit to minimize bandwidth
- **🦀 Performant** - Written in Rust for reliability and performance

## Supported Games

### Current
- **iRacing** - Full support via shared memory (Windows only)
- **Demo Adapter** - Generates synthetic telemetry for testing

### Planned
- Assetto Corsa
- Assetto Corsa Competizione
- F1 2024
- rFactor 2
- Automobilista 2
- beamNG.drive

## Quick Start

### Installation

```bash
# Clone the repository
git clone https://github.com/yourusername/opensimtelemetry
cd opensimtelemetry

# Build the project
cargo build --release

# Run the server
cargo run --release
```

The server will start on `http://localhost:9100`

### Using the Web UI

1. Open your browser to `http://localhost:9100`
2. The demo adapter will automatically be active
3. You'll see live telemetry data updating at 60 Hz
4. Add output sinks to forward data to other applications

### API Usage

#### List Adapters

```bash
curl http://localhost:9100/api/adapters
```

Response:
```json
[
  {
    "name": "Demo",
    "detected": true,
    "active": true
  }
]
```

#### Stream Telemetry (SSE)

```bash
curl http://localhost:9100/api/telemetry/stream
```

**Field Filtering:**

To reduce bandwidth, specify which fields you need:

```bash
# Only RPM, speed, and gear
curl "http://localhost:9100/api/telemetry/stream?fields=rpm,speed,gear"

# Include G-forces and lap time
curl "http://localhost:9100/api/telemetry/stream?fields=g_force,current_lap_time,speed,rpm"
```

#### Create Output Sink

**HTTP POST:**
```bash
curl -X POST http://localhost:9100/api/sinks \
  -H "Content-Type: application/json" \
  -d '{
    "id": "motion-sim",
    "sink_type": {"type": "http", "url": "http://localhost:8080/telemetry"},
    "field_mask": "g_force,velocity,acceleration"
  }'
```

**UDP:**
```bash
curl -X POST http://localhost:9100/api/sinks \
  -H "Content-Type: application/json" \
  -d '{
    "id": "udp-sink",
    "sink_type": {"type": "udp", "host": "127.0.0.1", "port": 9200},
    "field_mask": "rpm,speed,gear,throttle,brake"
  }'
```

**File (NDJSON):**
```bash
curl -X POST http://localhost:9100/api/sinks \
  -H "Content-Type: application/json" \
  -d '{
    "id": "file-logger",
    "sink_type": {"type": "file", "path": "/tmp/telemetry.ndjson"},
    "field_mask": null
  }'
```

#### List Sinks

```bash
curl http://localhost:9100/api/sinks
```

#### Delete Sink

```bash
curl -X DELETE http://localhost:9100/api/sinks/motion-sim
```

## Telemetry Data Model

The unified telemetry frame includes:

### Motion Data
- Position (world space)
- Velocity (car-local)
- Acceleration (car-local)
- G-forces (lateral, longitudinal, vertical)
- Rotation (pitch, yaw, roll)
- Angular velocity & acceleration

### Vehicle State
- Speed
- RPM
- Gear (current & max)
- Throttle, brake, clutch, steering inputs
- Engine temperature
- Fuel level & capacity

### Wheel Data (per wheel: FL, FR, RL, RR)
- Suspension travel
- Tyre pressure
- Tyre temperature (surface & inner)
- Tyre wear
- Slip ratio & slip angle
- Vertical load
- Rotation speed

### Lap Timing
- Current lap time
- Last lap time
- Best lap time
- Sector times
- Lap number
- Race position

### Session Info
- Session type (practice, qualifying, race, etc.)
- Time remaining
- Track temperature
- Air temperature
- Track name
- Car name
- Flag status

### Damage
- Per-panel damage levels
- Engine damage
- Transmission damage

## iRacing Adapter Details

The iRacing adapter connects to iRacing via shared memory and provides comprehensive telemetry data:

### Available Data
- **Motion**: Velocity, acceleration, G-forces (lateral, longitudinal, vertical)
- **Rotation**: Pitch, yaw, roll, and angular velocities
- **Vehicle**: RPM, gear, throttle, brake, clutch, steering angle
- **Engine**: Water temperature, fuel level
- **Wheels**: Per-wheel data including suspension deflection, tire pressure, temperature (inner/surface), wear, rotation speed
- **Lap Timing**: Current/last/best lap times, lap number
- **Session**: Time remaining, track temperature, air temperature, race position
- **Flags**: Basic flag status (green, yellow, checkered)

### Requirements
- **Windows only** (iRacing shared memory is Windows-specific)
- **iRacing must be running** with a session active
- Uses the [iracing.rs](https://github.com/leoadamek/iracing.rs) library

### Notes
- Data updates at iRacing's telemetry rate (~60 Hz)
- Position data not available (iRacing doesn't expose world coordinates)
- Session info (track name, car name) could be added via session data API
- The adapter automatically detects when iRacing starts/stops

## Architecture

```
┌─────────────┐
│   Games     │  Assetto Corsa, iRacing, F1, etc.
└─────┬───────┘
      │ (UDP/Shared Memory)
┌─────▼──────────────────────────────┐
│        ost-adapters                │  Game-specific parsers
│  ┌──────┐  ┌──────┐  ┌──────┐    │
│  │ Demo │  │  AC  │  │iRacing│... │
│  └──────┘  └──────┘  └──────┘    │
└─────┬──────────────────────────────┘
      │
┌─────▼──────────────────────────────┐
│        ost-core                    │  Unified data model
│   TelemetryFrame + FieldMask      │
└─────┬──────────────────────────────┘
      │
┌─────▼──────────────────────────────┐
│        ost-server                  │  Server with API & Web UI
│  ┌─────────┐  ┌──────────┐       │
│  │ Manager │──│Broadcast │       │
│  └─────────┘  │ Channel  │       │
│               └────┬─────┘       │
│   ┌──────────┐    │  ┌─────────┐ │
│   │  API     │◄───┴─►│ Web UI  │ │
│   │  (REST)  │       │  (SSE)  │ │
│   └────┬─────┘       └─────────┘ │
└────────┼──────────────────────────┘
         │
    ┌────▼────┐
    │  Sinks  │  Output destinations
    └─────────┘
```

## Development

### Project Structure

```
OpenSimTelemetry/
├── ost-core/           # Core library with data model
│   ├── src/
│   │   ├── adapter.rs  # TelemetryAdapter trait
│   │   ├── model.rs    # TelemetryFrame data structure
│   │   └── units.rs    # Type-safe units
│   └── Cargo.toml
├── ost-adapters/       # Game-specific adapters
│   ├── src/
│   │   ├── demo.rs     # Demo adapter
│   │   └── ...         # Future game adapters
│   └── Cargo.toml
├── ost-server/         # Server application
│   ├── src/
│   │   ├── main.rs     # Entry point
│   │   ├── state.rs    # Shared state
│   │   ├── manager.rs  # Adapter lifecycle
│   │   ├── api.rs      # REST API
│   │   ├── web_ui.rs   # Embedded UI
│   │   ├── sinks.rs    # Output sinks
│   │   └── ui.html     # Web interface
│   └── Cargo.toml
└── Cargo.toml          # Workspace root
```

### Adding a New Adapter

1. Create a new file in `ost-adapters/src/` (e.g., `iracing.rs`)
2. Implement the `TelemetryAdapter` trait:

```rust
use ost_core::adapter::TelemetryAdapter;
use ost_core::model::TelemetryFrame;

pub struct IRacingAdapter {
    active: bool,
    // ... adapter-specific fields
}

impl TelemetryAdapter for IRacingAdapter {
    fn name(&self) -> &str {
        "iRacing"
    }

    fn detect(&self) -> bool {
        // Check if iRacing is running
        // e.g., check for shared memory region
        false
    }

    fn start(&mut self) -> Result<()> {
        // Open shared memory or UDP socket
        self.active = true;
        Ok(())
    }

    fn stop(&mut self) -> Result<()> {
        // Clean up resources
        self.active = false;
        Ok(())
    }

    fn read_frame(&mut self) -> Result<Option<TelemetryFrame>> {
        // Read from game and convert to TelemetryFrame
        if !self.active {
            return Ok(None);
        }
        // ... read and convert data
        Ok(Some(frame))
    }

    fn is_active(&self) -> bool {
        self.active
    }
}
```

3. Register the adapter in `ost-server/src/manager.rs`:

```rust
state.register_adapter(Box::new(IRacingAdapter::new())).await;
```

### Building

```bash
# Build all crates
cargo build

# Build in release mode
cargo build --release

# Build specific crate
cargo build -p ost-core
cargo build -p ost-adapters
cargo build -p ost-server

# Run tests
cargo test

# Run with logging
RUST_LOG=info cargo run
```

### Cross-Platform Builds

**Automated Releases:**
GitHub Actions automatically builds for all platforms when you push a tag:

```bash
git tag v0.1.0
git push origin v0.1.0
```

This creates release binaries for:
- Windows (x64) - with full iRacing support
- macOS (Intel x64 & Apple Silicon ARM64)
- Linux (x64)

**Local Cross-Compilation (macOS to Windows):**

This project uses [`cargo-xwin`](https://github.com/rust-cross/cargo-xwin) for cross-compiling
to Windows from macOS. It automatically downloads the Windows SDK on first use.
Requires Rust installed via [rustup](https://rustup.rs/) (not Homebrew) for target management.

```bash
# One-time setup
just setup-cross

# Build Windows binary
just build-windows
```

The binary will be at `target/x86_64-pc-windows-msvc/release/ost-server.exe`.

Without `just`:
```bash
rustup target add x86_64-pc-windows-msvc
cargo install cargo-xwin
cargo xwin build --release --target x86_64-pc-windows-msvc
```

### Testing

```bash
# Run all tests
cargo test

# Run tests for specific crate
cargo test -p ost-core

# Run with output
cargo test -- --nocapture
```

## Field Filtering

Field filtering minimizes bandwidth by only transmitting requested fields.

### Available Fields

**Motion:** `position`, `velocity`, `acceleration`, `g_force`, `rotation`, `angular_velocity`, `angular_acceleration`

**Vehicle:** `speed`, `rpm`, `gear`, `max_gears`, `throttle`, `brake`, `clutch`, `steering`, `engine_temp`, `fuel_level`, `fuel_capacity`

**Wheels:** `wheels` (includes all per-wheel data)

**Timing:** `current_lap_time`, `last_lap_time`, `best_lap_time`, `sector_times`, `lap_number`, `race_position`, `num_cars`

**Session:** `session_type`, `session_time_remaining`, `track_temp`, `air_temp`, `track_name`, `car_name`, `flag`

**Damage:** `damage` (includes all damage data)

**Extras:** `extras` (game-specific fields)

### Bandwidth Comparison

**Full frame:** ~2-3 KB per frame @ 60 Hz = **120-180 KB/s**

**Filtered (rpm, speed, gear, throttle, brake):** ~200 bytes per frame @ 60 Hz = **12 KB/s** (90% reduction)

**G-forces only:** ~100 bytes per frame @ 60 Hz = **6 KB/s** (95% reduction)

## Use Cases

### Motion Simulator
Connect your motion platform to receive G-force and acceleration data:
```bash
curl -X POST http://localhost:9100/api/sinks \
  -H "Content-Type: application/json" \
  -d '{
    "id": "motion-platform",
    "sink_type": {"type": "udp", "host": "192.168.1.100", "port": 9200},
    "field_mask": "g_force,acceleration,velocity,speed"
  }'
```

### Telemetry Overlay
Stream minimal data for an on-screen overlay:
```bash
curl "http://localhost:9100/api/telemetry/stream?fields=rpm,speed,gear,current_lap_time,best_lap_time"
```

### Data Logging
Record complete telemetry for analysis:
```bash
curl -X POST http://localhost:9100/api/sinks \
  -H "Content-Type: application/json" \
  -d '{
    "id": "logger",
    "sink_type": {"type": "file", "path": "/home/user/telemetry/session.ndjson"}
  }'
```

### Race Analysis Tool
Forward telemetry to your analysis application:
```bash
curl -X POST http://localhost:9100/api/sinks \
  -H "Content-Type: application/json" \
  -d '{
    "id": "analyzer",
    "sink_type": {"type": "http", "url": "http://localhost:3000/api/telemetry"}
  }'
```

## Coordinate System

OpenSimTelemetry uses a **right-handed, car-local** coordinate system:

- **X-axis**: Right (positive = right side of car)
- **Y-axis**: Up (positive = upward)
- **Z-axis**: Forward (positive = forward direction)

### G-Force Interpretation
- **Lateral (X)**: Positive = right turn, Negative = left turn
- **Vertical (Y)**: Negative = compression (downforce), Positive = lift
- **Longitudinal (Z)**: Positive = acceleration, Negative = braking

## License

Licensed under the Apache License, Version 2.0. See [LICENSE](LICENSE) for details.

Copyright 2026 OpenSimTelemetry Contributors

## Contributing

Contributions are welcome! Please see [CONTRIBUTING.md](CONTRIBUTING.md) for guidelines.

### Wanted: Game Adapters

We're looking for contributors to implement adapters for:
- Assetto Corsa
- Assetto Corsa Competizione
- iRacing
- F1 series
- rFactor 2
- Automobilista 2
- BeamNG.drive
- Richard Burns Rally

## Acknowledgments

Inspired by the need for better interoperability in the sim racing ecosystem. Special thanks to the developers of [CrewChiefV4](https://github.com/mrbelowski/CrewChiefV4) and [SimHub](https://www.simhubdash.com/) for their pioneering work in unified telemetry.
