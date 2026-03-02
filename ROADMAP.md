# Roadmap

Agent Instructions: Pick idea from the top, ask any clarifying questions, make a plan, and confirm before working. Remove entry from list when completed and committed.

* Data model: shift_light should be part of electronics, not driver, and fuel_capacity & setup_name should be on vehicle. car_name should be on vehicle. Tick/Timestamp shouldn't be in their own namespace, same with game. Maybe meta?
* Interface: If iRacing has been detected, but no data, it should say "Simulator not running"
* Interface: The default layout should have two graphs. The first one is labeled Pedals and Speed, and have Speed, RPM, throttle, broke, clutch, ABS. The second graph is labelled Steering, includes steering input, yaw rate, and other relevant inputs.
* Interface: Y-axes should be on the right side of the chart


## WIP: do not work on these


### Adapters

20. Adapter: Assetto Corsa Competizione (ACC) — shared memory reader on Windows for car telemetry, track data, weather
21. Adapter: F1 series (EA) — UDP packet receiver for F1 2024+ telemetry protocol, cross-platform
22. Adapter: rFactor 2 — shared memory adapter with multi-class and competitor data
23. Adapter: BeamNG.drive — soft-body physics telemetry with deformation and structural integrity data
24. Adapter: Generic UDP receiver — configurable NDJSON/binary/msgpack UDP listener so users can pipe arbitrary telemetry sources
