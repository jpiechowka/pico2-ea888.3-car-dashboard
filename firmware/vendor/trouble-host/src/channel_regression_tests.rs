extern crate std;

use super::*;
use crate::mock_controller::MockController;
use crate::prelude::{ConnParams, DefaultPacketPool};
use crate::{Address, HostResources, Stack};
use bt_hci::param::{AddrKind, BdAddr};
use core::future::Future;
use core::pin::pin;
use core::task::Waker;

fn with_link(
    f: impl FnOnce(
        &Stack<'_, MockController, DefaultPacketPool>,
        BleHost<'_, MockController, DefaultPacketPool>,
        ConnHandle,
    ),
) {
    let mut resources = HostResources::<DefaultPacketPool, 2, 2>::new();
    let stack = crate::new(MockController::new(), &mut resources).build();
    let host = stack.host();
    let conn = ConnHandle::new(1);
    host.connections()
        .connect(
            conn,
            Address::new(AddrKind::PUBLIC, BdAddr::new([0; 6])),
            LeConnRole::Central,
            ConnParams::new(),
        )
        .unwrap();
    let Poll::Ready(connection) = host.connections().poll_accept(LeConnRole::Central, &[], None) else {
        panic!()
    };
    f(&stack, host, conn);
    host.connections()
        .disconnected(conn, bt_hci::param::Status::SUCCESS)
        .unwrap();
    drop(connection);
}

#[test]
fn cancelled_create_releases_slot_before_initial_signal_completes() {
    with_link(|_, host, conn| {
        let config = L2capChannelConfig::default();
        {
            let mut create = pin!(host.channels().create(conn, 0x80, &config, host));
            assert!(create
                .as_mut()
                .poll(&mut Context::from_waker(Waker::noop()))
                .is_pending());
            assert_eq!(
                host.channels().channel(ChannelIndex(0)).state,
                ChannelState::Connecting(1)
            );
        }
        assert_eq!(
            host.channels().channel(ChannelIndex(0)).state,
            ChannelState::Disconnected
        );
    });
}

#[test]
fn pending_accept_errors_and_cancellation_release_reference() {
    for invalid_config in [true, false] {
        with_link(|stack, host, conn| {
            let index = host
                .channels()
                .alloc(conn, Some(0x41), |s| {
                    s.state = ChannelState::PeerConnecting(1);
                    s.peer_mtu = 64;
                    s.peer_mps = 23;
                    s.inc_ref();
                })
                .unwrap();
            let pending = L2capPendingConnection::new(index, host.channels(), conn);
            let config = L2capChannelConfig {
                mps: invalid_config.then_some(u16::MAX),
                ..Default::default()
            };
            {
                let mut accept = pin!(pending.accept(stack, &config));
                let result = accept.as_mut().poll(&mut Context::from_waker(Waker::noop()));
                if invalid_config {
                    assert!(matches!(result, Poll::Ready(Err(_))));
                } else {
                    assert!(result.is_pending());
                }
            }
            assert_eq!(host.channels().channel(index).refcount, 0);
            assert_ne!(host.channels().channel(index).state, ChannelState::Connected);
        });
    }
}

#[test]
fn dropped_pending_after_disconnect_releases_reference() {
    with_link(|_, host, conn| {
        let index = host
            .channels()
            .alloc(conn, Some(0x41), |s| {
                s.state = ChannelState::PeerConnecting(1);
                s.inc_ref();
            })
            .unwrap();
        let pending = L2capPendingConnection::new(index, host.channels(), conn);
        host.channels().disconnected(conn).unwrap();
        drop(pending);
        assert_eq!(host.channels().channel(index).refcount, 0);
    });
}

#[test]
#[cfg(not(feature = "l2cap-sdu-reassembly-optimization"))]
fn truncated_first_dynamic_frame_is_rejected_without_panicking() {
    for len in [0, 1] {
        with_link(|_, host, conn| {
            let index = host
                .channels()
                .alloc(conn, Some(0x41), |s| {
                    s.state = ChannelState::Connected;
                    s.mtu = 64;
                    s.mps = 23;
                    s.flow_control = CreditFlowControl::new(CreditFlowPolicy::Every(1), 2);
                })
                .unwrap();
            let packet = DefaultPacketPool::allocate().unwrap();
            assert!(host.channels().dispatch(conn, BASE_ID, Pdu::new(packet, len)).is_err());
            assert_eq!(host.channels().channel(index).state, ChannelState::Disconnecting);
        });
    }
}

#[test]
fn successful_response_rejects_invalid_peer_parameters() {
    for (cid, mtu, mps) in [
        (0x3f, 23, 23),
        (0x80, 23, 23),
        (0x40, 0, 23),
        (0x40, 23, 0),
        (0x40, 23, 1),
        (0x40, 23, 65534),
    ] {
        with_link(|_, host, conn| {
            let index = host
                .channels()
                .alloc(conn, None, |s| s.state = ChannelState::Connecting(1))
                .unwrap();
            let response = LeCreditConnRes {
                dcid: cid,
                mtu,
                mps,
                credits: 1,
                result: LeCreditConnResultCode::Success,
            };
            assert!(host.channels().handle_connect_response(conn, 1, &response).is_err());
            assert_ne!(host.channels().channel(index).state, ChannelState::Connected);
        });
    }
}

#[test]
fn disconnect_signal_cannot_target_another_link_or_wrong_cid_pair() {
    with_link(|_, host, conn| {
        let index = host
            .channels()
            .alloc(conn, Some(0x41), |s| {
                s.state = ChannelState::Connected;
            })
            .unwrap();
        // Identifier, length, DCID and SCID are all well formed. The first
        // request belongs to another logical link; the second has a wrong SCID.
        for (sender, scid) in [(ConnHandle::new(2), 0x41), (conn, 0x42)] {
            let signal = [6, 9, 4, 0, 0x40, 0, scid, 0];
            let _ = host.channels().signal(sender, &signal, host.connections());
            assert_eq!(host.channels().channel(index).state, ChannelState::Connected);
        }
        host.channels()
            .signal(conn, &[6, 9, 4, 0, 0x40, 0, 0x41, 0], host.connections())
            .unwrap();
        assert_eq!(host.channels().channel(index).state, ChannelState::PeerDisconnecting(9));
    });
}

#[test]
fn valid_credit_responses_preserve_independent_link_cids_and_credits() {
    with_link(|_, host, conn| {
        let other = ConnHandle::new(2);
        let old = host
            .channels()
            .alloc(other, Some(0x41), |s| s.state = ChannelState::Connected)
            .unwrap();
        let index = host
            .channels()
            .alloc(conn, None, |s| s.state = ChannelState::Connecting(1))
            .unwrap();
        let response = LeCreditConnRes {
            dcid: 0x41,
            mtu: 64,
            mps: 23,
            credits: 3,
            result: LeCreditConnResultCode::Success,
        };
        host.channels().handle_connect_response(conn, 1, &response).unwrap();
        let channel = host.channels().channel(index);
        assert_eq!(channel.state, ChannelState::Connected);
        assert_eq!((channel.peer_mtu, channel.peer_mps, channel.peer_credits), (64, 23, 3));
        drop(channel);
        host.channels()
            .handle_credit_flow(conn, &LeCreditFlowInd { cid: 0x41, credits: 2 })
            .unwrap();
        assert_eq!(host.channels().channel(index).peer_credits, 5);
        assert_eq!(host.channels().channel(old).peer_credits, 0);
    });
}

#[test]
fn foreign_link_data_cannot_consume_credits_or_disconnect_channel() {
    with_link(|_, host, conn| {
        let index = host
            .channels()
            .alloc(conn, Some(0x41), |s| {
                s.state = ChannelState::Connected;
                s.mtu = 64;
                s.mps = 23;
                s.flow_control = CreditFlowControl::new(CreditFlowPolicy::Every(1), 2);
            })
            .unwrap();
        let other = ConnHandle::new(2);
        assert!(host.channels().received(other, BASE_ID, 1).is_err());
        assert!(host.channels().check_pdu_len(other, BASE_ID, 99).is_err());
        assert!(host.channels().check_sdu_len(other, BASE_ID, 99).is_err());
        host.channels().disconnect_by_cid(other, BASE_ID);
        let packet = DefaultPacketPool::allocate().unwrap();
        assert!(host.channels().dispatch(other, BASE_ID, Pdu::new(packet, 0)).is_err());
        assert_eq!(host.channels().channel(index).flow_control.available(), 2);
        assert_eq!(host.channels().channel(index).state, ChannelState::Connected);
        host.channels().received(conn, BASE_ID, 1).unwrap();
        assert_eq!(host.channels().channel(index).flow_control.available(), 1);
        host.channels().disconnect_by_cid(conn, BASE_ID);
        host.channels()
            .handle_disconnect_response(other, BASE_ID, 0x41)
            .unwrap();
        host.channels().handle_disconnect_response(conn, BASE_ID, 0x42).unwrap();
        assert_eq!(host.channels().channel(index).state, ChannelState::Disconnecting);
        host.channels().handle_disconnect_response(conn, BASE_ID, 0x41).unwrap();
        assert_eq!(host.channels().channel(index).state, ChannelState::Disconnected);
    });
}

#[test]
fn incoming_credit_request_rejects_bad_parameters_without_allocating() {
    for (cid, mtu, mps, result) in [
        (0x3f, 23, 23, LeCreditConnResultCode::InvalidSourceId),
        (0x80, 23, 23, LeCreditConnResultCode::InvalidSourceId),
        (0x40, 22, 23, LeCreditConnResultCode::UnacceptableParameters),
        (0x40, 23, 1, LeCreditConnResultCode::UnacceptableParameters),
        (0x40, 23, 65535, LeCreditConnResultCode::UnacceptableParameters),
    ] {
        with_link(|_, host, conn| {
            host.channels().register_spsm(0x80);
            host.connections().set_l2cap_listening(0, true);
            host.channels()
                .handle_connect_request(
                    conn,
                    7,
                    &LeCreditConnReq {
                        spsm: 0x80,
                        scid: cid,
                        mtu,
                        mps,
                        credits: 0,
                    },
                    host.connections(),
                )
                .unwrap();
            assert_eq!(
                host.channels().channel(ChannelIndex(0)).state,
                ChannelState::Disconnected
            );
            let Poll::Ready((_, reply)) = pin!(host.connections().outbound())
                .as_mut()
                .poll(&mut Context::from_waker(Waker::noop()))
            else {
                panic!()
            };
            assert_eq!(&reply.as_ref()[16..18], &(result as u16).to_le_bytes());
        });
    }
}

#[test]
fn pending_connection_reports_peer_mtu_before_acceptance() {
    with_link(|_, host, conn| {
        let index = host
            .channels()
            .alloc(conn, Some(0x41), |s| {
                s.state = ChannelState::PeerConnecting(1);
                s.peer_mtu = 64;
                s.inc_ref();
            })
            .unwrap();
        let pending = L2capPendingConnection::new(index, host.channels(), conn);
        assert_eq!(pending.mtu(), 64);
    });
}
