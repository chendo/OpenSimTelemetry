# Roadmap

Agent Instructions: Read the road map, group related changes, ask any clarifying questions, make a plan, and confirm before working. Remove entry from list when completed and committed.

* Feature: Serving mode, where it allows OST to be deployed to a server for easy trial usage. Users can upload IBT files, which redirects them to a unique session ID. Management interface and API behind HTTP basic. Cap disk usage at 10gb by default, then starts deleting old ones.


## WIP: do not work on these


### Adapters

20. Adapter: Assetto Corsa Competizione (ACC) — shared memory reader on Windows for car telemetry, track data, weather
21. Adapter: F1 series (EA) — UDP packet receiver for F1 2024+ telemetry protocol, cross-platform
22. Adapter: rFactor 2 — shared memory adapter with multi-class and competitor data
23. Adapter: BeamNG.drive — soft-body physics telemetry with deformation and structural integrity data
24. Adapter: Generic UDP receiver — configurable NDJSON/binary/msgpack UDP listener so users can pipe arbitrary telemetry sources
