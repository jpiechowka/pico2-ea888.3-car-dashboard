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
python scripts/capture_benchmark.py candidate-name --filter 'scene/|acquisition/'
```

The capture runner freezes source and compiler inputs before execution and rejects source changes during the run. Do not run builds/tests or other benchmarks concurrently with timing collection. Raw samples and the source archive are saved under `baselines/`.

The frame-based history operation was replaced by a timestamped measurement
operation. Compare the four UI fixtures and the sensor fixture separately:

```text
python scripts/restore_baseline.py ae94c721
cargo bench --locked --bench dashboard_perf -- '^(framebuffer|clipped_contiguous|boost_cell|mini_graph)/' --baseline ae94c721 --noplot
python scripts/restore_baseline.py stage03_timestamp
cargo bench --locked --bench dashboard_perf -- sensor_state --baseline stage03_timestamp --noplot
```

This restores committed measurements into Criterion's ignored `target/` tree.
Do not overwrite the historical baseline with current code. Use the same
machine and fixture version for comparisons. The baseline is from one Windows
host, not a cross-machine speed target. Host nanoseconds cannot establish
RP2350 frame rate, DMA overlap, display scan rate, input latency or ECU
compatibility.
