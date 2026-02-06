//! Low-level drawing primitives shared across widgets.
//!
//! # Feature Flags
//!
//! - **`simple-outline`**: Uses 2-pass shadow instead of 8-pass outline for `draw_value_with_outline()`. Reduces draw
//!   calls from 9 to 3 per text, significantly improving FPS on embedded targets.

use embedded_graphics::mono_font::{MonoFont, MonoTextStyle};
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::{Line, PrimitiveStyle, Rectangle};
use embedded_graphics::text::{Text, TextStyle};

use crate::ui::{BLACK, WHITE};

/// Draw a cell's background rectangle with 2px inset.
pub fn draw_cell_background<D>(
    display: &mut D,
    x: u32,
    y: u32,
    w: u32,
    h: u32,
    bg_color: Rgb565,
) where
    D: DrawTarget<Color = Rgb565>,
{
    if w < 4 || h < 4 {
        return;
    }
    Rectangle::new(Point::new(x as i32 + 2, y as i32 + 2), Size::new(w - 4, h - 4))
        .into_styled(PrimitiveStyle::with_fill(bg_color))
        .draw(display)
        .ok();
}

/// Draw a trend arrow indicator (up or down).
pub fn draw_trend_arrow<D>(
    display: &mut D,
    x: i32,
    y: i32,
    rising: bool,
    color: Rgb565,
) where
    D: DrawTarget<Color = Rgb565>,
{
    let arrow_style = PrimitiveStyle::with_stroke(color, 1);
    if rising {
        Line::new(Point::new(x, y + 4), Point::new(x, y - 4))
            .into_styled(arrow_style)
            .draw(display)
            .ok();
        Line::new(Point::new(x - 3, y - 1), Point::new(x, y - 4))
            .into_styled(arrow_style)
            .draw(display)
            .ok();
        Line::new(Point::new(x + 3, y - 1), Point::new(x, y - 4))
            .into_styled(arrow_style)
            .draw(display)
            .ok();
    } else {
        Line::new(Point::new(x, y - 4), Point::new(x, y + 4))
            .into_styled(arrow_style)
            .draw(display)
            .ok();
        Line::new(Point::new(x - 3, y + 1), Point::new(x, y + 4))
            .into_styled(arrow_style)
            .draw(display)
            .ok();
        Line::new(Point::new(x + 3, y + 1), Point::new(x, y + 4))
            .into_styled(arrow_style)
            .draw(display)
            .ok();
    }
}

/// Draw a mini sparkline graph showing sensor history.
#[allow(clippy::too_many_arguments)]
pub fn draw_mini_graph<D, F>(
    display: &mut D,
    x: i32,
    y: i32,
    w: u32,
    h: u32,
    buffer: &[f32],
    buffer_size: usize,
    start_idx: usize,
    count: usize,
    data_min: f32,
    data_max: f32,
    color_fn: F,
) where
    D: DrawTarget<Color = Rgb565>,
    F: Fn(f32) -> Rgb565,
{
    if count < 2 {
        return;
    }

    if w < 5 || h < 5 {
        return;
    }

    let graph_width = w as i32 - 4;
    let graph_height = h as i32 - 4;
    let graph_x = x + 2;
    let graph_y = y + 2;

    let max_x = graph_x + graph_width - 1;
    let max_y = graph_y + graph_height - 1;

    let data_range = data_max - data_min;
    let y_scale = if data_range > 0.1 {
        (graph_height - 1) as f32 / data_range
    } else {
        0.0
    };

    // Step by 2 for performance: reduces line draws by ~50% with minimal visual impact
    let step = 2usize;
    let x_step = (graph_width - 1) as f32 / (count - 1).max(1) as f32;

    let mut prev_screen_x = 0i32;
    let mut prev_screen_y = 0i32;
    let mut first_point = true;

    for i in (0..count).step_by(step) {
        let buffer_idx = (start_idx + i) % buffer_size;
        let value = buffer[buffer_idx];

        let screen_x = (graph_x + (i as f32 * x_step) as i32).min(max_x);
        let screen_y = if y_scale > 0.0 {
            (graph_y + graph_height - 1 - ((value - data_min) * y_scale) as i32).clamp(graph_y, max_y)
        } else {
            graph_y + (graph_height - 1) / 2
        };

        let line_color = color_fn(value);
        let line_style = PrimitiveStyle::with_stroke(line_color, 1);

        if !first_point {
            Line::new(Point::new(prev_screen_x, prev_screen_y), Point::new(screen_x, screen_y))
                .into_styled(line_style)
                .draw(display)
                .ok();
        }

        prev_screen_x = screen_x;
        prev_screen_y = screen_y;
        first_point = false;
    }
}

// =============================================================================
// Text Outline Drawing
// =============================================================================

/// Determine the outline color for a given text color.
///
/// Returns BLACK for light text (WHITE, YELLOW, etc.) and WHITE for dark text (BLACK).
/// This ensures maximum contrast for readability on any background.
#[inline]
fn outline_color_for_text(text_color: Rgb565) -> Rgb565 {
    let raw = text_color.into_storage();
    let r5 = u32::from((raw >> 11) & 0x1F);
    let g6 = u32::from((raw >> 5) & 0x3F);
    let b5 = u32::from(raw & 0x1F);
    let r8 = (r5 << 3) | (r5 >> 2);
    let g8 = (g6 << 2) | (g6 >> 4);
    let b8 = (b5 << 3) | (b5 >> 2);
    let luma = (r8 * 77 + g8 * 150 + b8 * 29) >> 8;

    if luma >= 128 { BLACK } else { WHITE }
}

/// Draw text with a contrasting outline for visibility on any background.
///
/// The outline color is automatically selected based on text color luminance:
/// - Light text (WHITE, YELLOW, etc.) → BLACK outline
/// - Dark text (BLACK) → WHITE outline
///
/// # Performance Modes
///
/// - **Default (simulator)**: Full 8-direction outline (9 draw calls per text)
/// - **`simple-outline` feature (Pico)**: Simple 2-direction shadow (3 draw calls per text)
///
/// The `simple-outline` feature reduces draw calls from 9 to 3, significantly improving
/// FPS on embedded targets while maintaining good readability.
pub fn draw_value_with_outline<D>(
    display: &mut D,
    text: &str,
    position: Point,
    font: &MonoFont<'_>,
    text_color: Rgb565,
    text_style: TextStyle,
) where
    D: DrawTarget<Color = Rgb565>,
{
    let outline_color = outline_color_for_text(text_color);
    let outline_char_style = MonoTextStyle::new(font, outline_color);
    let main_char_style = MonoTextStyle::new(font, text_color);

    // Simple shadow mode: 2 offsets (bottom-right shadow) for embedded performance
    #[cfg(feature = "simple-outline")]
    const OFFSETS: [(i32, i32); 2] = [(1, 1), (1, 0)];

    // Full outline mode: 8 directions for maximum visibility on desktop
    #[cfg(not(feature = "simple-outline"))]
    const OFFSETS: [(i32, i32); 8] = [
        (-1, -1),
        (0, -1),
        (1, -1), // top row
        (-1, 0),
        (1, 0), // middle row (skip center)
        (-1, 1),
        (0, 1),
        (1, 1), // bottom row
    ];

    for (dx, dy) in OFFSETS {
        let offset_pos = Point::new(position.x + dx, position.y + dy);
        Text::with_text_style(text, offset_pos, outline_char_style, text_style)
            .draw(display)
            .ok();
    }

    // Draw main text on top
    Text::with_text_style(text, position, main_char_style, text_style)
        .draw(display)
        .ok();
}
