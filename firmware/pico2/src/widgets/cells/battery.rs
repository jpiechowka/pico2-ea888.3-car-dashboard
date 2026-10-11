use embedded_graphics::mono_font::MonoTextStyle;
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::*;
use embedded_graphics::text::Text;

use super::{SensorDisplayData, label_color_for_bg, label_style_for_text, peak_highlight_for_text};
use crate::ui::{BLACK, CENTERED, LABEL_FONT, ORANGE, VALUE_FONT_MEDIUM, WHITE};
use crate::widgets::primitives::{draw_cell_background, draw_mini_graph, draw_trend_arrow, draw_value_with_outline};

#[allow(clippy::too_many_arguments)]
pub fn draw_batt_cell<D>(
    display: &mut D,
    x: u32,
    y: u32,
    w: u32,
    h: u32,
    voltage: f32,
    min_voltage: f32,
    max_voltage: f32,
    state: &SensorDisplayData<'_>,
    bg_color: Rgb565,
    is_critical: bool,
) -> Rgb565
where
    D: DrawTarget<Color = Rgb565>,
{
    let text = crate::widgets::cell_text::battery(voltage, min_voltage, max_voltage);
    draw_cell_background(display, x, y, w, h, bg_color);

    let base_text = label_color_for_bg(bg_color);
    let label_style = label_style_for_text(base_text);
    let peak_color = peak_highlight_for_text(base_text);

    let center_x = (x + w / 2) as i32;
    let center_y = (y + h / 2) as i32;
    let value_x = center_x;

    Text::with_text_style("ECU V", Point::new(center_x, y as i32 + 14), label_style, CENTERED)
        .draw(display)
        .ok();

    if let Some(rising) = state.trend {
        draw_trend_arrow(display, center_x + 20, y as i32 + 10, rising, base_text);
    }

    let value_color = if state.is_new_peak { peak_color } else { base_text };

    draw_value_with_outline(
        display,
        &text[0],
        Point::new(value_x, center_y - 7),
        VALUE_FONT_MEDIUM,
        value_color,
        CENTERED,
    );

    let graph_y = center_y + 4;
    let graph_h = 20u32;
    let graph_w = w - 16;
    let graph_x = x as i32 + 8;

    let graph_line_color = base_text;
    draw_mini_graph(
        display,
        graph_x,
        graph_y,
        graph_w,
        graph_h,
        state.graph_buffer,
        state.graph_buffer_size,
        state.graph_start_idx,
        state.graph_count,
        state.graph_min,
        state.graph_max,
        |_| graph_line_color,
    );

    let minmax_color = if base_text == BLACK {
        BLACK
    } else if is_critical {
        WHITE
    } else {
        ORANGE
    };
    let minmax_style = MonoTextStyle::new(LABEL_FONT, minmax_color);

    Text::with_text_style(
        &text[1],
        Point::new(center_x, (y + h) as i32 - 18),
        minmax_style,
        CENTERED,
    )
    .draw(display)
    .ok();

    Text::with_text_style(
        &text[2],
        Point::new(center_x, (y + h) as i32 - 8),
        minmax_style,
        CENTERED,
    )
    .draw(display)
    .ok();

    bg_color
}
