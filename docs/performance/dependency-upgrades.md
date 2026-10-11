# Dependency upgrade performance log

The immutable `ae94c721` Criterion samples are in `host/baselines/ae94c721/`.
The historical dependency-only candidate runs below used the same fixture and
then-pinned `nightly-2026-10-10` on the same Windows i9-13900HX host. They are host
rendering costs, not device FPS. Six known UI failures still existed at that
checkpoint; subsequent correctness commits fixed them. Current builds use the
newer dated nightly recorded in [the dependency review](../dependencies/upgrade-2026-10-11.md).

## `heapless` 0.9.2 to 0.9.3

After updating both firmware and host lockfiles, the host suite remained at
4 pass / 6 known fail. Both stock and 300/75 firmware release images compiled.
Two fresh Criterion comparisons against the named baseline reported:

| Workload | First candidate median | Second candidate median | Baseline median |
| --- | ---: | ---: | ---: |
| Full clear | 2,157 ns | 2,122 ns | 2,224 ns |
| Clipped fill | 5,437 ns | 5,363 ns | 5,538 ns |
| Boost widget | 13,341 ns | 12,833 ns | 12,475 ns |
| Mini graph | 2,134 ns | 2,211 ns | 2,097 ns |
| Sensor update | 5.96 ns | 5.40 ns | 5.04 ns |

The repeated boost result is slower by about 3-7% in median time; `heapless`
string formatting is on that workload, so this is a possible regression to
retest with interleaved A/B runs after the widget's known PSI bug is fixed.
The sensor update does not use `heapless` and also moved, indicating host
timing drift. The current evidence does not support a device performance
claim or a stable timing gate.
