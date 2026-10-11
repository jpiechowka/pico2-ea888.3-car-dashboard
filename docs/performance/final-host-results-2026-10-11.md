# Final host performance checkpoint

Source: `0f60e0614fe934352d4f871f3eeac7cadc0f8278`. Runner:
`ea888-workstation-01`, Windows 11 build 22631, Intel family 6/model 183/stepping 1.
Compiler: nightly-2026-10-11, rustc `1.101.0-nightly (6f5da406b 2026-10-10)`,
`x86_64-pc-windows-msvc`. Criterion 0.8.2, default `simple-outline` feature.

## Repeatability and provenance

Two complete captures each contain **72 cases with 30 raw samples per case**,
one second of warmup and two seconds of requested measurement time. Both have
identical 179-file source/input hash maps, effective compiler/configuration,
Cargo options and runner identity. No concurrent build or benchmark-input edit
was performed. Runner identity records the operator's machine assignment; it
does not continuously measure background load, power policy or temperature.

The predeclared 20% review budget passes all 72 comparisons. The largest median
increase between unchanged runs is **11.01%** for the coolant transaction;
the largest decrease is **6.45%** for sparse disjoint damage. Median absolute
change across cases is **1.85%**. This establishes an observed repeatability
range for this pair, not a universal noise bound or a code-change speedup.
No outlier capture was removed or replaced.

- [Run A metadata and measurements](../../host/baselines/stage08_final_a/metadata.json)
- [Run B metadata and measurements](../../host/baselines/stage08_final_b/metadata.json)
- [Complete 72-case comparison](final-host-comparison-2026-10-11.md)
- [Fixed workloads, output oracles and work counts](benchmark-coverage.md)

Each run directory retains `source.zip`, pre-run inputs, completion metadata
and all four Criterion JSON files per case. The comparison verifies ZIP CRC,
entry hashes, sample counts, raw-derived medians and confidence intervals.
ZIP bytes differ because their entry timestamps differ; their input contents
and per-file hashes are identical.

```text
# Run from host/, using new names for any future capture.
python scripts/capture_benchmark.py stage08_final_a --runner-id ea888-workstation-01
python scripts/capture_benchmark.py stage08_final_b --runner-id ea888-workstation-01
python scripts/compare_benchmarks.py baselines/stage08_final_a baselines/stage08_final_b --budget-percent 20 --gate
```

The capture tool deliberately refuses those names now that they exist.
Both outline configurations pass correctness and benchmark smoke checks;
timings in this report apply only to the recorded default outline configuration.

## Matched rendering paths

These compare alternative paths **within each capture**, using the same scenes
and verified pixels. Values are median microseconds per listed operation, with
run A / run B shown separately. Batches are not single-frame times.

| Workload | Full A / B (us) | Retained A / B (us) | Observed difference |
|---|---:|---:|---|
| One unchanged dashboard | 100.061 / 103.605 | 1.893 / 1.896 | 98.1%-98.2% less host elapsed time |
| One boost-cell change | 101.385 / 103.257 | 10.277 / 10.620 | 89.7%-89.9% less host elapsed time |
| Eight all-channel/history/overlay changes | 991.052 / 994.666 | 983.907 / 1013.573 | -0.7% / +1.9%; no established advantage |
| Eight Demo ingestion/snapshot/render steps | 1012.539 / 1029.084 | 993.558 / 1024.577 | -1.9% / -0.4%; small relative to host variation |
| Seventeen slow/bursty Session/parser/render steps | 678.704 / 664.923 | 193.380 / 194.790 | 70.7%-71.5% less host elapsed time |

The all-change result preserves the worst-case cost: retained rendering cannot
skip genuinely changed content and sometimes adds cache work. The slow/bursty
replay benefits from repeated unchanged, unavailable or single-channel states.
Its 9000 ms logical span does not wait 9000 ms and its 17 render calls do not
represent 17 DMA completions. See the workload record for what each path excludes.

## Guard and observability costs

Criterion measures elapsed wall time, including host scheduling effects. These
are host execution costs of the final implementation, not device latency estimates.
The legacy mailbox fixture preserves the earlier code in the same binary;
its independent atomic fields provide a weaker concurrency contract.

| Work | Run A / B |
|---|---:|
| Legacy input atomic roundtrip | 14.618 / 14.833 ns |
| Coherent bounded mailbox roundtrip | 33.963 / 34.079 ns |
| Record all frame-window metrics | 41.210 / 40.712 ns |
| Summarize a window seeded with 1500 frames | 1.032 / 1.059 us |
| Session coolant transaction and snapshot | 418.825 / 464.917 ns |
| Diagnostic distribution export | 313.094 / 319.378 ns |
| Journal load | 14.164 / 14.342 us |
| Rollover in the in-memory NOR model | 17.525 / 17.638 us |

Mailbox coherence costs about 19 ns more in this uncontended host fixture
(130%-132%); it is an accepted correctness cost, not an optimization claim.
The journal's measured time excludes real flash erase/program delays. Frame-window
export excludes RTT transmission. The separate
[target-kernel experiment](target-benchmarks.md) records configured clocks,
counter/timebase state and finite checksums when eventually run on hardware.

## Historical comparisons and limits

The initial [audited-source baseline](host-baseline-ae94c721.md), corrected
clipping/graph/header captures, grouped dependency captures and later storage,
history, acquisition, transport and persistence captures remain immutable.
They record both gains and added correctness cost. In particular, drawing every
history point costs more than the audited implementation that omitted points.
The earlier retained-scene stage also recorded increased full-draw work while
fixing stale pixels; that cost was not hidden by its retained-path savings.

Do not divide these final numbers by the original five-case baseline to claim
an overall overhaul speedup: fixtures, correct output, toolchain and comparison
policy changed across those stages. Policy 2 rejects incompatible benchmark
helpers and compiler/build identities. A future code change should keep these
fixtures fixed and capture a new matched pair with the same runner controls.

No RP2350 cycle result, physical SPI/DMA throughput, electrical margin, panel
refresh/FPS, tearing, input-to-photon latency or ECU compatibility is measured
here. Those remain explicit gates in the
[acceptance record](../validation/overhaul-2026-10-11.md).
