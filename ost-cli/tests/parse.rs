//! Integration tests for `ost-cli parse`.
//!
//! These spawn the actual compiled binary via `env!("CARGO_BIN_EXE_ost-cli")`
//! and feed it the real `fixtures/race.ibt` file. Fixture-based tests are
//! skipped silently when the fixture is absent so the suite stays
//! portable; pure arg-parsing and error-path tests run unconditionally.
//!
//! Fast by design: for any test that would otherwise stream all 73k
//! frames, we read only the header + a handful of frame lines from the
//! child's piped stdout, then `kill()` the child. Each fixture test
//! completes in ~1–2 seconds in debug mode.

use std::io::{BufRead, BufReader, Read};
use std::path::PathBuf;
use std::process::{Command, Stdio};

/// Absolute path to the compiled `ost-cli` binary. Cargo populates this
/// for integration tests in a crate that has a `[[bin]]` target.
const BIN: &str = env!("CARGO_BIN_EXE_ost-cli");

fn fixture_path() -> PathBuf {
    let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest.join("../fixtures/race.ibt")
}

fn fixture_present() -> bool {
    fixture_path().exists()
}

/// Spawn `ost-cli <args...>`, capture stdout up to `max_lines`, then
/// kill the child. Returns the captured lines.
fn run_and_capture_lines(args: &[&str], max_lines: usize) -> Vec<String> {
    let mut child = Command::new(BIN)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn ost-cli");

    let stdout = child.stdout.take().expect("no stdout handle");
    let mut reader = BufReader::new(stdout);
    let mut lines = Vec::new();
    for _ in 0..max_lines {
        let mut line = String::new();
        match reader.read_line(&mut line) {
            Ok(0) => break,
            Ok(_) => {
                if line.ends_with('\n') {
                    line.pop();
                }
                lines.push(line);
            }
            Err(_) => break,
        }
    }
    drop(reader);
    let _ = child.kill();
    let _ = child.wait();
    lines
}

fn parse_header(line: &str) -> serde_json::Value {
    serde_json::from_str(line).expect("header line should parse as JSON")
}

// ==================== Arg parsing / error paths ====================

#[test]
fn prints_help_with_flag() {
    let out = Command::new(BIN).arg("--help").output().unwrap();
    assert!(out.status.success());
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("ost-cli"));
    assert!(text.contains("parse"));
    assert!(text.contains("--mode"));
    assert!(text.contains("sparse"));
    assert!(text.contains("dense"));
    assert!(text.contains("compact"));
}

#[test]
fn no_subcommand_fails() {
    let out = Command::new(BIN).output().unwrap();
    assert!(!out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("missing subcommand"));
}

#[test]
fn parse_with_no_input_fails() {
    let out = Command::new(BIN).arg("parse").output().unwrap();
    assert!(!out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("missing <input>"));
}

#[test]
fn parse_with_unknown_mode_fails() {
    let out = Command::new(BIN)
        .args(["parse", "some.ibt", "--mode", "bogus"])
        .output()
        .unwrap();
    assert!(!out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("unknown --mode"));
}

#[test]
fn parse_with_unknown_format_fails() {
    let out = Command::new(BIN)
        .args(["parse", "foo.unknown", "--format", "bogus"])
        .output()
        .unwrap();
    assert!(!out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("unknown --format"));
}

#[test]
fn parse_stdin_without_format_fails() {
    // `-` input but no --format is a hard error — we can't infer.
    let out = Command::new(BIN)
        .args(["parse", "-"])
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert!(!out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("--format is required"));
}

#[test]
fn parse_with_missing_file_fails() {
    let out = Command::new(BIN)
        .args(["parse", "/definitely/does/not/exist.ibt"])
        .output()
        .unwrap();
    assert!(!out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("parse error") || err.contains("not found") || err.contains("No such"),
        "expected a parse error on stderr, got: {err}"
    );
}

// ==================== Fixture-backed end-to-end tests ====================

#[test]
fn parse_sparse_default_mode_header_and_first_frames() {
    if !fixture_present() {
        eprintln!("skipping: fixtures/race.ibt not present");
        return;
    }
    let fixture = fixture_path();
    let lines = run_and_capture_lines(&["parse", fixture.to_str().unwrap()], 6);
    assert!(lines.len() >= 2, "expected header + at least 1 frame");

    let header = parse_header(&lines[0]);
    assert_eq!(header["format"], "ost-parse");
    assert_eq!(header["version"], 1);
    assert_eq!(header["source_format"], "ibt");
    assert_eq!(header["mode"], "sparse");
    assert_eq!(header["total_frames"], 73225);
    assert_eq!(header["metadata"]["track_name"], "Tsukuba Circuit 2k Full");
    let replay_id = header["metadata"]["replay_id"].as_str().unwrap();
    assert_eq!(replay_id.len(), 16);
    let channels = header["channels"].as_array().unwrap();
    assert!(channels.iter().any(|c| c == "vehicle.speed"));

    // Frame lines are JSON objects of dot-path → value.
    for line in &lines[1..] {
        let obj: serde_json::Map<String, serde_json::Value> =
            serde_json::from_str(line).expect("frame line should parse as object");
        assert!(!obj.is_empty());
        for (_, v) in &obj {
            assert!(!v.is_object() && !v.is_array());
        }
    }
}

#[test]
fn parse_dense_mode_frames_are_objects_with_numeric_fallback() {
    if !fixture_present() {
        return;
    }
    let fixture = fixture_path();
    let lines = run_and_capture_lines(&["parse", fixture.to_str().unwrap(), "--mode", "dense"], 6);
    assert!(lines.len() >= 2);

    let header = parse_header(&lines[0]);
    assert_eq!(header["mode"], "dense");

    // In dense mode the first frame must already contain every numeric
    // channel (carry-forward initialised to 0).
    let first: serde_json::Map<String, serde_json::Value> =
        serde_json::from_str(&lines[1]).unwrap();
    for ch in [
        "vehicle.speed",
        "vehicle.rpm",
        "motion.g_force.x",
        "motion.g_force.y",
        "motion.g_force.z",
    ] {
        assert!(first.contains_key(ch), "dense first frame missing {ch}");
        assert!(first[ch].is_number());
    }
}

#[test]
fn parse_compact_mode_frames_are_positional_numeric_arrays() {
    if !fixture_present() {
        return;
    }
    let fixture = fixture_path();
    let lines = run_and_capture_lines(
        &["parse", fixture.to_str().unwrap(), "--mode", "compact"],
        6,
    );
    assert!(lines.len() >= 2);

    let header = parse_header(&lines[0]);
    assert_eq!(header["mode"], "compact");

    let channels = header["channels"].as_array().unwrap();
    assert!(!channels.is_empty());
    // Compact header excludes strings.
    assert!(!channels.iter().any(|c| c == "session.track_name"));
    assert!(channels.iter().any(|c| c == "vehicle.speed"));

    // Every frame is a JSON array of length channels.len(), numeric-only.
    for line in &lines[1..] {
        let arr: Vec<serde_json::Value> =
            serde_json::from_str(line).expect("compact frame should parse as JSON array");
        assert_eq!(arr.len(), channels.len());
        for v in &arr {
            assert!(v.is_number());
        }
    }
}

#[test]
fn parse_writes_output_file_with_dash_flag() {
    if !fixture_present() {
        return;
    }
    // --output to a real path. We run the full parse (no early exit) but
    // cap it with a tiny subset via --mode compact (smallest per line)
    // and still let it complete. To keep this test fast even in debug,
    // we terminate the child once the file starts being written and
    // trust the RAII flush — but the simpler portable approach is to
    // use a short-lived temp directory and just kill the child after a
    // small delay.
    //
    // Actually the cleanest fast path is: stream stdout to -, capture
    // to a string, then write it ourselves. But we specifically want to
    // exercise the --output code path. So spawn with --output to a file,
    // wait briefly, kill the child, and verify the file has a valid
    // header line.
    let out_dir = std::env::temp_dir().join(format!("ost-cli-test-{}", std::process::id()));
    std::fs::create_dir_all(&out_dir).unwrap();
    let out_path = out_dir.join("out.ndjson");
    let fixture = fixture_path();

    let mut child = Command::new(BIN)
        .args([
            "parse",
            fixture.to_str().unwrap(),
            "--mode",
            "compact",
            "--output",
            out_path.to_str().unwrap(),
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();

    // Poll for the file to exist and contain at least one newline
    // (header line flushed). Give up after 10s.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    let mut first_line: Option<String> = None;
    while std::time::Instant::now() < deadline {
        if out_path.exists() {
            if let Ok(mut f) = std::fs::File::open(&out_path) {
                let mut buf = String::new();
                let _ = f.read_to_string(&mut buf);
                if let Some(idx) = buf.find('\n') {
                    first_line = Some(buf[..idx].to_string());
                    break;
                }
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }

    let _ = child.kill();
    let _ = child.wait();

    let _ = std::fs::remove_file(&out_path);
    let _ = std::fs::remove_dir(&out_dir);

    let header_line = first_line.expect("expected a header line to be written");
    let header = parse_header(&header_line);
    assert_eq!(header["mode"], "compact");
    assert_eq!(header["total_frames"], 73225);
}

#[test]
fn parse_stdin_with_format_ibt() {
    if !fixture_present() {
        return;
    }
    // Spawn with the .ibt fed via stdin. ost-cli spools stdin to a temp
    // file and parses from there. We read the first few lines from
    // stdout then kill the child.
    let ibt_bytes = std::fs::read(fixture_path()).expect("read fixture");

    let mut child = Command::new(BIN)
        .args(["parse", "-", "--format", "ibt"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();

    {
        use std::io::Write;
        let mut stdin = child.stdin.take().expect("stdin handle");
        // Write the whole fixture in a background thread so the writer
        // doesn't block on a full pipe while we're trying to read the
        // output.
        std::thread::spawn(move || {
            let _ = stdin.write_all(&ibt_bytes);
        });
    }

    let stdout = child.stdout.take().unwrap();
    let mut reader = BufReader::new(stdout);
    let mut header_line = String::new();
    reader
        .read_line(&mut header_line)
        .expect("read header line");
    drop(reader);
    let _ = child.kill();
    let _ = child.wait();

    let header_line = header_line.trim_end();
    assert!(!header_line.is_empty(), "header line should not be empty");
    let header = parse_header(header_line);
    assert_eq!(header["format"], "ost-parse");
    assert_eq!(header["source_format"], "ibt");
    assert_eq!(header["total_frames"], 73225);
}
