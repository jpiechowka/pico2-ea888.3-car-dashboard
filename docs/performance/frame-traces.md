# Coherent software frame traces

Every successful display completion returns its exact `FrameToken` and timing
record with the buffer lease. The profiling page's render, flush and frame
intervals now come from that one completed record. Independent atomic timing
fields and the inverse-loop-time "Max FPS" estimate were removed.

| Field | Meaning |
| --- | --- |
| Render start/end | Software wall-clock interval for raster work |
| Active cycles | Sum of DWT deltas around raster segments, including interrupts and excluding cooperative suspension; not calibrated target cycles yet |
| Blocked time | Time acquiring the next buffer after rendering, including call overhead |
| Queued time | Timestamp immediately before submitting the buffer lease |
| Started time | Timestamp after the first poll of the payload DMA future |
| Completed time | Timestamp after the HAL write returns and CS is raised |
| Payload bytes | Sum of RGB565 payloads across all windows; 153600 for a full frame |
| Sample time/age | Oldest accepted timestamp among snapshot channels, to HAL return; absent before samples exist |
| Input batch | Unique applied batch IDs, earliest software observation, latest debounce acceptance/consumption and event count |

Render, queue, start and completion ordering is checked. Invalid timestamps,
invalid byte counts, and zero frame sequences cannot create a `FrameTrace`.
Errors keep the buffer in the failure/quarantine path. The optional `frame-trace`
feature retains 64 records in a bounded ring, tracks overwritten records, and
rejects stale/duplicate sequences. This ring is intended for later probe
inspection. A raw memory read concurrent with writes is not a coherent export;
physical capture tooling and target measurements are still pending. Separate
[30-second summaries and validated RTT export](telemetry-windows.md) provide
coherent, bounded long-duration software statistics.

## Input correlation

Button observation begins at the last stable candidate edge seen by the polling
code, with millisecond origin precision. Encoder motion begins at successful
I2C delta-read completion; encoder button origin is the debounce candidate.
These do not include unobserved physical edge or polling delay before detection.
Motion and press values and their timestamps are consumed under one short
critical section. No lock spans I2C, drawing, logging or an await.

The render owner assigns monotonic batch IDs when applying UI changes. A trace
retains the oldest observation and latest acceptance/consumption in a coalesced
batch, and attaches it once to the submitted frame token. Old A/B completions
cannot acquire newer input metadata. A failed transfer contributes no latency
sample. Ignored controls, saturated/rejected encoder polls, reconnect discards
and inputs producing no framebuffer damage have separate counters. A resolved
visible Dashboard warning conservatively excludes the pending batch and increments
`obscured_events`, including when unrelated pixels change. User feedback takes
priority, so visible feedback and diagnostic page changes remain eligible even
while an alarm is active.
Batch observation latency uses the oldest observation, while consume latency
uses the latest consumption; neither is a per-detent distribution.
Brightness changes may also affect PWM before any framebuffer transfer.

A requested statistics reset retains its original observation/consumption and
enters a frame only when the matching acknowledgement is applied on Dashboard.
If the page changed, it is counted as ignored for presentation timing. Settings
maintenance retains pending input metadata across the discarded render, keeps
wall-clock age, and records a gap. HAL completion remains different from actual
visible response; simultaneous controls may replace an intermediate state.

`stage08_input` retains policy 2 source archives and 30 samples per paired host
case. The exact old mailbox source from `56af170` is a benchmark-only fixture.
For one motion, press and consume roundtrip, its median is 15.437 ns; the new
coherent mailbox is 34.803 ns. This is an explicit CPU cost for coherent timing
and guarded counters, using the host critical-section backend. It is not an
RP2350 interrupt-duration measurement or an input-latency guarantee.

## Completion rate

`CompletionRate` samples the actual completed-write counter at monotonic times.
It reports at intervals of at least one second, uses elapsed time for both
current and cumulative average rates, handles u32 counter wrap, and rejects
backwards clocks or implausible backwards counter resets. No completions means
zero current FPS, regardless of render-loop activity. Changing pages restarts
the averaging window. A counter change of over half the u32 range between
samples is rejected; the intended sample interval is much shorter than that.

## Physical limits and upstream review

The RP HAL source was reread at the locked `embassy-rp` 0.10.0 `spi.rs:435-456`:
DMA is configured before the first await, then SPI BSY and RX state are drained
before return. See [published SPI source](https://docs.rs/embassy-rp/0.10.0/src/embassy_rp/spi.rs.html).
The callback occurs after the first poll, so its timestamp is an observation
after arming, not a captured first-wire-edge timestamp. HAL return can be delayed
by executor scheduling after the wire transfer finishes. Consequently these
intervals cannot prove physical SPI duration, tearing, panel scanout, physical
FPS or sample-to-visible latency. Those require SPI/TE/CS scope traces and a
trusted acquisition reference on hardware.

Five host fixtures verify interval arithmetic, malformed and extreme clocks,
bounded ring overwrite/replay, weighted rate windows, stalls and count wrap.
At the initial frame-trace checkpoint, all 77 host runtime tests and two compile-fail tests passed in both outline modes.
Strict Clippy and stock plus `frame-trace,serial-display,cpu300-spi75-1v30`
release builds pass. This is compilation and host verification only.
