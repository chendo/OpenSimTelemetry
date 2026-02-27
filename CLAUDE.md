# Development Guide

## Build Commands

Use `just` for all build tasks:

```
just check    # Fast compilation check (no codegen)
just build    # Debug build
just test     # Run all workspace tests
just lint     # Clippy lints
just fmt      # Format code
just ci       # Full CI check (check + lint + test + fmt-check)
just run      # Run server locally (debug, port 9100)
```

## Project Structure

- `ost-core` — data model, adapter trait, units
- `ost-adapters` — sim adapters (iRacing on Windows, demo everywhere)
- `ost-server` — axum HTTP/SSE server + embedded web UI (`src/ui.html`)

## Key Files

- `ost-server/src/ui.html` — single-file web dashboard (~3000 lines, JS/CSS/HTML)
- `ost-server/src/api.rs` — REST API routes and handlers
- `ost-server/src/state.rs` — shared AppState with broadcast channel
- `ost-server/src/manager.rs` — adapter lifecycle (detection, start/stop, frame reading)
- `ost-server/src/replay.rs` — .ibt file replay state and playback
- `ost-core/src/model.rs` — TelemetryFrame and all sub-structs
- `ost-adapters/src/ibt_parser.rs` — iRacing .ibt binary file parser

## Testing

`just test` runs all workspace tests. The server tests use `tower::ServiceExt::oneshot` for HTTP endpoint testing without binding a port.

Do NOT use `cargo run` or `just run` during automated work — it binds port 9100 and blocks.
