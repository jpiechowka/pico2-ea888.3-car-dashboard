//! Pre-computed static text styles to avoid per-frame object construction.
//!
//! # Optimization: Static Style Constants
//!
//! `MonoTextStyle` and `TextStyle` objects were being created every frame in each
//! draw function. While these are small stack allocations, the construction involves
//! copying font references and building style structs repeatedly.
//!
//! By defining these as `const`, the compiler can:
//! 1. Compute the style objects at compile time
//! 2. Store them in the binary's read-only data section
//! 3. Reference them directly without any runtime construction
//!
//! ## Before optimization (in each draw function):
//! ```ignore
//! let centered = TextStyleBuilder::new().alignment(Alignment::Center).build();
//! let label_style = MonoTextStyle::new(&FONT_6X10, WHITE);
//! let value_style = MonoTextStyle::new(&PROFONT_24_POINT, WHITE);
//! ```
//!
//! ## After optimization (const, computed once at compile time):
//! ```ignore
//! pub const CENTERED: TextStyle = TextStyleBuilder::new().alignment(Alignment::Center).build();
//! pub const LABEL_STYLE_WHITE: MonoTextStyle<'static, Rgb565> = MonoTextStyle::new(&FONT_6X10, WHITE);
//! pub const VALUE_STYLE_WHITE: MonoTextStyle<'static, Rgb565> = MonoTextStyle::new(&PROFONT_24_POINT, WHITE);
//! ```
//!
//! # Dynamic Color Styles
//!
//! Some styles need dynamic colors (e.g., blinking effects, temperature-based colors).
//! For these, we expose `LABEL_FONT` so callers can create `MonoTextStyle::new(LABEL_FONT, color)`
//! with minimal overhead - just the color varies, font reference is shared.

use embedded_graphics::{
    mono_font::{
        MonoFont, MonoTextStyle,
        ascii::{FONT_6X10, FONT_10X20},
    },
    pixelcolor::Rgb565,
    text::{Alignment, TextStyle, TextStyleBuilder},
};
use profont::{PROFONT_18_POINT, PROFONT_24_POINT};

use crate::colors::{BLACK, ORANGE, WHITE};

// =============================================================================
// Text Alignment Styles (const - zero runtime cost)
// =============================================================================

/// Centered text alignment. Used for cell labels, values, and popup text.
pub const CENTERED: TextStyle = TextStyleBuilder::new().alignment(Alignment::Center).build();

/// Left-aligned text. Used for console output in loading screen.
pub const LEFT_ALIGNED: TextStyle = TextStyleBuilder::new().alignment(Alignment::Left).build();

/// Right-aligned text. Used for FPS counter in header.
pub const RIGHT_ALIGNED: TextStyle = TextStyleBuilder::new().alignment(Alignment::Right).build();

// =============================================================================
// Font References (for dynamic color styles)
// =============================================================================

/// Small label font (6x10 pixels). Exposed for creating dynamic-color styles.
/// Usage: `MonoTextStyle::new(LABEL_FONT, dynamic_color)`
pub const LABEL_FONT: &MonoFont = &FONT_6X10;

// =============================================================================
// Pre-computed Text Styles (const - zero runtime cost)
// =============================================================================

/// Small white text for labels on dark backgrounds.
pub const LABEL_STYLE_WHITE: MonoTextStyle<'static, Rgb565> = MonoTextStyle::new(&FONT_6X10, WHITE);

/// Small black text for labels on light backgrounds (yellow, orange, green).
pub const LABEL_STYLE_BLACK: MonoTextStyle<'static, Rgb565> = MonoTextStyle::new(&FONT_6X10, BLACK);

/// Small orange text for max value displays.
pub const LABEL_STYLE_ORANGE: MonoTextStyle<'static, Rgb565> = MonoTextStyle::new(&FONT_6X10, ORANGE);

/// Medium white text for header title (10x20 pixels).
pub const TITLE_STYLE_WHITE: MonoTextStyle<'static, Rgb565> = MonoTextStyle::new(&FONT_10X20, WHITE);

/// Large white text for sensor values (`ProFont` 24pt).
pub const VALUE_STYLE_WHITE: MonoTextStyle<'static, Rgb565> = MonoTextStyle::new(&PROFONT_24_POINT, WHITE);

/// Large black text for sensor values on light backgrounds.
pub const VALUE_STYLE_BLACK: MonoTextStyle<'static, Rgb565> = MonoTextStyle::new(&PROFONT_24_POINT, BLACK);

/// Medium value font (`ProFont` 18pt, ~12px wide). Used for cells with longer values (e.g., battery "10.0V").
/// Smaller than `ProFont` 24pt (~14px) but visually similar style.
pub const VALUE_FONT_MEDIUM: &MonoFont = &PROFONT_18_POINT;
