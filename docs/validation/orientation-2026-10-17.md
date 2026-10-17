# Orientation verification, 17 October 2026

This feature adds native landscape and portrait rendering to the existing Pico 2 W
application and Rust Studio. It does not qualify the physical display or ECU.
See the [workflow](../configurator/orientation.md) and
[hardware acceptance matrix](../hardware/display-orientation.md).

## Contracts and review

- Device orientation, independent Boot/Splash/view overrides, and inherited
  Debug/Logs geometry travel through configuration, project storage, artwork
  conversion, build inputs and immutable frame requests.
- Landscape is 320×240 with a 4×2 grid; portrait is 240×320 with a 2×4 grid.
  Repacking preserves widget identities, spans, options and order. Flips preserve
  logical geometry. Both shapes use the same two 153600-byte framebuffer payloads.
- Frozen legacy EAV and project fixtures exercise explicit migration. Current
  encoders output EAV5, EASP2, EASA2 and EAVP2. Asset shape is checked at project
  admission and standalone firmware build time.
- Transport review checked full redraw on rotation, controller command order,
  DMA completion, partial-update ancestry, faults and delayed completions.
  Manufacturer/controller and locked dependency references are recorded in the
  [hardware guide](../hardware/display-orientation.md).
- Independent reviews covered shared formats/build admission, all-screen
  rendering, Studio state transitions and the practical image checker. Findings
  were repaired with focused regressions, including stale canvas hit targets,
  invalid JSON with a new portrait view ID, flat portrait histories, blocked
  artwork Undo/Redo, and folded serializer stack accounting.

## Software checks

The integration ran shared workspace, host, configurator and helper tests,
strict nightly Clippy for native/embedded/Wasm code, ARM feature-path compilation,
and a production Trunk build. Focused tests exercise both native shapes, all gauge
styles/spans, warning/quality transitions, history gaps, startup scenes, Debug
rows, Logs records, overlays and retained-buffer equality. Portrait graph tests
separate the plotted curve from duration glyphs so label pixels cannot conceal a
one-row graph.

Actual browser checks exercised orientation proposals, Undo/Redo, mounting flips,
invalid-overlap repair by pointer/keyboard/menu, candidate-only invalid JSON,
portrait Debug/Logs, protected recovery, independent startup shapes, retained PNG
conversion and mismatch admission. Responsive checks preserve native pixel
dimensions while resizing the CSS presentation. The final Studio release replay
also checked reconversion → Undo into mismatch → Redo repair and two Undo steps
restoring matching orientation/artwork. Export stayed blocked during mismatch;
history remained available. The expected mismatch produced a contextual
`StartupAsset` error while retaining the last completed preview. Pure publication-state tests
cover failed canvas writes; real browser `putImageData` failure injection was not
performed. Browser simulation is synthetic and cannot establish device timing.
The final browser workflow replay used Studio source `7094b15`; the subsequent
display change only compacts internal transfer rectangles, covered by the panel
model, boundary regressions and linked images below.

## Linked ARM image

A genuine `capture_target_bench.py --firmware --image-gate practical` run passed
at source **2d6b2cc** (integrated as `3a7a42b`), with device Portrait, eight 1×1
widgets, landscape Boot info and custom portrait Splash artwork. It used pinned
`nightly-2026-10-16`, explicit
`cortex-m33`, hard-float ARM, release optimization 3, fat LTO and
`demo,cpu280-spi70-1v30,simple-outline`. The selected overclock is an expert build
configuration, not evidence of safe voltage or stable hardware.

| Result | Bytes |
| --- | ---: |
| Allocated static RAM | 401256 |
| Core0 headroom | 123032 |
| Conservative boot stack bound | 3344 |
| Core1 construction / runtime bounds | 18816 / 15712 |
| Dashboard stack bound | 26728 |
| Core0 reserve | 8192 |
| Core1 stack capacity / reserve | 32768 / 4096 |

The capture checked selected/linked config, profile and artwork bytes, clock
identity, flash layout, boot metadata, named stack paths and malformed flash
artifacts under ordinary and optimized Python. All 390 captured source hashes
were independently compared with the integrated Git source, without mismatches.
Immutable local artifacts are retained under
`target/ori-compact/source/firmware/target/bench-captures/native`:

- Manifest SHA256: `647e1e52325e957feece753c8aa26d3fd0a19446b7163326593f2b5e4c99058e`
- ELF SHA256: `a32bf8066a67d0989b94be8b01b35cca7dba94fef12ccc2e2d20f5f2e3cc23e0`
- Memory receipt SHA256: `0a7d17f5420ba910367ad80ee461cefddc8335b3f1fd7a1582ac531d9b7b6e36`

### Traced vehicle image regression

CI exposed a real static-memory regression in the Longan and target-benchmark
variants with frame/TE tracing. Adding geometry grew each damage rectangle from
8 to 10 bytes; sixteen-entry plans are retained in four futures/channel locations.
Both variants had 65344 bytes of headroom, below the unchanged 65536-byte minimum.

The repair stores the native 320-pixel axis in 16-bit fields and the 240-pixel axis
in 8-bit fields, with geometry in an unused origin bit. Validation precedes every
narrowing cast. Six-byte rectangles preserve all coordinates, extents, sixteen
regions and both framebuffers. All-origin address/stride tests, malformed inputs,
vertical merging through row 319, panel reconstruction and transfer-failure tests
cover this representation.

Both traced variants now pass genuine practical image captures with 458680 bytes
of static RAM and 65608 bytes of headroom: 264 bytes recovered, 72 above the minimum.
Their conservative boot/construction/runtime/dashboard stack bounds are
3328/28008/17312/29216 bytes. Stack reserves and all memory gates are unchanged.
The narrow margin is enforced on every supported CI image; it is not a physical
stack measurement. Earlier failed captures and the intermediate eight-byte attempt
are retained as local evidence.

These are practical static checks with source-reviewed call paths and fixed child
allowances. They are not a complete machine-code stack or exception/reentry
proof. The capture's source inventory is not a claim of complete compiler input
closure. Physical stack high-water, tearing, scanout, clock/voltage behavior and
real ECU support remain unverified. No Pico was flashed for this feature.

## Performance evidence

The [matched landscape report](../performance/orientation-landscape-2026-10-17.md)
retains 360 raw samples, source hashes and two opposite-order comparison pairs.
It found no repeatable regression in three selected existing workloads. Desktop
variation prevents a precise speedup claim and the measurements are not physical
FPS. Shape transitions require a full 153600-byte payload; unchanged orientation
retains the existing partial-transfer path. Portrait renders directly with a
480-byte stride, without a transpose pass or third framebuffer.
