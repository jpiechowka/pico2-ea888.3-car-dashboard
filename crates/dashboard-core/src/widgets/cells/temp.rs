//! Legacy temperature cell with optional canonical average and raw history.
use embedded_graphics::mono_font::MonoTextStyle;
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::*;
use embedded_graphics::text::Text;

use super::{SensorDisplayData, label_color_for_bg, label_style_for_text, peak_highlight_for_text};
use crate::ui::{BLACK, CENTERED, LABEL_FONT, ORANGE, VALUE_FONT, VALUE_FONT_MEDIUM, WHITE};
use crate::widgets::primitives::{draw_cell_background, draw_history_graph, draw_trend_arrow, draw_value_with_outline};

/// Celsius value rounding to four positive digits, switching to the narrower value font.
const TEMP_LARGE_VALUE_THRESHOLD: f32 = 999.5;

/// Returns whether `value` degrees Celsius is at least 999.5; NaN returns false.
/// This legacy heuristic is not general string fitting for negative/extreme inputs.
pub fn temp_uses_medium_font(value: f32) -> bool { value >= TEMP_LARGE_VALUE_THRESHOLD }

/// Large-font baseline offset in pixels relative to the cell's vertical center.
const TEMP_VALUE_Y_LARGE: i32 = -12;

/// Medium-font baseline offset in pixels relative to the cell's vertical center.
const TEMP_VALUE_Y_MEDIUM: i32 = -10;

#[allow(clippy::too_many_arguments)]
/// Draws `label`, current `temp`/`max_temp` Celsius and `state` trend/peak/history/
/// optional average into `display` at bounded pixel `x`, `y`, width `w`, height `h`.
/// Uses `bg_color`; `is_critical` adjusts statistic contrast. Returns the background.
/// Caller supplies qualified finite data, a short fitting label, `w >= 16`, legacy
/// cell geometry and clipping; extreme coordinates can overflow. Draw errors are discarded.
pub fn draw_temp_cell<D>(
    display: &mut D,
    x: u32,
    y: u32,
    w: u32,
    h: u32,
    label: &str,
    temp: f32,
    max_temp: f32,
    state: &SensorDisplayData<'_>,
    bg_color: Rgb565,
    is_critical: bool,
) -> Rgb565
where
    D: DrawTarget<Color = Rgb565>,
{
    draw_cell_background(display, x, y, w, h, bg_color);

    let base_text = label_color_for_bg(bg_color);
    let label_style = label_style_for_text(base_text);
    let peak_color = peak_highlight_for_text(base_text);

    let center_x = (x + w / 2) as i32;
    let center_y = (y + h / 2) as i32;
    let value_x = center_x;

    let label_x = center_x;

    Text::with_text_style(label, Point::new(label_x, y as i32 + 14), label_style, CENTERED)
        .draw(display)
        .ok();

    if let Some(rising) = state.trend {
        let arrow_x = label_x + (label.len() as i32 * 3) + 8;
        draw_trend_arrow(display, arrow_x, y as i32 + 10, rising, base_text);
    }

    let text = crate::widgets::cell_text::temperature(temp, max_temp, state.average);
    let value_color = if state.is_new_peak { peak_color } else { base_text };

    let (value_font, value_y_offset) = if temp_uses_medium_font(temp) {
        (VALUE_FONT_MEDIUM, TEMP_VALUE_Y_MEDIUM)
    } else {
        (VALUE_FONT, TEMP_VALUE_Y_LARGE)
    };

    draw_value_with_outline(
        display,
        &text[0],
        Point::new(value_x, center_y + value_y_offset),
        value_font,
        value_color,
        CENTERED,
    );

    let graph_y = center_y + 4;
    let graph_h = 20u32;
    let graph_w = w - 16;
    let graph_x = x as i32 + 8;

    let graph_line_color = base_text;
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

    let avg_color = if base_text == BLACK {
        BLACK
    } else if is_critical {
        WHITE
    } else {
        ORANGE
    };
    let avg_style = MonoTextStyle::new(LABEL_FONT, avg_color);

    let max_style = avg_style;

    if state.average.is_some() {
        Text::with_text_style(&text[1], Point::new(center_x, (y + h) as i32 - 22), avg_style, CENTERED)
            .draw(display)
            .ok();
    }

    let max_pos = Point::new(center_x, (y + h) as i32 - 6);
    let max_text = Text::with_text_style(&text[2], max_pos, max_style, CENTERED);

    max_text.draw(display).ok();

    bg_color
}
