# Browser verification, 12 October 2026

This is the original capacity baseline. Subsequent larger-font and rejected-edit
checks are recorded in [Value sizing and diagnostic previews](preview-hardening.md).

Release build: nightly-2026-10-11, Leptos 0.8.22, Trunk 0.21.14,
wasm-bindgen 0.2.129. Tested in the Codex in-app Chromium browser on Windows 11,
Intel Family 6 Model 183, runner `ea888-workstation-01`, served from localhost.
The embedded browser version was not exposed by the test tooling. Desktop
traces use a 1440×1000 viewport; the narrow layout was inspected at 390×844.

## Measured browser baseline

Two warmed runs used eight views and 64 widgets. A exercises view changes,
bar/PSI, warnings and sample advance; B includes accepted view-name edits and
automatic browser-save preparation. All samples are retained, including B's
38.2 ms maximum. Both runs have zero dropped/in-flight samples and Long Tasks
support was detected. No long tasks were reported.

| Measurement | A | B |
|---|---:|---:|
| Recorded inputs | 64 | 64 |
| Input to next presentation opportunity, p95 | 28.7 ms | 31.5 ms |
| Maximum input latency | 29.4 ms | 38.2 ms |
| Rust preview render, p95 | 0.7 ms | 0.6 ms |
| JSON export / browser handoff, 53,928 B | 1.1 ms | 0.9 ms |
| Binary export / browser handoff, 5,287 B | 0.8 ms | 0.6 ms |

The measured first preview was 141.8 ms after navigation (49.1 ms after Rust
mount) for the default eight-widget document, and 130.0 ms (82.4 ms after mount)
for a maximum-document reload. These are localhost observations, not controlled
cold-cache or network-download benchmarks. A 34,070 B compact JSON import took
1.8 ms; the 5,287 B binary import took 1.5 ms. Browser JSON export is pretty
printed, accounting for its larger size.

Input timing begins at the Rust handler and ends at the animation boundary after
canvas submission. It includes an opportunity to paint, not optical confirmation.
Successful file-import timing includes asynchronous file reading, validation,
acceptance and synchronous save preparation, excluding subsequent Web Lock and
storage-write completion. Export ends when the browser accepts the download
handoff, excluding user interaction and disk completion. Startup and latest
successful operation records belong to the page session and can precede a trace.

Baseline budgets on this runner: p95 input latency ≤50 ms in each warmed run,
no editor long task >100 ms, localhost first preview ≤500 ms, accepted file
operation ≤50 ms, and total uncompressed production assets ≤1.5 MiB. Different
devices, network conditions and browsers need their own measurements.

| Release asset | Uncompressed bytes |
|---|---:|
| Wasm | 1,236,654 |
| Generated JavaScript loader | 45,617 |
| CSS | 15,429 |
| HTML | 1,356 |
| Total | 1,299,056 |

Source: `6a5d10e640b28aa77043cd0f29d214754f96cf16`, including the final audit fixes.
Wasm SHA-256: `54867f66cd8f5e110555ea416f8f2857532c7596ad922fefede527be4d366459`.
Trace A: `8da74e64b3d227b27fb4551b68d7cf3dd43a31bd7d3e3ea53884799671fa596c`.
Trace B: `77d18195ba0a84a4e85c17cea4fe869476fe37f049cff1ee19a9cf18bab53ffc`.
Raw traces, source/asset manifests and screenshots are retained locally under
the ignored `firmware/target/configurator-audit` directory. Earlier traces are
retained separately and are not substituted for the final instrumented build.

## Browser workflow checks

- Library drop near cell boundaries; existing-widget collision rejected with
  layout preserved and a visible error; keyboard movement retains focus.
- Resize through all three spans; invalid numeric input blocks save/export;
  partial `0.` then `7` remains `0.7`; discard restores the accepted document.
- High/low rule changes swap hysteresis endpoints atomically. Same widget ID
  with a different imported channel and subsequent Undo restores the correct
  inspector, source and AFR options.
- Actual JSON and binary downloads; JSON recompiles to the same 524-byte binary
  byte for byte; binary file import succeeds; corrupted binary import preserves
  the document and reports rejection.
- Reload preserves accepted configuration and selected view. Two tabs detect
  conflicting saved bytes; automatic replacement stops; explicit Save here and
  reload recover the chosen draft.
- The final audit reproduced a pointer-selection autosave failure before the
  correction. The corrected build saves selection on pointer start: same-cell
  clicks, rejected overlaps, out-of-grid cancellation and successful moves all
  preserve the accepted selection/document after reload. Invalid drafts still
  block pointer selection. The minimum-load control appears only for lambda,
  whose runtime rule applies that guard.
- Play/Pause, multiline warning appearance, fuel estimate inputs, source
  candidate unavailability, desktop and narrow layouts were inspected. No
  horizontal overflow at 390 px; no captured browser warnings or errors.

The 53 native editor/preview/geometry/clock tests, strict native/Wasm Clippy and
production build pass. Pure tests and source review cover stale callbacks,
pointer cancellation and denied/quota storage outcomes. Touch/pen cancellation,
forced browser quota failures and all browser engines were not manually
exercised. Firmware hardware acceptance remains separate.
