# Local Embassy RP patches

This tree retains the published `embassy-rp` **0.10.0** base, recorded by
`.cargo_vcs_info.json` as upstream commit
`84444a19eb57d978e3c09fcdc8e60cdd7278eb03`. Local changes are intentional;
replacing this directory with the published crate would lose them. Licenses and
the original upstream provenance remain intact.

The [vendor overview](../README.md) records the earlier UART ownership,
cross-core flash, PIO 0.4 and safety-documentation work. The
[20 October audit note](../../../docs/dependencies/full-code-audit-2026-10-20.md)
records the additional corrections, exact dependency alignment, upstream
references, reproduction commands and verification limits.

## Corrections added by the 20 October audit

| Area | Retained local behavior |
| --- | --- |
| PIO and clocks | Separate state/wakers/pin ownership for each PIO instance; validated PIO dividers; correct RP235x fractional clock fields and encoded maximum divider; widened PLL arithmetic. |
| GPIO and PWM | Cancel a GPIO wait in the interrupt bank where it was armed; use a nonzero PWM disable mask; transfer running PWM ownership when splitting outputs. |
| Calendar and AON timer | Validate dates and timestamp bounds before narrowing; password-protected interrupt writes, explicit pending-state handling, checked relative deadlines and core-bound alarm ownership. |
| I2C slave, TRNG and boot metadata | Reject empty slave destinations without losing a pending byte; stop TRNG work/interrupts on cancellation; bound partition names/families and correct rolling-window metadata size. |
| RP235x ROM | Correct the `validate_ns_buffer` calling convention and document its RCP boolean arguments. |
| Hardware UART | Correct fractional rounding and long break arithmetic; retain independent TX DMA ownership during RX cleanup; preserve early-break continuation; genuinely nonblocking serial methods; flush through FIFO/shifter completion; clear break on cancellation. |
| Optional PIO programs | Checked fractional timing and multiplication; I2S format admission/stop; OneWire presence and cancellation recovery; PWM duration/queue handling; SPI DMA aliasing and completion; exact step counts and cancellation recovery; WS2812 drain/reset timing. |
| PIO UART | Checked TX/RX baud calculation; flush waits for FIFO/OSR idle and a full stop-bit interval while keeping TX high. Cancelling flush does not abort or reset transmission. |
| Optional PSRAM | Validate timing/admission before hardware mutation; account for cache-line completion in select timing; bound dummy bits; distinguish identity/size errors; preserve mapping and core-resume cleanup. |

These fixes preserve the reviewed flash-maintenance and UART ownership contracts.
PIO helper error recovery is specific to each protocol: SPI idles low; UART
idles high. A cancelled write may already have transmitted a prefix. Do not
equate cancellation cleanup with transactional rollback.

## Scope and maintenance

The supported hardware remains Pico 2 W / RP2350 Cortex-M33. Both radio consumers
use `cyw43_pio::PioSpi`, which is separate from this crate's optional
`pio_programs::spi`. PIM715 backlight/RGB use ordinary PWM, not the WS2812 helper.
The selected USB-powered BLE assembly does not use the optional PIO UART,
OneWire, I2S, stepper or PSRAM helpers. The retained Longan alternative uses
hardware buffered UART. See the dated note for consumer paths and limitations.

`Cargo.toml` and `Cargo.toml.orig` include native-only `embassy-time` mock-driver
and generic-queue dev features for permanent UART future/timing regressions.
They do not select a production ARM timer implementation. The standalone
`Cargo.lock` is aligned to the firmware workspace's resolved package identities;
run native tests from this actual manifest with `--locked`, as documented in the
dated note. MMIO tests must run with one test thread.

The native tests check arithmetic, modeled register state and future lifecycle.
They do not emulate the RP2350, its PIO instruction engine, bus ordering or wire
timing. Physical clock tolerance, UART stop bits, DMA/IRQ races, radio operation
and flash/XIP behavior still need their applicable hardware validation. No fix
here is evidence of the cause of a photographed watchdog reset.
