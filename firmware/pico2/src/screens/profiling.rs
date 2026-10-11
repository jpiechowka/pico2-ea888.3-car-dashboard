use core::fmt::Write;

use dashboard_pico2::display::telemetry::FrameTrace;
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
    pub trace: Option<FrameTrace>,
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
    pub configured_cpu_mhz: u32,

    pub requested_spi_hz: u32,
    pub configured_spi_hz: u32,

    pub requested_voltage_mv: u32,
    pub vreg_setting_mv: Option<u32>,
}

#[allow(clippy::manual_checked_ops)]
pub async fn draw_profiling_page<D>(
    display: &mut D,
    data: &ProfilingData,
    work: &mut crate::profiling::RenderWork,
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

    Text::new("SOFTWARE TIMING", Point::new(col1, y), header_style)
        .draw(display)
        .ok();
    work.checkpoint().await;
    y += line_height;

    let mut s: String<24> = String::new();
    let _ = write!(s, "Done FPS: {:.1}", data.current_fps);
    Text::new(&s, Point::new(col1, y), highlight_style).draw(display).ok();
    work.checkpoint().await;
    y += line_height;

    s.clear();
    let _ = write!(s, "Avg. FPS: {:.1}", data.average_fps);
    Text::new(&s, Point::new(col1, y), highlight_style).draw(display).ok();
    work.checkpoint().await;
    y += line_height;

    s.clear();
    let _ = write!(s, "Frame: {}", data.frame_count);
    Text::new(&s, Point::new(col1, y), value_style).draw(display).ok();
    work.checkpoint().await;
    y += line_height;

    s.clear();
    let _ = write!(s, "Render: {} us", data.trace.map_or(0, |t| t.render_us()));
    Text::new(&s, Point::new(col1, y), value_style).draw(display).ok();
    work.checkpoint().await;
    y += line_height;

    s.clear();
    let _ = write!(s, "Flush*: {} us", data.trace.map_or(0, |t| t.transfer_observed_us()));
    Text::new(&s, Point::new(col1, y), value_style).draw(display).ok();
    work.checkpoint().await;
    y += line_height;

    s.clear();
    let _ = write!(s, "Frame: {} us", data.trace.map_or(0, |t| t.total_us()));
    Text::new(&s, Point::new(col1, y), highlight_style).draw(display).ok();
    work.checkpoint().await;
    y += line_height;

    s.clear();
    match data.trace.and_then(|t| t.sample_age_us()) {
        Some(age) => {
            let _ = write!(s, "Age*: {} ms", age / 1000);
        }
        None => {
            let _ = write!(s, "Age*: --");
        }
    }
    Text::new(&s, Point::new(col1, y), value_style).draw(display).ok();
    work.checkpoint().await;
    y += line_height + 4;

    Text::new("BUFFERS", Point::new(col1, y), header_style)
        .draw(display)
        .ok();
    work.checkpoint().await;
    y += line_height;

    s.clear();
    let _ = write!(s, "Swaps: {}", data.buffer_swaps);
    Text::new(&s, Point::new(col1, y), value_style).draw(display).ok();
    work.checkpoint().await;
    y += line_height;

    s.clear();
    let _ = write!(s, "Waits: {}", data.buffer_waits);
    let waits_style = if data.buffer_waits > 0 {
        highlight_style
    } else {
        value_style
    };
    Text::new(&s, Point::new(col1, y), waits_style).draw(display).ok();
    work.checkpoint().await;
    y += line_height;

    s.clear();
    let _ = write!(s, "Render: buf{}", data.render_buffer_idx);
    Text::new(&s, Point::new(col1, y), value_style).draw(display).ok();
    work.checkpoint().await;
    y += line_height;

    s.clear();
    let _ = write!(s, "Flush:  buf{}", data.flush_buffer_idx);
    Text::new(&s, Point::new(col1, y), value_style).draw(display).ok();

    y = 12;

    Text::new("CORE 0", Point::new(col2, y), header_style)
        .draw(display)
        .ok();
    work.checkpoint().await;
    y += line_height;

    s.clear();
    let _ = write!(
        s,
        "UI work: {}.{}%",
        data.cpu0_util_percent / 10,
        data.cpu0_util_percent % 10
    );
    let util0_style = if data.cpu0_util_percent > 850 {
        highlight_style
    } else {
        value_style
    };
    Text::new(&s, Point::new(col2, y), util0_style).draw(display).ok();
    work.checkpoint().await;
    y += line_height;

    s.clear();
    let _ = write!(s, "Cycles: {}K", data.frame_cycles / 1000);
    Text::new(&s, Point::new(col2, y), value_style).draw(display).ok();
    work.checkpoint().await;
    y += line_height;

    s.clear();
    let _ = write!(
        s,
        "SP now: {}K/{}K",
        data.core0_stack_used_kb, data.core0_stack_total_kb
    );
    Text::new(&s, Point::new(col2, y), value_style).draw(display).ok();
    work.checkpoint().await;
    y += line_height + 4;

    Text::new("CORE 1", Point::new(col2, y), header_style)
        .draw(display)
        .ok();
    work.checkpoint().await;
    y += line_height;

    s.clear();
    let _ = write!(
        s,
        "Acq work: {}.{}%",
        data.cpu1_util_percent / 10,
        data.cpu1_util_percent % 10
    );
    let util1_style = if data.cpu1_util_percent > 850 {
        highlight_style
    } else {
        value_style
    };
    Text::new(&s, Point::new(col2, y), util1_style).draw(display).ok();
    work.checkpoint().await;
    y += line_height;

    s.clear();
    let _ = write!(
        s,
        "SP seen: {}K/{}K",
        data.core1_stack_used_kb, data.core1_stack_total_kb
    );
    Text::new(&s, Point::new(col2, y), value_style).draw(display).ok();
    work.checkpoint().await;
    y += line_height + 4;

    Text::new("SYSTEM", Point::new(col2, y), header_style)
        .draw(display)
        .ok();
    work.checkpoint().await;
    y += line_height;

    s.clear();
    let _ = write!(s, "CPU:{}/{}M", data.configured_cpu_mhz, data.requested_cpu_mhz);
    Text::new(&s, Point::new(col2, y), value_style).draw(display).ok();
    work.checkpoint().await;
    y += line_height;

    s.clear();
    match data.vreg_setting_mv {
        Some(mv) => {
            let _ = write!(s, "Vset:{}/{}mV", mv, data.requested_voltage_mv);
        }
        None => {
            let _ = write!(s, "Vset:--/{}mV", data.requested_voltage_mv);
        }
    }
    Text::new(&s, Point::new(col2, y), value_style).draw(display).ok();
    work.checkpoint().await;
    y += line_height;

    s.clear();
    let _ = write!(
        s,
        "SPI:{:.1}/{:.1}M",
        data.configured_spi_hz as f32 / 1_000_000.0,
        data.requested_spi_hz as f32 / 1_000_000.0
    );
    Text::new(&s, Point::new(col2, y), value_style).draw(display).ok();
    work.checkpoint().await;
    y += line_height;

    s.clear();
    let _ = write!(s, "RAM: {}K/{}K", data.static_ram_kb, data.ram_total_kb);
    Text::new(&s, Point::new(col2, y), value_style).draw(display).ok();
    work.checkpoint().await;
    y += line_height;

    s.clear();
    let _ = write!(s, "FB: 2x{}K", crate::profiling::FRAMEBUFFER_SIZE / 1024);
    Text::new(&s, Point::new(col2, y), value_style).draw(display).ok();
    work.checkpoint().await;
    y += line_height;

    s.clear();
    if data.brightness_percent == 0 {
        let _ = write!(s, "BL: OFF");
    } else {
        let _ = write!(s, "BL: {}%", data.brightness_percent);
    }
    Text::new(&s, Point::new(col2, y), value_style).draw(display).ok();
    let _ = y;

    Text::new("*HAL return, not scanout; Y: Logs", Point::new(col1, 226), header_style)
        .draw(display)
        .ok();
}
