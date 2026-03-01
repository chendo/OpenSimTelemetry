# Adapter Developer Guide

This guide walks through creating a new telemetry adapter for OpenSimTelemetry.

## Architecture Overview

```
Your Adapter (ost-adapters)
    │
    ▼
TelemetryAdapter trait (ost-core)
    │
    ▼
Server manager loop (ost-server)
    │
    ▼
Broadcast channel → SSE / Persistence / Sinks
```

Adapters live in the `ost-adapters` crate and implement the `TelemetryAdapter` trait from `ost-core`. The server's manager loop calls `detect()` to find running games, `start()` to initialize, and `read_frame()` at ~60Hz to poll telemetry.

## Step 1: Implement the Trait

Create a new file in `ost-adapters/src/` (e.g., `mygame.rs`):

```rust
use anyhow::Result;
use ost_core::model::*;
use ost_core::units::*;
use ost_core::TelemetryAdapter;
use chrono::Utc;

pub struct MyGameAdapter {
    active: bool,
    // Add game-specific state: shared memory handle, socket, etc.
}

impl MyGameAdapter {
    pub fn new() -> Self {
        Self { active: false }
    }
}

impl TelemetryAdapter for MyGameAdapter {
    fn key(&self) -> &str { "mygame" }
    fn name(&self) -> &str { "My Game" }

    fn detect(&self) -> bool {
        // Check if the game process is running
        // On Windows: check for shared memory or process name
        // On Linux/macOS: check for socket, file, or process
        false
    }

    fn start(&mut self) -> Result<()> {
        // Open shared memory, connect to socket, etc.
        self.active = true;
        Ok(())
    }

    fn stop(&mut self) -> Result<()> {
        // Clean up resources
        self.active = false;
        Ok(())
    }

    fn read_frame(&mut self) -> Result<Option<TelemetryFrame>> {
        if !self.active { return Ok(None); }

        // Read raw data from game
        // Convert to TelemetryFrame
        let frame = TelemetryFrame {
            timestamp: Utc::now(),
            game: "mygame".to_string(),
            tick: Some(0),  // Frame counter from the sim
            motion: Some(MotionData {
                velocity: Some(Vector3::new(
                    MetersPerSecond(0.0),
                    MetersPerSecond(0.0),
                    MetersPerSecond(50.0),  // Forward speed
                )),
                g_force: Some(Vector3::new(
                    GForce(0.1),   // Lateral
                    GForce(1.0),   // Vertical (1g = gravity)
                    GForce(-0.5),  // Longitudinal (braking)
                )),
                // ... other fields default to None
                ..Default::default()  // Won't work — see below
            }),
            vehicle: Some(VehicleData {
                speed: Some(MetersPerSecond(50.0)),
                rpm: Some(Rpm(7500.0)),
                gear: Some(4),
                throttle: Some(Percentage(0.85)),
                brake: Some(Percentage(0.0)),
                ..Default::default()  // Won't work for non-Default types
            }),
            // Tip: easiest approach is to build via serde_json
            ..serde_json::from_value(serde_json::json!({
                "timestamp": Utc::now(),
                "game": "mygame"
            })).unwrap()
        };

        Ok(Some(frame))
    }

    fn is_active(&self) -> bool { self.active }
}
```

**Tip**: Since TelemetryFrame has many optional fields, the easiest way to build partial frames is via JSON deserialization:

```rust
let json = serde_json::json!({
    "timestamp": Utc::now(),
    "game": "mygame",
    "tick": tick_counter,
    "vehicle": {
        "speed": speed_ms,
        "rpm": rpm,
        "gear": gear,
        "throttle": throttle_pct,
        "brake": brake_pct
    },
    "motion": {
        "g_force": { "x": lat_g, "y": vert_g, "z": lon_g }
    },
    "timing": {
        "lap_number": current_lap,
        "current_lap_time": lap_time_secs,
        "last_lap_time": last_lap_secs
    }
});
let frame: TelemetryFrame = serde_json::from_value(json)?;
```

## Step 2: Register the Adapter

In `ost-adapters/src/lib.rs`, add your module and export:

```rust
mod mygame;
pub use mygame::MyGameAdapter;
```

In `ost-server/src/main.rs`, register the adapter (look for the `register_adapter` calls):

```rust
state.register_adapter(Box::new(MyGameAdapter::new())).await;
```

## Step 3: Map Game Variables

### Coordinate System

OST uses a right-handed, car-local coordinate system:
- **X**: Right (positive = right side of car)
- **Y**: Up (positive = up)
- **Z**: Forward (positive = forward direction)

Many sims use different conventions. Common mappings:
- iRacing: X=right, Y=up, Z=forward (matches OST)
- ACC: X=right, Y=up, Z=forward (matches OST)
- rFactor: X=right, Y=up, Z=forward (matches OST)

### Units

All values use SI units via typed wrappers. Convert from game-native units:

| OST Unit | Type | Common Conversions |
|----------|------|-------------------|
| Speed | `MetersPerSecond(f32)` | km/h × 0.27778, mph × 0.44704 |
| RPM | `Rpm(f32)` | Usually direct |
| Temperature | `Celsius(f32)` | °F: (f - 32) × 5/9 |
| Pressure | `Kilopascals(f32)` | PSI × 6.89476, bar × 100 |
| Distance | `Meters(f32)` | km × 1000, mi × 1609.34 |
| Angle | `Degrees(f32)` | rad × 57.2958 |
| Time | `Seconds(f32)` | Usually direct |
| Percentage | `Percentage(f32)` | 0.0 to 1.0 range |

### Game-Specific Data (Extras)

Data that doesn't fit the normalized model goes in `extras`:

```rust
frame.extras.insert(
    "mygame/RawTelemetryValue".to_string(),
    serde_json::json!(42.0),
);
```

Use your game key as a prefix (e.g., `mygame/`) to namespace extras.

## Step 4: Platform-Specific Builds

Use `cfg` attributes for platform-specific code:

```rust
#[cfg(target_os = "windows")]
mod mygame;

#[cfg(not(target_os = "windows"))]
mod mygame_stub;  // No-op stub that never detects
```

See `ost-adapters/src/iracing.rs` for a full example of a Windows-only adapter using shared memory.

## Step 5: Testing

Add tests in your adapter file or in `ost-adapters/tests/`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_adapter_key() {
        let adapter = MyGameAdapter::new();
        assert_eq!(adapter.key(), "mygame");
    }

    #[test]
    fn test_frame_produces_valid_data() {
        let mut adapter = MyGameAdapter::new();
        adapter.start().unwrap();
        let frame = adapter.read_frame().unwrap().unwrap();
        assert_eq!(frame.game, "mygame");
        // Verify speed, RPM, etc. are in reasonable ranges
    }
}
```

Run tests: `just test`

## Reference: Demo Adapter

The `DemoAdapter` in `ost-adapters/src/demo.rs` is a complete working example that generates synthetic telemetry. It demonstrates:
- Realistic value generation with sine-wave physics
- All TelemetryFrame sections populated
- Extras with game-prefixed keys
- Proper start/stop lifecycle
- `detect()` always returning true (for development)
