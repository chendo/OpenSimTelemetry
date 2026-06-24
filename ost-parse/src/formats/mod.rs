//! Format dispatch and the [`ReplayParser`] trait.
//!
//! v1 ships with `ibt` only. To add a new format, drop a new module
//! under `formats/` and register its extension(s) in
//! [`parser_for_extension`].

use std::io;
use std::path::Path;

pub mod ibt;

/// Errors a [`ReplayParser`] can return.
#[derive(Debug, thiserror::Error)]
pub enum ParseError {
    #[error("I/O error: {0}")]
    Io(#[from] io::Error),
    #[error("file not found: {0}")]
    NotFound(String),
    #[error("unsupported format: {0}")]
    UnsupportedFormat(String),
    #[error("parse failed: {0}")]
    Parse(String),
}

/// Frame emission mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FrameMode {
    /// Each frame line is a JSON object containing only the channels
    /// that have a value at that tick. Carry-forward is the consumer's
    /// responsibility.
    #[default]
    Sparse,
    /// Each frame line is a JSON object containing every numeric channel
    /// from the discovered union, with the previous emitted value (or
    /// `0`) filled in for any channel that's missing or non-finite this
    /// tick. String channels remain sparse — they only appear in frames
    /// where they're present.
    Dense,
    /// Each frame line is a positional JSON **array** of length
    /// `header.channels.len()`. Values are in the same order as the
    /// header's `channels` list, which is the full channel union (numeric
    /// AND string) — a JSON array holds mixed types, so string columns sit
    /// positionally alongside numeric ones. Carry-forward applies to every
    /// column: numeric columns default to `0`, string columns to `null`
    /// until first seen, and both retain their last value when absent.
    Compact,
}

impl FrameMode {
    /// Parse a mode name (case-insensitive).
    pub fn parse(name: &str) -> Option<Self> {
        match name.to_ascii_lowercase().as_str() {
            "sparse" => Some(FrameMode::Sparse),
            "dense" => Some(FrameMode::Dense),
            "compact" => Some(FrameMode::Compact),
            _ => None,
        }
    }

    /// Lowercase wire name (`"sparse"`, `"dense"`, `"compact"`).
    pub fn as_wire_str(&self) -> &'static str {
        match self {
            FrameMode::Sparse => "sparse",
            FrameMode::Dense => "dense",
            FrameMode::Compact => "compact",
        }
    }
}

/// Parser configuration.
#[derive(Debug, Clone, Default)]
pub struct ParseOptions {
    pub mode: FrameMode,
    /// When true, skip full-file scans (lap index, track outline, channel
    /// discovery). The header's `laps`, `track_outline`, and `channels`
    /// will be empty, and frame output begins after reading only the file
    /// headers. Channels for dense/compact carry-forward are discovered
    /// from the first frame.
    pub stream: bool,
    /// When true, emit periodic progress lines to **stderr** while writing
    /// frames, formatted as `@progress <frames_done> <total> <channels>
    /// <current_lap>`. Used by the Feather path, whose single IPC file can't
    /// be decoded incrementally for progress on the consumer side. No-op for
    /// the streaming NDJSON path (the consumer counts lines itself).
    pub progress: bool,
}

impl ParseOptions {
    pub fn sparse() -> Self {
        Self {
            mode: FrameMode::Sparse,
            ..Default::default()
        }
    }

    pub fn dense() -> Self {
        Self {
            mode: FrameMode::Dense,
            ..Default::default()
        }
    }

    pub fn compact() -> Self {
        Self {
            mode: FrameMode::Compact,
            ..Default::default()
        }
    }
}

/// A streaming parser for one replay file format.
///
/// `Send + Sync` so handlers can move a `Box<dyn ReplayParser>` into a
/// `tokio::task::spawn_blocking` closure.
pub trait ReplayParser: Send + Sync {
    /// Parse the file at `path` and stream NDJSON lines into `writer`.
    ///
    /// Writes one [`SessionHeader`](crate::wire::SessionHeader) line
    /// followed by one frame line per sample. Memory usage is bounded by
    /// the channel set, not by file size — frames are read in batches
    /// internally and emitted as they're produced.
    ///
    /// The writer should be wrapped in a `BufWriter` by the caller for
    /// efficient line-by-line output.
    fn parse_to_ndjson(
        &self,
        path: &Path,
        writer: &mut dyn io::Write,
        options: &ParseOptions,
    ) -> Result<(), ParseError>;

    /// Parse the file at `path` and write a columnar **Feather** (Arrow IPC
    /// File) stream into `writer`.
    ///
    /// The schema carries one field per channel — numeric channels as
    /// `Float32`, string channels as `Utf8` — and the full
    /// [`SessionHeader`](crate::wire::SessionHeader) (laps, track outline,
    /// metadata, channel order) JSON-encoded in the schema's custom
    /// metadata under the key `ost_header`. Every column carries forward
    /// (numeric default `0`, string default null), exactly like
    /// [`FrameMode::Compact`]. This trades the per-float decimal formatting
    /// and JSON parse of the NDJSON path for a raw f32 memcpy on both ends.
    ///
    /// The default implementation reports the format unsupported; only
    /// adapters that opt in (currently `ibt`) implement it.
    fn parse_to_feather(
        &self,
        _path: &Path,
        _writer: &mut dyn io::Write,
        _options: &ParseOptions,
    ) -> Result<(), ParseError> {
        Err(ParseError::UnsupportedFormat(
            "feather output not supported for this format".to_string(),
        ))
    }
}

/// Look up a parser by lowercase file extension (e.g. `"ibt"`).
pub fn parser_for_extension(ext: &str) -> Option<Box<dyn ReplayParser>> {
    match ext.to_ascii_lowercase().as_str() {
        "ibt" => Some(Box::new(ibt::IbtReplayParser)),
        _ => None,
    }
}

/// Look up a parser for a path by inspecting its extension.
pub fn parser_for_path(path: &Path) -> Option<Box<dyn ReplayParser>> {
    path.extension()
        .and_then(|e| e.to_str())
        .and_then(parser_for_extension)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extension_dispatch_handles_ibt() {
        assert!(parser_for_extension("ibt").is_some());
        assert!(parser_for_extension("IBT").is_some());
        assert!(parser_for_extension("nope").is_none());
    }

    #[test]
    fn path_dispatch_handles_ibt() {
        assert!(parser_for_path(Path::new("/tmp/foo.ibt")).is_some());
        assert!(parser_for_path(Path::new("/tmp/foo.IBT")).is_some());
        assert!(parser_for_path(Path::new("/tmp/foo")).is_none());
        assert!(parser_for_path(Path::new("/tmp/foo.csv")).is_none());
    }

    #[test]
    fn parse_options_defaults_to_sparse() {
        assert_eq!(ParseOptions::default().mode, FrameMode::Sparse);
        assert_eq!(ParseOptions::dense().mode, FrameMode::Dense);
        assert_eq!(ParseOptions::sparse().mode, FrameMode::Sparse);
        assert_eq!(ParseOptions::compact().mode, FrameMode::Compact);
    }

    #[test]
    fn frame_mode_parse_round_trips() {
        for m in [FrameMode::Sparse, FrameMode::Dense, FrameMode::Compact] {
            assert_eq!(FrameMode::parse(m.as_wire_str()), Some(m));
        }
        assert_eq!(FrameMode::parse("SPARSE"), Some(FrameMode::Sparse));
        assert_eq!(FrameMode::parse("Compact"), Some(FrameMode::Compact));
        assert_eq!(FrameMode::parse("nope"), None);
    }
}
