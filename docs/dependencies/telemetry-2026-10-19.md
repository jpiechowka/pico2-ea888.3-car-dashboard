# Poll telemetry dependency

`dashboard-core` now directly pins `pin-project-lite = 0.2.17`, already resolved
in the firmware, host and configurator graphs. The root lock gains that same
version; other affected locks only add the dependency edge. No existing version
was upgraded for this change.

The [documented projection macro](https://docs.rs/pin-project-lite/0.2.17/pin_project_lite/)
and locked source were checked: the crate is `no_std`, has no runtime allocation
and lets a `Future` wrapper safely project its pinned inner future without custom
unsafe code. The wrapper holds the inner future once. Nested async measurement
functions had retained extra large radio-future storage in the linked image.

Regression tests bound wrapper storage around a 4 KiB `!Unpin` future, check
address stability between polls, and retain cancellation, suspended-time and
error-propagation checks. ARM linked-image measurements verify actual task RAM;
native object sizes alone do not establish target memory use.
