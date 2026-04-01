//! Diagnostic tool: inspect an .ibt file's lap index and track outline.
//!
//! Usage: cargo run -p ost-adapters --example ibt_inspect -- <path.ibt>

use ost_adapters::ibt_parser::IbtFile;
use std::env;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

fn main() {
    let path = env::args().nth(1).unwrap_or_else(|| {
        eprintln!("Usage: ibt_inspect <path.ibt>");
        std::process::exit(1);
    });

    let mut ibt = IbtFile::open(Path::new(&path)).unwrap_or_else(|e| {
        eprintln!("Failed to open {path}: {e}");
        std::process::exit(1);
    });

    println!("=== IBT File: {path} ===");
    println!("Records: {}", ibt.record_count());

    // Session info
    let yaml = ibt.session_info_yaml();
    for line in yaml.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("TrackName:")
            || trimmed.starts_with("TrackDisplayName:")
            || trimmed.starts_with("TrackLength:")
            || trimmed.starts_with("DriverCarIdx:")
            || trimmed.starts_with("CarScreenName:")
        {
            println!("  {trimmed}");
        }
    }

    // Variable availability
    let vars = ibt.var_headers_ref();
    let has_lap = vars.iter().any(|v| v.name == "Lap");
    let has_pct = vars.iter().any(|v| v.name == "LapDistPct");
    println!("\nVariables: Lap={has_lap}  LapDistPct={has_pct}");

    // Raw scan for per-lap LapDistPct stats
    let record_count = ibt.record_count();
    let buf_len = ibt.header_ref().buf_len as usize;
    let sample_offset = ibt.sample_data_offset();

    let pct_off = vars
        .iter()
        .find(|v| v.name == "LapDistPct")
        .map(|v| v.offset as usize);

    // Print all available variable names for reference
    println!("\n=== All Variables ({}) ===", vars.len());
    let mut var_names: Vec<&str> = vars.iter().map(|v| v.name.as_str()).collect();
    var_names.sort();
    for chunk in var_names.chunks(8) {
        println!("  {}", chunk.join(", "));
    }

    // Bulk-read all sample data
    let total_bytes = buf_len * record_count;
    let mut bulk = vec![0u8; total_bytes];
    {
        let mut file = std::fs::File::open(&path).unwrap();
        file.seek(SeekFrom::Start(sample_offset)).unwrap();
        file.read_exact(&mut bulk).unwrap();
    }

    let read_f32 = |frame: usize, off: usize| -> f32 {
        let s = frame * buf_len + off;
        if s + 4 <= bulk.len() {
            f32::from_le_bytes(bulk[s..s + 4].try_into().unwrap())
        } else {
            f32::NAN
        }
    };
    // Build lap index
    println!("\n=== Lap Index ===");
    let laps = ibt.build_lap_index().unwrap_or_else(|e| {
        eprintln!("Failed to build lap index: {e}");
        std::process::exit(1);
    });

    println!(
        "{:<5} {:<6} {:<8} {:<12} {:<8} {:<8} {:<8}",
        "Idx", "Lap#", "Frames", "Time", "AvgPct", "StdDev", "Frames",
    );
    println!("{}", "-".repeat(65));
    for (i, lap) in laps.iter().enumerate() {
        let time_str = match lap.lap_time_secs {
            Some(t) => format!("{t:.3}s"),
            None => "--".to_string(),
        };
        let incomplete = if lap.incomplete { " (reset)" } else { "" };

        let end_frame = if i + 1 < laps.len() {
            laps[i + 1].start_frame
        } else {
            record_count
        };
        let num_frames = end_frame.saturating_sub(lap.start_frame);
        let mut pct_values: Vec<f64> = Vec::new();

        for frame in lap.start_frame..end_frame.min(record_count) {
            if let Some(off) = pct_off {
                let p = read_f32(frame, off);
                if !p.is_nan() {
                    pct_values.push(p as f64);
                }
            }
        }
        let avg_pct = if !pct_values.is_empty() {
            pct_values.iter().sum::<f64>() / pct_values.len() as f64
        } else {
            f64::NAN
        };
        let std_dev = if pct_values.len() > 1 {
            let variance = pct_values
                .iter()
                .map(|p| (p - avg_pct).powi(2))
                .sum::<f64>()
                / (pct_values.len() - 1) as f64;
            variance.sqrt()
        } else {
            f64::NAN
        };

        let invalid = lap.invalid_reason.as_deref().unwrap_or("");
        println!(
            "{:<5} {:<6} {:<8} {:<12} {:<8.4} {:<8.4} {:<8} {}{incomplete}",
            i,
            lap.lap_number,
            num_frames,
            time_str,
            avg_pct,
            std_dev,
            pct_values.len(),
            invalid,
        );
    }

    let complete = laps
        .iter()
        .filter(|l| l.lap_time_secs.is_some() && !l.incomplete)
        .count();
    let incomplete_count = laps.iter().filter(|l| l.incomplete).count();
    println!(
        "\nTotal: {} entries, {} complete, {} incomplete (reset)",
        laps.len(),
        complete,
        incomplete_count
    );

    if let Some(best) = laps
        .iter()
        .filter(|l| l.lap_time_secs.is_some() && !l.incomplete && l.invalid_reason.is_none())
        .min_by(|a, b| {
            a.lap_time_secs
                .unwrap()
                .partial_cmp(&b.lap_time_secs.unwrap())
                .unwrap()
        })
    {
        println!(
            "Best lap: #{} @ {:.3}s (frame {})",
            best.lap_number,
            best.lap_time_secs.unwrap(),
            best.start_frame
        );
    } else {
        println!("Best lap: none (no complete laps)");
    }

    // Dump pct for a specific lap if --dump-lap N is specified
    if let Some(dump_arg) = env::args().nth(2) {
        if dump_arg == "--dump-lap" {
            if let Some(lap_idx_str) = env::args().nth(3) {
                let lap_idx: usize = lap_idx_str.parse().unwrap();
                if lap_idx < laps.len() {
                    let end = if lap_idx + 1 < laps.len() {
                        laps[lap_idx + 1].start_frame
                    } else {
                        record_count
                    };
                    println!(
                        "\n=== Pct Dump for lap index {} (frames {}..{}) ===",
                        lap_idx, laps[lap_idx].start_frame, end
                    );
                    let mut prev_pct = f32::NAN;
                    for frame in laps[lap_idx].start_frame..end.min(record_count) {
                        if let Some(off) = pct_off {
                            let p = read_f32(frame, off);
                            let delta = if prev_pct.is_nan() { 0.0 } else { p - prev_pct };
                            // Only print on significant change or discontinuity
                            if delta.abs() > 0.005
                                || prev_pct.is_nan()
                                || frame == laps[lap_idx].start_frame
                                || frame == end - 1
                            {
                                println!("  frame {}: pct={:.6} delta={:.6}", frame, p, delta);
                            }
                            prev_pct = p;
                        }
                    }
                }
            }
        }
    }

    // Track outline
    println!("\n=== Track Outline ===");
    let outline = ibt.build_track_outline(&laps).unwrap_or_else(|e| {
        eprintln!("Failed to build track outline: {e}");
        std::process::exit(1);
    });
    println!("{} points", outline.len());
    if let (Some(first), Some(last)) = (outline.first(), outline.last()) {
        println!("  First: [{:.6}, {:.6}]", first[0], first[1]);
        println!("  Last:  [{:.6}, {:.6}]", last[0], last[1]);
    }
}
