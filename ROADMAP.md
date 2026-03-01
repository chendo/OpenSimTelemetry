# Roadmap

Agent Instructions: Pick idea from the top, ask any clarifying questions, make a plan, and confirm before working. Remove entry from list when completed and committed.

* Interface: Devise common colour scheme for colours. Throttle should should be green, brake red, clutch blue. Brake-related things like ABS should be red-ish. X axis is red, Y is green, Z is blue. Ideally it will be clear and consistent. For motion, we will need to use different colours for its various XYZ but we should have a consistent way we change the colours so it's easy to tell if it's g-force, rotation, or velocity

## WIP: do not work on these

* Interface/API: Add settings for units like miles/km/meters, radians/degrees, mm/inches. For API, default to metric, radians, but come up with query params that allow changing the return values
* Bug: iRacing-specific data is no longer visible in metric lists. It should appear under its own iracing namespace, instead of extras.