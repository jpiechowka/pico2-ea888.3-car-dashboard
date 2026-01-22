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
| `common` | Platform-agnostic `no_std` code: colors, config, styles, thresholds, animations. No time dependencies. Uses `libm` for math. |
| `simulator` | Windows simulator using `embedded-graphics-simulator` + SDL2. Uses `std::time` for timing. Contains `Popup`, `SensorState`, widgets, screens. |
| `pico` | RP2350 firmware using Embassy async runtime. Uses `embassy_time` for timing. |

### Key Design Decisions

- **Time abstraction:** Time-dependent code stays in platform-specific crates (`std::time::Instant` in simulator, `embassy_time` in pico)
- **no_std compatibility:** Common crate uses `libm` for math functions (`sinf`, `cosf`)
- **Widgets/screens:** Currently simulator-specific (use `SimulatorDisplay` directly); can be made generic with traits when pico needs them

### Build Commands

```
# Simulator (requires SDL2)
cargo build -p dashboard-simulator --release
cargo run -p dashboard-simulator --release

# Pico (requires thumbv8m.main-none-eabihf target)
cargo build -p dashboard-pico --release
cargo run -p dashboard-pico --release  # flash via probe-rs
```

### Dependencies Setup

**Simulator (Windows via Scoop):**
```
scoop bucket add extras
scoop install sdl2
```
Then set the `SDL2` environment variable to the Scoop install path (e.g., `%USERPROFILE%\scoop\apps\sdl2\current`).

**Simulator (Windows manual):**
- Download `SDL2-devel-x.x.x-VC.zip` from https://github.com/libsdl-org/SDL/releases
- Copy `SDL2.lib` to Rust toolchain lib folder, or set `SDL2` environment variable

**Pico:**
```
rustup target add thumbv8m.main-none-eabihf
```
