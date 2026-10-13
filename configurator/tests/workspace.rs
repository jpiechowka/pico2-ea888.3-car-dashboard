use dashboard_configurator::config::*;
use dashboard_configurator::editor::*;
use dashboard_configurator::simulator::ControlSimulator;
use dashboard_configurator::workspace::*;

#[test]
fn commands_recheck_owner_readiness_and_leave_rejected_state_unchanged() {
    let mut editor = Editor::new(default_config()).unwrap();
    let mut workspace = WorkspaceState::default();
    let command = Command::Undo;
    editor.apply(Edit::SetReducedMotion(true)).unwrap();
    let stamp = editor.stamp();
    assert_eq!(command.availability(&editor, stamp), None);
    let handle = editor.open_json().unwrap();
    let text = editor.json_draft().unwrap().text().to_string();
    editor.set_json_text(handle, text.clone()).unwrap();
    let before = (editor.stamp(), editor.history_len(), editor.preview().config().clone());
    assert_eq!(
        command.availability(&editor, stamp),
        Some("Apply or Discard pending JSON first")
    );
    assert!(dispatch(&mut editor, &mut workspace, command, stamp).is_err());
    assert_eq!(
        (editor.stamp(), editor.history_len(), editor.preview().config().clone()),
        before
    );
    assert_eq!(editor.json_draft().unwrap().text(), text);
    editor.discard_json().unwrap();
    assert!(dispatch(&mut editor, &mut workspace, command, stamp).is_ok());
    let original = Editor::new(default_config()).unwrap();
    let stamp = original.stamp();
    let mut replacement = Editor::new(default_config()).unwrap();
    assert_eq!(original.revision().get(), replacement.revision().get());
    assert!(
        dispatch(
            &mut replacement,
            &mut workspace,
            Command::Navigate(Workspace::Device),
            stamp
        )
        .is_err()
    );
}

#[test]
fn captured_duplicate_cannot_follow_deleted_widget_or_changed_selection() {
    let mut editor = Editor::new(default_config()).unwrap();
    let mut workspace = WorkspaceState::default();
    let id = editor.preview().config().widgets[0].id;
    let view = editor.preview().config().widgets[0].view;
    editor.select(Selection { view, widget: Some(id) }).unwrap();
    let command = Command::selected_widget(&editor);
    let stamp = editor.stamp();
    editor.apply(Edit::DeleteWidget(id)).unwrap();
    let before = (editor.history_len(), editor.preview().config().clone());
    assert!(dispatch(&mut editor, &mut workspace, command, stamp).is_err());
    assert_eq!((editor.history_len(), editor.preview().config().clone()), before);
}

#[test]
fn duplicate_and_export_use_real_reducers_and_capacity_reasons() {
    let mut config = default_config();
    let widget = config.widgets[0].clone();
    config.widgets.clear();
    config.widgets.push(widget).unwrap();
    let mut editor = Editor::new(config).unwrap();
    let view = editor.preview().config().widgets[0].view;
    let id = editor.preview().config().widgets[0].id;
    editor.select(Selection { view, widget: Some(id) }).unwrap();
    let mut workspace = WorkspaceState::default();
    let stamp = editor.stamp();
    let command = Command::selected_widget(&editor);
    let history = editor.history_len().0;
    assert_eq!(
        dispatch(&mut editor, &mut workspace, command, stamp),
        Ok(CommandEffect::Changed)
    );
    assert_eq!(editor.preview().config().widgets.len(), 2);
    assert_eq!(editor.history_len().0, history + 1);
    editor.undo().unwrap();
    assert_eq!(editor.preview().config().widgets.len(), 1);
    let import = editor.begin_import().unwrap();
    let stamp = editor.stamp();
    assert_eq!(
        Command::Export(ExportFormat::Eav).availability(&editor, stamp),
        Some("Wait for or cancel the current import")
    );
    let before = (editor.stamp(), editor.history_len());
    assert!(dispatch(&mut editor, &mut workspace, Command::Export(ExportFormat::Eav), stamp).is_err());
    assert_eq!((editor.stamp(), editor.history_len()), before);
    assert!(editor.import_is_current(import));
    editor.cancel_import(import).unwrap();
    let pointer = editor.begin_pointer(9, PointerTarget::Library(Channel::Oil)).unwrap();
    let stamp = editor.stamp();
    assert_eq!(
        Command::Undo.availability(&editor, stamp),
        Some("Finish or cancel the active pointer gesture")
    );
    assert!(dispatch(&mut editor, &mut workspace, Command::Undo, stamp).is_err());
    assert_eq!(editor.active_pointer(), Some(pointer));
    editor.cancel_pointer(pointer).unwrap();
    let stamp = editor.stamp();
    assert_eq!(
        dispatch(&mut editor, &mut workspace, Command::Export(ExportFormat::Json), stamp),
        Ok(CommandEffect::Export(ExportFormat::Json))
    );
}

#[test]
fn find_opens_hidden_group_without_accepting_raw_or_changing_selection() {
    use dashboard_configurator::field_registry::*;
    let mut editor = Editor::new(default_config()).unwrap();
    editor
        .set_raw(editor.field_token(), RawField::ControlHoldMilliseconds, "bad".into())
        .unwrap();
    let target = Registry::retained_raw_target(&editor, 0, TargetOwner::Root).unwrap();
    let before = (
        editor.stamp(),
        editor.selection(),
        editor.history_len(),
        editor.raw_fields().to_vec(),
    );
    let mut workspace = WorkspaceState::default();
    workspace.set_expanded(GroupId::Bindings, false);
    let stamp = editor.stamp();
    assert_eq!(
        dispatch(&mut editor, &mut workspace, Command::Find(target), stamp),
        Ok(CommandEffect::Inspect(target))
    );
    assert_eq!(workspace.active(), Workspace::Device);
    assert!(workspace.expanded(GroupId::Bindings));
    assert_eq!(
        (
            editor.stamp(),
            editor.selection(),
            editor.history_len(),
            editor.raw_fields().to_vec()
        ),
        before
    );
    for command in [
        Command::CopyAppearance,
        Command::ApplyAppearance(AppearanceScope::AllWidgets),
    ] {
        assert!(command.availability(&editor, stamp).is_some());
        assert!(dispatch(&mut editor, &mut workspace, command, stamp).is_err());
    }
}

#[test]
fn open_json_command_reuses_the_owned_clean_or_pending_draft() {
    let mut editor = Editor::default();
    let mut workspace = WorkspaceState::default();
    let stamp = editor.stamp();
    let key = editor.accepted_config_key();
    let effect = dispatch(&mut editor, &mut workspace, Command::OpenJson, stamp).unwrap();
    let CommandEffect::OpenJson(handle) = effect else {
        panic!("expected owned JSON route")
    };
    assert!(!editor.has_pending_json());
    assert_eq!(workspace.active(), Workspace::Diagnostics);
    let text = editor.json_draft().unwrap().text().to_owned();
    editor.set_json_text(handle, text).unwrap();
    assert_eq!(Command::OpenJson.availability(&editor, stamp), None);
    assert_eq!(
        dispatch(&mut editor, &mut workspace, Command::OpenJson, stamp),
        Ok(CommandEffect::OpenJson(handle))
    );
    assert!(editor.has_pending_json());
    assert_eq!(
        (editor.stamp(), editor.accepted_config_key(), editor.history_len()),
        (stamp, key, (0, 0))
    );
}

#[test]
fn shortcuts_preserve_ime_editable_undo_and_browser_commands() {
    let shortcut =
        |key: &str, editable, composing| shortcut_action(key, true, false, false, false, composing, editable);
    assert_eq!(shortcut("k", true, false), Some(ShortcutAction::Palette));
    assert_eq!(shortcut("k", false, true), None);
    assert_eq!(shortcut("z", true, false), None);
    assert_eq!(shortcut("z", false, false), Some(ShortcutAction::Undo));
    assert_eq!(
        shortcut_action("z", true, true, false, false, false, false),
        Some(ShortcutAction::Redo)
    );
    for key in ["r", "f", "+", "-", "0", "l"] {
        assert_eq!(shortcut(key, false, false), None);
    }
    assert_eq!(shortcut_action("k", true, false, true, false, false, false), None);
    assert_eq!(shortcut_action("k", true, false, false, true, false, false), None);
    assert_eq!(shortcut_action("k", true, true, false, false, false, false), None);
}

#[test]
fn focus_requests_are_retired_on_close_supersession_and_exhaustion() {
    let mut requests = FocusRequests::default();
    let first = requests.issue().unwrap();
    assert!(requests.is_current(first));
    requests.cancel();
    assert!(!requests.is_current(first));
    let second = requests.issue().unwrap();
    let third = requests.issue().unwrap();
    assert!(!requests.is_current(second));
    assert!(requests.is_current(third));
}

#[test]
fn full_grid_and_view_capacity_reject_without_history_movement() {
    let mut config = default_config();
    let widget = config.widgets[0].clone();
    config.widgets.clear();
    config.widgets.push(widget).unwrap();
    let mut editor = Editor::new(config).unwrap();
    let widget = &editor.preview().config().widgets[0];
    editor
        .select(Selection {
            view: widget.view,
            widget: Some(widget.id),
        })
        .unwrap();
    let mut workspace = WorkspaceState::default();
    for _ in 0..8 {
        let command = Command::selected_widget(&editor);
        let stamp = editor.stamp();
        if command.availability(&editor, stamp).is_some() {
            break;
        }
        dispatch(&mut editor, &mut workspace, command, stamp).unwrap();
    }
    let command = Command::selected_widget(&editor);
    let stamp = editor.stamp();
    assert_eq!(
        command.availability(&editor, stamp),
        Some("No free space for this widget size in its view")
    );
    let before = (editor.history_len(), editor.preview().config().clone());
    assert!(dispatch(&mut editor, &mut workspace, command, stamp).is_err());
    assert_eq!((editor.history_len(), editor.preview().config().clone()), before);
    while editor.preview().config().views.len() < MAX_VIEWS {
        editor.apply(Edit::AddView { name: "Extra".into() }).unwrap();
    }
    let command = Command::selected_view(&editor);
    let stamp = editor.stamp();
    assert_eq!(
        command.availability(&editor, stamp),
        Some("The 8-view limit is reached")
    );
    let before = editor.history_len();
    assert!(dispatch(&mut editor, &mut workspace, command, stamp).is_err());
    assert_eq!(editor.history_len(), before);
}

#[test]
fn simulator_recovery_command_preserves_editor_and_matches_protected_y_hold() {
    let mut editor = Editor::new(default_config()).unwrap();
    let mut workspace = WorkspaceState::default();
    let mut command_sim = ControlSimulator::new(editor.preview().clone());
    let mut button_sim = ControlSimulator::new(editor.preview().clone());
    for sim in [&mut command_sim, &mut button_sim] {
        sim.advance(1500);
        sim.tap(ButtonId::B);
    }
    let before = (
        editor.stamp(),
        editor.selection(),
        editor.history_len(),
        editor.accepted_config_key(),
    );
    let stamp = editor.stamp();
    assert_eq!(
        dispatch(&mut editor, &mut workspace, Command::RecoverSimulator, stamp),
        Ok(CommandEffect::RecoverSimulator)
    );
    command_sim.recover();
    button_sim.hold(ButtonId::Y);
    assert_eq!(command_sim.summary(), button_sim.summary());
    assert_eq!(command_sim.now_ms(), button_sim.now_ms());
    assert_eq!(
        (
            editor.stamp(),
            editor.selection(),
            editor.history_len(),
            editor.accepted_config_key()
        ),
        before
    );
}

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
