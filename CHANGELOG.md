# Changelog

All notable changes to OpenSimTelemetry are documented in this file.

## Unreleased

### Fixes

- **Position is no longer truncated to `f32` in Feather output.** Every
  numeric channel was emitted as `Float32`, but iRacing stores `Lat` and `Lon`
  as doubles precisely because `f32` cannot hold them: an absolute coordinate
  spends its mantissa on the magnitude. Rounding to the nearest `f32` cost
  85cm of longitude and 21cm of latitude at The Bend. At 60Hz a car covers
  about half a metre per frame, so the error exceeded the step between samples
  — the recorded path became a staircase on which half the frames repeated the
  previous position exactly, invisible across a lap and the entire shape of
  the line across one corner. `motion.latitude` and `motion.longitude` are now
  `Float64`; the pedals, g-forces, speed and distances are natively `f32` in
  the IBT and are unchanged.

- **The session clock is no longer narrowed to `f32` either.** `SessionTime`,
  `SessionTimeRemain` and `SessionTimeOfDay` now travel as a new
  `SessionSeconds(f64)` rather than `Seconds(f32)`. It is the same unit as a
  lap time but not the same range: a lap time lives in the tens or hundreds,
  where `f32` still resolves microseconds, while a session clock reaches
  86,400 in a twenty-four hour race, where consecutive `f32` values sit 7.8ms
  apart. Serialisation keeps the existing four-decimal convention but at
  double width, so the clock is now accurate to 0.1ms whatever the session
  length instead of degrading as it runs.

- **Lap timing is no longer discarded for laps that aren't clean full tours.**
  `lap_time_secs` was withheld when a lap's `LapDistPct` distribution didn't
  look like a complete lap, or when iRacing's `LapLastLapTime` disagreed with
  the frame range — both normal around a pit stop. On a 25-lap race that cost
  8 lap times, including two ordinary racing laps next to the stops. Every lap
  that has a time now reports it; whether the lap was a full tour is reported
  beside it as `full_lap`, and how the time was arrived at as `time_source`.
  Track-outline selection now asks for `full_lap` explicitly, where it used to
  rely on timing being absent for anything else.

## 0.6.0 — 2026-07-29

Maintenance release with no functional changes since 0.5.0 — internal lint
cleanup in `ost-server`. This is the first tagged release since v0.1.0 and
ships the 0.4.0 and 0.5.0 changes documented below.

## 0.5.0 — 2026-06-24

### Features

- **`ost-cli parse --feather`** — emits a columnar **Feather** (Arrow IPC File) stream instead of NDJSON: one column per channel (numeric as `Float32`, string as `Utf8`), with the full `SessionHeader` JSON-encoded in the Arrow schema metadata under `ost_header`. Columns carry forward like `--mode compact`. Skips per-float decimal formatting and per-frame `JSON.parse`, roughly halving wire size. Purely additive — NDJSON remains the default. See CHANGELOG-API.md for details.
- **`ost-cli parse --progress`** — emits `@progress <frames_done> <total> <channels> <current_lap>` lines to stderr every 1000 frames, for upload UIs consuming the Feather path (a single Arrow IPC file can't be decoded incrementally). Off by default; stdout is unchanged.

## 0.4.0 — 2026-06-24

### Bug Fixes

- **`--mode compact` no longer drops string channels** (BREAKING) — `SessionHeader.channels` in compact mode is now the full channel union (numeric and string) and string columns ride along positionally in frame arrays (`null` until first seen, carried forward after). Consumers must stop assuming every compact slot is a number. See CHANGELOG-API.md for migration notes.
- **`vehicle.track_surface` is now a track location, not a material** (BREAKING) — the field was sourced from iRacing's `irsdk_TrkLoc` location but decoded through the material table, producing wrong values. It now serializes as a normalized location enum string (`"NotInWorld"`, `"OffTrack"`, `"InPitStall"`, `"ApproachingPits"`, `"OnTrack"`, `"Unknown"`). Raw iRacing codes remain available under `iracing.PlayerTrackSurface` and `iracing.PlayerTrackSurfaceMaterial`.

## 0.3.0 — 2026-04-12

### Features

- **`--stream` mode for `ost-cli parse`** — skips full-file scans (lap index, track outline) and begins frame output immediately after reading file headers. Channels are discovered from the first frame and included in the header. `laps` and `track_outline` are omitted (`null`) in stream mode.
- **`SessionHeader.laps` and `track_outline` are now nullable** — `null` when unavailable (stream mode), present as arrays otherwise. `channels` is always populated.

## 0.2.0 — 2026-04-11

First tagged release. Everything below is the work that has landed since
the project began; future releases will document deltas from the previous
version.

### Features

- **iRacing adapter** with live telemetry on Windows and .ibt file replay on all platforms
- **Demo adapter** for testing without a simulator connected
- **Web dashboard** with real-time telemetry visualization via SSE streaming
- **.ibt file replay** with full playback controls (play, pause, scrub, loop markers, lap navigation)
- **Saved replay browser** with NDJSON+ZSTD persistence for loading previous sessions
- **Graph widgets** with multi-Y-axis support, arbitrary channel plotting, crosshair tooltips synced across graphs, click-to-seek, horizontal scroll-to-seek, and graph presets management
- **Vehicle widget** with steering wheel visualization, vertical pedal bars, and max G-force tracking with canvas markers
- **Wheels widget** with tire temps, shock velocity, load-colored tire rectangles, per-tread wear zones, and data availability warnings
- **All Channels widget** with unit display, wildcard/regex filtering, update frequency control, and create-graph integration
- **Computed channels** allowing user-defined JS expressions compiled via `new Function()`
- **Boolean and text/enum channel graphing** displayed as colored bars
- **Channel picker** with checkboxes, search aliases, left-aligned labels, and enabled channels sorted to top
- **Graph legend** with click-to-toggle visibility, hover-to-dim other series, and dot-to-X on hover
- **Remote streaming** via header URL input with status feedback and auto-connect
- **API documentation widget** with endpoint reference and examples
- **Telemetry throughput indicator** in the header
- **GridStack.js layout** for drag-and-drop widget arrangement
- **Graph presets** with naming, suggestions, and management via settings modal
- **Lap navigation** and lap time computation from SessionTime deltas
- **Semantic colour scheme for channels: throttle=green, brake=red, clutch=blue, ABS=reddish; axis convention X=red, Y=green, Z=blue; motion categories differentiated by brightness/hue shift (g-force bright, rotation warm-shifted, rates lighter)

### API & Data

- **SSE-based status and sinks push** consolidated into a single SSE connection
- **Rate parameter** on SSE telemetry endpoints for client-side throttling
- **Optional authentication** via Bearer token, query parameter, or HTTP Basic with browser login prompt
- **Server-side history buffer** with seek-back and configuration UI
- **Telemetry persistence** with NDJSON+ZSTD compression
- **UDP sink** with configurable update rate (HTTP and file sinks removed)
- **Channel mask** for filtering telemetry fields in API responses
- **Chunked replay fetching** with pread optimization, abort support, and caching
- **IBT conversion endpoint** (`POST /api/convert/ibt`) — upload .ibt file and stream back ZSTD-compressed NDJSON without buffering entire output in memory
- **`ost-parse` crate** — streaming NDJSON parse path shared between the CLI and HTTP front-ends, bounded by channel set size rather than session length
- **`ost-cli` binary** — synchronous `ost-cli parse <input>` command that streams NDJSON to stdout or a file, with `--format`, `--output`, and `--mode sparse|dense|compact` options and stdin spooling for pipeline use
- **Stateless parse endpoint** (`POST /api/parse`) — streams NDJSON frames from an uploaded replay file via raw body or multipart upload, with `?format` and `?mode` query parameters and no shared server state between concurrent requests

### Infrastructure

- GitHub Actions CI/CD workflows with release builds
- Windows cross-compilation support via cargo-xwin
- Comprehensive test suites across all crates
- Modular UI source files with build.rs concatenation
- `just` task runner for all build commands
- Apache 2.0 license

### Improvements

- Replay scrubbing optimized with pread, abort, caching, and field filtering
- Replay bar UX improvements with sticky positioning
- Full viewport fetch during scrubbing instead of cursor-only chunks
- Interval ticker with frame skipping for smooth real-time replay playback
- Explicit chunk tracking replacing fragile `_hasChunk` approach
- Parallel viewport chunk loading to eliminate graph gaps
- Adapter frame suppression during active replay
- Blocking I/O moved off async runtime to fix replay upload hangs
- Float values rounded to 5 decimal places in API responses
- Settings converted to modal dialog
- Improved header button visibility
- Graph time window scales horizontal scroll speed proportionally
- En-dash used for range representation in channels pane
- Default graph channels include clutch, ABS active, and steering angle
- Channel labels improved with full paths in picker

### Bug Fixes

- Fixed memory leaks in web UI
- Fixed replay graphs not populating when seeking past loaded data
- Fixed replay UI not restoring on page reload
- Fixed replay chunk fetch flood from unawaited `ensureLoaded` calls
- Fixed graph timescale shrinking at replay start/end boundaries
- Fixed duplicate frame fetch requests during playback and scrubbing
- Fixed chunk cache discarding non-adjacent prefetch data
- Fixed missing session/weather/wheels data in replay mode
- Fixed channel picker silently failing when no frame data or on render error
- Fixed channel picker and preset menu clipped by `overflow:hidden`
- Fixed extras field mask case sensitivity
- Fixed iRacing extras data grouped under "Extras" section; now appears under its own adapter-named section (e.g. "Iracing")
- Fixed default graph channels using wrong units for steering and angular velocity
- Fixed crosshair on paused replay
- Fixed live telemetry throughput calculation
- Fixed iRacing wheel data extraction on Windows
- Fixed Windows build Send/Sync and lifetime issues
- Fixed create-graph button, graph prefetch/loading, and lap time correlation
- Fixed error display in graphs when data regions fail to load
