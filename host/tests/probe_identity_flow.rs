//! Real identity flow against synthetic I/O seams; no device or private capture input.

#[path = "../../firmware/ble-probe/src/errors.rs"]
mod errors;
#[path = "../../firmware/ble-probe/src/identity_flow.rs"]
mod identity_flow;

use core::future::{Future, pending, poll_fn};
use core::pin::pin;
use core::task::{Context, Poll, Waker};
use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::rc::Rc;

use dashboard_core::ble::canyonero::{
    CleanupResult,
    DecodeError,
    Epoch,
    Event as SessionEvent,
    InfoRequest,
    Outcome,
    Reason,
    Session,
    Stage,
    TransportDomain,
    TransportFault,
};
use dashboard_core::discovery::identity::{ChannelParameters, Evidence, Update};
use dashboard_core::discovery::{Characteristic, FailureReason, InspectionItem, ReadResult, Service, Uuid};
use identity_flow::{Clock, FreshEndpoints, Publication, Transport, VendorClient};

/// Synthetic epoch with no private peer identifier.
const EPOCH: Epoch = Epoch {
    connection: 1,
    operation: 1,
    maintenance: 0,
};

/// Deterministic monotonic milliseconds, advanced explicitly by each test.
#[derive(Default)]
struct Time(
    /// Test-selected boot-relative milliseconds.
    Cell<u64>,
);

impl Clock for Time {
    /// Returns the test-selected monotonic milliseconds.
    fn now_ms(&self) -> u64 { self.0.get() }

    /// Waits for test time to reach `deadline_ms`, with no real sleep.
    async fn until(
        &self,
        deadline_ms: u64,
    ) {
        poll_fn(|_| {
            if self.0.get() >= deadline_ms {
                Poll::Ready(())
            } else {
                Poll::Pending
            }
        })
        .await;
    }
}

/// Synthetic vendor read script; requested handles prove the real flow's scoped routing.
#[derive(Default)]
struct Vendor {
    /// Actual requested handles in call order.
    reads: RefCell<Vec<u16>>,
    /// First read error, when configured.
    error: Option<FailureReason>,
    /// First read remains pending when true.
    blocked: bool,
    /// First successful value fills its64-byte buffer when true.
    full: bool,
    /// Override first copied count, including zero or an invalid oversize result.
    first_len: Option<usize>,
}

impl VendorClient for Vendor {
    /// Fills `bytes` for requested fresh `handle`, returning scripted copied count/error.
    async fn read(
        &self,
        handle: u16,
        bytes: &mut [u8],
    ) -> Result<usize, FailureReason> {
        let first = self.reads.borrow().is_empty();
        self.reads.borrow_mut().push(handle);
        // Real reads may modify the destination before cancellation or an error.
        bytes.fill(0xA5);
        if first && self.blocked {
            pending::<()>().await;
        }
        if first && let Some(error) = self.error {
            return Err(error);
        }
        if first && self.full {
            return Ok(bytes.len());
        }
        if first && let Some(len) = self.first_len {
            return Ok(len);
        }
        bytes[0] = 0x42;
        Ok(1)
    }
}

/// Complete synthetic SDU whose drop proves the pool owner can be released before publication.
struct OwnedSdu {
    /// Complete application bytes supplied by this synthetic packet.
    bytes: Vec<u8>,
    /// Count incremented on owner release.
    drops: Rc<Cell<usize>>,
}

impl AsRef<[u8]> for OwnedSdu {
    /// Returns the complete owned synthetic packet bytes without a second copy.
    fn as_ref(&self) -> &[u8] { &self.bytes }
}

impl Drop for OwnedSdu {
    /// Records release of this packet owner.
    fn drop(&mut self) { self.drops.set(self.drops.get() + 1); }
}

/// Scripted channel seam; all sequencing and admission is real production flow code.
#[derive(Default)]
struct Link {
    /// Number of candidate-channel open calls.
    opens: usize,
    /// Exact outbound application SDUs.
    sent: Vec<Vec<u8>>,
    /// Complete inbound SDUs, never private captures.
    received: VecDeque<Vec<u8>>,
    /// Exact peer channel refusal when configured.
    open_error: Option<TransportFault>,
    /// Blocks one phase:1=open,2=send,3=receive;0 means all scripted ready.
    blocked: u8,
    /// Count of released receive owners.
    drops: Rc<Cell<usize>>,
}

impl Transport for Link {
    /// Owned complete fixture packet, tracking release timing.
    type Sdu = OwnedSdu;

    /// Opens once and returns synthetic actual limits or the scripted refusal.
    async fn open(&mut self) -> Result<ChannelParameters, TransportFault> {
        self.opens += 1;
        if self.blocked == 1 {
            pending::<()>().await;
        }
        if let Some(error) = self.open_error {
            return Err(error);
        }
        Ok(ChannelParameters {
            local_mtu: Some(245),
            local_mps: Some(247),
            peer_mtu: Some(245),
            peer_mps: Some(23),
            ..identity_flow::offered_parameters()
        })
    }

    /// Records actual INFO `request` bytes once, then optionally blocks; returns successful submission.
    async fn send(
        &mut self,
        request: InfoRequest,
    ) -> Result<(), TransportFault> {
        self.sent.push(request.bytes().to_vec());
        if self.blocked == 2 {
            pending::<()>().await;
        }
        Ok(())
    }

    /// Returns the next complete packet or remains pending without inventing EOF.
    async fn receive(&mut self) -> Result<Self::Sdu, TransportFault> {
        if self.blocked == 3 || self.received.is_empty() {
            pending::<()>().await;
        }
        Ok(OwnedSdu {
            bytes: self.received.pop_front().unwrap(),
            drops: self.drops.clone(),
        })
    }
}

/// Returns a session after a fully completed synthetic standard inventory at0ms.
fn admitted() -> Session {
    let mut session = Session::new();
    session.start(EPOCH, 0).unwrap();
    session.event(EPOCH, 0, SessionEvent::Connected).unwrap();
    session.event(EPOCH, 0, SessionEvent::Discovered).unwrap();
    session
}

/// Polls `future` once, returning its ready value; panics if the script unexpectedly waits.
fn ready<T>(future: impl Future<Output = T>) -> T {
    let mut future = pin!(future);
    match future.as_mut().poll(&mut Context::from_waker(Waker::noop())) {
        Poll::Ready(value) => value,
        Poll::Pending => panic!("synthetic immediate flow unexpectedly pending"),
    }
}

/// Initial queue backpressure is bounded and canceled starts resume exactly once before cleanup.
#[test]
fn initial_publication_obeys_connect_deadline_and_cancellation() {
    for scenario in 0..3 {
        let clock = Time(Cell::new(17));
        let mut session = Session::new();
        session.start(EPOCH, clock.now_ms()).unwrap();
        let mut publication = Publication::new(clock.now_ms());
        let updates = RefCell::new(Vec::new());
        let connected = Cell::new(false);
        let emit = async |update| {
            if scenario != 2 {
                pending::<()>().await;
            }
            updates.borrow_mut().push(update);
        };
        let deadline = session.deadline_ms().unwrap();
        {
            let mut work = pin!(identity_flow::connect(
                &clock,
                deadline,
                &mut publication,
                &emit,
                async {
                    connected.set(true);
                    pending::<()>().await;
                }
            ));
            let mut cx = Context::from_waker(Waker::noop());
            assert!(work.as_mut().poll(&mut cx).is_pending());
            if scenario == 0 {
                clock.0.set(deadline);
                assert_eq!(work.as_mut().poll(&mut cx), Poll::Ready(Err(FailureReason::Timeout)));
            }
        }
        assert_eq!(connected.get(), scenario == 2);
        if scenario == 0 {
            identity_flow::fail(&mut session, EPOCH, &clock, FailureReason::Timeout);
        } else {
            clock.0.set(18);
            session.event(EPOCH, clock.now_ms(), SessionEvent::Cancel).unwrap();
        }
        assert_eq!(session.stage(), Stage::Closing);
        assert_eq!(
            session.fault().unwrap().reason,
            if scenario == 0 {
                Reason::TimedOut
            } else {
                Reason::Cancelled
            }
        );
        let flush = async |update| {
            updates.borrow_mut().push(update);
        };
        ready(identity_flow::publish_progress(
            &mut publication,
            &session,
            &clock,
            &flush,
        ));
        ready(identity_flow::publish_progress(
            &mut publication,
            &session,
            &clock,
            &flush,
        ));
        let updates = updates.into_inner();
        assert!(matches!(updates.first(), Some(Update::Start { at_ms: 17 })));
        assert_eq!(updates.iter().filter(|u| matches!(u, Update::Start { .. })).count(), 1);
        let mut inspection = dashboard_core::discovery::Inspection::new();
        inspection.started_at_ms = Some(17);
        for update in updates {
            inspection.accept(InspectionItem::Identity(update));
        }
        assert_eq!(inspection.malformed, 0);
        let evidence = inspection.identity.unwrap();
        assert_eq!(evidence.started_at_ms, 17);
        assert_eq!(evidence.stage, Stage::Closing);
    }
}

/// Returns the literal synthetic five-field INFO frame, never a device capture.
fn info() -> Vec<u8> { b"\x1f\x91\0\x09A\nB\nC\nD\nE".to_vec() }

/// Complete real sequencing issues exactly four scoped reads, one open and one INFO SDU.
#[test]
fn real_flow_preserves_raw_identity_and_waits_for_cleanup() {
    let mut session = admitted();
    let clock = Time::default();
    let vendor = Vendor::default();
    let mut link = Link::default();
    link.received.push_back(info());
    let mut publication = Publication::default();
    let updates = RefCell::new(Vec::new());
    let emit = async |update| {
        updates.borrow_mut().push(update);
    };
    assert_eq!(
        ready(identity_flow::run_connected(
            &mut session,
            &clock,
            &vendor,
            &mut link,
            (10, [12, 14, 16, 18]),
            &mut publication,
            &emit
        )),
        Ok(false)
    );
    assert_eq!(*vendor.reads.borrow(), [12, 14, 16, 18]);
    assert_eq!(link.opens, 1);
    assert_eq!(link.sent, [vec![0x1F, 0x11, 0, 0]]);
    assert_eq!(link.drops.get(), 1);
    assert_eq!(session.stage(), Stage::Closing);
    assert_eq!(session.outcome(), None);
    let mut evidence = Evidence::new(0);
    for update in updates.into_inner() {
        assert!(evidence.apply(update));
    }
    assert_eq!(evidence.raw(), info());
    assert!(!evidence.successful());
    session
        .event(EPOCH, 1, SessionEvent::CleanupComplete(CleanupResult::Clean))
        .unwrap();
    assert_eq!(session.outcome(), Some(Outcome::Success));
    assert!(evidence.apply(Update::End {
        at_ms: 1,
        cleanup: session.cleanup()
    }));
    assert!(evidence.successful());
}

/// Protected/unavailable/full vendor evidence is retained and keeps successful INFO limited.
#[test]
fn vendor_limitations_do_not_trigger_security_or_other_commands() {
    for (error, full, expected) in [
        (Some(FailureReason::Att(5)), false, ReadResult::Protected),
        (Some(FailureReason::Att(10)), false, ReadResult::Unavailable),
        (None, true, ReadResult::Ok),
    ] {
        let mut session = admitted();
        let clock = Time::default();
        let vendor = Vendor {
            error,
            full,
            ..Default::default()
        };
        let mut link = Link::default();
        link.received.push_back(info());
        let updates = RefCell::new(Vec::new());
        assert_eq!(
            ready(identity_flow::run_connected(
                &mut session,
                &clock,
                &vendor,
                &mut link,
                (10, [12, 14, 16, 18]),
                &mut Publication::default(),
                &async |update| {
                    updates.borrow_mut().push(update);
                }
            )),
            Ok(true)
        );
        let values: Vec<_> = updates
            .into_inner()
            .into_iter()
            .filter_map(|u| if let Update::Vendor(v) = u { Some(v) } else { None })
            .collect();
        assert_eq!(values.len(), 4);
        assert_eq!(values[0].result, expected);
        assert_eq!(values[0].failure, error);
        assert_eq!(values[0].value.truncated, full);
        assert_eq!(link.sent, [vec![0x1F, 0x11, 0, 0]]);
    }
}

/// Local vendor failures abort before channel open and preserve the failed read itself.
#[test]
fn failed_vendor_read_prevents_channel_open() {
    for error in [
        FailureReason::Disconnected,
        FailureReason::InvalidValue,
        FailureReason::Capacity,
    ] {
        let mut session = admitted();
        let clock = Time::default();
        let vendor = Vendor {
            error: Some(error),
            ..Default::default()
        };
        let mut link = Link::default();
        let updates = RefCell::new(Vec::new());
        assert_eq!(
            ready(identity_flow::run_connected(
                &mut session,
                &clock,
                &vendor,
                &mut link,
                (10, [12, 14, 16, 18]),
                &mut Publication::default(),
                &async |update| {
                    updates.borrow_mut().push(update);
                }
            )),
            Err(error)
        );
        assert_eq!(link.opens, 0);
        assert!(link.sent.is_empty());
        assert!(session.fault().is_some());
        assert!(
            updates
                .borrow()
                .iter()
                .any(|u| matches!(u, Update::Vendor(v) if v.failure==Some(error)))
        );
    }
}

/// Only successfully copied prefixes may escape; failed or oversized reads discard partial bytes.
#[test]
fn vendor_publication_bounds_and_clears_unacknowledged_bytes() {
    for (first_len, error, expected_len, expected_result) in [
        (Some(0), None, 0, ReadResult::Ok),
        (Some(63), None, 63, ReadResult::Ok),
        (Some(64), None, 64, ReadResult::Ok),
        (Some(65), None, 0, ReadResult::Error),
        (None, Some(FailureReason::Att(5)), 0, ReadResult::Protected),
        (None, Some(FailureReason::Disconnected), 0, ReadResult::Error),
    ] {
        let mut session = admitted();
        let clock = Time::default();
        let vendor = Vendor {
            first_len,
            error,
            ..Default::default()
        };
        let mut link = Link::default();
        link.received.push_back(info());
        let updates = RefCell::new(Vec::new());
        let result = ready(identity_flow::run_connected(
            &mut session,
            &clock,
            &vendor,
            &mut link,
            (10, [12, 14, 16, 18]),
            &mut Publication::default(),
            &async |update| {
                updates.borrow_mut().push(update);
            },
        ));
        let values: Vec<_> = updates
            .into_inner()
            .into_iter()
            .filter_map(|u| if let Update::Vendor(v) = u { Some(v) } else { None })
            .collect();
        let first = &values[0];
        assert_eq!(first.result, expected_result);
        assert_eq!(usize::from(first.value.len), expected_len);
        assert!(first.value.bytes[..expected_len].iter().all(|&b| b == 0xA5));
        assert!(first.value.bytes[expected_len..].iter().all(|&b| b == 0));
        assert_eq!(first.value.truncated, expected_len == 64);
        if expected_result == ReadResult::Error {
            assert!(result.is_err());
            assert_eq!(values.len(), 1);
            assert_eq!(link.opens, 0);
        } else {
            assert!(result.is_ok());
            assert_eq!(values.len(), 4);
            assert_eq!(values[1].value.as_slice(), [0x42]);
            assert!(values[1].value.bytes[1..].iter().all(|&b| b == 0));
        }
    }
}

/// A canceled partial read is not a completed observation; a timed-out read is an empty error.
#[test]
fn partial_vendor_read_cancellation_and_timeout_never_publish_payload() {
    for timeout in [false, true] {
        let mut session = admitted();
        let clock = Time::default();
        let vendor = Vendor {
            blocked: true,
            ..Default::default()
        };
        let mut link = Link::default();
        let mut publication = Publication::default();
        let updates = RefCell::new(Vec::new());
        let emit = async |update| {
            updates.borrow_mut().push(update);
        };
        {
            let mut work = pin!(identity_flow::run_connected(
                &mut session,
                &clock,
                &vendor,
                &mut link,
                (10, [12, 14, 16, 18]),
                &mut publication,
                &emit,
            ));
            let mut cx = Context::from_waker(Waker::noop());
            assert!(work.as_mut().poll(&mut cx).is_pending());
            if timeout {
                clock.0.set(3_000);
                assert_eq!(work.as_mut().poll(&mut cx), Poll::Ready(Err(FailureReason::Timeout)));
            }
        }
        ready(identity_flow::publish_pending(&mut publication, &session, &emit));
        ready(identity_flow::publish_pending(&mut publication, &session, &emit));
        let values: Vec<_> = updates
            .into_inner()
            .into_iter()
            .filter_map(|u| if let Update::Vendor(v) = u { Some(v) } else { None })
            .collect();
        assert_eq!(values.len(), usize::from(timeout));
        if let Some(value) = values.first() {
            assert_eq!(value.result, ReadResult::Error);
            assert_eq!(value.failure, Some(FailureReason::Timeout));
            assert_eq!(value.value, dashboard_core::discovery::Payload::empty());
            assert_eq!(value.elapsed_ms, 3_000);
        }
        assert_eq!(link.opens, 0);
        assert!(link.sent.is_empty());
    }
}

/// Peer L2CAP refusal must retain its16-bit namespace/code and never send application bytes.
#[test]
fn channel_refusal_preserves_exact_code() {
    let mut session = admitted();
    let clock = Time::default();
    let refusal = TransportFault {
        domain: TransportDomain::L2cap,
        code: Some(0x0005),
        partial_send: false,
    };
    let mut link = Link {
        open_error: Some(refusal),
        ..Default::default()
    };
    assert_eq!(
        ready(identity_flow::run_connected(
            &mut session,
            &clock,
            &Vendor::default(),
            &mut link,
            (10, [12, 14, 16, 18]),
            &mut Publication::default(),
            &async |_| {}
        )),
        Err(FailureReason::Other)
    );
    assert_eq!(session.fault().unwrap().reason, Reason::Transport(refusal));
    assert!(link.sent.is_empty());
}

/// Every split across complete SDUs preserves the raw stream, while malformed replies remain failures.
#[test]
fn split_sdus_and_malformed_frames_use_production_decoder() {
    for split in 1..info().len() {
        let mut session = admitted();
        let clock = Time::default();
        let mut link = Link::default();
        link.received.push_back(info()[..split].to_vec());
        link.received.push_back(info()[split..].to_vec());
        assert_eq!(
            ready(identity_flow::run_connected(
                &mut session,
                &clock,
                &Vendor::default(),
                &mut link,
                (10, [12, 14, 16, 18]),
                &mut Publication::default(),
                &async |_| {}
            )),
            Ok(false)
        );
        assert_eq!(session.decoder().raw(), info());
        assert_eq!(link.drops.get(), 2);
    }
    for (bytes, reason) in [
        (vec![], Reason::UnexpectedEmptySdu),
        (b"\x1f\xe5\0\0".to_vec(), Reason::Protocol(DecodeError::Remote(0xE5))),
        ([info(), info()].concat(), Reason::Protocol(DecodeError::TrailingData)),
    ] {
        let mut session = admitted();
        let clock = Time::default();
        let mut link = Link::default();
        link.received.push_back(bytes);
        assert_eq!(
            ready(identity_flow::run_connected(
                &mut session,
                &clock,
                &Vendor::default(),
                &mut link,
                (10, [12, 14, 16, 18]),
                &mut Publication::default(),
                &async |_| {}
            )),
            Err(FailureReason::InvalidValue)
        );
        assert_eq!(session.fault().unwrap().reason, reason);
        assert_eq!(link.drops.get(), 1);
    }
}

/// A canceled publication resumes from retained offsets after releasing the received packet.
#[test]
fn cancelled_chunk_publication_can_flush_exact_evidence_during_cleanup() {
    let mut session = admitted();
    let clock = Time::default();
    let vendor = Vendor::default();
    let mut link = Link::default();
    link.received.push_back(info());
    let dropped = link.drops.clone();
    let mut publication = Publication::default();
    let updates = RefCell::new(Vec::new());
    let emit = async |update| {
        if matches!(update, Update::Chunk { .. }) {
            assert_eq!(dropped.get(), 1);
            pending::<()>().await;
        }
        updates.borrow_mut().push(update);
    };
    {
        let mut work = pin!(identity_flow::run_connected(
            &mut session,
            &clock,
            &vendor,
            &mut link,
            (10, [12, 14, 16, 18]),
            &mut publication,
            &emit
        ));
        assert!(work.as_mut().poll(&mut Context::from_waker(Waker::noop())).is_pending());
    }
    session.event(EPOCH, 1, SessionEvent::Cancel).unwrap();
    ready(identity_flow::publish_pending(
        &mut publication,
        &session,
        &async |update| {
            updates.borrow_mut().push(update);
        },
    ));
    let mut evidence = Evidence::new(0);
    for update in updates.into_inner() {
        assert!(evidence.apply(update));
    }
    assert_eq!(evidence.raw(), info());
    assert_eq!(evidence.sdu_count, 1);
    assert!(!evidence.raw_incomplete());
    session
        .event(EPOCH, 2, SessionEvent::CleanupComplete(CleanupResult::Clean))
        .unwrap();
    assert!(matches!(session.outcome(), Some(Outcome::Failed(_))));
}

/// Per-read and whole-stage deadlines drop pending I/O and prohibit further sends.
#[test]
fn blocked_io_times_out_at_its_fixed_deadline() {
    for blocked in 0..=3 {
        let mut session = admitted();
        let clock = Time::default();
        let vendor = Vendor {
            blocked: blocked == 0,
            ..Default::default()
        };
        let mut link = Link {
            blocked,
            ..Default::default()
        };
        let mut publication = Publication::default();
        let mut work = pin!(identity_flow::run_connected(
            &mut session,
            &clock,
            &vendor,
            &mut link,
            (10, [12, 14, 16, 18]),
            &mut publication,
            &async |_| {}
        ));
        let mut cx = Context::from_waker(Waker::noop());
        assert!(work.as_mut().poll(&mut cx).is_pending());
        clock.0.set(match blocked {
            0 => 3_000,
            1 => 5_000,
            _ => 2_000,
        });
        assert_eq!(work.as_mut().poll(&mut cx), Poll::Ready(Err(FailureReason::Timeout)));
    }
}

/// Pinned, non-Copy operation used to observe address stability and exclusive owner destruction.
struct BorrowedWork<'a> {
    /// Readiness is changed by the test between polls.
    ready: &'a Cell<bool>,
    /// First polled address; subsequent polls must keep the same owner location.
    address: &'a Cell<usize>,
    /// Destruction count, independent of timer or borrowed wrapper destruction.
    drops: &'a Cell<usize>,
    /// Prevent movement after pinning, matching address-sensitive async owners.
    _pinned: core::marker::PhantomPinned,
}

impl Future for BorrowedWork<'_> {
    /// Completion carries one test value; no resource ownership is returned.
    type Output = u8;

    /// Polls `self` at its fixed address; `_cx` is unused. Returns Pending until
    /// externally ready, then Ready(42); asserts that prior polls used this address.
    fn poll(
        self: core::pin::Pin<&mut Self>,
        _cx: &mut Context<'_>,
    ) -> Poll<u8> {
        let address = core::ptr::from_ref(self.as_ref().get_ref()) as usize;
        if self.address.get() == 0 {
            self.address.set(address);
        }
        assert_eq!(self.address.get(), address);
        if self.ready.get() {
            Poll::Ready(42)
        } else {
            Poll::Pending
        }
    }
}

impl Drop for BorrowedWork<'_> {
    /// Records the owning scope's release; dropping a pinned borrow must not invoke this.
    fn drop(&mut self) { self.drops.set(self.drops.get() + 1); }
}

/// Success, timeout and outer cancellation all release the pinned owner before subsequent work.
#[test]
fn borrowed_deadline_work_stays_pinned_until_its_owner_scope_ends() {
    for scenario in 0..3 {
        let clock = Time::default();
        let complete = Cell::new(false);
        let address = Cell::new(0);
        let drops = Cell::new(0);
        {
            let mut operation = pin!(BorrowedWork {
                ready: &complete,
                address: &address,
                drops: &drops,
                _pinned: core::marker::PhantomPinned,
            });
            {
                let mut timed = pin!(identity_flow::before(&clock, 10, operation.as_mut()));
                let mut cx = Context::from_waker(Waker::noop());
                assert!(timed.as_mut().poll(&mut cx).is_pending());
                assert!(timed.as_mut().poll(&mut cx).is_pending());
                if scenario < 2 {
                    complete.set(true);
                    if scenario == 1 {
                        clock.0.set(10);
                    }
                    assert_eq!(
                        timed.as_mut().poll(&mut cx),
                        Poll::Ready(if scenario == 0 {
                            Ok(42)
                        } else {
                            Err(FailureReason::Timeout)
                        })
                    );
                }
            }
            assert_eq!(drops.get(), 0, "the timer owns a borrow, not the operation");
        }
        assert_eq!(drops.get(), 1, "owner ended before cleanup/next operation");
    }
}

/// Duplicate services, nonreadable declarations, missing values or handle aliasing never qualify.
#[test]
fn endpoint_ambiguity_is_rejected_before_proprietary_work() {
    for scenario in 0..4 {
        let mut endpoints = FreshEndpoints::default();
        let service = Service {
            start: 10,
            end: 30,
            uuid: uuid(0xFFF1),
        };
        endpoints.observe(&InspectionItem::Service(service));
        if scenario == 0 {
            endpoints.observe(&InspectionItem::Service(Service {
                start: 40,
                end: 60,
                ..service
            }));
        }
        for (index, value) in [0x0081, 0x0082, 0xFFF2, 0xFFF3].into_iter().enumerate() {
            if scenario == 2 && index == 3 {
                continue;
            }
            endpoints.observe(&InspectionItem::Characteristic(Characteristic {
                service_start: 10,
                declaration: 11 + index as u16 * 2,
                handle: if scenario == 3 { 12 } else { 12 + index as u16 * 2 },
                properties: if scenario == 1 { 0 } else { 2 },
                uuid: uuid(value),
            }));
        }
        assert!(endpoints.finish().is_err());
    }
}

/// The existing error adapter continues to preserve exact ATT classification in this shared test context.
#[test]
fn read_error_mapping_preserves_att_namespace() {
    assert_eq!(
        errors::failure_reason::<()>(trouble_host::BleHostError::BleHost(trouble_host::Error::Att(
            trouble_host::att::AttErrorCode::new(5)
        ))),
        FailureReason::Att(5)
    );
}

/// Completed reads, channel opens and INFO writes survive cancellation at their publication await.
#[test]
fn cancelled_irreversible_observations_are_flushed_once_during_cleanup() {
    for phase in 0..3 {
        let mut session = admitted();
        let clock = Time::default();
        clock.0.set(1);
        let vendor = Vendor::default();
        let mut link = Link::default();
        link.received.push_back(info());
        let mut publication = Publication::default();
        let updates = RefCell::new(Vec::new());
        let emit = async |update| {
            let block = match update {
                Update::Vendor(_) => phase == 0,
                Update::Channel(parameters) => phase == 1 && parameters.local_mtu.is_some(),
                Update::RequestSubmitted { .. } => phase == 2,
                _ => false,
            };
            if block {
                pending::<()>().await;
            }
            updates.borrow_mut().push(update);
        };
        {
            let mut work = pin!(identity_flow::run_connected(
                &mut session,
                &clock,
                &vendor,
                &mut link,
                (10, [12, 14, 16, 18]),
                &mut publication,
                &emit
            ));
            assert!(work.as_mut().poll(&mut Context::from_waker(Waker::noop())).is_pending());
        }
        clock.0.set(2);
        session.event(EPOCH, 2, SessionEvent::Cancel).unwrap();
        ready(identity_flow::publish_progress(
            &mut publication,
            &session,
            &clock,
            &async |update| {
                updates.borrow_mut().push(update);
            },
        ));
        ready(identity_flow::publish_progress(
            &mut publication,
            &session,
            &clock,
            &async |update| {
                updates.borrow_mut().push(update);
            },
        ));
        let updates = updates.into_inner();
        let mut evidence = Evidence::new(0);
        for &update in &updates {
            assert!(
                evidence.apply(update),
                "out-of-order retained update in phase {phase}: {update:?}"
            );
        }
        let expected_count = match phase {
            0 => {
                updates
                    .iter()
                    .filter(|u| matches!(u, Update::Vendor(v) if v.handle == 12 && v.result == ReadResult::Ok))
                    .count()
            }
            1 => {
                updates
                    .iter()
                    .filter(|u| matches!(u, Update::Channel(p) if p.local_mtu == Some(245)))
                    .count()
            }
            _ => {
                updates
                    .iter()
                    .filter(|u| matches!(u, Update::RequestSubmitted { .. }))
                    .count()
            }
        };
        assert_eq!(expected_count, 1, "completed operation evidence lost in phase {phase}");
        if phase == 2 {
            assert!(
                updates
                    .iter()
                    .any(|u| matches!(u, Update::RequestSubmitted { at_ms: 1 }))
            );
        }
    }
}

/// Returns a synthetic short UUID from `value` in Bluetooth little-endian order.
fn uuid(value: u16) -> Uuid { Uuid::from_wire(&value.to_le_bytes()).unwrap() }

/// A completed operation crossing the deadline retains its observation but cannot advance authority.
#[test]
fn completion_at_deadline_retains_value_without_advancing_session() {
    let mut session = admitted();
    let clock = Time::default();
    let deadline = session.deadline_ms().unwrap();
    assert_eq!(
        ready(identity_flow::before(&clock, deadline, async {
            clock.0.set(deadline);
            42
        })),
        Ok(42)
    );
    assert!(
        session
            .event(EPOCH, clock.now_ms(), SessionEvent::ParametersReady)
            .is_err()
    );
    assert_eq!(session.stage(), Stage::Closing);
    assert_eq!(session.fault().unwrap().reason, Reason::TimedOut);
}

/// Fresh endpoint collection must retain only the scoped service and ordered value handles.
#[test]
fn fresh_inventory_qualifies_four_unique_readable_vendor_handles() {
    let mut endpoints = FreshEndpoints::default();
    endpoints.observe(&InspectionItem::Service(Service {
        start: 10,
        end: 30,
        uuid: uuid(0xFFF1),
    }));
    for (index, value) in [0x0081, 0x0082, 0xFFF2, 0xFFF3].into_iter().enumerate() {
        endpoints.observe(&InspectionItem::Characteristic(Characteristic {
            service_start: 10,
            declaration: 11 + index as u16 * 2,
            handle: 12 + index as u16 * 2,
            properties: 2,
            uuid: uuid(value),
        }));
    }
    assert_eq!(endpoints.finish(), Ok((10, [12, 14, 16, 18])));
}
