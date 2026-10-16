# Performance pass, 16 October 2026

This distinct pass follows the repository refactor and existing-test audit.
The baseline is commit `3712decae9ad4e9948de3bd96160a608a8990db4`, whose
[CI run](https://github.com/jpiechowka/pico2-ea888.3-car-dashboard/actions/runs/38965431908)
passed all 18 jobs. Timing evidence is separate from correctness, image admission
and physical board or ECU qualification.

## Frozen baseline

The complete current suite ran sequentially on the Windows i9-13900HX host,
using `nightly-2026-10-16`, rustc `4ebd25825`, LLVM 23.1.4 and the host crate's
default outline. All **143 cases** contain 30 raw samples. Capture policy 2
validated the source archive, all 259 input hashes, raw sample/median identities,
compiler and effective Cargo configuration. The runner identifier is
`johnny-i9-13900hx-01`; the Windows power scheme was Balanced. No other task
build or benchmark ran concurrently. The desktop was not an isolated laboratory
runner; inspect distributions and repeats before attributing small differences.

[Raw samples, compiler metadata and source archive](../../host/baselines/group5_before/)
are retained, including outliers. These are CPU fixture timings on a PC. They
do not measure Pico cycles, SPI throughput, optical FPS, animation smoothness or
ECU support. See the [workload definitions](benchmark-coverage.md) and
[comparison policy](benchmark-policy.md).

| Baseline fixture | Median |
| --- | ---: |
| Full framebuffer clear | 2.354 µs |
| Solid full rectangle | 23.233 µs |
| Solid odd-x/width rectangle | 3.097 µs |
| Solid one-pixel-wide rectangle | 0.142 µs |
| Contiguous full rectangle | 169.373 µs |
| Contiguous odd-x/width rectangle | 23.570 µs |
| Contiguous four-edge-clipped rectangle | 165.833 µs |

## Measurement and implementation rules

Keep representative full paths beside microbenchmarks, compare identical work,
and treat wall-time variability explicitly. That follows the
[Rust Performance Book's benchmarking guidance](https://nnethercote.github.io/perf-book/benchmarking.html).
The planned fill experiment uses bounded row slices to make loop bounds explicit,
as described in its [bounds-check discussion](https://nnethercote.github.io/perf-book/bounds-checks.html),
while retaining every clipping and iterator-consumption guard. A source-level
hypothesis is not a speedup result.

Firmware already selects `thumbv8m.main-none-eabihf` and
`-C target-cpu=cortex-m33`. The latter enables the supported single-precision
FPU and DSP compiler features. The
[rustc target contract](https://doc.rust-lang.org/rustc/platform-support/thumbv8m.main-none-eabi.html)
forbids disabling `fpregs` with this hard-float ABI. Firmware build routes must
preserve these settings; host and Wasm tools retain their own targets.

The two 153,600-byte framebuffers, affine DMA ownership, cooperative checkpoints,
raw warning/statistics inputs and resource reserves remain constraints.
Presentation smoothing cannot alter warning timing, and cached display facts
cannot authorize a build or flash. The separate unfinished instruction proof is
research; production retains the documented practical image checks.

## Studio artifact matching

The Build & Flash artifact list now caches only display digests. Selection and
build-option changes reuse them; accepted configuration, startup profile and
artwork identities invalidate them. Pending edits and invalid options are checked
on every query. Actual Build and Flash actions still capture the current project
and perform their original authorization checks.

The reproducible native Rust harness is
[`artifact_identity_perf.rs`](../../configurator/examples/artifact_identity_perf.rs):

```text
cargo run --manifest-path configurator/Cargo.toml --locked --release --example artifact_identity_perf -- 100
```

It runs five batches of 100 real editor-action/query iterations per case,
alternating fresh-capture and cached order. Both operations run in the same
release executable (`opt-level=3`, fat LTO, one codegen unit), using the compiler
and host above. Fixture admission and warmup are outside timing. The large
fixture contains an exactly 524,288-byte validated RGB565 asset and a
526,267-byte converted project. Its original artwork is an external handle;
queries do not materialize it. The tiny procedural project is 1,445 bytes.

| Query, median µs | Fresh capture and projection | Cached display |
| --- | ---: | ---: |
| Tiny project: selection | 20.781 | 0.936 |
| Tiny project: options | 22.040 | 0.985 |
| Tiny project: content changed | 39.434 | 40.503 |
| Tiny project: blocked/repair | 11.152 | 0.545 |
| Maximum artwork: selection | 5,540.671 | 3.563 |
| Maximum artwork: options | 5,741.586 | 3.592 |
| Maximum artwork: content changed | 8,912.165 | 8,868.705 |
| Maximum artwork: blocked/repair | 4,495.377 | 3.292 |

[All 80 raw samples and fixture hashes](data/2026-10-16/studio-identity-raw.txt)
are retained (SHA-256 `8dc6e0e562559797c4199920bfc2ee1a370d3528d3e6d0583d072056030a0848`).
The harness and implementation were measured at `b288eb4`, integrated unchanged
as `f275deb`. Large content-miss batches ranged from about 5.7 to 9.3 ms on both
paths: **no content-miss gain is claimed**. Tiny misses added about 1.1 µs.
The old-operation harness projects a fresh build into four copied digest strings;
the former UI retained that build directly. These are equivalent-query timings,
not exact old/new browser instruction traces. The mounted artifact list is the
consumer; an empty list need not evaluate the lazy memo at all.

The display memo no longer retains the 526,267-byte project payload in the large
fixture (1,445 bytes for the tiny fixture). It retains bounded keys, options and
digest strings; cold misses still transiently package the full project. This is
a retained-payload reduction, not a measured browser RSS or peak-memory figure.
No renderer, animation clock, source pin or firmware data path changes here.

Oracle tests compare cached results with fresh captures through content changes,
undo/redo, imports, owner replacement, same-stamp JSON drafts, pending pointer
actions and invalid/repaired build options. Native/Wasm strict Clippy and the
existing upload/flash authority tests cover the changed path. Native timings
do not establish browser response latency or Pico FPS.
