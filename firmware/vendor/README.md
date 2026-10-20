# Local Embassy RP corrections

Base: the complete published `embassy-rp 0.10.0` crate, upstream commit
`84444a19eb57d978e3c09fcdc8e60cdd7278eb03`. Both original licenses are retained.
`embassy-rp-provenance.json` records the download URL, archive SHA-256, all 73
original file hashes and the complete reviewed overlay. `.gitattributes`
preserves these bytes across platforms. The current
[HAL patch summary](embassy-rp/LOCAL_PATCHES.md) and
[20 October source audit](../../docs/dependencies/full-code-audit-2026-10-20.md)
extend the historical UART, multicore, flash and PWM work below with runtime
and cancellation fixes. Shared helpers are compiled by host/native tests.

Run `cargo run --locked -p dashboard-provenance -- hal --vendor-root firmware/vendor`
from the repository root, or `python scripts/check_vendor.py` from `firmware`.
Supply `--archive /path/to/embassy-rp-0.10.0.crate` to also verify the original archive.
CI verifies the fork tree. A future upstream upgrade must repeat the source,
ownership, interrupt and hardware review before removing this patch.

## Source checks (11 October 2026)

The published source was compared with the exact upstream fixes and RP2350
register documentation. The following fixes are absent from the release:

| Upstream | Reviewed behavior | Local implementation |
| --- | --- | --- |
| [PR 7014](https://github.com/embassy-rs/embassy/pull/7014), merge `ced168b778e3390f15f7b3c1a57eb77c063e6a9d` | Register RX waker before checking/rearming; rearm on empty reads | Same ordering for healthy RX. An error stays latched until explicit discard, because automatic recovery could concatenate bytes across known loss. |
| [PR 7015](https://github.com/embassy-rs/embassy/pull/7015), merge `8ba7a9c1f110f5b1834630ac2b6cfab436e33022` | TX full-ring/flush lost wakeups | Register before state checks and pend even when the TX ring is full. |
| [PR 7129](https://github.com/embassy-rs/embassy/pull/7129), merge `816b4e95d70820d0a44d0a6499ed5009efb9a0bf` | TX wrap can leave data queued without a new interrupt | Drain across wrap spans, with a 32-byte work limit and a further software kick if needed. |
| [Issue 6892](https://github.com/embassy-rs/embassy/issues/6892) | Erased storage lifetime permits stack-backed buffers to escape | All constructors/conversions require static buffers and peripheral ownership. |
| [Issue 1635](https://github.com/embassy-rs/embassy/issues/1635) | Software pends and NVIC enable are core-local | Handles are neither `Send` nor `Sync`. A guarded core claim also covers separately converted blocking halves. |

Hardware reference: [RP2350 datasheet, UART chapter 12.1](https://pip-assets.raspberrypi.com/categories/1214-rp2350/documents/RP-008373-DS-2-rp2350-datasheet.pdf),
particularly receive errors, the 32-entry FIFOs, transition-triggered TX IRQ
(12.1.6.3), UARTRIS/ICR/IMSC, and UARTFR.BUSY. The exact published HAL ring,
critical-section implementation, UART/PAC register access and the above upstream
patches were read. The ring is `embassy-hal-internal 0.5.0`; host tests compile
that same dependency and `src/uart/buffered_io.rs`.

## Ownership and fault contract

- Construct and use the UART, its IRQ, both halves and their Drop on one core.
  Direct handles cannot cross cores. Static blocking halves can be converted
  independently only on the same core. Conversion checks the actual UART
  instance before installing buffers. Initialization is one critical section;
  an existing ring cannot be replaced, and adding a half preserves the TX kick
  and a latched RX fault. The last Drop disables its local IRQ before releasing
  the core claim. Each Drop masks its own peripheral IRQ sources first.
- RX errors are sticky, including raw interrupt error status before FIFO reads.
  No queued prefix or subsequent bytes are delivered once an error is observed.
  A reader checks the error both before and after copying. The ISR masks faulty
  RX, does bounded work and performs no formatted logging. A full ring still
  enables error IRQs so hardware overrun is observable.
- `rx_error()` is observational. `discard_rx()` masks RX, drains software spans
  and at most 32 FIFO entries, and clears old error state before draining so new
  errors remain observable. A still-nonempty FIFO is treated as loss. Successful
  discard **does not establish protocol alignment**. Invalidate the parser and
  transaction first, then perform a qualified adapter handshake. Bytes already
  delivered before an interrupt cannot retrospectively be revoked.
- The legacy nonblocking RX trait uses the same ring and sticky errors, avoiding
  a second FIFO consumer. `BufRead::consume` checks the contiguous readable
  length before changing indices. Cancelling an async read/write/flush retains
  no DMA or user slice pointer; only complete copied ring bytes remain.
- Async `flush` waits for the software ring, hardware FIFO and shifter to finish.
  The application still supplies the operation deadline; hardware-stalled output
  is not an unbounded application wait. UART completion is not a CAN TX ACK.

## Checks and remaining limitations

`host/tests/buffered_uart.rs` exercises the actual shared operations: every ring
wrap and fault position, wakeups injected during rearm/drain, sticky error
precedence, 32-byte limits, invalid consumption, core ownership and 100,000-byte
concurrent publication. `scripts/check_uart_api.py` compiles a positive target
control and rejects stack buffers, reborrowed peripherals, borrowed conversions,
cross-core moves and shared handles. All six invalid control cases compiled
against the initial released API before the ownership patch.

The async acquisition wrapper still excludes blocking methods. The 20 October
audit corrected legacy nonblocking TX/flush to return `WouldBlock` and added a
drop guard that clears an asserted break on cancellation. Break setup still
performs a blocking pre-flush, so it is not a fully nonblocking operation.
Hardware FIFOs, interrupt latency and recovery are not tested
by the host model. At 115200 8N1 with the published 28/32 RX threshold, four byte
times are about 347 us; a 1024-byte software ring cannot compensate for long IRQ
masking. Measure that budget and error injection on the actual RP2350/adapter.

The original vendor checkpoint exposed the `cortex_m_rt::pre_init` deprecation
and `proc-macro-error2 2.0.1` future incompatibility. The 16 October compatibility
backports below resolve both without suppressing diagnostics.

Historical UART-patch checkpoint before runtime integration: 150 host runtime
tests and two compile-fail
checks pass in both outline modes; all seven UART target API cases pass; strict
application/host Clippy and formatting pass. Stock and serial/full-frame builds
use 374,352 bytes of static RAM with 149,936 bytes of core-0 headroom. Demo with
300 MHz and both traces uses 381,136 bytes with 143,152 bytes of headroom. These
builds did not yet instantiate the acquisition UART; the target API control did.
The current optional longan-rx12 runtime now constructs it on core1; see
[the integrated owner](../../docs/vehicle/diagnostic-transport.md#core-1-runtime-and-qualification-gate).

`host/baselines/stage06_uart` preserves the first Rust CPU fixture for the shared
RX ring path, before runtime integration: one RX12 record, 1024-byte ring,
rotating wrap offsets, FIFO production plus consumer copies and sticky-error
checks. It excludes actual registers, IRQ entry, waker scheduling and wire time.
The 30-sample median is about 8.3 ns on the recorded host. This is a new fixture,
not evidence of a speedup over the released driver or physical throughput.

## RP2350 flash prerequisites

The secure SIO pause wrapper and helper reside entirely in initialized SRAM.
The release ELF gate checks their exact instruction bytes, complete startup
copy, relative branch and IRQ25 vector. See
[flash persistence](../../docs/hardware/flash-persistence.md) for the reproduced
released-HAL placement defect and primary source references.

`blocking_jedec_id` now supports RP2350 through the existing HAL lockout. Its
fixed `9F 00 00 00` transaction has one 100,000-poll budget across all entry,
TX, RX and final waits. It releases CS only after BUSY clears and both FIFOs
are empty. Budget exhaustion or inconsistent state stays in RAM for watchdog
recovery. The four-byte protocol is shared with host tests for wire order,
ID bytes, every budget cutoff, delayed readiness and FIFO faults.

Both JEDEC and erase/program RAM helpers preserve six QSPI pads, M0 read timing
and direct timing. Copied boot2 must first restore the same read command/format;
individual readbacks and DSB/ISB precede return. Unknown restoration never logs,
panics or returns through flash. ROM functions are resolved and the 256-byte
BOOTRAM setup copied to aligned stack storage before XIP changes. See
[core0 flash review](../../docs/hardware/flash-core0.md) for the inspected bytes
and source cross-checks.

Task2's 14 October source correction uses six explicit constant pad indices for
capture, restoration writes and readbacks. SizeZ had emitted a flash iterator
call after application timing was restored but before pad restoration/readbacks
completed. The corrected source preserves register order, masks, barriers and
fail-stop behavior. Independent complete SRAM instruction review at `e6ca58f`
with nightly-2026-10-12 covered Speed3, Speed2, SizeS and SizeZ, including all
literals and dependencies through restoration. The gate accepts their four
exact coherent body tuples; see the current table in the core0 review. The
earlier unsafe SizeZ artifact remains rejected. Full artifact gates and physical
flash qualification remain separate. Original archive hashes and upstream
revision are retained.

This is the application's single-CS, normal QSPI, 3.3 V Pico 2 W contract. The
caller must reject CS1/PSRAM/custom mappings, quiesce its owners, provide SRAM
program data and retain a running watchdog. HAL pause/resume/DMA/ROM waits still
have no software-returning timeout. Debug persistence is disabled until its
distinct machine code is reviewed. Physical flash and recovery validation is
pending; host protocol tests do not exercise QMI registers.

## Historical PWM documentation correction (13 October 2026)

The `Config` period comment now agrees with RP2350 section12.5.2.6:
`(top + 1) * (phase_correct ? 2 : 1) * divider`. No PWM instructions or API
changed. Five original files are modified and two helpers added; the provenance
manifest retains the original PWM hash alongside its exact corrected hash.
The PIM715 RGB adapter uses the existing constructor and independent compare
configuration with owned slices5/6, inversion, TOP9999 and divider1. The later
20 October audit also corrects PWM teardown and split ownership; this paragraph
records only the earlier documentation checkpoint.

## PIO and startup compatibility backports (16 October 2026)

At that checkpoint the base remained the published 0.10.0 archive and the prior
UART, multicore, flash, flash-probe and PWM patch bytes were unchanged. Original
hashes remain immutable; the current overlay is described in the 20 October note.

- [Embassy commit 8c38d7e](https://github.com/embassy-rs/embassy/commit/8c38d7e55d00732a9d5cb0b5255bae80d1891781)
  updates PIO to 0.4. Both local Cargo manifests now agree. Upstream's additional
  IR NEC assembler-call conversion does not apply to published 0.10.0, which
  has no such module. Existing PIO instruction helpers and programs compile
  unchanged. The firmware lock selects pio/core/parser/proc 0.4.0 and removes
  proc-macro-error2 and its attribute crate. [PIO PR 85](https://github.com/rp-rs/pio-rs/pull/85)
  replaces that macro diagnostic dependency with Syn.
- [Embassy commit c003af17](https://github.com/embassy-rs/embassy/commit/c003af1767be1b7602ecf2a3b155eb6ce543e06a)
  supplies the exact assembly startup fragment. Rust must not execute before
  RAM initialization. RP2350 releases SIO spinlock31, asserts then clears
  PROC1 reset with readback waits, and sets ACTLR.EXTEXCLALL. RP2040 retains
  its separate force-on/force-off sequence. The post-runtime core1 helper is
  unchanged. The unused private `write_and_wait` helper is retained to avoid
  unrelated changes.

The constants were cross-checked against the locked rp-pac 7.0.0 register
accessors: RP2350 SIO spinlock31 `0xd000017c`, PSM FRCE_OFF `0x40018004`,
PROC1 bit24, and Cortex-M ICB ACTLR `0xe000e008` bit29. These are the same
operations as the original Rust implementation, expressed without stack,
initialized RAM, Rust calls or compiler-generated startup dependencies.

Focused checks on nightly-2026-10-16: ARM check and one default release link
passed without either previous warning. The stripped release reset begins at
`0x10000140` and calls the hook at `0x10000198` before BSS/data setup. Direct
inspection verified its complete 42-byte instruction body and three MMIO
literals; body SHA-256 is
`2e607823f0c33ea503d542e8961cbbe910238fc5a80886a4cd822b9e3598bc06`.
The original archive and all vendor file hashes passed `check_vendor.py`.

The startup consumer retains the historical Rust grammars and adds a separate
exact 42-byte assembly contract. It binds typed symbol/extent, zero frame,
complete Thumb coverage, each literal opcode and read-only pool, MMIO values,
reset call sequence and preserved LR. The current linked Reset/hook passes
this focused consumer. Mutation tests reject every changed code byte and
changed literals, mapping, symbols, frame metadata, branch/return text or pool
aliases. Historical startup/preclaim/history tests remain unchanged.

The experimental full-proof path qualifies pre-init through private compositions;
its complete ordinary execution proof remains unfinished and fails closed.
Ordinary builds use the separate [practical image gate](../../docs/performance/memory-layout.md),
which retains source/config identities, memory and conservative stack bounds,
boot/flash checks and explicit assumptions. It does not issue a complete
instruction proof. Consumed-input closure and physical qualification remain open.

This does not requalify flash machine-code gates, other profile/feature images,
RP2040 linking, or physical reset/PIO timing. Those checks remain separate;
no hardware was accessed. The single linked artifact is evidence for this
startup backport, not approval to flash a new image.

## ACTLR ordering review (16 October 2026)

The EXTEXCLALL helper comment now describes external exclusive transactions,
without claiming that it changes every memory region's shareability. No register
operation or startup instruction changed. See the [source review](../../docs/hardware/actlr-startup-review.md)
for the distinct core0/core1 paths and the limit of the barrier finding.
