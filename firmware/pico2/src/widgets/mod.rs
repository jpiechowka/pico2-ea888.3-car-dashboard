pub mod cell_text;
mod cells;
mod header;
mod popups;
pub mod primitives;

pub use cells::{
    SensorDisplayData,
    afr_color_status,
    battery_color,
    draw_afr_cell,
    draw_batt_cell,
    draw_boost_cell,
    draw_temp_cell,
    is_critical_afr,
    is_critical_battery,
    is_critical_egt,
    is_critical_iat,
    is_critical_oil_dsg,
    is_critical_water,
    is_low_temp_oil,
    temp_color_egt,
    temp_color_iat,
    temp_color_oil_dsg,
    temp_color_water,
    temp_uses_medium_font,
};
pub use header::{draw_dividers, draw_header};
pub use popups::{
    draw_boost_unit_popup,
    draw_brightness_popup,
    draw_danger_manifold_popup,
    draw_fps_toggle_popup,
    draw_reset_popup,
};
