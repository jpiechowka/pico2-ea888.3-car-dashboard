//! Sensor display cells for the OBD dashboard grid.
//!
//! Each cell displays a sensor value with color-coded background based on thresholds,
//! trend arrows, mini sparkline graphs, and warning indicators.
//!
//! # Cell Types
//!
//! - `boost`: Boost pressure in BAR/PSI with easter egg
//! - `temp`: Temperature cells (OIL, COOLANT, DSG, IAT, EGT) with color thresholds
//! - `battery`: Battery voltage with min/max tracking
//! - `afr`: Air-fuel ratio with lambda conversion and status
//!
//! # Value Display
//!
//! All sensor values use contrasting outlines for visibility on any background color.
//! The outline color is automatically selected based on luminance (light text -> dark outline,
//! dark text -> light outline).
//!
//! # Warning States
//!
//! - **Critical (high temp)**: Background flashes RED, text shakes (blink + shake)
//! - **Low (oil only)**: "LOW" badge in top-left with blinking colors when < 75C

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

// =============================================================================
// Sensor Display Data
// =============================================================================

/// Data needed to render a sensor cell.
///
/// This struct decouples the rendering from the state management,
/// allowing different SensorState implementations (std vs no_std)
/// to be used with the same rendering code.
pub struct SensorDisplayData<'a> {
    /// Current trend direction (Some(true) = rising, Some(false) = falling, None = stable).
    pub trend: Option<bool>,
    /// Whether a new peak was just recorded (for highlight effect).
    pub is_new_peak: bool,
    /// Graph history buffer.
    pub graph_buffer: &'a [f32],
    /// Size of the graph buffer (for circular buffer indexing).
    pub graph_buffer_size: usize,
    /// Starting index in the circular buffer.
    pub graph_start_idx: usize,
    /// Number of valid samples in the buffer.
    pub graph_count: usize,
    /// Minimum value in graph data.
    pub graph_min: f32,
    /// Maximum value in graph data.
    pub graph_max: f32,
    /// Rolling average value.
    pub average: Option<f32>,
}

impl<'a> SensorDisplayData<'a> {
    /// Create display data with no graph/trends (minimal display).
    #[allow(dead_code)]
    pub const fn empty() -> Self {
        Self {
            trend: None,
            is_new_peak: false,
            graph_buffer: &[],
            graph_buffer_size: 0,
            graph_start_idx: 0,
            graph_count: 0,
            graph_min: 0.0,
            graph_max: 0.0,
            average: None,
        }
    }
}

// =============================================================================
// Color Helper Functions
// =============================================================================

#[allow(dead_code)]
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

// =============================================================================
// Style Selection Functions
// =============================================================================

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
