//! Deferred vehicle construction preserves immutable configuration and exclusive port ownership without I/O.
use std::cell::Cell;
use std::rc::Rc;
use std::task::{Context, Poll, Waker};

use dashboard_host_validation::diagnostics::client::ClientError;
use dashboard_host_validation::diagnostics::service::{Fault, IoError, LinkState, Port, Profile, Vehicle};
use dashboard_host_validation::diagnostics::session::SessionError;
use dashboard_host_validation::samples::SensorId;

/// Shared ownership/I/O observations for one sequential constructor regression.
#[derive(Default)]
struct Counts {
    /// Number of owned test ports destroyed, expected exactly once per owner.
    drops: Cell<u32>,
    /// All port polls/status/discard calls, including calls that make no progress.
    io: Cell<u32>,
}
/// Nonprogressing owned port; any unexpected setup I/O remains observable.
struct OwnedPort(
    /// Shared counters survive each individual port's destruction.
    Rc<Counts>,
);
impl Drop for OwnedPort {
    /// Records destruction once; no real peripheral/resource is released by this test double.
    fn drop(&mut self) { self.0.drops.set(self.0.drops.get() + 1); }
}
impl Port for OwnedPort {
    /// Counts the read attempt and returns `Pending` without changing the
    /// unnamed destination or registering the unnamed context's waker; setup must never poll it.
    fn poll_read(
        &mut self,
        _: &mut Context<'_>,
        _: &mut [u8],
    ) -> Poll<Result<usize, IoError>> {
        self.0.io.set(self.0.io.get() + 1);
        Poll::Pending
    }

    /// Counts the write attempt and returns `Pending` without consuming the
    /// unnamed source or using the unnamed context; no completion wake is scheduled.
    fn poll_write(
        &mut self,
        _: &mut Context<'_>,
        _: &[u8],
    ) -> Poll<Result<usize, IoError>> {
        self.0.io.set(self.0.io.get() + 1);
        Poll::Pending
    }

    /// Counts the flush attempt and returns `Pending`, intentionally ignoring
    /// the unnamed task context because this is an unexpected-I/O witness.
    fn poll_flush(
        &mut self,
        _: &mut Context<'_>,
    ) -> Poll<Result<(), IoError>> {
        self.0.io.set(self.0.io.get() + 1);
        Poll::Pending
    }

    /// Counts the status read and returns false; no physical UART is represented.
    fn busy(&self) -> bool {
        self.0.io.set(self.0.io.get() + 1);
        false
    }

    /// Counts the error-status read and returns `None` for the fault-free fixture.
    fn rx_error(&self) -> Option<IoError> {
        self.0.io.set(self.0.io.get() + 1);
        None
    }

    /// Counts the discard request and returns `Ok(())`; the fixture stores no received bytes.
    fn discard_rx(&mut self) -> Result<(), IoError> {
        self.0.io.set(self.0.io.get() + 1);
        Ok(())
    }
}

/// One sequential construction trace checks deferred origin issuance, identical
/// bound state, no setup I/O, invalid-profile drops and exactly-once port destruction.
#[test]
fn prepared_construction_preserves_bound_configuration_time_and_port_ownership() {
    let mut document = dashboard_config::default_config();
    document.history_seconds[dashboard_config::Channel::Lambda.index()] = 10;
    let config = dashboard_config::validate(document).unwrap();
    let counts = Rc::new(Counts::default());
    let first = Vehicle::configured(OwnedPort(counts.clone()), Profile::UNQUALIFIED, 42, &config).unwrap();
    let first_origin = first.snapshot().warning_run_id().unwrap();
    let prepared = Vehicle::prepare_configured(OwnedPort(counts.clone()), Profile::UNQUALIFIED, 42, &config).unwrap();
    let unused = Vehicle::prepare_configured(OwnedPort(counts.clone()), Profile::UNQUALIFIED, 42, &config).unwrap();
    drop(unused);
    assert_eq!(counts.drops.get(), 1);
    assert_eq!(counts.io.get(), 0);
    let mut assembled = prepared.assemble();
    assert_eq!(assembled.snapshot().warning_run_id(), Some(first_origin + 1));
    assert_eq!(first.snapshot(), assembled.snapshot());
    assert_eq!(first.link(), assembled.link());
    assert_eq!(first.telemetry(), assembled.telemetry());
    assert!(core::ptr::eq(assembled.configuration().unwrap(), &config));
    assert_eq!(
        assembled
            .snapshot()
            .sensor(SensorId::Lambda)
            .history()
            .duration_seconds(),
        10
    );
    assert_eq!(assembled.link().state, LinkState::Recovering);
    assert_eq!(counts.io.get(), 0);

    let error = Vehicle::prepare_configured(
        OwnedPort(counts.clone()),
        Profile {
            responder: 0,
            ..Profile::UNQUALIFIED
        },
        42,
        &config,
    )
    .err()
    .unwrap();
    assert_eq!(error, SessionError::Client(ClientError::Address));
    assert_eq!(counts.drops.get(), 2);
    assert_eq!(counts.io.get(), 0);
    assembled.step(&mut Context::from_waker(Waker::noop()), 41);
    assert_eq!(assembled.link().last_fault, Some(Fault::Clock));
    drop(first);
    drop(assembled);
    assert_eq!(counts.drops.get(), 4);

    let legacy = Vehicle::new(OwnedPort(counts.clone()), Profile::UNQUALIFIED, 42).unwrap();
    assert_eq!(legacy.snapshot().warning_run_id(), Some(first_origin + 2));
    assert!(legacy.configuration().is_none());
    assert_eq!(legacy.link().state, LinkState::Recovering);
    assert_eq!(legacy.telemetry().at_ms, 0);
    drop(legacy);
    assert_eq!(counts.drops.get(), 5);
}
