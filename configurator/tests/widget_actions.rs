use dashboard_configurator::config::*;
use dashboard_configurator::editor::*;
use dashboard_configurator::geometry::{PanelPoint, validate_draft_drop};
use dashboard_configurator::widget_actions::{Direction, WidgetMenu, menu_position};

#[test]
fn duplicate_widget_copies_advanced_settings_with_independent_identity_and_warning_state() {
    use dashboard_configurator::simulator::ControlSimulator;
    use dashboard_core::alarms::RuleIndex;
    let mut raw = default_config();
    raw.widgets.truncate(1);
    raw.widgets[0].advanced.smoothing_ms = 250;
    raw.widgets[0].advanced.peak_mode = PeakSelection::Maximum;
    raw.widgets[0].warning.enabled = true;
    raw.widgets[0].warning.trigger = 2.;
    raw.widgets[0].warning.clear = 1.;
    raw.widgets[0].warning.on_ms = 1;
    let mut e = Editor::new(raw).unwrap();
    let original = e.preview().config().widgets[0].clone();
    e.apply(Edit::DuplicateWidget {
        id: 1,
        view: 1,
        placement: Placement {
            column: 1,
            row: 0,
            span: Span::OneByOne,
        },
    })
    .unwrap();
    let mut copy = e.preview().config().widgets[1].clone();
    assert_ne!(copy.id, original.id);
    assert_eq!(copy.advanced, original.advanced);
    assert_eq!(copy.warning, original.warning);
    copy.advanced.smoothing_ms = 750;
    copy.advanced.peak_mode = PeakSelection::Minimum;
    copy.warning.trigger = 3.;
    copy.warning.clear = 2.8;
    e.apply(Edit::UpdateWidget(copy)).unwrap();
    assert_eq!(e.preview().config().widgets[0], original);
    let mut sim = ControlSimulator::new(e.preview().clone());
    let start = sim.now_ms();
    let values = [2.5, 90., 95., 80., 30., 700., 13.8, 1.];
    sim.advance_to(start, Some(values));
    sim.advance_to(start + 1, Some(values));
    let snap = sim.snapshot();
    assert!(
        snap.warning_eligibility(RuleIndex::new(0).unwrap(), sim.now_ms())
            .visible()
    );
    assert!(
        !snap
            .warning_eligibility(RuleIndex::new(1).unwrap(), sim.now_ms())
            .visible()
    );
}

fn pair() -> Editor {
    let mut config = default_config();
    config.widgets.truncate(2);
    config.widgets[0].placement = Placement {
        column: 0,
        row: 0,
        span: Span::OneByOne,
    };
    config.widgets[1].placement = Placement {
        column: 1,
        row: 0,
        span: Span::OneByOne,
    };
    Editor::new(config).unwrap()
}

#[test]
fn nudge_respects_the_whole_widget_at_every_cell_and_size() {
    for span in [Span::OneByOne, Span::TwoByOne, Span::TwoByTwo] {
        for column in 0..=u8::MAX {
            for row in [0, 1, 2, u8::MAX] {
                let p = Placement { column, row, span };
                for (dir, dx, dy) in [
                    (Direction::Left, -1, 0),
                    (Direction::Right, 1, 0),
                    (Direction::Up, 0, -1),
                    (Direction::Down, 0, 1),
                ] {
                    let c = i16::from(column) + dx;
                    let r = i16::from(row) + dy;
                    let fits = p.rect().is_some()
                        && c >= 0
                        && r >= 0
                        && c + i16::from(span.columns()) <= 4
                        && r + i16::from(span.rows()) <= 2;
                    let next = dir.placement(p);
                    assert_eq!(next.is_some(), fits, "{p:?} {dir:?}");
                    if let Some(next) = next {
                        assert_eq!((i16::from(next.column), i16::from(next.row), next.span), (c, r, span));
                    }
                }
            }
        }
    }
}

#[test]
fn menu_can_pick_a_fully_covered_widget_then_repair_the_rejected_layout() {
    let mut editor = pair();
    let before = editor.export_binary().unwrap();
    assert!(matches!(
        editor.apply(Edit::ResizeWidget {
            id: 1,
            span: Span::TwoByTwo
        }),
        Err(EditorError::Validation(_))
    ));
    let mut menu = WidgetMenu::open(&mut editor, 1, Some(PanelPoint { x: 90.0, y: 40.0 })).unwrap();
    assert_eq!(menu.widgets().iter().map(|w| w.0).collect::<Vec<_>>(), [2, 1]);
    assert!(!menu.can_move(&editor, Direction::Left));
    menu.select(&mut editor, 2).unwrap();
    assert_eq!(menu.target(), 2);
    assert_eq!(editor.selection().widget, Some(2));
    assert_eq!(editor.export_binary(), Err(EditorError::InvalidDraft));
    assert!(
        validate_draft_drop(
            editor.editing_config(),
            1,
            PointerTarget::Widget(2),
            Placement {
                column: 2,
                row: 0,
                span: Span::OneByOne
            }
        )
        .is_ok()
    );
    menu.move_widget(&mut editor, Direction::Right).unwrap();
    assert!(!editor.has_invalid_draft());
    assert_eq!(editor.preview().config().widgets[0].placement.span, Span::TwoByTwo);
    assert_eq!(editor.preview().config().widgets[1].placement.column, 2);
    assert_eq!(menu.delete(&mut editor), Err(EditorError::StaleToken));
    editor.undo().unwrap();
    assert_eq!(editor.export_binary().unwrap(), before);
}

#[test]
fn menu_commands_reject_replaced_editors_old_revisions_and_unlisted_targets() {
    let mut editor = pair();
    let mut menu = WidgetMenu::open(&mut editor, 1, None).unwrap();
    let revision = editor.revision();
    assert_eq!(menu.select(&mut editor, 2), Err(EditorError::MissingWidget(2)));
    assert_eq!(editor.revision(), revision);
    assert_eq!(menu.move_widget(&mut editor, Direction::Left), Ok(revision));
    let mut replacement = pair();
    assert_eq!(menu.delete(&mut replacement), Err(EditorError::StaleToken));
    let token = editor.field_token();
    editor.set_raw(token, RawField::ViewName(1), "".into()).unwrap();
    assert!(editor.has_invalid_draft());
    assert!(!menu.is_current(&editor));
    assert_eq!(menu.delete(&mut editor), Err(EditorError::StaleToken));
    assert!(matches!(
        WidgetMenu::open(&mut editor, 1, None),
        Err(EditorError::InvalidDraft)
    ));
    editor.discard_raw().unwrap();
    assert_eq!(
        menu.move_widget(&mut editor, Direction::Right),
        Err(EditorError::StaleToken)
    );
}

#[test]
fn hit_list_is_half_open_view_scoped_and_ignores_nonfinite_points() {
    let mut editor = pair();
    for point in [
        None,
        Some(PanelPoint { x: 80.0, y: 25.0 }),
        Some(PanelPoint { x: f64::NAN, y: 30.0 }),
        Some(PanelPoint {
            x: f64::INFINITY,
            y: 30.0,
        }),
    ] {
        let menu = WidgetMenu::open(&mut editor, 1, point).unwrap();
        assert_eq!(menu.widgets().iter().map(|w| w.0).collect::<Vec<_>>(), [1]);
    }
    let menu = WidgetMenu::open(&mut editor, 1, Some(PanelPoint { x: 80.0, y: 26.0 })).unwrap();
    assert_eq!(menu.widgets().iter().map(|w| w.0).collect::<Vec<_>>(), [2, 1]);
    editor.apply(Edit::AddView { name: "Second".into() }).unwrap();
    let view = editor.selection().view;
    editor
        .apply(Edit::AddWidget {
            view,
            channel: Channel::Egt,
            placement: Placement {
                column: 0,
                row: 0,
                span: Span::OneByOne,
            },
        })
        .unwrap();
    let id = editor.selection().widget.unwrap();
    let menu = WidgetMenu::open(&mut editor, id, Some(PanelPoint { x: 20.0, y: 30.0 })).unwrap();
    assert_eq!(menu.widgets().iter().map(|w| w.0).collect::<Vec<_>>(), [id]);
}

#[test]
fn menu_delete_repairs_overlap_and_can_delete_rejected_new_widgets() {
    let mut editor = pair();
    let before = editor.export_binary().unwrap();
    assert!(
        editor
            .apply(Edit::AddWidget {
                view: 1,
                channel: Channel::Egt,
                placement: Placement {
                    column: 0,
                    row: 0,
                    span: Span::OneByOne
                }
            })
            .is_err()
    );
    let id = editor.selection().widget.unwrap();
    let menu = WidgetMenu::open(&mut editor, id, None).unwrap();
    menu.delete(&mut editor).unwrap();
    assert_eq!(editor.export_binary().unwrap(), before);
    assert!(editor.selection().widget.is_none());
    assert!(!menu.is_current(&editor));
}

#[test]
fn viewport_fit_handles_edges_oversized_menus_and_bad_measurements() {
    assert_eq!(
        menu_position((990.0, 790.0), (250.0, 300.0), (1000.0, 800.0)),
        Some((742.0, 492.0))
    );
    assert_eq!(
        menu_position((-20.0, -20.0), (250.0, 300.0), (1000.0, 800.0)),
        Some((8.0, 8.0))
    );
    assert_eq!(
        menu_position((10.0, 10.0), (250.0, 300.0), (100.0, 80.0)),
        Some((8.0, 8.0))
    );
    assert_eq!(menu_position((0.0, 0.0), (1.0, 1.0), (1.0, 1.0)), Some((0.0, 0.0)));
    for invalid in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert_eq!(menu_position((invalid, 0.0), (10.0, 10.0), (100.0, 100.0)), None);
        assert_eq!(menu_position((0.0, 0.0), (invalid, 10.0), (100.0, 100.0)), None);
        assert_eq!(menu_position((0.0, 0.0), (10.0, 10.0), (invalid, 100.0)), None);
    }
    assert_eq!(menu_position((0.0, 0.0), (0.0, 10.0), (100.0, 100.0)), None);
    assert_eq!(menu_position((0.0, 0.0), (10.0, 10.0), (-1.0, 100.0)), None);
}
