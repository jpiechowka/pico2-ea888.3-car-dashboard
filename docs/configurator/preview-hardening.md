# Value sizing and diagnostic previews

## Contract and implementation order

1. Preserve a source-frozen four-workload renderer baseline before changing
   rendering. Add tests that demonstrate larger visible primary numbers on
   2x1 and 2x2 widgets. Keep compact cells readable.
2. Fit the complete primary string, including sign, decimals and unit, in the
   space remaining after label, trend, secondary value, graph and footer rows.
   Integer scaling uses existing font bitmaps without another framebuffer or
   heap allocation on the Pico. A one-pixel outline and clipping are independent
   guards. If text cannot fit, omit it rather than draw outside its allocation.
3. Retain rejected edits separately from the accepted document. Automatically
   show a clearly marked **INVALID PREVIEW** with affected-widget markers and
   the rejected values. Save, export and asynchronous writes remain blocked.
   Corrections or explicit discard retire stale callbacks and restore normal
   rendering. Diagnostic rendering never creates an invalid `ValidatedConfig`.
4. Show renderable rejected labels and placement attempts. Report fields that
   cannot be represented (invalid numbers, unsupported glyphs, off-screen
   placements) explicitly; safe fallback rendering is identified, not presented
   as the rejected configuration's exact firmware output.
5. Exercise elapsed-time animation, extreme values, rounding/size transitions,
   both retained buffers, invalid/repair/discard lifecycles, and the production
   demo generator at the preview boundary. Firmware and browser already share
   `dashboard_core::demo::values`; the raster export example now uses it too.
6. Verify native tests with both outline modes, Rust/Wasm builds, a Pico 2
   280/70 build and memory gate, actual browser interactions, and repeated
   focused benchmarks. Review the complete change before pushing master.

The existing configuration limits and validators stay authoritative. Invalid
documents are inspection-only; they cannot become firmware or saved drafts.
Browser performance is not physical panel FPS. No firmware is flashed as part
of this change.

## Font implementation references

- [MonoFont 0.8.2](https://docs.rs/embedded-graphics/0.8.2/embedded_graphics/mono_font/struct.MonoFont.html)
- [ProFont 0.7.0](https://docs.rs/profont/0.7.0/profont/)
- [MonoTextStyle source](https://docs.rs/embedded-graphics/0.8.2/src/embedded_graphics/mono_font/mono_text_style.rs.html)
- [Pimoroni display dimensions](https://shop.pimoroni.com/products/pico-display-pack-2-0)
- [Pico 2 datasheet](https://datasheets.raspberrypi.com/pico/pico-2-datasheet.pdf)

The locked source was inspected: measurement counts characters and spacing;
alphabetic placement subtracts the font baseline; foreground glyph rendering
can be forwarded to a bounded scaled draw target. ProFont 24 point has a
16x29 cell and 18 point a 12x22 cell. Scaling uses these actual bitmap metrics,
not CSS font sizes or point-size assumptions.

## Diagnostic boundary

The immutable accepted configuration remains the source of exports, persistence,
sensor acquisition and undo history. Raw fields and rejected typed edits have
separate storage. Every invalid edit retires pending field/pointer/save/import
callbacks. No raw or rejected configuration is converted to `ValidatedConfig`.

For inspection, each widget is projected to bounded grid geometry and validated
in isolation. Outside-grid positions clamp to the nearest valid position while
retaining size/style, with explicit fallback descriptions. Decimal counts
above three are capped, invalid fixed graph ranges become Auto, unsupported AFR
is disabled, and warning simulation is suspended. Accepted source/fuel settings
are retained. Raw labels use at most 54 characters (view names 34), substitute
unsupported glyphs, then clip to their own lane. Exact rejected inputs and
global choices are listed in a wrapping, scrollable diagnostic panel. Advanced
readout, smoothing, hold, reference and band bounds/order use named safe
fallbacks. Raw RGB565 widget and band colours cross a separate diagnostic-only
override, so invalid contrast remains inspectable without weakening validation.

Both participants of overlaps are marked. Overlap can obscure content, as on a
screen; the marked placement and attempted size remain inspectable. Out-of-bounds
placements are explicitly listed. The browser banner, frame and widget badges
stay visible while the draft is invalid, and the pixel header contains its own
INVALID PREVIEW watermark. These pixels are not an exact firmware prediction.

## Verification, 12 October 2026

Implementation: `93d194b` (fonts), `f62f52f` (compact rendering optimization),
and `25aa5c5` (diagnostic previews). The web application uses Leptos 0.8.22,
custom HTML/CSS and Rust compiled to Wasm, with framework-generated JavaScript.
The pinned compiler is nightly-2026-10-11, rustc `6f5da406b`.

| Check | Result |
|---|---|
| Host correctness | 326 tests passed in each outline mode, including two ownership compile-fail tests |
| Editor/geometry/preview/timing | 62 tests passed |
| Strict Clippy | Host all targets, editor native and Wasm passed |
| Formatting | Root shared workspace, host and editor passed |
| Production web build | Trunk 0.21.14 release build passed |
| Pico build | Demo 280/70 with frame and TE tracing passed |
| Linked RAM | 441,936 B static; 82,352 B core 0 headroom, unchanged |
| Flash helpers | Reviewed SRAM bytes, startup copy and IRQ25 checks passed; 18 negative cases rejected under normal and optimized Python |

Tests include an independent glyph-pixel expansion oracle, one-pixel outlines,
unscaled/scaled panel edges, extreme coordinates, unsupported strings, complete
formatted widths, 768 widget option/placement combinations, size changes at
rounding thresholds and both retained buffers. Diagnostic tests inspect actual
label and placement pixels, unchanged neighbours, overlapping/off-screen edits,
NaN/oversized/control-character input, global source/fuel changes, atomic repair,
discard and stale callbacks. Timing tests cover irregular/repeated timestamps,
pause/resume, invalid clocks and long elapsed times. None establishes the absence
of every possible bug.

Independent review found missing attempted values/affected-widget markers for
rejected global settings. A failing regression reproduced it before correction.
The final review also checked the compact optimization against the locked
embedded-graphics-core 0.4.1 DrawTarget contract; fitting bounds contain the full
unscaled glyph and outline, while widget and panel clipping remain in place.
Both scaled and unscaled panel-edge cases are exercised.

Final target capture: `firmware/target/bench-captures/preview-hardening-final-verified`.
ELF SHA-256: `2b6f73adbded13f810aa2a5b8810c587cd59041a4e660471589dda610c5eb092`.
The source/config archive and manifest identify the linked inputs. An earlier
attempt failed because the sandbox denied `rust-lld`; that incomplete capture is
retained separately and is not counted as a passing build. No device was flashed,
and no new electrical, ECU, stack high-water or physical FPS result is claimed.

## Focused renderer measurements

Same four unchanged Rust workloads, 30 samples each, Windows 11/Intel Family 6
Model 183, pinned CPU 0, runner `johnny-desktop-cpu0`. Browser animation and other
builds were stopped during measurement. Every capture retains raw samples,
source bytes, hashes, compiler and fixture identity under
`host/baselines/configurator_font_*`.

| Workload | Before, us | Initial A / B, us | Optimized A / B, us | Before to optimized B |
|---|---:|---:|---:|---:|
| Default full | 139.535 | 198.030 / 202.421 | 151.932 / 152.456 | +9.26% |
| Default retained stationary | 4.249 | 3.291 / 3.153 | 3.109 / 3.006 | -29.25% |
| Wide full | 96.610 | 82.950 / 79.996 | 73.629 / 75.778 | -21.56% |
| Wide retained stationary | 0.891 | 1.000 / 0.927 | 0.922 / 0.982 | +10.20% |

The initial implementation had a reproducible 45.07% compact full-redraw
regression. Removing a redundant per-pixel lane clip on the proven-fit scale-1
path reduced it to 9.26%. The scaled path retains its extra lane clip. All four
final before/after comparisons pass the 20% gate; the optimized A/B repeat also
passes (largest increase 6.48%). Preserve the failed intermediate results.

These are host medians. The before capture has variability and the new wide
renderer draws larger glyphs, so this is a recorded workload comparison, not a
universal speedup or an estimate of device FPS. Full redraw and stationary
retained rendering measure different work; neither measures the panel or DMA.

Reproduce from the repository root:

```powershell
python host/scripts/compare_benchmarks.py host/baselines/configurator_font_before host/baselines/configurator_font_optimized_b --budget-percent 20 --gate
python host/scripts/compare_benchmarks.py host/baselines/configurator_font_optimized_a host/baselines/configurator_font_optimized_b --budget-percent 20 --gate
```

To take new timings, use `host/scripts/capture_benchmark.py` with a fresh capture
name, the same runner/CPU affinity and `--filter '^views/'`. Keep source files
unchanged and other builds/animation stopped until capture completes.

## Actual browser checks

Final release assets were exercised in the Codex in-app Chromium browser:

- Rejected long labels immediately show the red INVALID PREVIEW banner, pixel
  watermark, frame and widget marker. Exact attempted input is listed below.
- Simultaneous NaN fuel and overlong label edits remain invalid when only one
  field is corrected. Correcting both restores export; discard restores accepted
  data. The last accepted data remains the source of sensor/fuel calculations.
- Enabling polling for a disabled source shows the attempted `EveryMs(1500)`
  source settings and marks its oil widget. Discard restores normal editing.
- Overlapping 2x2 placement is rejected but inspectable, with both participants
  marked. Valid 2x1/2x2 numbers are visibly larger; extreme-width values shrink
  within the cell. Bar/PSI switching, animation, pause and saved-view reload work.
- At a 390x844 viewport, the invalid banner wraps and there is no horizontal
  document overflow (375 px content width after the scrollbar).
- Final browser error/warning log was empty. Temporary test data was removed;
  the accepted default document returned to eight widgets and 466 B.

A warmed 1440x1000 trace with two views/13 widgets recorded 64 interactions
(sample advance, bar/PSI, rejected labels and correction). Input-to-next-
presentation-opportunity p95 was 15.6 ms, maximum 15.8 ms; preview-render p95
was 0.8 ms. There were no dropped/in-flight samples or reported long tasks.
First preview after navigation was 215.7 ms (135.7 ms after Rust mount) for the
13-widget document. This is one observed local trace, not a cross-browser or
maximum-capacity latency guarantee; the earlier capacity baseline remains in
[browser-verification.md](browser-verification.md).

Production assets total 1,376,393 B: Wasm 1,312,423 B, generated loader 45,617 B,
CSS 16,997 B and HTML 1,356 B, below the existing 1.5 MiB budget.
Wasm SHA-256: `5d963ed807c2b372006eaf63cff8b10847dd86e906f9f9500341dbfba88bb0de`.
Trace SHA-256: `22e2596d213e6216402148b144d4ef1f2d451ecd9b17fd339d3e1f99be53ccb9`.
Raw trace, asset manifest, screenshots and test logs are local ignored evidence
in `firmware/target/preview-hardening-audit`.
