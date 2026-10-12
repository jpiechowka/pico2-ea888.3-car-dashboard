# Gauges and colours

## Implementation plan, 2026-10-12

Goal: six bounded gauge styles and independent widget colours, using the same
Rust renderer and configuration in View Studio and Pico Demo/Vehicle modes.
Automatic range expansion was confirmed by the user: configured bounds are
seeds, accepted observed extrema can expand them, and statistics reset starts
again. Firmware reads existing producer extrema without new cross-core state.
The browser retains accepted run extrema across its bounded history replays,
resetting on a rewind, source change or synthetic value-mode change.

### Design and source checks

- The [PIM715 specification](https://shop.pimoroni.com/products/pico-display-pack-2-8)
  identifies a 320×240 ST7789V IPS panel. The 26-pixel header leaves 80×107,
  160×107 and 160×214 widget rectangles in the existing 4×2 layout.
- [Pico 2 specifications](https://www.raspberrypi.com/products/raspberry-pi-pico-2/)
  and the [board datasheet](https://datasheets.raspberrypi.com/pico/pico-2-datasheet.pdf)
  establish 520 KiB physical SRAM. Our linker admission policy uses the main
  512 KiB region and reserves the other banks separately. Measure linked RAM
  and stack effects; a file-size cap alone is not a RAM proof.
- [LVGL scale](https://lvgl.io/docs/open/widgets/scale) and
  [bar](https://lvgl.io/docs/open/widgets/bar) documentation demonstrate linear,
  circular, needle and segmented indicators. These are design references;
  implementation remains embedded-graphics 0.8.2 with no additional runtime.
- Read the locked embedded-graphics `arc/styled.rs`, `line/styled.rs` and
  `draw_target/clipped.rs`: arcs scan the enclosing circle, and clipping limits
  output but does not bound the original iterator's work. Use bounded geometry,
  fixed segment counts and integer line drawing for round scales. Do not feed
  unchecked sizes or floating values into drawing iterators.

### Styles and layout

Numeric/history remains the default. Add circular arc (270 degrees), needle
dial (180 degrees), horizontal bar, vertical bar and segmented bar. Every style
retains a fitted numeric value, label, units/AFR and unavailable indication.
Gauge styles replace history graphs; average, extrema and trend preferences
remain stored but are displayed only where the shared capability policy allows
them. Compact gauges prioritize the measurement and scale; wide/tall gauges
use the extra space. All output is clipped to its owned rectangle, including
invalid-draft diagnostic projections. Fixed geometry has no FPS-driven motion.

Ranges are finite canonical units, separated by at least 0.001 in stored f32
precision, and within channel edit domains. Defaults are visual scales, never suggested vehicle alarm thresholds.
Fixed ranges clamp only the indicator, never the numeric measurement or alarms.
Expanded ranges read accepted producer extrema; stale/invalid samples show no
indicator. Reset, source/mode changes, frozen samples, rewinds and bar/PSI are
explicit verification cases. The retained key includes indicator position and
effective range, including changes smaller than numeric rounding.

Colours are stored as RGB565 with JSON hex presentation. Separate roles cover
background, border, label, value, secondary text, graph, gauge, track, ticks,
average, minimum, maximum, trend, peak highlight and warning foreground/background.
Combined/list warnings have a separate foreground/background pair; cycle-one
warnings use their widget pair. The blink phase exchanges the two warning
colours. Validate all stored text/indicator colours on the quantized panel values at a
conservative 3:1 ratio; indicator versus track and each warning pair are checked
separately. Border and track are decorative. All palette controls remain editable,
including currently inactive roles, so changing a style cannot trap a palette.
The bounded Q16 sRGB tables follow the
[W3C luminance formula](https://www.w3.org/WAI/WCAG22/Understanding/contrast-minimum.html).
This is a product visibility guard, not a WCAG conformance claim. Warning widgets
switch directly between readable palette endpoints, replacing the firmware-only
red fade; popup blinking exchanges its foreground and background. No per-frame
colour conversion or extra cross-core ownership is introduced. Invalid edits
remain explicitly marked, inspectable, and blocked from save/export.

### Reviewable stages

1. Capture `^views/` Criterion baseline and existing correctness tests. Add
   configuration v2 and explicit v1 JSON/binary host migration. Firmware rejects
   unsupported old binary versions with its existing fault/default path; the
   build tool upgrades old input before embedding. Preserve saved browser views.
   Keep 16 KiB binary and 128 KiB JSON limits. Prove maximum document and target
   RAM/stack; avoid silently increasing resource budgets.
2. RED/GREEN range, geometry, pixel and retained-buffer tests; implement shared
   gauges and editor controls. Test all channels, spans, min/mid/max/outside,
   nonfinite/stale/unavailable, duplicate-channel independent ranges, reset,
   irregular time and unit toggles. Audit this stage before colour rendering.
3. RED/GREEN colour parsing, contrast, warning page and retained-cache tests;
   implement widget palettes and independent combined warnings. Add Rust colour
   controls and safe diagnostic fallbacks. Audit end-to-end again.
4. Real browser workflow, malformed edits/repair, import/export/reload/undo,
   animation stress and screenshots. Both outline host suites and strict lints;
   Wasm production build; Demo and Vehicle target builds plus ELF RAM gates.
   Repeat numeric benchmarks and add each gauge full/retained workload, record
   actual regressions and gains. Commit often, push master, check exact CI.

Physical readability, panel scanout FPS, device stack high-water, overclock
stability and live DNUC ECU qualification require hardware validation. The
user's working profile remains 280/70; this work does not request a flash.
