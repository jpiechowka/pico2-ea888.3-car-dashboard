//! Profiling/debug screen for performance analysis.
//!
//! Displays comprehensive system metrics in a two-column layout optimized for 320x240:
//!
//! # Left Column - Timing & Buffers
//!
//! - **FPS**: Current (instantaneous) frames per second (highlighted in yellow)
//! - **Avg**: Average FPS since last reset (highlighted in yellow)
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
//! - **CPU**: Clock frequency requested/actual MHz (yellow if mismatch)
//! - **Volt**: Core voltage requested/actual (yellow if mismatch)
//! - **SPI**: Display bus speed (requested/actual MHz from hardware)
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
    pub average_fps: f32,
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

    // CPU frequency (MHz)
    pub requested_cpu_mhz: u32,
    pub actual_cpu_mhz: u32,

    // SPI frequency (MHz)
    pub requested_spi_mhz: u32,
    pub actual_spi_mhz: u32,

    // Voltage (millivolts, e.g., 1100 = 1.10V)
    pub requested_voltage_mv: u32,
    pub actual_voltage_mv: u32,
}

/// Draw the profiling/debug page.
///
/// Shows performance metrics including FPS, render/flush times, buffer stats, and memory.
/// Two-column layout to fit all info on 320x240 screen.
#[allow(clippy::manual_checked_ops)] // Explicit zero-check is clearer for embedded
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
    let _ = write!(s, "Avg: {:.1}", data.average_fps);
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

    // CPU frequency display: requested / actual (from DWT init)
    s.clear();
    if data.actual_cpu_mhz > 0 && data.actual_cpu_mhz != data.requested_cpu_mhz {
        let _ = write!(s, "CPU: {}/{} MHz", data.requested_cpu_mhz, data.actual_cpu_mhz);
    } else {
        let _ = write!(s, "CPU: {} MHz", data.requested_cpu_mhz);
    }
    // Highlight if mismatch
    let cpu_style = if data.actual_cpu_mhz != data.requested_cpu_mhz {
        highlight_style
    } else {
        value_style
    };
    Text::new(&s, Point::new(col2, y), cpu_style).draw(display).ok();
    y += line_height;

    // Voltage display: requested / actual (from VREG hardware)
    s.clear();
    let req_v = data.requested_voltage_mv / 100; // e.g., 1100 -> 11 (for 1.1)
    let act_v = data.actual_voltage_mv / 100;
    let _ = write!(s, "Volt: {}.{}/{}.{}V", req_v / 10, req_v % 10, act_v / 10, act_v % 10);
    // Highlight if actual >= 1.40V or mismatch
    // Highlight if mismatch
    let volt_style = if data.actual_voltage_mv != data.requested_voltage_mv {
        highlight_style
    } else {
        value_style
    };
    Text::new(&s, Point::new(col2, y), volt_style).draw(display).ok();
    y += line_height;

    // SPI frequency display (requested / actual from hardware)
    s.clear();
    if data.actual_spi_mhz > 0 {
        let _ = write!(s, "SPI: {}/{} MHz", data.requested_spi_mhz, data.actual_spi_mhz);
    } else {
        let _ = write!(s, "SPI: {} MHz", data.requested_spi_mhz);
    }
    Text::new(&s, Point::new(col2, y), value_style).draw(display).ok();
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
