//! Boost pressure cell rendering.

use core::fmt::Write;

use embedded_graphics::mono_font::MonoTextStyle;
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::*;
use embedded_graphics::text::Text;
use heapless::String;

use super::value_style_for_color;
use crate::thresholds::BAR_TO_PSI;
use crate::ui::{BLACK, CENTERED, LABEL_FONT, LABEL_STYLE_ORANGE, LABEL_STYLE_WHITE, PINK, WHITE};
use crate::widgets::primitives::draw_cell_background;

#[allow(clippy::too_many_arguments)]
pub fn draw_boost_cell<D>(
    display: &mut D,
    x: u32,
    y: u32,
    w: u32,
    h: u32,
    boost_bar: f32,
    max_boost: f32,
    show_psi: bool,
    show_easter_egg: bool,
    blink_on: bool,
    shake_offset: i32,
) where
    D: DrawTarget<Color = Rgb565>,
{
    draw_cell_background(display, x, y, w, h, BLACK);

    let center_x = (x + w / 2) as i32;
    let center_y = (y + h / 2) as i32;
    let value_x = center_x + shake_offset;

    Text::with_text_style(
        "BOOST REL",
        Point::new(center_x, y as i32 + 14),
        LABEL_STYLE_WHITE,
        CENTERED,
    )
    .draw(display)
    .ok();

    let boost_psi = boost_bar * BAR_TO_PSI;

    let mut value_str: String<16> = String::new();
    if show_psi {
        let _ = write!(value_str, "{boost_psi:.1}");
    } else {
        let _ = write!(value_str, "{boost_bar:.2}");
    }
    let value_color = if show_easter_egg {
        if blink_on { PINK } else { WHITE }
    } else {
        WHITE
    };
    let value_style = value_style_for_color(value_color);
    Text::with_text_style(&value_str, Point::new(value_x, center_y - 8), value_style, CENTERED)
        .draw(display)
        .ok();

    let unit_label = if show_psi { "PSI" } else { "BAR" };
    Text::with_text_style(
        unit_label,
        Point::new(center_x, center_y + 10),
        LABEL_STYLE_WHITE,
        CENTERED,
    )
    .draw(display)
    .ok();

    let mut conv_str: String<16> = String::new();
    if show_psi {
        let _ = write!(conv_str, "{boost_bar:.2} BAR");
    } else {
        let _ = write!(conv_str, "{boost_psi:.1} PSI");
    }
    Text::with_text_style(
        &conv_str,
        Point::new(center_x, center_y + 22),
        LABEL_STYLE_WHITE,
        CENTERED,
    )
    .draw(display)
    .ok();

    if show_easter_egg {
        let easter_color = if blink_on { WHITE } else { PINK };
        let easter_style = MonoTextStyle::new(LABEL_FONT, easter_color);
        Text::with_text_style(
            "Fast AF Boi!",
            Point::new(center_x, (y + h) as i32 - 8),
            easter_style,
            CENTERED,
        )
        .draw(display)
        .ok();
    } else {
        let mut max_str: String<16> = String::new();
        if show_psi {
            let _ = write!(max_str, "MAX {max_boost:.1}");
        } else {
            let _ = write!(max_str, "MAX {max_boost:.2}");
        }
        Text::with_text_style(
            &max_str,
            Point::new(center_x, (y + h) as i32 - 8),
            LABEL_STYLE_ORANGE,
            CENTERED,
        )
        .draw(display)
        .ok();
    }
}
