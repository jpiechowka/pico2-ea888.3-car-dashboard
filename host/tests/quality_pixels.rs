//! Quality-aware pixels distinguish valid zero, unavailable diagnostics and unqualified vehicle warnings.
use dashboard_host_validation::acquisition::{AcquisitionModel, DashboardSnapshot, SensorId};
use dashboard_host_validation::dashboard::{DashboardCache, DashboardScene};
use dashboard_host_validation::display::St7789Renderer;
use dashboard_host_validation::render::FpsMode;
use dashboard_host_validation::samples::{Quality, Quantity, Sample, Source};
use dashboard_host_validation::{AlignedFrame, colors};
use embedded_graphics::prelude::IntoStorage;

/// Missing diagnostics differ from known zeros/false/empty values; nonfinite FPS matches the unavailable raster.
#[test]
fn debug_unavailable_trace_fault_clocks_and_nonfinite_fps_are_not_measured_zero() {
    use dashboard_host_validation::debug_view::{DebugData, DebugLine, DebugPage};
    use dashboard_host_validation::display::pipeline::{BufferId, FrameToken};
    use dashboard_host_validation::display::telemetry::{FrameTrace, RenderTrace};
    let unknown = DebugData::default();
    let known = DebugData {
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
        current_fps: Some(0.),
        config_fault: Some(false),
        configured_cpu_hz: Some(0),
        requested_cpu_hz: Some(0),
        vreg_setting_mv: Some(0),
        requested_voltage_mv: Some(0),
        source_status: Some(""),
        ..Default::default()
    };
    let map = dashboard_config::ControlMap::default();
    for line in [
        DebugLine::RenderTime,
        DebugLine::TransferTime,
        DebugLine::FrameTime,
        DebugLine::SampleAge,
        DebugLine::DoneFps,
        DebugLine::ConfigFault,
        DebugLine::CpuClock,
        DebugLine::VregSetting,
        DebugLine::SourceStatus,
    ] {
        let mut a = AlignedFrame::boxed();
        let mut b = AlignedFrame::boxed();
        DebugPage::new(&unknown, &map).draw_line(&mut St7789Renderer::new(&mut a.0), line);
        DebugPage::new(&known, &map).draw_line(&mut St7789Renderer::new(&mut b.0), line);
        assert!(a.0 != b.0, "missing and zero differ for {line:?}");
    }
    for value in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        let data = DebugData {
            current_fps: Some(value),
            ..Default::default()
        };
        let mut a = AlignedFrame::boxed();
        let mut b = AlignedFrame::boxed();
        DebugPage::new(&data, &map).draw_line(&mut St7789Renderer::new(&mut a.0), DebugLine::DoneFps);
        DebugPage::new(&unknown, &map).draw_line(&mut St7789Renderer::new(&mut b.0), DebugLine::DoneFps);
        assert!(a.0 == b.0);
    }
}

/// Returns a raw legacy scene borrowing `snapshot`, deriving status/freshness
/// at milliseconds `now_ms` with red candidate backgrounds and no overlay.
fn scene(
    snapshot: &DashboardSnapshot,
    now_ms: u64,
) -> DashboardScene<'_> {
    DashboardScene {
        readings: None,
        layout: None,
        measurements: snapshot,
        status: dashboard_host_validation::vehicle_status::VehicleStatus::derive(false, None, snapshot, now_ms),
        now_ms,
        blink_on: true,
        boost_psi: false,
        fps_mode: FpsMode::Off,
        fps_instant: 0.0,
        fps_average: 0.0,
        backgrounds: [colors::RED; 8],
        overlay: None,
    }
}

/// Qualified coolant zero differs from missing data, but generic vehicle samples never inherit unqualified demo
/// critical colours.
#[test]
fn unavailable_is_not_zero_and_vehicle_coolant_has_no_unverified_critical_coloring() {
    let mut model = AcquisitionModel::new();
    let mut unavailable = AlignedFrame::boxed();
    scene(&model.snapshot(), 0).draw_full(&mut St7789Renderer::new(&mut unavailable.0));
    let mut real = AlignedFrame::boxed();
    for (index, value) in [0.0, 90.0, 95.0, 130.0].into_iter().enumerate() {
        let at = index as u64;
        model
            .ingest_sample(
                Sample {
                    value: Quantity::CoolantC(value),
                    received_at_ms: at,
                    sequence: at + 1,
                    source: Source::Obd { ecu: 0x7E8, pid: 0x05 },
                },
                at,
            )
            .unwrap();
        scene(&model.snapshot(), at).draw_full(&mut St7789Renderer::new(&mut real.0));
        assert!(real.0 != unavailable.0, "valid zero is an actual measurement");
        for y in 28..132 {
            for x in 241..318 {
                assert_ne!(
                    real.pixel(x, y),
                    colors::RED.into_storage(),
                    "unverified vehicle policy at {value}C"
                );
            }
        }
    }
}

/// Both buffers converge to full redraw through freshness expiry, explicit transport fault and later valid recovery.
#[test]
fn stale_fault_and_recovery_replay_both_buffers_without_old_numbers_or_graphs() {
    let mut model = AcquisitionModel::new();
    let mut cache = DashboardCache::new();
    let mut buffers = [AlignedFrame::boxed(), AlignedFrame::boxed()];
    let mut full = AlignedFrame::boxed();
    for step in 0..7 {
        let now = step * 1000;
        if step == 0 || step == 6 {
            model
                .ingest_sample(
                    Sample {
                        value: Quantity::ModuleVolts(13.8),
                        received_at_ms: now,
                        sequence: step + 1,
                        source: Source::Obd { ecu: 0x7E8, pid: 0x42 },
                    },
                    now,
                )
                .unwrap();
        }
        if step == 4 {
            model.mark_fault(SensorId::Battery, Quality::TransportError, now);
        }
        let snapshot = model.snapshot();
        if step == 3 {
            assert_eq!(snapshot.value_at(SensorId::Battery, now), None);
        }
        if step == 6 {
            assert_eq!(snapshot.value_at(SensorId::Battery, now), Some(13.8));
        }
        let scene = scene(&snapshot, now);
        scene.draw_full(&mut St7789Renderer::new(&mut full.0));
        for (buffer, frame) in buffers.iter_mut().enumerate() {
            cache.draw(&mut St7789Renderer::new(&mut frame.0), buffer, &scene);
            assert!(frame.0 == full.0, "quality replay step {step} buffer {buffer}");
        }
    }
}
