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
    for _ in 0..2 {
        state.mark_display_cleared();
        assert!(state.check_header_dirty(FpsMode::Instant, 40.0, 40.0));
        state.end_frame();
    }
    for buffer in [0, 1] {
        assert!(
            state.check_header_dirty(FpsMode::Instant, 41.0, 40.0),
            "buffer {buffer} must receive the updated header"
        );
        state.end_frame();
    }
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
