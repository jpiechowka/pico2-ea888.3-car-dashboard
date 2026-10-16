use dashboard_configurator::config::*;
use dashboard_configurator::editor::*;

#[test]
fn default_repack_preserves_order_settings_and_exact_history() {
    let mut editor = Editor::default();
    let original = editor
        .export_project(dashboard_config_tools::project::ExportMode::Complete)
        .unwrap();
    let before = editor.preview().config().clone();
    editor.apply(Edit::SetOrientation(Orientation::Portrait)).unwrap();
    let after = editor.preview().config();
    assert_eq!(after.orientation, Orientation::Portrait);
    for (old, new) in before.widgets.iter().zip(after.widgets.iter()) {
        let mut expected = old.clone();
        expected.placement.column = new.placement.column;
        expected.placement.row = new.placement.row;
        assert_eq!(&expected, new);
        assert!(new.placement.rect_in(Geometry::Portrait).is_some());
    }
    editor.undo().unwrap();
    assert_eq!(
        editor
            .export_project(dashboard_config_tools::project::ExportMode::Complete)
            .unwrap(),
        original
    );
    editor.redo().unwrap();
    assert_eq!(editor.preview().config().orientation, Orientation::Portrait);
}

#[test]
fn explicit_override_is_unchanged_and_flip_does_not_repack() {
    let mut editor = Editor::default();
    editor
        .apply(Edit::SetViewOrientation {
            id: 1,
            orientation: Some(Orientation::Landscape),
        })
        .unwrap();
    let before = editor.preview().config().widgets.clone();
    editor.apply(Edit::SetOrientation(Orientation::Portrait)).unwrap();
    assert_eq!(editor.preview().config().widgets, before);
    editor
        .apply(Edit::SetViewOrientation {
            id: 1,
            orientation: Some(Orientation::LandscapeInverted),
        })
        .unwrap();
    assert_eq!(editor.preview().config().widgets, before);
}

#[test]
fn portrait_hit_testing_uses_exact_integer_row_edges() {
    use dashboard_configurator::geometry::*;
    let rect = ContentRect {
        left: 0.,
        top: 0.,
        width: 240.,
        height: 320.,
    };
    for (y, row) in [
        (26., 0),
        (98.999, 0),
        (99., 1),
        (172.999, 1),
        (173., 2),
        (245.999, 2),
        (246., 3),
        (319.999, 3),
    ] {
        assert_eq!(
            cell_at_in(Geometry::Portrait, rect, 239., y),
            Ok(Cell { column: 1, row })
        );
    }
    assert_eq!(
        cell_at_in(Geometry::Portrait, rect, 0., 320.),
        Err(GeometryError::OutsideCanvas)
    );
}

#[test]
fn mixed_layout_repacking_is_complete_and_retains_overridden_views() {
    let mut raw = default_config();
    raw.widgets.clear();
    let placements = [
        Placement {
            column: 0,
            row: 0,
            span: Span::TwoByTwo,
        },
        Placement {
            column: 2,
            row: 0,
            span: Span::TwoByOne,
        },
        Placement {
            column: 2,
            row: 1,
            span: Span::OneByOne,
        },
        Placement {
            column: 3,
            row: 1,
            span: Span::OneByOne,
        },
    ];
    for (i, p) in placements.into_iter().enumerate() {
        raw.widgets
            .push(default_widget(Channel::Oil, i as u16 + 1, 1, p))
            .unwrap();
    }
    raw.views
        .push(View {
            id: 2,
            name: "Explicit".try_into().unwrap(),
            orientation: Some(Orientation::Landscape),
        })
        .unwrap();
    raw.widgets
        .push(default_widget(
            Channel::Boost,
            9,
            2,
            Placement {
                column: 3,
                row: 1,
                span: Span::OneByOne,
            },
        ))
        .unwrap();
    let mut e = Editor::new(raw).unwrap();
    let baseline = e.export_binary().unwrap();
    let pointer = e.begin_pointer(7, PointerTarget::Widget(1)).unwrap();
    let field = e.field_token();
    e.apply(Edit::SetOrientation(Orientation::Portrait)).unwrap();
    assert_eq!(e.active_pointer(), None);
    assert_eq!(
        e.commit_pointer(pointer, 7, placements[0]),
        Err(EditorError::StaleToken)
    );
    assert_eq!(
        e.apply_if_current(field, Edit::SetReducedMotion(true)),
        Err(EditorError::StaleToken)
    );
    assert_eq!(e.preview().config().widgets[4].placement.column, 3);
    let mask = e
        .preview()
        .config()
        .widgets
        .iter()
        .filter(|w| w.view == 1)
        .fold(0, |mask, w| mask | w.placement.occupied_in(Geometry::Portrait).unwrap());
    assert_eq!(mask, 255);
    e.undo().unwrap();
    assert_eq!(e.export_binary().unwrap(), baseline);
}

#[test]
fn every_admissible_span_multiset_fits_both_shapes() {
    for square in 0..=2 {
        for wide in 0..=4 {
            for single in 0..=8 {
                if square * 4 + wide * 2 + single > 8 {
                    continue;
                }
                for geometry in Geometry::ALL {
                    let mut before = default_config();
                    before.widgets.clear();
                    let mut next = before.clone();
                    next.orientation = if geometry == Geometry::Portrait {
                        Orientation::Portrait
                    } else {
                        Orientation::Landscape
                    };
                    before.orientation = if geometry == Geometry::Portrait {
                        Orientation::Landscape
                    } else {
                        Orientation::Portrait
                    };
                    for span in std::iter::repeat_n(Span::TwoByTwo, square)
                        .chain(std::iter::repeat_n(Span::TwoByOne, wide))
                        .chain(std::iter::repeat_n(Span::OneByOne, single))
                    {
                        let id = next.widgets.len() as u16 + 1;
                        next.widgets
                            .push(default_widget(
                                Channel::Oil,
                                id,
                                1,
                                Placement {
                                    column: 0,
                                    row: 0,
                                    span,
                                },
                            ))
                            .unwrap();
                    }
                    dashboard_configurator::orientation::repack_changed_views(&before, &mut next).unwrap();
                    assert!(validate(next).is_ok());
                }
            }
        }
    }
}

#[test]
fn published_pixels_and_rejected_render_keep_their_own_geometry() {
    use dashboard_configurator::preview::{Preview, PreviewOptions};
    let mut e = Editor::default();
    let mut p = Preview::new();
    p.render(e.preview(), 1, PreviewOptions::default()).unwrap();
    assert_eq!(p.geometry(), Geometry::Landscape);
    e.apply(Edit::SetOrientation(Orientation::Portrait)).unwrap();
    let old = p.rgba().to_vec();
    assert!(p.render(e.preview(), 255, PreviewOptions::default()).is_err());
    assert_eq!(p.geometry(), Geometry::Landscape);
    assert_eq!(p.rgba(), old);
    p.render(e.preview(), 1, PreviewOptions::default()).unwrap();
    assert_eq!(p.geometry(), Geometry::Portrait);
    assert_eq!(p.rgba().len(), 320 * 240 * 4);
}

#[test]
fn invalid_portrait_draft_projects_candidate_geometry() {
    use dashboard_configurator::diagnostic::Draft;
    use dashboard_configurator::preview::{Preview, PreviewOptions};
    let e = Editor::default();
    let mut p = Preview::new();
    let mut raw = e.preview().config().clone();
    raw.orientation = Orientation::Portrait;
    raw.widgets[0].placement.row = 3;
    let draft = Draft {
        config: raw,
        raw: vec![],
        error: EditorError::InvalidDraft,
    };
    p.render_diagnostic(e.preview(), &draft, 1, PreviewOptions::default())
        .unwrap();
    assert_eq!(p.geometry(), Geometry::Portrait);
    assert!(p.rgb565()[246 * 240 * 2..].iter().any(|&b| b != 0));
}

#[test]
fn pending_pixels_keep_shape_through_startup_transition() {
    use dashboard_config_tools::project::Project;
    use dashboard_configurator::preview::Preview;
    use dashboard_configurator::simulator::ControlSimulator;
    use dashboard_core::display::pipeline::{BufferId, FrameToken};
    let config = validate(default_config()).unwrap();
    let profile = startup::StartupProfile {
        status_ms: 100,
        duration_ms: 100,
        status_orientation: Some(Orientation::Portrait),
        ..Default::default()
    };
    let project = Project::new(&config, profile, None).unwrap();
    let mut sim = ControlSimulator::new(config);
    sim.restart_project(&project).unwrap();
    let mut preview = Preview::new();
    let token = FrameToken {
        buffer: BufferId::A,
        sequence: 1,
    };
    preview.render_simulation_submitted(&mut sim, token).unwrap();
    let pixels = preview.rgba().to_vec();
    assert_eq!(preview.geometry(), Geometry::Portrait);
    sim.advance_to(sim.now_ms() + 1000, None);
    assert_eq!(preview.pending_pixels(&sim, token), Some(pixels.as_slice()));
    assert_eq!(preview.geometry(), Geometry::Portrait);
    assert!(sim.complete_frame(token));
    assert!(preview.finish_pending(token));
    sim.advance_to(sim.now_ms() + 100, None);
    preview.render_simulation(&sim).unwrap();
    assert_eq!(preview.geometry(), Geometry::Landscape);
}

#[test]
fn debug_and_logs_inherit_default_while_dashboard_override_keeps_its_shape() {
    use dashboard_configurator::preview::Preview;
    use dashboard_configurator::simulator::ControlSimulator;
    let mut raw = default_config();
    raw.orientation = Orientation::Portrait;
    raw.views[0].orientation = Some(Orientation::Landscape);
    let mut sim = ControlSimulator::new(validate(raw).unwrap());
    let mut p = Preview::new();
    p.render_simulation(&sim).unwrap();
    assert_eq!(p.geometry(), Geometry::Landscape);
    sim.tap(ButtonId::Y);
    assert_eq!(sim.session().page(), ControlPage::Debug);
    p.render_simulation(&sim).unwrap();
    assert_eq!(p.geometry(), Geometry::Portrait);
    sim.tap(ButtonId::Y);
    assert_eq!(sim.session().page(), ControlPage::Logs);
    p.render_simulation(&sim).unwrap();
    assert_eq!(p.geometry(), Geometry::Portrait);
    sim.tap(ButtonId::Y);
    p.render_simulation(&sim).unwrap();
    assert_eq!(p.geometry(), Geometry::Landscape);
}

#[test]
fn portrait_drag_anchor_survives_css_resize_and_uses_last_row() {
    use dashboard_configurator::geometry::*;
    use dashboard_configurator::widget_actions::Direction;
    let g = Geometry::Portrait;
    let rect = ContentRect {
        left: 17.,
        top: 35.,
        width: 240.,
        height: 320.,
    };
    let placement = Placement {
        column: 1,
        row: 3,
        span: Span::OneByOne,
    };
    let offset = grab_offset_in(g, rect, 17. + 150., 35. + 319., placement).unwrap();
    let scaled = ContentRect {
        width: 480.,
        height: 640.,
        ..rect
    };
    assert_eq!(
        drop_placement_in(g, scaled, 17. + 300., 35. + 638., placement.span, offset),
        Ok(placement)
    );
    assert_eq!(Direction::Down.placement_in(g, placement), None);
    assert_eq!(Direction::Right.placement_in(g, placement), None);
    assert_eq!(Direction::Up.placement_in(g, placement).unwrap().row, 2);
    assert_eq!(Direction::Left.placement_in(g, placement).unwrap().column, 0);
}

#[test]
fn ambiguous_repack_inputs_are_rejected_without_mutation() {
    let before = default_config();
    let mut next = before.clone();
    next.orientation = Orientation::Portrait;
    next.widgets[1].id = next.widgets[0].id;
    let retained = next.clone();
    assert_eq!(
        dashboard_configurator::orientation::repack_changed_views(&before, &mut next),
        Err(EditorError::AmbiguousTarget)
    );
    assert_eq!(next, retained);
}
