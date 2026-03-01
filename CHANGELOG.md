# Changelog

All notable changes to OpenSimTelemetry are documented in this file.

## Unreleased

### Features

- **iRacing adapter** with live telemetry on Windows and .ibt file replay on all platforms
- **Demo adapter** for testing without a simulator connected
- **Web dashboard** with real-time telemetry visualization via SSE streaming
- **.ibt file replay** with full playback controls (play, pause, scrub, loop markers, lap navigation)
- **Saved replay browser** with NDJSON+ZSTD persistence for loading previous sessions
- **Graph widgets** with multi-Y-axis support, arbitrary metric plotting, crosshair tooltips synced across graphs, click-to-seek, horizontal scroll-to-seek, and graph presets management
- **Vehicle widget** with steering wheel visualization, vertical pedal bars, and max G-force tracking with canvas markers
- **Wheels widget** with tire temps, shock velocity, load-colored tire rectangles, per-tread wear zones, and data availability warnings
- **All Metrics widget** with unit display, wildcard/regex filtering, update frequency control, and create-graph integration
- **Computed metrics** allowing user-defined JS expressions compiled via `new Function()`
- **Boolean and text/enum metric graphing** displayed as colored bars
- **Metric picker** with checkboxes, search aliases, left-aligned labels, and enabled metrics sorted to top
- **Graph legend** with click-to-toggle visibility, hover-to-dim other series, and dot-to-X on hover
- **Remote streaming** via header URL input with status feedback and auto-connect
- **API documentation widget** with endpoint reference and examples
- **Telemetry throughput indicator** in the header
- **GridStack.js layout** for drag-and-drop widget arrangement
- **Graph presets** with naming, suggestions, and management via settings modal
- **Lap navigation** and lap time computation from SessionTime deltas
- **Semantic colour scheme** for metrics: throttle=green, brake=red, clutch=blue, ABS=reddish; axis convention X=red, Y=green, Z=blue; motion categories differentiated by brightness/hue shift (g-force bright, rotation warm-shifted, rates lighter)

### API & Data

- **SSE-based status and sinks push** consolidated into a single SSE connection
- **Rate parameter** on SSE telemetry endpoints for client-side throttling
- **Optional authentication** via Bearer token, query parameter, or HTTP Basic with browser login prompt
- **Server-side history buffer** with seek-back and configuration UI
- **Telemetry persistence** with NDJSON+ZSTD compression
- **UDP sink** with configurable update rate (HTTP and file sinks removed)
- **Metric mask** for filtering telemetry fields in API responses
- **Chunked replay fetching** with pread optimization, abort support, and caching
- **IBT conversion endpoint** (`POST /api/convert/ibt`) — upload .ibt file and stream back ZSTD-compressed NDJSON without buffering entire output in memory

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
- En-dash used for range representation in metrics pane
- Default graph metrics include clutch, ABS active, and steering angle
- Metric labels improved with full paths in picker

### Bug Fixes

- Fixed memory leaks in web UI
- Fixed replay graphs not populating when seeking past loaded data
- Fixed replay UI not restoring on page reload
- Fixed replay chunk fetch flood from unawaited `ensureLoaded` calls
- Fixed graph timescale shrinking at replay start/end boundaries
- Fixed duplicate frame fetch requests during playback and scrubbing
- Fixed chunk cache discarding non-adjacent prefetch data
- Fixed missing session/weather/wheels data in replay mode
- Fixed metric picker silently failing when no frame data or on render error
- Fixed metric picker and preset menu clipped by `overflow:hidden`
- Fixed extras field mask case sensitivity
- Fixed iRacing extras data grouped under "Extras" section; now appears under its own adapter-named section (e.g. "Iracing")
- Fixed default graph metrics using wrong units for steering and angular velocity
- Fixed crosshair on paused replay
- Fixed live telemetry throughput calculation
- Fixed iRacing wheel data extraction on Windows
- Fixed Windows build Send/Sync and lifetime issues
- Fixed create-graph button, graph prefetch/loading, and lap time correlation
- Fixed error display in graphs when data regions fail to load
