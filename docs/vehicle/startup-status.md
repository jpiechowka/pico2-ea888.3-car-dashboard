# Startup and transport status

The production path no longer plays the scripted 13-second loading/welcome
sequence or asserts ECU connection/vehicle identity before a response. Display
initialization retains the controller delays; the ordinary dashboard starts
once the display task and input paths have been initialized. Demo is still an
explicit synthetic data build, with no automatic fallback.

The header distinguishes NO LINK, STARTING, RETRY, RAW ONLY, WAIT ECU, DATA,
STALE and RESET REQ. DATA requires at least one fresh source-matching typed
measurement; it does not mean all channels work or that this ECU software is
qualified. Staleness is reevaluated from time even without a new publication.
RAW ONLY never implies ECU communication. RESET REQ is terminal transport
quarantine; reset/power cycling is required while the UI continues to run.

The Debug page shows the previous watchdog reason as WD FORCED, WD TIMEOUT or
OTHER. These use the locked HAL's `Watchdog::reset_reason`, which reads RP2350
WATCHDOG_REASON FORCE/TIMER. OTHER does not assert power-on rather than an
external or software reset. The reason is also logged at startup.

The shared status policy has table-driven source/freshness cases and actual
pixel comparisons of both retained framebuffer headers through every link
state. Existing popup removal and full/retained replays still pass. These host
checks do not measure cold/warm device startup time, first physical pixel or
first ECU sample; those remain hardware acceptance measurements.
