# Reset, alarm and multicast boundary review

Reviewed 2 November 2026 against the vendored Embassy RP and CYW43 sources and
their locked dependencies. These are software corrections; no device was reset,
flashed or used for radio traffic during validation.

## RP2350 reset mask

The retained RP2040 mask covered only bits 0 through 24. RP2350 has 29 reset
fields: TRNG, UART0, UART1 and USBCTRL occupy bits 25 through 28. Clock
initialization writes the complete reset register, then releases selected
peripherals before and after configuring clocks. Omitting the upper fields can
release them early and omit them from the later reset-done wait.

`embassy-rp/src/reset.rs` now selects the mask by silicon feature: `0x01FFFFFF`
for RP2040 and `0x1FFFFFFF` for RP2350. Existing clock-stage exclusions are
unchanged, including the deliberate early exclusions and peripherals whose
clocks are disabled. The selected Pico 2 W initialization uses this path.

The primary reference is the official Pico SDK
[RP2350 reset register definition](https://github.com/raspberrypi/pico-sdk/blob/master/src/rp2350/hardware_regs/include/hardware/regs/resets.h)
(`RESETS_RESET_BITS` and the TRNG/UART0/UART1/USBCTRL fields), cross-checked with
the locked `rp-pac 7.0.0` RP235x register getters. The SDK
[runtime initialization](https://github.com/raspberrypi/pico-sdk/blob/master/src/rp2_common/pico_runtime_init/runtime_init.c)
also distinguishes early reset exclusions from the post-clock release. These
online references were read on the review date; they do not change the source
pins recorded by the repository's vendor provenance.

An actual production-constant regression failed on the old mask at the TRNG
getter. At that repair checkpoint, the 100-test native RP235xa HAL suite passed, as
did the focused RP2040 control. These tests inspect the real PAC field mapping
without MMIO. Final strict Cortex-M33 private HAL documentation also passed
with both the CI feature set and the selected board features. These checks do
not prove boot timing or physical reset behavior.

## Retained RP2040 alarm activation

The RTC scheduler enabled the interrupt and comparator, then waited only for
`MATCH_ACTIVE`. Its interrupt handler clears `MATCH_ENA` before recording the
occurrence. If that handler runs and the hardware retires activation before the
foreground observes it, the old condition can wait for an acknowledgment that
is no longer owed. The actual register/ISR chain supports this software
interleaving; no physical occurrence was reproduced.

The production wait now uses one register snapshot and continues only while
`MATCH_ENA && !MATCH_ACTIVE`. It therefore finishes when activation is observed
or the interrupt has retired the request. Interrupt masking, deadlines,
rearming, and the existing occurrence latch are unchanged. This module is
RP2040-only and is not compiled into the selected RP2350 dashboard/probe.

The official
[RP2040 RTC register definitions](https://github.com/raspberrypi/pico-sdk/blob/master/src/rp2040/hardware_regs/include/hardware/regs/rtc.h)
identify `MATCH_ACTIVE` as read-only bit 29 and `MATCH_ENA` as the writable
global enable in bit 28. These match locked `rp-pac 7.0.0`; they provide no
guarantee that foreground observes activation before the interrupt runs.

The production predicate was extracted unchanged first. A bounded snapshot
test for an activation interval missed during the interrupt failed on the old
predicate, while the ordinary activation control passed. Both pass after the
change, as does the complete 95-test native RP2040 suite, private rustdoc and
the scoped format check. This is interleaving hardening, not measured RTC timing
or physical hardware qualification.

## CYW43 multicast reply admission

The retained Wi-Fi multicast APIs ignored the number of bytes returned by the
actual IOCTL handoff and trusted a little-endian 32-bit count in a zero-filled
64-byte buffer. A four-byte reply declaring eleven entries panicked in both
list and add. A four-byte reply declaring one entry published a fabricated
all-zero address from the unfilled buffer.

Both methods now use one reply validator before indexing or publishing. It
requires the four-byte count, at most ten entries, and all six bytes of each
declared address. Extra unused storage is accepted so full fixed-capacity
replies remain compatible. This preserves the existing count/address wire
layout and does not introduce new firmware commands.

`list_multicast_addresses` now returns
`Result<usize, AddMulticastAddressError>`; `MalformedResponse` leaves the entire
caller output unchanged. Success still returns the count and preserves the
unused output tail. Add reports the same error before sending an update;
duplicate detection, the ten-entry limit and the existing update payload stay
unchanged. Canceling an already submitted update cannot undo its radio-side
effects. General IOCTL firmware-error handling is outside this scoped change.

The full-repository caller search found no application or example caller of
these APIs. Dashboard and probe images use Bluetooth, not this retained Wi-Fi
control route. The malformed input is a controller-response boundary; no
remote-peer injection path was established.

Three regressions failed against the old code using the actual public Control
futures and IOCTL response handoff. Five final tests cover zero, one, ten,
eleven and maximum-u32 counts, short headers/bodies, full-buffer replies,
duplicates, capacity exhaustion, output preservation, and the bytes of a
successful update. The full native Bluetooth/log context passed 45 tests; the
Bluetooth-only/firmware-logs context passed 43. Private rustdoc and the scoped
format check passed. An additional strict standalone CYW43 Clippy run failed
on 99 existing library diagnostics and one additional existing runner-test
diagnostic; it is not reported as a passing gate. Final integrated image and
provenance checks remain separate from these native results.
