//! Offline cylinder admission, independent freshness and coherent publication regressions.
use dashboard_core::acquisition::{AcquisitionModel, DashboardSnapshot};
use dashboard_core::ignition::{Cylinder, IgnitionSample, IgnitionSnapshot, IgnitionSource};
use dashboard_core::samples::{DataMode, Quality, SampleError, Source};

/// Returns a synthetic candidate for `cylinder` with signed `degrees`, millisecond
/// receive `at` and explicit per-cylinder `sequence`; the owner still validates it.
fn sample(
    cylinder: Cylinder,
    degrees: f32,
    at: u64,
    sequence: u64,
) -> IgnitionSample {
    IgnitionSample {
        cylinder,
        degrees,
        received_at_ms: at,
        sequence,
        source: Source::Demo,
    }
}

/// Catches accidental Vehicle enablement and missing-to-zero substitution.
#[test]
fn vehicle_stays_unsupported_and_new_demo_stays_unknown() {
    let mut vehicle = AcquisitionModel::new();
    let before = vehicle.snapshot();
    assert!(!vehicle.ingest_ignition_demo([0.0; 4], 0));
    assert_eq!(
        vehicle.ingest_ignition(sample(Cylinder::One, 0.0, 0, 1), 0),
        Err(SampleError::Source)
    );
    assert_eq!(vehicle.snapshot(), before);
    let demo = AcquisitionModel::demo().snapshot();
    for cylinder in Cylinder::ALL {
        assert_eq!(before.ignition.quality_at(cylinder, 0), Quality::Unsupported);
        assert_eq!(demo.ignition.quality_at(cylinder, 0), Quality::Unknown);
        assert_eq!(demo.ignition.value_at(cylinder, DataMode::Demo, 0), None);
        assert_eq!(demo.ignition.received_at_ms(cylinder), None);
        assert_eq!(demo.ignition.source(cylinder), IgnitionSource::Unknown);
    }
}

/// Catches shared timestamps, exclusive expiry and retained numbers after expiry.
#[test]
fn cylinders_expire_independently_and_reset_does_not_refresh() {
    let mut owner = AcquisitionModel::demo();
    for (cylinder, at, degrees) in [
        (Cylinder::One, 0, 0.0),
        (Cylinder::Two, 1, -2.5),
        (Cylinder::Three, 2, 1.25),
        (Cylinder::Four, 3, -0.0),
    ] {
        owner.ingest_ignition(sample(cylinder, degrees, at, 1), at).unwrap();
    }
    let before = owner.snapshot();
    assert_eq!(before.sequence, 4);
    for (i, cylinder) in Cylinder::ALL.into_iter().enumerate() {
        let at = i as u64;
        assert_eq!(before.ignition.quality_at(cylinder, at + 500), Quality::Valid);
        assert_eq!(before.ignition.quality_at(cylinder, at + 501), Quality::Stale);
        assert_eq!(before.ignition.value_at(cylinder, DataMode::Demo, at + 501), None);
        assert_eq!(before.ignition.value_at(cylinder, DataMode::Vehicle, at), None);
    }
    assert_eq!(
        before
            .ignition
            .value_at(Cylinder::Four, DataMode::Demo, 3)
            .unwrap()
            .to_bits(),
        (-0.0_f32).to_bits()
    );
    owner.reset_statistics(1);
    assert_eq!(owner.snapshot().ignition, before.ignition);
    owner
        .ingest_ignition(sample(Cylinder::Four, -7.0, 502, 2), 502)
        .unwrap();
    let after = owner.snapshot();
    assert_eq!(after.ignition.quality_at(Cylinder::Two, 502), Quality::Stale);
    assert_eq!(after.ignition.value_at(Cylinder::Four, DataMode::Demo, 502), Some(-7.0));
    assert_eq!(before.ignition.value_at(Cylinder::Four, DataMode::Demo, 3), Some(-0.0));
}

/// Catches invalid typed values remaining visible and bad batch entries faulting siblings.
#[test]
fn numeric_bounds_typed_faults_and_batch_ignore_are_distinct() {
    let mut owner = AcquisitionModel::demo();
    assert!(owner.ingest_ignition_demo([-327.68, 327.67, 0.0, -0.0], 0));
    for (cylinder, expected) in Cylinder::ALL.into_iter().zip([-327.68, 327.67, 0.0, -0.0]) {
        assert_eq!(
            owner.snapshot().ignition.value_at(cylinder, DataMode::Demo, 0),
            Some(expected)
        );
    }
    for (i, value) in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, -327.69, 327.68]
        .into_iter()
        .enumerate()
    {
        let at = i as u64 + 1;
        assert_eq!(
            owner.ingest_ignition(sample(Cylinder::One, value, at, at + 1), at),
            Err(SampleError::InvalidValue)
        );
        let snap = owner.snapshot();
        assert_eq!(snap.ignition.quality_at(Cylinder::One, at), Quality::InvalidPayload);
        assert_eq!(snap.ignition.value_at(Cylinder::One, DataMode::Demo, at), None);
        assert_eq!(snap.ignition.received_at_ms(Cylinder::One), Some(0));
        assert_eq!(snap.ignition.sequence(Cylinder::One), at + 1);
    }
    let before = owner.snapshot();
    assert!(owner.ingest_ignition_demo([f32::NAN, -1.0, f32::INFINITY, -2.0], 6));
    let after = owner.snapshot();
    assert_eq!(after.sequence, before.sequence + 1);
    assert_eq!(after.ignition.status_at_ms(Cylinder::One), 5);
    assert_eq!(after.ignition.quality_at(Cylinder::One, 6), Quality::InvalidPayload);
    assert_eq!(after.ignition.received_at_ms(Cylinder::Three), Some(0));
    assert_eq!(after.ignition.value_at(Cylinder::Two, DataMode::Demo, 6), Some(-1.0));
    assert_eq!(after.ignition.value_at(Cylinder::Four, DataMode::Demo, 6), Some(-2.0));
    assert!(!owner.ingest_ignition_demo([f32::NAN; 4], 7));
    assert_eq!(owner.snapshot(), after);
}

/// Catches rejected source/time/order inputs mutating publication or last-good data.
#[test]
fn rejection_is_atomic_and_status_time_blocks_old_recovery() {
    let mut owner = AcquisitionModel::demo();
    owner.ingest_ignition(sample(Cylinder::One, -2.0, 10, 2), 10).unwrap();
    let before = owner.snapshot();
    for (candidate, now, error) in [
        (sample(Cylinder::One, 1.0, 12, 3), 11, SampleError::Future),
        (sample(Cylinder::One, 1.0, 11, 0), 11, SampleError::OutOfOrder),
        (sample(Cylinder::One, 1.0, 11, 2), 11, SampleError::OutOfOrder),
        (sample(Cylinder::One, 1.0, 10, 3), 11, SampleError::OutOfOrder),
        (sample(Cylinder::One, 1.0, 9, 3), 11, SampleError::OutOfOrder),
        (
            IgnitionSample {
                source: Source::Unknown,
                ..sample(Cylinder::One, 1.0, 11, 3)
            },
            11,
            SampleError::Source,
        ),
        (
            IgnitionSample {
                source: Source::Obd { ecu: 0x7E8, pid: 0x05 },
                ..sample(Cylinder::One, 1.0, 11, 3)
            },
            11,
            SampleError::Source,
        ),
        (
            IgnitionSample {
                source: Source::ObdMapDifference { ecu: 0x7E8 },
                ..sample(Cylinder::One, 1.0, 11, 3)
            },
            11,
            SampleError::Source,
        ),
    ] {
        assert_eq!(owner.ingest_ignition(candidate, now), Err(error));
        assert_eq!(owner.snapshot(), before);
        assert_eq!(owner.snapshot().publication_identity(), before.publication_identity());
    }
    assert!(owner.mark_ignition_fault(Cylinder::One, Quality::TransportError, 20));
    let fault = owner.snapshot();
    assert_eq!(
        owner.ingest_ignition(sample(Cylinder::One, 0.0, 19, 3), 20),
        Err(SampleError::OutOfOrder)
    );
    for quality in [Quality::Valid, Quality::Stale, Quality::TransportError] {
        assert!(!owner.mark_ignition_fault(Cylinder::One, quality, 20));
    }
    assert!(!owner.mark_ignition_fault(Cylinder::One, Quality::Unknown, 19));
    assert_eq!(owner.snapshot(), fault);
    owner.ingest_ignition(sample(Cylinder::One, 0.0, 20, 3), 20).unwrap();
    assert_eq!(
        owner.snapshot().ignition.value_at(Cylinder::One, DataMode::Demo, 20),
        Some(0.0)
    );
    let recovered = owner.snapshot();
    assert!(!owner.ingest_ignition_demo([0.0, f32::NAN, f32::NAN, f32::NAN], 20));
    assert_eq!(owner.snapshot(), recovered);
}

/// Catches clock catch-up reviving a sample invalidated by owner clock reversal.
#[test]
fn clock_reversal_is_sticky_until_new_sample() {
    let mut owner = AcquisitionModel::demo();
    owner.ingest_ignition_demo([-1.0; 4], 100);
    owner.tick(200);
    owner.tick(99);
    let invalid = owner.snapshot();
    for cylinder in Cylinder::ALL {
        assert_eq!(invalid.ignition.quality_at(cylinder, 200), Quality::InvalidPayload);
        assert_eq!(invalid.ignition.value_at(cylinder, DataMode::Demo, 200), None);
        assert!(invalid.ignition.status_at_ms(cylinder) >= 100);
    }
    owner.tick(200);
    assert_eq!(owner.snapshot().ignition, invalid.ignition);
    owner.ingest_ignition(sample(Cylinder::One, -0.1, 201, 2), 201).unwrap();
    assert_eq!(
        owner.snapshot().ignition.value_at(Cylinder::One, DataMode::Demo, 201),
        Some(-0.1)
    );
    assert_eq!(
        owner.snapshot().ignition.value_at(Cylinder::Two, DataMode::Demo, 201),
        None
    );
}

/// Catches timestamp sentinels truncating the supported u64 range and budget growth.
#[test]
fn full_timestamp_range_and_memory_caps_are_preserved() {
    let mut owner = AcquisitionModel::demo();
    owner
        .ingest_ignition(sample(Cylinder::Four, -1.0, u64::MAX, 1), u64::MAX)
        .unwrap();
    assert_eq!(owner.snapshot().ignition.received_at_ms(Cylinder::Four), Some(u64::MAX));
    assert_eq!(
        owner
            .snapshot()
            .ignition
            .value_at(Cylinder::Four, DataMode::Demo, u64::MAX),
        Some(-1.0)
    );
    assert_eq!(
        owner.snapshot().ignition.quality_at(Cylinder::Four, u64::MAX - 1),
        Quality::InvalidPayload
    );
    assert!(core::mem::size_of::<IgnitionSnapshot>() <= 128);
    assert!(core::mem::size_of::<DashboardSnapshot>() <= 4160);
    assert!(core::mem::size_of::<AcquisitionModel<'_>>() <= 16384);
    println!(
        "IgnitionSnapshot={} DashboardSnapshot={} AcquisitionModel={}",
        core::mem::size_of::<IgnitionSnapshot>(),
        core::mem::size_of::<DashboardSnapshot>(),
        core::mem::size_of::<AcquisitionModel<'_>>()
    );
}

/// Catches a late-cylinder sequence overflow partially applying earlier entries.
#[test]
fn batch_preflights_all_required_cylinder_sequences() {
    let mut owner = AcquisitionModel::demo();
    owner
        .ingest_ignition(sample(Cylinder::Four, -1.0, 0, u64::MAX), 0)
        .unwrap();
    let before = owner.snapshot();
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            owner.ingest_ignition_demo([-2.0; 4], 1)
        }))
        .is_err()
    );
    assert_eq!(owner.snapshot(), before);
    assert_eq!(owner.snapshot().publication_identity(), before.publication_identity());
    assert!(owner.ingest_ignition_demo([-2.0, -3.0, -4.0, f32::NAN], 1));
    assert_eq!(owner.snapshot().ignition.sequence(Cylinder::One), 1);
    assert_eq!(owner.snapshot().ignition.sequence(Cylinder::Four), u64::MAX);
    assert_eq!(
        owner.snapshot().ignition.value_at(Cylinder::Four, DataMode::Demo, 1),
        Some(-1.0)
    );
}

/// Catches a new source session inheriting Demo values or accepting old publications.
#[test]
fn vehicle_replacement_clears_group_and_rejects_old_owner_publications() {
    let mut demo = AcquisitionModel::demo();
    demo.ingest_ignition_demo([0.0, -2.5, 1.0, -0.1], 10);
    let old = demo.snapshot();
    let mut displayed = old;
    let vehicle = AcquisitionModel::new().snapshot();
    assert!(displayed.accept(vehicle));
    assert!(!displayed.accept(old));
    for cylinder in Cylinder::ALL {
        assert_eq!(displayed.ignition.source(cylinder), IgnitionSource::Unknown);
        assert_eq!(displayed.ignition.quality_at(cylinder, 10), Quality::Unsupported);
        assert_eq!(displayed.ignition.value_at(cylinder, DataMode::Vehicle, 10), None);
    }
}

/// Catches group insertion shifting scalar rules or aliasing a grouped slot to Boost.
#[test]
fn mixed_config_keeps_disabled_group_slots_and_scalar_peaks() {
    use dashboard_config::{
        Channel,
        PeakSelection,
        Placement,
        Span,
        default_config,
        default_ignition_widget,
        default_widget,
        validate,
    };
    use dashboard_core::alarms::{RuleIndex, Status};
    let mut config = default_config();
    config.widgets.clear();
    let first_view = config.views[0].clone();
    for id in 2..=8 {
        let mut view = first_view.clone();
        view.id = id;
        config.views.push(view).unwrap();
    }
    for index in 0..64 {
        let placement = Placement {
            column: index % 4,
            row: (index % 8) / 4,
            span: Span::OneByOne,
        };
        let view = index / 8 + 1;
        let mut widget = if index % 2 == 0 {
            default_ignition_widget(u16::from(index) + 1, view, placement)
        } else {
            default_widget(Channel::Boost, u16::from(index) + 1, view, placement)
        };
        if index % 2 == 1 {
            widget.warning.enabled = true;
            widget.warning.on_ms = 100;
            widget.warning.trigger = 1.5;
            widget.warning.clear = 1.0;
            widget.advanced.peak_mode = PeakSelection::Maximum;
        } else {
            widget.warning.enabled = false;
        }
        config.widgets.push(widget).unwrap();
    }
    let config = validate(config).unwrap();
    let mut owner = AcquisitionModel::demo_configured(&config);
    for at in [0, 100] {
        owner.ingest_batch([2.0, 90.0, 95.0, 80.0, 30.0, 700.0, 13.8, 1.0], at);
        owner.ingest_ignition_demo([-6.0; 4], at);
        owner.tick(at);
    }
    let snapshot = owner.snapshot();
    for index in 0..64 {
        let rule = RuleIndex::new(index).unwrap();
        if index % 2 == 0 {
            assert_eq!(snapshot.rule_status_at(rule, 100), Status::Disabled);
            assert_eq!(snapshot.peak_at(rule, 100), None);
        } else {
            assert_eq!(snapshot.rule_status_at(rule, 100), Status::Active);
            assert_eq!(snapshot.peak_at(rule, 100), Some(2.0));
        }
    }
    owner.mark_ignition_fault(Cylinder::One, Quality::InvalidPayload, 101);
    assert_eq!(owner.snapshot().peaks(), snapshot.peaks());
}

/// Catches a publication combining different cylinders or generations under Watch coalescing.
#[test]
fn concurrent_group_publications_are_coherent_and_eventually_latest() {
    use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
    use embassy_sync::watch::Watch;
    let watch = Watch::<CriticalSectionRawMutex, DashboardSnapshot, 1>::new();
    let mut receiver = watch.receiver().unwrap();
    std::thread::scope(|scope| {
        scope.spawn(|| {
            let mut owner = AcquisitionModel::demo();
            for sequence in 1..=1_000u64 {
                let magnitude = (sequence % 100) as f32;
                owner.ingest_ignition_demo(
                    [magnitude, -magnitude, magnitude + 1.0, -magnitude - 1.0],
                    sequence * 10,
                );
                watch.sender().send(owner.snapshot());
            }
        });
        let mut displayed = DashboardSnapshot::EMPTY;
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while displayed.sequence < 1_000 {
            assert!(std::time::Instant::now() < deadline, "group publication stalled");
            if let Some(snapshot) = receiver.try_changed() {
                assert!(displayed.accept(snapshot));
                let at = displayed.sequence * 10;
                let magnitude = (displayed.sequence % 100) as f32;
                for (cylinder, expected) in
                    Cylinder::ALL
                        .into_iter()
                        .zip([magnitude, -magnitude, magnitude + 1.0, -magnitude - 1.0])
                {
                    assert_eq!(displayed.ignition.sequence(cylinder), displayed.sequence);
                    assert_eq!(displayed.ignition.received_at_ms(cylinder), Some(at));
                    assert_eq!(displayed.ignition.status_at_ms(cylinder), at);
                    assert_eq!(
                        displayed.ignition.value_at(cylinder, DataMode::Demo, at),
                        Some(expected)
                    );
                    assert_eq!(displayed.ignition.source(cylinder), IgnitionSource::Demo);
                }
            } else {
                std::thread::yield_now();
            }
        }
    });
}

/// Catches shared phases, frame-dependent steps and integer overflow at long uptimes.
#[test]
fn demo_generator_is_bounded_and_uses_independent_elapsed_time_phases() {
    use dashboard_core::demo::ignition_values;
    assert_eq!(ignition_values(0), [0.0, 0.0, -4.2, -2.7]);
    assert_eq!(ignition_values(8_000), ignition_values(0));
    assert_eq!(ignition_values(4_000)[0], -6.0);
    assert_eq!(ignition_values(7_000)[0], 0.5);
    for at in [0, 2_001, 4_000, 7_999, u64::MAX - 1, u64::MAX] {
        for value in ignition_values(at) {
            assert!(value.is_finite() && (-6.0..=1.0).contains(&value));
        }
    }
}
