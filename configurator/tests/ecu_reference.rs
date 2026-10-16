use dashboard_configurator::workspace::{TabKey, Workspace, WorkspaceState};
#[test]
fn reference_tab_extends_keyboard_navigation_without_changing_old_preferences() {
    assert_eq!(Workspace::ALL.len(), 6, "ECU Reference must be reachable");
    let (mut state, issue) =
        WorkspaceState::from_preferences(Some(r#"{"version":1,"active":"BuildFlash","expanded":[]}"#));
    assert!(issue.is_none());
    state.focus(TabKey::Next);
    assert_eq!(state.focused().label(), "ECU Reference");
    state.focus(TabKey::Next);
    assert_eq!(state.focused(), Workspace::Views);
    state.focus(TabKey::Previous);
    assert_eq!(state.focused().label(), "ECU Reference");
}

use dashboard_configurator::config::{DEFINITIONS, DefinitionId};
use dashboard_configurator::ecu_reference::{ENTRIES, MAX_QUERY_BYTES, ReferenceState, Section, Status};
#[test]
fn searches_conflicts_and_filters_without_losing_selected_detail() {
    let mut state = ReferenceState::default();
    for term in ["39c0", "boost", "DQ381", "GPF", "E5", "gear", "65535", "38EB"] {
        state.set_query(term);
        assert!(!state.results().is_empty(), "missing {term}");
    }
    state.set_query("GEAR 210f");
    state.status = Some(Status::Conflict);
    assert_eq!(state.results().len(), 1);
    let id = state.results()[0].id;
    assert!(state.select(id));
    state.section = Some(Section::Vehicle);
    assert!(state.results().is_empty());
    assert_eq!(state.selected().id, id);
    assert!(!state.select("does-not-exist"));
    assert_eq!(state.selected().id, id);
}
#[test]
fn query_is_utf8_bounded_and_definition_links_are_total() {
    let mut state = ReferenceState::default();
    state.set_query(&"€".repeat(500));
    assert!(state.query().len() <= MAX_QUERY_BYTES);
    assert!(state.truncated);
    for id in DEFINITIONS {
        state.open_definition(id);
        assert_eq!(state.selected().definition, Some(id));
        assert!(state.results().iter().any(|e| e.id == state.selected().id));
    }
    assert!(ENTRIES.len() <= 64);
    assert_eq!(ENTRIES.iter().filter(|e| e.status == Status::Implemented).count(), 4);
    assert!(
        ENTRIES
            .iter()
            .all(|e| !e.id.is_empty() && e.references.iter().all(|url| url.starts_with("https://")))
    );
    for (index, entry) in ENTRIES.iter().enumerate() {
        assert!(!ENTRIES[..index].iter().any(|other| other.id == entry.id));
    }
    state.open_definition(DefinitionId::ObdLambda24);
    assert_eq!(state.selected().status, Status::Conflict);
}

#[test]
fn reference_navigation_keeps_raw_and_pending_json_without_an_undo_entry() {
    use dashboard_configurator::config::default_config;
    use dashboard_configurator::editor::{Editor, RawField};
    use dashboard_configurator::workspace::{Command, dispatch};
    let mut editor = Editor::new(default_config()).unwrap();
    editor
        .set_raw(
            editor.field_token(),
            RawField::ControlHoldMilliseconds,
            "not a number".into(),
        )
        .unwrap();
    let before = (
        editor.stamp(),
        editor.selection(),
        editor.history_len(),
        editor.accepted_config_key(),
        editor.raw_fields().to_vec(),
    );
    let mut workspace = WorkspaceState::default();
    let stamp = editor.stamp();
    dispatch(
        &mut editor,
        &mut workspace,
        Command::Navigate(Workspace::EcuReference),
        stamp,
    )
    .unwrap();
    assert_eq!(
        (
            editor.stamp(),
            editor.selection(),
            editor.history_len(),
            editor.accepted_config_key(),
            editor.raw_fields().to_vec()
        ),
        before
    );
    let mut editor = Editor::new(default_config()).unwrap();
    let handle = editor.open_json().unwrap();
    let pending = "{\n  deliberately unfinished".to_string();
    editor.set_json_text(handle, pending.clone()).unwrap();
    let before = (
        editor.stamp(),
        editor.selection(),
        editor.history_len(),
        editor.accepted_config_key(),
    );
    let stamp = editor.stamp();
    dispatch(
        &mut editor,
        &mut workspace,
        Command::Navigate(Workspace::EcuReference),
        stamp,
    )
    .unwrap();
    dispatch(&mut editor, &mut workspace, Command::Navigate(Workspace::Views), stamp).unwrap();
    assert_eq!(editor.json_draft().unwrap().text(), pending);
    assert_eq!(
        (
            editor.stamp(),
            editor.selection(),
            editor.history_len(),
            editor.accepted_config_key()
        ),
        before
    );
    let encoded = workspace.preferences().unwrap();
    assert!(WorkspaceState::from_preferences(Some(&encoded)).1.is_none());
}
