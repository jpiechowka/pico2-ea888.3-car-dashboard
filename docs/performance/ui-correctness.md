# UI correctness performance log

All numbers are host Criterion medians on the same Windows i9-13900HX and
`nightly-2026-10-10` used for the immutable `ae94c721` baseline. The results
do not establish RP2350 throughput or visible FPS. The baseline clipped-fill
output was incorrect because colors from hidden source columns were reused.

## Contiguous RGB565 clipping

The first source-aligned implementation iterated every source pixel and
checked visibility individually. Its left-clipped 128 by 80 fill measured
20.61 us, versus the incorrect baseline's 5.538 us. A row-level version
skips hidden source spans and writes only contiguous visible pixels. It
measured 7.971 us in a second run, about 46.7% slower than the incorrect
baseline and 61.3% faster than the first correct implementation. The larger
than expected difference should be checked again on a controlled runner;
these two runs are not an on-device performance result.

The pixel contract tests now pass for left, right, top, bottom, four corners,
unclipped byte order, and fully outside rectangles. Other UI failures remain
until their separate fixes land. Full-clear medians moved from 2.224 us in the
baseline to 2.463 us and 2.375 us across these two runs although that code was
untouched, illustrating host timing drift.

Reproduce from `host/` after restoring the committed baseline:

```text
python scripts/restore_baseline.py ae94c721
cargo test --locked --test ui_contract
cargo bench --locked --bench dashboard_perf -- --baseline ae94c721 --noplot
```
