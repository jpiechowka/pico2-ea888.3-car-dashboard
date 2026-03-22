use core::fmt::Write;

use embedded_graphics::mono_font::MonoTextStyle;
use embedded_graphics::prelude::*;
use embedded_graphics::text::Text;
use heapless::String;

use crate::ui::{BLACK, GREEN, LABEL_FONT, WHITE, YELLOW};

#[derive(Clone, Copy, Default)]
pub struct ProfilingData {
    pub current_fps: f32,
    pub average_fps: f32,
    pub frame_count: u32,
    pub render_time_us: u32,
    pub flush_time_us: u32,
    pub total_frame_time_us: u32,
    pub brightness_percent: u32,

    pub buffer_swaps: u32,
    pub buffer_waits: u32,
    pub render_buffer_idx: usize,
    pub flush_buffer_idx: usize,

    pub core0_stack_used_kb: u32,
    pub core0_stack_total_kb: u32,
    pub core1_stack_used_kb: u32,
    pub core1_stack_total_kb: u32,
    pub static_ram_kb: u32,
    pub ram_total_kb: u32,

    pub cpu0_util_percent: u32,
    pub cpu1_util_percent: u32,
    pub frame_cycles: u32,

    pub requested_cpu_mhz: u32,
    pub actual_cpu_mhz: u32,

    pub requested_spi_mhz: u32,
    pub actual_spi_mhz: u32,

    pub requested_voltage_mv: u32,
    pub actual_voltage_mv: u32,
}

#[allow(clippy::manual_checked_ops)]
pub fn draw_profiling_page<D>(
    display: &mut D,
    data: &ProfilingData,
) where
    D: DrawTarget<Color = embedded_graphics::pixelcolor::Rgb565>,
{
    let header_style = MonoTextStyle::new(LABEL_FONT, GREEN);
    let value_style = MonoTextStyle::new(LABEL_FONT, WHITE);
    let highlight_style = MonoTextStyle::new(LABEL_FONT, YELLOW);

    display.clear(BLACK).ok();

    let col1 = 4;
    let col2 = 164;
    let line_height = 14;

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
    let _ = write!(s, "Avg. FPS: {:.1}", data.average_fps);
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

    let max_fps = if data.total_frame_time_us > 0 {
        1_000_000.0 / data.total_frame_time_us as f32
    } else {
        0.0
    };
    s.clear();
    let _ = write!(s, "Max: {:.0} FPS", max_fps);
    Text::new(&s, Point::new(col1, y), value_style).draw(display).ok();
    y += line_height + 4;

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

    y = 12;

    Text::new("CORE 0", Point::new(col2, y), header_style)
        .draw(display)
        .ok();
    y += line_height;

    s.clear();
    let _ = write!(
        s,
        "Util: {}.{}%",
        data.cpu0_util_percent / 10,
        data.cpu0_util_percent % 10
    );
    let util0_style = if data.cpu0_util_percent > 850 {
        highlight_style
    } else {
        value_style
    };
    Text::new(&s, Point::new(col2, y), util0_style).draw(display).ok();
    y += line_height;

    s.clear();
    let _ = write!(s, "Cycles: {}K", data.frame_cycles / 1000);
    Text::new(&s, Point::new(col2, y), value_style).draw(display).ok();
    y += line_height;

    s.clear();
    let _ = write!(s, "Stack: {}K/{}K", data.core0_stack_used_kb, data.core0_stack_total_kb);
    Text::new(&s, Point::new(col2, y), value_style).draw(display).ok();
    y += line_height + 4;

    Text::new("CORE 1", Point::new(col2, y), header_style)
        .draw(display)
        .ok();
    y += line_height;

    s.clear();
    let _ = write!(
        s,
        "Util: {}.{}%",
        data.cpu1_util_percent / 10,
        data.cpu1_util_percent % 10
    );
    let util1_style = if data.cpu1_util_percent > 850 {
        highlight_style
    } else {
        value_style
    };
    Text::new(&s, Point::new(col2, y), util1_style).draw(display).ok();
    y += line_height;

    s.clear();
    let _ = write!(s, "Stack: {}K/{}K", data.core1_stack_used_kb, data.core1_stack_total_kb);
    Text::new(&s, Point::new(col2, y), value_style).draw(display).ok();
    y += line_height + 4;

    Text::new("SYSTEM", Point::new(col2, y), header_style)
        .draw(display)
        .ok();
    y += line_height;

    s.clear();
    let _ = write!(s, "CPU: {} MHz", data.requested_cpu_mhz);
    Text::new(&s, Point::new(col2, y), value_style).draw(display).ok();
    y += line_height;

    s.clear();
    let act_v = data.actual_voltage_mv / 100;
    let _ = write!(s, "Volt: {}.{}V", act_v / 10, act_v % 10);
    Text::new(&s, Point::new(col2, y), value_style).draw(display).ok();
    y += line_height;

    s.clear();
    let _ = write!(s, "SPI: {} MHz", data.requested_spi_mhz);
    Text::new(&s, Point::new(col2, y), value_style).draw(display).ok();
    y += line_height;

    s.clear();
    let _ = write!(s, "RAM: {}K/{}K", data.static_ram_kb, data.ram_total_kb);
    Text::new(&s, Point::new(col2, y), value_style).draw(display).ok();
    y += line_height;

    s.clear();
    let _ = write!(s, "FB: 2x{}K", crate::profiling::FRAMEBUFFER_SIZE / 1024);
    Text::new(&s, Point::new(col2, y), value_style).draw(display).ok();
    y += line_height;

    s.clear();
    if data.brightness_percent == 0 {
        let _ = write!(s, "BL: OFF");
    } else {
        let _ = write!(s, "BL: {}%", data.brightness_percent);
    }
    Text::new(&s, Point::new(col2, y), value_style).draw(display).ok();
    let _ = y;

    Text::new("Press Y for Logs", Point::new(col1, 226), header_style)
        .draw(display)
        .ok();
}
