# Configurable dashboard validation

For the current v2 gauges, colours and stack-budget corrections, see
[Gauges and colours](gauges-and-colours.md). Its resource figures supersede the
historical images below; none of these records establish hardware qualification.

For the later larger-font and invalid-preview implementation, see
[Value sizing and diagnostic previews](preview-hardening.md), including its
updated correctness, browser, performance and linked-image evidence.

Software baseline: `6a5d10e640b28aa77043cd0f29d214754f96cf16`, 12 October 2026.
Toolchain: nightly-2026-10-11, rustc `6f5da406b`, LLVM 23.1.3.
This record separates host/browser/linked-image evidence from device validation.

## Requirements and implementation

| Requirement | Implementation and evidence |
|---|---|
| Lightweight runtime file | Versioned, length/CRC-checked, canonical Postcard `.eav`; default 466 B, capacity fixture 5,287 B; maximum accepted 16 KiB. JSON is editable host input, maximum 128 KiB. |
| Rust web application | Leptos CSR, generated Wasm loader, no authored JavaScript or Node/npm; production build and Wasm lint in CI. |
| Flexible 4×2 dashboard | Up to eight views, 64 widgets total; 1×1, 2×1 and 2×2 spans; shared overlap/text/number validation. |
| Configurable presentation | Bounded labels, precision, graphs/ranges, statistics, trend, AFR; actual firmware pixel renderer in the browser. |
| Independent warnings | Per-widget limits, enablement, message, applicability and dwell; all rules evaluated before snapshot coalescing; ordered one/three-message pages and scope. |
| ECU provenance | Closed source catalogue with request/decoder/reference information, route/poll/freshness bounds; unsupported candidates remain unavailable. |
| Named view controls | Dashboard encoder navigation and names, RAM selection retained across pages, configured boot view on reboot; other page controls preserved. |
| Backlight and feedback | Full/50% Dim/delayed Off; matching completed frame controls PWM/LED; feedback takes priority over warnings. |
| Fuel and units | Optional AFR with explicit equivalent/stoichiometric/blend assumptions; bar/PSI remains a presentation conversion. |
| Durable editing | Bounded undo/redo, invalid-draft export gate, stale async/pointer/field guards, truthful storage acknowledgement and cross-tab conflict protection. |
| Concurrency and performance | Immutable config before core 1, one acquisition owner, bounded Watch snapshot, existing affine DMA leases; host benchmarks and linked RAM gates. |

The architecture/design, schema and implementation plan were reviewed before
implementation. Each stage was committed separately. Subsequent reviews traced
configuration from file export through startup, acquisition, warnings, layout,
retained pixels, frame completion, LED/backlight and recovery. Review corrections
include canonical integer encoding across host/ARM, direct f32 JSON rounding,
typed warning hysteresis switching, stale browser bindings and coordinated saves.

The final independent review found and corrected three additional paths:
pointer selection now schedules a replacement save even for cancelled or
unchanged drags; configuration fallback preserves the Demo/transport marker
and appears on Debug; the minimum-load editor is restricted to lambda, whose
warning rule actually consumes that guard. The pointer failure was reproduced
in the browser before fixing it. The fault-header test failed against the old
code using an independent glyph oracle, then passed for both framebuffers.
The regression matrix also exercises pending saves completing before/after
unchanged, cancelled, rejected and successful pointer movements.

## Current local correctness checks

| Suite | Result |
|---|---:|
| Host, default outline | 320 passed, including two ownership compile-fail tests |
| Host, full outline | 320 passed, including two ownership compile-fail tests |
| Shared core | 10 passed per outline |
| Binary configuration | 19 passed, including compile-fail contracts |
| JSON configuration | 23 passed, including precision and malformed-input contracts |
| Configuration build/file tools | 4 passed |
| Editor, geometry, preview and elapsed-time clock | 53 passed |
| Rust benchmark smoke | 91 workloads |

Strict host Clippy passes in both outline modes; shared/config/build-tool and
native/Wasm editor Clippy pass. Portable core checks pass on Thumb and Wasm.
Traced Demo 280/70 and Longan target Clippy pass. Vendor provenance and logging
policy pass. Capture, memory-admission and logging-policy tests pass under normal
and optimized Python, retaining negative-case checks when assertions are disabled.

Browser workflow, two-tab conflict, exact file equality, malformed-file rejection,
responsive layout, asset sizes and maximum-document latency are documented in
[browser-verification.md](browser-verification.md). Browser exception injection
limits are explicit there. The firmware and browser use the same core/config
implementations; tests do not substitute a second rendering/validation model.

## Frozen target artifacts

Both final Demo 280/70 images include frame/TE diagnostics and pass the memory,
SRAM helper, startup/vector and corrupted-artifact gates. Static main RAM is
441,936 B, leaving 82,352 B for core 0, above the 65,536 B admission threshold.
Two framebuffer cells remain 153,604 B each; immutable configuration occupies
6,840 B. No heap, third framebuffer, new flash journal format, or GPIO/DMA owner
is introduced by the configurator.

The corrected images were independently compared with the earlier delivery
images. All 213 archived source entries match the current files; only the
configured header, main and Debug screen changed in the target inventory.
Static allocations remain identical. Stack-changing instruction sequences were
compared across all 318 unique function bodies: main's active poll grows by
8 B to 15,952 B. The normal boot call-chain estimate is now 51,152 B for the
default document and 51,336 B for the capacity document. The inspected core 1
snapshot chain remains 18,992 B within its 32,768 B stack. These are linked-code
estimates excluding interrupt/exception/panic overlap and indirect waker paths,
not measured stack watermarks or guarantees for other profiles.

| Final capture | ELF SHA-256 |
|---|---|
| `configurator-postaudit-default` | `00e7fd216cbf7bd9b2d46c53bc2b76d79d2fa083eeea001c824da727644a0491` |
| `configurator-postaudit-maximum` | `3830c3a54a152260e98e34b61fea5a474fa33ca5975de69a7c866837a0dc2d33` |

Captured bytes, manifests, source archives, ELF files and local command logs
remain under ignored `firmware/target` paths. The build wrapper reproduces new
captures without flashing. JSON feature changes are restricted to host tools;
the ARM target remains allocation-free and binary-only.

## CI and performance

All 14 jobs passed on the final implementation commit in
[run 38233195354](https://github.com/jpiechowka/pico2-ea888.3-car-dashboard/actions/runs/38233195354).
This includes nine firmware build variants, shared native/Thumb/Wasm contracts,
host contracts and benchmark smoke, strict target lint, formatting, and the
production Rust browser build. Builds of experimental 290/300 MHz profiles
establish compilation only; 290/72 did not work on the user's display.

The [host performance record](../performance/configurator-results-2026-10-12.md)
retains both full 91-case captures, the reviewed historical workload map and the
focused repeat investigation. Browser latency, raw asset sizes and measurement
boundaries are in [browser-verification.md](browser-verification.md).

## Hardware acceptance

No image was flashed during this configurator task. The user's previous 280/70
demo observation does not qualify this larger configured image. Physical stack
watermarks, warning/LED/PWM timing, display scanout, electrical/thermal stability,
power-cut recovery and DNUC ECU support remain unverified.

Follow [building.md](building.md#configuration-specific-device-checks) for the
view/control/warning/AFR checks, then the
[Pico/DNUC runbook](../validation/pico2-dnuc-runbook.md) for hardware and vehicle
qualification. Default Vehicle mode remains unqualified with unavailable data;
documentary Simos/CANchecked candidates do not change that boundary.
