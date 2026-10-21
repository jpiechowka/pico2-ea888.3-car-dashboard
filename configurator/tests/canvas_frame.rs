//! Pixel-publication authority across scene, draft, geometry and editor identity changes.
use dashboard_configurator::canvas_frame::{CanvasFrame, PreviewMode, PublishedCanvas};
use dashboard_configurator::config::*;
use dashboard_configurator::editor::*;

/// Returns `e`'s selected view identity in `mode`; panics if the fixture loses its view.
fn frame(
    e: &Editor,
    mode: PreviewMode,
) -> CanvasFrame {
    CanvasFrame::capture(
        e,
        mode,
        e.editing_config()
            .view_orientation(e.selection().view)
            .unwrap()
            .geometry(),
    )
}
/// Return a valid two-view editor; panic on fixture capacity or validation drift.
fn editor() -> Editor {
    let mut c = default_config();
    c.views
        .push(View {
            id: 2,
            name: "Second".try_into().unwrap(),
            orientation: None,
        })
        .unwrap();
    Editor::new(c).unwrap()
}

#[test]
/// Assert switching lanes grants no pointer authority until appearance pixels succeed.
fn current_mode_cannot_admit_retained_simulation_or_startup_pixels() {
    for mode in [PreviewMode::Simulation, PreviewMode::Startup] {
        let e = editor();
        let mut canvas = PublishedCanvas::default();
        assert!(!canvas.permits(frame(&e, PreviewMode::Appearance)));
        canvas.publish(frame(&e, mode), Ok::<(), ()>(())).unwrap();
        assert!(!canvas.permits(frame(&e, PreviewMode::Appearance)));
        assert!(
            canvas
                .publish(frame(&e, PreviewMode::Appearance), Err("canvas failure"))
                .is_err()
        );
        assert!(!canvas.permits(frame(&e, PreviewMode::Appearance)));
        canvas
            .publish(frame(&e, PreviewMode::Appearance), Ok::<(), ()>(()))
            .unwrap();
        assert!(canvas.permits(frame(&e, PreviewMode::Appearance)));
    }
}

#[test]
/// Assert view/revision changes and failed writes revoke old pixel authority.
fn same_shape_view_and_revision_changes_wait_for_successful_publication() {
    let mut e = editor();
    let mut canvas = PublishedCanvas::default();
    canvas
        .publish(frame(&e, PreviewMode::Appearance), Ok::<(), ()>(()))
        .unwrap();
    e.select(Selection { view: 2, widget: None }).unwrap();
    let requested = frame(&e, PreviewMode::Appearance);
    assert!(!canvas.permits(requested));
    assert!(canvas.publish(requested, Err("putImageData")).is_err());
    assert!(!canvas.permits(requested));
    canvas.publish(requested, Ok::<(), ()>(())).unwrap();
    assert!(canvas.permits(requested));
    e.apply(Edit::SetReducedMotion(true)).unwrap();
    assert!(!canvas.permits(frame(&e, PreviewMode::Appearance)));
    canvas
        .publish(frame(&e, PreviewMode::Appearance), Ok::<(), ()>(()))
        .unwrap();
    canvas.fail();
    assert!(!canvas.permits(frame(&e, PreviewMode::Appearance)));
}

#[test]
/// Assert pending JSON remains read-only even after rendering, and rejected edits retire hits.
fn pending_json_and_rejected_draft_retire_the_previous_hit_layer() {
    let mut e = editor();
    let mut canvas = PublishedCanvas::default();
    let before = frame(&e, PreviewMode::Appearance);
    canvas.publish(before, Ok::<(), ()>(())).unwrap();
    let handle = e.open_json().unwrap();
    let text = e.json_draft().unwrap().text().to_owned();
    let parse = e.set_json_text(handle, text).unwrap();
    assert!(!canvas.permits(frame(&e, PreviewMode::Appearance)));
    e.finish_json_parse(parse).unwrap();
    canvas
        .publish(frame(&e, PreviewMode::Appearance), Ok::<(), ()>(()))
        .unwrap();
    assert!(
        !canvas.permits(frame(&e, PreviewMode::Appearance)),
        "even published pending JSON is read-only"
    );
    e.discard_json().unwrap();
    assert!(!canvas.permits(frame(&e, PreviewMode::Appearance)));
    canvas
        .publish(frame(&e, PreviewMode::Appearance), Ok::<(), ()>(()))
        .unwrap();
    let mut widget = e.preview().config().widgets[0].clone();
    widget.gauge.min = widget.gauge.max;
    assert!(e.apply(Edit::UpdateWidget(widget)).is_err());
    assert!(!canvas.permits(frame(&e, PreviewMode::Appearance)));
}

#[test]
/// Assert only selection within unchanged rendered content can advance a canvas stamp.
fn selecting_a_widget_preserves_drag_but_cannot_rebind_changed_view_or_config() {
    let mut e = editor();
    let mut canvas = PublishedCanvas::default();
    let before = frame(&e, PreviewMode::Appearance);
    canvas.publish(before, Ok::<(), ()>(())).unwrap();
    let token = e.begin_pointer(1, PointerTarget::Widget(2)).unwrap();
    let selected = frame(&e, PreviewMode::Appearance);
    canvas.selected(before, selected);
    assert!(canvas.permits(selected));
    e.cancel_pointer(token).unwrap();
    e.select(Selection { view: 2, widget: None }).unwrap();
    canvas.selected(selected, frame(&e, PreviewMode::Appearance));
    assert!(!canvas.permits(frame(&e, PreviewMode::Appearance)));
    let before = frame(&e, PreviewMode::Appearance);
    canvas.publish(before, Ok::<(), ()>(())).unwrap();
    e.apply(Edit::SetReducedMotion(true)).unwrap();
    canvas.selected(before, frame(&e, PreviewMode::Appearance));
    assert!(!canvas.permits(frame(&e, PreviewMode::Appearance)));
}

#[test]
/// Assert resize failure blocks retained pixels and replacement editors cannot inherit authority.
fn failed_resize_and_replaced_editor_cannot_reuse_old_pixel_authority() {
    let mut e = editor();
    let mut canvas = PublishedCanvas::default();
    let landscape = frame(&e, PreviewMode::Appearance);
    canvas.publish(landscape, Ok::<(), ()>(())).unwrap();
    e.apply(Edit::SetOrientation(Orientation::Portrait)).unwrap();
    let portrait = frame(&e, PreviewMode::Appearance);
    assert!(!canvas.permits(portrait));
    assert!(canvas.publish(portrait, Err("resize restoration")).is_err());
    assert!(!canvas.permits(portrait));
    assert!(
        !canvas.permits(landscape),
        "failure remains closed even if old pixels were restored"
    );
    canvas.publish(portrait, Ok::<(), ()>(())).unwrap();
    assert!(canvas.permits(portrait));
    let replacement = Editor::new(e.preview().config().clone()).unwrap();
    assert!(!canvas.permits(frame(&replacement, PreviewMode::Appearance)));
}
