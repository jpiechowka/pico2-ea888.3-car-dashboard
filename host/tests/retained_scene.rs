use dashboard_host_validation::acquisition::AcquisitionModel;
use dashboard_host_validation::backlight::Brightness;
use dashboard_host_validation::dashboard::{DashboardCache, DashboardScene, Overlay};
use dashboard_host_validation::display::St7789Renderer;
use dashboard_host_validation::display::damage::DamagePlan;
use dashboard_host_validation::render::FpsMode;
use dashboard_host_validation::{AlignedFrame, colors};

#[test]
fn diagnostic_screens_clear_both_prior_histories_and_logs_hints_preserve_counters() {
    use dashboard_host_validation::debug_view::{DebugData, DebugLine, DebugPage};
    use dashboard_host_validation::logging::{Level, LogStore};
    use dashboard_host_validation::logging_view::LogPage;
    use embassy_sync::blocking_mutex::raw::NoopRawMutex;
    let map = dashboard_config::ControlMap::default();
    let logs = LogStore::<NoopRawMutex>::new();
    logs.push(Level::Fault, 1, format_args!("separate latest fault"));
    for n in 0..150 {
        logs.push(Level::Info, n, format_args!("{}", "X".repeat(41)));
    }
    let page = LogPage::capture(&logs, None);
    let mut reference = AlignedFrame::boxed();
    page.draw_full(&mut St7789Renderer::new(&mut reference.0));
    let counters = reference.0[219 * 320 * 2..].to_vec();
    page.draw_hint(&mut St7789Renderer::new(&mut reference.0), &map);
    assert_eq!(&reference.0[219 * 320 * 2..], counters.as_slice());
    assert!(reference.0[205 * 320 * 2..215 * 320 * 2].iter().any(|b| *b != 0));
    let mut histories = [AlignedFrame::boxed(), AlignedFrame::boxed()];
    histories[0].0.fill(0x55);
    histories[1].0.fill(0xCC);
    let model = AcquisitionModel::demo();
    let measurements = model.snapshot();
    let dashboard = DashboardScene {
        readings: None,
        layout: None,
        measurements: &measurements,
        now_ms: 0,
        status: dashboard_host_validation::vehicle_status::VehicleStatus::Demo,
        blink_on: true,
        boost_psi: false,
        fps_mode: FpsMode::Off,
        fps_instant: 0.,
        fps_average: 0.,
        backgrounds: [colors::BLACK; 8],
        overlay: None,
    };
    let mut dashboard_reference = AlignedFrame::boxed();
    dashboard.draw_full(&mut St7789Renderer::new(&mut dashboard_reference.0));
    let long = DebugData {
        source_status: Some("long source status survives until replaced"),
        config_fault: Some(true),
        config_fault_label: Some("long configuration error label"),
        ..Default::default()
    };
    for frame in &mut histories {
        dashboard.draw_full(&mut St7789Renderer::new(&mut frame.0));
        DebugPage::new(&long, &map).draw(&mut St7789Renderer::new(&mut frame.0));
        let short = DebugData {
            source_status: Some(""),
            config_fault: Some(false),
            ..Default::default()
        };
        for line in [DebugLine::SourceStatus, DebugLine::ConfigFault] {
            DebugPage::new(&short, &map).draw_line(&mut St7789Renderer::new(&mut frame.0), line);
        }
        let mut clean = AlignedFrame::boxed();
        DebugPage::new(&short, &map).draw(&mut St7789Renderer::new(&mut clean.0));
        assert!(frame.0 == clean.0);
        page.draw_header(&mut St7789Renderer::new(&mut frame.0));
        for row in 0..=12 {
            logs.push(Level::Info, 200, format_args!("producer changes between operations"));
            page.draw_row(&mut St7789Renderer::new(&mut frame.0), row);
        }
        page.draw_hint(&mut St7789Renderer::new(&mut frame.0), &map);
        page.draw_footer(&mut St7789Renderer::new(&mut frame.0));
        assert!(frame.0 == reference.0);
        dashboard.draw_full(&mut St7789Renderer::new(&mut frame.0));
        assert!(
            frame.0 == dashboard_reference.0,
            "diagnostic glyphs cannot survive the return to Dashboard"
        );
    }
}

#[test]
fn extreme_cell_text_cannot_overwrite_a_neighbor_or_leave_old_glyphs() {
    let mut model = AcquisitionModel::demo();
    model.ingest_batch([-12345.5, 90.0, 95.0, 80.0, 30.0, 750.0, 13.8, 1.0], 0);
    let snapshot = model.snapshot();
    let scene = DashboardScene {
        readings: None,
        layout: None,
        measurements: &snapshot,
        now_ms: 0,
        status: dashboard_host_validation::vehicle_status::VehicleStatus::Demo,
        blink_on: true,
        boost_psi: false,
        fps_mode: FpsMode::Off,
        fps_instant: 0.0,
        fps_average: 0.0,
        backgrounds: [colors::BLACK; 8],
        overlay: None,
    };
    let mut frame = AlignedFrame::boxed();
    frame.0.fill(0x5A);
    scene.draw_cell(&mut St7789Renderer::new(&mut frame.0), 0);
    for y in 0..240 {
        for x in 80..320 {
            assert_eq!(frame.pixel(x, y), 0x5A5A, "neighbor pixel {x},{y}");
        }
    }
    // Reuse the actual pixels, then shorten both current and maximum text.
    model.ingest_batch([0.0, 90.0, 95.0, 80.0, 30.0, 750.0, 13.8, 1.0], 1);
    model.reset_statistics(1);
    let shortened = model.snapshot();
    let scene = DashboardScene {
        readings: None,
        layout: None,
        measurements: &shortened,
        now_ms: 1,
        ..scene
    };
    let mut clean = AlignedFrame::boxed();
    clean.0.fill(0x5A);
    scene.draw_cell(&mut St7789Renderer::new(&mut frame.0), 0);
    scene.draw_cell(&mut St7789Renderer::new(&mut clean.0), 0);
    assert!(frame.0 == clean.0, "long glyphs must not survive a shorter value");
}

#[test]
fn retained_peak_and_trend_expire_without_a_new_snapshot() {
    let mut model = AcquisitionModel::demo();
    for sample in 0..30 {
        let oil = 90.0 + sample as f32;
        model.ingest_batch([1.0, oil, 95.0, 80.0, 30.0, 750.0, 13.8, 1.0], sample * 200);
    }
    let snapshot = model.snapshot();
    let mut scene = DashboardScene {
        readings: None,
        layout: None,
        measurements: &snapshot,
        now_ms: 5800,
        status: dashboard_host_validation::vehicle_status::VehicleStatus::Demo,
        blink_on: true,
        boost_psi: false,
        fps_mode: FpsMode::Off,
        fps_instant: 0.0,
        fps_average: 0.0,
        backgrounds: [colors::BLACK; 8],
        overlay: None,
    };
    assert!(snapshot.sensors[1].is_new_peak_at(scene.now_ms));
    assert!(snapshot.sensors[1].get_trend_at(scene.now_ms).is_some());
    let mut cache = DashboardCache::new();
    let mut retained = AlignedFrame::boxed();
    let mut reference = AlignedFrame::boxed();
    cache.draw(&mut St7789Renderer::new(&mut retained.0), 0, &scene);
    for now in [6550, 7801] {
        scene.now_ms = now;
        let redraw = cache.draw(&mut St7789Renderer::new(&mut retained.0), 0, &scene);
        assert_ne!(redraw.cells & (1 << 4), 0, "oil's timed state must invalidate at {now}");
        scene.draw_full(&mut St7789Renderer::new(&mut reference.0));
        assert!(retained.0 == reference.0);
    }
}

#[test]
fn each_buffer_retains_exact_visible_values_and_ignores_hidden_precision() {
    let mut model = AcquisitionModel::demo();
    model.ingest_batch([1.0001, 90.0, 95.0, 80.0, 30.0, 750.0, 13.8, 1.0], 0);
    let first = model.snapshot();
    let mut scene = DashboardScene {
        readings: None,
        layout: None,
        measurements: &first,
        now_ms: 0,
        status: dashboard_host_validation::vehicle_status::VehicleStatus::Demo,
        blink_on: true,
        boost_psi: false,
        fps_mode: FpsMode::Off,
        fps_instant: 40.0,
        fps_average: 39.0,
        backgrounds: [colors::BLACK; 8],
        overlay: None,
    };
    let mut cache = DashboardCache::new();
    for buffer in [0, 1] {
        assert_eq!(cache.prepare(buffer, &scene).cells, 0xFF);
        assert_eq!(cache.prepare(buffer, &scene).cells, 0);
    }
    // Only boost changes here; its two unit strings and peak stay identical.
    model.ingest_batch(
        [
            1.0002,
            f32::NAN,
            f32::NAN,
            f32::NAN,
            f32::NAN,
            f32::NAN,
            f32::NAN,
            f32::NAN,
        ],
        1,
    );
    let next = model.snapshot();
    scene.measurements = &next;
    scene.now_ms = 1;
    for buffer in [0, 1] {
        assert_eq!(cache.prepare(buffer, &scene).cells & 1, 0);
    }
    scene.boost_psi = true;
    for buffer in [0, 1] {
        assert_eq!(cache.prepare(buffer, &scene).cells & 1, 1);
    }
    assert!(cache.prepare(99, &scene).clear); // invalid caller fails open
}

#[test]
fn alternating_retained_frames_and_partial_panel_equal_full_redraw_after_every_change() {
    let mut model = AcquisitionModel::demo();
    let mut cache = DashboardCache::new();
    let mut frames = [AlignedFrame::boxed(), AlignedFrame::boxed()];
    let mut panel = AlignedFrame::boxed();
    let mut reference = AlignedFrame::boxed();
    for step in 0..240 {
        let now = step * 100;
        if step % 3 == 0 {
            let wave = (step % 30) as f32;
            model.ingest_batch(
                [
                    wave / 12.0,
                    -15.0 + wave * 5.0,
                    79.4 + wave * 1.5,
                    90.0 + wave * 2.0,
                    -20.0 + wave * 3.0,
                    970.0 + wave * 3.0,
                    11.0 + wave / 8.0,
                    (10.0 + wave / 3.0) / 14.7,
                ],
                now,
            );
        }
        if step == 121 {
            model.reset_statistics(1);
        }
        let snapshot = model.snapshot();
        let overlay = match step % 37 {
            3..=5 => Some(Overlay::Reset),
            6 => Some(Overlay::Brightness(Brightness::Full)),
            7 => Some(Overlay::Brightness(Brightness::Dim)),
            8 => Some(Overlay::Brightness(Brightness::Off)),
            9..=11 => Some(Overlay::Fps(FpsMode::Combined)),
            12..=14 => Some(Overlay::BoostUnit(step % 2 == 0)),
            15..=20 => {
                Some(Overlay::Critical(
                    dashboard_host_validation::samples::SensorId::Egt,
                    step % 2 == 0,
                ))
            }
            _ => None,
        };
        let scene = DashboardScene {
            readings: None,
            layout: None,
            measurements: &snapshot,
            now_ms: now,
            status: dashboard_host_validation::vehicle_status::VehicleStatus::Demo,
            blink_on: step % 2 == 0,
            boost_psi: step % 7 < 3,
            fps_mode: if step % 11 == 0 {
                FpsMode::Combined
            } else {
                FpsMode::Off
            },
            fps_instant: 40.0 + (step % 3) as f32 * 0.3,
            fps_average: 39.0,
            backgrounds: if step % 4 == 0 {
                [colors::ORANGE; 8]
            } else {
                [colors::BLACK; 8]
            },
            overlay,
        };
        let buffer = if step % 5 == 0 { 0 } else { (step % 2) as usize };
        // Simulate another page overwriting a buffer; cache must be invalidated.
        if step % 41 == 0 {
            frames[buffer].0.fill(0x5A);
            cache.invalidate(buffer);
        }
        cache.draw(&mut St7789Renderer::new(&mut frames[buffer].0), buffer, &scene);
        scene.draw_full(&mut St7789Renderer::new(&mut reference.0));
        assert!(frames[buffer].0 == reference.0, "retained scene step {step}");
        let plan = DamagePlan::between(&panel.0, &frames[buffer].0);
        for region in plan.regions() {
            for range in region.payload_ranges() {
                panel.0[range.clone()].copy_from_slice(&frames[buffer].0[range]);
            }
        }
        assert!(panel.0 == reference.0, "panel step {step}");
    }
}

#[test]
fn public_untracked_snapshots_force_each_physical_buffer_dirty() {
    use dashboard_host_validation::acquisition::DashboardSnapshot;
    use dashboard_host_validation::samples::{DataMode, Quality, SampleMeta, Source};
    use dashboard_host_validation::sensor_state::{PeakMode, SensorState};
    let mut sensor = SensorState::new();
    sensor.ingest(90.0, 0, PeakMode::Maximum);
    let mut snapshot = DashboardSnapshot::EMPTY;
    snapshot.mode = DataMode::Demo;
    snapshot.sensors = [sensor.snapshot(); 8];
    snapshot.metadata = [SampleMeta {
        received_at_ms: Some(0),
        status_at_ms: 0,
        sequence: 1,
        source: Source::Demo,
        quality: Quality::Valid,
    }; 8];
    let scene = DashboardScene {
        readings: None,
        layout: None,
        measurements: &snapshot,
        now_ms: 0,
        status: dashboard_host_validation::vehicle_status::VehicleStatus::Demo,
        blink_on: false,
        boost_psi: false,
        fps_mode: FpsMode::Off,
        fps_instant: 0.0,
        fps_average: 0.0,
        backgrounds: [colors::BLACK; 8],
        overlay: None,
    };
    let mut cache = DashboardCache::new();
    let mut frames = [AlignedFrame::boxed(), AlignedFrame::boxed()];
    let mut reference = AlignedFrame::boxed();
    scene.draw_full(&mut St7789Renderer::new(&mut reference.0));
    for _ in 0..2 {
        for (buffer, frame) in frames.iter_mut().enumerate() {
            let redraw = cache.draw(&mut St7789Renderer::new(&mut frame.0), buffer, &scene);
            assert_eq!(redraw.cells, 255);
            assert!(frame.0 == reference.0);
        }
    }
}

#[test]
fn identities_cover_independent_histories_channel_reassignment_and_config_republication() {
    use dashboard_host_validation::configured_scene::Layout;
    use dashboard_host_validation::view_config::{self, GraphRange};
    let mut original = view_config::default_config();
    let first_config = view_config::validate(original.clone()).unwrap();
    original.widgets[1].display.graph_range = GraphRange::Fixed { min: 0.7, max: 1.3 };
    original.widgets[1].colors.value = view_config::Color(0xFFFF);
    // Use a known valid alternate visual configuration with unchanged IDs.
    let changed_config = view_config::validate(original).unwrap();
    let mut a = AcquisitionModel::demo_configured(&first_config);
    let mut b = AcquisitionModel::demo_configured(&first_config);
    a.ingest_batch([1.0, 70.0, 95.0, 80.0, 30.0, 750.0, 13.8, 1.0], 0);
    b.ingest_batch([1.0, 80.0, 95.0, 80.0, 30.0, 750.0, 13.8, 1.0], 0);
    a.ingest_batch([1.0, 90.0, 95.0, 80.0, 30.0, 750.0, 13.8, 1.0], 2000);
    b.ingest_batch([1.0, 90.0, 95.0, 80.0, 30.0, 750.0, 13.8, 1.0], 2000);
    let first = a.snapshot();
    let second = b.snapshot();
    let mut reassigned = second;
    reassigned.sensors.swap(1, 2);
    a.reset_statistics(1);
    a.ingest_batch([1.0, 90.0, 95.0, 80.0, 30.0, 750.0, 13.8, 1.0], 4000);
    let reset = a.snapshot();
    let mut cache = DashboardCache::new();
    let mut frames = [AlignedFrame::boxed(), AlignedFrame::boxed()];
    let mut reference = AlignedFrame::boxed();
    for (snapshot, config, now) in [
        (&first, &first_config, 2000),
        (&second, &first_config, 2000),
        (&reassigned, &first_config, 2000),
        (&second, &changed_config, 2000),
        (&reset, &first_config, 4000),
    ] {
        let scene = DashboardScene {
            readings: None,
            layout: Layout::new(config, config.config().boot_view, false),
            measurements: snapshot,
            now_ms: now,
            status: dashboard_host_validation::vehicle_status::VehicleStatus::Demo,
            blink_on: false,
            boost_psi: false,
            fps_mode: FpsMode::Off,
            fps_instant: 0.0,
            fps_average: 0.0,
            backgrounds: [colors::BLACK; 8],
            overlay: None,
        };
        scene.draw_full(&mut St7789Renderer::new(&mut reference.0));
        for (buffer, frame) in frames.iter_mut().enumerate() {
            cache.draw(&mut St7789Renderer::new(&mut frame.0), buffer, &scene);
            assert!(frame.0 == reference.0);
            assert_eq!(cache.prepare(buffer, &scene).cells, 0);
        }
    }
}
