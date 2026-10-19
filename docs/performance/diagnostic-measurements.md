# Diagnostic measurements

The 2026-10-19 audit traced dashboard Debug and standalone BLE probe values from
their producers through the shared renderer. These are software observations;
none establishes scanout, electrical margins or vehicle health.

## Rates, work and memory

| Measurement | Dashboard Debug | BLE probe |
| --- | --- | --- |
| Current FPS | Successful HAL completions / actual elapsed window, at least 1 s | Same shared `CompletionRate` |
| Average FPS | Total completions / elapsed time since page change | Total completions / elapsed time since probe UI startup |
| Core 0 | Input and raster DWT cycles / loop wall time, excluding cooperative yields | All top-level task-poll elapsed time / wall window |
| Core 1 | Synchronous acquisition/model/publication DWT cycles / wall window | UI task-poll elapsed time / wall window |
| Static | Linked allocations within 512 KiB arena | Same linker-derived boundary |
| Core 0 stack | Current SP depth | Maximum observed at poll boundaries |
| Core 1 stack | Maximum of periodic acquisition samples | Maximum observed at poll boundaries |

Average FPS is elapsed-time weighted, never the arithmetic mean of window rates.
An already queued frame may complete in a newly selected dashboard page's first
window; this is completion-time attribution. Dashboard frame count remains a
lifetime counter. Probe navigation and peer recovery leave its meter running.
SPI HAL return follows DMA completion, BSY clearing and CS high; scheduling delay
can extend the observed time. It does not measure LCD scanout or tearing.

The work fields cover different scopes, so their percentages are not directly
comparable. IRQ time inside measured segments is included; other work outside
them is excluded. Probe work additionally excludes instrumentation after each
poll. Probe windows expire after two seconds without a fresh observation.
Neither sparse SP sampling scheme measures deepest unobserved call depth.
Both framebuffers and the allocated core-1 stack are already in `Static`.

## Corrections

- Count `Waits` only when a completion receive is not immediately ready. A result
  arriving immediately afterward can avoid suspension; this counts observed
  backpressure, not measured stall duration. Nonzero waits are normal.
- Initialize `Flush` as unavailable and clear it after successful HAL completion.
  A failed transfer retains its ownership indication and buffer quarantine.
- Segment input cycles around cooperative startup validation, as already done
  for raster work. Other tasks during the yield no longer inflate UI work.
- Check each core's DWT enable/implementation/trace bits and progress over 64 NOPs
  at initialization. Failure displays unavailable work/cycles, not false zero.
  This neither resets CYCCNT nor detects later external debugger changes.
- Reject out-of-range core-0 SP observations; display `--` instead of zero.
  Round observed stack depths upward consistently and label linked RAM `Static`.

## Hardware/source cross-check

- The [Pico 2 W specification](https://datasheets.raspberrypi.com/picow/pico-2-w-datasheet.pdf)
  and [RP2350 datasheet](https://datasheets.raspberrypi.com/rp2350/rp2350-datasheet.pdf)
  distinguish 520 KiB physical SRAM from our 512 KiB linker arena. The two scratch
  banks remain unused by this layout. Core-1 bounds come from the actual HAL
  `Stack.mem` allocation; core 0 uses linked static-end/stack-top symbols.
- Locked `embassy-rp` 0.10.0 `time_driver.rs` reads TIMER0 high/low/high coherently
  at 1 MHz. Probe work therefore needs no CPU-clock conversion or new IRQ.
- Dashboard DWT accesses are core-local PPB registers: DEMCR `0xE000EDFC`,
  CTRL `0xE0001000`, CYCCNT `0xE0001004`. Availability checks retain the existing
  CoreSight unlock sequence without resetting the shared counter.
- CPU frequency is the HAL's configured/cached clock, not a frequency-counter
  measurement. SPI rate is register-derived from SPI0 CR0/CPSR and configured
  peripheral clock: `clk_peri / (CPSDVSR × (SCR + 1))`.
- `Vset` is the accepted regulator setting, not measured voltage. The locked HAL
  accepts VSEL codes 0–15, mapped to `550 + 50 × code` mV; it rejects the higher,
  nonlinear encodings. No ADC or wireless shared pin is touched by these metrics.

See the actual [HAL source](../../firmware/vendor/embassy-rp/src/),
[display ownership](display-pipeline.md) and [memory admission](memory-layout.md).

## Verification boundaries

Regression tests cover unequal/idle FPS windows, wrapping counters, rejected
clock/count regressions, work suspension/cancellation/error propagation,
availability bits, stack bounds, independent Debug bitmaps and probe telemetry
clipping on list/details/state screens. The gallery uses the real shared RGB565
renderer with explicitly synthetic values. Native/Wasm/ARM checks and linked
image admission remain separate from hardware measurements.

The final stock and 280/70 probe images use 373160 static bytes (+440 against the
preceding utility), retaining 151128 bytes for the main stack and headroom. The
280/70 demo dashboard uses 441384 static bytes with 82904 bytes remaining. All
three passed the existing practical image limits with Cortex-M33 code generation.
A storage/pinning regression guards the single-copy task wrapper; bounded FPS
formatting avoids general floating-point formatting on the utility's stack.

Remaining device checks: confirm both counters progress, compare expected timer
windows, inspect stack samples under heavy radio/UI load, check scan/detail/input
recovery, and observe display completion on the physical board. Timer accuracy,
whole-core occupancy and stack high-water are not established by host tests.
No automatic flash follows this audit.
