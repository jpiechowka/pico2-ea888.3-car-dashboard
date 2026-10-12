use dashboard_configurator::config::*;
use dashboard_configurator::editor::*;

#[test]
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
fn mapping_repairs_preserve_unrelated_invalid_edits_and_explain_parse_blockers() {
    let mut editor = Editor::default();
    editor
        .set_raw(editor.field_token(), RawField::WarningPageMilliseconds, "bad".into())
        .unwrap();
    let mut map = editor.editing_config().controls;
    map.screens[0].buttons[0].tap = ControlAction::CycleFps;
    assert_eq!(
        editor.apply(Edit::SetControls(map)),
        Err(EditorError::Parse(RawField::WarningPageMilliseconds))
    );
    assert_eq!(editor.raw_fields()[0].text, "bad");
    editor
        .set_raw(editor.field_token(), RawField::WarningPageMilliseconds, "0".into())
        .unwrap();
    assert!(editor.apply(Edit::SetControls(map)).is_err());
    assert_eq!(editor.editing_config().controls, map);
    assert_eq!(editor.raw_fields()[0].text, "0");
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
}

#[test]
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
