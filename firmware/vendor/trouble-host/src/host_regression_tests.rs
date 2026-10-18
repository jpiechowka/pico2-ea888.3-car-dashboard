extern crate std;
use bt_hci::param::BdAddr;
use embassy_futures::block_on;

use super::*;
use crate::mock_controller::MockController;
use crate::prelude::DefaultPacketPool;
use crate::HostResources;

fn ingress(bytes: &[u8], fill_queue: bool, exhaust_pool: bool, should_fail: bool) {
    let mut resources = HostResources::<DefaultPacketPool, 1, 1>::new();
    let stack = crate::new(MockController::new(), &mut resources).build();
    let host = BleHost::new(&stack.controller, &stack.host_state);
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
            mgr.post_gatt_client(handle, Pdu::new(DefaultPacketPool::allocate().unwrap(), 1))
                .unwrap();
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
fn valid_notification_is_delivered() {
    ingress(&[4, 0, 4, 0, att::ATT_HANDLE_VALUE_NTF, 1, 0, 42], false, false, false);
}

#[test]
fn empty_and_short_notification_disconnect() {
    ingress(&[0, 0, 4, 0], false, false, true);
    ingress(&[1, 0, 4, 0, att::ATT_HANDLE_VALUE_NTF], false, false, true);
}

#[test]
fn notification_and_indication_queue_exhaustion_disconnect() {
    for opcode in [att::ATT_HANDLE_VALUE_NTF, att::ATT_HANDLE_VALUE_IND] {
        ingress(&[4, 0, 4, 0, opcode, 1, 0, 42], true, false, true);
    }
}

#[test]
fn packet_pool_exhaustion_disconnects_before_att_decode() {
    ingress(&[4, 0, 4, 0, att::ATT_HANDLE_VALUE_NTF, 1, 0, 42], false, true, true);
}

#[test]
fn truncated_acl_and_oversized_reassembly_disconnect() {
    ingress(&[1], false, false, true);
    let mut bytes = std::vec![0; DefaultPacketPool::MTU + 5];
    bytes[..4].copy_from_slice(&[0xFF, 0xFF, 4, 0]);
    ingress(&bytes, false, false, true);
}

#[test]
fn canceled_connection_success_is_quarantined_until_terminal_event() {
    for cancel_sent in [false, true] {
        let mut resources = HostResources::<DefaultPacketPool, 1, 1>::new();
        let stack = crate::new(MockController::new(), &mut resources).build();
        let host = BleHost::new(&stack.controller, &stack.host_state);
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
fn successful_but_unaccepted_link_is_quarantined_when_caller_cancels() {
    let mut resources = HostResources::<DefaultPacketPool, 1, 1>::new();
    let stack = crate::new(MockController::new(), &mut resources).build();
    let host = BleHost::new(&stack.controller, &stack.host_state);
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
