use super::*;
use crate::attribute::{Characteristic, CharacteristicProp, PermissionLevel, Service};
use crate::connection::ConnParams;
use crate::mock_controller::MockController;
use crate::prelude::DefaultPacketPool;
use crate::{Address, HostResources};
use bt_hci::param::{AddrKind, BdAddr, ConnHandle, LeConnRole};
use core::future::Future;
use core::pin::pin;
use core::task::{Context, Poll, Waker};
use embassy_sync::blocking_mutex::raw::NoopRawMutex;

fn server() -> (
    AttributeServer<'static, NoopRawMutex, DefaultPacketPool, 8, 1>,
    Characteristic<u8>,
) {
    let mut table = AttributeTable::new();
    let characteristic = table
        .add_service(Service { uuid: 0x180fu16.into() })
        .add_characteristic_small(
            0x2a19u16,
            [
                CharacteristicProp::Notify,
                CharacteristicProp::Indicate,
                CharacteristicProp::Write,
            ],
            7u8,
        )
        .build();
    (AttributeServer::new(table), characteristic)
}

fn subscribe(
    server: &AttributeServer<'_, NoopRawMutex, DefaultPacketPool, 8, 1>,
    conn: &Connection<'_, DefaultPacketPool>,
    cccd: u16,
    bits: u8,
) {
    server.connect(conn).unwrap();
    let mut response = [0; 23];
    let len = server
        .process(
            conn,
            &AttClient::Request(AttReq::Write {
                handle: cccd,
                data: &[bits, 0],
            }),
            &mut response,
        )
        .unwrap()
        .unwrap();
    assert_eq!(&response[..len], &[att::ATT_WRITE_RSP]);
}

#[test]
fn bulk_notify_checks_permissions_and_does_not_hide_assembly_failure() {
    for oversized in [false, true] {
        let mut resources = HostResources::<DefaultPacketPool, 1, 1>::new();
        let stack = crate::new(MockController::new(), &mut resources).build();
        let manager = stack.host().connections();
        manager
            .connect(
                ConnHandle::new(0),
                Address::new(AddrKind::PUBLIC, BdAddr::new([0; 6])),
                LeConnRole::Central,
                ConnParams::new(),
            )
            .unwrap();
        let Poll::Ready(conn) = manager.poll_accept(LeConnRole::Central, &[], None) else {
            panic!()
        };
        let (server, characteristic) = server();
        let cccd = characteristic.cccd_handle.unwrap();
        subscribe(&server, &conn, cccd, 1);
        if !oversized {
            server.table().with_attribute_mut(cccd, |att| {
                att.permissions.write = PermissionLevel::EncryptionRequired;
            });
        }
        let data = [0x42; DefaultPacketPool::MTU + 1];
        let data = if oversized { &data[..] } else { &data[..1] };
        let mut send = pin!(server.notify(&stack, characteristic.handle, data));
        assert!(matches!(
            send.as_mut().poll(&mut Context::from_waker(Waker::noop())),
            Poll::Ready(Err(_))
        ));
        let mut outbound = pin!(manager.outbound());
        assert!(outbound
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop()))
            .is_pending());
    }
}

#[test]
fn bulk_indications_wait_for_confirmation_and_cancel_closes_bearer() {
    for confirm in [false, true] {
        let mut resources = HostResources::<DefaultPacketPool, 1, 1>::new();
        let stack = crate::new(MockController::new(), &mut resources).build();
        let manager = stack.host().connections();
        manager
            .connect(
                ConnHandle::new(0),
                Address::new(AddrKind::PUBLIC, BdAddr::new([0; 6])),
                LeConnRole::Central,
                ConnParams::new(),
            )
            .unwrap();
        let Poll::Ready(conn) = manager.poll_accept(LeConnRole::Central, &[], None) else {
            panic!()
        };
        let (server, characteristic) = server();
        subscribe(&server, &conn, characteristic.cccd_handle.unwrap(), 2);
        {
            let mut send = pin!(server.notify(&stack, characteristic.handle, &[8u8]));
            assert!(send.as_mut().poll(&mut Context::from_waker(Waker::noop())).is_pending());
            let mut outbound = pin!(manager.outbound());
            let Poll::Ready((_, packet)) = outbound.as_mut().poll(&mut Context::from_waker(Waker::noop())) else {
                panic!("indication must be queued before awaiting confirmation");
            };
            assert_eq!(packet.as_ref()[4], att::ATT_HANDLE_VALUE_IND);
            if confirm {
                manager.signal_indication_confirmation(conn.handle());
                assert!(matches!(
                    send.as_mut().poll(&mut Context::from_waker(Waker::noop())),
                    Poll::Ready(Ok(()))
                ));
            }
        }
        assert_eq!(conn.is_connected(), confirm);
    }
}

#[test]
fn concurrent_indications_serialize_and_idle_confirmations_cannot_complete_a_send() {
    let mut resources = HostResources::<DefaultPacketPool, 1, 1>::new();
    let stack = crate::new(MockController::new(), &mut resources).build();
    let manager = stack.host().connections();
    manager
        .connect(
            ConnHandle::new(0),
            Address::new(AddrKind::PUBLIC, BdAddr::new([0; 6])),
            LeConnRole::Central,
            ConnParams::new(),
        )
        .unwrap();
    let Poll::Ready(conn) = manager.poll_accept(LeConnRole::Central, &[], None) else {
        panic!()
    };
    let (server, characteristic) = server();
    subscribe(&server, &conn, characteristic.cccd_handle.unwrap(), 2);
    let mut cx = Context::from_waker(Waker::noop());
    // A confirmation received with no transaction must not be cached.
    manager.signal_indication_confirmation(conn.handle());
    let mut first = pin!(server.notify(&stack, characteristic.handle, &[1u8]));
    assert!(first.as_mut().poll(&mut cx).is_pending());
    assert!(pin!(manager.outbound()).as_mut().poll(&mut cx).is_ready());
    {
        let mut cancelled_waiter = pin!(server.notify(&stack, characteristic.handle, &[2u8]));
        assert!(cancelled_waiter.as_mut().poll(&mut cx).is_pending());
        assert!(pin!(manager.outbound()).as_mut().poll(&mut cx).is_pending());
    }
    assert!(
        conn.is_connected(),
        "cancelling an unsent waiter must preserve the active transaction"
    );
    let mut second = pin!(server.notify(&stack, characteristic.handle, &[3u8]));
    assert!(second.as_mut().poll(&mut cx).is_pending());
    manager.signal_indication_confirmation(conn.handle());
    assert!(matches!(first.as_mut().poll(&mut cx), Poll::Ready(Ok(()))));
    assert!(second.as_mut().poll(&mut cx).is_pending());
    let Poll::Ready((_, packet)) = pin!(manager.outbound()).as_mut().poll(&mut cx) else {
        panic!()
    };
    assert_eq!(&packet.as_ref()[4..], &[att::ATT_HANDLE_VALUE_IND, 3, 0, 3]);
    manager.signal_indication_confirmation(conn.handle());
    assert!(matches!(second.as_mut().poll(&mut cx), Poll::Ready(Ok(()))));
    assert!(conn.is_connected());
}

#[test]
fn indication_assembly_failure_releases_slot_without_disconnect_or_false_success() {
    let mut resources = HostResources::<DefaultPacketPool, 1, 1>::new();
    let stack = crate::new(MockController::new(), &mut resources).build();
    let manager = stack.host().connections();
    manager
        .connect(
            ConnHandle::new(0),
            Address::new(AddrKind::PUBLIC, BdAddr::new([0; 6])),
            LeConnRole::Central,
            ConnParams::new(),
        )
        .unwrap();
    let Poll::Ready(conn) = manager.poll_accept(LeConnRole::Central, &[], None) else {
        panic!()
    };
    let (server, characteristic) = server();
    subscribe(&server, &conn, characteristic.cccd_handle.unwrap(), 2);
    let data = [0u8; DefaultPacketPool::MTU + 1];
    let mut cx = Context::from_waker(Waker::noop());
    assert!(matches!(
        pin!(server.notify(&stack, characteristic.handle, &data))
            .as_mut()
            .poll(&mut cx),
        Poll::Ready(Err(_))
    ));
    assert!(conn.is_connected());
    assert!(pin!(manager.outbound()).as_mut().poll(&mut cx).is_pending());
    // A later, valid operation can acquire the released slot.
    let mut valid = pin!(server.notify(&stack, characteristic.handle, &[7u8]));
    assert!(valid.as_mut().poll(&mut cx).is_pending());
    assert!(pin!(manager.outbound()).as_mut().poll(&mut cx).is_ready());
    manager.signal_indication_confirmation(conn.handle());
    conn.disconnect();
    assert!(matches!(
        valid.as_mut().poll(&mut cx),
        Poll::Ready(Err(Error::Disconnected))
    ));
}

#[test]
#[cfg(feature = "att-queued-writes")]
fn prepared_writes_check_permissions_and_reject_invalid_execute_flags_without_clearing() {
    let mut resources = HostResources::<DefaultPacketPool, 1, 1>::new();
    let stack = crate::new(MockController::new(), &mut resources).build();
    let manager = stack.host().connections();
    manager
        .connect(
            ConnHandle::new(0),
            Address::new(AddrKind::PUBLIC, BdAddr::new([0; 6])),
            LeConnRole::Central,
            ConnParams::new(),
        )
        .unwrap();
    let Poll::Ready(conn) = manager.poll_accept(LeConnRole::Central, &[], None) else {
        panic!()
    };
    let (server, characteristic) = server();
    server.connect(&conn).unwrap();
    server.table().with_attribute_mut(characteristic.handle, |a| {
        a.permissions.write = PermissionLevel::NotAllowed;
    });
    let mut response = [0; 23];
    for (handle, code) in [(0, 1u8), (u16::MAX, 1), (characteristic.handle, 3)] {
        let len = server
            .process(
                &conn,
                &AttClient::Request(AttReq::PrepareWrite {
                    handle,
                    offset: 0,
                    value: &[8],
                }),
                &mut response,
            )
            .unwrap()
            .unwrap();
        assert_eq!(
            &response[..len],
            &[
                att::ATT_ERROR_RSP,
                att::ATT_PREPARE_WRITE_REQ,
                handle as u8,
                (handle >> 8) as u8,
                code
            ]
        );
        assert_eq!(conn.with_prepare_write(|q| q.handle), 0);
    }
    server.table().with_attribute_mut(characteristic.handle, |a| {
        a.permissions.write = PermissionLevel::Allowed;
    });
    server
        .process(
            &conn,
            &AttClient::Request(AttReq::PrepareWrite {
                handle: characteristic.handle,
                offset: 0,
                value: &[9],
            }),
            &mut response,
        )
        .unwrap();
    let len = server
        .process(
            &conn,
            &AttClient::Request(AttReq::ExecuteWrite { flags: 2 }),
            &mut response,
        )
        .unwrap()
        .unwrap();
    assert_eq!(
        &response[..len],
        &[att::ATT_ERROR_RSP, att::ATT_EXECUTE_WRITE_REQ, 0, 0, 4]
    );
    assert_eq!(
        conn.with_prepare_write(|q| (q.handle, q.len)),
        (characteristic.handle, 1)
    );
    server
        .process(
            &conn,
            &AttClient::Request(AttReq::ExecuteWrite { flags: 0 }),
            &mut response,
        )
        .unwrap();
    assert_eq!(conn.with_prepare_write(|q| q.handle), 0);
    assert_eq!(characteristic.get(&server).unwrap(), 7);
    server
        .process(
            &conn,
            &AttClient::Request(AttReq::PrepareWrite {
                handle: characteristic.handle,
                offset: 0,
                value: &[10],
            }),
            &mut response,
        )
        .unwrap();
    let len = server
        .process(
            &conn,
            &AttClient::Request(AttReq::ExecuteWrite { flags: 1 }),
            &mut response,
        )
        .unwrap()
        .unwrap();
    assert_eq!(&response[..len], &[att::ATT_EXECUTE_WRITE_RSP]);
    assert_eq!(characteristic.get(&server).unwrap(), 10);
}
