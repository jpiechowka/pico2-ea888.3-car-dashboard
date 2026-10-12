# Power-user configuration roadmap

The feature decisions below were approved on 12 October 2026, including the later Build and Flash addition. Work is staged so configuration, firmware, the browser preview and the simulator share the same contracts. A stage is complete only after its tests and review; this document does not claim that planned features already work.

## Approved scope and order

| Stage | Deliverable | Acceptance boundary |
| --- | --- | --- |
| 1 | Per-screen tap/hold mappings, encoder assignments, protected recovery | Shared bounded gesture/action model; onboard routes to all screens and views; default tap behavior preserved; Y held for 2 seconds always returns to Dashboard at Full brightness |
| 2 | RGB normal/warning policies | Independent RGB/brightness, Off/Solid/Blink/Breathe, bounded timing, warning scope and screen-off policy; default completed-warning red blink and steady green preserved |
| 3 | Per-widget units and readouts | Temperature C/F; boost follows button or fixed bar/PSI/kPa with optional secondary unit; lambda/AFR primary/both with explicit fuel basis; canonical warnings/ranges |
| 4 | Display smoothing and reduced motion | Elapsed-time filter, off by default, bounded response, reset on missing/stale data; raw samples still drive warnings and extrema; reduced motion disables transitions |
| 5 | Shared history duration | One bounded duration per channel, fixed memory, presets 10 s/30 s/2 min/5 min; honest timestamps and missing-data gaps |
| 6 | Peak, reference and operating bands | Off by default; bounded count, canonical values, manual reference clearly identified, style/size capability rules |
| 7 | Advanced warnings | Window rules, priorities, optional latch/snooze/ack; active acknowledged status remains visible, new/higher priority interrupts snooze; reachable onboard acknowledgment required |
| 8 | Organized editor and power tools | Views, Device, Data & warnings, Diagnostics; grouped searchable settings, linked errors, appearance copy/presets, keyboard command palette; repairable invalid preview and undo/redo retained |
| 9 | Diagnostics and all-screen simulator | Bounded filtered console/event history/export; the same gesture/control logic on Dashboard/Debug/Logs; buttons/encoder/LED, missing/stale data and failures; no default per-frame console spam |
| 10 | JSON editor, diff and resource inspector | Explicit Apply, bounded parser, linked validation errors; rejected changes automatically marked INVALID PREVIEW; accepted serialization remains blocked until repaired; source/decoder and measured resource information |
| 11 | Build and Flash workspace | Local Rust helper, prerequisite setup/checks, actual toolchain and argument display, shared profile/custom-clock validation, BOOTSEL device selection and verified flashing; existing 280/70/1.30 V retained in Expert mode, new candidates also exposed through Cargo |

Stages may share a schema release where required by integration, but each commit must build and have meaningful focused tests. Historical JSON/binary import uses explicit frozen wire models, not structs that silently grow with new fields. Firmware rejects unsupported versions and records its fallback fault. No new ECU signal is admitted without the existing evidence/qualification gates.

Stage 1 is implemented and reviewed through `4a2ca28`, with all 14 CI jobs passing for that commit. The editor uses the shared control catalogue and gesture/session simulator; invalid mapping and numeric edits remain repairable with atomic undo/redo. Full pixel simulation of Debug and Logs belongs to stage 9. The remaining stages are planned work.

Before growing the configuration, implement and measure the [static configuration storage plan](../superpowers/plans/2026-10-12-config-storage.md). Then recover unnecessary render-cache storage and establish persistent simulation ownership. Stages 2 through 7 share one complete schema release after runtime integration and resource admission. The [workspace contract](workspace-and-diagnostics-design.md) covers stages 8 through 10 and their connection to Build & Flash.

## Architecture and resource rules

- `dashboard-config` owns compact configuration types, defaults, compatibility and validators; `dashboard-core` owns pure control/time/value/renderer behavior. Firmware owns GPIO/PWM, DMA and inter-core adapters. Rust/Leptos owns browser editing, accessibility, storage and console adapters.
- The display remains 320 x 240, with a 26-pixel header and 4 x 2 cells. Supported spans remain 1 x 1, 2 x 1 and 2 x 2. No feature may rely on pixels outside its clipped rectangle.
- Keep fixed bounds: eight views, 64 widgets, 16 KiB binary configuration, 128 KiB JSON. The v3 ARM configuration is 9,672 bytes under a 10,240-byte object guard. Any increase requires actual target object/ELF and boot-stack measurements; do not merely relax the assertion.
- History is shared per channel. Render-time animation must use monotonic elapsed time and bounded work, never frame count, catch-up loops proportional to a stall, or fabricated ECU samples.
- Inputs and outputs have one owner. Frame tokens remain the authority for presented overlays and deferred backlight shutdown; queued drawing is not proof of presentation.
- Keep logs bounded, avoid nested locks, preserve latest-fault reporting, and make truncation/drops visible. No default per-frame firmware or browser logging.
- Defaults preserve existing appearance and controls. Synthetic engine thresholds remain DemoOnly. E5 is not an exact ethanol measurement. A configured reference is not an ECU-requested target.

## Research and verification

Hardware references: [PIM715 schematic](https://cdn.shopify.com/s/files/1/0174/1800/files/pico_display_2_8_schematic.pdf?v=1728314048), [RP2350 datasheet](https://datasheets.raspberrypi.com/rp2350/rp2350-datasheet.pdf), [Adafruit encoder guide](https://learn.adafruit.com/adafruit-i2c-qt-rotary-encoder/python-circuitpython), and the exact locked `firmware/vendor/embassy-rp` source. Automotive workflow inspiration: [Haltech dash navigation](https://support.haltech.com/portal/en/kb/articles/navigating-dashes-in-nsp). Browser interaction: [WAI tabs](https://www.w3.org/WAI/ARIA/apg/patterns/tabs/) and [keyboard guidance](https://www.w3.org/WAI/ARIA/apg/practices/keyboard-interface/).

Each stage tests positive paths, boundary rejection and recovery, timestamp discontinuities, stale/missing inputs and default migration. Integration covers accepted/rejected draft -> preview -> export -> embedded startup -> runtime -> completed display/LED state. Benchmark bounded hot paths and record regressions without presenting host time as Pico FPS. Existing host performance qualification remains open in `docs/performance/gauges-and-colours-2026-10-12.md`.

The user reports that 280 MHz CPU / 70 MHz SPI at 1.30 V works on this device; 1.30 V exceeds the RP2350 datasheet's 1.21 V absolute maximum and is not a safety qualification. The user explicitly requested that it remain available in Expert mode. See the [flashing design](flashing-design.md) for divisor rounding, new candidate profiles, host prerequisites and artifact/device boundaries. Physical scanout/FPS, electrical/optical timings, all-path stack high-water, long-duration operation and DNUC ECU support require hardware validation. The new feature authorizes implementing flashing controls; it does not request flashing the connected device during development.
