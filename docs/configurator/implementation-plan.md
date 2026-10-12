# Configurable views implementation plan

> Execute in reviewable stages with the test-driven-development and requesting-code-review skills. Track live progress in uncommitted TODO.md. User authorized implementation after end-to-end plan review and continued work on master.

**Goal:** Deliver a Rust browser editor and bounded runtime configuration that
control named Pico dashboard layouts, presentation and warnings.

**Architecture:** Shared no_std schema/validation and dashboard core; Leptos
CSR browser shell; JSON editing plus compact checked Postcard runtime envelope;
one immutable startup config shared by acquisition/rendering owners.

**Tech stack:** Pinned latest nightly, current stable Leptos/web-sys/Serde/Postcard,
existing embedded-graphics/heapless, Trunk, existing Criterion/host/ELF gates.

## Global constraints

The companion design and schema-v1 contract are normative:4x2, spans1x1/2x1/2x2, max8views/64widgets,
max8visible, names14/labels12-or24/messages20ASCII, decoded config8192B,
binary16KiB, import128KiB, core0headroom64KiB, no runtime heap. Preserve all
source/context/staleness, affine framebuffer, flash-owner and input contracts.
No handwritten JavaScript, no guessed live ECU support, no audit/TODO commits.
Capture honest host performance independently of device/optical validation.

## Stage 0: evidence and baseline

Files: this design/plan; `host/baselines/configurator_before_{a,b}`;
ignored `firmware/target/configurator-audit` reports and logs.

- [ ] Review every user requirement against the design, source research and
  state transitions. Resolve the confirmed grid/Wasm/embedding choices.
- [ ] Capture two 75-case baselines on unchanged e710216 with
  `python scripts/capture_benchmark.py configurator_before_a --runner-id ea888-workstation-01`
  and the corresponding `_b` run from `host`; 30 samples each, archive exact
  source and compiler, inspect variance with the existing comparison script.
  Retain noisy runs and repeat with measured child-process CPU affinity; do not
  attribute same-source timing shifts to a patch or weaken the comparison gate.
- [ ] Independently review the design for ownership, parser, UI/state,
  decoding/provenance and budget gaps. Fix material findings before code.
- [ ] Commit the plan and reproducible baseline in separate small commits.

## Stage 1: shared dashboard core without behavior changes

Create `crates/dashboard-core/Cargo.toml` and `src/lib.rs`. Move the pure modules
currently path-imported by `host/src/lib.rs` into that crate, preserving their
module paths. Keep GPIO/clock/flash hardware drivers, tasks, CPU sampling and
hardware startup in firmware. Firmware library and host re-export the core.
Forward `simple-outline`; keep firmware target features on the hardware crate.

Modify `firmware/pico2/Cargo.toml`, `firmware/pico2/src/lib.rs`, `host/Cargo.toml`,
`host/src/lib.rs` and `host/scripts/capture_benchmark.py` to capture new shared
sources/manifests. Update `firmware/scripts/capture_target_bench.py` source
inventory and `check_logging_policy.py` scan roots, with negative tests proving
that moved files remain checked. Reconcile documentation links to moved files.

- [ ] Add the portable compile contract to the existing tests/builds; first
  demonstrate that a browser-target core cannot be imported before extraction.
- [ ] Move exact pure source bytes, add the no_std crate and re-exports.
- [ ] Run host tests both outlines, strict Clippy/formatting, ARM check/release
  and Wasm core check. Retain existing pixel comparisons and ownership failures.
- [ ] Capture unchanged scene workloads; investigate >20% repeated regressions.
  Audit boundaries and commit `refactor(core): share portable dashboard modules`.

## Stage 2: bounded schema, catalogue, validation and codec

Create `crates/dashboard-config/{Cargo.toml,src/lib.rs,src/model.rs,src/validate.rs,
src/codec.rs,src/catalog.rs,src/fuel.rs,src/defaults.rs,tests/contracts.rs}`.
Add shared dependency to the core, host and later editor. Catalogue source
records include revision/access/decoder semantics, including unsupported leads.

Public contract:

```rust
pub const MAX_VIEWS: usize = 8;
pub const MAX_WIDGETS: usize = 64;
pub const MAX_FILE_BYTES: usize = 16 * 1024;
pub fn validate(config: Config) -> Result<ValidatedConfig, ValidationError>;
pub fn decode(bytes: &[u8]) -> Result<ValidatedConfig, ConfigError>;
pub fn encode<'a>(config: &ValidatedConfig, out: &'a mut [u8])
    -> Result<&'a [u8], ConfigError>;
```

`Config` is the closed model enumerated in `schema-v1.md`.
`ValidatedConfig` has private `config: Config` and `stoich: f32` fields. Its
read-only `config(&self) -> &Config` and `stoich(&self) -> f32` methods expose
validated data; no mutable accessor can bypass validation. `default_config()`
returns the complete reviewed editable default. Error types carry a closed
error code, optional view/widget ID and field enum, with no allocated strings.

Concrete first acceptance contract (expand into independent cases):

```rust
let valid = validate(default_config()).unwrap();
let mut bytes = [0; MAX_FILE_BYTES];
let encoded = encode(&valid, &mut bytes).unwrap();
assert_eq!(decode(encoded).unwrap().config(), valid.config());
for cut in 0..encoded.len() { assert!(decode(&encoded[..cut]).is_err()); }
let mut corrupt = encoded.to_vec();
*corrupt.last_mut().unwrap() ^= 1;
assert!(decode(&corrupt).is_err());
```

- [ ] Establish RED for codec/geometry/numeric contracts before implementation.
- [ ] Implement fixed bounds, checked arithmetic and typed errors with useful
  view/widget/field locations. Validate printable glyphs and actual geometry.
- [ ] Test every edge of bounds, duplicate IDs, references, overlap, all spans,
  capacity+1, bad enum/version/length/CRC, trailing bytes, nonfinite values,
  hysteresis/dwell, graph bounds, source edits and unsupported source status.
  Use an independent CRC known answer, a golden envelope and malformed bodies
  with recomputed valid checksums so rejection reaches the semantic validator.
- [ ] Test fuel0/100/endpoints, mass versus volume, positive densities,
  percent bounds, monotonic values and independently calculated examples.
- [ ] Fuzz bounded random byte inputs and round trips; check size_of on host/ARM.
  Benchmark default/max decode/validate/encode and malformed fast rejection.
- [ ] Review exact new dependency features/source, run no_std ARM/Wasm checks,
  audit and commit `feat(config): add bounded dashboard configuration`.

## Stage 3: runtime embedding and immutable ownership

Modify `firmware/pico2/build.rs`, `src/main.rs`, `src/tasks/acquisition.rs`,
shared `diagnostics/service.rs` and `diagnostics/session.rs`, profile/build
script and CI. Add a default exported config fixture and the
configuration path option to the existing PowerShell workflow. Add a small
host command to validate/compile JSON and binary files through the shared crate.

- [ ] Test missing file, invalid payload, changed file rebuild, default selection
  and bounded runtime fallback. Build must reject a supplied invalid file.
- [ ] Embed bytes in flash with Cargo rerun directives; parse once into a
  StaticCell before core1 starts, then pass shared immutable references.
  Read/validate/copy the same bytes into OUT_DIR and embed that snapshot; capture
  the selected file/hash, track environment selection and changed input files.
- [ ] Thread that reference through both Demo construction and
  `Vehicle -> Session -> AcquisitionModel`; cover reconnect/recovery,
  maintenance restart and statistics resets without losing configuration.
- [ ] Runtime error logs a config fault, selects a reviewed complete default
  and preserves transport unqualified state. No partial apply or new journal.
- [ ] Link stock/280Demo/serial profiles, inspect config placement, static sizes,
  stack headroom, exact SRAM helper/startup/IRQ and18corruptELF gates.
  Raise `check_memory_layout.py` acceptance to65536B and test the new boundary;
  retain the separate linker hard floor. Inspect parser temporaries/task frames.
- [ ] Review bootstrap/publication/maintenance ownership, then commit.

## Stage 4: variable widgets and retained rendering

Modify shared `dashboard.rs`, `widgets/cell_text.rs`, `widgets/cells/*`,
`widgets/header.rs`, `widgets/primitives.rs` and their scene/pixel tests.
Extract small `layout`, `widget_text` and `configured_scene` units where needed.
Keep one draw implementation for host, firmware and browser.

- [ ] RED: all three spans, moved widgets, holes, named header, custom label,
  toggled graph/stats/AFR and unit-dependent strings render expected pixels.
- [ ] Implement validated rectangles, common clipping/text measurement and
  shared formatting. The retained key includes every visible option/value.
- [ ] Both physical buffers must equal full redraw for view changes, field
  changes, source quality changes, overlays and AFR-only rounding differences.
- [ ] Test maximum names/status/FPS/messages, signed/large numbers, two-row
  graphs, borders and neighboring pixels; run both outlines and timed replays.
- [ ] Audit geometry/config identity against transfer lifecycle and commit.

## Stage 5: configurable warnings and named-view state

Modify shared `alarms.rs`, `acquisition.rs`, `dashboard.rs`, popup/state modules,
`status_led.rs`; firmware `main.rs` and acquisition task. Add pure view-selection
and warning-page owners. Reuse the current alarm evaluator for per-widget rules.

- [ ] RED: per-instance enable/threshold/message/dwell; repeated frames cannot
  advance warnings; fresh/invalid/context/sequence edge cases retain guards.
- [ ] Evaluate all configured rules on accepted samples/faults before Watch
  publication. Keep channel history/polling shared and view-independent.
  Explicitly separate RuleIndex from SensorId; include rule63/channel0 and
  duplicate-channel rules with different limits in the regression suite.
- [ ] Add cycle and three-line warning pages with monotonic time, stable order,
  chosen scope, active-set changes, stale removal and feedback precedence.
- [ ] Add modulo view selection, one-view no-op, named feedback, page-return
  preservation, simultaneous motion/click, overflow deltas and Off cancellation.
  Route Y before encoder; apply motion before clicks and let click feedback win.
  Exclude invisible/PWM-zero and modulo-no-op changes from visible input counts.
- [ ] Preserve byte-compatible settings journal, bar/psi and reset generation;
  prove old completions cannot apply a new view's warning/LED/Off state.
- [ ] Test serial/overlap/unchanged/maintenance paths and cache repair. Benchmark
  max64rules + snapshot costs and verify ELF memory/stack budgets. Audit/commit.

## Stage 6: Rust editor state and validation workflow

Create `configurator/{Cargo.toml,rust-toolchain.toml,Trunk.toml,index.html,
src/lib.rs,src/main.rs,src/editor.rs,src/storage.rs,src/io.rs,tests/editor.rs}`.
Use the shared schema and renderer directly. Host-test the pure editing reducer.

- [ ] RED: add/move/resize/delete/duplicate widgets and views, collision failure
  rollback, imported replacements, dangling drag cancellation, undo/redo bounds.
- [ ] Implement transactional edits and import; invalid fields disable export;
  last valid preview cannot be mistaken for an accepted invalid draft.
  Follow the design's document revision, late import/save, delete/duplicate,
  selection and raw-field policies; test each before wiring browser callbacks.
- [ ] Use typed strict JSON with unknown/duplicate-field rejection. Validate
  nonfinite values before serialization, which otherwise may become null.
- [ ] Save validated versioned drafts only on successful local-storage writes;
  surface denied/quota/corrupt storage and keep file export usable.
- [ ] Round-trip every option through JSON/binary/editor/firmware decode.
  Audit reducers and commit before adding complex UI interactions.

## Stage 7: browser UI and exact pixel preview

Create focused `configurator/src/components/{studio,library,canvas,inspector,
views,warnings,fuel,sources,export}.rs` and `configurator/public/studio.css`.
Implement the companion design's palette/type/layout; no custom JavaScript.

- [ ] Build the initial shell and exact renderer preview with named view tabs,
  library and inspector; compile production Wasm before expanding interactions.
  Convert panel-order big-endian RGB565 bytes explicitly to canvas RGBA; test
  pure red/green/blue/black/white and exact rendered fixture colors.
- [ ] Bind Rust pointer events with capture/cancel and checked coordinate maps;
  keyboard/click alternatives exercise the same tested editing reducer.
- [ ] Add all validated display/alert/fuel/source settings and import/export,
  visible character limits, scope/mode choice and explicit candidate badges.
- [ ] Preserve bar/psi preview toggle, synthetic demo controls, warning preview
  and selected view/draft persistence. Respect reduced motion and focus states.
- [ ] Browser-test real drag/drop, resize, invalid edits, file round trips,
  reload/local-storage failure, pointer cancel, keyboard and narrow viewport.
  Inspect screenshots at desktop/narrow sizes and fix overlaps/contrast.
- [ ] Record release Wasm/asset bytes and actual interaction/render latency;
  require two warmed max-document traces with p95 input-to-preview<=50ms and
  no attributable editor task>100ms. Audit the compiled page/state before commit.

## Stage 8: final regression, evidence and delivery

- [ ] Extend reproducible benchmark capture to all shared/config sources and
  fixtures. Compare identical legacy workloads and report newly changed
  workloads separately. Capture two final runs; investigate repeated>20% costs.
- [ ] Full host/core/config/editor tests and strict lints; deterministic malformed
  corpus; browser workflow; both outlines; all firmware CI profiles and guards.
- [ ] Check exact default and worst config RAM/flash/layout on target, minimum
  64KiB core0 headroom; normal+optimized corrupted ELF checks. No claim of
  measured hardware cycles/optical FPS from host timing.
- [ ] Independent end-to-end review of schema->export->boot->acquisition->view
  ->render->completion->LED/backlight and error/recovery paths; fix findings.
- [ ] Document source catalogue, limits, build/export examples and device test
  steps. Commit often, push reviewed master stages and inspect exact CI.
- [ ] Save final command/hash/results and resume state in ignored evidence and
  TODO.md. Distinguish ready-to-flash software from physically observed results.
