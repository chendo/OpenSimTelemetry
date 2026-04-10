//! Streaming NDJSON parse path for OpenSimTelemetry replay files.
//!
//! This crate is the single source of truth for converting a replay file
//! (currently `.ibt`) into the [NDJSON wire format](../README.md).
//! Both the `ost-cli` binary and the `ost-server` `POST /api/parse`
//! endpoint call into here.
//!
//! ## Memory model
//!
//! The parser holds an `IbtFile` handle plus one frame's worth of values
//! at a time. Peak memory is bounded by the channel set, not by session
//! length. Output is streamed line-by-line into a `Write` impl.
//!
//! ## Wire format
//!
//! The output is newline-delimited JSON. The first line is a
//! [`SessionHeader`](crate::wire::SessionHeader) describing channels and
//! metadata. Every subsequent line is a [frame object](crate::wire) keyed
//! by dot-separated channel names. See [README](../README.md) for the full
//! spec.

pub mod flatten;
pub mod formats;
pub mod wire;

pub use formats::{
    parser_for_extension, parser_for_path, FrameMode, ParseError, ParseOptions, ReplayParser,
};
pub use wire::{LapInfo, ReplayMetadata, SessionHeader};
