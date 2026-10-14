# Core0 flash source and release instruction review

Reviewed 11 October 2026 with nightly-2026-10-10. No hardware execution.

## Three source checks

1. [RP2350 QMI documentation and register definitions](https://github.com/raspberrypi/pico-sdk/blob/079c6f39023649b154152db30f1d781e884879bc/src/rp2350/hardware_regs/include/hardware/regs/qmi.h):
   enter direct mode before waiting for BUSY; never release chip select while
   BUSY; RXEMPTY/TXFULL must be checked for each byte. Direct EN does not control
   the independent explicit chip select bits.
2. [Pico SDK 2.3.1 flash.c](https://github.com/raspberrypi/pico-sdk/blob/079c6f39023649b154152db30f1d781e884879bc/src/rp2_common/hardware_flash/flash.c)
   and [pinned bootrom](https://github.com/raspberrypi/pico-bootrom-rp2350/blob/c6cdb1711f32c3e34faaebd58618a6d096dbd52e/src/main/arm/varm_generic_flash.c):
   connect resets pads/mux and exit changes QMI read timing/format. Copied boot2
   restores boot configuration, so later application timing/pads need explicit
   preservation. The native bootrom waits for final BUSY before deassertion.
3. Exact vendored Embassy RP 0.10.0, rp-pac 7.0.0 and cortex-m 0.7.9 source:
   HAL pauses core1 before its critical section, copies BOOTRAM and resolves
   ROM functions before the RAM inner. The PAC pad indices are 0..5; IO_QSPI
   mux indices are 2..7, because entries 0/1 are USB. Cortex barriers inline.

The [Pico 2 board specification](https://datasheets.raspberrypi.com/pico/pico-2-datasheet.pdf)
identifies W25Q32RV. Its manufacturer JEDEC tuple is EF/40/16 (4 MiB); this tuple
alone does not identify a chip revision. The project still reserves space
within its conservative 2 MiB layout. The 12 clock divider requests at most
25 MHz for this command at the supported CPU profiles, without qualifying the
actual board clocks or supply.

## RAM execution proof for the reviewed artifact

[Independent LLVM instruction listing](flash-core0-arm.txt), from the linked
Longan plus TE release ELF with SHA256
`e9bc87c0db07cfa01f378cd96d12c9ac288cf245a5e47ae98ec925877a1db279`:

| Inner | SRAM extent | Bytes | SHA256 of instructions |
| --- | --- | ---: | --- |
| JEDEC | 200000b4..20000554 | 1184 | a45a5800403e13de2cafd0c6d27e232397784cf812cb81edd53327e0ee69a24e |
| Erase/program | 20000554..2000069c | 328 | 767c70efd1162ef93b11f51b87f9f54eb8b7797de4aaf1fa7ab05c630468f5f2 |

All direct branches stay within their respective inner, including WFE failure
loops. There are no PC-relative flash literals, direct calls to external code,
panic paths or outlined memory helpers. Four JEDEC and up to six write BLX
instructions use the prepared function table: ROM connect, exit, optional
erase/program, cache flush and copied boot2. The entry thunks execute from flash
before connect/exit and only jump to RAM. Wrapper memcpy/lookup also precedes
connect/exit. The actual returned ROM pointers and BOOTRAM contents remain
runtime platform assumptions, not values established by the ELF.

The JEDEC inner writes direct CSR 03000001, 03000005 then 03000000, exchanges
exactly four bytes, discards RX0 and forms the 24-bit ID. Its common finite
budget includes all ten waits and every failed poll. Failure leaves unknown
hardware state and therefore never returns through XIP. Restoration compares
M0 read command/format before restoring timing, writes and reads back all six
pads, checks direct controls/timing/FIFOs, and issues DSB/ISB before return.
Erase and program use the same preservation contract.

The exact-byte gate is intentionally conservative: compiler or code changes
require renewed disassembly review. Complete `.data` SRAM bounds, startup copy
and LOAD source checks apply to these routines as well as core1's handler.
Seventeen corrupted ELF fixtures reject wrong location, size, bytes, missing
inner, IRQ vector, branch and startup-copy metadata. This is not a general
machine-code verifier or evidence of successful physical erase/program.

## Remaining physical checks

### Task2 alternate optimization correction, 14 October 2026

The historical table above describes the reviewed Speed3 artifact. A fresh
SizeZ inspection found a normal flash iterator call after copied boot2 returned
and application timing was installed, while saved pads and their readbacks were
still incomplete. It was outside the direct-mode transaction, and the
intermediate timing/pad combination had not been validated. That artifact
remains rejected.

The local source now spells out all six pad captures, writes and comparisons
with constant indices. Register order, RFMT/RCMD precheck, timing/direct masks,
barriers, fail-stop and ROM/boot2 sequence are preserved. Source review and fresh
whole-body dependency review for Speed3, Speed2, SizeS and SizeZ are pending.
No new digest tuple or hardware qualification follows from this source edit.

On 11 October the updated `nightly-2026-10-11` compiler
(`6f5da406b56c2db6607202b63daee73aa8c61297`, LLVM 23.1.3) reproduced every
reviewed SRAM body byte, address and size. The frozen Longan+TE ELF SHA256 is
`9d48712e80c49958060dd9c6662aa66ac5242f576c9459e889ed1b15de2fbd1f`.
Independent parsing also confirmed unchanged Reset, pre-init, vector table and
14,264 initialized data bytes. The source LMA and Reset's matching source
literal both moved by eight bytes to `1003c960`; the startup copy remains
correctly mapped. All 17 corrupted-artifact checks pass. Static RAM remains
383,656 bytes, with 140,632 bytes core0 headroom. These are linked-image checks.

Measure real JEDEC, ROM/board revision, pin voltage and QSPI state before/after
probe/save. Exercise cold-cache dual-core rollover, UART overflow/rebootstrap,
pending input/display work, power loss, unknown ID and stalled QMI/ROM recovery.
The new poll counter bounds software work only; HAL and ROM waits rely on the
running hardware watchdog. No overclock profile has persistence signoff.
