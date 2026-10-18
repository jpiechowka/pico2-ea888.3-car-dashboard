# Bounded logs and RTT policy

The production `logging` module is shared with the host crate. Accepted history
records receive checked, increasing u64 sequences. A scroll anchor identifies an
entry, so new records do not move a historical window until its entries are
evicted. The history holds 128 records; snapshots copy at most 12 visible records
by index and release their guard before rendering or yielding. Sequence exhaustion
rejects new history records without aliasing an old anchor.

Messages hold 40 UTF-8 bytes, preserve a prefix at truncation, and stop formatting
at capacity. Formatting occurs before locks. Arbitrary custom formatters can still
execute arbitrary code; the buffer limit is not a time bound on such code. Rendering
normalizes control characters to `?` so a message cannot draw into another row.
Timestamps are stored in u64 milliseconds; the five displayed digits wrap at
100000 ms and are not the record identity.

History insertion uses `try_lock`. Independent saturating counters track rejected
insertions from contention or sequence exhaustion (`lost`), formatting truncation
(`cut`), and accepted-history evictions (`old`). Counter snapshots are independent,
not transactional with history. The footer describes application logs only.

A separate protected slot keeps the latest explicit fault through informational
eviction and history contention. Replacements have a separate counter. Copies under
the Embassy raw critical section are bounded; that global multicore lock can still
wait for its owner. The slot is RAM and does not survive reset. Encoder, display,
supervisor and new transport faults enter it before optional output or terminal
recovery. Transport fault observations are coalesced at the 10 ms publication limit;
the latest fault and cumulative counter remain available in `LinkSnapshot`.

## BLE transition records

The radio owner logs only changes in link state or fault, after releasing its
status lock. `BLE <epoch> <state> <reason>` retains the full reconnect epoch and
a distinct state/reason in at most 39 ASCII bytes. Any explicit fault, or the
terminal Fault state, uses Fault severity and the protected latest-fault slot.
Normal connect, ready and maintenance transitions use Info and do not replace it.
No peer address, passkey or adapter payload enters these records.

State codes are `OFF`, `START`, `CONN`, `SEC`, `DISC`, `IDENT`, `READY`, `SYNTH`,
`BACK`, `PARK` and `FAULT`, respectively: disabled, starting, connecting, securing,
discovering, identifying, adapter ready, synthetic ready, backoff, maintenance and
terminal fault. `READY` does not imply qualified vehicle data.

Reasons `CONFIG`, `SECURITY`, `DISCOVER`, `LINK`, `RX-LOSS`, `TIMEOUT`, `CLOCK` and
`EXHAUST` identify the corresponding link fault; `OK` means no fault detail.
Protocol reasons retain a `P-` prefix: `DISCON`, `EPOCH`, `BUSY`, `UNQUAL`, `INIT`,
`TOKEN`, `LOSS`, `LENGTH`, `OVER`, `CLOCK`, `TIME`, `EXHAUS`, `UNEXP` and `REPLY`.
Parser reasons retain `E-`: `OVER`, `ASCII`, `FRAME`, `SEQ`, `OTHER` (foreign frame),
and `SHORT` (incomplete response). These codes preserve nested failure distinctions
without relying on truncatable Rust Debug output.

Host regressions exercise all 11 states and all 29 optional reasons at epoch zero
and u64 maximum through the real bounded log store, plus retained faults through
normal reconnect and history eviction. They do not validate radio hardware timing.

## Locked dependency review

For the current defmt-rtt 1.3.1, ordinary writes ignore host-selected blocking flags with
`disable-blocking-mode`. Its [exact channel implementation](https://github.com/knurling-rs/defmt/blob/719491ca3529abd07205165b9b10e4d988552bc2/firmware/defmt-rtt/src/channel.rs)
still permits `flush()` to wait indefinitely. No application path may call it.
The [encoder implementation](https://github.com/knurling-rs/defmt/blob/719491ca3529abd07205165b9b10e4d988552bc2/firmware/defmt-rtt/src/lib.rs)
uses IPSR alone with `drop-on-contention`: both cores in thread mode share IPSR0,
which can alias ownership after a failed CAS. Keep that optional feature off.
Default encoding uses Embassy RP's core-aware global critical section.

The all-feature Cargo graph and conservative source guard in
`firmware/scripts/check_logging_policy.py` enforce the selected policy. The guard
rejects aliases/imports as well as direct flush calls; it is not a complete Rust
parser or a proof about future dependencies. Legal grouped and root-qualified
import bypasses have explicit regression fixtures.

The original audit checked published archives and all 199 original files of defmt-rtt 1.3.0, defmt and
macros 1.1.1, panic-probe 1.0.0, critical-section 1.2.0 and cortex-m 0.7.9 were checked
against lock checksums. Release/migration evidence and primary discussions were
checked alongside source: [RTT PR915](https://github.com/knurling-rs/defmt/pull/915),
[1051](https://github.com/knurling-rs/defmt/issues/1051#issuecomment-4313075319),
[PR1053](https://github.com/knurling-rs/defmt/pull/1053), and
[panic source](https://github.com/knurling-rs/defmt/blob/f6decae6e09d1216aff052aa0733a6b5e632a100/firmware/panic-probe/src/lib.rs).
The 18 October maintenance pass re-read the locked 1.3.1 channel/encoder and
confirmed this policy with defmt 1.2.0. The versions and file count above describe
the historical audit, not the current dependency graph.
Panic-probe print-defmt does not flush, reset, or populate the application slot.
Reporting is best effort; watchdog recovery applies after watchdog initialization.

Nonblocking RTT can overwrite unread bytes and partial encoded frames. It exposes
no loss counter; application counters cannot measure this. No universal wait-free
or guaranteed panic-print claim follows from this configuration.

## Historical verification and cost

These measurements describe the original logging stage. Current image limits and
BLE resource evidence are in [memory layout](memory-layout.md); rerun comparisons
on matched toolchains before attributing a change to logging.

Eight new shared logging tests cover capacity/UTF-8, sequence exhaustion, saturated
counters, busy history, protected faults, scroll eviction, simultaneous producers,
control characters, and snapshot pixel stability while production continues. Both
outline modes pass 204 runtime tests and two compile-fail checks. Strict host,
Demo and Longan Clippy pass (existing dependency deprecation/future-compatibility
warnings remain). The logging policy guard passes.

Optimized ELF checks: Longan static RAM 380864 B / core0 headroom 143424 B;
Demo300 with both traces 384944 B / 139344 B. The increased log record identity,
fault slot and page snapshot cost 2240 static bytes versus the preceding stage.

Source-frozen Criterion run `stage07_logging_fixed`, 30 samples on the same Windows host:

| Fixture | Host median |
|---|---:|
| Insert formatted record and capture visible snapshot | 163.292 ns |
| Full log page | 54.472 us |
| Retained stationary dashboard | 1.903 us |

The two log fixtures are new baselines. Stationary dashboard was 1.909 us in the
previous run; this small difference is recorded without attributing it to a code
change or claiming a speedup. Raw samples, compiler, command and source archive are
under `host/baselines/stage07_logging_fixed`. No device FPS or multicore timing claim.

Physical gates remain: disconnected debugger, disconnect after mode2 selection,
both-core bursts, fault and panic during contention, and observed watchdog resets.

The earlier `stage07_logging` capture is exploratory: its full-page fixture reused
the insertion benchmark store, so calibration and filtering changed the messages.
It is retained for provenance and is excluded from regression comparisons. The
fixed fixtures have constant insertion text and a separately seeded render store.
