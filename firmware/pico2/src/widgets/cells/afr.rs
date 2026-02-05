//! Air-Fuel Ratio (AFR) cell rendering.

use core::fmt::Write;

use embedded_graphics::mono_font::MonoTextStyle;
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::*;
use embedded_graphics::text::Text;
use heapless::String;

use super::{SensorDisplayData, label_color_for_bg, label_style_for_text};
use crate::colors::{BLACK, BLUE, DARK_TEAL, GREEN, ORANGE, RED};
use crate::styles::{CENTERED, LABEL_FONT, VALUE_FONT};
use crate::thresholds::{AFR_LEAN_CRITICAL, AFR_OPTIMAL_MAX, AFR_RICH, AFR_RICH_AF, AFR_STOICH};
use crate::widgets::primitives::{draw_cell_background, draw_mini_graph, draw_value_with_outline};

#[allow(clippy::too_many_arguments)]
pub fn draw_afr_cell<D>(
    display: &mut D,
    x: u32,
    y: u32,
    w: u32,
    h: u32,
    afr: f32,
    state: &SensorDisplayData<'_>,
    blink_on: bool,
    shake_offset: i32,
    bg_override: Option<Rgb565>,
) -> Rgb565
where
    D: DrawTarget<Color = Rgb565>,
{
    let is_critical = afr > AFR_LEAN_CRITICAL;
    let (mut bg_color, status) = if afr < AFR_RICH_AF {
        (BLUE, "RICH AF")
    } else if afr < AFR_RICH {
        (DARK_TEAL, "RICH")
    } else if afr < AFR_OPTIMAL_MAX {
        (GREEN, "OPTIMAL")
    } else if afr <= AFR_LEAN_CRITICAL {
        (ORANGE, "LEAN")
    } else {
        (RED, "LEAN AF")
    };

    if let Some(override_color) = bg_override {
        bg_color = override_color;
    }

    if is_critical && !blink_on {
        bg_color = BLACK;
    }

    let text_color = label_color_for_bg(bg_color);
    let label_style = label_style_for_text(text_color);

    draw_cell_background(display, x, y, w, h, bg_color);

    let center_x = (x + w / 2) as i32;
    let center_y = (y + h / 2) as i32;
    let value_x = center_x + shake_offset;

    Text::with_text_style("AFR/LAMBDA", Point::new(center_x, y as i32 + 14), label_style, CENTERED)
        .draw(display)
        .ok();

    let mut value_str: String<16> = String::new();
    let _ = write!(value_str, "{afr:.1}");

    draw_value_with_outline(
        display,
        &value_str,
        Point::new(value_x, center_y - 14),
        VALUE_FONT,
        text_color,
        CENTERED,
    );

    let lambda = afr / AFR_STOICH;
    let mut lambda_str: String<16> = String::new();
    let _ = write!(lambda_str, "L {lambda:.2}");
    let lambda_style = MonoTextStyle::new(LABEL_FONT, text_color);
    Text::with_text_style(&lambda_str, Point::new(center_x, center_y + 4), lambda_style, CENTERED)
        .draw(display)
        .ok();

    let graph_y = center_y + 14;
    let graph_h = 16u32;
    let graph_w = w - 16;
    let graph_x = x as i32 + 8;

    let graph_line_color = text_color;
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

    let status_style = MonoTextStyle::new(LABEL_FONT, text_color);
    Text::with_text_style(status, Point::new(center_x, (y + h) as i32 - 8), status_style, CENTERED)
        .draw(display)
        .ok();

    bg_color
}
