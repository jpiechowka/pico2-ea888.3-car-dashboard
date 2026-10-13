use dashboard_configurator::config::*;
use dashboard_configurator::editor::*;
use dashboard_configurator::simulator::ControlSimulator;
use dashboard_configurator::workspace::*;

#[test]
fn manual_focus_and_activation_are_distinct_and_preserve_editor_and_simulator() {
    let mut editor = Editor::new(default_config()).unwrap();
    editor
        .set_raw(editor.field_token(), RawField::ControlHoldMilliseconds, "bad".into())
        .unwrap();
    let stamp = editor.stamp();
    let raw = editor.raw_fields().to_vec();
    let mut sim = ControlSimulator::new(editor.preview().clone());
    sim.advance(2500);
    let time = sim.now_ms();
    let view = sim.session().view_id();
    let mut timing = dashboard_configurator::preview::Preview::new();
    let token = dashboard_core::display::pipeline::FrameToken {
        buffer: dashboard_core::display::pipeline::BufferId::A,
        sequence: 1,
    };
    let pending = timing.render_simulation_submitted(&mut sim, token).unwrap().to_vec();
    let mut state = WorkspaceState::default();
    state.focus(TabKey::Next);
    assert_eq!(state.active(), Workspace::Views);
    assert_eq!(state.focused(), Workspace::Device);
    state.activate_focused();
    assert_eq!(state.active(), Workspace::Device);
    state.open(Workspace::Diagnostics, GroupId::Files);
    assert!(state.expanded(GroupId::Files));
    assert_eq!(editor.stamp(), stamp);
    assert_eq!(editor.raw_fields(), raw);
    assert_eq!((sim.now_ms(), sim.session().view_id()), (time, view));
    assert_eq!(timing.rgba(), pending);
    assert!(sim.complete_frame(token));
}

#[test]
fn strict_preferences_and_bounds_use_safe_defaults() {
    for text in [
        "",
        "{}",
        r#"{"version":2,"active":"Views","expanded":[]}"#,
        r#"{"version":1,"active":"Unknown","expanded":[]}"#,
        r#"{"version":1,"active":"Views","expanded":[],"extra":0}"#,
        r#"{"version":1,"active":"Views","active":"Device","expanded":[]}"#,
        r#"{"version":1,"active":"Views","expanded":["Layout","Layout"]}"#,
    ] {
        assert!(WorkspaceState::from_preferences(Some(text)).1.is_some());
        assert_eq!(
            WorkspaceState::from_preferences(Some(text)).0.active(),
            Workspace::Views
        );
    }
    assert!(WorkspaceState::from_preferences(None).1.is_none());
    assert!(WorkspaceState::from_preferences(Some(&" ".repeat(4097))).1.is_some());
    let mut state = WorkspaceState::default();
    state.open(Workspace::Device, GroupId::Indicator);
    let text = state.preferences().unwrap();
    assert!(text.len() <= 4096);
    let (restored, error) = WorkspaceState::from_preferences(Some(&text));
    assert!(error.is_none());
    assert_eq!(restored.active(), Workspace::Device);
    assert!(restored.expanded(GroupId::Indicator));
    state.focus(TabKey::Last);
    assert_eq!(state.focused(), Workspace::BuildFlash);
    state.focus(TabKey::Next);
    assert_eq!(state.focused(), Workspace::Views);
}

#[test]
fn located_raw_is_clean_exact_and_preserves_selection_then_requires_retained_repair() {
    let mut editor = Editor::new(default_config()).unwrap();
    let selection = editor.selection();
    let id = editor.preview().config().widgets[1].id;
    let target = editor
        .object_inspection_target(InspectionOwner::Widget { index: 1 })
        .unwrap();
    let stamp = editor.stamp();
    assert_eq!(editor.inspect(target).unwrap().field, None);
    assert_eq!(editor.stamp(), stamp);
    assert_eq!(
        editor.set_raw(
            editor.field_token(),
            RawField::Widget(id, WidgetField::Label),
            "Oil".into()
        ),
        Err(EditorError::SelectionMismatch)
    );
    editor
        .set_located_raw(target, RawField::Widget(id, WidgetField::Label), "bad\n".into())
        .unwrap();
    assert_eq!(editor.selection(), selection);
    assert!(editor.has_invalid_draft());
    assert!(
        editor
            .set_located_raw(target, RawField::Widget(id, WidgetField::Label), "Oil".into())
            .is_err()
    );
    let raw = editor.raw_inspection_target(0).unwrap();
    editor.set_inspected_raw(raw, "Oil".into()).unwrap();
    assert!(!editor.has_invalid_draft());
    assert_eq!(editor.selection(), selection);
    assert_eq!(editor.preview().config().widgets[1].label.as_str(), "Oil");
    assert_eq!(editor.history_len().0, 1);
    editor.undo().unwrap();
    assert_eq!(editor.selection(), selection);
}

#[test]
fn located_object_cannot_authorize_another_owner_or_pending_json() {
    let mut editor = Editor::new(default_config()).unwrap();
    let stamp = editor.stamp();
    let widget = editor
        .object_inspection_target(InspectionOwner::Widget { index: 1 })
        .unwrap();
    let root = editor.object_inspection_target(InspectionOwner::Root).unwrap();
    assert!(
        editor
            .set_located_raw(root, RawField::Widget(1, WidgetField::Label), "wrong".into())
            .is_err()
    );
    assert!(
        editor
            .set_located_raw(widget, RawField::ViewName(0), "wrong".into())
            .is_err()
    );
    assert_eq!(editor.stamp(), stamp);
    assert!(
        editor
            .object_inspection_target(InspectionOwner::Widget { index: 64 })
            .is_err()
    );
    let handle = editor.open_json().unwrap();
    let text = editor.json_draft().unwrap().text().to_string();
    editor.set_json_text(handle, text).unwrap();
    assert_eq!(
        editor.set_located_raw(root, RawField::ControlHoldMilliseconds, "900".into()),
        Err(EditorError::JsonPending)
    );
}

#[test]
fn every_owner_kind_is_checked_and_nonobject_authority_cannot_create_raw_fields() {
    let mut editor = Editor::new(default_config()).unwrap();
    for owner in [
        InspectionOwner::Root,
        InspectionOwner::View { index: 0 },
        InspectionOwner::Widget { index: 1 },
        InspectionOwner::Channel(Channel::Oil),
    ] {
        let target = editor.object_inspection_target(owner).unwrap();
        let resolved = editor.inspect(target).unwrap();
        assert_eq!(resolved.field, None);
        assert_eq!(resolved.raw_field, None);
        assert!(resolved.id_lookup_unambiguous);
    }
    let mut malformed = default_config();
    malformed.widgets[0].label = "bad\n".try_into().unwrap();
    let issue = dashboard_configurator::diagnostic::collect_candidate(&malformed).issues[0];
    let semantic = editor.inspection_target(issue.location).unwrap();
    assert_eq!(
        editor.set_located_raw(semantic, RawField::Widget(1, WidgetField::Label), "No".into()),
        Err(EditorError::FieldUnavailable)
    );
    let root = editor.object_inspection_target(InspectionOwner::Root).unwrap();
    let before = (editor.stamp(), editor.history_len(), editor.selection());
    assert_eq!(
        editor.set_located_raw(root, RawField::Fuel(FuelField::Ratio), "14.7".into()),
        Err(EditorError::FieldUnavailable)
    );
    assert_eq!(
        editor.set_located_raw(root, RawField::ControlHoldMilliseconds, "0".repeat(1025)),
        Err(EditorError::RawTooLong)
    );
    assert_eq!((editor.stamp(), editor.history_len(), editor.selection()), before);
    let import = editor.begin_import().unwrap();
    assert_eq!(
        editor.set_located_raw(root, RawField::ControlHoldMilliseconds, "900".into()),
        Err(EditorError::ImportPending)
    );
    editor.cancel_import(import).unwrap();
    editor.apply(Edit::SetReducedMotion(true)).unwrap();
    assert_eq!(
        editor.set_located_raw(root, RawField::ControlHoldMilliseconds, "900".into()),
        Err(EditorError::StaleToken)
    );
    let target = editor.object_inspection_target(InspectionOwner::Root).unwrap();
    let mut another = Editor::new(default_config()).unwrap();
    assert_eq!(
        another.set_located_raw(target, RawField::ControlHoldMilliseconds, "900".into()),
        Err(EditorError::StaleToken)
    );
}

#[test]
fn located_raw_rechecks_pointer_and_fixed_variant_band_bounds() {
    let mut editor = Editor::new(default_config()).unwrap();
    let root = editor.object_inspection_target(InspectionOwner::Root).unwrap();
    let pointer = editor.begin_pointer(13, PointerTarget::Library(Channel::Oil)).unwrap();
    assert_eq!(
        editor.set_located_raw(root, RawField::ControlHoldMilliseconds, "900".into()),
        Err(EditorError::PointerActive)
    );
    editor.cancel_pointer(pointer).unwrap();
    let widget = editor
        .object_inspection_target(InspectionOwner::Widget { index: 0 })
        .unwrap();
    assert_eq!(
        editor.set_located_raw(widget, RawField::Widget(1, WidgetField::BandMinimum(0)), "0".into()),
        Err(EditorError::FieldUnavailable)
    );
    assert_eq!(
        editor.set_located_raw(widget, RawField::Widget(1, WidgetField::GraphMinimum), "0".into()),
        Err(EditorError::FieldUnavailable)
    );
    let channel = editor
        .object_inspection_target(InspectionOwner::Channel(Channel::Oil))
        .unwrap();
    assert_eq!(
        editor.set_located_raw(channel, RawField::HistorySeconds(Channel::Water), "150".into()),
        Err(EditorError::FieldUnavailable)
    );
    editor
        .set_located_raw(channel, RawField::HistorySeconds(Channel::Oil), "150".into())
        .unwrap();
    assert_eq!(editor.preview().config().history_seconds[Channel::Oil.index()], 150);
}
