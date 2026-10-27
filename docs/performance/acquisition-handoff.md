# Acquisition before display coalescing

## Contract

Core 1 owns all eight `SensorState` accumulators. Every accepted measurement
updates current value, extrema and time-sampled history before a
`DashboardSnapshot` is published. Core 0 only reads presentation snapshots.
Rendering fewer frames can discard intermediate presentations without losing
between-frame extrema or changing the average/history sampling cadence.

The same owner carries a separate four-cylinder ignition group in the coherent
snapshot. These observations have independent source, sequence, capture time and
quality, with no scalar history/extrema allocation. Demo batches preflight all
counters before changing admitted cylinders and advance the outer measurement
sequence once per accepted group batch. A malformed member cannot block a valid
one. Typed numeric failures publish that cylinder's invalid quality while keeping
its last-good time. The group adds no vehicle producer or decoder.

Readers expire each cylinder after 500 ms without refreshing observations.
Statistics reset leaves those observations and timestamps intact. Shared warning
dwell uses admitted sample times; elapsed display time alone cannot activate or
clear a warning. Clearing needs all four fresh cylinders at or above the shared
clear threshold. Published masks keep the group warning and its cylinder-line
popup coherent with the measurements. See [sample quality](../vehicle/sample-quality.md).

Invalid and duplicate/regressed measurements are rejected per channel. One bad
channel cannot prevent another valid channel from updating. Snapshot sequence
and reset generation reject regressions; sequence exhaustion stops acquisition
before any partial mutation. Reset requests use a single latest-generation
command. The producer acknowledges the generation together with the reset data;
the display does not report completion before this acknowledgement. Replayed
reset generations do nothing.

The new-peak highlight and trend presentation expire using the display's current monotonic time,
even without another producer snapshot. The independently configured gauge
peak hold has its own 100–60000 ms lifetime and clears on unavailable data. The integrated model now carries typed
source/quality/disconnect state; Demo is explicit and the optional BLE prototype
does not enable ECU requests. See [sample quality](../vehicle/sample-quality.md) and
[transport qualification](../vehicle/diagnostic-transport.md).

## Source checks

1. Embassy's [Watch contract](https://docs.embassy.dev/embassy-sync/0.8.0/default/watch/index.html)
   permits overwrite before a receiver reads. This reproduced a missed peak in
   the old production handoff. Its published 0.8.0 `watch.rs` was checked: send
   replaces the value and increments a u64 revision under the mutex; receive
   clones the snapshot under the same mutex. Statistics run outside that lock.
2. The [Signal contract](https://docs.embassy.dev/embassy-sync/0.8.0/default/signal/index.html)
   and published `signal.rs` confirm overwrite/take semantics. They fit an
   idempotent latest reset generation with a single producer and consumer.
3. Published `embassy-rp` 0.10.0 `critical_section_impl.rs` uses spinlock 31,
   core ownership and interrupt masking/restoration, including nested calls.
   Host tests use the std critical-section implementation and do not validate
   RP2350 spinlock latency. Tests bound DashboardSnapshot at 4160 bytes and the
   acquisition owner at 16384 bytes. The admitted advanced ARM snapshot is
   4144 bytes; these bounds are separate from linked task/stack admission. No allocation or callback is
   introduced inside the Watch lock. Target critical-section duration still
   requires measurement. Watch's own u64 revision has no wrap recovery; at
   100 Hz its exhaustion is far beyond a vehicle service life.

## Historical stage 04 verification and baseline

Eight host tests cover between-frame peaks, 20/40/60 Hz and irregular readers,
130 seconds of history including ring wrap, malformed/late/retried channels,
coalesced reset acknowledgements/replays, expired indicators, bounded storage,
and 10000 concurrent publications with coherent values/timestamps/sequences.
Producer and reader use independent deadlines on a 1 ms simulated timeline;
the 130-second regular reader counts are asserted as 2601/5201/7801, including
the read at time zero. Reset success is shown only when the latest requested
generation is acknowledged, never merely because the button was pressed.
The rest of the host suite and strict host/firmware Clippy pass. Stock and
300 MHz target release builds link. None is a physical concurrency test.

New Criterion fixtures are saved in `host/baselines/stage04_acquisition/`:

| Fixture | Host median |
| --- | ---: |
| Eight-channel batch and snapshot | 443 ns |
| Watch publish/read copy round trip | 76.5 ns |

These establish a new baseline for the new contract. They are not a speedup
comparison with the old raw-value handoff, and are not RP2350 cycle counts.
The historical baseline retains its original host/toolchain/fixture and raw
samples. For a current capture, use a new name from `host`:
`python scripts/capture_benchmark.py acquisition-new-run --runner-id ea888-workstation-01 --filter acquisition`.
Use [the comparison policy](benchmark-policy.md) for matched pairs; never relabel
current expanded workloads as the old stage04 capture.
