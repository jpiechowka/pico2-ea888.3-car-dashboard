//! Host-only dynamic-channel lifetime, credit, peer-identity and malformed-frame regressions.
extern crate std;

use super::*;
use crate::mock_controller::MockController;
use crate::prelude::{ConnParams, DefaultPacketPool};
use crate::{Address, HostResources, Stack};
use bt_hci::param::{AddrKind, BdAddr};
use core::future::Future;
use core::pin::pin;
use core::task::Waker;

/// Runs `f` with a stack, host and accepted central link handle, then reports disconnection and drops the retained link owner; fixture setup failures panic.
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
/// Dropping the pending create future restores its Connecting slot to Disconnected.
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
/// Invalid config and canceled acceptance both release the pending connection reference without publishing Connected.
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
/// A pending accept token still releases its reference after link teardown.
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
/// Zero/one-byte first frames cannot supply the SDU length and initiate channel disconnection.
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
/// A peer success result does not admit illegal CID, MTU or MPS values.
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
/// Only a matching link and local/remote CID pair may move the channel to peer-disconnecting.
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
/// Negotiation and added credits affect only the matching link, even when another link reuses the peer CID.
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
/// Cross-link receive/length/disconnect operations preserve the original channel and its credit balance.
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
/// Malformed peer proposals produce the specified refusal code without occupying a channel slot.
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
/// A pending incoming token exposes negotiated peer MTU before acceptance.
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

/// The first K-frame cannot already exceed its declared SDU size; reject and close immediately.
#[test]
#[cfg(not(feature = "l2cap-sdu-reassembly-optimization"))]
fn first_dynamic_fragment_cannot_exceed_declared_sdu() {
    for (declared, supplied, accepted) in [
        (0u16, 0usize, true),
        (0, 1, false),
        (1, 1, true),
        (1, 2, false),
        (2, 1, true),
    ] {
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
            let mut packet = DefaultPacketPool::allocate().unwrap();
            packet.as_mut()[..2].copy_from_slice(&declared.to_le_bytes());
            packet.as_mut()[2..2 + supplied].fill(0xa5);
            let result = host.channels().dispatch(conn, BASE_ID, Pdu::new(packet, 2 + supplied));
            assert_eq!(result.is_ok(), accepted, "declared={declared}, supplied={supplied}");
            if !accepted {
                let channel = host.channels().channel(index);
                assert_eq!(channel.state, ChannelState::Disconnecting);
                assert!(!channel.reassembly.in_progress());
            }
        });
    }
}

/// Equal numeric CIDs in independent stacks do not make their reader/writer halves interchangeable.
#[test]
fn merge_requires_the_same_manager_as_well_as_index() {
    with_link(|_, first, first_conn| {
        with_link(|_, second, second_conn| {
            let first_index = first
                .channels()
                .alloc(first_conn, Some(0x41), |slot| {
                    slot.state = ChannelState::Connected;
                    slot.inc_ref();
                })
                .unwrap();
            let second_index = second
                .channels()
                .alloc(second_conn, Some(0x41), |slot| {
                    slot.state = ChannelState::Connected;
                    slot.inc_ref();
                })
                .unwrap();
            assert_eq!(first_index, second_index);
            let (first_writer, first_reader) = L2capChannel::new(first_index, first.channels()).split();
            let (second_writer, second_reader) = L2capChannel::new(second_index, second.channels()).split();
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                L2capChannel::merge(first_writer, second_reader)
            }));
            assert!(result.is_err(), "accepted halves from different stacks");
            assert_eq!(first.channels().channel(first_index).refcount, 1);
            assert_eq!(second.channels().channel(second_index).refcount, 1);
            drop(first_reader);
            drop(second_writer);
        });
    });
    with_link(|_, host, conn| {
        let index = host
            .channels()
            .alloc(conn, Some(0x41), |slot| {
                slot.state = ChannelState::Connected;
                slot.inc_ref();
            })
            .unwrap();
        let (writer, reader) = L2capChannel::new(index, host.channels()).split();
        let channel = L2capChannel::merge(writer, reader);
        assert_eq!(host.channels().channel(index).refcount, 1);
        drop(channel);
        assert_eq!(host.channels().channel(index).refcount, 0);
    });
}

/// A maximum-length SDU includes its two-byte prefix in K-frame admission before controller I/O.
#[test]
fn maximum_sdu_waits_for_all_segment_credits() {
    with_link(|_, host, conn| {
        let index = host
            .channels()
            .alloc(conn, Some(0x41), |s| {
                s.state = ChannelState::Connected;
                s.peer_mtu = u16::MAX;
                s.peer_mps = 255;
                s.peer_credits = 257;
            })
            .unwrap();
        let payload = std::vec![0; u16::MAX as usize];
        let mut packet = [0; 259];
        {
            let mut send = pin!(host.channels().send(index, &payload, &mut packet, host));
            assert!(send.as_mut().poll(&mut Context::from_waker(Waker::noop())).is_pending());
            assert_eq!(
                host.channels().channel(index).peer_credits,
                257,
                "258 K-frames need 258 credits before reservation"
            );
        }
        assert_eq!(host.channels().channel(index).peer_credits, 257);
    });
}

/// Credit admission matches the actual first-prefix/remaining-chunk segmentation at wire endpoints.
#[test]
fn sdu_credit_count_matches_segments() {
    let payload = std::vec![0; u16::MAX as usize];
    for mps in [23u16, 24, 255, 256, 65533] {
        for length in [0u16, 1, mps - 2, mps - 1, mps, 65533, 65534, 65535] {
            let bytes = &payload[..usize::from(length)];
            let (_, remaining) = bytes.split_at(bytes.len().min(usize::from(mps) - 2));
            assert_eq!(
                usize::from(sdu_credits(length, mps)),
                1 + remaining.chunks(usize::from(mps)).count()
            );
        }
    }
}

/// Split/merge use only their net reference change, including the u8 ownership ceiling.
#[test]
fn split_merge_preserve_reference_budget_at_limit() {
    for merging in [false, true] {
        with_link(|_, host, conn| {
            let index = host
                .channels()
                .alloc(conn, Some(0x41), |s| {
                    s.state = ChannelState::Connected;
                    s.inc_ref();
                })
                .unwrap();
            let (mut writer, reader) = L2capChannel::new(index, host.channels()).split();
            let refs: std::vec::Vec<_> = (0..252).map(|_| writer.channel_ref()).collect();
            if merging {
                let extra = writer.channel_ref();
                assert_eq!(host.channels().channel(index).refcount, 255);
                let channel = L2capChannel::merge(writer, reader);
                assert_eq!(host.channels().channel(index).refcount, 254);
                drop(channel);
                drop(extra);
            } else {
                let channel = L2capChannel::merge(writer, reader);
                assert_eq!(host.channels().channel(index).refcount, 253);
                // One extra retained reference makes a legitimate split reach255 exactly.
                host.channels().inc_ref(index);
                let (writer, reader) = channel.split();
                assert_eq!(host.channels().channel(index).refcount, 255);
                host.channels().dec_ref(index);
                drop(writer);
                drop(reader);
            }
            drop(refs);
            assert_eq!(host.channels().channel(index).refcount, 0);
        });
    }
}

/// Cancellation refunds cannot overflow when a peer replenishes credits while a send is reserved.
#[test]
fn cancelled_credit_reservation_rejects_refund_overflow() {
    with_link(|_, host, conn| {
        let index = host
            .channels()
            .alloc(conn, Some(0x41), |s| {
                s.state = ChannelState::Connected;
                s.peer_credits = 1;
            })
            .unwrap();
        let Poll::Ready(Ok(grant)) = host.channels().poll_request_to_send(index, 1, None) else {
            panic!()
        };
        host.channels()
            .handle_credit_flow(
                conn,
                &LeCreditFlowInd {
                    cid: 0x41,
                    credits: u16::MAX,
                },
            )
            .unwrap();
        drop(grant);
        assert_eq!(host.channels().channel(index).state, ChannelState::Disconnecting);
        assert!(host.channels().poll_disconnecting(None).is_ready());
    });
}

/// Partial confirmation refunds only unsent frames; exact u16 capacity succeeds and closed slots stay closed.
#[test]
fn credit_refund_preserves_valid_and_closed_balances() {
    for close in [false, true] {
        with_link(|_, host, conn| {
            let index = host
                .channels()
                .alloc(conn, Some(0x41), |s| {
                    s.state = ChannelState::Connected;
                    s.peer_credits = 2;
                })
                .unwrap();
            let Poll::Ready(Ok(mut grant)) = host.channels().poll_request_to_send(index, 2, None) else {
                panic!()
            };
            grant.confirm(1);
            host.channels()
                .handle_credit_flow(
                    conn,
                    &LeCreditFlowInd {
                        cid: 0x41,
                        credits: u16::MAX - 1,
                    },
                )
                .unwrap();
            if close {
                host.channels().channel_mut(index).close();
            }
            drop(grant);
            let slot = host.channels().channel(index);
            assert_eq!(slot.peer_credits, if close { 0 } else { u16::MAX });
            assert_eq!(
                slot.state,
                if close {
                    ChannelState::Disconnected
                } else {
                    ChannelState::Connected
                }
            );
        });
    }
}

/// Equal slot numbers in two stacks cannot redirect a channel read to the other peer's queue.
#[test]
fn channel_receive_rejects_foreign_stack_before_dequeue() {
    with_link(|_, first, first_conn| {
        with_link(|second_stack, second, second_conn| {
            let first_index = first
                .channels()
                .alloc(first_conn, Some(0x41), |s| {
                    s.state = ChannelState::Connected;
                    s.inc_ref();
                })
                .unwrap();
            let second_index = second
                .channels()
                .alloc(second_conn, Some(0x41), |s| {
                    s.state = ChannelState::Connected;
                })
                .unwrap();
            let mut packet = DefaultPacketPool::allocate().unwrap();
            packet.as_mut()[0] = 0x42;
            second
                .channels()
                .channel(second_index)
                .inbound
                .try_send(Pdu::new(packet, 1))
                .unwrap();
            let mut channel = L2capChannel::new(first_index, first.channels());
            let mut output = [0xa5];
            {
                let mut receive = pin!(channel.receive(second_stack, &mut output));
                assert!(matches!(
                    receive.as_mut().poll(&mut Context::from_waker(Waker::noop())),
                    Poll::Ready(Err(BleHostError::BleHost(Error::InvalidValue)))
                ));
            }
            assert_eq!(output, [0xa5]);
            assert!(matches!(
                second
                    .channels()
                    .channel(second_index)
                    .inbound
                    .poll_receive(&mut Context::from_waker(Waker::noop())),
                Poll::Ready(Some(_))
            ));
        });
    });
}

/// A link command cannot target an unrelated controller merely because its handle number matches.
#[test]
fn connection_command_rejects_foreign_stack_before_io() {
    with_link(|_, first, _| {
        with_link(|second_stack, _, _| {
            let connection = first.connections().connections().next().unwrap();
            let mut rssi = pin!(connection.rssi(second_stack));
            assert!(matches!(
                rssi.as_mut().poll(&mut Context::from_waker(Waker::noop())),
                Poll::Ready(Err(BleHostError::BleHost(Error::InvalidValue)))
            ));
        });
    });
}

/// Wrong-stack pending acceptance releases only its original token and leaves the unrelated slot untouched.
#[test]
fn pending_accept_rejects_foreign_stack_before_reference_transfer() {
    with_link(|_, first, first_conn| {
        with_link(|second_stack, second, second_conn| {
            let first_index = first
                .channels()
                .alloc(first_conn, Some(0x41), |s| {
                    s.state = ChannelState::PeerConnecting(1);
                    s.inc_ref();
                })
                .unwrap();
            let second_index = second
                .channels()
                .alloc(second_conn, Some(0x41), |s| {
                    s.state = ChannelState::Connected;
                    s.inc_ref();
                })
                .unwrap();
            let second_owner = L2capChannel::new(second_index, second.channels());
            {
                let pending = L2capPendingConnection::new(first_index, first.channels(), first_conn);
                let config = L2capChannelConfig::default();
                let mut accept = pin!(pending.accept(second_stack, &config));
                assert!(matches!(
                    accept.as_mut().poll(&mut Context::from_waker(Waker::noop())),
                    Poll::Ready(Err(BleHostError::BleHost(Error::InvalidValue)))
                ));
            }
            assert_eq!(first.channels().channel(first_index).refcount, 0);
            assert_eq!(second.channels().channel(second_index).refcount, 1);
            assert_eq!(second.channels().channel(second_index).state, ChannelState::Connected);
            drop(second_owner);
        });
    });
}

/// Listener and outgoing-channel constructors reject a foreign link before changing either stack.
#[test]
fn channel_constructors_reject_foreign_connection_identity() {
    with_link(|_, first, first_conn| {
        with_link(|second_stack, second, _| {
            let connection = first.connections().connections().next().unwrap();
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                L2capChannel::listen(second_stack, &connection)
            }));
            assert!(result.is_err());
            assert!(!first.connections().is_l2cap_listening(first_conn));
            let config = L2capChannelConfig::default();
            let mut create = pin!(L2capChannel::create(second_stack, &connection, 0x80, &config));
            assert!(matches!(
                create.as_mut().poll(&mut Context::from_waker(Waker::noop())),
                Poll::Ready(Err(BleHostError::BleHost(Error::InvalidValue)))
            ));
            assert_eq!(
                second.channels().channel(ChannelIndex(0)).state,
                ChannelState::Disconnected
            );
        });
    });
}

/// Two peer requests may be retained independently; polling skips the token already handed to the caller.
#[test]
fn next_pending_skips_already_claimed_requests() {
    with_link(|_, host, conn| {
        for peer in [0x41, 0x42] {
            host.channels()
                .alloc(conn, Some(peer), |s| {
                    s.state = ChannelState::PeerConnecting(1);
                })
                .unwrap();
        }
        let first = {
            let mut next = pin!(host.channels().next_pending(conn, host.connections()));
            let Poll::Ready(Ok(token)) = next.as_mut().poll(&mut Context::from_waker(Waker::noop())) else {
                panic!()
            };
            token
        };
        let second = {
            let mut next = pin!(host.channels().next_pending(conn, host.connections()));
            let Poll::Ready(Ok(token)) = next.as_mut().poll(&mut Context::from_waker(Waker::noop())) else {
                panic!()
            };
            token
        };
        assert_eq!(host.channels().channel(ChannelIndex(0)).refcount, 1);
        assert_eq!(host.channels().channel(ChannelIndex(1)).refcount, 1);
        {
            let mut next = pin!(host.channels().next_pending(conn, host.connections()));
            assert!(next.as_mut().poll(&mut Context::from_waker(Waker::noop())).is_pending());
        }
        drop(first);
        drop(second);
        assert_eq!(host.channels().channel(ChannelIndex(0)).refcount, 0);
        assert_eq!(host.channels().channel(ChannelIndex(1)).refcount, 0);
    });
}

/// Duplicate listener construction fails before clearing or replacing the original listener's shared admission flag.
#[test]
fn duplicate_listener_preserves_original_admission() {
    with_link(|stack, host, handle| {
        let connection = host.connections().get_connected_handle(handle).unwrap();
        let first = L2capChannel::listen(stack, &connection);
        let second = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            L2capChannel::listen(stack, &connection)
        }));
        assert!(second.is_err(), "duplicate listener must be rejected");
        assert!(host.connections().is_l2cap_listening(handle));
        drop(first);
        assert!(!host.connections().is_l2cap_listening(handle));
        let replacement = L2capChannel::listen(stack, &connection);
        assert!(host.connections().is_l2cap_listening(handle));
        drop(replacement);
        assert!(!host.connections().is_l2cap_listening(handle));
    });
}

/// Counts wakeups issued by channel teardown without polling the listener reentrantly.
#[derive(Default)]
struct ListenerWakeCount(
    /// Number of wake calls observed by the fixture.
    std::sync::atomic::AtomicUsize,
);
impl std::task::Wake for ListenerWakeCount {
    /// Records a wake through owned `self`; no other state changes.
    fn wake(self: std::sync::Arc<Self>) {
        self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    }
}

/// Exercises old listener `action` (0=next, 1=accept, 2=drop) after a new peer reuses its numeric handle; asserts exact-slot isolation and balanced pending references.
fn listener_handle_reuse(action: u8) {
    use embassy_futures::block_on;
    let mut resources = HostResources::<DefaultPacketPool, 2, 2>::new();
    let stack = crate::new(MockController::new(), &mut resources).build();
    let host = stack.host();
    let mgr = host.connections();
    let handle = ConnHandle::new(1);
    mgr.connect(handle, Address::random([1; 6]), LeConnRole::Central, ConnParams::new())
        .unwrap();
    let old_conn = block_on(mgr.accept(LeConnRole::Central, &[]));
    let old = L2capChannel::listen(&stack, &old_conn);
    host.channels().register_spsm(0x80);
    let wakes = std::sync::Arc::new(ListenerWakeCount::default());
    let waker = Waker::from(wakes.clone());
    let mut cx = Context::from_waker(&waker);
    let new_conn;
    let new;
    {
        let config = L2capChannelConfig::default();
        let mut next = pin!(async {
            if action == 1 {
                old.accept(&config).await.map(|_| ()).map_err(|error| match error {
                    BleHostError::BleHost(error) => error,
                    _ => panic!("no controller I/O expected"),
                })
            } else {
                old.next().await.map(|_| ())
            }
        });
        if action != 2 {
            assert!(next.as_mut().poll(&mut cx).is_pending());
        }
        let before = wakes.0.load(std::sync::atomic::Ordering::SeqCst);
        // Follow the host DisconnectionComplete handler's manager ordering.
        mgr.disconnected(handle, bt_hci::param::Status::SUCCESS).unwrap();
        host.channels().disconnected(handle).unwrap();
        if action != 2 {
            assert!(wakes.0.load(std::sync::atomic::Ordering::SeqCst) > before);
        }
        mgr.connect(handle, Address::random([2; 6]), LeConnRole::Central, ConnParams::new())
            .unwrap();
        new_conn = block_on(mgr.accept(LeConnRole::Central, &[]));
        assert_ne!(old_conn.index(), new_conn.index());
        new = L2capChannel::listen(&stack, &new_conn);
        host.channels()
            .signal(handle, &[0x14, 9, 10, 0, 0x80, 0, 0x40, 0, 64, 0, 23, 0, 1, 0], mgr)
            .unwrap();
        if action != 2 {
            assert!(matches!(
                next.as_mut().poll(&mut cx),
                Poll::Ready(Err(Error::Disconnected))
            ));
        }
    }
    assert_eq!(host.channels().channel(ChannelIndex(0)).refcount, 0);
    drop(old);
    assert!(mgr.is_l2cap_listening(handle));
    assert_eq!(
        host.channels().channel(ChannelIndex(0)).state,
        ChannelState::PeerConnecting(9)
    );
    assert!(
        pin!(mgr.outbound()).as_mut().poll(&mut cx).is_pending(),
        "old listener must not reject new request"
    );
    let pending = block_on(new.next()).unwrap();
    assert_eq!(pending.mtu(), 64);
    assert_eq!(host.channels().channel(ChannelIndex(0)).refcount, 1);
    drop(pending);
    assert_eq!(host.channels().channel(ChannelIndex(0)).refcount, 0);
    drop(new);
    assert!(!mgr.is_l2cap_listening(handle));
}

/// An old next future already waiting before terminal close cannot consume the new peer's ready request.
#[test]
fn old_listener_next_cannot_claim_reused_handle() {
    listener_handle_reuse(0);
}

/// Accept inherits the same per-poll identity check and cannot send a response for the new peer.
#[test]
fn old_listener_accept_cannot_claim_reused_handle() {
    listener_handle_reuse(1);
}

/// Dropping an old listener cannot reject or close requests belonging to the reused handle's new peer.
#[test]
fn old_listener_drop_cannot_reject_reused_handle() {
    listener_handle_reuse(2);
}

/// Canceling an empty listener wait retains no channel request and leaves the same listener usable for a later arrival.
#[test]
fn listener_cancelled_wait_preserves_later_request() {
    use embassy_futures::block_on;
    with_link(|stack, host, handle| {
        let connection = host.connections().get_connected_handle(handle).unwrap();
        let listener = L2capChannel::listen(stack, &connection);
        host.channels().register_spsm(0x80);
        {
            let mut waiting = pin!(listener.next());
            assert!(waiting
                .as_mut()
                .poll(&mut Context::from_waker(Waker::noop()))
                .is_pending());
        }
        host.channels()
            .signal(
                handle,
                &[0x14, 9, 10, 0, 0x80, 0, 0x40, 0, 64, 0, 23, 0, 1, 0],
                host.connections(),
            )
            .unwrap();
        assert_eq!(host.channels().channel(ChannelIndex(0)).refcount, 0);
        let pending = block_on(listener.next()).unwrap();
        assert_eq!(pending.mtu(), 64);
        assert_eq!(host.channels().channel(ChannelIndex(0)).refcount, 1);
        drop(pending);
        assert_eq!(host.channels().channel(ChannelIndex(0)).refcount, 0);
    });
}
