//! Independent shared-render composition checks for accepted and inspection projections.

use dashboard_configurator::config::*;
use dashboard_configurator::editor::Editor;
use dashboard_configurator::preview::Preview;
use dashboard_configurator::simulator::ControlSimulator;
use dashboard_core::backlight::Brightness;
use dashboard_core::colors::BLACK;
use dashboard_core::dashboard::Overlay;
use dashboard_core::debug_view::{DebugData, DebugPage};
use dashboard_core::display::pipeline::{BufferId, FrameToken};
use dashboard_core::display::{FrameBuffer, St7789Renderer};
use embedded_graphics::prelude::DrawTarget;

/// Returns independently composed Debug framebuffer for `sim`, using display inspection
/// session when `inspection` and candidate config when `candidate`. Candidate requires
/// a live inspection proof; absent proof or impossible drawing failure panics.
fn debug_pixels(
    sim: &ControlSimulator,
    inspection: bool,
    candidate: bool,
) -> Box<FrameBuffer> {
    let session = if inspection {
        sim.display_session()
    } else {
        sim.session()
    };
    let config = if candidate {
        sim.inspection_candidate().unwrap().config()
    } else {
        sim.config()
    };
    let data = DebugData {
        source_status: Some("SYNTHETIC DEMO"),
        config_fault: Some(inspection && !candidate),
        brightness: Some(session.backlight().brightness()),
        settings_status: Some(if candidate {
            "UNAPPLIED JSON"
        } else if inspection {
            "INSPECTION ONLY"
        } else {
            "ACCEPTED CONFIG"
        }),
        ..Default::default()
    };
    let mut pixels = Box::new(FrameBuffer::new());
    let mut display = St7789Renderer::new(&mut pixels);
    DebugPage::new(&data, &config.config().controls).draw(&mut display);
    if let Some(overlay) = Overlay::configured_for_page(ControlPage::Debug, session.overlay(config), None) {
        overlay.draw(&mut display);
    }
    if session.backlight().compare() == 0 {
        display.clear(BLACK).unwrap();
    }
    pixels
}

#[test]
/// Debug output matches independent shared rendering through brightness feedback, expiry, off and restore.
fn accepted_debug_composes_rotation_toggle_and_expired_feedback() {
    let mut sim = ControlSimulator::new(validate(default_config()).unwrap());
    sim.tap(ButtonId::Y);
    assert_eq!(sim.session().page(), ControlPage::Debug);
    let mut preview = Preview::new();
    for step in 0..5 {
        match step {
            0 => sim.rotate(-1),
            1 | 3 => sim.advance(2100),
            2 | 4 => sim.click(),
            _ => unreachable!(),
        }
        if matches!(step, 0 | 2 | 4) {
            assert!(matches!(
                sim.session().overlay(sim.config()),
                Some(Overlay::Brightness(Brightness::Percent(_)))
            ));
        } else {
            assert_eq!(sim.session().overlay(sim.config()), None);
        }
        preview.render_simulation(&sim).unwrap();
        assert!(
            preview.rgb565() == &debug_pixels(&sim, false, false)[..],
            "Debug composition step {step}"
        );
        if step == 3 {
            assert_eq!(sim.session().backlight().compare(), 0);
            assert!(preview.rgb565().iter().all(|byte| *byte == 0));
        }
    }
}

#[test]
/// Candidate/invalid Debug controls alter only inspection pixels, preserving accepted pending bytes,
/// clock/source/completion/LED metadata and denying physical-frame acknowledgment.
fn inspection_debug_overlay_preserves_accepted_pending_bytes_and_metadata() {
    let mut editor = Editor::default();
    let mut sim = ControlSimulator::new(editor.preview().clone());
    sim.require_frame_completion();
    sim.tap(ButtonId::Y);
    sim.rotate(-1);
    let token = FrameToken {
        buffer: BufferId::A,
        sequence: 71,
    };
    let mut timing = Preview::new();
    let pending = timing.render_simulation_submitted(&mut sim, token).unwrap().to_vec();
    assert!(timing.rgb565() == &debug_pixels(&sim, false, false)[..]);
    let accepted = (
        sim.source_key(),
        sim.snapshot(),
        sim.now_ms(),
        sim.completed_frame(),
        sim.completed_warning(),
        sim.led_duty(),
        sim.session().popup(),
        sim.session().backlight().compare(),
    );
    let handle = editor.open_json().unwrap();
    let mut raw = editor.preview().config().clone();
    raw.views[0].name = "CANDIDATE".try_into().unwrap();
    let parse = editor
        .set_json_text(handle, serde_json::to_string(&raw).unwrap())
        .unwrap();
    editor.finish_json_parse(parse).unwrap();
    let proof = editor.json_draft().unwrap().preview_proof().unwrap();
    let mut scratch = Preview::new();
    for candidate in [true, false] {
        if candidate {
            sim.inspect_candidate(proof.clone());
        } else {
            sim.inspect_invalid(raw.controls);
        }
        sim.tap(ButtonId::Y);
        assert_eq!(sim.display_session().page(), ControlPage::Debug);
        for step in 0..4 {
            match step {
                0 => sim.rotate(-1),
                1 | 3 => sim.advance(2100),
                2 => sim.click(),
                _ => unreachable!(),
            }
            if candidate {
                scratch.render_candidate_simulation(&sim, &proof).unwrap();
            } else {
                scratch.render_inspection(&sim, None).unwrap();
            }
            assert!(
                scratch.rgb565() == &debug_pixels(&sim, true, candidate)[..],
                "inspection {candidate}, step {step}"
            );
            if step == 3 {
                assert_eq!(sim.display_session().backlight().compare(), 0);
                assert!(scratch.rgb565().iter().all(|byte| *byte == 0));
            }
            assert_eq!(timing.pending_pixels(&sim, token), Some(pending.as_slice()));
            assert_eq!(
                (
                    sim.source_key(),
                    sim.snapshot(),
                    sim.now_ms(),
                    sim.completed_frame(),
                    sim.completed_warning(),
                    sim.led_duty(),
                    sim.session().popup(),
                    sim.session().backlight().compare()
                ),
                accepted
            );
            assert!(!sim.complete_frame(token));
        }
    }
}

#[test]
/// Candidate Logs history remains separate; invalid JSON and Discard restore accepted pixels and scroll anchor.
fn accepted_logs_keep_history_anchor_during_invalid_json_and_discard() {
    let mut editor = Editor::default();
    let mut sim = ControlSimulator::new(editor.preview().clone());
    sim.tap(ButtonId::Y);
    sim.tap(ButtonId::Y);
    sim.rotate(-8);
    let accepted_anchor = sim.session().log_anchor();
    assert!(accepted_anchor.is_some());
    let accepted_page = sim.log_page();
    let mut expected = Box::new(FrameBuffer::new());
    let mut display = St7789Renderer::new(&mut expected);
    accepted_page.draw_full(&mut display);
    accepted_page.draw_hint(&mut display, &editor.preview().config().controls);
    let mut preview = Preview::new();
    preview.render_simulation(&sim).unwrap();
    assert!(preview.rgb565() == &expected[..]);

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
    sim.tap(ButtonId::Y);
    sim.rotate(-2);
    assert_ne!(sim.display_session().log_anchor(), accepted_anchor);
    let candidate_page = sim.log_page();
    let mut candidate_expected = Box::new(FrameBuffer::new());
    let mut display = St7789Renderer::new(&mut candidate_expected);
    candidate_page.draw_full(&mut display);
    candidate_page.draw_hint(&mut display, &raw.controls);
    preview.render_candidate_simulation(&sim, &proof).unwrap();
    assert!(preview.rgb565() == &candidate_expected[..]);
    assert!(candidate_expected[..] != expected[..]);

    let parse = editor.set_json_text(handle, "{".into()).unwrap();
    editor.finish_json_parse(parse).unwrap();
    sim.inspect_invalid(editor.preview().config().controls);
    assert_eq!(sim.display_session().log_anchor(), None);
    preview.render_simulation(&sim).unwrap();
    assert!(
        preview.rgb565() == &expected[..],
        "invalid JSON must display accepted HISTORY"
    );
    editor.discard_json().unwrap();
    sim.resume_accepted_bindings();
    preview.render_simulation(&sim).unwrap();
    assert_eq!(sim.session().log_anchor(), accepted_anchor);
    assert!(
        preview.rgb565() == &expected[..],
        "Discard must preserve accepted HISTORY"
    );
}
