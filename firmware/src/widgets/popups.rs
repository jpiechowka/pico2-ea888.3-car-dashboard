//! Non-modal popup overlays for status messages.
//!
//! Popups appear centered on screen with a white border and red background.
//! Only one popup displays at a time (most recent wins). These are non-modal
//! overlays: button input is still processed while a popup is visible, so
//! users can switch between popups or trigger other actions without waiting.
//!
//! # Optimizations Applied
//!
//! ## 1. Pre-computed Popup Dimensions and Positions
//! All popup sizes, positions, and text coordinates are `const` values computed
//! at compile time. This eliminates per-frame:
//! - Centering calculations: `(SCREEN_WIDTH - popup_width) / 2`
//! - Border offset calculations
//! - Text position arithmetic
//!
//! ## 2. Const `PrimitiveStyle`
//! `PrimitiveStyle::with_fill` is const fn in embedded-graphics 0.8, so fill styles
//! are computed at compile time and stored in the binary's read-only section.
//!
//! ## 3. Static Text Styles
//! Uses `CENTERED` and `TITLE_STYLE_WHITE` from styles module instead of
//! constructing `TextStyleBuilder` and `MonoTextStyle` each frame.
//!
//! ## 4. Uses Global `CENTER_X/CENTER_Y`
//! Text positions reference pre-computed screen center coordinates from config.

use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::{PrimitiveStyle, Rectangle};
use embedded_graphics::text::Text;
use embedded_graphics_simulator::SimulatorDisplay;

use crate::colors::{RED, WHITE};
use crate::config::{CENTER_X, CENTER_Y, SCREEN_HEIGHT, SCREEN_WIDTH};
use crate::styles::{CENTERED, TITLE_STYLE_WHITE};

// =============================================================================
// Reset Popup Layout Constants
// =============================================================================

/// Width of the "MIN/MAX RESET" popup.
const RESET_POPUP_WIDTH: u32 = 180;
/// Height of the "MIN/MAX RESET" popup.
const RESET_POPUP_HEIGHT: u32 = 60;
/// X position (centered on screen).
const RESET_POPUP_X: i32 = (SCREEN_WIDTH - RESET_POPUP_WIDTH) as i32 / 2;
/// Y position (centered on screen).
const RESET_POPUP_Y: i32 = (SCREEN_HEIGHT - RESET_POPUP_HEIGHT) as i32 / 2;

// =============================================================================
// FPS Toggle Popup Layout Constants
// =============================================================================

/// Width of the "FPS ON/OFF" popup (smaller than reset popup).
const FPS_POPUP_WIDTH: u32 = 140;
/// Height of the "FPS ON/OFF" popup.
const FPS_POPUP_HEIGHT: u32 = 50;
/// X position (centered on screen).
const FPS_POPUP_X: i32 = (SCREEN_WIDTH - FPS_POPUP_WIDTH) as i32 / 2;
/// Y position (centered on screen).
const FPS_POPUP_Y: i32 = (SCREEN_HEIGHT - FPS_POPUP_HEIGHT) as i32 / 2;

// =============================================================================
// Pre-computed Text Positions (Optimization)
// =============================================================================

/// Position of "MIN/MAX" text (first line of reset popup).
const RESET_TEXT1_POS: Point = Point::new(CENTER_X, CENTER_Y - 5);
/// Position of "RESET" text (second line of reset popup).
const RESET_TEXT2_POS: Point = Point::new(CENTER_X, CENTER_Y + 15);
/// Position of "FPS ON/OFF" text (single line, vertically centered).
const FPS_TEXT_POS: Point = Point::new(CENTER_X, CENTER_Y + 5);

// =============================================================================
// Pre-computed Primitive Styles (Optimization: const fn in embedded-graphics 0.8)
// =============================================================================

/// White fill style for popup borders.
/// `PrimitiveStyle::with_fill` is const fn, so this is computed at compile time.
const WHITE_FILL: PrimitiveStyle<Rgb565> = PrimitiveStyle::with_fill(WHITE);

/// Red fill style for popup backgrounds.
/// `PrimitiveStyle::with_fill` is const fn, so this is computed at compile time.
const RED_FILL: PrimitiveStyle<Rgb565> = PrimitiveStyle::with_fill(RED);

// =============================================================================
// Pre-computed Rectangle Geometry (Optimization: all constructors are const fn)
// =============================================================================

/// Reset popup border rectangle (outer white rectangle).
const RESET_BORDER_POS: Point = Point::new(RESET_POPUP_X - 3, RESET_POPUP_Y - 3);
const RESET_BORDER_SIZE: Size = Size::new(RESET_POPUP_WIDTH + 6, RESET_POPUP_HEIGHT + 6);

/// Reset popup background rectangle (inner red rectangle).
const RESET_BG_POS: Point = Point::new(RESET_POPUP_X, RESET_POPUP_Y);
const RESET_BG_SIZE: Size = Size::new(RESET_POPUP_WIDTH, RESET_POPUP_HEIGHT);

/// FPS popup border rectangle (outer white rectangle).
const FPS_BORDER_POS: Point = Point::new(FPS_POPUP_X - 3, FPS_POPUP_Y - 3);
const FPS_BORDER_SIZE: Size = Size::new(FPS_POPUP_WIDTH + 6, FPS_POPUP_HEIGHT + 6);

/// FPS popup background rectangle (inner red rectangle).
const FPS_BG_POS: Point = Point::new(FPS_POPUP_X, FPS_POPUP_Y);
const FPS_BG_SIZE: Size = Size::new(FPS_POPUP_WIDTH, FPS_POPUP_HEIGHT);

// =============================================================================
// Drawing Functions
// =============================================================================

/// Draw the "MIN/AVG/MAX RESET" popup.
///
/// Displayed when min/max/avg values and graphs are reset (B button press).
/// Popup has a white 3px border around a red background.
///
/// # Optimizations
/// - Uses pre-computed `RESET_BORDER_POS/SIZE` and `RESET_BG_POS/SIZE` geometry
/// - Uses const `WHITE_FILL` and `RED_FILL` styles
/// - Uses pre-computed `RESET_TEXT1_POS/RESET_TEXT2_POS`
/// - Uses static `TITLE_STYLE_WHITE` and `CENTERED` styles
pub fn draw_reset_popup(display: &mut SimulatorDisplay<Rgb565>) {
    // White border (drawn as larger rectangle behind the main popup)
    Rectangle::new(RESET_BORDER_POS, RESET_BORDER_SIZE)
        .into_styled(WHITE_FILL)
        .draw(display)
        .ok();

    // Red background
    Rectangle::new(RESET_BG_POS, RESET_BG_SIZE)
        .into_styled(RED_FILL)
        .draw(display)
        .ok();

    // Two-line centered text
    Text::with_text_style("MIN/AVG/MAX", RESET_TEXT1_POS, TITLE_STYLE_WHITE, CENTERED)
        .draw(display)
        .ok();
    Text::with_text_style("RESET", RESET_TEXT2_POS, TITLE_STYLE_WHITE, CENTERED)
        .draw(display)
        .ok();
}

/// Draw the "FPS ON/OFF" popup.
///
/// Displayed when FPS counter visibility is toggled (X button press).
/// Smaller than reset popup since it only has one line of text.
///
/// # Optimizations
/// - Uses pre-computed `FPS_BORDER_POS/SIZE` and `FPS_BG_POS/SIZE` geometry
/// - Uses const `WHITE_FILL` and `RED_FILL` styles
/// - Uses pre-computed `FPS_TEXT_POS`
/// - Uses static `TITLE_STYLE_WHITE` and `CENTERED` styles
/// - Static string selection (no heap allocation)
pub fn draw_fps_toggle_popup(
    display: &mut SimulatorDisplay<Rgb565>,
    fps_enabled: bool,
) {
    // White border
    Rectangle::new(FPS_BORDER_POS, FPS_BORDER_SIZE)
        .into_styled(WHITE_FILL)
        .draw(display)
        .ok();

    // Red background
    Rectangle::new(FPS_BG_POS, FPS_BG_SIZE)
        .into_styled(RED_FILL)
        .draw(display)
        .ok();

    // Single line text - static string literal (no allocation)
    let status = if fps_enabled { "FPS ON" } else { "FPS OFF" };
    Text::with_text_style(status, FPS_TEXT_POS, TITLE_STYLE_WHITE, CENTERED)
        .draw(display)
        .ok();
}

/// Draw the "BOOST: BAR/PSI" popup.
///
/// Displayed when boost unit is toggled (A button press).
/// Uses the same geometry as FPS popup (same size, single line).
/// Unit labels are uppercase for consistency with other dashboard text.
///
/// # Optimizations
/// - Reuses FPS popup geometry constants (same size)
/// - Uses const `WHITE_FILL` and `RED_FILL` styles
/// - Static string selection (no heap allocation)
pub fn draw_boost_unit_popup(
    display: &mut SimulatorDisplay<Rgb565>,
    show_psi: bool,
) {
    // White border (reuse FPS popup geometry)
    Rectangle::new(FPS_BORDER_POS, FPS_BORDER_SIZE)
        .into_styled(WHITE_FILL)
        .draw(display)
        .ok();

    // Red background
    Rectangle::new(FPS_BG_POS, FPS_BG_SIZE)
        .into_styled(RED_FILL)
        .draw(display)
        .ok();

    // Single line text - static string literal (no allocation, uppercase for consistency)
    let unit = if show_psi { "BOOST: PSI" } else { "BOOST: BAR" };
    Text::with_text_style(unit, FPS_TEXT_POS, TITLE_STYLE_WHITE, CENTERED)
        .draw(display)
        .ok();
}
