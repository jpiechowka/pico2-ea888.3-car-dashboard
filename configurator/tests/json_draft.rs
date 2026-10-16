use dashboard_configurator::config::*;
use dashboard_configurator::editor::*;
use dashboard_configurator::json_draft::JsonState;

#[test]
fn draft_projection_tracks_equal_text_and_parse_phase_without_accepted_mutation() {
    use dashboard_configurator::json_draft::JsonPhase;
    let mut editor = Editor::default();
    let stamp = editor.stamp();
    let key = editor.accepted_config_key();
    let handle = editor.open_json().unwrap();
    let clean = editor.json_draft().unwrap().projection();
    assert_eq!(clean.phase(), JsonPhase::Clean);
    let text = editor.json_draft().unwrap().text().to_owned();
    let token = editor.set_json_text(handle, text.clone()).unwrap();
    let pending = editor.json_draft().unwrap().projection();
    assert_ne!(pending, clean);
    assert_eq!(pending.phase(), JsonPhase::Pending);
    editor.finish_json_parse(token).unwrap();
    let parsed = editor.json_draft().unwrap().projection();
    assert_ne!(parsed, pending);
    assert_eq!(parsed.phase(), JsonPhase::ValidUnapplied);
    editor.set_json_text(handle, text).unwrap();
    let second = editor.json_draft().unwrap().projection();
    assert_ne!(second, pending);
    assert_eq!(
        (editor.stamp(), editor.accepted_config_key(), editor.history_len()),
        (stamp, key, (0, 0))
    );
    assert_eq!(
        editor.set_json_text(handle, "ą".repeat(MAX_JSON_BYTES)),
        Err(EditorError::JsonTooLong)
    );
    assert_eq!(editor.json_draft().unwrap().projection(), second);
    let token = editor.set_json_text(handle, "{".into()).unwrap();
    editor.finish_json_parse(token).unwrap();
    assert_eq!(
        editor.json_draft().unwrap().projection().phase(),
        JsonPhase::SyntaxError
    );
}

#[test]
fn json_view_fallback_is_guarded_and_does_not_select_another_firmware_view() {
    use dashboard_configurator::json_draft::inspection_view;
    let mut candidate = default_config();
    assert_eq!(inspection_view(&candidate, 255), Some(candidate.boot_view));
    candidate.views.clear();
    assert_eq!(inspection_view(&candidate, 1), None);
    let mut candidate = default_config();
    candidate.views.clear();
    candidate
        .views
        .push(View {
            orientation: None,
            id: 4,
            name: "ONE".try_into().unwrap(),
        })
        .unwrap();
    candidate.views.push(candidate.views[0].clone()).unwrap();
    assert_eq!(inspection_view(&candidate, 4), None);
}

fn parse_text(
    editor: &mut Editor,
    text: String,
) {
    let handle = editor.open_json().unwrap();
    let token = editor.set_json_text(handle, text).unwrap();
    editor.finish_json_parse(token).unwrap();
}

#[test]
fn opening_is_clean_but_the_first_identical_keystroke_blocks_export_until_discard() {
    let mut editor = Editor::default();
    let before = editor.stamp();
    let bytes = editor.export_binary().unwrap();
    let draft = editor.open_json().unwrap();
    assert!(!editor.has_pending_json());
    assert_eq!(editor.action_readiness().block, None);
    assert_eq!(editor.stamp(), before);
    let text = editor.json_draft().unwrap().text().to_owned();
    let token = editor.set_json_text(draft, text).unwrap();
    assert_eq!(editor.action_readiness().block, Some(ActionBlock::JsonPending));
    assert_eq!(editor.export_binary(), Err(EditorError::JsonPending));
    assert_eq!(editor.history_len(), (0, 0));
    editor.finish_json_parse(token).unwrap();
    assert_eq!(editor.json_preview_label(), Some("UNAPPLIED JSON PREVIEW"));
    assert_eq!(editor.preview().config(), &default_config());
    editor.discard_json().unwrap();
    assert_eq!(editor.stamp(), before);
    assert_eq!(editor.export_binary().unwrap(), bytes);
}

#[test]
fn semantic_invalid_and_syntax_invalid_candidates_do_not_replace_accepted_state() {
    let mut editor = Editor::default();
    let bytes = editor.export_binary().unwrap();
    let json = editor.export_json().unwrap();
    parse_text(&mut editor, json.replacen("\"decimals\":2", "\"decimals\":255", 1));
    assert_eq!(editor.json_preview_label(), Some("INVALID PREVIEW"));
    assert!(
        matches!(editor.json_draft().unwrap().state(), JsonState::Candidate { report, .. } if !report.issues.is_empty())
    );
    assert!(matches!(editor.apply_json(), Err(EditorError::Validation(_))));
    assert_eq!(editor.history_len(), (0, 0));
    let handle = editor.json_draft().unwrap().handle();
    let token = editor.set_json_text(handle, "{\n  bad".into()).unwrap();
    assert!(matches!(editor.json_draft().unwrap().state(), JsonState::Unparsed));
    editor.finish_json_parse(token).unwrap();
    assert!(
        matches!(editor.json_draft().unwrap().state(), JsonState::SyntaxError { line: 2, column: 3, message } if !message.is_empty())
    );
    assert_eq!(editor.json_preview_label(), Some("INVALID JSON"));
    assert!(editor.apply_json().is_err());
    editor.discard_json().unwrap();
    assert_eq!(editor.export_binary().unwrap(), bytes);
}

#[test]
fn parse_tokens_track_each_keystroke_transaction_and_editor_owner() {
    let mut editor = Editor::default();
    let text = editor.export_json().unwrap();
    let draft = editor.open_json().unwrap();
    let old = editor.set_json_text(draft, text.clone()).unwrap();
    let current = editor.set_json_text(draft, text.clone()).unwrap();
    assert_eq!(editor.finish_json_parse(old), Err(EditorError::StaleToken));
    editor.finish_json_parse(current).unwrap();
    editor.discard_json().unwrap();
    let draft = editor.open_json().unwrap();
    let newer = editor.set_json_text(draft, text.clone()).unwrap();
    assert_eq!(editor.finish_json_parse(current), Err(EditorError::StaleToken));
    let mut replacement = Editor::default();
    parse_text(&mut replacement, text);
    assert_eq!(replacement.finish_json_parse(newer), Err(EditorError::StaleToken));
}

#[test]
fn typing_retires_existing_import_and_prepared_save_even_after_discard() {
    let mut editor = Editor::default();
    let bytes = editor.export_binary().unwrap();
    let text = editor.export_json().unwrap();
    let import = editor.begin_import().unwrap();
    let draft = editor.open_json().unwrap();
    editor.set_json_text(draft, text.clone()).unwrap();
    editor.discard_json().unwrap();
    assert_eq!(
        editor.complete_import(import, ImportFormat::Binary, &bytes),
        Err(EditorError::StaleToken)
    );
    let save = editor.begin_save().unwrap();
    let draft = editor.open_json().unwrap();
    editor.set_json_text(draft, text).unwrap();
    assert!(!editor.save_is_current(save.token()));
    editor.discard_json().unwrap();
    assert_eq!(
        editor.acknowledge_save(save.token(), Ok(())),
        Err(EditorError::StaleToken)
    );
}

#[test]
fn accepted_import_retires_clean_json_handles_and_parse_callbacks() {
    let mut editor = Editor::default();
    let text = editor.export_json().unwrap();
    let handle = editor.open_json().unwrap();
    let token = editor.begin_import().unwrap();
    editor
        .complete_import(token, ImportFormat::Json, text.as_bytes())
        .unwrap();
    assert_eq!(editor.set_json_text(handle, text), Err(EditorError::StaleToken));
}

#[test]
fn utf8_limit_rejects_overflow_without_accepting_text_or_retiring_the_current_parse() {
    let mut editor = Editor::default();
    let text = editor.export_json().unwrap();
    let mut exact = text.clone();
    exact.push_str(&" ".repeat(MAX_JSON_BYTES - exact.len()));
    let draft = editor.open_json().unwrap();
    let token = editor.set_json_text(draft, exact.clone()).unwrap();
    assert_eq!(
        editor.set_json_text(draft, format!("{exact}ą")),
        Err(EditorError::JsonTooLong)
    );
    assert_eq!(editor.json_draft().unwrap().text(), exact);
    editor.finish_json_parse(token).unwrap();
    editor.apply_json().unwrap();
    assert_eq!(editor.preview().config(), &default_config());
    assert_eq!(editor.history_len(), (1, 0));
}

#[test]
fn apply_is_one_atomic_undo_entry_and_reparses_the_current_text() {
    let mut editor = Editor::default();
    let before = editor.export_binary().unwrap();
    let json = editor.export_json().unwrap();
    parse_text(
        &mut editor,
        json.replacen("\"reduced_motion\":false", "\"reduced_motion\":true", 1),
    );
    let key = editor.accepted_config_key();
    editor.apply_json().unwrap();
    assert!(!editor.has_pending_json());
    assert!(editor.preview().config().reduced_motion);
    assert_ne!(editor.accepted_config_key(), key);
    assert_eq!(editor.history_len(), (1, 0));
    editor.undo().unwrap();
    assert_eq!(editor.export_binary().unwrap(), before);
    let same_key = editor.accepted_config_key();
    let rev = editor.accepted_revision();
    parse_text(&mut editor, json);
    editor.apply_json().unwrap();
    assert_eq!(editor.history_len(), (1, 0));
    assert_ne!(editor.accepted_revision(), rev);
    assert_eq!(editor.accepted_config_key(), same_key);
    let text = editor.export_json().unwrap();
    parse_text(&mut editor, text);
    let handle = editor.json_draft().unwrap().handle();
    editor.set_json_text(handle, "{".into()).unwrap();
    assert!(editor.apply_json().is_err());
    assert!(editor.has_pending_json());
    assert_eq!(editor.history_len(), (1, 0));
}

#[test]
fn all_public_visual_and_persistence_actions_recheck_pending_json() {
    let mut editor = Editor::default();
    let text = editor.export_json().unwrap();
    let field = editor.field_token();
    parse_text(&mut editor, text);
    let stamp = editor.stamp();
    assert_eq!(
        editor.apply(Edit::SetReducedMotion(true)),
        Err(EditorError::JsonPending)
    );
    assert_eq!(
        editor.set_raw(field, RawField::ViewName(1), "New".into()),
        Err(EditorError::JsonPending)
    );
    assert_eq!(editor.undo(), Err(EditorError::JsonPending));
    assert_eq!(editor.redo(), Err(EditorError::JsonPending));
    assert_eq!(editor.begin_import(), Err(EditorError::JsonPending));
    assert_eq!(
        editor.begin_pointer(1, PointerTarget::Widget(1)),
        Err(EditorError::JsonPending)
    );
    assert_eq!(
        editor.select(Selection {
            view: 1,
            widget: Some(1)
        }),
        Err(EditorError::JsonPending)
    );
    assert_eq!(editor.discard_raw(), Err(EditorError::JsonPending));
    assert!(matches!(editor.begin_save(), Err(EditorError::JsonPending)));
    assert_eq!(editor.export_json(), Err(EditorError::JsonPending));
    assert_eq!(editor.export_binary(), Err(EditorError::JsonPending));
    assert_eq!(editor.stamp(), stamp);
    assert_eq!(editor.history_len(), (0, 0));
}

#[test]
fn semantic_inspection_checks_exact_candidate_indices_and_duplicate_parent_view_ids() {
    let mut large = default_config();
    large.widgets[7].display.decimals = 255;
    let report = dashboard_configurator::diagnostic::collect_candidate(&large);
    let old_location = report.issues[0].location;
    let mut editor = Editor::default();
    let mut small = default_config();
    small.widgets.truncate(1);
    small.views.push(small.views[0].clone()).unwrap();
    small.widgets[0].display.decimals = 255;
    parse_text(&mut editor, serde_json::to_string(&small).unwrap());
    assert_eq!(editor.inspection_target(old_location), Err(EditorError::InvalidIndex));
    let JsonState::Candidate { report, .. } = editor.json_draft().unwrap().state() else {
        panic!()
    };
    let location = report
        .issues
        .iter()
        .find(|issue| issue.location.field() == Field::Decimals)
        .unwrap()
        .location;
    let target = editor.inspection_target(location).unwrap();
    assert_eq!(editor.inspect(target).unwrap().widget_index, Some(0));
    assert!(!editor.inspect(target).unwrap().id_lookup_unambiguous);
    editor.discard_json().unwrap();
    assert_eq!(editor.inspect(target), Err(EditorError::StaleToken));
}

#[test]
fn visual_raw_inspection_is_read_only_and_needs_a_fresh_transaction_after_repair() {
    let mut editor = Editor::default();
    editor
        .set_raw(editor.field_token(), RawField::ViewName(1), "".into())
        .unwrap();
    let revision = editor.revision();
    let raw = editor.raw_fields().to_vec();
    let handle = editor.open_json().unwrap();
    assert!(editor.json_draft().unwrap().is_read_only());
    assert_eq!(
        editor.set_json_text(handle, "{}".into()),
        Err(EditorError::InvalidDraft)
    );
    assert_eq!(editor.raw_fields(), raw);
    assert_eq!(editor.revision(), revision);
    editor.discard_json().unwrap();
    assert_eq!(editor.raw_fields(), raw);
    editor.discard_raw().unwrap();
    assert_eq!(editor.set_json_text(handle, "{}".into()), Err(EditorError::StaleToken));
    let fresh = editor.open_json().unwrap();
    assert!(!editor.json_draft().unwrap().is_read_only());
    assert!(editor.set_json_text(fresh, "{}".into()).is_ok());
}

#[test]
fn indexed_inspection_of_duplicate_ids_is_read_only_and_stales_on_new_text() {
    let mut editor = Editor::default();
    let mut config = default_config();
    config.widgets[1].id = config.widgets[0].id;
    let text = serde_json::to_string(&config).unwrap();
    parse_text(&mut editor, text.clone());
    let JsonState::Candidate { report, .. } = editor.json_draft().unwrap().state() else {
        panic!()
    };
    let issue = report
        .issues
        .iter()
        .find(|issue| issue.location.field() == Field::WidgetId)
        .unwrap();
    let target = editor.inspection_target(issue.location).unwrap();
    let before = (editor.stamp(), editor.selection(), editor.history_len());
    let resolved = editor.inspect(target).unwrap();
    assert_eq!(resolved.widget_index, Some(1));
    assert_eq!(resolved.json_path.as_deref(), Some("/widgets/1/id"));
    assert!(!resolved.id_lookup_unambiguous);
    assert_eq!((editor.stamp(), editor.selection(), editor.history_len()), before);
    assert_eq!(editor.json_draft().unwrap().text(), text);
    let handle = editor.json_draft().unwrap().handle();
    editor.set_json_text(handle, "{}".into()).unwrap();
    assert_eq!(editor.inspect(target), Err(EditorError::StaleToken));
}

#[test]
fn raw_inspection_resolves_another_invalid_field_without_selecting_or_accepting_it() {
    let mut editor = Editor::default();
    editor
        .select(Selection {
            view: 1,
            widget: Some(1),
        })
        .unwrap();
    editor
        .set_raw(
            editor.field_token(),
            RawField::Widget(1, WidgetField::Decimals),
            "bad".into(),
        )
        .unwrap();
    let target = editor.raw_inspection_target(0).unwrap();
    let before = (editor.stamp(), editor.selection(), editor.history_len());
    let resolved = editor.inspect(target).unwrap();
    assert_eq!(resolved.raw_field, Some(RawField::Widget(1, WidgetField::Decimals)));
    assert!(resolved.id_lookup_unambiguous);
    assert_eq!((editor.stamp(), editor.selection(), editor.history_len()), before);
    assert_eq!(editor.raw_fields()[0].text, "bad");
    editor.discard_raw().unwrap();
    assert_eq!(editor.inspect(target), Err(EditorError::StaleToken));
}

#[test]
fn strict_current_parser_preserves_duplicate_unknown_version_and_size_checks() {
    let text = Editor::default().export_json().unwrap();
    assert_eq!(parse_current_json_candidate(text.as_bytes()).unwrap(), default_config());
    let version_field = format!("\"version\":{VERSION}");
    for bad in [
        text.replacen(&version_field, &format!("{version_field},{version_field}"), 1),
        text.replacen(&version_field, &format!("{version_field},\"unknown\":0"), 1),
        text.replacen(
            &version_field,
            &format!("{version_field},\"unknown\":0,\"unknown\":1"),
            1,
        ),
        text.replacen("\"reference\":0.0", "\"reference\":0.0,\"reference\":0.0", 1),
        text.replacen("\"trigger_high\":0.0", "\"trigger_high\":0.0,\"trigger_high\":0.0", 1),
        text.replacen("\"trigger_high\":0.0", "\"trigger_high\":0.0,\"unknown\":0", 1),
        format!("{text} null"),
    ] {
        assert_ne!(bad, text, "fixture must contain the intended mutation");
        assert!(matches!(
            parse_current_json_candidate(bad.as_bytes()),
            Err(ConfigError::Json { .. })
        ));
    }
    for version in (0..VERSION).chain([VERSION + 1]) {
        let bad = text.replacen(&version_field, &format!("\"version\":{version}"), 1);
        assert_eq!(parse_current_json_candidate(bad.as_bytes()), Err(ConfigError::Version));
    }
    let mut bands = default_config();
    bands.widgets[0]
        .advanced
        .set_bands(&[Band::new(0.0, 1.0, Color(0xFFFF), "ONE").unwrap()])
        .unwrap();
    let text = serde_json::to_string(&bands).unwrap();
    for bad in [
        text.replacen("\"min\":0.0", "\"min\":0.0,\"min\":0.0", 1),
        text.replacen("\"min\":0.0", "\"min\":0.0,\"unknown\":0", 1),
    ] {
        assert_ne!(bad, text);
        assert!(matches!(
            parse_current_json_candidate(bad.as_bytes()),
            Err(ConfigError::Json { .. })
        ));
    }
    assert_eq!(
        parse_current_json_candidate(&vec![b' '; MAX_JSON_BYTES + 1]),
        Err(ConfigError::Length)
    );
    for depth in [127, 128, 129] {
        let bad = format!("{}0{}", "[".repeat(depth), "]".repeat(depth));
        assert!(matches!(
            parse_current_json_candidate(bad.as_bytes()),
            Err(ConfigError::Json { .. })
        ));
    }
}
