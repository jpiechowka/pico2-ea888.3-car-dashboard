//! Legacy FPS formatting and two-buffer header repair checked against fresh pixels.
use dashboard_host_validation::render::{FpsMode, RenderState};
use dashboard_host_validation::renderer::St7789Renderer;
use dashboard_host_validation::{AlignedFrame, widgets};

/// Checks ties-to-even integer FPS formatting, disabled output and explicit nonfinite/out-of-range markers.
#[test]
fn fps_text_has_bounded_output_and_explicit_invalid_values() {
    use dashboard_host_validation::render::format_fps;
    for (value, expected) in [
        (40.5, "40 FPS"),
        (40.6, "41 FPS"),
        (41.5, "42 FPS"),
        (-0.0, "0 FPS"),
        (-1.0, "-- FPS"),
        (f32::NAN, "-- FPS"),
        (f32::INFINITY, "-- FPS"),
        (f32::MAX, "-- FPS"),
    ] {
        assert_eq!(format_fps(FpsMode::Instant, value, 0.0).as_str(), expected);
    }
    assert_eq!(format_fps(FpsMode::Combined, 9999.0, 9999.0).as_str(), "9999/9999 AVG");
    assert_eq!(
        format_fps(FpsMode::Combined, f32::NAN, f32::NEG_INFINITY).as_str(),
        "--/-- AVG"
    );
    assert!(format_fps(FpsMode::Off, f32::MAX, f32::MAX).is_empty());
}

/// Page clears invalidate both physical headers even when their formatted FPS values remain unchanged.
#[test]
fn page_transition_clears_each_buffer_before_returning_to_cached_header() {
    let mut state = RenderState::new();
    let mut frames = [AlignedFrame::boxed(), AlignedFrame::boxed()];
    let mut reference = AlignedFrame::boxed();
    widgets::draw_header(
        &mut St7789Renderer::new(&mut reference.0),
        FpsMode::Combined,
        40.0,
        39.0,
    );
    for buffer in [0, 1, 0, 1, 0] {
        // main clears both alternating buffers on a page transition.
        frames[buffer].0.fill(0);
        state.mark_display_cleared();
        assert!(state.check_header_dirty(buffer, FpsMode::Combined, 40.0, 39.0));
        widgets::draw_header(
            &mut St7789Renderer::new(&mut frames[buffer].0),
            FpsMode::Combined,
            40.0,
            39.0,
        );
        state.end_frame();
        assert!(frames[buffer].0[..320 * 26 * 2] == reference.0[..320 * 26 * 2]);
    }
}

/// Alternating buffers converge to fresh headers across rounding boundaries and every FPS display mode.
#[test]
fn header_pixels_converge_after_rounding_boundaries_and_mode_changes() {
    let mut state = RenderState::new();
    let mut frames = [AlignedFrame::boxed(), AlignedFrame::boxed()];
    for mode in [FpsMode::Instant, FpsMode::Average, FpsMode::Combined, FpsMode::Off] {
        for value in [40.0, 40.5, 40.6, 41.0, 41.5, 41.6] {
            let mut reference = AlignedFrame::boxed();
            widgets::draw_header(&mut St7789Renderer::new(&mut reference.0), mode, value, value);
            for buffer in [0, 1, 0, 1] {
                if state.check_header_dirty(buffer, mode, value, value) {
                    widgets::draw_header(&mut St7789Renderer::new(&mut frames[buffer].0), mode, value, value);
                }
                state.end_frame();
                assert!(
                    frames[buffer].0[..320 * 26 * 2] == reference.0[..320 * 26 * 2],
                    "header pixels: {mode:?}, {value}, buffer {buffer}"
                );
            }
        }
    }
}

/// Removing an opaque popup repairs corrupted header bytes in both buffers without requiring new telemetry.
#[test]
fn popup_removal_repairs_both_header_buffers_without_a_value_change() {
    let mut state = RenderState::new();
    let mut frames = [AlignedFrame::boxed(), AlignedFrame::boxed()];
    for buffer in [0, 1] {
        assert!(state.check_header_dirty(buffer, FpsMode::Off, 0.0, 0.0));
        widgets::draw_header(&mut St7789Renderer::new(&mut frames[buffer].0), FpsMode::Off, 0.0, 0.0);
        state.end_frame();
    }
    let expected = frames[0].0.to_vec();
    state.update_popup(Some(1));
    for frame in &mut frames {
        frame.0[100..110].fill(0);
    }
    state.update_popup(None);
    for buffer in [0, 1, 0, 1] {
        if state.check_header_dirty(buffer, FpsMode::Off, 0.0, 0.0) {
            widgets::draw_header(&mut St7789Renderer::new(&mut frames[buffer].0), FpsMode::Off, 0.0, 0.0);
        }
        state.end_frame();
        assert!(
            frames[buffer].0[..320 * 26 * 2] == expected[..320 * 26 * 2],
            "buffer {buffer}"
        );
    }
}
