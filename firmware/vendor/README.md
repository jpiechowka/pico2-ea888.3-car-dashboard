# Local Embassy RP corrections

Base: the complete published `embassy-rp 0.10.0` crate, upstream commit
`84444a19eb57d978e3c09fcdc8e60cdd7278eb03`. Both original licenses are retained.
`embassy-rp-provenance.json` records the download URL, archive SHA-256, all 73
original file hashes and all seven patched/added file hashes. `.gitattributes`
preserves these bytes across platforms. UART, multicore and flash have runtime corrections. PWM has only a period-comment
correction (phase-correct factor2, ordinary factor1); UART ring operations and the fixed JEDEC transaction have
additional shared helpers compiled by the host tests.

Run `python scripts/check_vendor.py` from `firmware`. Supply
`--archive /path/to/embassy-rp-0.10.0.crate` to also verify the original archive.
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
- Async `flush` completes when the **software ring** is empty. The owner must
  separately wait for BUSY to clear, under a deadline, before acknowledging a
  complete wire write or changing baud. UART completion is not a CAN TX ACK.

## Checks and remaining limitations

`host/tests/buffered_uart.rs` exercises the actual shared operations: every ring
wrap and fault position, wakeups injected during rearm/drain, sticky error
precedence, 32-byte limits, invalid consumption, core ownership and 100,000-byte
concurrent publication. `scripts/check_uart_api.py` compiles a positive target
control and rejects stack buffers, reborrowed peripherals, borrowed conversions,
cross-core moves and shared handles. All six invalid control cases compiled
against the initial released API before the ownership patch.

The async acquisition wrapper must exclude blocking/legacy NB TX methods and
`send_break`: those inherited methods can spin; cancelling `send_break` after
assertion leaves the break asserted. This fork does not claim those APIs are
cancellation safe. Hardware FIFOs, interrupt latency and recovery are not tested
by the host model. At 115200 8N1 with the published 28/32 RX threshold, four byte
times are about 347 us; a 1024-byte software ring cannot compensate for long IRQ
masking. Measure that budget and error injection on the actual RP2350/adapter.

Vendoring exposes the existing `cortex_m_rt::pre_init` deprecation warning in
unchanged `src/lib.rs`; the crate registry normally caps dependency diagnostics.
The unrelated `proc-macro-error2 2.0.1` future incompatibility remains documented
in the dependency review. Neither warning is hidden by modifying upstream files.

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
fail-stop behavior. All four optimization modes require fresh complete SRAM
instruction review before their coherent byte tuples can be admitted. The
earlier SizeZ artifact remains rejected; this source correction is not physical
flash qualification. Original archive hashes and upstream revision are retained.

This is the application's single-CS, normal QSPI, 3.3 V Pico 2 contract. The
caller must reject CS1/PSRAM/custom mappings, quiesce its owners, provide SRAM
program data and retain a running watchdog. HAL pause/resume/DMA/ROM waits still
have no software-returning timeout. Debug persistence is disabled until its
distinct machine code is reviewed. Physical flash and recovery validation is
pending; host protocol tests do not exercise QMI registers.

## PWM documentation correction (13 October 2026)

The `Config` period comment now agrees with RP2350 section12.5.2.6:
`(top + 1) * (phase_correct ? 2 : 1) * divider`. No PWM instructions or API
changed. Five original files are modified and two helpers added; the provenance
manifest retains the original PWM hash alongside its exact corrected hash.
The PIM715 RGB adapter uses the existing constructor and independent compare
configuration with owned slices5/6, inversion, TOP9999 and divider1.
