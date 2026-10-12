# Control Mappings Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox syntax for tracking.

**Goal:** Deliver the approved configurable physical controls with protected recovery and browser/firmware parity.

**Architecture:** Compact validated mappings live in dashboard-config. Pure gesture/action code lives in dashboard-core and is consumed by firmware and browser simulation. Hardware ownership and frame-completion guards remain in their existing adapters.

**Tech Stack:** Rust nightly, no_std, serde/postcard, Embassy RP2350, Leptos/Wasm.

## Global Constraints

- Read `docs/configurator/control-mapping-design.md`; its default table and state rules are the approved concrete design.
- Work on the existing master checkout as requested. Never reset/clean. Commit only task source/tests/product docs, using conventional subjects, no AI attribution/em dashes. Keep TODO.md and target/audit evidence uncommitted.
- Keep 320x240 / 4x2 layout, 8 views / 64 widgets, 16 KiB EAV / 128 KiB JSON and 10,240-byte ARM config guard. Do not relax resource assertions without measured evidence.
- Hardware profile remains 280/70 at 1.30 V. No flashing. No new dependency is needed for the controls stage.
- Record meaningful RED/GREEN evidence and self-review in the task report. Run focused tests while editing, broader affected tests once before commit.

### Task 1: Compact control catalogue and independent validation

**Files:** Create `crates/dashboard-config/src/controls.rs`, `crates/dashboard-config/tests/controls.rs`; modify `crates/dashboard-config/src/lib.rs` to export the module. Do not yet add fields to Config or change VERSION, codec, firmware or UI.

**Interfaces:** Expose `ControlPage`, `ButtonId`, `ControlAction`, `RotationAction`, `ButtonBinding`, `ScreenBindings`, `ControlMap` with the exact variants/fields/defaults in the design. Expose `ControlAction::allowed_on(self, page: ControlPage) -> bool`, `RotationAction::allowed_on(self, page: ControlPage) -> bool`, and `validate_controls(&ControlMap) -> Result<(), ControlError>`.

`ControlError` is an enum with `HoldDuration`, `ReservedRecovery { page: ControlPage }`, `IncompatibleButton { page: ControlPage, button: ButtonId, hold: bool }`, `IncompatibleEncoderClick { page: ControlPage }`, `IncompatibleEncoderRotation { page: ControlPage }`, `UnreachableScreen { page: ControlPage }`, `MissingViewNavigation`. Errors must identify the actual failing binding. Derive Copy/Debug/PartialEq/Eq and serde on configuration enums/structs; deny unknown struct fields. No heap allocation. Derive Default on ControlPage (Dashboard) so core can later re-export it.

- [ ] Write independent tests for the default table and validation before the module implementation. For example:

```rust
use dashboard_config::*;
#[test]
fn default_onboard_routes_and_encoder_bindings_are_valid() {
    let map = ControlMap::default();
    assert_eq!(map.hold_ms, 800);
    assert_eq!(map.screens[0].buttons[0], ButtonBinding {
        tap: ControlAction::ToggleBoostUnits, hold: ControlAction::PreviousView,
    });
    assert_eq!(map.screens[0].buttons[3].hold, ControlAction::None);
    assert_eq!(map.screens[2].encoder_click, ControlAction::None);
    assert_eq!(validate_controls(&map), Ok(()));
}
#[test]
fn encoder_cannot_replace_required_onboard_view_navigation() {
    let mut map = ControlMap::default();
    map.screens[0].buttons[0].hold = ControlAction::None;
    map.screens[0].buttons[1].hold = ControlAction::None;
    assert_eq!(validate_controls(&map), Err(ControlError::MissingViewNavigation));
}
```

- [ ] Run `cargo test -p dashboard-config --test controls --locked`; record the expected missing API failure.
- [ ] Implement the finite catalogue, defaults and validator. Compatibility is an explicit match over actions/screens; reachability is bounded over three nodes, not recursive. Add recovery edges from every node to Dashboard before testing reachability from Dashboard. Match enum casts only for validated indexes. Keep enum variant order stable.

```rust
// Reachability sketch: each iteration propagates all currently reachable
// screens; three iterations suffice for the complete three-node graph.
let mut reachable = 1u8 << ControlPage::Dashboard.index();
for _ in 0..3 {
    for page in ControlPage::ALL {
        if reachable & (1 << page.index()) != 0 {
            // Enumerate onboard tap/hold navigation only, plus recovery.
        }
    }
}
```

- [ ] Add the full independent action/screen compatibility table, rotation table, 399/400/800/1500/1501 hold boundaries, reserved Y hold on each screen, absent/one-way/encoder-only routes, alternate valid direct routes, one-direction wrapped-view fallback, serde unknown enum/field rejection under JSON feature, and compact size bounds (`ControlMap` <= 64 bytes). Assert actual outputs rather than duplicating the implementation's traversal.
- [ ] Run `cargo test -p dashboard-config --locked` and `cargo test -p dashboard-config --features json --locked`, fmt and strict config Clippy. Append outputs to the ignored report. Review the diff against the design, then commit `feat(config): define validated per-screen control mappings`.

### Task 2: Shared gestures, deterministic dispatch and recovery

**Files:** Create `crates/dashboard-core/src/controls.rs` and `host/tests/control_gestures.rs`; modify `crates/dashboard-core/src/lib.rs`, `crates/dashboard-core/src/state/pages.rs`, `crates/dashboard-core/src/backlight.rs` and focused backlight tests. Make `ControlAction::destination` in `crates/dashboard-config/src/controls.rs` public so the runtime reuses the validated catalogue's navigation mapping.

**Interfaces:** Consume Task 1's validated map. Re-export `ControlPage as Page` from existing pages module to retain downstream imports. Add `ControlGestures::new()`, `reset()`, and `update(&mut self, map: &ControlMap, page: ControlPage, pressed: [bool; 4], now_ms: u64) -> ControlBatch`. Define `ControlEvent { pub button: ButtonId, pub action: ControlAction, pub observation: Observation }`; `ControlBatch { pub actions: heapless::Vec<ControlEvent, 4>, pub recovery: Option<Observation>, pub discarded: u32 }`. `ControlBatch::transition(&self, page: ControlPage) -> Option<ControlPage>` resolves the surviving actual page transition (ignoring self-navigation). Batch precedence is resolved inside update: recovery removes/counts all actions; otherwise the first actual transition removes/counts all other actions. Add `Backlight::wake_full()`, which cancels every pending Off state and restores Full, including restore brightness. Frame completion with an older token after wake must not turn it off. Do not yet modify firmware, Config, codec or the editor.

- [ ] Add failing time traces: bootstrap released at0, press A at100, accept150, release200/250 -> exactly one tap; hold A through899/900 -> exactly one hold at900, release -> no tap; boot held Y -> no recovery until a stable release and new hold. For Y press first observed100, recovery fires at2100, never2099. Include bouncing candidate transitions, same timestamp, regression and u64 boundary. Write test helpers that replay explicit `(time, levels)` traces and assert semantic actions, for example:

```rust
let map = ControlMap::default();
let mut gestures = ControlGestures::new();
assert!(gestures.update(&map, Page::Dashboard, [false; 4], 0).actions.is_empty());
gestures.update(&map, Page::Dashboard, [true, false, false, false], 100);
gestures.update(&map, Page::Dashboard, [true, false, false, false], 150);
assert!(gestures.update(&map, Page::Dashboard, [true, false, false, false], 899).actions.is_empty());
let held = gestures.update(&map, Page::Dashboard, [true, false, false, false], 900);
assert_eq!(held.actions[0].action, ControlAction::PreviousView);
assert_eq!(held.actions[0].observation.observed_us, 100_000);
```
- [ ] Run `cargo test --manifest-path host/Cargo.toml --locked --test control_gestures --test backlight_controls` and save RED output. Implement fixed per-button state with release arming, press candidate time, stable debounce and a single fired flag. On page changes, disarm held keys and suppress old-screen release. Ignore regressed times without advancing state. Recovery/page changes have explicit deterministic precedence. Recovery disarms all held keys even on Dashboard. Ordinary None-hold retains tap on release; holds require the current sampled level still pressed (release observed after a stall must not invent recovery). Clamp invalid ordinary hold duration to the safe documented bounds in this defensive pure helper, ignore incompatible raw actions and count them, and make overflow converting milliseconds to observation microseconds fail without action or panic. Firmware will still supply a validated map.
- [ ] Test every compatible action gesture, simultaneous recovery vs reset/units and page change vs actions, fixed Y behavior despite a raw invalid map passed to the pure helper, maintenance reset, multi-second gaps, ordinary hold bound saturation and stale Off token completion after wake. Encoder-batch suppression is an integration test in Task4 (this module does not own the encoder mailbox). Test old button releases after a page switch, same-page recovery while another key is still held, no-hold long tap, release before threshold but debounce after threshold, and raw release observed only after a long pause.
- [ ] Run host control/backlight/input/navigation tests, core/config suites, formatting and strict affected Clippy. Commit `feat(controls): add shared hold gestures and protected recovery` after reviewable self-check.

### Task 3: Shared action session for firmware and simulation

**Files:** Create `crates/dashboard-core/src/control_session.rs`, `host/tests/control_session.rs`; export from core lib. The session is a single owner of current UI control state, not an executor or another shared global.

**Interfaces:** Consume ControlMap, ControlBatch, EncoderBatch and the existing ViewSelection, Backlight, FpsMode, Popup and LogStore. Expose `ControlSession::new(config: &ValidatedConfig, fps: FpsMode, boost_psi: bool) -> Self`. Its state contains page, view selection, backlight, FPS mode, boost units, active popup and log anchor, with accessors and explicit mutable backlight access for existing frame-completion calls. Expose `apply<M: RawMutex>(&mut self, config: &ValidatedConfig, map: &ControlMap, buttons: ControlBatch, encoder: EncoderBatch, logs: &LogStore<M>, now_ms: u64) -> ControlEffects`, `expire_feedback(now_ms)` and `statistics_acknowledged(now_ms)`. `ControlEffects` contains page_changed/view_changed/units_changed/fps_changed/recovered/reset_requested booleans, `observations` and `reset_observation` Options of Observation, and a saturating `ignored: u32` count. No GPIO, clocks, allocation, waiting, async I/O or acquisition ownership enters this module. LogStore operations are existing nonblocking bounded operations; no lock crosses this call.

- [ ] Add failing action traces for each default binding and custom compatible encoder click. Test protected recovery from Full/Dim/Off, every pending shutdown phase and all pages. For example:

```rust
let config = dashboard_config::validate(dashboard_config::default_config()).unwrap();
let mut session = ControlSession::new(&config, FpsMode::Off, false);
// Test helpers create timestamped ControlBatch/EncoderBatch, never GPIO mocks.
apply_action(&mut session, &config, ControlAction::CycleBrightness, 100);
assert_eq!(session.backlight().compare(), 5000);
recover(&mut session, &config, 200);
assert_eq!(session.page(), Page::Dashboard);
assert_eq!(session.backlight().compare(), 10000);
assert!(session.popup().is_none());
```

Define the named test helpers in the test file by constructing the public batch types and a real LogStore with a host-safe mutex. Run `cargo test --manifest-path host/Cargo.toml --locked --test control_session` for RED.
- [ ] Implement priority across the complete input transaction: recovery first; first actual button transition next; actual encoder-click transition next; remaining button actions in A/B/X/Y order followed by encoder motion and clicks. A transition discards all remaining input. Navigation from an encoder batch invokes one transition against the captured old screen and counts extra click events as discarded; it never chains mappings from newly selected screens. The session ignores time regressions and never reinterprets previous-screen actions on a new screen.
- [ ] Reuse ViewSelection, Backlight and ControlAction::destination. Navigation interrupts feedback, clears popup and resets log anchor. Recovery calls wake_full, clears feedback and log anchor, selects Dashboard and preserves view/FPS/unit choices. View actions work only on Dashboard; brightness actions dispatch to existing exact preset/legacy algorithms; ordinary popup feedback interrupts a pending Dashboard shutdown. Statistics reset returns its observation separately and waits for the adapter's acknowledgment before showing Reset. Acknowledgments while off Dashboard show no popup.
- [ ] Apply Logs older as delta -1 and newer as +1 through LogStore::scroll; LogsLive restores None. Empty or busy history must not invent movement. Encoder delta remains bounded +/-128 and presses at most8, with no unbounded loop on malformed input. Reject invalid action/page combinations even for a raw map. Validate observation ordering within each event. `now_ms` is the consumption millisecond bucket: an observation is not future-dated when `accepted_us / 1000 <= now_ms`; require `observed_us <= accepted_us`. Cover +999 us within the current bucket as accepted and the next millisecond as rejected, including recovery. Preserve checked time-conversion overflow handling. Aggregate accepted batch observations using earliest observed/latest accepted/count sum without dropping legitimate motion/click events merely because their timestamps arrive in opposite order; preserve checked overflow and count discarded observations. A logical action that changes no visible state is ignored consistently (single-view rotation, saturated brightness, unchanged log anchor). Do not report a reset applied before acknowledgment.
- [ ] Test every compatible action, None actions, custom encoder navigation, mixed movement/clicks, malformed/out-of-order observations, maximum batch, saturation, two-frame Off completion, popup expiration, reset acknowledgment, and three-screen round trips preserving selected view. Run focused controls/backlight/input/log/navigation suites, core/config tests and strict affected Clippy; then commit `refactor(controls): share action state across device and simulator`.

### Task 4: Versioned control configuration and frozen migrations

**Files:** Modify `crates/dashboard-config/src/{model,defaults,validate,codec,legacy}.rs` and config tests; add `crates/dashboard-config/tests/fixtures/v2-default.{json,eav}` from the existing v2 defaults; regenerate `configs/default.{json,eav}`. Adapt editor exhaustive error matches and existing version tests only as needed to keep all consumers building. No firmware action dispatch or new UI controls in this task.

**Interfaces:** Add `controls: ControlMap` to Config, bump VERSION to3, and call `validate_controls` from shared validation. Represent the exact cause as `Field::Controls(ControlError)` so screen/button/gesture identity survives to the editor; keep ValidationError Copy/Eq. Map hold duration to Bounds, incompatible/reserved bindings to Incompatible, missing routes to Reference. `default_config` uses ControlMap::default. Host JSON-enabled imports accept frozen v1/v2 JSON and EAV and fill default controls. The no-JSON firmware decoder accepts only v3 and preserves logged boot fallback for older/malformed images.

- [ ] Preserve the v2 fixture bytes before changing types. Add tests requiring both old versions to upgrade to current, preserve views/widget values/gauges/colours and initialize exact default controls. Test current custom controls round trips, missing/unknown fields, incompatible bindings, broken onboard routes and reserved recovery. A current v3 document missing controls is invalid rather than a silently defaulted partial document.

```rust
#[test]
fn current_configuration_requires_recovery_safe_controls() {
    let mut config = dashboard_config::default_config();
    config.controls.screens[0].buttons[0].hold = dashboard_config::ControlAction::None;
    config.controls.screens[0].buttons[1].hold = dashboard_config::ControlAction::None;
    let error = dashboard_config::validate(config).unwrap_err();
    assert_eq!(error.field, dashboard_config::Field::Controls(
        dashboard_config::ControlError::MissingViewNavigation));
}
```

- [ ] Run `cargo test -p dashboard-config --locked --features json` for RED, then implement the field, errors and migrations. Freeze old display/warning/presentation/gauge wire shapes that later approved features will extend; do not deserialize an old payload into a growing current struct. Keep strict version/header matching, CRC, bounded length, no trailing bytes and canonical Postcard checks. Test malformed legacy documents with recalculated CRC so version/payload validation is actually reached. Confirm no-JSON builds reject both old EAV fixtures.
- [ ] Regenerate defaults with `cargo run -p dashboard-config-tools --bin dashboard-config -- default configs/default.json` and the same command for `configs/default.eav`. Verify the tools import/compile path, editor load/migration tests and both config feature modes. Run workspace tests/strict affected Clippy and `cargo check -p dashboard-core --locked --target thumbv8m.main-none-eabihf` plus Wasm; preserve the10,240-byte target guard. Report actual file sizes and target type size evidence when available. Commit `feat(config): persist controls with strict legacy migration`.

### Task 5: Firmware adapter and completed-frame integration

**Files:** Modify firmware `pico2/src/main.rs` and its input/button adapters, related host integration tests and control hardware/product docs. Remove only the duplicated hardcoded button action adapter once its replacement is covered. Keep the encoder driver's debouncer if still used.

**Interfaces:** Firmware consumes Config.controls, ControlGestures and ControlSession. Main owns GPIO/PWM, immutable boot config, layout, frame-completion/leases, settings policy, reset-generation requests and InputTracker; ControlSession exclusively owns page/view/backlight/FPS/unit/popup/log-anchor state. Its accessors from Task3 are `page`, `view_selection`, `view_id`, `backlight`, `backlight_mut`, `fps`, `boost_psi`, `popup`, `log_anchor`, `apply`, `expire_feedback`, and `statistics_acknowledged`.

- [ ] Add regression traces for a page-changing button with same-batch encoder motion/clicks, old-page held releases, recovery while already Dashboard/Off, reset generation acknowledgment after page navigation, and maintenance suppression. For a transition transaction, assert destination selected once, brightness/view unchanged by stale encoder events, ignored counts account for the discarded events, and no reset popup before acknowledgment. Reuse the actual pure session rather than a duplicate mock dispatcher.
- [ ] Sample A/B/X/Y into `[bool;4]` in that order, take the encoder batch, then capture monotonic input time and call the shared session once. Sampling the time after the batch avoids rejecting concurrently accepted encoder observations as future-dated. Do not dispatch encoder events using the newly changed screen. Route effects to reset requests and observation accounting, rebuild layout only on a view change, reset the FPS measurement window on a page change, and preserve warning-pager pause semantics. Log page/view/unit/FPS/recovery transitions only when they occur.

```rust
let controls = &dashboard_configuration.config.config().controls;
let pressed = [btn_a.is_low(), btn_b.is_low(), btn_x.is_low(), btn_y.is_low()];
let encoder = ENCODER_EVENTS.take_observed();
let now_ms = Instant::now().as_millis();
let buttons = gestures.update(controls, session.page(),
    pressed, now_ms);
let effects = session.apply(&dashboard_configuration.config, controls, buttons,
    encoder, &crate::profiling::LOGS, now_ms);
```

- [ ] Feed actual completion tokens to `session.backlight_mut()` at the existing lease-completion sites; keep LED presentation and PWM tied to completed frames. Save the session's FPS/unit preferences through existing journal policy. Call `statistics_acknowledged` only when Core1's requested generation is observed; discard/reset pending observations under existing maintenance rules. On maintenance completion reset gestures and the encoder event boundary so held/reconnected inputs cannot fire old actions.
- [ ] Run the affected host input/session/reset/backlight/navigation tests and both outlines once. Capture Demo280/70 and Vehicle maximum configuration ELF builds through the existing artifact capture workflow, verify memory/flash helpers and stack gates, and inspect changed adapter state transitions. Run strict firmware Clippy/format. Keep config/static RAM limits unchanged unless actual evidence requires a separately reviewed design. Update docs to describe tap-on-release, holds and protected recovery. Commit `feat(firmware): apply configurable controls and recovery`.

### Task 6: Repairable control editor and simulator control foundation

**Files:** Modify editor action/reducer and UI fields; create focused `configurator/src/ui/controls.rs` and simulator control module/tests; use existing file layout conventions for exports.

**Interfaces:** A Device controls panel edits Config.controls transactionally; a simulator adapter consumes ControlGestures and shared action dispatch. Existing draft/accepted/config version/storage contracts remain authoritative.

- [ ] Add reducer tests for compatible mapping edits, incompatible invalid draft/repair, missing onboard routes, immutable Y hold, undo/redo and imported v1/v2 defaults. Assert accepted serialization remains blocked for invalid mappings while controls stay editable.
- [ ] Add accessible per-screen binding controls, filtered compatible action menus, locked recovery description, hold duration and encoder options. Display current onboard fallback routes and link each validation error to its control. Use the shared catalogue, not duplicated compatibility tables.
- [ ] Add deterministic simulator press/release/hold and encoder controls driven by the shared dispatcher, with Dashboard/Debug/Logs state visible. This is the control foundation; full shared screen rendering and diagnostics are delivered by the separate approved diagnostics stage. Test all actions, reset feedback, page changes and recovery from Off.
- [ ] Run native editor tests, strict native/Wasm Clippy, production Trunk build and real browser remap/invalid repair/undo/reload/recovery checks. Audit controls end to end, commit `feat(studio): add control mapping editor and simulation`, then push master and check exact CI.
