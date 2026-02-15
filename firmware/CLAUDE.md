# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Summary

Embedded Rust firmware for a Raspberry Pi Pico 2 (RP2350) car dashboard targeting a 2019 Seat Leon Cupra (5F) with EA888.3 2.0 TSI "290" engine (DNUC) and DQ381 DSG 7-speed transmission. Drives a 320×240 ST7789 LCD (Pimoroni PIM715) displaying real-time sensor data in a 4×2 grid layout. Uses Embassy async runtime, dual-core rendering with DMA double-buffering, and `no_std`.

**This is a read-only gauge display — never implement anything that modifies vehicle behavior.**

## Sensors and Thresholds

| Sensor | Description |
|--------|-------------|
| Boost | Turbocharger boost pressure (BAR/PSI toggle) |
| AFR / Lambda | Air-fuel ratio / Lambda sensor |
| Battery | Voltage |
| Coolant | Coolant temperature |
| Oil | Oil temperature |
| DSG | Transmission temperature |
| IAT | Intake Air Temperature |
| EGT | Exhaust Gas Temperature |

**EGT color thresholds:**
- < 300°C — BLUE (cold/warming)
- 600–850°C — YELLOW (spirited driving)
- 850–950°C — ORANGE (high load)
- >= 950°C — RED (blink + shake)
- >= 1050°C — "Danger To Manifold" easter egg popup (blinking red/white)

Sensor thresholds are defined in `config/sensors.rs` with compile-time validation. OBDEleven is used as ground truth for validating units/scaling and signal availability.

## Build Commands

All commands run from the `firmware/` directory:

```bash
cargo pico2                  # Build release (150 MHz stock)
cargo pico2-run              # Build & flash (hold BOOTSEL + plug USB first)
cargo pico2-fast             # Build with simple-outline (faster FPS)
cargo pico2-fast-run         # Build & flash with simple-outline

# Overclock profiles (all include simple-outline):
# Format: cargo pico2-{cpu_mhz}-{spi_mhz}-{voltage}
cargo pico2-250-62-1v10      # 250 MHz @ 1.10V (62.5 MHz SPI)
cargo pico2-280-70-1v30      # 280 MHz @ 1.30V (70 MHz SPI)
cargo pico2-290-72-1v30      # 290 MHz @ 1.30V (72.5 MHz SPI)
cargo pico2-300-75-1v30      # 300 MHz @ 1.30V (75 MHz SPI, embassy-rp max)
# Add -run suffix to any alias to flash: cargo pico2-300-75-1v30-run

# Explicit build command with features:
cargo build -p dashboard-pico2 --target thumbv8m.main-none-eabihf --release --features simple-outline,cpu280-spi70-1v30
```

**Setup:** `rustup target add thumbv8m.main-none-eabihf` (nightly toolchain auto-configured via `rust-toolchain.toml`)

**Flashing:** Hold BOOTSEL, plug USB, run `cargo pico2-run`. `picotool` is bundled in `firmware/tools/`.

## Quality Checks

CI runs these four checks — run before submitting changes:

```bash
cargo fmt --all --check
cargo test -p dashboard-pico2 --lib --target x86_64-pc-windows-msvc        # Windows
cargo test -p dashboard-pico2 --lib --target x86_64-unknown-linux-gnu       # Linux/macOS
cargo clippy -p dashboard-pico2 --target thumbv8m.main-none-eabihf -- -D warnings
cargo build -p dashboard-pico2 --target thumbv8m.main-none-eabihf --release
```

Run a single test with output:
```bash
cargo test -p dashboard-pico2 --lib --target x86_64-pc-windows-msvc -- test_name --nocapture
```

**Testable modules:** `config::layout`, `config::sensors`, `profiling::cpu_cycles`, `profiling::memory`, `render`, `state::pages`, `state::sensor_state`, `ui::colors`

ARM-only functions (inline asm, hardware registers like `MemoryStats::collect()`, `cpu_cycles::read()`) return placeholder values (0) during host tests — test the logic, not the hardware.

## Architecture

### Dual-Core Rendering Pipeline

```text
Core 0 (Main Task):         Core 1 (Flush Task):
┌─────────────┐             ┌─────────────┐
│ Render to A │────signal──→│ Flush A     │ (23ms via DMA)
│ Swap to B   │             │             │
│ Render to B │────signal──→│ Flush B     │ (23ms via DMA)
│ Swap to A   │             │             │
└─────────────┘             └─────────────┘
    ~2ms each                 Runs in parallel
```

- `embassy_sync::Signal` coordinates buffer handoff; atomic counters track profiling stats
- Two static 153KB Rgb565 framebuffers (307KB total)

### Memory Layout (RP2350, 512KB RAM)

| Component | Size | Notes |
|-----------|------|-------|
| Framebuffer A | 153KB | Static, double buffer |
| Framebuffer B | 153KB | Static, double buffer |
| Other statics | ~32KB | Profiling, state, etc. |
| Stack | ~172KB | Remaining |

Memory stats are collected via the SP register (inline assembly) and displayed on the Debug page.

### Library vs Binary Split

- `lib.rs` — testable pure-logic modules. Uses `#![cfg_attr(not(test), no_std)]` so tests run on host with `std`.
- `main.rs` — embedded entry point with hardware init, overclock setup, Embassy tasks, and the render loop. Not testable on host.

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

**`simple-outline`:**

| Mode | Draw Calls | Description |
|------|-----------|-------------|
| Default (`cargo pico2`) | 9 per value | Full 8-direction outline for maximum visibility |
| `simple-outline` (`cargo pico2-fast`) | 3 per value | 2-direction shadow (bottom-right) for better FPS |

**Overclock features** (SPI clock = CPU clock / 4):

| Feature | CPU | Voltage | SPI | Description |
|---------|-----|---------|-----|-------------|
| Default | 150 MHz | 1.10V | 37.5 MHz | Stock RP2350 |
| `cpu250-spi62-1v10` | 250 MHz | 1.10V | 62.5 MHz | ST7789 datasheet max SPI |
| `cpu280-spi70-1v30` | 280 MHz | 1.30V | 70 MHz | Beyond ST7789 datasheet |
| `cpu290-spi72-1v30` | 290 MHz | 1.30V | 72.5 MHz | Higher overclock |
| `cpu300-spi75-1v30` | 300 MHz | 1.30V | 75 MHz | Embassy-rp max supported |

### Performance Optimizations

- **32-bit word writes:** Framebuffer fill writes 2 pixels at a time
- **Async DMA transfers:** Full-screen SPI transfers don't block CPU
- **Pre-configured window:** CASET/RASET set once during init, skipped per flush
- **Conditional header redraw:** Only on FPS change or page switch
- **Divider draw-once:** Redrawn only after popup closes
- **Graph line reduction:** Mini graphs draw every other point (`step_by(2)`)
- **Time-based animations:** Color transitions use wall-clock time, FPS-independent
- **Fixed-point color lerp:** Integer interpolation for smooth color changes

## Change Discipline

When making any change:

1. **Read the full context** (modules + docs + comments) before editing
2. **Update all impacted code paths** and state transitions:
   - Dashboard ↔ debug page switching
   - Popup lifecycle (show → expire → cleanup)
   - Unit toggles (BAR ↔ PSI) and formatting edge cases
   - Reset behavior (min/max/avg/history/peak highlight)
3. **Audit and update comments/docs** when behavior changes
4. **Run the quality checks** listed above and fix issues
5. **Verify UI layout** still fits 320×240:
   - Check worst-case values and long strings
   - Confirm popups don't overlap in unintended ways

### Invariants

- 320×240 layout must not clip (labels/values/graphs/popups)
- UI must remain readable at a glance: consistent contrast, no flicker near thresholds
- Popups must cleanly draw/clear without leaving artifacts
- State resets must clear _all_ derived state (min/max/avg/history/peaks)

### Embedded Performance

- No heap allocation in hot paths — use `heapless::String` + `core::fmt::Write`
- Compile-time constants for layout and styles
- Keep render work predictable — no hidden expensive operations per frame
- Preserve existing optimization intent (conditional redraws, fixed-point lerp, etc.)
- Widgets are generic over `DrawTarget<Color = Rgb565>`
- `micromath` for fast trig approximations (max error 0.002 for sin/cos)

## Code Style

- `max_width = 120`; `fn_params_layout = "Vertical"`; `group_imports = "StdExternalCrate"` (see `rustfmt.toml`)
- Edition 2024, nightly toolchain
- Concise commit messages — no `Co-Authored-By` tags

## Repository Structure

- `firmware/` — Rust workspace containing the `dashboard-pico2` crate
- `hardware/bom/` — Component lists; `hardware/gerber/` — PCB files; `hardware/schematics/` — Circuit diagrams
- `mechanical/stl/` — 3D print files; `mechanical/cad/` — Source CAD models
- `docs/` — Build instructions, pinouts, assembly guides

## Hardware Reference

**PIM715 Display Pack 2.8" Pinout:**
- Buttons: GPIO 12 (A), 13 (B), 14 (X), 15 (Y) — active-low with pull-up
- RGB LED: GPIO 26 (R), 27 (G), 28 (B) — active-low
- Display (ST7789 via SPI0): CS=17, DC=16, CLK=18, MOSI=19, Backlight=20
- Native 240×320 portrait, rotated 90° for 320×240 landscape
- Reset tied to RUN pin (resets with Pico, no GPIO needed)

**Pico 2 WH note:** Onboard LED is on the CYW43 WiFi chip, not a GPIO — use PIM715 RGB LED instead.
