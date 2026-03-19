use core::fmt::Write;

use embedded_graphics::mono_font::MonoTextStyle;
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::{PrimitiveStyle, Rectangle};
use embedded_graphics::text::Text;
use heapless::String;

use super::{SensorDisplayData, label_color_for_bg, label_style_for_text, peak_highlight_for_text};
use crate::thresholds::{
    COOLANT_COLD_MAX,
    COOLANT_CRITICAL,
    EGT_COLD_MAX,
    EGT_CRITICAL,
    EGT_HIGH_LOAD,
    EGT_SPIRITED,
    IAT_COLD,
    IAT_CRITICAL,
    IAT_EXTREME_COLD,
    IAT_HOT,
    IAT_WARM,
    OIL_DSG_CRITICAL,
    OIL_DSG_ELEVATED,
    OIL_DSG_HIGH,
    OIL_LOW_TEMP,
};
use crate::ui::{BLACK, BLUE, CENTERED, GREEN, LABEL_FONT, ORANGE, RED, VALUE_FONT, VALUE_FONT_MEDIUM, WHITE, YELLOW};
use crate::widgets::primitives::{draw_cell_background, draw_mini_graph, draw_trend_arrow, draw_value_with_outline};

const TEMP_LARGE_VALUE_THRESHOLD: f32 = 999.5;

const TEMP_VALUE_Y_LARGE: i32 = -12;

const TEMP_VALUE_Y_MEDIUM: i32 = -10;

const LOW_BADGE_MARGIN: u32 = 9;

const LOW_LABEL_SHIFT: i32 = 12;

pub fn temp_color_oil_dsg(temp: f32) -> (Rgb565, Rgb565) {
    if temp >= OIL_DSG_CRITICAL {
        (RED, WHITE)
    } else if temp >= OIL_DSG_HIGH {
        (ORANGE, BLACK)
    } else if temp >= OIL_DSG_ELEVATED {
        (YELLOW, BLACK)
    } else {
        (BLACK, WHITE)
    }
}

pub fn temp_color_water(temp: f32) -> (Rgb565, Rgb565) {
    if temp > COOLANT_CRITICAL {
        (RED, WHITE)
    } else if temp >= COOLANT_COLD_MAX {
        (GREEN, BLACK)
    } else {
        (ORANGE, BLACK)
    }
}

pub fn is_critical_oil_dsg(temp: f32) -> bool { temp >= OIL_DSG_CRITICAL }

pub fn is_critical_water(temp: f32) -> bool { temp > COOLANT_CRITICAL }

pub fn temp_color_iat(temp: f32) -> (Rgb565, Rgb565) {
    if temp >= IAT_CRITICAL {
        (RED, WHITE)
    } else if temp >= IAT_HOT {
        (ORANGE, BLACK)
    } else if temp >= IAT_WARM {
        (YELLOW, BLACK)
    } else if temp >= IAT_COLD {
        (GREEN, BLACK)
    } else {
        (BLUE, WHITE)
    }
}

pub fn is_critical_iat(temp: f32) -> bool { temp >= IAT_CRITICAL || temp <= IAT_EXTREME_COLD }

pub fn temp_color_egt(temp: f32) -> (Rgb565, Rgb565) {
    if temp >= EGT_CRITICAL {
        (RED, WHITE)
    } else if temp >= EGT_HIGH_LOAD {
        (ORANGE, BLACK)
    } else if temp >= EGT_SPIRITED {
        (YELLOW, BLACK)
    } else if temp >= EGT_COLD_MAX {
        (GREEN, BLACK)
    } else {
        (BLUE, WHITE)
    }
}

pub fn is_critical_egt(temp: f32) -> bool { temp >= EGT_CRITICAL }

pub fn is_low_temp_oil(temp: f32) -> bool { temp < OIL_LOW_TEMP }

fn draw_low_warning_badge<D>(
    display: &mut D,
    x: u32,
    y: u32,
    blink_on: bool,
) where
    D: DrawTarget<Color = Rgb565>,
{
    let badge_w = 32u32;
    let badge_h = 14u32;
    let badge_x = (x + LOW_BADGE_MARGIN) as i32;
    let badge_y = (y + 4) as i32;

    let (bg_color, text_color) = if blink_on { (RED, WHITE) } else { (WHITE, BLACK) };

    Rectangle::new(
        Point::new(badge_x - 1, badge_y - 1),
        Size::new(badge_w + 2, badge_h + 2),
    )
    .into_styled(PrimitiveStyle::with_fill(BLACK))
    .draw(display)
    .ok();

    Rectangle::new(Point::new(badge_x, badge_y), Size::new(badge_w, badge_h))
        .into_styled(PrimitiveStyle::with_fill(bg_color))
        .draw(display)
        .ok();

    let label_style = MonoTextStyle::new(LABEL_FONT, text_color);
    Text::with_text_style(
        "LOW",
        Point::new(badge_x + badge_w as i32 / 2, badge_y + 10),
        label_style,
        CENTERED,
    )
    .draw(display)
    .ok();
}

#[allow(clippy::too_many_arguments)]
pub fn draw_temp_cell<D, F, C, L>(
    display: &mut D,
    x: u32,
    y: u32,
    w: u32,
    h: u32,
    label: &str,
    temp: f32,
    max_temp: f32,
    state: &SensorDisplayData<'_>,
    color_fn: F,
    critical_fn: C,
    low_fn: Option<L>,
    blink_on: bool,
    shake_offset: i32,
    bg_override: Option<Rgb565>,
) -> Rgb565
where
    D: DrawTarget<Color = Rgb565>,
    F: Fn(f32) -> (Rgb565, Rgb565),
    C: Fn(f32) -> bool,
    L: Fn(f32) -> bool,
{
    let (mut bg_color, _) = color_fn(temp);
    let is_critical = critical_fn(temp);
    let max_is_critical = critical_fn(max_temp);
    let is_low = low_fn.as_ref().is_some_and(|f| f(temp));

    if let Some(override_color) = bg_override {
        bg_color = override_color;
    }

    if is_critical && !blink_on {
        bg_color = BLACK;
    }

    draw_cell_background(display, x, y, w, h, bg_color);

    if is_low {
        draw_low_warning_badge(display, x, y, blink_on);
    }

    let base_text = label_color_for_bg(bg_color);
    let label_style = label_style_for_text(base_text);
    let peak_color = peak_highlight_for_text(base_text);

    let center_x = (x + w / 2) as i32;
    let center_y = (y + h / 2) as i32;
    let value_x = center_x + shake_offset;

    let label_x = if is_low { center_x + LOW_LABEL_SHIFT } else { center_x };

    Text::with_text_style(label, Point::new(label_x, y as i32 + 14), label_style, CENTERED)
        .draw(display)
        .ok();

    if let Some(rising) = state.trend {
        let arrow_x = label_x + (label.len() as i32 * 3) + 8;
        draw_trend_arrow(display, arrow_x, y as i32 + 10, rising, base_text);
    }

    let mut value_str: String<16> = String::new();
    let _ = write!(value_str, "{temp:.0}C");
    let value_color = if state.is_new_peak { peak_color } else { base_text };

    let (value_font, value_y_offset) = if temp >= TEMP_LARGE_VALUE_THRESHOLD {
        (VALUE_FONT_MEDIUM, TEMP_VALUE_Y_MEDIUM)
    } else {
        (VALUE_FONT, TEMP_VALUE_Y_LARGE)
    };

    draw_value_with_outline(
        display,
        &value_str,
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

    let avg_color = if base_text == BLACK {
        BLACK
    } else if is_critical {
        WHITE
    } else {
        ORANGE
    };
    let avg_style = MonoTextStyle::new(LABEL_FONT, avg_color);

    let max_warning = max_is_critical && !is_critical;
    let max_color = if max_is_critical {
        if is_critical { WHITE } else { RED }
    } else if base_text == BLACK {
        BLACK
    } else if is_critical {
        WHITE
    } else {
        ORANGE
    };
    let max_style = MonoTextStyle::new(LABEL_FONT, max_color);

    if let Some(avg) = state.average {
        let mut avg_str: String<16> = String::new();
        let _ = write!(avg_str, "AVG {avg:.0}C");
        Text::with_text_style(&avg_str, Point::new(center_x, (y + h) as i32 - 22), avg_style, CENTERED)
            .draw(display)
            .ok();
    }

    let mut max_str: String<16> = String::new();
    let _ = write!(max_str, "MAX {max_temp:.0}C");
    let max_pos = Point::new(center_x, (y + h) as i32 - 6);
    let max_text = Text::with_text_style(&max_str, max_pos, max_style, CENTERED);

    if max_warning {
        let bb = max_text.bounding_box();
        let pad: i32 = 2;
        let cell_left = x as i32 + 2;
        let cell_right = (x + w) as i32 - 2;
        let badge_left = (bb.top_left.x - pad).max(cell_left);
        let badge_right = (bb.top_left.x + bb.size.width as i32 + pad).min(cell_right);
        let badge_pos = Point::new(badge_left, bb.top_left.y - pad);
        let badge_width = (badge_right - badge_left).max(0) as u32;
        let badge_size = Size::new(badge_width, bb.size.height + (pad as u32 * 2));
        Rectangle::new(badge_pos, badge_size)
            .into_styled(PrimitiveStyle::with_fill(BLACK))
            .draw(display)
            .ok();
    }

    max_text.draw(display).ok();

    bg_color
}
