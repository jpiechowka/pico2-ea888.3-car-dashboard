# Landscape host comparison, 17 October 2026

A focused matched comparison of the pre-orientation baseline `91805422ae1ba3ee4b2f34dccd027cd0cd039b59`
and integrated orientation revision `80f36a012e86c85d84c87933913b42bae1bf1323` found
no repeatable landscape regression in three selected workloads. Full-redraw
medians were lower in both pairs, but the repeat had a broad confidence interval.
These observations do not establish a precise speedup or physical display FPS.

## Workloads and controls

The existing `host/benches/view_perf.rs` workloads were used without editing their
bodies or production code:

- `views/default_full`: prepare presentation readings and redraw the configured
  default numeric dashboard, including its seeded history.
- `views/default_retained_stationary`: prepare readings and alternate the two
  retained buffers with stationary data.
- `views/Circular_default_retained_moving`: prepare readings and alternate changed
  circular-gauge snapshots on each reuse of each physical buffer.

Each run used Criterion 0.8.2, 30 samples per workload, 1 second warm-up and
2 seconds measurement. Default `simple-outline`, the same host Cargo manifest and
lockfile, and the normal optimized bench profile were retained. Compiler identity:
`nightly-2026-10-16`, rustc `1.101.0-nightly (4ebd25825 2026-10-15)`, LLVM 23.1.4,
`x86_64-pc-windows-msvc`. The capture policy rejected compiler/profile/configuration
overrides; `RUSTUP_TOOLCHAIN=nightly-2026-10-16` was recorded in all four runs.

Runner `johnny-desktop-cpu2` used Windows 11 build 22631 and the recorded Intel64
Family 6 Model 183 Stepping 1 processor. The wrapper and actual Criterion process
were both observed at affinity mask 4 (logical CPU 2) in every run. Other task
builds/tests were held during the coordinated quiet window; compilation happened
between captures. This was a desktop run, without thermal, power-policy or
background-service isolation. Affinity sidecars are observations, not an automatic
load-control certificate.

Order was **before A → after A → after B → before B**. The first pair's substantial
apparent improvement and high outliers justified one reversed-order paired repeat.
No further repeats were selected. Captures ran from 23:00:05 to 23:02:50 UTC on
16 October (17 October locally). The four runs contain 360 raw samples in total.

## Results

Values are Criterion medians derived independently from each raw time/iteration
sample, in microseconds. Negative change means less host elapsed time.

| Workload | Before A | After A | Change A | Before B | After B | Change B |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Default full | 174.304 | 130.701 | -25.02% | 171.702 | 137.617 | -19.85% |
| Default retained stationary | 3.339 | 3.428 | +2.66% | 3.381 | 3.211 | -5.03% |
| Circular default retained moving | 140.033 | 125.294 | -10.53% | 125.586 | 121.795 | -3.02% |

The full-redraw median decreased in both pairs. Pair A's 95% median intervals
were disjoint (173.301–179.013 before, 130.257–133.823 after), but pair B's intervals
overlap (170.609–175.815 before, 134.049–172.592 after). Do not promote the observed
20–25% median reduction to a tightly established gain.

Moving redraw had lower medians in both pairs, but the baseline itself shifted
from 140.033 to 125.586 microseconds between runs. This run variation is comparable
to the first pair's apparent gain. Stationary redraw changed direction on repeat;
its initial +2.66% had overlapping median intervals. No consistent material
regression appears in these samples. This small workload set does not exclude
regressions elsewhere or attribute differences to a particular code change.

Neither acquisition transport, damage planning, SPI transfer nor physical panel
scanout is timed by these three cases. Their median reciprocal is not device FPS.
Portrait layout, transition costs and hardware acceptance need their own evidence;
see [screen orientation](../configurator/orientation.md) and the
[physical matrix](../hardware/display-orientation.md#physical-acceptance-matrix).

## Source identity and comparison limits

`capture_benchmark.py` froze source ZIPs and per-file SHA256 hashes before each run,
then verified those inputs and compiler/configuration unchanged afterward. Each
baseline capture contains 259 inputs; each candidate contains 261. Raw sample
counts, finite values, exact reconstructed medians, confidence intervals, ZIP CRCs
and source hashes passed the existing comparison loader.

The strict whole-suite comparator deliberately rejects these revision pairs:
`fixture or capture source changed; review workload semantics separately`.
The full fixture inventory changed even though the three selected workloads did
not. A scoped [comparison audit](orientation-landscape-2026-10-17/compare-selected.py)
checks the actual archived sources and confirms:

- The entire previous view benchmark file is identical after removing the newly
  added, unselected `orientation` benchmark function and its registration call.
- Rule/config benchmark changes are confined to new inherited orientation fields
  and the unselected late-invalid-enum fixture's replacement for its old bool.
- All other benchmark files, capture scripts, host build inputs, compiler,
  environment, Cargo configuration, runner identity and selected coverage match.

This is a reviewed comparison of selected unchanged workload bodies, not a passing
strict whole-suite gate. It does not change or weaken the repository comparator.
Baseline repeats differ in one archived source file's CRLF/LF representation:
`crates/dashboard-startup-tools/src/lib.rs`. Its normalized text is identical, and
that converter is not a dependency of this host benchmark. Candidate repeat
source hashes are identical. No source edits were made for the measurements.

## Evidence and reproduction

[Pair A](orientation-landscape-2026-10-17/comparison-a.json) and
[pair B](orientation-landscape-2026-10-17/comparison-b.json) retain full median
intervals and comparison dispositions. Their adjacent run directories contain
compact copies of all raw benchmark JSON, full metadata, console output and
wrapper/actual-process affinity sidecars. The original captures, including
`source.zip`, remain in the assigned worktree's `host/baselines/orientation_landscape_*`
directories. [Archive identities](orientation-landscape-2026-10-17/archives.json)
record exact local paths and SHA256 values. Large source ZIPs are intentionally
not committed; the committed metadata retains every archived source hash.

To reproduce in a clean isolated checkout, select the corresponding exact revision,
compile the executable before timing, then run the capture below during a quiet
window. Use a fresh run name on every invocation. Set the affinity/runner identity
to the actual measured machine and observe the running Criterion process mask.

```powershell
cargo bench --locked --offline --manifest-path host/Cargo.toml --bench dashboard_perf --no-run
$taskProcess = Get-Process -Id $PID
$taskProcess.ProcessorAffinity = [IntPtr]4
$env:RUSTUP_TOOLCHAIN = 'nightly-2026-10-16'
python host/scripts/capture_benchmark.py fresh_orientation_run --runner-id johnny-desktop-cpu2 --filter '^views/(default_full|default_retained_stationary|Circular_default_retained_moving)$'
```

With the full original capture directories, validate the selected comparison:

```powershell
python docs/performance/orientation-landscape-2026-10-17/compare-selected.py host/baselines/orientation_landscape_before_a host/baselines/orientation_landscape_after_a
python docs/performance/orientation-landscape-2026-10-17/compare-selected.py host/baselines/orientation_landscape_before_b host/baselines/orientation_landscape_after_b
```

That audit requires the local source ZIPs; the committed compact evidence alone
cannot perform ZIP provenance validation. Raw medians can be independently reproduced from the compact samples; confidence
interval records are retained in the JSON estimates. Historical captures
were preserved. No image gate, firmware flash or broad test run formed part of
this performance comparison.
