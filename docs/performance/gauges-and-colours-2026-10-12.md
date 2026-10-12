# Gauge and colour performance, 12 October 2026

These are x86_64 Windows CPU renderer/cache timings, not Pico FPS, SPI/DMA
transfer time, end-to-end acquisition latency or physical scanout measurements.
The pinned compiler is nightly-2026-10-11, rustc 6f5da406b, LLVM 23.1.3.
Raw captures retain 30 samples per case, confidence estimates, exact source
archives, hashes, compiler/build settings and explicit runner identity.

## Workloads

`host/benches/view_perf.rs` now has 34 cases:

- The original four Numeric cases: default full/stationary, and wide
  full/stationary. The default contains eight 1×1 widgets; the historical
  `wide` name actually means two 2×2 widgets (lambda and EGT).
- Five new styles × those two layouts × full/stationary/moving = 30 new cases.
  Moving cases alternate base/changed snapshots on every reuse of each physical
  buffer after warm-up. Both snapshots are prepared outside the measured closure.
  Stationary cases exercise retained-key calculation and skip drawing.

The original four preserve layouts, input samples, timestamps, measured bodies,
black-box boundaries and cache warm-up. The default average-text hue changed,
so the baseline pictures are not byte-identical, although glyph/layout work is
the same. New styles suppress graph/trend and some compact statistics according
to the capability policy. Cross-style ratios are not equivalent-workload gains.
The new 30 cases have no pre-change timing baseline.

These cases do not time acquisition/config parsing, warnings, 2×1 layouts or
actual expansion beyond configured seeds. Expanded-limit arithmetic still runs
for gauges, using accepted extrema. Correctness tests separately cover expansion,
reset, 2×1 layout, unit conversion, invalid/stale values and warnings.

## CPU 0 results and rejected repeatability claim

Captures: `host/baselines/gauges_before`, `gauges_after_a`, `gauges_after_b`;
runner `johnny-desktop-cpu0`, process affinity mask 1. The two after captures
have identical source/hash maps. Their strict 20% directional comparison yields
20 pass, 9 review and 5 regression classifications **on unchanged code**.
Median absolute A/B variation is 22.45%; 20 of 34 cases differ by more than 20%
in either direction. Variation ranges from -56.88% to +195.65%.

| Common Numeric case | Before, µs | After A, µs | After B, µs | A / B change from before |
| --- | ---: | ---: | ---: | --- |
| Default full | 159.131 | 192.772 | 201.080 | +21.14% / +26.36% |
| Default stationary | 5.750 | 3.498 | 6.606 | -39.17% / +14.88% |
| Wide full | 78.941 | 85.844 | 253.799 | +8.74% / +221.50% |
| Wide stationary | 1.137 | 1.096 | 0.950 | -3.59% / -16.49% |

These results do not clear a performance gate or establish an attributable
gain. Default full redraw has a potential slowdown in both new medians, but
neither new lower confidence bound exceeds the old upper bound by 20%.
The original four can be reviewed semantically as above; the strict comparator
correctly refuses the before/after archives because fixture hashes and coverage
changed. Do not disable that protection to obtain a passing result.

Reproduction of the unchanged-source comparison:

```powershell
python host/scripts/compare_benchmarks.py host/baselines/gauges_after_a host/baselines/gauges_after_b --budget-percent 20 --gate
```

## CPU 2 replay and confirmed cache regression

The original 207 baseline inputs were replayed byte-for-byte in an isolated
checkout, verified against the original source archive. The new captures use
runner `johnny-desktop-cpu2`, affinity mask 4; the actual benchmark process mask
was checked. These are `gauges_cpu2_before`, `gauges_cpu2_after_a` and
`gauges_cpu2_after_b`. All 210 after-input hashes match across both CPU 0 and
CPU 2 pairs. No builds or other benchmark runs ran concurrently with timing.

| Common Numeric case | Before, µs | After A, µs | After B, µs | A / B change from before |
| --- | ---: | ---: | ---: | --- |
| Default full | 166.453 | 205.825 | 203.878 | +23.65% / +22.48% |
| Default stationary | 3.649 | 5.756 | 5.591 | +57.75% / +53.23% |
| Wide full | 82.338 | 157.994 | 82.473 | +91.88% / +0.16% |
| Wide stationary | 1.795 | 1.204 | 1.091 | -32.94% / -39.21% |

Unchanged-code A/B classifications improve to 32 pass, 2 review, 0 regression.
However, median absolute variation is still 11.81%, with 10/34 cases varying by
more than 20% in either direction. The two upward reviews are Dial wide moving
(+66.44%) and Horizontal wide full (+42.91%). The complete comparisons are in
[CPU 0 CSV](gauges-cpu0-repeatability.csv) and
[CPU 2 CSV](gauges-cpu2-repeatability.csv).

**Default stationary was a measured regression, not just a noisy median.** Both
after lower confidence limits, 5.086830 and 4.776915 µs, exceed the before upper
limit (3.915573 µs) multiplied by 1.2, or 4.698687 µs. Default full's repeated
22-24% median slowdown does not cross that conservative CI boundary. These
results prompted the following correction rather than a blanket noise dismissal.

## Removing unnecessary work

Source review confirms Numeric stationary does not calculate gauge expansion.
It still performs the original 30 floating-point formatting calls and seven
60-entry graph-key constructions. New work includes larger widget clones and
comparisons (gauge settings and 32 bytes of colours), a gauge-key field and
capability/colour branches. There are no allocations in this path. Two small
changes preserve all prior cache inputs and pixel contracts:

1. Only store a cell key when the existing dirty/clear predicate requires it.
   Equal keys already describe the physical buffer, so rewriting all eight
   complete widget/graph/text keys every stationary frame is unnecessary.
2. Fill the widget's inset only when its warning background differs from the
   outer background. The outer fill already covered it, and the one-pixel
   border cannot reach the two-pixel inset.

`gauges_cache_write_cpu2` isolates the first change: its only changed source
relative to CPU 2 B is `dashboard.rs`. Four cases retain 30 samples each.
Default stationary becomes 4.289 µs (+17.54% versus the original), with CI
[3.617, 5.776] µs; it no longer crosses the 20% regression boundary in this
capture. Its interval overlaps both earlier versions, so this alone does not
prove a durable performance improvement. Full redraw does not use that cache
assignment and still varies, demonstrating remaining measurement variability.

A new work-count regression test first failed on the duplicate inset fill.
It now checks all three spans across normal, warning, warning removal and stale
states, plus independent interior/gutter/neighbor pixel oracles. Existing actual
pixel comparisons and retained-buffer tests cover colour-only warnings, opaque
overlay removal, changed configuration, unavailable data and both physical buffers.

## Final focused captures and qualification status

The final production sources are frozen in `gauges_optimized_cpu2_a` and
`gauges_optimized_cpu2_b`. Both contain the same 210 source hashes and the same
four Numeric workloads, with 30 samples each. This final pair targets the
identified cache/redraw risk; the earlier 34-case matrices remain evidence for
all gauge styles before these two minimal optimizations. No fixture was changed
or result discarded to obtain a pass.

| Common Numeric case | Original CPU 2, µs | Final A, µs | Final B, µs | A / B change from original |
| --- | ---: | ---: | ---: | --- |
| Default full | 166.453 | 188.855 | 233.945 | +13.46% / +40.55% |
| Default stationary | 3.649 | 5.709 | 3.545 | +56.46% / -2.85% |
| Wide full | 82.338 | 147.305 | 89.943 | +78.90% / +9.24% |
| Wide stationary | 1.795 | 2.206 | 2.378 | +22.88% / +32.45% |

Final A/B strict comparison is **3 pass, 1 review, 0 regression**. The remaining
review is default full (+23.88% on unchanged source). The directional pass count
does not establish repeatability: 3/4 absolute changes exceed 20%, and median
absolute variation is 30.89%. See the
[repeatability CSV](gauges-optimized-repeatability.csv) and
[common-case estimates/CI CSV](gauges-final-common-cases.csv). The conservative
20% CI boundary versus the original is crossed by wide full in final A, but not
in B. Default stationary does not cross it in either final capture. This does
not erase the earlier confirmed stationary regression or establish that it is
durably resolved. Wide stationary's repeated >20% median increase also remains
a review item, despite overlapping intervals.

**Host performance qualification remains open.** These captures support the
specific reductions in unnecessary work, but cannot establish a stable net
speedup or clear an overall 20% regression gate. The removed operations are
verified by source and work-count/pixel tests; there is no claimed CPU-cycle,
physical FPS or scanout improvement. The final browser build rendered the
selected eight-widget synthetic preview in 0.9 ms on one observed update; that
is a UI observation, not a benchmark or device timing claim. Further performance
qualification requires a controlled runner and separate Pico measurements, not
repeatedly selecting the fastest result from this session.

Run the final pair on one otherwise idle machine, without builds or edits:

```powershell
$taskProcess = [Diagnostics.Process]::GetCurrentProcess()
$taskProcess.ProcessorAffinity = [IntPtr]4
python host/scripts/capture_benchmark.py my_gauges_a --runner-id my-desktop-cpu2 --filter '^views/(default|wide)'
python host/scripts/capture_benchmark.py my_gauges_b --runner-id my-desktop-cpu2 --filter '^views/(default|wide)'
python host/scripts/compare_benchmarks.py host/baselines/my_gauges_a host/baselines/my_gauges_b --budget-percent 20 --gate
```

Choose the affinity and runner name for the actual machine. Source and workload
identity are enforced; the original v1-to-v2 common-case comparison is explicitly
semantic because the benchmark fixture file grew. Do not bypass the comparator's
identity checks. All nine raw captures are retained (4,680 measured samples).
The renderer and resource correctness record is in
[Gauges and colours](../configurator/gauges-and-colours.md).
