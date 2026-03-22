# Pico2 EA888.3 Car Dashboard

A custom car dashboard project built on the Raspberry Pi Pico 2 (RP2350) for EA888.3 engines. This embedded system provides real-time vehicle data visualization and monitoring capabilities.

## Directory Structure

```text
.
├── firmware/           # Rust-based firmware workspace
│   ├── pico2/          # RP2350 Embassy firmware (drives ST7789 display)
│   │   └── src/
│   │       ├── main.rs         # Entry point, dual-core setup, main render loop
│   │       ├── lib.rs          # Library root
│   │       ├── config/         # Configuration (layout, sensor thresholds)
│   │       ├── drivers/        # Hardware drivers (ST7789, SPI, double-buffer)
│   │       ├── peripherals/    # External peripherals (I2C rotary encoder via Seesaw)
│   │       ├── tasks/          # Async tasks (flush on Core 0, demo sensors on Core 1)
│   │       ├── profiling/      # Performance utilities (CPU cycles, memory, log buffer)
│   │       ├── state/          # Application state (sensor state, pages, button, popup, input)
│   │       ├── ui/             # UI styling (colors, styles, animations)
│   │       ├── widgets/        # UI widgets
│   │       │   ├── cells/      # Sensor cell renderers (boost, temp, battery, afr)
│   │       │   ├── header.rs   # Header bar
│   │       │   └── popups.rs   # Popup overlays (FPS, reset, boost unit, brightness)
│   │       └── screens/        # Screen renderers (boot, loading, welcome, profiling, logs)
│   └── tools/          # Bundled tools like picotool
├── hardware/           # Hardware schematics and PCB designs
├── mechanical/         # CAD files and mechanical designs
└── docs/               # Project documentation
```

## Firmware Setup

### Prerequisites

- Rust nightly toolchain (auto-configured via `rust-toolchain.toml`)
- ARM Cortex-M target for Pico 2

### Quick Start

All commands run from the `firmware/` directory:

```bash
cd firmware

# Build & flash Pico 2 (hold BOOTSEL, plug USB)
cargo pico2-run

# Build only (no flash)
cargo pico2

# Build with simple-outline optimization
cargo pico2-fast

# Build & flash with simple-outline optimization
cargo pico2-fast-run

# === Overclock profiles (all include simple-outline) ===
# Format: cargo pico2-{cpu_mhz}-{spi_mhz}-{voltage}

# 250 MHz @ 1.10V (62.5 MHz SPI - ST7789 datasheet max)
cargo pico2-250-62-1v10
cargo pico2-250-62-1v10-run

# 280 MHz @ 1.30V (70 MHz SPI - beyond ST7789 datasheet)
cargo pico2-280-70-1v30
cargo pico2-280-70-1v30-run

# 290 MHz @ 1.30V (72.5 MHz SPI)
cargo pico2-290-72-1v30
cargo pico2-290-72-1v30-run

# 300 MHz @ 1.30V (75 MHz SPI - embassy-rp max)
cargo pico2-300-75-1v30
cargo pico2-300-75-1v30-run
```

### Pico 2 (RP2350)

1. Add the ARM target:

   ```bash
   rustup target add thumbv8m.main-none-eabihf
   ```

2. Flash (hold BOOTSEL, plug USB, then run):

   ```bash
   cargo pico2-run
   ```

**Note:** `picotool` is bundled in `firmware/tools/` and used automatically by `cargo pico2-run`.

**Display:** The firmware drives the Pimoroni PIM715 Display Pack 2.8" (ST7789, 320×240) via SPI.

**Encoder:** Adafruit I2C STEMMA QT Rotary Encoder (product 5880) connected via the PIM715's STEMMA QT port (I2C0: GP4/SDA, GP5/SCL at 400 kHz). Uses the Adafruit Seesaw protocol.

### Architecture

The firmware uses both Cortex-M33 cores on the RP2350:

- **Core 0:** Runs the Embassy async executor with the main render loop, display flush task (DMA SPI), and encoder polling task (I2C). Handles all UI rendering, button/encoder input, and PWM backlight control.
- **Core 1:** Runs a dedicated Embassy executor with the demo sensor data task. Publishes sensor values to Core 0 via an Embassy Watch channel. Reports its own CPU utilization and stack high-water mark via atomics.

Cross-core communication uses `AtomicU32`/`AtomicBool` for simple values and `embassy_sync::Watch` for structured data. The log buffer uses a `Mutex<CriticalSectionRawMutex>` (backed by RP2350 hardware spinlocks) for safe cross-core writes.

Double-buffered rendering (2 × 150 KB framebuffers) allows Core 0 to render the next frame while the previous frame is being flushed over SPI DMA.

### Boot Sequence

On startup, the firmware displays two boot screens:

1. **Loading Screen** (~6 seconds) - Console-style initialization messages displayed sequentially
2. **Welcome Screen** (7 seconds) - AEZAKMI logo with animated blinking stars (4s filling + 3s blinking)

After the boot sequence (~13 seconds total), the main dashboard is displayed.

### Controls

#### PIM715 Buttons

| Button | Action |
|--------|--------|
| **X** | Cycle FPS display: Off → Instant → Average → Combined → Off (Dashboard only) |
| **Y** | Cycle pages: Dashboard → Debug → Logs → Dashboard |
| **A** | Toggle boost unit: BAR ↔ PSI (Dashboard only) |
| **B** | Reset min/max/avg statistics (Dashboard only) |

#### Rotary Encoder (Adafruit 5880)

| Input | Dashboard / Debug | Logs |
|-------|-------------------|------|
| **Rotate CW** | Decrease brightness (-5%, min 0% = off) | Scroll up (older) |
| **Rotate CCW** | Increase brightness (+5%) | Scroll down (newer) |
| **Press** | Toggle backlight on/off | No action |

Brightness defaults to 100% on boot. Rotating down to 0% turns the backlight off. When toggling the backlight off via button press, the "BL: OFF" popup is displayed for 1.5 seconds before the backlight is actually turned off, so the user can see the confirmation. Brightness is controlled via PWM on GP20 (slice 2, channel A), with the 0-100% user range remapped to the LED's visible duty cycle range.

> **PWM slice note:** GP20 maps to PWM slice 2, channel A via the hardware's `pin/2 % 8` GPIO-to-PWM wiring (same on RP2040 and RP2350). The RP2350 adds PWM slices 8–11 for GPIOs 30+; embassy-rp 0.10.0 gates these behind the `_rp235x` feature flag.

### FPS Display Modes

- **Off**: No FPS displayed in header
- **Instant**: Shows current FPS (e.g., "50 FPS")
- **Average**: Shows average FPS since last page switch (e.g., "48 AVG")
- **Combined**: Shows both instant and average (e.g., "50/48 AVG")

### On-Screen Log Viewer

The Logs page (accessible via Y button) displays the last 128 log entries in a scrollable view. All firmware events (boot, task spawning, encoder init, periodic profiling stats, page changes, etc.) are logged via the `log_info!()` macro to a circular buffer. Use the rotary encoder to scroll through entries. A scroll indicator (e.g., "5-17/42") appears in the header when the buffer is scrollable. When scrolled, the view is anchored so new log entries don't shift the visible content.

### Config File Inheritance

The `rustfmt.toml` and `rust-toolchain.toml` files are inherited in subdirectories,
so `cargo fmt` and `cargo clippy` work from any subdirectory.

## License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.
