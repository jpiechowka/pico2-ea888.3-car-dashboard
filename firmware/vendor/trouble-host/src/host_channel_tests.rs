//! Actual channel/host parameter negotiation and cancellation regressions with an in-memory HCI recorder.

extern crate std;
use super::*;
use crate::l2cap::{L2capChannel, L2capChannelConfig};
use crate::mock_controller::MockController;
use crate::prelude::DefaultPacketPool;
use crate::HostResources;
use bt_hci::param::BdAddr;
use core::future::Future;
use core::pin::pin;
use core::task::{Context, Waker};
use embassy_futures::block_on;

#[derive(Default)]
/// Records ACL payloads and asynchronous command opcodes; one-shot failure switches exercise host cleanup without hardware.
struct ParamController {
    /// Submitted ACL data bytes, excluding the HCI ACL header.
    acl: RefCell<std::vec::Vec<std::vec::Vec<u8>>>,
    /// Asynchronous HCI opcodes in submission order.
    commands: RefCell<std::vec::Vec<u16>>,
    /// Optional HCI status consumed by the next asynchronous command.
    fail_command: RefCell<Option<bt_hci::param::Error>>,
    /// One-shot transport failure before recording the next ACL packet.
    fail_acl: RefCell<bool>,
    /// Maximum recorded ACL writes before transport waits; None leaves it unbounded.
    acl_limit: core::cell::Cell<Option<usize>>,
    /// Last transport-capacity waiter, deliberately independent of connection state.
    acl_waker: RefCell<Option<Waker>>,
}
impl embedded_io::ErrorType for ParamController {
    /// Synthetic transport failure type; ACL writes use Other.
    type Error = embedded_io::ErrorKind;
}
impl Controller for ParamController {
    /// 259-byte owned receive scratch; reads intentionally never finish.
    type Buffer<'a> = [u8; 259];
    /// Returns zeroed 259-byte scratch without resource limits.
    fn alloc_buf(&self) -> Result<Self::Buffer<'_>, Self::Error> {
        Ok([0; 259])
    }
    /// Returns a permanently pending receive future; the unused output buffer is never written.
    async fn read<'a>(&self, _: &'a mut Self::Buffer<'_>) -> Result<ControllerToHostPacket<'a>, Self::Error> {
        core::future::pending().await
    }
    /// Records `packet` payload and returns Ok, or consumes the one-shot failure before recording bytes.
    async fn write_acl_data(&self, packet: &AclPacket<'_>) -> Result<(), Self::Error> {
        if self.fail_acl.replace(false) {
            return Err(embedded_io::ErrorKind::Other);
        }
        poll_fn(|cx| {
            if self
                .acl_limit
                .get()
                .is_some_and(|limit| self.acl.borrow().len() >= limit)
            {
                *self.acl_waker.borrow_mut() = Some(cx.waker().clone());
                Poll::Pending
            } else {
                Poll::Ready(())
            }
        })
        .await;
        self.acl.borrow_mut().push(packet.data().to_vec());
        Ok(())
    }
    /// Panics: synchronous-data transport is outside these fixtures; returns no result.
    async fn write_sync_data(&self, _: &bt_hci::data::SyncPacket<'_>) -> Result<(), Self::Error> {
        unreachable!()
    }
    /// Panics: ISO transport is outside these fixtures; returns no result.
    async fn write_iso_data(&self, _: &bt_hci::data::IsoPacket<'_>) -> Result<(), Self::Error> {
        unreachable!()
    }
}
/// Synchronous command execution is deliberately unsupported by this recorder.
impl<C: SyncCmd> ControllerCmdSync<C> for ParamController {
    /// Panics for every synchronous command; returns no result.
    async fn exec(&self, _: &C) -> Result<C::Return, cmd::Error<Self::Error>> {
        unreachable!()
    }
}
/// Asynchronous commands retain their opcode and consume an optional injected HCI failure.
impl<C: AsyncCmd> ControllerCmdAsync<C> for ParamController {
    /// Records the command opcode; returns Ok or the injected HCI failure, consuming that failure.
    async fn exec(&self, _: &C) -> Result<(), cmd::Error<Self::Error>> {
        self.commands.borrow_mut().push(C::OPCODE.to_raw());
        match self.fail_command.borrow_mut().take() {
            Some(error) => Err(cmd::Error::Hci(error)),
            None => Ok(()),
        }
    }
}

#[test]
/// A full application queue sends a rejected parameter response; zero signaling identifiers never produce output.
fn parameter_requests_reject_queue_pressure_and_ignore_invalid_zero_identifiers() {
    let mut resources = HostResources::<DefaultPacketPool, 1, 1>::new();
    let stack = crate::new(ParamController::default(), &mut resources).build();
    let host = stack.host();
    let mgr = host.connections();
    let peer = Address::random([1, 2, 3, 4, 5, 6]);
    mgr.connect(ConnHandle::new(1), peer, LeConnRole::Central, ConnParams::new())
        .unwrap();
    let conn = block_on(mgr.accept(LeConnRole::Central, &[peer]));
    for _ in 0..crate::config::CONNECTION_EVENT_QUEUE_SIZE {
        mgr.post_handle_event(
            conn.handle(),
            ConnectionEvent::ConnectionParamsUpdated {
                conn_interval: Duration::from_millis(30),
                peripheral_latency: 0,
                supervision_timeout: Duration::from_secs(2),
            },
        )
        .unwrap();
    }
    host.channels()
        .signal(conn.handle(), &[0x12, 9, 8, 0, 24, 0, 40, 0, 0, 0, 200, 0], mgr)
        .unwrap();
    let (_, reply) = block_on(mgr.outbound());
    assert_eq!(reply.as_ref(), &[6, 0, 5, 0, 0x13, 9, 2, 0, 1, 0]);
    drop(reply);
    for data in [&[0x12, 0, 8, 0, 24, 0, 40, 0, 0, 0, 200, 0][..], &[0x12, 0, 8, 0][..]] {
        let _ = host.channels().signal(conn.handle(), data, mgr);
        assert!(pin!(mgr.outbound())
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop()))
            .is_pending());
    }
    assert!(conn.is_connected());
    mgr.disconnected(conn.handle(), Status::SUCCESS).unwrap();
}

#[cfg(feature = "central")]
#[test]
/// Unknown connection rejects without ACL reply; other controller errors preserve status and send rejection under the original identifier.
fn connection_parameter_controller_errors_preserve_failure_and_reply_identity() {
    for error in [
        bt_hci::param::Error::CMD_DISALLOWED,
        bt_hci::param::Error::UNKNOWN_CONN_IDENTIFIER,
    ] {
        let mut resources = HostResources::<DefaultPacketPool, 1, 1>::new();
        let stack = crate::new(ParamController::default(), &mut resources).build();
        let host = stack.host();
        host.state
            .initialized
            .init(InitialState {
                acl_max: 251,
                acl_total: 4,
            })
            .ok()
            .unwrap();
        let mgr = host.connections();
        mgr.set_link_credits(4);
        let peer = Address::random([1, 2, 3, 4, 5, 6]);
        mgr.connect(ConnHandle::new(1), peer, LeConnRole::Central, ConnParams::new())
            .unwrap();
        let conn = block_on(mgr.accept(LeConnRole::Central, &[peer]));
        host.channels()
            .signal(conn.handle(), &[0x12, 9, 8, 0, 24, 0, 40, 0, 0, 0, 200, 0], mgr)
            .unwrap();
        let ConnectionEvent::RequestConnectionParams(request) = block_on(conn.next()) else {
            panic!()
        };
        *stack.controller.fail_command.borrow_mut() = Some(error);
        let result = block_on(request.accept(None, &stack));
        if error == bt_hci::param::Error::UNKNOWN_CONN_IDENTIFIER {
            assert!(matches!(result, Err(BleHostError::BleHost(Error::Disconnected))));
            assert!(stack.controller.acl.borrow().is_empty());
        } else {
            assert!(matches!(result, Err(BleHostError::BleHost(Error::Hci(actual))) if actual == error));
            assert_eq!(
                &*stack.controller.acl.borrow(),
                &[std::vec![6, 0, 5, 0, 0x13, 9, 2, 0, 1, 0]]
            );
        }
        mgr.disconnected(conn.handle(), Status::SUCCESS).unwrap();
    }
}

#[cfg(feature = "central")]
#[test]
/// Accept, explicit reject and invalid override preserve the peer identifier; only valid acceptance issues a controller update.
fn connection_parameter_reply_matches_original_identifier_for_accept_reject_and_invalid_override() {
    for action in 0..3 {
        let mut resources = HostResources::<DefaultPacketPool, 1, 1>::new();
        let stack = crate::new(ParamController::default(), &mut resources).build();
        let host = stack.host();
        host.state
            .initialized
            .init(InitialState {
                acl_max: 251,
                acl_total: 4,
            })
            .ok()
            .unwrap();
        let mgr = host.connections();
        mgr.set_link_credits(4);
        let peer = Address::random([1, 2, 3, 4, 5, 6]);
        mgr.connect(ConnHandle::new(1), peer, LeConnRole::Central, ConnParams::new())
            .unwrap();
        let conn = block_on(mgr.accept(LeConnRole::Central, &[peer]));
        host.channels()
            .signal(conn.handle(), &[0x12, 0xa7, 8, 0, 24, 0, 40, 0, 0, 0, 200, 0], mgr)
            .unwrap();
        let ConnectionEvent::RequestConnectionParams(request) = block_on(conn.next()) else {
            panic!()
        };
        if action == 0 {
            block_on(request.accept(None, &stack)).unwrap();
        } else if action == 1 {
            block_on(request.reject(&stack)).unwrap();
        } else {
            let invalid = crate::connection::RequestedConnParams {
                min_connection_interval: Duration::from_micros(0),
                ..Default::default()
            };
            block_on(request.accept(Some(&invalid), &stack)).unwrap();
        }
        assert_eq!(
            &*stack.controller.acl.borrow(),
            &[std::vec![6, 0, 5, 0, 0x13, 0xa7, 2, 0, u8::from(action != 0), 0]]
        );
        assert_eq!(stack.controller.commands.borrow().len(), usize::from(action == 0));
        mgr.disconnected(conn.handle(), Status::SUCCESS).unwrap();
    }
}

#[cfg(feature = "connection-params-update")]
#[test]
/// Link-layer acceptance emits only LE Remote Connection Parameter Request Reply, with no unrelated update or L2CAP signal.
fn link_layer_parameter_reply_does_not_start_an_unrelated_connection_update() {
    let mut resources = HostResources::<DefaultPacketPool, 1, 1>::new();
    let stack = crate::new(ParamController::default(), &mut resources).build();
    let host = stack.host();
    host.state
        .initialized
        .init(InitialState {
            acl_max: 251,
            acl_total: 1,
        })
        .ok()
        .unwrap();
    let peer = Address::random([1, 2, 3, 4, 5, 6]);
    host.connections()
        .connect(ConnHandle::new(1), peer, LeConnRole::Peripheral, ConnParams::new())
        .unwrap();
    let conn = block_on(host.connections().accept(LeConnRole::Peripheral, &[peer]));
    let request = ConnectionParamsRequest::new_link_layer(RequestedConnParams::default(), conn.handle());
    block_on(request.accept(None, &stack)).unwrap();
    // LE Remote Connection Parameter Request Reply, OCF0x20/OGF0x08.
    assert_eq!(&*stack.controller.commands.borrow(), &[0x2020]);
    assert!(stack.controller.acl.borrow().is_empty());
    host.connections().disconnected(conn.handle(), Status::SUCCESS).unwrap();
}

#[test]
/// Zero/one-byte first K-frames reject and close except ordinary HCI reassembly legitimately awaiting missing bytes.
fn short_dynamic_frames_fail_at_ingress_in_both_reassembly_modes() {
    for length in [0u8, 1] {
        for fragmented in [false, true] {
            let mut resources = HostResources::<DefaultPacketPool, 1, 1>::new();
            let stack = crate::new(MockController::new(), &mut resources)
                .register_l2cap_spsm(0x80)
                .build();
            let host = stack.host();
            let handle = ConnHandle::new(1);
            host.connections()
                .connect(
                    handle,
                    Address::new(AddrKind::PUBLIC, BdAddr::new([0; 6])),
                    LeConnRole::Central,
                    ConnParams::new(),
                )
                .unwrap();
            let Poll::Ready(conn) = host.connections().poll_accept(LeConnRole::Central, &[], None) else {
                panic!()
            };
            let listener = L2capChannel::listen(&stack, &conn);
            host.channels()
                .signal(
                    handle,
                    &[0x14, 1, 10, 0, 0x80, 0, 0x41, 0, 64, 0, 23, 0, 2, 0],
                    host.connections(),
                )
                .unwrap();
            let pending = block_on(listener.next()).unwrap();
            let config = L2capChannelConfig::default();
            let mut accept = pin!(pending.accept(&stack, &config));
            assert!(accept
                .as_mut()
                .poll(&mut Context::from_waker(Waker::noop()))
                .is_pending());
            let bytes = [if fragmented { 3 } else { length }, 0, 0x40, 0, 0];
            let packet = AclPacket::new(
                handle,
                AclPacketBoundary::FirstFlushable,
                AclBroadcastFlag::PointToPoint,
                &bytes[..4 + length as usize],
            );
            let result = host.handle_acl(packet, &DummyHandler);
            // Normal HCI reassembly may legally await the remainder of a
            // fragmented first frame. Optimized SDU reassembly needs two bytes.
            if fragmented && !cfg!(feature = "l2cap-sdu-reassembly-optimization") {
                assert!(result.is_ok());
            } else {
                assert!(result.is_err());
                assert!(!conn.is_connected());
            }
        }
    }
}

/// Initializes `host` with eight 251-byte ACL credits for deterministic signaling tests; duplicate initialization panics.
fn ready_channel_host(host: BleHost<'_, ParamController, DefaultPacketPool>) {
    host.state
        .initialized
        .init(InitialState {
            acl_max: 251,
            acl_total: 8,
        })
        .ok()
        .unwrap();
    host.connections().set_link_credits(8);
}

/// Injects a response into `host` for `handle` and `identifier`; `invalid` selects zero MPS, `rejected` selects refusal. Deliberately ignores admission errors so callers inspect create completion.
fn channel_response(
    host: BleHost<'_, ParamController, DefaultPacketPool>,
    handle: ConnHandle,
    identifier: u8,
    invalid: bool,
    rejected: bool,
) {
    let mps = if invalid { 0 } else { 23 };
    let result = if rejected { 2 } else { 0 };
    let _ = host.channels().signal(
        handle,
        &[0x15, identifier, 10, 0, 0x41, 0, 64, 0, mps, 0, 2, 0, result, 0],
        host.connections(),
    );
}

#[test]
/// A nominally successful response with zero MPS makes the actual pending create return an error.
fn invalid_channel_success_returns_error_when_actual_create_is_polled() {
    let mut resources = HostResources::<DefaultPacketPool, 1, 1>::new();
    let stack = crate::new(ParamController::default(), &mut resources).build();
    let host = stack.host();
    ready_channel_host(host);
    let handle = ConnHandle::new(1);
    let peer = Address::random([1, 2, 3, 4, 5, 6]);
    host.connections()
        .connect(handle, peer, LeConnRole::Central, ConnParams::new())
        .unwrap();
    let conn = block_on(host.connections().accept(LeConnRole::Central, &[peer]));
    let config = L2capChannelConfig::default();
    let mut create = pin!(L2capChannel::create(&stack, &conn, 0x80, &config));
    let mut cx = Context::from_waker(Waker::noop());
    assert!(create.as_mut().poll(&mut cx).is_pending());
    channel_response(host, handle, 1, true, false);
    assert!(matches!(create.as_mut().poll(&mut cx), Poll::Ready(Err(_))));
}

#[test]
/// Malformed, rejected, mismatched and disconnected handshakes retain their slot until the owning future completes or drops, then permit another link.
fn failed_channel_create_reserves_slot_until_poll_or_cancel_then_allows_reuse() {
    // Malformed success, explicit rejection, wrong identifier and link loss.
    for failure in 0..4 {
        for cancel in [false, true] {
            let mut resources = HostResources::<DefaultPacketPool, 2, 1>::new();
            let stack = crate::new(ParamController::default(), &mut resources).build();
            let host = stack.host();
            ready_channel_host(host);
            let handle = ConnHandle::new(1);
            let peer = Address::random([1, 2, 3, 4, 5, 6]);
            host.connections()
                .connect(handle, peer, LeConnRole::Central, ConnParams::new())
                .unwrap();
            let conn = block_on(host.connections().accept(LeConnRole::Central, &[peer]));
            let next_handle = ConnHandle::new(2);
            let next_peer = Address::random([2, 2, 3, 4, 5, 6]);
            host.connections()
                .connect(next_handle, next_peer, LeConnRole::Central, ConnParams::new())
                .unwrap();
            let next_conn = block_on(host.connections().accept(LeConnRole::Central, &[next_peer]));
            let config = L2capChannelConfig::default();
            let mut cx = Context::from_waker(Waker::noop());
            {
                let mut create = pin!(L2capChannel::create(&stack, &conn, 0x80, &config));
                assert!(create.as_mut().poll(&mut cx).is_pending());
                if failure == 3 {
                    host.connections().disconnected(handle, Status::SUCCESS).unwrap();
                    host.channels().disconnected(handle).unwrap();
                } else {
                    channel_response(
                        host,
                        handle,
                        if failure == 2 { 9 } else { 1 },
                        failure == 0,
                        failure == 1,
                    );
                }
                {
                    let mut overlapping = pin!(L2capChannel::create(&stack, &next_conn, 0x80, &config));
                    assert!(
                        matches!(
                            overlapping.as_mut().poll(&mut cx),
                            Poll::Ready(Err(BleHostError::BleHost(Error::NoChannelAvailable)))
                        ),
                        "failure {failure}, cancel {cancel}"
                    );
                }
                if !cancel {
                    assert!(matches!(create.as_mut().poll(&mut cx), Poll::Ready(Err(_))));
                }
            }
            let mut next = pin!(L2capChannel::create(&stack, &next_conn, 0x80, &config));
            assert!(next.as_mut().poll(&mut cx).is_pending());
            let identifier = stack.controller.acl.borrow().last().unwrap()[5];
            channel_response(host, next_handle, identifier, false, false);
            let Poll::Ready(Ok(channel)) = next.as_mut().poll(&mut cx) else {
                panic!("released slot must support a new successful create")
            };
            assert_eq!(channel.peer_mtu(), 64);
        }
    }
}

#[test]
/// A transport error releases the create reservation; a subsequent real handshake succeeds in the same slot.
fn channel_create_send_error_releases_slot_for_successful_retry() {
    let mut resources = HostResources::<DefaultPacketPool, 1, 1>::new();
    let stack = crate::new(ParamController::default(), &mut resources).build();
    let host = stack.host();
    ready_channel_host(host);
    let handle = ConnHandle::new(1);
    let peer = Address::random([1, 2, 3, 4, 5, 6]);
    host.connections()
        .connect(handle, peer, LeConnRole::Central, ConnParams::new())
        .unwrap();
    let conn = block_on(host.connections().accept(LeConnRole::Central, &[peer]));
    let config = L2capChannelConfig::default();
    let mut cx = Context::from_waker(Waker::noop());
    *stack.controller.fail_acl.borrow_mut() = true;
    {
        let mut create = pin!(L2capChannel::create(&stack, &conn, 0x80, &config));
        assert!(matches!(
            create.as_mut().poll(&mut cx),
            Poll::Ready(Err(BleHostError::Controller(embedded_io::ErrorKind::Other)))
        ));
    }
    let mut retry = pin!(L2capChannel::create(&stack, &conn, 0x80, &config));
    assert!(retry.as_mut().poll(&mut cx).is_pending());
    let identifier = stack.controller.acl.borrow().last().unwrap()[5];
    channel_response(host, handle, identifier, false, false);
    assert!(matches!(retry.as_mut().poll(&mut cx), Poll::Ready(Ok(_))));
}

#[test]
/// Canceling after peer success retains the slot until the control path sends/confirms disconnect, after which a new create succeeds.
fn cancelled_create_after_success_keeps_channel_until_disconnect_is_processed() {
    let mut resources = HostResources::<DefaultPacketPool, 1, 1>::new();
    let stack = crate::new(ParamController::default(), &mut resources).build();
    let host = stack.host();
    ready_channel_host(host);
    let handle = ConnHandle::new(1);
    let peer = Address::random([1, 2, 3, 4, 5, 6]);
    host.connections()
        .connect(handle, peer, LeConnRole::Central, ConnParams::new())
        .unwrap();
    let conn = block_on(host.connections().accept(LeConnRole::Central, &[peer]));
    let config = L2capChannelConfig::default();
    let mut cx = Context::from_waker(Waker::noop());
    {
        let mut create = pin!(L2capChannel::create(&stack, &conn, 0x80, &config));
        assert!(create.as_mut().poll(&mut cx).is_pending());
        channel_response(host, handle, 1, false, false);
        // Cancellation wins before the successful result is polled.
    }
    {
        let mut overlapping = pin!(L2capChannel::create(&stack, &conn, 0x80, &config));
        assert!(matches!(
            overlapping.as_mut().poll(&mut cx),
            Poll::Ready(Err(BleHostError::BleHost(Error::NoChannelAvailable)))
        ));
    }
    let Poll::Ready(disconnect) = host.channels().poll_disconnecting(None) else {
        panic!("successful handshake must be disconnected after cancellation")
    };
    block_on(disconnect.send(&host)).unwrap();
    assert_eq!(&stack.controller.acl.borrow().last().unwrap()[4..5], &[6]);
    // The control task confirms completion of its disconnect signal.
    disconnect.confirm();
    let mut retry = pin!(L2capChannel::create(&stack, &conn, 0x80, &config));
    assert!(retry.as_mut().poll(&mut cx).is_pending());
    let identifier = stack.controller.acl.borrow().last().unwrap()[5];
    channel_response(host, handle, identifier, false, false);
    assert!(matches!(retry.as_mut().poll(&mut cx), Poll::Ready(Ok(_))));
}

/// A transport blocked after reserving credits cannot publish bytes after its exact link closes and the numeric handle is reused.
#[test]
fn pending_acl_transport_does_not_retarget_reused_handle() {
    let mut resources = HostResources::<DefaultPacketPool, 2, 1>::new();
    let stack = crate::new(ParamController::default(), &mut resources).build();
    let host = stack.host();
    host.state
        .initialized
        .init(InitialState {
            acl_max: 251,
            acl_total: 4,
        })
        .ok()
        .unwrap();
    let mgr = host.connections();
    mgr.set_link_credits(4);
    let handle = ConnHandle::new(1);
    mgr.connect(handle, Address::random([1; 6]), LeConnRole::Central, ConnParams::new())
        .unwrap();
    let old = block_on(mgr.accept(LeConnRole::Central, &[]));
    let mut sender = block_on(host.l2cap_pdu(handle)).unwrap();
    stack.controller.acl_limit.set(Some(0));
    let mut send = pin!(sender.send(&[42]));
    let mut cx = Context::from_waker(Waker::noop());
    assert!(send.as_mut().poll(&mut cx).is_pending());
    mgr.disconnected(handle, Status::SUCCESS).unwrap();
    mgr.connect(handle, Address::random([2; 6]), LeConnRole::Central, ConnParams::new())
        .unwrap();
    let new = block_on(mgr.accept(LeConnRole::Central, &[]));
    assert_ne!(old.index(), new.index());
    stack.controller.acl_limit.set(None);
    assert!(matches!(
        send.as_mut().poll(&mut cx),
        Poll::Ready(Err(BleHostError::BleHost(Error::Disconnected)))
    ));
    assert!(stack.controller.acl.borrow().is_empty());
    let Poll::Ready(Ok(new_lock)) = mgr.poll_acquire_acl_send_lock(handle, None) else {
        panic!("new link lock unchanged");
    };
    let Poll::Ready(Ok(_grant)) = new_lock.poll_request_to_send(4, None) else {
        panic!("new credits unchanged");
    };
}

/// Counts executor wakeups independently of the deliberately blocked transport.
#[derive(Default)]
struct AclWakeCount(
    /// Monotonic count observed by the polling test.
    std::sync::atomic::AtomicUsize,
);
impl std::task::Wake for AclWakeCount {
    /// Records one wake through the owned `self` handle without polling any future reentrantly.
    fn wake(self: std::sync::Arc<Self>) {
        self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    }
}

/// Terminal close, local close and last-owner drop wake a transport-blocked sender; already submitted prefixes remain untouched.
#[test]
fn pending_acl_transport_close_wakes_and_preserves_only_committed_prefix() {
    for prefix in [0, 1] {
        for close in 0..3 {
            let mut resources = HostResources::<DefaultPacketPool, 1, 1>::new();
            let stack = crate::new(ParamController::default(), &mut resources).build();
            let host = stack.host();
            host.state
                .initialized
                .init(InitialState {
                    acl_max: 2,
                    acl_total: 4,
                })
                .ok()
                .unwrap();
            let mgr = host.connections();
            mgr.set_link_credits(4);
            let handle = ConnHandle::new(1);
            mgr.connect(handle, Address::random([1; 6]), LeConnRole::Central, ConnParams::new())
                .unwrap();
            let mut owner = Some(block_on(mgr.accept(LeConnRole::Central, &[])));
            let mut sender = block_on(host.l2cap_pdu(handle)).unwrap();
            stack.controller.acl_limit.set(Some(prefix));
            let mut send = pin!(sender.send(&[1, 2, 3, 4]));
            let wakes = std::sync::Arc::new(AclWakeCount::default());
            let waker = Waker::from(wakes.clone());
            let mut cx = Context::from_waker(&waker);
            assert!(send.as_mut().poll(&mut cx).is_pending());
            assert_eq!(stack.controller.acl.borrow().len(), prefix);
            let before = wakes.0.load(std::sync::atomic::Ordering::SeqCst);
            match close {
                0 => mgr.disconnected(handle, Status::SUCCESS).unwrap(),
                1 => owner.as_ref().unwrap().disconnect(),
                _ => drop(owner.take()),
            }
            assert!(wakes.0.load(std::sync::atomic::Ordering::SeqCst) > before);
            assert!(matches!(
                send.as_mut().poll(&mut cx),
                Poll::Ready(Err(BleHostError::BleHost(Error::Disconnected)))
            ));
            assert_eq!(stack.controller.acl.borrow().len(), prefix);
            if prefix != 0 {
                assert_eq!(stack.controller.acl.borrow()[0], [1, 2]);
            }
        }
    }
}

/// Live transport resumption sends all fragments; canceling its capacity wait refunds only the unsubmitted fragment's credit.
#[test]
fn pending_acl_transport_live_resume_and_cancellation_balance_credits() {
    for cancel in [false, true] {
        let mut resources = HostResources::<DefaultPacketPool, 1, 1>::new();
        let stack = crate::new(ParamController::default(), &mut resources).build();
        let host = stack.host();
        host.state
            .initialized
            .init(InitialState {
                acl_max: 2,
                acl_total: 4,
            })
            .ok()
            .unwrap();
        let mgr = host.connections();
        mgr.set_link_credits(4);
        let handle = ConnHandle::new(1);
        mgr.connect(handle, Address::random([1; 6]), LeConnRole::Central, ConnParams::new())
            .unwrap();
        let _owner = block_on(mgr.accept(LeConnRole::Central, &[]));
        let mut sender = block_on(host.l2cap_pdu(handle)).unwrap();
        stack.controller.acl_limit.set(Some(1));
        {
            let mut send = pin!(sender.send(&[1, 2, 3, 4]));
            let mut cx = Context::from_waker(Waker::noop());
            assert!(send.as_mut().poll(&mut cx).is_pending());
            if !cancel {
                stack.controller.acl_limit.set(None);
                assert!(matches!(send.as_mut().poll(&mut cx), Poll::Ready(Ok(()))));
            }
        }
        let remaining = if cancel { 3 } else { 2 };
        assert_eq!(stack.controller.acl.borrow().len(), if cancel { 1 } else { 2 });
        drop(sender);
        let Poll::Ready(Ok(lock)) = mgr.poll_acquire_acl_send_lock(handle, None) else {
            panic!("lock released");
        };
        let Poll::Ready(Ok(_grant)) = lock.poll_request_to_send(remaining, None) else {
            panic!("only unsubmitted credits refunded");
        };
        assert!(!lock.has_link_credits(1));
    }
}
