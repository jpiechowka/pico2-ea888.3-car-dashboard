# Pico2 EA888.3 Car Dashboard

[![CI](https://github.com/jpiechowka/pico2-ea888.3-car-dashboard/actions/workflows/ci.yml/badge.svg)](https://github.com/jpiechowka/pico2-ea888.3-car-dashboard/actions/workflows/ci.yml)

A custom car dashboard project built on the Raspberry Pi Pico 2 (RP2350) for EA888.3 engines. This embedded system provides real-time vehicle data visualization and monitoring capabilities.

## Directory Structure

```
.
├── firmware/           # Rust-based firmware workspace
│   ├── common/         # Shared no_std library (colors, config, widgets, SensorState)
│   ├── simulator/      # Windows simulator binary (uses SDL2)
│   ├── pico/           # RP2350 Embassy firmware (drives ST7789 display)
    ├── tools/          # Bundled tools like picotool
│   └── vendor/sdl2/    # Bundled SDL2 (lib + dll)
├── hardware/           # Hardware schematics and PCB designs
├── mechanical/         # CAD files and mechanical designs
└── docs/               # Project documentation
```

## Architecture

The firmware uses a shared-code architecture:

- **common crate**: Platform-agnostic `no_std` code including generic UI widgets, sensor state tracking, colors, styles, and configuration constants
- **simulator**: Windows simulator using SDL2 for development. Re-exports widgets from common.
- **pico**: RP2350 firmware using Embassy async runtime and mipidsi driver for ST7789 display. Uses the same widgets as the simulator.

## Firmware Setup

### Prerequisites

- Rust nightly toolchain (auto-configured via `rust-toolchain.toml`)
- For Pico: ARM Cortex-M target

### Quick Start

All commands run from the `firmware/` directory:

```bash
cd firmware

# Build & run simulator (Windows)
cargo sim

# Build & flash Pico 2 (hold BOOTSEL, plug USB)
cargo pico-run

# Build only (no flash)
cargo pico
```

### Simulator (Windows)

SDL2 is bundled in `vendor/sdl2/`. The build script automatically:
- Links against the bundled `SDL2.lib`
- Copies `SDL2.dll` to the target directory

Just run:
```bash
cargo sim
```

**First-time setup (if vendor/sdl2 is empty):**
1. Install SDL2 via Scoop: `scoop bucket add extras && scoop install sdl2`
2. Copy files: `cp ~/scoop/apps/sdl2/current/lib/SDL2.{lib,dll} vendor/sdl2/`

### Pico 2 (RP2350)

1. Add the ARM target:
   ```bash
   rustup target add thumbv8m.main-none-eabihf
   ```

2. Flash (hold BOOTSEL, plug USB, then run):
   ```bash
   cargo pico-run
   ```

**Note:** `picotool` is bundled in `firmware/tools/` and used automatically by `cargo pico-run`.

**Display:** The firmware drives the Pimoroni PIM715 Display Pack 2.8" (ST7789, 320×240) via SPI, rendering the same dashboard UI as the simulator.

### Config File Inheritance

The `rustfmt.toml` and `rust-toolchain.toml` files are inherited in subdirectories,
so `cargo fmt` and `cargo clippy` work from any subdirectory.

## License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.