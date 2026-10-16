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

This baseline record contains no optimization gain claim. Matched candidate
measurements and their disposition follow in this same report after review.
