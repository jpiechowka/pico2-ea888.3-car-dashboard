use dashboard_host_validation::acquisition::AcquisitionModel;
use dashboard_host_validation::alarms::{RuleIndex, Status};
use dashboard_host_validation::samples::{Quality, SensorId};
use dashboard_host_validation::view_config::*;
use dashboard_host_validation::warning_state::{RequestError, WarningCommand, WarningRequests};
fn index(i: usize) -> RuleIndex { RuleIndex::new(i).unwrap() }
fn config(
    latch: bool,
    snooze: u16,
) -> ValidatedConfig {
    let mut c = default_config();
    c.widgets.truncate(1);
    let w = &mut c.widgets[0].warning;
    w.enabled = true;
    w.trigger = 2.;
    w.clear = 1.;
    w.on_ms = 100;
    w.off_ms = 100;
    w.advanced.latch = latch;
    w.advanced.snooze_s = snooze;
    c.controls.screens[0].buttons[0].tap = ControlAction::AcknowledgeWarnings;
    validate(c).unwrap()
}
fn sample(
    m: &mut AcquisitionModel<'_>,
    x: f32,
    at: u64,
) {
    assert!(m.ingest_batch([x, 90., 95., 80., 30., 700., 13.8, 1.], at));
}
fn command(
    m: &AcquisitionModel<'_>,
    request_id: u64,
) -> WarningCommand {
    let s = m.snapshot();
    WarningCommand {
        request_id,
        run_id: s.warning_run_id().unwrap(),
        observed_event_serial: s.warning_event_serial(),
        rules: 1,
    }
}
#[test]
fn latch_clear_and_ack_work_in_both_orders_and_stale_never_clears() {
    for ack_first in [false, true] {
        let c = config(true, 0);
        let mut m = AcquisitionModel::demo_configured(&c);
        sample(&mut m, 3., 0);
        sample(&mut m, 3., 100);
        assert!(m.snapshot().warning_eligibility(index(0), 100).latched);
        if ack_first {
            let cmd = command(&m, 1);
            assert_eq!(m.apply_warning_command(cmd, 100).accepted, 1);
        }
        m.mark_fault(SensorId::Boost, Quality::TransportError, 110);
        let e = m.snapshot().warning_eligibility(index(0), 110);
        assert_eq!(e.status, Status::Unavailable);
        assert!(e.latched);
        assert!(e.visible());
        sample(&mut m, 0., 200);
        sample(&mut m, 0., 300);
        assert_eq!(m.snapshot().rule_status_at(index(0), 300), Status::Normal);
        assert_eq!(m.snapshot().warning_eligibility(index(0), 300).latched, !ack_first);
        if !ack_first {
            let cmd = command(&m, 1);
            assert_eq!(m.apply_warning_command(cmd, 300).accepted, 1);
        }
        assert!(!m.snapshot().warning_eligibility(index(0), 300).latched);
    }
}
#[test]
fn commands_reject_newer_episodes_old_runs_and_replays_are_idempotent() {
    let c = config(true, 5);
    let mut m = AcquisitionModel::demo_configured(&c);
    sample(&mut m, 3., 0);
    sample(&mut m, 3., 100);
    let old = command(&m, 1);
    let accepted = m.apply_warning_command(old, 100);
    assert_eq!(accepted.accepted, 1);
    let first = m.snapshot();
    assert!(first.warning_eligibility(index(0), 100).snoozed);
    assert_eq!(m.apply_warning_command(old, 200), accepted);
    m.tick(5100);
    assert!(!m.snapshot().warning_eligibility(index(0), 5100).snoozed);
    assert_ne!(first.publication_identity(), m.snapshot().publication_identity());
    sample(&mut m, 0., 5200);
    sample(&mut m, 0., 5300);
    sample(&mut m, 3., 5400);
    sample(&mut m, 3., 5500);
    assert_eq!(
        m.apply_warning_command(WarningCommand { request_id: 2, ..old }, 5500)
            .rejected,
        1
    );
    assert!(!m.snapshot().warning_eligibility(index(0), 5500).acknowledged);
    let current = command(&m, 3);
    assert_eq!(
        m.apply_warning_command(
            WarningCommand {
                run_id: current.run_id + 1,
                ..current
            },
            5500
        )
        .rejected,
        1
    );
    assert_eq!(m.apply_warning_command(current, 5499).rejected, 1);
    assert_eq!(m.apply_warning_command(current, 5500).accepted, 1);
    assert_eq!(m.apply_warning_command(old, 5500).rejected, 1);
    m.reset_statistics(1);
    assert!(m.snapshot().warning_eligibility(index(0), 5500).acknowledged);
    let replacement = AcquisitionModel::demo_configured(&c);
    assert_ne!(replacement.snapshot().warning_run_id(), m.snapshot().warning_run_id());
}
#[test]
fn requester_has_one_pending_command_and_requires_matching_result() {
    let c = config(true, 0);
    let mut m = AcquisitionModel::demo_configured(&c);
    sample(&mut m, 3., 0);
    sample(&mut m, 3., 100);
    let s = m.snapshot();
    let mut requests = WarningRequests::new();
    let cmd = requests
        .request(s.warning_run_id().unwrap(), s.warning_event_serial(), 1)
        .unwrap();
    assert_eq!(
        requests.request(cmd.run_id, cmd.observed_event_serial, 1),
        Err(RequestError::Busy)
    );
    assert_eq!(requests.pending(), Some(cmd));
    let result = m.apply_warning_command(cmd, 100);
    assert!(requests.complete(result));
    assert_eq!(requests.pending(), None);
    assert!(!requests.complete(result));
}

#[test]
fn statistics_reset_preserves_unacknowledged_latch_and_new_event_cancels_every_snooze() {
    let mut c = config(true, 5).config().clone();
    let mut other = c.widgets[0].clone();
    other.id = 2;
    other.placement.column = 1;
    other.warning.trigger = 4.;
    other.warning.clear = 3.;
    other.warning.advanced.priority = Priority::Critical;
    c.widgets.push(other).unwrap();
    let c = validate(c).unwrap();
    let mut m = AcquisitionModel::demo_configured(&c);
    sample(&mut m, 2., 0);
    sample(&mut m, 2., 100);
    m.reset_statistics(1);
    assert!(!m.snapshot().warning_eligibility(index(0), 100).acknowledged);
    let cmd = command(&m, 1);
    assert_eq!(m.apply_warning_command(cmd, 100).accepted, 1);
    m.tick(100);
    assert!(m.snapshot().warning_eligibility(index(0), 100).snoozed);
    sample(&mut m, 4., 200);
    sample(&mut m, 4., 300);
    let s = m.snapshot();
    assert!(!s.warning_eligibility(index(0), 300).snoozed);
    assert!(s.warning_eligibility(index(0), 300).acknowledged);
    assert!(!s.warning_eligibility(index(1), 300).acknowledged);
    // This completed-page watermark predates rule1: producer admits the new event first.
    let result = m.apply_warning_command(
        WarningCommand {
            request_id: 2,
            rules: 3,
            ..cmd
        },
        300,
    );
    assert_eq!(result.accepted, 1);
    assert_eq!(result.rejected, 2);
    assert!(m.snapshot().warning_eligibility(index(1), 300).attention());
}
#[test]
fn all64_rules_and_rule63_commands_do_not_alias_channels() {
    let seed = config(true, 0).config().clone();
    let mut c = seed.clone();
    c.widgets.clear();
    c.views.clear();
    for view in 1..=8 {
        c.views
            .push(View {
                id: view,
                name: "View".try_into().unwrap(),
            })
            .unwrap();
        for cell in 0..8 {
            let mut w = seed.widgets[0].clone();
            w.id = (view as u16 - 1) * 8 + cell as u16 + 1;
            w.view = view;
            w.placement.column = cell % 4;
            w.placement.row = cell / 4;
            c.widgets.push(w).unwrap();
        }
    }
    let c = validate(c).unwrap();
    let mut m = AcquisitionModel::demo_configured(&c);
    sample(&mut m, 3., 0);
    sample(&mut m, 3., 100);
    assert_eq!(m.snapshot().warning_event_serial(), 64);
    let result = m.apply_warning_command(
        WarningCommand {
            rules: u64::MAX,
            ..command(&m, 1)
        },
        100,
    );
    assert_eq!(result.accepted, u64::MAX);
    assert_eq!(result.rejected, 0);
    assert!(m.snapshot().warning_eligibility(index(63), 100).acknowledged);
}
#[test]
fn nonlatched_clear_and_stale_stop_visibility_without_tick_confirming_clear() {
    let c = config(false, 0);
    let mut m = AcquisitionModel::demo_configured(&c);
    sample(&mut m, 3., 0);
    sample(&mut m, 3., 100);
    sample(&mut m, 0., 200);
    for at in 201..300 {
        m.tick(at);
    }
    assert!(m.snapshot().warning_eligibility(index(0), 299).visible());
    sample(&mut m, 0., 300);
    assert!(!m.snapshot().warning_eligibility(index(0), 300).visible());
    sample(&mut m, 3., 400);
    sample(&mut m, 3., 500);
    m.tick(1001);
    assert!(!m.snapshot().warning_eligibility(index(0), 1001).visible());
}
#[test]
fn snooze_saturates_at_maximum_clock_and_expiry_resumes_attention() {
    let c = config(true, 5);
    let mut m = AcquisitionModel::demo_configured(&c);
    sample(&mut m, 3., u64::MAX - 300);
    sample(&mut m, 3., u64::MAX - 200);
    let cmd = command(&m, 1);
    assert_eq!(m.apply_warning_command(cmd, u64::MAX - 200).accepted, 1);
    m.tick(u64::MAX - 1);
    assert!(m.snapshot().warning_eligibility(index(0), u64::MAX - 1).snoozed);
    m.tick(u64::MAX);
    assert!(m.snapshot().warning_eligibility(index(0), u64::MAX).attention());
}
#[test]
fn latch_cleared_label_and_stale_label_preserve_raw_status() {
    let c = config(true, 0);
    let mut m = AcquisitionModel::demo_configured(&c);
    sample(&mut m, 3., 0);
    sample(&mut m, 3., 100);
    sample(&mut m, 0., 200);
    sample(&mut m, 0., 300);
    assert_eq!(m.snapshot().warning_eligibility(index(0), 300).label(), "CLEARED / ACK");
    m.tick(801);
    let e = m.snapshot().warning_eligibility(index(0), 801);
    assert_eq!(e.label(), "LATCHED / DATA STALE");
    assert_eq!(e.status, Status::Unavailable);
}

#[test]
fn remembered_latch_does_not_skip_raw_on_dwell_after_a_fault() {
    let c = config(true, 0);
    let mut m = AcquisitionModel::demo_configured(&c);
    sample(&mut m, 3., 0);
    sample(&mut m, 3., 100);
    m.mark_fault(SensorId::Boost, Quality::TransportError, 110);
    sample(&mut m, 3., 200);
    let e = m.snapshot().warning_eligibility(index(0), 200);
    assert_eq!(e.status, Status::Pending);
    assert!(e.latched);
    sample(&mut m, 3., 300);
    assert_eq!(m.snapshot().warning_event_serial(), 2);
}

#[test]
fn outside_window_side_change_stays_in_same_episode() {
    let mut c = config(true, 0).config().clone();
    let w = &mut c.widgets[0].warning;
    w.mode = LimitMode::OutsideWindow;
    w.trigger = 0.;
    w.clear = 1.;
    w.advanced.trigger_high = 3.;
    w.advanced.clear_high = 2.;
    let c = validate(c).unwrap();
    let mut m = AcquisitionModel::demo_configured(&c);
    sample(&mut m, 0., 0);
    sample(&mut m, 0., 100);
    let serial = m.snapshot().warning_event_serial();
    sample(&mut m, 3., 200);
    sample(&mut m, 3., 300);
    assert_eq!(m.snapshot().warning_event_serial(), serial);
    assert_eq!(m.snapshot().rule_status_at(index(0), 300), Status::Active);
}
#[test]
fn contextual_windows_keep_running_and_measured_manifold_qualification() {
    use dashboard_host_validation::alarms::{Context, EgtLocation};
    use dashboard_host_validation::samples::{Quantity, Sample, Source};
    for channel in [Channel::Battery, Channel::Egt] {
        let mut c = config(true, 0).config().clone();
        let mut w = default_widget(channel, 1, 1, c.widgets[0].placement);
        w.warning.enabled = true;
        w.warning.mode = LimitMode::OutsideWindow;
        w.warning.on_ms = 100;
        w.warning.off_ms = 100;
        (
            w.warning.trigger,
            w.warning.clear,
            w.warning.advanced.trigger_high,
            w.warning.advanced.clear_high,
        ) = if channel == Channel::Battery {
            (10., 11., 16., 15.)
        } else {
            (100., 200., 1000., 900.)
        };
        c.widgets[0] = w;
        let c = validate(c).unwrap();
        let mut m = AcquisitionModel::demo_configured(&c);
        for (at, qualified, expected) in [
            (0, false, Status::Unavailable),
            (100, false, Status::Unavailable),
            (200, true, Status::Pending),
            (300, true, Status::Active),
        ] {
            let sample = Sample {
                value: if channel == Channel::Battery {
                    Quantity::ModuleVolts(10.)
                } else {
                    Quantity::ExhaustC(100.)
                },
                source: Source::Demo,
                sequence: at + 1,
                received_at_ms: at,
            };
            let context = Context {
                source: Source::Demo,
                sequence: at + 1,
                received_at_ms: at,
                running: Some(qualified),
                warm: None,
                load_percent: None,
                fuel_cut: None,
                target_lambda: None,
                egt_location: if qualified {
                    EgtLocation::MeasuredManifold
                } else {
                    EgtLocation::Modeled
                },
            };
            m.ingest_contextual(sample, Some(context), at).unwrap();
            assert_eq!(m.snapshot().rule_status_at(index(0), at), expected);
        }
    }
}

#[test]
fn exact_transport_retry_does_not_mutate_publication_or_snooze_deadline() {
    let c = config(true, 5);
    let mut m = AcquisitionModel::demo_configured(&c);
    sample(&mut m, 3., 0);
    sample(&mut m, 3., 100);
    let cmd = command(&m, 1);
    let result = m.apply_warning_command(cmd, 100);
    let before = m.snapshot();
    assert_eq!(m.apply_warning_command(cmd, 100), result);
    assert_eq!(before.publication_identity(), m.snapshot().publication_identity());
    m.apply_warning_command(cmd, 200);
    m.tick(5100);
    assert!(!m.snapshot().warning_eligibility(index(0), 5100).snoozed);
}
