# Host benchmark capture and comparison

Correctness tests and Rust Criterion benchmarks run on the host. Their timings
do not establish RP2350 cycles, physical display FPS or ECU support.

Run from `host`, with no concurrent builds or source edits:

```text
python scripts/capture_benchmark.py before-change --runner-id ea888-workstation-01 --filter 'scene/|acquisition/'
# Apply the implementation change, keeping workload and capture scripts fixed.
python scripts/capture_benchmark.py after-change --runner-id ea888-workstation-01 --filter 'scene/|acquisition/'
python scripts/compare_benchmarks.py baselines/before-change baselines/after-change --budget-percent 20
```

`runner-id` is an operator attestation identifying one physical controlled
machine. Assign a different ID to each machine; OS/CPU descriptions cannot prove
machine identity. The identifier is not an automatic load, thermal or power
policy check. Keep those conditions stable and inspect distributions. Shared
GitHub runners run correctness and benchmark smoke checks, with no timing gate.

Capture policy 2 rejects direct compiler overrides (`RUSTC`), compiler wrappers,
target, profile and rustflags environment overrides, and effective Cargo
configuration overrides. `RUSTUP_TOOLCHAIN` is an allowed rustup selector: its
value is recorded and must match between captures. Cargo uses the exact resolved
rustup compiler recorded by path and `-Vv`; the wrapper pins `RUSTC` internally
after rejecting caller overrides. The dated toolchain,
manifests, lockfiles, Rust implementation, all benchmark files and capture
scripts are archived before execution and checked again afterward. A failed
run retains its inputs but does not produce accepted completion metadata.
Names cannot reuse an existing capture or Criterion raw directory.

The comparison checks ZIP CRC, all archived hashes, complete raw files, at
least 30 finite positive samples per case, and identical measurement coverage.
It independently derives the median from Criterion's `time / iterations`
samples. Runner identity, OS/CPU, compiler, environment, Cargo configuration,
build options, host manifest/lock/toolchain, every benchmark helper and both capture scripts must match.
An implementation difference is allowed; a changed workload requires separate
review and a new matched pair. Source archives also make such review possible.

The supplied percentage budget is explicit. Changes within budget pass; a
larger point estimate requires review. Disjoint 95% median intervals beyond
the budget are labeled regression. `--gate` fails on either review or
regression and is appropriate only on a controlled quiet runner. This heuristic
is not proof of causality or a substitute for repeated measurements.

Earlier checked-in captures remain historical evidence. Some predate source
archives, and captures through `stage07_maintenance` predate the effective
compiler/configuration checks and explicit runner ID. They cannot pass policy
2 comparisons and are not retroactively upgraded. Their reported costs and
correctness changes remain documented with their original limitations.

Primary implementation references: [Cargo configuration and precedence](https://doc.rust-lang.org/cargo/reference/config.html),
[effective configuration inspection](https://doc.rust-lang.org/cargo/reference/unstable.html#config),
and the locked Criterion 0.8.2 `src/analysis/mod.rs` median calculation.
