//! Header bar and divider line rendering.
//!
//! # Optimizations Applied
//!
//! ## 1. Pre-computed Position Constants
//! All fixed positions (header rectangle, title position, FPS position, divider endpoints)
//! are defined as `const Point` and `const Size`. This eliminates:
//! - Per-frame coordinate calculations
//! - Integer division operations
//! - Type casts from u32 to i32
//!
//! ## 2. Static Text Styles
//! Uses `CENTERED`, `RIGHT_ALIGNED`, `TITLE_STYLE_WHITE`, `LABEL_STYLE_WHITE` from
//! the styles module instead of constructing new style objects each frame.
//!
//! ## 3. Const `PrimitiveStyle`
//! `PrimitiveStyle::with_fill` and `with_stroke` are const fn in embedded-graphics 0.8,
//! so `HEADER_FILL_STYLE` and `DIVIDER_STYLE` are computed at compile time.
//!
//! ## 4. Heapless String for FPS
//! FPS display uses `heapless::String<16>` with `core::fmt::Write` trait instead
//! of `format!()`, avoiding heap allocation.
//!
//! ## 5. Simplified `draw_dividers()` API
//! The function no longer takes layout parameters - it uses pre-computed constants
//! directly, reducing function call overhead and making the API simpler.

use core::fmt::Write;

use embedded_graphics::{
    pixelcolor::Rgb565,
    prelude::*,
    primitives::{Line, PrimitiveStyle, Rectangle},
    text::Text,
};
use embedded_graphics_simulator::SimulatorDisplay;
use heapless::String;

use crate::{
    colors::{GRAY, RED},
    config::{COL_WIDTH, HEADER_HEIGHT, ROW_HEIGHT, SCREEN_HEIGHT, SCREEN_WIDTH},
    styles::{CENTERED, LABEL_STYLE_WHITE, RIGHT_ALIGNED, TITLE_STYLE_WHITE},
};

// =============================================================================
// Header Layout Constants (Optimization: computed at compile time)
// =============================================================================

/// Position of "OBD Sim" title text (centered horizontally).
const HEADER_TITLE_POS: Point = Point::new(160, 19);

/// Position of FPS counter (right-aligned, 5px from edge).
const HEADER_FPS_POS: Point = Point::new((SCREEN_WIDTH - 5) as i32, 17);

/// Top-left corner of header rectangle.
const HEADER_RECT_POS: Point = Point::new(0, 0);

/// Size of header rectangle (full width, 26px tall).
const HEADER_RECT_SIZE: Size = Size::new(SCREEN_WIDTH, 26);

// =============================================================================
// Divider Line Endpoints (Optimization: pre-computed from layout constants)
// =============================================================================
//
// Note: Endpoints use SCREEN_HEIGHT - 1 and SCREEN_WIDTH - 1 because valid
// pixel coordinates are 0..239 and 0..319 (exclusive upper bound).

/// First vertical divider (between column 0 and 1) - start point.
const DIV_V1_START: Point = Point::new(COL_WIDTH as i32, HEADER_HEIGHT as i32);
/// First vertical divider - end point (y = 239, not 240).
const DIV_V1_END: Point = Point::new(COL_WIDTH as i32, (SCREEN_HEIGHT - 1) as i32);

/// Second vertical divider (between column 1 and 2) - start point.
const DIV_V2_START: Point = Point::new((COL_WIDTH * 2) as i32, HEADER_HEIGHT as i32);
/// Second vertical divider - end point (y = 239, not 240).
const DIV_V2_END: Point = Point::new((COL_WIDTH * 2) as i32, (SCREEN_HEIGHT - 1) as i32);

/// Third vertical divider (between column 2 and 3) - start point.
const DIV_V3_START: Point = Point::new((COL_WIDTH * 3) as i32, HEADER_HEIGHT as i32);
/// Third vertical divider - end point (y = 239, not 240).
const DIV_V3_END: Point = Point::new((COL_WIDTH * 3) as i32, (SCREEN_HEIGHT - 1) as i32);

/// Horizontal divider (between row 0 and 1) - start point.
const DIV_H_START: Point = Point::new(0, (HEADER_HEIGHT + ROW_HEIGHT) as i32);
/// Horizontal divider - end point (x = 319, not 320).
const DIV_H_END: Point = Point::new((SCREEN_WIDTH - 1) as i32, (HEADER_HEIGHT + ROW_HEIGHT) as i32);

// =============================================================================
// Pre-computed Primitive Styles (Optimization: const fn in embedded-graphics 0.8)
// =============================================================================

/// Gray stroke style for divider lines (1px wide).
/// `PrimitiveStyle::with_stroke` is const fn, so this is computed at compile time.
const DIVIDER_STYLE: PrimitiveStyle<Rgb565> = PrimitiveStyle::with_stroke(GRAY, 1);

/// Red fill style for header background.
/// `PrimitiveStyle::with_fill` is const fn, so this is computed at compile time.
const HEADER_FILL_STYLE: PrimitiveStyle<Rgb565> = PrimitiveStyle::with_fill(RED);

// =============================================================================
// Drawing Functions
// =============================================================================

/// Draw the header bar with title and optional FPS counter.
///
/// The header is a red rectangle spanning the full width of the display,
/// with "OBD Sim" centered and an optional FPS counter on the right.
///
/// # Optimizations
/// - Uses pre-computed `HEADER_RECT_POS` and `HEADER_RECT_SIZE` constants
/// - Uses static `HEADER_FILL_STYLE` (const `PrimitiveStyle`)
/// - Uses static `TITLE_STYLE_WHITE` and `CENTERED` styles
/// - FPS string uses `heapless::String` (no heap allocation)
pub fn draw_header(display: &mut SimulatorDisplay<Rgb565>, show_fps: bool, fps: f32) {
    // Draw red header background using const style
    Rectangle::new(HEADER_RECT_POS, HEADER_RECT_SIZE)
        .into_styled(HEADER_FILL_STYLE)
        .draw(display)
        .ok();

    // Draw centered title using static style
    Text::with_text_style("OBD Sim", HEADER_TITLE_POS, TITLE_STYLE_WHITE, CENTERED)
        .draw(display)
        .ok();

    // Optional FPS display on the right side
    if show_fps {
        // Optimization: heapless::String avoids format! heap allocation
        let mut fps_str: String<16> = String::new();
        let _ = write!(fps_str, "{fps:.0} FPS");
        Text::with_text_style(&fps_str, HEADER_FPS_POS, LABEL_STYLE_WHITE, RIGHT_ALIGNED)
            .draw(display)
            .ok();
    }
}

/// Draw grid divider lines between cells.
///
/// Draws three vertical lines (separating 4 columns) and one horizontal line
/// (separating 2 rows). Lines are gray (GRAY color constant) and 1px wide.
///
/// # Optimizations
/// - Uses pre-computed line endpoint constants (`DIV_V1_START`, etc.)
/// - Uses const `DIVIDER_STYLE` (`PrimitiveStyle::with_stroke` is const fn)
/// - No parameters needed - layout is fixed and known at compile time
pub fn draw_dividers(display: &mut SimulatorDisplay<Rgb565>) {
    // Vertical divider between columns 0 and 1
    Line::new(DIV_V1_START, DIV_V1_END)
        .into_styled(DIVIDER_STYLE)
        .draw(display)
        .ok();

    // Vertical divider between columns 1 and 2
    Line::new(DIV_V2_START, DIV_V2_END)
        .into_styled(DIVIDER_STYLE)
        .draw(display)
        .ok();

    // Vertical divider between columns 2 and 3
    Line::new(DIV_V3_START, DIV_V3_END)
        .into_styled(DIVIDER_STYLE)
        .draw(display)
        .ok();

    // Horizontal divider between rows 0 and 1
    Line::new(DIV_H_START, DIV_H_END)
        .into_styled(DIVIDER_STYLE)
        .draw(display)
        .ok();
}
