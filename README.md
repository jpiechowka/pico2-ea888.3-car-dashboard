# Pico2 EA888.3 Car Dashboard

A custom car dashboard project built on the Raspberry Pi Pico 2 (RP2350) for EA888.3 engines. This embedded system provides real-time vehicle data visualization and monitoring capabilities.

## Directory Structure

```
.
├── firmware/           # Rust-based firmware workspace
│   ├── common/         # Shared no_std library (platform-agnostic)
│   ├── simulator/      # Windows simulator binary (uses SDL2)
│   └── pico/           # RP2350 Embassy firmware
├── hardware/           # Hardware schematics and PCB designs
├── mechanical/         # CAD files and mechanical designs
└── docs/               # Project documentation
```

## Firmware Setup

### Prerequisites

- Rust toolchain (nightly recommended)
- For simulator: SDL2 development libraries
- For Pico: ARM Cortex-M target

### Simulator (Windows)

1. Install SDL2 development libraries:
   - Download `SDL2-devel-x.x.x-VC.zip` from https://github.com/libsdl-org/SDL/releases
   - Extract and copy `SDL2.lib` to your Rust toolchain lib folder, or set the `SDL2` environment variable pointing to the extracted folder

2. Build and run:
   ```
   cd firmware
   cargo build -p dashboard-simulator --release
   cargo run -p dashboard-simulator --release
   ```

### Pico 2 (RP2350)

1. Add the ARM target:
   ```
   rustup target add thumbv8m.main-none-eabihf
   ```

2. Build:
   ```
   cd firmware
   cargo build -p dashboard-pico --release
   ```

3. Flash using probe-rs:
   ```
   cargo run -p dashboard-pico --release
   ```

## License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.