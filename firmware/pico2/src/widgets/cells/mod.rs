mod afr;
mod battery;
mod boost;
mod temp;

pub use afr::draw_afr_cell;
pub use battery::draw_batt_cell;
pub use boost::draw_boost_cell;
use embedded_graphics::mono_font::MonoTextStyle;
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::IntoStorage;
pub use temp::{
    draw_temp_cell,
    is_critical_egt,
    is_critical_iat,
    is_critical_oil_dsg,
    is_critical_water,
    is_low_temp_oil,
    temp_color_egt,
    temp_color_iat,
    temp_color_oil_dsg,
    temp_color_water,
};

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
    pub graph_buffer: &'a [f32],
    pub graph_buffer_size: usize,
    pub graph_start_idx: usize,
    pub graph_count: usize,
    pub graph_min: f32,
    pub graph_max: f32,
    pub average: Option<f32>,
}

impl<'a> SensorDisplayData<'a> {}

pub fn label_color_for_bg(bg_color: Rgb565) -> Rgb565 {
    let luma = calculate_luminance(bg_color);
    if luma < 128 { WHITE } else { BLACK }
}

#[inline]
pub(crate) fn peak_highlight_for_text(base_text: Rgb565) -> Rgb565 { if base_text == WHITE { YELLOW } else { BLACK } }

#[inline]
pub(crate) fn calculate_luminance(color: Rgb565) -> u32 {
    let raw = color.into_storage();
    let r5 = u32::from((raw >> 11) & 0x1F);
    let g6 = u32::from((raw >> 5) & 0x3F);
    let b5 = u32::from(raw & 0x1F);

    let r8 = (r5 << 3) | (r5 >> 2);
    let g8 = (g6 << 2) | (g6 >> 4);
    let b8 = (b5 << 3) | (b5 >> 2);

    (r8 * 77 + g8 * 150 + b8 * 29) >> 8
}

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
