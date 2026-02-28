# Roadmap

Agent Instructions: Pick idea from the top, ask any clarifying questions, make a plan, and confirm before working. Remove entry from list when completed and committed.

* Sinks: Remove file sink and HTTP post option. Add update rate option, default to 60hz
* Interface: Add API pane, which has short instructions and examples to access the API
* API: Add `rate` parameter to telemetry API endpoints that affects update rate which can be from `0.0-60.0`. Defaults to 60. If set to 1, that's one update per second, 0.5 is once every two seconds.
* Feature: Can optionally point to another OST instance to stream data from. Text box at the top that defaults to http://localhost:9100/
* API: Add authentication via Authorization header or query param. Defaults to no authentication
* Interface: Optional HTTP Basic authentication
* Interface: Add warning in Wheels pane that some games don't have live data for all metrics here
* Graphs: Scrolling horizontally on a graph will move the cursor