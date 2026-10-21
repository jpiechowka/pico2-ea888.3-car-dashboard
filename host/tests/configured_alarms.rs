//! Configured per-widget rules retain independent channel qualification, dwell and hysteresis.
use dashboard_host_validation::acquisition::AcquisitionModel;
use dashboard_host_validation::alarms::{RuleAlerts, RuleEngine, RuleIndex, Status};
use dashboard_host_validation::samples::{Quality, SensorId};
use dashboard_host_validation::view_config::*;

/// Returns bounded rule index `n`; panics outside 0..64.
fn index(n: usize) -> RuleIndex { RuleIndex::new(n).unwrap() }

/// Returns eight full views with 64 boost rules, including disabled duplicates and a distinct final threshold.
/// Activation/clear dwell is 500/250 ms; panics if the maximal fixture fails shared validation.
fn maximum() -> ValidatedConfig {
    let mut config = default_config();
    config.views.clear();
    config.widgets.clear();
    for view in 1..=8 {
        config
            .views
            .push(View {
                id: view,
                name: "View".try_into().unwrap(),
                orientation: None,
            })
            .unwrap();
        for cell in 0..8 {
            let mut widget = default_widget(
                Channel::Boost,
                (view as u16 - 1) * 8 + cell as u16 + 1,
                view,
                Placement {
                    column: cell % 4,
                    row: cell / 4,
                    span: Span::OneByOne,
                },
            );
            widget.warning.enabled = cell != 1;
            widget.warning.trigger = if view == 8 && cell == 7 { 1.0 } else { 3.0 };
            widget.warning.clear = widget.warning.trigger - 0.2;
            widget.warning.on_ms = 500;
            widget.warning.off_ms = 250;
            config.widgets.push(widget).unwrap();
        }
    }
    validate(config).unwrap()
}

/// Ingests boost `boost` in bar with fixed companion values into `model` at `at` ms; panics on rejection.
fn sample(
    model: &mut AcquisitionModel<'_>,
    boost: f32,
    at: u64,
) {
    assert!(model.ingest_batch([boost, 90., 95., 80., 30., 700., 13.8, 1.], at));
}

/// Rule 63 remains independent of channel zero and disabled duplicates within fixed rule-storage bounds.
#[test]
fn rule63_is_independent_from_channel0_and_disabled_duplicate() {
    let config = maximum();
    let mut model = AcquisitionModel::demo_configured(&config);
    for at in [0, 250, 500] {
        sample(&mut model, 2., at);
    }
    let snapshot = model.snapshot();
    assert_eq!(snapshot.rule_status_at(index(63), 500), Status::Active);
    assert_eq!(snapshot.rule_status_at(index(0), 500), Status::Normal);
    assert_eq!(snapshot.rule_status_at(index(1), 500), Status::Disabled);
    assert_eq!(snapshot.rule_status_at(index(63), 1000), Status::Active);
    assert_eq!(snapshot.rule_status_at(index(63), 1001), Status::Unavailable);
    assert_eq!(snapshot.rule_status_at(index(63), 499), Status::Unavailable);
    assert!(RuleIndex::new(64).is_none());
    assert!(RuleIndex::new(usize::MAX).is_none());
    assert!(core::mem::size_of::<RuleEngine<'_>>() <= 4376);
    assert!(core::mem::size_of::<RuleAlerts>() <= 680);
}

/// Faults and stale gaps restart dwell; statistics reset preserves an already qualified active warning.
#[test]
fn faults_gaps_and_statistics_resets_do_not_reuse_dwell() {
    let config = maximum();
    let mut model = AcquisitionModel::demo_configured(&config);
    sample(&mut model, 2., 0);
    sample(&mut model, 2., 250);
    assert_eq!(model.snapshot().rule_status_at(index(63), 500), Status::Pending);
    model.mark_fault(SensorId::Boost, Quality::TransportError, 300);
    assert_eq!(model.snapshot().rule_status_at(index(63), 300), Status::Unavailable);
    for (at, status) in [(500, Status::Pending), (750, Status::Pending), (1000, Status::Active)] {
        sample(&mut model, 2., at);
        assert_eq!(model.snapshot().rule_status_at(index(63), at), status);
    }
    assert!(model.reset_statistics(1));
    assert_eq!(model.snapshot().rule_status_at(index(63), 1000), Status::Active);
    sample(&mut model, 2., 1501);
    assert_eq!(model.snapshot().rule_status_at(index(63), 1501), Status::Pending);
    for at in [1751, 2001] {
        sample(&mut model, 2., at);
    }
    assert_eq!(model.snapshot().rule_status_at(index(63), 2001), Status::Active);
    sample(&mut model, 0.8, 2251);
    assert_eq!(model.snapshot().rule_status_at(index(63), 2251), Status::Active);
    sample(&mut model, 0.8, 2501);
    assert_eq!(model.snapshot().rule_status_at(index(63), 2501), Status::Normal);
}

/// Vehicle applicability cannot qualify absent sources or missing context needed by a voltage warning.
#[test]
fn vehicle_applicability_does_not_enable_unknown_sources_or_contexts() {
    let mut config = default_config();
    config.widgets[0].warning.enabled = true;
    config.widgets[0].warning.applicability = Applicability::VehicleAndDemo;
    let config = validate(config).unwrap();
    let model = AcquisitionModel::configured(&config, 0x7E8);
    assert_eq!(model.snapshot().rule_status_at(index(0), 0), Status::Unavailable);
    assert_eq!(model.snapshot().rule_status_at(index(1), 0), Status::Disabled);
    let mut config = config.config().clone();
    let voltage = config
        .widgets
        .iter()
        .position(|w| w.channel == Channel::Battery)
        .unwrap();
    config.widgets[voltage].warning.applicability = Applicability::VehicleAndDemo;
    config.widgets[voltage].warning.on_ms = 1;
    let config = validate(config).unwrap();
    let mut model = AcquisitionModel::configured(&config, 0x7E8);
    use dashboard_host_validation::samples::{Quantity, Sample, Source};
    for at in [1, 250] {
        model
            .ingest_sample(
                Sample {
                    value: Quantity::ModuleVolts(10.),
                    source: Source::Obd { ecu: 0x7E8, pid: 0x42 },
                    sequence: at,
                    received_at_ms: at,
                },
                at,
            )
            .unwrap();
    }
    assert_eq!(
        model.snapshot().rule_status_at(index(voltage), 250),
        Status::Unavailable
    );
}

/// All 64 rules use their configured channel, so a lambda fault invalidates only lambda rules.
#[test]
fn every_rule_uses_its_channel_for_context_metadata_expiry_and_faults() {
    let mut cfg = maximum().config().clone();
    for (i, widget) in cfg.widgets.iter_mut().enumerate() {
        let channel = Channel::ALL[(i + 3) % 8];
        let mut replacement = default_widget(channel, widget.id, widget.view, widget.placement);
        replacement.warning.enabled = true;
        *widget = replacement;
    }
    let cfg = validate(cfg).unwrap();
    let mut model = AcquisitionModel::demo_configured(&cfg);
    for at in [0, 250, 500, 750, 1000] {
        model.ingest_batch([10., 150., 120., 130., 80., 1100., 10., 0.7], at);
    }
    for i in 0..64 {
        assert_eq!(
            model.snapshot().rule_status_at(index(i), 1000),
            Status::Active,
            "rule {i}"
        );
    }
    model.mark_fault(SensorId::Lambda, Quality::InvalidPayload, 1100);
    for (i, widget) in cfg.config().widgets.iter().enumerate() {
        assert_eq!(
            model.snapshot().rule_status_at(index(i), 1100),
            if widget.channel == Channel::Lambda {
                Status::Unavailable
            } else {
                Status::Active
            },
            "rule {i}"
        );
    }
}

/// Time-only expiry changes neither sample sequence nor sensors and prevents later backdated dwell reuse.
#[test]
fn time_only_ticks_invalidate_expired_dwell_without_sampling_or_rebinding() {
    let config = maximum();
    let mut model = AcquisitionModel::demo_configured(&config);
    sample(&mut model, 2., 0);
    sample(&mut model, 2., 250);
    let pending = model.snapshot();
    model.tick(500);
    assert_eq!(model.snapshot().rule_status_at(index(63), 500), Status::Pending);
    assert_eq!(model.snapshot().sequence, pending.sequence);
    assert_eq!(model.snapshot().sensors, pending.sensors);
    model.tick(1_000_000);
    assert_eq!(
        model.snapshot().rule_status_at(index(63), 1_000_000),
        Status::Unavailable
    );
    // The rule clock remembers the time-only gap even if a later caller offers
    // a reading captured after the last sample but before the last tick.
    sample(&mut model, 2., 500);
    assert_eq!(model.snapshot().rule_status_at(index(63), 500), Status::Unavailable);
    for (at, status) in [
        (1_000_001, Status::Pending),
        (1_000_251, Status::Pending),
        (1_000_501, Status::Active),
    ] {
        sample(&mut model, 2., at);
        assert_eq!(model.snapshot().rule_status_at(index(63), at), status);
    }
}

/// Inclusive window thresholds require measurement-driven activation/clear dwell; ticks cannot qualify them.
#[test]
fn v4_windows_use_inclusive_endpoints_and_measurement_dwell() {
    for mode in [LimitMode::InsideWindow, LimitMode::OutsideWindow] {
        let mut c = default_config();
        c.widgets.truncate(1);
        let w = &mut c.widgets[0].warning;
        w.enabled = true;
        w.mode = mode;
        w.on_ms = 100;
        w.off_ms = 100;
        (w.trigger, w.clear, w.advanced.trigger_high, w.advanced.clear_high) = if mode == LimitMode::OutsideWindow {
            (0.0, 1.0, 3.0, 2.0)
        } else {
            (1.0, 0.0, 2.0, 3.0)
        };
        let trigger = w.trigger;
        let clear = w.clear;
        let c = validate(c).unwrap();
        let mut model = AcquisitionModel::demo_configured(&c);
        sample(&mut model, trigger, 0);
        assert_eq!(model.snapshot().rule_status_at(index(0), 0), Status::Pending);
        for at in 1..100 {
            model.tick(at);
        }
        assert_eq!(model.snapshot().rule_status_at(index(0), 99), Status::Pending);
        sample(&mut model, trigger, 100);
        assert_eq!(model.snapshot().rule_status_at(index(0), 100), Status::Active);
        sample(&mut model, clear, 200);
        assert_eq!(model.snapshot().rule_status_at(index(0), 200), Status::Active);
        sample(&mut model, clear, 300);
        assert_eq!(model.snapshot().rule_status_at(index(0), 300), Status::Normal);
    }
}

/// Both ends of inside/outside windows are inclusive, with adjacent hysteresis preserving prior state.
#[test]
fn v4_window_high_endpoints_and_adjacent_hysteresis_are_inclusive() {
    for mode in [LimitMode::InsideWindow, LimitMode::OutsideWindow] {
        for high in [false, true] {
            let mut c = default_config();
            c.widgets.truncate(1);
            let w = &mut c.widgets[0].warning;
            w.enabled = true;
            w.mode = mode;
            w.on_ms = 100;
            w.off_ms = 100;
            (w.trigger, w.clear, w.advanced.trigger_high, w.advanced.clear_high) = if mode == LimitMode::OutsideWindow {
                (0., 1., 3., 2.)
            } else {
                (1., 0., 2., 3.)
            };
            let (trigger, clear, adjacent) = match (mode, high) {
                (LimitMode::OutsideWindow, false) => (0., 1., 0.5),
                (LimitMode::OutsideWindow, true) => (3., 2., 2.5),
                (LimitMode::InsideWindow, false) => (1., 0., 0.5),
                _ => (2., 3., 2.5),
            };
            let c = validate(c).unwrap();
            let mut m = AcquisitionModel::demo_configured(&c);
            sample(&mut m, adjacent, 0);
            sample(&mut m, adjacent, 100);
            assert_eq!(m.snapshot().rule_status_at(index(0), 100), Status::Normal);
            sample(&mut m, trigger, 200);
            m.tick(300);
            assert_eq!(m.snapshot().rule_status_at(index(0), 300), Status::Pending);
            sample(&mut m, trigger, 300);
            assert_eq!(m.snapshot().rule_status_at(index(0), 300), Status::Active);
            sample(&mut m, adjacent, 400);
            sample(&mut m, adjacent, 500);
            assert_eq!(m.snapshot().rule_status_at(index(0), 500), Status::Active);
            sample(&mut m, clear, 600);
            m.tick(700);
            assert_eq!(m.snapshot().rule_status_at(index(0), 700), Status::Active);
            sample(&mut m, clear, 700);
            assert_eq!(m.snapshot().rule_status_at(index(0), 700), Status::Normal);
        }
    }
}
