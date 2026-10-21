//! ATT ingress behavior when the GATT feature is disabled; uses a fake controller and no devices.

use super::*;
use crate::mock_controller::MockController;
use crate::prelude::DefaultPacketPool;
use bt_hci::param::BdAddr;
use core::future::Future;
use core::pin::pin;
use core::task::{Context, Waker};
use embassy_futures::block_on;

#[test]
/// Commands/confirmations produce no response; unsupported read requests return ATT Request Not Supported with attribute handle zero and keep the link live.
fn no_gatt_commands_and_confirmations_are_silent_but_requests_get_unsupported() {
    for att in [&[0x52, 1, 0, 42][..], &[0x1e][..], &[0x0a, 1, 0][..]] {
        let mut resources = crate::HostResources::<DefaultPacketPool, 1, 1>::new();
        let stack = crate::new(MockController::new(), &mut resources).build();
        let host = BleHost::new(&stack.controller, &stack.host_state);
        let mgr = host.connections();
        mgr.connect(
            ConnHandle::new(7),
            Address::new(AddrKind::RANDOM, BdAddr::new([1; 6])),
            LeConnRole::Central,
            ConnParams::new(),
        )
        .unwrap();
        let Poll::Ready(conn) = mgr.poll_accept(LeConnRole::Central, &[], None) else {
            panic!()
        };
        let mut packet = [0; 8];
        packet[..4].copy_from_slice(&[att.len() as u8, 0, 4, 0]);
        packet[4..4 + att.len()].copy_from_slice(att);
        host.handle_acl(
            AclPacket::new(
                conn.handle(),
                AclPacketBoundary::FirstFlushable,
                AclBroadcastFlag::PointToPoint,
                &packet[..4 + att.len()],
            ),
            &DummyHandler,
        )
        .unwrap();
        if att[0] == 0x0a {
            let (_, response) = block_on(mgr.outbound());
            // Unsupported requests have no associated attribute, not HCI handle 7.
            assert_eq!(response.as_ref(), &[5, 0, 4, 0, 1, 0x0a, 0, 0, 6]);
        }
        assert!(pin!(mgr.outbound())
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop()))
            .is_pending());
        assert!(conn.is_connected());
        mgr.disconnected(conn.handle(), Status::SUCCESS).unwrap();
    }
}
