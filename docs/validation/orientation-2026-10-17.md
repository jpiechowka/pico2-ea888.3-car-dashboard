# Orientation verification, 17 October 2026

This feature adds native landscape and portrait rendering to the existing Pico 2
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

## Linked ARM image

A genuine `capture_target_bench.py --firmware --image-gate practical` run passed
at source **c8c5f62**, with device Portrait, eight 1×1 widgets, landscape Boot info
and custom portrait Splash artwork. It used pinned `nightly-2026-10-16`, explicit
`cortex-m33`, hard-float ARM, release optimization 3, fat LTO and
`demo,cpu280-spi70-1v30,simple-outline`. The selected overclock is an expert build
configuration, not evidence of safe voltage or stable hardware.

| Result | Bytes |
| --- | ---: |
| Allocated static RAM | 401520 |
| Core0 headroom | 122768 |
| Conservative boot stack bound | 3344 |
| Core1 construction / runtime bounds | 18816 / 15712 |
| Dashboard stack bound | 26736 |
| Core0 reserve | 8192 |
| Core1 stack capacity / reserve | 32768 / 4096 |

The capture checked selected/linked config, profile and artwork bytes, clock
identity, flash layout, boot metadata, named stack paths and malformed flash
artifacts under ordinary and optimized Python. All 390 captured source hashes
were reverified. Immutable local artifacts are retained under
`target/ori-final/source/firmware/target/bench-captures/native`:

- Manifest SHA256: `50e02a5d3a4dd95f6fd14f9e2c86fa06e35c45b8e33255386113f50d5bb34889`
- ELF SHA256: `78954a0f36c492f61de8519e45d58ec1e005d80f06a7fdcb3af7dcd170fcc624`
- Memory receipt SHA256: `98a82cf4583c347e87379ff0abcaa4d07cd2152787ca9bb393bf9b6934a0e72b`

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
