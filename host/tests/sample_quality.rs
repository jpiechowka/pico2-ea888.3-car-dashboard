use dashboard_host_validation::acquisition::{AcquisitionModel, DashboardSnapshot, SensorId};
use dashboard_host_validation::samples::{DataMode, Quality, Quantity, Sample, SampleError, Source};

fn coolant(
    value: f32,
    sequence: u64,
    at: u64,
) -> Sample {
    Sample {
        value: Quantity::CoolantC(value),
        sequence,
        received_at_ms: at,
        source: Source::Obd { ecu: 0x7E8, pid: 0x05 },
    }
}

#[test]
fn vehicle_startup_and_channel_absence_never_publish_zero_as_a_measurement() {
    let mut model = AcquisitionModel::new();
    assert_eq!(model.snapshot().mode, DataMode::Vehicle);
    for sensor in SensorId::ALL {
        assert_eq!(model.snapshot().quality(sensor, 0), Quality::Unknown);
        assert_eq!(model.snapshot().value_at(sensor, 0), None);
    }
    assert!(
        !model.ingest_batch([90.0; 8], 0),
        "untyped demo injection must be disabled in Vehicle"
    );
    assert_eq!(model.ingest_sample(coolant(90.0, 1, 10), 10), Ok(()));
    let snapshot = model.snapshot();
    assert_eq!(snapshot.value_at(SensorId::Water, 10), Some(90.0));
    assert_eq!(snapshot.value_at(SensorId::Oil, 10), None);
    assert_eq!(snapshot.metadata[SensorId::Water as usize].sequence, 1);
}

#[test]
fn staleness_is_a_presentation_deadline_and_does_not_fabricate_history() {
    let mut model = AcquisitionModel::new();
    model.ingest_sample(coolant(-20.0, 1, 100), 100).unwrap();
    let snapshot = model.snapshot();
    let boundary = 100 + SensorId::Water.stale_after_ms();
    assert_eq!(snapshot.quality(SensorId::Water, boundary), Quality::Valid);
    assert_eq!(snapshot.quality(SensorId::Water, boundary + 1), Quality::Stale);
    assert_eq!(snapshot.value_at(SensorId::Water, boundary + 1), None);
    assert_eq!(snapshot.quality(SensorId::Water, 99), Quality::InvalidPayload);
    assert_eq!(model.snapshot(), snapshot);
}

#[test]
fn sequence_time_and_source_checks_reject_replay_without_mutating_a_channel() {
    let mut model = AcquisitionModel::new();
    model.ingest_sample(coolant(80.0, 5, 100), 100).unwrap();
    let before = model.snapshot();
    for sample in [
        coolant(100.0, 5, 101),
        coolant(100.0, 4, 102),
        coolant(100.0, 6, 100),
        coolant(100.0, 6, 99),
    ] {
        assert_eq!(model.ingest_sample(sample, 200), Err(SampleError::OutOfOrder));
        assert_eq!(model.snapshot(), before);
    }
    assert_eq!(
        model.ingest_sample(coolant(100.0, 6, 201), 200),
        Err(SampleError::Future)
    );
    let mismatched = Sample {
        source: Source::Obd { ecu: 0x7E8, pid: 0x42 },
        ..coolant(90.0, 6, 101)
    };
    assert_eq!(model.ingest_sample(mismatched, 101), Err(SampleError::Source));
    let changed_ecu = Sample {
        source: Source::Obd { ecu: 0x7E9, pid: 0x05 },
        ..coolant(90.0, 6, 101)
    };
    assert_eq!(model.ingest_sample(changed_ecu, 101), Err(SampleError::Source));
    assert_eq!(model.snapshot(), before);
}

#[test]
fn faults_suppress_values_preserve_statistics_and_recover_only_with_a_new_valid_sample() {
    let mut model = AcquisitionModel::new();
    model.ingest_sample(coolant(80.0, 1, 10), 10).unwrap();
    model.ingest_sample(coolant(120.0, 2, 20), 20).unwrap();
    let before = *model.snapshot().sensor(SensorId::Water);
    assert_eq!(
        model.ingest_sample(coolant(f32::NAN, 3, 30), 30),
        Err(SampleError::InvalidValue)
    );
    assert_eq!(model.snapshot().quality(SensorId::Water, 30), Quality::InvalidPayload);
    assert_eq!(model.snapshot().value_at(SensorId::Water, 30), None);
    let snapshot = model.snapshot();
    let after = snapshot.sensor(SensorId::Water);
    assert_eq!(
        (
            after.value(),
            after.minimum(),
            after.maximum(),
            after.get_average(),
            after.sampled_at_ms()
        ),
        (
            before.value(),
            before.minimum(),
            before.maximum(),
            before.get_average(),
            before.sampled_at_ms()
        )
    );
    assert_eq!(after.history().bounds(), None);
    assert!(!model.mark_fault(SensorId::Water, Quality::Valid, 31));
    assert!(!model.mark_fault(SensorId::Water, Quality::TransportError, 29));
    assert!(model.mark_fault(SensorId::Water, Quality::TransportError, 40));
    assert_eq!(
        model.ingest_sample(coolant(90.0, 4, 39), 50),
        Err(SampleError::OutOfOrder)
    );
    model.ingest_sample(coolant(90.0, 4, 50), 50).unwrap();
    assert_eq!(model.snapshot().value_at(SensorId::Water, 50), Some(90.0));
    assert_eq!(model.snapshot().sensor(SensorId::Water).maximum(), Some(120.0));
    assert!(model.mark_fault(SensorId::Egt, Quality::Unsupported, 50));
    assert_eq!(model.snapshot().quality(SensorId::Egt, 1_000_000), Quality::Unsupported);
}

#[test]
fn demo_is_explicit_and_cannot_cross_into_vehicle_or_masquerade_as_an_obd_source() {
    let demo_sample = Sample {
        value: Quantity::Lambda(1.0),
        sequence: 1,
        received_at_ms: 0,
        source: Source::Demo,
    };
    let mut vehicle = AcquisitionModel::new();
    assert_eq!(vehicle.ingest_sample(demo_sample, 0), Err(SampleError::Source));
    let mut demo = AcquisitionModel::demo();
    demo.ingest_sample(demo_sample, 0).unwrap();
    assert_eq!(demo.snapshot().mode, DataMode::Demo);
    assert_eq!(demo.snapshot().value_at(SensorId::Lambda, 0), Some(1.0));
    assert_eq!(demo.ingest_sample(coolant(80.0, 1, 10), 10), Err(SampleError::Source));
    assert_eq!(DashboardSnapshot::EMPTY.mode, DataMode::Vehicle);
}

#[test]
fn demo_batches_preserve_each_typed_channel_sequence_and_preflight_exhaustion() {
    let mut model = AcquisitionModel::demo();
    model
        .ingest_sample(
            Sample {
                value: Quantity::Lambda(1.0),
                received_at_ms: 0,
                sequence: 100,
                source: Source::Demo,
            },
            0,
        )
        .unwrap();
    assert!(model.ingest_batch([1.0; 8], 1));
    assert_eq!(model.snapshot().metadata[SensorId::Lambda as usize].sequence, 101);
    assert_eq!(
        model.ingest_sample(
            Sample {
                value: Quantity::Lambda(1.0),
                received_at_ms: 2,
                sequence: 3,
                source: Source::Demo,
            },
            2
        ),
        Err(SampleError::OutOfOrder)
    );

    model
        .ingest_sample(
            Sample {
                value: Quantity::Lambda(1.0),
                received_at_ms: 2,
                sequence: u64::MAX,
                source: Source::Demo,
            },
            2,
        )
        .unwrap();
    let before = model.snapshot();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| model.ingest_batch([2.0; 8], 3)));
    assert!(result.is_err());
    assert_eq!(
        model.snapshot(),
        before,
        "exhaustion must fail before any channel changes"
    );
}

#[test]
fn every_channel_has_a_bounded_deadline_and_unrelated_can_ids_are_rejected() {
    let mut demo = AcquisitionModel::demo();
    demo.ingest_batch([1.0; 8], 100);
    for sensor in SensorId::ALL {
        let limit = 100 + sensor.stale_after_ms();
        assert_eq!(demo.snapshot().quality(sensor, limit), Quality::Valid);
        assert_eq!(demo.snapshot().quality(sensor, limit + 1), Quality::Stale);
        assert_eq!(demo.snapshot().value_at(sensor, u64::MAX), None);
    }
    for ecu in [0, 0x123, 0x7E0, 0x7E7, 0x7F0, 0x800, u16::MAX] {
        let mut vehicle = AcquisitionModel::new();
        assert_eq!(
            vehicle.ingest_sample(
                Sample {
                    source: Source::Obd { ecu, pid: 5 },
                    ..coolant(0.0, 1, 0)
                },
                0
            ),
            Err(SampleError::Source)
        );
    }
}

#[test]
fn configured_raw_peaks_reject_replays_and_clear_on_invalid_payload() {
    use dashboard_host_validation::alarms::RuleIndex;
    use dashboard_host_validation::view_config::*;
    let mut c = default_config();
    let position = c.widgets.iter().position(|w| w.channel == Channel::Water).unwrap();
    c.widgets[position].advanced.peak_mode = PeakSelection::Maximum;
    c.widgets[position].advanced.peak_hold_ms = 200;
    let c = validate(c).unwrap();
    let mut m = AcquisitionModel::configured(&c, 0x7E8);
    let index = RuleIndex::new(position).unwrap();
    m.ingest_sample(coolant(120., 1, 100), 100).unwrap();
    assert_eq!(m.snapshot().peak_at(index, 100), Some(120.));
    assert_eq!(
        m.ingest_sample(coolant(130., 1, 200), 200),
        Err(SampleError::OutOfOrder)
    );
    m.tick(300);
    assert_eq!(m.snapshot().peak_at(index, 300), None);
    m.ingest_sample(coolant(110., 2, 310), 310).unwrap();
    assert_eq!(
        m.ingest_sample(coolant(f32::NAN, 3, 320), 320),
        Err(SampleError::InvalidValue)
    );
    assert_eq!(m.snapshot().peak_at(index, 320), None);
}
