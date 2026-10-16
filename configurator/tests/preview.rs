use dashboard_configurator::config::*;
use dashboard_configurator::preview::*;
use dashboard_core::alarms::{RuleIndex, Status};
use dashboard_core::colors::{BLACK, RED};
use dashboard_core::configured_scene::Layout;
use dashboard_core::dashboard::{DashboardScene, Overlay};
use dashboard_core::display::{FrameBuffer, St7789Renderer};
use dashboard_core::render::FpsMode;
use dashboard_core::samples::{DataMode, Quality, SensorId, Source};
use dashboard_core::vehicle_status::VehicleStatus;

#[test]
fn candidate_observation_readings_never_address_accepted_rule_peaks_or_warnings() {
    use dashboard_core::acquisition::AcquisitionModel;
    use dashboard_core::presentation::PresentationState;
    let mut raw = default_config();
    raw.widgets.truncate(1);
    raw.widgets[0].warning.enabled = true;
    raw.widgets[0].warning.on_ms = 1;
    raw.widgets[0].warning.advanced.latch = true;
    raw.widgets[0].advanced.peak_mode = PeakSelection::Maximum;
    raw.controls.screens[0].buttons[0].tap = ControlAction::AcknowledgeWarnings;
    let accepted = validate(raw.clone()).unwrap();
    let mut model = AcquisitionModel::demo_configured(&accepted);
    model.ingest_batch([3., 90., 95., 80., 30., 700., 13.8, 1.], 1000);
    model.ingest_batch([4., 90., 95., 80., 30., 700., 13.8, 1.], 1001);
    let snapshot = model.snapshot();
    let before = snapshot;
    let mut ordinary = PresentationState::new();
    let readings = ordinary.prepare(&accepted, &snapshot, 1, 1001, false);
    assert!(readings.widget(0).unwrap().warning.visible());
    assert!(readings.widget(0).unwrap().peak.is_some());
    let ordinary_warning = readings.widget(0).unwrap().warning;
    // Candidate index zero is now an Oil widget, not the accepted Boost rule.
    raw.widgets[0] = default_widget(Channel::Oil, 77, 1, raw.widgets[0].placement);
    let candidate = validate(raw).unwrap();
    let mut scratch = PresentationState::new();
    let candidate_readings = scratch
        .prepare(&candidate, &snapshot, 1, 1001, false)
        .without_rule_state();
    let widget = candidate_readings.widget(0).unwrap();
    assert_eq!(widget.value, Some(90.));
    assert_eq!(widget.peak, None);
    assert_eq!(widget.warning.status, Status::Disabled);
    assert!(!widget.warning.latched);
    assert!(!widget.warning.cleared);
    assert!(!widget.warning.acknowledged);
    assert!(!widget.warning.snoozed);
    assert!(!widget.warning.visible());
    assert!(!widget.warning.attention());
    assert_eq!(snapshot, before);
    assert_eq!(
        snapshot.quality(SensorId::Oil, 1001),
        before.quality(SensorId::Oil, 1001)
    );
    assert_eq!(snapshot.publication_identity(), before.publication_identity());
    assert_eq!(
        ordinary
            .prepare(&accepted, &snapshot, 1, 1001, false)
            .widget(0)
            .unwrap()
            .warning,
        ordinary_warning
    );
}

#[test]
fn canvas_failures_retry_exact_pending_bytes_and_never_complete_new_scratch() {
    use dashboard_configurator::event_adapters::{RenderIssue, complete_simulation_pixels, consume_simulation_pixels};
    use dashboard_configurator::simulator::ControlSimulator;
    use dashboard_core::display::pipeline::{BufferId, FrameToken};
    let mut sim = ControlSimulator::new(fixture());
    let mut p = Preview::new();
    let token = FrameToken {
        buffer: BufferId::A,
        sequence: 1,
    };
    p.render_simulation_submitted(&mut sim, token).unwrap();
    let retained = p.rgba().to_vec();
    let old = sim.completed_warning();
    for failure in [
        RenderIssue::CanvasAllocation,
        RenderIssue::CanvasContext,
        RenderIssue::CanvasDraw,
    ] {
        assert_eq!(
            consume_simulation_pixels(&sim, &p, token, |pixels| {
                assert!(pixels == retained);
                Err(failure)
            }),
            Err(failure)
        );
        assert_eq!(sim.completed_warning(), old);
    }
    sim.advance_to(sim.now_ms() + 250, Some([3., 150., 150., 150., 80., 1000., 10., 1.4]));
    assert_eq!(p.render_simulation(&sim).err(), Some(PreviewError::PendingFrame));
    assert_eq!(
        p.render_appearance(sim.config(), 1, PreviewOptions::default()).err(),
        Some(PreviewError::PendingFrame)
    );
    complete_simulation_pixels(&mut sim, &mut p, token, |pixels| {
        assert!(pixels == retained);
        Ok(())
    })
    .unwrap();
    assert!(complete_simulation_pixels(&mut sim, &mut p, token, |_| Ok(())).is_err());
    let next = FrameToken {
        buffer: BufferId::A,
        sequence: 2,
    };
    p.render_simulation_submitted(&mut sim, next).unwrap();
    sim.restart(fixture());
    let mut called = false;
    assert!(
        consume_simulation_pixels(&sim, &p, next, |_| {
            called = true;
            Ok(())
        })
        .is_err()
    );
    assert!(!called);
}

#[test]
fn same_content_new_simulator_cannot_consume_previous_owners_pending_pixels() {
    use dashboard_configurator::event_adapters::consume_simulation_pixels;
    use dashboard_configurator::simulator::ControlSimulator;
    use dashboard_core::display::pipeline::{BufferId, FrameToken};
    let mut old = ControlSimulator::new(fixture());
    let mut preview = Preview::new();
    let token = FrameToken {
        buffer: BufferId::A,
        sequence: 1,
    };
    preview.render_simulation_submitted(&mut old, token).unwrap();
    let replacement = ControlSimulator::new(fixture());
    let mut called = false;
    assert!(
        consume_simulation_pixels(&replacement, &preview, token, |_| {
            called = true;
            Ok(())
        })
        .is_err()
    );
    assert!(!called);
}

#[test]
fn scenario_minmax_retains_raw_extrema_while_elapsed_display_is_smoothed() {
    use dashboard_configurator::simulator::{ControlSimulator, Scenario, SimulationInput};
    let mut raw = default_config();
    raw.widgets.truncate(1);
    raw.widgets[0].warning.enabled = false;
    raw.widgets[0].advanced.smoothing_ms = 5000;
    let mut sim = ControlSimulator::new(validate(raw).unwrap());
    sim.set_scenario(Scenario::MinMax).unwrap();
    let start = sim.now_ms();
    let mut p = Preview::new();
    sim.scenario_step(start).unwrap();
    p.render_simulation(&sim).unwrap();
    sim.inject(SimulationInput::Channel {
        channel: Channel::Boost,
        value: 10.,
        at_ms: start + 250,
    });
    p.render_simulation(&sim).unwrap();
    let snap = sim.snapshot();
    assert_eq!(snap.sensor(SensorId::Boost).minimum(), Some(-1.));
    assert_eq!(snap.sensor(SensorId::Boost).maximum(), Some(10.));
    let mut fresh = Preview::new();
    fresh.render_simulation(&sim).unwrap();
    assert!(
        p.rgb565() != fresh.rgb565(),
        "retained elapsed smoothing must differ from a fresh raw display"
    );
}

#[test]
fn all_page_round_trips_equal_fresh_full_raster_and_inspection_reads_live_samples() {
    use dashboard_configurator::editor::{Editor, RawField};
    use dashboard_configurator::simulator::ControlSimulator;
    let mut sim = ControlSimulator::new(fixture());
    sim.advance_to(sim.now_ms() + 1, Some([2., 90., 95., 80., 30., 700., 13.8, 1.]));
    let snapshot = sim.snapshot();
    let mut reused = Preview::new();
    for _ in 0..6 {
        reused.render_simulation(&sim).unwrap();
        let mut full = Preview::new();
        full.render_simulation(&sim).unwrap();
        assert!(reused.rgb565() == full.rgb565());
        sim.tap(ButtonId::Y);
    }
    let mut editor = Editor::default();
    editor
        .set_raw(editor.field_token(), RawField::ControlHoldMilliseconds, "bad".into())
        .unwrap();
    sim.inspect_invalid(editor.preview().config().controls);
    let accepted = sim.snapshot();
    let mut scratch = Preview::new();
    scratch.render_inspection(&sim, editor.diagnostic().as_ref()).unwrap();
    assert_eq!(
        scratch.snapshot().unwrap().sensor(SensorId::Boost).value(),
        snapshot.sensor(SensorId::Boost).value()
    );
    assert_eq!(sim.snapshot(), accepted);
    assert!(scratch.warning_page().is_none());
    sim.resume_accepted_bindings();
    reused.render_simulation(&sim).unwrap();
    let mut full = Preview::new();
    full.render_simulation(&sim).unwrap();
    assert!(reused.rgb565() == full.rgb565());
}

#[test]
fn invalid_dashboard_off_masks_diagnostics_and_recovery_preserves_accepted_pending_pixels() {
    use dashboard_configurator::editor::{Editor, RawField};
    use dashboard_configurator::event_adapters::complete_simulation_pixels;
    use dashboard_configurator::simulator::ControlSimulator;
    use dashboard_core::backlight::Brightness;
    use dashboard_core::display::pipeline::{BufferId, FrameToken};

    let mut editor = Editor::new(default_config()).unwrap();
    let mut sim = ControlSimulator::new(editor.preview().clone());
    sim.advance_to(sim.now_ms() + 1, Some([2., 90., 95., 80., 30., 700., 13.8, 1.]));
    let accepted_snapshot = sim.snapshot();
    let accepted_source = sim.source_key();
    let accepted_now = sim.now_ms();
    let accepted_compare = sim.session().backlight().compare();
    let accepted_completed = sim.completed_frame();
    let accepted_warning = sim.completed_warning();
    let mut timing = Preview::new();
    let token = FrameToken {
        buffer: BufferId::A,
        sequence: 55,
    };
    let pending_bytes = timing.render_simulation_submitted(&mut sim, token).unwrap().to_vec();

    editor
        .set_raw(editor.field_token(), RawField::ControlHoldMilliseconds, "bad".into())
        .unwrap();
    let mut draft = editor.diagnostic().unwrap();
    // Also exercise the optional warning-palette paint after the widget pass.
    draft.config.warning_colors.foreground = Color(0x07E0);
    draft.config.warning_colors.background = Color(0x001F);
    sim.inspect_invalid(draft.config.controls);
    let mut scratch = Preview::new();
    scratch.render_inspection(&sim, Some(&draft)).unwrap();
    assert_eq!(
        &scratch.rgb565()[..2],
        &[0xF8, 0x00],
        "visible invalid watermark before Off"
    );

    sim.hold(ButtonId::X); // Full -> Dim
    sim.hold(ButtonId::X); // Dim -> Off, with feedback still visible
    assert_eq!(sim.display_session().page(), ControlPage::Dashboard);
    assert_eq!(sim.display_session().backlight().brightness(), Brightness::Off);
    assert!(sim.display_session().backlight().compare() > 0);
    sim.advance(2100);
    assert_eq!(sim.display_session().backlight().compare(), 0);
    scratch.render_inspection(&sim, Some(&draft)).unwrap();
    assert!(
        scratch.rgb565().iter().all(|byte| *byte == 0),
        "Off must mask widgets, palette and watermark"
    );
    assert!(
        scratch
            .rgba()
            .as_chunks::<4>()
            .0
            .iter()
            .all(|pixel| *pixel == [0, 0, 0, 255])
    );

    sim.recover(); // Protected held Y restores candidate Full.
    assert_eq!(sim.display_session().backlight().brightness(), Brightness::Full);
    assert!(sim.display_session().backlight().compare() > 0);
    scratch.render_inspection(&sim, Some(&draft)).unwrap();
    assert_eq!(
        &scratch.rgb565()[..2],
        &[0xF8, 0x00],
        "recovery restores invalid diagnostic pixels"
    );

    assert_eq!(sim.session().page(), ControlPage::Dashboard);
    assert_eq!(sim.session().backlight().brightness(), Brightness::Full);
    assert_eq!(sim.session().backlight().compare(), accepted_compare);
    assert_eq!(sim.now_ms(), accepted_now);
    assert_eq!(sim.source_key(), accepted_source);
    assert_eq!(sim.snapshot(), accepted_snapshot);
    assert_eq!(sim.completed_frame(), accepted_completed);
    assert_eq!(sim.completed_warning(), accepted_warning);
    assert_eq!(timing.pending_frame(), Some(token));
    assert_eq!(timing.pending_pixels(&sim, token), Some(pending_bytes.as_slice()));
    assert!(
        !sim.complete_frame(token),
        "candidate cannot complete the accepted pending frame"
    );
    sim.resume_accepted_bindings();
    complete_simulation_pixels(&mut sim, &mut timing, token, |pixels| {
        assert_eq!(pixels, pending_bytes);
        Ok(())
    })
    .unwrap();
    assert_eq!(sim.completed_frame(), Some(token));
}

fn fixture() -> ValidatedConfig { validate(default_config()).unwrap() }

#[test]
fn unapplied_candidate_simulation_matches_control_units_popup_fps_view_and_time() {
    use dashboard_configurator::simulator::ControlSimulator;
    use dashboard_core::display::pipeline::{BufferId, FrameToken};
    use dashboard_core::popup::Popup;
    use dashboard_core::presentation::PresentationState;

    let mut raw = default_config();
    raw.views
        .push(View {
            orientation: None,
            id: 2,
            name: "SECOND".try_into().unwrap(),
        })
        .unwrap();
    for widget in &mut raw.widgets {
        widget.warning.enabled = false;
    }
    let mut extra = raw.widgets[0].clone();
    extra.id = 99;
    extra.view = 2;
    raw.widgets.push(extra).unwrap();
    let accepted = validate(raw.clone()).unwrap();
    let mut sim = ControlSimulator::new(accepted);
    sim.advance_to(sim.now_ms() + 1, Some([2., 90., 95., 80., 30., 700., 13.8, 1.]));
    let accepted_snapshot = sim.snapshot();
    let accepted_source = sim.source_key();
    let accepted_now = sim.now_ms();
    let accepted_session = (sim.session().page(), sim.session().boost_psi(), sim.session().popup());
    let accepted_warning = sim.completed_warning();
    let mut timing = Preview::new();
    let token = FrameToken {
        buffer: BufferId::A,
        sequence: 41,
    };
    let pending = timing.render_simulation_submitted(&mut sim, token).unwrap().to_vec();
    raw.views[0].name = "TRACK".try_into().unwrap();
    raw.views[1].name = "CANDIDATE".try_into().unwrap();
    // Replace membership/order too: pixels and controls must share this actual
    // admitted JSON candidate, rather than only testing renamed accepted IDs.
    raw.views[0].id = 9;
    raw.views[1].id = 4;
    raw.views.swap(0, 1);
    raw.boot_view = 9;
    for w in &mut raw.widgets {
        w.view = if w.view == 2 { 4 } else { 9 };
    }
    let mut editor = dashboard_configurator::editor::Editor::default();
    let handle = editor.open_json().unwrap();
    let parse = editor
        .set_json_text(handle, serde_json::to_string(&raw).unwrap())
        .unwrap();
    editor.finish_json_parse(parse).unwrap();
    let proof = editor.json_draft().unwrap().preview_proof().unwrap();
    let candidate = proof.config();
    sim.inspect_candidate(proof.clone());
    assert_eq!(sim.display_session().view_id(), 9);
    sim.tap(ButtonId::Y);
    sim.tap(ButtonId::Y);
    sim.tap(ButtonId::Y);
    sim.tap(ButtonId::A);
    assert!(sim.display_session().boost_psi());
    assert!(matches!(sim.display_session().popup(), Some(Popup::BoostUnit(_))));
    let mut scratch = Preview::new();
    for transition in 0..5 {
        match transition {
            1 => sim.advance(2100),    // Unit popup expires; PSI still applies.
            2 => sim.tap(ButtonId::X), // Candidate FPS feedback/header.
            3 => sim.rotate(1),        // Candidate selected view/name feedback.
            4 => sim.advance(3100),    // View popup expires; time remains candidate time.
            _ => {}
        }
        scratch.render_candidate_simulation(&sim, &proof).unwrap();
        let session = sim.display_session();
        let snapshot = sim.snapshot();
        let now = sim.display_now_ms();
        let mut presentation = PresentationState::new();
        let readings = presentation.prepare(candidate, &snapshot, session.view_id(), now, session.boost_psi());
        let scene = DashboardScene {
            readings: Some(&readings),
            layout: Layout::new(candidate, session.view_id(), false),
            measurements: &snapshot,
            status: VehicleStatus::Demo,
            now_ms: now,
            blink_on: dashboard_core::animations::blink_on(now),
            boost_psi: session.boost_psi(),
            fps_mode: session.fps(),
            fps_instant: 0.,
            fps_average: 0.,
            backgrounds: [BLACK; 8],
            overlay: Overlay::configured_for_page(session.page(), session.overlay(candidate), None),
        };
        let mut wanted = Box::new(FrameBuffer::new());
        scene.draw_full(&mut St7789Renderer::new(&mut wanted));
        assert!(
            scratch.rgb565() == &wanted[..],
            "shared candidate scene at transition {transition}"
        );
        assert_eq!(scratch.rendered_at_ms(), Some(now));
        assert_eq!(scratch.snapshot(), Some(&accepted_snapshot));
        assert!(scratch.warning_page().is_none());
        assert_eq!(sim.snapshot(), accepted_snapshot);
        assert_eq!(sim.source_key(), accepted_source);
        assert_eq!(sim.now_ms(), accepted_now);
        assert_eq!(
            (sim.session().page(), sim.session().boost_psi(), sim.session().popup()),
            accepted_session
        );
        assert_eq!(sim.completed_warning(), accepted_warning);
        assert_eq!(sim.completed_frame(), None);
        assert_eq!(timing.pending_pixels(&sim, token), Some(pending.as_slice()));
    }
    assert!(!sim.complete_frame(token));
    let parse = editor
        .set_json_text(handle, serde_json::to_string(&raw).unwrap())
        .unwrap();
    editor.finish_json_parse(parse).unwrap();
    let replacement = editor.json_draft().unwrap().preview_proof().unwrap();
    assert_eq!(
        scratch.render_candidate_simulation(&sim, &replacement),
        Err(PreviewError::InspectionOnly)
    );
}

#[test]
fn borrowed_debug_data_and_actual_map_draw_into_existing_preview_without_simulator_submission() {
    use dashboard_core::debug_view::{DebugData, DebugPage};
    let mut map = dashboard_config::ControlMap::default();
    map.screens[dashboard_config::ControlPage::Debug.index()].buttons[0].tap = dashboard_config::ControlAction::Logs;
    let data = DebugData {
        source_status: Some("synthetic"),
        config_fault: Some(false),
        ..Default::default()
    };
    let mut preview = Preview::new();
    let storage = preview.rgb565().as_ptr();
    let rgba = preview.render_debug(&data, &map);
    assert_eq!(rgba.len(), 320 * 240 * 4);
    assert_eq!(preview.rgb565().as_ptr(), storage, "one persistent framebuffer");
    let mut wanted = Box::new(FrameBuffer::new());
    DebugPage::new(&data, &map).draw(&mut St7789Renderer::new(&mut wanted));
    assert_eq!(preview.rgb565(), &wanted[..]);
}

#[test]
fn dense_gauge_markers_and_legends_reach_the_browser_with_shared_framebuffer_parity() {
    for kind in GaugeKind::ALL {
        let mut raw = default_config();
        raw.widgets.clear();
        let mut widget = default_widget(
            Channel::Lambda,
            1,
            1,
            Placement {
                column: 0,
                row: 0,
                span: Span::TwoByTwo,
            },
        );
        widget.gauge.kind = kind;
        widget.display.graph = false;
        widget.display.average = true;
        widget.display.extrema = true;
        widget.advanced.readout = Readout::LambdaAndAfr;
        widget.advanced.reference_mode = ReferenceMode::Manual;
        widget.advanced.reference = 1.;
        widget.advanced.peak_mode = PeakSelection::Minimum;
        widget
            .advanced
            .set_bands(&[
                Band::new(0.7, 0.9, Color(0x07E0), "ABCDE").unwrap(),
                Band::new(0.9, 1.1, Color(0x07FF), "FGHIJ").unwrap(),
                Band::new(1.1, 1.3, Color(0xF81F), "KLMNO").unwrap(),
            ])
            .unwrap();
        raw.widgets.push(widget).unwrap();
        let config = validate(raw).unwrap();
        let mut preview = Preview::new();
        let options = PreviewOptions {
            values: Some([1., 90., 80., 80., 30., 600., 12., 0.95]),
            ..Default::default()
        };
        preview.render_appearance(&config, 1, options).unwrap();
        assert_exact_shared_scene(&preview, &config, 1, options);
        for color in [0x07E0_u16, 0x07FF, 0xF81F] {
            assert_eq!(
                preview
                    .rgb565()
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .any(|p| *p == color.to_be_bytes()),
                !kind.is_numeric()
            );
        }
        if let Some(dir) = std::env::var_os("GAUGE_EVIDENCE_DIR") {
            use std::io::Write;
            let mut file =
                std::fs::File::create(std::path::PathBuf::from(dir).join(format!("dense-{kind:?}.ppm"))).unwrap();
            file.write_all(b"P6\n320 240\n255\n").unwrap();
            for pixel in preview.rgb565().as_chunks::<2>().0 {
                file.write_all(&Color(u16::from_be_bytes([pixel[0], pixel[1]])).rgb888())
                    .unwrap();
            }
        }
    }
}

#[test]
fn preview_observed_extrema_only_expand_until_rewind_or_sample_source_changes() {
    let config = fixture();
    let mut preview = Preview::new();
    let mut bounds = [(f32::INFINITY, f32::NEG_INFINITY); 8];
    for sample_ms in (0..240_000).step_by(3_001) {
        preview
            .render(
                &config,
                1,
                PreviewOptions {
                    sample_ms,
                    ..Default::default()
                },
            )
            .unwrap();
        for (i, id) in SensorId::ALL.into_iter().enumerate() {
            let sensor = preview.snapshot().unwrap().sensor(id);
            let current = (sensor.minimum().unwrap(), sensor.maximum().unwrap());
            assert!(
                current.0 <= bounds[i].0 && current.1 >= bounds[i].1,
                "preview shrank {id:?} at {sample_ms}"
            );
            bounds[i] = current;
        }
    }
    preview
        .render(
            &config,
            1,
            PreviewOptions {
                sample_ms: 0,
                values: Some([1.0; 8]),
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(preview.snapshot().unwrap().sensor(SensorId::Boost).maximum(), Some(1.0));
}

#[test]
fn browser_samples_match_the_firmware_demo_generator_at_the_actual_rendered_time() {
    let config = fixture();
    let mut preview = Preview::new();
    for sample_ms in [
        0,
        1,
        118_000,
        6_283_184,
        6_283_185,
        30 * 24 * 60 * 60 * 1000,
        u64::MAX - 118_001,
    ] {
        preview
            .render(
                &config,
                1,
                PreviewOptions {
                    sample_ms,
                    ..Default::default()
                },
            )
            .unwrap();
        let now = preview.rendered_at_ms().unwrap();
        let expected = dashboard_core::demo::values(now);
        for (sensor, value) in SensorId::ALL.into_iter().zip(expected) {
            assert_eq!(
                preview.snapshot().unwrap().sensor(sensor).value(),
                Some(value),
                "{sample_ms}, {sensor:?}"
            );
        }
    }
}

#[test]
fn stress_sets_have_bounded_values_and_recover_to_the_shared_demo() {
    let config = fixture();
    let mut preview = Preview::new();
    for set in [
        SampleSet::Minimum,
        SampleSet::Maximum,
        SampleSet::WidthStress,
        SampleSet::Zero,
        SampleSet::Demo,
    ] {
        for psi in [false, true] {
            let options = PreviewOptions {
                values: set.values(),
                boost_psi: psi,
                ..Default::default()
            };
            preview.render(&config, 1, options).unwrap();
            assert_exact_shared_scene(&preview, &config, 1, options);
        }
    }
}

#[test]
fn rgb565_is_big_endian_and_expands_every_channel_to_full_rgba_range() {
    let words = [0xF800_u16, 0x07E0, 0x001F, 0, 0xFFFF, 0x4208, 0x028A];
    let source: Vec<u8> = words.into_iter().flat_map(u16::to_be_bytes).collect();
    let mut output = vec![99; 28];
    rgb565_to_rgba(&source, &mut output).unwrap();
    assert_eq!(
        output,
        [
            255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 0, 0, 0, 255, 255, 255, 255, 255, 66, 65, 66, 255, 0, 81,
            82, 255,
        ]
    );
}

#[test]
fn bad_conversion_lengths_reject_without_partial_writes() {
    for (input, output_len) in [(vec![0; 1], 4), (vec![0; 4], 7), (vec![0; 4], 9), (vec![0; 2], 0)] {
        let mut output = vec![77; output_len];
        assert_eq!(rgb565_to_rgba(&input, &mut output), Err(PreviewError::BufferLength));
        assert_eq!(output, vec![77; output_len]);
    }
    assert_eq!(rgb565_to_rgba(&[], &mut []), Ok(()));
}

#[test]
fn preview_reuses_complete_buffers_and_populates_only_demo_histories() {
    let mut preview = Preview::new();
    let config = fixture();
    let rgba_ptr = preview.rgba().as_ptr();
    let frame_ptr = preview.rgb565().as_ptr();
    for sample_ms in [0, 3500] {
        let rgba = preview
            .render(
                &config,
                1,
                PreviewOptions {
                    sample_ms,
                    ..Default::default()
                },
            )
            .unwrap();
        assert_eq!(rgba.len(), 320 * 240 * 4);
        assert!(rgba.as_chunks::<4>().0.iter().all(|pixel| pixel[3] == 255));
        assert_eq!(rgba.as_ptr(), rgba_ptr);
        assert_eq!(preview.rgb565().len(), 320 * 240 * 2);
        assert_eq!(preview.rgb565().as_ptr(), frame_ptr);
        assert!(preview.rgb565().iter().any(|&byte| byte != 0));
        let snapshot = preview.snapshot().unwrap();
        let now = preview.rendered_at_ms().unwrap();
        assert_eq!(snapshot.mode, DataMode::Demo);
        assert_eq!(
            snapshot.sequence, 61,
            "work is a fixed 60 history points and one final dwell point"
        );
        for sensor in SensorId::ALL {
            assert_eq!(snapshot.metadata[sensor as usize].source, Source::Demo);
            assert_eq!(snapshot.quality(sensor, now), Quality::Valid);
            let state = &snapshot.sensors[sensor as usize];
            assert_eq!(
                state.history().count(),
                if sensor.stale_after_ms() < 2000 { 1 } else { 60 }
            );
            assert!(state.get_average().is_some());
            assert!(state.minimum().is_some());
            assert!(state.maximum().is_some());
        }
        assert_eq!(preview.warning_page(), None);
    }
}

fn assert_exact_shared_scene(
    preview: &Preview,
    config: &ValidatedConfig,
    view: u8,
    options: PreviewOptions,
) {
    let snapshot = preview.snapshot().unwrap();
    let now = preview.rendered_at_ms().unwrap();
    let layout = Layout::new(config, view, false).unwrap();
    let blink = options.warnings && dashboard_core::animations::blink_on(options.sample_ms);
    let scene = DashboardScene {
        readings: None,
        layout: Some(layout),
        measurements: snapshot,
        status: VehicleStatus::Demo,
        now_ms: now,
        blink_on: blink,
        boost_psi: options.boost_psi,
        fps_mode: FpsMode::Off,
        fps_instant: 0.0,
        fps_average: 0.0,
        backgrounds: core::array::from_fn(|slot| {
            if blink && layout.alarm_active(slot, snapshot, now) {
                RED
            } else {
                BLACK
            }
        }),
        overlay: preview.warning_page().map(Overlay::Warnings),
    };
    let mut expected = Box::new(FrameBuffer::new());
    scene.draw_full(&mut St7789Renderer::new(&mut expected));
    assert_eq!(
        preview.rgb565(),
        &expected[..],
        "preview must draw the production scene, including header, fonts, graph and overlay"
    );
    let mut rgba = vec![0; 320 * 240 * 4];
    rgb565_to_rgba(&expected[..], &mut rgba).unwrap();
    assert_eq!(preview.rgba(), &rgba);
}

#[test]
fn pixels_match_the_shared_renderer_for_both_units_and_warning_phases() {
    let config = fixture();
    let mut preview = Preview::new();
    for options in [
        PreviewOptions::default(),
        PreviewOptions {
            boost_psi: true,
            ..Default::default()
        },
        PreviewOptions {
            warnings: true,
            ..Default::default()
        },
        PreviewOptions {
            warnings: true,
            sample_ms: 500,
            ..Default::default()
        },
    ] {
        preview.render(&config, 1, options).unwrap();
        assert_exact_shared_scene(&preview, &config, 1, options);
    }
}

#[test]
fn psi_only_changes_boost_presentation_and_custom_samples_remain_canonical() {
    let config = fixture();
    let mut preview = Preview::new();
    let options = PreviewOptions {
        values: Some([1.8, 97.0, 89.0, 90.0, 32.0, 700.0, 13.8, 0.91]),
        ..Default::default()
    };
    preview.render(&config, 1, options).unwrap();
    let bar = preview.rgba().to_vec();
    let snapshot = *preview.snapshot().unwrap();
    preview
        .render(
            &config,
            1,
            PreviewOptions {
                boost_psi: true,
                ..options
            },
        )
        .unwrap();
    assert_ne!(preview.rgba(), bar);
    assert_eq!(preview.snapshot().unwrap(), &snapshot);
    assert_eq!(
        snapshot.value_at(SensorId::Boost, preview.rendered_at_ms().unwrap()),
        Some(1.8)
    );
    for (index, (old, new)) in bar
        .as_chunks::<4>()
        .0
        .iter()
        .zip(preview.rgba().as_chunks::<4>().0)
        .enumerate()
    {
        if old != new {
            assert!(
                index % 320 < 80 && (26..133).contains(&(index / 320)),
                "unit change escaped the boost cell"
            );
        }
    }
}

#[test]
fn invalid_view_samples_and_time_preserve_the_last_complete_preview() {
    let config = fixture();
    let mut preview = Preview::new();
    preview.render(&config, 1, PreviewOptions::default()).unwrap();
    let bytes = preview.rgba().to_vec();
    let frame = preview.rgb565().to_vec();
    let snapshot = *preview.snapshot().unwrap();
    let time = preview.rendered_at_ms();
    assert_eq!(
        preview.render(&config, 200, PreviewOptions::default()).unwrap_err(),
        PreviewError::UnknownView(200)
    );
    for value in [f32::NAN, f32::INFINITY, 1_000_001.0] {
        let mut values = [1.0; 8];
        values[3] = value;
        assert_eq!(
            preview
                .render(
                    &config,
                    1,
                    PreviewOptions {
                        values: Some(values),
                        ..Default::default()
                    }
                )
                .unwrap_err(),
            PreviewError::InvalidSample(Channel::Dsg)
        );
    }
    assert_eq!(
        preview
            .render(
                &config,
                1,
                PreviewOptions {
                    sample_ms: u64::MAX,
                    ..Default::default()
                }
            )
            .unwrap_err(),
        PreviewError::TimeOverflow
    );
    assert_eq!(preview.rgba(), bytes);
    assert_eq!(preview.rgb565(), frame);
    assert_eq!(preview.snapshot().unwrap(), &snapshot);
    assert_eq!(preview.rendered_at_ms(), time);
}

fn warnings_config() -> Config {
    let mut config = default_config();
    config.widgets.clear();
    for index in 0..4 {
        let mut widget = default_widget(
            Channel::Oil,
            u16::from(index) + 1,
            1,
            Placement {
                column: index,
                row: 0,
                span: Span::OneByOne,
            },
        );
        widget.warning.message = format!("Warning {}", index + 1).as_str().try_into().unwrap();
        config.widgets.push(widget).unwrap();
    }
    config
}

#[test]
fn immediate_warning_demo_compresses_dwell_without_changing_the_export_document() {
    let mut config = warnings_config();
    for widget in &mut config.widgets {
        widget.warning.on_ms = 30_000;
    }
    for source in &mut config.sources {
        source.freshness_ms = 1;
    }
    let config = validate(config).unwrap();
    let before = config.clone();
    let mut preview = Preview::new();
    preview
        .render(
            &config,
            1,
            PreviewOptions {
                warnings: true,
                ..Default::default()
            },
        )
        .unwrap();
    let snapshot = preview.snapshot().unwrap();
    let now = preview.rendered_at_ms().unwrap();
    for index in 0..4 {
        assert_eq!(
            snapshot.rule_status_at(RuleIndex::new(index).unwrap(), now),
            Status::Active
        );
    }
    let page = preview.warning_page().unwrap();
    assert_eq!(page.lines()[0].as_str(), "Warning 1");
    assert_eq!((page.number, page.total), (1, 4));
    assert_eq!(snapshot.sequence, 61);
    assert_eq!(config, before);
}

#[test]
fn warning_pages_use_configured_messages_modes_intervals_and_scope() {
    let mut config = warnings_config();
    config.warnings.mode = WarningMode::ListThree;
    config.warnings.page_ms = 500;
    config
        .views
        .push(View {
            orientation: None,
            id: 2,
            name: "Other".try_into().unwrap(),
        })
        .unwrap();
    config.widgets[3].view = 2;
    let mut config = validate(config).unwrap();
    let mut preview = Preview::new();
    for (sample_ms, number, names) in [
        (0, 1, vec!["Warning 1", "Warning 2", "Warning 3"]),
        (500, 2, vec!["Warning 4"]),
    ] {
        preview
            .render(
                &config,
                1,
                PreviewOptions {
                    sample_ms,
                    warnings: true,
                    ..Default::default()
                },
            )
            .unwrap();
        let page = preview.warning_page().unwrap();
        assert_eq!((page.number, page.total), (number, 2));
        assert_eq!(page.lines().iter().map(|s| s.as_str()).collect::<Vec<_>>(), names);
    }
    let mut edit = config.config().clone();
    edit.warnings.scope = WarningScope::CurrentView;
    config = validate(edit).unwrap();
    preview
        .render(
            &config,
            1,
            PreviewOptions {
                sample_ms: 501,
                warnings: true,
                ..Default::default()
            },
        )
        .unwrap();
    let page = preview.warning_page().unwrap();
    assert_eq!((page.number, page.total, page.lines().len()), (1, 1, 3));
    preview
        .render(
            &config,
            2,
            PreviewOptions {
                sample_ms: 502,
                warnings: true,
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(preview.warning_page().unwrap().lines()[0].as_str(), "Warning 4");
}

#[test]
fn warning_demo_preserves_threshold_enablement_and_synthetic_context_gates() {
    let mut config = default_config();
    for widget in &mut config.widgets {
        widget.warning.enabled = widget.channel == Channel::Oil || widget.channel == Channel::Lambda;
        if widget.channel == Channel::Oil {
            widget.warning.trigger = 170.0;
        }
        if widget.channel == Channel::Lambda {
            widget.warning.min_load_percent = 90.0;
        }
    }
    let config = validate(config).unwrap();
    let mut preview = Preview::new();
    preview
        .render(
            &config,
            1,
            PreviewOptions {
                warnings: true,
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(
        preview.warning_page(),
        None,
        "Oil synthetic 150 is below trigger; Lambda synthetic context load is 80%"
    );
}

#[test]
fn warning_toggle_rewind_and_document_replacement_restart_paging() {
    let config = validate(warnings_config()).unwrap();
    let mut preview = Preview::new();
    for (sample_ms, warnings, expected) in [
        (0, true, Some(1)),
        (2000, true, Some(2)),
        (2001, false, None),
        (2002, true, Some(1)),
        (4002, true, Some(2)),
        (0, true, Some(1)),
    ] {
        preview
            .render(
                &config,
                1,
                PreviewOptions {
                    sample_ms,
                    warnings,
                    ..Default::default()
                },
            )
            .unwrap();
        assert_eq!(preview.warning_page().map(|page| page.number), expected);
    }
    let mut changed = config.config().clone();
    changed.widgets[0].warning.message = "New message".try_into().unwrap();
    let changed = validate(changed).unwrap();
    preview
        .render(
            &changed,
            1,
            PreviewOptions {
                sample_ms: 2000,
                warnings: true,
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(preview.warning_page().unwrap().lines()[0].as_str(), "New message");
}

#[test]
fn clearing_view_removes_old_pixels_and_wide_layout_uses_exact_shared_scene() {
    let mut preview = Preview::new();
    let config = fixture();
    preview.render(&config, 1, PreviewOptions::default()).unwrap();
    let mut empty = config.config().clone();
    empty.widgets.clear();
    let empty = validate(empty).unwrap();
    preview.render(&empty, 1, PreviewOptions::default()).unwrap();
    assert!(preview.rgb565()[26 * 320 * 2..].iter().all(|&byte| byte == 0));
    assert_exact_shared_scene(&preview, &empty, 1, PreviewOptions::default());
    let mut wide = empty.config().clone();
    wide.widgets
        .push(default_widget(
            Channel::Lambda,
            64,
            1,
            Placement {
                column: 0,
                row: 0,
                span: Span::TwoByTwo,
            },
        ))
        .unwrap();
    wide.widgets
        .push(default_widget(
            Channel::Oil,
            4,
            1,
            Placement {
                column: 2,
                row: 0,
                span: Span::TwoByOne,
            },
        ))
        .unwrap();
    let wide = validate(wide).unwrap();
    preview.render(&wide, 1, PreviewOptions::default()).unwrap();
    assert_exact_shared_scene(&preview, &wide, 1, PreviewOptions::default());
}

#[test]
fn session_reporting_survives_replacement_failed_preview_and_clear() {
    use dashboard_configurator::cache_reporting::report;
    use dashboard_configurator::simulator::ControlSimulator;
    use dashboard_core::cache_diagnostics::{
        CacheExhaustionCause as Cause,
        CacheExhaustionSeen,
        CacheExhaustionStatus as Status,
    };
    let mut seen = CacheExhaustionSeen::new();
    let mut visible = Vec::new();
    let mut console_calls = 0;
    for _ in 0..3 {
        let config = fixture();
        let mut preview = Preview::new();
        assert!(preview.render(&config, 255, PreviewOptions::default()).is_err());
        let mut simulator = ControlSimulator::new(config.clone());
        simulator.restart(config);
        for status in [
            preview.cache_exhaustion_status(),
            simulator.cache_exhaustion_status(),
            Status::from_cause(Cause::ConfigIssuer),
            Status::from_cause(Cause::GraphRevision(Channel::Egt)),
        ] {
            report(&mut seen, status, |cause| visible.push(cause), |_| console_calls += 1);
        }
        // Clearing visible diagnostic history cannot reset the mounted Ui mask.
        visible.clear();
    }
    assert_eq!(console_calls, 2);
    let mut new_session = CacheExhaustionSeen::new();
    report(
        &mut new_session,
        Status::from_cause(Cause::ConfigIssuer),
        |_| {},
        |_| console_calls += 1,
    );
    assert_eq!(console_calls, 3);
}

#[test]
fn simulation_preview_borrows_persistent_samples_and_uses_real_dwell() {
    use dashboard_configurator::simulator::ControlSimulator;
    let mut raw = default_config();
    raw.widgets[0].warning.enabled = true;
    raw.widgets[0].warning.trigger = 1.5;
    raw.widgets[0].warning.clear = 1.;
    raw.widgets[0].warning.on_ms = 500;
    let mut sim = ControlSimulator::new(validate(raw).unwrap());
    let start = sim.now_ms();
    let mut preview = Preview::new();
    for (elapsed, expected) in [(0, Status::Pending), (250, Status::Pending), (500, Status::Active)] {
        sim.advance_to(start + elapsed, Some([2., 90., 95., 80., 30., 700., 13.8, 1.]));
        preview.render_simulation(&sim).unwrap();
        assert_eq!(preview.mode(), PreviewMode::Simulation);
        assert_eq!(preview.snapshot(), Some(&sim.snapshot()));
        assert_eq!(
            preview
                .snapshot()
                .unwrap()
                .rule_status_at(RuleIndex::new(0).unwrap(), sim.now_ms()),
            expected
        );
    }
    assert!(preview.warning_page().is_some());
    assert_eq!(preview.snapshot().unwrap().sensors[0].history().count(), 1);
    sim.advance_to(start + 1_000_000, None);
    preview.render_simulation(&sim).unwrap();
    assert_eq!(preview.snapshot().unwrap().sequence, 3);
    assert_eq!(preview.warning_page(), None);
    preview
        .render_appearance(sim.config(), 1, PreviewOptions::default())
        .unwrap();
    assert_eq!(preview.mode(), PreviewMode::Appearance);
}

#[test]
fn appearance_history_stays_fixed_work_for_every_duration_and_has_its_own_origin() {
    use dashboard_core::acquisition::AcquisitionModel;
    for seconds in [10, 30, 60, 120, 300, 600, 900, 137] {
        let mut raw = default_config();
        raw.history_seconds = [seconds; 8];
        let cfg = validate(raw).unwrap();
        let mut live = AcquisitionModel::demo_configured(&cfg);
        live.ingest_batch([1.; 8], 0);
        let live_snapshot = live.snapshot();
        let mut preview = Preview::new();
        preview.render_appearance(&cfg, 1, PreviewOptions::default()).unwrap();
        assert_eq!(preview.mode(), PreviewMode::Appearance);
        let generated = preview.snapshot().unwrap();
        assert_eq!(generated.sequence, 61, "fixed 60+1 synthetic observations");
        for i in 0..8 {
            let history = generated.sensors[i].history();
            assert_eq!(history.duration_seconds(), seconds);
            assert!(history.iter().count() <= 60);
            assert!(history.iter().any(|(_, value)| value.is_some()));
            assert_ne!(
                generated.sensors[i].graph_cache_identity(),
                live_snapshot.sensors[i].graph_cache_identity()
            );
        }
        if seconds == 10 {
            assert!(
                generated.sensors[1].history().iter().any(|(_, value)| value.is_none()),
                "sparse synthetic readings are not backfilled"
            );
        }
    }
}

#[test]
fn appearance_smoothing_continues_across_synthetic_owners_and_rejection_is_atomic() {
    let mut raw = dashboard_config::default_config();
    raw.widgets[0].advanced.smoothing_ms = 1000;
    let config = dashboard_config::validate(raw).unwrap();
    let mut preview = Preview::new();
    let mut options = PreviewOptions {
        values: Some([0., 90., 80., 80., 30., 600., 12., 1.]),
        ..Default::default()
    };
    preview.render_appearance(&config, 1, options).unwrap();
    options.sample_ms = 100;
    options.values.as_mut().unwrap()[0] = 1.;
    preview.render_appearance(&config, 1, options).unwrap();
    let smooth = preview.rgb565().to_vec();
    let mut fresh = Preview::new();
    fresh.render_appearance(&config, 1, options).unwrap();
    assert!(smooth != fresh.rgb565(), "appearance reconstruction reset the filter");
    let rgba = preview.rgba().to_vec();
    let before_revision = preview.snapshot().unwrap().publication_identity();
    let mut bad = options;
    bad.sample_ms = 200;
    bad.values.as_mut().unwrap()[0] = f32::NAN;
    assert!(preview.render_appearance(&config, 1, bad).is_err());
    assert!(preview.rgb565() == smooth);
    assert!(preview.rgba() == rgba);
    assert_eq!(preview.snapshot().unwrap().publication_identity(), before_revision);
    // Duplicate-time successful rendering is still the same prepared value.
    preview.render_appearance(&config, 1, options).unwrap();
    assert!(preview.rgb565() == smooth);
    // Backward visual time starts a new trajectory at the fresh input.
    options.sample_ms = 50;
    preview.render_appearance(&config, 1, options).unwrap();
    fresh.render_appearance(&config, 1, options).unwrap();
    assert!(preview.rgb565() == fresh.rgb565());
}

#[test]
fn simulation_owner_replacement_and_mode_config_view_reset_never_adopt_old_filters() {
    use dashboard_configurator::simulator::{ControlSimulator, SAMPLE_START_MS};
    let mut raw = default_config();
    raw.widgets[0].advanced.smoothing_ms = 1000;
    let cfg = validate(raw.clone()).unwrap();
    let mut sim = ControlSimulator::new(cfg.clone());
    let mut preview = Preview::new();
    sim.advance_to(SAMPLE_START_MS + 1, Some([0., 90., 80., 80., 30., 600., 12., 1.]));
    preview.render_simulation(&sim).unwrap();
    sim.advance_to(SAMPLE_START_MS + 101, Some([1., 90., 80., 80., 30., 600., 12., 1.]));
    preview.render_simulation(&sim).unwrap();
    let filtered = preview.rgb565().to_vec();
    let mut fresh = Preview::new();
    fresh.render_simulation(&sim).unwrap();
    assert!(filtered != fresh.rgb565());
    sim.restart(cfg.clone());
    sim.advance_to(SAMPLE_START_MS + 101, Some([1., 90., 80., 80., 30., 600., 12., 1.]));
    preview.render_simulation(&sim).unwrap();
    fresh.render_simulation(&sim).unwrap();
    assert!(preview.rgb565() == fresh.rgb565());
    // Switching through appearance cannot lend its trajectory back to Simulation.
    preview
        .render_appearance(
            &cfg,
            1,
            PreviewOptions {
                values: Some([0., 90., 80., 80., 30., 600., 12., 1.]),
                ..Default::default()
            },
        )
        .unwrap();
    preview.render_simulation(&sim).unwrap();
    assert!(preview.rgb565() == fresh.rgb565());
    // Changing immutable config at the same time also starts at raw current.
    raw.widgets[0].advanced.readout = Readout::Kpa;
    let changed = validate(raw).unwrap();
    let options = PreviewOptions {
        sample_ms: 100,
        values: Some([1., 90., 80., 80., 30., 600., 12., 1.]),
        ..Default::default()
    };
    preview.render_appearance(&changed, 1, options).unwrap();
    fresh = Preview::new();
    fresh.render_appearance(&changed, 1, options).unwrap();
    assert!(preview.rgb565() == fresh.rgb565());
}

#[test]
fn hidden_control_page_round_trip_without_intermediate_render_snaps_same_view() {
    use dashboard_configurator::simulator::{ControlSimulator, SAMPLE_START_MS};
    use dashboard_core::pages::Page;
    let mut raw = default_config();
    raw.widgets[0].advanced.smoothing_ms = 5000;
    let mut sim = ControlSimulator::new(validate(raw).unwrap());
    let mut preview = Preview::new();
    sim.advance_to(SAMPLE_START_MS + 1, Some([0., 90., 80., 80., 30., 600., 12., 1.]));
    preview.render_simulation(&sim).unwrap();
    sim.advance_to(SAMPLE_START_MS + 101, Some([1., 90., 80., 80., 30., 600., 12., 1.]));
    preview.render_simulation(&sim).unwrap();
    sim.tap(ButtonId::Y);
    assert_eq!(sim.session().page(), Page::Debug);
    sim.tap(ButtonId::Y);
    assert_eq!(sim.session().page(), Page::Logs);
    sim.tap(ButtonId::Y);
    assert_eq!(sim.session().page(), Page::Dashboard);
    assert_eq!(sim.session().view_id(), 1);
    assert!(sim.now_ms() - SAMPLE_START_MS < 5000);
    sim.advance_to(sim.now_ms() + 1, Some([1., 90., 80., 80., 30., 600., 12., 1.]));
    preview.render_simulation(&sim).unwrap();
    let mut fresh = Preview::new();
    fresh.render_simulation(&sim).unwrap();
    assert!(
        preview.rgb565() == fresh.rgb565(),
        "hidden cells resumed their old trajectory"
    );
}

#[test]
fn hidden_view_round_trip_without_intermediate_render_snaps_original_widgets() {
    use dashboard_configurator::simulator::{ControlSimulator, SAMPLE_START_MS};
    let mut raw = default_config();
    raw.widgets[0].advanced.smoothing_ms = 5000;
    raw.views
        .push(View {
            orientation: None,
            id: 2,
            name: "Other".try_into().unwrap(),
        })
        .unwrap();
    let mut widget = raw.widgets[0].clone();
    widget.id = 100;
    widget.view = 2;
    raw.widgets.push(widget).unwrap();
    let mut sim = ControlSimulator::new(validate(raw).unwrap());
    let mut preview = Preview::new();
    sim.advance_to(SAMPLE_START_MS + 1, Some([0., 90., 80., 80., 30., 600., 12., 1.]));
    preview.render_simulation(&sim).unwrap();
    sim.advance_to(SAMPLE_START_MS + 101, Some([1., 90., 80., 80., 30., 600., 12., 1.]));
    preview.render_simulation(&sim).unwrap();
    sim.rotate(1);
    assert_eq!(sim.session().view_id(), 2);
    sim.rotate(-1);
    assert_eq!(sim.session().view_id(), 1);
    sim.advance_to(sim.now_ms() + 1, Some([1., 90., 80., 80., 30., 600., 12., 1.]));
    preview.render_simulation(&sim).unwrap();
    let mut fresh = Preview::new();
    fresh.render_simulation(&sim).unwrap();
    assert!(
        preview.rgb565() == fresh.rgb565(),
        "hidden widgets resumed an old trajectory"
    );
}

#[test]
fn same_batch_view_round_trip_snaps_existing_trajectory_without_changing_acquisition() {
    use dashboard_configurator::simulator::{ControlSimulator, SAMPLE_START_MS};
    let mut raw = default_config();
    raw.widgets[0].advanced.smoothing_ms = 5000;
    raw.controls.screens[0].encoder_click = dashboard_config::ControlAction::NextView;
    raw.views
        .push(View {
            orientation: None,
            id: 2,
            name: "Other".try_into().unwrap(),
        })
        .unwrap();
    let mut widget = raw.widgets[0].clone();
    widget.id = 100;
    widget.view = 2;
    raw.widgets.push(widget).unwrap();
    let mut sim = ControlSimulator::new(validate(raw).unwrap());
    let mut preview = Preview::new();
    sim.advance_to(SAMPLE_START_MS + 1, Some([0., 90., 80., 80., 30., 600., 12., 1.]));
    preview.render_simulation(&sim).unwrap();
    sim.advance_to(SAMPLE_START_MS + 101, Some([1., 90., 80., 80., 30., 600., 12., 1.]));
    preview.render_simulation(&sim).unwrap();
    let mut fresh = Preview::new();
    fresh.render_simulation(&sim).unwrap();
    assert!(
        preview.rgb565() != fresh.rgb565(),
        "must establish a filtered trajectory"
    );
    let before = sim.snapshot();
    let ignored = sim.ignored();
    sim.encoder(0, 2);
    assert_eq!(sim.session().view_id(), 1);
    assert_eq!(sim.ignored(), ignored, "both NextView actions must be accepted");
    let after = sim.snapshot();
    assert_eq!(before.sequence, after.sequence);
    assert_eq!(before.reset_generation, after.reset_generation);
    for (before, after) in before.sensors.iter().zip(after.sensors.iter()) {
        assert_eq!(before.value().map(f32::to_bits), after.value().map(f32::to_bits));
        assert_eq!(before.minimum().map(f32::to_bits), after.minimum().map(f32::to_bits));
        assert_eq!(before.maximum().map(f32::to_bits), after.maximum().map(f32::to_bits));
        assert_eq!(
            before.get_average().map(f32::to_bits),
            after.get_average().map(f32::to_bits)
        );
        assert_eq!(before.graph_cache_identity(), after.graph_cache_identity());
        assert_eq!(
            before.history().iter().collect::<Vec<_>>(),
            after.history().iter().collect::<Vec<_>>()
        );
    }
    assert!(sim.now_ms() - SAMPLE_START_MS < 5000);
    preview.render_simulation(&sim).unwrap();
    fresh.render_simulation(&sim).unwrap();
    assert!(
        preview.rgb565() == fresh.rgb565(),
        "same-batch view round trip retained smoothing"
    );
}

#[test]
fn shared_blink_boundaries_match_appearance_simulation_and_palette() {
    use dashboard_configurator::simulator::ControlSimulator;
    let mut raw = warnings_config();
    for w in &mut raw.widgets {
        w.warning.on_ms = 1;
    }
    let config = validate(raw).unwrap();
    let mut preview = Preview::new();
    let mut sim = ControlSimulator::new(config.clone());
    let start = sim.now_ms();
    sim.advance_to(start, Some(dashboard_configurator::preview::WARNING_VALUES));
    for elapsed in [1, 199, 200, 201, 399, 400, 401, 599, 600] {
        let expected = dashboard_core::animations::blink_on(elapsed);
        sim.advance_to(start + elapsed, Some(dashboard_configurator::preview::WARNING_VALUES));
        preview.render_simulation(&sim).unwrap();
        assert_eq!(preview.warning_page().unwrap().blink, expected);
        preview
            .render_appearance(
                &config,
                1,
                PreviewOptions {
                    sample_ms: elapsed,
                    warnings: true,
                    ..Default::default()
                },
            )
            .unwrap();
        assert_eq!(preview.warning_page().unwrap().blink, expected);
        assert_exact_shared_scene(
            &preview,
            &config,
            1,
            PreviewOptions {
                sample_ms: elapsed,
                warnings: true,
                ..Default::default()
            },
        );
    }
    preview
        .render_appearance(
            &config,
            1,
            PreviewOptions {
                warnings: false,
                sample_ms: 400,
                ..Default::default()
            },
        )
        .unwrap();
    assert!(preview.warning_page().is_none());
}

#[test]
fn diagnostic_palette_uses_shared_phase_at_boundaries_and_long_skip() {
    use dashboard_configurator::diagnostic::Draft;
    use dashboard_configurator::editor::EditorError;
    let c = validate(default_config()).unwrap();
    let mut raw = c.config().clone();
    raw.warning_colors.foreground = Color(0x07E0);
    raw.warning_colors.background = Color(0x001F);
    let colors = raw.warning_colors;
    let draft = Draft {
        config: raw,
        raw: Vec::new(),
        error: EditorError::InvalidDraft,
    };
    let mut preview = Preview::new();
    for elapsed in [199, 200, 399, 400, 120_599, 120_600] {
        preview
            .render_diagnostic(
                &c,
                &draft,
                1,
                PreviewOptions {
                    sample_ms: elapsed,
                    ..Default::default()
                },
            )
            .unwrap();
        let color = if dashboard_core::animations::blink_on(elapsed) {
            colors.background
        } else {
            colors.foreground
        };
        let at = (60 * 320 + 60) * 2;
        assert_eq!(&preview.rgb565()[at..at + 2], &color.0.to_be_bytes());
    }
}

#[test]
fn simulation_feedback_expiry_selects_warning_in_the_same_preparation() {
    use dashboard_configurator::simulator::ControlSimulator;
    let mut raw = warnings_config();
    for w in &mut raw.widgets {
        w.warning.on_ms = 1;
    }
    let mut sim = ControlSimulator::new(validate(raw).unwrap());
    let start = sim.now_ms();
    sim.advance_to(start, Some(dashboard_configurator::preview::WARNING_VALUES));
    sim.advance_to(start + 1, Some(dashboard_configurator::preview::WARNING_VALUES));
    sim.tap(ButtonId::A);
    let expires = sim.session().popup().unwrap().start_time() + 3000;
    sim.advance_to(expires, Some(dashboard_configurator::preview::WARNING_VALUES));
    sim.advance_to(expires + 1, Some(dashboard_configurator::preview::WARNING_VALUES));
    // A second action creates feedback with all alarms freshly active.
    sim.tap(ButtonId::A);
    let expires = sim.session().popup().unwrap().start_time() + 3000;
    for now in (sim.now_ms() + 100..expires).step_by(100) {
        sim.advance_to(now, Some(dashboard_configurator::preview::WARNING_VALUES));
    }
    sim.advance_to(expires, Some(dashboard_configurator::preview::WARNING_VALUES));
    assert!(sim.session().popup().is_none());
    let mut preview = Preview::new();
    preview.render_simulation(&sim).unwrap();
    assert!(preview.warning_page().is_some());
}
