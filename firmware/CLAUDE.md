# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Summary

Embedded Rust firmware for a Raspberry Pi Pico 2 (RP2350) car dashboard targeting a 2019 Seat Leon Cupra with EA888.3 engine. Drives a 320×240 ST7789 LCD (Pimoroni PIM715) displaying real-time sensor data (boost, AFR, battery, coolant, oil, DSG, IAT, EGT) in a 4×2 grid layout. Uses Embassy async runtime, dual-core rendering with DMA double-buffering, and `no_std`.

## Build Commands

All commands run from the `firmware/` directory:

```bash
cargo pico2                  # Build release (150 MHz stock)
cargo pico2-run              # Build & flash (hold BOOTSEL + plug USB first)
cargo pico2-fast             # Build with simple-outline (faster FPS)
cargo pico2-fast-run         # Build & flash with simple-outline

# Overclock profiles (all include simple-outline):
cargo pico2-250-62-1v10      # 250 MHz @ 1.10V
cargo pico2-300-75-1v30      # 300 MHz @ 1.30V (max)
# Add -run suffix to flash: cargo pico2-300-75-1v30-run
```

## Quality Checks (CI runs all four)

```bash
cargo fmt --all --check
cargo test -p dashboard-pico2 --lib --target x86_64-pc-windows-msvc   # Windows
cargo test -p dashboard-pico2 --lib --target x86_64-unknown-linux-gnu  # Linux/macOS
cargo clippy -p dashboard-pico2 --target thumbv8m.main-none-eabihf -- -D warnings
cargo build -p dashboard-pico2 --target thumbv8m.main-none-eabihf --release
```

Run a single test with output:
```bash
cargo test -p dashboard-pico2 --lib --target x86_64-pc-windows-msvc -- test_name --nocapture
```

## Architecture

### Dual-Core Rendering Pipeline

- **Core 0** renders frames to buffer A/B alternately (~2ms per frame)
- **Core 1** flushes the completed buffer to display via DMA (~23ms at 37.5 MHz SPI)
- `embassy_sync::Signal` coordinates buffer handoff; atomic counters track profiling stats
- Two static 153KB framebuffers (307KB of 512KB RAM)

### Library vs Binary Split

- `lib.rs` — testable pure-logic modules (`config`, `render`, `profiling`, `state`, `ui`). Uses `#![cfg_attr(not(test), no_std)]` so tests run on host with `std`.
- `main.rs` — embedded entry point with hardware init, overclock setup, Embassy tasks, and the render loop. Not testable on host.

ARM-specific functions (inline asm, hardware registers) return placeholder values (0) during host tests.

### Module Responsibilities

| Module | Purpose |
|--------|---------|
| `config/` | Layout constants (320×240 grid math), sensor thresholds with compile-time validation |
| `drivers/` | Custom async ST7789 driver with DMA, SPI config for PIM715 |
| `tasks/` | Embassy tasks: display flush (Core 1), demo sensor value generation |
| `profiling/` | DWT cycle counter, stack/RAM usage, dual-output log macros (`defmt` + buffer) |
| `state/` | Sensor history/trends/peaks, page navigation, button debounce, popup lifecycle, input dispatch |
| `ui/` | Rgb565 color constants, pre-computed text styles, time-based color animations |
| `screens/` | Boot sequence (loading → welcome), debug/profiling page, log viewer |
| `widgets/` | Sensor cell renderers (boost/temp/battery/afr), header bar, popup overlays, drawing primitives |

### Feature Flags

- `simple-outline` — 3-draw text outline (fast) vs 9-draw (full quality, default)
- `cpu250-spi62-1v10` through `cpu300-spi75-1v30` — overclock profiles setting CPU/SPI/voltage at compile time

## Key Constraints

- **Read-only gauge display** — never implement anything that modifies vehicle behavior
- **No heap in hot paths** — use `heapless::String` + `core::fmt::Write`
- **320×240 must not clip** — verify worst-case values and long strings fit the grid
- **Popup lifecycle** — show → expire → cleanup must not leave rendering artifacts
- **State resets** must clear all derived state (min/max/avg/history/peaks)
- **Conditional redraws** — header only redraws on FPS/page change; dividers redraw after popup close

## Code Style

- `max_width = 120` in rustfmt.toml; `fn_params_layout = "Vertical"`; `group_imports = "StdExternalCrate"`
- No `Co-Authored-By` tags in commit messages
- Prefer compile-time constants for layout and styles
- Widgets are generic over `DrawTarget<Color = Rgb565>`
