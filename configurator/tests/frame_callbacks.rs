use dashboard_configurator::config::*;
use dashboard_configurator::editor::Editor;
use dashboard_configurator::frame_callbacks::FrameCallbacks;
use dashboard_configurator::preview::Preview;
use dashboard_configurator::simulator::ControlSimulator;
use dashboard_core::input_model::EncoderBatch;

#[test]
fn final_held_release_retires_pending_inspection_draw_and_allows_new_pixels() {
    let mut editor = Editor::default();
    let mut sim = ControlSimulator::new(editor.preview().clone());
    let handle = editor.open_json().unwrap();
    let mut raw = editor.preview().config().clone();
    raw.views[0].name = "CANDIDATE".try_into().unwrap();
    let parse = editor
        .set_json_text(handle, serde_json::to_string(&raw).unwrap())
        .unwrap();
    editor.finish_json_parse(parse).unwrap();
    let proof = editor.json_draft().unwrap().preview_proof().unwrap();
    sim.inspect_candidate(proof.clone());
    sim.tap(ButtonId::Y);
    let accepted = (sim.source_key(), sim.snapshot(), sim.now_ms(), sim.session().page());
    let mut preview = Preview::new();
    let before = preview.render_candidate_simulation(&sim, &proof).unwrap().to_vec();
    let mut callbacks = FrameCallbacks::new();
    let old_callback = callbacks
        .gate
        .capture_for_editor(sim.source_key(), editor.accepted_config_key())
        .unwrap();

    // Adapter ordering: continuously held Debug A, then last release + debounce tick.
    let mut held = [false; 4];
    held[ButtonId::A.index()] = true;
    let start = sim.display_now_ms();
    sim.physical_at(held, EncoderBatch::default(), start + 1);
    sim.physical_at(held, EncoderBatch::default(), start + 51);
    sim.physical_at(held, EncoderBatch::default(), start + 1551);
    sim.physical_at([false; 4], EncoderBatch::default(), start + 1552);
    sim.physical_at([false; 4], EncoderBatch::default(), start + 1602);
    let old_draw = callbacks.queue_appearance().unwrap();
    callbacks.set_appearance_handle(old_draw, 11).unwrap();
    callbacks.completion = Some(12);
    let mut cancelled = Vec::new();
    callbacks.retire(|handle| cancelled.push(handle)); // final release interval cleanup
    assert!(
        !callbacks
            .gate
            .is_current_for_editor(old_callback, sim.source_key(), editor.accepted_config_key())
    );

    sim.rotate(-1); // next candidate update must schedule and actually repaint
    let new_draw = callbacks
        .queue_appearance()
        .expect("interval retirement must free the inspection draw slot");
    callbacks.set_appearance_handle(new_draw, 13).unwrap();
    assert!(
        !callbacks.finish_appearance(old_draw),
        "late retired RAF cannot clear a newer draw"
    );
    assert!(callbacks.queue_appearance().is_none(), "new draw still owns the slot");
    assert!(callbacks.finish_appearance(new_draw));
    let after = preview.render_candidate_simulation(&sim, &proof).unwrap();
    assert_ne!(
        after, before,
        "the new admitted callback paints candidate brightness pixels"
    );
    assert_eq!(cancelled, [12, 11]);
    assert_eq!(
        (sim.source_key(), sim.snapshot(), sim.now_ms(), sim.session().page()),
        accepted
    );
    assert_eq!(sim.completed_frame(), None);
}

#[test]
fn late_finished_draw_and_scheduler_failure_cannot_clear_a_new_request() {
    let mut callbacks = FrameCallbacks::new();
    let first = callbacks.queue_appearance().unwrap();
    assert!(callbacks.finish_appearance(first)); // scheduling failed
    let second = callbacks.queue_appearance().unwrap();
    callbacks.set_appearance_handle(second, 22).unwrap();
    assert!(!callbacks.finish_appearance(first));
    assert_eq!(callbacks.set_appearance_handle(first, 21), Err(21));
    assert!(callbacks.queue_appearance().is_none());
    let mut cancelled = Vec::new();
    callbacks.retire(|h| cancelled.push(h));
    assert_eq!(cancelled, [22]);
    assert!(!callbacks.finish_appearance(second));
    assert!(callbacks.queue_appearance().is_some());
    callbacks.retire(|h| cancelled.push(h)); // slot queued before a handle is installed
    assert_eq!(cancelled, [22]);
    assert!(callbacks.queue_appearance().is_some());
}
