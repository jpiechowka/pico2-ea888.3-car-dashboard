//! Sensor display cells for the OBD dashboard grid.
//!
//! Each cell displays a single sensor reading with:
//! - Label at the top (e.g., "OIL", "IAT", "EGT", "COOL", "BATT")
//! - Main value in the center (large font)
//! - Secondary info at the bottom (max values, status indicators, mini-graphs)
//!
//! # Cell Types
//!
//! | Cell | Sensor | Secondary Info |
//! |------|--------|----------------|
//! | `draw_boost_cell` | Turbo boost pressure | MAX boost, BAR/PSI toggle, easter egg |
//! | `draw_temp_cell` | Temperature (generic) | Mini-graph, AVG, MAX temp, trend arrow |
//! | `draw_batt_cell` | Battery voltage | Mini-graph, MIN/MAX values, trend arrow |
//! | `draw_afr_cell` | Air-Fuel Ratio | Mini-graph, Lambda conversion, status indicator |
//!
//! # Optimizations Applied
//!
//! ## Background Redraw
//! All cell drawing functions always redraw their background every frame. This is
//! necessary because sensor values animate continuously - without clearing the
//! background, old text would remain visible causing visual artifacts.
//!
//! ## Heapless String Formatting
//! All cells use `heapless::String<16>` or `<20>` with `core::fmt::Write` trait
//! for value formatting. This avoids heap allocation that `format!()` would require.
//!
//! ```ignore
//! // Before (requires allocator):
//! let value_str = format!("{:.1}V", voltage);
//!
//! // After (stack-allocated, no_std compatible):
//! let mut value_str: String<16> = String::new();
//! let _ = write!(value_str, "{:.1}V", voltage);
//! ```
//!
//! ## Static Style Constants
//! Cells use pre-computed styles from [`crate::styles`]:
//! - `CENTERED` - text alignment (computed at compile time)
//! - `LABEL_STYLE_WHITE`, `LABEL_STYLE_BLACK` - small font styles
//! - `VALUE_STYLE_WHITE`, `VALUE_STYLE_BLACK` - large `ProFont` 24pt styles
//! - `LABEL_FONT` - font reference for creating dynamic-color styles
//!
//! ## Style Selection Functions
//! Helper functions (`label_style_for_bg`, `value_style_for_color`) return
//! references to static styles when possible, only constructing new styles
//! when a custom color is needed.
//!
//! ## Shake Animation Support
//! Cells with critical states (oil/coolant/DSG ≥ 110°C, battery < 12V) accept
//! a `shake_offset` parameter that shifts all text horizontally. This creates
//! an attention-grabbing wiggle effect. See [`crate::animations`] for the
//! offset calculation using sine wave oscillation.
//!
//! # Color Coding
//!
//! ## Text Color Rules (consistent across all cells)
//! Text color is determined by perceptual luminance (ITU-R BT.601 formula) of the
//! background, ensuring correct contrast during smooth color transitions.
//! - **Dark backgrounds** (black, red, blue, dark teal) → **WHITE** text
//! - **Light backgrounds** (yellow, orange, green) → **BLACK** text
//!
//! ## Peak Highlight
//! When a new peak value is detected, the value text is highlighted:
//! - **YELLOW** on dark backgrounds (high visibility)
//! - **BLACK with WHITE shadow** on light backgrounds (drop shadow for visibility)
//!
//! ## Background Colors by Sensor
//!
//! **Oil/DSG Temperature:**
//! - Black (normal, <90°C) → Yellow (90°C) → Orange (100°C) → Red (110°C+)
//!
//! **Coolant Temperature:**
//! - Orange (cold, <75°C) → Green (optimal, 75-90°C) → Red (>90°C)
//!
//! **Battery Voltage:**
//! - Black (normal, ≥12.5V) → Orange (warning, 12.0-12.5V) → Red (critical, <12.0V)
//!
//! **AFR (Air-Fuel Ratio):** (tuned for turbocharged engines)
//! - Blue (RICH AF, <12.0) → Dark Teal (RICH, 12.0-14.0)
//! - Green (OPTIMAL, 14.0-14.9) → Orange (LEAN, 14.9-15.5)
//! - Red + blink + shake (LEAN AF, >15.5) - critical!

use core::fmt::Write;

use embedded_graphics::{
    mono_font::MonoTextStyle,
    pixelcolor::Rgb565,
    prelude::*,
    primitives::{PrimitiveStyle, Rectangle},
    text::Text,
};
use embedded_graphics_simulator::SimulatorDisplay;
use heapless::String;
use profont::PROFONT_24_POINT;

use crate::{
    colors::{BLACK, BLUE, DARK_TEAL, GREEN, ORANGE, PINK, RED, WHITE, YELLOW},
    state::SensorState,
    styles::{
        CENTERED, LABEL_FONT, LABEL_STYLE_BLACK, LABEL_STYLE_ORANGE, LABEL_STYLE_WHITE, VALUE_FONT_MEDIUM,
        VALUE_STYLE_BLACK, VALUE_STYLE_WHITE,
    },
    thresholds::{
        AFR_LEAN_CRITICAL, AFR_OPTIMAL_MAX, AFR_RICH, AFR_RICH_AF, AFR_STOICH, BAR_TO_PSI, BATT_CRITICAL, BATT_WARNING,
        COOLANT_COLD_MAX, COOLANT_CRITICAL, EGT_COLD_MAX, EGT_CRITICAL, EGT_HIGH_LOAD, EGT_SPIRITED, IAT_COLD,
        IAT_CRITICAL, IAT_EXTREME_COLD, IAT_HOT, IAT_WARM, OIL_DSG_CRITICAL, OIL_DSG_ELEVATED, OIL_DSG_HIGH,
    },
    widgets::primitives::{draw_cell_background, draw_mini_graph, draw_trend_arrow},
};

// =============================================================================
// Color Helper Functions
// =============================================================================
//
// Color is determined by perceptual luminance (ITU-R BT.601) to ensure correct
// contrast during smooth color transitions. Uses integer approximation:
//   luma = (77*R + 150*G + 29*B) >> 8  (where R,G,B are 8-bit values)
//
// Color Consistency Rules:
// - Dark backgrounds (luma < 128) → WHITE text
// - Light backgrounds (luma >= 128) → BLACK text
// - Peak highlight: YELLOW on dark, BLACK with shadow on light
// - Secondary info (max values, min/max ranges):
//   - BLACK on light backgrounds
//   - ORANGE on black backgrounds (for accent)
//   - WHITE on red critical backgrounds
//   - RED to highlight critical max values
// =============================================================================

/// Determine appropriate label text color for a given background.
///
/// Uses perceptual luminance (ITU-R BT.601) to work correctly with any
/// background color, including mid-transition colors from `ColorTransition`.
/// Returns WHITE for dark backgrounds and BLACK for light backgrounds.
///
/// Human eyes perceive green as brighter than red/blue, so we weight
/// the channels accordingly: R*0.299 + G*0.587 + B*0.114
pub fn label_color_for_bg(bg_color: Rgb565) -> Rgb565 {
    let luma = calculate_luminance(bg_color);
    if luma < 128 { WHITE } else { BLACK }
}

/// Return high-contrast highlight color for peak value display.
///
/// Uses perceptual luminance (ITU-R BT.601) to select a readable highlight:
/// - YELLOW on dark backgrounds (high visibility)
/// - BLACK on light backgrounds (maintains readability on yellow/orange/green)
///
/// Note: Internal code uses `peak_highlight_for_text()` which takes pre-computed
/// `base_text` color to avoid recomputing luminance. This function is kept for
/// API compatibility with tests.
#[cfg(test)]
fn peak_highlight_color(bg_color: Rgb565) -> Rgb565 {
    peak_highlight_for_text(label_color_for_bg(bg_color))
}

/// Return high-contrast highlight color for peak value display (optimized).
///
/// **Optimization**: Takes pre-computed `base_text` color instead of
/// recomputing luminance.
/// - YELLOW on dark backgrounds (`base_text` == WHITE)
/// - BLACK on light backgrounds (`base_text` == BLACK)
#[inline]
fn peak_highlight_for_text(base_text: Rgb565) -> Rgb565 {
    if base_text == WHITE { YELLOW } else { BLACK }
}

/// Calculate perceptual luminance from an Rgb565 color.
///
/// Uses ITU-R BT.601 weights: 0.299*R + 0.587*G + 0.114*B
/// Integer approximation: (77*R + 150*G + 29*B) >> 8
///
/// # RGB565 to 8-bit Expansion
///
/// For more accurate expansion from 5/6-bit to 8-bit, we use the formula:
/// - 5-bit: `(val << 3) | (val >> 2)` (maps 0-31 to 0-255)
/// - 6-bit: `(val << 2) | (val >> 4)` (maps 0-63 to 0-255)
///
/// This replicates the high bits into the low bits, reducing quantization
/// error at threshold boundaries compared to simple shift.
#[inline]
fn calculate_luminance(color: Rgb565) -> u32 {
    let raw = color.into_storage();
    let r5 = u32::from((raw >> 11) & 0x1F);
    let g6 = u32::from((raw >> 5) & 0x3F);
    let b5 = u32::from(raw & 0x1F);

    // Accurate 5/6-bit to 8-bit expansion
    let r8 = (r5 << 3) | (r5 >> 2);
    let g8 = (g6 << 2) | (g6 >> 4);
    let b8 = (b5 << 3) | (b5 >> 2);

    // ITU-R BT.601 luminance
    (r8 * 77 + g8 * 150 + b8 * 29) >> 8
}

/// Get background and text colors for oil/DSG temperature.
///
/// Oil and DSG (transmission) have similar operating ranges:
/// - Normal operation: below `OIL_DSG_ELEVATED` (black background)
/// - Elevated: `OIL_DSG_ELEVATED` to `OIL_DSG_HIGH` (yellow warning)
/// - High: `OIL_DSG_HIGH` to `OIL_DSG_CRITICAL` (orange warning)
/// - Critical: >= `OIL_DSG_CRITICAL` (red alert, will blink)
///
/// Returns `(background_color, text_color)` tuple.
pub fn temp_color_oil_dsg(temp: f32) -> (Rgb565, Rgb565) {
    if temp >= OIL_DSG_CRITICAL {
        (RED, WHITE)
    } else if temp >= OIL_DSG_HIGH {
        (ORANGE, BLACK)
    } else if temp >= OIL_DSG_ELEVATED {
        (YELLOW, BLACK)
    } else {
        (BLACK, WHITE)
    }
}

/// Get background and text colors for coolant temperature.
///
/// Coolant has a narrower optimal range than oil:
/// - Cold: below `COOLANT_COLD_MAX` (orange, engine not at operating temp)
/// - Optimal: `COOLANT_COLD_MAX` to `COOLANT_CRITICAL` (green, normal operation)
/// - Overheating: above `COOLANT_CRITICAL` (red alert, will blink)
///
/// Returns `(background_color, text_color)` tuple.
pub fn temp_color_water(temp: f32) -> (Rgb565, Rgb565) {
    if temp > COOLANT_CRITICAL {
        (RED, WHITE)
    } else if temp >= COOLANT_COLD_MAX {
        (GREEN, BLACK)
    } else {
        (ORANGE, BLACK)
    }
}

/// Check if oil/DSG temperature is critical (triggers blinking).
pub fn is_critical_oil_dsg(temp: f32) -> bool {
    temp >= OIL_DSG_CRITICAL
}

/// Check if coolant temperature is critical (triggers blinking).
pub fn is_critical_water(temp: f32) -> bool {
    temp > COOLANT_CRITICAL
}

/// Check if AFR is critical (LEAN AF triggers blinking and shake).
pub fn is_critical_afr(afr: f32) -> bool {
    afr > AFR_LEAN_CRITICAL
}

/// Get background and text colors for Intake Air Temperature (IAT).
///
/// IAT affects air density and engine performance:
/// - Very cold: below `IAT_COLD` (blue, ice/icing risk in intake)
/// - Cold/Optimal: `IAT_COLD` to `IAT_WARM` (green, cool dense air is good for power)
/// - Warm: `IAT_WARM` to `IAT_HOT` (yellow, reduced power potential)
/// - Hot: `IAT_HOT` to `IAT_CRITICAL` (orange, possible intercooler issue)
/// - Critical: above `IAT_CRITICAL` (red, heat soak)
///
/// Returns `(background_color, text_color)` tuple.
pub fn temp_color_iat(temp: f32) -> (Rgb565, Rgb565) {
    if temp >= IAT_CRITICAL {
        (RED, WHITE)
    } else if temp >= IAT_HOT {
        (ORANGE, BLACK)
    } else if temp >= IAT_WARM {
        (YELLOW, BLACK)
    } else if temp >= IAT_COLD {
        (GREEN, BLACK)
    } else {
        (BLUE, WHITE) // Below IAT_COLD - icing risk
    }
}

/// Check if IAT is critical (triggers blinking).
///
/// Critical when extremely hot (>= `IAT_CRITICAL`, heat soak) or extremely cold (<= `IAT_EXTREME_COLD`, icing risk).
pub fn is_critical_iat(temp: f32) -> bool {
    temp >= IAT_CRITICAL || temp <= IAT_EXTREME_COLD
}

/// Get background and text colors for Exhaust Gas Temperature (EGT).
///
/// EGT indicates combustion conditions (pre-cat sensor):
/// - Cold: below `EGT_COLD_MAX` (blue, engine warming up)
/// - Normal cruise: `EGT_COLD_MAX` to `EGT_SPIRITED` (green, typical driving)
/// - Spirited: `EGT_SPIRITED` to `EGT_HIGH_LOAD` (yellow, hard driving)
/// - Hard acceleration: `EGT_HIGH_LOAD` to `EGT_CRITICAL` (orange, high load)
/// - Critical: above `EGT_CRITICAL` (red, lean condition/detonation risk)
///
/// Returns `(background_color, text_color)` tuple.
pub fn temp_color_egt(temp: f32) -> (Rgb565, Rgb565) {
    if temp >= EGT_CRITICAL {
        (RED, WHITE)
    } else if temp >= EGT_HIGH_LOAD {
        (ORANGE, BLACK)
    } else if temp >= EGT_SPIRITED {
        (YELLOW, BLACK)
    } else if temp >= EGT_COLD_MAX {
        (GREEN, BLACK)
    } else {
        (BLUE, WHITE) // Cold/warming up
    }
}

/// Check if EGT is critical (triggers blinking).
///
/// Critical when exhaust temp indicates lean condition or detonation risk.
pub fn is_critical_egt(temp: f32) -> bool {
    temp >= EGT_CRITICAL
}

// =============================================================================
// Style Selection Functions (Optimization: prefer static styles)
// =============================================================================

/// Select label style based on pre-computed base text color.
///
/// **Optimization**: Takes the already-computed `base_text` color instead of
/// recomputing luminance. Returns reference to static `LABEL_STYLE_WHITE` or
/// `LABEL_STYLE_BLACK` constants instead of constructing new styles.
#[inline]
fn label_style_for_text(base_text: Rgb565) -> MonoTextStyle<'static, Rgb565> {
    if base_text == WHITE {
        LABEL_STYLE_WHITE
    } else {
        LABEL_STYLE_BLACK
    }
}

/// Select value style for a given color.
///
/// **Optimization**: Returns reference to static `VALUE_STYLE_WHITE` or
/// `VALUE_STYLE_BLACK` when possible. Only constructs a new style for
/// custom colors (e.g., yellow for peak highlight, pink for easter egg).
fn value_style_for_color(color: Rgb565) -> MonoTextStyle<'static, Rgb565> {
    if color == WHITE {
        VALUE_STYLE_WHITE
    } else if color == BLACK {
        VALUE_STYLE_BLACK
    } else {
        // Custom color requires constructing a new style
        MonoTextStyle::new(&PROFONT_24_POINT, color)
    }
}

// =============================================================================
// Cell Drawing Functions
// =============================================================================

/// Draw the boost pressure cell with configurable unit display.
///
/// Displays turbo boost pressure in either bar or PSI, with conversion shown below.
/// Features an easter egg that triggers when boost hits ~2.0 bar (1.95 bar / 29 PSI).
///
/// # Layout (Bar Mode - Default)
/// ```text
/// ┌─────────────┐
/// │  BOOST REL  │  Label (top)
/// │    1.85     │  Value in bar (center, large font)
/// │     bar     │  Unit label
/// │  26.8 PSI   │  PSI conversion (small)
/// │  max 1.85   │  Peak value in bar OR easter egg
/// └─────────────┘
/// ```
///
/// # Layout (PSI Mode)
/// ```text
/// ┌─────────────┐
/// │  BOOST REL  │  Label (top)
/// │    26.8     │  Value in PSI (center, large font)
/// │     PSI     │  Unit label
/// │  1.85 bar   │  Bar conversion (small)
/// │  max 29.0   │  Peak value in PSI OR easter egg
/// └─────────────┘
/// ```
///
/// # Parameters
/// - `boost_bar`: Current boost in bar (always stored in bar internally)
/// - `max_boost`: Max boost in the current display unit (bar or PSI)
/// - `show_psi`: If true, display PSI as primary unit; if false, display bar
/// - `show_easter_egg`: If true, show "Fast AF Boi!" and blink value
/// - `blink_on`: Blink state for critical/easter egg animations
/// - `shake_offset`: Horizontal shake for critical state animation
///
/// # Easter Egg
/// Triggers at ~2.0 bar (1.95 bar / 29 PSI). Shows "Fast AF Boi!" and blinks value pink/white.
///
/// # Optimization
/// Uses `heapless::String<16>` for formatting values without heap allocation.
#[allow(clippy::too_many_arguments)]
pub fn draw_boost_cell(
    display: &mut SimulatorDisplay<Rgb565>,
    x: u32,
    y: u32,
    w: u32,
    h: u32,
    boost_bar: f32,
    max_boost: f32,
    show_psi: bool,
    show_easter_egg: bool,
    blink_on: bool,
    shake_offset: i32,
) {
    // Always redraw background - values change every frame and would leave artifacts
    draw_cell_background(display, x, y, w, h, BLACK);

    // Center positions - shake only applied to main value to prevent overflow
    let center_x = (x + w / 2) as i32;
    let center_y = (y + h / 2) as i32;
    let value_x = center_x + shake_offset; // Only value shakes

    // Label at top (no shake - prevents overflow on narrow 80px cells)
    Text::with_text_style(
        "BOOST REL",
        Point::new(center_x, y as i32 + 14),
        LABEL_STYLE_WHITE,
        CENTERED,
    )
    .draw(display)
    .ok();

    // Calculate both values
    let boost_psi = boost_bar * BAR_TO_PSI;

    // Main value - display in current unit mode, shakes when easter egg
    // Blinks white/pink when easter egg active
    let mut value_str: String<16> = String::new();
    if show_psi {
        let _ = write!(value_str, "{boost_psi:.1}");
    } else {
        let _ = write!(value_str, "{boost_bar:.2}");
    }
    let value_color = if show_easter_egg {
        if blink_on { PINK } else { WHITE }
    } else {
        WHITE
    };
    let value_style = value_style_for_color(value_color);
    Text::with_text_style(&value_str, Point::new(value_x, center_y - 8), value_style, CENTERED)
        .draw(display)
        .ok();

    // Unit label below the value (uppercase for consistency)
    let unit_label = if show_psi { "PSI" } else { "BAR" };
    Text::with_text_style(
        unit_label,
        Point::new(center_x, center_y + 10),
        LABEL_STYLE_WHITE,
        CENTERED,
    )
    .draw(display)
    .ok();

    // Conversion to other unit below unit label (uppercase units)
    let mut conv_str: String<16> = String::new();
    if show_psi {
        let _ = write!(conv_str, "{boost_bar:.2} BAR");
    } else {
        let _ = write!(conv_str, "{boost_psi:.1} PSI");
    }
    Text::with_text_style(
        &conv_str,
        Point::new(center_x, center_y + 22),
        LABEL_STYLE_WHITE,
        CENTERED,
    )
    .draw(display)
    .ok();

    // Easter egg message OR peak boost at bottom
    if show_easter_egg {
        let easter_color = if blink_on { WHITE } else { PINK };
        let easter_style = MonoTextStyle::new(LABEL_FONT, easter_color);
        Text::with_text_style(
            "Fast AF Boi!",
            Point::new(center_x, (y + h) as i32 - 8),
            easter_style,
            CENTERED,
        )
        .draw(display)
        .ok();
    } else {
        // Peak boost at bottom in current unit (uppercase MAX, always orange)
        let mut max_str: String<16> = String::new();
        if show_psi {
            let _ = write!(max_str, "MAX {max_boost:.1}");
        } else {
            let _ = write!(max_str, "MAX {max_boost:.2}");
        }
        Text::with_text_style(
            &max_str,
            Point::new(center_x, (y + h) as i32 - 8),
            LABEL_STYLE_ORANGE,
            CENTERED,
        )
        .draw(display)
        .ok();
    }
}

/// Draw a generic temperature cell (oil, coolant, or DSG).
///
/// This is a generic function that works for any temperature sensor.
/// The color behavior is controlled by the `color_fn` and `critical_fn`
/// parameters, allowing different thresholds for different sensors.
///
/// # Layout
/// ```text
/// ┌─────────────┐
/// │   OIL ↑     │  Label + trend arrow (top)
/// │    95C      │  Value (center, large font)
/// │ [~~graph~~] │  Mini sparkline (2 min history)
/// │  AVG 92C    │  5-minute rolling average
/// │  MAX 105C   │  Peak value (bottom)
/// └─────────────┘
/// ```
///
/// # Features
/// - **Dynamic background**: Changes color based on temperature ranges
/// - **Trend arrow**: Shows rising/falling based on sensor history
/// - **Peak highlight**: Value color changes briefly when new max is reached (YELLOW on dark backgrounds, BLACK on
///   light backgrounds for readability)
/// - **Critical blink**: Background blinks at ~4Hz when in critical range
/// - **Mini-graph**: 2-minute sparkline with auto-scaling Y-axis
/// - **Rolling average**: 5-minute average displayed below the graph
/// - **Smooth transitions**: Optional `bg_override` enables smooth color fades
///
/// # Shake Animation
/// Pass `shake_offset` from [`crate::animations::calculate_shake_offset`] to add
/// horizontal wiggle when in critical state. Pass 0 for no shake.
///
/// # Type Parameters
/// - `F`: Color function `Fn(f32) -> (Rgb565, Rgb565)` returns (bg, text) colors
/// - `C`: Critical check `Fn(f32) -> bool` returns true if value should blink
///
/// # Optimization
/// Uses `heapless::String<16>` for value formatting, static styles where possible.
/// Background is always redrawn since values animate continuously.
/// Returns the actual background color used (accounting for blink state).
#[allow(clippy::too_many_arguments)]
pub fn draw_temp_cell<F, C>(
    display: &mut SimulatorDisplay<Rgb565>,
    x: u32,
    y: u32,
    w: u32,
    h: u32,
    label: &str,
    temp: f32,
    max_temp: f32,
    state: &SensorState,
    color_fn: F,
    critical_fn: C,
    blink_on: bool,
    shake_offset: i32,
    bg_override: Option<Rgb565>,
) -> Rgb565
where
    F: Fn(f32) -> (Rgb565, Rgb565),
    C: Fn(f32) -> bool,
{
    let (mut bg_color, _discrete_text_color) = color_fn(temp);
    let is_critical = critical_fn(temp);
    let max_is_critical = critical_fn(max_temp);

    // Apply transition override if provided (smooth color transitions)
    if let Some(override_color) = bg_override {
        bg_color = override_color;
    }

    // Blink effect: alternate between colored and black background
    if is_critical && !blink_on {
        bg_color = BLACK;
    }

    // Always redraw background - values change every frame and would leave artifacts
    draw_cell_background(display, x, y, w, h, bg_color);

    // Derive ALL colors from final bg_color (after override + blink)
    // Optimization: compute base_text once, derive all other colors from it
    let base_text = label_color_for_bg(bg_color);
    let label_style = label_style_for_text(base_text);
    let peak_color = peak_highlight_for_text(base_text);

    // Center positions - shake only applied to main value to prevent overflow
    let center_x = (x + w / 2) as i32;
    let center_y = (y + h / 2) as i32;
    let value_x = center_x + shake_offset; // Only value shakes

    // Label at top (no shake - prevents overflow on narrow 80px cells)
    Text::with_text_style(label, Point::new(center_x, y as i32 + 14), label_style, CENTERED)
        .draw(display)
        .ok();

    // Trend arrow next to label (no shake)
    if let Some(rising) = state.get_trend() {
        let arrow_x = center_x + (label.len() as i32 * 3) + 8;
        draw_trend_arrow(display, arrow_x, y as i32 + 10, rising, base_text);
    }

    // Main value - highlighted when new peak detected, shakes when critical
    // Optimization: heapless::String avoids heap allocation
    let mut value_str: String<16> = String::new();
    let _ = write!(value_str, "{temp:.0}C");
    let value_color = if state.is_new_peak { peak_color } else { base_text };
    // Peak shadow effect: on light backgrounds, BLACK peak text isn't visually distinct
    // from normal BLACK text. Add WHITE drop shadow to make peaks noticeable.
    if state.is_new_peak && value_color == BLACK {
        let shadow_style = value_style_for_color(WHITE);
        Text::with_text_style(
            &value_str,
            Point::new(value_x + 1, center_y - 11),
            shadow_style,
            CENTERED,
        )
        .draw(display)
        .ok();
    }
    let value_style = value_style_for_color(value_color);
    Text::with_text_style(&value_str, Point::new(value_x, center_y - 12), value_style, CENTERED)
        .draw(display)
        .ok();

    // Mini-graph showing temperature history (2 minutes, auto-scaled)
    // Graph area: below value, above AVG/MAX labels (no shake)
    let graph_y = center_y + 4;
    let graph_h = 20u32; // Height of graph area (reduced to fit AVG line)
    let graph_w = w - 16; // Width with padding
    let graph_x = x as i32 + 8;

    // Graph line color: always contrast with current background
    let graph_line_color = base_text;
    let (buffer, start_idx, count, data_min, data_max) = state.get_graph_data();
    draw_mini_graph(
        display,
        graph_x,
        graph_y,
        graph_w,
        graph_h,
        buffer,
        start_idx,
        count,
        data_min,
        data_max,
        |_| graph_line_color,
    );

    // AVG color: readable on any background
    // On light backgrounds: BLACK for readability
    // On dark backgrounds: ORANGE for accent, WHITE on red critical bg
    let avg_color = if base_text == BLACK {
        BLACK
    } else if is_critical {
        WHITE // On red critical background
    } else {
        ORANGE // Accent on black background
    };
    let avg_style = MonoTextStyle::new(LABEL_FONT, avg_color);

    // MAX color logic:
    // - When max_is_critical && !is_critical: We need to warn "you overheated earlier" but current temp is normal. Use
    //   RED text on BLACK badge for maximum visibility.
    // - When max_is_critical && is_critical: We're currently critical, RED on RED is invisible. Use WHITE text instead
    //   (matches the current critical state styling).
    // - Otherwise: Use standard styling (BLACK on light bg, ORANGE/WHITE on dark bg).
    let max_warning = max_is_critical && !is_critical;
    let max_color = if max_is_critical {
        if is_critical {
            WHITE // Currently critical: RED bg, use WHITE text
        } else {
            RED // Warning badge: RED text on BLACK badge
        }
    } else if base_text == BLACK {
        BLACK // Light background, non-critical
    } else if is_critical {
        WHITE // On red critical background, non-critical max
    } else {
        ORANGE // Accent on black background
    };
    let max_style = MonoTextStyle::new(LABEL_FONT, max_color);

    // 5-minute rolling average (compact format for 80px cells)
    if let Some(avg) = state.get_average() {
        let mut avg_str: String<16> = String::new();
        let _ = write!(avg_str, "AVG {avg:.0}C");
        Text::with_text_style(&avg_str, Point::new(center_x, (y + h) as i32 - 22), avg_style, CENTERED)
            .draw(display)
            .ok();
    }

    // Peak temperature at bottom (compact format for 80px cells)
    let mut max_str: String<16> = String::new();
    let _ = write!(max_str, "MAX {max_temp:.0}C");
    let max_pos = Point::new(center_x, (y + h) as i32 - 6);
    let max_text = Text::with_text_style(&max_str, max_pos, max_style, CENTERED);

    // Draw black badge behind MAX text when showing critical max warning
    // This ensures RED text is always readable regardless of cell background color
    // Badge is clamped to cell boundaries to prevent overflow
    if max_warning {
        let bb = max_text.bounding_box();
        let pad: i32 = 2;
        // Cell boundaries (with 2px inset from draw_cell_background)
        let cell_left = x as i32 + 2;
        let cell_right = (x + w) as i32 - 2;
        // Calculate badge position, clamped to cell boundaries
        let badge_left = (bb.top_left.x - pad).max(cell_left);
        let badge_right = (bb.top_left.x + bb.size.width as i32 + pad).min(cell_right);
        let badge_pos = Point::new(badge_left, bb.top_left.y - pad);
        let badge_width = (badge_right - badge_left).max(0) as u32;
        let badge_size = Size::new(badge_width, bb.size.height + (pad as u32 * 2));
        Rectangle::new(badge_pos, badge_size)
            .into_styled(PrimitiveStyle::with_fill(BLACK))
            .draw(display)
            .ok();
    }

    max_text.draw(display).ok();

    // Return actual background color for dirty rectangle tracking
    bg_color
}

/// Draw the battery voltage cell with mini-graph.
///
/// Shows battery voltage with mini-graph and MIN/MAX values.
/// Critical when voltage drops below 12.0V (alternator failure, battery drain).
///
/// # Layout
/// ```text
/// ┌─────────────┐
/// │   BATT ↓    │  Label + trend arrow (top)
/// │   13.8V     │  Value (center, medium font)
/// │ [~~graph~~] │  Mini sparkline (2 min history)
/// │ MIN 12.1V   │  Session minimum
/// │ MAX 14.2V   │  Session maximum
/// └─────────────┘
/// ```
///
/// # Color States
/// - **Black**: Normal (≥12.5V)
/// - **Orange**: Warning (12.0-12.5V)
/// - **Red (blinking at ~4Hz)**: Critical (<12.0V)
///
/// # Features
/// - **Peak highlight**: Value color changes when new MIN or MAX is detected (YELLOW on dark backgrounds, BLACK on
///   light backgrounds for readability)
/// - **Smooth transitions**: Optional `bg_override` enables smooth color fades
///
/// # Shake Animation
/// Pass `shake_offset` from [`crate::animations::calculate_shake_offset`] to add
/// horizontal wiggle when in critical state. Pass 0 for no shake.
///
/// # Optimization
/// Uses `heapless::String<16>` for value formatting, static styles where possible.
/// Background is always redrawn since values animate continuously.
/// Returns the actual background color used (accounting for blink state).
#[allow(clippy::too_many_arguments)]
pub fn draw_batt_cell(
    display: &mut SimulatorDisplay<Rgb565>,
    x: u32,
    y: u32,
    w: u32,
    h: u32,
    voltage: f32,
    min_voltage: f32,
    max_voltage: f32,
    state: &SensorState,
    blink_on: bool,
    shake_offset: i32,
    bg_override: Option<Rgb565>,
) -> Rgb565 {
    // Determine colors based on voltage level
    let is_critical = voltage < BATT_CRITICAL;
    let mut bg_color = if voltage < BATT_CRITICAL {
        RED // Critical: alternator failure or battery drain
    } else if voltage < BATT_WARNING {
        ORANGE // Warning: battery getting low
    } else {
        BLACK // Normal: healthy charging system
    };

    // Apply transition override if provided (smooth color transitions)
    if let Some(override_color) = bg_override {
        bg_color = override_color;
    }

    // Blink effect for critical voltage
    if is_critical && !blink_on {
        bg_color = BLACK;
    }

    // Always redraw background - values change every frame and would leave artifacts
    draw_cell_background(display, x, y, w, h, bg_color);

    // Derive ALL colors from final bg_color (after override + blink)
    // Optimization: compute base_text once, derive all other colors from it
    let base_text = label_color_for_bg(bg_color);
    let label_style = label_style_for_text(base_text);
    let peak_color = peak_highlight_for_text(base_text);

    // Center positions - shake only applied to main value to prevent overflow
    let center_x = (x + w / 2) as i32;
    let center_y = (y + h / 2) as i32;
    let value_x = center_x + shake_offset; // Only value shakes

    // Label at top (no shake - prevents overflow on narrow 80px cells)
    Text::with_text_style("BATT", Point::new(center_x, y as i32 + 14), label_style, CENTERED)
        .draw(display)
        .ok();

    // Trend arrow (positioned to right of "BATT" label, no shake)
    if let Some(rising) = state.get_trend() {
        draw_trend_arrow(display, center_x + 20, y as i32 + 10, rising, base_text);
    }

    // Main value - highlighted when new MIN/MAX detected, shakes when critical
    // Uses medium font (ProFont 18pt) to fit in narrow 80px cell
    // Optimization: heapless::String avoids heap allocation
    let mut value_str: String<16> = String::new();
    let _ = write!(value_str, "{voltage:.1}V");
    let value_color = if state.is_new_peak { peak_color } else { base_text };
    // Peak shadow effect: on light backgrounds, BLACK peak text isn't visually distinct
    // from normal BLACK text. Add WHITE drop shadow to make peaks noticeable.
    if state.is_new_peak && value_color == BLACK {
        let shadow_style = MonoTextStyle::new(VALUE_FONT_MEDIUM, WHITE);
        Text::with_text_style(
            &value_str,
            Point::new(value_x + 1, center_y - 6),
            shadow_style,
            CENTERED,
        )
        .draw(display)
        .ok();
    }
    let value_style = MonoTextStyle::new(VALUE_FONT_MEDIUM, value_color);
    Text::with_text_style(&value_str, Point::new(value_x, center_y - 7), value_style, CENTERED)
        .draw(display)
        .ok();

    // Mini-graph showing voltage history (2 minutes, auto-scaled, no shake)
    let graph_y = center_y + 4;
    let graph_h = 20u32; // Height of graph area
    let graph_w = w - 16; // Width with padding
    let graph_x = x as i32 + 8;

    // Graph line color: always contrast with current background
    let graph_line_color = base_text;
    let (buffer, start_idx, count, data_min, data_max) = state.get_graph_data();
    draw_mini_graph(
        display,
        graph_x,
        graph_y,
        graph_w,
        graph_h,
        buffer,
        start_idx,
        count,
        data_min,
        data_max,
        |_| graph_line_color,
    );

    // MIN/MAX secondary color:
    // On light backgrounds: BLACK for readability
    // On dark backgrounds: ORANGE for accent, WHITE on red critical bg
    let minmax_color = if base_text == BLACK {
        BLACK
    } else if is_critical {
        WHITE // On red critical background
    } else {
        ORANGE // Accent on black background
    };
    let minmax_style = MonoTextStyle::new(LABEL_FONT, minmax_color);

    // MIN value line (uppercase, no colon)
    let mut min_str: String<16> = String::new();
    let _ = write!(min_str, "MIN {min_voltage:.1}V");
    Text::with_text_style(
        &min_str,
        Point::new(center_x, (y + h) as i32 - 18),
        minmax_style,
        CENTERED,
    )
    .draw(display)
    .ok();

    // MAX value line (uppercase, no colon)
    let mut max_str: String<16> = String::new();
    let _ = write!(max_str, "MAX {max_voltage:.1}V");
    Text::with_text_style(
        &max_str,
        Point::new(center_x, (y + h) as i32 - 8),
        minmax_style,
        CENTERED,
    )
    .draw(display)
    .ok();

    // Return actual background color for dirty rectangle tracking
    bg_color
}

/// Draw the Air-Fuel Ratio (AFR) cell with Lambda conversion and mini-graph.
///
/// Shows the air-fuel mixture ratio with Lambda (VAG-style) conversion below.
/// Color-coded to show mixture quality at a glance.
///
/// # Layout
/// ```text
/// ┌─────────────┐
/// │ AFR/LAMBDA  │  Label (top)
/// │   14.7      │  AFR value (center, large font)
/// │  L 1.00     │  Lambda conversion (smaller)
/// │ [~~graph~~] │  Mini sparkline (2 min history)
/// │  OPTIMAL    │  Status text (bottom)
/// └─────────────┘
/// ```
///
/// # Lambda Conversion
/// Lambda = AFR / 14.7 (stoichiometric ratio for gasoline)
/// - Lambda 1.0 = stoichiometric (14.7:1 AFR)
/// - Lambda < 1.0 = rich mixture
/// - Lambda > 1.0 = lean mixture
///
/// VAG (Volkswagen Group) ECUs typically display Lambda instead of AFR.
///
/// # AFR Ranges and Colors (tuned for turbocharged engines)
/// - **Blue** (RICH AF): AFR < 12.0, Lambda < 0.82
/// - **Dark Teal** (RICH): AFR 12.0-14.0, Lambda 0.82-0.95
/// - **Green** (OPTIMAL): AFR 14.0-14.9, Lambda 0.95-1.01
/// - **Orange** (LEAN): AFR 14.9-15.5, Lambda 1.01-1.05
/// - **Red** (LEAN AF): AFR > 15.5, Lambda > 1.05 (blinks at ~4Hz + shakes)
///
/// # Features
/// - **Smooth transitions**: Optional `bg_override` enables smooth color fades (currently passed as None since AFR
///   color changes are meaningful thresholds)
///
/// # Shake Animation
/// Pass `shake_offset` from [`crate::animations::calculate_shake_offset`] to add
/// horizontal wiggle when in LEAN AF state. Pass 0 for no shake.
///
/// # Optimization
/// Uses `heapless::String<16>` for value formatting.
/// Background is always redrawn since values animate continuously.
/// Returns the actual background color used for render state tracking.
#[allow(clippy::too_many_arguments)]
pub fn draw_afr_cell(
    display: &mut SimulatorDisplay<Rgb565>,
    x: u32,
    y: u32,
    w: u32,
    h: u32,
    afr: f32,
    state: &SensorState,
    blink_on: bool,
    shake_offset: i32,
    bg_override: Option<Rgb565>,
) -> Rgb565 {
    // Determine background and status based on AFR value
    // Thresholds tuned for turbocharged engines - conservative about lean conditions
    let is_critical = afr > AFR_LEAN_CRITICAL; // LEAN AF is critical
    let (mut bg_color, status) = if afr < AFR_RICH_AF {
        (BLUE, "RICH AF") // Very rich - fuel washing, fouling risk
    } else if afr < AFR_RICH {
        (DARK_TEAL, "RICH") // Rich - safe for power/cooling under load
    } else if afr < AFR_OPTIMAL_MAX {
        (GREEN, "OPTIMAL") // Efficient cruise, slightly rich of stoich (14.7)
    } else if afr <= AFR_LEAN_CRITICAL {
        (ORANGE, "LEAN") // Getting lean - watch under load
    } else {
        (RED, "LEAN AF") // Dangerous lean - detonation risk, blinks + shakes
    };

    // Apply transition override if provided (smooth color transitions)
    if let Some(override_color) = bg_override {
        bg_color = override_color;
    }

    // Blink effect for critical lean condition
    if is_critical && !blink_on {
        bg_color = BLACK;
    }

    // Derive ALL colors from final bg_color (after override + blink)
    // Optimization: compute base_text once, derive all other colors from it
    let text_color = label_color_for_bg(bg_color);
    let label_style = label_style_for_text(text_color);

    // Always redraw background - values change every frame and would leave artifacts
    draw_cell_background(display, x, y, w, h, bg_color);

    // Center positions - shake only applied to main value to prevent overflow
    let center_x = (x + w / 2) as i32;
    let center_y = (y + h / 2) as i32;
    let value_x = center_x + shake_offset; // Only value shakes

    // Label at top (no shake - prevents overflow on narrow 80px cells)
    Text::with_text_style("AFR/LAMBDA", Point::new(center_x, y as i32 + 14), label_style, CENTERED)
        .draw(display)
        .ok();

    // Main AFR value (large font, centered, shakes when critical)
    let mut value_str: String<16> = String::new();
    let _ = write!(value_str, "{afr:.1}");
    let value_style = value_style_for_color(text_color);
    Text::with_text_style(&value_str, Point::new(value_x, center_y - 14), value_style, CENTERED)
        .draw(display)
        .ok();

    // Lambda conversion below AFR value (smaller font, no shake)
    // Lambda = AFR / AFR_STOICH (14.7)
    let lambda = afr / AFR_STOICH;
    let mut lambda_str: String<16> = String::new();
    let _ = write!(lambda_str, "L {lambda:.2}");
    let lambda_style = MonoTextStyle::new(LABEL_FONT, text_color);
    Text::with_text_style(&lambda_str, Point::new(center_x, center_y + 4), lambda_style, CENTERED)
        .draw(display)
        .ok();

    // Mini-graph showing AFR history (2 minutes, auto-scaled, no shake)
    let graph_y = center_y + 14;
    let graph_h = 16u32; // Height of graph area (compact to fit status)
    let graph_w = w - 16; // Width with padding
    let graph_x = x as i32 + 8;

    // Graph line color: always contrast with current background
    let graph_line_color = text_color;
    let (buffer, start_idx, count, data_min, data_max) = state.get_graph_data();
    draw_mini_graph(
        display,
        graph_x,
        graph_y,
        graph_w,
        graph_h,
        buffer,
        start_idx,
        count,
        data_min,
        data_max,
        |_| graph_line_color,
    );

    // Status text at bottom
    let status_style = MonoTextStyle::new(LABEL_FONT, text_color);
    Text::with_text_style(status, Point::new(center_x, (y + h) as i32 - 8), status_style, CENTERED)
        .draw(display)
        .ok();

    // Return actual background color for dirty rectangle tracking
    bg_color
}

// =============================================================================
// Unit Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::thresholds::{BOOST_EASTER_EGG_BAR, BOOST_EASTER_EGG_PSI};

    // -------------------------------------------------------------------------
    // Luminance / Color Tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_label_color_for_bg_dark_backgrounds() {
        // Dark backgrounds should return WHITE text
        assert_eq!(label_color_for_bg(BLACK), WHITE, "BLACK should give WHITE text");
        assert_eq!(label_color_for_bg(RED), WHITE, "RED should give WHITE text");
        assert_eq!(label_color_for_bg(BLUE), WHITE, "BLUE should give WHITE text");
        assert_eq!(label_color_for_bg(DARK_TEAL), WHITE, "DARK_TEAL should give WHITE text");
    }

    #[test]
    fn test_label_color_for_bg_light_backgrounds() {
        // Light backgrounds should return BLACK text
        assert_eq!(label_color_for_bg(GREEN), BLACK, "GREEN should give BLACK text");
        assert_eq!(label_color_for_bg(YELLOW), BLACK, "YELLOW should give BLACK text");
        assert_eq!(label_color_for_bg(ORANGE), BLACK, "ORANGE should give BLACK text");
        assert_eq!(label_color_for_bg(WHITE), BLACK, "WHITE should give BLACK text");
    }

    #[test]
    fn test_peak_highlight_color_dark_backgrounds() {
        // Dark backgrounds should return YELLOW highlight
        assert_eq!(
            peak_highlight_color(BLACK),
            YELLOW,
            "BLACK bg should give YELLOW highlight"
        );
        assert_eq!(peak_highlight_color(RED), YELLOW, "RED bg should give YELLOW highlight");
        assert_eq!(
            peak_highlight_color(BLUE),
            YELLOW,
            "BLUE bg should give YELLOW highlight"
        );
    }

    #[test]
    fn test_peak_highlight_color_light_backgrounds() {
        // Light backgrounds should return BLACK highlight
        assert_eq!(
            peak_highlight_color(GREEN),
            BLACK,
            "GREEN bg should give BLACK highlight"
        );
        assert_eq!(
            peak_highlight_color(YELLOW),
            BLACK,
            "YELLOW bg should give BLACK highlight"
        );
        assert_eq!(
            peak_highlight_color(ORANGE),
            BLACK,
            "ORANGE bg should give BLACK highlight"
        );
    }

    // -------------------------------------------------------------------------
    // Temperature Color Function Tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_temp_color_oil_dsg_normal() {
        let (bg, _) = temp_color_oil_dsg(85.0);
        assert_eq!(bg, BLACK, "Oil temp 85C should be BLACK (normal)");
    }

    #[test]
    fn test_temp_color_oil_dsg_elevated() {
        let (bg, _) = temp_color_oil_dsg(95.0);
        assert_eq!(bg, YELLOW, "Oil temp 95C should be YELLOW (elevated)");
    }

    #[test]
    fn test_temp_color_oil_dsg_high() {
        let (bg, _) = temp_color_oil_dsg(105.0);
        assert_eq!(bg, ORANGE, "Oil temp 105C should be ORANGE (high)");
    }

    #[test]
    fn test_temp_color_oil_dsg_critical() {
        let (bg, _) = temp_color_oil_dsg(115.0);
        assert_eq!(bg, RED, "Oil temp 115C should be RED (critical)");
    }

    #[test]
    fn test_temp_color_oil_dsg_thresholds() {
        // Test exact threshold values
        let (bg_89, _) = temp_color_oil_dsg(89.9);
        let (bg_90, _) = temp_color_oil_dsg(90.0);
        let (bg_99, _) = temp_color_oil_dsg(99.9);
        let (bg_100, _) = temp_color_oil_dsg(100.0);
        let (bg_109, _) = temp_color_oil_dsg(109.9);
        let (bg_110, _) = temp_color_oil_dsg(110.0);

        assert_eq!(bg_89, BLACK, "89.9C should be BLACK");
        assert_eq!(bg_90, YELLOW, "90C should be YELLOW");
        assert_eq!(bg_99, YELLOW, "99.9C should be YELLOW");
        assert_eq!(bg_100, ORANGE, "100C should be ORANGE");
        assert_eq!(bg_109, ORANGE, "109.9C should be ORANGE");
        assert_eq!(bg_110, RED, "110C should be RED");
    }

    #[test]
    fn test_temp_color_water_cold() {
        let (bg, _) = temp_color_water(70.0);
        assert_eq!(bg, ORANGE, "Coolant 70C should be ORANGE (cold)");
    }

    #[test]
    fn test_temp_color_water_optimal() {
        let (bg, _) = temp_color_water(85.0);
        assert_eq!(bg, GREEN, "Coolant 85C should be GREEN (optimal)");
    }

    #[test]
    fn test_temp_color_water_hot() {
        let (bg, _) = temp_color_water(95.0);
        assert_eq!(bg, RED, "Coolant 95C should be RED (overheating)");
    }

    #[test]
    fn test_temp_color_water_thresholds() {
        let (bg_74, _) = temp_color_water(74.9);
        let (bg_75, _) = temp_color_water(75.0);
        let (bg_90, _) = temp_color_water(90.0);
        let (bg_91, _) = temp_color_water(90.1);

        assert_eq!(bg_74, ORANGE, "74.9C should be ORANGE (cold)");
        assert_eq!(bg_75, GREEN, "75C should be GREEN (optimal)");
        assert_eq!(bg_90, GREEN, "90C should be GREEN (optimal)");
        assert_eq!(bg_91, RED, "90.1C should be RED (hot)");
    }

    // -------------------------------------------------------------------------
    // Critical State Function Tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_is_critical_oil_dsg() {
        assert!(!is_critical_oil_dsg(109.9), "109.9C should not be critical");
        assert!(is_critical_oil_dsg(110.0), "110C should be critical");
        assert!(is_critical_oil_dsg(120.0), "120C should be critical");
    }

    #[test]
    fn test_is_critical_water() {
        assert!(!is_critical_water(90.0), "90C should not be critical");
        assert!(is_critical_water(90.1), "90.1C should be critical");
        assert!(is_critical_water(100.0), "100C should be critical");
    }

    #[test]
    fn test_is_critical_afr() {
        assert!(!is_critical_afr(15.5), "AFR 15.5 should not be critical");
        assert!(is_critical_afr(15.6), "AFR 15.6 should be critical");
        assert!(is_critical_afr(16.0), "AFR 16.0 should be critical");
    }

    // -------------------------------------------------------------------------
    // IAT (Intake Air Temperature) Tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_temp_color_iat_very_cold() {
        let (bg, text) = temp_color_iat(-10.0);
        assert_eq!(bg, BLUE, "IAT -10C should be BLUE (icing risk)");
        assert_eq!(text, WHITE, "IAT -10C should have WHITE text");
    }

    #[test]
    fn test_temp_color_iat_optimal() {
        let (bg, text) = temp_color_iat(15.0);
        assert_eq!(bg, GREEN, "IAT 15C should be GREEN (optimal)");
        assert_eq!(text, BLACK, "IAT 15C should have BLACK text");
    }

    #[test]
    fn test_temp_color_iat_warm() {
        let (bg, _) = temp_color_iat(35.0);
        assert_eq!(bg, YELLOW, "IAT 35C should be YELLOW (warm)");
    }

    #[test]
    fn test_temp_color_iat_hot() {
        let (bg, _) = temp_color_iat(55.0);
        assert_eq!(bg, ORANGE, "IAT 55C should be ORANGE (hot)");
    }

    #[test]
    fn test_temp_color_iat_critical() {
        let (bg, text) = temp_color_iat(70.0);
        assert_eq!(bg, RED, "IAT 70C should be RED (critical)");
        assert_eq!(text, WHITE, "IAT 70C should have WHITE text");
    }

    #[test]
    fn test_temp_color_iat_thresholds() {
        // Test exact threshold values
        let (bg_neg, _) = temp_color_iat(-0.1);
        let (bg_0, _) = temp_color_iat(0.0);
        let (bg_24, _) = temp_color_iat(24.9);
        let (bg_25, _) = temp_color_iat(25.0);
        let (bg_44, _) = temp_color_iat(44.9);
        let (bg_45, _) = temp_color_iat(45.0);
        let (bg_59, _) = temp_color_iat(59.9);
        let (bg_60, _) = temp_color_iat(60.0);

        assert_eq!(bg_neg, BLUE, "-0.1C should be BLUE");
        assert_eq!(bg_0, GREEN, "0C should be GREEN");
        assert_eq!(bg_24, GREEN, "24.9C should be GREEN");
        assert_eq!(bg_25, YELLOW, "25C should be YELLOW");
        assert_eq!(bg_44, YELLOW, "44.9C should be YELLOW");
        assert_eq!(bg_45, ORANGE, "45C should be ORANGE");
        assert_eq!(bg_59, ORANGE, "59.9C should be ORANGE");
        assert_eq!(bg_60, RED, "60C should be RED");
    }

    #[test]
    fn test_is_critical_iat() {
        assert!(!is_critical_iat(59.9), "IAT 59.9C should not be critical");
        assert!(is_critical_iat(60.0), "IAT 60C should be critical (hot)");
        assert!(!is_critical_iat(-19.9), "IAT -19.9C should not be critical");
        assert!(is_critical_iat(-20.0), "IAT -20C should be critical (extreme cold)");
        assert!(is_critical_iat(-30.0), "IAT -30C should be critical");
    }

    // -------------------------------------------------------------------------
    // EGT (Exhaust Gas Temperature) Tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_temp_color_egt_cold() {
        let (bg, text) = temp_color_egt(200.0);
        assert_eq!(bg, BLUE, "EGT 200C should be BLUE (warming up)");
        assert_eq!(text, WHITE, "EGT 200C should have WHITE text");
    }

    #[test]
    fn test_temp_color_egt_normal() {
        let (bg, text) = temp_color_egt(400.0);
        assert_eq!(bg, GREEN, "EGT 400C should be GREEN (normal cruise)");
        assert_eq!(text, BLACK, "EGT 400C should have BLACK text");
    }

    #[test]
    fn test_temp_color_egt_spirited() {
        let (bg, _) = temp_color_egt(600.0);
        assert_eq!(bg, YELLOW, "EGT 600C should be YELLOW (spirited)");
    }

    #[test]
    fn test_temp_color_egt_high() {
        let (bg, _) = temp_color_egt(750.0);
        assert_eq!(bg, ORANGE, "EGT 750C should be ORANGE (high load)");
    }

    #[test]
    fn test_temp_color_egt_critical() {
        let (bg, text) = temp_color_egt(900.0);
        assert_eq!(bg, RED, "EGT 900C should be RED (critical)");
        assert_eq!(text, WHITE, "EGT 900C should have WHITE text");
    }

    #[test]
    fn test_temp_color_egt_thresholds() {
        // Test exact threshold values
        let (bg_299, _) = temp_color_egt(299.9);
        let (bg_300, _) = temp_color_egt(300.0);
        let (bg_499, _) = temp_color_egt(499.9);
        let (bg_500, _) = temp_color_egt(500.0);
        let (bg_699, _) = temp_color_egt(699.9);
        let (bg_700, _) = temp_color_egt(700.0);
        let (bg_849, _) = temp_color_egt(849.9);
        let (bg_850, _) = temp_color_egt(850.0);

        assert_eq!(bg_299, BLUE, "299.9C should be BLUE");
        assert_eq!(bg_300, GREEN, "300C should be GREEN");
        assert_eq!(bg_499, GREEN, "499.9C should be GREEN");
        assert_eq!(bg_500, YELLOW, "500C should be YELLOW");
        assert_eq!(bg_699, YELLOW, "699.9C should be YELLOW");
        assert_eq!(bg_700, ORANGE, "700C should be ORANGE");
        assert_eq!(bg_849, ORANGE, "849.9C should be ORANGE");
        assert_eq!(bg_850, RED, "850C should be RED");
    }

    #[test]
    fn test_is_critical_egt() {
        assert!(!is_critical_egt(849.9), "EGT 849.9C should not be critical");
        assert!(is_critical_egt(850.0), "EGT 850C should be critical");
        assert!(is_critical_egt(900.0), "EGT 900C should be critical");
    }

    // -------------------------------------------------------------------------
    // Constant Value Tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_bar_to_psi_conversion() {
        let bar = 1.0;
        let psi = bar * BAR_TO_PSI;
        // BAR_TO_PSI in thresholds.rs is 14.5038, allow small tolerance
        assert!((psi - 14.5).abs() < 0.01, "1 bar should equal ~14.5 PSI");
    }

    #[test]
    fn test_stoich_afr() {
        assert!((AFR_STOICH - 14.7).abs() < 0.001, "Stoichiometric AFR should be 14.7");
    }

    #[test]
    fn test_easter_egg_thresholds() {
        // Easter egg triggers at ~2 bar - verify threshold is in expected range
        // Use const assertion to satisfy clippy
        const _: () = assert!(BOOST_EASTER_EGG_BAR < 2.0);
        const _: () = assert!(BOOST_EASTER_EGG_BAR > 1.9);

        // PSI threshold should correspond to ~2 bar
        let expected_psi = 2.0 * BAR_TO_PSI;
        assert!(
            (BOOST_EASTER_EGG_PSI - expected_psi).abs() < 0.5,
            "Easter egg PSI threshold should be ~29 PSI"
        );
    }
}
