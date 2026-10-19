use dashboard_pico2::debug_view::{DebugData, DebugLine, DebugPage};
use dashboard_pico2::display::telemetry::FrameTrace;
use embedded_graphics::prelude::*;
#[derive(Clone, Copy, Default)]
pub struct ProfilingData {
    pub reset_cause: &'static str,
    pub vehicle_status: &'static str,
    pub config_fault: bool,
    pub current_fps: Option<f32>,
    pub average_fps: Option<f32>,
    pub frame_count: u32,
    pub trace: Option<FrameTrace>,
    pub brightness: dashboard_pico2::backlight::Brightness,
    pub settings_status: &'static str,

    pub buffer_swaps: u32,
    pub buffer_waits: u32,
    pub render_buffer_idx: usize,
    pub flush_buffer_idx: Option<usize>,

    pub core0_stack_used_kb: Option<u32>,
    pub core0_stack_total_kb: u32,
    pub core1_stack_used_kb: Option<u32>,
    pub core1_stack_total_kb: u32,
    pub static_ram_kb: u32,
    pub ram_total_kb: u32,

    pub cpu0_util_percent: Option<u32>,
    pub cpu1_util_percent: Option<u32>,
    pub frame_cycles: Option<u32>,

    pub requested_cpu_hz: u32,
    pub configured_cpu_hz: u32,

    pub requested_spi_hz: u32,
    pub configured_spi_hz: u32,

    pub requested_voltage_mv: u32,
    pub vreg_setting_mv: Option<u32>,
}

impl<'a> From<&'a ProfilingData> for DebugData<'a> {
    fn from(data: &'a ProfilingData) -> Self {
        Self {
            reset_cause: Some(data.reset_cause),
            source_status: Some(data.vehicle_status),
            config_fault: Some(data.config_fault),
            config_fault_label: None,
            current_fps: data.current_fps,
            average_fps: data.average_fps,
            frame_count: Some(data.frame_count),
            trace: data.trace,
            brightness: Some(data.brightness),
            settings_status: Some(data.settings_status),
            buffer_swaps: Some(data.buffer_swaps),
            buffer_waits: Some(data.buffer_waits),
            render_buffer_idx: Some(data.render_buffer_idx),
            flush_buffer_idx: data.flush_buffer_idx,
            core0_stack_used_kb: data.core0_stack_used_kb,
            core0_stack_total_kb: Some(data.core0_stack_total_kb),
            core1_stack_used_kb: data.core1_stack_used_kb,
            core1_stack_total_kb: Some(data.core1_stack_total_kb),
            static_ram_kb: Some(data.static_ram_kb),
            ram_total_kb: Some(data.ram_total_kb),
            cpu0_util_percent: data.cpu0_util_percent,
            cpu1_util_percent: data.cpu1_util_percent,
            frame_cycles: data.frame_cycles,
            requested_cpu_hz: Some(data.requested_cpu_hz),
            configured_cpu_hz: Some(data.configured_cpu_hz),
            requested_spi_hz: Some(data.requested_spi_hz),
            configured_spi_hz: Some(data.configured_spi_hz),
            requested_voltage_mv: Some(data.requested_voltage_mv),
            vreg_setting_mv: data.vreg_setting_mv,
            framebuffer_count: Some(2),
            framebuffer_bytes: Some(crate::profiling::FRAMEBUFFER_SIZE as u32),
        }
    }
}

pub async fn draw_profiling_page<D>(
    display: &mut D,
    data: &ProfilingData,
    map: &dashboard_pico2::view_config::ControlMap,
    work: &mut crate::profiling::RenderWork,
) where
    D: DrawTarget<Color = embedded_graphics::pixelcolor::Rgb565>,
{
    // main cleared/checkpointed this buffer and captured all producer values.
    // Only the immutable small snapshot survives; row formatting/target borrows
    // end synchronously before every yield. No lock or owner is held here.
    let snapshot = DebugData::from(data);
    let page = DebugPage::new(&snapshot, map);
    for line in DebugLine::ALL {
        page.draw_line(display, line);
        work.checkpoint().await;
    }
}
