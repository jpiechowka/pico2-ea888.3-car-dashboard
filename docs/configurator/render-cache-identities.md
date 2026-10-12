# Exact render cache identities

Configuration and graph cache keys are runtime proofs with private fields. They
are never serialized, derived from an address, or computed as a content hash.
Configuration validation and ConfigSlot publication issue a new identity only
after accepting a complete configuration. Cloning a validated configuration
preserves its identity; semantic equality and v3 JSON/EAV ignore it.

Each acquisition model receives one origin. Graph identities contain that
origin, the channel, and a checked revision. Graph samples, gaps and resets
advance the revision. Copies of an unchanged snapshot preserve the identity.
Channel is necessary because callers can move public sensor snapshots between
slots. Public standalone sensor snapshots are untracked.

The retained cache stores configuration identity plus widget index and the graph
identity. Text, source, quality, geometry, gauge and overlay comparisons remain
part of rendering. Missing identity forces cell redraw on each physical buffer.
Exhaustion therefore cannot freeze pixels or depend on successful diagnostics.

## Exhaustion and reporting lifetime

Checked 32-bit issuers never wrap. The last successful issue is still tracked;
only a failed increment reports exhaustion. Failed graph revisions remain
untracked through later samples and resets. Config validation remains usable
when its cache identity cannot be issued.

`CacheExhaustionStatus` and `CacheExhaustionSeen` are two-byte masks. Only ten
typed causes can be constructed: config issuer, acquisition issuer and eight
graph channels. Public untracked snapshots have empty status. Core code does no
logging and provides a bounded iterator and pure newly-seen operation.

Firmware keeps one seen mask for the boot, observes delivered status before
sample acceptance, and sends new causes to the existing log sink. Publication
compares status as well as sample/reset sequence, so metadata-only transitions
survive duplicate filtering without advancing measurement time. Sticky status
survives Watch coalescing. The browser keeps one mask in the mounted Studio Ui
across editor, simulator and preview replacement. Prepared preview status is
available after a render error. New causes produce one visible and one console
warning. Clearing visible history does not clear the mask; a new boot or mount
starts a new lifetime. A dropped sink attempt does not retry every frame.

## Measured ARM storage

Pinned nightly-2026-10-12, Cortex-M33, release, traced CPU280/SPI70 builds:

| Type | Before (bytes) | After (bytes) |
| --- | ---: | ---: |
| DashboardCache | 9,056 | 3,040 |
| CellKey | 544 | 168 |
| Graph payload / optional identity | 252 | 12 |
| Optional Widget / identity and index | 144 | 8 |
| SensorState | 1,240 | 1,248 |
| SensorSnapshot | 328 | 336 |
| DashboardSnapshot | 3,680 | 3,744 |
| AcquisitionModel | 13,864 | 13,936 |
| ValidatedConfig | 9,676 | 9,684 |

The cache saves 6,016 bytes: 2,176 from widget keys and 3,840 from graph keys.
Including all owners, publication tuple, issuers and linker alignment, both
maximum artifacts save 5,808 static RAM bytes against the ConfigSlot baseline.
The graph token is 12 bytes including channel, not an eight-byte estimate.

| Maximum traced artifact | Static RAM | Core0 headroom | Boot | Core1 construction | Core1 runtime | Dashboard |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Demo | 439,824 | 84,464 | 2,356 | 7,824 | 16,184 | 27,288 |
| Vehicle | 449,808 | 74,480 | 2,356 | 26,464 | 12,616 | 28,320 |

Captures are `cache6-demo280-traced` and `cache6-vehicle280-traced` under the
ignored `firmware/target/bench-captures` directory. Both use the same normalized
8,462-byte maximum v3 EAV with SHA-256
`e8a02f13e60930e37d32b0c7762c2b2675d1ea132962c109c084d25f6bb89c38`.

Publication uses one shared outlined helper for normal and maintenance paths.
The gate counts its 3,792-byte frame, the 4,400-byte model snapshot and the
168-byte sensor helper along the nested runtime path. Vehicle construction is
`3232 + 22208 + 1024 = 26464`; runtime is
`3232 + max(3792 + 4400 + 168 + 1024, 8192) = 12616`.
The construction and runtime paths have separate admission calculations.

The 65,536-byte minimum headroom, 8,192/4,096-byte Core0/Core1 reserves and
10,240-byte object guards are unchanged. Named stack paths and reviewed machine
code pass the build gates. This is software artifact evidence; it is not a
whole-program stack high-water measurement, Pico FPS measurement or physical
hardware qualification.
