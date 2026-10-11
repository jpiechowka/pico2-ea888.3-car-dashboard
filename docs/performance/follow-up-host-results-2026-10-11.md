# Follow-up performance experiments, 11 October 2026

## Decision

Neither optional optimization is included in the final production renderer.
The tile-band shortcut was rejected after repeated adverse measurements. The
aligned fill refactor in `9d3e9c6` passed pixel/ownership checks, but its timing
acceptance was inconclusive and it was reverted in `95b5a38`. The new tests and
all raw experiments are retained. No failed gate has been waived or relabeled
as a passing performance result.

Immediately after the revert, all 179 working-tree benchmark input hashes
exactly matched `audit2_before`, including the restored damage and framebuffer
files. The new tests separately cover band corners, both pixel bytes and 33 nonzero
solid-fill color/geometry combinations. They pass on the restored code.

The earlier [72-case checkpoint](final-host-results-2026-10-11.md) remains a
historical result. This follow-up adds three fixed damage cases for a total of
75; it does not retroactively change old fixtures or establish new device
performance, physical FPS or ECU support.

## Retained captures and provenance

All runs used Criterion 0.8.2, 30 samples, one second warmup and two seconds
requested measurement per case, default `simple-outline`, the same
`nightly-2026-10-11` compiler and runner `ea888-workstation-01`. No concurrent
build or captured-source edit was performed. ZIP CRC, all 179 input hashes,
complete raw records, sample-derived medians and comparison compatibility were
checked using the existing capture/comparison code.

| Capture under `host/baselines/` | Cases | Source difference from before | Result |
|---|---:|---|---|
| [audit2_before](../../host/baselines/audit2_before/metadata.json) | 75 | None | Expanded-suite baseline |
| [audit2_band_candidate](../../host/baselines/audit2_band_candidate/metadata.json) | 11 | Only `damage.rs`, exact whole-band equality before tile comparison | Late change in every band exceeded budget |
| [audit2_band_repeat](../../host/baselines/audit2_band_repeat/metadata.json) | 11 | Same candidate bytes | Same adverse case repeated; shortcut removed |
| [audit2_fill_candidate](../../host/baselines/audit2_fill_candidate/metadata.json) | 75 | Only `framebuffer.rs`, shared aligned span fill | 74 pass, 1 review; unchanged controls also shifted |
| [audit2_pinned_before](../../host/baselines/audit2_pinned_before/metadata.json) | 75 | None | CPU0 comparison baseline |
| [audit2_pinned_after](../../host/baselines/audit2_pinned_after/metadata.json) | 75 | Same fill candidate bytes | 49 pass, 7 review, 19 regression labels; gate failed |
| [audit2_pinned_repeat](../../host/baselines/audit2_pinned_repeat/metadata.json) | 8 | Same source and executable as pinned-after | 6 pass, 1 review, 1 regression label; repeatability insufficient |

Some candidates were captured with uncommitted implementation changes. Their
recorded `head` identifies the checkout base; the frozen `source.zip` and
per-file hashes identify the actual measured source. Each directory keeps all
four Criterion JSON records per case, pre-run inputs and completion metadata.
No unfavorable capture or sample was discarded.

This byte-identity check describes the captured working tree at validation
time. The repository requests CRLF text on checkout. Switching to the merged
`master` subsequently changed 86 input hashes solely through newline conversion;
all 179 files were checked against the frozen archive and had no other content
difference. This is not a new timing capture. Use the original source archives
for exact captured bytes and retain the comparator's strict hash checks;
newline normalization is not an exemption from comparison identity. The seven
published capture directories preserve their original bytes across checkout.

The CPU-pinned captures used a PowerShell process affinity mask of 1, inherited
by Python/Cargo/Criterion and restored on exit. Both complete pinned runs have
an independently sampled Criterion process mask of 1 in
`criterion-affinity.json`. The short repeat has the wrapper's verified mask and
inheritance record, but no separate live Criterion mask sample. The
[Windows API documents child-process affinity inheritance](https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-setprocessaffinitymask).
Affinity limits migration; it does not prove stable background load, power or
thermal conditions. No system power or priority setting was changed. The
comparison tool does not authenticate the added affinity sidecars; these were
checked separately. Do not pool pinned and unpinned runs into one controlled
comparison.

The executable hash recorded before and after the short repeat is identical:
`ab7a885e4a9f884933cbc243c8f84e7c7badeac793cfe6f35df6752533eff9cd`.
Its 179 source hashes also match. Thus the repeat differences cannot establish
a source-change speedup or regression. Their physical cause was not measured.

## What the measurements showed

The whole-band shortcut adds a scan before the existing tile scan. For a late
change in every band, median time increased **68.70%** and **64.62%** in the two
attempts against the matching before subset. Both comparisons exceeded the
20% budget with separated median intervals. Its apparent stationary-case
benefit did not justify accepting that worst case.

The fill candidate showed promising isolated solid-fill reductions: the pinned
full-width, clipped and odd-width cases were about 78%-81% lower. However,
clear was 23.00% higher and many untouched control paths shifted substantially
in both directions. The full gate failed. A repeat of the same executable then
showed the following:

| Unchanged executable case | Median change |
|---|---:|
| Clear full framebuffer | +24.52% |
| Retained one-cell render | +37.47% |
| Alarm batch | -23.10% |
| Full solid fill | +12.24% |

The statistical labels describe these samples; they do not establish causality
under this observed variability. There is no accepted new aggregate speedup.
The conservative result is to keep the previously verified renderer and retain
the optional candidate in history for future controlled host and target work.
No further timing rerun was used to select a favorable result.

## Reproduce the analysis

The [complete comparison tables](follow-up-comparisons-2026-10-11.md) retain all
rows, including every review/regression label. The ordinary full comparisons
can be regenerated from `host/`:

```text
python scripts/compare_benchmarks.py baselines/audit2_before baselines/audit2_fill_candidate --budget-percent 20
python scripts/compare_benchmarks.py baselines/audit2_pinned_before baselines/audit2_pinned_after --budget-percent 20
```

Adding `--gate` fails for both, as expected. For a filtered experiment, first
validate both complete captures with `load`, keep their full metadata, and
restrict only the in-memory before-record dictionary to the candidate's keys
before calling `compare`. This preserves every source/provenance check and
does not modify capture files. The table document gives those pairs explicitly.
Future captures must use new names; the capture tool rejects reuse.

## Remaining opportunities

These need separate measurement before implementation:

- Revisit aligned solid spans on a demonstrably stable runner and the RP2350,
  retaining the nonzero pixel oracle and clear/whole-scene controls.
- Measure snapshot/cache-key work before adding sequence-based shortcuts;
  unchanged values can still require age, quality and history expiry updates.
- Measure a single histogram pass for several percentiles; this is low-cadence
  work and may not materially affect the frame budget.
- Evaluate narrow-rectangle DMA setup versus packing or wider transfers using
  actual bytes, SRAM cost and electrical traces. Host rendering times do not
  predict DMA or panel scan behavior.

The [hardware runbook](../validation/pico2-dnuc-runbook.md) preserves the target,
panel, transport and vehicle qualification gates.
