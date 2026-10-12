# Browser verification, 12 October 2026

Release build: nightly-2026-10-11, Leptos 0.8.22, Trunk 0.21.14,
wasm-bindgen 0.2.129. Tested in the Codex in-app Chromium browser on Windows 11,
Intel Family 6 Model 183, runner `ea888-workstation-01`, served from localhost.
The embedded browser version was not exposed by the test tooling. Desktop
traces use a 1440×1000 viewport; the narrow layout was inspected at 390×844.

## Measured browser baseline

Two warmed runs used eight views and 64 widgets. A exercises view changes,
bar/PSI, warnings and sample advance; B includes accepted view-name edits and
automatic browser-save preparation. All samples are retained, including B's
51.1 ms maximum. Both runs have zero dropped/in-flight samples and Long Tasks
support was detected. No long tasks were reported.

| Measurement | A | B |
|---|---:|---:|
| Recorded inputs | 64 | 64 |
| Input to next presentation opportunity, p95 | 28.5 ms | 34.0 ms |
| Maximum input latency | 30.9 ms | 51.1 ms |
| Rust preview render, p95 | 0.6 ms | 0.6 ms |
| JSON export / browser handoff, 53,928 B | 1.2 ms | 1.0 ms |
| Binary export / browser handoff, 5,287 B | 0.8 ms | 0.7 ms |

The measured first preview was 92.1 ms after navigation (25.9 ms after Rust
mount) for the default eight-widget document, and 48.6 ms (14.0 ms after mount)
for a maximum-document reload. These are localhost observations, not controlled
cold-cache or network-download benchmarks. A 34,070 B compact JSON import took
14.7 ms; the 5,287 B binary import took 1.5 ms. Browser JSON export is pretty
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
| Wasm | 1,235,362 |
| Generated JavaScript loader | 45,617 |
| CSS | 15,429 |
| HTML | 1,356 |
| Total | 1,297,764 |

Wasm SHA-256: `b4b7782af59f824d1d97c07f1f1c73fde99600e8275b6792d1b5d97f76f62263`.
Trace A: `69ec22ad70cb18269425b6084c2a3fddd3ec2253ebd7226d3de7b436373446e7`.
Trace B: `3ed820ea49cb60ae74fcee79c93e6d433347e50304b0f7e0ac211233e62036ca`.
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
- Play/Pause, multiline warning appearance, fuel estimate inputs, source
  candidate unavailability, desktop and narrow layouts were inspected. No
  horizontal overflow at 390 px; no captured browser warnings or errors.

The 52 native editor/preview/geometry/clock tests, strict native/Wasm Clippy and
production build pass. Pure tests and source review cover stale callbacks,
pointer cancellation and denied/quota storage outcomes. Touch/pen cancellation,
forced browser quota failures and all browser engines were not manually
exercised. Firmware hardware acceptance remains separate.
