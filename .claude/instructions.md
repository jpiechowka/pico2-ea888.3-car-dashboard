# Seat Leon Gauge Cluster — Raspberry Pi Pico 2 WH (RP2350 ARM Cortex M33 + CYW4343) + Pimoroni PIM715 Pico Display Pack 2.8

---

## Overview

This is a Rust project that builds a compact, high-contrast, glanceable digital gauge cluster for a car. It targets a Raspberry Pi Pico 2 (RP2350) driving a 320×240 ST7789-based LCD (Pimoroni Pico Display Pack 2.8"), rendering a 4×2 sensor grid plus overlays (popups) and a debug/profiling page.

---

## Important Context

### Target Platform

**MCU/Board:** Raspberry Pi Pico 2 (RP2350)

- Dual-core MCU running no_std firmware on the Cortex-M33 build target (`thumbv8m.main-none-eabihf`).

**Display:** Pico Display Pack 2.8" (ST7789 family)
- Resolution: 320×240
- Format: `Rgb565` (native for ST7789)
- Input: 4 physical buttons mapped as `X` / `Y` / `A` / `B`
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
   - Ensure changes make sense visually at the device's scale

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

- `firmware/` - Rust workspace containing the `dashboard-pico2` crate
- `hardware/bom/` - Component lists for ordering parts
- `hardware/gerber/` - PCB manufacturing files
- `hardware/schematics/` - Circuit diagrams and design docs
- `mechanical/stl/` - Final STL files ready for 3D printing
- `mechanical/cad/` - Source 3D model files
- `docs/` - Build instructions, pinouts, assembly guides, etc.

---

## Firmware

The firmware is a single-crate Cargo workspace:

```
firmware/
├── Cargo.toml          # Workspace root
└── pico2/              # dashboard-pico2: RP2350 Embassy firmware
    └── src/
        ├── main.rs     # Entry point, button handling, main loop
        ├── st7789.rs   # Custom async ST7789 driver with DMA
        ├── display.rs  # Display initialization helpers
        ├── screens/    # Screen renderers (loading, welcome, profiling, logs)
        ├── widgets/    # UI widgets (cells, header, popups, primitives)
        ├── colors.rs   # RGB565 color constants
        ├── config.rs   # Layout and display configuration
        ├── styles.rs   # Pre-computed text styles
        ├── thresholds.rs # Sensor threshold values
        ├── animations.rs # Color transitions
        ├── render.rs   # Cell indices and render state tracking
        ├── sensor_state.rs # Sensor state tracking
        ├── pages.rs    # Page navigation enum (Dashboard, Debug, Logs)
        ├── log_buffer.rs # Log buffer with levels and dual-output macros
        └── memory.rs   # Memory profiling (stack/RAM usage)
```

### Key Design Decisions

- **Embassy async:** Uses `embassy_time` for timing, async DMA for display transfers
- **no_std:** Uses `micromath` for fast trig approximations (max error 0.002 for sin/cos)
- **Widgets:** Generic over `DrawTarget<Color = Rgb565>`, all rendering code in one crate
- **Display driver:** Custom async ST7789 driver with double-buffered framebuffers (307KB total) and DMA transfers

### Dual-Core Architecture

The firmware uses parallel render/flush for maximum performance:

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

- **Double buffering:** Two 153KB framebuffers allow rendering while flushing
- **Signal synchronization:** Uses `embassy_sync::Signal` for buffer handoff
- **Atomic counters:** Track buffer swaps, waits, and timing for profiling

### Memory Layout (RP2350)

| Component | Size | Notes |
|-----------|------|-------|
| RAM Total | 512KB | 0x20000000 - 0x20080000 |
| Framebuffer A | 153KB | Static, double buffer |
| Framebuffer B | 153KB | Static, double buffer |
| Other statics | ~32KB | Estimated overhead |
| Stack | ~172KB | Remaining for stack/heap |

Memory stats are collected via the MSP register and displayed on the Debug page.

### Performance Features

The crate has two feature flags for performance:

**`simple-outline` feature:**

| Mode                                   | Draw Calls  | Description                                      |
|----------------------------------------|-------------|--------------------------------------------------|
| Default (`cargo pico2`)                | 9 per value | Full 8-direction outline for maximum visibility  |
| `simple-outline` (`cargo pico2-fast`)  | 3 per value | 2-direction shadow (bottom-right) for better FPS |

**`overclock` and `turbo-oc` features:**

| Mode                     | CPU Clock | Voltage | SPI Clock | Description                          |
|--------------------------|-----------|---------|-----------|--------------------------------------|
| Default                  | 150 MHz   | 1.10V   | 37.5 MHz  | Stock RP2350 frequency               |
| `overclock` (`-oc`)      | 250 MHz   | 1.10V   | 62.5 MHz  | Optimal SPI (ST7789 max)             |
| `turbo-oc` (`-turbo`)    | 375 MHz   | 1.30V   | 62.5 MHz  | Maximum CPU performance              |

Note: 250 MHz was chosen for `overclock` because it divides evenly to 62.5 MHz SPI (250/4=62.5). The `turbo-oc` feature pushes to 375 MHz at 1.30V for maximum CPU performance (per Pimoroni testing, RP2350 is stable up to 420 MHz @ 1.30V).

**Feature combinations:**

- `cargo pico2-fast-oc` - Simple outlines + 250 MHz (balanced)
- `cargo pico2-fast-turbo` - Simple outlines + 375 MHz (maximum performance)

**Additional optimizations:**

- **32-bit word writes:** Framebuffer fill operations use 32-bit writes (2 pixels at a time)
- **Async DMA transfers:** Full-screen SPI transfers use DMA without blocking CPU
- **SPI at max speed:** 62.5 MHz SPI clock (ST7789 maximum)
- **Pre-configured window:** Display window is set once during init(), skipping redundant CASET/RASET commands per flush

### Build Commands

All commands run from the `firmware/` directory:

```bash
# Using cargo aliases (recommended)
cargo pico2           # Build pico2 firmware
cargo pico2-run       # Build & flash pico2 firmware
cargo pico2-fast      # Build pico2 with simple-outline optimization
cargo pico2-fast-run  # Build & flash pico2 with simple-outline
cargo pico2-oc        # Build pico2 with 250 MHz overclock
cargo pico2-oc-run    # Build & flash pico2 with 250 MHz overclock
cargo pico2-fast-oc   # Build pico2 with simple-outline + overclock
cargo pico2-fast-oc-run # Build & flash with simple-outline + overclock

# Explicit commands
cargo build -p dashboard-pico2 --target thumbv8m.main-none-eabihf --release
cargo run -p dashboard-pico2 --target thumbv8m.main-none-eabihf --release
# With simple-outline optimization:
cargo build -p dashboard-pico2 --target thumbv8m.main-none-eabihf --release --features simple-outline
```

The `rustfmt.toml` and `rust-toolchain.toml` files are inherited in subdirectories,
so `cargo fmt` and `cargo clippy` work from any subdirectory.

### Dependencies Setup

**Pico 2:**
```bash
rustup target add thumbv8m.main-none-eabihf
```

**Flashing Pico 2:**
1. Hold BOOTSEL button, plug in USB
2. Run: `cargo pico2-run`

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
