# Bounded partial updates

The renderer owns two full framebuffers. Before queueing a new image, it waits
for the previous completion and compares against that returned buffer while
both leases are still owned by the UI. Only afterward may the old reference
become the next render buffer. `PanelHistory` records each buffer's generation
and orientation, the last completed panel frame and one pending transfer. A stale,
modified, differently oriented or uncompleted reference cannot authorize a partial
update. Damage plans bind their rectangles and row strides to native geometry.

Every orientation change forces a full repaint and full transfer, including
identical black pixels and 180° flips. Render invalidation also compares the queued
panel target, so rapid landscape → portrait → landscape selection cannot reuse a
stale retained image. Partial updates resume only from matching completed content.

The planner compares exact RGB565 pixels in 16 by 16 tiles, unions adjacent
tiles and merges equal vertical runs. It holds at most 16 rectangles. Exceeding
that budget or selecting rectangles covering at least 75% of the panel
uses one full-frame transfer. More than 128 payload DMA starts also forces a
full frame: an alternating vertical-stripe fixture otherwise required 2400
starts for half an image. The fixture failed before adding this guard.
These thresholds are provisional; physical
SPI command/DMA setup costs have not been measured.

Unchanged images at the same orientation emit no commands. Their ready token is
discarded, the last completed reference remains untouched, and the UI suspends for 16 ms. Input,
acquisition and render progress remain supervised while display progress is
marked idle. A later queue generation starts a fresh display watchdog deadline,
even when the idle-to-pending transition falls between supervisor observations.

## Shared transport and failure path

The production `flush_oriented_plan` is also executed by the host panel model.
It validates full replacement before changing MADCTL and then uses `flush_plan`
for the pixel windows. Each request carries immutable orientation metadata. A
region emits CASET, RASET and RAMWR with inclusive bounds. Full-width bands
use one contiguous slice; narrower rectangles use the framebuffer stride for
each row. CS stays low throughout each region's RGB565 payload. No additional
packing allocation or live buffer alias is introduced.

The started callback occurs once, after polling the first payload DMA write.
Completion and payload telemetry cover all windows, including subsequent
setup/gaps. The owner is retained until every payload has returned from the HAL
and CS is high. A command/row failure stops further writes, closes CS and
enters the existing quarantine/reset path. Timeouts never cancel payload DMA.

`full-frame` bypasses damage selection. `serial-display` additionally waits
for each completion before rendering again. CI compiles the stock overlap
path, trace plus 300 MHz path, and serial/full-frame fallback separately.

## Source checks

1. [Sitronix ST7789V specification](https://files.seeedstudio.com/wiki/Grove-1.2inch-ips-display/ST7789V_SPEC_V1.2.pdf),
   sections 9.1.20 through 9.1.22: window address parameters and RAMWR traversal.
   The selected MADCTL MV setting exchanges the address dimensions. Partial
   memory writes do not enable the controller's separate partial-display mode.
2. [Pimoroni's ST7789 implementation](https://github.com/pimoroni/pimoroni-pico/blob/b41010d1e9809a62c878e067d7e17ae56f3c4684/drivers/st7789/st7789.cpp)
   and [Pico Display Pack 2.8 specification](https://shop.pimoroni.com/products/pico-display-pack-2-8):
   320 by 240 addressing, command/data selection, RGB565 operation and board identity.
3. Exact locked Embassy RP 0.10.0 SPI/DMA source: payload DMA is armed before
   first suspension and SPI busy/RX state is drained before returning. The
   [published source](https://docs.rs/embassy-rp/0.10.0/src/embassy_rp/spi.rs.html)
   was checked directly; this does not establish generic cancellation safety.

The sleep-out delay was also checked: the cited specification requires at
least 5 ms before subsequent commands; the existing 10 ms exceeds that.
Its separate 120 ms restriction concerns entering sleep again. No claim of
verified cold-start or panel electrical timing follows from this source check.

See the [orientation source checks and physical matrix](../hardware/display-orientation.md)
for all four MADCTL values, native windows, payload ownership and scanout limits.
Current orientation tests add both strides, forced full rotations, identical-black
frames, delayed completions and command/payload fault boundaries. The measurements
and test counts below remain the historical partial-update stage evidence.

## Historical host results and remaining work

Eight damage/generation tests cover corners, malformed/overflowing rectangles,
RGB565 bytes, stationary scenes, 250 randomized updates, dense/budget fallback,
stale completion and discard rules. Two tests execute the shared transaction
path, reconstruct panel pixels and inject failure at every command and row.

The initial Criterion fixture is retained in `host/baselines/stage05_initial`
with 30 samples, source/fixture hashes and machine/toolchain metadata:

| Fixture | Host median | RGB565 payload |
| --- | ---: | ---: |
| Stationary | 3.063 us | 0 B |
| One 80 by 96 cell | 2.891 us | 15360 B |
| Entire image changed | 0.283 us | 153600 B |

Dense comparison stops at the first changed bytes of each tile, which explains
its lower planner cost. This is a new CPU-work baseline, not a device speedup.
After the DMA-start guard, the same fixtures measured 2.906, 2.792 and 0.281 us
respectively; raw records are in `stage05_bounded`. The small differences fall
within the variation observed across host runs, so no speedup is attributed
to the guard. The initial source is preserved in commit `01b49e0`.
The shared `DashboardScene` now drives both the device and host reference
renderer, consolidating five temperature-cell call paths. Its full redraw
baseline is 83.101 us for the fixed eight-cell scene in
`host/baselines/stage05_scene_full`, preserved before retained-cell work.
Each framebuffer now retains exact formatted text, colors, time-expiring peak
and trend indicators, font/critical flags, canonical graph values and overlays.
The cache uses the same text formatters as the widgets. Removing or replacing
an opaque popup restores the full underlay; another page invalidates only the
buffer it overwrites. Equal images at the same orientation still emit no SPI commands.

Four retained-scene tests exercise 240 scripted scenes, alternating and repeated
buffer selection, unit/FPS changes, every popup, history/reset, page overwrites,
peak/trend expiry without a new snapshot and long-to-short signed values. Every
retained frame and its damage-applied panel equals a fresh full redraw. Cell
interiors are clipped and cleared: the regression test first demonstrated that
a long negative boost string could overwrite the next cell.

The same fixture after cell clipping and shared formatting has a full redraw
median of 108.802 us, about 31% above the earlier 83.101 us. This correctness
cost is retained explicitly. The current paired fixtures measure:

| Current fixture | Full redraw | Retained redraw | Reduction in host CPU time |
| --- | ---: | ---: | ---: |
| Unchanged eight-cell scene | 108.802 us | 1.864 us | 98.3% |
| Only boost changes each iteration | 124.452 us | 11.404 us | 90.8% |

Raw 30-sample measurements, exact command and source hashes are preserved in
`host/baselines/stage05_retained`. Acquisition, damage planning and transfer are
outside these render timings. The paired paths use identical scene fixtures;
only one buffer is used to isolate rendering cost. These are local host results
with observed run-to-run variation, not throughput or FPS on RP2350.

102 runtime tests and two compile-fail checks pass in both outline modes.
Strict host/firmware Clippy passes. The stock release ELF reports 373440 bytes
static RAM and 150848 bytes core-0 headroom, an increase of 5608 static bytes
from the partial-only checkpoint. Headroom is linker capacity, not measured
worst-case stack use. Physical acceptance still requires CS/SPI/TE traces, orientation and
stride verification on the actual panel, p95/p99 update intervals, input
latency, tearing observations and stock/overclock stability. No physical FPS
or 60 Hz target achievement is claimed.
