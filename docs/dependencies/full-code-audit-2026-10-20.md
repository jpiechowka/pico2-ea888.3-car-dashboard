# HAL and linker audit, 20 October 2026

This note records the HAL and linker portion of the full code audit. It is a
software evidence checkpoint, not a declaration that all integration or physical
qualification is complete. The source remains the local
[`embassy-rp` tree](../../firmware/vendor/embassy-rp/LOCAL_PATCHES.md).
Radio source and binary provenance are documented separately in the
[vendor overview](../../firmware/vendor/README.md) and
[CYW43 firmware notes](../../firmware/vendor/cyw43-firmware/README.md).

## Published base and copied inputs

The latest published stable Embassy RP release checked on 20 October is
**0.10.0**, released on 20 March 2026. The local tree retains that version and
its upstream source identity
`84444a19eb57d978e3c09fcdc8e60cdd7278eb03`; it does not claim to be unmodified
upstream. Existing PIO 0.4, buffered UART ownership and flash-maintenance patches
remain. The published crate still declares PIO 0.3. Sources:
[release inventory](https://docs.rs/crate/embassy-rp/latest),
[published manifest](https://docs.rs/crate/embassy-rp/0.10.0/source/Cargo.toml).

The HAL is Rust source. The two `memory.x` files are text linker scripts defining
addresses and sections. The CYW43 inputs are firmware/configuration binaries.
These have different provenance and validation requirements; a newer generic
linker example cannot replace the application's reserved memory layout.

## Actual application consumers

| Route | Relationship to these changes |
| --- | --- |
| Dashboard and standalone probe | Both use the local HAL for RP2350 peripherals and clocks. Each current radio route creates one PIO instance, then constructs `cyw43_pio::PioSpi`; the optional HAL SPI program is a different implementation. |
| PIM715 display and RGB | Backlight/RGB use ordinary hardware PWM. GPIO and PWM ownership changes affect the common peripheral layer. The WS2812 helper does not drive the PIM715 RGB LED. |
| Retained Longan alternative | `firmware/pico2/src/drivers/longan.rs` uses core-owned `BufferedUart` and static ring buffers. It polls read/write/flush futures; it does not use the PIO UART helper. This alternative is outside the selected USB-powered BLE assembly. |
| Optional HAL facilities | PIO UART, OneWire, I2S, stepper, PIO PWM, WS2812, I2C slave, AON alarm, TRNG, ROM buffer validation and PSRAM corrections retain library behavior for future consumers. The selected two application routes do not establish physical qualification for them. |

The radio constructors are in
[`pico2/src/tasks/ble.rs`](../../firmware/pico2/src/tasks/ble.rs) and
[`ble-probe/src/radio.rs`](../../firmware/ble-probe/src/radio.rs).
Current use of one PIO instance and integer SYS dividers limits exposure to the
multi-instance and fractional-divider defects. It does not make the shared
library contracts correct without the fixes. None of these source findings
identifies a physical watchdog-reset cause.

## Retained corrections

The table describes source behavior and regression scope. Hardware references
appear below; the local locked implementation remains the authority for what
this repository executes.

| Source area | Correction and boundary covered |
| --- | --- |
| `pio/mod.rs`, `clocks.rs` | Allocate state/wakers/pin ownership per PIO0/1/2, validate PIO divisors, use RP235x's 16 fractional SYS/GPOUT bits, interpret encoded maximum dividers and widen PLL calculations before validation. |
| `gpio.rs`, `pwm.rs` | Disarm a cancelled GPIO wait in its original CPU interrupt bank. PWM teardown writes the actual disable bit; split transfers running ownership instead of stopping outputs through the original owner's destructor. |
| `datetime/*`, `aon_timer/mod.rs` | Reject invalid calendar fields and timestamps outside 1970..4095 before narrowing. Alarm interrupt writes include the required password and explicit W1C handling, clear stale software completion, check deadline overflow and retain core affinity. |
| `i2c_slave.rs`, `trng.rs` | Empty/full slave buffers cannot silently lose a pending byte. TRNG completion or cancellation stops generation and its interrupt source. |
| `block.rs`, `rom_data/rp235x.rs` | Bound UTF-8 partition names to 127 bytes and extra UF2 families to three, include encoded name length/padding, correct rolling-window item size, and match the ROM buffer-validation ABI including the output status pointer. |
| `uart/mod.rs`, `uart/buffered*.rs` | Round the complete 16.6 divisor with carry; widen break-duration arithmetic; preserve TX DMA enable when RX ends; account for a consumed break byte before deciding whether to resume DMA; implement `WouldBlock` for nonblocking traits. Buffered flush waits for ring, FIFO and shifter completion. A drop guard clears break on cancellation. |
| `pio_programs/clock_divider.rs`, `clk.rs` | Share checked fractional clock conversion, including the encoded maximum divider; reject zero, overflow and unrepresentable requests before pin mutation. |
| `pio_programs/i2s.rs`, `pwm.rs` | Validate supported I2S sample/channel layouts and actually stop input on `stop()`. Convert PWM duration with fractional clocks and wide arithmetic; handle a stopped state machine and full queued levels without indefinite waiting. PIO instruction overhead remains part of the period. |
| `pio_programs/onewire.rs`, `stepper.rs` | Treat idle-high OneWire sampling as absence; validate strong-pull-up commands and restore bus/state-machine ownership on cancellation. Emit the requested step count, including zero and signed-minimum boundaries, and clear stale IRQ/state on cancelled motion. |
| `pio_programs/spi.rs`, `ws2812.rs` | Avoid overlapping Rust references in in-place SPI DMA, restore protocol-specific idle after cancellation and wait for actual PIO transmit idle. WS2812 waits for FIFO/OSR drain and reset intervals, including a cancelled predecessor and an empty LED array. |
| `pio_programs/uart.rs` | Check `baud * 8`, retain fractional divisors for RX and TX, wait for fresh transmit-idle evidence and a stop-bit interval on flush. TX remains high; cancelling flush leaves queued/in-flight data intact. |
| `psram.rs` | Validate sizes, dummy-bit encoding and timing fields before QMI mutation or core pause. Reserve worst-case cache-line completion in MAX_SELECT calculations, bound deselect timing, distinguish unknown identity from size mismatch and resume the other core on failure. Mapping setup does not depend on an optional enter-quad command. |

Hardware UART break setup still performs a blocking pre-flush; these changes do
not make every UART operation nonblocking. Cancelled writes may have transmitted
a prefix. OneWire cancellation requires a new reset transaction. PSRAM remains
unused by the supported assembly and requires its own RAM/XIP and electrical
qualification before adoption.

### PIO UART flush reasoning

At 125 MHz SYS and 1 Mbaud, the previous integer divider selected 15, yielding
approximately 1.041667 Mbaud. The shared 16.8 divider represents 15.625, retaining
the requested rate at this operating point. A large `baud * 8` previously wrapped
in release builds; construction now rejects overflow explicitly.

The TX program stalls at `pull side 1 [7]`. Fresh stall evidence with an empty
FIFO establishes the high stop/idle state, but the instruction's delay runs only
after the stalled pull completes. Flush therefore also waits a conservative
monotonic interval for eight PIO cycles at the configured divider, rounded up
with a timer-quantization margin. It requires a working Embassy time driver and
an unchanged SYS clock during use. Cancellation does not reset the state machine
or force UART idle low. This implements the buffered-output completion intent of
[`embedded-io-async` 0.7 `Write::flush`](https://docs.rs/embedded-io-async/0.7.0/embedded_io_async/trait.Write.html).
The instruction sequence follows Raspberry Pi's
[PIO UART example](https://github.com/raspberrypi/pico-examples/blob/master/pio/uart_tx/uart_tx.pio);
stall/delay and fractional-clock semantics are described in the
[Pico C/C++ SDK manual](https://datasheets.raspberrypi.com/pico/raspberry-pi-pico-c-sdk.pdf),
sections 3.4.5 and 5.1.16.

## Complete upstream linker comparison

Both application maps were compared with the complete official
[`examples/rp235x/memory.x` at `45262e30`](https://github.com/embassy-rs/embassy/blob/45262e30e5d6d659b7aaeb3703b45112d9217a90/examples/rp235x/memory.x),
including all five commits in that path's history. Its SHA-256 is
`ff3f6f642c3ead3e63ba775e0daa214893b9e6102f9136644132f76e59caf696`.
The patches and actual consumers were inspected, rather than inferring changes
from commit titles. All missing applicable corrections for the supported
flash/XIP images are included.

| Official change | Actual content and disposition |
| --- | --- |
| [`1948c414`](https://github.com/embassy-rs/embassy/commit/1948c4146aacd005459989ea4d2d24f8a5e32188) | Derive `.text` alignment from the input section. Applied to both active maps. |
| [`25bae18e`](https://github.com/embassy-rs/embassy/commit/25bae18ea622593b5dca15e8fb96d5e185eb8685) | Rename SRAM4/SRAM5 to SRAM8/SRAM9. Despite its address-fix title, the numeric addresses and lengths do not change. Applied to both active maps and the copied inactive RISC-V script. |
| [`6dae3753`](https://github.com/embassy-rs/embassy/commit/6dae375333aa086cf0aa1ce350d4d04fc2df19d7) | Add an optional whole-program RAM example selected by `__run_from_ram`, with a separate data-section override, boot-ROM load metadata, sealing and relocated UF2 packaging. This is a different execution mode, not a missing XIP correction. No current route selects it. |
| [`a5169226`](https://github.com/embassy-rs/embassy/commit/a516922622c24cad2ce9bb8c0bb7389d29870774) | Move the RAM-example explanation into an opening comment; no semantic change. |
| [`39328359`](https://github.com/embassy-rs/embassy/commit/3932835998802fc3abf7cce4f736e072858ebfd1) | Rename the example directory from `rp23` to `rp235x`; the map contents are identical. |

This accounts for every difference from the current upstream map. It does not
claim that all earlier history under renamed paths was researched. The generic
example's unreserved flash sizes must not replace the product's reservations.

### Section-derived alignment and SRAM names

The dashboard and probe now use the expression from Embassy commit
[`1948c4146aacd005459989ea4d2d24f8a5e32188`](https://github.com/embassy-rs/embassy/commit/1948c4146aacd005459989ea4d2d24f8a5e32188):

```ld
_stext = ALIGN(ADDR(.start_block) + SIZEOF(.start_block), ALIGNOF(.text));
```

This replaces the local fixed 16-byte alignment with the alignment required by
the actual `.text` input. The official commit changes the same expression in
the RP235x application, bootloader and ordinary example. The SRAM naming
correction is adopted as well; the product-specific sections remain.

| Local map | Preserved flash contract |
| --- | --- |
| [`pico2/memory.x`](../../firmware/pico2/memory.x) | 2040 KiB image region from `0x10000000`; 8 KiB settings at `[0x101FE000, 0x10200000)`. |
| [`ble-probe/memory.x`](../../firmware/ble-probe/memory.x) | 1944 KiB image region from `0x10000000`; 96 KiB captures at `[0x101E6000, 0x101FE000)`; the same 8 KiB settings reservation. |

Both preserve the conservative 2 MiB address contract on the selected Pico 2 W,
512 KiB main SRAM, heap anchors, a 32 KiB minimum core0 stack-space linker
assertion, boot metadata, configuration/assets and informational stack-size
sections. The practical image gate additionally requires 64 KiB of core0
headroom. The boot block remains in the first 4 KiB. No memory budget or
reservation is relaxed.

SRAM8/SRAM9 describe the unchanged 4 KiB banks at `0x20080000` and `0x20081000`.
No production section is placed there. Core0's top remains `0x20080000`, and
each core1 stack is a real 32 KiB static allocation within main SRAM. The
diagnostic screens continue to report the 512 KiB main-RAM budget, not the
additional unused banks. The RISC-V copy is not selected by the supported
Cortex-M33 build and this cleanup does not add RISC-V support.

The review traced both build scripts and Cargo routes, the resolved
`cortex-m-rt` 0.7.7 reset/linker implementation, HAL boot metadata and multicore
startup, runtime stack allocations, diagnostics and image gates. They use
actual addresses, `__sheap`, `_stack_start` or the stack object's pointer;
none uses the renamed scratch-bank identifiers. The existing `.data.ram_func`
flash-maintenance code is copied by ordinary `.data` startup and does not
require the optional whole-program RAM layout.

[`test_linker_layout.py`](../../firmware/scripts/test_linker_layout.py) compiles
ARM objects with 16-, 32- and 256-byte `.text` alignment and links each against
both actual maps. Four higher-alignment cases failed before the change; all six
cases pass afterward, under normal and optimized Python. Each now also fills
both scratch banks and checks their origins/sizes. Four additional negative
cases require the linker to reject a one-byte overflow of either bank in either
map. The test checks load-segment bounds, flash reservations and RAM/stack
anchors. These synthetic ELFs verify linker behavior; final application
ELF/UF2 admission remains a separate gate.

## Standalone HAL lock alignment

`firmware/Cargo.lock` remains the production resolution. The HAL's independent
`Cargo.lock` was stale relative to its existing PIO 0.4 manifest patch. It has now
been resolved from the firmware lock so the permanent native regression suite
can run from the actual vendor manifest with `--locked`.

At this checkpoint, the HAL lock contains **169** package identities, all present
in the firmware lock's **233**. Identity means `(name, version, source)`; all
shared registry checksums match too. No additional package/version was needed
for the native test features. Feature selections and graph edges are specific
to each workspace; package identity equality is not feature-graph equality.

| Package | Old standalone HAL lock | Aligned standalone/firmware version |
| --- | --- | --- |
| `pio`, `pio-core`, `pio-parser`, `pio-proc` | 0.3.0 | 0.4.0 |
| `fixed` | 1.30.0 | 1.29.0 |
| `az` | 1.3.0 | 1.2.1 |
| `embassy-time-queue-utils` | 0.3.0 | 0.3.2 |
| `embassy-usb-driver` | 0.2.0 | 0.2.2 |
| `cortex-m` | 0.7.7 | 0.7.9 |
| `cortex-m-rt` | 0.7.5 | 0.7.7 |
| `rp-pac` | 7.0.0 | 7.0.0 |
| `embassy-time` | 0.5.1 | 0.5.1 |
| `embedded-io` / `embedded-io-async` current family | 0.7.1 / 0.7.0 | 0.7.1 / 0.7.0 |

The 0.6.1 embedded-I/O family remains where dependencies require it. The
`fixed`/`az` versions follow the existing embedded-graphics compatibility
constraint in the [18 October inventory](constraints-2026-10-18.md).
The standalone delta removes 64 and adds 62 package identities; version
replacements contribute to both counts. This is not 62 new production
dependencies, and the production firmware lock was unchanged by this alignment.
The complete resolved list is the checked-in HAL lock, rather than another
independently maintained version table.

SHA-256 at the verified checkpoint:

| File | SHA-256 |
| --- | --- |
| `firmware/Cargo.lock` | `a95034fb759969c0110e8b8603e7d76223ca14e1bfada6e118801036109fa90c` |
| `firmware/vendor/embassy-rp/Cargo.lock` | `3d3504fed07591a0bff5db763d5f91b911422c411d2558b1af8dfe5758539d80` |

Both HAL manifests add `embassy-time` 0.5.1 dev features `mock-driver` and
`generic-queue-8` only under `cfg(not(target_os = "none"))`. Production ARM
features and dependencies are unchanged by this test support.

## Reproduction and limits

From the repository root, using its pinned nightly and a populated Cargo cache:

```powershell
cargo test --offline --locked --manifest-path firmware/vendor/embassy-rp/Cargo.toml --target x86_64-pc-windows-msvc --no-default-features --features rp235xa,_test,critical-section/std --lib -- --test-threads=1
cargo test --offline --locked --release --manifest-path firmware/vendor/embassy-rp/Cargo.toml --target x86_64-pc-windows-msvc --no-default-features --features rp235xa,_test,critical-section/std --lib pio_programs::uart::tests -- --test-threads=1
python -m unittest discover -s firmware/scripts -p test_linker_layout.py
python -O -m unittest discover -s firmware/scripts -p test_linker_layout.py
```

The actual HAL manifest passed **54/54** native library tests. The optimized
PIO UART selection passed **7/7**; its original fractional-baud and flush tests
failed before correction, and the release test also reproduced the wrapped-baud
admission. Nine existing native-only compiler warnings remained in the HAL run.
The linker test passed six valid map/alignment combinations and four expected
bank-overflow rejections in both Python modes.
An isolated ARM check of the UART change also passed for
`thumbv8m.main-none-eabihf` with `-C target-cpu=cortex-m33`.

The Windows MMIO tests use allocated register storage and explicit state
transitions; the UART tests poll real HAL futures with a mock time driver. They
do not emulate write-one-to-clear hardware, PIO execution, interrupt arbitration
or physical bus timing. Pure arithmetic tests and source review complement these
models. A native pass cannot qualify stop-bit duration on a pin, DMA/XIP races,
radio interoperability, voltage margins, display scanout or real stack high-water.
Real application builds, feature combinations and linked-image checks belong to
the integration matrix. No device was accessed or flashed for this checkpoint.

## Hardware and implementation references

- [RP2350 datasheet](https://datasheets.raspberrypi.com/rp2350/rp2350-datasheet.pdf): clock, PIO, QMI, GPIO, PWM, power-manager and boot-ROM contracts.
- [Raspberry Pi PIO register definitions](https://github.com/raspberrypi/pico-sdk/blob/master/src/rp2350/hardware_regs/include/hardware/regs/pio.h): divider encoding, FIFO/shift state and debug flags.
- [Raspberry Pi clock register definitions](https://github.com/raspberrypi/pico-sdk/blob/master/src/rp2350/hardware_regs/include/hardware/regs/clocks.h): RP235x fractional divider widths.
- [Raspberry Pi QMI register definitions](https://github.com/raspberrypi/pico-sdk/blob/master/src/rp2350/hardware_regs/include/hardware/regs/qmi.h): timing fields and select-duration semantics.
- [Raspberry Pi POWMAN register definitions](https://github.com/raspberrypi/pico-sdk/blob/master/src/rp2350/hardware_regs/include/hardware/regs/powman.h): password-protected writes and interrupt fields.
- [Raspberry Pi boot-ROM declarations](https://github.com/raspberrypi/pico-sdk/blob/master/src/rp2_common/pico_bootrom/include/pico/bootrom.h): RP235x ROM function interfaces.
- Locked `rp-pac` 7.0.0 and the local HAL sources were inspected for the actual field/API behavior. Upstream references explain hardware intent; they do not substitute for the code selected by the lockfile.
