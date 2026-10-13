use dashboard_configurator::config::*;
use dashboard_configurator::editor::*;

fn assert_exports_blocked(editor: &mut Editor) {
    assert!(editor.export_json().is_err());
    assert!(editor.export_binary().is_err());
    assert!(editor.begin_save().is_err());
}

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
