//! Bounded diagnostic drawing shared by firmware and synthetic previews.
//! Callers capture measurements before drawing; this module owns no producer.
use core::fmt::{self, Write};

use dashboard_config::{ControlMap, ControlPage};
use embedded_graphics::mono_font::MonoTextStyle;
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::Rectangle;
use embedded_graphics::text::Text;
use heapless::String;

use crate::backlight::Brightness;
use crate::display::telemetry::FrameTrace;
use crate::ui::{BLACK, GREEN, LABEL_FONT, WHITE, YELLOW};

/// None is unavailable, not a measured zero. CPU frequencies are exact Hz
/// from configuration/register-derived metadata, not physical measurements.
#[derive(Clone, Copy, Default)]
pub struct DebugData<'a> {
    /// Captured reset classification label, not a proven root cause.
    pub reset_cause: Option<&'a str>,
    /// Already derived acquisition/adapter status label.
    pub source_status: Option<&'a str>,
    /// Configuration fallback/fault flag; absent shows unknown, false clears the row.
    pub config_fault: Option<bool>,
    /// Optional specific label used only when `config_fault` is true.
    pub config_fault_label: Option<&'a str>,
    /// Latest completed-transfer rate per second; invalid/negative values display unavailable.
    pub current_fps: Option<f32>,
    /// Window-average completed-transfer rate per second; not physical scanout FPS.
    pub average_fps: Option<f32>,
    /// Completed software frame count from the adapter's instrumentation.
    pub frame_count: Option<u32>,
    /// Coherent completed frame timing/cycle/source-age observation.
    pub trace: Option<FrameTrace>,
    /// Current requested brightness state, separate from measured panel luminance.
    pub brightness: Option<Brightness>,
    /// Captured persistence state label, such as saved/default/pending.
    pub settings_status: Option<&'a str>,
    /// Instrumented render/flush ownership swaps.
    pub buffer_swaps: Option<u32>,
    /// Instrumented waits for an in-flight transfer, not necessarily faults.
    pub buffer_waits: Option<u32>,
    /// Current render slot0/1; other values display unavailable.
    pub render_buffer_idx: Option<usize>,
    /// Current flush slot0/1; other values display unavailable.
    pub flush_buffer_idx: Option<usize>,
    /// Core0 current stack-pointer usage sample in KiB, not continuous high-water.
    pub core0_stack_used_kb: Option<u32>,
    /// Core0 configured stack capacity in KiB; displayed only in `1..=512`.
    pub core0_stack_total_kb: Option<u32>,
    /// Core1 retained observed stack-pointer usage in KiB, not continuous high-water.
    pub core1_stack_used_kb: Option<u32>,
    /// Core1 configured stack capacity in KiB; displayed only in `1..=512`.
    pub core1_stack_total_kb: Option<u32>,
    /// Linked/static RAM accounting in KiB, not dynamic allocation high-water.
    pub static_ram_kb: Option<u32>,
    /// RAM capacity in KiB; invalid/zero/>512 capacity becomes unavailable.
    pub ram_total_kb: Option<u32>,
    /// Tenths of percent, retaining the existing firmware producer units.
    pub cpu0_util_percent: Option<u32>,
    /// Instrumented acquisition work in tenths of percent `0..=1000`, not whole-core utilization.
    pub cpu1_util_percent: Option<u32>,
    /// Observed frame-work cycles, displayed as truncated decimal thousands.
    pub frame_cycles: Option<u32>,
    /// Requested CPU clock in Hz, displayed after the configured value.
    pub requested_cpu_hz: Option<u32>,
    /// Configuration/register-derived CPU clock in Hz, preserving all fractional MHz digits.
    pub configured_cpu_hz: Option<u32>,
    /// Requested SPI clock in Hz, displayed rounded to tenths of MHz.
    pub requested_spi_hz: Option<u32>,
    /// Divider-derived SPI clock in Hz; missing divider evidence remains unavailable.
    pub configured_spi_hz: Option<u32>,
    /// Requested core regulator voltage in millivolts.
    pub requested_voltage_mv: Option<u32>,
    /// Register-derived regulator setting in millivolts, not a measured voltage.
    pub vreg_setting_mv: Option<u32>,
    /// Caller-reported number of allocated framebuffers.
    pub framebuffer_count: Option<u32>,
    /// Bytes per framebuffer, displayed as truncated KiB.
    pub framebuffer_bytes: Option<u32>,
}

/// One synchronous row operation. Firmware yields after each member of ALL.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DebugLine {
    /// Software timing section heading.
    TimingHeader,
    /// Current completed-transfer FPS.
    DoneFps,
    /// Averaged completed-transfer FPS.
    AverageFps,
    /// Completed frame count.
    CompletedFrames,
    /// Completed trace's render interval in microseconds.
    RenderTime,
    /// Observed flush interval through HAL return in microseconds.
    TransferTime,
    /// Trace's end-to-end frame interval in microseconds.
    FrameTime,
    /// Sample age at completion, truncated to milliseconds when available.
    SampleAge,
    /// Buffer ownership section heading.
    BuffersHeader,
    /// Instrumented buffer swaps.
    Swaps,
    /// Pipeline wait count without treating ordinary backpressure as failure.
    Waits,
    /// Current render slot0/1 or unavailable.
    RenderBuffer,
    /// Current flush slot0/1 or unavailable.
    FlushBuffer,
    /// UI core section heading.
    Core0Header,
    /// Instrumented UI work percentage, not whole-core utilization.
    Core0Utilization,
    /// Observed frame CPU cycles in decimal thousands.
    FrameCycles,
    /// Current core0 stack-pointer sample/capacity in KiB.
    Core0Stack,
    /// Acquisition core section heading.
    Core1Header,
    /// Instrumented acquisition work percentage.
    Core1Utilization,
    /// Retained core1 stack-pointer sample/capacity in KiB.
    Core1Stack,
    /// Configuration and memory section heading.
    SystemHeader,
    /// Configured/requested CPU MHz with exact fractional digits.
    CpuClock,
    /// Register-setting/requested regulator millivolts.
    VregSetting,
    /// Divider-derived/requested SPI MHz rounded to tenths.
    SpiClock,
    /// Static RAM accounting/capacity in KiB.
    Ram,
    /// Framebuffer count and per-buffer KiB.
    Framebuffers,
    /// Brightness and preference persistence status.
    BrightnessSettings,
    /// Captured reset classification label.
    ResetCause,
    /// Captured source status label.
    SourceStatus,
    /// Configuration fault/fallback label or blank when healthy admission is known.
    ConfigFault,
    /// Explicit HAL-return versus physical-scanout qualification.
    TransferNote,
    /// Current Debug-to-Logs navigation route and protected recovery hint.
    ControlsHint,
}

impl DebugLine {
    /// Fixed32-operation order; portrait lays these vertically and firmware can yield between operations.
    pub const ALL: [Self; 32] = [
        Self::TimingHeader,
        Self::DoneFps,
        Self::AverageFps,
        Self::CompletedFrames,
        Self::RenderTime,
        Self::TransferTime,
        Self::FrameTime,
        Self::SampleAge,
        Self::BuffersHeader,
        Self::Swaps,
        Self::Waits,
        Self::RenderBuffer,
        Self::FlushBuffer,
        Self::Core0Header,
        Self::Core0Utilization,
        Self::FrameCycles,
        Self::Core0Stack,
        Self::Core1Header,
        Self::Core1Utilization,
        Self::Core1Stack,
        Self::SystemHeader,
        Self::CpuClock,
        Self::VregSetting,
        Self::SpiClock,
        Self::Ram,
        Self::Framebuffers,
        Self::BrightnessSettings,
        Self::ResetCause,
        Self::SourceStatus,
        Self::ConfigFault,
        Self::TransferNote,
        Self::ControlsHint,
    ];

    /// Returns landscape x/baseline-y/row-width pixels, with two full-width footer rows.
    fn position(self) -> (i32, i32, u32) {
        use DebugLine::*;
        let (x, y) = match self {
            TimingHeader => (4, 12),
            DoneFps => (4, 26),
            AverageFps => (4, 40),
            CompletedFrames => (4, 54),
            RenderTime => (4, 68),
            TransferTime => (4, 82),
            FrameTime => (4, 96),
            SampleAge => (4, 110),
            BuffersHeader => (4, 128),
            Swaps => (4, 142),
            Waits => (4, 156),
            RenderBuffer => (4, 170),
            FlushBuffer => (4, 184),
            Core0Header => (164, 12),
            Core0Utilization => (164, 26),
            FrameCycles => (164, 40),
            Core0Stack => (164, 54),
            Core1Header => (164, 72),
            Core1Utilization => (164, 86),
            Core1Stack => (164, 100),
            SystemHeader => (164, 118),
            CpuClock => (164, 132),
            VregSetting => (164, 146),
            SpiClock => (164, 160),
            Ram => (164, 174),
            Framebuffers => (164, 188),
            BrightnessSettings => (164, 202),
            ResetCause => (4, 198),
            SourceStatus => (4, 212),
            ConfigFault => (164, 212),
            TransferNote => (4, 226),
            ControlsHint => (4, 236),
        };
        (
            x,
            y,
            if matches!(self, TransferNote | ControlsHint) {
                316
            } else {
                156
            },
        )
    }
}

/// Borrowed immutable capture and controls for synchronous/resumable diagnostic drawing.
pub struct DebugPage<'a> {
    /// Already captured values; rendering acquires no producer/measurement locks.
    data: &'a DebugData<'a>,
    /// Accepted controls used to derive navigation hints at draw time.
    map: &'a ControlMap,
}

impl<'a> DebugPage<'a> {
    /// Returns a page borrowing captured `data` and accepted control `map`; neither is copied or mutated.
    pub const fn new(
        data: &'a DebugData<'a>,
        map: &'a ControlMap,
    ) -> Self {
        Self { data, map }
    }

    /// Host/browser convenience; firmware uses draw_line and cooperative yields.
    /// Clears native geometry on `display` then draws all32 rows. Target errors are
    /// discarded; no measurement acquisition or physical transfer occurs here.
    pub fn draw<D: DrawTarget<Color = Rgb565>>(
        &self,
        display: &mut D,
    ) {
        let geometry = crate::configured_scene::target_geometry(display);
        display
            .clipped(&Rectangle::new(
                Point::zero(),
                Size::new(u32::from(geometry.width()), u32::from(geometry.height())),
            ))
            .clear(BLACK)
            .ok();
        for line in DebugLine::ALL {
            self.draw_line(display, line);
        }
    }

    /// Clears and draws one `line` on `display`, clipping to its reserved row.
    /// Exact240x320 uses compact portrait rows; other geometry uses landscape lanes.
    /// Invalid work/rate/buffer/memory observations remain unavailable; long labels
    /// are bounded UTF-8 prefixes with controls replaced by `?`. Target errors are discarded.
    pub fn draw_line<D: DrawTarget<Color = Rgb565>>(
        &self,
        display: &mut D,
        line: DebugLine,
    ) {
        use DebugLine::*;
        let portrait = crate::configured_scene::target_geometry(display) == dashboard_config::Geometry::Portrait;
        let (x, y, width) = if portrait {
            let index = DebugLine::ALL.iter().position(|v| *v == line).unwrap_or(0);
            (4, 9 + index as i32 * 9, 232)
        } else {
            line.position()
        };
        let mut text = LineText::new((width / if portrait { 4 } else { 6 }) as usize);
        let data = self.data;
        let color = match line {
            TimingHeader | BuffersHeader | Core0Header | Core1Header | SystemHeader | TransferNote | ControlsHint => {
                GREEN
            }
            DoneFps | AverageFps | FrameTime | SourceStatus | ConfigFault => YELLOW,
            Core0Utilization if data.cpu0_util_percent.is_some_and(|v| (851..=1000).contains(&v)) => YELLOW,
            Core1Utilization if data.cpu1_util_percent.is_some_and(|v| (851..=1000).contains(&v)) => YELLOW,
            _ => WHITE,
        };
        match line {
            TimingHeader => {
                let _ = text.write_str("SOFTWARE TIMING");
            }
            DoneFps => fps(&mut text, "Done FPS", data.current_fps),
            AverageFps => fps(&mut text, "Avg. FPS", data.average_fps),
            CompletedFrames => {
                let _ = write!(text, "Frame: {}", Available(data.frame_count));
            }
            RenderTime => duration(&mut text, "Render", data.trace.map(|t| t.render_us()), "us"),
            TransferTime => duration(&mut text, "Flush*", data.trace.map(|t| t.transfer_observed_us()), "us"),
            FrameTime => duration(&mut text, "Frame", data.trace.map(|t| t.total_us()), "us"),
            SampleAge => {
                duration(
                    &mut text,
                    "Age*",
                    data.trace.and_then(|t| t.sample_age_us()).map(|v| v / 1000),
                    "ms",
                )
            }
            BuffersHeader => {
                let _ = text.write_str("BUFFERS");
            }
            Swaps => {
                let _ = write!(text, "Swaps: {}", Available(data.buffer_swaps));
            }
            Waits => {
                let _ = write!(text, "Waits: {}", Available(data.buffer_waits));
            }
            RenderBuffer => buffer(&mut text, "Render: ", data.render_buffer_idx),
            FlushBuffer => buffer(&mut text, "Flush:  ", data.flush_buffer_idx),
            Core0Header => {
                let _ = text.write_str("CORE 0");
            }
            Core0Utilization => util(&mut text, "UI work", data.cpu0_util_percent),
            FrameCycles => {
                let _ = write!(text, "Cycles: {}K", Available(data.frame_cycles.map(|v| v / 1000)));
            }
            Core0Stack => {
                let (used, total) = memory_pair(data.core0_stack_used_kb, data.core0_stack_total_kb);
                let _ = write!(text, "SP now: {}K/{}K", Available(used), Available(total));
            }
            Core1Header => {
                let _ = text.write_str("CORE 1");
            }
            Core1Utilization => util(&mut text, "Acq work", data.cpu1_util_percent),
            Core1Stack => {
                let (used, total) = memory_pair(data.core1_stack_used_kb, data.core1_stack_total_kb);
                let _ = write!(text, "SP seen: {}K/{}K", Available(used), Available(total));
            }
            SystemHeader => {
                let _ = text.write_str("SYSTEM");
            }
            CpuClock => {
                let _ = write!(
                    text,
                    "CPU:{}/{}M",
                    Available(data.configured_cpu_hz.map(Mhz)),
                    Available(data.requested_cpu_hz.map(Mhz))
                );
            }
            VregSetting => {
                let _ = write!(
                    text,
                    "Vset:{}/{}mV",
                    Available(data.vreg_setting_mv),
                    Available(data.requested_voltage_mv)
                );
            }
            SpiClock => {
                let _ = write!(
                    text,
                    "SPI:{}/{}M",
                    Available(data.configured_spi_hz.map(SpiMhz)),
                    Available(data.requested_spi_hz.map(SpiMhz))
                );
            }
            Ram => {
                let (used, total) = memory_pair(data.static_ram_kb, data.ram_total_kb);
                let _ = write!(text, "Static: {}K/{}K", Available(used), Available(total));
            }
            Framebuffers => {
                let _ = write!(
                    text,
                    "FB: {}x{}K",
                    Available(data.framebuffer_count),
                    Available(data.framebuffer_bytes.map(|v| v / 1024))
                );
            }
            BrightnessSettings => {
                if let Some(brightness) = data.brightness {
                    let _ = write!(text, "{}", brightness.label());
                } else {
                    let _ = text.write_str("BL: --");
                }
                let _ = write!(text, " {}", data.settings_status.unwrap_or("--"));
            }
            ResetCause => {
                let _ = write!(text, "Reset: {}", data.reset_cause.unwrap_or("--"));
            }
            SourceStatus => {
                let _ = text.write_str(data.source_status.unwrap_or("SOURCE:--"));
            }
            ConfigFault => {
                let _ = text.write_str(match data.config_fault {
                    Some(true) => data.config_fault_label.unwrap_or("CONFIG!"),
                    Some(false) => "",
                    None => "CONFIG:--",
                });
            }
            TransferNote => {
                let _ = text.write_str("*HAL return, not scanout");
            }
            ControlsHint => {
                let _ = text.write_str(
                    crate::controls::navigation_hint(self.map, ControlPage::Debug, ControlPage::Logs).as_str(),
                );
            }
        }
        if portrait {
            draw_compact_row(display, x, y, width, text.value.as_str(), color);
        } else {
            draw_row(display, x, y, width, text.value.as_str(), color);
        }
    }
}

/// Also used for the separate Logs hint row; coordinates remain absolute.
/// Clears a10-pixel row on `display` and draws `text` in `color` at bounded panel
/// `x`, baseline `y`, clipped to `width` pixels. Caller supplies sanitized/fitting
/// text and representable coordinates; target errors are discarded.
pub(crate) fn draw_row<D: DrawTarget<Color = Rgb565>>(
    display: &mut D,
    x: i32,
    y: i32,
    width: u32,
    text: &str,
    color: Rgb565,
) {
    let bounds = Rectangle::new(Point::new(x, y - 7), Size::new(width, 10));
    let mut row = display.clipped(&bounds);
    row.clear(BLACK).ok();
    Text::new(text, Point::new(x, y), MonoTextStyle::new(LABEL_FONT, color))
        .draw(&mut row)
        .ok();
}

/// Formats a bounded sanitized prefix without scanning the rest of a long label.
struct LineText {
    /// Retained valid UTF-8 prefix, at most64bytes.
    value: String<64>,
    /// Remaining Unicode scalar/glyph budget; byte capacity can stop writing earlier.
    remaining: usize,
}
impl LineText {
    /// Returns an empty bounded formatter allowing at most `glyphs` Unicode scalars and64bytes.
    fn new(glyphs: usize) -> Self {
        Self {
            value: String::new(),
            remaining: glyphs,
        }
    }
}
impl Write for LineText {
    /// Appends a bounded prefix of `text`, replacing controls with `?` and stopping
    /// at glyph/UTF-8-byte capacity. Returns success even when clipped; does not scan
    /// the suffix beyond the remaining glyph budget.
    fn write_str(
        &mut self,
        text: &str,
    ) -> fmt::Result {
        for character in text.chars().take(self.remaining) {
            let character = if character.is_control() { '?' } else { character };
            if self.value.push(character).is_err() {
                self.remaining = 0;
                break;
            }
            self.remaining -= 1;
        }
        Ok(())
    }
}

/// Display wrapper preserving absence as `--`, independent of valid numeric zero.
struct Available<T>(
    /// Optional already admitted observation to format.
    Option<T>,
);
impl<T: fmt::Display> fmt::Display for Available<T> {
    /// Writes the present value or `--` to `f`; returns the formatter's result.
    fn fmt(
        &self,
        f: &mut fmt::Formatter<'_>,
    ) -> fmt::Result {
        match &self.0 {
            Some(v) => v.fmt(f),
            None => f.write_str("--"),
        }
    }
}

/// Integer arithmetic retains every meaningful fractional MHz digit.
struct Mhz(
    /// Exact integer Hz, rendered as decimal MHz without insignificant trailing zeros.
    u32,
);
impl fmt::Display for Mhz {
    /// Writes exact decimal MHz to `f`; returns its write result without floating-point rounding.
    fn fmt(
        &self,
        f: &mut fmt::Formatter<'_>,
    ) -> fmt::Result {
        write!(f, "{}", self.0 / 1_000_000)?;
        let mut fraction = self.0 % 1_000_000;
        if fraction != 0 {
            let mut digits = 6;
            while fraction.is_multiple_of(10) {
                fraction /= 10;
                digits -= 1;
            }
            write!(f, ".{fraction:0digits$}")?;
        }
        Ok(())
    }
}
/// Compact SPI display wrapper intentionally rounded to one decimal MHz.
struct SpiMhz(
    /// Configured/requested integer Hz, converted to f32 only for compact display.
    u32,
);
impl fmt::Display for SpiMhz {
    /// Writes rounded tenths of MHz to `f`; returns its formatting result.
    fn fmt(
        &self,
        f: &mut fmt::Formatter<'_>,
    ) -> fmt::Result {
        write!(f, "{:.1}", self.0 as f32 / 1_000_000.)
    }
}
/// Appends `label` and optional transfers/second `value` to `text`, using shared
/// invalid/negative-rate admission and bounded destination clipping.
fn fps(
    text: &mut LineText,
    label: &str,
    value: Option<f32>,
) {
    let _ = write!(text, "{label}: {}", crate::diagnostic_values::rate_text(value));
}
/// Appends `label`, optional nonnegative `value` and caller unit suffix `unit`
/// to `text`; missing observations show `--` without a fabricated duration.
fn duration(
    text: &mut LineText,
    label: &str,
    value: Option<u64>,
    unit: &str,
) {
    let _ = match value {
        Some(v) => write!(text, "{label}: {v} {unit}"),
        None => write!(text, "{label}: --"),
    };
}
/// Appends `label` and slot `value` to `text`, rendering only indices0/1 as buffers and all others as unavailable.
fn buffer(
    text: &mut LineText,
    label: &str,
    value: Option<usize>,
) {
    let _ = match value.filter(|&index| index < 2) {
        Some(v) => write!(text, "{label}buf{v}"),
        None => write!(text, "{label}--"),
    };
}

/// Returns admitted `(used, total)` KiB observations: `total` must be1..=512,
/// and `used` must not exceed it. Invalid capacity suppresses both; valid zero usage remains zero.
fn memory_pair(
    used: Option<u32>,
    total: Option<u32>,
) -> (Option<u32>, Option<u32>) {
    let total = total.filter(|&value| (1..=512).contains(&value));
    (
        used.filter(|&value| total.is_some_and(|capacity| value <= capacity)),
        total,
    )
}
/// Appends `label` and instrumented work `value` in tenths of percent to `text`;
/// missing or >1000 observations use the shared unavailable representation.
fn util(
    text: &mut LineText,
    label: &str,
    value: Option<u32>,
) {
    let _ = write!(text, "{label}: {}", crate::diagnostic_values::work_text(value));
}

/// Clears a seven-pixel row on `display`, then draws `text` in `color` with4x6
/// glyphs at bounded panel `x`, baseline `y`, clipped to `width` pixels. Caller
/// supplies sanitized text/representable coordinates; target errors are discarded.
pub(crate) fn draw_compact_row<D: DrawTarget<Color = Rgb565>>(
    display: &mut D,
    x: i32,
    y: i32,
    width: u32,
    text: &str,
    color: Rgb565,
) {
    let bounds = Rectangle::new(Point::new(x, y - 5), Size::new(width, 7));
    let mut row = display.clipped(&bounds);
    row.clear(BLACK).ok();
    Text::new(
        text,
        Point::new(x, y),
        MonoTextStyle::new(&embedded_graphics::mono_font::ascii::FONT_4X6, color),
    )
    .draw(&mut row)
    .ok();
}
