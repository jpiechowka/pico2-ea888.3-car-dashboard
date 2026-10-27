//! Shared cylinder formatting, clipped pixels and retained-buffer correctness.
use dashboard_host_validation::acquisition::AcquisitionModel;
use dashboard_host_validation::configured_scene::{Layout, widget_text};
use dashboard_host_validation::dashboard::{DashboardCache, DashboardScene};
use dashboard_host_validation::display::St7789Renderer;
use dashboard_host_validation::ignition::{Cylinder, IgnitionSample};
use dashboard_host_validation::render::FpsMode;
use dashboard_host_validation::samples::{Quality, Source};
use dashboard_host_validation::vehicle_status::VehicleStatus;
use dashboard_host_validation::view_config::*;
use dashboard_host_validation::{AlignedFrame, colors};

/// Returns one admitted group at `span`/`orientation`, with `mode` and `decimals` precision (0..3).
fn configuration(
    span: Span,
    orientation: Orientation,
    mode: Readout,
    decimals: u8,
) -> ValidatedConfig {
    let mut raw = default_config();
    raw.widgets.clear();
    raw.orientation = orientation;
    let mut widget = default_ignition_widget(
        1,
        1,
        Placement {
            column: 0,
            row: 0,
            span,
        },
    );
    widget.advanced.readout = mode;
    widget.display.decimals = decimals;
    raw.widgets.push(widget).unwrap();
    validate(raw).unwrap()
}

/// Returns exact display lanes for the first group in `config`, using `model`
/// at `now` in monotonic milliseconds. Invalid built-in fixtures panic.
fn text(
    config: &ValidatedConfig,
    model: &AcquisitionModel<'_>,
    now: u64,
) -> [String; 6] {
    widget_text(
        &config.config().widgets[0],
        &model.snapshot(),
        now,
        false,
        config.stoich(),
        &config.config().fuel,
    )
    .map(|line| line.as_str().to_owned())
}

#[test]
/// Raw sign controls RET even when rounding hides magnitude; signed zero stays OK.
fn modes_keep_signed_values_and_never_turn_missing_into_ok() {
    let mut model = AcquisitionModel::demo();
    model.ingest_ignition_demo([0.0, -2.5, -0.00001, -0.0], 100);
    for (mode, expected) in [
        (Readout::IgnitionNumeric, ["0.0°", "−2.5°", "−0.0°", "0.0°"]),
        (Readout::IgnitionStatus, ["OK", "RET", "RET", "OK"]),
        (
            Readout::IgnitionStatusAndNumber,
            ["OK 0.0°", "RET −2.5°", "RET −0.0°", "OK 0.0°"],
        ),
    ] {
        let config = configuration(Span::OneByOne, Orientation::Landscape, mode, 1);
        assert_eq!(&text(&config, &model, 600)[..4], &expected);
        assert_eq!(&text(&config, &model, 601)[..4], &["STALE"; 4]);
        assert_eq!(&text(&config, &AcquisitionModel::new(), 0)[..4], &["N/A"; 4]);
        assert_eq!(&text(&config, &AcquisitionModel::demo(), 0)[..4], &["NO DATA"; 4]);
    }
}

#[test]
/// All precision/domain extremes fit complete 16-byte lanes; faults stay per-cylinder.
fn bounded_precision_and_independent_faults() {
    let mut model = AcquisitionModel::demo();
    model.ingest_ignition_demo([-327.68, 327.67, -1.25, 0.5], 100);
    for decimals in 0..=3 {
        let config = configuration(
            Span::OneByOne,
            Orientation::Portrait,
            Readout::IgnitionStatusAndNumber,
            decimals,
        );
        let lines = text(&config, &model, 100);
        assert_eq!(
            lines[0],
            format!("RET −{:.prec$}°", 327.68_f32, prec = decimals as usize)
        );
        assert_eq!(lines[1], format!("OK {:.prec$}°", 327.67_f32, prec = decimals as usize));
        assert!(lines.iter().all(|line| line.len() <= 16));
    }
    model.mark_ignition_fault(Cylinder::Two, Quality::TransportError, 101);
    let config = configuration(
        Span::OneByOne,
        Orientation::Landscape,
        Readout::IgnitionStatusAndNumber,
        1,
    );
    assert_eq!(
        &text(&config, &model, 101)[..4],
        &["RET −327.7°", "LINK ERR", "RET −1.2°", "OK 0.5°"]
    );
    model
        .ingest_ignition(
            IgnitionSample {
                cylinder: Cylinder::Three,
                degrees: f32::NAN,
                received_at_ms: 102,
                sequence: 2,
                source: Source::Demo,
            },
            102,
        )
        .unwrap_err();
    assert_eq!(text(&config, &model, 102)[2], "INVALID");
}

#[test]
/// All spans/mountings and precision modes repaint both buffers exactly through gaps.
fn retained_group_matches_full_renderer_on_each_cylinder_transition() {
    for orientation in Orientation::ALL {
        for span in [Span::OneByOne, Span::TwoByOne, Span::TwoByTwo] {
            for mode in [
                Readout::IgnitionNumeric,
                Readout::IgnitionStatus,
                Readout::IgnitionStatusAndNumber,
            ] {
                let config = configuration(span, orientation, mode, 3);
                let mut model = AcquisitionModel::demo();
                let mut cache = DashboardCache::new();
                let mut buffers = [AlignedFrame::boxed(), AlignedFrame::boxed()];
                let mut full = AlignedFrame::boxed();
                for step in 0..7 {
                    let now = match step {
                        0 => 0,
                        1 => 100,
                        2 => 200,
                        3 => 701,
                        4 => 702,
                        5 => 703,
                        _ => 704,
                    };
                    match step {
                        1 => {
                            model.ingest_ignition_demo([-327.68, 327.67, -0.00001, 0.0], now);
                        }
                        2 => {
                            model.ingest_ignition_demo([-327.68, 0.0, -0.00001, 0.0], now);
                        }
                        4 => {
                            model.ingest_ignition_demo([0.0, -2.5, 0.0, -1.0], now);
                        }
                        5 => {
                            model.mark_ignition_fault(Cylinder::Four, Quality::TransportError, now);
                        }
                        6 => {
                            model.mark_ignition_fault(Cylinder::One, Quality::InvalidPayload, now);
                        }
                        _ => {}
                    }
                    let snapshot = model.snapshot();
                    let scene = DashboardScene {
                        readings: None,
                        layout: Layout::new(&config, 1, false),
                        measurements: &snapshot,
                        now_ms: now,
                        status: VehicleStatus::Demo,
                        blink_on: false,
                        boost_psi: false,
                        fps_mode: FpsMode::Off,
                        fps_instant: 0.0,
                        fps_average: 0.0,
                        backgrounds: [colors::BLACK; 8],
                        overlay: None,
                    };
                    assert_eq!(scene.cell_sensor(0), None, "group must not borrow a scalar sensor");
                    assert_eq!(scene.gauge_reading(0), None);
                    scene.draw_full(&mut St7789Renderer::with_geometry(&mut full.0, orientation.geometry()));
                    for (index, frame) in buffers.iter_mut().enumerate() {
                        cache.draw(
                            &mut St7789Renderer::with_geometry(&mut frame.0, orientation.geometry()),
                            index,
                            &scene,
                        );
                        assert!(
                            frame.0 == full.0,
                            "{orientation:?} {span:?} {mode:?} step{step} buffer{index}"
                        );
                    }
                }
            }
        }
    }
}

/// Persistent ACK/latched/unavailable badges repaint both buffers in every span.
#[test]
fn retained_warning_badges_and_blink_do_not_hide_cylinder_quality() {
    use dashboard_host_validation::warning_state::WarningCommand;
    for orientation in Orientation::ALL {
        for span in [Span::OneByOne, Span::TwoByOne, Span::TwoByTwo] {
            let mut raw = configuration(span, orientation, Readout::IgnitionStatusAndNumber, 3)
                .config()
                .clone();
            raw.widgets[0].warning.on_ms = 100;
            raw.widgets[0].warning.off_ms = 100;
            raw.widgets[0].warning.advanced.latch = true;
            raw.widgets[0].warning.advanced.snooze_s = 5;
            raw.controls.screens[0].buttons[0].tap = ControlAction::AcknowledgeWarnings;
            let config = validate(raw).unwrap();
            let mut owner = AcquisitionModel::demo_configured(&config);
            let mut cache = DashboardCache::new();
            let mut buffers = [AlignedFrame::boxed(), AlignedFrame::boxed()];
            let mut full = AlignedFrame::boxed();
            let mut before_ack = AlignedFrame::boxed();
            for (step, now) in [0, 100, 101, 601, 602, 702].into_iter().enumerate() {
                match step {
                    0 | 1 => {
                        owner.ingest_ignition_demo([-327.68, 0.0, -0.001, 327.67], now);
                    }
                    2 => {
                        let previous = owner.snapshot();
                        assert_eq!(
                            owner
                                .apply_warning_command(
                                    WarningCommand {
                                        request_id: 1,
                                        run_id: previous.warning_run_id().unwrap(),
                                        observed_event_serial: previous.warning_event_serial(),
                                        rules: 1,
                                    },
                                    now
                                )
                                .accepted,
                            1
                        );
                    }
                    3 => {
                        owner.tick(now);
                    }
                    _ => {
                        owner.ingest_ignition_demo([0.0; 4], now);
                    }
                }
                let snapshot = owner.snapshot();
                for blink in [false, true] {
                    let scene = DashboardScene {
                        readings: None,
                        layout: Layout::new(&config, 1, false),
                        measurements: &snapshot,
                        now_ms: now,
                        status: VehicleStatus::Demo,
                        blink_on: blink,
                        boost_psi: false,
                        fps_mode: FpsMode::Off,
                        fps_instant: 0.0,
                        fps_average: 0.0,
                        backgrounds: [colors::BLACK; 8],
                        overlay: None,
                    };
                    scene.draw_full(&mut St7789Renderer::with_geometry(&mut full.0, orientation.geometry()));
                    for (index, frame) in buffers.iter_mut().enumerate() {
                        cache.draw(
                            &mut St7789Renderer::with_geometry(&mut frame.0, orientation.geometry()),
                            index,
                            &scene,
                        );
                        assert!(
                            frame.0 == full.0,
                            "{orientation:?} {span:?} step{step} blink{blink} buffer{index}"
                        );
                    }
                    if step == 1 && !blink {
                        before_ack.0.copy_from_slice(&full.0[..]);
                    }
                    if step == 2 {
                        assert!(
                            before_ack.0 != full.0,
                            "ACK badge must change actual pixels with unchanged values"
                        );
                    }
                }
                if step == 3 {
                    assert_eq!(&text(&config, &owner, now)[..4], &["STALE"; 4]);
                }
            }
        }
    }
}
