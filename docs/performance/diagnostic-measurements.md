# Diagnostic measurements

The [2026-10-20 re-audit](../validation/diagnostics-reaudit-2026-10-20.md)
traced dashboard Debug and standalone BLE probe values from their producers
through the shared renderer, exports and Studio preview. These are software
observations; none establishes scanout, electrical margins or vehicle health.

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
Successful display completion follows DMA completion, BSY clearing and CS high;
scheduling delay can extend the observed time. It does not measure LCD scanout or
tearing.

The work fields cover different scopes, so their percentages are not directly
comparable. IRQ time inside measured segments is included; other work outside
them is excluded. Probe work additionally excludes instrumentation after each
poll. Probe windows expire after two seconds without a fresh observation.
Neither sparse SP sampling scheme measures deepest unobserved call depth.
Both framebuffers and the allocated core-1 stack are already in `Static`.

Dashboard FPS stays unavailable until its first complete window after boot or a
page change. A qualified idle window is a measured zero. Both routes expire rate
and work observations after two seconds; a future timestamp is unavailable.
Dashboard core1 publishes work, sampled stack depth and observation time in one
bounded cross-core copy. Its stack maximum remains a historical observation,
while its work percentage expires. The first stack sample is unavailable until
the acquisition owner actually observes SP. Transfer trace timings and sample
age refer to the last completed frame, not the current age of vehicle data.

Both screens round memory numerator and capacity upward to KiB (`K` in compact
labels), after validating exact byte bounds. These rounded values must not be
added together to infer exact free memory. Invalid rates, work percentages,
memory pairs and buffer indices display unavailable.
Debug's Cycles label instead uses thousands of cycles, rounded downward; a valid
count below 1000 is `0K`. Its cycles, work percentage and observation time are
published together after maintenance/discard decisions. Invalid, future or stale
work also hides its cycles. An otherwise valid total wider than the cycle field
can leave work available while the cycle display remains unknown.

Studio's running synthetic preview has no hardware transfer observation and
shows unavailable FPS. Explicitly labelled renderer fixtures may supply synthetic
rates for layout tests; real firmware retains measured idle zero.

The probe's compact `C0`/`C1` values are task-poll work percentages. `SP0`/`SP1`
are sampled stack-depth maxima against their allocated capacities. They are not
whole-core utilization or a physical stack watermark. A displayed `1K` depth
means a positive observation up to 1024 bytes after rounding, not exactly 1 KiB.
`Static` is an allocation boundary, not memory that became live during scanning.

On slot, confirmation and saved-evidence pages the header still describes the
**currently running Pico**. Capture timestamps and link parameters below it are
historical observations from the saved attempt. Loading an old capture does not
restore its old FPS/work readings or reset the running meters. Flash maintenance
and USB export are included where their task polls intersect the measured scope;
waiting for another owner or USB does not count as active poll work.

## Corrections

Boot-reset evidence is separate from CPU/RAM/stack measurements and from `HCIbad`.
The shared watchdog boundary retains a checked previous stage and last-observed
boot-relative time, saturated at `u32::MAX` milliseconds. That is neither the
reset instant nor a stack watermark. Power removal can erase it; a saved v4
capture freezes what was available on that boot. Y recovery clears the live
fault acknowledgment without changing boot evidence. See the
[2026-10-20 reset/name follow-up](../validation/probe-slot3-discovery-2026-10-20.md).
A controlled RP2350 NORMAL ROM reboot can use the watchdog timer and overwrite
the retained stage/checksum. `WD TIMEOUT` therefore identifies the hardware
indication without proving an application stall. The conservative classifier
is shared by both images; [exact source evidence](../validation/probe-slot4-followup-2026-10-21.md#post-flash-watchdog-evidence)
keeps this distinct from unresolved cold-start reports.

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
- Reject invalid/overflowing cycle segments and zero-denominator windows rather
  than converting them to idle work. A two-second wall-time guard also rejects
  intervals long enough to hide an entire CYCCNT revolution. Measurement failure
  does not stop rendering. Logs use the same optional work formatter as Debug.
- [FRAME2 exports](telemetry-windows.md#versioned-export-and-collector) retain
  valid transfer counts/timing when cycle observations are unavailable. Their
  cycle sum is explicitly `na`, not zero or a partial sum.
- Probe standard-read duration is frozen before reliable event publication, so
  queue backpressure cannot inflate only its failure record. Global scan-error
  age is unavailable when its observation is newer than the UI snapshot. Shared
  capture admission rejects a global scan timestamp after the capture time,
  while retaining global errors that predate the selected peer.

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
  peripheral clock: `clk_peri / (CPSDVSR × (SCR + 1))`. Checked decoding requires
  an even prescaler from 2 through 254 and a nonzero result; unavailable metadata
  stays unavailable through Debug and rejects a target benchmark.
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

The same unavailable-value, formatting and exact-byte memory checks serve both
firmware routes. Regressions distinguish an unobserved first window from a valid
idle zero, a future/stale observation from current data, and invalid render cycles
from otherwise valid completed-frame timing. A sampled stack maximum can remain
available after its work window expires; it remains historical evidence.

The preceding diagnostic delivery's stock and 280/70 probe images used 373160
static bytes (+440 against the preceding utility), retaining 151128 bytes for
the main stack and headroom. The 280/70 demo dashboard used 441384 static bytes
with 82904 bytes remaining. All
three passed the existing practical image limits with Cortex-M33 code generation
at that delivery. These are historical image values; current changes require
fresh linked-image admission and must not reuse those byte counts as current.
A storage/pinning regression guards the single-copy task wrapper; bounded FPS
formatting avoids general floating-point formatting on the utility's stack.

Remaining device checks: confirm both counters progress, compare expected timer
windows, inspect stack samples under heavy radio/UI load, check scan/detail/input
recovery, and observe display completion on the physical board. Timer accuracy,
whole-core occupancy and stack high-water are not established by host tests.
No automatic flash follows this audit.

Record physical acceptance against the exact image using the
[capture and diagnostic bench checklist](../validation/pico2-dnuc-runbook.md#standalone-probe-captures-and-diagnostics).

Current software checks and exact linked images are recorded in the
[whole-code audit](../validation/full-code-audit-2026-10-20.md#verification-checkpoint).
The [2026-10-19 record](../validation/probe-offline-audit-2026-10-19.md)
retains the historical image values for that delivery.
