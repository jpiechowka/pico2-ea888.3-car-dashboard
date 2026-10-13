# Timestamped sensor history

Core1 feeds `SensorState` for each accepted measurement before publishing a
coalescible display snapshot. Typed samples also check source, sequence and
channel quality. See [acquisition ownership](acquisition-handoff.md) and
[sample quality](../vehicle/sample-quality.md). It rejects nonfinite values, values outside the dashboard's
one-million-unit guard, and timestamps that do not advance. Rendering evaluates
immutable `SensorSnapshot::is_new_peak_at` and `get_trend_at` against current time;
it never inserts a sample or mutates the producer history. The demo
producer attaches `embassy_time::Instant::now().as_millis()` at capture. In the
locked Embassy Time 0.5.1 source, `Instant` is MCU ticks since startup and
`as_millis()` converts that common clock to `u64` milliseconds
(`instant.rs:8-23,112-115`). Typed samples carry an independent sequence; per-channel time guards still
reject non-advancing millisecond timestamps.

| Statistic | Provisional wall-clock rule | Value policy |
| --- | --- | --- |
| Graph | 60 timed buckets across the channel's selected 10-900 s duration (default 120 s) | Last qualified observation in each bucket; absent buckets break the line |
| Average | At most one point per 5,000 ms; 60 points | Sample-weighted mean of those accepted points |
| Trend | At most one point per 200 ms; 50 points | Compare accepted points aged 0-2 s with those aged >2-4 s; no arrow without both windows |
| Peak highlight | 750 ms from the measurement timestamp | Expires even if no new measurement arrives |

The first valid sample fills each empty statistic immediately and initializes
both extrema, including negative temperatures, without a peak flash. Current
value, extrema and history share one acceptance check. The maximum, range and
no-highlight policies are explicit. A reset retains the current value, seeds
extrema from it, empties the average, graph and trend, and clears the peak
highlight; the next **new** sample
starts each window. Every widget of a channel borrows the same canonical history,
including widgets with different display units. The graph reserves sixty positions
across the selected duration and labels that duration while filling. A slow sensor
can leave gaps at short durations. No observation is held, interpolated or copied
into missing buckets; raw extrema and peak processing still see every sample.

The producer's existing tick advances history even without measurements. It clears
at most sixty slots after a stall, never loops through every missed interval, and
uses quotient/remainder arithmetic in both timestamp directions. HistoryView
returns bucket times and Option values, with Option bounds over valid buckets only.
A single point is visible; segments join adjacent valid buckets only. Faults and
freshness loss break connectivity. Recovery within the same bucket can replace that
bucket, so invalidating an outage also deliberately discards at most one preceding
valid point. Older qualified history then ages out normally. New source/configuration
owners, time regression and statistics reset clear the window; changing duration
clears history without resetting independent extrema or averages.

Publication uses the acquisition owner's private run identity and a separate checked
revision, independent of raw sample sequence and reset generation. Timer-only scrolling,
peak expiry and alarm output changes therefore reach both retained framebuffers without
advancing per-sensor identity or warning dwell. After revision/issuer exhaustion,
transport deduplication is conservatively disabled while state remains live. Graph
cache revisions include scrolling, validity, replacement and reset changes; exhaustion
also disables graph reuse. Browser Appearance labels its generated history and still
performs a fixed 60+1 synthetic observations through a private shared acquisition owner;
Simulation keeps its persistent owner. Freshness suppresses stale live values
and raw active warnings. Configured latch episodes retain their independent
acknowledgment/qualified-clear lifetime, with unavailable quality shown separately;
they never restore a stale value or advance dwell without fresh data. The temperature trend uses the existing 0.5 C threshold; EGT uses
5 C, battery 0.05 V and lambda 0.1. These are display-arrow
sensitivity settings, not validated vehicle alarm thresholds.

Focused host tests cover rendering schedules, duplicate/regressed timestamps,
nonfinite samples, explicit fault gaps, negative bounds/eviction, all duration presets,
a custom 137 s duration, u64 timestamp boundaries, fixed work after day-long stalls,
publication exhaustion, average/peak independence and canonical dual-unit history. The
subsequent acquisition-owner tests cover real Watch coalescing without losing
between-frame statistics. Physical acquisition and interrupt behavior remain
unverified.

## Historical stage 03 performance record

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

To capture the current operation with current source/compiler evidence:

```text
cd host
cargo test --locked
python scripts/capture_benchmark.py history-new-run --runner-id ea888-workstation-01 --filter sensor_state
```
