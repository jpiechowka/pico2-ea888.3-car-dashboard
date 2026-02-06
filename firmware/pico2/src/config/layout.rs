//! Display and layout configuration constants.
//!
//! # Optimization: Pre-computed Layout Constants
//!
//! Layout calculations like `SCREEN_WIDTH / 4` are computed at compile time as `const`,
//! avoiding per-frame arithmetic. These constants are used throughout the rendering code
//! instead of recalculating positions every frame.

// =============================================================================
// Display Configuration
// =============================================================================

/// Display width in pixels (ST7789 on Pimoroni PIM715: 320x240)
pub const SCREEN_WIDTH: u32 = 320;

/// Display height in pixels
pub const SCREEN_HEIGHT: u32 = 240;

// =============================================================================
// Sensor State Configuration
// =============================================================================

/// Number of samples to keep in sensor history for trend detection.
/// Larger values = smoother trends but slower response to changes.
pub const HISTORY_SIZE: usize = 50;

/// Minimum difference between recent and older averages to show a trend arrow.
/// Below this threshold, no arrow is displayed (considered stable).
pub const TREND_THRESHOLD: f32 = 0.5;

// =============================================================================
// Pre-computed Layout Constants (Optimization)
// =============================================================================

/// Header bar height in pixels.
pub const HEADER_HEIGHT: u32 = 26;

/// Width of each cell column (screen divided into 4 columns).
/// Pre-computed to avoid division every frame.
pub const COL_WIDTH: u32 = SCREEN_WIDTH / 4;

/// Height of each cell row (remaining height after header, divided into 2 rows).
/// Pre-computed to avoid arithmetic every frame.
pub const ROW_HEIGHT: u32 = (SCREEN_HEIGHT - HEADER_HEIGHT) / 2;

/// Screen center X coordinate. Used for centering popups and text.
/// Pre-computed as i32 to avoid casts in drawing code.
pub const CENTER_X: i32 = (SCREEN_WIDTH / 2) as i32;

/// Screen center Y coordinate. Used for centering popups and text.
/// Pre-computed as i32 to avoid casts in drawing code.
pub const CENTER_Y: i32 = (SCREEN_HEIGHT / 2) as i32;
