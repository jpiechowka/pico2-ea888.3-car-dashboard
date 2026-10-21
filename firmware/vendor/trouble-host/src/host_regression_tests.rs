//! Ingress resource failure, terminal acknowledgment and canceled connection establishment regressions; no controller I/O.

extern crate std;
use bt_hci::param::BdAddr;
use embassy_futures::block_on;

use super::*;
use crate::mock_controller::MockController;
use crate::prelude::DefaultPacketPool;
use crate::HostResources;

/// Injects L2CAP `bytes` into a fake accepted link. `fill_queue`/`exhaust_pool` force bounded-resource pressure; `should_fail` selects close/no-delivery assertions versus exact client bytes. Always completes terminal cleanup; fixture/assertion failure panics.
fn ingress(bytes: &[u8], fill_queue: bool, exhaust_pool: bool, should_fail: bool) {
    let mut resources = HostResources::<DefaultPacketPool, 1, 1>::new();
    let stack = crate::new(MockController::new(), &mut resources).build();
    let host = BleHost::new(&stack.controller, stack.host_state);
    let mgr = host.connections();
    let handle = ConnHandle::new(0);
    mgr.connect(
        handle,
        Address::new(AddrKind::RANDOM, BdAddr::new([1; 6])),
        LeConnRole::Central,
        ConnParams::new(),
    )
    .unwrap();
    let Poll::Ready(conn) = mgr.poll_accept(LeConnRole::Central, &[], None) else {
        panic!()
    };
    if fill_queue {
        for _ in 0..crate::config::L2CAP_RX_QUEUE_SIZE {
            // Small pools may fill before the receive queue. Either resource limit
            // must fail closed, and the fixture must still reach normal cleanup.
            let Some(packet) = DefaultPacketPool::allocate() else {
                break;
            };
            mgr.post_gatt_client(handle, Pdu::new(packet, 1)).unwrap();
        }
    }
    let mut held = std::vec::Vec::new();
    if exhaust_pool {
        while let Some(packet) = DefaultPacketPool::allocate() {
            held.push(packet);
        }
    }
    let packet = AclPacket::new(
        handle,
        AclPacketBoundary::FirstFlushable,
        AclBroadcastFlag::PointToPoint,
        bytes,
    );
    let result = host.handle_acl(packet, &DummyHandler);
    if should_fail {
        assert!(result.is_err());
        assert!(
            mgr.poll_disconnecting(None).is_ready(),
            "ingress loss must request disconnect"
        );
        assert!(
            block_on(conn.next_gatt_client()).is_none(),
            "no bytes after failed epoch"
        );
    } else {
        assert!(result.is_ok());
        assert!(mgr.poll_disconnecting(None).is_pending());
        assert_eq!(block_on(conn.next_gatt_client()).unwrap().as_ref(), &bytes[4..]);
    }
    mgr.disconnected(handle, Status::SUCCESS).unwrap();
}

#[test]
/// A well-formed notification delivers its exact ATT bytes without requesting disconnect.
fn valid_notification_is_delivered() {
    ingress(&[4, 0, 4, 0, att::ATT_HANDLE_VALUE_NTF, 1, 0, 42], false, false, false);
}

#[test]
/// Missing ATT opcode or truncated notification fields close the bearer before client delivery.
fn empty_and_short_notification_disconnect() {
    ingress(&[0, 0, 4, 0], false, false, true);
    ingress(&[1, 0, 4, 0, att::ATT_HANDLE_VALUE_NTF], false, false, true);
}

#[test]
/// Both unsolicited value opcodes close the link when the bounded client queue cannot retain their packet.
fn notification_and_indication_queue_exhaustion_disconnect() {
    for opcode in [att::ATT_HANDLE_VALUE_NTF, att::ATT_HANDLE_VALUE_IND] {
        ingress(&[4, 0, 4, 0, opcode, 1, 0, 42], true, false, true);
    }
}

#[test]
/// Pool exhaustion rejects ingress before ATT decoding and requests disconnect.
fn packet_pool_exhaustion_disconnects_before_att_decode() {
    ingress(&[4, 0, 4, 0, att::ATT_HANDLE_VALUE_NTF, 1, 0, 42], false, true, true);
}

#[test]
/// Short L2CAP headers and first fragments exceeding reassembly capacity reject without exposing partial data.
fn truncated_acl_and_oversized_reassembly_disconnect() {
    ingress(&[1], false, false, true);
    let mut bytes = std::vec![0; DefaultPacketPool::MTU + 5];
    bytes[..4].copy_from_slice(&[0xFF, 0xFF, 4, 0]);
    ingress(&bytes, false, false, true);
}

#[test]
/// A successful controller completion racing requested/sent cancellation stays unaccepted and nonidle until terminal disconnect.
fn canceled_connection_success_is_quarantined_until_terminal_event() {
    for cancel_sent in [false, true] {
        let mut resources = HostResources::<DefaultPacketPool, 1, 1>::new();
        let stack = crate::new(MockController::new(), &mut resources).build();
        let host = BleHost::new(&stack.controller, stack.host_state);
        let command = host.connect_command_state();
        block_on(command.request(false));
        command.cancel(false);
        if cancel_sent {
            command.cancel_started();
        }
        assert!(!command.is_idle());
        assert!(!stack.connections_idle());
        assert!(host.handle_connection(
            Status::SUCCESS,
            ConnHandle::new(0),
            Address::new(AddrKind::RANDOM, BdAddr::new([1; 6])),
            LeConnRole::Central,
            ConnParams::new(),
            #[cfg(feature = "security")]
            ResolvablePrivateAddrs::none()
        ));
        assert!(command.is_idle());
        assert!(host
            .connections()
            .poll_accept(LeConnRole::Central, &[], None)
            .is_pending());
        assert!(host.connections().poll_disconnecting(None).is_ready());
        assert!(!stack.connections_idle());
        host.connections()
            .disconnected(ConnHandle::new(0), Status::SUCCESS)
            .unwrap();
        assert!(stack.connections_idle());
    }
}

#[test]
/// Canceling after successful establishment but before acceptance schedules close and prevents handing out the link.
fn successful_but_unaccepted_link_is_quarantined_when_caller_cancels() {
    let mut resources = HostResources::<DefaultPacketPool, 1, 1>::new();
    let stack = crate::new(MockController::new(), &mut resources).build();
    let host = BleHost::new(&stack.controller, stack.host_state);
    block_on(host.connect_command_state().request(false));
    assert!(host.handle_connection(
        Status::SUCCESS,
        ConnHandle::new(0),
        Address::new(AddrKind::RANDOM, BdAddr::new([1; 6])),
        LeConnRole::Central,
        ConnParams::new(),
        #[cfg(feature = "security")]
        ResolvablePrivateAddrs::none()
    ));
    host.connect_command_state().cancel(false);
    assert!(host.connections().cancel_unaccepted_central());
    assert!(host
        .connections()
        .poll_accept(LeConnRole::Central, &[], None)
        .is_pending());
    assert!(host.connections().poll_disconnecting(None).is_ready());
}

#[test]
/// Requested and submitted close remain nonterminal; only the controller completion makes is_disconnected true.
fn disconnect_status_requires_controller_event() {
    let mut resources = HostResources::<DefaultPacketPool, 1, 1>::new();
    let stack = crate::new(MockController::new(), &mut resources).build();
    let mgr = &stack.host_state.connections;
    let handle = ConnHandle::new(0);
    mgr.connect(
        handle,
        Address::new(AddrKind::RANDOM, BdAddr::new([1; 6])),
        LeConnRole::Central,
        ConnParams::new(),
    )
    .unwrap();
    let Poll::Ready(conn) = mgr.poll_accept(LeConnRole::Central, &[], None) else {
        panic!()
    };
    assert!(!conn.is_disconnected());
    conn.disconnect();
    assert!(!conn.is_disconnected());
    let Poll::Ready(request) = mgr.poll_disconnecting(None) else {
        panic!()
    };
    request.confirm();
    assert!(!conn.is_disconnected());
    mgr.disconnected(handle, Status::SUCCESS).unwrap();
    assert!(conn.is_disconnected());
}

/// Dropping an entire stack returns inbound queue and partial-reassembly packets before the backing resources are reused.
#[test]
fn stack_drop_releases_borrowed_inbound_packet_storage() {
    let available = || {
        let mut packets = std::vec::Vec::new();
        while let Some(packet) = DefaultPacketPool::allocate() {
            packets.push(packet);
        }
        packets.len()
    };
    let before = available();
    let mut resources = HostResources::<DefaultPacketPool, 1, 1>::new();
    for _ in 0..2 {
        {
            let stack = crate::new(MockController::new(), &mut resources).build();
            let host = stack.host();
            let handle = ConnHandle::new(0);
            host.connections()
                .connect(
                    handle,
                    Address::new(AddrKind::PUBLIC, BdAddr::new([1; 6])),
                    LeConnRole::Central,
                    ConnParams::new(),
                )
                .unwrap();
            host.connections()
                .post_gatt(handle, Pdu::new(DefaultPacketPool::allocate().unwrap(), 1))
                .unwrap();
            host.connections()
                .post_gatt_client(handle, Pdu::new(DefaultPacketPool::allocate().unwrap(), 1))
                .unwrap();
            host.connections()
                .reassembly(handle, |assembly| {
                    assembly.init(4, 10, DefaultPacketPool::allocate().unwrap())
                })
                .unwrap();
            assert_eq!(available(), before - 3);
        }
        assert_eq!(available(), before);
    }
}
