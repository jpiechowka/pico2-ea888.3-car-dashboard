use dashboard_host_validation::render::{FpsMode, RenderState};
use dashboard_host_validation::renderer::St7789Renderer;
use dashboard_host_validation::{AlignedFrame, primitives, ui, widgets};
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::Rectangle;
use embedded_graphics::text::Text;

fn draw_colors(
    rect: Rectangle,
    colors: impl IntoIterator<Item = Rgb565>,
) -> Box<AlignedFrame> {
    let mut frame = AlignedFrame::boxed();
    St7789Renderer::new(&mut frame.0)
        .fill_contiguous(&rect, colors)
        .unwrap();
    frame
}

#[test]
fn a_changed_header_invalidates_both_alternating_frames() {
    let mut state = RenderState::new();
    for buffer in [0, 1] {
        state.mark_display_cleared();
        assert!(state.check_header_dirty(buffer, FpsMode::Instant, 40.0, 40.0));
        state.end_frame();
    }
    for buffer in [0, 1] {
        assert!(
            state.check_header_dirty(buffer, FpsMode::Instant, 41.0, 40.0),
            "buffer {buffer} must receive the updated header"
        );
        state.end_frame();
    }
}

#[test]
fn header_cache_tracks_buffers_independently_when_render_order_repeats() {
    let mut state = RenderState::new();
    assert!(state.check_header_dirty(0, FpsMode::Combined, 40.0, 39.0));
    state.end_frame();
    assert!(!state.check_header_dirty(0, FpsMode::Combined, 40.0, 39.0));
    state.end_frame();
    assert!(state.check_header_dirty(1, FpsMode::Combined, 40.0, 39.0));
    state.end_frame();
    assert!(state.check_header_dirty(0, FpsMode::Combined, 41.0, 39.0));
    state.end_frame();
    assert!(state.check_header_dirty(1, FpsMode::Combined, 41.0, 39.0));
    state.end_frame();
    assert!(!state.check_header_dirty(1, FpsMode::Combined, 41.0, 39.0));
}

#[test]
fn unknown_header_buffer_id_fails_open_without_corrupting_valid_cache() {
    let mut state = RenderState::new();
    assert!(state.check_header_dirty(usize::MAX, FpsMode::Instant, 1.0, 0.0));
    state.end_frame();
    assert!(state.check_header_dirty(0, FpsMode::Instant, 1.0, 0.0));
}

#[test]
fn header_only_tracks_values_visible_in_each_fps_mode() {
    let mut state = RenderState::new();
    assert!(state.check_header_dirty(0, FpsMode::Off, 40.0, 39.0));
    state.end_frame();
    assert!(!state.check_header_dirty(0, FpsMode::Off, 80.0, 79.0));
    assert!(state.check_header_dirty(0, FpsMode::Instant, 80.0, 79.0));
    state.end_frame();
    assert!(!state.check_header_dirty(0, FpsMode::Instant, 80.2, 99.0));
    assert!(!state.check_header_dirty(0, FpsMode::Instant, 80.5, 99.0));
    assert!(state.check_header_dirty(0, FpsMode::Instant, 80.6, 99.0));
    state.end_frame();
    assert!(state.check_header_dirty(0, FpsMode::Average, 80.5, 99.0));
    state.end_frame();
    assert!(!state.check_header_dirty(0, FpsMode::Average, 5.0, 99.2));
    assert!(state.check_header_dirty(0, FpsMode::Combined, 5.0, 99.2));
    state.end_frame();
    assert!(state.check_header_dirty(0, FpsMode::Combined, 5.0, 100.0));
}

#[test]
fn popup_and_display_clear_force_header_redraw() {
    let mut state = RenderState::new();
    assert!(state.check_header_dirty(0, FpsMode::Off, 0.0, 0.0));
    state.end_frame();
    assert!(!state.check_header_dirty(0, FpsMode::Off, 0.0, 0.0));
    state.update_popup(Some(1));
    state.end_frame();
    state.update_popup(None);
    assert!(state.check_header_dirty(0, FpsMode::Off, 0.0, 0.0));
    state.end_frame();
    state.mark_display_cleared();
    assert!(state.check_header_dirty(0, FpsMode::Off, 0.0, 0.0));
}

#[test]
fn left_clipping_keeps_the_original_source_column() {
    let frame = draw_colors(
        Rectangle::new(Point::new(-1, 0), Size::new(3, 1)),
        [Rgb565::RED, Rgb565::GREEN, Rgb565::BLUE],
    );
    assert_eq!(frame.pixel(0, 0), Rgb565::GREEN.into_storage());
    assert_eq!(frame.pixel(1, 0), Rgb565::BLUE.into_storage());
}

#[test]
fn right_clipping_discards_the_hidden_pixel_before_the_next_row() {
    let frame = draw_colors(
        Rectangle::new(Point::new(319, 0), Size::new(2, 2)),
        [Rgb565::RED, Rgb565::GREEN, Rgb565::BLUE, Rgb565::WHITE],
    );
    assert_eq!(frame.pixel(319, 0), Rgb565::RED.into_storage());
    assert_eq!(frame.pixel(319, 1), Rgb565::BLUE.into_storage());
}

#[test]
fn top_clipping_discards_an_entire_source_row() {
    let frame = draw_colors(
        Rectangle::new(Point::new(3, -1), Size::new(2, 2)),
        [Rgb565::RED, Rgb565::GREEN, Rgb565::BLUE, Rgb565::WHITE],
    );
    assert_eq!(frame.pixel(3, 0), Rgb565::BLUE.into_storage());
    assert_eq!(frame.pixel(4, 0), Rgb565::WHITE.into_storage());
}

#[test]
fn bottom_clipping_keeps_only_the_first_source_row() {
    let frame = draw_colors(
        Rectangle::new(Point::new(3, 239), Size::new(2, 2)),
        [Rgb565::RED, Rgb565::GREEN, Rgb565::BLUE, Rgb565::WHITE],
    );
    assert_eq!(frame.pixel(3, 239), Rgb565::RED.into_storage());
    assert_eq!(frame.pixel(4, 239), Rgb565::GREEN.into_storage());
}

#[test]
fn clipped_rgb565_rows_keep_their_original_source_positions() {
    let colors = [
        Rgb565::RED,
        Rgb565::GREEN,
        Rgb565::BLUE,
        Rgb565::WHITE,
        Rgb565::YELLOW,
        Rgb565::CYAN,
        Rgb565::MAGENTA,
        Rgb565::BLACK,
        Rgb565::RED,
    ];
    for (left, top) in [(-1, -1), (318, -1), (-1, 238), (318, 238)] {
        let rect = Rectangle::new(Point::new(left, top), Size::new(3, 3));
        let frame = draw_colors(rect, colors);
        for source_y in 0..3 {
            for source_x in 0..3 {
                let screen_x = left + source_x;
                let screen_y = top + source_y;
                if (0..320).contains(&screen_x) && (0..240).contains(&screen_y) {
                    let expected = colors[(source_y * 3 + source_x) as usize].into_storage();
                    assert_eq!(
                        frame.pixel(screen_x as usize, screen_y as usize),
                        expected,
                        "source ({source_x},{source_y}), rect at ({left},{top})"
                    );
                }
            }
        }
    }
}

#[test]
fn completely_outside_contiguous_fill_leaves_the_frame_untouched() {
    for rect in [
        Rectangle::new(Point::new(-5, 0), Size::new(3, 3)),
        Rectangle::new(Point::new(320, 0), Size::new(3, 3)),
        Rectangle::new(Point::new(0, -5), Size::new(3, 3)),
        Rectangle::new(Point::new(0, 240), Size::new(3, 3)),
    ] {
        let frame = draw_colors(rect, [Rgb565::RED; 9]);
        assert!(frame.0.iter().all(|byte| *byte == 0), "rect={rect:?}");
    }
}

#[test]
fn truncated_clipped_color_stream_never_reuses_a_hidden_pixel() {
    let mut frame = AlignedFrame::boxed();
    St7789Renderer::new(&mut frame.0)
        .fill_contiguous(
            &Rectangle::new(Point::new(-1, 4), Size::new(3, 2)),
            [Rgb565::RED, Rgb565::GREEN, Rgb565::BLUE, Rgb565::WHITE],
        )
        .unwrap();
    assert_eq!(frame.pixel(0, 4), Rgb565::GREEN.into_storage());
    assert_eq!(frame.pixel(1, 4), Rgb565::BLUE.into_storage());
    assert_eq!(frame.pixel(0, 5), 0);
    assert_eq!(frame.pixel(1, 5), 0);
}

#[test]
fn unclipped_rgb565_is_big_endian_on_the_wire() {
    let frame = draw_colors(
        Rectangle::new(Point::new(10, 10), Size::new(2, 1)),
        [Rgb565::RED, Rgb565::BLUE],
    );
    assert_eq!(frame.pixel(10, 10), 0xF800);
    assert_eq!(frame.pixel(11, 10), 0x001F);
}

#[test]
fn solid_fill_matches_a_pixel_reference_at_odd_and_clipped_edges() {
    for (x, width) in [(-1, 4), (1, 1), (1, 4), (318, 4)] {
        let mut frame = AlignedFrame::boxed();
        let rect = Rectangle::new(Point::new(x, 2), Size::new(width, 3));
        St7789Renderer::new(&mut frame.0)
            .fill_solid(&rect, Rgb565::RED)
            .unwrap();
        for y in 0..240 {
            for col in 0..320 {
                let expected = u16::from(rect.contains(Point::new(col as i32, y as i32))) * 0xF800;
                assert_eq!(frame.pixel(col, y), expected, "rect={rect:?}, pixel={col},{y}");
            }
        }
    }
}

fn assert_boost_peak_text(
    expected: &str,
    psi: bool,
    max_boost_bar: f32,
) {
    let mut actual = AlignedFrame::boxed();
    let mut reference = AlignedFrame::boxed();
    widgets::draw_boost_cell(
        &mut St7789Renderer::new(&mut actual.0),
        0,
        26,
        80,
        107,
        1.0,
        max_boost_bar,
        psi,
        false,
        true,
        0,
    );
    Text::with_text_style(expected, Point::new(40, 125), ui::LABEL_STYLE_ORANGE, ui::CENTERED)
        .draw(&mut St7789Renderer::new(&mut reference.0))
        .unwrap();
    for y in 116..132 {
        for x in 2..78 {
            assert_eq!(
                actual.pixel(x, y),
                reference.pixel(x, y),
                "expected peak label {expected} at {x},{y}"
            );
        }
    }
}

#[test]
fn bar_peak_label_is_preserved() { assert_boost_peak_text("MAX 2.00", false, 2.0); }

#[test]
fn psi_peak_label_converts_the_canonical_bar_value() { assert_boost_peak_text("MAX 29.0", true, 2.0); }

#[test]
fn psi_peak_label_rounds_small_bar_values_after_conversion() { assert_boost_peak_text("MAX 1.5", true, 0.1); }

#[test]
fn newest_graph_impulse_is_visible_at_full_history_capacity() {
    let mut samples = [0.0f32; 60];
    let mut steady = AlignedFrame::boxed();
    let mut impulse = AlignedFrame::boxed();
    primitives::draw_mini_graph(
        &mut St7789Renderer::new(&mut steady.0),
        0,
        0,
        64,
        20,
        &samples,
        60,
        0,
        60,
        0.0,
        100.0,
        |_| Rgb565::WHITE,
    );
    samples[59] = 100.0;
    primitives::draw_mini_graph(
        &mut St7789Renderer::new(&mut impulse.0),
        0,
        0,
        64,
        20,
        &samples,
        60,
        0,
        60,
        0.0,
        100.0,
        |_| Rgb565::WHITE,
    );
    assert!(steady.0 != impulse.0, "newest impulse must change graph pixels");
}

fn graph_frame(
    samples: &[f32],
    size: usize,
    start: usize,
    count: usize,
) -> Box<AlignedFrame> {
    let mut frame = AlignedFrame::boxed();
    primitives::draw_mini_graph(
        &mut St7789Renderer::new(&mut frame.0),
        0,
        0,
        64,
        20,
        samples,
        size,
        start,
        count,
        0.0,
        100.0,
        |_| Rgb565::WHITE,
    );
    frame
}

#[test]
fn two_sample_graph_draws_its_latest_endpoint() {
    let flat = graph_frame(&[0.0, 0.0], 2, 0, 2);
    let rising = graph_frame(&[0.0, 100.0], 2, 0, 2);
    assert!(flat.0 != rising.0, "second sample must affect graph pixels");
}

#[test]
fn newest_wrapped_graph_sample_changes_the_pixels() {
    let steady = [0.0f32; 60];
    let mut impulse = steady;
    impulse[16] = 100.0; // start 17, count 60: final sample wraps to 16.
    assert!(
        graph_frame(&steady, 60, 17, 60).0 != graph_frame(&impulse, 60, 17, 60).0,
        "newest wrapped sample must affect graph pixels"
    );
}

#[test]
fn malformed_graph_ring_metadata_draws_nothing() {
    let samples = [25.0f32; 60];
    for (size, start, count) in [(0, 0, 2), (61, 0, 2), (60, 0, 61), (60, 60, 2), (60, usize::MAX, 2)] {
        let frame = graph_frame(&samples, size, start, count);
        assert!(frame.0.iter().all(|byte| *byte == 0), "{size}, {start}, {count}");
    }
}

#[test]
fn every_graph_sample_can_show_an_isolated_spike_after_ring_wrap() {
    for start in [0, 17, 59] {
        let flat = [0.0; 60];
        let reference = graph_frame(&flat, 60, start, 60);
        for offset in 0..60 {
            let mut samples = flat;
            samples[(start + offset) % 60] = 100.0;
            assert!(
                graph_frame(&samples, 60, start, 60).0 != reference.0,
                "lost spike at offset {offset}, ring start {start}"
            );
        }
    }
}

#[test]
fn single_graph_sample_is_visible_at_the_newest_edge() {
    let frame = graph_frame(&[100.0], 1, 0, 1);
    assert_eq!(frame.pixel(61, 2), Rgb565::WHITE.into_storage());
}

#[test]
fn constant_graph_uses_the_middle_row() {
    let mut frame = AlignedFrame::boxed();
    primitives::draw_mini_graph(
        &mut St7789Renderer::new(&mut frame.0),
        0,
        0,
        64,
        20,
        &[5.0; 60],
        60,
        0,
        60,
        5.0,
        5.0,
        |_| Rgb565::WHITE,
    );
    for x in 2..62 {
        assert_eq!(frame.pixel(x, 9), Rgb565::WHITE.into_storage());
    }
}

#[test]
fn graph_rejects_invalid_scales_and_unrepresentable_geometry() {
    for (x, y, w, h, min, max) in [
        (0, 0, 64, 20, f32::NAN, 100.0),
        (0, 0, 64, 20, 0.0, f32::INFINITY),
        (0, 0, 64, 20, 100.0, 0.0),
        (0, 0, 64, 20, -f32::MAX, f32::MAX),
        (i32::MAX, 0, 64, 20, 0.0, 100.0),
        (0, i32::MAX, 64, 20, 0.0, 100.0),
        (0, 0, u32::MAX, 20, 0.0, 100.0),
    ] {
        let mut frame = AlignedFrame::boxed();
        primitives::draw_mini_graph(
            &mut St7789Renderer::new(&mut frame.0),
            x,
            y,
            w,
            h,
            &[0.0, 100.0],
            2,
            0,
            2,
            min,
            max,
            |_| Rgb565::WHITE,
        );
        assert!(frame.0.iter().all(|byte| *byte == 0));
    }
}

#[test]
fn nonfinite_graph_sample_breaks_the_line_without_fabricating_a_point() {
    let frame = graph_frame(&[0.0, f32::NAN, 100.0], 3, 0, 3);
    assert_eq!(frame.pixel(2, 17), Rgb565::WHITE.into_storage());
    assert_eq!(frame.pixel(61, 2), Rgb565::WHITE.into_storage());
    for x in 3..61 {
        for y in 2..18 {
            assert_eq!(frame.pixel(x, y), 0);
        }
    }
}

#[test]
fn small_finite_graph_range_still_uses_the_full_height() {
    let mut frame = AlignedFrame::boxed();
    primitives::draw_mini_graph(
        &mut St7789Renderer::new(&mut frame.0),
        0,
        0,
        64,
        20,
        &[12.0, 12.05],
        2,
        0,
        2,
        12.0,
        12.05,
        |_| Rgb565::WHITE,
    );
    assert_eq!(frame.pixel(2, 17), Rgb565::WHITE.into_storage());
    assert_eq!(frame.pixel(61, 2), Rgb565::WHITE.into_storage());
}
