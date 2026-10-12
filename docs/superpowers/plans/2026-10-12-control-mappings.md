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

**Files:** Create `crates/dashboard-core/src/controls.rs` and `host/tests/control_gestures.rs`; modify `crates/dashboard-core/src/lib.rs`, `crates/dashboard-core/src/state/pages.rs`, `crates/dashboard-core/src/backlight.rs` and focused backlight tests.

**Interfaces:** Consume Task 1's validated map. Re-export `ControlPage as Page` from existing pages module to retain downstream imports. Add `ControlGestures::new()`, `reset()`, and `update(&mut self, map: &ControlMap, page: ControlPage, pressed: [bool; 4], now_ms: u64) -> ControlBatch`; `ControlBatch` has a bounded action list carrying ButtonId and Observation, a recovery flag/observation, and discarded count. Expose `ControlBatch::transition()` for a priority screen change. Add `BacklightController::wake_full()` (use the existing concrete controller name in source if different), which cancels every pending Off state and restores Full. Frame completion with an older token after wake must not turn it off.

- [ ] Add failing time traces: bootstrap released at0, press A at100, accept150, release200/250 -> exactly one tap; hold A through899/900 -> exactly one hold at900, release -> no tap; boot held Y -> no recovery until a stable release and new hold. Recovery starts at debounced candidate origin and fires once at2,000ms, never at1,999ms. Include bouncing candidate transitions, same timestamp, regression and u64 boundary. Write test helpers that replay explicit `(time, levels)` traces and assert semantic actions.
- [ ] Run the focused gesture/backlight test commands and save RED output. Implement fixed per-button state with release arming, press candidate time, stable debounce and a single fired flag. On page changes, disarm held keys and suppress old-screen release. Ignore regressed times without advancing state. Recovery/page changes have explicit deterministic precedence.
- [ ] Test every action gesture, simultaneous recovery vs reset/units/encoder batch, fixed Y behavior despite a raw invalid map passed to the pure helper, maintenance reset, multi-second gaps, ordinary hold bound saturation and stale Off token completion after wake.
- [ ] Run host control/backlight/input/navigation tests, core/config suites, formatting and strict affected Clippy. Commit `feat(controls): add shared hold gestures and protected recovery` after reviewable self-check.

### Task 3: Versioned config and firmware adapter integration

**Files:** Modify config model/defaults/validate/codec/legacy and tests; `configs/default.json/.eav`; firmware main/input/button adapters; editor error match adapters needed to compile. Freeze v2 import models under the host-only legacy module.

**Interfaces:** Add `controls: ControlMap` to Config, bump VERSION to3, invoke `validate_controls` from shared validation with a structured controls field/error. `default_config` uses ControlMap::default. Host imports frozen v1/v2 and fills default controls. Firmware accepts only v3 and retains logged fallback on old/malformed images. Firmware consumes ControlGestures and executes compatible actions with its existing view/backlight/log/FPS/reset owners; use shared encoder action dispatch helpers rather than reimplementing mapping semantics in UI later.

- [ ] Capture v2 fixtures before changing the codec; write failing migration and semantic validation tests. Freeze display/warning/presentation types used by old wire models so future fields cannot alter v1/v2 imports. Assert CRC/trailing/noncanonical-varint rejection remains intact.
- [ ] Route A/B/X/Y levels through shared gestures, apply recovery first, page change second, and remaining actions deterministically. Discard encoder input from a transaction that changes screen. Keep reset observations tied to Core1 acknowledgment and maintenance resets. Preserve current last-view and log-anchor behavior. Replace the old hardcoded action function only after tests exercise its replacement.
- [ ] Regenerate default JSON/EAV through dashboard-config-tools. Test both host outlines and config JSON/no-JSON, build Demo280/70 and Vehicle targets and measure ARM config sizes/static RAM/boot stack. Do not increase limits blindly. Update control docs with actual tap-on-release and holds. Commit `feat(firmware): apply configurable controls and recovery`.

### Task 4: Repairable control editor and simulator control foundation

**Files:** Modify editor action/reducer and UI fields; create focused `configurator/src/ui/controls.rs` and simulator control module/tests; use existing file layout conventions for exports.

**Interfaces:** A Device controls panel edits Config.controls transactionally; a simulator adapter consumes ControlGestures and shared action dispatch. Existing draft/accepted/config version/storage contracts remain authoritative.

- [ ] Add reducer tests for compatible mapping edits, incompatible invalid draft/repair, missing onboard routes, immutable Y hold, undo/redo and imported v1/v2 defaults. Assert accepted serialization remains blocked for invalid mappings while controls stay editable.
- [ ] Add accessible per-screen binding controls, filtered compatible action menus, locked recovery description, hold duration and encoder options. Display current onboard fallback routes and link each validation error to its control. Use the shared catalogue, not duplicated compatibility tables.
- [ ] Add deterministic simulator press/release/hold and encoder controls driven by the shared dispatcher, with Dashboard/Debug/Logs state visible. This is the control foundation; full shared screen rendering and diagnostics are delivered by the separate approved diagnostics stage. Test all actions, reset feedback, page changes and recovery from Off.
- [ ] Run native editor tests, strict native/Wasm Clippy, production Trunk build and real browser remap/invalid repair/undo/reload/recovery checks. Audit controls end to end, commit `feat(studio): add control mapping editor and simulation`, then push master and check exact CI.
