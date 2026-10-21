//! Retained layout repair, candidate targeting and undo/persistence authority regressions.

use dashboard_configurator::config::*;
use dashboard_configurator::editor::*;

/// Returns a placement at zero-based `column`/`row` with `span`; does not validate the footprint.
fn placement(
    column: u8,
    row: u8,
    span: Span,
) -> Placement {
    Placement { column, row, span }
}

/// Returns adjacent Boost/IAT widgets with deterministic IDs and free cells for repair; invalid fixture panics.
fn pair() -> Editor {
    let mut config = default_config();
    config.widgets.clear();
    config
        .widgets
        .push(default_widget(Channel::Boost, 1, 1, placement(0, 0, Span::OneByOne)))
        .unwrap();
    config
        .widgets
        .push(default_widget(Channel::Iat, 2, 1, placement(1, 0, Span::OneByOne)))
        .unwrap();
    Editor::new(config).unwrap()
}

/// Creates a rejected 2×2 resize in `editor` and asserts its specific overlap failure.
fn overlap(editor: &mut Editor) {
    assert!(matches!(
        editor.apply(Edit::ResizeWidget {
            id: 1,
            span: Span::TwoByTwo
        }),
        Err(EditorError::Validation(ValidationError {
            code: ErrorCode::Overlap,
            ..
        }))
    ));
}

#[test]
/// Indexed raw repair changes its exact retained owner without changing selection; oversize/stale/semantic-only targets
/// fail.
fn guarded_raw_repair_of_another_retained_owner_preserves_inspection_selection() {
    let mut editor = pair();
    overlap(&mut editor);
    editor
        .select(Selection {
            view: 1,
            widget: Some(1),
        })
        .unwrap();
    editor
        .set_raw(
            editor.field_token(),
            RawField::Widget(1, WidgetField::Label),
            "ONE".into(),
        )
        .unwrap();
    editor
        .select(Selection {
            view: 1,
            widget: Some(2),
        })
        .unwrap();
    editor
        .set_raw(
            editor.field_token(),
            RawField::Widget(2, WidgetField::Label),
            "TWO".into(),
        )
        .unwrap();
    let target = editor.raw_inspection_target(0).unwrap();
    let selection = editor.selection();
    let revision = editor.revision();
    let accepted = editor.preview().clone();
    assert_eq!(
        editor.set_inspected_raw(target, "x".repeat(MAX_RAW_TEXT_BYTES + 1)),
        Err(EditorError::RawTooLong)
    );
    assert_eq!(editor.revision(), revision);
    assert_eq!(editor.raw_fields()[0].text, "ONE");
    assert_eq!(
        editor.set_raw(
            editor.field_token(),
            RawField::Widget(1, WidgetField::Label),
            "FIX".into()
        ),
        Err(EditorError::SelectionMismatch)
    );
    editor.set_inspected_raw(target, "FIX".into()).unwrap();
    assert_eq!(editor.selection(), selection);
    assert_ne!(editor.revision(), revision);
    assert_eq!(editor.preview(), &accepted);
    assert_eq!(editor.raw_fields()[0].text, "FIX");
    assert_eq!(editor.raw_fields()[1].text, "TWO");
    assert_eq!(
        editor.set_inspected_raw(target, "LATE".into()),
        Err(EditorError::StaleToken)
    );
    let report = editor.diagnostic().unwrap().validation_report();
    let semantic = editor.inspection_target(report.issues[0].location).unwrap();
    assert_eq!(
        editor.set_inspected_raw(semantic, "ARBITRARY".into()),
        Err(EditorError::FieldUnavailable)
    );
    editor
        .apply(Edit::MoveWidget {
            id: 1,
            view: 1,
            column: 2,
            row: 0,
        })
        .unwrap();
    assert_eq!(editor.preview().config().widgets[0].label, "FIX");
    assert_eq!(editor.preview().config().widgets[1].label, "TWO");
    assert!(editor.raw_fields().is_empty());
}

#[test]
/// Placement selects the moved widget even on retained overlap; undo/discard restore the previous selection.
fn placing_a_focused_widget_selects_it_and_undo_restores_previous_selection() {
    let mut editor = pair();
    editor
        .select(Selection {
            view: 1,
            widget: Some(1),
        })
        .unwrap();
    editor
        .apply(Edit::PlaceWidget {
            id: 2,
            placement: placement(2, 0, Span::OneByOne),
        })
        .unwrap();
    assert_eq!(
        editor.selection(),
        Selection {
            view: 1,
            widget: Some(2)
        }
    );
    editor.undo().unwrap();
    assert_eq!(
        editor.selection(),
        Selection {
            view: 1,
            widget: Some(1)
        }
    );
    assert!(
        editor
            .apply(Edit::PlaceWidget {
                id: 2,
                placement: placement(0, 0, Span::OneByOne)
            })
            .is_err()
    );
    assert_eq!(
        editor.selection().widget,
        Some(2),
        "the moved overlapping widget must be selectable above its peers"
    );
    editor.discard_raw().unwrap();
    assert_eq!(editor.selection().widget, Some(1));
}

#[test]
/// Dragging the covered widget admits the original rejected resize in one history step and retires old
/// field/save/pointer tokens.
fn drag_repairs_visible_overlap_without_losing_rejected_resize() {
    let mut editor = pair();
    let before = editor.export_binary().unwrap();
    let stale_save = editor.begin_save().unwrap();
    overlap(&mut editor);
    let stale_field = editor.field_token();
    assert_eq!(editor.export_binary(), Err(EditorError::InvalidDraft));
    let drag = editor
        .begin_pointer(7, PointerTarget::Widget(2))
        .expect("overlap must remain draggable");
    editor.commit_pointer(drag, 7, placement(3, 0, Span::OneByOne)).unwrap();
    assert!(!editor.has_invalid_draft());
    assert_eq!(editor.preview().config().widgets[0].placement.span, Span::TwoByTwo);
    assert_eq!(editor.preview().config().widgets[1].placement.column, 3);
    assert_eq!(editor.selection().widget, Some(2));
    assert_eq!(editor.history_len(), (1, 0));
    assert!(!editor.save_is_current(stale_save.token()));
    assert_eq!(
        editor.apply_if_current(stale_field, Edit::DeleteWidget(1)),
        Err(EditorError::StaleToken)
    );
    assert_eq!(
        editor.commit_pointer(drag, 7, placement(2, 0, Span::OneByOne)),
        Err(EditorError::StaleToken)
    );
    let recovered = editor.export_binary().unwrap();
    editor.undo().unwrap();
    assert_eq!(editor.export_binary().unwrap(), before);
    editor.redo().unwrap();
    assert_eq!(editor.export_binary().unwrap(), recovered);
}

#[test]
/// Partial repairs retain candidate movement without accepted publication; final deletion admits one combined history
/// entry.
fn multiple_collisions_can_be_repaired_in_steps_with_export_blocked_between_them() {
    let mut editor = pair();
    editor
        .apply(Edit::AddWidget {
            view: 1,
            channel: Channel::Oil,
            placement: placement(0, 1, Span::OneByOne),
        })
        .unwrap();
    let accepted = editor.preview().clone();
    let history = editor.history_len();
    overlap(&mut editor);
    assert!(matches!(
        editor.apply(Edit::MoveWidget {
            id: 2,
            view: 1,
            column: 3,
            row: 0
        }),
        Err(EditorError::Validation(_))
    ));
    assert_eq!(editor.preview(), &accepted);
    assert_eq!(editor.history_len(), history);
    assert_eq!(editor.diagnostic().unwrap().config.widgets[1].placement.column, 3);
    assert_eq!(editor.begin_save().err(), Some(EditorError::InvalidDraft));
    editor.apply(Edit::DeleteWidget(3)).unwrap();
    assert!(editor.diagnostic().is_none());
    assert_eq!(editor.preview().config().widgets.len(), 2);
    assert_eq!(editor.preview().config().widgets[0].placement.span, Span::TwoByTwo);
    assert_eq!(editor.history_len().0, history.0 + 1);
}

#[test]
/// New overlapping candidate widgets remain selectable and cancellable while library additions/export stay blocked;
/// discard retires pointers.
fn rejected_new_widget_is_selectable_movable_and_cancellable() {
    let mut editor = pair();
    let before = editor.export_binary().unwrap();
    assert!(
        editor
            .apply(Edit::AddWidget {
                view: 1,
                channel: Channel::Oil,
                placement: placement(0, 0, Span::OneByOne)
            })
            .is_err()
    );
    editor
        .select(Selection {
            view: 1,
            widget: Some(3),
        })
        .expect("new candidate widget is a real draft target");
    let drag = editor.begin_pointer(2, PointerTarget::Widget(3)).unwrap();
    editor.cancel_pointer(drag).unwrap();
    assert_eq!(editor.diagnostic().unwrap().config.widgets.len(), 3);
    assert_eq!(editor.preview().config().widgets.len(), 2);
    assert_eq!(
        editor.begin_pointer(3, PointerTarget::Library(Channel::Egt)),
        Err(EditorError::InvalidDraft)
    );
    editor.discard_raw().unwrap();
    assert_eq!(editor.selection().widget, None);
    assert_eq!(editor.export_binary().unwrap(), before);
    assert_eq!(
        editor.commit_pointer(drag, 2, placement(3, 0, Span::OneByOne)),
        Err(EditorError::StaleToken)
    );
}

#[test]
/// Raw view-name changes survive multiple rejected resize repairs and enter accepted history only with valid layout.
fn invalid_resize_preserves_new_field_edits_until_atomic_layout_repair() {
    let mut editor = pair();
    overlap(&mut editor);
    let token = editor.field_token();
    editor.set_raw(token, RawField::ViewName(1), "Other".into()).unwrap();
    assert!(matches!(
        editor.apply(Edit::SetFuel(Fuel::EquivalentGasoline)),
        Err(EditorError::Validation(_))
    ));
    // A complete raw value may still await the layout repair; do not drop it.
    editor
        .apply(Edit::ResizeWidget {
            id: 1,
            span: Span::TwoByOne,
        })
        .unwrap_err();
    editor
        .apply(Edit::ResizeWidget {
            id: 1,
            span: Span::OneByOne,
        })
        .unwrap();
    assert!(!editor.has_invalid_draft());
    assert_ne!(editor.field_token(), token);
    assert_eq!(editor.history_len(), (1, 0));
    assert_eq!(editor.preview().config().views[0].name, "Other");
}
