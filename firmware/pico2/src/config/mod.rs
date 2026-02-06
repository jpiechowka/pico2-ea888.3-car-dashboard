//! Application configuration.
//!
//! - `layout`: Display dimensions and pre-computed layout constants
//! - `sensors`: Sensor threshold values and validation

pub mod layout;
pub mod sensors;

// Re-export layout constants at config level for convenience
pub use layout::{
    CENTER_X,
    CENTER_Y,
    COL_WIDTH,
    HEADER_HEIGHT,
    HISTORY_SIZE,
    ROW_HEIGHT,
    SCREEN_HEIGHT,
    SCREEN_WIDTH,
    TREND_THRESHOLD,
};
// Re-export sensor thresholds at config level for convenience
pub use sensors::{
    AFR_LEAN_CRITICAL,
    AFR_OPTIMAL_MAX,
    AFR_RICH,
    AFR_RICH_AF,
    AFR_STOICH,
    BAR_TO_PSI,
    BATT_CRITICAL,
    BATT_WARNING,
    BOOST_EASTER_EGG_BAR,
    BOOST_EASTER_EGG_PSI,
    COOLANT_COLD_MAX,
    COOLANT_CRITICAL,
    EGT_COLD_MAX,
    EGT_CRITICAL,
    EGT_DANGER_MANIFOLD,
    EGT_HIGH_LOAD,
    EGT_SPIRITED,
    IAT_COLD,
    IAT_CRITICAL,
    IAT_EXTREME_COLD,
    IAT_HOT,
    IAT_WARM,
    OIL_DSG_CRITICAL,
    OIL_DSG_ELEVATED,
    OIL_DSG_HIGH,
    OIL_LOW_TEMP,
    is_critical_battery,
};
