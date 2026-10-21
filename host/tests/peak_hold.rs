//! Per-widget raw peak selection/expiry remains independent of paint cadence and smoothing.
use dashboard_host_validation::acquisition::AcquisitionModel;
use dashboard_host_validation::alarms::RuleIndex;
use dashboard_host_validation::samples::{Quality, SensorId};
use dashboard_host_validation::view_config::*;
/// Returns three boost widgets: maximum for 200/400ms and minimum for 200ms,
/// with warnings disabled so peak markers are tested independently.
fn config() -> ValidatedConfig {
    let mut c = default_config();
    c.widgets.truncate(3);
    for (i, w) in c.widgets.iter_mut().enumerate() {
        *w = default_widget(Channel::Boost, w.id, w.view, w.placement);
        w.display = default_widget(Channel::Boost, 1, 1, w.placement).display;
        w.advanced = AdvancedDisplay::for_channel(Channel::Boost, false);
        w.warning.enabled = false;
        w.advanced.peak_mode = if i == 2 {
            PeakSelection::Minimum
        } else {
            PeakSelection::Maximum
        };
        w.advanced.peak_hold_ms = if i == 1 { 400 } else { 200 };
    }
    validate(c).unwrap()
}
/// Publishes canonical boost `x` bar into demo model `m` at milliseconds `at`;
/// panics if the fixture violates monotonic valid batch admission.
fn sample(
    m: &mut AcquisitionModel<'_>,
    x: f32,
    at: u64,
) {
    assert!(m.ingest_batch([x, 90., 95., 80., 30., 700., 13.8, 1.], at));
}
/// Returns widget-rule `i`'s currently eligible peak from `m` at milliseconds
/// `now`, or `None` for expired/unqualified state; panics for an invalid rule index.
fn peak(
    m: &AcquisitionModel<'_>,
    i: usize,
    now: u64,
) -> Option<f32> {
    m.snapshot().peak_at(RuleIndex::new(i).unwrap(), now)
}
/// Coalesced raw extrema preserve each widget's duration/selection, expire via publication and clear on transport
/// failure.
#[test]
fn raw_extremes_survive_coalescing_with_independent_durations() {
    let c = config();
    let mut m = AcquisitionModel::demo_configured(&c);
    sample(&mut m, 1., 0);
    sample(&mut m, 3., 20);
    sample(&mut m, -0.5, 40);
    sample(&mut m, 2., 60);
    assert_eq!(peak(&m, 0, 60), Some(3.));
    assert_eq!(peak(&m, 1, 60), Some(3.));
    assert_eq!(peak(&m, 2, 60), Some(-0.5));
    let before = m.snapshot();
    m.tick(220);
    let after = m.snapshot();
    assert_eq!(peak(&m, 0, 220), None);
    assert_eq!(peak(&m, 1, 220), Some(3.));
    assert_eq!(peak(&m, 2, 220), Some(-0.5));
    assert_eq!(before.sequence, after.sequence);
    assert_ne!(before.publication_identity(), after.publication_identity());
    m.tick(240);
    assert_eq!(peak(&m, 2, 240), None);
    sample(&mut m, 2., 250);
    assert_eq!(peak(&m, 0, 250), Some(2.));
    assert_eq!(peak(&m, 1, 250), Some(3.));
    m.mark_fault(SensorId::Boost, Quality::TransportError, 260);
    assert_eq!(peak(&m, 0, 260), None);
}
/// Duplicate samples, reset, stale time and clock reversal cannot retain or extend expired peak markers.
#[test]
fn repeats_invalids_staleness_and_reset_cannot_extend_markers() {
    let c = config();
    let mut m = AcquisitionModel::demo_configured(&c);
    sample(&mut m, 3., 0);
    assert!(!m.ingest_batch([3., 90., 95., 80., 30., 700., 13.8, 1.], 0));
    m.tick(200);
    assert_eq!(peak(&m, 0, 200), None);
    sample(&mut m, 2., 210);
    m.reset_statistics(1);
    assert_eq!(peak(&m, 0, 210), None);
    sample(&mut m, 1., 220);
    m.tick(1000);
    assert_eq!(peak(&m, 1, 1000), None);
    sample(&mut m, 2., 1100);
    m.tick(1000);
    assert_eq!(peak(&m, 0, 1100), None);
}

/// An accepted but already stale delayed sample contributes no valid peak-mask bit.
#[test]
fn delayed_stale_sample_never_publishes_a_valid_peak_marker() {
    use dashboard_host_validation::samples::{Quantity, Sample, Source};
    let c = config();
    let mut m = AcquisitionModel::demo_configured(&c);
    m.ingest_sample(
        Sample {
            value: Quantity::BoostBar(3.),
            source: Source::Demo,
            sequence: 1,
            received_at_ms: 100,
        },
        1000,
    )
    .unwrap();
    assert_eq!(m.snapshot().peaks().valid_mask(), 0);
}
