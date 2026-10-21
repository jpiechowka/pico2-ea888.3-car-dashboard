//! Legacy lambda cell with an explicitly assumed 14.7 gasoline-equivalent AFR reference.
use embedded_graphics::mono_font::MonoTextStyle;
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::*;
use embedded_graphics::text::Text;

use super::{SensorDisplayData, label_color_for_bg, label_style_for_text};
use crate::ui::{BLACK, CENTERED, LABEL_FONT, VALUE_FONT};
use crate::widgets::primitives::{draw_cell_background, draw_history_graph, draw_value_with_outline};

#[allow(clippy::too_many_arguments)]
/// Draws lambda-ratio `lambda` and `state` raw history into `display` at bounded
/// panel `x`, `y`, width `w`, height `h`. `shake_offset` shifts only the value;
/// `bg_override` selects background or defaults black. Returns that background.
/// Caller supplies qualified values, `w >= 16`, fitting legacy-cell geometry and
/// clipping; arbitrary geometry can overflow and target errors are discarded.
pub fn draw_lambda_cell<D>(
    display: &mut D,
    x: u32,
    y: u32,
    w: u32,
    h: u32,
    lambda: f32,
    state: &SensorDisplayData<'_>,
    shake_offset: i32,
    bg_override: Option<Rgb565>,
) -> Rgb565
where
    D: DrawTarget<Color = Rgb565>,
{
    let bg_color = bg_override.unwrap_or(BLACK);
    let text = crate::widgets::cell_text::lambda(lambda);

    let text_color = label_color_for_bg(bg_color);
    let label_style = label_style_for_text(text_color);

    draw_cell_background(display, x, y, w, h, bg_color);

    let center_x = (x + w / 2) as i32;
    let center_y = (y + h / 2) as i32;
    let value_x = center_x + shake_offset;

    Text::with_text_style("LAMBDA", Point::new(center_x, y as i32 + 14), label_style, CENTERED)
        .draw(display)
        .ok();

    draw_value_with_outline(
        display,
        &text[0],
        Point::new(value_x, center_y - 14),
        VALUE_FONT,
        text_color,
        CENTERED,
    );

    Text::with_text_style(&text[1], Point::new(center_x, center_y + 5), label_style, CENTERED)
        .draw(display)
        .ok();

    let graph_y = center_y + 14;
    let graph_h = 16u32;
    let graph_w = w - 16;
    let graph_x = x as i32 + 8;

    let graph_line_color = text_color;
    draw_history_graph(
        display,
        graph_x,
        graph_y,
        graph_w,
        graph_h,
        state.history,
        state.history.bounds(),
        |_| graph_line_color,
    );

    let status_style = MonoTextStyle::new(LABEL_FONT, text_color);
    Text::with_text_style(
        &text[2],
        Point::new(center_x, (y + h) as i32 - 8),
        status_style,
        CENTERED,
    )
    .draw(display)
    .ok();

    bg_color
}
