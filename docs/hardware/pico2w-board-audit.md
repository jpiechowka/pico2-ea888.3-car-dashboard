# Pico 2 W board and pin audit

Reviewed 17 October 2026. The owner's photograph identifies **Raspberry Pi Pico
2 W**, mounted on Pimoroni PIM715. Earlier non-W descriptions were incorrect.
This checks source and published drawings; it does not measure the board,
qualify its overclock, or establish physical wireless operation. Updated
2026-10-18 for the optional [BLE runtime](../vehicle/ble-runtime.md).

## Internal pins differ; application header pins remain compatible

No application peripheral remap is needed. The internal allocation differs:

| Resource | Pico 2 W connection | Consequence |
| --- | --- | --- |
| GP23 | Wireless power-on | Not external SMPS mode control |
| GP24 | Wireless data / IRQ | Not USB VBUS sensing |
| GP25 | Wireless chip select | Not the onboard LED |
| GP29 / ADC3 | Wireless clock, shared VSYS/3 measurement | Serialize ADC access with radio use and the documented chip-select state |
| WL_GPIO0 | Onboard green LED | Requires wireless-chip control |
| WL_GPIO1 | Board SMPS power-save mode | Separate from RP2350 core-voltage selection |
| WL_GPIO2 | USB VBUS presence | Requires wireless-chip access |

Sources: [official W pinout](https://datasheets.raspberrypi.com/picow/pico-2-w-pinout.pdf),
[W datasheet, §§2 and 3](https://datasheets.raspberrypi.com/picow/pico-2-w-datasheet.pdf),
[W schematic, sheets 1–2](https://datasheets.raspberrypi.com/picow/pico-2-w-schematic.pdf),
and [SDK board header at 079c6f39](https://github.com/raspberrypi/pico-sdk/blob/079c6f39023649b154152db30f1d781e884879bc/src/boards/include/boards/pico2_w.h).
The SDK corroborates wiring; this Rust application does not execute its setup.

## Actual application ownership

The [entry point](../../firmware/pico2/src/main.rs) constructs these peripherals.
Physical pin numbers refer to the W header drawing, not a mirrored solder view.

| Function | GPIO | Physical pins | Owner |
| --- | --- | --- | --- |
| Optional Longan UART0 TX / RX | 0 / 1 | 1 / 2 | Core 1 buffered UART; no UART DMA |
| Encoder I2C0 SDA / SCL | 4 / 5 | 6 / 7 | Core 0 encoder task / I2C IRQ |
| PIM715 A / B / X / Y | 12 / 13 / 14 / 15 | 16 / 17 / 19 / 20 | Core 0 input tasks |
| LCD DC / CS | 16 / 17 | 21 / 22 | Core 0 display task |
| LCD SPI0 SCK / TX | 18 / 19 | 24 / 25 | Core 0 SPI0, DMA channel 0 |
| LCD backlight | 20 | 26 | PWM slice 2A |
| LCD TE | 21 | 27 | Optional timing observation |
| PIM715 RGB R / G / B | 26 / 27 / 28 | 31 / 32 / 34 | PWM slices 5A / 5B / 6A |
| CYW43439 power/data/CS/clock | 23 / 24 / 25 / 29 | Internal W connections | Optional BLE core0 owner, PIO0 SM0 / IRQ0, DMA channel 1 |

PIM715 RGB is an external active-low LED on GP26–28, distinct from the W's
onboard green LED. LCD reset connects to RUN (physical pin 30). GP7–11 also reach
the pack's SP/CE expansion and are only spare while it is unused. Checked against
the [PIM715 schematic](https://cdn.shopify.com/s/files/1/0174/1800/files/pico_display_2_8_schematic.pdf?v=1728314048).

The `ble-obd` feature owns GP23/24/25/29. With BLE settings enabled it initializes
CYW43439 and the Trouble host; with settings disabled it retains power low and
CS high and participates in flash maintenance without initializing the radio.
No ADC sampler shares GP29. Non-BLE builds do not initialize CYW43439. These are
software ownership facts, not measured shutdown current or a guarantee about
every warm-reset electrical state.

The radio runner and its initialization retain PIO/DMA ownership until normal
bus completion; an outer timeout must not drop those futures. Connection and
GATT deadlines operate above bounded queues. Flash park is acknowledged only at
a completed bus boundary with radio CS high and DMA1 idle; the settings driver
rechecks all DMA channels immediately before writing. A missing acknowledgment
rejects the save. Watchdog recovery covers a stalled owner without reusing its
DMA buffer. See the [source and dependency review](../dependencies/radio-review-2026-10-18.md)
for the distinction between software completion and physical validation.

## HAL, reset and clock audit

The resolved HAL is the reviewed `embassy-rp` 0.10.0 source in
[`firmware/vendor/embassy-rp`](../../firmware/vendor/README.md), with `rp235xa`.

- `src/lib.rs` configuration/init initializes clocks, time, DMA and GPIO IRQs;
  it does not configure CYW43 or a non-W LED.
- `src/gpio.rs::init` sets GPIO IRQ state/priority. It does not select a board
  LED, VBUS detector or SMPS pin.
- `src/clocks.rs::init` resets IO bank/pads and configures the clock tree and
  RP2350 POWMAN regulator. This is not WL_GPIO1 regulator-mode control.
- RP235x pre-init releases the boot-ROM spinlock, resets core 1 and enables the
  required exclusive transactions. It does not perform the RP2040 whole-SIO
  reset or bring up a radio driver.
- [`clock_profile.rs`](../../firmware/pico2/src/clock_profile.rs) uses a 12 MHz
  crystal, matching schematic X1. Core-voltage telemetry is a register setting,
  not a measurement. Profile validation and electrical caveats remain.
- [`settings_flash.rs`](../../firmware/pico2/src/drivers/settings_flash.rs) checks
  IO_QSPI register entries for USB/QSPI and GPIO bit 17 for LCD CS. Those indices
  are not hidden uses of header GP0/1 or W internal wireless pins.

Firmware still targets `thumbv8m.main-none-eabihf` with
`-C target-cpu=cortex-m33`. The `pico2` paths, package names, Cargo aliases and
commands are stable identifiers. Board correction does not require a different
CPU target or rewriting saved projects.

## Flash and power boundaries

The W schematic shows 32-Mbit flash and the SDK declares 4 MiB. The datasheet's
feature list agrees; §3.1 retains a contradictory 2 MB sentence. The build keeps
the deliberate **2 MiB layout**, including the final 8 KiB settings area, and
the settings driver's fail-closed JEDEC check (`0xEF4016`). Neither this audit nor
picotool measures the fitted capacity. Studio requires manufacturer-4-MB
**Pico 2 W** confirmation for the selected device.

Helper protocol 2 carries `genuine_pico2_w`. Old live consent cannot authorize
a new flash; restart the updated helper and refresh Studio. Flash history remains
historical evidence, never a resumed write. See the
[flash contract](../configurator/flashing-design.md).

VBUS pin 40 feeds VSYS pin 39 through the onboard diode. External power still
needs reverse-current blocking and sequencing. 3V3_EN pin 37 disables the board
supply; it is not a universal disconnect for USB or signal wires. The W datasheet
specifies VSYS 1.8–5.5 V and a −20 to +70 °C operating range including self-heating.
Do not import another board's rating or infer automotive qualification. See the
[protected power proposal](../../hardware/protection-plan.md).

The selected prototype uses USB power, not that historical protection assembly.
Removing dashboard USB power does not remove an OBD dongle's vehicle-side supply.
Adapter sleep, USB startup/dips, installed temperature and antenna clearance
remain physical checks; the software identity-only loop may prevent adapter
inactivity sleep.

## Radio and future supply sensing

Wireless is an optional prototype with a separate physical qualification gate.
PIO0/DMA1 ownership preserves display DMA0 and both fixed framebuffers. Radio
firmware, pools, crypto, task futures and stack belong in exact linked-image
admission; see [runtime resources](../vehicle/ble-runtime.md#resources-and-ownership).
Bonds currently stay in RAM. Any future persistence must obey dual-core/XIP
maintenance, and GP29/VSYS sampling must serialize with CYW43. Reconnect epochs,
overclock/radio coexistence, USB power and antenna clearance need device evidence.
The [selected USB arrangement](../../hardware/README.md) supersedes the older
[transport comparison](../../hardware/transport-options.md).

## Verification boundary

Source review covers pin owners, HAL defaults, clocks, reset, settings flash,
CPU flags and flash-consent migration. Regression tests cover protocol admission,
session mismatch, selected-device consent and historical recovery. Native/Wasm
checks and ARM builds establish software compatibility, not measured pin levels,
radio behavior, ECU support, capacity, thermal margin or physical FPS.
