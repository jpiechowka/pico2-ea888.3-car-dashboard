use dashboard_configurator::config::*;
use dashboard_configurator::editor::*;

#[test]
fn warning_direction_changes_preserve_hysteresis_and_undo_atomically() {
    let mut editor = Editor::default();
    for id in [1, 3, 5, 8] {
        let original = editor
            .preview()
            .config()
            .widgets
            .iter()
            .find(|w| w.id == id)
            .unwrap()
            .warning
            .clone();
        let mode = match original.mode {
            LimitMode::High => LimitMode::Low,
            LimitMode::Low => LimitMode::High,
            _ => continue,
        };
        editor.apply(Edit::SetWarningMode { id, mode }).unwrap();
        let changed = editor
            .preview()
            .config()
            .widgets
            .iter()
            .find(|w| w.id == id)
            .unwrap()
            .warning
            .clone();
        assert_eq!(changed.mode, mode);
        assert_eq!((changed.trigger, changed.clear), (original.clear, original.trigger));
        editor.undo().unwrap();
        assert_eq!(
            editor
                .preview()
                .config()
                .widgets
                .iter()
                .find(|w| w.id == id)
                .unwrap()
                .warning,
            original
        );
    }
    let before = editor.preview().clone();
    assert!(
        editor
            .apply(Edit::SetWarningMode {
                id: 1,
                mode: LimitMode::LambdaDeviation
            })
            .is_err()
    );
    assert_eq!(editor.preview(), &before);
}

fn empty() -> Editor {
    let mut config = default_config();
    config.widgets.clear();
    Editor::new(config).unwrap()
}

fn placement(
    column: u8,
    row: u8,
    span: Span,
) -> Placement {
    Placement { column, row, span }
}

fn add(
    editor: &mut Editor,
    column: u8,
    row: u8,
    span: Span,
) -> u16 {
    editor
        .apply(Edit::AddWidget {
            view: editor.selection().view,
            channel: Channel::Oil,
            placement: placement(column, row, span),
        })
        .unwrap();
    editor.selection().widget.unwrap()
}

#[test]
fn rejected_move_and_resize_leave_document_selection_revision_and_history_unchanged() {
    let mut editor = empty();
    let first = add(&mut editor, 0, 0, Span::OneByOne);
    add(&mut editor, 1, 0, Span::OneByOne);
    let before = editor.preview().clone();
    let selection = editor.selection();
    let revision = editor.revision();
    let history = editor.history_len();
    for edit in [
        Edit::MoveWidget {
            id: first,
            view: 1,
            column: 1,
            row: 0,
        },
        Edit::ResizeWidget {
            id: first,
            span: Span::TwoByOne,
        },
        Edit::MoveWidget {
            id: first,
            view: 1,
            column: u8::MAX,
            row: 0,
        },
    ] {
        assert!(matches!(editor.apply(edit), Err(EditorError::Validation(_))));
        assert_eq!(editor.preview(), &before);
        assert_eq!(editor.selection(), selection);
        assert_eq!(editor.revision(), revision);
        assert_eq!(editor.history_len(), history);
    }
    editor
        .apply(Edit::MoveWidget {
            id: first,
            view: 1,
            column: 2,
            row: 0,
        })
        .unwrap();
    editor
        .apply(Edit::ResizeWidget {
            id: first,
            span: Span::TwoByTwo,
        })
        .unwrap();
    assert_eq!(
        editor.preview().config().widgets[0].placement,
        placement(2, 0, Span::TwoByTwo)
    );
}

#[test]
fn duplicate_and_delete_preserve_fields_assign_new_ids_and_repair_boot_and_selection() {
    let mut editor = empty();
    let widget = add(&mut editor, 0, 0, Span::TwoByOne);
    let mut changed = editor.preview().config().widgets[0].clone();
    changed.label = "long preserved label".try_into().unwrap();
    changed.display.decimals = 2;
    changed.warning.message = "Custom warning".try_into().unwrap();
    editor.apply(Edit::UpdateWidget(changed.clone())).unwrap();
    editor
        .apply(Edit::DuplicateWidget {
            id: widget,
            view: 1,
            placement: placement(2, 0, Span::TwoByOne),
        })
        .unwrap();
    let duplicate = editor.selection().widget.unwrap();
    assert_ne!(duplicate, widget);
    assert_eq!(editor.preview().config().widgets[1].warning, changed.warning);
    assert!(
        editor
            .apply(Edit::ResizeWidget {
                id: duplicate,
                span: Span::OneByOne
            })
            .is_err()
    );
    assert_eq!(editor.preview().config().widgets[1].label, changed.label);
    editor
        .apply(Edit::DuplicateView {
            id: 1,
            name: "Track".into(),
        })
        .unwrap();
    let copied_view = editor.selection().view;
    assert_ne!(copied_view, 1);
    let copied: Vec<_> = editor
        .preview()
        .config()
        .widgets
        .iter()
        .filter(|w| w.view == copied_view)
        .collect();
    assert_eq!(copied.len(), 2);
    assert!(copied.iter().all(|w| w.id != widget && w.id != duplicate));
    assert_ne!(copied[0].id, copied[1].id);
    assert_eq!(copied[0].label, changed.label);
    editor.apply(Edit::SetBootView(copied_view)).unwrap();
    editor.apply(Edit::DeleteView(copied_view)).unwrap();
    assert_eq!(editor.selection(), Selection { view: 1, widget: None });
    assert_eq!(editor.preview().config().boot_view, 1);
    assert_eq!(editor.preview().config().widgets.len(), 2);
    assert_eq!(editor.apply(Edit::DeleteView(1)), Err(EditorError::LastView));
    editor.apply(Edit::DeleteWidget(widget)).unwrap();
    assert_eq!(editor.preview().config().widgets.len(), 1);
}

#[test]
fn capacity_failure_is_atomic_and_ids_do_not_renumber_existing_objects() {
    let mut editor = Editor::default();
    for n in 2..=8 {
        editor
            .apply(Edit::DuplicateView {
                id: 1,
                name: format!("View {n}"),
            })
            .unwrap();
    }
    assert_eq!(editor.preview().config().widgets.len(), 64);
    round_trip(&mut editor);
    let before = editor.preview().clone();
    let revision = editor.revision();
    assert_eq!(
        editor.apply(Edit::DuplicateView {
            id: 1,
            name: "Overflow".into()
        }),
        Err(EditorError::ViewCapacity)
    );
    assert_eq!(
        editor.apply(Edit::DuplicateWidget {
            id: 1,
            view: 1,
            placement: placement(0, 0, Span::OneByOne)
        }),
        Err(EditorError::WidgetCapacity)
    );
    assert_eq!(editor.preview(), &before);
    assert_eq!(editor.revision(), revision);
    assert_eq!(editor.preview().config().widgets[0].id, 1);
}

#[test]
fn ordered_views_wrap_selection_on_delete_and_reordering_is_undoable() {
    let mut editor = empty();
    editor.apply(Edit::AddView { name: "Second".into() }).unwrap();
    let second = editor.selection().view;
    editor.apply(Edit::AddView { name: "Third".into() }).unwrap();
    let third = editor.selection().view;
    editor.apply(Edit::ReorderView { id: third, index: 0 }).unwrap();
    assert_eq!(
        editor.preview().config().views.iter().map(|v| v.id).collect::<Vec<_>>(),
        vec![third, 1, second]
    );
    editor.apply(Edit::DeleteView(second)).unwrap();
    assert_eq!(editor.selection().view, third);
    editor.undo().unwrap();
    assert_eq!(editor.preview().config().views.len(), 3);
    editor.redo().unwrap();
    assert_eq!(editor.preview().config().views.len(), 2);
}

#[test]
fn undo_is_bounded_and_a_new_accepted_edit_discards_redo() {
    let mut editor = empty();
    for n in 1..=40 {
        editor
            .apply(Edit::RenameView {
                id: 1,
                name: format!("Name {n}"),
            })
            .unwrap();
    }
    for _ in 0..32 {
        editor.undo().unwrap();
    }
    assert_eq!(editor.preview().config().views[0].name.as_str(), "Name 8");
    assert_eq!(editor.undo(), Err(EditorError::NoUndo));
    editor.redo().unwrap();
    assert_eq!(editor.preview().config().views[0].name.as_str(), "Name 9");
    editor
        .apply(Edit::RenameView {
            id: 1,
            name: "New branch".into(),
        })
        .unwrap();
    assert_eq!(editor.redo(), Err(EditorError::NoRedo));
}

#[test]
fn selection_and_typed_edits_reject_dangling_ids_and_invalid_values() {
    let mut editor = Editor::default();
    assert_eq!(
        editor.select(Selection {
            view: 200,
            widget: None
        }),
        Err(EditorError::MissingView(200))
    );
    assert_eq!(
        editor.select(Selection {
            view: 1,
            widget: Some(200)
        }),
        Err(EditorError::MissingWidget(200))
    );
    let before = editor.preview().clone();
    assert!(
        editor
            .apply(Edit::SetFuel(Fuel::Stoichiometric { ratio: f32::NAN }))
            .is_err()
    );
    assert!(editor.apply(Edit::SetBootView(200)).is_err());
    assert!(
        editor
            .apply(Edit::RenameView {
                id: 1,
                name: "bad\nname".into()
            })
            .is_err()
    );
    assert_eq!(editor.preview(), &before);
    editor
        .apply(Edit::SetFuel(Fuel::Stoichiometric { ratio: 14.0 }))
        .unwrap();
    assert_eq!(editor.preview().stoich(), 14.0);
}

#[test]
fn partial_numeric_text_is_retained_without_replacing_preview_or_allowing_export() {
    let mut editor = Editor::default();
    editor
        .select(Selection {
            view: 1,
            widget: Some(5),
        })
        .unwrap();
    let preview = editor.preview().clone();
    let history = editor.history_len();
    let old_revision = editor.revision();
    editor
        .set_raw(
            editor.field_token(),
            RawField::Widget(5, WidgetField::WarningTrigger),
            "-".into(),
        )
        .unwrap();
    assert_eq!(editor.raw_fields()[0].text, "-");
    assert!(editor.has_invalid_draft());
    assert!(editor.draft_error().is_some());
    assert_eq!(editor.preview(), &preview);
    assert_eq!(editor.history_len(), history);
    assert_ne!(editor.revision(), old_revision);
    assert_eq!(
        editor.select(Selection { view: 1, widget: None }),
        Err(EditorError::InvalidDraft)
    );
    assert_eq!(editor.apply(Edit::DeleteWidget(5)), Err(EditorError::InvalidDraft));
    assert_eq!(editor.undo(), Err(EditorError::InvalidDraft));
    assert_eq!(editor.export_json(), Err(EditorError::InvalidDraft));
    assert_eq!(editor.export_binary(), Err(EditorError::InvalidDraft));
    editor
        .set_raw(
            editor.field_token(),
            RawField::Widget(5, WidgetField::WarningTrigger),
            "140".into(),
        )
        .unwrap();
    assert!(!editor.has_invalid_draft());
    assert_eq!(editor.preview().config().widgets[4].warning.trigger, 140.0);
    assert_eq!(editor.history_len().0, history.0 + 1);
    assert!(editor.export_json().is_ok());
    editor.undo().unwrap();
    assert_eq!(editor.preview(), &preview);
}

#[test]
fn interdependent_invalid_fields_commit_together_and_explicit_discard_cancels_old_events() {
    let mut editor = Editor::default();
    editor
        .select(Selection {
            view: 1,
            widget: Some(5),
        })
        .unwrap();
    let original = editor.preview().clone();
    editor
        .set_raw(
            editor.field_token(),
            RawField::Widget(5, WidgetField::WarningTrigger),
            "100".into(),
        )
        .unwrap();
    assert!(editor.has_invalid_draft()); // Existing clear 125 is above the proposed trigger.
    editor
        .set_raw(
            editor.field_token(),
            RawField::Widget(5, WidgetField::WarningClear),
            "90".into(),
        )
        .unwrap();
    assert!(!editor.has_invalid_draft());
    let warning = &editor.preview().config().widgets[4].warning;
    assert_eq!((warning.trigger, warning.clear), (100.0, 90.0));
    editor.undo().unwrap();
    assert_eq!(editor.preview(), &original);
    editor
        .set_raw(
            editor.field_token(),
            RawField::Widget(5, WidgetField::Label),
            "Far too long for this narrow widget".into(),
        )
        .unwrap();
    assert_eq!(editor.raw_fields()[0].text, "Far too long for this narrow widget");
    let old_field = editor.field_token();
    editor.discard_raw().unwrap();
    assert!(!editor.has_invalid_draft());
    assert_eq!(
        editor.set_raw(old_field, RawField::Widget(5, WidgetField::Label), "Late".into()),
        Err(EditorError::StaleToken)
    );
    assert_eq!(editor.preview(), &original);
}

#[test]
fn raw_nonfinite_values_and_fields_from_old_selections_cannot_enter_exports() {
    let mut editor = Editor::default();
    editor
        .apply(Edit::SetFuel(Fuel::Stoichiometric { ratio: 14.7 }))
        .unwrap();
    for raw in ["NaN", "inf", "-inf", "", "1e999"] {
        editor
            .set_raw(editor.field_token(), RawField::Fuel(FuelField::Ratio), raw.into())
            .unwrap();
        assert_eq!(editor.raw_fields()[0].text, raw);
        assert!(editor.export_json().is_err());
        assert_eq!(editor.preview().stoich(), 14.7);
    }
    editor.discard_raw().unwrap();
    let old_field = editor.field_token();
    editor
        .select(Selection {
            view: 1,
            widget: Some(2),
        })
        .unwrap();
    assert_eq!(
        editor.set_raw(old_field, RawField::ViewName(1), "Old field".into()),
        Err(EditorError::StaleToken)
    );
    assert_eq!(
        editor.set_raw(
            editor.field_token(),
            RawField::Widget(5, WidgetField::Label),
            "Wrong".into()
        ),
        Err(EditorError::SelectionMismatch)
    );
    assert_eq!(
        editor.set_raw(
            editor.field_token(),
            RawField::Fuel(FuelField::Ratio),
            "x".repeat(MAX_RAW_TEXT_BYTES + 1)
        ),
        Err(EditorError::RawTooLong)
    );
}

#[test]
fn pointer_commit_requires_current_request_original_pointer_and_revision() {
    let mut editor = empty();
    let widget = add(&mut editor, 0, 0, Span::OneByOne);
    let old = editor.begin_pointer(7, PointerTarget::Widget(widget)).unwrap();
    let current = editor.begin_pointer(7, PointerTarget::Widget(widget)).unwrap();
    assert_eq!(
        editor.commit_pointer(old, 7, placement(1, 0, Span::OneByOne)),
        Err(EditorError::StaleToken)
    );
    assert_eq!(
        editor.commit_pointer(current, 8, placement(1, 0, Span::OneByOne)),
        Err(EditorError::StaleToken)
    );
    assert_eq!(editor.active_pointer(), Some(current));
    editor
        .commit_pointer(current, 7, placement(2, 0, Span::TwoByTwo))
        .unwrap();
    assert_eq!(
        editor.preview().config().widgets[0].placement,
        placement(2, 0, Span::TwoByTwo)
    );
    assert_eq!(editor.active_pointer(), None);
    assert_eq!(
        editor.commit_pointer(current, 7, placement(0, 0, Span::OneByOne)),
        Err(EditorError::StaleToken)
    );
}

#[test]
fn pointer_cancel_and_rejected_drop_do_not_modify_the_document() {
    let mut editor = empty();
    let widget = add(&mut editor, 0, 0, Span::OneByOne);
    add(&mut editor, 1, 0, Span::OneByOne);
    let before = editor.preview().clone();
    let pointer = editor.begin_pointer(1, PointerTarget::Widget(widget)).unwrap();
    assert!(
        editor
            .commit_pointer(pointer, 1, placement(1, 0, Span::OneByOne))
            .is_err()
    );
    assert_eq!(editor.active_pointer(), None);
    assert_eq!(editor.preview(), &before);
    let pointer = editor
        .begin_pointer(2, PointerTarget::Library(Channel::Lambda))
        .unwrap();
    editor.cancel_pointer(pointer).unwrap();
    assert_eq!(
        editor.commit_pointer(pointer, 2, placement(2, 0, Span::OneByOne)),
        Err(EditorError::StaleToken)
    );
    assert_eq!(editor.preview(), &before);
    let pointer = editor
        .begin_pointer(2, PointerTarget::Library(Channel::Lambda))
        .unwrap();
    editor
        .commit_pointer(pointer, 2, placement(2, 0, Span::OneByOne))
        .unwrap();
    assert_eq!(editor.preview().config().widgets[2].channel, Channel::Lambda);
}

#[test]
fn edits_view_changes_undo_and_raw_input_retire_existing_pointer_transactions() {
    let mut editor = empty();
    let widget = add(&mut editor, 0, 0, Span::OneByOne);
    let token = editor.begin_pointer(1, PointerTarget::Widget(widget)).unwrap();
    editor.apply(Edit::AddView { name: "Other".into() }).unwrap();
    assert_eq!(
        editor.commit_pointer(token, 1, placement(1, 0, Span::OneByOne)),
        Err(EditorError::StaleToken)
    );
    editor
        .select(Selection {
            view: 1,
            widget: Some(widget),
        })
        .unwrap();
    let token = editor.begin_pointer(1, PointerTarget::Widget(widget)).unwrap();
    editor.undo().unwrap();
    assert_eq!(
        editor.commit_pointer(token, 1, placement(1, 0, Span::OneByOne)),
        Err(EditorError::StaleToken)
    );
    editor
        .select(Selection {
            view: 1,
            widget: Some(widget),
        })
        .unwrap();
    let token = editor.begin_pointer(1, PointerTarget::Widget(widget)).unwrap();
    editor
        .set_raw(
            editor.field_token(),
            RawField::Widget(widget, WidgetField::Decimals),
            "-".into(),
        )
        .unwrap();
    assert_eq!(editor.active_pointer(), None);
    assert_eq!(
        editor.commit_pointer(token, 1, placement(1, 0, Span::OneByOne)),
        Err(EditorError::StaleToken)
    );
    assert_eq!(
        editor.begin_pointer(1, PointerTarget::Widget(widget)),
        Err(EditorError::InvalidDraft)
    );
}

#[test]
fn imported_replacement_is_one_undoable_transaction_and_cancels_same_document_field_events() {
    let mut editor = empty();
    let before = editor.preview().clone();
    let json = Editor::default().export_json().unwrap();
    let token = editor.begin_import().unwrap();
    editor
        .complete_import(token, ImportFormat::Json, json.as_bytes())
        .unwrap();
    assert_eq!(editor.preview().config().widgets.len(), 8);
    assert_eq!(editor.selection(), Selection { view: 1, widget: None });
    editor.undo().unwrap();
    assert_eq!(editor.preview(), &before);
    editor.redo().unwrap();
    let field = editor.field_token();
    let token = editor.begin_import().unwrap();
    editor
        .complete_import(token, ImportFormat::Json, json.as_bytes())
        .unwrap();
    assert_eq!(
        editor.set_raw(field, RawField::ViewName(1), "Late".into()),
        Err(EditorError::StaleToken)
    );
}

#[test]
fn older_or_edited_import_requests_cannot_replace_current_state() {
    let mut editor = empty();
    let bytes = Editor::default().export_binary().unwrap();
    let old = editor.begin_import().unwrap();
    let current = editor.begin_import().unwrap();
    assert_eq!(
        editor.complete_import(old, ImportFormat::Binary, &bytes),
        Err(EditorError::StaleToken)
    );
    editor.complete_import(current, ImportFormat::Binary, &bytes).unwrap();
    let old = editor.begin_import().unwrap();
    editor
        .apply(Edit::RenameView {
            id: 1,
            name: "New edit".into(),
        })
        .unwrap();
    assert_eq!(
        editor.complete_import(old, ImportFormat::Binary, &bytes),
        Err(EditorError::StaleToken)
    );
    assert_eq!(editor.preview().config().views[0].name.as_str(), "New edit");
    let old = editor.begin_import().unwrap();
    editor
        .set_raw(editor.field_token(), RawField::ViewName(1), "".into())
        .unwrap();
    assert_eq!(
        editor.complete_import(old, ImportFormat::Binary, &bytes),
        Err(EditorError::StaleToken)
    );
    assert_eq!(editor.begin_import(), Err(EditorError::InvalidDraft));
    editor.discard_raw().unwrap();
    let token = editor.begin_import().unwrap();
    editor.cancel_import(token).unwrap();
    assert_eq!(
        editor.complete_import(token, ImportFormat::Binary, &bytes),
        Err(EditorError::StaleToken)
    );
}

#[test]
fn malformed_oversize_duplicate_unknown_and_trailing_imports_are_atomic() {
    let mut editor = empty();
    let good = Editor::default().export_json().unwrap();
    let bad_inputs = [
        good.replacen("\"version\": 1", "\"version\": 1, \"version\": 1", 1),
        good.replacen("\"version\": 1", "\"version\": 1, \"unknown\": 1", 1),
        format!("{good} null"),
        good.replacen("14.7", "null", 1)
            .replacen("\"trigger\": 2.5", "\"trigger\": null", 1),
        " ".repeat(MAX_JSON_BYTES + 1),
    ];
    let before = editor.preview().clone();
    let revision = editor.revision();
    for bytes in bad_inputs {
        let token = editor.begin_import().unwrap();
        assert!(
            editor
                .complete_import(token, ImportFormat::Json, bytes.as_bytes())
                .is_err()
        );
        assert_eq!(editor.preview(), &before);
        assert_eq!(editor.revision(), revision);
        assert_eq!(editor.history_len(), (0, 0));
    }
    let mut corrupt = Editor::default().export_binary().unwrap();
    corrupt[12] ^= 1;
    let token = editor.begin_import().unwrap();
    assert_eq!(
        editor.complete_import(token, ImportFormat::Binary, &corrupt),
        Err(EditorError::Codec(ConfigError::Checksum))
    );
    let token = editor.begin_import().unwrap();
    assert_eq!(
        editor.complete_import(token, ImportFormat::Binary, &vec![0; MAX_FILE_BYTES + 1]),
        Err(EditorError::Codec(ConfigError::Length))
    );
    assert_eq!(editor.preview(), &before);
}

#[test]
fn persistence_requires_success_acknowledgment_and_failed_storage_keeps_exports_usable() {
    let mut editor = Editor::default();
    assert_eq!(editor.save_status(), SaveStatus::Dirty);
    let request = editor.begin_save().unwrap();
    assert_eq!(editor.save_status(), SaveStatus::Saving);
    assert_eq!(editor.last_saved_revision(), None);
    let revision = editor.revision();
    editor.acknowledge_save(request.token(), Ok(())).unwrap();
    assert_eq!(editor.save_status(), SaveStatus::Saved);
    assert_eq!(editor.last_saved_revision(), Some(revision));
    for failure in [
        StorageFailure::Denied,
        StorageFailure::Quota,
        StorageFailure::Unavailable,
    ] {
        editor
            .apply(Edit::RenameView {
                id: 1,
                name: format!("{failure:?}"),
            })
            .unwrap();
        let request = editor.begin_save().unwrap();
        assert_eq!(
            editor.acknowledge_save(request.token(), Err(failure)),
            Err(EditorError::Storage(failure))
        );
        assert_eq!(editor.save_status(), SaveStatus::Failed(failure));
        assert_eq!(editor.last_saved_revision(), Some(revision));
        assert!(editor.export_json().is_ok());
        assert!(editor.export_binary().is_ok());
    }
    editor
        .set_raw(editor.field_token(), RawField::ViewName(1), "".into())
        .unwrap();
    assert_eq!(editor.save_status(), SaveStatus::InvalidDraft);
    assert!(matches!(editor.begin_save(), Err(EditorError::InvalidDraft)));
}

#[test]
fn late_save_acknowledgments_never_mark_newer_edits_or_requests_saved() {
    let mut editor = Editor::default();
    let old_revision = editor.revision();
    let old = editor.begin_save().unwrap();
    let current = editor.begin_save().unwrap();
    assert_eq!(
        editor.acknowledge_save(old.token(), Ok(())),
        Err(EditorError::StaleToken)
    );
    assert_eq!(editor.save_status(), SaveStatus::Saving);
    editor
        .apply(Edit::RenameView {
            id: 1,
            name: "New edit".into(),
        })
        .unwrap();
    assert_eq!(
        editor.acknowledge_save(current.token(), Ok(())),
        Err(EditorError::StaleToken)
    );
    assert_eq!(editor.last_saved_revision(), Some(old_revision));
    assert_eq!(editor.save_status(), SaveStatus::Dirty);
    let request = editor.begin_save().unwrap();
    editor
        .set_raw(editor.field_token(), RawField::ViewName(1), "".into())
        .unwrap();
    assert_eq!(
        editor.acknowledge_save(request.token(), Ok(())),
        Err(EditorError::StaleToken)
    );
    assert_eq!(editor.save_status(), SaveStatus::InvalidDraft);
}

#[test]
fn pointer_selection_save_survives_noop_cancel_rejection_and_successful_move() {
    for outcome in 0..4 {
        for save_before_release in [false, true] {
            let mut editor = empty();
            let first = add(&mut editor, 0, 0, Span::OneByOne);
            add(&mut editor, 1, 0, Span::OneByOne);
            editor
                .apply(Edit::RenameView {
                    id: 1,
                    name: "Pending edit".into(),
                })
                .unwrap();
            let previous = editor.begin_save().unwrap();
            let pointer = editor.begin_pointer(7, PointerTarget::Widget(first)).unwrap();
            assert!(!editor.save_is_current(previous.token()));
            // The browser adapter must schedule this replacement even if the
            // subsequent placement does not change the document.
            let selected = editor.begin_save().unwrap();
            assert_eq!(editor.active_pointer(), Some(pointer));
            if save_before_release {
                editor.acknowledge_save(selected.token(), Ok(())).unwrap();
            }
            match outcome {
                0 => {
                    editor
                        .commit_pointer(pointer, 7, placement(0, 0, Span::OneByOne))
                        .unwrap();
                }
                1 => editor.cancel_pointer(pointer).unwrap(),
                2 => {
                    assert!(
                        editor
                            .commit_pointer(pointer, 7, placement(1, 0, Span::OneByOne))
                            .is_err()
                    );
                }
                _ => {
                    editor
                        .commit_pointer(pointer, 7, placement(2, 1, Span::OneByOne))
                        .unwrap();
                }
            }
            assert_eq!(editor.active_pointer(), None);
            assert!(!editor.save_is_current(previous.token()));
            let final_save = if outcome == 3 {
                assert!(!editor.save_is_current(selected.token()));
                editor.begin_save().unwrap()
            } else {
                selected
            };
            if !save_before_release || outcome == 3 {
                assert!(editor.save_is_current(final_save.token()));
                editor.acknowledge_save(final_save.token(), Ok(())).unwrap();
            }
            assert_eq!(editor.save_status(), SaveStatus::Saved);
            let restored = Editor::from_saved_state(final_save.json().as_bytes()).unwrap();
            assert_eq!(restored.preview(), editor.preview());
            assert_eq!(
                restored.selection(),
                Selection {
                    view: 1,
                    widget: Some(first)
                }
            );
            assert_eq!(restored.preview().config().views[0].name, "Pending edit");
        }
    }
}

#[test]
fn saved_envelope_restores_selection_separately_from_boot_view_and_rejects_corruption() {
    let mut editor = empty();
    editor
        .apply(Edit::AddView {
            name: "Selected".into(),
        })
        .unwrap();
    let selected_view = editor.selection().view;
    let widget = add(&mut editor, 0, 0, Span::OneByOne);
    let request = editor.begin_save().unwrap();
    let restored = Editor::from_saved_state(request.json().as_bytes()).unwrap();
    assert_eq!(restored.preview(), editor.preview());
    assert_eq!(restored.preview().config().boot_view, 1);
    assert_eq!(
        restored.selection(),
        Selection {
            view: selected_view,
            widget: Some(widget)
        }
    );
    assert_eq!(restored.save_status(), SaveStatus::Saved);
    assert_eq!(restored.history_len(), (0, 0));
    let value: serde_json::Value = serde_json::from_str(request.json()).unwrap();
    for (field, bad) in [
        ("editor_version", 2),
        ("selected_view", 200),
        ("selected_widget", 60000),
    ] {
        let mut corrupt = value.clone();
        corrupt[field] = bad.into();
        assert!(Editor::from_saved_state(serde_json::to_string(&corrupt).unwrap().as_bytes()).is_err());
    }
    for text in [
        request
            .json()
            .replacen("\"editor_version\":1", "\"editor_version\":1,\"editor_version\":1", 1),
        request
            .json()
            .replacen("\"editor_version\":1", "\"editor_version\":1,\"extra\":1", 1),
        format!("{} null", request.json()),
        " ".repeat(MAX_SAVED_BYTES + 1),
    ] {
        assert!(Editor::from_saved_state(text.as_bytes()).is_err());
    }
}

#[test]
fn replacing_editor_instance_cannot_reuse_tokens_from_an_older_instance() {
    let mut old = Editor::default();
    let mut replacement = Editor::default();
    assert_eq!(
        replacement.set_raw(old.field_token(), RawField::ViewName(1), "Wrong owner".into()),
        Err(EditorError::StaleToken)
    );
    let old_pointer = old.begin_pointer(7, PointerTarget::Widget(1)).unwrap();
    let current_pointer = replacement.begin_pointer(7, PointerTarget::Widget(1)).unwrap();
    assert_eq!(
        replacement.commit_pointer(old_pointer, 7, placement(0, 0, Span::OneByOne)),
        Err(EditorError::StaleToken)
    );
    assert_eq!(replacement.active_pointer(), Some(current_pointer));
    let old_import = old.begin_import().unwrap();
    let current_import = replacement.begin_import().unwrap();
    let bytes = old.export_binary().unwrap();
    assert_eq!(
        replacement.complete_import(old_import, ImportFormat::Binary, &bytes),
        Err(EditorError::StaleToken)
    );
    replacement
        .complete_import(current_import, ImportFormat::Binary, &bytes)
        .unwrap();
    let mut old = Editor::default();
    let mut replacement = Editor::default();
    let old_save = old.begin_save().unwrap();
    let current_save = replacement.begin_save().unwrap();
    assert_eq!(
        replacement.acknowledge_save(old_save.token(), Ok(())),
        Err(EditorError::StaleToken)
    );
    assert_eq!(replacement.save_status(), SaveStatus::Saving);
    replacement.acknowledge_save(current_save.token(), Ok(())).unwrap();
}

#[test]
fn typed_inspector_callbacks_must_still_belong_to_the_current_selection() {
    let mut editor = Editor::default();
    let old = editor.field_token();
    editor
        .select(Selection {
            view: 1,
            widget: Some(1),
        })
        .unwrap();
    assert_eq!(
        editor.apply_if_current(old, Edit::SetFuel(Fuel::Stoichiometric { ratio: 14.0 })),
        Err(EditorError::StaleToken)
    );
    editor
        .apply_if_current(
            editor.field_token(),
            Edit::SetFuel(Fuel::Stoichiometric { ratio: 14.0 }),
        )
        .unwrap();
    assert_eq!(editor.preview().stoich(), 14.0);
}

fn round_trip(editor: &mut Editor) {
    let json = editor.export_json().unwrap();
    let binary = editor.export_binary().unwrap();
    let mut imported = empty();
    let request = imported.begin_import().unwrap();
    imported
        .complete_import(request, ImportFormat::Json, json.as_bytes())
        .unwrap();
    assert_eq!(imported.preview().config(), editor.preview().config());
    assert_eq!(imported.export_binary().unwrap(), binary);
    let request = imported.begin_import().unwrap();
    imported
        .complete_import(request, ImportFormat::Binary, &binary)
        .unwrap();
    assert_eq!(imported.preview(), editor.preview());
    // This is the same bounded decoder called by the firmware boot path.
    assert_eq!(decode(&binary).unwrap(), *editor.preview());
    let saved = editor.begin_save().unwrap();
    let restored = Editor::from_saved_state(saved.json().as_bytes()).unwrap();
    assert_eq!(restored.preview(), editor.preview());
    assert_eq!(restored.selection(), editor.selection());
}

#[test]
fn every_display_option_fuel_variant_warning_mode_and_span_round_trips() {
    let mut editor = Editor::default();
    editor.apply(Edit::AddView { name: "Wide".into() }).unwrap();
    add(&mut editor, 0, 0, Span::TwoByTwo);
    add(&mut editor, 2, 0, Span::TwoByOne);
    add(&mut editor, 2, 1, Span::OneByOne);
    for flags in 0_u8..32 {
        let mut widget = editor
            .preview()
            .config()
            .widgets
            .iter()
            .find(|w| w.id == 2)
            .unwrap()
            .clone();
        widget.display = DisplayOptions {
            decimals: flags % 4,
            graph: flags & 1 != 0,
            average: flags & 2 != 0,
            extrema: flags & 4 != 0,
            trend: flags & 8 != 0,
            afr: flags & 16 != 0,
            graph_range: if flags % 2 == 0 {
                GraphRange::Auto
            } else {
                GraphRange::Fixed { min: 0.7, max: 1.2 }
            },
        };
        widget.warning.enabled = flags % 2 == 0;
        widget.warning.applicability = if flags % 2 == 0 {
            Applicability::DemoOnly
        } else {
            Applicability::VehicleAndDemo
        };
        widget.warning.message = "Configured lambda".try_into().unwrap();
        editor.apply(Edit::UpdateWidget(widget)).unwrap();
        editor
            .apply(Edit::SetWarningPresentation(WarningPresentation {
                scope: if flags & 1 == 0 {
                    WarningScope::AllViews
                } else {
                    WarningScope::CurrentView
                },
                mode: if flags & 2 == 0 {
                    WarningMode::CycleOne
                } else {
                    WarningMode::ListThree
                },
                page_ms: 500 + u16::from(flags) * 250,
            }))
            .unwrap();
        let mut blend = Blend::illustrative();
        blend.percent = 35.5;
        let fuel = match flags % 5 {
            0 => Fuel::EquivalentGasoline,
            1 => Fuel::Stoichiometric { ratio: 14.0 },
            2 => {
                blend.basis = BlendBasis::Mass;
                Fuel::EthanolBlend(blend)
            }
            3 => Fuel::EthanolBlend(blend),
            _ => {
                blend.composition_origin = CompositionOrigin::Measured;
                blend.density_reference = DensityReference::CommonCelsius(20.0);
                Fuel::EthanolBlend(blend)
            }
        };
        editor.apply(Edit::SetFuel(fuel)).unwrap();
        round_trip(&mut editor);
    }
}

#[test]
fn every_catalogue_source_and_route_survives_editor_and_firmware_round_trips() {
    let mut editor = Editor::default();
    for id in DEFINITIONS {
        let entry = definition(id);
        let channel = entry.channel.unwrap_or(Channel::Oil);
        for route in [
            Route::Unknown,
            Route::DeploymentProfile,
            Route::Physical11 {
                request: 0x7E0,
                response: 0x7E8,
            },
        ] {
            let mut source = entry.source(channel);
            source.route = route;
            source.freshness_ms = 1;
            for poll in [Poll::Off, Poll::EveryMs(10000)] {
                if id == DefinitionId::Disabled && poll != Poll::Off {
                    continue;
                }
                source.poll = poll;
                editor.apply(Edit::SetSource { channel, source }).unwrap();
                round_trip(&mut editor);
            }
        }
    }
}

#[test]
fn all_raw_inspector_targets_apply_to_the_correct_field() {
    let mut editor = Editor::default();
    editor
        .select(Selection {
            view: 1,
            widget: Some(5),
        })
        .unwrap();
    let mut widget = editor.preview().config().widgets[4].clone();
    widget.display.graph_range = GraphRange::Fixed {
        min: -100.0,
        max: 2000.0,
    };
    editor.apply(Edit::UpdateWidget(widget)).unwrap();
    for (field, text) in [
        (WidgetField::Label, "Engine oil"),
        (WidgetField::Decimals, "2"),
        (WidgetField::GraphMinimum, "-50"),
        (WidgetField::GraphMaximum, "300"),
        (WidgetField::WarningMessage, "Oil warning"),
        (WidgetField::WarningTrigger, "140"),
        (WidgetField::WarningClear, "120"),
        (WidgetField::WarningLoad, "55.5"),
        (WidgetField::WarningOn, "1500"),
        (WidgetField::WarningOff, "900"),
    ] {
        editor
            .set_raw(editor.field_token(), RawField::Widget(5, field), text.into())
            .unwrap();
        assert!(!editor.has_invalid_draft(), "{field:?}");
    }
    let widget = &editor.preview().config().widgets[4];
    assert_eq!(widget.label.as_str(), "Engine oil");
    assert_eq!(widget.display.decimals, 2);
    assert_eq!(widget.display.graph_range, GraphRange::Fixed { min: -50.0, max: 300.0 });
    assert_eq!(widget.warning.message.as_str(), "Oil warning");
    assert_eq!(
        (
            widget.warning.trigger,
            widget.warning.clear,
            widget.warning.min_load_percent
        ),
        (140.0, 120.0, 55.5)
    );
    assert_eq!((widget.warning.on_ms, widget.warning.off_ms), (1500, 900));
    let mut blend = Blend::illustrative();
    blend.density_reference = DensityReference::CommonCelsius(20.0);
    editor.apply(Edit::SetFuel(Fuel::EthanolBlend(blend))).unwrap();
    for (field, text) in [
        (FuelField::Percent, "30"),
        (FuelField::GasolineRatio, "14.1"),
        (FuelField::EthanolRatio, "9.1"),
        (FuelField::GasolineDensity, "0.74"),
        (FuelField::EthanolDensity, "0.79"),
        (FuelField::ReferenceCelsius, "21"),
    ] {
        editor
            .set_raw(editor.field_token(), RawField::Fuel(field), text.into())
            .unwrap();
        assert!(!editor.has_invalid_draft(), "{field:?}");
    }
    let Fuel::EthanolBlend(blend) = editor.preview().config().fuel else {
        panic!("blend lost");
    };
    assert_eq!(
        (blend.percent, blend.gasoline_stoich, blend.ethanol_stoich),
        (30.0, 14.1, 9.1)
    );
    assert_eq!((blend.gasoline_density, blend.ethanol_density), (0.74, 0.79));
    assert_eq!(blend.density_reference, DensityReference::CommonCelsius(21.0));
    let mut source = editor.preview().config().sources[Channel::Oil.index()];
    source.route = Route::Physical11 {
        request: 0x7E0,
        response: 0x7E8,
    };
    editor
        .apply(Edit::SetSource {
            channel: Channel::Oil,
            source,
        })
        .unwrap();
    for (field, text) in [
        (SourceField::Request, "0x7e1"),
        (SourceField::Response, "0X7e9"),
        (SourceField::PollMilliseconds, "2500"),
        (SourceField::FreshnessMilliseconds, "1500"),
    ] {
        editor
            .set_raw(editor.field_token(), RawField::Source(Channel::Oil, field), text.into())
            .unwrap();
        assert!(!editor.has_invalid_draft(), "{field:?}");
    }
    let source = editor.preview().config().sources[Channel::Oil.index()];
    assert_eq!(
        source.route,
        Route::Physical11 {
            request: 0x7E1,
            response: 0x7E9
        }
    );
    assert_eq!(source.poll, Poll::EveryMs(2500));
    assert_eq!(source.freshness_ms, 1500);
    editor
        .set_raw(editor.field_token(), RawField::WarningPageMilliseconds, "2500".into())
        .unwrap();
    editor
        .set_raw(editor.field_token(), RawField::ViewName(1), "Edited".into())
        .unwrap();
    assert_eq!(editor.preview().config().warnings.page_ms, 2500);
    assert_eq!(editor.preview().config().views[0].name.as_str(), "Edited");
}

#[test]
fn widget_order_and_cross_view_moves_keep_selection_and_undo_coherent() {
    let mut editor = Editor::default();
    editor.apply(Edit::AddView { name: "Other".into() }).unwrap();
    let other = editor.selection().view;
    editor
        .apply(Edit::MoveWidget {
            id: 5,
            view: other,
            column: 0,
            row: 0,
        })
        .unwrap();
    assert_eq!(
        editor.selection(),
        Selection {
            view: other,
            widget: Some(5)
        }
    );
    editor.apply(Edit::ReorderWidget { id: 5, index: 0 }).unwrap();
    assert_eq!(editor.preview().config().widgets[0].id, 5);
    assert_eq!(editor.preview().config().widgets[1].id, 1);
    editor.undo().unwrap();
    assert_eq!(editor.preview().config().widgets[4].id, 5);
    editor.undo().unwrap();
    assert_eq!(editor.preview().config().widgets[4].view, 1);
    assert_eq!(
        editor.selection(),
        Selection {
            view: other,
            widget: None
        }
    );
}

#[test]
fn deleting_an_unselected_view_preserves_a_surviving_widget_selection() {
    let mut editor = Editor::default();
    editor
        .apply(Edit::AddView {
            name: "Remove me".into(),
        })
        .unwrap();
    let removed = editor.selection().view;
    editor.apply(Edit::SetBootView(removed)).unwrap();
    editor
        .select(Selection {
            view: 1,
            widget: Some(5),
        })
        .unwrap();
    editor.apply(Edit::DeleteView(removed)).unwrap();
    assert_eq!(
        editor.selection(),
        Selection {
            view: 1,
            widget: Some(5)
        }
    );
    assert_eq!(editor.preview().config().boot_view, 1);
}

#[test]
fn an_actual_older_write_after_a_newer_success_cannot_leave_a_false_saved_indicator() {
    let mut editor = Editor::default();
    let old_revision = editor.revision();
    let old = editor.begin_save().unwrap();
    editor
        .apply(Edit::RenameView {
            id: 1,
            name: "Newest".into(),
        })
        .unwrap();
    let current = editor.begin_save().unwrap();
    editor.acknowledge_save(current.token(), Ok(())).unwrap();
    assert_eq!(editor.save_status(), SaveStatus::Saved);
    // The acknowledgment means the old bytes were actually written, not just
    // that a stale timer woke up. Current content must stay visibly unsaved.
    assert_eq!(
        editor.acknowledge_save(old.token(), Ok(())),
        Err(EditorError::StaleToken)
    );
    assert_eq!(editor.save_status(), SaveStatus::Dirty);
    assert_eq!(editor.last_saved_revision(), Some(old_revision));
    assert_eq!(editor.preview().config().views[0].name.as_str(), "Newest");
    let retry = editor.begin_save().unwrap();
    editor.acknowledge_save(retry.token(), Ok(())).unwrap();
    assert_eq!(editor.save_status(), SaveStatus::Saved);
    assert_eq!(
        editor.acknowledge_save(retry.token(), Ok(())),
        Err(EditorError::StaleToken)
    );
    assert_eq!(editor.save_status(), SaveStatus::Saved);
}

#[test]
fn storage_adapter_can_check_a_prepared_write_before_touching_storage() {
    let mut editor = Editor::default();
    let old = editor.begin_save().unwrap();
    assert!(editor.save_is_current(old.token()));
    let new = editor.begin_save().unwrap();
    assert!(!editor.save_is_current(old.token()));
    assert!(editor.save_is_current(new.token()));
    editor
        .set_raw(editor.field_token(), RawField::ViewName(1), "".into())
        .unwrap();
    assert!(!editor.save_is_current(new.token()));
}

#[test]
fn importing_signed_zero_preserves_its_canonical_payload_instead_of_treating_it_as_a_noop() {
    let mut config = default_config();
    config.fuel = Fuel::EthanolBlend(Blend::illustrative());
    let mut editor = Editor::new(config.clone()).unwrap();
    let Fuel::EthanolBlend(blend) = &mut config.fuel else {
        unreachable!()
    };
    blend.percent = -0.0;
    let mut buffer = vec![0; MAX_FILE_BYTES];
    let validated = validate(config).unwrap();
    let bytes = encode(&validated, &mut buffer).unwrap();
    let request = editor.begin_import().unwrap();
    editor.complete_import(request, ImportFormat::Binary, bytes).unwrap();
    let Fuel::EthanolBlend(blend) = editor.preview().config().fuel else {
        unreachable!()
    };
    assert_eq!(blend.percent.to_bits(), (-0.0_f32).to_bits());
    assert_eq!(editor.export_binary().unwrap(), bytes);
}

#[test]
fn decimal_raw_json_and_saved_inputs_preserve_the_same_device_value() {
    let mut config = default_config();
    config.fuel = Fuel::EthanolBlend(Blend {
        percent: 0.0,
        ..Blend::illustrative()
    });
    let mut initial = Editor::new(config.clone()).unwrap();
    let json_template = initial.export_json().unwrap();
    let saved_template = initial.begin_save().unwrap().json().to_owned();
    assert!(json_template.contains("\"percent\": 0.0"));
    assert!(saved_template.contains("\"percent\":0.0"));
    for (text, expected_bits) in [
        ("1.0000000596046448", 0x3F80_0001),
        ("1.0000000596046447", 0x3F80_0000),
        ("1.000000059604644775390625", 0x3F80_0000),
        ("1.0000001788139343", 0x3F80_0001),
        ("0.0", 0x0000_0000),
        ("-0.0", 0x8000_0000),
        ("12.75", 0x414C_0000),
    ] {
        let mut typed = Editor::new(config.clone()).unwrap();
        typed
            .set_raw(typed.field_token(), RawField::Fuel(FuelField::Percent), text.into())
            .unwrap();
        assert!(!typed.has_invalid_draft());
        let Fuel::EthanolBlend(blend) = typed.preview().config().fuel else {
            unreachable!()
        };
        assert_eq!(blend.percent.to_bits(), expected_bits, "raw decimal {text}");
        let binary = typed.export_binary().unwrap();

        let json = json_template.replacen("\"percent\": 0.0", &format!("\"percent\": {text}"), 1);
        let mut imported = Editor::new(config.clone()).unwrap();
        let request = imported.begin_import().unwrap();
        imported
            .complete_import(request, ImportFormat::Json, json.as_bytes())
            .unwrap();
        assert_eq!(imported.export_binary().unwrap(), binary, "JSON decimal {text}");

        let saved = saved_template.replacen("\"percent\":0.0", &format!("\"percent\":{text}"), 1);
        let restored = Editor::from_saved_state(saved.as_bytes()).unwrap();
        assert_eq!(restored.export_binary().unwrap(), binary, "saved decimal {text}");
        round_trip(&mut typed);
    }
}

#[test]
fn decimal_raw_and_json_graph_bounds_reject_the_same_equal_endpoints() {
    let mut config = default_config();
    let id = config.widgets[0].id;
    config.widgets[0].display.graph_range = GraphRange::Fixed {
        min: 0.0,
        max: f32::from_bits(0x3F80_0001),
    };
    let mut typed = Editor::new(config.clone()).unwrap();
    typed
        .select(Selection {
            view: config.widgets[0].view,
            widget: Some(id),
        })
        .unwrap();
    typed
        .set_raw(
            typed.field_token(),
            RawField::Widget(id, WidgetField::GraphMinimum),
            "1.0000000596046448".into(),
        )
        .unwrap();
    let Some(EditorError::Validation(expected)) = typed.draft_error() else {
        panic!("equal graph endpoints must remain an invalid raw draft")
    };
    assert_eq!(expected.field, Field::GraphRange);
    assert_eq!(expected.code, ErrorCode::Bounds);
    assert_eq!(typed.preview().config(), &config);

    let mut imported = Editor::new(config).unwrap();
    let before = imported.export_binary().unwrap();
    let revision = imported.revision();
    let template = imported.export_json().unwrap();
    assert!(template.contains("\"min\": 0.0"));
    let json = template.replacen("\"min\": 0.0", "\"min\": 1.0000000596046448", 1);
    let request = imported.begin_import().unwrap();
    assert_eq!(
        imported.complete_import(request, ImportFormat::Json, json.as_bytes()),
        Err(EditorError::Codec(ConfigError::Validation(expected)))
    );
    assert_eq!(imported.export_binary().unwrap(), before);
    assert_eq!(imported.revision(), revision);
    assert_eq!(imported.history_len(), (0, 0));
}
