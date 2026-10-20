# Dashboard and BLE probe diagnostics re-audit

20 October 2026. This is a fresh diagnostics review within the
[whole-code audit](full-code-audit-2026-10-20.md), including the actual firmware
producers, shared arithmetic, cross-core publication, rendering, saved captures,
host exports and Studio preview. It supplements the earlier measurement review.
No device, flash, slot write or vehicle operation was performed.

## Findings and corrections

| Finding | Observable consequence | Correction and regression evidence |
| --- | --- | --- |
| Dashboard published a new cycle count before deciding whether flash maintenance would discard that render | Debug could combine the discarded render's cycles with the preceding iteration's still-fresh work percentage | `FrameWorkObservation` uses private fields to bind cycles, work and timestamp. Publication occurs after the maintenance/discard branches. The original assignment sequence reproduced the mismatch; shared tests cover coherent replacement, unknown, idle zero, future/stale boundaries and a cycle count too wide for the display. The real executor branch was traced in source, not executed on a Pico. |
| SPI divider metadata treated prescaler zero as a valid zero-Hz value | Debug could display a fabricated configured rate | Shared checked decoding accepts even CPSDVSR 2–254 and SCR+1 1–256. Invalid/zero rate remains `None` through Debug. Tests classify all 256 prescaler encodings and exercise fractional profiles and boundary divisors. |
| Standard-read failure duration was sampled again after awaiting event publication | A single read had different durations in its retained value and first-failure record | Freeze elapsed time before publication and reuse it. A real suspended host publication reproduced 40 ms versus 0 ms; success, protected ATT response, hardware failure and timeout-result cases now agree. |
| Saved capture validation omitted the global scan-failure timestamp | CRC-valid data could claim an observation after the capture itself | Shared encode/decode admission rejects `last.at_ms > captured_at_ms`. Earlier global errors remain valid even when they predate the selected peer. Tests cover zero, earlier, equal, future and independently modified valid-CRC input. |
| Live scan age used saturating subtraction | A report newer than the UI frame snapshot appeared as `0s ago` | Checked subtraction displays `age unavailable`; equal-time zero and ordinary positive ages remain valid. Saved pages already display the absolute observation time and did not have this age-calculation defect. |
| Studio supplied synthetic FPS as `0.0` without a device completion observation | FPS controls showed a measured-looking zero | All synthetic scene constructors use the established unavailable representation. Independent literal-glyph pixel tests cover Instant, Average, Combined and Off in both orientations. Hardware firmware still reports real idle zero. |

Benchmark clock records now require available SPI and regulator metadata before
the experiment proceeds; incomplete clock records are omitted and the run fails.
Legacy records documented zero as unknown VSEL and must still be interpreted
under that historical contract. Dashboard log labels use `UI` and `Acq` to state
their measured scope. The TE observer comment now matches the reviewed HAL's
interrupt cleanup on cancellation.

## End-to-end measurement paths

### Dashboard

1. HAL initialization establishes the configured clocks. Each core initializes
   its own DWT availability observation without resetting CYCCNT.
2. Core 0 measures input and raster segments around cooperative yields. The
   iteration's wall interval includes its waits; its cycle numerator excludes
   code outside those instrumented segments.
3. Core 1 measures synchronous acquisition/model/publication work, accumulates
   it over a window, and samples its own SP. Work, stack observation and time
   cross cores in one bounded locked copy.
4. The display owner alone records successful HAL transfer completion. Frame
   identity, timestamps, payload and optional active cycles enter the bounded
   trace/window pipeline after validation.
5. Debug copies a `ProfilingData` snapshot into shared `DebugData`. Cooperative
   row rendering uses that immutable copy, so yielding between rows does not
   mix different diagnostic snapshots.

### BLE probe

1. Boot samples the linked memory boundaries and actual core-1 stack allocation.
2. Each top-level core-0 task and the sole core-1 UI task is wrapped once.
   The wrapper measures the synchronous `Future::poll` span and then samples SP.
   It does not hold the metrics lock while polling the task.
3. The display owner increments completion count only after successful transfer
   and CS release. The metrics snapshot copies the counter, observation time,
   work and stack values under one cross-core critical section.
4. UI drains bounded event queues and copies scan/fault state separately.
   Those independent observations need not have exactly the frame's earlier
   timestamp. Future-relative diagnostic ages therefore remain unavailable.
5. Every probe page uses the current metrics header. Saving freezes admitted
   peer/inspection evidence with a fresh capture timestamp; FPS/work/SP readings
   are not serialized into the capture body.

Relevant implementation:
[dashboard loop](../../firmware/pico2/src/main.rs),
[dashboard profiling](../../firmware/pico2/src/profiling/),
[acquisition](../../firmware/pico2/src/tasks/acquisition.rs),
[display owner](../../firmware/pico2/src/tasks/flush.rs),
[probe metrics](../../firmware/ble-probe/src/metrics.rs),
[probe UI](../../firmware/ble-probe/src/ui.rs),
[shared diagnostic values](../../crates/dashboard-core/src/profiling/diagnostic_values.rs).

## Field matrix: rates, work, memory and configuration

`K` in compact memory labels means KiB rounded upward, not decimal kilobytes.
All memory relationships are checked in exact bytes before presentation.

| Field | Source, units and interpretation | Unknown, expiry and reset |
| --- | --- | --- |
| Current / Done FPS | Successful software transfer completions divided by the actual elapsed window, at least one second | Unknown before first complete window. A qualified idle window is zero. Future observations or those older than two seconds are unavailable. |
| Average FPS | Total accepted completions divided by total elapsed time; elapsed-time weighted | Dashboard restarts the meter on page change. Probe uses UI-start origin across navigation/recovery. A previously queued frame can complete after a dashboard page change and belongs to the new completion-time window. |
| Dashboard UI work | DWT cycles in input/raster segments divided by configured CPU Hz and iteration wall time; tenths of percent | Missing counter, invalid segment, overflow or invalid denominator gives unknown. Cycles and work expire together after two seconds. Maintenance-discarded work does not partially replace the previous observation. |
| Dashboard acquisition work | Synchronous acquisition/model/publication cycles over the wall window | One coherent core-1 publication. Work expires independently from the historical stack sample. |
| Probe W C0 / C1 | Sum of synchronous top-level task-poll spans over actual wall time; tenths of percent | First window unknown; idle zero valid; overlap, reversal and overflow invalidate the window; later valid windows can recover. Two-second freshness limit. |
| Cycles | Dashboard input+raster numerator from the same observation as UI work; `K` means 1000 cycles, rounded downward | Missing/invalid/stale work suppresses cycles. A valid count below 1000 displays `0K`. A mathematically valid wide total can retain work while the narrower cycle display is unavailable. |
| Static | Linked `__sheap - 0x20000000` against the 512 KiB main arena | Includes both framebuffers, static task storage and reserved core-1 stack. It is allocated space, not changing live usage. Zero capacity/out-of-range geometry is invalid. |
| Dashboard SP now | Current core-0 `top - SP` sample between linked bounds | Out-of-range SP is unknown, not zero. It is one instantaneous sample. |
| Dashboard SP seen | Largest accepted core-1 acquisition sample within its actual stack allocation | Unknown until sampled. Historical maximum does not expire when work becomes stale. |
| Probe SP0 / SP1 | Largest accepted post-poll stack depth on each core | Invalid current sample shows unknown while retaining internal history for a later valid sample. Neither field measures deepest calls between polls. |
| Framebuffers | Two fixed 153600-byte RGB565 payloads | Already included in Static; no third buffer or hidden transpose allocation. |
| CPU configured / requested | Cached HAL clock versus selected build/profile requirement | Configuration metadata, not frequency-counter measurement. Firmware initialization requires a nonzero configured clock. |
| SPI configured / requested | SPI0 CR0/CPSR divider applied to configured `clk_peri`, versus requested rate | Invalid divider or zero result is unknown. Fractional MHz remains visible. No assumed universal CPU:SPI ratio. |
| Vset / requested | Checked HAL regulator selection versus requested millivolts | Unsupported encoding is unknown. This is not an ADC measurement of the core rail. |
| Brightness | Current Full/Dim/Off policy or legacy control scale | Does not measure panel luminance. Boot/recovery and completed-frame shutdown rules remain separate. |
| Settings, reset, source, config fault | Current bounded status, checked reset context, acquisition/source status and optional fault flag | Absence is distinct from valid false/zero. Saved preferences are not reported as persisted before write completion. Synthetic sources remain labelled. |

IRQ time interrupting a measured segment/poll is included; IRQ and scheduler work
outside it is excluded. Probe instrumentation after `poll` is excluded. Dashboard
unmeasured tasks and trailing work are excluded. Thus these percentages are
neither whole-core utilization nor directly comparable between firmware routes.
They must not be used to infer that the remaining percentage is available CPU.

Both static memory and the core-1 capacity include that allocated stack; adding
the displayed fields double-counts it. Rounded values must not reconstruct exact
free memory. RP2350 has 520 KiB physical SRAM, while the admitted main arena is
512 KiB. Renaming the two additional banks to upstream `SRAM8`/`SRAM9` changes
no addresses, memory arithmetic, screen denominator or capture reservation.
See [memory admission](../performance/memory-layout.md).

## Field matrix: completed frames, logs and telemetry

| Field | Meaning and admission | Limit |
| --- | --- | --- |
| Frames | Successful HAL completion counter | Counter wrap is handled by rate arithmetic; this is not an unbounded lifetime integer. |
| RenderTime | Render end minus render start of the last accepted completed trace | Wall duration includes scheduling pauses; it differs from active cycle work. |
| TransferTime | HAL completion minus transfer start | Includes software scheduling delay and is not panel scanout time. |
| FrameTime | Completion minus render start | Describes the last completed trace, not the in-progress Debug frame. |
| Age* | Sample age at that frame's completion | Frozen historical age, not the current age of live vehicle data. Missing sample remains unknown. |
| Swaps / Waits | Buffer swaps and observed completion-receive backpressure | Waits is a count, not stall duration; a result arriving just afterward may avoid suspension. |
| Render / Flush | Checked buffer identity 0 or 1 | Idle Flush is unavailable. Failure retains ownership/quarantine instead of pretending idle or recycling a live buffer. |
| Log lost / cut / old | Saturating loss/truncation/overwritten-history observations | Independent counters are not one atomic snapshot with the history. Saturation cannot represent further increments exactly. |
| Log timestamps and latest fault | Bounded boot-relative entries; separately retained latest fault | Display truncation does not alter stored entry identity. No UTC, complete history or causal-order guarantee. |
| FRAME2 windows | At-least-30-second reports with actual elapsed time, admitted frame/byte totals and nine timing distributions | RTT and bounded queues can lose reports. Missing cycle evidence is `na`; valid transfer timing survives. |
| Quantiles | Nearest-rank inclusive bucket upper bounds; sample count accompanies each distribution | Empty distribution means no observation, not a zero-latency sample. Bucket upper bounds are not exact individual durations. |
| INPUT1 / DIAG1 | Typed input totals and bounded diagnostic observations tied to the reporting window | Host parser rejects malformed rows, inconsistent sequence/framing and invalid scalar values; no hardware timing guarantee follows. |
| Browser Preview Stats | Browser monotonic elapsed time for bounded, identity-checked operations | Browser time is neither DWT cycles nor Pico FPS. Stale, cancelled, nonfinite and reversed observations are rejected. |

The parent review followed the actual transfer owner through `FrameTrace`,
`TraceRing`, `FrameWindow`, Rust row serialization and the full Python collector
and its tests. Sequence, monotonic ordering, even payload bounds, arithmetic
overflow and absent cycle fields remain checked before mutation/publication.
An empty window has real zero totals and no quantiles. The valid zero/unknown
distinction survives JSON export. See
[telemetry contract](../performance/telemetry-windows.md) and
[collector](../../host/scripts/parse_telemetry.py).

## Probe evidence, fault and capture values

| Field group | Producer and interpretation | Failure/identity boundary |
| --- | --- | --- |
| Peer/page/slot counts | Bounded catalogue, admitted inventory vectors, pagination and four slot selections | Counts are actual admitted records, not requested records. Limits/partial results remain visible. |
| Lost / full | Report-channel enqueue loss and catalogue capacity rejection | Saturating counters. Catalogue recovery resets its contents/full count; lifetime dropped-report evidence is separate. |
| HCIbad | Malformed legacy HCI scan batches | Global count, not selected-peer fault count and not all BLE errors. A zero count does not rule out watchdog, storage, USB, encoder or other failures. |
| Scan error context | Revision, entry/count, report bytes, remaining bytes, optional declared length and observation time | Parsing stops at the first bad entry. Raw length is not an accepted payload length. Live future-relative age is unknown; saved time is absolute within its original boot. |
| Peer name/address/type | Observed bounded name with source; exact address/type identity | Cached name never substitutes for raw packet data or crosses address/type. Random subtype is not authentication or permanent identity. |
| Peer age/reports | Last accepted peer report and accepted report count | A just-arrived peer newer than the frame snapshot is treated as age zero for selection. Inspection freshness uses the bounded policy. Packet/global diagnostic ages retain separate checked validity. |
| RSSI / TX power | Signed controller RSSI and advertised TX field | Missing/invalid RSSI remains unknown. These values are not distance measurements. Packet RSSI belongs to that packet; latest peer RSSI can differ. |
| AD bytes/flags/IDs/UUIDs | Bounded legacy packet parser plus retained raw wire hex | Malformed/truncated payloads stay explicit. Raw bytes preserve evidence not expressible by the display font. |
| Service/characteristic/descriptor/standard-read values | Validated GATT inventory, exact wire UUIDs, handle relationships and bounded read results | Query range, last accepted handle and rejected prefix remain separate. Protected or failed reads cannot become successful empty values. |
| Connection elapsed/parameters | Observed successful connection completion and current controller-provided parameters | Connection interval ticks are 1250 us, supervision ticks 10000 us, latency is an event count. Observed parameters are not merely the requested bounds. |
| ATT MTU | Negotiated value after successful client setup | Unavailable before negotiation; saved admission bounds 23–517. It does not identify a proprietary protocol or usable application payload. |
| Read/stage elapsed | Monotonic observed operation duration, saturated to u32 milliseconds | The fixed read record excludes later publication waits. Whole-inspection timeout still bounds the entire future, including its internal queue waits. |
| ATT/HCI/local/disconnect errors | Exact available error byte and separate local/terminal observations | Missing code stays absent. First failure remains first; later cleanup evidence cannot erase it or manufacture clean success. |
| Cleanup / completion / LED latch | State acknowledgments plus atomic live fault bits | Normal reason alone is insufficient; inventory, allowed reads, cleanup and fault history must all qualify. Saved success never creates live green. |
| Watchdog stage/time | Checked retained previous observation plus raw reset and ROM boot context | Last recorded stage/time is not the precise reset instant or proven cause. Power removal can erase it; Y acknowledgment does not rewrite boot evidence. |
| GSPI and build clocks | Configured shared radio divider and saved build/profile clock metadata | Configuration-derived values, not measured wires. Legacy absent fields remain unavailable. |
| Capture revision/time/source identity | Validated frozen boot-relative evidence and build digest | No invented UTC or cross-boot time alignment. Global scan time cannot follow capture time. |
| Slot generation/CRC/content ID | Validated journal generation, payload integrity and host SHA256 | CRC/hash detect or identify bytes; they do not authenticate a device. Save/export does not initiate BLE traffic. |

Metrics, scan diagnostics, drop counters and LED bits have separate bounded
publication mechanisms. The screen is not claimed to be one globally simultaneous
snapshot of all of them. The metrics lock contains bounded timer reads, copying
and arithmetic, never task polling, drawing or awaited peripheral work.

Pause, browsing slots and Y recovery do not restart the probe FPS origin. Saved
headers describe the currently running Pico; saved bodies retain the original
timestamps, errors, identity and connection observations. USB LIST/GET uses
generation-checked byte leases and bounded host reads. Cancellation/session loss
does not authorize a stale response or new BLE work. Legacy captures cannot
retain optional fields from a previously decoded newer capture.

## Display and synthetic-state review

All 32 shared Debug lines were traced, including headings and control hints.
Landscape uses two clipped columns; portrait uses one bounded row per line.
Each cooperative row clears its own area before drawing, preventing old longer
values from surviving a shorter replacement. Tests cover independent expected
pixels, missing/zero/nonfinite values, maximum integers, memory bounds, custom
control hints, portrait layout and full-versus-cooperative redraw.

Very large synthetic values can be clipped to their assigned text lane; clipping
is bounded and does not promise that every possible u64 decimal value plus units
fits. Production frame deadlines bound ordinary timing values. Host fixture
galleries may intentionally inject labelled synthetic metrics. The running
Studio simulator now shows unavailable FPS because it has no hardware transfer
observation; its browser timing profiler remains a separate instrument.

## Source and specification cross-check

- The [RP2350 datasheet](https://datasheets.raspberrypi.com/rp2350/rp2350-datasheet.pdf)
  and [Pico 2 W board specification](https://datasheets.raspberrypi.com/picow/pico-2-w-datasheet.pdf)
  establish the memory and register context. DWT, SPI, TIMER0 and POWMAN fields
  were checked against the actual locked PAC/HAL code.
- The [Arm Cortex-M33 TRM, issue 07](https://documentation-service.arm.com/static/641b2edcde84571cc27f3caa)
  describes DWT register availability and controls. The actual
  [cortex-m 0.7.9 implementation](https://docs.rs/cortex-m/0.7.9/src/cortex_m/peripheral/dwt.rs.html)
  confirms the trace-enable prerequisite, NOCYCCNT bit and software unlock key.
  Firmware tests initial enable/progress without resetting the counter.
- The [official SDK SPI implementation](https://github.com/raspberrypi/pico-sdk/blob/master/src/rp2_common/hardware_spi/spi.c)
  corroborates the peripheral-clock/divider calculation and busy/drain behavior.
  The local Embassy async write and ST7789 CS sequence were read directly.
- Locked Embassy RP 0.10.0 reads TIMER0 high/low/high coherently at 1 MHz;
  Embassy Time 0.5.1 converts that tick count directly to microseconds. Normal
  debug-pause behavior means diagnostic time is not a promise of elapsed
  real-world time across debugger halts. The benchmark separately checks and
  temporarily changes/restores its timer debug-pause controls.
- [Bluetooth HCI](https://www.bluetooth.com/wp-content/uploads/Files/Specification/HTML/Core-60/out/en/host-controller-interface/host-controller-interface-functional-specification.html)
  supplies connection/RSSI units and unavailable encodings; actual bt-hci 0.10.1
  decoding and Trouble parameter publication were traced. These observations
  do not establish adapter or ECU compatibility.
- The [Rust Future contract](https://doc.rust-lang.org/std/future/trait.Future.html)
  explains the synchronous polling boundary. The actual locked critical-section
  implementation masks local interrupts and uses a cross-core spinlock; no
  single-core mutex assumption was used.

## Verification and acceptance limits

The detailed local evidence records regressions before and after the fixes,
including 19 inventory, 9 capture-semantic, 22 shared probe-render, 61 capture/
export/UI/wire, 35 preview and 41 focused shared diagnostic test executions.
Additional portrait, retained-render, logging, progress-health, strict native/
Wasm and ARM checks are recorded in the enclosing audit. These groups overlap;
they are not a unique-test total. Final integrated and exact-image results belong
in the [whole-code verification table](full-code-audit-2026-10-20.md#verification-checkpoint).

Physical timer/clock accuracy, IRQ contribution, debugger interference, deepest
stack usage, radio interoperability, power/reset behavior, display scanout and
tearing remain unqualified. Two-second freshness is evaluated when the software
runs; a stalled UI can leave old LCD pixels visible until recovery/reset.
Source review and host tests cannot cause the physical display to erase itself.
The changes do not establish a cause for the photographed watchdog resets or
the missing TV.

For the next separately authorized device run, retain exact image/source identity
and compare cold boot, long idle, scan, inspection, error/recovery, slot browsing,
save and USB export. Record headers separately from frozen capture bodies.
Observe both core counters, valid-zero versus unknown transitions and stack
samples under load. Compare SPI/timing with independent instrumentation before
calling them measured electrical performance. Use the
[physical validation runbook](pico2-dnuc-runbook.md#standalone-probe-captures-and-diagnostics).
