# Portable core extraction verification

The extraction moves64 production source files byte-for-byte from the firmware
tree to `crates/dashboard-core`. Hardware GPIO/clock/DMA drivers, executor tasks,
startup, CPU-register sampling and flash maintenance stay in firmware. The
firmware and host libraries re-export the portable modules; `simple-outline`
is explicitly forwarded. No runtime behavior was intentionally changed.

## Correctness and target checks

- Host contracts:293 tests per outline, including two ownership compile failures.
- Shared core unit contracts:10 per outline. These previously ran inside host;
  CI now runs them explicitly, preserving303 total tests per outline.
- All75 benchmark smoke cases, host/core strict Clippy, Demo280/70+traces strict
  target Clippy, and formatting checks pass.
- The no_std core checks on Thumb and wasm32-unknown-unknown. No browser assets
  or configurator functionality exist at this stage.
- The linked Demo280/70+frame/TE trace image has428592B static RAM and95696B
  core0 headroom, equal to the baseline. SRAM helper bytes, startup copy and
  IRQ25 gates pass; all18 corrupted artifacts reject with ordinary and
  optimized Python.
- Shared source inventory and logging-guard coverage have negative tests.
  Target/host capture tools include the root workspace and portable sources.

The first root Clippy run exited inside unchanged `az` without a diagnostic in
the restricted process. Repeating the same command/source with compiler process
access succeeded. No dependency source or lint was changed to hide that failure.

## Reviewed workload comparison

`configurator_core_scenes` contains six unchanged scene fixtures,30 samples per
case, fixed logical CPU0 affinity. The actual Criterion process mask was
observed as1. Every archived `host/benches` hash matches the pinned before run.
All64 moved production file hashes match the pre-move values.

| Case | Before ns | Extracted ns | Change |
| --- | ---: | ---: | ---: |
| Full eight changed frames | 1352261.40 | 1333504.76 | -1.39% |
| Full dashboard | 151904.80 | 138156.58 | -9.05% |
| Full redraw with one changed cell | 146720.29 | 146715.52 | -0.00% |
| Retained eight changed frames | 1383952.10 | 1533435.72 | +10.80% |
| Retained one changed cell | 13397.79 | 13049.79 | -2.60% |
| Retained stationary | 2066.25 | 1941.16 | -6.05% |

No case crosses the20% investigation threshold in this run. This is a
descriptive migration comparison, not a passed automatic timing gate or a
speedup claim. Manifest/library boundaries and the inventory helper changed,
and the strict comparator correctly rejects that identity mismatch. The
same-source noise already observed in [the baseline](baseline.md) remains a
limit. Future renderer changes need matched captures and repeated evidence.

No hardware was flashed in this stage. Host timings and linked RAM bounds do
not establish optical FPS, peak stack depth, ECU support or overclock stability.
