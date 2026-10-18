use super::tests::test_client;
use super::*;
use core::future::Future;
use core::pin::pin;
use core::task::{Context, Poll, Waker};

fn reply(client: &GattClient<'_, crate::mock_controller::MockController, DefaultPacketPool, 1>, bytes: &[u8]) {
    let mut packet = DefaultPacketPool::allocate().unwrap();
    packet.as_mut()[..bytes.len()].copy_from_slice(bytes);
    client
        .response_channel
        .try_send((client.connection.handle(), Pdu::new(packet, bytes.len())))
        .unwrap();
}

#[test]
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

#[test]
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

struct Drain(&'static crate::connection_manager::ConnectionManager<'static, DefaultPacketPool>);
impl Drop for Drain {
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
