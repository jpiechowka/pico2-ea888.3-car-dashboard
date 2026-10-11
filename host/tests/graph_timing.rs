use dashboard_host_validation::acquisition::{AcquisitionModel, DashboardSnapshot};
use dashboard_host_validation::dashboard::{DashboardCache, DashboardScene};
use dashboard_host_validation::display::St7789Renderer;
use dashboard_host_validation::render::FpsMode;
use dashboard_host_validation::samples::SensorId;
use dashboard_host_validation::sensor_state::SensorState;
use dashboard_host_validation::vehicle_status::VehicleStatus;
use dashboard_host_validation::{AlignedFrame, colors};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::watch::Watch;

#[test]
fn coalesced_graph_history_and_both_buffers_match_literal_points_after_a_long_display_stall() {
    for periods in [
        &[14_u64][..],
        &[16, 17, 17],
        &[25],
        &[50],
        &[3, 250, 7, 1000],
        &[130_000],
    ] {
        let watch = Watch::<CriticalSectionRawMutex, DashboardSnapshot, 1>::new();
        let mut reader = watch.receiver().unwrap();
        let mut model = AcquisitionModel::demo();
        let mut displayed = DashboardSnapshot::EMPTY;
        let mut next_read = 0;
        let mut reads = 0;
        let mut cache = DashboardCache::new();
        let mut frames = [AlignedFrame::boxed(), AlignedFrame::boxed()];
        for now in (0..=124_000).step_by(10) {
            // One sample per 100 ms; oil graph must select 0,2,...124 seconds.
            if now % 100 == 0 {
                model.ingest_batch([1.0, now as f32 / 1000.0, 95.0, 80.0, 30.0, 750.0, 14.0, 1.0], now);
                watch.sender().send(model.snapshot());
            }
            if now >= next_read || now == 124_000 {
                if let Some(latest) = reader.try_changed() {
                    displayed.accept(latest);
                }
                next_read = now + periods[reads % periods.len()];
                reads += 1;
            }
            // Keep real retained pixels from before wrap in both buffers.
            // Readers continue coalescing independently until the next paint.
            if now == 0 || now == 124_000 {
                let scene = DashboardScene {
                    measurements: &displayed,
                    status: VehicleStatus::Demo,
                    now_ms: now,
                    blink_on: true,
                    boost_psi: false,
                    fps_mode: FpsMode::Off,
                    fps_instant: 0.0,
                    fps_average: 0.0,
                    backgrounds: [colors::BLACK; 8],
                    overlay: None,
                };
                for (buffer, frame) in frames.iter_mut().enumerate() {
                    cache.draw(&mut St7789Renderer::new(&mut frame.0), buffer, &scene);
                }
            }
        }
        let (values, start, count, min, max) = displayed.sensor(SensorId::Oil).get_graph_data();
        assert_eq!((count, min, max), (60, 6.0, 124.0));
        let actual: Vec<_> = (0..count).map(|i| values[(start + i) % 60]).collect();
        let expected: Vec<_> = (6..=124).step_by(2).map(|value| value as f32).collect();
        assert_eq!(actual, expected);
        assert!(frames[0].0 == frames[1].0, "both old buffers must converge after stall");
        // Independent pixel oracle: oil cell starts at (0,133), its center
        // is y=186 and graph starts at (8,190). The newest maximum is (69,192).
        assert_eq!(frames[0].pixel(69, 192), 0xFFFF);
    }
}

#[test]
fn irregular_acquisition_selects_real_samples_without_backfill_or_fps_ticks() {
    let mut state = SensorState::new();
    for (at, value) in [
        (0, 1.0),
        (1990, 2.0),
        (2010, 3.0),
        (4000, 4.0),
        (4020, 5.0),
        (100_000, 9.0),
    ] {
        assert!(state.update_at(value, false, at));
        state.tick(at);
    }
    assert!(!state.update_at(99.0, false, 100_000));
    let (values, start, count, ..) = state.get_graph_data();
    assert_eq!(start, 0);
    assert_eq!(count, 4);
    assert_eq!(&values[..4], &[1.0, 3.0, 5.0, 9.0]);
}
