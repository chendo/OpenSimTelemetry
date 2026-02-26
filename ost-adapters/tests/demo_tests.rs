//! Integration tests for the DemoAdapter

use ost_adapters::DemoAdapter;
use ost_core::adapter::TelemetryAdapter;

#[test]
fn test_demo_adapter_name() {
    let adapter = DemoAdapter::new();
    assert_eq!(adapter.name(), "Demo");
}

#[test]
fn test_demo_adapter_detect_always_true() {
    let adapter = DemoAdapter::new();
    assert!(adapter.detect(), "DemoAdapter should always be detected");
}

#[test]
fn test_demo_adapter_initially_inactive() {
    let adapter = DemoAdapter::new();
    assert!(
        !adapter.is_active(),
        "DemoAdapter should be inactive before start()"
    );
}

#[test]
fn test_demo_adapter_read_frame_when_inactive_returns_none() {
    let mut adapter = DemoAdapter::new();
    let frame = adapter.read_frame().unwrap();
    assert!(
        frame.is_none(),
        "read_frame() should return None when adapter is inactive"
    );
}

#[test]
fn test_demo_adapter_start_and_stop() {
    let mut adapter = DemoAdapter::new();

    adapter.start().expect("start() should succeed");
    assert!(adapter.is_active(), "Adapter should be active after start()");

    adapter.stop().expect("stop() should succeed");
    assert!(
        !adapter.is_active(),
        "Adapter should be inactive after stop()"
    );
}

#[test]
fn test_demo_adapter_produces_valid_frame() {
    let mut adapter = DemoAdapter::new();
    adapter.start().expect("start() should succeed");

    let frame = adapter
        .read_frame()
        .expect("read_frame() should not error")
        .expect("read_frame() should return Some after start()");

    // Check game name
    assert_eq!(frame.game, "Demo");

    // Check that key fields are populated
    assert!(frame.speed.is_some(), "speed should be populated");
    assert!(frame.rpm.is_some(), "rpm should be populated");
    assert!(frame.gear.is_some(), "gear should be populated");
    assert!(frame.throttle.is_some(), "throttle should be populated");
    assert!(frame.brake.is_some(), "brake should be populated");
    assert!(frame.wheels.is_some(), "wheels should be populated");
    assert!(frame.position.is_some(), "position should be populated");
    assert!(frame.velocity.is_some(), "velocity should be populated");
    assert!(frame.g_force.is_some(), "g_force should be populated");
    assert!(frame.track_name.is_some(), "track_name should be populated");
    assert!(frame.car_name.is_some(), "car_name should be populated");
    assert!(
        frame.session_type.is_some(),
        "session_type should be populated"
    );
}

#[test]
fn test_demo_adapter_frame_values_in_reasonable_range() {
    let mut adapter = DemoAdapter::new();
    adapter.start().expect("start() should succeed");

    let frame = adapter
        .read_frame()
        .expect("read_frame() should not error")
        .expect("read_frame() should return Some");

    // RPM should be in a reasonable range (3000-7000 for the demo)
    let rpm = frame.rpm.unwrap().0;
    assert!(
        (2000.0..=8000.0).contains(&rpm),
        "RPM {} should be in reasonable range",
        rpm
    );

    // Speed should be positive and reasonable (m/s)
    let speed = frame.speed.unwrap().0;
    assert!(
        (0.0..=100.0).contains(&speed),
        "Speed {} should be in reasonable range",
        speed
    );

    // Gear should be 1-6
    let gear = frame.gear.unwrap();
    assert!(
        (1..=6).contains(&gear),
        "Gear {} should be between 1 and 6",
        gear
    );

    // Throttle should be 0.0 to 1.0
    let throttle = frame.throttle.unwrap().0;
    assert!(
        (0.0..=1.0).contains(&throttle),
        "Throttle {} should be between 0 and 1",
        throttle
    );

    // Brake should be 0.0 to 1.0
    let brake = frame.brake.unwrap().0;
    assert!(
        (0.0..=1.0).contains(&brake),
        "Brake {} should be between 0 and 1",
        brake
    );

    // Fuel level should be 0.0 to 1.0
    let fuel = frame.fuel_level.unwrap().0;
    assert!(
        (0.0..=1.0).contains(&fuel),
        "Fuel level {} should be between 0 and 1",
        fuel
    );
}

#[test]
fn test_demo_adapter_produces_multiple_frames() {
    let mut adapter = DemoAdapter::new();
    adapter.start().expect("start() should succeed");

    // Read several frames to ensure we can produce data continuously
    for i in 0..5 {
        let frame = adapter
            .read_frame()
            .expect("read_frame() should not error")
            .expect(&format!("Frame {} should be Some", i));
        assert_eq!(frame.game, "Demo");
    }
}

#[test]
fn test_demo_adapter_frame_has_extras() {
    let mut adapter = DemoAdapter::new();
    adapter.start().expect("start() should succeed");

    let frame = adapter
        .read_frame()
        .expect("read_frame() should not error")
        .expect("read_frame() should return Some");

    // The demo adapter adds a demo_frame_count extra
    assert!(
        frame.extras.contains_key("demo_frame_count"),
        "Frame extras should contain demo_frame_count"
    );
}

#[test]
fn test_demo_adapter_frame_serializes_to_json() {
    let mut adapter = DemoAdapter::new();
    adapter.start().expect("start() should succeed");

    let frame = adapter
        .read_frame()
        .expect("read_frame() should not error")
        .expect("read_frame() should return Some");

    // Should serialize without error
    let json = serde_json::to_string(&frame).expect("Frame should serialize to JSON");
    assert!(!json.is_empty(), "JSON should not be empty");

    // Should be valid JSON
    let parsed: serde_json::Value =
        serde_json::from_str(&json).expect("JSON should be parseable");
    assert_eq!(parsed["game"], "Demo");
}

#[test]
fn test_demo_adapter_default_trait() {
    // DemoAdapter implements Default
    let adapter = DemoAdapter::default();
    assert_eq!(adapter.name(), "Demo");
    assert!(!adapter.is_active());
}
