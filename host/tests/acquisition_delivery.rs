use dashboard_host_validation::acquisition::{AcquisitionModel, DashboardSnapshot, ResetRequests, SensorId};
use dashboard_host_validation::sensor_state::{PeakMode, SensorState};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::signal::Signal;
use embassy_sync::watch::Watch;

#[test]
fn between_frame_peak_survives_coalesced_publication() {
    let watch = Watch::<CriticalSectionRawMutex, DashboardSnapshot, 1>::new();
    let sender = watch.sender();
    let mut receiver = watch.receiver().unwrap();
    let mut model = AcquisitionModel::demo();
    for (at, value) in [(0, 1.0), (20, 2.0), (30, 1.0)] {
        model.ingest_batch([value; 8], at);
        sender.send(model.snapshot());
    }
    let snapshot = receiver.try_changed().unwrap();
    assert_eq!(snapshot.sensor(SensorId::Oil).maximum(), Some(2.0));
    assert_eq!(snapshot.sensor(SensorId::Oil).value(), Some(1.0));
    assert!(receiver.try_changed().is_none());
}

#[test]
fn display_cadence_never_changes_statistics() {
    for periods in [&[50][..], &[25][..], &[16, 17, 17][..], &[1, 250, 37, 3, 1000][..]] {
        let watch = Watch::<CriticalSectionRawMutex, DashboardSnapshot, 1>::new();
        let mut receiver = watch.receiver().unwrap();
        let mut model = AcquisitionModel::demo();
        let mut reference = SensorState::new();
        let mut display = DashboardSnapshot::EMPTY;
        let mut next_read = 0;
        let mut reads = 0;
        for at in 0..=130_000u64 {
            let value = if at % 1130 == 20 {
                125.0
            } else {
                (at % 2510) as f32 / 100.0 - 10.0
            };
            if at.is_multiple_of(10) {
                reference.ingest(value, at, PeakMode::Maximum);
                model.ingest_batch([value; 8], at);
                watch.sender().send(model.snapshot());
            }
            if at >= next_read {
                if let Some(snapshot) = receiver.try_changed() {
                    assert!(display.accept(snapshot));
                }
                assert_eq!(*display.sensor(SensorId::Oil), reference.snapshot());
                next_read += periods[reads % periods.len()];
                reads += 1;
            }
        }
        match periods {
            [50] => assert_eq!(reads, 2601),
            [25] => assert_eq!(reads, 5201),
            [16, 17, 17] => assert_eq!(reads, 7801),
            _ => {}
        }
        if let Some(snapshot) = receiver.try_changed() {
            display.accept(snapshot);
        }
        assert_eq!(*display.sensor(SensorId::Oil), reference.snapshot());
        assert_eq!(display.sensor(SensorId::Oil).maximum(), Some(125.0));
        assert_eq!(display.sensor(SensorId::Oil).history().count(), 60);
    }
}

#[test]
fn reset_success_waits_for_the_latest_requested_generation() {
    let mut requests = ResetRequests::default();
    assert!(!requests.acknowledge(0));
    assert!(!requests.acknowledge(1));
    assert_eq!(requests.request(), 1);
    assert!(!requests.acknowledge(0));
    assert!(!requests.acknowledge(2));
    assert_eq!(requests.request(), 2);
    assert!(!requests.acknowledge(1));
    assert!(requests.acknowledge(2));
    assert!(!requests.acknowledge(2));
}

#[test]
fn invalid_and_out_of_order_samples_are_isolated_per_channel() {
    let mut model = AcquisitionModel::demo();
    assert!(model.ingest_batch([5.0; 8], 100));
    let before = model.snapshot();
    for (values, at) in [
        ([f32::NAN; 8], 101),
        ([f32::INFINITY; 8], 102),
        ([9.0; 8], 100),
        ([9.0; 8], 99),
    ] {
        assert!(!model.ingest_batch(values, at));
        assert_eq!(model.snapshot(), before);
    }
    assert!(model.ingest_batch([f32::NAN, -20.0, 6.0, 7.0, 8.0, 9.0, 10.0, 11.0], 103));
    let after = model.snapshot();
    assert_eq!(after.sequence, 2);
    assert_eq!(after.sensor(SensorId::Boost), before.sensor(SensorId::Boost));
    assert_eq!(after.sensor(SensorId::Oil).minimum(), Some(-20.0));
    // Retry can fill the one channel without reaccepting the others' timestamp.
    assert!(model.ingest_batch([2.0; 8], 103));
    assert_eq!(model.snapshot().sensor(SensorId::Boost).value(), Some(2.0));
    assert_eq!(model.snapshot().sensor(SensorId::Oil), after.sensor(SensorId::Oil));
}

#[test]
fn reset_generation_is_acknowledged_atomically_and_replays_do_nothing() {
    let commands = Signal::<CriticalSectionRawMutex, u64>::new();
    let mut model = AcquisitionModel::demo();
    model.ingest_batch([1.0; 8], 0);
    model.ingest_batch([20.0; 8], 10);
    model.ingest_batch([2.0; 8], 20);
    let old = model.snapshot();
    commands.signal(1);
    commands.signal(2);
    assert!(model.reset_statistics(commands.try_take().unwrap()));
    assert!(commands.try_take().is_none());
    let reset = model.snapshot();
    assert_eq!(reset.sequence, old.sequence);
    assert_eq!(reset.reset_generation, 2);
    let oil = reset.sensor(SensorId::Oil);
    assert_eq!(oil.value(), Some(2.0));
    assert_eq!(oil.maximum(), Some(2.0));
    assert_eq!(oil.get_average(), None);
    assert_eq!(oil.history().count(), 0);
    assert!(!oil.is_new_peak_at(20));
    assert!(!model.reset_statistics(2));
    assert!(!model.reset_statistics(1));
    assert_eq!(model.snapshot(), reset);
    let mut display = old;
    assert!(display.accept(reset));
    assert!(!display.accept(old));
    assert!(!display.accept(reset));
    model.ingest_batch([3.0; 8], 30);
    assert!(display.accept(model.snapshot()));
    assert_eq!(display.sensor(SensorId::Oil).maximum(), Some(3.0));
    assert!(model.reset_statistics(u64::MAX));
    assert!(!model.reset_statistics(0));
}

#[test]
fn snapshot_indicators_expire_without_a_new_measurement() {
    let mut model = AcquisitionModel::demo();
    for at in (0..=4000).step_by(200) {
        model.ingest_batch([at as f32; 8], at);
    }
    let snapshot = model.snapshot();
    let oil = snapshot.sensor(SensorId::Oil);
    assert_eq!(oil.get_trend_at(4000), Some(true));
    assert_eq!(oil.get_trend_at(6000), Some(true));
    assert_eq!(oil.get_trend_at(6001), None);
    assert_eq!(oil.get_trend_at(3999), None);
    assert!(oil.is_new_peak_at(4749));
    assert!(!oil.is_new_peak_at(4750));
    assert!(!oil.is_new_peak_at(3999));
    assert!(!DashboardSnapshot::EMPTY.sensor(SensorId::Oil).is_new_peak_at(0));
    assert_eq!(DashboardSnapshot::EMPTY.sensor(SensorId::Oil).get_trend_at(0), None);
}

#[test]
fn concurrent_watch_publication_is_coherent_and_eventually_latest() {
    let watch = Watch::<CriticalSectionRawMutex, DashboardSnapshot, 1>::new();
    let mut receiver = watch.receiver().unwrap();
    std::thread::scope(|scope| {
        scope.spawn(|| {
            let mut model = AcquisitionModel::demo();
            for i in 1..=10_000u64 {
                model.ingest_batch([i as f32; 8], i * 10);
                watch.sender().send(model.snapshot());
            }
        });
        let mut display = DashboardSnapshot::EMPTY;
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while display.sequence < 10_000 {
            assert!(std::time::Instant::now() < deadline, "publication stalled");
            if let Some(snapshot) = receiver.try_changed() {
                assert!(display.accept(snapshot));
                for sensor in display.sensors {
                    assert_eq!(sensor.value(), Some(display.sequence as f32));
                    assert_eq!(sensor.sampled_at_ms(), Some(display.sequence * 10));
                    assert_eq!(sensor.maximum(), Some(display.sequence as f32));
                    assert_eq!(sensor.minimum(), Some(1.0));
                }
            } else {
                std::thread::yield_now();
            }
        }
    });
}

#[test]
fn publication_copy_and_single_owner_storage_have_fixed_bounds() {
    assert!(
        // Traced Task5 ARM snapshot4144 B, rounded guard4160 B.
        core::mem::size_of::<DashboardSnapshot>() <= 4160,
        "snapshot {} B",
        core::mem::size_of::<DashboardSnapshot>()
    );
    // One engine shares storage with the historical replay engine; no config copy.
    assert!(core::mem::size_of::<AcquisitionModel>() <= 12_288 + 4096);
}

#[test]
fn untracked_public_snapshots_do_not_invent_diagnostic_events() {
    use dashboard_host_validation::cache_diagnostics::CacheExhaustionSeen;
    let mut seen = CacheExhaustionSeen::new();
    let mut snapshot = DashboardSnapshot::EMPTY;
    let mut state = dashboard_host_validation::sensor_state::SensorState::new();
    state.update_at(10.0, false, 0);
    state.reset_graph();
    snapshot.sensors[0] = state.snapshot();
    assert_eq!(snapshot.sensors[0].graph_cache_identity(), None);
    assert_eq!(seen.observe(snapshot.cache_exhaustion_status()).causes().count(), 0);
    assert_eq!(
        snapshot.publication_identity(),
        DashboardSnapshot::EMPTY.publication_identity()
    );
}

#[test]
fn timer_only_publication_is_accepted_without_advancing_sample_identity() {
    let mut model = AcquisitionModel::demo();
    model.ingest_batch([1.0; 8], 0);
    let mut displayed = model.snapshot();
    model.tick(2_000);
    let scrolled = model.snapshot();
    assert_eq!(scrolled.sequence, displayed.sequence);
    assert_eq!(scrolled.metadata, displayed.metadata);
    assert!(displayed.accept(scrolled), "timer changes must reach the receiver");
    assert!(!displayed.accept(scrolled), "duplicate publication remains suppressed");
}

#[test]
fn freshness_gap_preserves_prior_qualified_buckets_until_eviction() {
    let mut model = AcquisitionModel::demo();
    model.ingest_batch([1.0; 8], 0);
    model.ingest_batch([2.0; 8], 2_000);
    model.ingest_batch([3.0; 8], 4_000);
    let before = model.snapshot();
    model.tick(8_000);
    let gap = model.snapshot();
    assert_eq!(gap.metadata, before.metadata);
    assert_eq!(gap.sensors[1].history().bounds(), Some((1.0, 3.0)));
    assert_eq!(gap.sensors[1].history().iter().last().unwrap().1, None);
    model.ingest_batch([4.0; 8], 8_001);
    let recovered = model.snapshot();
    assert_eq!(recovered.sensors[1].history().count(), 4);
    assert_eq!(recovered.sensors[1].minimum(), Some(1.0));
}

#[test]
fn configured_dual_units_share_channel_history_and_new_owner_clears_it() {
    use dashboard_host_validation::view_config::{Channel, default_config, validate};
    let mut raw = default_config();
    raw.history_seconds[Channel::Lambda.index()] = 10;
    let placement = raw.widgets[0].placement;
    let id = raw.widgets[0].id;
    let mut twin = raw
        .widgets
        .iter()
        .find(|w| w.channel == Channel::Lambda)
        .unwrap()
        .clone();
    twin.placement = placement;
    twin.id = id;
    twin.display.afr = !twin.display.afr;
    twin.advanced.readout = if twin.display.afr {
        dashboard_host_validation::view_config::Readout::Afr
    } else {
        dashboard_host_validation::view_config::Readout::Lambda
    };
    raw.widgets[0] = twin;
    let config = validate(raw).unwrap();
    let mut model = AcquisitionModel::demo_configured(&config);
    model.ingest_batch([1.0; 8], 0);
    model.ingest_batch([2.0; 8], 167);
    let snapshot = model.snapshot();
    assert_eq!(snapshot.sensors[7].history().duration_seconds(), 10);
    assert_eq!(
        snapshot.sensors[7].history().iter().collect::<Vec<_>>(),
        [(0, Some(1.0)), (167, Some(2.0))]
    );
    assert_eq!(
        config
            .config()
            .widgets
            .iter()
            .filter(|w| w.channel == Channel::Lambda)
            .count(),
        2
    );
    let mut replacement = AcquisitionModel::demo_configured(&config);
    let empty = replacement.snapshot();
    assert_eq!(empty.sensors[7].history().bounds(), None);
    let mut displayed = snapshot;
    assert!(displayed.accept(empty), "new run can begin with sequence zero");
    assert!(!displayed.accept(snapshot), "old run cannot replace new run");
    replacement.ingest_batch([3.0; 8], 0);
    assert!(displayed.accept(replacement.snapshot()));
}
