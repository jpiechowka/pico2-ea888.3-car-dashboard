//! Fixed runtime constants and compatibility exports; validated user configuration lives in dashboard-config.
/// Clock-derived PWM divider and legacy slider mapping.
pub mod backlight;
/// Legacy landscape geometry and bounded trend sampling constants.
pub mod layout;
/// Legacy pressure conversion and explicitly synthetic boost animation threshold.
pub mod sensors;
/// Historical Longan UART buffers and unqualified deployment profile.
pub mod vehicle;

/// Compatibility exports for landscape drawing and trend sampling.
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
/// Legacy pressure scaling and demo-only animation trigger.
pub use sensors::{BAR_TO_PSI, BOOST_EASTER_EGG_BAR};
