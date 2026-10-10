# Host validation and benchmarks

This independent host crate compiles the firmware's pure model, widget and
renderer code without importing the RP2350 HAL. The temporary `build.rs`
extraction ends when the display driver's pure renderer is separated from SPI.
The aligned frame type avoids relying on accidental byte-array alignment, but
does not prove the current firmware's global framebuffer API is sound.

From `host/`, run `cargo test --locked` for correctness and
`cargo test --locked --benches` for the benchmark smoke check. The
audited baseline intentionally has failing correctness contracts. For a named
performance baseline use:

```text
cargo bench --locked --bench dashboard_perf -- --save-baseline ae94c721
```

After a fresh checkout, run `python scripts/restore_baseline.py ae94c721` to
load the committed measurements into Criterion's ignored `target/` tree.
Compare a candidate with `--baseline ae94c721` on the same machine and fixture
version. The committed baseline is a historical reference from one Windows
host, not a cross-machine speed target. Host nanoseconds cannot establish
RP2350 frame rate, DMA overlap, display scan rate, input latency or ECU
compatibility.
