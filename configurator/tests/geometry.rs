use dashboard_configurator::config::*;
use dashboard_configurator::editor::PointerTarget;
use dashboard_configurator::geometry::*;

const RECT: ContentRect = ContentRect {
    left: 37.5,
    top: 91.25,
    width: 640.0,
    height: 480.0,
};

fn place(
    column: u8,
    row: u8,
    span: Span,
) -> Placement {
    Placement { column, row, span }
}

fn client(
    rect: ContentRect,
    x: f64,
    y: f64,
) -> (f64, f64) {
    (rect.left + x / 320.0 * rect.width, rect.top + y / 240.0 * rect.height)
}

#[test]
fn content_rect_scales_fractional_and_nonuniform_canvas_coordinates() {
    for rect in [
        RECT,
        ContentRect {
            left: -40.25,
            top: 9.5,
            width: 333.25,
            height: 197.5,
        },
    ] {
        for (x, y) in [(0.0, 0.0), (160.0, 120.0), (319.0, 239.0)] {
            let (cx, cy) = client(rect, x, y);
            let point = panel_point(rect, cx, cy).unwrap();
            assert!((point.x - x).abs() < 1e-10);
            assert!((point.y - y).abs() < 1e-10);
        }
    }
}

#[test]
fn every_cell_has_exact_half_open_boundaries_below_the_header() {
    for row in 0..2 {
        for column in 0..4 {
            for (x, y) in [
                (column as f64 * 80.0, 26.0 + row as f64 * 107.0),
                (column as f64 * 80.0 + 79.999, 26.0 + row as f64 * 107.0 + 106.999),
            ] {
                let (cx, cy) = client(RECT, x, y);
                assert_eq!(cell_at(RECT, cx, cy), Ok(Cell { column, row }));
            }
        }
    }
    let (cx, cy) = client(RECT, 80.0, 25.999);
    assert_eq!(cell_at(RECT, cx, cy), Err(GeometryError::Header));
    assert!(
        panel_point(RECT, cx, cy).is_ok(),
        "the header is still part of the canvas"
    );
    for (x, y) in [(-0.01, 26.0), (320.0, 26.0), (0.0, -0.01), (0.0, 240.0)] {
        let (cx, cy) = client(RECT, x, y);
        assert_eq!(panel_point(RECT, cx, cy), Err(GeometryError::OutsideCanvas));
    }
}

#[test]
fn nonfinite_and_degenerate_rectangles_and_pointers_are_rejected() {
    for rect in [
        ContentRect { width: 0.0, ..RECT },
        ContentRect { height: -1.0, ..RECT },
        ContentRect {
            width: f64::NAN,
            ..RECT
        },
        ContentRect {
            top: f64::INFINITY,
            ..RECT
        },
        ContentRect {
            left: f64::MAX,
            width: f64::MAX,
            ..RECT
        },
        ContentRect {
            left: f64::MAX,
            width: 1.0,
            ..RECT
        },
    ] {
        assert_eq!(panel_point(rect, 50.0, 150.0), Err(GeometryError::InvalidRect));
    }
    for (x, y) in [(f64::NAN, 200.0), (200.0, f64::INFINITY), (f64::NEG_INFINITY, 0.0)] {
        assert_eq!(panel_point(RECT, x, y), Err(GeometryError::NonFinitePointer));
    }
}

#[test]
fn dragging_preserves_grab_offset_across_canvas_resize_and_snaps_nearest_origin() {
    let start = place(1, 0, Span::TwoByOne);
    let (x, y) = client(RECT, 213.0, 98.0);
    let offset = grab_offset(RECT, x, y, start).unwrap();
    assert_eq!(drop_placement(RECT, x, y, start.span, offset), Ok(start));
    let small = ContentRect {
        left: 8.0,
        top: 13.0,
        width: 160.0,
        height: 120.0,
    };
    let (x, y) = client(small, 293.0, 205.0);
    assert_eq!(
        drop_placement(small, x, y, start.span, offset),
        Ok(place(2, 1, start.span))
    );
    let (x, y) = client(RECT, 245.0, 98.0);
    assert_eq!(
        drop_placement(RECT, x, y, start.span, offset),
        Ok(start),
        "32 panel pixels do not cross the 40px snap threshold"
    );
    let (x, y) = client(RECT, 254.0, 98.0);
    assert_eq!(
        drop_placement(RECT, x, y, start.span, offset),
        Ok(place(2, 0, start.span))
    );
}

#[test]
fn grab_must_be_within_the_original_widget_and_offsets_must_fit_the_drop_span() {
    let placement = place(1, 0, Span::TwoByTwo);
    for (x, y) in [(79.0, 30.0), (240.0, 30.0), (80.0, 25.0)] {
        let (cx, cy) = client(RECT, x, y);
        assert_eq!(grab_offset(RECT, cx, cy, placement), Err(GeometryError::OutsideWidget));
    }
    let (x, y) = client(RECT, 220.0, 200.0);
    let offset = grab_offset(RECT, x, y, placement).unwrap();
    assert_eq!(
        drop_placement(RECT, x, y, Span::OneByOne, offset),
        Err(GeometryError::InvalidOffset)
    );
}

#[test]
fn every_span_requires_its_full_snapped_area_inside_the_grid() {
    for span in [Span::OneByOne, Span::TwoByOne, Span::TwoByTwo] {
        for row in 0..2 {
            for column in 0..4 {
                let (x, y) = client(RECT, column as f64 * 80.0, 26.0 + row as f64 * 107.0);
                let result = drop_placement(RECT, x, y, span, GrabOffset::TOP_LEFT);
                if column + span.columns() <= 4 && row + span.rows() <= 2 {
                    assert_eq!(result, Ok(place(column, row, span)));
                } else {
                    assert_eq!(result, Err(GeometryError::OutsideGrid));
                }
            }
        }
        let (x, y) = client(RECT, 40.0 * span.columns() as f64, 26.0 + 53.5 * span.rows() as f64);
        assert_eq!(
            drop_placement(RECT, x, y, span, GrabOffset::center(span)),
            Ok(place(0, 0, span))
        );
    }
    let (x, y) = client(RECT, 10.0, 20.0);
    assert_eq!(
        drop_placement(RECT, x, y, Span::OneByOne, GrabOffset::TOP_LEFT),
        Err(GeometryError::Header)
    );
    let (x, y) = client(RECT, 5.0, 50.0);
    assert_eq!(
        drop_placement(RECT, x, y, Span::OneByOne, GrabOffset::center(Span::OneByOne)),
        Err(GeometryError::OutsideGrid)
    );
}

#[test]
fn shared_validation_allows_self_occupancy_and_rejects_collisions_without_mutation() {
    let config = validate(default_config()).unwrap();
    let before = config.clone();
    let first = &config.config().widgets[0];
    assert_eq!(
        validate_drop(&config, 1, PointerTarget::Widget(first.id), first.placement),
        Ok(())
    );
    let occupied = config.config().widgets[1].placement;
    assert!(matches!(
        validate_drop(&config, 1, PointerTarget::Widget(first.id), occupied),
        Err(GeometryError::Validation(ValidationError {
            code: ErrorCode::Overlap,
            ..
        }))
    ));
    assert!(matches!(
        validate_drop(&config, 1, PointerTarget::Library(Channel::Oil), occupied),
        Err(GeometryError::Validation(ValidationError {
            code: ErrorCode::Overlap,
            ..
        }))
    ));
    assert_eq!(config, before);
}

#[test]
fn drop_validation_obeys_references_labels_capacity_and_sparse_widget_ids() {
    let mut config = default_config();
    config.widgets.clear();
    let mut widget = default_widget(Channel::Oil, 64, 1, place(0, 0, Span::TwoByOne));
    widget.label = "WIDE OIL NAME".try_into().unwrap();
    config.widgets.push(widget).unwrap();
    let config = validate(config).unwrap();
    assert!(matches!(
        validate_drop(&config, 1, PointerTarget::Widget(64), place(0, 0, Span::OneByOne)),
        Err(GeometryError::Validation(ValidationError {
            field: Field::Label,
            ..
        }))
    ));
    assert_eq!(
        validate_drop(
            &config,
            1,
            PointerTarget::Library(Channel::Oil),
            place(2, 1, Span::TwoByOne)
        ),
        Ok(())
    );
    assert_eq!(
        validate_drop(
            &config,
            200,
            PointerTarget::Library(Channel::Oil),
            place(0, 0, Span::OneByOne)
        ),
        Err(GeometryError::MissingView)
    );
    assert_eq!(
        validate_drop(&config, 1, PointerTarget::Widget(404), place(0, 0, Span::OneByOne)),
        Err(GeometryError::MissingWidget)
    );
    let mut full = default_config();
    for view in 2..=8 {
        full.views
            .push(View {
                orientation: None,
                id: view,
                name: format!("View {view}").as_str().try_into().unwrap(),
            })
            .unwrap();
        for slot in 0..8 {
            full.widgets
                .push(default_widget(
                    Channel::Oil,
                    (u16::from(view) - 1) * 8 + u16::from(slot) + 1,
                    view,
                    place(slot % 4, slot / 4, Span::OneByOne),
                ))
                .unwrap();
        }
    }
    let full = validate(full).unwrap();
    assert_eq!(
        validate_drop(
            &full,
            1,
            PointerTarget::Library(Channel::Oil),
            place(0, 0, Span::OneByOne)
        ),
        Err(GeometryError::Capacity)
    );
}

#[test]
fn stationary_and_jittered_release_after_canvas_reflow_only_keeps_selection() {
    use dashboard_configurator::editor::{Editor, PointerTarget, Selection};
    for span in [Span::OneByOne, Span::TwoByOne, Span::TwoByTwo] {
        let mut config = default_config();
        config.widgets.clear();
        config
            .widgets
            .push(default_widget(Channel::Oil, 1, 1, place(0, 0, span)))
            .unwrap();
        let mut editor = Editor::new(config).unwrap();
        let accepted = editor.preview().clone();
        let original = place(0, 0, span);
        let (x, y) = client(RECT, 20.0, 46.0);
        let offset = grab_offset(RECT, x, y, original).unwrap();
        let shifted = ContentRect {
            top: RECT.top - 150.0,
            ..RECT
        };
        // The old unconditional release mapping would move even this still pointer.
        assert_ne!(drop_placement(shifted, x, y, span, offset), Ok(original));
        for jitter in [0.0, 1.0, DRAG_THRESHOLD_CSS_PX - 0.01] {
            let history_before = editor.history_len();
            let token = editor.begin_pointer(7, PointerTarget::Widget(1)).unwrap();
            assert!(matches!(
                editor.begin_save(),
                Err(dashboard_configurator::editor::EditorError::PointerActive)
            ));
            let selected_revision = editor.revision();
            let selected_history = editor.history_len();
            assert_eq!(selected_history, history_before);
            let mut gesture = DragGesture::new(x, y).unwrap();
            assert_eq!(
                gesture
                    .placement(shifted, x + jitter, y, token.target(), span, offset)
                    .unwrap(),
                None
            );
            assert!(!gesture.is_dragging());
            editor.cancel_pointer(token).unwrap();
            assert_eq!(
                editor.selection(),
                Selection {
                    view: 1,
                    widget: Some(1)
                }
            );
            assert_eq!(editor.revision(), selected_revision);
            assert_eq!(editor.history_len(), selected_history);
            assert_eq!(editor.preview(), &accepted);
            assert!(!editor.has_invalid_draft());
            let request = editor.begin_save().unwrap();
            let restored = Editor::from_saved_state(request.json().as_bytes()).unwrap();
            assert_eq!(restored.selection(), editor.selection());
            assert_eq!(restored.preview(), &accepted);
            assert_eq!(editor.history_len(), selected_history);
        }
    }
}

#[test]
fn real_drag_uses_original_anchor_and_library_uses_pointed_cell() {
    use dashboard_configurator::editor::Editor;
    for span in [Span::OneByOne, Span::TwoByOne, Span::TwoByTwo] {
        let mut config = default_config();
        config.widgets.clear();
        config
            .widgets
            .push(default_widget(Channel::Oil, 1, 1, place(0, 0, span)))
            .unwrap();
        let mut editor = Editor::new(config).unwrap();
        let (x, y) = client(RECT, 20.0, 46.0);
        let offset = grab_offset(RECT, x, y, place(0, 0, span)).unwrap();
        let resized = ContentRect {
            width: 320.0,
            height: 240.0,
            ..RECT
        };
        let (mx, my) = client(resized, 100.0, 46.0);
        let token = editor.begin_pointer(7, PointerTarget::Widget(1)).unwrap();
        let mut gesture = DragGesture::new(x, y).unwrap();
        let position = gesture
            .placement(resized, mx, my, token.target(), span, offset)
            .unwrap()
            .unwrap();
        assert_eq!(position, place(1, 0, span));
        editor.commit_pointer(token, 7, position).unwrap();
        assert_eq!(editor.preview().config().widgets[0].placement, position);
    }
    let (x, y) = client(RECT, 0.0, 26.0);
    let mut library = DragGesture::new(0.0, 0.0).unwrap();
    assert_eq!(
        library
            .placement(
                RECT,
                x,
                y,
                PointerTarget::Library(Channel::Oil),
                Span::OneByOne,
                GrabOffset::center(Span::OneByOne)
            )
            .unwrap(),
        Some(place(0, 0, Span::OneByOne))
    );
}

#[test]
fn drag_threshold_is_css_distance_latches_and_rejects_nonfinite_samples() {
    for (dx, dy, active) in [
        (3.99, 0.0, false),
        (4.0, 0.0, true),
        (0.0, -4.0, true),
        (2.9, 2.9, true),
    ] {
        let mut gesture = DragGesture::new(100.0, 200.0).unwrap();
        gesture
            .placement(
                RECT,
                100.0 + dx,
                200.0 + dy,
                PointerTarget::Library(Channel::Oil),
                Span::OneByOne,
                GrabOffset::TOP_LEFT,
            )
            .unwrap();
        assert_eq!(gesture.is_dragging(), active);
        if active {
            gesture
                .placement(
                    RECT,
                    100.0,
                    200.0,
                    PointerTarget::Library(Channel::Oil),
                    Span::OneByOne,
                    GrabOffset::TOP_LEFT,
                )
                .unwrap();
            assert!(gesture.is_dragging());
        }
    }
    assert_eq!(DragGesture::new(f64::NAN, 0.0), Err(GeometryError::NonFinitePointer));
    let mut gesture = DragGesture::new(100.0, 200.0).unwrap();
    for (x, y) in [(f64::NAN, 200.0), (100.0, f64::INFINITY)] {
        assert_eq!(
            gesture.placement(
                RECT,
                x,
                y,
                PointerTarget::Library(Channel::Oil),
                Span::OneByOne,
                GrabOffset::TOP_LEFT
            ),
            Err(GeometryError::NonFinitePointer)
        );
        assert!(!gesture.is_dragging());
    }
}

#[test]
fn moved_gesture_does_not_bypass_wrong_pointer_cancel_or_stale_token() {
    use dashboard_configurator::editor::{Edit, Editor, EditorError};
    let mut editor = Editor::new(default_config()).unwrap();
    let token = editor.begin_pointer(7, PointerTarget::Widget(1)).unwrap();
    let mut gesture = DragGesture::new(0.0, 0.0).unwrap();
    let (x, y) = client(RECT, 40.0, 80.0);
    let position = gesture
        .placement(
            RECT,
            x,
            y,
            token.target(),
            Span::OneByOne,
            GrabOffset::center(Span::OneByOne),
        )
        .unwrap()
        .unwrap();
    let selected = editor.revision();
    assert_eq!(editor.commit_pointer(token, 8, position), Err(EditorError::StaleToken));
    assert_eq!(editor.active_pointer(), Some(token));
    editor.cancel_pointer(token).unwrap();
    assert_eq!(editor.commit_pointer(token, 7, position), Err(EditorError::StaleToken));
    assert_eq!(editor.revision(), selected);
    let token = editor.begin_pointer(7, PointerTarget::Widget(1)).unwrap();
    editor.apply(Edit::SetReducedMotion(true)).unwrap();
    let revision = editor.revision();
    assert_eq!(editor.commit_pointer(token, 7, position), Err(EditorError::StaleToken));
    assert_eq!(editor.revision(), revision);
}
