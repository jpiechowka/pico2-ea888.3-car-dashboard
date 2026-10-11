# Coherent software frame traces

Every successful display completion returns its exact `FrameToken` and timing
record with the buffer lease. The profiling page's render, flush and frame
intervals now come from that one completed record. Independent atomic timing
fields and the inverse-loop-time "Max FPS" estimate were removed.

| Field | Meaning |
| --- | --- |
| Render start/end | Software wall-clock interval for raster work |
| Active cycles | DWT delta around raster work, including interrupts; not calibrated target cycles yet |
| Blocked time | Time acquiring the next buffer after rendering, including call overhead |
| Queued time | Timestamp immediately before submitting the buffer lease |
| Started time | Timestamp after the first poll of the payload DMA future |
| Completed time | Timestamp after the HAL write returns and CS is raised |
| Payload bytes | RGB565 bytes written, currently 153600 for a full frame |
| Sample time/age | Oldest accepted timestamp among snapshot channels, to HAL return; absent before samples exist |

Render, queue, start and completion ordering is checked. Invalid timestamps,
invalid byte counts, and zero frame sequences cannot create a `FrameTrace`.
Errors keep the buffer in the failure/quarantine path. The optional `frame-trace`
feature retains 64 records in a bounded ring, tracks overwritten records, and
rejects stale/duplicate sequences. This ring is intended for later probe
inspection. A raw memory read concurrent with writes is not a coherent export;
physical capture tooling and target measurements are still pending.

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
All 77 host runtime tests and two compile-fail tests pass in both outline modes.
Strict Clippy and stock plus `frame-trace,serial-display,cpu300-spi75-1v30`
release builds pass. This is compilation and host verification only.
