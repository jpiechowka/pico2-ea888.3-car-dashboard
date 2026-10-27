//! Configuration-bound sample admission, freshness, polling periods and warning run identity.
use dashboard_config::*;
use dashboard_host_validation::acquisition::AcquisitionModel;
use dashboard_host_validation::alarms::RuleIndex;
use dashboard_host_validation::diagnostics::CanFrame;
use dashboard_host_validation::diagnostics::client::TxAction;
use dashboard_host_validation::diagnostics::session::Session;
use dashboard_host_validation::samples::{Quality, Quantity, Sample, SampleError, SensorId, Source};

/// Completes `tx` on `session` at `now` ms, then injects single-frame `data` from 0x7E8 one ms later.
/// Uses transport epoch 1; panics for invalid tokens, rejected frames, payloads over seven bytes or time overflow.
fn respond(
    session: &mut Session<'_>,
    tx: TxAction,
    data: &[u8],
    now: u64,
) {
    session.tx_completed(tx.token, now).unwrap();
    let mut frame = [0; 8];
    frame[0] = data.len() as u8;
    frame[1..1 + data.len()].copy_from_slice(data);
    session
        .receive(&CanFrame::new(0x7E8, &frame).unwrap(), 1, now + 1)
        .unwrap();
}

/// Statistics resets retain the exact immutable config and its shortened freshness/alarm deadline.
#[test]
fn immutable_configuration_survives_statistics_resets_and_limits_snapshot_freshness() {
    let mut config = default_config();
    config.sources[Channel::Water.index()].freshness_ms = 100;
    let config = validate(config).unwrap();
    let mut model = AcquisitionModel::demo_configured(&config);
    for at in (0..=1500).step_by(50) {
        model.ingest_batch([1.0, 90.0, 120.0, 80.0, 30.0, 750.0, 13.8, 1.0], at);
    }
    let snapshot = model.snapshot();
    let water_rule = RuleIndex::new(
        config
            .config()
            .widgets
            .iter()
            .position(|w| w.channel == Channel::Water.into())
            .unwrap(),
    )
    .unwrap();
    assert_eq!(snapshot.value_at(SensorId::Water, 1600), Some(120.0));
    assert_eq!(snapshot.quality(SensorId::Water, 1601), Quality::Stale);
    assert!(snapshot.rule_active_at(water_rule, 1600));
    assert!(!snapshot.rule_active_at(water_rule, 1601));
    model.reset_statistics(7);
    assert!(core::ptr::eq(model.configuration().unwrap(), &config));
    assert_eq!(model.snapshot().quality(SensorId::Water, 1601), Quality::Stale);
}

/// Gaps beyond configured freshness restart alarm dwell instead of accumulating separated observations.
#[test]
fn tightened_freshness_breaks_alarm_dwell_across_measurement_gaps() {
    let mut config = default_config();
    config.sources[Channel::Water.index()].freshness_ms = 100;
    let config = validate(config).unwrap();
    let mut model = AcquisitionModel::demo_configured(&config);
    for at in [0, 1000, 2000] {
        model.ingest_batch([1.0, 90.0, 120.0, 80.0, 30.0, 750.0, 13.8, 1.0], at);
    }
    let water_rule = RuleIndex::new(
        config
            .config()
            .widgets
            .iter()
            .position(|w| w.channel == Channel::Water.into())
            .unwrap(),
    )
    .unwrap();
    assert_eq!(
        model.snapshot().rule_status_at(water_rule, 2000),
        dashboard_host_validation::alarms::Status::Pending
    );
}

/// Candidate, disabled and differently routed channels reject legacy live samples without publishing values.
#[test]
fn configured_candidates_disabled_and_other_routes_reject_old_live_sources() {
    for source in [
        definition(DefinitionId::SimosCoolant11cd).source(Channel::Water),
        ChannelSource {
            poll: dashboard_config::Poll::Off,
            ..default_config().sources[Channel::Water.index()]
        },
        ChannelSource {
            route: Route::Physical11 {
                request: 0x700,
                response: 0x708,
            },
            ..default_config().sources[Channel::Water.index()]
        },
    ] {
        let mut config = default_config();
        config.sources[Channel::Water.index()] = source;
        let config = validate(config).unwrap();
        let mut model = AcquisitionModel::configured(&config, 0x7E8);
        assert_eq!(
            model.ingest_sample(
                Sample {
                    value: Quantity::CoolantC(90.0),
                    received_at_ms: 1,
                    sequence: 1,
                    source: Source::Obd { ecu: 0x7E8, pid: 5 }
                },
                1
            ),
            Err(SampleError::Source)
        );
        assert_eq!(model.snapshot().value_at(SensorId::Water, 1), None);
    }
}

/// Selected poll periods bound both successful and failed retries while configuration survives epoch/reset changes.
#[test]
fn selected_period_bounds_success_and_retry_admission_across_new_epochs() {
    let mut config = default_config();
    for source in &mut config.sources {
        source.poll = dashboard_config::Poll::Off;
    }
    config.sources[Channel::Water.index()].poll = dashboard_config::Poll::EveryMs(5000);
    let config = validate(config).unwrap();
    let mut session = Session::configured(0x7E8, 1, &config).unwrap();
    let discovery = session.poll(0).unwrap().unwrap();
    respond(&mut session, discovery, &[0x41, 0, 0x08, 0, 0, 0], 0); // only PID05
    let sample = session.poll(21).unwrap().unwrap();
    respond(&mut session, sample, &[0x41, 5, 130], 21);
    assert!(session.poll(5021).unwrap().is_none());
    let retry = session.poll(5022).unwrap().unwrap();
    respond(&mut session, retry, &[0x7F, 0x01, 0x22], 5022);
    assert!(session.poll(10022).unwrap().is_none());
    assert!(session.poll(10023).unwrap().is_some());
    session.transport_fault(10024).unwrap();
    session.change_epoch(2, 10025).unwrap();
    assert!(core::ptr::eq(session.configuration().unwrap(), &config));
    session.reset_statistics(9);
    assert!(core::ptr::eq(session.configuration().unwrap(), &config));
}

/// Disabled sources plus an unqualified lambda candidate produce no diagnostic discovery or sample queries.
#[test]
fn all_disabled_or_candidate_configuration_emits_no_diagnostic_queries() {
    let mut config = default_config();
    for source in &mut config.sources {
        source.poll = dashboard_config::Poll::Off;
    }
    config.sources[Channel::Lambda.index()].poll = dashboard_config::Poll::EveryMs(500);
    let config = validate(config).unwrap();
    let mut session = Session::configured(0x7E8, 1, &config).unwrap();
    for now in [0, 1000, 10_000, 100_000] {
        assert!(session.poll(now).unwrap().is_none());
    }
}

/// Rejected warning acknowledgment is published; communication epoch changes preserve warning run identity.
#[test]
fn session_warning_command_result_and_run_survive_communication_epoch() {
    use dashboard_host_validation::warning_state::WarningCommand;
    let c = validate(default_config()).unwrap();
    let mut session = Session::configured(0x7E8, 1, &c).unwrap();
    let run = session.snapshot().warning_run_id().unwrap();
    let result = session.apply_warning_command(
        WarningCommand {
            request_id: 1,
            run_id: run,
            observed_event_serial: 0,
            rules: u64::MAX,
        },
        0,
    );
    assert_eq!(result.accepted, 0);
    assert_eq!(result.rejected, u64::MAX);
    assert_eq!(session.snapshot().warning_command_result(), result);
    session.transport_fault(1).unwrap();
    session.change_epoch(2, 2).unwrap();
    assert_eq!(session.snapshot().warning_run_id(), Some(run));
}
