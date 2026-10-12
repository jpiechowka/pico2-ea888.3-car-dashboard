# Configurator baseline

The three source-frozen captures below contain the same182 input hashes at
`e710216d0773b114f7b00c157a023376f5f66809`, pinned nightly-2026-10-11,
75 Criterion cases with30 samples each. Raw samples, confidence intervals,
manifests and exact source ZIPs are retained under `host/baselines`.

| Capture | Runner | Use |
| --- | --- | --- |
| configurator_before_a | ea888-workstation-01, ordinary scheduler | Baseline and variance evidence |
| configurator_before_b | same machine, ordinary scheduler | Same-source repeat |
| configurator_pinned_before_a | same machine, logical CPU0 affinity mask1 | Fixed-affinity baseline for subsequent matched runs |

The ordinary repeat exceeded the20% investigation threshold on14 of75 cases,
including one disjoint-confidence-interval result. These are unchanged-source
timing differences, not software regressions. Both captures remain intact.
The third capture records the shell affinity and an independently observed
Criterion child-process affinity of1. Affinity alone does not guarantee a quiet
machine, fixed CPU frequency or identical cache conditions; subsequent results
need repeat evidence before attributing gains or regressions.

From `host`, each capture uses:

```powershell
python scripts/capture_benchmark.py configurator_before_a --runner-id ea888-workstation-01
python scripts/compare_benchmarks.py baselines/configurator_before_a baselines/configurator_before_b --budget-percent 20
```

For a new pinned run, set the current PowerShell process `ProcessorAffinity`
to1, invoke the capture command with a fresh name, observe the actual Criterion
child mask, and restore the original shell mask in `finally`. Record the
command, UTC times, mask and exit status alongside the capture. The application
source and benchmark inputs must not change during a capture.

Extraction changes manifests and the capture inventory, so the conservative
comparator will correctly refuse a direct automated gate across that boundary.
Do not weaken its workload identity checks. Review the archived workload/source
mapping explicitly, report that comparison as a reviewed migration comparison,
then establish a matching post-extraction baseline for subsequent patches.
New configuration/merged-widget workloads must be reported separately from the
unchanged legacy fixtures.

These are host timings only. No new firmware was flashed and no optical FPS,
hardware cycles, ECU support or sustained overclock stability was measured.

The [final performance record](../performance/configurator-results-2026-10-12.md)
retains both 91-case captures, their failed repeatability gate, the reviewed
historical workload mapping and the completed seven-case focused investigation.
