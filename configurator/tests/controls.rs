//! Control-map repair and synthetic callback/input authority regressions.

use dashboard_configurator::config::*;
use dashboard_configurator::editor::*;

/// Asserts `editor` cannot export JSON/binary or begin persistence while its draft is inadmissible.
fn assert_exports_blocked(editor: &mut Editor) {
    assert!(editor.export_json().is_err());
    assert!(editor.export_binary().is_err());
    assert!(editor.begin_save().is_err());
}

#[test]
/// Unreachable screen routes stay inspectable and block export until repaired back to accepted bytes.
fn broken_routes_remain_editable_until_repaired() {
    let mut editor = Editor::new(default_config()).unwrap();
    let accepted = editor.export_binary().unwrap();
    let mut map = editor.editing_config().controls;
    map.screens[0].buttons[3].tap = ControlAction::None;
    assert!(editor.apply(Edit::SetControls(map)).is_err());
    assert!(editor.has_invalid_draft());
    assert!(editor.export_binary().is_err());
    assert_eq!(editor.editing_config().controls, map);
    map.screens[0].buttons[3].tap = ControlAction::NextScreen;
    editor.apply(Edit::SetControls(map)).unwrap();
    assert_eq!(editor.export_binary().unwrap(), accepted);
}

#[test]
/// Hold-time repairs retain rejected bindings, retire old tokens and publish the repaired map as one undoable edit.
fn hold_repairs_preserve_rejected_bindings_and_stale_tokens_are_blocked() {
    let mut editor = Editor::new(default_config()).unwrap();
    let mut map = editor.editing_config().controls;
    map.screens[0].buttons[0].tap = ControlAction::ScrollOlder;
    assert!(editor.apply(Edit::SetControls(map)).is_err());
    let stale = editor.field_token();
    editor
        .set_raw(stale, RawField::ControlHoldMilliseconds, "bad".into())
        .unwrap();
    assert_eq!(
        editor.apply_if_current(stale, Edit::SetControls(map)),
        Err(EditorError::StaleToken)
    );
    editor
        .set_raw(editor.field_token(), RawField::ControlHoldMilliseconds, "900".into())
        .unwrap();
    assert_eq!(
        editor.editing_config().controls.screens[0].buttons[0].tap,
        ControlAction::ScrollOlder
    );
    map.hold_ms = 900;
    map.screens[0].buttons[0].tap = ControlAction::ToggleBoostUnits;
    editor.apply(Edit::SetControls(map)).unwrap();
    assert_eq!(editor.preview().config().controls, map);
    editor.undo().unwrap();
    assert_eq!(editor.preview().config().controls, ControlMap::default());
    editor.redo().unwrap();
    assert_eq!(editor.preview().config().controls, map);
}

#[test]
/// Illegal dwell, reserved Y hold, screen actions and unreachable routes remain repairable but cannot serialize.
fn every_rejected_control_kind_can_be_repaired_and_never_serialized() {
    let mut maps = Vec::new();
    let mut map = ControlMap {
        hold_ms: 399,
        ..ControlMap::default()
    };
    maps.push(map);
    map = ControlMap::default();
    map.screens[1].buttons[3].hold = ControlAction::Dashboard;
    maps.push(map);
    map = ControlMap::default();
    map.screens[2].buttons[0].tap = ControlAction::CycleFps;
    maps.push(map);
    map = ControlMap::default();
    map.screens[1].encoder_click = ControlAction::ScrollOlder;
    maps.push(map);
    map = ControlMap::default();
    map.screens[1].encoder_rotation = RotationAction::Views;
    maps.push(map);
    map = ControlMap::default();
    map.screens[0].buttons[0].hold = ControlAction::None;
    map.screens[0].buttons[1].hold = ControlAction::None;
    maps.push(map);
    for map in maps {
        let mut editor = Editor::default();
        assert!(editor.apply(Edit::SetControls(map)).is_err());
        assert_eq!(editor.editing_config().controls, map);
        assert!(editor.export_json().is_err());
        assert!(editor.export_binary().is_err());
        assert!(editor.begin_save().is_err());
        editor.apply(Edit::SetControls(ControlMap::default())).unwrap();
        assert!(!editor.has_invalid_draft());
    }
}

#[test]
/// Map/hold repairs preserve unrelated raw warning failures; final repair admits all changes in one history step.
fn mapping_repairs_preserve_unrelated_invalid_edits_and_explain_parse_blockers() {
    let mut editor = Editor::default();
    editor
        .set_raw(editor.field_token(), RawField::WarningPageMilliseconds, "bad".into())
        .unwrap();
    assert_exports_blocked(&mut editor);
    let mut map = editor.editing_config().controls;
    map.screens[0].buttons[0].tap = ControlAction::CycleFps;
    assert_eq!(
        editor.apply(Edit::SetControls(map)),
        Err(EditorError::Parse(RawField::WarningPageMilliseconds))
    );
    assert_eq!(editor.raw_fields()[0].text, "bad");
    assert_eq!(editor.editing_config().controls, map);
    assert_exports_blocked(&mut editor);
    editor
        .set_raw(editor.field_token(), RawField::WarningPageMilliseconds, "0".into())
        .unwrap();
    assert_exports_blocked(&mut editor);
    assert!(editor.apply(Edit::SetControls(map)).is_err());
    assert_eq!(editor.editing_config().controls, map);
    assert_eq!(editor.raw_fields()[0].text, "0");
    assert_exports_blocked(&mut editor);
    // Hold repairs preserve this parsed, rejected unrelated warning edit.
    editor
        .set_raw(editor.field_token(), RawField::ControlHoldMilliseconds, "1000".into())
        .unwrap();
    assert_eq!(editor.editing_config().warnings.page_ms, 0);
    assert_eq!(
        editor.editing_config().controls.screens[0].buttons[0].tap,
        ControlAction::CycleFps
    );
    assert!(editor.export_json().is_err());
    assert!(editor.export_binary().is_err());
    assert!(editor.begin_save().is_err());
    let stale = editor.field_token();
    editor
        .set_raw(stale, RawField::WarningPageMilliseconds, "2500".into())
        .unwrap();
    assert!(!editor.has_invalid_draft());
    assert_eq!(editor.preview().config().warnings.page_ms, 2500);
    assert_eq!(
        editor.preview().config().controls.screens[0].buttons[0].tap,
        ControlAction::CycleFps
    );
    assert_eq!(editor.preview().config().controls.hold_ms, 1000);
    assert!(editor.export_json().is_ok());
    assert!(editor.export_binary().is_ok());
    assert!(editor.begin_save().is_ok());
    assert_eq!(
        editor.set_raw(stale, RawField::WarningPageMilliseconds, "3000".into()),
        Err(EditorError::StaleToken)
    );
    editor.undo().unwrap();
    assert_eq!(editor.preview().config(), &default_config());
    editor.redo().unwrap();
    assert_eq!(editor.preview().config().warnings.page_ms, 2500);
    assert_eq!(
        editor.preview().config().controls.screens[0].buttons[0].tap,
        ControlAction::CycleFps
    );
}

#[test]
/// Repair can add existing fields to a rejected map, while missing IDs leave its draft unchanged.
fn rejected_map_allows_new_existing_fields_but_rejects_missing_targets() {
    let mut editor = Editor::default();
    editor
        .set_raw(editor.field_token(), RawField::WarningPageMilliseconds, "0".into())
        .unwrap();
    let mut map = editor.editing_config().controls;
    map.screens[0].buttons[0].tap = ControlAction::CycleFps;
    assert!(editor.apply(Edit::SetControls(map)).is_err());
    let before = editor.editing_config().clone();
    for field in [RawField::ViewName(255), RawField::Widget(u16::MAX, WidgetField::Label)] {
        assert!(editor.set_raw(editor.field_token(), field, "2500".into()).is_err());
        assert_eq!(editor.editing_config(), &before);
    }
    editor
        .set_raw(
            editor.field_token(),
            RawField::WarningColor { foreground: true },
            "#FFFFFF".into(),
        )
        .unwrap();
    assert_eq!(editor.editing_config().controls, before.controls);
    editor
        .set_raw(editor.field_token(), RawField::WarningPageMilliseconds, "bad".into())
        .unwrap();
    assert_eq!(editor.raw_fields()[0].text, "bad");
    assert_eq!(editor.editing_config().controls, map);
    assert!(editor.export_binary().is_err());
    assert!(editor.begin_save().is_err());
    editor
        .set_raw(editor.field_token(), RawField::WarningPageMilliseconds, "2500".into())
        .unwrap();
    assert_eq!(editor.preview().config().controls, map);
    assert!(editor.export_binary().is_ok());
}

#[test]
/// Repairing warning text or hold time preserves separate invalid map/fuel settings until those are repaired.
fn retained_raw_repair_does_not_drop_another_rejected_setting() {
    let mut editor = Editor::default();
    editor
        .set_raw(editor.field_token(), RawField::WarningPageMilliseconds, "0".into())
        .unwrap();
    let mut map = editor.editing_config().controls;
    map.screens[0].buttons[0].tap = ControlAction::ScrollOlder;
    assert!(editor.apply(Edit::SetControls(map)).is_err());
    editor
        .set_raw(editor.field_token(), RawField::WarningPageMilliseconds, "2500".into())
        .unwrap();
    assert_eq!(editor.editing_config().controls, map);
    assert_eq!(editor.editing_config().warnings.page_ms, 2500);
    assert!(editor.export_json().is_err());
    assert!(editor.export_binary().is_err());
    assert!(editor.begin_save().is_err());
    map.screens[0].buttons[0].tap = ControlAction::CycleFps;
    editor.apply(Edit::SetControls(map)).unwrap();
    assert_eq!(editor.preview().config().warnings.page_ms, 2500);
    assert_eq!(editor.preview().config().controls, map);
    editor.undo().unwrap();
    assert_eq!(editor.preview().config(), &default_config());

    // A distinct typed rejection also survives control and hold-text repairs.
    let mut editor = Editor::default();
    assert!(
        editor
            .apply(Edit::SetFuel(Fuel::Stoichiometric { ratio: 0.0 }))
            .is_err()
    );
    assert!(editor.apply(Edit::SetControls(map)).is_err());
    editor
        .set_raw(editor.field_token(), RawField::ControlHoldMilliseconds, "bad".into())
        .unwrap();
    editor
        .set_raw(editor.field_token(), RawField::ControlHoldMilliseconds, "1000".into())
        .unwrap();
    assert_eq!(editor.editing_config().fuel, Fuel::Stoichiometric { ratio: 0.0 });
    assert_eq!(
        editor.editing_config().controls.screens[0].buttons[0].tap,
        ControlAction::CycleFps
    );
    editor
        .set_raw(editor.field_token(), RawField::Fuel(FuelField::Ratio), "14.7".into())
        .unwrap();
    assert!(!editor.has_invalid_draft());
    assert_eq!(editor.preview().config().fuel, Fuel::Stoichiometric { ratio: 14.7 });
}

#[test]
/// Frozen v1/v2 imports acquire current default controls; undo restores the pre-import customized map.
fn historical_imports_receive_default_controls_in_the_editor() {
    for bytes in [
        include_bytes!("../../crates/dashboard-config/tests/fixtures/v1-default.json").as_slice(),
        include_bytes!("../../crates/dashboard-config/tests/fixtures/v2-default.json").as_slice(),
    ] {
        let mut editor = Editor::default();
        let map = ControlMap {
            hold_ms: 1000,
            ..ControlMap::default()
        };
        editor.apply(Edit::SetControls(map)).unwrap();
        let token = editor.begin_import().unwrap();
        editor.complete_import(token, ImportFormat::Json, bytes).unwrap();
        assert_eq!(editor.editing_config().controls, ControlMap::default());
        editor.undo().unwrap();
        assert_eq!(editor.editing_config().controls, map);
    }
}
#[test]
/// Callback ownership rejects another source, retired generations and absent source identity, while recapture works.
fn callback_lifecycle_retires_hidden_cleanup_and_source_changed_work() {
    use dashboard_configurator::simulator::CallbackGate;
    let mut gate = CallbackGate::new();
    let sim = dashboard_configurator::simulator::ControlSimulator::new(
        dashboard_configurator::config::validate(dashboard_configurator::config::default_config()).unwrap(),
    );
    let other = dashboard_configurator::simulator::ControlSimulator::new(
        dashboard_configurator::config::validate(dashboard_configurator::config::default_config()).unwrap(),
    );
    let token = gate.capture(sim.source_key()).unwrap();
    assert!(gate.is_current(token, sim.source_key()));
    assert!(!gate.is_current(token, other.source_key()));
    gate.retire();
    assert!(!gate.is_current(token, sim.source_key()));
    let resumed = gate.capture(sim.source_key()).unwrap();
    assert!(gate.is_current(resumed, sim.source_key()));
    assert!(gate.capture(None).is_none());
}

#[test]
/// Actual accepted editor identity invalidates a callback even before its reactive simulator source updates.
fn callback_admission_rejects_changed_editor_before_reactive_source_sync() {
    use dashboard_configurator::editor::{Edit, Editor};
    use dashboard_configurator::simulator::{CallbackGate, ControlSimulator};
    let mut editor = Editor::default();
    let sim = ControlSimulator::new(editor.preview().clone());
    let gate = CallbackGate::new();
    let token = gate
        .capture_for_editor(sim.source_key(), editor.accepted_config_key())
        .unwrap();
    assert!(gate.is_current_for_editor(token, sim.source_key(), editor.accepted_config_key()));
    // The actual Editor changed; the reactive source observer has not run yet.
    editor.apply(Edit::SetReducedMotion(true)).unwrap();
    assert!(!gate.is_current_for_editor(token, sim.source_key(), editor.accepted_config_key()));
}

#[test]
/// Over-byte-limit visible text blocks prior admitted channel/reset values until explicitly repaired.
fn rejected_visible_utf8_text_cannot_dispatch_previous_channel_or_reset_value() {
    use dashboard_configurator::config::*;
    use dashboard_configurator::simulator::{BoundedText, ControlSimulator};
    let mut sim = ControlSimulator::new(validate(default_config()).unwrap());
    let mut channel = BoundedText::<64>::default();
    assert!(channel.observe("2"));
    sim.custom_input(Channel::Boost, channel.admitted().unwrap());
    let before = sim.snapshot();
    assert!(!channel.observe(&"é".repeat(64)));
    assert!(channel.is_blocked());
    assert!(channel.admitted().is_none());
    if let Some(text) = channel.admitted() {
        sim.custom_input(Channel::Boost, text);
    }
    assert_eq!(sim.snapshot(), before);
    assert!(channel.text().len() <= 64);
    assert!(channel.observe("3"));
    sim.custom_input(Channel::Boost, channel.admitted().unwrap());
    assert_eq!(
        sim.snapshot().sensor(dashboard_core::samples::SensorId::Boost).value(),
        Some(3.)
    );
    sim.tap(ButtonId::B);
    let token = sim.pending_reset_token().unwrap();
    let mut ack = BoundedText::<20>::default();
    assert!(ack.observe(&token.to_string()));
    assert!(!ack.observe(&"é".repeat(20)));
    if let Some(text) = ack.admitted() {
        sim.acknowledge_reset_token(text.parse().unwrap());
    }
    assert_eq!(sim.pending_reset_token(), Some(token));
    assert_eq!(sim.snapshot().reset_generation, 0);
    assert!(ack.observe(&token.to_string()));
    assert!(sim.acknowledge_reset_token(ack.admitted().unwrap().parse().unwrap()));
}

#[test]
/// Scenario time follows active elapsed milliseconds across callback frequencies and excludes paused intervals.
fn scenario_active_elapsed_clock_does_not_count_callbacks_or_hidden_interval() {
    use dashboard_configurator::config::*;
    use dashboard_configurator::simulator::{ControlSimulator, Scenario};
    use dashboard_configurator::timing::Playback;
    for interval in [1, 14, 25, 50, 1000] {
        let mut sim = ControlSimulator::new(validate(default_config()).unwrap());
        sim.set_scenario(Scenario::Normal).unwrap();
        let start = sim.now_ms();
        let mut clock = Playback::new(start);
        clock.start(10.);
        for at in (11..3010).step_by(interval) {
            sim.scenario_step(clock.sample(at as f64)).unwrap();
        }
        sim.scenario_step(clock.pause(3010.)).unwrap();
        assert_eq!(sim.now_ms(), start + 3000);
        assert_eq!(clock.sample(999999.), start + 3000);
        clock.start(1_000_000.);
        sim.scenario_step(clock.sample(1_000_050.)).unwrap();
        assert_eq!(sim.now_ms(), start + 3050);
    }
}
