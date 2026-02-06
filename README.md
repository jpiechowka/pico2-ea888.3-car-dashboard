# Pico2 EA888.3 Car Dashboard

[![CI](https://github.com/jpiechowka/pico2-ea888.3-car-dashboard/actions/workflows/ci.yml/badge.svg)](https://github.com/jpiechowka/pico2-ea888.3-car-dashboard/actions/workflows/ci.yml)

A custom car dashboard project built on the Raspberry Pi Pico 2 (RP2350) for EA888.3 engines. This embedded system provides real-time vehicle data visualization and monitoring capabilities.

## Directory Structure

```text
.
├── firmware/           # Rust-based firmware workspace
│   ├── pico2/          # RP2350 Embassy firmware (drives ST7789 display)
│   │   └── src/
│   │       ├── main.rs         # Entry point, main loop
│   │       ├── lib.rs          # Library root (testable modules)
│   │       ├── config/         # Configuration (layout, sensor thresholds)
│   │       ├── drivers/        # Hardware drivers (ST7789, SPI config)
│   │       ├── tasks/          # Async tasks (flush, demo)
│   │       ├── profiling/      # Performance utilities (CPU cycles, memory, logging)
│   │       ├── state/          # Application state (sensor state, pages, button, popup, input)
│   │       ├── ui/             # UI styling (colors, styles, animations)
│   │       ├── widgets/        # UI widgets
│   │       │   ├── cells/      # Sensor cell renderers (boost, temp, battery, afr)
│   │       │   ├── header.rs   # Header bar
│   │       │   └── popups.rs   # Popup overlays
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

### Boot Sequence

On startup, the firmware displays two boot screens:

1. **Loading Screen** (~6 seconds) - Console-style initialization messages displayed sequentially
2. **Welcome Screen** (7 seconds) - AEZAKMI logo with animated blinking stars (4s filling + 3s blinking)

After the boot sequence (~13 seconds total), the main dashboard is displayed.

### Button Controls

| Button | Action |
|--------|--------|
| **X** | Cycle FPS display: Off → Instant → Average → Combined → Off |
| **Y** | Cycle pages: Dashboard → Debug → Logs → Dashboard |
| **A** | Toggle boost unit: BAR ↔ PSI |
| **B** | Reset min/max/avg statistics |

### FPS Display Modes

- **Off**: No FPS displayed in header
- **Instant**: Shows current FPS (e.g., "50 FPS")
- **Average**: Shows average FPS since last page switch (e.g., "48 AVG")
- **Combined**: Shows both instant and average (e.g., "50/48 AVG")

### Testing

The firmware is structured as a library + binary crate to enable host-based testing. Tests run on your development machine (not on the embedded target).

```bash
cd firmware

# Run all tests on host (Linux/macOS)
cargo test -p dashboard-pico2 --lib --target x86_64-unknown-linux-gnu

# Run all tests on host (Windows)
cargo test -p dashboard-pico2 --lib --target x86_64-pc-windows-msvc

# Run tests with output
cargo test -p dashboard-pico2 --lib --target x86_64-unknown-linux-gnu -- --nocapture
```

**Note:** Tests use `#![cfg_attr(not(test), no_std)]` to enable `std` during testing while remaining `no_std` for the embedded binary.

### Config File Inheritance

The `rustfmt.toml` and `rust-toolchain.toml` files are inherited in subdirectories,
so `cargo fmt` and `cargo clippy` work from any subdirectory.

## License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.
