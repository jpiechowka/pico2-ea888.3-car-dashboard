# Seat Leon Gauge Cluster — Raspberry Pi Pico 2 WH (RP2350 ARM Cortex M33 + CYW4343) + Pimoroni PIM715 Pico Display Pack 2.8

## Overview

This is a Rust project that builds a compact, high-contrast, glanceable **digital gauge cluster** for a car.
It targets a **Raspberry Pi Pico 2 (RP2350)** driving a **320×240 ST7789-based LCD** (Pimoroni Pico Display Pack 2.8"),
rendering a **4×2 sensor grid** plus overlays (popups) and a debug/profiling page.

---

## Important context

### Target platform

- **MCU/Board:** Raspberry Pi **Pico 2 (RP2350)**  
  - Dual-core MCU; firmware is expected to run on the **Cortex‑M33** build target (e.g. `thumbv8m.main-none-eabihf`) once the project is moved to `no_std`.

- **Display:** **Pico Display Pack 2.8"** (ST7789 family)  
  - **Resolution:** 320×240  
  - **Format:** `Rgb565` (native for ST7789)  
  - **Input:** 4 physical buttons mapped in the simulator as `X` / `Y` / `A` / `B`  
  - **Extras:** onboard RGB LED + Qwiic/STEMMA QT connector (I²C expansion)

- **HAL + async model:**
  - Target a modern **RP2350 HAL** (choose the canonical crate at implementation time; consult upstream docs).
  - Use **Embassy async** to separate concerns:
    - OBD/UDS polling task(s)
    - UI render task
    - Button/input task
    - Logging/telemetry task (optional)

### Vehicle context (why these gauges)

- **Car (built and tested on):** 2019 **Seat Leon Cupra (5F)**  
- **Engine:** **EA888.3 2.0 TSI “290”** (engine code **DNUC**)  
- **Transmission:** DSG 7-speed, **DQ381** family
- Dashboard is tuned around the sensors that matter most for this platform:
  - **Boost**
  - **AFR / Lambda**
  - **Battery voltage**
  - **Coolant temp**
  - **Oil temp**
  - **DSG temp**
  - **IAT (Intake Air Temp)**
  - **EGT (Exhaust Gas Temp)**

---

## OBDEleven

I have **OBDEleven** and will use it as the practical “ground truth” tool to:

- Confirm which signals are available on the ECU/TCU and under what conditions
- Validate units/scaling (e.g., PSI vs bar, °C ranges, lambda/AFR conversion)
- Cross-check gauge readings during bring-up to quickly catch conversion or threshold mistakes

---

## Electronics and hardware planned

*(Some specifics will be finalized once the on-car data path is chosen.)*

- **Core UI hardware**
  - Raspberry Pi Pico 2 (RP2350)
  - Pimoroni Pico Display Pack 2.8" (ST7789, 320×240, buttons, RGB LED)

- **Vehicle data interface (OBD-II / UDS)**
  - OBD-II port connection and a physical layer suitable for your car (typically CAN on modern VAG platforms).
  - A safe, robust interface design (transceiver, ESD protection, proper grounding, etc.).

- **Power**
  - Automotive 12V is noisy → plan for **clean power regulation** and protection:
    - fuse, reverse polarity protection, transient suppression/TVS, filtering
    - stable 5V/3.3V rails for the Pico + display

- **Mechanical / enclosure**
  - Vibration-safe mounting, cable strain relief
  - Enclosure that prevents glare and minimizes driver distraction

- **Expansion options**
  - Qwiic/STEMMA QT (I²C) expansion for future sensors or peripherals

---

## AI agent notes and “how to help effectively”

These items maximize what an agentic workflow (or AI assistant) can do safely and repeatably:

### High-leverage artifacts to maintain

- **Spec doc** (recommended): a short `docs/spec.md` for layout + state machine rules
- **Performance budget** (recommended): what is allowed in the render loop (no alloc, fixed buffers, etc.)

### Invariants the agent must preserve

- **320×240** layout must not clip (labels/values/graphs/popups)
- UI must remain readable at a glance:
  - consistent contrast rules (luminance-based text color selection)
  - avoid flicker/instability when values hover near thresholds
- Popups must cleanly draw/clear without leaving artifacts
- State resets must reset *all* derived state (min/max/avg/history/peaks)

### Safety & scope boundaries

- This is a **read-only gauge display** project; do not implement anything that modifies vehicle behavior.

---

## Code style and quality gates

### Rust / dependencies

- Prefer the **newest stable Rust** where possible; this repo currently uses **edition 2024** and a **nightly toolchain**.
- Keep dependencies current; **consult upstream docs** when behavior/APIs are uncertain.

### Change discipline (required)

When making any change:

1. **Read the full context** (modules + docs + comments) before editing.
2. Update **all impacted code paths** and **state transitions**:
   - dashboard ↔ debug page switching
   - popup lifecycle (show → expire → cleanup)
   - unit toggles (bar ↔ PSI) and formatting edge cases
   - reset behavior (min/max/avg/history/peak highlight)
3. **Audit and update comments/docs** when behavior changes.
4. Run the full quality loop and fix issues:
   - `cargo fmt --all`
   - `cargo clippy --all-targets --all-features` (fix or justify)
   - `cargo test`
5. Verify UI layout still fits **320×240**:
   - check worst-case values and long strings
   - confirm popups don’t overlap in unintended ways
   - ensure changes make sense visually at the simulator’s scale

### Embedded performance posture

- Avoid heap allocation in hot paths (use `heapless::String` + `core::fmt::Write`)
- Prefer compile-time constants for layout and styles
- Keep render work predictable (no hidden expensive operations per frame)
- Preserve the existing optimization intent (conditional header/divider redraw, fixed-point color lerp, etc.)

---

## Git and version control preferences

- **Commit messages:** Keep concise and descriptive
- **Co-authorship:** Do not add `Co-Authored-By` tags in commit messages

---

## Directory Structure

- `firmware/` - All Rust code for the embedded system
- `hardware/bom/` - Component lists for ordering parts
- `hardware/gerber/` - PCB manufacturing files
- `hardware/schematics/` - Circuit diagrams and design docs
- `mechanical/stl/` - Final STL files ready for 3D printing
- `mechanical/cad/` - Source 3D model files
- `docs/` - Build instructions, pinouts, assembly guides, etc.