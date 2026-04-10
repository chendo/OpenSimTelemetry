# ost-parse

Streaming NDJSON parse path for OpenSimTelemetry replay files.

This crate is the single source of truth for converting a replay file
(currently `.ibt`, more formats welcome) into a line-delimited JSON
stream that downstream tools can consume frame by frame. Two front-ends
share the same parser:

- **`ost-cli`** — a synchronous binary that TM (or any caller) shells
  out to per upload. Subprocess gives true parallelism, OS-bounded
  memory, and automatic cleanup on exit.
- **`POST /api/parse`** in `ost-server` — a stateless HTTP endpoint that
  does the same thing over HTTP, for callers that prefer not to fork.

Both call `ost_parse::ReplayParser::parse_to_ndjson` directly. There is
exactly one implementation of how a replay file becomes the wire format.

## Memory model

The parser holds an `IbtFile` handle, the discovered channel set
(~hundreds of strings), and one frame's worth of channel values at a
time. Peak memory is bounded by the channel set, **not** by session
length. Output is streamed line-by-line into a `Write` impl — wrap it in
a `BufWriter` for efficiency.

## Wire format

Newline-delimited JSON. **Line 1** is a `SessionHeader`. **Every line
after that** is a JSON object representing one frame, keyed by
dot-separated channel names.

### Header line

```json
{
  "format": "ost-parse",
  "version": 1,
  "source_format": "ibt",
  "mode": "sparse",           // or "dense" / "compact"
  "metadata": {
    "track_name": "Tsukuba Circuit 2k Full",
    "car_name": "Formula Renault 2.0",
    "tick_rate": 60.0,
    "duration_secs": 1220.4,
    "file_size": 76251409,
    "replay_id": "03cf2ad37f4c0cf2"
  },
  "laps": [
    { "lap_number": 1, "lap_index": 0, "start_frame": 0, "lap_time_secs": 83.21, "incomplete": false, "invalid_reason": null }
  ],
  "track_outline": [[35.6, 140.1], [35.6, 140.1], ...],
  "channels": ["meta.tick", "motion.g_force.x", "vehicle.rpm", "vehicle.speed", ...],
  "total_frames": 73225
}
```

- `format` — always `"ost-parse"`.
- `version` — wire format version. Bumped on incompatible changes.
- `source_format` — `"ibt"` for v1.
- `mode` — `"sparse"`, `"dense"`, or `"compact"` (see below).
- `metadata.replay_id` — stable 16-char hex hash of
  `(file_size, total_frames, track_name, car_name)`. Identical to the
  hash that `ost-server::replay::ReplayState::from_file` produces, so
  callers can use one ID across both code paths.
- `laps` — lap boundary index, mirroring
  `ost_adapters::ibt_parser::IbtFile::build_lap_index`.
- `track_outline` — `[lat, lng]` pairs for the track map widget. On-track
  points only.
- `channels` — sorted union of channel names discovered during channel
  discovery (every 100th sample plus the last sample). Informational in
  sparse mode; defines the per-frame numeric set in dense mode; defines
  the positional column index in compact mode (numeric-only).
- `total_frames` — total sample count from the source file's header.

### Frame lines

Each subsequent line is a JSON object: keys are dot-separated channel
paths, values are numbers, booleans-as-numbers (`0`/`1`), or strings.

```json
{"meta.tick":13,"vehicle.speed":38.3,"vehicle.rpm":5210.5,"motion.g_force.x":0.04,...}
```

#### Channel naming rules

- Nested objects → dot-joined keys (`motion.g_force.x`).
- Booleans → `0` or `1`.
- Arrays of length ≤ 8 with scalar elements → expanded to `name.0`,
  `name.1`, …
- Arrays longer than 8, or containing nested objects, are skipped (not
  channelised at all).
- `null` is dropped.
- Strings pass through unchanged.

### Frame emission modes

The wire format supports three emission modes, selected via
`--mode <mode>` on the CLI or `?mode=<mode>` on the HTTP endpoint.
Default is `sparse`.

#### `sparse` (default)

Each frame line is a **JSON object** containing only the channels that
have a real value at that tick. Missing channels are simply absent from
the object. Carry-forward (the rule "use the previous emitted value if
this tick is missing this channel") is the consumer's responsibility.
The header's `channels` list lets consumers pre-allocate columns.

Sparse output is the smallest on the wire and the simplest to produce.

```jsonc
{"vehicle.speed":38.3,"vehicle.rpm":5210.5}
{"vehicle.speed":38.4,"vehicle.rpm":5215.0}
```

#### `dense`

Each frame line is a **JSON object** containing every numeric channel
from the discovered union, with the carry-forward rule applied: if a
channel is missing or non-finite this tick, the previous emitted value
is sent (or `0` if no previous value exists yet). String channels
remain sparse — they appear only in frames where they're populated,
because there's no meaningful "carry-forward 0" for a string.

Dense mode guarantees equal-length numeric columns at the cost of much
larger output (every channel name repeats every line). Use this when
the consumer doesn't want to do carry-forward itself.

```jsonc
{"vehicle.speed":38.3,"vehicle.rpm":5210.5,"pit.in_pit":0,...}
{"vehicle.speed":38.4,"vehicle.rpm":5215.0,"pit.in_pit":0,...}
```

#### `compact`

Each frame line is a **positional JSON array** of numbers, one slot per
entry in `header.channels`, in the same order. Same carry-forward
semantics as `dense`: if a channel is missing or non-finite this tick,
the previous emitted value is written (or `0` if no previous value).

Compact mode is **numeric-only**: string channels are excluded entirely
from `header.channels` and from every frame, because positional arrays
can't accommodate variable per-frame string slots. If you need strings,
use `sparse` or `dense`.

Compact is the smallest-on-the-wire option (no repeated key strings) at
the cost of needing the header to decode each frame.

```jsonc
// header: "channels": ["motion.g_force.x", "vehicle.rpm", "vehicle.speed", ...]
[0.04, 5210.5, 38.3, ...]
[0.05, 5215.0, 38.4, ...]
```

## Usage

### CLI

```bash
# Auto-detect format from extension, sparse NDJSON to stdout.
ost-cli parse file.ibt > out.ndjson

# Dense mode, output to a file.
ost-cli parse file.ibt --mode dense --output out.ndjson

# Compact (positional-array) mode.
ost-cli parse file.ibt --mode compact --output out.ndjson

# From stdin (--format is required because we can't infer from a name).
cat file.ibt | ost-cli parse - --format ibt > out.ndjson
```

### HTTP

`POST /api/parse` accepts either a raw body or a multipart upload.
Content-Type drives the dispatch.

```bash
# Raw body
curl -H "Authorization: Bearer $OST_KEY" \
     -H "Content-Type: application/octet-stream" \
     --data-binary @file.ibt \
     'http://localhost:9100/api/parse?format=ibt'

# Dense mode
curl -H "Authorization: Bearer $OST_KEY" \
     -H "Content-Type: application/octet-stream" \
     --data-binary @file.ibt \
     'http://localhost:9100/api/parse?format=ibt&mode=dense'

# Compact mode (positional arrays)
curl -H "Authorization: Bearer $OST_KEY" \
     -H "Content-Type: application/octet-stream" \
     --data-binary @file.ibt \
     'http://localhost:9100/api/parse?format=ibt&mode=compact'

# Multipart (browser-friendly)
curl -H "Authorization: Bearer $OST_KEY" \
     -F file=@file.ibt \
     'http://localhost:9100/api/parse'
```

The response is `Content-Type: application/x-ndjson` and is streamed
line by line — clients can start reading the header before the rest of
the file is parsed. Closing the connection mid-stream causes the parser
to exit via `BrokenPipe` and clean up its temp file.

### Library

```rust
use std::io::{stdout, BufWriter};
use ost_parse::{parser_for_extension, ParseOptions};

let parser = parser_for_extension("ibt").expect("ibt parser");
let mut out = BufWriter::new(stdout().lock());
parser.parse_to_ndjson(
    std::path::Path::new("file.ibt"),
    &mut out,
    &ParseOptions::sparse(),
)?;
```

## Adding a new format

1. Drop a new module under [`src/formats/`](src/formats/) implementing
   the [`ReplayParser`](src/formats/mod.rs) trait.
2. Register its extension(s) in `parser_for_extension`.
3. Add an integration test under [`tests/`](tests/) gated on a fixture
   file.

## Testing

```bash
# Fast unit + smoke tests (default).
cargo test -p ost-parse

# Slow end-to-end fixture tests (parses all 73k frames in both modes).
cargo test -p ost-parse -- --include-ignored
```

The slow integration tests are gated on `fixtures/race.ibt` being
present and skipped silently otherwise, so the suite stays portable.
