# Gauges and colours

## Using the controls

Start `./configurator/Studio.ps1 -Serve`, open `http://127.0.0.1:8080`, select a
widget and choose **Gauge style**. Set **Scale minimum/maximum** in canonical
units and optionally enable **Expand with observed values**. Expansion never
shrinks the configured range; turning it off clamps the indicator again. The
numeric reading and warning thresholds remain independent of that scale.

| Presentation | Sizes | History / trend | Average / extrema |
| --- | --- | --- | --- |
| Numeric & history | 1×1, 2×1, 2×2 | Optional | Optional |
| Circular arc, needle dial, horizontal, vertical, segmented bar | 1×1, 2×1, 2×2 | Replaced by gauge | Optional at 2×2 |

Every style retains fitted values, labels and unavailable-state feedback.
Lambda offers lambda, AFR estimate or both with declared fuel assumptions;
boost offers Follow dashboard bar/PSI or fixed bar, PSI and kPa. Only Follow
dashboard responds to the physical/preview toggle. Equal primary/secondary units
are omitted. Compact gauges retain secondary choices for wider sizes.
Hidden numeric-view preferences remain stored when changing gauge styles.
Round gauges use a bounded line lookup; bars use fixed geometry. None adds a
frame-count-driven animation or a second data generator.

Open **Widget colours** for the separate text, graph, scale and warning roles.
Colour pickers and hex fields use the same validator. Hex values show the
actual RGB565 representation after quantization. In **Warning display**, set
the separate combined-warning pair for list mode; cycle mode uses the active
warning widget's pair. A background change can invalidate several existing
colours, so all stored roles remain editable even if the current style hides them.

Rejected edits appear automatically in the clearly marked **INVALID PREVIEW**.
Invalid colours remain visible for diagnosis with safe geometry; invalid rules
or geometry never become the accepted device configuration. Save/export remain
blocked until repaired or discarded. Use the stress-value selector to inspect
channel endpoints, extreme digit widths and unavailable AFR. Export JSON for
editing and `.eav` for a firmware build. Historical v1/v2/v3/v4 files migrate to the
current v5 schema. Native landscape/portrait grids and per-screen overrides are
described in [screen orientation](orientation.md). See
[advanced controls](advanced-controls.md) for smoothing, history, markers,
bands and warning acknowledgment.

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
  160×107 and 160×214 widget rectangles in the landscape 4×2 layout. Native portrait uses the [2×4 layout](orientation.md).
- [Pico 2 W specifications](https://www.raspberrypi.com/products/raspberry-pi-pico-2-w/)
  and the [board datasheet](https://datasheets.raspberrypi.com/picow/pico-2-w-datasheet.pdf)
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

## Resource audit and stack correction

The linked ARM review found a real acquisition-stack defect that a static RAM
total could not detect: the initial Vehicle acquisition poll alone reserved
38,576 bytes against a 32,768-byte core-1 stack. An older pre-configurator
Longan image also exceeded that stack on its snapshot call chain, so this is
not attributed solely to the new gauges. Merely outlining constructors made
some local frames smaller while making their cumulative call chains worse.
Those experiments were rejected.

The correction constructs the configured acquisition owner directly, instead
of constructing a complete legacy owner and replacing its large alarm state.
A plain-owner firmware helper avoids a large temporary Result in the resident
acquisition poll. Const sensor/histogram templates preserve their initial state.
A thin boot task parses the configuration, queues the runtime dashboard task,
then returns. This separates parser and runtime stack lifetimes. Awaiting the
runtime body instead would retain the parent frame and defeat that boundary.

The locked Embassy executor 0.10.0 Spawner/raw executor source was inspected:
spawn queues a task without eagerly polling it. Cortex-M runtime 0.7.7 enables
the FPU before the entry point, supporting bounded floating-point config
validation before HAL initialization. Parsing now precedes watchdog startup;
it reads only bounded immutable embedded bytes, without peripheral or time
access. Peripheral setup, UART construction on core 1, immutable configuration
publication, framebuffer ownership and DMA lifetimes are preserved.
The exact locked 0.7.7 source and linked Reset instructions both set CPACR and
execute DSB/ISB before calling main. An earlier review named 0.7.5 incorrectly;
the checked version is 0.7.7 and the initialization conclusion is unchanged.

Target Rust [stack metadata](https://doc.rust-lang.org/beta/unstable-book/compiler-flags/emit-stack-sizes.html)
and the [LLVM format](https://llvm.org/docs/CodeGenerator.html#stack-size-section)
were checked against linked instructions. `.stack_sizes` is retained as INFO,
outside every LOAD segment; it adds no runtime RAM or loaded code/data. The
mandatory memory-layout gate rejects missing/malformed metadata, includes
outlined task bodies behind small poll shims, and checks cumulative named
paths with 8 KiB core-0 / 4 KiB core-1 reserves. Ordinary child allowances are
1 KiB for construction/boot helpers, 8 KiB for acquisition runtime and 16 KiB
for dashboard runtime. Source or dependency changes require reviewing these
allowances against disassembly; do not increase budgets just to pass.

### Final frozen builds, nightly 2026-10-11

Both use the 280/70 profile and frame/TE traces. The Demo capture imports the
466-byte v1 default and embeds its canonical 912-byte v2 upgrade. The Vehicle
capture embeds an 8-view, 64-widget, 8,430-byte maximum fixture with long text,
high IDs, all styles and three-byte colour varints. It remains an unqualified
Vehicle transport. Each archive contains 219 unchanged build-source inputs.

| Metric (bytes) | Demo, migrated default | Vehicle, maximum |
| --- | ---: | ---: |
| Static main RAM | 445,656 | 455,640 |
| Core-0 stack headroom | 78,632 | 68,648 |
| Admitted boot path, including child allowance | 50,160 | 50,344 |
| Admitted core-1 construction path | 7,616 | 25,456 |
| Admitted core-1 runtime path | 14,784 | 13,592 |
| Admitted dashboard path | 33,424 | 34,376 |

Independent instruction review measured the normal Vehicle construction chain
at 24,648 bytes, leaving 8,120 bytes on core 1 before exception/indirect-path
qualification. Normal boot success/fallback measured 49,516/49,700 bytes for
Demo/Vehicle respectively. These are reviewed linked call paths, not physical
stack high-water measurements or whole-program proofs. IRQ/exception overlap,
panic paths and indirect waker/format calls still need device qualification.

| Frozen capture | ELF SHA-256 |
| --- | --- |
| gauges-optimized-demo-v1-verified | `5404feeb2c154617f9b86d5446f613321058aa92c4c7bc156c3e0b1106593d56` |
| gauges-optimized-vehicle-maximum-verified | `7f3a6509d5198c66465461503ab1092d7a7f7eb9d0b7323635436c3a3879e2b2` |

These supersede the historical `gauges-final-*` captures after the two minimal
cache/background-fill optimizations. Static objects, core-1 paths and boot
reserves are unchanged; the Vehicle dashboard frame is eight bytes smaller.
All 21 critical metadata frames were compared against final linked instructions.
Timing captures and the limits of performance qualification are recorded in
[the performance report](../performance/gauges-and-colours-2026-10-12.md).

## End-to-end verification

Independent source reviews covered schema migration, gauge bounds, producer
statistics, retained buffer keys, warning ownership/palettes, browser draft
transactions, resource construction and exact final ELF call frames. No source
or linked-resource blocker remained after the corrections above.

- Configuration tests cover old JSON/EAV migration, malformed/truncated files,
  finite limits and maximum documents. Colour tests check every RGB565 value
  against three reference partners (196,608 pairs) with an independent floating
  luminance oracle, plus each palette-role contract.
- Twelve gauge pixel tests cover all eight channels, three spans, five added
  styles, in/out-of-range values, stale/nonfinite/unavailable states, PSI/AFR,
  retained A/B buffers, palette changes and real Vehicle ingestion/error/reset.
  Public draw geometry is bounded before iteration, then clipped to ownership.
- Warning tests cover actual rule identity, off-view cycle colours, list colours
  with one or multiple warnings, blink and overlay removal on both buffers.
- The 80 editor tests cover raw colour/range edits, invalid projections, repair, stale
  callbacks, undo/redo, saved state and serialization. Native and Wasm strict
  lints, production assets, both host outline suites (337 runtime tests plus two
  compile-fail contracts per outline) and ARM strict lints pass. The shared
  workspace's JSON-enabled run passes 48 tests/contracts; the separate device
  configuration run without JSON passes 25.
- Real browser checks exercised compact/wide/large gauges, extreme digits,
  colour pickers and quantization, invalid contrast/range repair, warning modes,
  save/reload, JSON/EAV imports, and a 390×700 viewport without horizontal
  overflow. Browser export and the independent host compiler produced identical
  1,222-byte files for an 11-widget test document. No browser errors were logged.
- Six stack-gate regressions include malformed metadata, original overflow,
  misleading reduced local frames, hidden outlined bodies and reserve edges;
  they pass normally and under Python `-O`. Final ELF flash-lockout corruption
  tests also pass in both modes. Hardware was not flashed during this work.
