#[path = "../../firmware/pico2/src/cache_reporting.rs"]
mod firmware_reporting;
use dashboard_host_validation::acquisition::{AcquisitionModel, DashboardSnapshot};
use dashboard_host_validation::cache_diagnostics::{
    CacheExhaustionCause as Cause,
    CacheExhaustionSeen,
    CacheExhaustionStatus as Status,
};
use dashboard_host_validation::view_config::Channel;

#[test]
fn reporting_lifetime_consumes_failed_sink_attempts_once_per_cause() {
    let mut seen = CacheExhaustionSeen::default();
    let mut calls = Vec::new();
    for _ in 0..3 {
        for cause in seen
            .observe(DashboardSnapshot::EMPTY.cache_exhaustion_status())
            .causes()
        {
            calls.push(cause);
        }
    }
    assert!(calls.is_empty());
    for cause in [
        Cause::ConfigIssuer,
        Cause::AcquisitionIssuer,
        Cause::GraphRevision(Channel::Egt),
        Cause::GraphRevision(Channel::Oil),
    ] {
        for _ in 0..3 {
            firmware_reporting::report(&mut seen, Status::from_cause(cause), |new| calls.push(new));
            let mut replacement = AcquisitionModel::demo();
            replacement.reset_statistics(1);
            assert_eq!(
                seen.observe(replacement.snapshot().cache_exhaustion_status())
                    .causes()
                    .count(),
                0
            );
        }
    }
    assert_eq!(calls.len(), 4);
    let mut restart = CacheExhaustionSeen::default();
    assert_eq!(
        restart
            .observe(Status::from_cause(Cause::ConfigIssuer))
            .causes()
            .count(),
        1
    );
}

#[test]
fn independent_origins_and_reset_do_not_reuse_graph_identity() {
    let mut first = AcquisitionModel::demo();
    let mut second = AcquisitionModel::demo();
    first.ingest_batch([1.0; 8], 0);
    second.ingest_batch([2.0; 8], 0);
    first.ingest_batch([3.0; 8], 2_000);
    second.ingest_batch([3.0; 8], 2_000);
    let a = first.snapshot();
    let b = second.snapshot();
    assert_ne!(a.sensors[0].graph_cache_identity(), b.sensors[0].graph_cache_identity());
    assert_ne!(a.sensors[0].graph_cache_identity(), a.sensors[1].graph_cache_identity());
    first.ingest_batch([3.0; 8], 2_000);
    assert_eq!(
        a.sensors[0].graph_cache_identity(),
        first.snapshot().sensors[0].graph_cache_identity()
    );
    first.reset_statistics(1);
    assert_ne!(
        a.sensors[0].graph_cache_identity(),
        first.snapshot().sensors[0].graph_cache_identity()
    );
    assert_eq!(DashboardSnapshot::EMPTY.sensors[0].graph_cache_identity(), None);
}

#[test]
fn all_cause_messages_fit_the_existing_firmware_log_and_have_ten_distinct_bits() {
    let causes = [Cause::ConfigIssuer, Cause::AcquisitionIssuer]
        .into_iter()
        .chain(Channel::ALL.map(Cause::GraphRevision));
    let mut status = Status::EMPTY;
    for cause in causes {
        assert!(cause.to_string().len() <= 40);
        status = status.union(Status::from_cause(cause));
    }
    assert_eq!(status.causes().count(), 10);
    assert_eq!(std::mem::size_of::<Status>(), 2);
    assert_eq!(std::mem::size_of::<CacheExhaustionSeen>(), 2);
}
