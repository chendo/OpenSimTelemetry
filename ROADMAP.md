# Roadmap

Agent Instructions: Pick idea from the top, ask any clarifying questions, make a plan, and confirm before working. Remove entry from list when completed and committed.

* Persistence: Allow persistence of replay data. NDJSON + ZSTD. Buffer can be downloaded by the user in the browser, or automatically writes to disk. Add option to set frequency (10, 30, 60 default, custom), Store in ~/Documents/OpenSimTelemetry/telemetry/ on Windows, ~/.opensimtelemetry/telemetry/ on mac/linux. Filename: YYYY-MM-DD_track_car.ost.ndjson.zstd. Add these preferences to the settings.
* Interface: Allow browsing and loading of saved replays in those default folders. Replays are streamed from disk rather than loading it all into memory as these can be quite big.
* Sinks: Remove file sink and HTTP post option. Add update rate option, default to 60hz
* Interface: Add API pane, which has short instructions and examples to access the API
* API: Add `rate` parameter to telemetry API endpoints that affects update rate which can be from `0.0-60.0`. Defaults to 60. If set to 1, that's one update per second, 0.5 is once every two seconds.
* Feature: Can optionally point to another OST instance to stream data from. Text box at the top that defaults to http://localhost:9100/
* API: Add authentication via Authorization header or query param. Defaults to no authentication
* Interface: Optional HTTP Basic authentication
* Interface: Add warning in Wheels pane that some games don't have live data for all metrics here
* Graphs: Scrolling horizontally on a graph will move the cursor