# Timestamped sensor history, stage 03

The production `SensorState` consumes a measurement only when its producer sends
a new snapshot. It rejects nonfinite values, values outside the dashboard's
one-million-unit guard, and timestamps that do not advance. Rendering calls
`tick()` to expire a peak indication; it never inserts a sample. The demo
producer attaches `embassy_time::Instant::now().as_millis()` at capture. In the
locked Embassy Time 0.5.1 source, `Instant` is MCU ticks since startup and
`as_millis()` converts that common clock to `u64` milliseconds
(`instant.rs:8-23,112-115`). Millisecond truncation means distinct events in
one millisecond must eventually carry a separate sequence number if needed.

| Statistic | Provisional wall-clock rule | Value policy |
| --- | --- | --- |
| Graph | At most one point per 2,000 ms; 60 points | Latest accepted value at or after the due time; no gap backfill |
| Average | At most one point per 5,000 ms; 60 points | Sample-weighted mean of those accepted points |
| Trend | At most one point per 200 ms; 50 points | Compare accepted points aged 0-2 s with those aged >2-4 s; no arrow without both windows |
| Peak highlight | 750 ms from the measurement timestamp | Expires even if no new measurement arrives |

The first valid sample fills each empty statistic immediately and initializes
both extrema, including negative temperatures, without a peak flash. Current
value, extrema and history share one acceptance check. The maximum, range and
no-highlight policies are explicit. A reset retains the current value, seeds
extrema from it, empties the average, graph and trend, and clears the peak
highlight; the next **new** sample
starts each window. A long gap inserts at most one point when measurements
resume. The graph's horizontal spacing therefore represents accepted points,
not elapsed time across a gap. Source freshness and a time-aware panel view
remain stage 06/07 work. The temperature trend uses the existing 0.5 C
threshold; EGT uses 5 C, battery 0.05 V and AFR 0.1. These are display-arrow
sensitivity settings, not validated vehicle alarm thresholds.

Nine host tests cover 20/40/60 Hz and irregular render ticks, duplicate and
regressed timestamps, nonfinite samples, an empty interval, graph-ring wrap and
negative-minimum eviction, average-ring eviction, peak expiry, late samples,
reset and source-specific
trend sensitivity. They are model tests. The current Embassy `Watch` keeps the
latest demo value, so a slow render loop can still skip producer samples.
Stage 04's bounded acquisition handoff must close that gap before claiming
lossless peaks on hardware.

## Performance record

On the baseline Windows i9-13900HX host and pinned nightly, Criterion's old
`sensor_state/next_render_update` measured 5.04 ns median, but its operation
was called once per render with no timestamp. The stage 03 case is renamed
`sensor_state/next_measurement_update` and advances a 16 ms sample timestamp
with the same eight-value trace. Its named raw baseline is
`host/baselines/stage03_timestamp/sensor_state_next_measurement_update/`:
4.226 ns median, 95% median interval 4.132-4.347 ns. The benchmark source
SHA-256 is `AB1D50A97CAD1AA8100495768B756B5A21BBE83332642E45201EFA760501C233`.
These different operations are **not** a comparable speedup. A preceding
run of this renamed case measured 3.974 ns; an earlier same-name trial of the
new implementation measured 5.682 ns. The spread shows that single-run host
timing is noisy. None includes the demo producer,
cross-core handoff, rendering, SPI/DMA or a physical frame completion.

The follow-up extrema integration reuses the same timestamp benchmark fixture.
Its paired medians were 4.057 ns before and 4.369 ns after adding accepted
current-value and extrema storage. Criterion reported a 6.4% time increase
(4.2%-8.9% interval). The added state is retained for correctness. Raw runs are
in `host/baselines/review_extrema_before` and `review_extrema_complete`.

To repeat the new case:

```text
cd host
cargo test --locked
python scripts/restore_baseline.py stage03_timestamp
cargo bench --locked --bench dashboard_perf -- sensor_state --baseline stage03_timestamp --noplot
```
