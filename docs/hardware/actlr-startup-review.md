# RP2350 ACTLR startup review

Reviewed 16 October 2026. Scope: ACTLR.EXTEXCLALL and its current startup callers.
No physical reset, concurrency or flash qualification follows from this review.

## Register meaning

[Cortex-M33 TRM B2.2](https://documentation-service.arm.com/static/5f15c42420b7cf4bc5247f3a)
defines bit 29 as enabling external exclusive signaling for exclusive accesses
regardless of the address shareability attribute. It does not rewrite MPU memory
attributes or supply shared-data ordering. The previous helper comment describing
all memory as shareable was too broad and has been corrected.

The [RP2350 datasheet, M33 ACTLR register](https://datasheets.raspberrypi.com/rp2350/rp2350-datasheet.pdf)
places this bit at PPB offset 0xe008, reset 0. The selected Secure address is
0xe000e008. Both local paths preserve other ACTLR bits with a read/OR/write.
This review does not authorize a custom MPU or Non-secure entry path.

## Actual execution order

**Core0:** the Reset call enters the exact 42-byte stackless hook. It writes 1 to
SIO spinlock 31 at 0xd000017c, writes PROC1 bit 24 to PSM FRCE_OFF at 0x40018004
and waits for the matching readback, writes 0 and waits again, then sets ACTLR
bit 29 and returns through LR. No stack access or Rust call occurs in the hook.
The locked cortex-m-rt 0.7.7 Reset then zeroes BSS, copies data, sets CPACR and
executes DSB followed by ISB before calling main on the selected hard-float
thumbv8m.main-none-eabihf target (has_fpu). These barriers occur after
the ACTLR write and before application atomics. They are not inside pre-init;
the exact startup contract already checks them. A nonterminating PSM poll never
reaches the application.

**Core1:** spawn_core1's startup trampoline calls core1_setup, which installs
MSPLIM, sets ACTLR bit 29 and initializes GPIO interrupt routing. It then takes
the entry closure, executes a compiler fence, signals core0 through FIFO,
publishes IS_CORE1_INIT with Release ordering, enables the FIFO interrupt,
enables the FPU when selected, and invokes the entry closure. The locked
embassy-hal-internal 0.5.0 interrupt helpers and cortex-m 0.7.9 NVIC mask/unmask
use compiler fences/volatile register accesses, not DSB/ISB. There is no explicit
post-ACTLR DSB/ISB pair in this source path. A compiler fence must not be described
as providing one, and core0's barriers do not execute on core1.

## Barrier finding and limits

The [M33 generic guide](https://documentation-service.arm.com/static/5f16e93d20b7cf4bc524af1d),
sections 2.2.3–2.2.6 and 4.2.2, describes PPB as Device-nGnRnE and explains ordering
and synchronization barriers. Neither its ACTLR entry nor TRM B2.2 or the reviewed
RP2350 ACTLR description specifies an extra barrier sequence for EXTEXCLALL.
Device ordering alone is not presented here as a general proof of synchronization
for arbitrary system-register changes.

**No required ACTLR barrier omission was established by these sources.** Core0
already crosses explicit hardware barriers; core1 retains the upstream sequence.
Adding instructions based solely on an older task brief would imply a hardware
fix that this review has not demonstrated. The patch therefore corrects the
comment only. This is not a proof that every possible MPU/security/startup
configuration needs no barrier. Any stronger core1 synchronization claim needs
an applicable architectural requirement or vendor clarification; no such claim
is made here.

## Source provenance and checks

The vendor base is Embassy 0.10.0 at
[84444a19](https://github.com/embassy-rs/embassy/tree/84444a19eb57d978e3c09fcdc8e60cdd7278eb03/embassy-rp).
The exact assembly fragment is the reviewed
[c003af17 backport](https://github.com/embassy-rs/embassy/commit/c003af1767be1b7602ecf2a3b155eb6ce543e06a).
The core1 helper retains the published implementation. Locked runtime sources:
[cortex-m-rt 0.7.7](https://docs.rs/crate/cortex-m-rt/0.7.7/source/src/lib.rs),
[cortex-m 0.7.9 NVIC](https://docs.rs/crate/cortex-m/0.7.9/source/src/peripheral/nvic.rs),
[embassy-hal-internal 0.5.0 interrupts](https://docs.rs/crate/embassy-hal-internal/0.5.0/source/src/interrupt.rs).
These were inspected from the resolved local registry; the historical upstream
assembly fragment was compared with the preserved source snapshot.

Original vendor hashes remain unchanged; only the patched lib.rs hash follows
the comment correction. Existing exact opcode/literal/startup tests remain the
appropriate byte guards. No new hardware barrier test or changed byte allowance
is warranted by a comment-only patch.
