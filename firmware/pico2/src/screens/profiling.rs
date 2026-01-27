//! Profiling/debug screen for performance analysis.
//!
//! Displays comprehensive system metrics in a two-column layout optimized for 320x240:
//!
//! # Left Column - Timing & Buffers
//!
//! - **FPS**: Current frames per second (highlighted in yellow)
//! - **Frame**: Total frame count since boot
//! - **Render**: Time to render current frame (microseconds)
//! - **Flush**: Time for DMA transfer to display (microseconds)
//! - **Total**: Combined frame time (render + flush + overhead)
//! - **Max FPS**: Theoretical maximum based on total frame time
//! - **Buffer swaps**: Number of double-buffer swaps
//! - **Buffer waits**: Times render had to wait for flush (should be 0)
//! - **Render/Flush buffers**: Current buffer indices (0 or 1) - may show same value due to sampling timing
//!
//! # Right Column - Memory & System
//!
//! - **Stack**: Current stack usage (KB) vs total available
//! - **Static**: Static RAM allocation (framebuffers + overhead)
//! - **RAM**: Total RP2350 RAM (512KB)
//! - **CPU**: Clock frequency (150/250/375 MHz based on feature)
//! - **SPI**: Display bus speed (62.5 MHz max)
//! - **FB**: Framebuffer configuration (2×150K for double buffering)
//!
//! # Right Column - CPU Utilization
//!
//! - **Util**: CPU utilization percentage (0-100%, yellow if >80%)
//! - **Cycles**: CPU cycles used per frame (in thousands)

use core::fmt::Write;

use embedded_graphics::mono_font::MonoTextStyle;
use embedded_graphics::prelude::*;
use embedded_graphics::text::Text;
use heapless::String;

use crate::colors::{BLACK, GREEN, WHITE, YELLOW};
use crate::styles::LABEL_FONT;

/// Profiling data to display on the debug screen.
#[derive(Clone, Copy, Default)]
pub struct ProfilingData {
    // Timing
    pub current_fps: f32,
    pub frame_count: u32,
    pub render_time_us: u32,
    pub flush_time_us: u32,
    pub total_frame_time_us: u32,

    // Double buffer stats
    pub buffer_swaps: u32,
    pub buffer_waits: u32,
    pub render_buffer_idx: usize,
    pub flush_buffer_idx: usize,

    // Memory
    pub stack_used_kb: u32,
    pub stack_total_kb: u32,
    pub static_ram_kb: u32,
    pub ram_total_kb: u32,

    // CPU utilization
    pub cpu_util_percent: u32,
    pub frame_cycles: u32,
}

/// Draw the profiling/debug page.
///
/// Shows performance metrics including FPS, render/flush times, buffer stats, and memory.
/// Two-column layout to fit all info on 320x240 screen.
pub fn draw_profiling_page<D>(
    display: &mut D,
    data: &ProfilingData,
) where
    D: DrawTarget<Color = embedded_graphics::pixelcolor::Rgb565>,
{
    let header_style = MonoTextStyle::new(LABEL_FONT, GREEN);
    let value_style = MonoTextStyle::new(LABEL_FONT, WHITE);
    let highlight_style = MonoTextStyle::new(LABEL_FONT, YELLOW);

    // Clear screen
    display.clear(BLACK).ok();

    // Column positions
    let col1 = 4;
    let col2 = 164;
    let line_height = 14;

    // === LEFT COLUMN: Timing ===
    let mut y = 12;

    Text::new("TIMING", Point::new(col1, y), header_style)
        .draw(display)
        .ok();
    y += line_height;

    let mut s: String<24> = String::new();
    let _ = write!(s, "FPS: {:.1}", data.current_fps);
    Text::new(&s, Point::new(col1, y), highlight_style).draw(display).ok();
    y += line_height;

    s.clear();
    let _ = write!(s, "Frame: {}", data.frame_count);
    Text::new(&s, Point::new(col1, y), value_style).draw(display).ok();
    y += line_height;

    s.clear();
    let _ = write!(s, "Render: {} us", data.render_time_us);
    Text::new(&s, Point::new(col1, y), value_style).draw(display).ok();
    y += line_height;

    s.clear();
    let _ = write!(s, "Flush: {} us", data.flush_time_us);
    Text::new(&s, Point::new(col1, y), value_style).draw(display).ok();
    y += line_height;

    s.clear();
    let _ = write!(s, "Total: {} us", data.total_frame_time_us);
    Text::new(&s, Point::new(col1, y), highlight_style).draw(display).ok();
    y += line_height;

    // Max FPS calculation
    let max_fps = if data.total_frame_time_us > 0 {
        1_000_000.0 / data.total_frame_time_us as f32
    } else {
        0.0
    };
    s.clear();
    let _ = write!(s, "Max: {:.0} FPS", max_fps);
    Text::new(&s, Point::new(col1, y), value_style).draw(display).ok();
    y += line_height + 4;

    // === LEFT COLUMN: Double Buffer ===
    Text::new("BUFFERS", Point::new(col1, y), header_style)
        .draw(display)
        .ok();
    y += line_height;

    s.clear();
    let _ = write!(s, "Swaps: {}", data.buffer_swaps);
    Text::new(&s, Point::new(col1, y), value_style).draw(display).ok();
    y += line_height;

    s.clear();
    let _ = write!(s, "Waits: {}", data.buffer_waits);
    // Highlight waits > 0 as yellow (indicates render is slower than flush)
    let waits_style = if data.buffer_waits > 0 {
        highlight_style
    } else {
        value_style
    };
    Text::new(&s, Point::new(col1, y), waits_style).draw(display).ok();
    y += line_height;

    s.clear();
    let _ = write!(s, "Render: buf{}", data.render_buffer_idx);
    Text::new(&s, Point::new(col1, y), value_style).draw(display).ok();
    y += line_height;

    s.clear();
    let _ = write!(s, "Flush:  buf{}", data.flush_buffer_idx);
    Text::new(&s, Point::new(col1, y), value_style).draw(display).ok();

    // === RIGHT COLUMN: Memory ===
    y = 12;

    Text::new("MEMORY", Point::new(col2, y), header_style)
        .draw(display)
        .ok();
    y += line_height;

    s.clear();
    let _ = write!(s, "Stack: {}K/{}K", data.stack_used_kb, data.stack_total_kb);
    Text::new(&s, Point::new(col2, y), value_style).draw(display).ok();
    y += line_height;

    // Stack percentage
    let stack_pct = if data.stack_total_kb > 0 {
        (data.stack_used_kb * 100) / data.stack_total_kb
    } else {
        0
    };
    s.clear();
    let _ = write!(s, "       ({}%)", stack_pct);
    Text::new(&s, Point::new(col2, y), value_style).draw(display).ok();
    y += line_height;

    s.clear();
    let _ = write!(s, "Static: {}K", data.static_ram_kb);
    Text::new(&s, Point::new(col2, y), value_style).draw(display).ok();
    y += line_height;

    s.clear();
    let _ = write!(s, "RAM: {}K total", data.ram_total_kb);
    Text::new(&s, Point::new(col2, y), value_style).draw(display).ok();
    y += line_height + 4;

    // === RIGHT COLUMN: System ===
    Text::new("SYSTEM", Point::new(col2, y), header_style)
        .draw(display)
        .ok();
    y += line_height;

    #[cfg(feature = "turbo-oc")]
    Text::new("CPU: 375 MHz", Point::new(col2, y), value_style)
        .draw(display)
        .ok();

    #[cfg(feature = "overclock")]
    Text::new("CPU: 250 MHz", Point::new(col2, y), value_style)
        .draw(display)
        .ok();

    #[cfg(not(any(feature = "overclock", feature = "turbo-oc")))]
    Text::new("CPU: 150 MHz", Point::new(col2, y), value_style)
        .draw(display)
        .ok();
    y += line_height;

    Text::new("SPI: 62.5 MHz", Point::new(col2, y), value_style)
        .draw(display)
        .ok();
    y += line_height;

    // Framebuffers info
    s.clear();
    let _ = write!(s, "FB: 2x{}K", crate::memory::FRAMEBUFFER_SIZE / 1024);
    Text::new(&s, Point::new(col2, y), value_style).draw(display).ok();
    y += line_height + 4;

    // === RIGHT COLUMN: CPU Utilization ===
    Text::new("CPU UTIL", Point::new(col2, y), header_style)
        .draw(display)
        .ok();
    y += line_height;

    s.clear();
    let _ = write!(s, "Util: {}%", data.cpu_util_percent);
    // Highlight if high utilization (>80%)
    let util_style = if data.cpu_util_percent > 80 {
        highlight_style
    } else {
        value_style
    };
    Text::new(&s, Point::new(col2, y), util_style).draw(display).ok();
    y += line_height;

    s.clear();
    let _ = write!(s, "Cycles: {}K", data.frame_cycles / 1000);
    Text::new(&s, Point::new(col2, y), value_style).draw(display).ok();
    // y is not used after this, suppress warning
    let _ = y;

    // Footer
    Text::new("Press Y for Logs", Point::new(col1, 226), header_style)
        .draw(display)
        .ok();
}
