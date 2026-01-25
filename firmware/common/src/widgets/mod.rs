//! Widget components for the OBD dashboard display.
//!
//! All widgets are generic over `DrawTarget<Color = Rgb565>` for platform independence.

mod cells;
mod header;
mod popups;
mod primitives;

pub use cells::{
    SensorDisplayData,
    draw_afr_cell,
    draw_batt_cell,
    draw_boost_cell,
    draw_temp_cell,
    is_critical_afr,
    is_critical_egt,
    is_critical_iat,
    is_critical_oil_dsg,
    is_critical_water,
    is_low_temp_oil,
    label_color_for_bg,
    temp_color_egt,
    temp_color_iat,
    temp_color_oil_dsg,
    temp_color_water,
};
pub use header::{draw_dividers, draw_header};
pub use popups::{draw_boost_unit_popup, draw_fps_toggle_popup, draw_reset_popup};
pub use primitives::{draw_cell_background, draw_mini_graph, draw_trend_arrow, draw_value_with_outline};
