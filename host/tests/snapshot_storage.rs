//! Scalar snapshot storage preserves admitted values while recovering bounded publication space.
use dashboard_core::acquisition::{AcquisitionModel, DashboardSnapshot};
use dashboard_core::sensor_state::{PeakMode, SensorSnapshot, SensorState};

/// Guards the recovered 128 bytes needed by the four-cylinder publication without raising the existing cap.
#[test]
fn scalar_snapshot_recovers_group_publication_space() {
    eprintln!(
        "SensorSnapshot={} DashboardSnapshot={} AcquisitionModel={}",
        core::mem::size_of::<SensorSnapshot>(),
        core::mem::size_of::<DashboardSnapshot>(),
        core::mem::size_of::<AcquisitionModel>()
    );
    assert!(core::mem::size_of::<SensorSnapshot>() <= 328);
    assert!(core::mem::size_of::<DashboardSnapshot>() <= 4160);
    assert!(core::mem::size_of::<AcquisitionModel>() <= 16_384);
}

/// Public snapshots retain exact admitted float bits and retain unavailable statistics after reset.
#[test]
fn scalar_snapshot_preserves_finite_bits_and_reset_absence() {
    for value in [
        0.0,
        -0.0,
        f32::from_bits(1),
        -f32::from_bits(1),
        -1_000_000.0,
        1_000_000.0,
    ] {
        let mut state = SensorState::new();
        assert_eq!(state.snapshot(), SensorSnapshot::EMPTY);
        assert!(state.ingest(value, 0, PeakMode::None));
        let snapshot = state.snapshot();
        assert_eq!(snapshot.value().unwrap().to_bits(), value.to_bits());
        assert_eq!(snapshot.minimum().unwrap().to_bits(), value.to_bits());
        assert_eq!(snapshot.maximum().unwrap().to_bits(), value.to_bits());
        assert_eq!(
            snapshot.get_average().unwrap().to_bits(),
            state.get_average().unwrap().to_bits()
        );
        state.reset_statistics();
        assert_eq!(state.snapshot().value().unwrap().to_bits(), value.to_bits());
        assert_eq!(state.snapshot().get_average(), None);
    }
}

/// Semantic equality treats opposite signed zeros as equal while getters keep their distinct bits.
#[test]
fn scalar_snapshot_signed_zero_equality_is_numeric() {
    let mut positive = SensorState::new();
    let mut negative = SensorState::new();
    assert!(positive.ingest(0.0, 0, PeakMode::None));
    assert!(negative.ingest(-0.0, 0, PeakMode::None));
    assert_eq!(positive.snapshot(), negative.snapshot());
    assert_ne!(
        positive.snapshot().value().unwrap().to_bits(),
        negative.snapshot().value().unwrap().to_bits()
    );
}

/// Browser replay merges only extrema, leaving reading/time/history/mean and empty source snapshots intact.
#[test]
fn scalar_snapshot_replay_merge_preserves_non_extrema_fields() {
    let mut old = SensorState::new();
    assert!(old.ingest(-1_000_000.0, 0, PeakMode::Range));
    assert!(old.ingest(1_000_000.0, 5_000, PeakMode::Range));
    let mut current = SensorState::new();
    assert!(current.ingest(-f32::from_bits(1), 10_000, PeakMode::Range));
    let original = current.snapshot();
    let mut merged = original;
    merged.retain_extrema_from(&old.snapshot());
    assert_eq!(merged.minimum(), Some(-1_000_000.0));
    assert_eq!(merged.maximum(), Some(1_000_000.0));
    assert_eq!(merged.value().unwrap().to_bits(), original.value().unwrap().to_bits());
    assert_eq!(merged.sampled_at_ms(), original.sampled_at_ms());
    assert_eq!(
        merged.get_average().unwrap().to_bits(),
        original.get_average().unwrap().to_bits()
    );
    assert_eq!(
        merged.history().iter().collect::<Vec<_>>(),
        original.history().iter().collect::<Vec<_>>()
    );
    assert_eq!(merged.graph_cache_identity(), original.graph_cache_identity());
    let before_empty_merge = merged;
    merged.retain_extrema_from(&SensorSnapshot::EMPTY);
    assert_eq!(merged, before_empty_merge);
    let mut empty = SensorSnapshot::EMPTY;
    empty.retain_extrema_from(&old.snapshot());
    assert_eq!(empty.value(), None);
    assert_eq!(empty.get_average(), None);
    assert_eq!(empty.minimum(), Some(-1_000_000.0));
    assert_eq!(empty.maximum(), Some(1_000_000.0));
}

/// Mean ring wrap and reset preserve every public state result through snapshot conversion, including nonfinite
/// rejection.
#[test]
fn scalar_snapshot_matches_owner_across_mean_eviction_and_invalid_samples() {
    let mut state = SensorState::new();
    for index in 0..240_u64 {
        let value = match index % 4 {
            0 => -1_000_000.0,
            1 => 1_000_000.0,
            2 => f32::from_bits(1),
            _ => -0.0,
        };
        assert!(state.ingest(value, index * 5_000, PeakMode::Range));
        let snapshot = state.snapshot();
        for (actual, expected) in [
            (snapshot.value(), state.value()),
            (snapshot.minimum(), state.minimum()),
            (snapshot.maximum(), state.maximum()),
            (snapshot.get_average(), state.get_average()),
        ] {
            assert_eq!(actual.map(f32::to_bits), expected.map(f32::to_bits));
        }
        for invalid in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, 1_000_001.0, -1_000_001.0] {
            assert!(!state.ingest(invalid, index * 5_000 + 1, PeakMode::Range));
            assert_eq!(state.snapshot(), snapshot);
        }
        if index == 120 {
            state.reset_statistics();
            assert_eq!(state.snapshot().get_average(), None);
        }
    }
}
