use dashboard_host_validation::display::te::{TE_CAPACITY, TeObservations};

#[test]
fn startup_and_missing_edges_never_invent_a_period() {
    let mut edges = TeObservations::new();
    assert_eq!(edges.latest(), None);
    assert_eq!(edges.latest_interval_us(), None);
    assert_eq!(edges.age_us(100_000), None);
    assert!(edges.record(0));
    assert_eq!(edges.latest(), Some(0));
    assert_eq!(edges.latest_interval_us(), None);
    assert_eq!(edges.age_us(1_000_000), Some(1_000_000));
    assert!(edges.record(1_000_000));
    assert_eq!(edges.latest_interval_us(), Some(1_000_000));
    // A long gap remains an observation; it is never converted to a guessed
    // count of 60Hz frames or labeled a measured physical refresh period.
    assert_eq!(edges.len(), 2);
    assert_eq!(edges.age_us(999_999), None);
}

#[test]
fn bounded_ring_keeps_order_and_counts_overwritten_observations() {
    let mut edges = TeObservations::new();
    assert!(core::mem::size_of::<TeObservations>() <= 576);
    for i in 0..1_000 {
        assert!(edges.record(i * 16_667));
        assert_eq!(edges.len(), ((i + 1) as usize).min(TE_CAPACITY));
        assert_eq!(edges.latest(), Some(i * 16_667));
    }
    assert_eq!(edges.overwritten(), 1_000 - TE_CAPACITY as u64);
    for i in 0..TE_CAPACITY {
        assert_eq!(edges.get(i), Some((1_000 - TE_CAPACITY as u64 + i as u64) * 16_667));
    }
    assert_eq!(edges.get(TE_CAPACITY), None);
    assert_eq!(edges.get(usize::MAX), None);
    assert_eq!(edges.latest_interval_us(), Some(16_667));
}

#[test]
fn duplicate_and_regressed_observations_are_rejected_without_corrupting_history() {
    let mut edges = TeObservations::new();
    assert!(edges.record(50));
    assert!(!edges.record(50));
    assert!(!edges.record(49));
    assert_eq!(edges.rejected(), 2);
    assert_eq!(edges.len(), 1);
    assert_eq!(edges.latest(), Some(50));
    assert!(edges.record(u64::MAX));
    assert_eq!(edges.latest_interval_us(), Some(u64::MAX - 50));
    assert!(!edges.record(0));
    assert_eq!(edges.latest(), Some(u64::MAX));
}

#[test]
fn maintenance_gap_discards_pending_edge_and_requires_two_new_observations() {
    let mut edges = TeObservations::new();
    assert!(edges.record(10));
    assert!(edges.record(20));
    edges.mark_gap();
    assert_eq!(edges.gaps(), 1);
    assert_eq!(edges.latest_interval_us(), None);
    assert!(!edges.record(1000)); // pending interrupt may predate XIP restoration
    assert_eq!(edges.latest(), Some(20));
    assert_eq!(edges.discarded_after_gap(), 1);
    assert!(edges.record(1010));
    assert_eq!(edges.latest_interval_us(), None);
    assert!(!edges.record(1009));
    assert_eq!(edges.latest_interval_us(), None);
    assert!(edges.record(1020));
    assert_eq!(edges.latest_interval_us(), Some(10));
    assert_eq!(edges.get(0), Some(10)); // preserve earlier raw observations
    assert_eq!(edges.len(), 4);
    edges.mark_gap();
    edges.mark_gap();
    assert_eq!(edges.gaps(), 3);
    assert!(!edges.record(2000));
    assert_eq!(edges.discarded_after_gap(), 2);
    assert!(edges.record(2010));
    assert_eq!(edges.latest_interval_us(), None);
}
