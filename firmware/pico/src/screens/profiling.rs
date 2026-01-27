//! Profiling/debug screen for performance analysis.
//!
//! Displays render times, flush times, FPS, and system info.

use core::fmt::Write;

use dashboard_common::colors::{BLACK, GREEN, WHITE, YELLOW};
use dashboard_common::styles::LABEL_FONT;
use embedded_graphics::mono_font::MonoTextStyle;
use embedded_graphics::prelude::*;
use embedded_graphics::text::Text;
use heapless::String;

/// Profiling data to display on the debug screen.
pub struct ProfilingData {
    pub current_fps: f32,
    pub frame_count: u32,
    pub render_time_us: u32,
    pub flush_time_us: u32,
    pub total_frame_time_us: u32,
}

/// Draw the profiling/debug page.
///
/// Shows performance metrics including FPS, render/flush times, and system info.
pub fn draw_profiling_page<D>(
    display: &mut D,
    data: &ProfilingData,
) where
    D: DrawTarget<Color = embedded_graphics::pixelcolor::Rgb565>,
{
    let header_style = MonoTextStyle::new(LABEL_FONT, GREEN);
    let value_style = MonoTextStyle::new(LABEL_FONT, WHITE);
    let highlight_style = MonoTextStyle::new(LABEL_FONT, YELLOW);

    // Clear screen to prevent overlapping text
    display.clear(BLACK).ok();

    // Header
    Text::new("PROFILING", Point::new(4, 12), header_style)
        .draw(display)
        .ok();

    // FPS
    let mut fps_str: String<16> = String::new();
    let _ = write!(fps_str, "FPS: {:.1}", data.current_fps);
    Text::new(&fps_str, Point::new(4, 30), highlight_style)
        .draw(display)
        .ok();

    // Frame count
    let mut frame_str: String<20> = String::new();
    let _ = write!(frame_str, "Frame: {}", data.frame_count);
    Text::new(&frame_str, Point::new(4, 45), value_style).draw(display).ok();

    // Render time
    let mut render_str: String<24> = String::new();
    let _ = write!(render_str, "Render: {} us", data.render_time_us);
    Text::new(&render_str, Point::new(4, 65), value_style)
        .draw(display)
        .ok();

    // Flush time
    let mut flush_str: String<24> = String::new();
    let _ = write!(flush_str, "Flush:  {} us", data.flush_time_us);
    Text::new(&flush_str, Point::new(4, 80), value_style).draw(display).ok();

    // Total frame time
    let mut total_str: String<24> = String::new();
    let _ = write!(total_str, "Total:  {} us", data.total_frame_time_us);
    Text::new(&total_str, Point::new(4, 95), highlight_style)
        .draw(display)
        .ok();

    // Frame time in ms for easier reading
    let frame_ms = data.total_frame_time_us as f32 / 1000.0;
    let mut ms_str: String<24> = String::new();
    let _ = write!(ms_str, "        {:.1} ms", frame_ms);
    Text::new(&ms_str, Point::new(4, 110), value_style).draw(display).ok();

    // Theoretical max FPS
    let max_fps = if data.total_frame_time_us > 0 {
        1_000_000.0 / data.total_frame_time_us as f32
    } else {
        0.0
    };
    let mut max_fps_str: String<24> = String::new();
    let _ = write!(max_fps_str, "Max FPS: {:.1}", max_fps);
    Text::new(&max_fps_str, Point::new(4, 130), value_style)
        .draw(display)
        .ok();

    // Separator
    Text::new("----------------", Point::new(4, 150), value_style)
        .draw(display)
        .ok();

    // Build info - CPU speed
    #[cfg(feature = "overclock")]
    Text::new("CPU: 250 MHz (OC)", Point::new(4, 165), value_style)
        .draw(display)
        .ok();

    #[cfg(not(feature = "overclock"))]
    Text::new("CPU: 150 MHz", Point::new(4, 165), value_style)
        .draw(display)
        .ok();

    // SPI speed
    Text::new("SPI: 62.5 MHz", Point::new(4, 180), value_style)
        .draw(display)
        .ok();

    // Instructions
    Text::new("Press Y to return", Point::new(4, 220), header_style)
        .draw(display)
        .ok();
}
