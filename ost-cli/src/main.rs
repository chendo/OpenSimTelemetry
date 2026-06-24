//! `ost-cli` — thin synchronous wrapper around `ost-parse`.
//!
//! Usage:
//!
//! ```text
//! ost-cli parse <input> [--format ibt] [--output -|<path>] [--dense]
//! ost-cli parse - --format ibt < file.ibt > out.ndjson
//! ```
//!
//! `<input>` is a path to a replay file or `-` for stdin. When stdin is
//! used, the file is buffered to a temp path before parsing — the IBT
//! parser needs random access. Path-input is the lower-memory choice.
//!
//! Default output is stdout. The format is auto-detected from the
//! extension; pass `--format` to override (required when reading stdin).

use std::ffi::OsString;
use std::fs::File;
use std::io::{self, BufWriter, Write};
use std::path::PathBuf;
use std::process::ExitCode;

use ost_parse::{parser_for_extension, parser_for_path, FrameMode, ParseError, ParseOptions};

fn main() -> ExitCode {
    let args: Vec<OsString> = std::env::args_os().skip(1).collect();
    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            let _ = writeln!(io::stderr(), "ost-cli: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run(args: &[OsString]) -> Result<(), CliError> {
    let cmd = args
        .first()
        .ok_or_else(|| CliError::Usage("missing subcommand".to_string()))?;
    match cmd.to_str() {
        Some("parse") => run_parse(&args[1..]),
        Some("--help") | Some("-h") | Some("help") => {
            print_usage();
            Ok(())
        }
        _ => Err(CliError::Usage(format!(
            "unknown subcommand: {:?}",
            cmd.to_string_lossy()
        ))),
    }
}

fn run_parse(args: &[OsString]) -> Result<(), CliError> {
    let opts = ParseArgs::from_args(args)?;

    // Resolve input → path on disk + optional temp guard.
    let (input_path, _temp_guard) = match opts.input.as_str() {
        "-" => {
            // Spool stdin to a temp file under $TMPDIR/ost-cli.
            let dir = std::env::temp_dir().join("ost-cli");
            std::fs::create_dir_all(&dir)?;
            let temp = dir.join(format!("stdin-{}.ibt", std::process::id()));
            let mut file = File::create(&temp)?;
            io::copy(&mut io::stdin().lock(), &mut file)?;
            file.sync_all().ok();
            drop(file);
            (temp.clone(), Some(TempGuard(temp)))
        }
        path => (PathBuf::from(path), None),
    };

    // Resolve parser. --format wins; otherwise dispatch by extension.
    let parser = if let Some(fmt) = opts.format.as_deref() {
        parser_for_extension(fmt)
            .ok_or_else(|| CliError::Usage(format!("unknown --format: {fmt} (supported: ibt)")))?
    } else {
        parser_for_path(&input_path).ok_or_else(|| {
            CliError::Usage(format!(
                "could not infer format from {} (pass --format)",
                input_path.display()
            ))
        })?
    };

    let parse_opts = ParseOptions {
        mode: opts.mode,
        stream: opts.stream,
    };

    // Resolve output → BufWriter<dyn Write>.
    let mut writer: BufWriter<Box<dyn Write>> = match opts.output.as_deref() {
        None | Some("-") => BufWriter::new(Box::new(io::stdout().lock())),
        Some(path) => BufWriter::new(Box::new(File::create(path)?)),
    };

    parser
        .parse_to_ndjson(&input_path, &mut writer, &parse_opts)
        .map_err(CliError::Parse)?;

    writer.flush()?;
    Ok(())
}

#[derive(Debug)]
struct ParseArgs {
    input: String,
    format: Option<String>,
    output: Option<String>,
    mode: FrameMode,
    stream: bool,
}

impl ParseArgs {
    fn from_args(args: &[OsString]) -> Result<Self, CliError> {
        let mut input: Option<String> = None;
        let mut format: Option<String> = None;
        let mut output: Option<String> = None;
        let mut mode: FrameMode = FrameMode::Sparse;
        let mut stream = false;

        let mut i = 0;
        while i < args.len() {
            let arg = args[i]
                .to_str()
                .ok_or_else(|| CliError::Usage("non-utf8 argument".to_string()))?;
            match arg {
                "--format" => {
                    i += 1;
                    let val = args
                        .get(i)
                        .and_then(|s| s.to_str())
                        .ok_or_else(|| CliError::Usage("--format needs a value".to_string()))?;
                    format = Some(val.to_string());
                }
                "--output" | "-o" => {
                    i += 1;
                    let val = args
                        .get(i)
                        .and_then(|s| s.to_str())
                        .ok_or_else(|| CliError::Usage("--output needs a value".to_string()))?;
                    output = Some(val.to_string());
                }
                "--mode" => {
                    i += 1;
                    let val = args
                        .get(i)
                        .and_then(|s| s.to_str())
                        .ok_or_else(|| CliError::Usage("--mode needs a value".to_string()))?;
                    mode = FrameMode::parse(val).ok_or_else(|| {
                        CliError::Usage(format!(
                            "unknown --mode: {val} (expected sparse, dense, or compact)"
                        ))
                    })?;
                }
                "--stream" => {
                    stream = true;
                }
                "--help" | "-h" => {
                    print_usage();
                    std::process::exit(0);
                }
                positional if !positional.starts_with('-') || positional == "-" => {
                    if input.is_some() {
                        return Err(CliError::Usage(format!(
                            "unexpected positional arg: {positional}"
                        )));
                    }
                    input = Some(positional.to_string());
                }
                other => {
                    return Err(CliError::Usage(format!("unknown flag: {other}")));
                }
            }
            i += 1;
        }

        let input = input.ok_or_else(|| CliError::Usage("missing <input> argument".to_string()))?;
        if input == "-" && format.is_none() {
            return Err(CliError::Usage(
                "--format is required when reading from stdin".to_string(),
            ));
        }
        Ok(ParseArgs {
            input,
            format,
            output,
            mode,
            stream,
        })
    }
}

fn print_usage() {
    let _ = writeln!(
        io::stdout(),
        "ost-cli — streaming replay parser

USAGE:
    ost-cli parse <input> [OPTIONS]

ARGS:
    <input>          Path to a replay file, or '-' for stdin

OPTIONS:
    --format <FMT>   Override format detection (e.g. 'ibt'). Required for stdin.
    --output <PATH>  Write NDJSON to PATH (default: stdout). Use '-' for stdout.
    --mode <MODE>    Frame output mode. One of:
                       sparse   (default) per-frame JSON object of present channels
                       dense    per-frame JSON object of every numeric channel
                                with carry-forward; strings still sparse
                       compact  per-frame positional JSON array of every
                                channel (numeric and string) with
                                carry-forward, aligned to header.channels
    --stream         Skip full-file scans (lap index, track outline, channel
                     discovery). Header laps/track_outline/channels will be
                     empty; frame output starts immediately after file headers.
    -h, --help       Show this help"
    );
}

#[derive(Debug, thiserror::Error)]
enum CliError {
    #[error("{0}")]
    Usage(String),
    #[error("I/O error: {0}")]
    Io(#[from] io::Error),
    #[error("parse error: {0}")]
    Parse(ParseError),
}

/// RAII guard that deletes a temp file on drop.
struct TempGuard(PathBuf);

impl Drop for TempGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}
