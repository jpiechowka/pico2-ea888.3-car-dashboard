# Configured rendering

The device and host preview share `configured_scene`: landscape has four columns
by two rows (80 by 107 pixels per cell), portrait two columns by four rows
(120 pixels wide, row edges 26, 99, 173, 246, 320). Both use a 26-pixel header.
Validated widgets occupy
1x1, 2x1 or 2x2 cells. Holes remain black. Each widget owns its clipped rectangle
and border. A layout indexes its visible widgets once; repainting never scans
the full 64-widget document for each cell.

Labels, precision, graph ranges, trend, average, extrema and AFR options use
the same formatter in the raster and retained key. Text falls back through
measured font sizes; invalid or unrepresentable values become `--`. AFR uses
unrounded lambda. Gasoline equivalent, explicit stoichiometry and ethanol
estimates carry distinct visible legends, including the configured ethanol
percentage. E5 labelling alone does not establish a measured composition.

Both physical framebuffers maintain independent keys. Geometry changes clear
the whole frame, including old borders and newly empty areas. Overlay removal
repairs obscured cells. Configuration identity, fuel-dependent strings, quality,
graph samples, peak expiry and unit changes participate in invalidation.
The historical fixed renderer remains an explicit host replay adapter;
firmware uses the configured path. The published `configured_preview` example
binds acquisition to the selected configuration and prepares display readings
at each deterministic sample time. Its final snapshot includes configured
history, warnings, smoothing and peak state; rendering uses the same prepared
current value as firmware. It is a synthetic snapshot export, without control
interaction, completed-warning acknowledgment or a hardware LED simulator.
An unknown view is rejected before opening the output file.

```powershell
cargo run --locked --manifest-path host/Cargo.toml --example configured_preview -- dashboard.ppm
cargo run --locked --manifest-path host/Cargo.toml --example configured_preview -- dashboard.ppm my-dashboard.eav 1
```

See [orientation](orientation.md) for native geometry and per-screen resolution.
Historical v4 behavior/resource evidence is in [the release record](advanced-v4-release.md).

## Historical verification at the renderer stage

Host suites pass 308 tests per outline, including ownership compile-fail cases.
Eight new configured-scene tests cover all channels, spans and 32 display flag
combinations (768 canary rectangles), both buffers, empty views, maximum strings,
extreme numbers, AFR-only rounding changes, peak expiry, stale/fault transitions,
overlay removal and configuration changes. Default and wide PPM previews were
visually inspected. The preview command rejects an unknown view ID before
opening the output file.

```powershell
cargo run --manifest-path host/Cargo.toml --example configured_preview -- dashboard.ppm
```

`host/baselines/configurator_render_a` freezes 203 source inputs, compiler,
raw samples and measured Criterion CPU affinity (logical CPU 0). Each workload
has 30 samples. These are host elapsed times, not Pico timings or optical FPS.

| Workload | Median |
|---|---:|
| Configured default, full redraw | 152.233 us |
| Configured default, stationary retained check | 2.866 us |
| Two 2x2 widgets, full redraw | 80.081 us |
| Two 2x2 widgets, stationary retained check | 0.881 us |

The four workloads establish their first baseline. Six historical scene
workloads were also captured; their medians span 1.935 us to 1,028.106 us.
Adding the explicit historical layout selector changed fixture bytes, so the
strict comparator must reject a direct pre-stage comparison. No stage speedup
is claimed from that comparison or from unlike configured and fixed layouts.

Default and maximum-config Demo 280/70 release captures pass the linked memory,
SRAM-helper, startup/IRQ and 18 deliberately corrupted ELF rejection checks.
Static RAM is 438,016 B, leaving 86,272 B for core 0. The 2,512 B increase over
the embedding stage is the main async task pool; acquisition, configuration,
Watch and framebuffer static sizes are unchanged. Linked non-panic boot chains
sum to 50,240 B (default) and 50,424 B (maximum), before IRQ/startup residuals.
No additional framebuffer or heap is introduced. Flashing, physical scanout,
stack watermark and sustained overclock stability remain separate hardware
validation steps. Named encoder navigation and per-widget warning evaluation
are implemented in the following stage.
