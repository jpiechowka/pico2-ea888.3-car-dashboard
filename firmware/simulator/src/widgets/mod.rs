//! Widget components for the OBD dashboard display.
//!
//! Re-exports platform-agnostic widgets from the common crate.

// Re-export widgets used by the simulator
pub use dashboard_common::widgets::{
    draw_afr_cell,
    draw_batt_cell,
    draw_boost_cell,
    draw_boost_unit_popup,
    draw_dividers,
    draw_fps_toggle_popup,
    draw_header,
    draw_reset_popup,
    draw_temp_cell,
    is_critical_afr,
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
