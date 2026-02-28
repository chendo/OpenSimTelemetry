# Roadmap

Agent Instructions: Pick idea from the top, ask any clarifying questions, make a plan, and confirm before working. Remove entry from list when completed and committed.

* Bug: Still occasionally gaps in the graphed metrics
* Interface: First graph shows speed, rpm, throttle, brake, clutch, yaw rate, ABS active, steering angle
* Interface: Wheels viz doesn't make sense. Higher suspension travel means the wheels are loaded (braking causes front tires suspension travel to increase). Maybe instead of bar, have a rounded rectangle that represents the wheel and the redder the colour, the more loaded that wheel is 
* Data model: Add Computed Metrics, which takes JS/TS that can fetch metrics and process them, to return new metrics. The JS ideally is compiled or something so it's very fast.
* API: Add authentication via Authorization header or query param. Defaults to no authentication
* Interface: Optional HTTP Basic authentication
* Persistence: Allow persistence of replay data. NDJSON + ZSTD. Buffer can be downloaded by the user in the browser, or automatically writes to disk. Store in ~/Documents/OpenSimTelemetry/telemetry/ on Windows, ~/.opensimtelemetry/telemetry/ on mac/linux. Filename: YYYY-MM-DD_track_car.ost.ndjson.zstd
* Interface: Allow browsing and loading of saved replays in those default folders. Replays are streamed from disk rather than loading it all into memory as these can be quite big.
* Sinks: Remove file sink and HTTP post option. Add update rate option, default to 60hz
* Interface: Add API pane, which has short instructions and examples to access the API
* API: Add `rate` parameter to telemetry API endpoints that affects update rate which can be from `0.0-60.0`. Defaults to 60. If set to 1, that's one update per second, 0.5 is once every two seconds.
* Feature: Can optionally point to another OST instance to stream data from. Text box at the top that defaults to http://localhost:9100/
* Interface: Add warning in Wheels pane that some games don't have live data for all metrics here