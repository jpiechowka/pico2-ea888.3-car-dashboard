# Configurator host performance, 12 October 2026

The implementation has reproducible host measurements, including an unresolved
full-suite repeatability limitation. The later focused investigation passes;
it does not replace the earlier failed full-suite gate. Browser responsiveness
and linked firmware budgets have separate evidence in the
[validation record](../configurator/validation.md).

## Captures and provenance

All captures use Windows 11, Intel Family 6 Model 183, runner
`ea888-workstation-01`, nightly-2026-10-11 (`6f5da406b`, LLVM 23.1.3), logical
CPU 0 affinity and 30 Criterion samples per workload. Each retains the source
ZIP, SHA-256 inventory, raw samples, median estimates and 95% intervals.

| Capture under `host/baselines` | Source | Workloads | Capture interval, UTC |
|---|---|---:|---|
| `configurator_final_a` | `c23aa49` | 91 | 09:22:02–09:32:37 |
| `configurator_final_b` | `c23aa49` | 91 | 09:32:38–09:43:36 |
| `configurator_postaudit_a` | `6a5d10e` | 7 | 09:57:46–09:58:45 |
| `configurator_postaudit_b` | `6a5d10e` | 7 | 09:58:46–09:59:31 |

Each pair has 206 identical source hashes. The final pair precedes the final
review fixes; the focused pair captures the corrected source. Actual Criterion
child masks were observed as 1: PIDs 41436/16856 and 13180/43448 respectively.
The first pair's shell sidecar records its start only; complete capture metadata
and the observed successful process exit establish completion. The focused
pair's shell record contains both start/end times and exit 0 for each run.
Both pairs preserve these records without adding missing fields retrospectively.

All 16 unique `WORK` preflight records match the original pinned capture:
written pixels/source elements, dirty cells, header/clear work, damage bytes,
regions and transfer counts. The archive loader checks every source digest,
ZIP integrity, sample count and raw-derived median. Fixed affinity does not
guarantee constant frequency, thermal state or absence of other system work.

## Complete 91-workload result

The unchanged 20% comparator returned exit **1** for final A → B. It flags a
median increase over 20% as `review`; a new lower 95% bound above the old upper
bound times 1.20 is `regression`. These are numerical labels, not proof that a
source change caused the difference.

| Comparison | Pass | Review | Regression |
|---|---:|---:|---:|
| Same-source A → B, all 91 | 66 | 21 | 4 |
| Reverse B → A, diagnostic only | 89 | 1 | 1 |
| Reviewed historical references → A, 77 | 68 | 4 | 5 |
| Reviewed historical references → B, 77 | 60 | 14 | 3 |

B is slower on 63 of 91 workloads, with a median change of +7.01%. The four
forward regression labels are the 128-byte ISO-TP client, single-frame OBD
client, left-clipped contiguous fill and stationary damage plan. Their complete
source inventories are identical between runs. No specific scheduling, thermal
or compiler cause was established. All rows and confidence bounds remain in
[the 91-case table](configurator-results-91.csv).

### Historical workload mapping

The mapping was fixed before examining final timings. This is a reviewed
migration/subset comparison: the strict comparator correctly rejects historical
fixture/build-inventory or coverage changes. Its guards were not weakened.

| Reference capture | Eligible cases | Reason |
|---|---:|---|
| `configurator_pinned_before_a` | 59 | Original equivalent workloads across shared-core extraction |
| `configurator_render_a` | 10 | Four configured-view and six legacy scene fixtures |
| `configurator_rules_a` | 7 | Rules, alarms and expanded acquisition snapshot workloads |
| `configurator_codec_before_a` | 1 | Identical 16,385-byte oversize rejection |

The complete [77-case table](configurator-results-historical.csv) includes both
final runs, reference names and confidence bounds. Fourteen cases have separate
new baselines: eight default/maximum codec operations use changed fixtures;
session/vehicle transactions and four replay cases produce expanded snapshots.
Their old-to-new costs are not equivalent-work optimization measurements.
They remain included in the full 91-case repeatability comparison.

## Focused investigation

Four historical costs exceeded +20% in both full captures. The predeclared
follow-up retained all four and three controls. Two fresh runs were collected
after builds and browser testing stopped, with frozen corrected source.
The strict A → B and reverse gates both pass **7/7**, with median differences
from −3.84% to +2.26%.

| Workload | Historical median | Focused A | Focused B | Historical change A / B |
|---|---:|---:|---:|---:|
| Full framebuffer clear | 2.068 µs | 2.054 µs | 2.091 µs | −0.70% / +1.08% |
| Solid full-screen fill | 12.546 µs | 12.010 µs | 12.281 µs | −4.27% / −2.11% |
| Solid fill clipped at four edges | 12.232 µs | 11.953 µs | 12.018 µs | −2.28% / −1.74% |
| Retained all-changed scene, eight frames | 1.028106 ms | 0.999913 ms | 0.995452 ms | −2.74% / −3.18% |
| Single-frame OBD client | 77.071 ns | 66.727 ns | 65.828 ns | −13.42% / −14.59% |
| 64-rule warning page selection | 412.292 ns | 347.699 ns | 342.062 ns | −15.67% / −17.03% |
| Sensor tick/snapshot without new data | 24.964 ns | 30.865 ns | 29.679 ns | +23.63% / +18.88% |

The sensor cost remains 4.7–5.9 ns above its historical median: one `review` and
one `pass`, with neither meeting the confidence-bound regression rule. It is
not erased by the other results. No historical cost exceeds +20% in both
focused captures. See [all seven rows and intervals](configurator-results-focused.csv).

The larger fill/scene increases return near their historical levels and the
controls stabilize. This supports run variability as a contributor to the
earlier broad differences. It establishes neither a code speedup nor a new
full-suite pass on the corrected implementation. No further performance edit
was justified by this focused investigation.

## Configuration and rendering costs

These are new host baselines from the full `c23aa49` pair, including its measured
variance. Configuration is decoded once at firmware startup, not once per frame.
Retained stationary cases still examine the scene but draw no changed widgets.

| Workload | Full A median | Full B median |
|---|---:|---:|
| Default configuration decode | 4.030 µs | 4.438 µs |
| Capacity configuration decode | 30.995 µs | 34.231 µs |
| Default configuration encode | 2.041 µs | 2.094 µs |
| Capacity configuration encode | 24.457 µs | 24.293 µs |
| Default view, full drawing | 152.594 µs | 193.193 µs |
| Default view, retained stationary | 2.931 µs | 3.834 µs |
| Wide view, full drawing | 79.557 µs | 95.609 µs |
| Wide view, retained stationary | 1.217 µs | 1.016 µs |

The default full/retained and wide full A → B rows have review labels in the
complete table. These costs cannot be converted into RP2350 cycles or physical
FPS. The retained/full rows intentionally perform different drawing work.

## Reproduction

From the repository root, validate the saved comparisons without rerunning:

```powershell
python host/scripts/compare_benchmarks.py host/baselines/configurator_final_a host/baselines/configurator_final_b --budget-percent 20 --gate
# Expected exit 1: preserve the recorded repeatability failure.
python host/scripts/compare_benchmarks.py host/baselines/configurator_postaudit_a host/baselines/configurator_postaudit_b --budget-percent 20 --gate
# Expected exit 0: seven-case focused comparison only.
```

For fresh captures follow the [baseline procedure](../configurator/baseline.md)
and use new names. The focused filter, recorded in both manifests, is:

```text
^(fill_solid/(full|clipped_four_edges)|sensor_state/no_new_sample_tick_snapshot|scene/retained_all_changed_eight_frames|framebuffer/clear_full|client/single_frame_obd|rules/64_warning_page)$
```

All raw captures and tables remain available, including inconvenient results.
Host timing does not qualify SPI/TE/panel latency, physical stack usage, sustained
overclock operation, or ECU behavior. The
[browser record](../configurator/browser-verification.md) measures browser
response separately; the [device checks](../configurator/building.md#configuration-specific-device-checks)
remain pending.
