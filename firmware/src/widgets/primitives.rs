//! Low-level drawing primitives shared across widgets.
//!
//! This module contains reusable drawing functions that multiple widgets need.
//! These are intentionally simple and focused on a single responsibility.
//!
//! # Cell Background Inset
//!
//! The `draw_cell_background` function draws rectangles with a 2px inset from
//! the cell boundaries. This creates thin black borders between cells when the
//! display is cleared to black, providing visual separation without explicit
//! border drawing.
//!
//! Note: The main loop clears the display on first frame and when popups close.
//! Since cell drawing functions always redraw their backgrounds (values animate
//! every frame), the 2px border areas are preserved between the inset rectangles.
//!
//! # Mini Sparkline Graph
//!
//! The `draw_mini_graph` function renders a compact line graph showing sensor
//! history over time. The graph auto-scales to fit the data range (local min/max).
//!
//! The `color_fn` parameter accepts a closure that receives each sample value
//! and returns the line color for that segment. In practice, callers pass a
//! closure that ignores the value and returns a fixed high-contrast color
//! derived from the cell's background luminance (via `label_color_for_bg`).
//! This ensures the graph line always contrasts with the current background
//! regardless of the data values.
//!
//! X-axis scaling uses the actual sample count, not the maximum buffer size,
//! so sparse data (early in a session) spreads across the full graph width
//! rather than clustering on the left side.

use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::{Line, PrimitiveStyle, Rectangle};
use embedded_graphics_simulator::SimulatorDisplay;

use crate::state::GRAPH_HISTORY_SIZE;

/// Draw a cell's background rectangle with 2px inset.
///
/// The inset creates visual separation between cells without explicit borders.
/// When the display is cleared to black, the 2px gap appears as thin black lines.
///
/// # Parameters
/// - `x`, `y`: Top-left corner of the cell boundary
/// - `w`, `h`: Full cell dimensions (must be >= 4 to have drawable area)
/// - `bg_color`: Fill color for the background
///
/// # Inset Calculation
/// - Position: `(x + 2, y + 2)` - offset 2px from top-left
/// - Size: `(w - 4, h - 4)` - shrink by 2px on each side
///
/// # Safety
/// Returns early if dimensions are too small (w < 4 or h < 4) to prevent
/// u32 underflow in the size calculation.
pub fn draw_cell_background(
    display: &mut SimulatorDisplay<Rgb565>,
    x: u32,
    y: u32,
    w: u32,
    h: u32,
    bg_color: Rgb565,
) {
    // Guard against underflow with small dimensions
    if w < 4 || h < 4 {
        return;
    }
    Rectangle::new(Point::new(x as i32 + 2, y as i32 + 2), Size::new(w - 4, h - 4))
        .into_styled(PrimitiveStyle::with_fill(bg_color))
        .draw(display)
        .ok();
}

/// Draw a trend arrow indicator (up or down).
///
/// Used to show whether a sensor value is rising or falling based on
/// recent history. The arrow is drawn using three line segments:
/// - One vertical shaft
/// - Two diagonal segments forming the arrowhead
///
/// # Parameters
/// - `x`, `y`: Center point of the arrow
/// - `rising`: `true` for up arrow, `false` for down arrow
/// - `color`: Arrow stroke color (should contrast with cell background)
///
/// # Arrow Dimensions
/// - Height: 8px total (y-4 to y+4)
/// - Arrowhead width: 6px (x-3 to x+3)
pub fn draw_trend_arrow(
    display: &mut SimulatorDisplay<Rgb565>,
    x: i32,
    y: i32,
    rising: bool,
    color: Rgb565,
) {
    let arrow_style = PrimitiveStyle::with_stroke(color, 1);
    if rising {
        // Up arrow: shaft points upward, arrowhead at top
        Line::new(Point::new(x, y + 4), Point::new(x, y - 4))
            .into_styled(arrow_style)
            .draw(display)
            .ok();
        Line::new(Point::new(x - 3, y - 1), Point::new(x, y - 4))
            .into_styled(arrow_style)
            .draw(display)
            .ok();
        Line::new(Point::new(x + 3, y - 1), Point::new(x, y - 4))
            .into_styled(arrow_style)
            .draw(display)
            .ok();
    } else {
        // Down arrow: shaft points downward, arrowhead at bottom
        Line::new(Point::new(x, y - 4), Point::new(x, y + 4))
            .into_styled(arrow_style)
            .draw(display)
            .ok();
        Line::new(Point::new(x - 3, y + 1), Point::new(x, y + 4))
            .into_styled(arrow_style)
            .draw(display)
            .ok();
        Line::new(Point::new(x + 3, y + 1), Point::new(x, y + 4))
            .into_styled(arrow_style)
            .draw(display)
            .ok();
    }
}

/// Draw a mini sparkline graph showing sensor history.
///
/// The graph auto-scales to the local min/max of the data, providing a clear
/// visual representation of trends over time regardless of the absolute values.
///
/// # Parameters
/// - `x`, `y`: Top-left corner of the graph area
/// - `w`, `h`: Dimensions of the graph area
/// - `buffer`: Circular buffer of samples
/// - `start_idx`: Index of the oldest sample in the buffer
/// - `count`: Number of valid samples in the buffer
/// - `data_min`, `data_max`: Local min/max for Y-axis scaling
/// - `color_fn`: Closure returning line color for each sample. Receives the sample value but current usage ignores it,
///   returning a fixed high-contrast color.
///
/// # Graph Behavior
/// - X-axis: time (oldest left, newest right), spreads across full width
/// - Y-axis: auto-scaled to local min/max with 2px padding
/// - Sparse data fills the graph width (uses `count`, not buffer capacity)
/// - If min == max, draws a horizontal line at center
/// - Line color is determined per-segment by `color_fn`
///
/// # Coordinate System
/// The drawable area is `[graph_x, graph_x + graph_width - 1]` × `[graph_y, graph_y + graph_height - 1]`.
/// X step is computed from `(graph_width - 1)` so the last point lands on the last valid pixel.
/// Y coordinates are clamped to `[graph_y, graph_y + graph_height - 1]` to prevent overflow.
#[allow(clippy::too_many_arguments)]
pub fn draw_mini_graph<F>(
    display: &mut SimulatorDisplay<Rgb565>,
    x: i32,
    y: i32,
    w: u32,
    h: u32,
    buffer: &[f32],
    start_idx: usize,
    count: usize,
    data_min: f32,
    data_max: f32,
    color_fn: F,
) where
    F: Fn(f32) -> Rgb565,
{
    if count < 2 {
        return; // Need at least 2 points to draw a line
    }

    // Guard against small dimensions (need at least 5px for 1px drawable area + 4px padding)
    if w < 5 || h < 5 {
        return;
    }

    // Calculate drawable area with 2px padding on each side
    // Valid pixel coordinates: [graph_x, graph_x + graph_width - 1] (inclusive)
    let graph_width = w as i32 - 4;
    let graph_height = h as i32 - 4;
    let graph_x = x + 2;
    let graph_y = y + 2;

    // Max valid coordinates (inclusive bounds)
    let max_x = graph_x + graph_width - 1;
    let max_y = graph_y + graph_height - 1;

    // Calculate Y scaling (avoid division by zero)
    // Use (graph_height - 1) so max value maps to max_y, not max_y + 1
    let data_range = data_max - data_min;
    let y_scale = if data_range > 0.1 {
        (graph_height - 1) as f32 / data_range
    } else {
        0.0 // Flat line if range is too small
    };

    // Calculate X step to spread data across full width
    // Use (graph_width - 1) so last point lands on max_x, not max_x + 1
    let x_step = (graph_width - 1) as f32 / (count - 1).max(1) as f32;

    // Draw line segments between consecutive points
    let mut prev_screen_x = 0i32;
    let mut prev_screen_y = 0i32;
    let mut first_point = true;

    for i in 0..count {
        // Get sample in chronological order (oldest first)
        let buffer_idx = (start_idx + i) % GRAPH_HISTORY_SIZE;
        let value = buffer[buffer_idx];

        // Calculate screen position
        let screen_x = (graph_x + (i as f32 * x_step) as i32).min(max_x);
        let screen_y = if y_scale > 0.0 {
            // Invert Y (screen Y increases downward, but we want higher values at top)
            // Clamp to valid bounds [graph_y, max_y]
            (graph_y + graph_height - 1 - ((value - data_min) * y_scale) as i32).clamp(graph_y, max_y)
        } else {
            // Flat line in center
            graph_y + (graph_height - 1) / 2
        };

        // Get color based on current value
        let line_color = color_fn(value);
        let line_style = PrimitiveStyle::with_stroke(line_color, 1);

        // Draw line from previous point to current
        if !first_point {
            Line::new(Point::new(prev_screen_x, prev_screen_y), Point::new(screen_x, screen_y))
                .into_styled(line_style)
                .draw(display)
                .ok();
        }

        prev_screen_x = screen_x;
        prev_screen_y = screen_y;
        first_point = false;
    }
}
