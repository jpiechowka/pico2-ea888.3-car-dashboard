//! Shared dashboard drawing helpers; adapters retain framebuffer/transfer ownership.
/// Bounded visible strings shared with retained raster equality.
pub mod cell_text;
/// Legacy fixed-grid cells and borrowed display statistics.
mod cells;
/// Native landscape/portrait header and default-grid divider drawing.
mod header;
/// Bounded feedback and warning overlay rendering.
mod popups;
/// Background, graph, arrow and outlined text primitives.
pub mod primitives;

pub use cells::{
    SensorDisplayData,
    draw_batt_cell,
    draw_boost_cell,
    draw_lambda_cell,
    draw_temp_cell,
    temp_uses_medium_font,
};
pub use header::{draw_dividers, draw_header, draw_mode_header, draw_status_header};
pub use popups::{
    draw_boost_unit_popup,
    draw_brightness_popup,
    draw_critical_popup,
    draw_fps_toggle_popup,
    draw_reset_popup,
    draw_view_popup,
    draw_warning_page,
};
/// Bounded numeric/bar/dial geometry, bands and reference/peak markers.
pub mod gauge;
/// Whole-value font fitting and integer glyph enlargement.
pub mod value_text;
