use dashboard_configurator::config::*;
use dashboard_configurator::editor::*;

fn placement(
    column: u8,
    row: u8,
    span: Span,
) -> Placement {
    Placement { column, row, span }
}

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
fn invalid_resize_can_be_corrected_without_discard_and_new_field_edits_stay_blocked() {
    let mut editor = pair();
    overlap(&mut editor);
    let token = editor.field_token();
    assert_eq!(
        editor.set_raw(token, RawField::ViewName(1), "Other".into()),
        Err(EditorError::InvalidDraft)
    );
    assert_eq!(
        editor.apply(Edit::SetFuel(Fuel::EquivalentGasoline)),
        Err(EditorError::InvalidDraft)
    );
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
    assert_eq!(editor.history_len(), (0, 0));
}
