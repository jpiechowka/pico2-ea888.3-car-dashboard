//! Legacy fixed-grid cell renderers; the configured renderer supplies its own bounded lanes.
/// Voltage/current-extrema and history cell drawing.
mod battery;
/// Relative boost and alternate-unit/easter-egg drawing.
mod boost;
/// Lambda and explicitly assumed gasoline-equivalent AFR drawing.
mod lambda;
/// Temperature, optional average and extrema drawing.
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

/// Borrowed acquisition statistics for a synchronous legacy cell draw.
pub struct SensorDisplayData<'a> {
    /// `Some(true)` rising, `Some(false)` falling, `None` no eligible trend arrow.
    pub trend: Option<bool>,
    /// Caller-computed current peak-highlight visibility; no timing state is updated here.
    pub is_new_peak: bool,
    /// Raw canonical elapsed-time history, including missing-value gaps.
    pub history: crate::history::HistoryView<'a>,
    /// Optional canonical mean; only temperature cells display it.
    pub average: Option<f32>,
}

/// Returns black/white foreground selected by shared RGB565 luma for `bg_color`.
pub fn label_color_for_bg(bg_color: Rgb565) -> Rgb565 { crate::ui::colors::readable_foreground(bg_color) }

#[inline]
/// Returns yellow for white `base_text`, otherwise black, to mark a recent peak.
pub(crate) fn peak_highlight_for_text(base_text: Rgb565) -> Rgb565 { if base_text == WHITE { YELLOW } else { BLACK } }

#[inline]
/// Returns the shared white label style only for white `base_text`, otherwise black.
pub(crate) fn label_style_for_text(base_text: Rgb565) -> MonoTextStyle<'static, Rgb565> {
    if base_text == WHITE {
        LABEL_STYLE_WHITE
    } else {
        LABEL_STYLE_BLACK
    }
}

/// Returns the transparent large-value style in `color`, reusing fixed black/white styles when possible.
pub(crate) fn value_style_for_color(color: Rgb565) -> MonoTextStyle<'static, Rgb565> {
    if color == WHITE {
        VALUE_STYLE_WHITE
    } else if color == BLACK {
        VALUE_STYLE_BLACK
    } else {
        MonoTextStyle::new(VALUE_FONT, color)
    }
}
