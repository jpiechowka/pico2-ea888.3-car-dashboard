# Diagnostic lifetime telemetry

The single diagnostic owner keeps separate counters for capability pages
00/20/40 and supported sensor queries 05/0F/42/5C. Counters survive transport
epochs. They distinguish admission, completed request UART transmission,
decoded positive reply, terminal negative reply, timeout, malformed reply,
abandonment and transport fault affecting an active request. The aggregate
transport-fault count also includes faults while no request is active.

Reply timing starts when the original request's complete UART command drains
and BUSY clears. It ends on an accepted, decoded positive or terminal negative
reply. This includes bridge and host scheduling latency; it does not measure
ECU execution time alone or prove a CAN acknowledgement. NRC78 and flow-control
completions preserve the original timestamp. Stale tokens, foreign replies,
timeouts, malformed payloads and abandonment do not create reply-time samples.
An identical late wire reply remains a protocol ambiguity described in the
[transport contract](../vehicle/diagnostic-transport.md).

Each PID has a fixed 160-bin millisecond histogram: exact integer bins through
31, then sixteen bins per power of two through 8191 ms, plus an explicit
overflow bucket. It exports p50/p95/p99 inclusive bounds, actual minimum and
maximum, accepted count, overflow and rejected count. Counter exhaustion
freezes a distribution and counts rejections; it never silently wraps. Empty
distributions have no quantiles. Host tests compare all percentile bounds
against sorted data including zero, integer power boundaries and `u64::MAX`.

With both `longan-rx12` and `frame-trace`, core1 computes one coherent snapshot
per second, outside receive parsing, and publishes it through a bounded Watch.
Core0 exports `DIAG1`, seven `PID1`/`METRIC1` pairs and `END1,1` through the
reviewed nonblocking RTT backend. Every row includes its snapshot sequence.
Collectors must require all rows and an end marker, reject mixed/duplicate
groups, and report sequence gaps. Watch coalescing and RTT loss are possible;
these records are optional instrumentation, with no backpressure on acquisition.
The sequence is process-local, so record reset boundaries in captures.
The header distinguishes Session's last processed time (which can stop in
recovery) from publication time. Sequence gaps represent skipped snapshots,
including the deliberate 1 Hz producer / 0.5 Hz consumer coalescing, not lost
diagnostic replies. Lifetime distributions include every accepted reply.
Core1 cycle accounting includes histogram export work.

The deployment profile remains Unqualified. It emits no diagnostic requests,
so zero request/reply counts are expected until external adapter/vehicle
qualification is explicitly completed. No target timing has been measured.

## Host checkpoint

`stage08_diagnostics_fixed` archives 30 samples per case, all 167 source files
and policy 2 compiler/configuration evidence. Raw medians: coolant transaction
and snapshot 441.953 ns; fixed 56-transaction, seven-PID distribution export
265.019 ns. The export fixture is independent of preceding benchmark duration
or filter. `stage08_diagnostics` retains the initial exploratory export workload,
which depended on previous benchmark state and is excluded from comparisons.
The historical stage06 session median was 419.54 ns; changed fixtures/toolchain
and capture policy prevent treating the difference as an isolated regression.
