//! ATT MTU negotiation, ingress correlation and retained disconnect diagnostics with native fake links.

extern crate std;
use super::*;
use crate::mock_controller::MockController;
use crate::prelude::DefaultPacketPool;
use crate::HostResources;
use crate::{Connection, Stack};
use bt_hci::param::BdAddr;
use core::future::Future;
use embassy_futures::block_on;
/// Runs `f` with an accepted fake link, then synthesizes terminal cleanup and drains pending outbound ownership; fixture setup failures panic.
fn mtu_link(
    f: impl FnOnce(
        &Stack<'_, MockController, DefaultPacketPool>,
        &BleHost<'_, MockController, DefaultPacketPool>,
        &Connection<'_, DefaultPacketPool>,
    ),
) {
    let mut resources = HostResources::<DefaultPacketPool, 1, 1>::new();
    let stack = crate::new(MockController::new(), &mut resources).build();
    let host = BleHost::new(&stack.controller, stack.host_state);
    let mgr = host.connections();
    mgr.connect(
        ConnHandle::new(0),
        Address::new(AddrKind::RANDOM, BdAddr::new([1; 6])),
        LeConnRole::Central,
        ConnParams::new(),
    )
    .unwrap();
    let Poll::Ready(conn) = mgr.poll_accept(LeConnRole::Central, &[], None) else {
        panic!()
    };
    f(&stack, &host, &conn);
    if !conn.is_disconnected() {
        mgr.disconnected(conn.handle(), Status::SUCCESS).unwrap();
    }
    while let Poll::Ready(p) = core::pin::pin!(mgr.outbound())
        .as_mut()
        .poll(&mut core::task::Context::from_waker(core::task::Waker::noop()))
    {
        drop(p);
    }
}
/// Injects borrowed L2CAP `bytes` as a complete first ACL fragment for handle zero into `host`; returns actual ingress admission errors.
fn mtu_acl(host: &BleHost<'_, MockController, DefaultPacketPool>, bytes: &[u8]) -> Result<(), Error> {
    host.handle_acl(
        AclPacket::new(
            ConnHandle::new(0),
            AclPacketBoundary::FirstFlushable,
            AclBroadcastFlag::PointToPoint,
            bytes,
        ),
        &DummyHandler,
    )
}
#[test]
/// A peer requesting the minimum MTU receives local pool capacity while the negotiated MTU remains 23.
fn mtu_response_advertises_local_receive_capacity() {
    mtu_link(|_, host, conn| {
        mtu_acl(host, &[3, 0, 4, 0, 2, 23, 0]).unwrap();
        let (_, pdu) = block_on(host.connections().outbound());
        assert_eq!(conn.att_mtu(), 23);
        assert_eq!(&pdu.as_ref()[4..], &[3, (DefaultPacketPool::MTU - 4) as u8, 0]);
    });
}
#[test]
/// Requests/responses across zero, minimum and oversized MTUs clamp to 23..local capacity and mark the exchange complete.
fn mtu_values_negotiate_default_minimum_and_local_maximum() {
    use core::{
        pin::pin,
        task::{Context, Waker},
    };
    for mtu in (0u16..=23).chain([64, 247, 517, u16::MAX]) {
        for opcode in [2, 3] {
            mtu_link(|stack, host, conn| {
                let mut construction = pin!(crate::gatt::GattClient::<_, _, 1>::new(stack, conn));
                let mut cx = Context::from_waker(Waker::noop());
                if opcode == 3 {
                    assert!(construction.as_mut().poll(&mut cx).is_pending());
                    drop(block_on(host.connections().outbound()));
                }
                let value = mtu.to_le_bytes();
                mtu_acl(host, &[3, 0, 4, 0, opcode, value[0], value[1]]).unwrap();
                assert_eq!(
                    conn.att_mtu(),
                    if mtu < 23 {
                        23
                    } else {
                        mtu.min((DefaultPacketPool::MTU - 4) as u16)
                    }
                );
                assert!(conn.is_att_mtu_exchanged());
                if opcode == 2 {
                    let (_, response) = block_on(host.connections().outbound());
                    let local = ((DefaultPacketPool::MTU - 4) as u16).to_le_bytes();
                    assert_eq!(&response.as_ref()[4..], &[3, local[0], local[1]]);
                } else {
                    assert!(matches!(construction.as_mut().poll(&mut cx), Poll::Ready(Ok(_))));
                }
                assert_eq!(conn.local_disconnect_error(), None);
            });
        }
    }
}
#[test]
/// Terminal status remains queryable when a full event queue rejects the disconnect notification.
fn mtu_terminal_reason_survives_full_event_queue() {
    use core::{
        future::Future,
        pin::pin,
        task::{Context, Waker},
    };
    mtu_link(|_, host, conn| {
        let params = ConnParams::new();
        for _ in 0..crate::config::CONNECTION_EVENT_QUEUE_SIZE {
            host.connections()
                .post_handle_event(
                    conn.handle(),
                    ConnectionEvent::ConnectionParamsUpdated {
                        conn_interval: params.conn_interval,
                        peripheral_latency: params.peripheral_latency,
                        supervision_timeout: params.supervision_timeout,
                    },
                )
                .unwrap();
        }
        host.connections()
            .disconnected(conn.handle(), Status::UNSPECIFIED)
            .unwrap();
        for _ in 0..crate::config::CONNECTION_EVENT_QUEUE_SIZE {
            let _ = block_on(conn.next());
        }
        assert!(pin!(conn.next())
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop()))
            .is_pending());
        assert_eq!(conn.disconnect_reason(), Some(Status::UNSPECIFIED));
    });
}
#[test]
/// Notifications and indications during MTU setup preserve request correlation; indications receive exactly one confirmation.
fn mtu_constructor_handles_unsolicited_values_without_extra_confirmation() {
    use core::{
        pin::pin,
        task::{Context, Waker},
    };
    for opcode in [0x1b, 0x1d] {
        for value in [&[][..], &[42][..]] {
            mtu_link(|stack, host, conn| {
                let mut f = pin!(crate::gatt::GattClient::<_, _, 1>::new(stack, conn));
                let mut cx = Context::from_waker(Waker::noop());
                assert!(f.as_mut().poll(&mut cx).is_pending());
                let (_, p) = block_on(host.connections().outbound());
                assert_eq!(p.as_ref()[4], 2);
                drop(p);
                let mut bytes = std::vec![3 + value.len() as u8, 0, 4, 0, opcode, 1, 0];
                bytes.extend_from_slice(value);
                mtu_acl(host, &bytes).unwrap();
                if opcode == 0x1d {
                    let (_, confirmation) = block_on(host.connections().outbound());
                    assert_eq!(confirmation.as_ref(), &[1, 0, 4, 0, 0x1e]);
                }
                assert!(f.as_mut().poll(&mut cx).is_pending());
                assert!(pin!(host.connections().outbound()).as_mut().poll(&mut cx).is_pending());
                mtu_acl(host, &[3, 0, 4, 0, 3, 23, 0]).unwrap();
                assert!(matches!(f.as_mut().poll(&mut cx), Poll::Ready(Ok(_))));
                assert_eq!(conn.local_disconnect_error(), None);
            });
        }
    }
}
#[test]
/// Malformed MTU response closes the bearer and wakes construction before the terminal controller event, retaining the local cause.
fn mtu_local_malformed_acl_reports_disconnected_before_controller_event() {
    use core::{
        future::Future,
        pin::pin,
        task::{Context, Waker},
    };
    mtu_link(|stack, host, conn| {
        let mut f = pin!(crate::gatt::GattClient::<_, _, 1>::new(stack, conn));
        let mut cx = Context::from_waker(Waker::noop());
        assert!(f.as_mut().poll(&mut cx).is_pending());
        drop(block_on(host.connections().outbound()));
        assert!(matches!(mtu_acl(host, &[2, 0, 4, 0, 3, 23]), Err(Error::InvalidValue)));
        assert!(matches!(
            f.as_mut().poll(&mut cx),
            Poll::Ready(Err(BleHostError::BleHost(Error::Disconnected)))
        ));
        assert!(!conn.is_disconnected());
        assert_eq!(conn.disconnect_reason(), None);
        assert_eq!(conn.local_disconnect_error(), Some(Error::InvalidValue));
        assert!(pin!(conn.next()).as_mut().poll(&mut cx).is_pending());
    });
}

#[test]
/// Only ATT Request Not Supported falls back to MTU23; every other raw error propagates and successful fallback avoids duplicate exchange.
fn mtu_error_fallback_only_for_request_not_supported() {
    use core::{
        pin::pin,
        task::{Context, Waker},
    };
    for code in 0..=u8::MAX {
        mtu_link(|stack, host, conn| {
            let mut future = pin!(crate::gatt::GattClient::<_, _, 1>::new(stack, conn));
            let mut cx = Context::from_waker(Waker::noop());
            assert!(future.as_mut().poll(&mut cx).is_pending());
            drop(block_on(host.connections().outbound()));
            mtu_acl(host, &[5, 0, 4, 0, 1, 2, 0, 0, code]).unwrap();
            let result = future.as_mut().poll(&mut cx);
            if code == 6 {
                assert!(matches!(result, Poll::Ready(Ok(_))));
                drop(result);
                assert_eq!(conn.att_mtu(), 23);
                // The completed exchange must not be sent a second time on this link.
                assert!(matches!(
                    pin!(crate::gatt::GattClient::<_, _, 1>::new(stack, conn))
                        .as_mut()
                        .poll(&mut cx),
                    Poll::Ready(Ok(_))
                ));
                assert!(pin!(host.connections().outbound()).as_mut().poll(&mut cx).is_pending());
            } else {
                assert!(
                    matches!(result,Poll::Ready(Err(BleHostError::BleHost(Error::Att(received)))) if received.to_u8()==code),
                    "lost ATT code {code:#x}"
                );
            }
        });
    }
}
#[test]
/// First local error and terminal status survive cleanup and retained owners; actual slot reuse resets both.
fn mtu_diagnostics_survive_terminal_and_reset_only_on_slot_reuse() {
    let mut resources = HostResources::<DefaultPacketPool, 1, 1>::new();
    let stack = crate::new(MockController::new(), &mut resources).build();
    let host = BleHost::new(&stack.controller, stack.host_state);
    let mgr = host.connections();
    let handle = ConnHandle::new(0);
    let connect = || {
        mgr.connect(
            handle,
            Address::new(AddrKind::RANDOM, BdAddr::new([1; 6])),
            LeConnRole::Central,
            ConnParams::new(),
        )
    };
    connect().unwrap();
    let Poll::Ready(conn) = mgr.poll_accept(LeConnRole::Central, &[], None) else {
        panic!()
    };
    assert_eq!(conn.local_disconnect_error(), None);
    assert_eq!(conn.disconnect_reason(), None);
    mgr.record_local_disconnect_error(handle, &Error::OutOfMemory);
    mgr.record_local_disconnect_error(handle, &Error::InvalidValue);
    conn.disconnect();
    assert_eq!(conn.local_disconnect_error(), Some(Error::OutOfMemory));
    assert_eq!(conn.disconnect_reason(), None);
    mgr.disconnected(handle, Status::UNSPECIFIED).unwrap();
    assert!(matches!(block_on(conn.next()), ConnectionEvent::Disconnected { .. }));
    assert_eq!(conn.local_disconnect_error(), Some(Error::OutOfMemory));
    assert_eq!(conn.disconnect_reason(), Some(Status::UNSPECIFIED));
    assert!(connect().is_err(), "borrowed slot must not be reused");
    drop(conn);
    connect().unwrap();
    let Poll::Ready(conn) = mgr.poll_accept(LeConnRole::Central, &[], None) else {
        panic!()
    };
    assert_eq!(conn.local_disconnect_error(), None);
    assert_eq!(conn.disconnect_reason(), None);
    mgr.disconnected(handle, Status::SUCCESS).unwrap();
}

#[test]
/// A peer-initiated exchange before application acceptance supplies the later client MTU without another request.
fn mtu_peer_exchange_before_accept_skips_duplicate_request() {
    use core::{
        pin::pin,
        task::{Context, Waker},
    };
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
    mtu_acl(&host, &[3, 0, 4, 0, 2, 64, 0]).unwrap();
    drop(block_on(mgr.outbound()));
    let Poll::Ready(conn) = mgr.poll_accept(LeConnRole::Central, &[], None) else {
        panic!()
    };
    assert_eq!(conn.att_mtu(), 64);
    let mut cx = Context::from_waker(Waker::noop());
    assert!(matches!(
        pin!(crate::gatt::GattClient::<_, _, 1>::new(&stack, &conn))
            .as_mut()
            .poll(&mut cx),
        Poll::Ready(Ok(_))
    ));
    assert!(pin!(mgr.outbound()).as_mut().poll(&mut cx).is_pending());
    mgr.disconnected(handle, Status::SUCCESS).unwrap();
}
#[test]
/// Crossed requests advertise identical local capacity but do not complete the client until its own response arrives.
fn mtu_crossed_exchange_waits_for_own_response_and_advertises_symmetric_capacity() {
    use core::{
        pin::pin,
        task::{Context, Waker},
    };
    mtu_link(|stack, host, conn| {
        let mut f = pin!(crate::gatt::GattClient::<_, _, 1>::new(stack, conn));
        let mut cx = Context::from_waker(Waker::noop());
        assert!(f.as_mut().poll(&mut cx).is_pending());
        let (_, request) = block_on(host.connections().outbound());
        let capacity = [request.as_ref()[5], request.as_ref()[6]];
        drop(request);
        mtu_acl(host, &[3, 0, 4, 0, 2, 64, 0]).unwrap();
        let (_, response) = block_on(host.connections().outbound());
        assert_eq!(&response.as_ref()[4..], &[3, capacity[0], capacity[1]]);
        drop(response);
        assert!(f.as_mut().poll(&mut cx).is_pending());
        mtu_acl(host, &[3, 0, 4, 0, 3, 64, 0]).unwrap();
        assert!(matches!(f.as_mut().poll(&mut cx), Poll::Ready(Ok(_))));
        assert_eq!(conn.att_mtu(), 64);
    });
}
#[test]
/// Both MTU opcodes reject every tested non-three-byte shape and retain InvalidValue as the local close cause.
fn mtu_malformed_fixed_shape_records_local_error() {
    for opcode in [2, 3] {
        for length in [1, 2, 4, 5] {
            mtu_link(|_, host, conn| {
                let mut bytes = std::vec![length, 0, 4, 0, opcode];
                bytes.resize(4 + usize::from(length), 0);
                assert!(matches!(mtu_acl(host, &bytes), Err(Error::InvalidValue)));
                assert_eq!(conn.local_disconnect_error(), Some(Error::InvalidValue));
                assert_eq!(conn.disconnect_reason(), None);
            });
        }
    }
}
#[test]
/// Remote close wakes pending MTU construction with Disconnected and retains only the controller reason.
fn mtu_remote_disconnect_keeps_reason_without_local_error() {
    use core::{
        pin::pin,
        task::{Context, Waker},
    };
    mtu_link(|stack, host, conn| {
        let mut f = pin!(crate::gatt::GattClient::<_, _, 1>::new(stack, conn));
        let mut cx = Context::from_waker(Waker::noop());
        assert!(f.as_mut().poll(&mut cx).is_pending());
        drop(block_on(host.connections().outbound()));
        host.connections()
            .disconnected(conn.handle(), Status::UNSPECIFIED)
            .unwrap();
        assert!(matches!(
            f.as_mut().poll(&mut cx),
            Poll::Ready(Err(BleHostError::BleHost(Error::Disconnected)))
        ));
        assert_eq!(conn.disconnect_reason(), Some(Status::UNSPECIFIED));
        assert_eq!(conn.local_disconnect_error(), None);
    });
}
#[test]
/// Response allocation exhaustion closes the bearer with OutOfMemory; later terminal status remains a separate observation.
fn mtu_pool_exhaustion_keeps_first_local_error_before_terminal_reason() {
    use core::{
        pin::pin,
        task::{Context, Waker},
    };
    mtu_link(|stack, host, conn| {
        let mut f = pin!(crate::gatt::GattClient::<_, _, 1>::new(stack, conn));
        let mut cx = Context::from_waker(Waker::noop());
        assert!(f.as_mut().poll(&mut cx).is_pending());
        drop(block_on(host.connections().outbound()));
        let mut held = std::vec::Vec::new();
        while let Some(packet) = DefaultPacketPool::allocate() {
            held.push(packet);
        }
        assert!(matches!(
            mtu_acl(host, &[3, 0, 4, 0, 3, 23, 0]),
            Err(Error::OutOfMemory)
        ));
        assert!(matches!(
            f.as_mut().poll(&mut cx),
            Poll::Ready(Err(BleHostError::BleHost(Error::Disconnected)))
        ));
        assert_eq!(conn.local_disconnect_error(), Some(Error::OutOfMemory));
        assert_eq!(conn.disconnect_reason(), None);
        drop(held);
        host.connections()
            .disconnected(conn.handle(), Status::UNSPECIFIED)
            .unwrap();
        assert_eq!(conn.local_disconnect_error(), Some(Error::OutOfMemory));
        assert_eq!(conn.disconnect_reason(), Some(Status::UNSPECIFIED));
    });
}

#[test]
/// Every nonzero HCI completion status reports error without terminal state, reason, event or disconnect metric publication.
fn mtu_hci_failed_disconnect_keeps_link_nonterminal_and_reports_status() {
    use core::{
        pin::pin,
        task::{Context, Waker},
    };
    for status in 1..=u8::MAX {
        for local_request in [false, true] {
            mtu_link(|_, host, conn| {
                if local_request {
                    conn.disconnect();
                }
                let event = DisconnectionComplete::from_hci_bytes_complete(&[status, 0, 0, 0x13]).unwrap();
                let expected = Error::Hci(event.status.to_result().unwrap_err());
                assert_eq!(host.handle_disconnection_complete(event), Err(expected));
                assert!(!conn.is_disconnected());
                assert_eq!(conn.is_connected(), !local_request);
                assert_eq!(conn.disconnect_reason(), None);
                assert_eq!(host.state.metrics.borrow().disconnect_events, 0);
                assert!(pin!(conn.next())
                    .as_mut()
                    .poll(&mut Context::from_waker(Waker::noop()))
                    .is_pending());
            });
        }
    }
}
#[test]
/// Successful HCI completion preserves all 256 raw reason bytes and increments terminal metrics exactly once.
fn mtu_hci_successful_disconnect_retains_every_raw_reason() {
    for reason in 0..=u8::MAX {
        mtu_link(|_, host, conn| {
            let event = DisconnectionComplete::from_hci_bytes_complete(&[0, 0, 0, reason]).unwrap();
            let expected = event.reason;
            host.handle_disconnection_complete(event).unwrap();
            assert!(conn.is_disconnected());
            assert_eq!(conn.disconnect_reason(), Some(expected));
            assert_eq!(host.state.metrics.borrow().disconnect_events, 1);
            assert!(matches!(block_on(conn.next()), ConnectionEvent::Disconnected {reason} if reason == expected));
        });
    }
}
#[test]
/// Late rejected ACL packets after local/terminal close cannot replace the genuine disconnect diagnostic.
fn mtu_late_acl_after_local_cleanup_does_not_record_false_cause() {
    mtu_link(|_, host, conn| {
        conn.disconnect();
        assert!(mtu_acl(host, &[3, 0, 4, 0, 3, 23, 0]).is_err());
        host.connections()
            .record_local_disconnect_error(conn.handle(), &Error::NotFound);
        assert_eq!(conn.local_disconnect_error(), None);
        assert_eq!(conn.disconnect_reason(), None);
        let event = DisconnectionComplete::from_hci_bytes_complete(&[0, 0, 0, 0x16]).unwrap();
        let reason = event.reason;
        host.handle_disconnection_complete(event).unwrap();
        assert!(mtu_acl(host, &[3, 0, 4, 0, 3, 23, 0]).is_err());
        assert_eq!(conn.local_disconnect_error(), None);
        assert_eq!(conn.disconnect_reason(), Some(reason));
    });
}

#[test]
/// Idle, queued-duplicate and consumed-duplicate ATT responses reject at ingress, close the bearer and prevent subsequent requests.
fn client_idle_or_duplicate_responses_close_the_bearer_at_ingress() {
    use core::{
        pin::pin,
        task::{Context, Waker},
    };
    // Exercise both an idle duplicate and one arriving before the client task
    // has delivered the first response. Queue scheduling must not admit either.
    for consumed in [false, true] {
        mtu_link(|stack, host, conn| {
            conn.set_att_mtu(23);
            let client = block_on(crate::gatt::GattClient::<_, _, 1>::new(stack, conn)).unwrap();
            let mut task = pin!(client.task());
            let mut cx = Context::from_waker(Waker::noop());
            let mut value = [0u8; 8];
            let mut read = pin!(client.read_handle(0x0101, &mut value));
            assert!(read.as_mut().poll(&mut cx).is_pending());
            let (_, request) = block_on(host.connections().outbound());
            assert_eq!(&request.as_ref()[4..], &[0x0a, 1, 1]);
            drop(request);
            mtu_acl(host, &[2, 0, 4, 0, 0x0b, 0x42]).unwrap();
            if consumed {
                assert!(task.as_mut().poll(&mut cx).is_pending());
                assert!(matches!(read.as_mut().poll(&mut cx), Poll::Ready(Ok(1))));
            }
            assert_eq!(
                mtu_acl(host, &[2, 0, 4, 0, 0x0b, 0x42]),
                Err(Error::UnexpectedGattResponse)
            );
            assert!(!conn.is_connected());
            assert_eq!(conn.local_disconnect_error(), Some(Error::UnexpectedGattResponse));
            let mut next_value = [0u8; 8];
            assert!(matches!(
                pin!(client.read_handle(0x0202, &mut next_value)).as_mut().poll(&mut cx),
                Poll::Ready(Err(BleHostError::BleHost(Error::Disconnected)))
            ));
        });
    }
    mtu_link(|_, host, conn| {
        assert_eq!(
            mtu_acl(host, &[3, 0, 4, 0, 3, 64, 0]),
            Err(Error::UnexpectedGattResponse)
        );
        assert!(!conn.is_att_mtu_exchanged(), "unsolicited MTU must not change state");
    });
}

#[test]
/// Overlapping requests fail Busy without canceling the owner; dropping active read or MTU construction closes ambiguous correlation.
fn client_overlapping_requests_fail_busy_and_cancellation_closes_bearer() {
    use core::{
        pin::pin,
        task::{Context, Waker},
    };
    mtu_link(|stack, host, conn| {
        conn.set_att_mtu(23);
        let client = block_on(crate::gatt::GattClient::<_, _, 1>::new(stack, conn)).unwrap();
        let mut cx = Context::from_waker(Waker::noop());
        let mut first_value = [0u8; 8];
        {
            let mut first = pin!(client.read_handle(1, &mut first_value));
            assert!(first.as_mut().poll(&mut cx).is_pending());
            drop(block_on(host.connections().outbound()));
            let mut second_value = [0u8; 8];
            assert!(matches!(
                pin!(client.read_handle(2, &mut second_value)).as_mut().poll(&mut cx),
                Poll::Ready(Err(BleHostError::BleHost(Error::Busy)))
            ));
            assert!(pin!(host.connections().outbound()).as_mut().poll(&mut cx).is_pending());
            assert!(conn.is_connected(), "rejected overlap must not cancel the owner");
        }
        assert!(!conn.is_connected(), "an abandoned wire request cannot be reused");
        assert!(mtu_acl(host, &[2, 0, 4, 0, 0x0b, 42]).is_err());
    });
    mtu_link(|stack, host, conn| {
        let mut cx = Context::from_waker(Waker::noop());
        {
            let mut construction = pin!(crate::gatt::GattClient::<_, _, 1>::new(stack, conn));
            assert!(construction.as_mut().poll(&mut cx).is_pending());
            drop(block_on(host.connections().outbound()));
        }
        assert!(!conn.is_connected(), "abandoned MTU exchange must close the bearer");
    });
}

#[test]
/// Interleaved notifications and matching ATT errors preserve correlation, allowing subsequent reads to return their exact values.
fn client_matching_errors_notifications_and_sequential_reads_remain_usable() {
    use core::{
        pin::pin,
        task::{Context, Waker},
    };
    mtu_link(|stack, host, conn| {
        conn.set_att_mtu(23);
        let client = block_on(crate::gatt::GattClient::<_, _, 1>::new(stack, conn)).unwrap();
        let mut task = pin!(client.task());
        let mut cx = Context::from_waker(Waker::noop());
        for (handle, value) in [(1, None), (2, Some(42)), (3, Some(81))] {
            let mut buffer = [0u8; 8];
            {
                let mut read = pin!(client.read_handle(handle, &mut buffer));
                assert!(read.as_mut().poll(&mut cx).is_pending());
                let (_, request) = block_on(host.connections().outbound());
                assert_eq!(&request.as_ref()[4..], &[0x0a, handle as u8, 0]);
                drop(request);
                mtu_acl(host, &[4, 0, 4, 0, 0x1b, 9, 0, 99]).unwrap();
                assert!(task.as_mut().poll(&mut cx).is_pending());
                assert!(read.as_mut().poll(&mut cx).is_pending());
                if let Some(value) = value {
                    mtu_acl(host, &[2, 0, 4, 0, 0x0b, value]).unwrap();
                } else {
                    mtu_acl(host, &[5, 0, 4, 0, 1, 0x0a, handle as u8, 0, 2]).unwrap();
                }
                assert!(task.as_mut().poll(&mut cx).is_pending());
                match value {
                    Some(_) => assert!(matches!(read.as_mut().poll(&mut cx), Poll::Ready(Ok(1)))),
                    None => assert!(matches!(
                        read.as_mut().poll(&mut cx),
                        Poll::Ready(Err(BleHostError::BleHost(Error::Att(code))))
                            if code == crate::att::AttErrorCode::READ_NOT_PERMITTED
                    )),
                }
            }
            if let Some(value) = value {
                assert_eq!(buffer[0], value);
            }
            assert!(conn.is_connected());
        }
    });
}

#[test]
/// A wrong response opcode or mismatched error request closes the bearer with UnexpectedGattResponse.
fn client_wrong_response_opcode_or_error_request_closes_bearer() {
    use core::{
        pin::pin,
        task::{Context, Waker},
    };
    for bytes in [&[1, 0, 4, 0, 0x13][..], &[5, 0, 4, 0, 1, 0x12, 1, 0, 2][..]] {
        mtu_link(|stack, host, conn| {
            conn.set_att_mtu(23);
            let client = block_on(crate::gatt::GattClient::<_, _, 1>::new(stack, conn)).unwrap();
            let mut cx = Context::from_waker(Waker::noop());
            let mut buffer = [0; 8];
            let mut read = pin!(client.read_handle(1, &mut buffer));
            assert!(read.as_mut().poll(&mut cx).is_pending());
            drop(block_on(host.connections().outbound()));
            assert_eq!(mtu_acl(host, bytes), Err(Error::UnexpectedGattResponse));
            assert!(!conn.is_connected());
        });
    }
}

#[test]
/// A second client fails Busy; dropping an idle client releases ownership without closing the accepted link.
fn client_connection_allows_one_live_client_and_releases_clean_owner() {
    use core::{
        pin::pin,
        task::{Context, Waker},
    };
    mtu_link(|stack, _, conn| {
        conn.set_att_mtu(23);
        let client = block_on(crate::gatt::GattClient::<_, _, 1>::new(stack, conn)).unwrap();
        let mut cx = Context::from_waker(Waker::noop());
        assert!(matches!(
            pin!(crate::gatt::GattClient::<_, _, 1>::new(stack, conn))
                .as_mut()
                .poll(&mut cx),
            Poll::Ready(Err(BleHostError::BleHost(Error::Busy)))
        ));
        assert!(conn.is_connected());
        drop(client);
        assert!(matches!(
            pin!(crate::gatt::GattClient::<_, _, 1>::new(stack, conn))
                .as_mut()
                .poll(&mut cx),
            Poll::Ready(Ok(_))
        ));
    });
}
