//! Benchmarks for telemetry frame serialization and compression

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use ost_adapters::DemoAdapter;
use ost_core::TelemetryAdapter;

/// Generate a realistic telemetry frame from the demo adapter
fn make_demo_frame() -> ost_core::model::TelemetryFrame {
    let mut adapter = DemoAdapter::new();
    adapter.start().unwrap();
    adapter.read_frame().unwrap().unwrap()
}

fn bench_frame_json_serialize(c: &mut Criterion) {
    let frame = make_demo_frame();
    c.bench_function("frame_to_json", |b| {
        b.iter(|| {
            let json = serde_json::to_string(black_box(&frame)).unwrap();
            black_box(json);
        })
    });
}

fn bench_frame_json_filtered(c: &mut Criterion) {
    let frame = make_demo_frame();
    let mask = ost_core::model::MetricMask::parse("vehicle,timing");
    c.bench_function("frame_to_json_filtered", |b| {
        b.iter(|| {
            let json = black_box(&frame).to_json_filtered(Some(&mask)).unwrap();
            black_box(json);
        })
    });
}

fn bench_frame_msgpack_serialize(c: &mut Criterion) {
    let frame = make_demo_frame();
    c.bench_function("frame_to_msgpack", |b| {
        b.iter(|| {
            let bytes = rmp_serde::to_vec(black_box(&frame)).unwrap();
            black_box(bytes);
        })
    });
}

fn bench_frames_zstd_compress(c: &mut Criterion) {
    // Generate 60 frames (~1 second of data)
    let mut adapter = DemoAdapter::new();
    adapter.start().unwrap();
    let frames: Vec<_> = (0..60)
        .map(|_| adapter.read_frame().unwrap().unwrap())
        .collect();

    c.bench_function("compress_60_frames_zstd", |b| {
        b.iter(|| {
            let compressed = ost_server::persistence::compress_frames(black_box(&frames)).unwrap();
            black_box(compressed);
        })
    });
}

fn bench_ibt_parse_frame(c: &mut Criterion) {
    let fixture_path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../fixtures/race.ibt");
    if !fixture_path.exists() {
        eprintln!("Skipping ibt benchmark: fixture not found");
        return;
    }

    c.bench_function("ibt_parse_single_frame", |b| {
        b.iter_with_setup(
            || ost_adapters::ibt_parser::IbtFile::open(&fixture_path).unwrap(),
            |ibt| {
                let sample = ibt.read_sample(0).unwrap();
                let tf = ibt.sample_to_frame(&sample);
                black_box(tf);
            },
        )
    });
}

criterion_group!(
    benches,
    bench_frame_json_serialize,
    bench_frame_json_filtered,
    bench_frame_msgpack_serialize,
    bench_frames_zstd_compress,
    bench_ibt_parse_frame,
);
criterion_main!(benches);
