//! Debug/profiling page rendering.

use core::fmt::Write;

use dashboard_common::colors::{BLACK, GRAY, GREEN, ORANGE, WHITE, YELLOW};
use dashboard_common::config::{SCREEN_HEIGHT, SCREEN_WIDTH};
use dashboard_common::profiling::DebugLog;
use dashboard_common::styles::LABEL_FONT;
use embedded_graphics::mono_font::MonoTextStyle;
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::{Line, PrimitiveStyle, Rectangle};
use embedded_graphics::text::Text;
use embedded_graphics_simulator::SimulatorDisplay;
use heapless::String;

use crate::profiling::ProfilingMetrics;

const HEADER_Y: i32 = 12;
const HEADER_DIVIDER_Y: i32 = 18;
const SECTION_HEADER_Y: i32 = 28;
const STATS_Y: i32 = 40;
const LOG_DIVIDER_Y: i32 = 130;
const LOG_Y: i32 = 138;
const LOG_LINE_HEIGHT: i32 = 12;
const COL1_X: i32 = 4;
const COL2_X: i32 = 110;
const COL3_X: i32 = 215;
const STAT_LINE_HEIGHT: i32 = 13;

const DEBUG_BG: Rgb565 = BLACK;
const HEADER_COLOR: Rgb565 = GREEN;
const SECTION_COLOR: Rgb565 = GRAY;
const VALUE_COLOR: Rgb565 = WHITE;
const HIGHLIGHT_COLOR: Rgb565 = YELLOW;
const LOG_PROMPT_COLOR: Rgb565 = GREEN;
const LOG_TEXT_COLOR: Rgb565 = ORANGE;
const DIVIDER_COLOR: Rgb565 = GRAY;

const EST_STACK_KB: u32 = 4;
const SENSOR_STATE_STACK_BYTES: u32 = 560;
const SENSOR_STATE_HEAP_BYTES: u32 = 200;
const LOG_BUFFER_BYTES: u32 = 288;
const NUM_SENSORS: u32 = 7;

pub fn draw_debug_page(
    display: &mut SimulatorDisplay<Rgb565>,
    metrics: &ProfilingMetrics,
    log: &DebugLog,
    fps: f32,
) {
    display.clear(DEBUG_BG).ok();
    draw_header(display, metrics, fps);
    draw_horizontal_line(display, HEADER_DIVIDER_Y);
    draw_section_headers(display);
    draw_timing_column(display, metrics);
    draw_render_column(display, metrics);
    draw_memory_column(display);
    draw_horizontal_line(display, LOG_DIVIDER_Y);
    draw_log_terminal(display, log);
}

fn draw_header(
    display: &mut SimulatorDisplay<Rgb565>,
    metrics: &ProfilingMetrics,
    fps: f32,
) {
    let header_style = MonoTextStyle::new(LABEL_FONT, HEADER_COLOR);
    let info_style = MonoTextStyle::new(LABEL_FONT, VALUE_COLOR);

    Text::new("DEBUG VIEW", Point::new(COL1_X, HEADER_Y), header_style)
        .draw(display)
        .ok();

    let uptime = metrics.uptime_string();
    let mut uptime_str: String<24> = String::new();
    let _ = write!(uptime_str, "UP {uptime}");
    Text::new(&uptime_str, Point::new(160, HEADER_Y), info_style)
        .draw(display)
        .ok();

    let mut fps_str: String<12> = String::new();
    let _ = write!(fps_str, "{fps:.0} FPS");
    Text::new(&fps_str, Point::new(280, HEADER_Y), info_style)
        .draw(display)
        .ok();
}

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

fn draw_timing_column(
    display: &mut SimulatorDisplay<Rgb565>,
    metrics: &ProfilingMetrics,
) {
    let value_style = MonoTextStyle::new(LABEL_FONT, VALUE_COLOR);
    let highlight_style = MonoTextStyle::new(LABEL_FONT, HIGHLIGHT_COLOR);

    let x = COL1_X;
    let mut y = STATS_Y;

    let mut s: String<20> = String::new();
    let _ = write!(s, "Frame: {:.1}ms", metrics.frame_time_us as f32 / 1000.0);
    Text::new(&s, Point::new(x, y), value_style).draw(display).ok();
    y += STAT_LINE_HEIGHT;

    let mut s: String<20> = String::new();
    let _ = write!(s, "Render:{:.1}ms", metrics.render_time_us as f32 / 1000.0);
    Text::new(&s, Point::new(x, y), value_style).draw(display).ok();
    y += STAT_LINE_HEIGHT;

    let mut s: String<20> = String::new();
    let _ = write!(s, "Sleep: {:.1}ms", metrics.sleep_time_us as f32 / 1000.0);
    Text::new(&s, Point::new(x, y), value_style).draw(display).ok();
    y += STAT_LINE_HEIGHT;

    let min_ms = if metrics.frame_time_min_us == u32::MAX {
        0.0
    } else {
        metrics.frame_time_min_us as f32 / 1000.0
    };
    let mut s: String<20> = String::new();
    let _ = write!(s, "Min:   {min_ms:.1}ms");
    Text::new(&s, Point::new(x, y), highlight_style).draw(display).ok();
    y += STAT_LINE_HEIGHT;

    let mut s: String<20> = String::new();
    let _ = write!(s, "Max:   {:.1}ms", metrics.frame_time_max_us as f32 / 1000.0);
    Text::new(&s, Point::new(x, y), highlight_style).draw(display).ok();
    y += STAT_LINE_HEIGHT;

    let mut s: String<20> = String::new();
    let _ = write!(s, "Avg:   {:.1}ms", metrics.frame_time_avg_us() as f32 / 1000.0);
    Text::new(&s, Point::new(x, y), highlight_style).draw(display).ok();
}

fn draw_render_column(
    display: &mut SimulatorDisplay<Rgb565>,
    metrics: &ProfilingMetrics,
) {
    let value_style = MonoTextStyle::new(LABEL_FONT, VALUE_COLOR);

    let x = COL2_X;
    let mut y = STATS_Y;

    let mut s: String<20> = String::new();
    let _ = write!(s, "Frames:{}", metrics.total_frames);
    Text::new(&s, Point::new(x, y), value_style).draw(display).ok();
    y += STAT_LINE_HEIGHT;

    let mut s: String<20> = String::new();
    let _ = write!(s, "Hdrs:  {}", metrics.header_redraws);
    Text::new(&s, Point::new(x, y), value_style).draw(display).ok();
    y += STAT_LINE_HEIGHT;

    let mut s: String<20> = String::new();
    let _ = write!(s, "Cells: {}", metrics.cell_draws);
    Text::new(&s, Point::new(x, y), value_style).draw(display).ok();
    y += STAT_LINE_HEIGHT;

    let mut s: String<20> = String::new();
    let _ = write!(s, "Divs:  {}", metrics.divider_redraws);
    Text::new(&s, Point::new(x, y), value_style).draw(display).ok();
    y += STAT_LINE_HEIGHT;

    let mut s: String<20> = String::new();
    let _ = write!(s, "Trans: {}", metrics.color_transitions);
    Text::new(&s, Point::new(x, y), value_style).draw(display).ok();
    y += STAT_LINE_HEIGHT;

    let mut s: String<20> = String::new();
    let _ = write!(s, "Peaks: {}", metrics.peaks_detected);
    Text::new(&s, Point::new(x, y), value_style).draw(display).ok();
}

fn draw_memory_column(display: &mut SimulatorDisplay<Rgb565>) {
    let value_style = MonoTextStyle::new(LABEL_FONT, VALUE_COLOR);

    let x = COL3_X;
    let mut y = STATS_Y;

    let mut s: String<20> = String::new();
    let _ = write!(s, "Stack: ~{EST_STACK_KB}KB");
    Text::new(&s, Point::new(x, y), value_style).draw(display).ok();
    y += STAT_LINE_HEIGHT;

    let heap_bytes = NUM_SENSORS * SENSOR_STATE_HEAP_BYTES;
    let mut s: String<20> = String::new();
    let _ = write!(s, "Heap: ~{heap_bytes}B");
    Text::new(&s, Point::new(x, y), value_style).draw(display).ok();
    y += STAT_LINE_HEIGHT;

    let mut s: String<20> = String::new();
    let _ = write!(s, "Sensor:{NUM_SENSORS}x{SENSOR_STATE_STACK_BYTES}B");
    Text::new(&s, Point::new(x, y), value_style).draw(display).ok();
    y += STAT_LINE_HEIGHT;

    let mut s: String<20> = String::new();
    let _ = write!(s, "Log:   {LOG_BUFFER_BYTES}B");
    Text::new(&s, Point::new(x, y), value_style).draw(display).ok();
    y += STAT_LINE_HEIGHT;

    let total_bytes = (EST_STACK_KB * 1024) + (NUM_SENSORS * SENSOR_STATE_STACK_BYTES) + heap_bytes + LOG_BUFFER_BYTES;
    let total_kb = (total_bytes + 512) / 1024;
    let mut s: String<20> = String::new();
    let _ = write!(s, "Total: ~{total_kb}KB");
    let highlight_style = MonoTextStyle::new(LABEL_FONT, HIGHLIGHT_COLOR);
    Text::new(&s, Point::new(x, y), highlight_style).draw(display).ok();
}

fn draw_log_terminal(
    display: &mut SimulatorDisplay<Rgb565>,
    log: &DebugLog,
) {
    let prompt_style = MonoTextStyle::new(LABEL_FONT, LOG_PROMPT_COLOR);
    let text_style = MonoTextStyle::new(LABEL_FONT, LOG_TEXT_COLOR);

    Rectangle::new(
        Point::new(0, LOG_DIVIDER_Y + 2),
        Size::new(SCREEN_WIDTH, SCREEN_HEIGHT - LOG_DIVIDER_Y as u32 - 2),
    )
    .into_styled(PrimitiveStyle::with_fill(Rgb565::new(1, 2, 1)))
    .draw(display)
    .ok();

    let mut y = LOG_Y;

    for line in log.iter() {
        Text::new(">", Point::new(COL1_X, y), prompt_style).draw(display).ok();
        Text::new(line, Point::new(COL1_X + 10, y), text_style)
            .draw(display)
            .ok();
        y += LOG_LINE_HEIGHT;
    }

    Text::new("> _", Point::new(COL1_X, y), prompt_style).draw(display).ok();
}

fn draw_horizontal_line(
    display: &mut SimulatorDisplay<Rgb565>,
    y: i32,
) {
    Line::new(Point::new(2, y), Point::new(SCREEN_WIDTH as i32 - 2, y))
        .into_styled(PrimitiveStyle::with_stroke(DIVIDER_COLOR, 1))
        .draw(display)
        .ok();
}
