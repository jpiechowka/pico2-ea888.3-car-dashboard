# Seat Leon Gauge Cluster — Raspberry Pi Pico 2 WH (RP2350 ARM Cortex M33 + CYW4343) + Pimoroni PIM715 Pico Display Pack 2.8

---

## Overview

This is a Rust project that builds a compact, high-contrast, glanceable digital gauge cluster for a car. It targets a Raspberry Pi Pico 2 (RP2350) driving a 320×240 ST7789-based LCD (Pimoroni Pico Display Pack 2.8"), rendering a 4×2 sensor grid plus overlays (popups) and a debug/profiling page.

---

## Important Context

### Target Platform

**MCU/Board:** Raspberry Pi Pico 2 (RP2350)
- Dual-core MCU; firmware is expected to run on the Cortex-M33 build target (e.g. `thumbv8m.main-none-eabihf`) once the project is moved to `no_std`.

**Display:** Pico Display Pack 2.8" (ST7789 family)
- Resolution: 320×240
- Format: `Rgb565` (native for ST7789)
- Input: 4 physical buttons mapped in the simulator as `X` / `Y` / `A` / `B`
- Extras: onboard RGB LED + Qwiic/STEMMA QT connector (I²C expansion)

**HAL + async model:**
- Target a modern RP2350 HAL (choose the canonical crate at implementation time; consult upstream docs)
- Use Embassy async to separate concerns:
  - OBD/UDS polling task(s)
  - UI render task
  - Button/input task
  - Logging/telemetry task (optional)

### Vehicle Context

**Car (built and tested on):** 2019 Seat Leon Cupra (5F)

**Engine:** EA888.3 2.0 TSI "290" (engine code DNUC)

**Transmission:** DSG 7-speed, DQ381 family

**Dashboard sensors** tuned for this platform:

| Sensor | Description |
|--------|-------------|
| Boost | Turbocharger boost pressure |
| AFR / Lambda | Air-fuel ratio / Lambda sensor |
| Battery | Voltage |
| Coolant | Coolant temperature |
| Oil | Oil temperature |
| DSG | Transmission temperature |
| IAT | Intake Air Temperature |
| EGT | Exhaust Gas Temperature |

**EGT Thresholds:**
- Cold/warming: < 300°C (BLUE)
- Spirited driving: 600-850°C (YELLOW)
- High load: 850-950°C (ORANGE)
- Critical: >= 950°C (RED, blink + shake)
- "Danger To Manifold": >= 1050°C (Fast & Furious easter egg popup with blinking red/white warning)

---

## OBDEleven

I have OBDEleven and will use it as the practical "ground truth" tool to:

- Confirm which signals are available on the ECU/TCU and under what conditions
- Validate units/scaling (e.g., PSI vs bar, °C ranges, lambda/AFR conversion)
- Cross-check gauge readings during bring-up to quickly catch conversion or threshold mistakes

---

## Electronics and Hardware Planned

_Some specifics will be finalized once the on-car data path is chosen._

### Core UI Hardware

- Raspberry Pi Pico 2 (RP2350)
- Pimoroni Pico Display Pack 2.8" (ST7789, 320×240, buttons, RGB LED)

### Vehicle Data Interface (OBD-II / UDS)

- OBD-II port connection and a physical layer suitable for your car (typically CAN on modern VAG platforms)
- A safe, robust interface design (transceiver, ESD protection, proper grounding, etc.)

### Power

Automotive 12V is noisy → plan for clean power regulation and protection:
- Fuse, reverse polarity protection, transient suppression/TVS, filtering
- Stable 5V/3.3V rails for the Pico + display

### Mechanical / Enclosure

- Vibration-safe mounting, cable strain relief
- Enclosure that prevents glare and minimizes driver distraction

### Expansion Options

- Qwiic/STEMMA QT (I²C) expansion for future sensors or peripherals

---

## AI Agent Notes and "How to Help Effectively"

These items maximize what an agentic workflow (or AI assistant) can do safely and repeatably.

### High-Leverage Artifacts to Maintain

- Spec doc _(recommended)_: a short `docs/spec.md` for layout + state machine rules
- Performance budget _(recommended)_: what is allowed in the render loop (no alloc, fixed buffers, etc.)

### Invariants the Agent Must Preserve

- 320×240 layout must not clip (labels/values/graphs/popups)
- UI must remain readable at a glance:
  - Consistent contrast rules (luminance-based text color selection)
  - Avoid flicker/instability when values hover near thresholds
- Popups must cleanly draw/clear without leaving artifacts
- State resets must reset _all_ derived state (min/max/avg/history/peaks)

### Safety & Scope Boundaries

> **Safety:** This is a read-only gauge display project; do not implement anything that modifies vehicle behavior.

---

## Code Style and Quality Gates

### Rust / Dependencies

- Prefer the newest stable Rust where possible; this repo currently uses `edition = "2024"` and a nightly toolchain.
- Keep dependencies current; consult upstream docs when behavior/APIs are uncertain.

### Change Discipline (Required)

When making any change:

1. **Read the full context** (modules + docs + comments) before editing.

2. **Update all impacted code paths** and state transitions:
   - Dashboard ↔ debug page switching
   - Popup lifecycle (show → expire → cleanup)
   - Unit toggles (bar ↔ PSI) and formatting edge cases
   - Reset behavior (min/max/avg/history/peak highlight)

3. **Audit and update comments/docs** when behavior changes.

4. **Run the full quality loop** and fix issues:
   - `cargo fmt --all`
   - `cargo clippy --all-targets --all-features` (fix or justify)
   - `cargo test`

5. **Verify UI layout** still fits 320×240:
   - Check worst-case values and long strings
   - Confirm popups don't overlap in unintended ways
   - Ensure changes make sense visually at the simulator's scale

### Embedded Performance Posture

- Avoid heap allocation in hot paths (use `heapless::String` + `core::fmt::Write`)
- Prefer compile-time constants for layout and styles
- Keep render work predictable (no hidden expensive operations per frame)
- Preserve the existing optimization intent (conditional header/divider redraw, fixed-point color lerp, etc.)

---

## Git and Version Control Preferences

- **Commit messages:** Keep concise and descriptive
- **Co-authorship:** Do not add `Co-Authored-By` tags in commit messages

---

## Directory Structure

- `firmware/` - Rust workspace (see Firmware Workspace section below)
- `hardware/bom/` - Component lists for ordering parts
- `hardware/gerber/` - PCB manufacturing files
- `hardware/schematics/` - Circuit diagrams and design docs
- `mechanical/stl/` - Final STL files ready for 3D printing
- `mechanical/cad/` - Source 3D model files
- `docs/` - Build instructions, pinouts, assembly guides, etc.

---

## Firmware Workspace

The firmware is a Cargo workspace with three crates:

```
firmware/
├── Cargo.toml          # Workspace root
├── common/             # dashboard-common: shared no_std library
├── simulator/          # dashboard-simulator: Windows simulator binary
└── pico/               # dashboard-pico: RP2350 Embassy firmware
```

### Crate Responsibilities

| Crate | Purpose |
|-------|---------|
| `common` | Platform-agnostic `no_std` code: colors, config, styles, thresholds, animations, render state, `SensorState`, and generic `widgets` module. No time dependencies. Uses `micromath` for fast trig approximations. |
| `simulator` | Windows simulator using `embedded-graphics-simulator` + SDL2. Uses `std::time` for timing. Contains `Popup`, std-enhanced `SensorState` (accurate timing), screens. Re-exports widgets from common. |
| `pico` | RP2350 firmware using Embassy async runtime. Uses `embassy_time` for timing. Drives PIM715 ST7789 display via custom async ST7789 driver. Uses widgets and `SensorState` from common. Contains `screens/` module with boot screens (loading, welcome) and profiling page. |

### Key Design Decisions

- **Time abstraction:** Time-dependent code stays in platform-specific crates (`std::time::Instant` in simulator, `embassy_time` in pico)
- **no_std compatibility:** Common crate uses `micromath` for fast trig approximations (max error 0.002 for sin/cos)
- **Widgets:** Generic over `DrawTarget<Color = Rgb565>` in `common/widgets/`. Both simulator and pico use the same rendering code.
- **SensorState:** Two implementations: no_std version in `common/sensor_state.rs` (frame-based timing) and std version in `simulator/state.rs` (accurate timing). Both produce `SensorDisplayData` for widgets.
- **Display driver:** Pico uses custom async ST7789 driver with full-screen framebuffer (153KB) and DMA transfers

### Performance Optimizations (Pico)

The `dashboard-common` crate has a `simple-outline` feature flag for embedded performance:

| Mode                                 | Draw Calls  | Description                                      |
|--------------------------------------|-------------|--------------------------------------------------|
| Default (`cargo pico`)               | 9 per value | Full 8-direction outline for maximum visibility  |
| `simple-outline` (`cargo pico-fast`) | 3 per value | 2-direction shadow (bottom-right) for better FPS |

Use `cargo pico-fast` or `cargo pico-fast-run` to enable the `simple-outline` feature for improved frame rates on embedded targets.

**Overclocking:**

The `dashboard-pico` crate has an `overclock` feature that doubles the CPU clock from 150 MHz to 300 MHz:

| Mode                    | CPU Clock | Description                                    |
|-------------------------|-----------|------------------------------------------------|
| Default                 | 150 MHz   | Stock RP2350 frequency                         |
| `overclock` (`-oc`)     | 300 MHz   | 2x overclock at default 1.1V (safe, no cooling)|

Combine both features with `cargo pico-fast-oc` for maximum performance.

Additional optimizations in Pico firmware:

- **32-bit word writes:** Framebuffer fill operations use 32-bit writes (2 pixels at a time)
- **Async DMA transfers:** Full-screen SPI transfers use DMA without blocking CPU
- **SPI at max speed:** 62.5 MHz SPI clock (ST7789 maximum)
- **Pre-configured window:** Display window is set once during init(), skipping redundant CASET/RASET commands per flush

### Build Commands

All commands run from the `firmware/` directory:

```bash
# Using cargo aliases (recommended)
cargo sim            # Build & run simulator
cargo sim-fast       # Build & run simulator with simple-outline
cargo pico           # Build pico firmware
cargo pico-run       # Build & flash pico firmware
cargo pico-fast      # Build pico with simple-outline optimization
cargo pico-fast-run  # Build & flash pico with simple-outline
cargo pico-oc        # Build pico with 300 MHz overclock
cargo pico-oc-run    # Build & flash pico with 300 MHz overclock
cargo pico-fast-oc   # Build pico with simple-outline + overclock
cargo pico-fast-oc-run # Build & flash with simple-outline + overclock

# Explicit commands
cargo build -p dashboard-simulator --release
cargo run -p dashboard-simulator --release
cargo build -p dashboard-pico --target thumbv8m.main-none-eabihf --release
cargo run -p dashboard-pico --target thumbv8m.main-none-eabihf --release
# With simple-outline optimization:
cargo build -p dashboard-pico --target thumbv8m.main-none-eabihf --release --features dashboard-common/simple-outline
```

The `rustfmt.toml` and `rust-toolchain.toml` files are inherited in subdirectories,
so `cargo fmt` and `cargo clippy` work from any subdirectory.

### Dependencies Setup

**Simulator (Windows):**

SDL2 is bundled in `vendor/sdl2/`. The build script (`simulator/build.rs`) automatically:
- Links against the bundled `SDL2.lib`
- Copies `SDL2.dll` to the target directory

First-time setup (if `vendor/sdl2/` is empty):
```bash
scoop bucket add extras && scoop install sdl2
cp ~/scoop/apps/sdl2/current/lib/SDL2.{lib,dll} vendor/sdl2/
```

**Pico:**
```bash
rustup target add thumbv8m.main-none-eabihf
```

**Flashing Pico 2:**
1. Hold BOOTSEL button, plug in USB
2. Run: `cargo pico-run`

**Note:** `picotool` is bundled in `firmware/tools/` and used automatically for flashing.

### Hardware Notes

**PIM715 Display Pack 2.8" Pinout:**
- RGB LED: GPIO 26 (Red), GPIO 27 (Green), GPIO 28 (Blue) - active-low
- Buttons: GPIO 12 (A), GPIO 13 (B), GPIO 14 (X), GPIO 15 (Y) - active-low with pull-up
- Display (ST7789 via SPI0):
  - CS: GPIO 17
  - DC: GPIO 16
  - CLK: GPIO 18 (SPI0 SCK)
  - MOSI: GPIO 19 (SPI0 TX)
  - Backlight: GPIO 20
  - Reset: Tied to RUN pin (resets with Pico, no GPIO needed)
- Native panel: 240x320 (portrait), rotated 90° for 320x240 (landscape)

**Pico 2 WH (WiFi version):**
- The onboard LED is connected to the CYW43 WiFi chip, not a GPIO
- Use the PIM715 RGB LED for visual feedback instead
