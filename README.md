# Pico2 EA888.3 Car Dashboard

[![CI](https://github.com/jpiechowka/pico2-ea888.3-car-dashboard/actions/workflows/ci.yml/badge.svg)](https://github.com/jpiechowka/pico2-ea888.3-car-dashboard/actions/workflows/ci.yml)

A custom car dashboard project built on the Raspberry Pi Pico 2 (RP2350) for EA888.3 engines. This embedded system provides real-time vehicle data visualization and monitoring capabilities.

## Directory Structure

```text
.
├── firmware/           # Rust-based firmware workspace
│   ├── pico2/          # RP2350 Embassy firmware (drives ST7789 display)
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

# Build with overclocking (250 MHz)
cargo pico2-oc

# Build & flash with overclocking
cargo pico2-oc-run

# Build with simple-outline + overclocking (balanced)
cargo pico2-fast-oc

# Build & flash with simple-outline + overclocking
cargo pico2-fast-oc-run

# Build with turbo overclocking (375 MHz)
cargo pico2-turbo

# Build & flash with turbo overclocking
cargo pico2-turbo-run

# Build with simple-outline + turbo (maximum performance)
cargo pico2-fast-turbo

# Build & flash with simple-outline + turbo
cargo pico2-fast-turbo-run
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

### Config File Inheritance

The `rustfmt.toml` and `rust-toolchain.toml` files are inherited in subdirectories,
so `cargo fmt` and `cargo clippy` work from any subdirectory.

## License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.
