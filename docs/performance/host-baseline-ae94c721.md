# Host baseline at audited revision

Recorded 11 October 2026 (Europe/Warsaw). The production source remains the
audited `ae94c721f2f2124a9fbd675c3ba3cd780fe54644`; no `firmware/pico2/src`
file was changed before these measurements. The named Criterion baseline and
raw sample/estimate JSON are in `host/baselines/ae94c721/`.

## Reproduction

These commands describe the original baseline capture at `a8b366b`, whose
production source was still `ae94c721`. Reproduction requires that exact source
and an empty output directory; do not overwrite or reuse this name on current
code. Current captures follow [the benchmark policy](benchmark-policy.md).

Use the then-pinned `nightly-2026-10-10` (`rustc 1.101.0-nightly`, commit
`32dba69d6`) on `x86_64-pc-windows-msvc`. This run used Windows
`10.0.22631.0`, a 13th Gen Intel Core i9-13900HX, Criterion `0.8.2`, 30
samples, one second of warmup and two seconds of measurement per case.
The benchmark fixture SHA-256 is
`A091538E28CA203FA85467533A52BC4871C9D280848B4F16A5A49EDCACFF3C7E`.

```text
cd host
cargo test --locked --test ui_contract
cargo test --locked --benches
cargo bench --locked --bench dashboard_perf -- --save-baseline ae94c721 --noplot
```

The in-repo UI suite intentionally reports **4 passed, 6 failed** against the
audited source. The failures are alternating-buffer header invalidation,
left/right/top contiguous clipping, PSI peak formatting and omission of the
newest graph sample. The audit's external 17-test harness independently
reproduced **12 passed, 5 failed**; its eight UI cases did not include the new
top-edge check. The benchmark smoke check passed all five cases. Tests must be
green after their corresponding fixes; a benchmark speedup cannot excuse a
missing draw or wrong unit.

## Named host measurements

Values below are median nanoseconds per operation with Criterion's 95% median
confidence interval. The raw JSON is authoritative; numbers are rounded here.

| Workload | Median ns | 95% interval ns |
| --- | ---: | ---: |
| Full 153,600-byte framebuffer clear | 2,224 | 2,157–2,267 |
| Contiguous 128×80 fill, left clipped by 8 pixels | 5,538 | 5,379–5,635 |
| Boost widget with PSI peak | 12,475 | 12,280–12,786 |
| 60-sample mini graph with newest impulse | 2,097 | 2,066–2,129 |
| Current frame-based `SensorState::update` | 5.04 | 4.94–5.15 |

The graph and PSI cases currently benchmark incorrect output. Fixes may do
more work. A repeat run with unchanged production code showed apparent timing
changes of several percent, including a 12% swing in the boost case. Treat
this as a noise warning, not a measured regression or gain. Keep the named
baseline immutable during candidate comparisons; calibrate host variance with
repeat/interleaved runs before setting a timing gate. Ordinary CI should gate
pixel/output equality and bounded work, while a controlled runner can enforce
a reviewed performance budget.

## Firmware compile baseline

The dated audit nightly `2026-03-23` compiled stock and
`cpu300-spi75-1v30` release images with the locked original dependencies.
Its strict Clippy run reproduced six known diagnostics. On the updated latest
nightly, the first link failed because Cargo no longer applied flags from the
unquoted target table. The actual `rustc` invocation lacked both `-Tlink.x`
and `-Tdefmt.x`. Quoting the table key restored the linker flags; stock and
300/75 release builds then passed. These are compile results only.

No RP2350 cycles, physical DMA overlap, SPI payload rate, panel refresh,
visible FPS, input latency, ECU samples or vehicle compatibility were measured.
The next device report must state the scene, clock/register values, physical
trace points, completed-flush intervals and sample-to-present age separately.

Criterion `0.8.2` was selected from the [official release list](https://github.com/criterion-rs/criterion.rs/releases),
with its [breaking-change notes](https://github.com/criterion-rs/criterion.rs/blob/master/CHANGELOG.md)
and [source manifest](https://github.com/criterion-rs/criterion.rs/blob/master/Cargo.toml)
checked on 11 October 2026. The [Criterion baseline CLI documentation](https://bheisler.github.io/criterion.rs/book/user_guide/command_line_options.html)
distinguishes a measurement from `cargo test --benches` smoke execution.
