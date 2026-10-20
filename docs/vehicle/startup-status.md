# Startup and transport status

After display initialization, the configurable startup path shows completed
boot checks and current transport status, then the selected splash and Dashboard.
The defaults are a brief 600 ms status screen and a 2500 ms animation. It never
asserts ECU connection or vehicle identity before evidence exists. Acquisition
continues throughout; faults and protected recovery exit startup immediately.
See [startup settings and previews](../configurator/startup.md). Demo remains an
explicit synthetic data build, with no automatic fallback.

The header distinguishes NO LINK, STARTING, RETRY, RAW ONLY, WAIT ECU, DATA,
STALE and RESET REQ. DATA requires at least one fresh source-matching typed
measurement; it does not mean all channels work or that this ECU software is
qualified. Staleness is reevaluated from time even without a new publication.
RAW ONLY never implies ECU communication. RESET REQ is terminal transport
quarantine; reset/power cycling is required while the UI continues to run.

The Debug page and startup log use the shared `BootEvidence` classifier over
the raw RP2350 WATCHDOG_REASON and validated ROM boot metadata. Labels are
WD FORCED, WD TIMEOUT, WD BOTH, WD UNKNOWN, ROM REBOOT or OTHER. Known nonnormal
intentional ROM boot types produce ROM REBOOT; unknown reason bits stay visible.
OTHER does not assert power-on rather than an external or software reset.
A controlled NORMAL ROM reboot also uses the watchdog timer and may invalidate
the retained stage record. WD TIMEOUT describes the hardware indication, not
proof that firmware stalled or missed a feed. See the
[shared reset trace](../validation/probe-slot4-followup-2026-10-21.md#post-flash-watchdog-evidence).

The shared status policy has table-driven source/freshness cases and actual
pixel comparisons of both retained framebuffer headers through every link
state. Existing popup removal and full/retained replays still pass. These host
checks do not measure cold/warm device startup time, first physical pixel or
first ECU sample; those remain hardware acceptance measurements.
