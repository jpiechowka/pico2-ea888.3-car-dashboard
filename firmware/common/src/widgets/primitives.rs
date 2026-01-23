//! Low-level drawing primitives shared across widgets.

use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::{Line, PrimitiveStyle, Rectangle};

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

    let x_step = (graph_width - 1) as f32 / (count - 1).max(1) as f32;

    let mut prev_screen_x = 0i32;
    let mut prev_screen_y = 0i32;
    let mut first_point = true;

    for i in 0..count {
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
