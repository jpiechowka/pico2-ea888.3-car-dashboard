//! Delayed-event versus presentation-clock regression tests for canonical history.
use dashboard_core::acquisition::AcquisitionModel;
use dashboard_core::history::History;
use dashboard_core::samples::{Quantity, Sample, SensorId, Source};

/// Returns unit after a newly received but delayed sample updates its original
/// bucket without erasing earlier acquisition history; panics on accidental reanchor.
#[test]
fn accepted_delayed_sample_preserves_earlier_acquisition_history() {
    let mut model = AcquisitionModel::demo();
    for (sequence, at) in [(1, 0), (2, 2000), (3, 4000)] {
        model
            .ingest_sample(
                Sample {
                    value: Quantity::CoolantC(90.0),
                    received_at_ms: at,
                    sequence,
                    source: Source::Demo,
                },
                at,
            )
            .unwrap();
    }
    model.tick(4500);
    model
        .ingest_sample(
            Sample {
                value: Quantity::CoolantC(91.0),
                received_at_ms: 4250,
                sequence: 4,
                source: Source::Demo,
            },
            4500,
        )
        .unwrap();
    let snapshot = model.snapshot();
    assert_eq!(
        snapshot.sensor(SensorId::Water).history().iter().collect::<Vec<_>>(),
        [(0, Some(90.0)), (2000, Some(90.0)), (4000, Some(91.0))]
    );
}

/// Returns unit after checking delayed insertion, fault-gap duplicate rejection,
/// recovery and actual clock-reversal clearing; panics on chronology drift.
#[test]
fn delayed_history_events_preserve_window_and_fault_gap_until_real_clock_reversal() {
    let mut history = History::new(10);
    history.observe(1.0, 0);
    history.observe(2.0, 167);
    history.observe(3.0, 334);
    history.advance(1000);
    history.observe(4.0, 501);
    assert_eq!(
        history.view().iter().collect::<Vec<_>>(),
        [
            (0, Some(1.0)),
            (167, Some(2.0)),
            (334, Some(3.0)),
            (500, Some(4.0)),
            (667, None),
            (834, None),
            (1000, None),
        ]
    );
    history.invalidate(502);
    history.observe(99.0, 501);
    assert_eq!(history.view().count(), 2, "duplicate cannot refill a faulted bucket");
    history.observe(5.0, 503);
    assert_eq!(history.view().newest_ordinal(), 6);
    assert_eq!(history.view().point(56), Some((500, Some(5.0))));
    assert_eq!(history.view().point(55), Some((334, None)));
    history.advance(100);
    assert_eq!(history.view().count(), 0, "actual backwards clock clears history");
    history.observe(6.0, 100);
    assert_eq!(history.view().iter().collect::<Vec<_>>(), [(0, Some(6.0))]);
}

/// Returns unit after confirming expired samples/faults cannot alias live circular
/// slots and the oldest retained fault only removes its own point; panics on aliasing.
#[test]
fn events_outside_history_window_cannot_alias_live_ring_slots() {
    let mut history = History::new(60);
    history.observe(1.0, 0);
    history.advance(60_000);
    history.observe(99.0, 999);
    assert_eq!(history.view().count(), 0, "expired samples stay outside the window");
    history.observe(2.0, 1000);
    history.observe(3.0, 60_000);
    history.invalidate(1000);
    assert_eq!(history.view().count(), 1);
    assert_eq!(history.view().point(59), Some((60_000, Some(3.0))));
    history.invalidate(999);
    assert_eq!(history.view().count(), 1, "expired faults cannot clear current data");
}
