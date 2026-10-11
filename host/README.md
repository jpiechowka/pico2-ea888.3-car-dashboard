# Host validation and benchmarks

This independent host crate compiles the firmware's pure model, widget and
renderer code without importing the RP2350 HAL. The host imports the same
`display/framebuffer.rs` module as the firmware. Its fixed-size aligned storage
and renderer use safe Rust. DMA lifecycle tests are separate from pixel tests;
neither can prove electrical timing on the physical display.

From `host/`, run `cargo test --locked` for correctness and
`cargo test --locked --benches` for the benchmark smoke check. The
audited baseline intentionally has failing correctness contracts; the current
regression suite is expected to pass. Save new candidate runs under a new name:

```text
python scripts/capture_benchmark.py candidate-name --runner-id ea888-workstation-01 --filter 'scene/|acquisition/'
```

The capture runner freezes source and compiler inputs before execution and rejects source changes during the run. Do not run builds/tests or other benchmarks concurrently with timing collection. Raw samples and the source archive are saved under `baselines/`.

Historical captures include intentionally incorrect renderers and earlier
workloads/toolchains. Use their raw values to inspect the cost of correctness;
do not infer an isolated optimization from different semantics. To inspect an
old Criterion baseline without overwriting its committed evidence:

```text
python scripts/restore_baseline.py ae94c721
```

This restores committed measurements into Criterion's ignored `target/` tree.
Do not overwrite the historical baseline with current code. Use the same
machine and fixture version for comparisons. The baseline is from one Windows
host, not a cross-machine speed target. Host nanoseconds cannot establish
RP2350 frame rate, DMA overlap, display scan rate, input latency or ECU
compatibility.

See [capture and comparison policy](../docs/performance/benchmark-policy.md) for controlled timing gates and historical capture limits.

The current optional on-device experiment uses the same finite production
kernels as `target_kernel/` in the Rust benchmark. See
[target benchmark constraints and capture](../docs/performance/target-benchmarks.md).
The long-duration RTT frame/diagnostic parser and its actual Rust producer
fixture are documented in [telemetry windows](../docs/performance/telemetry-windows.md).
