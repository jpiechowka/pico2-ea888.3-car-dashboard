pub mod cell_text;
mod cells;
pub(crate) use cells::label_color_for_bg;
mod header;
mod popups;
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
};
