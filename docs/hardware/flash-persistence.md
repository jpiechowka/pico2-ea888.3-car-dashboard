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

## Acceptance boundary

Host journal power-cut models, release code gates and builds cannot qualify the
board. Still required: actual flash revision, clock/supply limits, cold-cache
dual-core writes, active UART/display/input transitions, watchdog fault injection,
every rollover/power-cut boundary, and reboot/update retention. Overclock modes
are not persistence-qualified by successful builds.
