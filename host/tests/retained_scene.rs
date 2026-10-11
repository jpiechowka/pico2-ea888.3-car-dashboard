use dashboard_host_validation::acquisition::AcquisitionModel;
use dashboard_host_validation::backlight::Brightness;
use dashboard_host_validation::dashboard::{DashboardCache, DashboardScene, Overlay};
use dashboard_host_validation::display::St7789Renderer;
use dashboard_host_validation::display::damage::DamagePlan;
use dashboard_host_validation::render::FpsMode;
use dashboard_host_validation::{AlignedFrame, colors};

#[test]
fn extreme_cell_text_cannot_overwrite_a_neighbor_or_leave_old_glyphs() {
    let mut model = AcquisitionModel::demo();
    model.ingest_batch([-12345.5, 90.0, 95.0, 80.0, 30.0, 750.0, 13.8, 1.0], 0);
    let snapshot = model.snapshot();
    let scene = DashboardScene {
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
