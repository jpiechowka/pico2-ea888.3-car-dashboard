//! Color constants for the OBD dashboard.
//!
//! # Optimization: Using Built-in `RgbColor` Trait Constants
//!
//! The `embedded_graphics` crate provides pre-defined color constants through the
//! `RgbColor` trait. Using these instead of manually constructing `Rgb565::new(r, g, b)`
//! ensures optimal values and improves code clarity.
//!
//! ## Rgb565 Color Format
//!
//! Rgb565 uses 16 bits per pixel: 5 bits red, 6 bits green, 5 bits blue.
//! - Red: 0-31 (5 bits)
//! - Green: 0-63 (6 bits)
//! - Blue: 0-31 (5 bits)
//!
//! This format is native to many embedded displays (including ST7789) and requires
//! no conversion when writing to the display buffer.

use embedded_graphics::pixelcolor::{Rgb565, RgbColor};

// =============================================================================
// Standard Colors (from RgbColor trait - guaranteed optimal values)
// =============================================================================

/// Pure black (0, 0, 0). Used for backgrounds and dark text.
pub const BLACK: Rgb565 = Rgb565::BLACK;

/// Pure white (31, 63, 31). Used for text on dark backgrounds.
pub const WHITE: Rgb565 = Rgb565::WHITE;

/// Pure red (31, 0, 0). Used for critical alerts (high temp, low voltage).
pub const RED: Rgb565 = Rgb565::RED;

/// Pure green (0, 63, 0). Used for optimal ranges (coolant temp, stoichiometric AFR).
pub const GREEN: Rgb565 = Rgb565::GREEN;

/// Pure blue (0, 0, 31). Used for rich AFR indication.
pub const BLUE: Rgb565 = Rgb565::BLUE;

/// Pure yellow (31, 63, 0). Used for warning states (approaching critical).
pub const YELLOW: Rgb565 = Rgb565::YELLOW;

/// Magenta/Pink (31, 0, 31). Used for easter egg effects and blinking highlights.
pub const PINK: Rgb565 = Rgb565::MAGENTA;

// =============================================================================
// Custom Colors (application-specific)
// =============================================================================

/// Orange warning color. Used for elevated temperatures and lean AFR.
/// RGB565: (31, 32, 0) - slightly darker than yellow.
pub const ORANGE: Rgb565 = Rgb565::new(31, 32, 0);

/// Dark gray for divider lines. Subtle enough to not distract from data.
/// RGB565: (8, 16, 8) - roughly 25% brightness.
pub const GRAY: Rgb565 = Rgb565::new(8, 16, 8);

/// Dark teal for slightly rich AFR indication.
/// RGB565: (0, 20, 10) - blue-green, darker than full cyan.
pub const DARK_TEAL: Rgb565 = Rgb565::new(0, 20, 10);
