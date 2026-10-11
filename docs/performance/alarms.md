# Stage07 alarm measurements

Measured on the same Windows i9-13900HX host with Criterion0.8.2, 30 samples, pinned nightly2026-10-10, default outline. These are CPU fixture timings, not physical FPS or RP2350 cycle measurements.

| Fixture | Earlier reference | First alarm integration | Final compact inputs |
|---|---:|---:|---:|
| Typed Vehicle sample + snapshot |277.5ns (stage06_typed)|560.8ns|268.4ns|
| Eight-channel Demo batch + snapshot |436.5ns (stage06_typed)|1427.1ns|540.5ns|
| Watch copy roundtrip |77.7ns (stage06_typed)|80.5ns|79.7ns|
| Full dashboard |106.862us (stage07_before)|107.898us|103.569us|
| Retained stationary |1.893us (stage07_before)|1.935us|1.909us|
| Full one-cell trace |105.678us (stage07_before)|110.011us|106.436us|
| Retained one-cell trace |11.363us (stage07_before)|11.438us|11.176us|
| New policy fixture: construction +3 eight-channel updates |not present|not measured|222.6ns|

Raw estimates, samples and metadata are in `host/baselines/stage07_*`. The first two runs used manual run/capture ordering, and their metadata explicitly records that no pre-run source archive was made. They are exploratory comparisons, not controlled regression gates. Earlier stage06 references also predate the new snapshot layout and alarm work.

The first integration made unnecessary full history snapshots even when Vehicle policies were disabled. The compact-input change removed those copies, reuses the same source/quality/time checks and avoids work for disabled policies. Demo still evaluates seven policies before publication, a real additional correctness cost. Its final median is about24% above the earlier no-alarm batch fixture. The small scene and Watch differences do not establish speed gains in this uncontrolled host environment. The retained path remains substantially cheaper than complete redraw for these fixtures.

`stage07_alarms_compact` and `stage07_alarms_final` were run by the new `host/scripts/capture_benchmark.py`. It captures relevant source bytes in `source.zip`, their hashes, compiler, command/features and selected compiler environment before execution; rejects changed source or reused run names; and archives fresh raw results afterward. CRC and every archived source hash were verified. To reproduce a frozen run, extract its source archive in an isolated directory, then execute the command from its metadata using the recorded toolchain. Dependencies are fixed by the included Cargo lockfiles. The script is a provenance guard, not a substitute for a dedicated idle benchmark runner.

The coherent host snapshot now occupies3112 bytes, including alert states and expiration deadlines; the fixed host bound is3200 bytes. The target ELF/RAM guard remains independently required because host layout does not prove target memory use.
