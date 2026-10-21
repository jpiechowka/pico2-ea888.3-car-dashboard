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

## Rust contract audit (2026-10-21)

Item-level documentation covers retained APIs as well as the selected M33 path.
The source review identified these additional boundary/lifetime corrections:

| Area | Correction and scope |
| --- | --- |
| Core 1 launch | Reject stacks shorter than the eight launch bookkeeping words before reset or pointer writes. This does not budget the launched closure's full stack. |
| RP2040 flash identifier | Empty output returns without entering the hardware path; nonempty reads are bounded to 65,532 bytes after accounting for four dummy bytes in the SSI count. RP2350 uses its own ROM route. |
| Hardware SPI | Widen divider ceiling arithmetic so large frequency requests cannot overflow; zero and unattainable low frequencies remain rejected. |
| Interrupt executor | Initialize storage and publish the started flag in the same cross-core critical section, before unmasking the IRQ. |
| PIO batch | The latest enable/disable request for each selected state machine wins; untouched machines retain their state. |
| HD44780 PIO | Use relative IRQ0 so each state machine signals its matching waiter. |
| AON timer | Validate LPOSC 1..=63 and XOSC 2..=65,535 kHz before writes; reject elapsed-millisecond conversion beyond Embassy's tick range instead of wrapping in release builds. |
| ADC | Reject an empty multichannel selection before register mutation. A zero-length destination is still not documented as a hardware no-op. |
| I2C | Reject zero bus frequency before division and peripheral disable; reject an empty async write iterator before waiting for a STOP that no command could generate. Constructors still perform reset/pin setup before timing validation. |
| PWM duty trait | Use TOP+1 for the representable full-duty endpoint; return an explicit error when TOP 65535 would require compare 65536. Direct `Config` compares and configured inversion remain unchanged. |
| RP2040 float arithmetic | Correct nonfinite multiply/divide signs and NaN handling; pass both `atan2` coordinates unchanged to ROM after NaN admission. Finite ROM approximations remain unchanged. |
| RP2040 float ABI | Signed EABI conversions truncate toward zero using negative magnitude; `float_to_fix64` returns `i64`, matching ROM. The latter is an intentional public signature correction. |
| RP2040 ROM cache | Replace the generated mutable-static pointer cache with atomic publication and exact trampoline identity. Resolve the actual ROM target before an XIP-sensitive call. |
| Integer intrinsic ABI | Remove u32 implementations of the three DI64 aliases; retain SI32 aliases and let compiler-builtins supply the correct 64-bit implementations. |
| RTC and clock observations | Respect the RP2040 zero divider encoding and reject RTC rates outside the retained `u16` representation; return cached RTC output and keep unmeasured LPOSC frequency unavailable. |
| UART line control | Compute the required 15-bit delay with shared wide ceiling arithmetic, including low peripheral clocks. |
| RP235x ROM ABI | Supply `pick_ab_parition`'s fourth `u32` boot-window-base argument; retain its historical spelling. No existing caller used this wrapper. |
| PIO pin windows | Translate explicit JMP pins and synchronizer bypass relative to GPIOBASE; reject incompatible windows before writes. Optional side-set enable consumes an encoding bit, not another GPIO. All machines still share GPIOBASE. |
| USB control lifecycle | Admit EP0 packet sizes 8/16/32/64; cancel obsolete SETUP/reset/disable work and reject stale address completion. Explicit disable does not rearm OUT. Re-enable restarts the bus event lifecycle. |
| USB ownership | Track endpoint generations and quiesce pending buffers through EP_ABORT/DONE. A bounded abort timeout or unsupported early RP2040 revision quarantines that direction without reusing its buffer. Hardware bus reset retains the primary driver's protocol-reset assumption. |

Native tests use production helpers, assembled PIO instructions or controlled
future/register fixtures. They do not establish hardware timing, ROM accuracy or
physical race freedom. The RP2040-only float/ROM routes are retained upstream API
coverage, not a newly supported dashboard board. Their final ABI/arithmetic checks
and source contracts are part of the same review.

USB generations add 128 static bytes; `ControlPipe` adds one `u32`. PIO `Config`
adds a private explicit-JMP flag. Whole-bus disable can perform at most
32 × 65,536 abort-status reads in a critical section; this bounds work, not wall
time. Fake-register tests do not model SIE timing, SRAM arbitration or silicon
errata. The [21 October audit](../../../docs/validation/rust-documentation-audit-2026-10-21.md)
records linked-image checks and the remaining physical limits.

References: [Arm run-time ABI, conversion functions](https://github.com/ARM-software/abi-aa/blob/main/rtabi32/rtabi32.rst),
[Pico SDK RP2040 float entry points](https://github.com/raspberrypi/pico-sdk/blob/master/src/rp2_common/pico_float/float_aeabi_rp2040.S),
[GCC integer runtime functions](https://gcc.gnu.org/onlinedocs/gccint/Integer-library-routines.html),
[embedded-hal PWM contract](https://docs.rs/embedded-hal/1.0.0/embedded_hal/pwm/trait.SetDutyCycle.html),
[TinyUSB endpoint abort and bus reset](https://github.com/hathach/tinyusb/blob/master/src/portable/raspberrypi/rp2040/dcd_rp2040.c),
and [Pico SDK GPIOBASE mapping](https://github.com/raspberrypi/pico-sdk/blob/master/src/rp2_common/hardware_pio/include/hardware/pio.h).
