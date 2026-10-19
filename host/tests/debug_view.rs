use dashboard_config::ControlMap;
use dashboard_host_validation::AlignedFrame;
use dashboard_host_validation::backlight::Brightness;
use dashboard_host_validation::colors::*;
use dashboard_host_validation::debug_view::{DebugData, DebugLine, DebugPage};
use dashboard_host_validation::display::St7789Renderer;
use dashboard_host_validation::display::pipeline::{BufferId, FrameToken};
use dashboard_host_validation::display::telemetry::{FrameTrace, RenderTrace};
use embedded_graphics::mono_font::MonoTextStyle;
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::Rectangle;
use embedded_graphics::text::Text;

fn measured() -> DebugData<'static> {
    DebugData {
        reset_cause: Some("POR"),
        source_status: Some("DATA"),
        config_fault: Some(false),
        current_fps: Some(0.),
        average_fps: Some(0.),
        frame_count: Some(0),
        trace: Some(
            FrameTrace::finish(
                FrameToken {
                    buffer: BufferId::A,
                    sequence: 1,
                },
                RenderTrace {
                    sample_us: Some(0),
                    ..RenderTrace::instant(0)
                },
                0,
                0,
                2,
            )
            .unwrap(),
        ),
        brightness: Some(Brightness::Full),
        settings_status: Some("SAVED"),
        buffer_swaps: Some(0),
        buffer_waits: Some(0),
        render_buffer_idx: Some(0),
        flush_buffer_idx: Some(1),
        core0_stack_used_kb: Some(0),
        core0_stack_total_kb: Some(64),
        core1_stack_used_kb: Some(0),
        core1_stack_total_kb: Some(32),
        static_ram_kb: Some(448),
        ram_total_kb: Some(512),
        cpu0_util_percent: Some(0),
        cpu1_util_percent: Some(0),
        frame_cycles: Some(0),
        requested_cpu_hz: Some(290_000_000),
        configured_cpu_hz: Some(290_400_000),
        requested_spi_hz: Some(72_000_000),
        configured_spi_hz: Some(72_600_000),
        requested_voltage_mv: Some(1300),
        vreg_setting_mv: Some(1300),
        framebuffer_count: Some(2),
        framebuffer_bytes: Some(153600),
        ..Default::default()
    }
}

fn literal(
    frame: &mut AlignedFrame,
    x: i32,
    y: i32,
    width: u32,
    text: &str,
    color: Rgb565,
) {
    let mut display = St7789Renderer::new(&mut frame.0);
    let mut row = display.clipped(&Rectangle::new(Point::new(x, y - 7), Size::new(width, 10)));
    row.clear(BLACK).unwrap();
    Text::new(
        text,
        Point::new(x, y),
        MonoTextStyle::new(dashboard_host_validation::ui::LABEL_FONT, color),
    )
    .draw(&mut row)
    .unwrap();
}

#[test]
fn complete_measured_screen_matches_independent_literal_bitmap() {
    let data = measured();
    let map = ControlMap::default();
    let mut actual = AlignedFrame::boxed();
    actual.0.fill(0xCC);
    DebugPage::new(&data, &map).draw(&mut St7789Renderer::new(&mut actual.0));
    let mut expected = AlignedFrame::boxed();
    for (x, y, text, color) in [
        (4, 12, "SOFTWARE TIMING", GREEN),
        (4, 26, "Done FPS: 0.0", YELLOW),
        (4, 40, "Avg. FPS: 0.0", YELLOW),
        (4, 54, "Frame: 0", WHITE),
        (4, 68, "Render: 0 us", WHITE),
        (4, 82, "Flush*: 0 us", WHITE),
        (4, 96, "Frame: 0 us", YELLOW),
        (4, 110, "Age*: 0 ms", WHITE),
        (4, 128, "BUFFERS", GREEN),
        (4, 142, "Swaps: 0", WHITE),
        (4, 156, "Waits: 0", WHITE),
        (4, 170, "Render: buf0", WHITE),
        (4, 184, "Flush:  buf1", WHITE),
        (164, 12, "CORE 0", GREEN),
        (164, 26, "UI work: 0.0%", WHITE),
        (164, 40, "Cycles: 0K", WHITE),
        (164, 54, "SP now: 0K/64K", WHITE),
        (164, 72, "CORE 1", GREEN),
        (164, 86, "Acq work: 0.0%", WHITE),
        (164, 100, "SP seen: 0K/32K", WHITE),
        (164, 118, "SYSTEM", GREEN),
        (164, 132, "CPU:290.4/290M", WHITE),
        (164, 146, "Vset:1300/1300mV", WHITE),
        (164, 160, "SPI:72.6/72.0M", WHITE),
        (164, 174, "Static: 448K/512K", WHITE),
        (164, 188, "FB: 2x150K", WHITE),
        (164, 202, "BL: FULL SAVED", WHITE),
        (4, 198, "Reset: POR", WHITE),
        (4, 212, "DATA", YELLOW),
        (164, 212, "", YELLOW),
        (4, 226, "*HAL return, not scanout", GREEN),
        (4, 236, "Y tap:Logs | Y hold 2s:Dash/Full", GREEN),
    ] {
        literal(&mut expected, x, y, if y >= 226 { 316 } else { 156 }, text, color);
    }
    assert!(actual.0 == expected.0);
}

#[test]
fn unavailable_work_and_stack_observations_are_not_rendered_as_zero() {
    let map = ControlMap::default();
    let data = DebugData {
        core0_stack_total_kb: Some(64),
        ..Default::default()
    };
    for (line, y, text) in [
        (DebugLine::Core0Utilization, 26, "UI work: --"),
        (DebugLine::FrameCycles, 40, "Cycles: --K"),
        (DebugLine::Core0Stack, 54, "SP now: --K/64K"),
        (DebugLine::Core1Utilization, 86, "Acq work: --"),
    ] {
        let mut actual = AlignedFrame::boxed();
        let mut expected = AlignedFrame::boxed();
        DebugPage::new(&data, &map).draw_line(&mut St7789Renderer::new(&mut actual.0), line);
        literal(&mut expected, 164, y, 156, text, WHITE);
        assert!(actual.0 == expected.0, "{text}");
    }
}

#[test]
fn exact_hz_preserves_fractional_and_sub100mhz_clocks_without_overflow() {
    let map = ControlMap::default();
    for (configured, requested, expected) in [
        (Some(290_400_000), Some(290_000_000), "CPU:290.4/290M"),
        (Some(12_345_678), Some(12_000_000), "CPU:12.345678/12M"),
        (Some(0), Some(0), "CPU:0/0M"),
        (None, Some(0), "CPU:--/0M"),
        (Some(u32::MAX), Some(1), "CPU:4294.967295/0.000001M"),
    ] {
        let data = DebugData {
            configured_cpu_hz: configured,
            requested_cpu_hz: requested,
            ..Default::default()
        };
        let mut actual = AlignedFrame::boxed();
        let mut wanted = AlignedFrame::boxed();
        DebugPage::new(&data, &map).draw_line(&mut St7789Renderer::new(&mut actual.0), DebugLine::CpuClock);
        literal(&mut wanted, 164, 132, 156, expected, WHITE);
        assert!(actual.0 == wanted.0, "{expected}");
    }
}

#[test]
fn per_line_replay_clears_fault_and_sanitizes_bounded_utf8_labels() {
    let map = ControlMap::default();
    let label = "ABCDE\nFGHIJ\rKLMNO\tPQRSTUVWX🦀🦀🦀";
    for (line, x, y, prefix, wanted) in [
        (DebugLine::SourceStatus, 4, 212, "", "ABCDE?FGHIJ?KLMNO?PQRSTUVWX"),
        (DebugLine::ConfigFault, 164, 212, "", "ABCDE?FGHIJ?KLMNO?PQRSTUVWX"),
        (DebugLine::ResetCause, 4, 198, "Reset: ", "Reset: ABCDE?FGHIJ?KLMNO?PQ"),
    ] {
        let data = DebugData {
            reset_cause: Some(label),
            source_status: Some(label),
            config_fault: Some(true),
            config_fault_label: Some(label),
            ..Default::default()
        };
        let mut actual = AlignedFrame::boxed();
        let mut expected = AlignedFrame::boxed();
        DebugPage::new(&data, &map).draw_line(&mut St7789Renderer::new(&mut actual.0), line);
        literal(&mut expected, x, y, 156, wanted, YELLOW);
        if !prefix.is_empty() {
            literal(&mut expected, x, y, 156, wanted, WHITE);
        }
        assert!(actual.0 == expected.0, "{line:?}");
    }
    let mut actual = AlignedFrame::boxed();
    let fault = DebugData {
        config_fault: Some(true),
        config_fault_label: Some(label),
        ..Default::default()
    };
    DebugPage::new(&fault, &map).draw_line(&mut St7789Renderer::new(&mut actual.0), DebugLine::ConfigFault);
    let ok = DebugData {
        config_fault: Some(false),
        ..Default::default()
    };
    DebugPage::new(&ok, &map).draw_line(&mut St7789Renderer::new(&mut actual.0), DebugLine::ConfigFault);
    assert!(actual.0 == AlignedFrame::boxed().0);
    let unicode = DebugData {
        source_status: Some("é🦀"),
        ..Default::default()
    };
    let mut expected = AlignedFrame::boxed();
    DebugPage::new(&unicode, &map).draw_line(&mut St7789Renderer::new(&mut actual.0), DebugLine::SourceStatus);
    literal(&mut expected, 4, 212, 156, "é🦀", YELLOW);
    assert!(actual.0 == expected.0);
}

fn bounds(line: DebugLine) -> Rectangle {
    use DebugLine::*;
    let (x, y, w) = match line {
        TimingHeader => (4, 12, 156),
        DoneFps => (4, 26, 156),
        AverageFps => (4, 40, 156),
        CompletedFrames => (4, 54, 156),
        RenderTime => (4, 68, 156),
        TransferTime => (4, 82, 156),
        FrameTime => (4, 96, 156),
        SampleAge => (4, 110, 156),
        BuffersHeader => (4, 128, 156),
        Swaps => (4, 142, 156),
        Waits => (4, 156, 156),
        RenderBuffer => (4, 170, 156),
        FlushBuffer => (4, 184, 156),
        Core0Header => (164, 12, 156),
        Core0Utilization => (164, 26, 156),
        FrameCycles => (164, 40, 156),
        Core0Stack => (164, 54, 156),
        Core1Header => (164, 72, 156),
        Core1Utilization => (164, 86, 156),
        Core1Stack => (164, 100, 156),
        SystemHeader => (164, 118, 156),
        CpuClock => (164, 132, 156),
        VregSetting => (164, 146, 156),
        SpiClock => (164, 160, 156),
        Ram => (164, 174, 156),
        Framebuffers => (164, 188, 156),
        BrightnessSettings => (164, 202, 156),
        ResetCause => (4, 198, 156),
        SourceStatus => (4, 212, 156),
        ConfigFault => (164, 212, 156),
        TransferNote => (4, 226, 316),
        ControlsHint => (4, 236, 316),
    };
    Rectangle::new(Point::new(x, y - 7), Size::new(w, 10))
}

struct Guard {
    target: Rectangle,
    allowed: Rectangle,
    pixels: usize,
}
impl Dimensions for Guard {
    fn bounding_box(&self) -> Rectangle { self.target }
}
impl DrawTarget for Guard {
    type Color = Rgb565;
    type Error = core::convert::Infallible;

    fn draw_iter<I>(
        &mut self,
        pixels: I,
    ) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Pixel<Rgb565>>,
    {
        for Pixel(point, _) in pixels {
            assert!(self.allowed.contains(point), "escaped operation at {point:?}");
            self.pixels += 1;
        }
        Ok(())
    }
}

#[test]
fn every_operation_clips_maximum_values_to_row_column_panel_and_actual_target() {
    let mut data = measured();
    data.current_fps = Some(f32::MAX);
    data.average_fps = Some(-f32::MAX);
    data.frame_count = Some(u32::MAX);
    data.buffer_swaps = Some(u32::MAX);
    data.buffer_waits = Some(u32::MAX);
    data.render_buffer_idx = Some(usize::MAX);
    data.flush_buffer_idx = Some(usize::MAX);
    data.core0_stack_used_kb = Some(u32::MAX);
    data.core0_stack_total_kb = Some(u32::MAX);
    data.core1_stack_used_kb = Some(u32::MAX);
    data.core1_stack_total_kb = Some(u32::MAX);
    data.cpu0_util_percent = Some(u32::MAX);
    data.cpu1_util_percent = Some(u32::MAX);
    data.frame_cycles = Some(u32::MAX);
    data.requested_cpu_hz = Some(u32::MAX);
    data.configured_cpu_hz = Some(u32::MAX);
    data.requested_spi_hz = Some(u32::MAX);
    data.configured_spi_hz = Some(u32::MAX);
    data.requested_voltage_mv = Some(u32::MAX);
    data.vreg_setting_mv = Some(u32::MAX);
    data.static_ram_kb = Some(u32::MAX);
    data.ram_total_kb = Some(u32::MAX);
    data.framebuffer_count = Some(u32::MAX);
    data.framebuffer_bytes = Some(u32::MAX);
    data.source_status = Some("🦀🦀🦀🦀🦀🦀🦀🦀🦀🦀🦀🦀🦀🦀🦀🦀🦀🦀🦀\nignored");
    data.settings_status = data.source_status;
    data.reset_cause = data.source_status;
    data.config_fault = Some(true);
    data.config_fault_label = data.source_status;
    data.trace = Some(
        FrameTrace::finish(
            FrameToken {
                buffer: BufferId::B,
                sequence: 1,
            },
            RenderTrace {
                end_us: u64::MAX,
                queued_us: u64::MAX,
                sample_us: Some(0),
                ..RenderTrace::instant(0)
            },
            u64::MAX,
            u64::MAX,
            2,
        )
        .unwrap(),
    );
    let map = ControlMap::default();
    for target in [
        Rectangle::new(Point::zero(), Size::new(320, 240)),
        Rectangle::new(Point::new(7, 9), Size::new(190, 222)),
        Rectangle::new(Point::new(-10, -10), Size::new(350, 270)),
    ] {
        let page = DebugPage::new(&data, &map);
        for line in DebugLine::ALL {
            let mut guard = Guard {
                target,
                allowed: bounds(line).intersection(&target),
                pixels: 0,
            };
            page.draw_line(&mut guard, line);
            if guard.allowed.size != Size::zero() {
                assert!(guard.pixels > 0);
            }
        }
        let mut guard = Guard {
            target,
            allowed: Rectangle::new(Point::zero(), Size::new(320, 240)).intersection(&target),
            pixels: 0,
        };
        page.draw(&mut guard);
    }
}

#[test]
fn max_numeric_text_keeps_color_units_and_configured_requested_order() {
    let map = ControlMap::default();
    let data = DebugData {
        buffer_waits: Some(u32::MAX),
        cpu0_util_percent: Some(u32::MAX),
        vreg_setting_mv: Some(u32::MAX),
        requested_voltage_mv: Some(u32::MAX),
        ..Default::default()
    };
    for (line, x, y, text, color) in [
        // Waiting for an in-flight transfer is ordinary pipeline backpressure.
        (DebugLine::Waits, 4, 156, "Waits: 4294967295", WHITE),
        (DebugLine::Core0Utilization, 164, 26, "UI work: --", WHITE),
        (DebugLine::VregSetting, 164, 146, "Vset:4294967295/4294967295mV", WHITE),
    ] {
        let mut actual = AlignedFrame::boxed();
        let mut expected = AlignedFrame::boxed();
        DebugPage::new(&data, &map).draw_line(&mut St7789Renderer::new(&mut actual.0), line);
        literal(&mut expected, x, y, 156, text, color);
        assert!(actual.0 == expected.0);
    }
}

#[test]
fn negative_fps_is_unavailable_instead_of_a_negative_transfer_rate() {
    let map = ControlMap::default();
    let data = DebugData {
        current_fps: Some(-1.0),
        ..Default::default()
    };
    let mut actual = AlignedFrame::boxed();
    let mut expected = AlignedFrame::boxed();
    DebugPage::new(&data, &map).draw_line(&mut St7789Renderer::new(&mut actual.0), DebugLine::DoneFps);
    literal(&mut expected, 4, 26, 156, "Done FPS: --", YELLOW);
    assert!(actual.0 == expected.0);
}

#[test]
fn impossible_memory_and_buffer_observations_remain_unavailable() {
    let map = ControlMap::default();
    let data = DebugData {
        core0_stack_used_kb: Some(33),
        core0_stack_total_kb: Some(32),
        core1_stack_used_kb: None,
        core1_stack_total_kb: Some(32),
        static_ram_kb: Some(513),
        ram_total_kb: Some(512),
        render_buffer_idx: Some(2),
        ..Default::default()
    };
    for (line, x, y, text) in [
        (DebugLine::Core0Stack, 164, 54, "SP now: --K/32K"),
        (DebugLine::Core1Stack, 164, 100, "SP seen: --K/32K"),
        (DebugLine::Ram, 164, 174, "Static: --K/512K"),
        (DebugLine::RenderBuffer, 4, 170, "Render: --"),
    ] {
        let mut actual = AlignedFrame::boxed();
        let mut expected = AlignedFrame::boxed();
        DebugPage::new(&data, &map).draw_line(&mut St7789Renderer::new(&mut actual.0), line);
        literal(&mut expected, x, y, 156, text, WHITE);
        assert!(actual.0 == expected.0, "{text}");
    }
}

#[test]
fn remapped_footer_preserves_scanout_qualification_and_recovery_below_fault_rows() {
    use dashboard_config::{ButtonId, ControlAction, ControlPage};
    let mut map = ControlMap::default();
    let screen = &mut map.screens[ControlPage::Debug.index()];
    screen.buttons[ButtonId::Y.index()].tap = ControlAction::Dashboard;
    screen.buttons[ButtonId::A.index()].hold = ControlAction::Logs;
    let data = measured();
    let mut actual = AlignedFrame::boxed();
    let mut expected = AlignedFrame::boxed();
    let page = DebugPage::new(&data, &map);
    for line in [DebugLine::TransferNote, DebugLine::ControlsHint] {
        page.draw_line(&mut St7789Renderer::new(&mut actual.0), line);
    }
    literal(&mut expected, 4, 226, 316, "*HAL return, not scanout", GREEN);
    literal(&mut expected, 4, 236, 316, "A hold:Logs | Y hold 2s:Dash/Full", GREEN);
    assert!(actual.0 == expected.0);
    assert!(actual.0[..219 * 320 * 2].iter().all(|v| *v == 0));
    assert!(actual.0[239 * 320 * 2..].iter().all(|v| *v == 0));
}
