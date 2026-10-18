# Repository maintenance audit, 18 October 2026

Baseline: `4fed659f74d911b92e12db4c3fa9a2c235b9af3d`. This pass covers firmware,
shared libraries, Studio, native tooling, scripts and documentation. The earlier
BLE prototype is one part of that scope. All findings below are software findings;
no Pico, USB device or vehicle was operated during these checks.

## Scope and review

The baseline inventory contained 432 maintained Rust files, 153 maintained Python
files, five PowerShell scripts and 110 Markdown files, plus separately reviewed
vendor sources. Review followed production call paths and their tests: both cores,
DMA/flash ownership, acquisition quality, all-screen controls/rendering, startup,
configuration migration, browser drafts/workers/storage, helper jobs/processes,
artifact admission and scripts. Independent reviews checked integration findings
against actual source, rather than treating reports as proof.

The firmware/shared-core review covered 33 firmware and 88 core source files plus
31 vendor patch diffs. Studio/config/startup/protocol coverage accounted for all
183 assigned baseline entries, including eleven frozen migration fixtures. The
host test audit read 74 test modules, two fixtures and twelve benchmark sources.
Python received normalized-AST checks for the initial mechanical formatting,
lint review across all maintained modules, and focused semantic/type audits at
process, binary parsing and provenance boundaries. The unfinished instruction
research remains separate; formatting its scripts does not complete its proof.

Coverage means source inspection and exercised contracts, not exhaustive state
exploration or a guarantee that every defect has been found.

## Repairs and shared ownership

| Area | Repair and regression evidence |
| --- | --- |
| Firmware diagnostics | Bounded fault summaries include transport epoch and fault detail; event ordering and suppression regressions preserve nonblocking logging. |
| Studio storage | Blocked upgrades and queued Web Locks fail visibly; ten-second timer bounds, retired-open ownership, transaction completion and cancellation prevent indefinite busy state or false Saved status. Canceled private Promise waits release their callback state while late-open cleanup remains owned. Browser suspension can delay timers; a committing write can have an unknown outcome. |
| Startup preview | A private immutable validated asset owner supplies borrowed views to readiness, preview and replay. Raw imports still validate; truncated/resealed data, Arc alias mutation, geometry, looping and frame boundaries have regressions. |
| Occupancy diagnostics | Each view reports its effective 4×2 or 2×4 shape. Native Undo tests and actual browser DOM checks cover mixed overrides. |
| Helper concurrency | Filesystem/project work leaves the global job mutex; an owned admission permit survives blocking work and errors. Worker panic/poison closure is visible instead of leaving a job running indefinitely. |
| Native errors | UTF-8 byte limits, bounded failure tails and loss indicators survive timeout/overflow. Failed machine-readable stdout is excluded from diagnostics. |
| File publication | Configuration CLI failures preserve existing outputs through atomic replacement. Setup serializes installers, bounds downloads, validates isolated staging and publishes verified executables individually. |
| Process supervision | Python capture, setup and compatibility wrappers reuse one supervisor: Windows Job ownership, POSIX process groups, bounded streams/deadlines and descendant cleanup. A POSIX descendant deliberately escaping its group is outside that guarantee. |
| Provenance | A typed Rust verifier replaces the duplicated Python source/archive implementation. Strict manifests, tree/path/link checks, streamed gzip/tar bounds and malformed-archive tests preserve admission without extraction. |
| Benchmark policy | Capture identity includes the shared supervisor. Orientation uses the required 30 samples; historical 20-sample records remain historical. The full host smoke suite contains 152 cases. |

The general helper JobList protocol admits more data than the browser response
cap. Review found no current exposed producer that can reach that cap; Setup is
a typed model with no HTTP route. Adding Setup or multiple arbitrary stage reasons
requires a fresh response-size review. This is a conditional source disposition,
not an enforced maximum-size guarantee.

## Dependencies and platforms

`rustup update` and the dated `nightly-2026-10-18` pin were checked together with
all five Cargo graphs. The [dependency report](../dependencies/upgrade-2026-10-18.md)
records 44 direct and 407 resolved registry names, compatible libc refresh,
current build tools and upstream constraints. Firmware retains the explicit
Cortex-M33 hard-float target, both 153600-byte framebuffers and unchanged
memory/stack admission reserves.

Ruff 0.17.1 checks formatting/lints across 155 maintained Python modules after
the additions. Mypy 2.4.0 checks seven production boundary modules strictly;
the rest is not advertised as fully typed. CI checks Python 3.11/3.15 on
Windows/Ubuntu and strict nightly Clippy across native, Wasm and ARM.

Linux work is limited to portable helper contracts, process tests, CI and manual
editor/build documentation. Native helper operations and supported flashing
remain Windows-only. [Setup and Linux scope](../configurator/setup-tools.md).

## Verification

The local integrated runs passed root workspace/config-JSON tests, host tests
with both outlines, 152 benchmark smoke cases, Studio tests, helper tests and
strict native/Wasm/ARM Clippy. ARM checks covered unavailable, BLE, legacy Longan,
demo with traces and target benchmark with BLE/traces. Formatting passed in all
five workspaces. The focused script batch passed 172 cases with one filesystem
capability skip where the volume did not expose a distinct DOS 8.3 alias.
PowerShell setup/build fixtures and synthetic helper HTTP checks also passed.

Real Edge failure suites passed at desktop and 760×1000 sizes: blocked database
upgrade/retry, retired upgrade abort, request/write cancellation, lock queuing,
quota/conflict, originals/history, worker timeout, canvas failure, invalid-edit
repair, helper failures and mixed-orientation occupancy. These tests use isolated
origins; an existing user's Studio server is preserved. CI repeats browser
contracts on Linux Chrome/Chromium. Current checks are defined in
[CI](../../.github/workflows/ci.yml); the published commit's run is the delivery
gate, separate from these local receipts.

Independent review added an owning-Waker regression for canceled browser storage
waits. It failed before the repair and passed afterward: open, read and write
waiters release their retained Promise state, detached handlers remain detached,
and canceled writes still roll back. No repeated user-triggered leak was found
in current callers; the repair hardens the shared cancellation contract.

Fresh traced BLE stock and 280/70 release captures both pass practical image and
flash-helper corruption checks on the new nightly. Each uses **458168 bytes of
static RAM**, leaving **66120 bytes before the core 0 stack top**. Conservative
named-frame bounds remain boot 3336, constructor 18736, runtime 15632,
dashboard 28200 and radio 16640 bytes, with unchanged core 0/core 1 reserves
of 8192/4096 bytes.
ELF SHA256: stock `32b105c72d84d06791c2a59b805437f07de1c4530244911b6d133589623c04cb`;
280/70 `1cf015f2b4f155e2b0879f887a5361e9d495020aa9efd10dbd64dde7be616b0f`.
These are source-bound static checks, not measured peak stacks or device runs.
Instruction-level proof, adapter interoperability/encryption, exact DNUC
measurements, power-loss behavior, electrical margins, scanout/FPS and physical
stack high-water remain unverified.

## Matched host measurements

Compiler: nightly-2026-10-18, rustc `8006091eb71ae7d9e9bccb66c6122c9d68af4466`,
LLVM 23.1.4, Windows x86-64 MSVC. Inputs, compiler and optimized build settings
were held fixed for each pair. Quiet slots excluded competing task builds;
ordinary desktop noise was not eliminated.

| Workload | Before | After | Interpretation |
| --- | ---: | ---: | --- |
| Maximum 524288-byte artwork readiness/save query | 7.422 ms | Below 1 µs | After result is timer-limited; no precise speedup ratio |
| Maximum artwork preview frame | 4.222 ms | 0.589 ms; repeat 0.605 ms | About 86% lower host cost |
| Small 1012-byte artwork preview | 73.1 µs | 79.0 µs; repeat 72.3 µs | Within noise; no gain claimed |
| HAL tree verification, warmed subprocess | 88.1 ms Python | 27.9 ms native Rust | Excludes compilation |
| Radio trees, warmed subprocess | 73.6 ms Python | 38.2 ms native Rust | Excludes compilation |
| Compatibility launchers | See preceding Python rows | 183.2 / 186.8 ms | Slower due to Python/Cargo startup; CI invokes the built binary directly |

Readiness repair is `874e8e7`; immutable asset reuse is `db8a3a0`. The
[benchmark source and fixture guide](../../configurator/benches/README.md)
describes admission and 128-frame preview batches. Seven raw batches per result
are retained under [measurement data](../performance/data/maintenance-2026-10-18/).
Provenance JSON retains exact measured source/binary hashes and seven interleaved
samples. Native tree measurements predate an archive-end repair unused by those
workloads; they are not final-binary archive-performance evidence. Compatibility
launcher timings also predate the later bounded-supervisor migration and do not
measure the final wrapper. Initial Rust compilation took 10.01 seconds and is
excluded. None of these host timings proves RP2350 cycles, browser presentation
latency or physical FPS.
