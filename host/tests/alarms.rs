//! Synthetic alarm qualification, hysteresis/dwell and producer-owned warning publication.
use dashboard_host_validation::acquisition::AcquisitionModel;
use dashboard_host_validation::alarms::{AlarmEngine, Context, EgtLocation, Limit, Rule, Status};
use dashboard_host_validation::backlight::Brightness;
use dashboard_host_validation::samples::{SensorId, Source};

/// Returns a demo-only rule for sensor `id` and canonical `limit`, with 500ms
/// activation/300ms clearing dwell; panics if the fixture policy is invalid.
fn policy(
    id: SensorId,
    limit: Limit,
) -> Rule {
    Rule::new(id, Source::Demo, limit, 500, 300, "synthetic test only").unwrap()
}
/// Returns an engine enabling only `id` with canonical `limit` and the synthetic
/// fixture's dwell policy; panics if rule/engine validation fails.
fn engine(
    id: SensorId,
    limit: Limit,
) -> AlarmEngine {
    let mut rules = [None; 8];
    rules[id as usize] = Some(policy(id, limit));
    AlarmEngine::new(rules).unwrap()
}
/// Publishes `value` in `id`'s canonical unit into demo `model` at milliseconds
/// `at`, retaining nominal other channels; panics if batch admission fails.
fn samples(
    model: &mut AcquisitionModel,
    id: SensorId,
    value: f32,
    at: u64,
) {
    let mut values = [1.0, 90.0, 95.0, 80.0, 30.0, 700.0, 13.8, 1.0];
    values[id as usize] = value;
    assert!(model.ingest_batch(values, at));
}
/// Returns context only for `id`, bound to `model`'s latest source/sequence/time,
/// with running/warm/no-cut, 80% load, target lambda 0.8 and measured manifold EGT.
/// Panics if that sensor has never received a timestamped sample.
fn context(
    model: &AcquisitionModel,
    id: SensorId,
) -> [Option<Context>; 8] {
    let snapshot = model.snapshot();
    let meta = snapshot.metadata[id as usize];
    let mut contexts = [None; 8];
    contexts[id as usize] = Some(Context {
        source: meta.source,
        sequence: meta.sequence,
        received_at_ms: meta.received_at_ms.unwrap(),
        running: Some(true),
        warm: Some(true),
        load_percent: Some(80.0),
        fuel_cut: Some(false),
        target_lambda: Some(0.8),
        egt_location: EgtLocation::MeasuredManifold,
    });
    contexts
}

/// New qualified samples drive exact activation/clear dwell; repeatedly evaluating one sample cannot finish activation.
#[test]
fn hysteresis_and_dwell_require_new_samples_not_repeated_render_ticks() {
    let id = SensorId::Water;
    let mut model = AcquisitionModel::demo();
    let mut engine = engine(id, Limit::High { on: 110.0, off: 105.0 });
    for (at, value, active) in [
        (0, 95.0, false),
        (10, 111.0, false),
        (509, 112.0, false),
        (510, 110.0, true),
        (600, 107.0, true),
        (700, 105.0, true),
        (999, 104.0, true),
        (1000, 105.0, false),
    ] {
        samples(&mut model, id, value, at);
        assert_eq!(engine.update(&model.snapshot(), &[None; 8], at).active(id), active);
        if at == 10 {
            assert!(!engine.update(&model.snapshot(), &[None; 8], 508).active(id));
        }
    }
}

/// Invalid threshold geometry, wrong quantity, unknown provenance, zero dwell and empty policy labels fail rule
/// admission.
#[test]
fn invalid_rules_cannot_enable_thresholds_or_claim_an_unknown_source() {
    for limit in [
        Limit::High { on: f32::NAN, off: 1.0 },
        Limit::High { on: 1.0, off: 1.0 },
        Limit::Low { on: 2.0, off: 1.0 },
        Limit::LambdaDeviation {
            on: 0.2,
            off: 0.1,
            min_load: 60.0,
        },
    ] {
        assert!(Rule::new(SensorId::Water, Source::Demo, limit, 500, 300, "test").is_err());
    }
    assert!(
        Rule::new(
            SensorId::Water,
            Source::Unknown,
            Limit::High { on: 110.0, off: 105.0 },
            500,
            300,
            "test"
        )
        .is_err()
    );
    assert!(
        Rule::new(
            SensorId::Water,
            Source::Demo,
            Limit::High { on: 110.0, off: 105.0 },
            0,
            300,
            "test"
        )
        .is_err()
    );
    assert!(
        Rule::new(
            SensorId::Water,
            Source::Demo,
            Limit::High { on: 110.0, off: 105.0 },
            500,
            300,
            ""
        )
        .is_err()
    );
}

/// Lambda deviation requires fresh matching source/sequence plus running, warm, loaded, no-cut and target context.
#[test]
fn lambda_requires_matching_fresh_running_warm_loaded_non_fuel_cut_context() {
    let id = SensorId::Lambda;
    let mut model = AcquisitionModel::demo();
    let mut engine = engine(
        id,
        Limit::LambdaDeviation {
            on: 0.15,
            off: 0.05,
            min_load: 60.0,
        },
    );
    samples(&mut model, id, 0.8, 0); // valid enrichment matching commanded lambda
    assert!(!engine.update(&model.snapshot(), &context(&model, id), 0).active(id));
    for at in [10, 510] {
        samples(&mut model, id, 1.1, at);
        engine.update(&model.snapshot(), &context(&model, id), at);
    }
    assert!(engine.snapshot().active(id));
    let valid = context(&model, id)[id as usize].unwrap();
    for invalid in [
        Context {
            fuel_cut: Some(true),
            ..valid
        },
        Context {
            running: Some(false),
            ..valid
        },
        Context {
            warm: Some(false),
            ..valid
        },
        Context {
            load_percent: Some(30.0),
            ..valid
        },
        Context {
            target_lambda: None,
            ..valid
        },
        Context { sequence: 1, ..valid },
        Context {
            source: Source::Unknown,
            ..valid
        },
        Context {
            received_at_ms: 511,
            ..valid
        },
    ] {
        let mut contexts = [None; 8];
        contexts[id as usize] = Some(invalid);
        assert!(!engine.update(&model.snapshot(), &contexts, 510).active(id));
    }
}

/// Disabled rules, freshness gaps and clock reversal suppress active output and restart sustained-condition
/// qualification.
#[test]
fn disabled_unknown_stale_gap_and_clock_failure_suppress_alarms() {
    let id = SensorId::Water;
    let mut model = AcquisitionModel::demo();
    let mut engine = engine(id, Limit::High { on: 110.0, off: 105.0 });
    for at in [0, 500] {
        samples(&mut model, id, 120.0, at);
        engine.update(&model.snapshot(), &[None; 8], at);
    }
    assert!(engine.snapshot().active(id));
    assert!(!engine.update(&model.snapshot(), &[None; 8], 3501).active(id));
    samples(&mut model, id, 120.0, 4000);
    assert!(!engine.update(&model.snapshot(), &[None; 8], 4000).active(id));
    assert!(!engine.update(&model.snapshot(), &[None; 8], 3999).active(id));
    assert!(
        !AlarmEngine::new([None; 8])
            .unwrap()
            .update(&model.snapshot(), &[None; 8], 4000)
            .active(id)
    );
}

/// Rejected context consumes its sample identity, preventing later context repair from backdating activation dwell.
#[test]
fn invalid_context_consumes_a_sample_and_cannot_backdate_dwell_on_repair() {
    let id = SensorId::Battery;
    let mut model = AcquisitionModel::demo();
    let mut engine = engine(id, Limit::Low { on: 11.5, off: 12.0 });
    assert_eq!(engine.snapshot().status(id), Status::Unavailable);
    samples(&mut model, id, 11.0, 0);
    assert_eq!(
        engine.update(&model.snapshot(), &[None; 8], 0).status(id),
        Status::Unavailable
    );
    assert_eq!(
        engine.update(&model.snapshot(), &context(&model, id), 400).status(id),
        Status::Unavailable
    );
    samples(&mut model, id, 11.0, 500);
    assert_eq!(
        engine.update(&model.snapshot(), &context(&model, id), 500).status(id),
        Status::Pending
    );
    samples(&mut model, id, 11.0, 1000);
    assert!(engine.update(&model.snapshot(), &context(&model, id), 1000).active(id));
}

/// A context observation gap resets voltage dwell even while the voltage's own freshness window remains open.
#[test]
fn unobserved_context_gap_breaks_dwell_even_when_the_voltage_is_fresh() {
    let id = SensorId::Battery;
    let mut model = AcquisitionModel::demo();
    let mut engine = engine(id, Limit::Low { on: 11.5, off: 12.0 });
    for at in [0, 501] {
        samples(&mut model, id, 11.0, at);
        assert!(!engine.update(&model.snapshot(), &context(&model, id), at).active(id));
    }
}

/// Each missing/invalid context field clears an active lambda alarm and cannot be repaired by replaying the same
/// sample.
#[test]
fn every_context_failure_clears_an_active_alarm_and_requires_new_data() {
    let id = SensorId::Lambda;
    let limit = Limit::LambdaDeviation {
        on: 0.15,
        off: 0.05,
        min_load: 60.0,
    };
    let corruptions: &[fn(&mut Context)] = &[
        |c| c.running = Some(false),
        |c| c.running = None,
        |c| c.warm = Some(false),
        |c| c.warm = None,
        |c| c.fuel_cut = Some(true),
        |c| c.fuel_cut = None,
        |c| c.load_percent = Some(59.99),
        |c| c.load_percent = Some(100.01),
        |c| c.load_percent = Some(f32::NAN),
        |c| c.load_percent = None,
        |c| c.target_lambda = Some(0.0),
        |c| c.target_lambda = Some(2.01),
        |c| c.target_lambda = Some(f32::INFINITY),
        |c| c.target_lambda = None,
        |c| c.sequence = 1,
        |c| c.source = Source::Unknown,
        |c| c.received_at_ms = 1001,
        |c| c.received_at_ms = 749,
    ];
    for (index, corrupt) in corruptions.iter().enumerate() {
        let mut model = AcquisitionModel::demo();
        let mut alarm = engine(id, limit);
        for at in [500, 1000] {
            samples(&mut model, id, 1.1, at);
            assert_eq!(
                alarm.update(&model.snapshot(), &context(&model, id), at).active(id),
                at == 1000
            );
        }
        let mut invalid = context(&model, id);
        corrupt(invalid[id as usize].as_mut().unwrap());
        assert_eq!(
            alarm.update(&model.snapshot(), &invalid, 1000).status(id),
            Status::Unavailable,
            "case {index}"
        );
        assert_eq!(
            alarm.update(&model.snapshot(), &context(&model, id), 1000).status(id),
            Status::Unavailable
        );
    }
}

/// Voltage alarms require running context; EGT accepts measured manifold provenance only, excluding
/// modeled/catalyst/unknown.
#[test]
fn voltage_needs_running_and_egt_needs_measured_manifold_provenance() {
    for id in [SensorId::Battery, SensorId::Egt] {
        for variant in 0..4 {
            let mut model = AcquisitionModel::demo();
            let limit = if id == SensorId::Battery {
                Limit::Low { on: 11.5, off: 12.0 }
            } else {
                Limit::High { on: 1000.0, off: 950.0 }
            };
            let mut alarm = engine(id, limit);
            for at in [0, 500] {
                samples(&mut model, id, if id == SensorId::Battery { 11.5 } else { 1000.0 }, at);
                let mut ctx = context(&model, id);
                let c = ctx[id as usize].as_mut().unwrap();
                if id == SensorId::Battery {
                    c.running = match variant {
                        0 => Some(true),
                        1 => Some(false),
                        _ => None,
                    };
                } else {
                    c.egt_location = match variant {
                        0 => EgtLocation::MeasuredManifold,
                        1 => EgtLocation::Modeled,
                        2 => EgtLocation::Catalyst,
                        _ => EgtLocation::Unknown,
                    };
                }
                assert_eq!(
                    alarm.update(&model.snapshot(), &ctx, at).active(id),
                    at == 500 && variant == 0
                );
            }
        }
    }
}

/// Threshold chatter restarts dwell; metadata faults and exact context-age limits cannot preserve qualification.
#[test]
fn chatter_clock_sequence_faults_and_context_age_have_exact_boundaries() {
    let id = SensorId::Water;
    let mut model = AcquisitionModel::demo();
    let mut alarm = engine(id, Limit::High { on: 110.0, off: 105.0 });
    for (at, value, active) in [
        (0, 110.0, false),
        (499, 109.9, false),
        (500, 110.0, false),
        (999, 110.0, false),
        (1000, 110.0, true),
        (1001, 105.0, true),
        (1299, 105.1, true),
        (1300, 105.0, true),
        (1599, 105.0, true),
        (1600, 105.0, false),
    ] {
        samples(&mut model, id, value, at);
        assert_eq!(alarm.update(&model.snapshot(), &[None; 8], at).active(id), active);
    }
    let valid = model.snapshot();
    for corrupt in [0, 1, 2, 3] {
        let mut bad = valid;
        match corrupt {
            0 => bad.metadata[id as usize].sequence -= 1,
            1 => bad.metadata[id as usize].received_at_ms = Some(1599),
            2 => bad.metadata[id as usize].source = Source::Unknown,
            _ => bad.metadata[id as usize].quality = dashboard_host_validation::samples::Quality::TransportError,
        }
        assert_eq!(alarm.update(&bad, &[None; 8], 1600).status(id), Status::Unavailable);
    }
    let id = SensorId::Battery;
    let mut model = AcquisitionModel::demo();
    let mut alarm = engine(id, Limit::Low { on: 11.5, off: 12.0 });
    samples(&mut model, id, 11.0, 500);
    let mut ctx = context(&model, id);
    ctx[id as usize].as_mut().unwrap().received_at_ms = 250;
    assert_eq!(alarm.update(&model.snapshot(), &ctx, 750).status(id), Status::Pending);
    assert_eq!(
        alarm.update(&model.snapshot(), &ctx, 751).status(id),
        Status::Unavailable
    );
}

/// Allowed sample/context skew cannot fill an unobserved interval or start dwell before both observations exist.
#[test]
fn context_skew_cannot_bridge_a_gap_or_backdate_sustained_condition() {
    let id = SensorId::Battery;
    let mut model = AcquisitionModel::demo();
    let mut alarm = engine(id, Limit::Low { on: 11.5, off: 12.0 });
    for (at, context_at) in [(500, 250), (1000, 1000)] {
        samples(&mut model, id, 11.0, at);
        let mut ctx = context(&model, id);
        ctx[id as usize].as_mut().unwrap().received_at_ms = context_at;
        assert!(!alarm.update(&model.snapshot(), &ctx, at).active(id));
    }
    let mut model = AcquisitionModel::demo();
    let mut alarm = engine(id, Limit::Low { on: 11.5, off: 12.0 });
    for (at, context_at, now) in [(0, 250, 250), (500, 500, 500)] {
        samples(&mut model, id, 11.0, at);
        let mut ctx = context(&model, id);
        ctx[id as usize].as_mut().unwrap().received_at_ms = context_at;
        assert!(!alarm.update(&model.snapshot(), &ctx, now).active(id));
    }
    let mut model = AcquisitionModel::demo();
    let mut alarm = engine(id, Limit::Low { on: 11.5, off: 12.0 });
    for (at, context_at) in [(0, 0), (400, 650)] {
        samples(&mut model, id, 11.0, at);
        let mut ctx = context(&model, id);
        ctx[id as usize].as_mut().unwrap().received_at_ms = context_at;
        assert!(!alarm.update(&model.snapshot(), &ctx, context_at).active(id));
    }
}

/// A sample seen during clock reversal is consumed, requiring new data before recovery can start dwell again.
#[test]
fn clock_fault_consumes_rejected_data_before_recovery() {
    let id = SensorId::Water;
    let mut model = AcquisitionModel::demo();
    let mut alarm = engine(id, Limit::High { on: 110.0, off: 105.0 });
    samples(&mut model, id, 120.0, 1000);
    alarm.update(&model.snapshot(), &[None; 8], 1000);
    alarm.update(&model.snapshot(), &[None; 8], 1500);
    samples(&mut model, id, 120.0, 1100);
    assert_eq!(
        alarm.update(&model.snapshot(), &[None; 8], 1499).status(id),
        Status::Unavailable
    );
    assert_eq!(
        alarm.update(&model.snapshot(), &[None; 8], 1500).status(id),
        Status::Unavailable
    );
    samples(&mut model, id, 120.0, 1600);
    assert_eq!(
        alarm.update(&model.snapshot(), &[None; 8], 1600).status(id),
        Status::Pending
    );
}

/// Acquisition evaluates intervening normal/fault samples before watch coalescing, so unseen interruptions still reset
/// dwell.
#[test]
fn acquisition_evaluates_normal_samples_and_faults_before_watch_coalescing() {
    use dashboard_host_validation::samples::Quality;
    use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
    use embassy_sync::watch::Watch;
    let id = SensorId::Water;
    let mut model = AcquisitionModel::demo();
    let watch: Watch<CriticalSectionRawMutex, _, 1> = Watch::new();
    let mut rx = watch.receiver().unwrap();
    for (at, value) in [(0, 120.0), (500, 95.0), (1000, 120.0)] {
        samples(&mut model, id, value, at);
        watch.sender().send(model.snapshot());
    }
    let latest = rx.try_changed().unwrap();
    assert_eq!(latest.alerts_at(1000).status(id), Status::Pending);
    assert!(model.mark_fault(id, Quality::TransportError, 1500));
    samples(&mut model, id, 120.0, 2000);
    assert_eq!(model.snapshot().alerts_at(2000).status(id), Status::Pending);
    samples(&mut model, id, 120.0, 3000);
    assert!(model.snapshot().alerts_at(3000).active(id));
    let published = model.snapshot();
    assert!(!published.alerts_at(6001).active(id));
    assert!(!published.alerts_at(2999).active(id));
    assert_eq!(
        AcquisitionModel::new().snapshot().alerts_at(0).status(id),
        Status::Disabled
    );
}

/// Independent priority/pixel oracles verify feedback precedence and full restoration of both buffers after warning
/// clear.
#[test]
fn user_feedback_precedes_warning_and_clear_restores_both_retained_buffers() {
    use dashboard_host_validation::dashboard::{DashboardCache, DashboardScene, Overlay};
    use dashboard_host_validation::display::St7789Renderer;
    use dashboard_host_validation::render::FpsMode;
    use dashboard_host_validation::{AlignedFrame, colors};
    let mut model = AcquisitionModel::demo();
    for at in [0, 500, 1000] {
        model.ingest_batch([1.0, 140.0, 120.0, 130.0, 70.0, 1100.0, 11.0, 1.3], at);
    }
    let active = model.snapshot();
    let alerts = active.alerts_at(1000);
    assert_eq!(alerts.priority(), Some(SensorId::Water));
    for popup in [
        None,
        Some(Overlay::Reset),
        Some(Overlay::Fps(FpsMode::Combined)),
        Some(Overlay::BoostUnit(true)),
        Some(Overlay::Brightness(Brightness::Percent(0))),
    ] {
        assert_eq!(
            Overlay::prioritize(popup, alerts, true),
            popup.or(Some(Overlay::Critical(SensorId::Water, true)))
        );
    }
    let mut scene = DashboardScene {
        readings: None,
        layout: None,
        measurements: &active,
        now_ms: 1000,
        status: dashboard_host_validation::vehicle_status::VehicleStatus::Demo,
        blink_on: true,
        boost_psi: false,
        fps_mode: FpsMode::Off,
        fps_instant: 0.0,
        fps_average: 0.0,
        backgrounds: [colors::RED; 8],
        overlay: Some(Overlay::Brightness(Brightness::Percent(0))),
    };
    let mut cache = DashboardCache::new();
    let mut frames = [AlignedFrame::boxed(), AlignedFrame::boxed()];
    let mut full = AlignedFrame::boxed();
    for (popup, blink) in [
        (None, true),
        (Some(Overlay::Fps(FpsMode::Average)), false),
        (Some(Overlay::Reset), true),
        (Some(Overlay::BoostUnit(true)), false),
        (Some(Overlay::Brightness(Brightness::Percent(0))), true),
        (None, false),
        (None, true),
    ] {
        scene.overlay = popup;
        scene.blink_on = blink;
        for (i, frame) in frames.iter_mut().enumerate() {
            cache.draw(&mut St7789Renderer::new(&mut frame.0), i, &scene);
            scene.draw_full(&mut St7789Renderer::new(&mut full.0));
            assert!(frame.0 == full.0);
            // Independent overlay oracle: shared full/retained rendering must
            // not agree on the same wrong priority decision.
            let expected = popup.unwrap_or(Overlay::Critical(SensorId::Water, blink));
            expected.draw(&mut St7789Renderer::new(&mut full.0));
            assert!(frame.0 == full.0, "feedback/warning priority on buffer {i}");
        }
    }
    for at in [1100, 1500, 1600] {
        model.ingest_batch([1.0, 95.0, 95.0, 90.0, 30.0, 700.0, 14.0, 1.0], at);
    }
    let normal = model.snapshot();
    assert_eq!(normal.alerts_at(1600).priority(), None);
    scene.measurements = &normal;
    scene.now_ms = 1600;
    for (i, frame) in frames.iter_mut().enumerate() {
        assert!(cache.draw(&mut St7789Renderer::new(&mut frame.0), i, &scene).clear);
        scene.draw_full(&mut St7789Renderer::new(&mut full.0));
        assert!(frame.0 == full.0, "critical overlay must fully restore buffer {i}");
    }
}

/// Activation and clearing follow producer samples identically across 20/40/60Hz and irregular display schedules.
#[test]
fn producer_alarm_timing_is_independent_of_twenty_forty_sixty_and_irregular_display_rates() {
    for schedule in [vec![50], vec![25], vec![16, 17, 17], vec![3, 199, 7, 51, 317]] {
        let mut model = AcquisitionModel::demo();
        let mut pending = dashboard_host_validation::acquisition::DashboardSnapshot::EMPTY;
        let mut displayed = pending;
        let mut next_display = 0;
        let mut step = 0;
        for now in 0..=2500 {
            if now % 100 == 0 {
                samples(&mut model, SensorId::Water, if now < 1300 { 120.0 } else { 95.0 }, now);
                pending = model.snapshot();
                assert_eq!(
                    pending.alerts_at(now).active(SensorId::Water),
                    (1000..1800).contains(&now)
                );
            }
            if now >= next_display {
                displayed.accept(pending);
                assert_eq!(
                    displayed.alerts_at(now).active(SensorId::Water),
                    (1000..1800).contains(&(now / 100 * 100))
                );
                next_display = now + schedule[step % schedule.len()];
                step += 1;
            }
        }
    }
}
