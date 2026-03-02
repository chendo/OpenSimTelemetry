# Roadmap

Agent Instructions: Pick idea from the top, ask any clarifying questions, make a plan, and confirm before working. Remove entry from list when completed and committed.

* Interface: Units need to be scale appropriate. For example, speeds are km/h, but acceleration is better off as mm/s^2 and velocity as mm/s. Lap distance should be in m or km, not mm. Air Pressure is showing as 92897 Pa which seems wrong. With metrics, the ones that we map we should normalise into values that make sense, but the iracing namespace ones should be 'left raw' in case we don't support a mapping yet, or we converted it wrong or something. Does 158.6 kPa sound right for tire pressure?
* Interface: rpm y-axis intervals should be whole 1000s
* Interface: display a y-axis for every unique unit
* Interface: When live, and scrolled back, graphs should continue updating with live data and the user is viewing that part

## WIP: do not work on these

* 

### Adapters

20. Adapter: Assetto Corsa Competizione (ACC) — shared memory reader on Windows for car telemetry, track data, weather
21. Adapter: F1 series (EA) — UDP packet receiver for F1 2024+ telemetry protocol, cross-platform
22. Adapter: rFactor 2 — shared memory adapter with multi-class and competitor data
23. Adapter: BeamNG.drive — soft-body physics telemetry with deformation and structural integrity data
24. Adapter: Generic UDP receiver — configurable NDJSON/binary/msgpack UDP listener so users can pipe arbitrary telemetry sources
