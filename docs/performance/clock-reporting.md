# Clock reporting

The profiling page shows **configured/requested** CPU and SPI rates. SPI
preserves its fractional MHz digit, including 37.5 MHz at the stock clock.
The divider calculation uses `clk_peri`, matching the locked HAL's SPI
prescaler implementation. These are calculated settings, not scope readings.
An unavailable peripheral clock or invalid CPSDVSR encoding displays `--`.
The shared decoder admits only even prescalers from 2 through 254 and the
encoded SCR divisor from 1 through 256; it never converts a missing rate to zero.

`Vset` shows the regulator selection in millivolts and the requested selection.
An unknown register encoding displays `--`. The HAL's checked `core_voltage()`
replaces the application's raw-register voltage conversion. This is not a
measurement of the physical core rail.

Reviewed sources:

- [RP2350 SDK register definitions](https://github.com/raspberrypi/pico-sdk/blob/master/src/rp2350/hardware_regs/include/hardware/regs/powman.h), VREG/VSEL encoding.
- [Locked Embassy clocks implementation](https://github.com/embassy-rs/embassy/blob/84444a19eb57d978e3c09fcdc8e60cdd7278eb03/embassy-rp/src/clocks.rs), `CoreVoltage` and `core_voltage()`.
- [Locked Embassy SPI implementation](https://github.com/embassy-rs/embassy/blob/84444a19eb57d978e3c09fcdc8e60cdd7278eb03/embassy-rp/src/spi.rs), `calc_prescs()` uses `clk_peri_freq()`.
- Locked `rp-pac 7.0.0` RP235x POWMAN accessor: VREG offset `0x0c`.

The software pipeline now counts completed transfers and exports coherent
[frame/input windows](telemetry-windows.md). [Target CPU benchmarks](target-benchmarks.md)
record configured clock metadata. Physical clock frequency, voltage, panel FPS
and overclock stability still require device acceptance checks.

The [20 October diagnostics re-audit](../validation/diagnostics-reaudit-2026-10-20.md)
traces these values through both firmware routes and the synthetic preview.
