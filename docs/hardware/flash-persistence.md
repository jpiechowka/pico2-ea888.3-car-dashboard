# Flash persistence contract and lockout prerequisite

## Reviewed source defect and correction

Published Embassy RP 0.10.0 places its RP2350 `SIO_IRQ_FIFO` pause handler in
flash. The optimized pre-change ELF confirmed address `0x10022a29`. Once that
handler acknowledges PAUSE_TOKEN, core0 may erase/program flash. Executing the
parked loop from flash violates the flash operation's preconditions.

This was checked against the [published HAL](https://github.com/embassy-rs/embassy/blob/84444a19eb57d978e3c09fcdc8e60cdd7278eb03/embassy-rp/src/multicore.rs),
the [RP2350 datasheet section 5.4.8.10](https://pip-assets.raspberrypi.com/categories/1214-rp2350/documents/RP-008373-DS-2-rp2350-datasheet.pdf),
and [Pico SDK 2.3.1's explicit RAM handler](https://github.com/raspberrypi/pico-sdk/blob/079c6f39023649b154152db30f1d781e884879bc/src/rp2_common/pico_multicore/multicore.c#L230-L247).
No matching upstream fix was found in the reviewed current multicore source.

The local fork adds RAM placement to the interrupt and extracts its unchanged
body into a private non-inlined RAM helper. Exact cortex-m-rt-macros 0.7.7 forwards
`link_section` to the trampoline and inner function but rejects `inline` on the
interrupt; the normal helper supplies that boundary. Exact cortex-m 0.7.9 uses
native inline assembly. No additional cortex-m feature is needed.

The [saved optimized disassembly](flash-lockout-arm.txt) shows:

- Exported IRQ wrapper: 10 bytes, address `0x20000000`, tail branch to `+12`.
- Helper: 168 bytes, address `0x2000000c`. All branches stay within it; no calls
  or PC-relative literal loads. SIO address `0xd0000050`, NVIC software-trigger
  address `0xe000ef00` and token constants are constructed with immediate
  instructions. Stack accesses use the existing SRAM stack.
- Pause acknowledgement through the WFE/FIFO loop and resume acknowledgement
  remains in these SRAM instructions. Return occurs after core0 restores XIP.
- The secure vector at exception `16+25` contains the Thumb wrapper address.
  Unused nonsecure `SIO_IRQ_FIFO_NS` remains the default handler and is unrelated
  to this secure pause protocol.

`check_flash_lockout.py` checks complete symbol extents, initialized `.data`
placement, vector, relative branch destination and exact reviewed instruction
digests. It also verifies `.data` flags, complete startup copy bounds, and the
unique initialized LOAD mapping against `__sidata`. This deliberately fails on different generated code: re-disassemble and
review before changing hashes. It is a fixed-code verification gate, not a
general machine-code safety analyzer. Nine corrupted artifact fixtures are
rejected. Stock, Longan and Demo300+both-traces pass identical code gates.

The fork provenance manifest retains all 73 original file hashes and now records
three modified source files plus the added UART helper. This prerequisite does
not yet enable runtime flash writes; core0's actual linked flash helper must also
be inspected when the journal is integrated.

## Reserved layout

Keep the repository's conservative 2 MiB flash contract. Official Pico 2 has
4 MiB W25Q32RV ([board datasheet](https://datasheets.raspberrypi.com/pico/pico-2-datasheet.pdf),
[SDK board header](https://github.com/raspberrypi/pico-sdk/blob/079c6f39023649b154152db30f1d781e884879bc/src/boards/include/boards/pico2.h)).
Program FLASH ends at `0x101fe000` (2040 KiB). Settings sectors occupy physical
offsets `0x1fe000` and `0x1ff000`, ending at `0x200000`. They have no initialized
ELF sections. The ELF gate checks every flash PT_LOAD physical file range,
including the flash copy of RAM data/code and the final boot block.

An ordinary loader using only ELF/UF2 ranges should preserve those sectors;
actual update preservation remains a hardware gate. Erase-all/flash-nuke clears
settings. No persistence guarantee follows from a linker reservation alone.

## Runtime integration requirements

The next implementation stages must satisfy all of these before writing:

1. One core0 flash owner limits all operations to the two journal sectors and
   verifies identity QMI mapping. No outer critical section/IRQ context: HAL
   pauses core1 before acquiring its own global critical section, otherwise a
   cross-core spinlock/IRQ deadlock is possible.
2. Stop display admission and drain the actual affine DMA lease; leave CS high,
   SPI idle, all owned DMA idle, no chained transfers and XIP STREAM_CTR zero.
3. Let the encoder finish I2C. Never cancel a live locked-HAL I2C/DMA future.
   Core1's UART owner drains TX/BUSY, invalidates its diagnostic epoch and parks
   its application work while retaining IRQ availability for HAL lockout.
4. A finite one-shot supervisor maintenance ticket starts only while healthy,
   never clears a latched fault, and never renews itself. Keep the hardware
   watchdog active; no feed loop inside synchronous flash writes.
5. A two-sector journal uses distinct 256-byte data and commit pages, binds
   schema/length/generation/CRC, checks value ranges, and verifies blank erase,
   programmed data and committed readback. Never erase the latest valid sector
   before a replacement commits. Torn slots remain consumed.
6. Resume UART only through discard and a fresh qualified handshake; the 32-byte
   hardware FIFO cannot bridge a flash erase. Preserve actual sample ages and
   explicitly break TE interval measurements across maintenance.

HAL async erase/write call blocking operations. Timer cancellation cannot bound
their ROM waits. W25Q32RV's manufacturer AC table gives 240 ms maximum 4 KiB erase
and 2 ms page program under specified conditions, not under hardware failure.
The current 250 ms supervisor-gap policy therefore needs explicit maintenance
handling before this integration. Relevant upstream issues are
[4464](https://github.com/embassy-rs/embassy/issues/4464) (blocking erase),
[3764](https://github.com/embassy-rs/embassy/issues/3764) (translated XIP reads), and
[4506](https://github.com/embassy-rs/embassy/issues/4506) (older release-build hang,
not a reproduced 0.10.0 defect).

## Shared maintenance models

The host and firmware compile the same three prerequisite models. These do not
yet authorize hardware writes:

- `ProgressMonitor` grants a move-only ticket for at most 1000 ms total,
  including preparation, flash and resumption. Entry requires normal health and
  an idle display. Observe calls during the window do not make watchdog feeding
  eligible. Nested, expired, replayed or exhausted tickets fail closed; faults
  remain latched. No display submission or completion is allowed in the window.
  Exit rebases task liveness once, preserving all sensor timestamps. Another
  grant requires fresh progress from acquisition, rendering and input.
- `Vehicle` stops admission and completes every byte of an existing UART
  command, software flush and BUSY-clear under its original 100 ms deadline.
  Old TX completion callbacks cannot schedule more traffic. Only then does it
  invalidate the receive/session epoch and park. Resume requires bounded
  discard and a fresh bootstrap. Terminal quarantine is never reset by a save.
- `TeObservations` keeps raw history, counts maintenance gaps, discards the
  first pending edge result after a gap and requires two new observations
  before exposing another interval. It never cancels the HAL edge future.

Nine added host tests cover these contracts including partial 14-byte commands,
queued old replies, inactive-Pump flush checks, fixed deadline boundaries,
clock/generation failures, fresh progress and retained trace history. Both
outline modes pass 228 runtime tests and two compile-fail checks. Strict host
and Longan firmware lints pass. Longan release uses 381080 bytes static SRAM
with 143208 bytes core0 headroom; the RAM lockout gate passes. Actual task
acknowledgements, core0 flash helper and hardware validation remain separate
integration requirements.

## Checked owner barrier and physical range

The shared `maintenance::Gate` adds a generation and phase checked barrier for
the input and acquisition owners. Both must acknowledge the current Park before
writing; stale, duplicate, failed or previous-phase replies cannot grant it.
Preparation has 400 ms, synchronous work has 500 ms from admission, and the
existing health ticket caps the whole window at 1000 ms. Release also works
after rejected preparation, but both owners must acknowledge Resume before the
ticket can end. Write/resume expiry and backwards initiator time remain fatal
across release. No lock or hardware wait is hidden in this model.

`settings::region::Range` independently confines reads, aligned 256-byte page
programs and aligned 4096-byte erases to the reserved 8192 bytes. It checks zero
length, addition/conversion overflow and end bounds before adding the physical
base. Five barrier tests exercise both owner orders, exact deadline boundaries,
replay/cancellation/failure and clock regression within each phase. The range
test checks every byte offset, page/sector boundary and oversized inputs.

These shared tests establish software admission rules; actual owner
acknowledgements and hardware operations require the integration checks above.

## Hardware acceptance boundary

Host journal power-cut models, release code gates and builds cannot qualify the
board. Still required: actual flash revision, clock/supply limits, cold-cache
dual-core writes, active UART/display/input transitions, watchdog fault injection,
every rollover/power-cut boundary, and reboot/update retention. Overclock modes
are not persistence-qualified by successful builds.
