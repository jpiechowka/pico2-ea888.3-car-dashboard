mod battery;
mod boost;
mod lambda;
mod temp;

pub use battery::draw_batt_cell;
pub use boost::draw_boost_cell;
use embedded_graphics::mono_font::MonoTextStyle;
use embedded_graphics::pixelcolor::Rgb565;
pub use lambda::draw_lambda_cell;
pub use temp::{draw_temp_cell, temp_uses_medium_font};

use crate::ui::{
    BLACK,
    LABEL_STYLE_BLACK,
    LABEL_STYLE_WHITE,
    VALUE_FONT,
    VALUE_STYLE_BLACK,
    VALUE_STYLE_WHITE,
    WHITE,
    YELLOW,
};

pub struct SensorDisplayData<'a> {
    pub trend: Option<bool>,
    pub is_new_peak: bool,
    pub history: crate::history::HistoryView<'a>,
    pub average: Option<f32>,
}

pub fn label_color_for_bg(bg_color: Rgb565) -> Rgb565 { crate::ui::colors::readable_foreground(bg_color) }

#[inline]
pub(crate) fn peak_highlight_for_text(base_text: Rgb565) -> Rgb565 { if base_text == WHITE { YELLOW } else { BLACK } }

#[inline]
pub(crate) fn label_style_for_text(base_text: Rgb565) -> MonoTextStyle<'static, Rgb565> {
    if base_text == WHITE {
        LABEL_STYLE_WHITE
    } else {
        LABEL_STYLE_BLACK
    }
}

pub(crate) fn value_style_for_color(color: Rgb565) -> MonoTextStyle<'static, Rgb565> {
    if color == WHITE {
        VALUE_STYLE_WHITE
    } else if color == BLACK {
        VALUE_STYLE_BLACK
    } else {
        MonoTextStyle::new(VALUE_FONT, color)
    }
}
