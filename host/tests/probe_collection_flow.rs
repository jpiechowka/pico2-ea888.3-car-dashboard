//! Real collector lifecycle with synthetic transport; never opens a device or port.
use core::future::{Future, pending, poll_fn};
use core::pin::pin;
use core::task::{Context, Poll, Waker};
use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::rc::Rc;

use dashboard_core::ble::canyonero::{CleanupOutcome, Epoch, Event, Session, Stage, collection as protocol};
use dashboard_core::discovery::collection::Update;
#[allow(dead_code, unexpected_cfgs)]
#[path = "../../firmware/ble-probe/src/bridge.rs"]
/// Actual cancellation intent and retained request generation shared with firmware.
mod bridge;
#[path = "../../firmware/ble-probe/src/collection_flow.rs"]
/// Production sequencing exercised below the hardware boundary.
mod collection_flow;
#[allow(dead_code)]
#[path = "../../firmware/ble-probe/src/errors.rs"]
/// Shared identity flow's error adapter; unrelated helpers are unused in this harness.
mod errors;
#[allow(dead_code)]
#[path = "../../firmware/ble-probe/src/identity_flow.rs"]
/// Actual deadline/clock seam; identity-specific helpers have their own flow tests.
mod identity_flow;
#[allow(dead_code)]
#[path = "../../firmware/ble-probe/src/status.rs"]
/// Bridge status dependency; no physical LED or hardware task runs in this harness.
mod status;
use collection_flow::Transport;
use identity_flow::Clock;

/// One synthetic operation identity, independent of physical peers.
const EPOCH: Epoch = Epoch {
    connection: 1,
    operation: 1,
    maintenance: 0,
};

/// Advances `session` through valid INFO at `now_ms`, returning its cleanup deadline.
fn identified(
    session: &mut Session,
    now_ms: u64,
) -> u64 {
    session.start(EPOCH, now_ms).unwrap();
    for event in [
        Event::Connected,
        Event::Discovered,
        Event::ParametersReady,
        Event::ChannelOpened,
    ] {
        session.event(EPOCH, now_ms, event).unwrap();
    }
    assert_eq!(
        session.take_request(EPOCH, now_ms).unwrap().bytes(),
        &[0x1F, 0x11, 0, 0]
    );
    session.event(EPOCH, now_ms, Event::InfoSent).unwrap();
    session
        .receive(EPOCH, now_ms, b"\x1f\x91\x00\x09A\nB\nC\nD\nE")
        .unwrap();
    assert_eq!(session.stage(), Stage::Closing);
    session.deadline_ms().unwrap()
}

/// Only explicit collector policy reserves a longer post-INFO window; cleanup cannot extend it.
#[test]
fn collector_window_is_explicit_and_real_cleanup_only_shortens_it() {
    assert_eq!(identified(&mut Session::new(), 100), 5_100);
    let mut session = Session::new_for_collection();
    assert_eq!(identified(&mut session, 100), 90_100);
    assert!(session.take_request(EPOCH, 101).is_err());
    session.begin_link_cleanup(EPOCH, 80_100).unwrap();
    assert_eq!(session.deadline_ms(), Some(85_100));
    session.begin_link_cleanup(EPOCH, 81_100).unwrap();
    assert_eq!(session.deadline_ms(), Some(85_100));
    assert!(session.outcome().is_none());
    let mut session = Session::new_for_collection();
    identified(&mut session, 100);
    session.begin_link_cleanup(EPOCH, 89_100).unwrap();
    assert_eq!(session.deadline_ms(), Some(90_100));
}

/// Collector policy does not extend failures before valid INFO or admit stale cleanup owners.
#[test]
fn collector_failed_identity_retains_short_cleanup_budget() {
    let mut session = Session::new_for_collection();
    session.start(EPOCH, 0).unwrap();
    session.event(EPOCH, 100, Event::Cancel).unwrap();
    assert_eq!(session.deadline_ms(), Some(5_100));
    assert!(
        session
            .begin_link_cleanup(Epoch { operation: 2, ..EPOCH }, 200)
            .is_err()
    );
    assert_eq!(session.deadline_ms(), Some(5_100));
    session.begin_link_cleanup(EPOCH, 200).unwrap();
    assert_eq!(session.deadline_ms(), Some(5_100));
}

/// Explicit monotonic test clock; no wall-clock wait or hardware timer is used.
#[derive(Default)]
struct Time(
    /// Explicit boot-relative milliseconds selected between polls.
    Cell<u64>,
);
impl Clock for Time {
    /// Returns the selected boot-relative milliseconds.
    fn now_ms(&self) -> u64 { self.0.get() }

    /// Waits for selected time to reach `deadline_ms`, without wall-clock sleeping.
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
        .await
    }
}
/// Synthetic owned receive packet; its destructor records release before publication.
struct Packet {
    /// Complete synthetic adapter envelope, owned until release.
    bytes: Vec<u8>,
    /// Shared number of packet-owner releases.
    drops: Rc<Cell<usize>>,
}
impl AsRef<[u8]> for Packet {
    /// Returns the immutable complete packet bytes, borrowed until this packet owner is released.
    fn as_ref(&self) -> &[u8] { &self.bytes }
}
impl Drop for Packet {
    /// Records exactly one owner release.
    fn drop(&mut self) { self.drops.set(self.drops.get() + 1) }
}
/// Bounded scripted adapter below the production flow; records every attempted command.
#[derive(Default)]
struct Link {
    /// Ordered exact SDUs handed to the synthetic writer.
    sent: Vec<Vec<u8>>,
    /// Scripted complete or intentionally partial response fragments.
    replies: VecDeque<Vec<u8>>,
    /// Shared packet-owner release count.
    drops: Rc<Cell<usize>>,
    /// Suspend every writer after recording its bytes.
    blocked: bool,
    /// Optional one-based command index to suspend after recording bytes.
    block_at: Option<usize>,
    /// Simulates a changed bridge generation at the final write boundary.
    stale: bool,
    /// Returns only the first three bytes, then waits indefinitely.
    partial: bool,
    /// Makes the second row span FF/CF with one required host flow-control frame.
    segmented: bool,
    /// Corrupts that first frame's DID before any flow-control authorization.
    foreign_first_frame: bool,
    /// Returns an unsupported-DID negative for the second row without changing session.
    negative_row: bool,
    /// One supplies a duplicate first SF; two supplies an early CF before CTS.
    queued_extra: u8,
    /// Zero is default01; one denies F186, two reports nondefault03.
    session_reply: u8,
}
impl Transport for Link {
    /// Owned packet exposing release order to assertions.
    type Sdu = Packet;

    /// Dequeues only a complete queued fixture, without polling or creating more input.
    /// Returns Some owned packet when queued, otherwise None; this fixture has no dequeue error.
    fn try_receive(&mut self) -> Result<Option<Self::Sdu>, protocol::Error> {
        Ok(self.replies.pop_front().map(|bytes| {
            Packet {
                bytes,
                drops: self.drops.clone(),
            }
        }))
    }

    /// Validates and records `request` against `session` at `clock` time, then scripts its
    /// exact correlated reply. Optional suspension retains unknown effects without completion.
    /// Returns unit after recording local completion, or the current epoch/state/timing rejection;
    /// an unexpected flow-control permit panics instead of silently widening the script.
    async fn send(
        &mut self,
        request: protocol::Request,
        session: &mut protocol::Session,
        clock: &impl Clock,
    ) -> Result<(), protocol::Error> {
        if self.stale {
            return Err(protocol::Error::Stale);
        }
        session.validate(request.token(), clock.now_ms())?;
        session.mark_attempt(request.token(), clock.now_ms())?;
        self.sent.push(request.bytes().to_vec());
        if self.blocked || self.block_at == Some(self.sent.len()) {
            pending::<()>().await;
        }
        let reply = match session.submission().unwrap().command {
            protocol::Command::Voltage => vec![0x1F, 0x92, 0, 2, 0x30, 0x39],
            protocol::Command::Open => vec![0x1F, 0xB0, 0, 1, 7],
            protocol::Command::Arbitration => vec![0x1F, 0x80, 0, 0],
            protocol::Command::Read(id) => {
                let [_, hi, lo] = id.request();
                if self.session_reply == 1 && id == dashboard_core::ble::canyonero::reads::ReadId::ActiveSession {
                    vec![0x1F, 0xB2, 0, 14, 7, 0, 0, 7, 0xE8, 0, 3, 0x7F, 0x22, 0x31, 0, 0, 0, 0]
                } else if self.session_reply == 2 && id == dashboard_core::ble::canyonero::reads::ReadId::ActiveSession
                {
                    vec![0x1F, 0xB2, 0, 14, 7, 0, 0, 7, 0xE8, 0, 4, 0x62, hi, lo, 3, 0, 0, 0]
                } else if self.negative_row && id == dashboard_core::ble::canyonero::reads::ReadId::OdxIdentifier {
                    vec![0x1F, 0xB2, 0, 14, 7, 0, 0, 7, 0xE8, 0, 3, 0x7F, 0x22, 0x31, 0, 0, 0, 0]
                } else if self.segmented && id == dashboard_core::ble::canyonero::reads::ReadId::OdxIdentifier {
                    vec![
                        0x1F,
                        0xB2,
                        0,
                        14,
                        7,
                        0,
                        0,
                        7,
                        0xE8,
                        0,
                        0x10,
                        10,
                        0x62,
                        hi,
                        if self.foreign_first_frame { lo ^ 1 } else { lo },
                        b'A',
                        b'B',
                        b'C',
                    ]
                } else {
                    vec![0x1F, 0xB2, 0, 14, 7, 0, 0, 7, 0xE8, 0, 4, 0x62, hi, lo, 1, 0, 0, 0]
                }
            }
            protocol::Command::Close => vec![0x1F, 0xB1, 0, 1, 7],
            protocol::Command::FlowControl(_) => {
                assert!(self.segmented && !self.foreign_first_frame);
                vec![
                    0x1F, 0xB2, 0, 14, 7, 0, 0, 7, 0xE8, 0, 0x21, b'D', b'E', b'F', b'G', 0, 0, 0,
                ]
            }
        };
        let extra = match (self.queued_extra, session.submission().unwrap().command) {
            (1, protocol::Command::Read(dashboard_core::ble::canyonero::reads::ReadId::ActiveSession)) => {
                Some(reply.clone())
            }
            (2, protocol::Command::Read(dashboard_core::ble::canyonero::reads::ReadId::OdxIdentifier)) => {
                Some(vec![
                    0x1F, 0xB2, 0, 14, 7, 0, 0, 7, 0xE8, 0, 0x21, b'D', b'E', b'F', b'G', 0, 0, 0,
                ])
            }
            _ => None,
        };
        self.replies
            .push_back(if self.partial { reply[..3].to_vec() } else { reply });
        if let Some(extra) = extra {
            self.replies.push_back(extra);
        }
        session.sent(request.token(), clock.now_ms())
    }

    /// Returns the next scripted packet, or waits until the outer deadline/cancellation.
    async fn receive(&mut self) -> Result<Self::Sdu, protocol::Error> {
        if self.replies.is_empty() {
            pending::<()>().await;
        }
        Ok(Packet {
            bytes: self.replies.pop_front().unwrap(),
            drops: self.drops.clone(),
        })
    }
}
/// Polls a fully synchronous synthetic `work`, panicking if it unexpectedly waits.
/// Returns the future's ready output without installing a scheduler or advancing the clock.
fn ready<T>(work: impl Future<Output = T>) -> T {
    let mut work = pin!(work);
    match work.as_mut().poll(&mut Context::from_waker(Waker::noop())) {
        Poll::Ready(value) => value,
        Poll::Pending => panic!("unexpected wait"),
    }
}

/// The complete finite trace uses one voltage/open/config,28 reads and one close; cleanup remains separate.
#[test]
fn complete_collection_releases_packets_and_publishes_before_next_command() {
    let clock = Time::default();
    let mut session = protocol::Session::new();
    let mut link = Link::default();
    let drops = link.drops.clone();
    let updates = RefCell::new(Vec::new());
    let mut publication = collection_flow::Publication::new(0);
    let emit = async |update| {
        if matches!(update, Update::Voltage { .. }) {
            assert_eq!(drops.get(), 1);
        }
        updates.borrow_mut().push(update);
    };
    assert_eq!(
        ready(collection_flow::run(
            &mut session,
            EPOCH,
            &clock,
            &mut link,
            &mut publication,
            &async || true,
            &emit
        )),
        Ok(())
    );
    assert!(publication.successful());
    assert_eq!(link.sent.len(), 32);
    assert_eq!(link.sent[0], [0x1F, 0x12, 0, 0]);
    assert_eq!(link.sent[1], [0x1F, 0x30, 0, 6, 0, 0, 7, 0xA1, 0x20, 0]);
    assert_eq!(link.sent.last().unwrap(), &[0x1F, 0x31, 0, 1, 7]);
    assert_eq!(
        updates
            .borrow()
            .iter()
            .filter(|u| matches!(u, Update::RowHeader { .. }))
            .count(),
        28
    );
    assert!(!updates.borrow().iter().any(|u| matches!(u, Update::End { .. })));
    ready(collection_flow::finish(
        &mut publication,
        &session,
        &clock,
        CleanupOutcome::Clean,
        &emit,
    ));
    assert!(matches!(
        updates.borrow().last(),
        Some(Update::End {
            completed: true,
            fault: None,
            cleanup: CleanupOutcome::Clean,
            ..
        })
    ));
    let mut evidence = dashboard_core::discovery::collection::Evidence::new(0);
    for &update in updates.borrow().iter().skip(1) {
        assert!(evidence.apply(update), "rejected {update:?}");
    }
}

/// Preflight rejection and stale final writers never submit a byte.
#[test]
fn capacity_and_current_owner_guard_precede_egress() {
    for stale in [false, true] {
        let mut session = protocol::Session::new();
        let mut link = Link {
            stale,
            ..Default::default()
        };
        let clock = Time::default();
        let mut publication = collection_flow::Publication::new(0);
        assert_eq!(
            ready(collection_flow::run(
                &mut session,
                EPOCH,
                &clock,
                &mut link,
                &mut publication,
                &async || stale,
                &async |_| {}
            )),
            Err(if stale {
                protocol::Error::Stale
            } else {
                protocol::Error::Capacity
            })
        );
        assert!(link.sent.is_empty());
    }
}

/// Unsupported rows remain useful raw evidence, but a finished sequence cannot claim full success.
#[test]
fn negative_read_is_retained_and_prevents_clean_collection_success() {
    let mut session = protocol::Session::new();
    let mut link = Link {
        negative_row: true,
        ..Default::default()
    };
    let clock = Time::default();
    let mut publication = collection_flow::Publication::new(0);
    let updates = RefCell::new(Vec::new());
    assert_eq!(
        ready(collection_flow::run(
            &mut session,
            EPOCH,
            &clock,
            &mut link,
            &mut publication,
            &async || true,
            &async |u| {
                updates.borrow_mut().push(u);
            }
        )),
        Ok(())
    );
    assert_eq!(link.sent.len(), 32);
    assert!(updates.borrow().iter().any(|u| {
        matches!(
            u,
            Update::RowHeader {
                status: protocol::ReadStatus::Negative(0x31),
                ..
            }
        )
    }));
    assert!(!publication.successful());
}

/// Queued duplicates or premature CFs revoke the next permit before any additional command.
#[test]
fn queued_unexpected_packets_stop_before_next_read_or_cts() {
    for queued_extra in [1, 2] {
        let mut session = protocol::Session::new();
        let mut link = Link {
            queued_extra,
            segmented: queued_extra == 2,
            ..Default::default()
        };
        let clock = Time::default();
        let mut publication = collection_flow::Publication::new(0);
        assert!(
            ready(collection_flow::run(
                &mut session,
                EPOCH,
                &clock,
                &mut link,
                &mut publication,
                &async || true,
                &async |_| {}
            ))
            .is_err()
        );
        assert_eq!(
            link.sent.len(),
            if queued_extra == 1 { 4 } else { 5 },
            "queued input must prevent the next egress"
        );
    }
}

/// Denied/nondefault F186 stops later reads but matching CLOSE and raw denial remain captureable.
#[test]
fn early_session_stop_survives_real_flow_publication_and_final_capture() {
    use dashboard_core::discovery::capture::{self, BuildIdentity, BuildProfile};
    use dashboard_core::discovery::{Advertisement, Input, InspectionItem, Peer, Probe, RadioPhase, ReportKind};
    for session_reply in [1, 2] {
        let mut session = protocol::Session::new();
        let mut link = Link {
            session_reply,
            ..Default::default()
        };
        let clock = Time::default();
        let mut publication = collection_flow::Publication::new(0);
        let updates = RefCell::new(Vec::new());
        let emit = async |u| {
            updates.borrow_mut().push(u);
        };
        assert_eq!(
            ready(collection_flow::run(
                &mut session,
                EPOCH,
                &clock,
                &mut link,
                &mut publication,
                &async || true,
                &emit
            )),
            Ok(())
        );
        assert!(!publication.successful());
        assert_eq!(link.sent.len(), 5);
        ready(collection_flow::finish(
            &mut publication,
            &session,
            &clock,
            CleanupOutcome::Clean,
            &emit,
        ));
        let mut probe = Probe::new();
        probe.identity_mode = true;
        probe.collection_enabled = true;
        probe.catalog.observe(Advertisement::new(
            Peer {
                address: [1, 2, 3, 4, 5, 6],
                random: false,
            },
            ReportKind::Connectable,
            -42,
            &[],
            0,
        ));
        probe.input(Input::Click, 0).unwrap();
        probe.phase = RadioPhase::Paused;
        probe.request.action = dashboard_core::discovery::Action::Collect(probe.selected_device().unwrap().peer);
        for update in updates.into_inner() {
            probe.inspection.accept(InspectionItem::Collection(update));
        }
        assert_eq!(probe.inspection.malformed, 0);
        let evidence = probe.inspection.collection.as_ref().unwrap();
        assert!(evidence.summary.completed);
        assert!(!evidence.successful());
        let mut bytes = [0; 8192];
        let len = capture::encode(
            &probe,
            BuildIdentity {
                source_digest: [1; 32],
                cpu_hz: 150_000_000,
                spi_hz: 37_500_000,
                profile: BuildProfile::Stock,
                source_identity_available: true,
            },
            1,
            &mut bytes,
        )
        .unwrap();
        let decoded = capture::decode(&bytes[..len]).unwrap();
        assert!(decoded.metadata().collection.unwrap().completed);
        assert_eq!(decoded.metadata().collection_row_count, 28);
    }
}

/// Back stops real pending collection work without losing its revision-tagged final evidence.
#[test]
fn retained_generation_cancel_finishes_and_exports_before_next_scan() {
    use dashboard_core::discovery::capture::{self, BuildIdentity, BuildProfile};
    use dashboard_core::discovery::{
        Action,
        Advertisement,
        Event as ProbeEvent,
        Input,
        InspectionItem,
        Peer,
        Probe,
        ProbeError,
        RadioPhase,
        ReportKind,
    };
    use embassy_futures::select::{Either, select};
    let mut probe = Probe::new();
    probe.identity_mode = true;
    probe.collection_enabled = true;
    probe.catalog.observe(Advertisement::new(
        Peer {
            address: [1, 2, 3, 4, 5, 6],
            random: false,
        },
        ReportKind::Connectable,
        -42,
        &[],
        0,
    ));
    probe.phase = RadioPhase::Scanning;
    probe.input(Input::Collect, 0);
    probe.collection_presented(probe.collection_prompt_id().unwrap(), 1);
    let request = probe.input(Input::Click, 2).unwrap();
    probe.phase = RadioPhase::Discovering;
    bridge::set_request(request);
    bridge::cancel_collection(0);
    bridge::cancel_collection(request.revision + 1);
    assert!(!bridge::collection_cancelled(0));
    assert!(!bridge::collection_cancelled(request.revision));
    let epoch = Epoch {
        connection: u64::from(request.revision),
        operation: u64::from(request.revision),
        maintenance: 0,
    };
    let probe = RefCell::new(probe);
    let clock = Time::default();
    clock.0.set(2);
    let mut session = protocol::Session::new();
    let mut publication = collection_flow::Publication::new(2);
    let mut link = Link {
        block_at: Some(4),
        ..Default::default()
    };
    let emit = async |update| {
        probe.borrow_mut().event(ProbeEvent::Item {
            revision: request.revision,
            item: InspectionItem::Collection(update),
        });
    };
    {
        let cancelled = poll_fn(|_| {
            if bridge::collection_cancelled(request.revision) {
                Poll::Ready(())
            } else {
                Poll::Pending
            }
        });
        let run = collection_flow::run(
            &mut session,
            epoch,
            &clock,
            &mut link,
            &mut publication,
            &async || true,
            &emit,
        );
        let mut work = pin!(select(cancelled, run));
        let mut cx = Context::from_waker(Waker::noop());
        assert!(work.as_mut().poll(&mut cx).is_pending());
        clock.0.set(3);
        assert!(probe.borrow_mut().input(Input::Back, 3).is_none());
        assert!(probe.borrow().cancel_collection);
        assert_eq!(probe.borrow().request, request);
        bridge::cancel_collection(request.revision);
        bridge::cancel_collection(request.revision);
        assert!(matches!(work.as_mut().poll(&mut cx), Poll::Ready(Either::First(()))));
    }
    assert_eq!(link.sent.len(), 4);
    let _ = session.cancel(epoch, clock.now_ms());
    ready(collection_flow::finish(
        &mut publication,
        &session,
        &clock,
        CleanupOutcome::Clean,
        &emit,
    ));
    let mut probe = probe.borrow_mut();
    probe.event(ProbeEvent::InspectionEnd {
        revision: request.revision,
        at_ms: 3,
        cleanup_complete: true,
        disconnect_reason: None,
    });
    probe.event(ProbeEvent::State {
        revision: request.revision,
        phase: RadioPhase::Paused,
        error: ProbeError::Transport,
    });
    assert_eq!(probe.request, request);
    assert_eq!(probe.inspection.malformed, 0);
    assert_eq!(
        probe.inspection.collection.as_ref().unwrap().summary.ended_at_ms,
        Some(3)
    );
    let mut bytes = [0; 8192];
    let len = capture::encode(
        &probe,
        BuildIdentity {
            source_digest: [1; 32],
            cpu_hz: 150_000_000,
            spi_hz: 37_500_000,
            profile: BuildProfile::Stock,
            source_identity_available: true,
        },
        3,
        &mut bytes,
    )
    .unwrap();
    let decoded = capture::decode(&bytes[..len]).unwrap();
    assert_eq!(
        decoded.metadata().collection.unwrap().fault,
        Some(protocol::Error::Cancelled)
    );
    assert_eq!(
        bridge::request(),
        request,
        "cancel must not reissue the completed action"
    );
    assert!(!bridge::collection_cancelled(request.revision + 1));
    let next = probe.input(Input::Back, 4).unwrap();
    assert!(matches!(next.action, Action::Scan(true)));
    assert_ne!(next.revision, request.revision);
    bridge::set_request(next);
    bridge::cancel_collection(next.revision);
    assert!(
        !bridge::collection_cancelled(next.revision),
        "Scan cannot acquire a collection stop marker"
    );
}

/// Only a correlated FF can cause the actual flow to submit CTS; foreign echoes abort without CLOSE.
#[test]
fn correlated_multiframe_response_alone_permits_fixed_flow_control() {
    for foreign_first_frame in [false, true] {
        let mut session = protocol::Session::new();
        let mut link = Link {
            segmented: true,
            foreign_first_frame,
            ..Default::default()
        };
        let clock = Time::default();
        let mut publication = collection_flow::Publication::new(0);
        let updates = RefCell::new(Vec::new());
        let emit = async |u| {
            updates.borrow_mut().push(u);
        };
        let result = ready(collection_flow::run(
            &mut session,
            EPOCH,
            &clock,
            &mut link,
            &mut publication,
            &async || true,
            &emit,
        ));
        if foreign_first_frame {
            assert_eq!(result, Err(protocol::Error::Diagnostic));
            assert_eq!(link.sent.len(), 5);
        } else {
            assert_eq!(result, Ok(()));
            assert_eq!(link.sent.len(), 33);
            assert_eq!(link.sent[5], [0x1F, 0x33, 0, 9, 7, 0x30, 4, 5, 0, 0, 0, 0, 0]);
            let raw: Vec<_> = updates
                .borrow()
                .iter()
                .filter_map(|u| {
                    if let Update::Chunk {
                        id: dashboard_core::ble::canyonero::reads::ReadId::OdxIdentifier,
                        bytes,
                        ..
                    } = u
                    {
                        Some(bytes.as_slice())
                    } else {
                        None
                    }
                })
                .flatten()
                .copied()
                .collect();
            assert_eq!(raw, [0x62, 0xF1, 0x9E, b'A', b'B', b'C', b'D', b'E', b'F', b'G']);
        }
    }
}

/// A pending writer can already have submitted bytes; cancellation and timeout preserve unknown effects.
#[test]
fn interrupted_collection_send_retains_unknown_without_close_or_retry() {
    for block_at in [1, 4] {
        for timeout in [false, true] {
            let mut session = protocol::Session::new();
            let mut link = Link {
                block_at: Some(block_at),
                ..Default::default()
            };
            let clock = Time::default();
            let mut publication = collection_flow::Publication::new(0);
            let updates = RefCell::new(Vec::new());
            let emit = async |u| {
                updates.borrow_mut().push(u);
            };
            {
                let mut work = pin!(collection_flow::run(
                    &mut session,
                    EPOCH,
                    &clock,
                    &mut link,
                    &mut publication,
                    &async || true,
                    &emit
                ));
                let mut cx = Context::from_waker(Waker::noop());
                assert!(work.as_mut().poll(&mut cx).is_pending());
                if timeout {
                    clock.0.set(2000);
                    assert_eq!(work.as_mut().poll(&mut cx), Poll::Ready(Err(protocol::Error::Timeout)));
                }
            }
            let _ = session.cancel(EPOCH, clock.now_ms());
            ready(collection_flow::finish(
                &mut publication,
                &session,
                &clock,
                CleanupOutcome::Clean,
                &emit,
            ));
            assert_eq!(link.sent.len(), block_at);
            assert_eq!(link.sent[0], [0x1F, 0x12, 0, 0]);
            if block_at == 4 {
                assert_eq!(
                    updates
                        .borrow()
                        .iter()
                        .filter(|u| matches!(u, Update::Attempt { .. }))
                        .count(),
                    1
                );
            }
            assert!(matches!(
                updates.borrow().last(),
                Some(Update::End {
                    completed: false,
                    last_submission: Some(protocol::Submission {
                        state: protocol::SendState::InFlightUnknown,
                        ..
                    }),
                    ..
                })
            ));
        }
    }
}

/// A fragmented reply preserved by the actual parser is exported after receive cancellation.
#[test]
fn interrupted_receive_preserves_terminal_prefix() {
    let mut session = protocol::Session::new();
    let mut link = Link {
        partial: true,
        ..Default::default()
    };
    let clock = Time::default();
    let mut publication = collection_flow::Publication::new(0);
    let updates = RefCell::new(Vec::new());
    let emit = async |u| {
        updates.borrow_mut().push(u);
    };
    {
        let mut work = pin!(collection_flow::run(
            &mut session,
            EPOCH,
            &clock,
            &mut link,
            &mut publication,
            &async || true,
            &emit
        ));
        assert!(work.as_mut().poll(&mut Context::from_waker(Waker::noop())).is_pending());
    }
    let _ = session.cancel(EPOCH, clock.now_ms());
    ready(collection_flow::finish(
        &mut publication,
        &session,
        &clock,
        CleanupOutcome::Clean,
        &emit,
    ));
    assert_eq!(link.drops.get(), 1);
    let raw: Vec<_> = updates
        .borrow()
        .iter()
        .filter_map(|u| {
            if let Update::TerminalChunk { bytes, .. } = u {
                Some(bytes.as_slice())
            } else {
                None
            }
        })
        .flatten()
        .copied()
        .collect();
    assert_eq!(raw, [0x1F, 0x92, 0]);
    assert_eq!(link.sent.len(), 1);
}

/// Missing core1 admission expires before a permit; Pending cleanup cannot fabricate End.
#[test]
fn admission_deadline_and_cleanup_observation_are_required() {
    let mut session = protocol::Session::new();
    let mut link = Link::default();
    let clock = Time::default();
    let mut publication = collection_flow::Publication::new(0);
    let updates = RefCell::new(Vec::new());
    let emit = async |u| {
        updates.borrow_mut().push(u);
    };
    {
        let admission = async || pending::<bool>().await;
        let mut work = pin!(collection_flow::run(
            &mut session,
            EPOCH,
            &clock,
            &mut link,
            &mut publication,
            &admission,
            &emit
        ));
        let mut cx = Context::from_waker(Waker::noop());
        assert!(work.as_mut().poll(&mut cx).is_pending());
        clock.0.set(2000);
        assert_eq!(work.as_mut().poll(&mut cx), Poll::Ready(Err(protocol::Error::Timeout)));
    }
    assert!(link.sent.is_empty());
    ready(collection_flow::finish(
        &mut publication,
        &session,
        &clock,
        CleanupOutcome::Pending,
        &emit,
    ));
    assert!(!updates.borrow().iter().any(|u| matches!(u, Update::End { .. })));
    ready(collection_flow::finish(
        &mut publication,
        &session,
        &clock,
        CleanupOutcome::TimedOut,
        &emit,
    ));
    assert!(matches!(
        updates.borrow().last(),
        Some(Update::End {
            fault: Some(protocol::Error::Timeout),
            cleanup: CleanupOutcome::TimedOut,
            ..
        })
    ));
}

/// A ready admission reply crossing its original deadline or reversing time never grants voltage.
#[test]
fn admission_completion_time_is_checked_before_first_permit() {
    for (completed_ms, expected) in [(99, protocol::Error::Clock), (2_100, protocol::Error::Timeout)] {
        let clock = Time::default();
        clock.0.set(100);
        let mut session = protocol::Session::new();
        let mut link = Link::default();
        let mut publication = collection_flow::Publication::new(100);
        let updates = RefCell::new(Vec::new());
        let emit = async |update| {
            updates.borrow_mut().push(update);
        };
        let admit = async || {
            clock.0.set(completed_ms);
            true
        };
        assert_eq!(
            ready(collection_flow::run(
                &mut session,
                EPOCH,
                &clock,
                &mut link,
                &mut publication,
                &admit,
                &emit
            )),
            Err(expected)
        );
        assert!(link.sent.is_empty());
        assert!(session.submission().is_none());
        // A later actual cleanup observation can flush retained failure evidence.
        clock.0.set(2_200);
        ready(collection_flow::finish(
            &mut publication,
            &session,
            &clock,
            CleanupOutcome::Clean,
            &emit,
        ));
        let mut evidence = dashboard_core::discovery::collection::Evidence::new(100);
        for &update in updates.borrow().iter().skip(1) {
            assert!(evidence.apply(update), "rejected {update:?}");
        }
        assert_eq!(evidence.summary.fault, Some(expected));
        assert!(!evidence.summary.completed);
    }
}

/// Cancellation at a row publication await retains the header and exact bytes after packet release.
#[test]
fn cancelled_row_publication_flushes_once_before_terminal_evidence() {
    let mut session = protocol::Session::new();
    let mut link = Link::default();
    let clock = Time::default();
    let drops = link.drops.clone();
    let mut publication = collection_flow::Publication::new(0);
    let updates = RefCell::new(Vec::new());
    let blocked = async |update| {
        if matches!(update, Update::RowHeader { .. }) {
            assert_eq!(drops.get(), 4);
            pending::<()>().await;
        }
        updates.borrow_mut().push(update);
    };
    {
        let mut work = pin!(collection_flow::run(
            &mut session,
            EPOCH,
            &clock,
            &mut link,
            &mut publication,
            &async || true,
            &blocked
        ));
        assert!(work.as_mut().poll(&mut Context::from_waker(Waker::noop())).is_pending());
    }
    let _ = session.cancel(EPOCH, clock.now_ms());
    let emit = async |u| {
        updates.borrow_mut().push(u);
    };
    ready(collection_flow::finish(
        &mut publication,
        &session,
        &clock,
        CleanupOutcome::Clean,
        &emit,
    ));
    ready(collection_flow::finish(
        &mut publication,
        &session,
        &clock,
        CleanupOutcome::Clean,
        &emit,
    ));
    let updates = updates.borrow();
    assert_eq!(link.sent.len(), 4);
    assert_eq!(
        updates.iter().filter(|u| matches!(u, Update::RowHeader { .. })).count(),
        1
    );
    let raw: Vec<_> = updates
        .iter()
        .filter_map(|u| {
            if let Update::Chunk { bytes, .. } = u {
                Some(bytes.as_slice())
            } else {
                None
            }
        })
        .flatten()
        .copied()
        .collect();
    assert_eq!(raw, [0x62, 0xF1, 0x86, 1]);
    assert_eq!(updates.iter().filter(|u| matches!(u, Update::End { .. })).count(), 1);
}
