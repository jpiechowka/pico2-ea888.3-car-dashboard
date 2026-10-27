//! Stateful client discovery pagination, error correlation, capacity and terminal-handle regressions using synthetic ATT responses.

use super::tests::test_client;
use super::*;
use crate::attribute::CharacteristicProp;
use core::future::Future;
use core::pin::pin;
use core::task::{Context, Poll, Waker};

/// Copies ATT `bytes` into a pool packet and injects it into `client` response queue; allocation, capacity or queue failures panic. This bypasses ingress correlation deliberately to exercise client validation.
fn reply<const N: usize>(
    client: &GattClient<'_, crate::mock_controller::MockController, DefaultPacketPool, N>,
    bytes: &[u8],
) {
    let mut packet = DefaultPacketPool::allocate().unwrap();
    packet.as_mut()[..bytes.len()].copy_from_slice(bytes);
    client
        .response_channel
        .try_send((client.connection.handle(), Pdu::new(packet, bytes.len())))
        .unwrap();
}

#[test]
/// Zero start and reversed service ranges terminate discovery with error instead of retrying invalid progress.
fn discovery_rejects_invalid_service_ranges_without_retrying() {
    for (start, end) in [(0u16, 1u16), (2, 1)] {
        let (_guard, conn) = connection();
        let client = test_client(&conn);
        let mut bytes = [0x11, 6, 0, 0, 0, 0, 0x0a, 0x18];
        bytes[2..4].copy_from_slice(&start.to_le_bytes());
        bytes[4..6].copy_from_slice(&end.to_le_bytes());
        reply(&client, &bytes);
        let mut future = pin!(client.services());
        assert!(matches!(
            future.as_mut().poll(&mut Context::from_waker(Waker::noop())),
            Poll::Ready(Err(_))
        ));
    }
}

#[test]
/// Read-by-type rejects handles outside the requested interval and duplicate entries within a response.
fn discovery_read_by_type_rejects_out_of_range_and_duplicate_handles() {
    for bytes in [
        &[0x09, 3, 0, 0, 42][..],
        &[0x09, 3, 4, 0, 42][..],
        &[0x09, 3, 2, 0, 42, 2, 0, 43][..],
    ] {
        let (_guard, conn) = connection();
        let client = test_client(&conn);
        reply(&client, bytes);
        let uuid = Uuid::new_short(0x2803);
        let mut future = pin!(client.read_by_type(1, 3, &uuid, |_, _| ControlFlow::<()>::Continue(())));
        assert!(matches!(
            future.as_mut().poll(&mut Context::from_waker(Waker::noop())),
            Poll::Ready(Err(_))
        ));
    }
}

#[test]
/// Handle65535 is delivered once and completes the requested range without wrapping to zero.
fn discovery_accepts_last_handle_without_overflow_or_extra_request() {
    let (_guard, conn) = connection();
    let client = test_client(&conn);
    reply(&client, &[0x09, 3, 0xff, 0xff, 42]);
    let uuid = Uuid::new_short(0x2803);
    let mut seen = 0;
    let mut future = pin!(client.read_by_type(0xffff, 0xffff, &uuid, |handle, data| {
        assert_eq!((handle, data), (0xffff, &[42][..]));
        seen += 1;
        ControlFlow::<()>::Continue(())
    }));
    assert!(matches!(
        future.as_mut().poll(&mut Context::from_waker(Waker::noop())),
        Poll::Ready(Ok(None))
    ));
}

#[test]
/// A read response exceeding negotiated MTU rejects even when destination capacity is sufficient.
fn discovery_read_rejects_value_larger_than_negotiated_mtu() {
    let (_guard, conn) = connection();
    conn.set_att_mtu(23);
    let client = test_client(&conn);
    let mut bytes = [42; 24];
    bytes[0] = 0x0b;
    reply(&client, &bytes);
    let mut dest = [0u8; 64];
    let mut future = pin!(client.read_handle(1, &mut dest));
    assert!(matches!(
        future.as_mut().poll(&mut Context::from_waker(Waker::noop())),
        Poll::Ready(Err(_))
    ));
}

use crate::prelude::DefaultPacketPool;

#[test]
/// A service ending at handle65535 retains its inclusive range and UUID without another query.
fn discovery_service_terminal_range_is_preserved() {
    let (_guard, conn) = connection();
    let client = test_client(&conn);
    reply(&client, &[0x11, 6, 1, 0, 0xff, 0xff, 0x0a, 0x18]);
    let mut future = pin!(client.services());
    let Poll::Ready(Ok(services)) = future.as_mut().poll(&mut Context::from_waker(Waker::noop())) else {
        panic!("valid service rejected")
    };
    assert_eq!(services[0].handle_range(), 1..=0xffff);
    assert_eq!(services[0].uuid(), Uuid::new_short(0x180a));
}

#[test]
/// Progressing pages deliver exact values; repeating the previous handle is rejected.
fn discovery_multiple_pages_preserve_values_and_reject_repeated_page() {
    for second_handle in [2u8, 3] {
        let (_guard, conn) = connection();
        let client = test_client(&conn);
        let uuid = Uuid::new_short(0x2803);
        let values = RefCell::new(std::vec::Vec::new());
        let mut future = pin!(client.read_by_type(1, 3, &uuid, |handle, data| {
            values.borrow_mut().push((handle, data[0]));
            ControlFlow::<()>::Continue(())
        }));
        let mut cx = Context::from_waker(Waker::noop());
        reply(&client, &[0x09, 3, 2, 0, 42]);
        assert!(future.as_mut().poll(&mut cx).is_pending());
        reply(&client, &[0x09, 3, second_handle, 0, 43]);
        let result = future.as_mut().poll(&mut cx);
        if second_handle == 3 {
            assert!(matches!(result, Poll::Ready(Ok(None))));
            assert_eq!(&*values.borrow(), &[(2, 42), (3, 43)]);
        } else {
            assert!(matches!(result, Poll::Ready(Err(_))));
        }
    }
}

/// Consumes one queued request from `guard` and checks its ATT body against `att`; missing output or different bytes panic.
fn expect_request(guard: &Drain, att: &[u8]) {
    let mut next = pin!(guard.0.outbound());
    let Poll::Ready((_, packet)) = next.as_mut().poll(&mut Context::from_waker(Waker::noop())) else {
        panic!("missing request")
    };
    assert_eq!(&packet.as_ref()[4..], att);
}

#[test]
/// Constructor accepts only exact MTU response or matching Request Not Supported fallback; malformed/unrelated responses close correlation.
fn discovery_mtu_constructor_correlates_error_opcode_and_preserves_fallback() {
    use bt_hci::param::{AddrKind, BdAddr, ConnHandle, LeConnRole};

    for (bytes, accepted) in [
        (&[0x01, 0x08, 0, 0, 0x0a][..], false),
        (&[0x01, 0x10, 0, 0, 0x06][..], false),
        (&[0x01][..], false),
        (&[0x01, 0x02, 0, 0][..], false),
        (&[0x01, 0x02, 0, 0, 0x06, 0][..], false),
        (&[0x01, 0x02, 0, 0, 0x06][..], true),
        (&[0x03][..], false),
        (&[0x03, 23][..], false),
        (&[0x03, 23, 0, 0][..], false),
        (&[0x1b][..], false),
        (&[0x1b, 1][..], false),
        (&[0x1d][..], false),
        (&[0x03, 23, 0][..], true),
    ] {
        let mut resources = crate::HostResources::<DefaultPacketPool, 1, 1>::new();
        let stack = crate::new(crate::mock_controller::MockController::new(), &mut resources).build();
        let mgr = &stack.host_state.connections;
        mgr.connect(
            ConnHandle::new(0),
            crate::Address::new(AddrKind::RANDOM, BdAddr::new([1; 6])),
            LeConnRole::Central,
            crate::connection::ConnParams::new(),
        )
        .unwrap();
        let Poll::Ready(conn) = mgr.poll_accept(LeConnRole::Central, &[], None) else {
            panic!("connection")
        };
        let mut future = pin!(GattClient::<_, _, 1>::new(&stack, &conn));
        let mut cx = Context::from_waker(Waker::noop());
        assert!(future.as_mut().poll(&mut cx).is_pending());
        let Poll::Ready((_, packet)) = pin!(mgr.outbound()).as_mut().poll(&mut cx) else {
            panic!("missing MTU request")
        };
        let mtu = (DefaultPacketPool::MTU as u16 - 4).to_le_bytes();
        assert_eq!(&packet.as_ref()[4..], &[0x02, mtu[0], mtu[1]]);
        drop(packet);
        let mut response = DefaultPacketPool::allocate().unwrap();
        response.as_mut()[..bytes.len()].copy_from_slice(bytes);
        mgr.post_gatt_client(conn.handle(), Pdu::new(response, bytes.len()))
            .unwrap();
        let result = future.as_mut().poll(&mut cx);
        if accepted {
            assert!(matches!(result, Poll::Ready(Ok(_))));
            assert!(pin!(conn.next_gatt_client()).as_mut().poll(&mut cx).is_pending());
        } else {
            assert!(
                matches!(
                    result,
                    Poll::Ready(Err(BleHostError::BleHost(Error::UnexpectedGattResponse)))
                ),
                "constructor accepted unrelated error {bytes:?}"
            );
            assert!(matches!(
                pin!(conn.next_gatt_client()).as_mut().poll(&mut cx),
                Poll::Ready(None)
            ));
        }
        assert!(pin!(mgr.outbound()).as_mut().poll(&mut cx).is_pending());
    }
}

/// Runs `f` against a real local stack and fresh accepted connection; teardown releases queued packets.
fn with_client_startup(
    f: impl FnOnce(&Stack<'_, crate::mock_controller::MockController, DefaultPacketPool>, Connection<'_, DefaultPacketPool>),
) {
    use bt_hci::param::{AddrKind, BdAddr, ConnHandle, LeConnRole};
    let mut resources = crate::HostResources::<DefaultPacketPool, 1, 1>::new();
    let stack = crate::new(crate::mock_controller::MockController::new(), &mut resources).build();
    let manager = &stack.host_state.connections;
    manager
        .connect(
            ConnHandle::new(0),
            crate::Address::new(AddrKind::RANDOM, BdAddr::new([1; 6])),
            LeConnRole::Central,
            crate::connection::ConnParams::new(),
        )
        .unwrap();
    let Poll::Ready(connection) = manager.poll_accept(LeConnRole::Central, &[], None) else {
        panic!("connection")
    };
    f(&stack, connection);
}

#[test]
/// Completed startup lends the exact final slot; occupied slots and a competing owner remain unchanged, then drop releases the claim.
fn discovery_new_into_lends_final_owner_and_preserves_busy_slot() {
    with_client_startup(|stack, connection| {
        let manager = &stack.host_state.connections;
        let mut slot = None;
        let mut cx = Context::from_waker(Waker::noop());
        let address = {
            let mut startup = pin!(GattClient::<_, _, 24>::new_into(stack, &connection, &mut slot));
            assert!(startup.as_mut().poll(&mut cx).is_pending());
            let Poll::Ready((_, packet)) = pin!(manager.outbound()).as_mut().poll(&mut cx) else {
                panic!("MTU request")
            };
            assert_eq!(packet.as_ref()[4], att::ATT_EXCHANGE_MTU_REQ);
            drop(packet);
            connection.set_att_mtu(23);
            let mut response = DefaultPacketPool::allocate().unwrap();
            response.as_mut()[..3].copy_from_slice(&[0x03, 23, 0]);
            manager
                .post_gatt_client(connection.handle(), Pdu::new(response, 3))
                .unwrap();
            let Poll::Ready(Ok(client)) = startup.as_mut().poll(&mut cx) else {
                panic!("startup")
            };
            client
                .known_services
                .borrow_mut()
                .push(ServiceHandle {
                    start: 1,
                    end: 4,
                    uuid: Uuid::new_short(0x1801),
                })
                .unwrap();
            client as *const _
        };
        assert!(core::ptr::eq(slot.as_ref().unwrap(), address));
        assert!(matches!(
            embassy_futures::block_on(GattClient::new_into(stack, &connection, &mut slot)),
            Err(BleHostError::BleHost(Error::Busy))
        ));
        assert!(core::ptr::eq(slot.as_ref().unwrap(), address));
        assert_eq!(slot.as_ref().unwrap().known_services.borrow()[0].handle_range(), 1..=4);
        let mut competing = None;
        assert!(matches!(
            embassy_futures::block_on(GattClient::<_, _, 24>::new_into(stack, &connection, &mut competing)),
            Err(BleHostError::BleHost(Error::Busy))
        ));
        assert!(competing.is_none());
        drop(slot.take());
        assert!(
            embassy_futures::block_on(GattClient::<_, _, 24>::new_into(stack, &connection, &mut competing)).is_ok()
        );
        assert!(
            pin!(manager.outbound()).as_mut().poll(&mut cx).is_pending(),
            "MTU exchange stays once-only"
        );
    });
}

#[test]
/// Allocation/ATT errors and canceled admitted startup leave no published client; only admission failures preserve a reusable bearer.
fn discovery_new_into_error_and_cancel_leave_empty_slot() {
    for failure in 0..4 {
        with_client_startup(|stack, connection| {
            let manager = &stack.host_state.connections;
            let mut slot = None;
            let mut held = std::vec::Vec::new();
            if failure == 0 {
                while let Some(packet) = DefaultPacketPool::allocate() {
                    held.push(packet);
                }
            }
            let mut cx = Context::from_waker(Waker::noop());
            {
                let mut startup = pin!(GattClient::<_, _, 24>::new_into(stack, &connection, &mut slot));
                if failure == 0 {
                    assert!(matches!(
                        startup.as_mut().poll(&mut cx),
                        Poll::Ready(Err(BleHostError::BleHost(Error::OutOfMemory)))
                    ));
                } else {
                    assert!(startup.as_mut().poll(&mut cx).is_pending());
                    let Poll::Ready((_, request)) = pin!(manager.outbound()).as_mut().poll(&mut cx) else {
                        panic!("MTU request")
                    };
                    drop(request);
                    if failure != 3 {
                        let bytes: &[u8] = if failure == 1 { &[] } else { &[0x01, 0x02, 0, 0, 0x05] };
                        let mut response = DefaultPacketPool::allocate().unwrap();
                        response.as_mut()[..bytes.len()].copy_from_slice(bytes);
                        manager
                            .post_gatt_client(connection.handle(), Pdu::new(response, bytes.len()))
                            .unwrap();
                        assert!(matches!(startup.as_mut().poll(&mut cx), Poll::Ready(Err(_))));
                    }
                }
            }
            assert!(slot.is_none());
            assert_eq!(connection.is_connected(), failure == 0);
            drop(held);
            if failure == 0 {
                // A failed allocation must release the sole-client claim before returning.
                connection.set_att_mtu(23);
                assert!(
                    embassy_futures::block_on(GattClient::<_, _, 24>::new_into(stack, &connection, &mut slot)).is_ok()
                );
            }
        });
    }
}

/// Borrow whose destructor verifies the actual client claim remains owned until work releases it.
struct StartupBorrow<'a, 'connection> {
    /// Final-slot client retained across the suspended inspection.
    client: &'a GattClient<'connection, crate::mock_controller::MockController, DefaultPacketPool, 24>,
    /// Address recorded before suspension, checked again during destruction.
    address: usize,
    /// Shared observation that borrowed work was dropped before cleanup.
    dropped: &'a core::cell::Cell<bool>,
}
impl Drop for StartupBorrow<'_, '_> {
    /// Verifies stable storage and a still-exclusive claim before recording work release.
    fn drop(&mut self) {
        assert_eq!(core::ptr::from_ref(self.client) as usize, self.address);
        assert!(matches!(
            ClientConnection::acquire(&self.client.connection),
            Err(Error::Busy)
        ));
        self.dropped.set(true);
    }
}

#[test]
/// Caller-owned startup storage drops after borrowers and before cleanup on success, event error and cancellation.
fn discovery_new_into_owner_scope_releases_before_cleanup() {
    for outcome in 0..3 {
        with_client_startup(|stack, connection| {
            connection.set_att_mtu(23);
            let finish = core::cell::Cell::new(false);
            let borrowed_dropped = core::cell::Cell::new(false);
            let mut cx = Context::from_waker(Waker::noop());
            {
                // Same owner scope as probe inspect/connected_work: startup and work
                // borrow this slot; the caller begins cleanup after this future drops.
                let inspection = async {
                    let mut slot = None;
                    let client = GattClient::<_, _, 24>::new_into(stack, &connection, &mut slot)
                        .await
                        .unwrap();
                    let address = core::ptr::from_ref(client) as usize;
                    let work = async {
                        let _borrow = StartupBorrow {
                            client,
                            address,
                            dropped: &borrowed_dropped,
                        };
                        core::future::poll_fn(|_| {
                            assert_eq!(core::ptr::from_ref(client) as usize, address);
                            if finish.get() && outcome == 0 {
                                Poll::Ready(Ok::<_, ()>(()))
                            } else {
                                Poll::Pending
                            }
                        })
                        .await
                    };
                    let events = core::future::poll_fn(|_| {
                        if finish.get() && outcome == 1 {
                            Poll::Ready(Err::<(), _>(()))
                        } else {
                            Poll::Pending
                        }
                    });
                    match embassy_futures::select::select(work, events).await {
                        embassy_futures::select::Either::First(result)
                        | embassy_futures::select::Either::Second(result) => result,
                    }
                };
                let mut inspection = pin!(inspection);
                assert!(inspection.as_mut().poll(&mut cx).is_pending());
                assert!(!borrowed_dropped.get());
                assert!(matches!(ClientConnection::acquire(&connection), Err(Error::Busy)));
                finish.set(true);
                if outcome != 2 {
                    assert_eq!(
                        inspection.as_mut().poll(&mut cx),
                        Poll::Ready(if outcome == 0 { Ok(()) } else { Err(()) })
                    );
                }
            }
            assert!(borrowed_dropped.get());
            // The actual client lease is gone before full-link cleanup begins.
            let claim = ClientConnection::acquire(&connection).expect("owner survived inspection scope");
            drop(claim);
            assert!(connection.is_connected());
            connection.disconnect();
        });
    }
}

#[test]
/// A real outer startup deadline drops the admitted MTU request, closes its bearer and leaves the final client slot empty.
fn discovery_new_into_deadline_drops_unpublished_startup() {
    with_client_startup(|stack, connection| {
        let manager = &stack.host_state.connections;
        let mut slot = None;
        let mut cx = Context::from_waker(Waker::noop());
        {
            let mut startup = pin!(with_timeout(
                Duration::from_millis(1),
                GattClient::<_, _, 24>::new_into(stack, &connection, &mut slot),
            ));
            assert!(startup.as_mut().poll(&mut cx).is_pending());
            let Poll::Ready((_, request)) = pin!(manager.outbound()).as_mut().poll(&mut cx) else {
                panic!("MTU request")
            };
            drop(request);
            std::thread::sleep(std::time::Duration::from_millis(5));
            assert!(matches!(startup.as_mut().poll(&mut cx), Poll::Ready(Err(_))));
        }
        assert!(slot.is_none());
        assert!(!connection.is_connected());
    });
}

#[test]
/// Truncated or trailing-byte ATT errors reject without issuing another discovery request.
fn discovery_error_response_requires_exact_wire_shape() {
    for bytes in [&[0x01][..], &[0x01, 0x08, 0, 0][..], &[0x01, 0x08, 0, 0, 0x06, 0][..]] {
        let (guard, conn) = connection();
        let client = test_client(&conn);
        let uuid = Uuid::new_short(0x2803);
        let mut future = pin!(client.read_by_type(1, 9, &uuid, |_, _| ControlFlow::<()>::Continue(())));
        let mut cx = Context::from_waker(Waker::noop());
        assert!(future.as_mut().poll(&mut cx).is_pending());
        expect_request(&guard, &[0x08, 1, 0, 9, 0, 3, 0x28]);
        reply(&client, bytes);
        assert!(
            matches!(
                future.as_mut().poll(&mut cx),
                Poll::Ready(Err(BleHostError::BleHost(Error::InvalidValue)))
            ),
            "accepted malformed error {bytes:?}"
        );
        assert!(pin!(guard.0.outbound()).as_mut().poll(&mut cx).is_pending());
    }
}

#[test]
/// Service/type/information discovery rejects mismatched request opcodes; matching Attribute Not Found completes while other errors propagate.
fn discovery_error_response_must_name_the_pending_request() {
    for (kind, wire) in [
        (0, &[0x10, 1, 0, 0xff, 0xff, 0, 0x28][..]),
        (1, &[0x08, 1, 0, 9, 0, 3, 0x28][..]),
        (2, &[0x04, 1, 0, 9, 0][..]),
    ] {
        for matching in [false, true] {
            for code in [0x0a, 0x06, 0x0e] {
                let (guard, conn) = connection();
                let client = test_client(&conn);
                let uuid = Uuid::new_short(0x2803);
                let mut future = pin!(async {
                    match kind {
                        0 => client.services().await.map(|_| ()),
                        1 => client
                            .read_by_type(1, 9, &uuid, |_, _| ControlFlow::<()>::Continue(()))
                            .await
                            .map(|_| ()),
                        _ => client
                            .find_information(1, 9, |_, _| ControlFlow::<()>::Continue(()))
                            .await
                            .map(|_| ()),
                    }
                });
                let mut cx = Context::from_waker(Waker::noop());
                assert!(future.as_mut().poll(&mut cx).is_pending());
                expect_request(&guard, wire);
                // Request Not Supported and Unlikely Error legitimately use a zero handle.
                let handle = if code == 0x0a { 1 } else { 0 };
                reply(&client, &[0x01, if matching { wire[0] } else { 0x0a }, handle, 0, code]);
                let result = future.as_mut().poll(&mut cx);
                if !matching {
                    assert!(
                        matches!(
                            result,
                            Poll::Ready(Err(BleHostError::BleHost(Error::UnexpectedGattResponse)))
                        ),
                        "kind={kind} code={code:#x}: mismatched request accepted"
                    );
                } else if code == 0x0a {
                    assert!(matches!(result, Poll::Ready(Ok(()))));
                } else {
                    assert!(
                        matches!(result, Poll::Ready(Err(BleHostError::BleHost(Error::Att(received)))) if received == if code == 0x06 { att::AttErrorCode::REQUEST_NOT_SUPPORTED } else { att::AttErrorCode::UNLIKELY_ERROR })
                    );
                }
                assert!(pin!(guard.0.outbound()).as_mut().poll(&mut cx).is_pending());
            }
        }
    }
}

#[test]
/// A wrong-opcode terminal error preserves already delivered callbacks but fails discovery and sends no retry.
fn discovery_mismatched_error_preserves_prior_characteristics_without_completing() {
    let (guard, conn) = connection();
    let client = test_client(&conn);
    let uuid = Uuid::new_short(0x2803);
    let values = RefCell::new(std::vec::Vec::new());
    let mut future = pin!(client.read_by_type(1, 9, &uuid, |handle, _| {
        values.borrow_mut().push(handle);
        ControlFlow::<()>::Continue(())
    }));
    let mut cx = Context::from_waker(Waker::noop());
    assert!(future.as_mut().poll(&mut cx).is_pending());
    expect_request(&guard, &[0x08, 1, 0, 9, 0, 3, 0x28]);
    reply(&client, &[0x09, 7, 2, 0, 0x20, 3, 0, 5, 0x2a]);
    assert!(future.as_mut().poll(&mut cx).is_pending());
    expect_request(&guard, &[0x08, 3, 0, 9, 0, 3, 0x28]);
    reply(&client, &[0x01, 0x10, 0, 0, 0x0a]);
    assert!(matches!(
        future.as_mut().poll(&mut cx),
        Poll::Ready(Err(BleHostError::BleHost(Error::UnexpectedGattResponse)))
    ));
    assert_eq!(&*values.borrow(), &[2]);
    assert!(pin!(guard.0.outbound()).as_mut().poll(&mut cx).is_pending());
}

#[test]
/// Invalid Offset terminates blob reads only when the error names the pending blob opcode.
fn discovery_blob_terminal_error_must_name_the_pending_request() {
    for matching in [false, true] {
        let (guard, conn) = connection();
        let client = test_client(&conn);
        let mut dest = [0; 22];
        let mut future = pin!(client.read_handle_blob(1, 0, &mut dest));
        let mut cx = Context::from_waker(Waker::noop());
        assert!(future.as_mut().poll(&mut cx).is_pending());
        expect_request(&guard, &[0x0c, 1, 0, 0, 0]);
        reply(&client, &[0x01, if matching { 0x0c } else { 0x0a }, 1, 0, 0x07]);
        let result = future.as_mut().poll(&mut cx);
        if matching {
            assert!(matches!(result, Poll::Ready(Ok(0))));
        } else {
            assert!(matches!(
                result,
                Poll::Ready(Err(BleHostError::BleHost(Error::UnexpectedGattResponse)))
            ));
        }
        assert!(pin!(guard.0.outbound()).as_mut().poll(&mut cx).is_pending());
    }
}

#[test]
/// Mixed-width characteristic pages advance past the last declaration and end normally on matching Attribute Not Found.
fn discovery_characteristic_cursor_and_attribute_not_found_are_normal() {
    let (guard, conn) = connection();
    let client = test_client(&conn);
    let uuid = Uuid::new_short(0x2803);
    let values = RefCell::new(std::vec::Vec::new());
    let mut future = pin!(client.read_by_type(1, 9, &uuid, |handle, data| {
        values.borrow_mut().push((handle, data.to_vec()));
        ControlFlow::<()>::Continue(())
    }));
    let mut cx = Context::from_waker(Waker::noop());
    assert!(future.as_mut().poll(&mut cx).is_pending());
    expect_request(&guard, &[0x08, 1, 0, 9, 0, 3, 0x28]);
    reply(
        &client,
        &[0x09, 7, 2, 0, 0x20, 3, 0, 5, 0x2a, 4, 0, 2, 5, 0, 0x3a, 0x2b],
    );
    assert!(future.as_mut().poll(&mut cx).is_pending());
    expect_request(&guard, &[0x08, 5, 0, 9, 0, 3, 0x28]);
    let third = [
        0x09, 21, 6, 0, 2, 7, 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16,
    ];
    reply(&client, &third);
    assert!(future.as_mut().poll(&mut cx).is_pending());
    expect_request(&guard, &[0x08, 7, 0, 9, 0, 3, 0x28]);
    reply(&client, &[0x01, 0x08, 7, 0, 0x0a]);
    assert!(matches!(future.as_mut().poll(&mut cx), Poll::Ready(Ok(None))));
    assert_eq!(
        &*values.borrow(),
        &[
            (2, std::vec![0x20, 3, 0, 5, 0x2a]),
            (4, std::vec![2, 5, 0, 0x3a, 0x2b]),
            (6, third[4..].to_vec()),
        ]
    );
    assert!(pin!(guard.0.outbound()).as_mut().poll(&mut cx).is_pending());
}

#[test]
/// Malformed later pages produce no callbacks from their partial entries and retain callbacks from prior valid pages.
fn discovery_malformed_next_page_preserves_only_prior_complete_responses() {
    let (guard, conn) = connection();
    let client = test_client(&conn);
    let uuid = Uuid::new_short(0x2803);
    let values = RefCell::new(std::vec::Vec::new());
    let mut future = pin!(client.read_by_type(1, 9, &uuid, |handle, _| {
        values.borrow_mut().push(handle);
        ControlFlow::<()>::Continue(())
    }));
    let mut cx = Context::from_waker(Waker::noop());
    reply(
        &client,
        &[0x09, 7, 2, 0, 0x20, 3, 0, 5, 0x2a, 4, 0, 2, 5, 0, 0x3a, 0x2b],
    );
    assert!(future.as_mut().poll(&mut cx).is_pending());
    expect_request(&guard, &[0x08, 1, 0, 9, 0, 3, 0x28]);
    expect_request(&guard, &[0x08, 5, 0, 9, 0, 3, 0x28]);
    reply(&client, &[0x09, 7, 6, 0, 2, 7, 0, 0xaa, 0xaa, 8]);
    assert!(matches!(
        future.as_mut().poll(&mut cx),
        Poll::Ready(Err(BleHostError::BleHost(Error::InvalidValue)))
    ));
    assert_eq!(&*values.borrow(), &[2, 4]);
    assert!(pin!(guard.0.outbound()).as_mut().poll(&mut cx).is_pending());
}

#[test]
/// Descriptor handle65535 completes normally; zero handle rejects.
fn discovery_descriptors_allow_terminal_handle_and_reject_invalid_order() {
    for bytes in [&[0x05, 1, 0xff, 0xff, 2, 0x29][..], &[0x05, 1, 0, 0, 2, 0x29][..]] {
        let (_guard, conn) = connection();
        let client = test_client(&conn);
        reply(&client, bytes);
        let mut future = pin!(client.find_information(1, 0xffff, |_, _| ControlFlow::<()>::Continue(())));
        let result = future.as_mut().poll(&mut Context::from_waker(Waker::noop()));
        if bytes[2] == 0xff {
            assert!(matches!(result, Poll::Ready(Ok(None))));
        } else {
            assert!(matches!(result, Poll::Ready(Err(_))));
        }
    }
}

#[test]
/// A full destination terminates blob reading with its copied byte count instead of requesting an unnecessary continuation.
fn discovery_blob_stops_when_destination_full() {
    let (_guard, conn) = connection();
    conn.set_att_mtu(23);
    let client = test_client(&conn);
    let mut bytes = [42; 23];
    bytes[0] = 0x0d;
    reply(&client, &bytes);
    let mut dest = [0; 22];
    let mut future = pin!(client.read_handle_blob(1, 0, &mut dest));
    assert!(matches!(
        future.as_mut().poll(&mut Context::from_waker(Waker::noop())),
        Poll::Ready(Ok(22))
    ));
}

#[test]
/// A full-size page near offset65535 reports overflow instead of wrapping the next request.
fn discovery_blob_offset_cannot_wrap() {
    let (_guard, conn) = connection();
    conn.set_att_mtu(23);
    let client = test_client(&conn);
    let mut bytes = [42; 23];
    bytes[0] = 0x0d;
    reply(&client, &bytes);
    let mut dest = [0; 64];
    let mut future = pin!(client.read_handle_blob(1, 0xfff0, &mut dest));
    assert!(matches!(
        future.as_mut().poll(&mut Context::from_waker(Waker::noop())),
        Poll::Ready(Err(_))
    ));
}
extern crate std;

/// Returns a client with empty N-service cache for accepted `connection`, without MTU exchange or controller I/O; duplicate client ownership panics.
fn service_client<const N: usize>(
    connection: &Connection<'static, DefaultPacketPool>,
) -> GattClient<'static, crate::mock_controller::MockController, DefaultPacketPool, N> {
    GattClient {
        known_services: RefCell::new(Vec::new()),
        _phantom: PhantomData,
        connection: ClientConnection::acquire(connection).unwrap(),
        response_channel: Channel::new(),
        notifications: PubSubChannel::new(),
    }
}

#[test]
/// Sparse16/128-bit service pages publish one complete cache only after terminal progress, preserving inclusive ranges.
fn discovery_services_mix_uuid_widths_and_sparse_groups_across_pages() {
    let (guard, conn) = connection();
    let client = service_client::<3>(&conn);
    let mut future = pin!(client.services());
    let mut cx = Context::from_waker(Waker::noop());
    assert!(future.as_mut().poll(&mut cx).is_pending());
    expect_request(&guard, &[0x10, 1, 0, 0xff, 0xff, 0, 0x28]);
    reply(&client, &[0x11, 6, 2, 0, 4, 0, 1, 0x18]);
    assert!(future.as_mut().poll(&mut cx).is_pending());
    expect_request(&guard, &[0x10, 5, 0, 0xff, 0xff, 0, 0x28]);
    assert!(
        client.known_services.borrow().is_empty(),
        "unfinished discovery must not cache a prefix"
    );
    reply(
        &client,
        &[
            0x11, 20, 11, 0, 15, 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16,
        ],
    );
    assert!(future.as_mut().poll(&mut cx).is_pending());
    expect_request(&guard, &[0x10, 16, 0, 0xff, 0xff, 0, 0x28]);
    reply(&client, &[0x11, 6, 20, 0, 0xff, 0xff, 0, 0x18]);
    let Poll::Ready(Ok(rows)) = future.as_mut().poll(&mut cx) else {
        panic!("legal mixed services failed");
    };
    assert_eq!(
        rows.iter()
            .map(ServiceHandle::handle_range)
            .collect::<std::vec::Vec<_>>(),
        [2..=4, 11..=15, 20..=0xffff]
    );
    assert_eq!(
        rows[1].uuid().as_raw(),
        &[1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16]
    );
    assert_eq!(client.known_services.borrow().len(), 3);
    assert!(pin!(guard.0.outbound()).as_mut().poll(&mut cx).is_pending());
}

#[test]
/// Filtered service pages reject overlap/reversal and cache only after valid progress reaches a matching terminal error.
fn discovery_filtered_group_pages_validate_progress_and_cache_only_success() {
    for (second_start, second_end, success) in [(1u8, 9u8, false), (4, 9, false), (5, 4, false), (5, 9, true)] {
        let (guard, conn) = connection();
        let client = service_client::<2>(&conn);
        let uuid = Uuid::new_short(0x1801);
        let mut future = pin!(client.services_by_uuid(&uuid));
        let mut cx = Context::from_waker(Waker::noop());
        assert!(future.as_mut().poll(&mut cx).is_pending());
        expect_request(&guard, &[0x06, 1, 0, 0xff, 0xff, 0, 0x28, 1, 0x18]);
        reply(&client, &[0x07, 1, 0, 4, 0]);
        assert!(future.as_mut().poll(&mut cx).is_pending());
        expect_request(&guard, &[0x06, 5, 0, 0xff, 0xff, 0, 0x28, 1, 0x18]);
        assert!(client.known_services.borrow().is_empty());
        reply(&client, &[0x07, second_start, 0, second_end, 0]);
        if success {
            assert!(future.as_mut().poll(&mut cx).is_pending());
            expect_request(&guard, &[0x06, 10, 0, 0xff, 0xff, 0, 0x28, 1, 0x18]);
            reply(&client, &[0x01, 0x06, 10, 0, 0x0a]);
            let Poll::Ready(Ok(rows)) = future.as_mut().poll(&mut cx) else {
                panic!("valid pagination failed");
            };
            assert_eq!(rows.len(), 2);
            assert_eq!(rows[1].handle_range(), 5..=9);
            assert_eq!(client.known_services.borrow().len(), 2);
        } else {
            assert!(matches!(future.as_mut().poll(&mut cx), Poll::Ready(Err(_))));
            assert!(client.known_services.borrow().is_empty());
        }
        assert!(pin!(guard.0.outbound()).as_mut().poll(&mut cx).is_pending());
    }
}

#[test]
/// Caller-buffer and owned APIs emit identical sparse mixed-width queries, rows and complete caches.
fn discovery_services_into_matches_owned_api_and_replaces_output() {
    for caller_buffer in [false, true] {
        let (guard, conn) = connection();
        let client = service_client::<3>(&conn);
        let mut output = Vec::new();
        output
            .push(ServiceHandle {
                start: 99,
                end: 100,
                uuid: Uuid::new_short(0x180F),
            })
            .unwrap();
        let mut cx = Context::from_waker(Waker::noop());
        {
            let mut future = pin!(async {
                if caller_buffer {
                    client.services_into(&mut output).await
                } else {
                    output = client.services().await?;
                    Ok(())
                }
            });
            assert!(future.as_mut().poll(&mut cx).is_pending());
            expect_request(&guard, &[0x10, 1, 0, 0xff, 0xff, 0, 0x28]);
            reply(&client, &[0x11, 6, 2, 0, 4, 0, 1, 0x18]);
            assert!(future.as_mut().poll(&mut cx).is_pending());
            expect_request(&guard, &[0x10, 5, 0, 0xff, 0xff, 0, 0x28]);
            assert!(client.known_services.borrow().is_empty());
            reply(
                &client,
                &[
                    0x11, 20, 11, 0, 15, 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16,
                ],
            );
            assert!(future.as_mut().poll(&mut cx).is_pending());
            expect_request(&guard, &[0x10, 16, 0, 0xff, 0xff, 0, 0x28]);
            assert!(client.known_services.borrow().is_empty());
            reply(&client, &[0x11, 6, 20, 0, 0xff, 0xff, 0, 0x18]);
            assert!(matches!(future.as_mut().poll(&mut cx), Poll::Ready(Ok(()))));
        }
        assert_eq!(
            output
                .iter()
                .map(ServiceHandle::handle_range)
                .collect::<std::vec::Vec<_>>(),
            [2..=4, 11..=15, 20..=0xffff]
        );
        assert_eq!(
            output[1].uuid().as_raw(),
            &[1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16]
        );
        assert_eq!(client.known_services.borrow().as_slice(), output.as_slice());
        assert!(pin!(guard.0.outbound()).as_mut().poll(&mut cx).is_pending());
    }
}

#[test]
/// Later malformed, ATT, capacity and cancellation outcomes retain only unpublished rows and leave prior cache intact.
fn discovery_services_into_partial_failures_and_cancellation_do_not_publish_cache() {
    for failure in 0..5 {
        let (guard, conn) = connection();
        let client = service_client::<1>(&conn);
        let previous = ServiceHandle {
            start: 50,
            end: 55,
            uuid: Uuid::new_short(0x180F),
        };
        client.known_services.borrow_mut().push(previous.clone()).unwrap();
        let mut output = Vec::new();
        output.push(previous.clone()).unwrap();
        let mut cx = Context::from_waker(Waker::noop());
        {
            let mut future = pin!(client.services_into(&mut output));
            reply(&client, &[0x11, 6, 1, 0, 4, 0, 1, 0x18]);
            assert!(future.as_mut().poll(&mut cx).is_pending());
            expect_request(&guard, &[0x10, 1, 0, 0xff, 0xff, 0, 0x28]);
            expect_request(&guard, &[0x10, 5, 0, 0xff, 0xff, 0, 0x28]);
            assert_eq!(
                client.known_services.borrow().as_slice(),
                core::slice::from_ref(&previous)
            );
            if failure != 3 {
                reply(
                    &client,
                    match failure {
                        0 => &[0x11, 6, 4, 0, 9, 0, 1, 0x18],
                        1 => &[0x01, 0x10, 5, 0, 0x05],
                        2 => &[0x11, 6, 5, 0, 0xff, 0xff, 1, 0x18],
                        _ => &[0x01, 0x10, 5, 0, 0x0a],
                    },
                );
                let Poll::Ready(Err(BleHostError::BleHost(error))) = future.as_mut().poll(&mut cx) else {
                    panic!("partial discovery must fail");
                };
                assert_eq!(
                    error,
                    match failure {
                        0 => Error::UnexpectedGattResponse,
                        1 => Error::Att(att::AttErrorCode::new(0x05)),
                        _ => Error::InsufficientSpace,
                    }
                );
            }
        }
        assert_eq!(output.len(), 1);
        assert_eq!(output[0].handle_range(), 1..=4);
        assert_eq!(output[0].uuid(), Uuid::new_short(0x1801));
        assert_eq!(client.known_services.borrow().as_slice(), &[previous]);
        if failure == 3 {
            assert!(
                !conn.is_connected(),
                "canceling the pending ATT request must close its bearer"
            );
        }
        assert!(pin!(guard.0.outbound()).as_mut().poll(&mut cx).is_pending());
    }
}

#[test]
/// First-poll clearing and an empty terminal result replace stale output without clearing the prior cache.
fn discovery_services_into_empty_result_replaces_output_without_erasing_cache() {
    let (guard, conn) = connection();
    let client = service_client::<1>(&conn);
    let previous = ServiceHandle {
        start: 50,
        end: 55,
        uuid: Uuid::new_short(0x180F),
    };
    client.known_services.borrow_mut().push(previous.clone()).unwrap();
    let mut output = Vec::new();
    output.push(previous.clone()).unwrap();
    {
        let mut future = pin!(client.services_into(&mut output));
        reply(&client, &[0x01, 0x10, 1, 0, 0x0a]);
        assert!(matches!(
            future.as_mut().poll(&mut Context::from_waker(Waker::noop())),
            Poll::Ready(Ok(()))
        ));
    }
    assert!(output.is_empty());
    assert_eq!(client.known_services.borrow().as_slice(), &[previous]);
    expect_request(&guard, &[0x10, 1, 0, 0xff, 0xff, 0, 0x28]);
}

#[test]
/// Filtered and unfiltered discovery exceeding bounded output capacity return InsufficientSpace with cache unchanged.
fn discovery_service_capacity_failure_never_publishes_a_partial_cache() {
    for filtered in [false, true] {
        let (guard, conn) = connection();
        let client = service_client::<1>(&conn);
        let uuid = Uuid::new_short(0x1801);
        let mut future = pin!(async {
            if filtered {
                client.services_by_uuid(&uuid).await
            } else {
                client.services().await
            }
        });
        let mut cx = Context::from_waker(Waker::noop());
        reply(
            &client,
            if filtered {
                &[0x07, 1, 0, 4, 0, 5, 0, 0xff, 0xff][..]
            } else {
                &[0x11, 6, 1, 0, 4, 0, 1, 0x18, 5, 0, 0xff, 0xff, 0, 0x18][..]
            },
        );
        assert!(matches!(
            future.as_mut().poll(&mut cx),
            Poll::Ready(Err(BleHostError::BleHost(Error::InsufficientSpace)))
        ));
        assert!(client.known_services.borrow().is_empty());
        expect_request(
            &guard,
            if filtered {
                &[0x06, 1, 0, 0xff, 0xff, 0, 0x28, 1, 0x18][..]
            } else {
                &[0x10, 1, 0, 0xff, 0xff, 0, 0x28][..]
            },
        );
        assert!(pin!(guard.0.outbound()).as_mut().poll(&mut cx).is_pending());
    }
}

#[test]
/// Cache merge deduplicates identical entries; insufficient combined capacity preserves the entire previous cache.
fn discovery_service_cache_merge_preserves_existing_entries_on_overflow() {
    for filtered in [false, true] {
        for overlap in [false, true] {
            let (guard, conn) = connection();
            let client = service_client::<2>(&conn);
            let uuid = Uuid::new_short(0x1801);
            let previous = ServiceHandle { start: 1, end: 4, uuid };
            client.known_services.borrow_mut().push(previous.clone()).unwrap();
            let mut future = pin!(async {
                if filtered {
                    client.services_by_uuid(&uuid).await
                } else {
                    client.services().await
                }
            });
            let mut cx = Context::from_waker(Waker::noop());
            let (first, end) = if overlap { (1, 4) } else { (5, 9) };
            let filtered_reply = [0x07, first, 0, end, 0, 10, 0, 0xff, 0xff];
            let all_reply = [0x11, 6, first, 0, end, 0, 1, 0x18, 10, 0, 0xff, 0xff, 1, 0x18];
            reply(&client, if filtered { &filtered_reply[..] } else { &all_reply[..] });
            let result = future.as_mut().poll(&mut cx);
            if overlap {
                let Poll::Ready(Ok(rows)) = result else {
                    panic!("duplicate cache entry consumed capacity")
                };
                assert_eq!(rows.len(), 2);
                assert_eq!(client.known_services.borrow().len(), 2);
                assert_eq!(client.known_services.borrow()[0], previous);
            } else {
                assert!(matches!(
                    result,
                    Poll::Ready(Err(BleHostError::BleHost(Error::InsufficientSpace)))
                ));
                assert_eq!(client.known_services.borrow().as_slice(), &[previous]);
            }
            expect_request(
                &guard,
                if filtered {
                    &[0x06, 1, 0, 0xff, 0xff, 0, 0x28, 1, 0x18][..]
                } else {
                    &[0x10, 1, 0, 0xff, 0xff, 0, 0x28][..]
                },
            );
            assert!(pin!(guard.0.outbound()).as_mut().poll(&mut cx).is_pending());
        }
    }
}

#[test]
/// Single and bulk characteristic discovery reject a value overlapping the next declaration before descriptor traffic.
fn discovery_overlapping_characteristic_value_stops_both_apis_before_descriptors() {
    for bulk in [false, true] {
        let (guard, conn) = connection();
        let client = test_client(&conn);
        let service = ServiceHandle {
            start: 1,
            end: 10,
            uuid: Uuid::new_short(0x1800),
        };
        let uuid = Uuid::new_short(0x2a00);
        let mut future = pin!(async {
            if bulk {
                client.characteristics::<2>(&service).await.map(|_| ())
            } else {
                client.characteristic_by_uuid::<[u8]>(&service, &uuid).await.map(|_| ())
            }
        });
        let mut cx = Context::from_waker(Waker::noop());
        reply(&client, &[0x09, 7, 2, 0, 0x10, 5, 0, 0, 0x2a, 4, 0, 2, 6, 0, 1, 0x2a]);
        assert!(matches!(future.as_mut().poll(&mut cx), Poll::Ready(Err(_))));
        expect_request(&guard, &[0x08, 1, 0, 10, 0, 3, 0x28]);
        assert!(pin!(guard.0.outbound()).as_mut().poll(&mut cx).is_pending());
    }
}

#[test]
/// CCCD lookup starts after the value and ends before the next declaration, preserving sparse handle bounds.
fn discovery_sparse_characteristic_cccd_query_stays_inside_its_definition() {
    let (guard, conn) = connection();
    let client = test_client(&conn);
    let service = ServiceHandle {
        start: 1,
        end: 12,
        uuid: Uuid::new_short(0x1800),
    };
    let uuid = Uuid::new_short(0x2a00);
    let mut future = pin!(client.characteristic_by_uuid::<[u8]>(&service, &uuid));
    let mut cx = Context::from_waker(Waker::noop());
    reply(&client, &[0x09, 7, 2, 0, 0x10, 4, 0, 0, 0x2a, 8, 0, 2, 10, 0, 1, 0x2a]);
    assert!(future.as_mut().poll(&mut cx).is_pending());
    expect_request(&guard, &[0x08, 1, 0, 12, 0, 3, 0x28]);
    expect_request(&guard, &[0x04, 5, 0, 7, 0]);
    reply(&client, &[0x05, 1, 6, 0, 2, 0x29]);
    let Poll::Ready(Ok(row)) = future.as_mut().poll(&mut cx) else {
        panic!("valid CCCD rejected");
    };
    assert_eq!((row.handle, row.end_handle, row.cccd_handle), (4, 7, Some(6)));
    assert!(pin!(guard.0.outbound()).as_mut().poll(&mut cx).is_pending());
}

#[test]
/// A terminal read value is valid; notification capability at handle65535 fails missing-CCCD lookup without overflow.
fn discovery_terminal_characteristic_value_is_valid_but_cannot_have_cccd() {
    for notify in [false, true] {
        let (guard, conn) = connection();
        let client = test_client(&conn);
        let service = ServiceHandle {
            start: 0xfffc,
            end: 0xffff,
            uuid: Uuid::new_short(0x1800),
        };
        let uuid = Uuid::new_short(0x2a00);
        let mut future = pin!(client.characteristic_by_uuid::<[u8]>(&service, &uuid));
        let mut cx = Context::from_waker(Waker::noop());
        reply(
            &client,
            &[0x09, 7, 0xfe, 0xff, if notify { 0x10 } else { 2 }, 0xff, 0xff, 0, 0x2a],
        );
        let result = future.as_mut().poll(&mut cx);
        if notify {
            assert!(matches!(
                result,
                Poll::Ready(Err(BleHostError::BleHost(Error::NotFound)))
            ));
        } else {
            let Poll::Ready(Ok(row)) = result else {
                panic!("terminal value rejected");
            };
            assert_eq!((row.handle, row.end_handle), (0xffff, 0xffff));
        }
        expect_request(&guard, &[0x08, 0xfc, 0xff, 0xff, 0xff, 3, 0x28]);
        assert!(pin!(guard.0.outbound()).as_mut().poll(&mut cx).is_pending());
    }
}

#[test]
/// An initial unused handle gap is not revisited after a terminal service response.
fn discovery_all_services_does_not_search_backwards_through_unused_handles() {
    let (guard, conn) = connection();
    let client = test_client(&conn);
    let mut future = pin!(client.services());
    let mut cx = Context::from_waker(Waker::noop());
    assert!(future.as_mut().poll(&mut cx).is_pending());
    expect_request(&guard, &[0x10, 1, 0, 0xff, 0xff, 0, 0x28]);
    reply(&client, &[0x11, 6, 2, 0, 0xff, 0xff, 1, 0x18]);
    let Poll::Ready(Ok(services)) = future.as_mut().poll(&mut cx) else {
        panic!("queried an already searched gap");
    };
    assert_eq!(services[0].handle_range(), 2..=0xffff);
    assert!(pin!(guard.0.outbound()).as_mut().poll(&mut cx).is_pending());
}

#[test]
/// Descriptor listing at handle65535 returns empty and lookup returns NotFound without sending a wrapped request.
fn discovery_terminal_value_has_no_wrapped_descriptor_query() {
    for lookup in [false, true] {
        let (guard, conn) = connection();
        let client = test_client(&conn);
        let characteristic = Characteristic::<[u8]> {
            handle: 0xffff,
            end_handle: 0xffff,
            props: CharacteristicProps::from([CharacteristicProp::Read]),
            cccd_handle: None,
            uuid: Uuid::new_short(0x2a00),
            phantom: PhantomData,
        };
        let uuid = Uuid::new_short(0x2901);
        let mut future = pin!(async {
            if lookup {
                client
                    .descriptor_by_uuid::<[u8], [u8]>(&characteristic, &uuid)
                    .await
                    .map(|_| 1)
            } else {
                client
                    .descriptors::<[u8], 1>(&characteristic)
                    .await
                    .map(|rows| rows.len())
            }
        });
        let mut cx = Context::from_waker(Waker::noop());
        let result = future.as_mut().poll(&mut cx);
        if lookup {
            assert!(matches!(
                result,
                Poll::Ready(Err(BleHostError::BleHost(Error::NotFound)))
            ));
        } else {
            assert!(matches!(result, Poll::Ready(Ok(0))));
        }
        assert!(pin!(guard.0.outbound()).as_mut().poll(&mut cx).is_pending());
    }
}

#[test]
/// Filtered zero/reversed service ranges reject before cache publication.
fn discovery_filtered_services_reject_invalid_ranges_before_caching() {
    for (start, end) in [(0u16, 1u16), (2, 1)] {
        let (guard, conn) = connection();
        let client = test_client(&conn);
        let uuid = Uuid::new_short(0x1801);
        let mut future = pin!(client.services_by_uuid(&uuid));
        let mut cx = Context::from_waker(Waker::noop());
        assert!(future.as_mut().poll(&mut cx).is_pending());
        expect_request(&guard, &[0x06, 1, 0, 0xff, 0xff, 0, 0x28, 1, 0x18]);
        let [a, b] = start.to_le_bytes();
        let [c, d] = end.to_le_bytes();
        reply(&client, &[0x07, a, b, c, d]);
        assert!(
            matches!(future.as_mut().poll(&mut cx), Poll::Ready(Err(_))),
            "accepted {start}..{end}"
        );
        assert!(client.known_services.borrow().is_empty());
        assert!(pin!(guard.0.outbound()).as_mut().poll(&mut cx).is_pending());
    }
}

#[test]
/// Single/bulk discovery reject declaration/value positions outside the containing service or ordered definition.
fn discovery_characteristic_apis_reject_value_outside_its_definition() {
    for bulk in [false, true] {
        for (declaration, value) in [(1u8, 2u8), (2, 0), (2, 2), (2, 11), (10, 11)] {
            let (guard, conn) = connection();
            let client = test_client(&conn);
            let service = ServiceHandle {
                start: 1,
                end: 10,
                uuid: Uuid::new_short(0x1800),
            };
            let uuid = Uuid::new_short(0x2a00);
            let mut future = pin!(async {
                if bulk {
                    client.characteristics::<4>(&service).await.map(|_| ())
                } else {
                    client.characteristic_by_uuid::<[u8]>(&service, &uuid).await.map(|_| ())
                }
            });
            let mut cx = Context::from_waker(Waker::noop());
            assert!(future.as_mut().poll(&mut cx).is_pending());
            expect_request(&guard, &[0x08, 1, 0, 10, 0, 3, 0x28]);
            reply(&client, &[0x09, 7, declaration, 0, 2, value, 0, 0, 0x2a]);
            assert!(
                matches!(future.as_mut().poll(&mut cx), Poll::Ready(Err(_))),
                "bulk={bulk} decl={declaration} value={value}"
            );
            assert!(pin!(guard.0.outbound()).as_mut().poll(&mut cx).is_pending());
        }
    }
}

#[test]
/// Single/bulk characteristic end bounds stop immediately before the next declaration despite sparse value handles.
fn discovery_characteristic_apis_stop_before_next_declaration_with_sparse_handles() {
    for bulk in [false, true] {
        let (guard, conn) = connection();
        let client = test_client(&conn);
        let service = ServiceHandle {
            start: 1,
            end: 12,
            uuid: Uuid::new_short(0x1800),
        };
        let uuid = Uuid::new_short(0x2a00);
        let mut future = pin!(async {
            if bulk {
                client
                    .characteristics::<4>(&service)
                    .await
                    .map(|rows| rows[0].end_handle)
            } else {
                client
                    .characteristic_by_uuid::<[u8]>(&service, &uuid)
                    .await
                    .map(|row| row.end_handle)
            }
        });
        let mut cx = Context::from_waker(Waker::noop());
        assert!(future.as_mut().poll(&mut cx).is_pending());
        expect_request(&guard, &[0x08, 1, 0, 12, 0, 3, 0x28]);
        reply(&client, &[0x09, 7, 2, 0, 2, 4, 0, 0, 0x2a, 8, 0, 2, 10, 0, 1, 0x2a]);
        if bulk {
            assert!(future.as_mut().poll(&mut cx).is_pending());
            expect_request(&guard, &[0x08, 9, 0, 12, 0, 3, 0x28]);
            reply(&client, &[0x01, 0x08, 9, 0, 0x0a]);
        }
        assert!(
            matches!(future.as_mut().poll(&mut cx), Poll::Ready(Ok(7))),
            "bulk={bulk}"
        );
        assert!(pin!(guard.0.outbound()).as_mut().poll(&mut cx).is_pending());
    }
}

#[test]
/// Descriptor results beyond capacity return InsufficientSpace rather than a truncated success.
fn discovery_descriptor_capacity_is_not_reported_as_complete() {
    let (guard, conn) = connection();
    let client = test_client(&conn);
    let characteristic = Characteristic::<[u8]> {
        handle: 3,
        end_handle: 5,
        props: CharacteristicProps::from([CharacteristicProp::Read]),
        cccd_handle: None,
        uuid: Uuid::new_short(0x2a00),
        phantom: PhantomData,
    };
    let mut future = pin!(client.descriptors::<[u8], 1>(&characteristic));
    let mut cx = Context::from_waker(Waker::noop());
    assert!(future.as_mut().poll(&mut cx).is_pending());
    expect_request(&guard, &[0x04, 4, 0, 5, 0]);
    reply(&client, &[0x05, 1, 4, 0, 1, 0x29, 5, 0, 2, 0x29]);
    assert!(matches!(
        future.as_mut().poll(&mut cx),
        Poll::Ready(Err(BleHostError::BleHost(Error::InsufficientSpace)))
    ));
}

/// Drains queued request packets and transferred slot leases at fixture teardown.
struct Drain(
    /// Leaked manager whose outbound resources must be returned to the shared pool.
    &'static crate::connection_manager::ConnectionManager<'static, DefaultPacketPool>,
);
impl Drop for Drain {
    /// Consumes all immediately queued packets/leases until Pending; does not wait for new work or initiate controller I/O.
    fn drop(&mut self) {
        loop {
            let mut next = pin!(self.0.outbound());
            match next.as_mut().poll(&mut Context::from_waker(Waker::noop())) {
                Poll::Ready(value) => drop(value),
                Poll::Pending => break,
            }
        }
    }
}
/// Returns a queue-drain guard and accepted fake static connection from a leaked isolated manager; setup failure panics.
fn connection() -> (Drain, Connection<'static, DefaultPacketPool>) {
    use bt_hci::param::{AddrKind, BdAddr, ConnHandle, LeConnRole};
    let mgr = crate::connection_manager::tests::setup();
    mgr.connect(
        ConnHandle::new(0),
        crate::Address::new(AddrKind::RANDOM, BdAddr::new([1; 6])),
        LeConnRole::Central,
        crate::connection::ConnParams::new(),
    )
    .unwrap();
    let Poll::Ready(conn) = mgr.poll_accept(LeConnRole::Central, &[], None) else {
        panic!("connection")
    };
    (Drain(mgr), conn)
}

/// UUID32 service filtering emits its Bluetooth-base UUID128 value, matching the legal service declaration encoding.
#[test]
fn service_uuid32_filter_uses_expanded_wire_value() {
    let (_guard, conn) = connection();
    let client = test_client(&conn);
    let uuid = Uuid::Uuid32(0x12345678u32.to_le_bytes());
    let mut future = pin!(client.services_by_uuid(&uuid));
    let mut cx = Context::from_waker(Waker::noop());
    assert!(future.as_mut().poll(&mut cx).is_pending());
    let mut outbound = pin!(conn.manager().outbound());
    let Poll::Ready((_, packet)) = outbound.as_mut().poll(&mut cx) else {
        panic!("service request queued");
    };
    assert_eq!(&packet.as_ref()[4..11], &[0x06, 1, 0, 0xff, 0xff, 0, 0x28]);
    assert_eq!(
        &packet.as_ref()[11..],
        &[0xfb, 0x34, 0x9b, 0x5f, 0x80, 0, 0, 0x80, 0, 0x10, 0, 0, 0x78, 0x56, 0x34, 0x12]
    );
}
