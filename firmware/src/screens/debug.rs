//! Debug/profiling page rendering.
//!
//! Displays system metrics, frame timing statistics, and a debug log terminal.
//! Accessible by pressing `Y` key to toggle from the main dashboard.
//!
//! # Layout
//!
//! ```text
//! ┌──────────────────────────────────────────────────────────────────┐
//! │ DEBUG VIEW                              UP 00:12:34       53 FPS │
//! ├──────────────────────────────────────────────────────────────────┤
//! │ TIMING              │ RENDER            │ MEMORY                 │
//! │ Frame:  20.0ms      │ Frames: 12847     │ Stack: ~4KB            │
//! │ Render: 0.5ms       │ Headers: 12       │ Heap:  0B (no-alloc)   │
//! │ Sleep:  19.5ms      │ Cells: 77082      │ Sensors: 7 x 320B      │
//! │ Min:    19.8ms      │ Dividers: 3       │ Graph: 7 x 120B        │
//! │ Max:    25.1ms      │ Trans: 34         │ Log: 288B              │
//! │ Avg:    20.1ms      │ Peaks: 8          │ Total: ~6KB            │
//! ├──────────────────────────────────────────────────────────────────┤
//! │ > System started                                                 │
//! │ > Page: Debug                                                    │
//! │ > _                                                              │
//! └──────────────────────────────────────────────────────────────────┘
//! ```

use core::fmt::Write;

use embedded_graphics::mono_font::MonoTextStyle;
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::{Line, PrimitiveStyle, Rectangle};
use embedded_graphics::text::Text;
use embedded_graphics_simulator::SimulatorDisplay;
use heapless::String;

use crate::colors::{BLACK, GRAY, GREEN, ORANGE, WHITE, YELLOW};
use crate::config::{SCREEN_HEIGHT, SCREEN_WIDTH};
use crate::profiling::{DebugLog, ProfilingMetrics};
use crate::styles::LABEL_FONT;

// =============================================================================
// Layout Constants
// =============================================================================

/// Header Y position (text baseline)
const HEADER_Y: i32 = 12;

/// Y position of divider below header
const HEADER_DIVIDER_Y: i32 = 18;

/// Y position where stats section headers start
const SECTION_HEADER_Y: i32 = 28;

/// Y position where stats values start
const STATS_Y: i32 = 40;

/// Y position of divider above log
const LOG_DIVIDER_Y: i32 = 130;

/// Y position where log terminal starts
const LOG_Y: i32 = 138;

/// Height of each log line (compact)
const LOG_LINE_HEIGHT: i32 = 12;

/// X position for left column (frame timing)
const COL1_X: i32 = 4;

/// X position for middle column (render stats)
const COL2_X: i32 = 110;

/// X position for right column (memory)
const COL3_X: i32 = 215;

/// Line height for stats (compact)
const STAT_LINE_HEIGHT: i32 = 13;

// =============================================================================
// Colors
// =============================================================================

/// Background color for debug page
const DEBUG_BG: Rgb565 = BLACK;

/// Header text color
const HEADER_COLOR: Rgb565 = GREEN;

/// Section header color (dimmer)
const SECTION_COLOR: Rgb565 = GRAY;

/// Value color (bright)
const VALUE_COLOR: Rgb565 = WHITE;

/// Highlight color for min/max/avg
const HIGHLIGHT_COLOR: Rgb565 = YELLOW;

/// Log prompt color
const LOG_PROMPT_COLOR: Rgb565 = GREEN;

/// Log text color
const LOG_TEXT_COLOR: Rgb565 = ORANGE;

/// Divider line color
const DIVIDER_COLOR: Rgb565 = GRAY;

// =============================================================================
// Memory Constants (estimated sizes for RP2350)
// =============================================================================

/// Estimated stack usage (main loop locals, function calls)
const EST_STACK_KB: u32 = 4;

/// `SensorState` stack portion estimate:
/// - `avg_buffer`: 60 * 4 bytes = 240 bytes
/// - `graph_buffer`: 60 * 4 bytes = 240 bytes
/// - misc fields (counters, min/max, etc.): ~80 bytes
///
/// Total stack: ~560 bytes
const SENSOR_STATE_STACK_BYTES: u32 = 560;

/// `SensorState` heap portion estimate (`VecDeque` buffer for trend history):
/// - `VecDeque` capacity: `HISTORY_SIZE` (50) samples
/// - 50 * 4 bytes (f32) = 200 bytes on heap
const SENSOR_STATE_HEAP_BYTES: u32 = 200;

/// Debug log buffer size (6 lines * 48 chars = 288 bytes)
const LOG_BUFFER_BYTES: u32 = 288;

/// Number of sensors being tracked (oil, water, DSG, IAT, EGT, batt, AFR)
const NUM_SENSORS: u32 = 7;

// =============================================================================
// Debug Page Drawing
// =============================================================================

/// Draw the debug/profiling page.
///
/// Clears the display and renders:
/// - Header with "DEBUG VIEW", uptime, and FPS
/// - Three columns: Frame timing, Render stats, Memory estimates
/// - Debug log terminal (bottom section)
pub fn draw_debug_page(
    display: &mut SimulatorDisplay<Rgb565>,
    metrics: &ProfilingMetrics,
    log: &DebugLog,
    fps: f32,
) {
    // Clear display
    display.clear(DEBUG_BG).ok();

    // Draw header
    draw_header(display, metrics, fps);

    // Draw divider below header
    draw_horizontal_line(display, HEADER_DIVIDER_Y);

    // Draw section headers
    draw_section_headers(display);

    // Draw three stat columns
    draw_timing_column(display, metrics);
    draw_render_column(display, metrics);
    draw_memory_column(display);

    // Draw divider above log
    draw_horizontal_line(display, LOG_DIVIDER_Y);

    // Draw log terminal
    draw_log_terminal(display, log);
}

/// Draw the header with title, uptime, and FPS.
fn draw_header(
    display: &mut SimulatorDisplay<Rgb565>,
    metrics: &ProfilingMetrics,
    fps: f32,
) {
    let header_style = MonoTextStyle::new(LABEL_FONT, HEADER_COLOR);
    let info_style = MonoTextStyle::new(LABEL_FONT, VALUE_COLOR);

    // Title
    Text::new("DEBUG VIEW", Point::new(COL1_X, HEADER_Y), header_style)
        .draw(display)
        .ok();

    // Uptime
    let uptime = metrics.uptime_string();
    let mut uptime_str: String<24> = String::new();
    let _ = write!(uptime_str, "UP {uptime}");
    Text::new(&uptime_str, Point::new(160, HEADER_Y), info_style)
        .draw(display)
        .ok();

    // FPS
    let mut fps_str: String<12> = String::new();
    let _ = write!(fps_str, "{fps:.0} FPS");
    Text::new(&fps_str, Point::new(280, HEADER_Y), info_style)
        .draw(display)
        .ok();
}

/// Draw section headers for the stat columns.
fn draw_section_headers(display: &mut SimulatorDisplay<Rgb565>) {
    let style = MonoTextStyle::new(LABEL_FONT, SECTION_COLOR);

    Text::new("TIMING", Point::new(COL1_X, SECTION_HEADER_Y), style)
        .draw(display)
        .ok();
    Text::new("RENDER", Point::new(COL2_X, SECTION_HEADER_Y), style)
        .draw(display)
        .ok();
    Text::new("MEMORY", Point::new(COL3_X, SECTION_HEADER_Y), style)
        .draw(display)
        .ok();
}

/// Draw frame timing statistics (left column).
fn draw_timing_column(
    display: &mut SimulatorDisplay<Rgb565>,
    metrics: &ProfilingMetrics,
) {
    let value_style = MonoTextStyle::new(LABEL_FONT, VALUE_COLOR);
    let highlight_style = MonoTextStyle::new(LABEL_FONT, HIGHLIGHT_COLOR);

    let x = COL1_X;
    let mut y = STATS_Y;

    // Frame time (current)
    let mut s: String<20> = String::new();
    let _ = write!(s, "Frame: {:.1}ms", metrics.frame_time_us as f32 / 1000.0);
    Text::new(&s, Point::new(x, y), value_style).draw(display).ok();
    y += STAT_LINE_HEIGHT;

    // Render time
    let mut s: String<20> = String::new();
    let _ = write!(s, "Render:{:.1}ms", metrics.render_time_us as f32 / 1000.0);
    Text::new(&s, Point::new(x, y), value_style).draw(display).ok();
    y += STAT_LINE_HEIGHT;

    // Sleep time
    let mut s: String<20> = String::new();
    let _ = write!(s, "Sleep: {:.1}ms", metrics.sleep_time_us as f32 / 1000.0);
    Text::new(&s, Point::new(x, y), value_style).draw(display).ok();
    y += STAT_LINE_HEIGHT;

    // Min frame time
    let min_ms = if metrics.frame_time_min_us == u32::MAX {
        0.0
    } else {
        metrics.frame_time_min_us as f32 / 1000.0
    };
    let mut s: String<20> = String::new();
    let _ = write!(s, "Min:   {min_ms:.1}ms");
    Text::new(&s, Point::new(x, y), highlight_style).draw(display).ok();
    y += STAT_LINE_HEIGHT;

    // Max frame time
    let mut s: String<20> = String::new();
    let _ = write!(s, "Max:   {:.1}ms", metrics.frame_time_max_us as f32 / 1000.0);
    Text::new(&s, Point::new(x, y), highlight_style).draw(display).ok();
    y += STAT_LINE_HEIGHT;

    // Average frame time
    let mut s: String<20> = String::new();
    let _ = write!(s, "Avg:   {:.1}ms", metrics.frame_time_avg_us() as f32 / 1000.0);
    Text::new(&s, Point::new(x, y), highlight_style).draw(display).ok();
}

/// Draw render counters (middle column).
fn draw_render_column(
    display: &mut SimulatorDisplay<Rgb565>,
    metrics: &ProfilingMetrics,
) {
    let value_style = MonoTextStyle::new(LABEL_FONT, VALUE_COLOR);

    let x = COL2_X;
    let mut y = STATS_Y;

    // Total frames
    let mut s: String<20> = String::new();
    let _ = write!(s, "Frames:{}", metrics.total_frames);
    Text::new(&s, Point::new(x, y), value_style).draw(display).ok();
    y += STAT_LINE_HEIGHT;

    // Header redraws
    let mut s: String<20> = String::new();
    let _ = write!(s, "Hdrs:  {}", metrics.header_redraws);
    Text::new(&s, Point::new(x, y), value_style).draw(display).ok();
    y += STAT_LINE_HEIGHT;

    // Cell draws
    let mut s: String<20> = String::new();
    let _ = write!(s, "Cells: {}", metrics.cell_draws);
    Text::new(&s, Point::new(x, y), value_style).draw(display).ok();
    y += STAT_LINE_HEIGHT;

    // Divider redraws
    let mut s: String<20> = String::new();
    let _ = write!(s, "Divs:  {}", metrics.divider_redraws);
    Text::new(&s, Point::new(x, y), value_style).draw(display).ok();
    y += STAT_LINE_HEIGHT;

    // Color transitions
    let mut s: String<20> = String::new();
    let _ = write!(s, "Trans: {}", metrics.color_transitions);
    Text::new(&s, Point::new(x, y), value_style).draw(display).ok();
    y += STAT_LINE_HEIGHT;

    // Peaks detected
    let mut s: String<20> = String::new();
    let _ = write!(s, "Peaks: {}", metrics.peaks_detected);
    Text::new(&s, Point::new(x, y), value_style).draw(display).ok();
}

/// Draw memory estimates (right column).
///
/// These are compile-time estimates for embedded deployment.
/// On real hardware, we could use linker symbols or DWT counters.
///
/// Note: `SensorState` uses `VecDeque` for trend history, which allocates on heap.
/// The heap estimate shows this allocation.
fn draw_memory_column(display: &mut SimulatorDisplay<Rgb565>) {
    let value_style = MonoTextStyle::new(LABEL_FONT, VALUE_COLOR);

    let x = COL3_X;
    let mut y = STATS_Y;

    // Stack estimate
    let mut s: String<20> = String::new();
    let _ = write!(s, "Stack: ~{EST_STACK_KB}KB");
    Text::new(&s, Point::new(x, y), value_style).draw(display).ok();
    y += STAT_LINE_HEIGHT;

    // Heap estimate (VecDeque buffers for trend history)
    let heap_bytes = NUM_SENSORS * SENSOR_STATE_HEAP_BYTES;
    let mut s: String<20> = String::new();
    let _ = write!(s, "Heap: ~{heap_bytes}B");
    Text::new(&s, Point::new(x, y), value_style).draw(display).ok();
    y += STAT_LINE_HEIGHT;

    // Sensor state memory (stack portion)
    let sensor_bytes = NUM_SENSORS * SENSOR_STATE_STACK_BYTES;
    let mut s: String<20> = String::new();
    let _ = write!(s, "Sensor:{NUM_SENSORS}x{SENSOR_STATE_STACK_BYTES}B");
    Text::new(&s, Point::new(x, y), value_style).draw(display).ok();
    y += STAT_LINE_HEIGHT;

    // Log buffer
    let mut s: String<20> = String::new();
    let _ = write!(s, "Log:   {LOG_BUFFER_BYTES}B");
    Text::new(&s, Point::new(x, y), value_style).draw(display).ok();
    y += STAT_LINE_HEIGHT;

    // Total estimate - calculate in bytes first, then convert to KB
    // This avoids truncation errors from integer division
    let total_bytes = (EST_STACK_KB * 1024) + sensor_bytes + heap_bytes + LOG_BUFFER_BYTES;
    let total_kb = (total_bytes + 512) / 1024; // Round to nearest KB
    let mut s: String<20> = String::new();
    let _ = write!(s, "Total: ~{total_kb}KB");
    let highlight_style = MonoTextStyle::new(LABEL_FONT, HIGHLIGHT_COLOR);
    Text::new(&s, Point::new(x, y), highlight_style).draw(display).ok();
}

/// Draw the debug log terminal section (compact).
fn draw_log_terminal(
    display: &mut SimulatorDisplay<Rgb565>,
    log: &DebugLog,
) {
    let prompt_style = MonoTextStyle::new(LABEL_FONT, LOG_PROMPT_COLOR);
    let text_style = MonoTextStyle::new(LABEL_FONT, LOG_TEXT_COLOR);

    // Draw terminal background (very dark green tint)
    Rectangle::new(
        Point::new(0, LOG_DIVIDER_Y + 2),
        Size::new(SCREEN_WIDTH, SCREEN_HEIGHT - LOG_DIVIDER_Y as u32 - 2),
    )
    .into_styled(PrimitiveStyle::with_fill(Rgb565::new(1, 2, 1)))
    .draw(display)
    .ok();

    let mut y = LOG_Y;

    // Draw log lines (compact spacing)
    for line in log.iter() {
        // Draw prompt
        Text::new(">", Point::new(COL1_X, y), prompt_style).draw(display).ok();

        // Draw message
        Text::new(line, Point::new(COL1_X + 10, y), text_style)
            .draw(display)
            .ok();

        y += LOG_LINE_HEIGHT;
    }

    // Draw cursor on next line
    Text::new("> _", Point::new(COL1_X, y), prompt_style).draw(display).ok();
}

/// Draw a horizontal divider line.
fn draw_horizontal_line(
    display: &mut SimulatorDisplay<Rgb565>,
    y: i32,
) {
    Line::new(Point::new(2, y), Point::new(SCREEN_WIDTH as i32 - 2, y))
        .into_styled(PrimitiveStyle::with_stroke(DIVIDER_COLOR, 1))
        .draw(display)
        .ok();
}
