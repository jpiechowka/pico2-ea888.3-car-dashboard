# Software failure validation, 17 October 2026

This pass extends the completed orientation and optimization work with actual
browser failure injection, deterministic generated inputs, state transitions and
native recovery tests. It also refreshes the shared splash renderer and adds five
themes, bringing the procedural set to ten. The starting commit was
`262c672529b0305d922493d5f7c3b198345be3a7`.

These are bounded software checks, not exhaustive correctness or hardware proof.
The instruction-level proof remains deferred. Production builds retain their
[practical image gates](../performance/memory-layout.md).

## Actual browser checks

The opt-in `browser-tests` feature mounts the production Rust/Wasm Studio and
exercises its browser adapters. A separate runner creates a temporary profile and
ephemeral loopback origin. Both the exact `?browser-tests=1` query and the runner's
server marker are required before tests modify storage or native methods. Normal
release builds omit the driver and fault seams.

| Stage | Exercised behavior and observable result |
| --- | --- |
| IndexedDB | A successful put followed by an actual transaction abort rejects settlement and preserves previously committed bytes. |
| Project storage | Foreign save capability, stored-envelope conflict and injected native `QuotaExceededError` cannot replace committed data. Corrupt restore and a valid restore racing a newer edit preserve the accepted editor. |
| Originals | A second same-origin iframe holds a real Web Lock while the production operation waits. Owner lifetime locks and pins retain live originals; unpinned data is collected. An actual transaction abort rolls back GC changes. |
| Helper transport | Socket close before headers, a truncated HTTP body and AbortController cancellation fail through the production bounded fetch path. |
| Artwork workers | A real silent Worker exercises the production ten-second deadline. Malformed replies clean up; a valid converted PNG reply reaches the acceptance callback. Valid stale/cancelled replies cannot accept again or replace newer state. The materialization lease is released. |
| Canvas | A healthy native canvas write publishes interaction targets. An injected `putImageData` exception returns `CanvasDraw` and revokes that publication. |
| Mounted DOM | Focus/input/blur events retain an overlong view-name draft, preserve accepted state and block export. Repair unblocks export. Selecting new theme tags 6–10 and returning to 0 updates the intended profile. |

Final local execution passed all seven stages on **Edge 155.0.4283.62**, with
runner exit 0. The browser is deliberately terminated after reporting. Missing
query and incorrect server-marker negative controls stayed inert and reached the
runner's short test deadline without reporting a stage.

Reproduce from `configurator/` with a local Chrome, Chromium or Edge executable:

```text
trunk build --release --locked --features browser-tests --dist target/browser-tests
python scripts/browser_failure_tests.py --browser <absolute-browser-path> --dist target/browser-tests --timeout 90
python -m unittest discover -s scripts -p "test_browser_process.py"
```

The four process tests cover a root exiting before its child, deadline cleanup,
failed spawn and preserving a different process tree. Windows atomically assigns
the browser to a kill-on-close Job; POSIX uses its own session/process group and
TERM/KILL cleanup even after the root exits. Deliberate POSIX process-group escape
is outside this runner's model. No shared browser profile or Studio process is
terminated. CI runs process tests on Linux and Windows, and the browser suite on
Linux Chrome/Chromium.

**Limits:** the quota exception is injected, not a full-disk experiment. The
second context is an iframe, not a crashed browser process. Worker messages are
synthetic replies delivered to real lifecycle handlers; the positive reply is
produced by the real PNG converter. Helper cases cover transport boundaries, not
the complete reconnect UI or USB. Earlier cross-realm test-driver casts failed
and were repaired; they were not production failures.

## Deterministic formats and state transitions

| Suite | Budget and invariant |
| --- | --- |
| Configuration formats | Seed `0x17ea5005`, 512 default cases: resealed EAV mutation, owned decode versus boot publication, independent EASP wire predicate, JSON/frozen migration convergence, nonfinite/boundary fuel values. |
| Complete projects | Seed `0x17ea7002`, 512 default cases: nested profile/descriptor mutations and maximum eight-view, 64-widget projects across all four orientations. |
| Artwork formats | Seed `0x17ea5a02`, 128 default cases per source/geometry: PNG/GIF mutations, resealed EASA payloads, red-pixel anchor, row canaries and whole/incremental validation consistency. |
| Editor | Seed `0x17ed1702`, 256 sequences: accepted edits, rejected repair, history, orientation, stale save/import callbacks and JSON drafts. |
| Presentation | Seeds 1–64, 64 cycles through four ownership phases: wrong transfer identities cannot advance or free a buffer. Another 256-event sequence per seed keeps quarantine absorbing. |
| Startup and backlight | Seeds 1–256: arbitrary polls, delayed matching completions, stale completions, popup/clear timing, replacement, recovery, stalls and time reversal. |
| Diagnostic transport | Every payload length 1–256, sequence wrapping, flow-control blocks, foreign IDs/epochs, padding and 36 consecutive-frame positions with four abort modes; a fresh transaction works after failure. |
| Retained rendering | Seed `0x17235001`, maximum views/widgets, all six gauge styles, four orientations and wrapped history. Ninety-six view/unit/overlay/sample changes exercise both buffers against a poisoned fresh full repaint. Warning frames must actually occur. |

Failures include test/case context; these are repeatable generated cases rather
than an unbounded fuzzing campaign. Whole/incremental EASA validation shares
validator code and is a consistency check, not an independent acceptance oracle.
The retained-render oracle independently forces a full repaint but shares drawing
primitives, so it does not prove the primitives visually correct.

Focused reproduction from the repository root:

```text
cargo test --locked -p dashboard-config --features json --test generated_formats
cargo test --locked -p dashboard-config-tools --test generated_formats
cargo test --locked -p dashboard-startup-tools --test generated_formats
cargo test --locked --manifest-path configurator/Cargo.toml --test generated_editor_state
cargo test --locked --manifest-path host/Cargo.toml --test generated_runtime --test generated_transport --test generated_render
```

`DASHBOARD_FORMAT_CASES=1024` ran successfully for the format suites locally.
The tests bound that environment override, and CI uses their default budgets.
Existing control, warning, LED, reset, startup, transport and orientation suites
remain in the ordinary workspace/host matrix.

### Mutation sensitivity

In an isolated source copy, removing the token/phase guard from
`FrameLifecycle::complete` caused the generated wrong-completion test to fail
(exit 101). Restoring the guard passed (exit 0). The real working-tree pipeline
source was unchanged. This checks one representative guard's test sensitivity;
it is not a mutation score or an instruction-level proof.

## Native helper recovery

The helper's 80-test local run and strict Clippy passed. New cases use actual
temporary files and the production HTTP dispatch/storage paths:

- A complete valid pending flash record cannot publish a successful outcome.
  Publishing the same bytes through the normal record location is a positive control.
- Corrupt latest journal data cannot fall back to an older success.
- A failed publication rename preserves the pending file and unknown outcome.
- Torn pending build metadata is quarantined while artifact bytes are retained.
- An upload delivering bytes and then `ConnectionReset` preserves the prior
  accepted project, creates no job, and allows an explicit successful retry.

Parallel temporary-file fixtures use unique paths. Existing prerequisite,
cancellation, image admission and restart tests remain active. No USB transfer,
OS crash, disk exhaustion or physical power cut was performed.

## Splash integration and visual evidence

All ten themes use the same Rust renderer in Studio and firmware. Elapsed-time
animation, reduced-motion stills, non-looping final frames, startup controls,
warning policy and custom artwork remain intact. Custom retains wire tag 5;
new tags 6–10 extend EASP v2, while v1 remains frozen at 0–5. Tests enumerate every
byte-valued tag for both versions. The UI uses explicit tags instead of picker
positions.

The visual tests sample 55 times per theme/geometry, check quiet edges and the
invariant recovery footer, and exercise loop/seek/final/reduced-motion behavior at
large timestamps. Source bounds and actual pixels received independent review.
These are not full-image golden tests or exhaustive visual proofs.

The [gallery](../configurator/startup-gallery.md) contains 20 native PNGs and 20
looping GIFs, plus reproducible capture commands and source/artifact hashes. GIF
packaging verifies exact RGB pixels and all forty 50 ms samples, allowing merged
identical frames without changing the two-second timeline. Actual Studio
[workflow captures](../configurator/gallery.md#capture-record) were refreshed in
an isolated origin, including portrait and the brainlet turbo-snail.

The richer illustration adds drawing work. A redundant full-buffer clear found
during review was removed. The exporter records 30 samples of 100 frames per
theme/geometry; before/after captures had concurrent host activity, so this pass
makes no controlled speedup or target-FPS claim. Existing Criterion workloads
passed their smoke run. Two fixed RGB565 buffers and resource limits are unchanged.

## Integrated verification and image limits

The integration run passed root workspace tests (233), JSON configuration tests
(113), full-outline core tests (46), host tests with each outline selection
(521 per selection), configurator native tests (406), and helper tests (80).
After the final tenth-theme addition, 12 focused configuration/generated tests
and 23 host visual/orientation/state/transport/render tests passed again. These
counts describe those local runs; subsequent CI logs are authoritative for the
exact published revision.

Formatting and strict nightly Clippy passed in affected workspaces, including
native, Wasm browser-feature and ARM paths. Shared core compiled for Wasm and
`thumbv8m.main-none-eabihf`; firmware retains `-C target-cpu=cortex-m33`. The pin
was `nightly-2026-10-16`, rustc `1.101.0-nightly (4ebd25825 2026-10-15)`, LLVM 23.1.4.

The final vehicle image with `longan-rx12,frame-trace,te-trace` passed practical
linked-image, configuration, memory, reviewed SRAM-code and corrupted-artifact
checks. ELF SHA256:
`a759719c43607469f75d7ee90a2cf0741a4479515f0d7b7d3af24311ffe08970`.

| Image property | Result |
| --- | ---: |
| Static RAM | 458,680 bytes |
| Remaining headroom | 65,608 bytes; minimum 65,536 |
| Core 0 reserve | 8,192 bytes |
| Core 1 stack / reserve | 32,768 / 4,096 bytes |
| Conservative boot / constructor bound | 3,328 / 28,008 bytes |
| Conservative dashboard / acquisition runtime bound | 29,216 / 17,312 bytes |

This particular traced image has only 72 bytes above the existing minimum
headroom gate. It passed without lowering the gate; future static growth must
still be measured. Named stack bounds carry the existing source-reviewed
assumptions and do not measure physical stack high-water.

Reproduce from `firmware/`, choosing a fresh capture name:

```text
python scripts/capture_target_bench.py <fresh-name> --firmware --image-gate practical --features longan-rx12,frame-trace,te-trace
```

Normal production Studio main Wasm SHA256:
`edc1cbea5eba9c69a8c134bb14837ea570c1fd403f9cc7ea578be3d7684202a2`
(5,183,086 bytes). The final browser-test Wasm was 5,349,827 bytes,
SHA256 `c4c044e92d1e31f73bbcce8d9d8d8ac6d9d99d7b797b487b362f6d0f172c9b1c`.
The production Wasm files omit the test-driver and injected-failure markers.
This is compiled exclusion evidence, not an attributed before/after size saving.

## Still requires physical or separate qualification

ECU support, CAN wiring, voltage/clock margins, display scanout/tearing/FPS,
physical controls and LEDs, stack high-water, flash power-cut behavior and USB
interruption remain in the [Pico 2/DNUC runbook](pico2-dnuc-runbook.md). The
instruction-level proof remains separate unfinished research. No hardware was
enumerated, flashed or sent diagnostic requests during this pass.
