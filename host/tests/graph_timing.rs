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
                    readings: None,
                    layout: None,
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
        let history = displayed.sensor(SensorId::Oil).history();
        assert_eq!((history.count(), history.bounds()), (60, Some((7.9, 124.0))));
        let actual: Vec<_> = history.iter().filter_map(|(_, v)| v).collect();
        let mut expected: Vec<_> = (6..124).step_by(2).map(|v| v as f32 + 1.9).collect();
        expected.push(124.0);
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
    let actual: Vec<_> = state.history().iter().filter_map(|(_, v)| v).collect();
    assert_eq!(actual, [2.0, 3.0, 5.0, 9.0]);
    assert!(state.history().iter().any(|(_, v)| v.is_none()));
}

#[test]
fn graph_identity_tracks_only_history_changes_and_reset_not_display_ticks() {
    let mut model = AcquisitionModel::demo();
    model.ingest_batch([1.0; 8], 0);
    let first = model.snapshot().sensors[0].graph_cache_identity();
    model.ingest_batch([2.0; 8], 1000);
    assert_ne!(model.snapshot().sensors[0].graph_cache_identity(), first);
    model.ingest_batch([3.0; 8], 2000);
    let sampled = model.snapshot().sensors[0].graph_cache_identity();
    assert_ne!(sampled, first);
    assert!(!model.ingest_batch([4.0; 8], 2000));
    assert_eq!(model.snapshot().sensors[0].graph_cache_identity(), sampled);
    model.reset_statistics(1);
    assert_ne!(model.snapshot().sensors[0].graph_cache_identity(), sampled);
}

#[test]
fn timed_graph_connects_only_adjacent_buckets_and_draws_isolated_points() {
    use dashboard_host_validation::history::History;
    use dashboard_host_validation::widgets::primitives::draw_history_graph;
    let mut history = History::new(10);
    history.observe(1.0, 9500); // bucket57
    history.observe(1.0, 9667); // bucket58
    history.advance(9834); // bucket59, absent
    let mut frame = AlignedFrame::boxed();
    draw_history_graph(
        &mut St7789Renderer::new(&mut frame.0),
        0,
        0,
        240,
        30,
        history.view(),
        history.view().bounds(),
        |_| colors::WHITE,
    );
    // Width236, 59 intervals: quotient/remainder positions 229,233,237. y=11.
    assert_eq!(frame.pixel(229, 11), 0xFFFF);
    assert_eq!(frame.pixel(231, 11), 0xFFFF, "adjacent buckets form a segment");
    assert_eq!(frame.pixel(236, 11), 0, "missing newest bucket is blank");
    history.invalidate(9835);
    history.observe(1.0, 9836); // recovery leaves bucket58 blank
    let mut gap = AlignedFrame::boxed();
    draw_history_graph(
        &mut St7789Renderer::new(&mut gap.0),
        0,
        0,
        240,
        30,
        history.view(),
        history.view().bounds(),
        |_| colors::WHITE,
    );
    assert_eq!(gap.pixel(229, 11), 0xFFFF);
    assert_eq!(gap.pixel(233, 11), 0, "fault breaks connectivity");
    assert_eq!(gap.pixel(237, 11), 0xFFFF, "single newest point remains visible");
}

#[test]
fn timer_scrolls_repeated_values_in_both_retained_buffers() {
    let mut model = AcquisitionModel::demo();
    model.ingest_batch([1.0; 8], 0);
    model.ingest_batch([1.0; 8], 2000);
    let mut cache = DashboardCache::new();
    let mut frames = [AlignedFrame::boxed(), AlignedFrame::boxed()];
    let before = model.snapshot();
    for now in [2000, 4000] {
        model.tick(now);
        let snapshot = model.snapshot();
        let scene = DashboardScene {
            readings: None,
            layout: None,
            measurements: &snapshot,
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
        assert!(frames[0].0 == frames[1].0);
        // Oil's newest graph point moves left when the new bucket is absent.
        assert_eq!(frames[0].pixel(69, 196), if now == 2000 { 0xFFFF } else { 0 });
        assert_eq!(snapshot.sequence, before.sequence);
        assert_eq!(snapshot.sensor(SensorId::Oil).value(), Some(1.0));
    }
}
