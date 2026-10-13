# Bounded target benchmark experiment

`target-bench` is default off. It runs once at boot on core0 after core1 starts,
before core0 spawns its display, encoder, TE and supervisor tasks. It borrows
the render owner's existing SRAM framebuffer. It never transmits these pixels,
feeds the watchdog, modifies the acquisition model or counts a completed frame.
No target run has been performed; all device results remain pending.

## Workload and validity

Four fixed kernels each execute eight times: full RGB565 clear, clipped fill,
graph plus boost widget, and 64 timestamped history updates plus snapshot.
The same production functions and fixture compile on the host. An independent
coordinate renderer checks every raster pixel; explicit history reference
values check all 512 new updates. Final FNV32 witnesses are respectively
3402950085, 4004879533, 3907113197 and 2271814903. The timed-history workload now hashes bucket times and explicit gaps; its previous sample-ring checksum was 1850849861. These are output checks,
not cryptographic identity. Setup and checksums are outside each timed batch.

Each iteration passes the actual mutable fixture, framebuffer and kernel
through `black_box`; a DSB completes CPU writes before the ending counter read.
Optimized ARM inspection retains eight calls and all four production kernels.
Interrupts and core1 stay enabled, so cycles and elapsed time include contention
and core0 interrupt work. There is no cache reset or controlled cache warmup.

Admission requires privileged core0 thread mode, enabled interrupts, a running
watchdog with at least five seconds left and no debugger-pause flags, a checked
1 MHz TIMER0 tick, and a configured CPU rate between 1 and 300 MHz. DWT must
advertise and enable CYCCNT with tracing enabled; an initial finite probe must
advance it. Timer regressions, zero progress, implausible cycle deltas, changed
timer state, wrong checksums and expired budgets reject the result.

The whole suite has a one-second admission/result-validity budget, including
setup and checksums. Checks surround each finite eight-operation batch; this is
not preemption at exactly one second. A fault or hung operation relies on the
already-running watchdog. At the checked CPU ceiling, one second is below the
32-bit counter wrap interval; wrapping subtraction permits one crossing.

TIMER0's DBGPAUSE bits for both cores are temporarily cleared and read back:
otherwise a debugger halting core1 can freeze the reference timer while core0
CYCCNT advances. The saved word is restored and checked on every normal success
or error return. Panic/reset has no Rust unwinding guarantee. External debugger
interference cannot be fully detected; discard interrupted/debug-stepped runs.
No singleton is stolen and no cycle counter is reset or disabled.

## Capture and records

Build from `firmware` (this command does not flash):

```text
python scripts/capture_target_bench.py bench-stock
```

The capture reserves a new directory under `target/bench-captures`, builds with
an isolated Cargo output tree, takes the unique emitted firmware executable
from Cargo JSON and freezes its bytes before checking RAM boundaries and all
four reviewed flash-critical functions. These checks remain active under
`python -O`. Source, effective Cargo configuration, exact compiler and both ELF
copies must remain unchanged. The archive includes crate-root compile inputs,
including the vendored README and linker template. A failed capture keeps an
incomplete directory; reuse requires a new name. Registry dependencies still
require the recorded lockfile and a trusted Cargo source/cache when rebuilding.

Keep `manifest.json`, `source.zip` and `pico2.elf` together. Decode RTT using
that exact ELF and retain the complete raw log with a device/run identifier,
board revision, power conditions, debugger state and ambient/load conditions.
All `BENCH` records are emitted only after timing and timer restoration.
RTT is nonblocking, so incomplete records can be lost.

| Record | Fields after the tag |
|---|---|
| `BENCHBUILD1` | compiler, profile, optimization, debug, target, `+`-joined features |
| `BENCHCLOCK1` | configured CPU Hz, peripheral Hz, reference Hz, VSEL mV or zero if unknown, requested SPI Hz, divider-derived SPI Hz |
| `BENCHENV1` | framebuffer address, core1 progress before/after, timer DBGPAUSE before/after, initial watchdog us, DWT CTRL, DEMCR |
| `BENCHXIP1` | XIP CTRL, QMI memory-0 timing, read format, read command register words |
| `BENCH1` | kernel 0..3, iterations, start us, end us, raw cycle delta, checksum, status |
| `BENCHEND1` | overall status, attempted sample count |

Accept a run only with complete metadata, four unique kernel rows in order,
eight iterations each, the expected checksums, zero individual status and
`BENCHEND1,0,4`. An earlier successful row does not prove later timer restoration
or suite validity. Status codes: 1 clock, 2 frequency, 3 counter unavailable,
4 counter range, 5 timer state, 6 restoration, 7 budget, 8 context/buffer,
9 watchdog, 10 checksum. Record failures as failures, never zero-cost samples.

Clock and voltage fields are configured register values, not electrical
measurements. XIP words describe configuration after the suite; cache warmth
and concurrent core1 activity remain uncontrolled. The probe is a finite boot
experiment, separate from the warmed-up, at-least-30-second
[frame and diagnostic capture](telemetry-windows.md). Neither demonstrates
physical SPI rate, panel FPS, TE presentation timing or ECU compatibility.

## Verified build and host checkpoint

The isolated `stage08_target_final` build on nightly-2026-10-11 produced ELF
SHA-256 `b44bb2fad052b4d5c3f2b263892c47992e541c9679b27a9361416e1643e7a1af`,
identical to the independently disassembled image. Static RAM is 438456 bytes
with 85832 bytes of core0 stack headroom in this Longan + frame/TE trace profile.
Headroom is a link-time bound, not a measured stack high-water mark. Both source
and artifact reviews found and corrected capture completeness, stale-output,
shared-output replacement and optimized-Python gate defects. Regression checks
cover those contracts and 18 corrupted ELF cases.

`host/baselines/stage08_target_kernels` retains policy-2 provenance and 30 raw
samples per kernel. Median cost per **eight iterations**: clear 17.095 us,
clipped fill 2.562 us, graph/widget 91.356 us, history 5.432 us. Host setup and
checksum are outside timing, and there is no target DSB instruction on the host.
These are initial host costs, not before/after gains or device cycle estimates.

## Source checks

The register rules were reconciled against the
[RP2350 datasheet](https://datasheets.raspberrypi.com/rp2350/rp2350-datasheet.pdf),
[Armv8-M architecture manual](https://documentation-service.arm.com/static/642174c42c42b25bcaeb08cd)
(DWT_CTRL/CYCCNT, pages 376-377) and
[Pico SDK register definitions at the reviewed revision](https://github.com/raspberrypi/pico-sdk/tree/079c6f39023649b154152db30f1d781e884879bc/src/rp2350/hardware_regs/include/hardware/regs).
The implementation was also checked against locked cortex-m 0.7.9 DWT/DCB/DSB,
rp-pac 7.0.0 TIMER/TICKS/QMI accessors, and vendored embassy-rp 0.10.0
`time_driver.rs` and clock setup. The timer driver uses TIMER0.
[Rust's black_box contract](https://doc.rust-lang.org/std/hint/fn.black_box.html)
is an optimization hint; disassembly and reference-output tests supply the
additional evidence for these exact kernels and compiler.
