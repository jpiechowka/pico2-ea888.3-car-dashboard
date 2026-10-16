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
