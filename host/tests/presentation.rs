use dashboard_host_validation::acquisition::AcquisitionModel;
use dashboard_host_validation::configured_scene::widget_text;
use dashboard_host_validation::view_config::*;

#[test]
fn configured_readout_uses_affine_temperature_and_fixed_pressure_units() {
    let mut raw = default_config();
    raw.widgets[0].advanced.readout = Readout::Kpa;
    raw.widgets[0].advanced.secondary = Secondary::Bar;
    raw.widgets[0].display.decimals = 3;
    raw.widgets[3].advanced.readout = Readout::Fahrenheit;
    let config = validate(raw).unwrap();
    let mut model = AcquisitionModel::demo_configured(&config);
    assert!(model.ingest_batch([1., 100., 100., 100., 100., 100., 12., 1.], 0));
    let snapshot = model.snapshot();
    let text = |index, toggle| {
        widget_text(
            &config.config().widgets[index],
            &snapshot,
            0,
            toggle,
            config.stoich(),
            &config.config().fuel,
        )
    };
    assert_eq!(text(0, false)[0], "100.000");
    assert_eq!(text(0, true)[0], "100.000");
    assert_eq!(text(0, true)[1], "1.000 BAR");
    // Every temperature input above is canonical 100 C.
    assert_eq!(text(3, false)[0], "212F");
}

use dashboard_host_validation::presentation::{PresentationState, Unit, smoothing_alpha};

#[test]
fn conversion_basis_and_display_domain_are_bounded_and_fail_closed() {
    assert_eq!(Unit::Fahrenheit.value(0., 14.7), 32.);
    assert_eq!(Unit::Fahrenheit.value(100., 14.7), 212.);
    assert_eq!(Unit::Fahrenheit.difference(10., 14.7), 18.);
    assert_eq!(Unit::Kpa.value(1., 14.7), 100.);
    assert!((Unit::Psi.value(1., 14.7) as f64 - 14.503773773020923).abs() < 0.000001);
    assert!((Unit::Psi.value(-0.5, 14.7) as f64 + 7.2518868865104615).abs() < 0.000001);
    assert_eq!(Unit::Afr.value(0.8, 15.), 12.);
    for (unit, min, max) in [
        (Unit::Fahrenheit, -100., 2000.),
        (Unit::Kpa, -1., 10.),
        (Unit::Psi, -1., 10.),
        (Unit::Volts, 0., 65.535),
        (Unit::Afr, 0.01, 10.),
    ] {
        for canonical in [min, -0., 0., 0.00049, 0.00051, max] {
            for stoich in [5., 25.] {
                for decimals in [0, 3] {
                    let mut line = heapless::String::new();
                    dashboard_host_validation::presentation::format_value(
                        &mut line,
                        canonical,
                        unit,
                        stoich,
                        decimals,
                        "MAX ",
                        unit.suffix(),
                    );
                    assert!(!line.is_empty() && line.len() <= 16);
                    assert!(line == "--" || line.ends_with(unit.suffix()));
                }
            }
        }
    }
    let mut line = heapless::String::try_from("old").unwrap();
    dashboard_host_validation::presentation::format_value(&mut line, f32::MAX, Unit::Fahrenheit, 25., 3, "MAX ", "F");
    assert_eq!(line, "--");
}

#[test]
fn dense_exponential_domain_stays_within_display_tolerance() {
    // Accepted absolute alpha error 0.001: <=0.1% of a target step.
    let mut worst = 0.0_f64;
    for i in 0..=160_000 {
        let x = i as f64 / 10_000.;
        let got = smoothing_alpha(i, 10_000);
        let want = 1. - (-x).exp();
        worst = worst.max((got as f64 - want).abs());
        assert!((0.0..=1.0).contains(&got));
    }
    assert!(worst <= 0.001, "worst absolute alpha error {worst}");
    eprintln!("dense alpha max absolute error={worst}");
}

fn smooth_config() -> ValidatedConfig {
    let mut raw = default_config();
    raw.widgets[0].advanced.smoothing_ms = 1000;
    validate(raw).unwrap()
}
fn values(boost: f32) -> [f32; 8] { [boost, 90., 80., 80., 30., 600., 12., 1.] }

#[test]
fn visible_filter_advances_once_and_resets_for_time_stale_owner_view_and_statistics() {
    let cfg = smooth_config();
    let mut model = AcquisitionModel::demo_configured(&cfg);
    let mut state = PresentationState::new();
    model.ingest_batch(values(0.), 1000);
    assert_eq!(
        state
            .prepare(&cfg, &model.snapshot(), 1, 1000, false)
            .widget(0)
            .unwrap()
            .value,
        Some(0.)
    );
    model.ingest_batch(values(1.), 1100);
    let snapshot = model.snapshot();
    let prepared = state.prepare(&cfg, &snapshot, 1, 1100, false);
    let first = prepared.widget(0).unwrap().value.unwrap();
    assert!((first - 0.09516258).abs() < 0.001);
    for _ in 0..10 {
        assert_eq!(prepared.widget(0).unwrap().value.unwrap().to_bits(), first.to_bits());
    }
    assert_eq!(
        state.prepare(&cfg, &snapshot, 1, 1100, false).widget(0).unwrap().value,
        Some(first)
    );
    // Backwards time has a fresh eligible input and snaps rather than underflowing.
    model.ingest_batch(values(1.), 1100);
    assert_eq!(
        state.prepare(&cfg, &snapshot, 1, 1099, false).widget(0).unwrap().value,
        None
    );
    assert_eq!(
        state.prepare(&cfg, &snapshot, 1, 1100, false).widget(0).unwrap().value,
        Some(1.)
    );
    assert_eq!(
        state.prepare(&cfg, &snapshot, 1, 9999, false).widget(0).unwrap().value,
        None
    );
    model.ingest_batch(values(0.5), 10000);
    assert_eq!(
        state
            .prepare(&cfg, &model.snapshot(), 1, 10000, false)
            .widget(0)
            .unwrap()
            .value,
        Some(0.5)
    );
    state.prepare(&cfg, &model.snapshot(), 2, 10000, false);
    model.ingest_batch(values(1.), 10100);
    assert_eq!(
        state
            .prepare(&cfg, &model.snapshot(), 1, 10100, false)
            .widget(0)
            .unwrap()
            .value,
        Some(1.)
    );
    let mut replacement = AcquisitionModel::demo_configured(&cfg);
    replacement.ingest_batch(values(0.), 10101);
    assert_eq!(
        state
            .prepare(&cfg, &replacement.snapshot(), 1, 10101, false)
            .widget(0)
            .unwrap()
            .value,
        Some(0.)
    );
    replacement.ingest_batch(values(1.), 10200);
    replacement.reset_statistics(1);
    assert_eq!(
        state
            .prepare(&cfg, &replacement.snapshot(), 1, 10200, false)
            .widget(0)
            .unwrap()
            .value,
        Some(1.)
    );
}

#[test]
fn irregular_changing_targets_and_raw_statistics_have_independent_trajectories() {
    let cfg = smooth_config();
    let mut model = AcquisitionModel::demo_configured(&cfg);
    let mut state = PresentationState::new();
    let mut expected = 0_f64;
    let mut last = 0;
    for (now, target) in [
        (0, 0.),
        (17, 1.),
        (83, 1.),
        (170, 0.4),
        (220, 0.8),
        (500, 0.2),
        (600, 0.2),
        (20000, 1.),
    ] {
        model.ingest_batch(values(target), now);
        let snapshot = model.snapshot();
        let output = state
            .prepare(&cfg, &snapshot, 1, now, false)
            .widget(0)
            .unwrap()
            .value
            .unwrap();
        expected += (1. - (-((now - last) as f64) / 1000.).exp()) * (target as f64 - expected);
        assert!(
            (output as f64 - expected).abs() <= 0.002,
            "{now}: {output} vs {expected}"
        );
        assert_eq!(
            snapshot
                .value_at(dashboard_host_validation::samples::SensorId::Boost, now)
                .unwrap()
                .to_bits(),
            target.to_bits()
        );
        last = now;
    }
}

#[test]
fn prepared_text_gauge_and_retained_keys_share_one_filter_epoch() {
    use dashboard_host_validation::configured_scene::Layout;
    use dashboard_host_validation::dashboard::{DashboardCache, DashboardScene};
    use dashboard_host_validation::display::St7789Renderer;
    use dashboard_host_validation::render::FpsMode;
    use dashboard_host_validation::vehicle_status::VehicleStatus;
    use dashboard_host_validation::{AlignedFrame, colors};
    let mut raw = default_config();
    raw.widgets[0].advanced.smoothing_ms = 1000;
    raw.widgets[0].advanced.readout = Readout::Kpa;
    raw.widgets[0].gauge.kind = GaugeKind::Horizontal;
    raw.widgets[0].display.decimals = 3;
    let cfg = validate(raw).unwrap();
    let mut model = AcquisitionModel::demo_configured(&cfg);
    let mut state = PresentationState::new();
    model.ingest_batch(values(0.), 0);
    state.prepare(&cfg, &model.snapshot(), 1, 0, false);
    model.ingest_batch(values(1.), 100);
    let snapshot = model.snapshot();
    let readings = state.prepare(&cfg, &snapshot, 1, 100, false);
    let scene = DashboardScene {
        readings: Some(&readings),
        layout: Layout::new(&cfg, 1, false),
        measurements: &snapshot,
        status: VehicleStatus::Demo,
        now_ms: 100,
        blink_on: false,
        boost_psi: false,
        fps_mode: FpsMode::Off,
        fps_instant: 0.,
        fps_average: 0.,
        backgrounds: [colors::BLACK; 8],
        overlay: None,
    };
    assert_eq!(scene.configured_text(0).unwrap()[0], "9.516");
    let gauge = scene.gauge_reading(0).unwrap();
    assert!((gauge.value.unwrap() - 9.516).abs() < 0.001);
    assert_eq!(gauge.min, -100.);
    assert_eq!(gauge.max, 300.);
    let mut cache = DashboardCache::new();
    let mut frames = [AlignedFrame::boxed(), AlignedFrame::boxed()];
    let mut full = AlignedFrame::boxed();
    scene.draw_full(&mut St7789Renderer::new(&mut full.0));
    for (index, frame) in frames.iter_mut().enumerate() {
        cache.draw(&mut St7789Renderer::new(&mut frame.0), index, &scene);
        assert!(frame.0 == full.0);
        assert_eq!(cache.prepare(index, &scene).cells, 0);
        assert_eq!(cache.prepare(index, &scene).cells, 0);
    }
    assert_eq!(scene.configured_text(0).unwrap()[0], "9.516");
}

#[test]
fn all_readout_modes_obey_capabilities_and_keep_afr_assumptions_visible_only_with_afr() {
    let base = default_config();
    for kind in GaugeKind::ALL {
        for span in [Span::OneByOne, Span::TwoByOne, Span::TwoByTwo] {
            for (channel, readouts) in [
                (
                    Channel::Boost,
                    &[Readout::FollowBoost, Readout::Bar, Readout::Psi, Readout::Kpa][..],
                ),
                (
                    Channel::Lambda,
                    &[Readout::Lambda, Readout::Afr, Readout::LambdaAndAfr][..],
                ),
                (Channel::Water, &[Readout::Celsius, Readout::Fahrenheit][..]),
                (Channel::Battery, &[Readout::Volts][..]),
            ] {
                let mut widget = base.widgets.iter().find(|w| w.channel == channel).unwrap().clone();
                widget.gauge.kind = kind;
                widget.placement.span = span;
                for &readout in readouts {
                    widget.advanced.readout = readout;
                    widget.display.afr = readout.includes_afr();
                    for decimals in [0, 3] {
                        widget.display.decimals = decimals;
                        for toggle in [false, true] {
                            let mut model = AcquisitionModel::demo();
                            model.ingest_batch(values(1.), 0);
                            let snapshot = model.snapshot();
                            let text =
                                widget_text(&widget, &snapshot, 0, toggle, 25., &Fuel::Stoichiometric { ratio: 25. });
                            let secondary = kind.is_numeric() || span != Span::OneByOne;
                            if readout == Readout::LambdaAndAfr {
                                assert_eq!(!text[1].is_empty(), secondary);
                                assert_eq!(!text[5].is_empty(), secondary, "hidden AFR has no assumption line");
                            }
                            if readout == Readout::Afr {
                                assert_eq!(text[5], "S 25.00");
                            }
                            if readout == Readout::Lambda {
                                assert!(text[1].is_empty() && text[5].is_empty());
                            }
                            let stale =
                                widget_text(&widget, &snapshot, 100_000, toggle, 25., &Fuel::EquivalentGasoline);
                            assert_eq!(stale[0], "--");
                            assert!(stale[1..5].iter().all(|s| s.is_empty()));
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn duplicate_secondary_is_omitted_and_fixed_pressure_ignores_toggle() {
    let base = default_config();
    let mut widget = base.widgets[0].clone();
    let mut model = AcquisitionModel::demo();
    model.ingest_batch(values(-0.5), 0);
    let snapshot = model.snapshot();
    for (primary, secondary) in [
        (Readout::Bar, Secondary::Bar),
        (Readout::Psi, Secondary::Psi),
        (Readout::Kpa, Secondary::Kpa),
    ] {
        widget.advanced.readout = primary;
        widget.advanced.secondary = secondary;
        let a = widget_text(&widget, &snapshot, 0, false, 14.7, &Fuel::EquivalentGasoline);
        let b = widget_text(&widget, &snapshot, 0, true, 14.7, &Fuel::EquivalentGasoline);
        assert_eq!(a, b);
        assert!(a[1].is_empty());
    }
    widget.advanced.readout = Readout::FollowBoost;
    widget.advanced.secondary = Secondary::Bar;
    assert!(widget_text(&widget, &snapshot, 0, false, 14.7, &Fuel::EquivalentGasoline)[1].is_empty());
    assert_eq!(
        widget_text(&widget, &snapshot, 0, true, 14.7, &Fuel::EquivalentGasoline)[1],
        "-0.50 BAR"
    );
}

#[test]
fn smoothing_off_reduced_motion_and_raw_warning_extrema_peak_paths_remain_canonical() {
    use dashboard_host_validation::alarms::RuleIndex;
    use dashboard_host_validation::samples::SensorId;
    let mut raw = default_config();
    raw.widgets[0].advanced.smoothing_ms = 1000;
    raw.widgets[0].advanced.peak_mode = PeakSelection::Maximum;
    raw.widgets[0].warning.on_ms = 1;
    let smooth = validate(raw.clone()).unwrap();
    raw.widgets[0].advanced.smoothing_ms = 0;
    let off = validate(raw.clone()).unwrap();
    raw.widgets[0].advanced.smoothing_ms = 1000;
    raw.reduced_motion = true;
    let reduced = validate(raw).unwrap();
    let mut models = [
        AcquisitionModel::demo_configured(&smooth),
        AcquisitionModel::demo_configured(&off),
        AcquisitionModel::demo_configured(&reduced),
    ];
    let mut states = core::array::from_fn::<_, 3, _>(|_| PresentationState::new());
    for (now, target) in [(0, 0.), (100, 3.), (200, 3.), (300, -0.5), (400, 0.3)] {
        for model in &mut models {
            model.ingest_batch(values(target), now);
        }
        let snapshots = models.each_ref().map(|m| m.snapshot());
        for i in 0..3 {
            let cfg = [&smooth, &off, &reduced][i];
            let reading = states[i].prepare(cfg, &snapshots[i], 1, now, false).widget(0).unwrap();
            if i > 0 {
                assert_eq!(reading.value.unwrap().to_bits(), target.to_bits());
            }
            let raw = snapshots[i].sensor(SensorId::Boost);
            let original = snapshots[0].sensor(SensorId::Boost);
            assert_eq!(raw.minimum().map(f32::to_bits), original.minimum().map(f32::to_bits));
            assert_eq!(raw.maximum().map(f32::to_bits), original.maximum().map(f32::to_bits));
            assert_eq!(
                raw.get_average().map(f32::to_bits),
                original.get_average().map(f32::to_bits)
            );
            assert_eq!(
                reading.peak.map(f32::to_bits),
                snapshots[0].peak_at(RuleIndex::new(0).unwrap(), now).map(f32::to_bits)
            );
            assert_eq!(
                reading.warning,
                snapshots[0].warning_eligibility(RuleIndex::new(0).unwrap(), now)
            );
        }
    }
}

#[test]
fn step_response_is_monotonic_bounded_and_stalls_snap_without_overflow() {
    let cfg = smooth_config();
    let mut model = AcquisitionModel::demo_configured(&cfg);
    let mut state = PresentationState::new();
    model.ingest_batch(values(0.), 0);
    state.prepare(&cfg, &model.snapshot(), 1, 0, false);
    let mut last = 0.;
    for now in (1..=2000).step_by(17) {
        model.ingest_batch(values(1.), now);
        let y = state
            .prepare(&cfg, &model.snapshot(), 1, now, false)
            .widget(0)
            .unwrap()
            .value
            .unwrap();
        assert!(y >= last && y <= 1.);
        last = y;
    }
    model.ingest_batch(values(0.), 20_000);
    assert_eq!(
        state
            .prepare(&cfg, &model.snapshot(), 1, 20_000, false)
            .widget(0)
            .unwrap()
            .value,
        Some(0.)
    );
    assert_eq!(smoothing_alpha(u64::MAX, 50), 1.);
    assert_eq!(smoothing_alpha(0, 50), 0.);
    assert_eq!(smoothing_alpha(0, 0), 1.);
}

#[test]
fn identity_exhaustion_and_source_changes_cannot_reuse_filtered_value() {
    use dashboard_host_validation::acquisition::DashboardSnapshot;
    use dashboard_host_validation::samples::{DataMode, Source};
    let cfg = smooth_config();
    let mut model = AcquisitionModel::demo_configured(&cfg);
    let mut state = PresentationState::new();
    model.ingest_batch(values(0.), 0);
    state.prepare(&cfg, &model.snapshot(), 1, 0, false);
    model.ingest_batch(values(1.), 100);
    let snapshot = model.snapshot();
    assert!(
        state
            .prepare(&cfg, &snapshot, 1, 100, false)
            .widget(0)
            .unwrap()
            .value
            .unwrap()
            < 1.
    );
    // Public untracked snapshots cannot accidentally adopt a tracked owner's state.
    let mut untracked = DashboardSnapshot::EMPTY;
    untracked.sensors = snapshot.sensors;
    untracked.metadata = snapshot.metadata;
    untracked.mode = DataMode::Demo;
    assert_eq!(
        state.prepare(&cfg, &untracked, 1, 101, false).widget(0).unwrap().value,
        Some(1.)
    );
    // An unavailable/mismatched source clears immediately; valid recovery snaps.
    let mut bad = snapshot;
    bad.metadata[0].source = Source::Unknown;
    assert_eq!(state.prepare(&cfg, &bad, 1, 102, false).widget(0).unwrap().value, None);
    assert_eq!(
        state.prepare(&cfg, &snapshot, 1, 103, false).widget(0).unwrap().value,
        Some(1.)
    );
}

#[test]
fn converted_secondary_and_fuel_only_changes_repair_both_prepared_framebuffers() {
    use dashboard_host_validation::configured_scene::Layout;
    use dashboard_host_validation::dashboard::{DashboardCache, DashboardScene};
    use dashboard_host_validation::display::St7789Renderer;
    use dashboard_host_validation::render::FpsMode;
    use dashboard_host_validation::vehicle_status::VehicleStatus;
    use dashboard_host_validation::{AlignedFrame, colors};
    let mut raw = default_config();
    raw.widgets[0].advanced.secondary = Secondary::Psi;
    raw.widgets[0].display.decimals = 0;
    raw.widgets[0].display.extrema = false;
    raw.widgets[0].display.average = false;
    raw.widgets[1].display.decimals = 0;
    raw.widgets[1].display.graph = false;
    let cfg = validate(raw.clone()).unwrap();
    raw.fuel = Fuel::Stoichiometric { ratio: 14.7 };
    let fuel = validate(raw).unwrap();
    let mut model = AcquisitionModel::demo_configured(&cfg);
    let mut presentation = PresentationState::new();
    let mut cache = DashboardCache::new();
    let mut frames = [AlignedFrame::boxed(), AlignedFrame::boxed()];
    let mut full = AlignedFrame::boxed();
    let mut previous_primary = None;
    let mut previous_secondary = None;
    for (now, boost, lambda, config) in [(0, 1., 1., &cfg), (1, 1.08, 1.01, &cfg), (2, 1.08, 1.01, &fuel)] {
        let mut batch = values(boost);
        batch[7] = lambda;
        model.ingest_batch(batch, now);
        let snapshot = model.snapshot();
        let readings = presentation.prepare(config, &snapshot, 1, now, false);
        let scene = DashboardScene {
            readings: Some(&readings),
            layout: Layout::new(config, 1, false),
            measurements: &snapshot,
            status: VehicleStatus::Demo,
            now_ms: now,
            blink_on: false,
            boost_psi: false,
            fps_mode: FpsMode::Off,
            fps_instant: 0.,
            fps_average: 0.,
            backgrounds: [colors::BLACK; 8],
            overlay: None,
        };
        let primary = scene.configured_text(0).unwrap()[0].clone();
        let secondary = scene.configured_text(0).unwrap()[1].clone();
        if now == 1 {
            assert_eq!(Some(&primary), previous_primary.as_ref());
            assert_ne!(Some(&secondary), previous_secondary.as_ref());
        }
        previous_primary = Some(primary);
        previous_secondary = Some(secondary);
        scene.draw_full(&mut St7789Renderer::new(&mut full.0));
        for (index, frame) in frames.iter_mut().enumerate() {
            let redraw = cache.draw(&mut St7789Renderer::new(&mut frame.0), index, &scene);
            assert_ne!(redraw.cells & 3, 0);
            assert!(frame.0 == full.0);
        }
    }
}

#[test]
fn duplicate_time_preserves_signed_zero_and_long_stall_snaps_exact_target_bits() {
    let cfg = smooth_config();
    let mut model = AcquisitionModel::demo_configured(&cfg);
    let mut state = PresentationState::new();
    model.ingest_batch(values(-0.0), 0);
    state.prepare(&cfg, &model.snapshot(), 1, 0, false);
    model.ingest_batch(values(1.), 1);
    let snapshot = model.snapshot();
    // A duplicate display timestamp uses the prior immutable frame value.
    // The old snapshot is fresh at zero and preserves its signed-zero bits.
    let mut zero_model = AcquisitionModel::demo_configured(&cfg);
    zero_model.ingest_batch(values(-0.0), 0);
    let zero = zero_model.snapshot();
    state.prepare(&cfg, &zero, 1, 0, false);
    assert_eq!(
        state
            .prepare(&cfg, &zero, 1, 0, false)
            .widget(0)
            .unwrap()
            .value
            .unwrap()
            .to_bits(),
        (-0.0_f32).to_bits()
    );
    state.prepare(&cfg, &snapshot, 1, 1, false);
    model.ingest_batch(values(0.1), 20_000);
    assert_eq!(
        state
            .prepare(&cfg, &model.snapshot(), 1, 20_000, false)
            .widget(0)
            .unwrap()
            .value
            .unwrap()
            .to_bits(),
        0.1_f32.to_bits()
    );
}

#[test]
fn transformed_domain_endpoints_and_sign_rounding_have_explicit_text_oracles() {
    use dashboard_host_validation::presentation::format_value;
    for (unit, x, stoich, decimals, want) in [
        (Unit::Fahrenheit, -100., 14.7, 0, "-148F"),
        (Unit::Fahrenheit, -100., 14.7, 3, "-148.000F"),
        (Unit::Fahrenheit, 2000., 14.7, 0, "3632F"),
        (Unit::Fahrenheit, 2000., 14.7, 3, "3632.000F"),
        (Unit::Kpa, -1., 14.7, 3, "-100.000 kPa"),
        (Unit::Kpa, 10., 14.7, 0, "1000 kPa"),
        (Unit::Psi, -1., 14.7, 3, "-14.504 PSI"),
        (Unit::Psi, 10., 14.7, 3, "145.038 PSI"),
        (Unit::Volts, 65.535, 14.7, 3, "65.535V"),
        (Unit::Volts, 65.535, 14.7, 0, "66V"),
        (Unit::Afr, 10., 25., 3, "250.000"),
        (Unit::Afr, 0.01, 5., 3, "0.050"),
        (Unit::Bar, -0.00049, 14.7, 3, "-0.000 BAR"),
        (Unit::Bar, -0.00051, 14.7, 3, "-0.001 BAR"),
    ] {
        let mut out = heapless::String::new();
        format_value(&mut out, x, unit, stoich, decimals, "", unit.suffix());
        assert_eq!(out, want);
        for span in [Span::OneByOne, Span::TwoByOne, Span::TwoByTwo] {
            let rect = Placement {
                column: 0,
                row: 0,
                span,
            }
            .rect()
            .unwrap();
            assert!(
                dashboard_host_validation::widgets::value_text::fit_value(
                    &out,
                    embedded_graphics::geometry::Size::new(u32::from(rect.width) - 4, 22),
                    3
                )
                .is_some()
            );
        }
    }
}
