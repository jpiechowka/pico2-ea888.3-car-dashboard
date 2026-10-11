# Fixed host workload coverage

The final host suite contains 72 Criterion cases on the production pure Rust
modules. The fixture sources are in `host/benches/dashboard_perf.rs`,
`scene_replay/` and `coverage/`. The latter two are also compiled by host tests.
Each case uses 30 samples, one second of warmup and two seconds of measurement.
The [capture policy](benchmark-policy.md) requires frozen source, compiler and
runner identity for comparisons.

These are CPU measurements. Logical gaps in a replay do not sleep, perform UART
I/O or represent elapsed benchmark time. No host result establishes ECU support,
RP2350 cycles, DMA overlap or physical FPS.

## Workload matrix

| Area | Fixed work and preflight |
|---|---|
| Framebuffer | Full clear; solid and contiguous fills with full, narrow, odd and four-edge-clipped geometry; independent coordinate/pixel and iterator-consumption oracle |
| Text and widgets | Production boost, lambda, battery and temperature cell drawing; poisoned cell interiors must be completely repaired; PSI peak, normal labels and outlined values; same-rounded text and changing digit width |
| History | New measurement; no-new-sample tick and snapshot; constant and newest-impulse 60-point graphs; ring wrap/eviction in the bundled target-history and eight-frame replay |
| View model and cache | Typed/batch ingestion, Watch copy, alarms, cold software cache, same physical-buffer unit change, valid/stale/invalid/recovered transitions and dirty-cell counts |
| Damage | Stationary, one cell, dense, one digit, wider number, sparse separated cells, separated-cell full fallback and popup removal; every planned byte range replays to exact full pixels |
| Transport | RX12, ISO-TP single/128-byte messages, client success, NRC rejection, malformed framing, first-response timeout and NRC78 followed by rejection; terminal cases reject late results |
| Model to image | Eight-frame Demo acquisition and 17-step slow/bursty RX12/Session replays, each with paired full and retained paths; state, age, quality, peaks and pixels checked |
| Ownership and observability | UART ring copy, session/vehicle service, input mailbox, diagnostic distributions, frame windows, bounded logging and settings/maintenance models |
| Device kernels | Four bounded kernels also built for the opt-in target benchmark; host results use the same finite work and checksum, with no claimed host-to-device conversion |

Both outline configurations pass correctness and benchmark smoke checks.
Published timing captures state their exact feature flags; do not infer the
other configuration's timing from a smoke run. Software `DashboardCache` coldness
does not measure CPU or XIP cache coldness. Actual serial/overlap/partial DMA
timings remain a [target and hardware gate](target-benchmarks.md).

## Fill work counts

The throughput denominator is actual written bytes, twice the written pixels.
Solid fill has no source iterator. Contiguous fill includes color generation and
skipping; it need not consume invisible trailing source elements.

| Geometry (x, y, width, height) | Written pixels | Contiguous source elements consumed |
|---|---:|---:|
| 0, 0, 320, 240 | 76800 | 76800 |
| 3, 5, 1, 200 | 200 | 200 |
| 3, 9, 127, 81 | 10287 | 10287 |
| -3, -2, 329, 245 | 76800 | 79612 of 80605 supplied |

## Scene and damage work counts

All setup, allocation, input preparation and deliberate cache warming occur
outside the measured batched closures. Terminal client fixtures start from the
same newly constructed, request-started and TX-acknowledged owner; their timings
include terminal processing, not that setup. A pending-then-negative case
contains two receive events. Full transaction cases remain separate.

The eight-frame scene replay changes all eight channels, advances full history
rings and changes overlays, backgrounds and FPS. It warms both physical buffer
caches first. Its alternating units are fixed per buffer; the separate
`unit_toggle_one_frame` changes units on the same buffer. Critical overlays in
the replay are scripted rendering coverage, not an alarm-activation test.
Prepared-scene cases exclude ingestion; `demo_model_to_*` includes ingestion and
snapshot construction. Both exclude Watch, damage planning and display transport.

| Cache transition | Render calls | Dirty cells per call | Full clear |
|---|---:|---|---|
| Cold cache | 1 | 8 | Yes |
| Same rounded text | 1 | 0 | No |
| Unit toggle | 1 | 1 | No |
| 99 to 100 coolant value/peak | 1 | 1 | No |
| Valid to stale to invalid to recovered | 3 after the initial valid frame | 1, 1, 1 | No |

The following damage counts use the default outline. They are emitted again in
the benchmark preflight. The planner measures region selection and strided
payload-range enumeration, without an intermediate packing buffer or DMA.

| Damage input | Bytes | Regions | Payload ranges |
|---|---:|---:|---:|
| Instant FPS 40.0 to 41.0 | 2048 | 1 | 32 |
| Coolant 99 to 100, including peak text | 6656 | 2 | 48 |
| Two scripted 8x12 patches in separate cells | 1024 | 2 | 32 |
| Real separated boost/IAT changes, full fallback | 153600 | 1 | 1 |
| Reset popup removal | 30720 | 1 | 80 |

The 17-step diagnostic replay spans logical times 1 through 9001 ms and parses
84 RX12 bytes in seven complete records, with split chunks and a three-record
burst. Six requests yield four positives (including capability discovery), one
timeout and one malformed payload. Unsolicited burst replies cannot change the
accepted peak. Final coolant extrema are 90 and 105 C; the last accepted sample
is at 5710 ms and becomes stale before the last render. Each of 17 software
render calls must match a full image. This is a synthetic qualified RX12 fixture,
not evidence that an installed adapter supplies trustworthy DLC/flags. It
excludes UART polling, bootstrap, Watch, damage and physical transfers.

The two unchanged one-cell rendering cases now own their initial pixels and
cache independently. Running a filter or the preceding full-render case cannot
silently change the retained case's initial state. Historical captures with the
older fixture remain historical, and cannot be compared under the new fixture
identity.
