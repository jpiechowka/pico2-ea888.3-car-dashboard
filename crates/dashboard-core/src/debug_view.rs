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
    pub reset_cause: Option<&'a str>,
    pub source_status: Option<&'a str>,
    pub config_fault: Option<bool>,
    pub config_fault_label: Option<&'a str>,
    pub current_fps: Option<f32>,
    pub average_fps: Option<f32>,
    pub frame_count: Option<u32>,
    pub trace: Option<FrameTrace>,
    pub brightness: Option<Brightness>,
    pub settings_status: Option<&'a str>,
    pub buffer_swaps: Option<u32>,
    pub buffer_waits: Option<u32>,
    pub render_buffer_idx: Option<usize>,
    pub flush_buffer_idx: Option<usize>,
    pub core0_stack_used_kb: Option<u32>,
    pub core0_stack_total_kb: Option<u32>,
    pub core1_stack_used_kb: Option<u32>,
    pub core1_stack_total_kb: Option<u32>,
    pub static_ram_kb: Option<u32>,
    pub ram_total_kb: Option<u32>,
    /// Tenths of percent, retaining the existing firmware producer units.
    pub cpu0_util_percent: Option<u32>,
    pub cpu1_util_percent: Option<u32>,
    pub frame_cycles: Option<u32>,
    pub requested_cpu_hz: Option<u32>,
    pub configured_cpu_hz: Option<u32>,
    pub requested_spi_hz: Option<u32>,
    pub configured_spi_hz: Option<u32>,
    pub requested_voltage_mv: Option<u32>,
    pub vreg_setting_mv: Option<u32>,
    pub framebuffer_count: Option<u32>,
    pub framebuffer_bytes: Option<u32>,
}

/// One synchronous row operation. Firmware yields after each member of ALL.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DebugLine {
    TimingHeader,
    DoneFps,
    AverageFps,
    CompletedFrames,
    RenderTime,
    TransferTime,
    FrameTime,
    SampleAge,
    BuffersHeader,
    Swaps,
    Waits,
    RenderBuffer,
    FlushBuffer,
    Core0Header,
    Core0Utilization,
    FrameCycles,
    Core0Stack,
    Core1Header,
    Core1Utilization,
    Core1Stack,
    SystemHeader,
    CpuClock,
    VregSetting,
    SpiClock,
    Ram,
    Framebuffers,
    BrightnessSettings,
    ResetCause,
    SourceStatus,
    ConfigFault,
    TransferNote,
    ControlsHint,
}

impl DebugLine {
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

pub struct DebugPage<'a> {
    data: &'a DebugData<'a>,
    map: &'a ControlMap,
}

impl<'a> DebugPage<'a> {
    pub const fn new(
        data: &'a DebugData<'a>,
        map: &'a ControlMap,
    ) -> Self {
        Self { data, map }
    }

    /// Host/browser convenience; firmware uses draw_line and cooperative yields.
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
            Waits if data.buffer_waits.is_some_and(|v| v > 0) => YELLOW,
            Core0Utilization if data.cpu0_util_percent.is_some_and(|v| v > 850) => YELLOW,
            Core1Utilization if data.cpu1_util_percent.is_some_and(|v| v > 850) => YELLOW,
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
                let _ = write!(
                    text,
                    "SP now: {}K/{}K",
                    Available(data.core0_stack_used_kb),
                    Available(data.core0_stack_total_kb)
                );
            }
            Core1Header => {
                let _ = text.write_str("CORE 1");
            }
            Core1Utilization => util(&mut text, "Acq work", data.cpu1_util_percent),
            Core1Stack => {
                let _ = write!(
                    text,
                    "SP seen: {}K/{}K",
                    Available(data.core1_stack_used_kb),
                    Available(data.core1_stack_total_kb)
                );
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
                let _ = write!(
                    text,
                    "RAM: {}K/{}K",
                    Available(data.static_ram_kb),
                    Available(data.ram_total_kb)
                );
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
    value: String<64>,
    remaining: usize,
}
impl LineText {
    fn new(glyphs: usize) -> Self {
        Self {
            value: String::new(),
            remaining: glyphs,
        }
    }
}
impl Write for LineText {
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

struct Available<T>(Option<T>);
impl<T: fmt::Display> fmt::Display for Available<T> {
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
struct Mhz(u32);
impl fmt::Display for Mhz {
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
struct SpiMhz(u32);
impl fmt::Display for SpiMhz {
    fn fmt(
        &self,
        f: &mut fmt::Formatter<'_>,
    ) -> fmt::Result {
        write!(f, "{:.1}", self.0 as f32 / 1_000_000.)
    }
}
fn fps(
    text: &mut LineText,
    label: &str,
    value: Option<f32>,
) {
    let _ = match value.filter(|v| v.is_finite()) {
        Some(v) => write!(text, "{label}: {v:.1}"),
        None => write!(text, "{label}: --"),
    };
}
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
fn buffer(
    text: &mut LineText,
    label: &str,
    value: Option<usize>,
) {
    let _ = match value {
        Some(v) => write!(text, "{label}buf{v}"),
        None => write!(text, "{label}--"),
    };
}
fn util(
    text: &mut LineText,
    label: &str,
    value: Option<u32>,
) {
    let _ = match value {
        Some(v) => write!(text, "{label}: {}.{}%", v / 10, v % 10),
        None => write!(text, "{label}: --"),
    };
}

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
