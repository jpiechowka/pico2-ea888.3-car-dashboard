//! Hardware diagnostic snapshot adapted to the shared Debug renderer.
use dashboard_pico2::debug_view::{DebugData, DebugLine, DebugPage};
use dashboard_pico2::display::telemetry::FrameTrace;
use embedded_graphics::prelude::*;
#[derive(Clone, Copy, Default)]
/// Immutable producer snapshot; all optional observations preserve unavailability.
pub struct ProfilingData {
    /// Conservative boot-evidence label; ambiguous reset evidence stays explicit.
    pub reset_cause: &'static str,
    /// Source/link status label, never a certification of ECU support.
    pub vehicle_status: &'static str,
    /// Whether boot configuration fell back after admission failure.
    pub config_fault: bool,
    /// Recent successful software-transfer rate; None is unavailable, not zero.
    pub current_fps: Option<f32>,
    /// Completed transfer rate since the current page epoch, or unavailable.
    pub average_fps: Option<f32>,
    /// Wrapping successful transfer count since boot, independent of page FPS epochs.
    pub frame_count: u32,
    /// Last matching completed transfer timing, or no observation.
    pub trace: Option<FrameTrace>,
    /// Current runtime backlight state used by the shared control footer.
    pub brightness: dashboard_pico2::backlight::Brightness,
    /// Journal state label such as SAVED, UNSAVED or VOLATILE.
    pub settings_status: &'static str,

    /// Wrapping submitted-frame count; not panel scanout count.
    pub buffer_swaps: u32,
    /// Wrapping count of completion receives that had to await progress.
    pub buffer_waits: u32,
    /// Current renderer-owned framebuffer index.
    pub render_buffer_idx: usize,
    /// DMA-owner framebuffer index, or None when no transfer is outstanding.
    pub flush_buffer_idx: Option<usize>,

    /// Core0 instantaneous sampled stack depth in integer KiB, or unavailable.
    pub core0_stack_used_kb: Option<u32>,
    /// Core0 linker-derived stack-plus-headroom region in integer KiB.
    pub core0_stack_total_kb: u32,
    /// Retained core1 sampled stack depth in integer KiB, or unavailable.
    pub core1_stack_used_kb: Option<u32>,
    /// Fixed core1 stack allocation in integer KiB.
    pub core1_stack_total_kb: u32,
    /// Linked static main-SRAM occupancy in integer KiB.
    pub static_ram_kb: u32,
    /// Conservative admitted main-SRAM size in integer KiB.
    pub ram_total_kb: u32,

    /// Core0 instrumented work fraction in tenths of percent, or unavailable.
    pub cpu0_util_percent: Option<u32>,
    /// Core1 instrumented work fraction in tenths of percent, or unavailable.
    pub cpu1_util_percent: Option<u32>,
    /// Instrumented render cycles for a completed observation, or unavailable.
    pub frame_cycles: Option<u32>,

    /// Build-profile requested CPU frequency in hertz.
    pub requested_cpu_hz: u32,
    /// PLL-derived configured CPU frequency in hertz; not measured electrically.
    pub configured_cpu_hz: u32,

    /// Build-profile requested display SPI frequency in hertz.
    pub requested_spi_hz: u32,
    /// Register/divider-derived SPI frequency in hertz, or invalid/unavailable.
    pub configured_spi_hz: Option<u32>,

    /// Build-profile core regulator request in millivolts.
    pub requested_voltage_mv: u32,
    /// Decoded regulator setting in millivolts, or unavailable; not measured voltage.
    pub vreg_setting_mv: Option<u32>,
}

impl<'a> From<&'a ProfilingData> for DebugData<'a> {
    /// Returns the shared Debug view of `data`, retaining missing observations and
    /// supplying the fixed two-buffer metadata without sampling live producers.
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
            configured_spi_hz: data.configured_spi_hz,
            requested_voltage_mv: Some(data.requested_voltage_mv),
            vreg_setting_mv: data.vreg_setting_mv,
            framebuffer_count: Some(2),
            framebuffer_bytes: Some(crate::profiling::FRAMEBUFFER_SIZE as u32),
        }
    }
}

/// Draws immutable `data` into already-cleared `display` using the current control
/// `map`; `work` excludes each cooperative yield between shared Debug lines.
/// No producer lock survives a yield. Cancellation leaves an incomplete scene,
/// which the caller must discard instead of submitting for display.
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
