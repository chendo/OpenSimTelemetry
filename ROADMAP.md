# Roadmap

Agent Instructions: Pick idea from the top, ask any clarifying questions, make a plan, and confirm before working. Remove entry from list when completed and committed.

## Everything below: Ask any clarifying questions before starting work. Batch if you need
### Dashboard & Widgets

1. Interface: Custom gauge widgets — circular tachometer, boost gauge, speed dial with threshold bands and needle animation
5. Interface: Telemetry recording indicator + session management UI — show when persistence is recording, list/rename/delete/tag saved sessions from UI
6. Interface: Data export wizard — export selected metrics to CSV/JSON for a time range with aggregation options
7. Interface: Configurable widget dashboard profiles — save/restore multiple dashboard layouts (e.g. "race", "setup", "coaching")
* Custom widgets: iframe, postMessage APIs with some helpers to make consuming events easy
* Interface/API: Add settings for units like miles/km/meters, radians/degrees, mm/inches. For API, default to metric, radians, but come up with query params that allow changing the return values

### API & Data

10. API: Time-series aggregation endpoints — `/api/history/aggregate?duration=60s&metrics=vehicle.speed` returning min/max/avg/stddev over configurable windows
13. API: Binary wire protocol option — MessagePack/CBOR format via `?format=binary` for SSE frames (10-15x smaller)
15. API: Interactive API docs — generate OpenAPI 3.0 schema, serve Swagger UI at `/docs`

### Persistence

17. Persistence: Retention policies and storage management — auto-cleanup rules (keep last N sessions, delete older than X days), show disk usage per session

### Adapters

20. Adapter: Assetto Corsa Competizione (ACC) — shared memory reader on Windows for car telemetry, track data, weather
21. Adapter: F1 series (EA) — UDP packet receiver for F1 2024+ telemetry protocol, cross-platform
22. Adapter: rFactor 2 — shared memory adapter with multi-class and competitor data
23. Adapter: BeamNG.drive — soft-body physics telemetry with deformation and structural integrity data
24. Adapter: Generic UDP receiver — configurable NDJSON/binary/msgpack UDP listener so users can pipe arbitrary telemetry sources

### Performance & Infrastructure

26. Infra: Adaptive frame rate throttling — detect client connection quality, dynamically adjust SSE frame rate to maintain smooth UI
27. Infra: End-to-end test suite — integration tests uploading sample .ibt files, verifying parsed frames, golden/snapshot testing for deserialization
28. Infra: Load testing CLI — `cargo bench` suite measuring frame throughput, SSE latency, graph rendering; baseline tracking across releases
30. Infra: Adapter developer guide + template — `cargo generate` template crate with walkthrough docs for mapping game vars to TelemetryFrame

## WIP: do not work on these
