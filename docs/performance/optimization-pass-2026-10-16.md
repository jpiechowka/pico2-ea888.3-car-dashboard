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

## Framebuffer fill results

The [complete after capture](../../host/baselines/group5_after/) at `f628941`
also contains 143 cases with 30 samples each. All compiler, workload, options and
archive checks match. [The full comparison](data/2026-10-16/full-comparison.md)
has no case crossing its 20% review threshold. However, unchanged acquisition,
protocol and stationary-scene kernels also became substantially faster. That
host-wide timing shift prevents attributing the broad differences to this patch.

A focused **before, after, after, before** sequence therefore repeated 12 existing
cases without changing their harness. The only source difference between these
runs is `framebuffer.rs`: the pre-patch `9ffdb83` file versus the integrated
candidate. Both sets otherwise use `f628941`; each archive identifies actual
source bytes rather than relying on its Git HEAD alone. The current candidate
was restored byte-for-byte after capture. No other task build ran concurrently.

[Raw before 1](../../host/baselines/group5_control_before1/),
[after 1](../../host/baselines/group5_control_after1/),
[after 2](../../host/baselines/group5_control_after2/) and
[before 2](../../host/baselines/group5_control_before2/)
retain all 30 samples per case, hashes and exact commands. The
[derived comparison](data/2026-10-16/fill-interleaved.json) takes the median of
the two run medians for each implementation:

| Fixture | Before | After | Change |
| --- | ---: | ---: | ---: |
| Solid full rectangle | 15.182 µs | 2.235 µs | -85.3% |
| Solid odd-x/width rectangle | 1.588 µs | 0.343 µs | -78.4% |
| Contiguous full rectangle | 82.672 µs | 71.701 µs | -13.3% |
| Contiguous four-edge-clipped rectangle | 82.595 µs | 71.362 µs | -13.6% |
| Configured default full view | 188.064 µs | 164.678 µs | -12.4% |
| Ordinary Debug full page | 91.752 µs | 70.181 µs | -23.5% |
| Solid one-pixel-wide, 200-row rectangle | 69.694 ns | 84.956 ns | **+21.9%** |
| Unchanged full clear control | 2.265 µs | 2.246 µs | -0.8% |
| Unchanged RX12 parser control | 14.617 ns | 14.356 ns | -1.8% |

The narrow solid-fill regression exceeds the generic 20% budget and is **accepted
explicitly**, not classified as passing or noise. Its cost is about 15.3 ns per
entire 200-row call. Wider fills and both complete-page fixtures consistently
improved in both after runs. No measured narrow-fill workload or target deadline
justifies adding another special-case branch here. Revisit this tradeoff if
on-device profiling identifies that path as material.

The change obtains one bounded byte slice per row, then writes fixed-size pixel
chunks. It adds no unchecked access, allocation or persistent state. Clipping,
big-endian panel bytes and exact source-iterator consumption remain unchanged.
These observations support the simpler row-based implementation on this host;
they do not establish target cycle counts or physical FPS.

## Measurement and implementation rules

Keep representative full paths beside microbenchmarks, compare identical work,
and treat wall-time variability explicitly. That follows the
[Rust Performance Book's benchmarking guidance](https://nnethercote.github.io/perf-book/benchmarking.html).
The fill implementation uses bounded row slices to make loop bounds explicit,
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

## Native build compiler reuse

Each native attempt now reuses its private host configuration compiler for the
final EAV canonicalization command. Previously it compiled the same tool again
in release mode. The host tool now uses its existing debug build; **firmware
still uses the selected optimized ARM profile**. Canonicalization still runs
and compares its exact output with the configuration linked into the image.
The public non-native CLI keeps its existing release canonicalizer.

One sequential pair used the same 1,824-byte accepted project, no custom artwork,
Stock Demo options, pinned compiler and locks. Registry downloads were warm;
each attempt's compiler output directories were fresh. Tiny phase markers were
identical on both sides. The patch is `165c405`, integrated as `f628941`.

| Interval | Before | After |
| --- | ---: | ---: |
| Complete admitted native job | 69.740 s | 45.810 s |
| Prerequisites | 0.124 s | 0.146 s |
| Source snapshot | 2.088 s | 1.655 s |
| Capture stage, including its checks | 62.936 s | 40.428 s |
| Independent artifact admission | 4.592 s | 3.581 s |
| Within capture: host options tool | 8.094 s | 9.064 s |
| Within capture: ARM Cargo | 29.598 s | 28.369 s |
| Within capture: canonicalization | 22.140 s | 0.104 s |

The complete job was 23.930 seconds shorter in this pair; removal of the second
host compilation accounts for about 22.037 seconds. Other intervals varied and
are not attributed to the patch. This **single pair is observational**, not a
statistically established 34% improvement or a minimum PC specification.
[Exact timings, options and input/source/artifact identities](data/2026-10-16/native-build-pair.json)
retain the comparison. Different attempt paths and source produce different ELF
hashes; both artifacts independently passed admission with identical input hashes,
canonical EAV bytes and practical-gate verdicts.

Five actual EAV fixtures (current default, accepted project, and versions 1–3)
produced identical canonical bytes using the debug and release executables.
Both rejected malformed input without replacing an existing output. Focused
capture tests passed in normal and optimized Python execution. No image, source,
memory, ownership, cancellation or UF2 check was removed.

Two excluded attempts are retained locally: verbose Cargo instrumentation
overloaded log processing and was cancelled; a restricted process-inspection
attempt failed with access denied. Neither contributes a timing result. The
successful pair used normal compiler output and the same helper process.

The observed running `pico2` compiler command contained
`--target thumbv8m.main-none-eabihf -C target-cpu=cortex-m33`, `opt-level=3` and
fat LTO. Cargo aliases, PowerShell recipes, native captures and CI all inherit
the firmware configuration; captures additionally reject ambient compiler and
profile overrides. Run raw firmware Cargo commands from `firmware/`: a manifest
path alone does not discover a descendant Cargo configuration, as documented by
[Cargo's configuration rules](https://doc.rust-lang.org/cargo/reference/config.html).

## Other candidates and unchanged contracts

The pass traced acquisition, presentation, retained rendering, transfer completion,
startup, controls and logs; Studio drafts, storage, workers, preview and helper
identity; and native setup, snapshots, compilation, admission and cleanup.
It follows the separate source refactor and existing-test audit, rather than
treating timing results as a correctness review.

- Reusing formatted widget strings or prepared layouts could remove bounded work,
  but would add lifetime, cache-identity or resident-memory costs. No measured
  benefit justified those changes in this pass.
- Studio resource-report cloning, repeated clock normalization and canvas copies
  remain candidates for browser profiling. The artifact-list measurements do not
  establish them as bottlenecks.
- Streaming native source verification could reduce temporary host allocations.
  The measured snapshot interval was much smaller than duplicate compilation;
  snapshot membership, repeated byte verification and identity locks stay intact.
- No peripheral clock, voltage, DMA register, stack allocation, history capacity
  or framebuffer count changed. Raw measurements still drive warnings, extrema
  and history; smoothing affects presentation only. Animation time and completed
  transfers retain their existing separate responsibilities.

The practical image gate checks static bounds and documented assumptions. It
does not prove live stack high-water, interrupt interleavings, optical scanout,
electrical margins or the car's diagnostic responses. Those remain the
[hardware validation runbook](../validation/pico2-dnuc-runbook.md)'s responsibility.

## Correctness and target checks

The integrated code at `f628941` passed
[all 18 CI jobs](https://github.com/jpiechowka/pico2-ea888.3-car-dashboard/actions/runs/38970518699),
including strict nightly Clippy, host contracts, Wasm, native helper contracts
and 12 firmware configurations. The fill iterator regression test passed before
the change and rejected a deliberate extra-consumption mutation. Fifty focused
render/framebuffer checks passed with each outline mode. Native canonicalization
tests passed in both normal and optimized Python; the display-cache oracle and
existing action-authorization tests cover independent identity and readiness.

The conservative named stack-path bounds were identical in baseline and candidate
CI for both Stock and Longan. These are static admission bounds, not measured
stack high-water:

| Image | Boot | Core1 construction | Core1 runtime | Dashboard |
| --- | ---: | ---: | ---: | ---: |
| Stock | 2,896 B | 18,744 B | 15,640 B | 26,208 B |
| Longan | 2,896 B | 28,008 B | 17,312 B | 28,728 B |

The standalone Cortex-M33 solid-fill function's emitted frame remained 88 bytes.
No persistent buffer or allocation was added by the fill implementation; the
two 153,600-byte framebuffers and existing stack reserves remain unchanged.
Every image still passed its practical memory and linked-input checks. The
instruction-by-instruction research proof was not run or claimed complete.

The final release Studio bundle was also exercised in the browser with two
retained admitted artifacts. Fifteen observed transitions covered matching
identities, changed/restored compiler options and profiles, view/widget
selection, accepted startup edits and undo, pending/invalid timing, and pending
malformed JSON with discard. Invalid or unapplied edits blocked Build and Export;
discard restored matching labels. Flash stayed disabled without device inspection
and board confirmation. The original 1,824-byte project exported byte-for-byte
unchanged afterward. No device scan or USB write occurred. These checks exercise
UI integration; cache allocation reuse and fresh dispatch authority have separate
automated oracles. Existing [workflow and widget screenshots](../configurator/gallery.md)
remain representative because these optimizations preserve visible behavior.
