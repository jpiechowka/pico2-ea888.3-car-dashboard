use dashboard_host_validation::acquisition::{AcquisitionModel, DashboardSnapshot, SensorId};
use dashboard_host_validation::dashboard::{DashboardCache, DashboardScene};
use dashboard_host_validation::display::St7789Renderer;
use dashboard_host_validation::render::FpsMode;
use dashboard_host_validation::samples::{Quality, Quantity, Sample, Source};
use dashboard_host_validation::{AlignedFrame, colors};
use embedded_graphics::prelude::IntoStorage;

fn scene(
    snapshot: &DashboardSnapshot,
    now_ms: u64,
) -> DashboardScene<'_> {
    DashboardScene {
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
