# Four-cylinder ignition widget validation, 27 October 2026

Offline implementation and software validation. No USB, BLE peer, flash, reboot
or ECU request was performed. The owner has observed OBDeleven channels on their
ECU; this change does not implement or qualify an independent vehicle decoder.

## Implemented contract

`WidgetChannel::IgnitionRetard` is separate from the eight scalar acquisition and
history indices. Four independently timed, qualified samples occupy a 2x2 cross:
cylinders 1/2 above 3/4. Numeric, OK/RET and simultaneous status plus signed number
work at 1x1, 2x1 and 2x2 in both native geometries and both mounting directions.
The shared synthetic producer drives Studio Appearance, Simulation and firmware
Demo. Vehicle mode remains unavailable. Missing, stale and faulted data cannot
render OK. Freshness is inclusive at 500 ms; 501 ms is stale.

One warning rule shares signed-degree thresholds, message, dwell, priority,
latch and acknowledgment settings across the four cylinders. Defaults are
strictly below zero to activate, at least zero to clear, 1000/500 ms dwell,
DemoOnly and `Ignition retarded`. Global displays one message; CylinderLines
owns a separate page with up to four affected-cylinder lines. Clearing needs
all four fresh and clear. New episode membership invalidates an older completed
acknowledgment. RET text follows the raw sign independently of warning thresholds.

Schema 7 explicitly adds the widget/readout and popup selectors. Frozen v1-v6
host migrations retain existing settings; firmware accepts current canonical
schema 7. Nine immutable v6 fixtures include advanced maximum configurations
and a complete project with original artwork. Postcard is non-self-describing,
so schema compatibility belongs to this application, not automatic library
versioning. [Postcard specification](https://postcard.jamesmunns.com/wire-format).

The signed representation domain is -327.68..327.67 degrees, not an engine safety
limit. The pinned public SimosTools definitions describe 0x200A..0x200D as signed
two-byte values with x/100 scaling. That is a presentation/research lead, not
proof of addressing, response bytes or decoder correctness for this independent
client. [Pinned implementation](https://github.com/Switchleg1/SimosTools/blob/f99771ceb0442c863601a84a282c6e67e6b311a8/app/src/main/java/com/app/simostools/PIDs.kt).

## Defects found and corrected

- Fresh siblings could hide a stale supporting cylinder and retain dwell or
  hysteresis. Preparation now checks each supporter's timestamp before replacing
  it. Delayed typed delivery cannot resurrect support after an observed gap.
- A scalar portrait-fit advisory interpreted six group cache lanes as scalar
  readout/statistics lanes. The checker now selects explicit scalar widgets
  after view-slot enumeration. Group rendering still measures/clips each lane;
  a genuine scalar overflow remains reported with its original slot identity.
- The standalone configured raster exporter omitted the group Demo producer;
  it now uses the same shared producer at the scalar batch timestamp.
- Maximum configuration preparation overflowed Windows' default host build-script
  stack. A joined 2 MiB worker matches the configuration CLI; spawn failure and
  unwind propagation remain explicit. The old binary reproduced 0xc00000fd on
  the same maximum input. This reserve is unrelated to either device stack.
- Practical admission still expected schema 6 and the old emitted validation
  shape. Schema 7 is required; new selector decode/serialize and readout children
  are counted at their actual parents. Exact optional re-outlined gauge validation
  is counted; missing/ambiguous required symbols and unknown helpers still reject.
  Historical families retain their previous required boundaries and all budgets.

The renderer uses bounded text, checked coordinate arithmetic, per-widget and
per-lane clipping, and propagates target errors. Unexpected unsupported geometry
renders a bounded fallback instead of extending into a neighbor. Locked
embedded-graphics 0.8.2 semantics were checked against source and
[DrawTarget documentation](https://docs.rs/embedded-graphics/0.8.2/embedded_graphics/draw_target/trait.DrawTarget.html).

## Verification performed

- Root workspace tests, JSON admission, core without default features and strict
  all-target workspace Clippy. Host tests with both outline modes, strict
  all-target Clippy and benchmark smoke. Studio native tests and strict native/
  Wasm Clippy; helper tests and strict Clippy. Changed-code formatting passes.
- Focused tests cover all spans/mountings/modes/precisions, signed extrema,
  nonfinite rejection, unavailable/stale/source-fault quality, sequence/time
  boundaries, independent freshness, activation/clear dwell, membership,
  ACK/snooze/latch and retained/full redraw parity on both buffers/blink phases.
  The browser-advisory regression includes 288 layout combinations and actual
  foreground glyph checks; its original false warning was reproduced first.
- Strict native, Wasm and affected ARM private rustdoc plus 15 root doctests;
  source inventory: 699 Rust files, 22008 items, zero uncovered contracts.
  Presence/rendering checks supplement source review, not semantic proof.
- All 175 Python inventory sources pass pinned Ruff lint/format and strict Mypy
  for Windows/Linux. Stack 42, practical-image 7, utility-image 17 and no-pairing
  4 regressions pass normally and under optimized Python. Independent review
  also reran the 42 stack cases in both modes.
- Actual optimized Studio browser: three readouts; three sizes; both shapes and
  mounting flips; keyboard placement boundaries and Undo/Redo; global/custom/
  cylinder warnings; all four maximum-length lines; invalid precision/message
  retention, blocked export and repair; missing/stale/one-cylinder fault scenes.
  Saved state survives reload. Exported 627-byte complete project reimports its
  widget/readout/warning/orientation, replacing a deliberately changed message.
  Dashboard JSON from that round trip is independently admitted by the shared CLI.
- Screenshots are actual shared-renderer pixels and production-browser output,
  labelled synthetic. See [gallery and capture identities](../configurator/gallery.md#ignition-retard-previews).

## Linked images and memory

Pinned nightly-2026-10-18, thumbv8m.main-none-eabihf, Cortex-M33. All eleven
fresh image captures pass their respective practical or utility admission gates.
Dashboard captures share 586 identical production input hashes; utilities share
705, with all 586 common entries identical. Inventory digests are respectively
`9a8bcec2dac12f57e56cbbae095fda27603156868bf7d5e3b9dea29a3ea20fac` and
`74962f03456dc6f4340ae6bdec9b3d07b88cc94527cdc7d5862359fd86068e8a`.

| Checked dashboard build | Static RAM B | Headroom B | Flash span B |
| --- | ---: | ---: | ---: |
| Stock Demo, eight-widget group fixture | 401640 | 122648 | 577984 |
| Stock BLE, both traces, default eight widgets | 458744 | 65544 | 1162608 |
| 280/70 BLE, traces, mixed 64 widgets and artwork | 458744 | 65544 | 1176152 |
| Stock Demo, mixed 64 widgets and artwork | 401640 | 122648 | 591824 |
| Stock Demo, scalar 64 widgets and artwork | 401640 | 122648 | 592232 |
| Stock target-bench/BLE, both traces | 458744 | 65544 | 1176936 |
| Stock full-outline Demo, both traces | 441792 | 82496 | 635696 |

All rows except full-outline use simple-outline. Artwork is the retained 1012-byte
synthetic fixture, not the maximum possible asset payload. Flash span is payload
end minus 0x10000000. Standard probe stock/280 allocates 410120 B with 114168 B
headroom; NGM identity stock/280 allocates 410880 B with 113408 B headroom. Their
separate ELF/UF2, no-pairing, discriminator and capture reservation checks pass.

BLE headroom is **only 8 B above the unchanged 65536 B floor**. No gate/reserve
was weakened. Both 153600-byte framebuffers, 32768-byte core1 stack and
8192/4096-byte core reserves remain. Conservative named bounds: Demo boot3352
(3368 with artwork), constructor18960, runtime15616, dashboard26488; full-outline
with traces dashboard28032. BLE constructor18880, runtime15536, dashboard28032,
radio19816. Utility bounds: boot17112, core017208, radio24648, render17416.

ARM object guards remain Config14596, ConfigSlot14620, Widget220,
AdvancedDisplay64, Snapshot4160 and acquisition owner16384 bytes. Measured snapshot
4144 and owner16184 fit after private finite-option packing; the four-cylinder
group occupies128 bytes. These are layout and conservative software bounds, not
continuous physical stack high-water. See [memory contract](../performance/memory-layout.md).

## Host performance comparison

Six legacy scalar acquisition/render workloads use 30 samples per case on the
same attested workstation/compiler/options. The original comparison correctly
refuses a changed benchmark helper: `Channel::Lambda.into()` replaces comparison
with the old scalar type. The measured dashboard workload is unchanged. A fresh
old-source reconstruction with exactly that compatibility edit produces a matched
pair; no comparison policy, raw record or budget was edited.

| Case | Before median ns | After median ns | Change |
| --- | ---: | ---: | ---: |
| Batch and snapshot | 623.481 | 620.622 | -0.46% |
| Typed sample and snapshot | 196.159 | 190.978 | -2.64% |
| Watch copy round trip | 258.000 | 270.945 | +5.02% |
| Full dashboard | 203132.589 | 209849.281 | +3.31% |
| Retained one cell | 19983.655 | 20879.595 | +4.48% |
| Retained stationary | 4723.082 | 4653.798 | -1.47% |

All pass the unchanged explicit 20% gate. Raw samples, confidence intervals and
source archives are in `host/baselines/ignition-before-matched-20261027` and
`ignition-after-20261027`; the earlier original before capture is retained too.
The reconstruction copied the unchanged vendored Trouble dependency omitted by
the historical archive inventory. The older original baseline was faster than
the fresh old-source rerun, so these results do not isolate causal changes from
host thermal/background variation. They do not measure the new group renderer,
RP2350 cycles or physical FPS. [Benchmark policy](../performance/benchmark-policy.md).

Physical readability, stack high-water, RF/electrical margins, scanout and exact
vehicle transport/decoder support remain unqualified. No saved slot was touched.
