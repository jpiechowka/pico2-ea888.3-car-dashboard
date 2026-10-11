# Acquisition before display coalescing

## Contract

Core 1 owns all eight `SensorState` accumulators. Every accepted measurement
updates current value, extrema and time-sampled history before a
`DashboardSnapshot` is published. Core 0 only reads presentation snapshots.
Rendering fewer frames can discard intermediate presentations without losing
between-frame extrema or changing the average/history sampling cadence.

Invalid and duplicate/regressed measurements are rejected per channel. One bad
channel cannot prevent another valid channel from updating. Snapshot sequence
and reset generation reject regressions; sequence exhaustion stops acquisition
before any partial mutation. Reset requests use a single latest-generation
command. The producer acknowledges the generation together with the reset data;
the display does not report completion before this acknowledgement. Replayed
reset generations do nothing.

Peak and trend presentation expires using the display's current monotonic time,
even without another producer snapshot. Sample validity/source/disconnect status
is a separate acquisition stage; the current producer is still synthetic Demo.

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
   RP2350 spinlock latency. The fixed snapshot copy is bounded to 3072 bytes by
   test; the acquisition owner to 12288 bytes. No allocation or callback is
   introduced inside the Watch lock. Target critical-section duration still
   requires measurement. Watch's own u64 revision has no wrap recovery; at
   100 Hz its exhaustion is far beyond a vehicle service life.

## Verification and baseline

Seven host tests cover between-frame peaks, 20/40/60 Hz and irregular readers,
130 seconds of history including ring wrap, malformed/late/retried channels,
coalesced reset acknowledgements/replays, expired indicators, bounded storage,
and 10000 concurrent publications with coherent values/timestamps/sequences.
The rest of the host suite and strict host/firmware Clippy pass. Stock and
300 MHz target release builds link. None is a physical concurrency test.

New Criterion fixtures are saved in `host/baselines/stage04_acquisition/`:

| Fixture | Host median |
| --- | ---: |
| Eight-channel batch and snapshot | 443 ns |
| Watch publish/read copy round trip | 76.5 ns |

These establish a new baseline for the new contract. They are not a speedup
comparison with the old raw-value handoff, and are not RP2350 cycle counts.
Run from `host`: `cargo bench --locked --bench dashboard_perf -- acquisition
--save-baseline stage04_acquisition`. Host/toolchain/fixture hash and raw samples
are stored with the baseline. Restore with the existing baseline restore script
before later comparisons.
