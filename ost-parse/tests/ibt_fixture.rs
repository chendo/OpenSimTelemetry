//! End-to-end parse tests against `fixtures/race.ibt`.
//!
//! These tests are skipped silently if the fixture file is absent so the
//! suite stays portable. With the fixture present they exercise the
//! whole streaming path: header line + every frame line, both sparse
//! and dense modes.

use std::io::{BufRead, BufReader, Cursor};
use std::path::PathBuf;

use ost_parse::wire::SessionHeader;
use ost_parse::{parser_for_extension, ParseOptions};
use serde_json::{Map, Value};

fn fixture_path() -> PathBuf {
    let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest.join("../fixtures/race.ibt")
}

fn fixture_present() -> bool {
    fixture_path().exists()
}

fn parse_to_lines(opts: &ParseOptions) -> (SessionHeader, Vec<Map<String, Value>>) {
    let parser = parser_for_extension("ibt").expect("ibt parser");
    let mut buf = Vec::new();
    parser
        .parse_to_ndjson(&fixture_path(), &mut buf, opts)
        .expect("parse_to_ndjson");
    let mut reader = BufReader::new(Cursor::new(buf));
    let mut first = String::new();
    reader.read_line(&mut first).unwrap();
    let header: SessionHeader = serde_json::from_str(first.trim()).expect("header parse");
    let mut frames = Vec::new();
    for line in reader.lines() {
        let line = line.unwrap();
        if line.is_empty() {
            continue;
        }
        let obj: Map<String, Value> = serde_json::from_str(&line).expect("frame parse");
        frames.push(obj);
    }
    (header, frames)
}

/// Slow end-to-end parse: 73k frames × 2 modes. Marked `#[ignore]` so
/// `cargo test` stays fast; run with `cargo test -- --include-ignored`
/// or in release mode (`cargo test --release -- --include-ignored`) for
/// CI / verification.
#[test]
#[ignore]
fn parses_real_ibt_sparse_full_session() {
    if !fixture_present() {
        eprintln!("skipping: fixtures/race.ibt not present");
        return;
    }
    let (header, frames) = parse_to_lines(&ParseOptions::sparse());

    // Header sanity (Tsukuba practice fixture, 73225 frames, 60Hz)
    assert_eq!(header.format, "ost-parse");
    assert_eq!(header.version, 1);
    assert_eq!(header.source_format, "ibt");
    assert_eq!(header.mode, "sparse");
    assert_eq!(header.total_frames, 73225);
    assert_eq!(header.metadata.tick_rate, 60.0);
    assert_eq!(header.metadata.track_name, "Tsukuba Circuit 2k Full");
    assert_eq!(header.metadata.replay_id.len(), 16);
    assert!(
        !header.channels.is_empty(),
        "channels list must not be empty"
    );
    // Channels are sorted (BTreeSet output).
    let mut sorted = header.channels.clone();
    sorted.sort();
    assert_eq!(header.channels, sorted, "channels list must be sorted");
    // Spot-check that some recognisable channel names exist.
    assert!(header.channels.iter().any(|c| c == "vehicle.speed"));
    assert!(header.channels.iter().any(|c| c == "motion.g_force.x"));

    // Frame count matches header.
    assert_eq!(frames.len(), 73225);

    // Sparse: most frames have *some* fields, none should be wider than
    // the channel union, no values should be objects/arrays.
    let max_keys = frames.iter().map(|f| f.len()).max().unwrap();
    assert!(max_keys > 0);
    assert!(max_keys <= header.channels.len());
    for f in &frames[..100] {
        for (_, v) in f {
            assert!(!v.is_object() && !v.is_array());
        }
    }
}

#[test]
#[ignore]
fn parses_real_ibt_dense_first_frames() {
    if !fixture_present() {
        eprintln!("skipping: fixtures/race.ibt not present");
        return;
    }
    let (header, frames) = parse_to_lines(&ParseOptions::dense());

    assert_eq!(header.mode, "dense");
    assert!(!frames.is_empty());

    // In dense mode, every frame should contain at least the numeric
    // channels in the header. (String channels can be sparse.)
    // Pick a few likely-numeric channels and assert they're in every
    // sampled frame.
    let must_have = [
        "vehicle.speed",
        "vehicle.rpm",
        "motion.g_force.x",
        "motion.g_force.y",
        "motion.g_force.z",
    ];
    for ch in must_have {
        if header.channels.iter().any(|c| c == ch) {
            for f in frames.iter().take(50) {
                assert!(
                    f.contains_key(ch),
                    "dense mode: channel {ch} missing from a frame"
                );
                let v = f.get(ch).unwrap();
                assert!(v.is_number(), "dense mode: {ch} should be a number");
            }
        }
    }

    // Frame count matches header.
    assert_eq!(frames.len(), header.total_frames as usize);
}

/// Fast smoke test: checks the header + first ~10 frame lines without
/// waiting for the whole file. Runs by default. Wraps the writer in a
/// custom sink that bails out as soon as it has enough lines.
#[test]
fn smoke_test_real_ibt_header_and_first_frames() {
    if !fixture_present() {
        eprintln!("skipping: fixtures/race.ibt not present");
        return;
    }

    use std::io::Write;

    /// Captures the first N newline-terminated lines, then signals EOF
    /// via WriteZero so the parser bails out quickly.
    struct EarlyExitSink {
        buf: Vec<u8>,
        target_lines: usize,
        lines_seen: usize,
    }
    impl Write for EarlyExitSink {
        fn write(&mut self, data: &[u8]) -> std::io::Result<usize> {
            for &b in data {
                self.buf.push(b);
                if b == b'\n' {
                    self.lines_seen += 1;
                    if self.lines_seen >= self.target_lines {
                        return Err(std::io::Error::from(std::io::ErrorKind::WriteZero));
                    }
                }
            }
            Ok(data.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    let parser = parser_for_extension("ibt").expect("ibt parser");
    let mut sink = EarlyExitSink {
        buf: Vec::new(),
        target_lines: 11, // header + 10 frames
        lines_seen: 0,
    };
    // Parsing returns an error once the sink "fills" — that's our exit
    // signal, not a real failure. Verify the captured bytes anyway.
    let _ = parser.parse_to_ndjson(&fixture_path(), &mut sink, &ParseOptions::sparse());

    let mut reader = BufReader::new(Cursor::new(sink.buf));
    let mut header_line = String::new();
    reader.read_line(&mut header_line).unwrap();
    let header: SessionHeader = serde_json::from_str(header_line.trim()).expect("header parse");

    assert_eq!(header.format, "ost-parse");
    assert_eq!(header.version, 1);
    assert_eq!(header.source_format, "ibt");
    assert_eq!(header.mode, "sparse");
    assert_eq!(header.total_frames, 73225);
    assert_eq!(header.metadata.tick_rate, 60.0);
    assert_eq!(header.metadata.track_name, "Tsukuba Circuit 2k Full");
    assert_eq!(header.metadata.replay_id.len(), 16);
    assert!(!header.channels.is_empty());
    assert!(header.channels.iter().any(|c| c == "vehicle.speed"));

    // Read the first few frame lines we captured.
    let mut frame_count = 0;
    for line in reader.lines() {
        let line = line.unwrap();
        if line.is_empty() {
            continue;
        }
        let obj: Map<String, Value> = serde_json::from_str(&line).expect("frame parse");
        assert!(!obj.is_empty(), "frame should have at least one channel");
        for (_, v) in &obj {
            assert!(!v.is_object() && !v.is_array());
        }
        frame_count += 1;
    }
    assert!(
        frame_count >= 5,
        "expected to capture several frames, got {frame_count}"
    );
}

/// Fast compact-mode smoke test: the header carries the full channel
/// union (numeric AND string), and each positional frame array holds the
/// string channel `meta.game` at its column — proving strings survive
/// compact output rather than being dropped.
#[test]
fn smoke_test_real_ibt_compact_carries_strings() {
    if !fixture_present() {
        eprintln!("skipping: fixtures/race.ibt not present");
        return;
    }

    use std::io::Write;

    struct EarlyExitSink {
        buf: Vec<u8>,
        target_lines: usize,
        lines_seen: usize,
    }
    impl Write for EarlyExitSink {
        fn write(&mut self, data: &[u8]) -> std::io::Result<usize> {
            for &b in data {
                self.buf.push(b);
                if b == b'\n' {
                    self.lines_seen += 1;
                    if self.lines_seen >= self.target_lines {
                        return Err(std::io::Error::from(std::io::ErrorKind::WriteZero));
                    }
                }
            }
            Ok(data.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    let parser = parser_for_extension("ibt").expect("ibt parser");
    let mut sink = EarlyExitSink {
        buf: Vec::new(),
        target_lines: 11, // header + 10 frames
        lines_seen: 0,
    };
    let _ = parser.parse_to_ndjson(&fixture_path(), &mut sink, &ParseOptions::compact());

    let mut reader = BufReader::new(Cursor::new(sink.buf));
    let mut header_line = String::new();
    reader.read_line(&mut header_line).unwrap();
    let header: SessionHeader = serde_json::from_str(header_line.trim()).expect("header parse");

    assert_eq!(header.mode, "compact");
    // The string channel is present in the compact header...
    let game_idx = header
        .channels
        .iter()
        .position(|c| c == "meta.game")
        .expect("compact header must include string channel meta.game");
    let speed_idx = header
        .channels
        .iter()
        .position(|c| c == "vehicle.speed")
        .expect("compact header must include vehicle.speed");

    // ...and every positional frame array carries its string value at the
    // right column, with numeric columns staying numeric.
    let mut frame_count = 0;
    for line in reader.lines() {
        let line = line.unwrap();
        if line.is_empty() {
            continue;
        }
        let arr: Vec<Value> = serde_json::from_str(&line).expect("compact frame is a JSON array");
        assert_eq!(arr.len(), header.channels.len());
        assert_eq!(arr[game_idx], Value::from("iRacing Replay"));
        assert!(arr[speed_idx].is_number());
        frame_count += 1;
    }
    assert!(
        frame_count >= 5,
        "expected several frames, got {frame_count}"
    );
}

#[test]
#[ignore]
fn replay_id_matches_replaystate_hash() {
    if !fixture_present() {
        eprintln!("skipping: fixtures/race.ibt not present");
        return;
    }
    // Recompute the same hash inline and compare.
    use ost_parse::wire::compute_replay_id;
    let (header, _) = parse_to_lines(&ParseOptions::sparse());
    let recomputed = compute_replay_id(
        header.metadata.file_size,
        header.total_frames as usize,
        &header.metadata.track_name,
        &header.metadata.car_name,
    );
    assert_eq!(header.metadata.replay_id, recomputed);
}
