//! Public exact-link commands, inert request queues and independently split worker shutdown.

extern crate std;

use core::cell::Cell;
use core::convert::Infallible;
use core::future::Future;
use core::pin::pin;
use core::task::Waker;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::task::Wake;

use super::*;
use crate::connection::{ConnectionEvent, PendingConnectionParams};
use crate::prelude::DefaultPacketPool;
use crate::HostResources;

/// Counts actual controller polls; async commands can be held pending without timers.
#[derive(Default)]
struct BoundaryController {
    /// Command/receive/ACL/ISO polling count, used to prove failed hosts perform no I/O.
    calls: Cell<usize>,
    /// When true, asynchronous commands complete successfully.
    ready: Cell<bool>,
}
impl embedded_io::ErrorType for BoundaryController {
    /// Infallible fake transport; unsupported operations panic explicitly.
    type Error = Infallible;
}
impl Controller for BoundaryController {
    /// Owned scratch for the unused receive boundary.
    type Buffer<'a> = [u8; 259];
    /// Returns zeroed receive scratch without hardware.
    fn alloc_buf(&self) -> Result<Self::Buffer<'_>, Self::Error> {
        Ok([0; 259])
    }
    /// Counts one receive attempt and never returns.
    async fn read<'a>(&self, _: &'a mut Self::Buffer<'_>) -> Result<ControllerToHostPacket<'a>, Self::Error> {
        self.calls.set(self.calls.get() + 1);
        core::future::pending().await
    }
    /// Counts one ACL submission and returns success without hardware.
    async fn write_acl_data(&self, _: &AclPacket<'_>) -> Result<(), Self::Error> {
        self.calls.set(self.calls.get() + 1);
        Ok(())
    }
    /// Panics without returning for unsupported SCO output.
    async fn write_sync_data(&self, _: &bt_hci::data::SyncPacket<'_>) -> Result<(), Self::Error> {
        panic!("unexpected SCO output")
    }
    /// Counts one ISO submission and returns success without hardware.
    async fn write_iso_data(&self, _: &bt_hci::data::IsoPacket<'_>) -> Result<(), Self::Error> {
        self.calls.set(self.calls.get() + 1);
        Ok(())
    }
}
impl<C: SyncCmd> ControllerCmdSync<C> for BoundaryController {
    /// Counts one synchronous command and never returns; worker-cancellation tests drop its pending completion.
    async fn exec(&self, _: &C) -> Result<C::Return, cmd::Error<Self::Error>> {
        self.calls.set(self.calls.get() + 1);
        core::future::pending().await
    }
}
impl<C: AsyncCmd> ControllerCmdAsync<C> for BoundaryController {
    /// Counts each poll and returns success only when the fixture ready flag is set.
    async fn exec(&self, _: &C) -> Result<(), cmd::Error<Self::Error>> {
        poll_fn(|_| {
            self.calls.set(self.calls.get() + 1);
            if self.ready.get() {
                Poll::Ready(Ok(()))
            } else {
                Poll::Pending
            }
        })
        .await
    }
}

/// Independently count each registered future's wake notifications.
#[derive(Default)]
struct CountWake(
    /// Number of delivered wake notifications.
    AtomicUsize,
);
impl Wake for CountWake {
    /// Counts this consumed notification.
    fn wake(self: Arc<Self>) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
    /// Counts this borrowed notification.
    fn wake_by_ref(self: &Arc<Self>) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}

/// Returns one poll of `future` with an inert waker; never loops on Pending.
fn once<F: Future>(future: core::pin::Pin<&mut F>) -> Poll<F::Output> {
    future.poll(&mut Context::from_waker(Waker::noop()))
}

/// Creates one accepted link and initialized host; `check` owns all test operations.
fn with_link(
    check: impl FnOnce(
        &crate::Stack<'_, BoundaryController, DefaultPacketPool>,
        crate::connection::Connection<'_, DefaultPacketPool>,
    ),
) {
    let mut resources = HostResources::<DefaultPacketPool, 2, 1>::new();
    let stack = crate::new(BoundaryController::default(), &mut resources).build();
    stack
        .host_state
        .initialized
        .init(InitialState {
            acl_max: 251,
            acl_total: 1,
        })
        .ok()
        .unwrap();
    let manager = &stack.host_state.connections;
    manager
        .connect(
            ConnHandle::new(1),
            Address::random([1; 6]),
            LeConnRole::Central,
            ConnParams::new(),
        )
        .unwrap();
    let Poll::Ready(connection) = manager.poll_accept(LeConnRole::Central, &[], None) else {
        panic!("link")
    };
    check(&stack, connection);
}

#[test]
/// Live success is preserved; a never-polled cancellation neither submits nor poisons the host.
fn public_command_success_and_unpolled_cancellation_remain_healthy() {
    with_link(|stack, connection| {
        drop(connection.set_phy(stack, bt_hci::param::PhyKind::Le2M));
        assert_eq!(stack.controller.calls.get(), 0);
        assert!(stack.host_state.connections.fault.check().is_ok());
        stack.controller.ready.set(true);
        assert!(matches!(
            once(pin!(connection.set_phy(stack, bt_hci::param::PhyKind::Le2M))),
            Poll::Ready(Ok(()))
        ));
        assert_eq!(stack.controller.calls.get(), 1);
        assert!(stack.host_state.connections.fault.check().is_ok());
    });
}

#[test]
/// Every waiting public command is woken on disconnect without sharing the control-runner waker.
fn two_public_commands_observe_disconnect_and_stop_before_repolling_controller() {
    with_link(|stack, connection| {
        let wake_a = Arc::new(CountWake::default());
        let wake_b = Arc::new(CountWake::default());
        let waker_a = Waker::from(wake_a.clone());
        let waker_b = Waker::from(wake_b.clone());
        let mut a = pin!(connection.set_phy(stack, bt_hci::param::PhyKind::Le2M));
        let mut b = pin!(connection.set_phy(stack, bt_hci::param::PhyKind::Le1M));
        assert!(a.as_mut().poll(&mut Context::from_waker(&waker_a)).is_pending());
        assert!(b.as_mut().poll(&mut Context::from_waker(&waker_b)).is_pending());
        assert_eq!(stack.controller.calls.get(), 2);
        stack
            .host_state
            .connections
            .disconnected(connection.handle(), Status::SUCCESS)
            .unwrap();
        assert!(wake_a.0.load(Ordering::SeqCst) > 0);
        assert!(wake_b.0.load(Ordering::SeqCst) > 0);
        assert!(matches!(
            once(a.as_mut()),
            Poll::Ready(Err(BleHostError::BleHost(Error::InvalidState)))
        ));
        assert!(matches!(
            once(b.as_mut()),
            Poll::Ready(Err(BleHostError::BleHost(Error::InvalidState)))
        ));
        assert_eq!(stack.controller.calls.get(), 2);
    });
}

#[test]
/// Dropping an admitted command permanently closes all independent worker/public entrypoints.
fn canceled_command_wakes_other_waiters_and_each_split_worker_fails_closed() {
    with_link(|stack, connection| {
        let wake = Arc::new(CountWake::default());
        let waker = Waker::from(wake.clone());
        let mut survivor = pin!(connection.set_phy(stack, bt_hci::param::PhyKind::Le1M));
        assert!(survivor.as_mut().poll(&mut Context::from_waker(&waker)).is_pending());
        {
            let mut canceled = pin!(connection.set_phy(stack, bt_hci::param::PhyKind::Le2M));
            assert!(once(canceled.as_mut()).is_pending());
        }
        assert!(wake.0.load(Ordering::SeqCst) > 0);
        assert!(matches!(
            once(survivor.as_mut()),
            Poll::Ready(Err(BleHostError::BleHost(Error::InvalidState)))
        ));
        let calls = stack.controller.calls.get();
        let mut combined = stack.runner();
        assert!(matches!(
            once(pin!(combined.run())),
            Poll::Ready(Err(BleHostError::BleHost(Error::InvalidState)))
        ));
        let mut rx = RxRunner { host: stack.host() };
        let mut tx = TxRunner { host: stack.host() };
        let mut control = ControlRunner { host: stack.host() };
        assert!(matches!(
            once(pin!(rx.run())),
            Poll::Ready(Err(BleHostError::BleHost(Error::InvalidState)))
        ));
        assert!(matches!(
            once(pin!(tx.run())),
            Poll::Ready(Err(BleHostError::BleHost(Error::InvalidState)))
        ));
        assert!(matches!(
            once(pin!(control.run())),
            Poll::Ready(Err(BleHostError::BleHost(Error::InvalidState)))
        ));
        assert!(matches!(
            once(pin!(connection.set_phy(stack, bt_hci::param::PhyKind::Le2M))),
            Poll::Ready(Err(BleHostError::BleHost(Error::InvalidState)))
        ));
        assert!(stack
            .host_state
            .connections
            .connect(
                ConnHandle::new(2),
                Address::random([2; 6]),
                LeConnRole::Central,
                ConnParams::new()
            )
            .is_err());
        assert_eq!(stack.controller.calls.get(), calls);
    });
}

#[test]
/// Requests inside their own queue never retain owners, including rejected queue overflow.
fn queued_requests_do_not_prevent_last_owner_disconnect() {
    with_link(|stack, connection| {
        let manager = &stack.host_state.connections;
        for _ in 0..crate::config::CONNECTION_EVENT_QUEUE_SIZE {
            manager
                .post_handle_event(
                    connection.handle(),
                    ConnectionEvent::RequestConnectionParams(PendingConnectionParams::new(Default::default(), 1)),
                )
                .unwrap();
        }
        assert!(manager
            .post_handle_event(
                connection.handle(),
                ConnectionEvent::RequestConnectionParams(PendingConnectionParams::new(Default::default(), 2))
            )
            .is_err());
        drop(connection);
        assert!(manager.poll_disconnecting(None).is_ready());
        assert_eq!(stack.controller.calls.get(), 0);
    });
}

#[test]
/// Dequeued tokens retain exactly one original owner and reject another stack before I/O.
fn parameter_token_retains_owner_and_rejects_crossed_stack() {
    with_link(|stack, connection| {
        let manager = &stack.host_state.connections;
        manager
            .post_handle_event(
                connection.handle(),
                ConnectionEvent::RequestConnectionParams(PendingConnectionParams::new(Default::default(), 1)),
            )
            .unwrap();
        let ConnectionEvent::RequestConnectionParams(token) = embassy_futures::block_on(connection.next()) else {
            panic!("token")
        };
        drop(connection);
        assert!(manager.poll_disconnecting(None).is_pending());
        let mut resources = HostResources::<DefaultPacketPool, 1, 1>::new();
        let other = crate::new(BoundaryController::default(), &mut resources).build();
        assert!(matches!(
            once(pin!(token.reject(&other))),
            Poll::Ready(Err(BleHostError::BleHost(Error::InvalidValue)))
        ));
        assert!(manager.poll_disconnecting(None).is_ready());
        assert_eq!(stack.controller.calls.get(), 0);
        assert_eq!(other.controller.calls.get(), 0);
        assert!(manager.fault.check().is_ok());
    });
}

#[test]
/// The production guard also rejects a terminal transition occurring within a Ready command poll.
fn disconnect_during_ready_is_not_returned_as_success() {
    let fault = crate::lifetime::HostFault::new();
    let changed = futures_intrusive::sync::LocalManualResetEvent::new(false);
    let live = Cell::new(true);
    let command = async {
        live.set(false);
        Ok::<_, BleHostError<Infallible>>(())
    };
    assert!(matches!(
        once(pin!(fault.link(&changed, || live.get(), core::pin::pin!(command)))),
        Poll::Ready(Err(BleHostError::BleHost(Error::InvalidState)))
    ));
    assert!(fault.check().is_err());
}

#[test]
/// A queued request drained after terminal state remains bound to the terminal slot.
fn queued_terminal_request_cannot_authorize_a_new_link() {
    with_link(|stack, connection| {
        let manager = &stack.host_state.connections;
        manager
            .post_handle_event(
                connection.handle(),
                ConnectionEvent::RequestConnectionParams(PendingConnectionParams::new(Default::default(), 1)),
            )
            .unwrap();
        manager.disconnected(connection.handle(), Status::SUCCESS).unwrap();
        manager
            .connect(
                connection.handle(),
                Address::random([1; 6]),
                LeConnRole::Central,
                ConnParams::new(),
            )
            .unwrap();
        let Poll::Ready(replacement) = manager.poll_accept(LeConnRole::Central, &[], None) else {
            panic!("replacement")
        };
        let ConnectionEvent::RequestConnectionParams(token) = embassy_futures::block_on(connection.next()) else {
            panic!("token")
        };
        assert!(matches!(
            once(pin!(token.reject(stack))),
            Poll::Ready(Err(BleHostError::BleHost(Error::Disconnected)))
        ));
        assert_ne!(connection.index(), replacement.index());
        assert_eq!(stack.controller.calls.get(), 0);
        assert!(manager.fault.check().is_ok());
    });
}

#[test]
/// Canceling a started split receive worker makes subsequent public commands fail before I/O.
fn canceled_worker_is_not_silently_restartable() {
    with_link(|stack, connection| {
        let mut rx = RxRunner { host: stack.host() };
        {
            let mut run = pin!(rx.run());
            assert!(once(run.as_mut()).is_pending());
        }
        assert_eq!(stack.controller.calls.get(), 1);
        assert!(matches!(
            once(pin!(connection.set_phy(stack, bt_hci::param::PhyKind::Le2M))),
            Poll::Ready(Err(BleHostError::BleHost(Error::InvalidState)))
        ));
        assert!(matches!(
            once(pin!(rx.run())),
            Poll::Ready(Err(BleHostError::BleHost(Error::InvalidState)))
        ));
        assert_eq!(stack.controller.calls.get(), 1);
    });
}

#[test]
/// A sender obtained before host failure cannot submit ACL bytes afterward.
fn retained_acl_sender_observes_permanent_host_failure() {
    with_link(|stack, connection| {
        let host = stack.host();
        let mut sender = embassy_futures::block_on(host.l2cap_pdu(connection.handle())).unwrap();
        stack.host_state.connections.fault.fail();
        assert!(matches!(
            once(pin!(sender.send(&[0, 0, 4, 0]))),
            Poll::Ready(Err(BleHostError::BleHost(Error::InvalidState)))
        ));
        assert_eq!(stack.controller.calls.get(), 0);
    });
}

#[cfg(feature = "iso")]
#[test]
/// Raw ISO submission obeys the host latch even though stream lifetime remains caller-owned.
fn iso_output_observes_permanent_host_failure() {
    with_link(|stack, _connection| {
        // Continuation fragment with an empty load, decoded through the public wire parser.
        let packet = bt_hci::data::IsoPacket::from_hci_bytes_complete(&[0, 0x10, 0, 0]).unwrap();
        stack.host_state.connections.fault.fail();
        assert!(matches!(
            once(pin!(stack.host().write_iso_data(&packet))),
            Poll::Ready(Err(BleHostError::BleHost(Error::InvalidState)))
        ));
        assert_eq!(stack.controller.calls.get(), 0);
    });
}

#[cfg(feature = "connection-params-update")]
#[test]
/// Both HCI reply variants preserve exact owner identity after same-handle replacement.
fn stale_link_layer_replies_do_not_target_replacement() {
    for accept in [false, true] {
        with_link(|stack, connection| {
            let manager = &stack.host_state.connections;
            manager
                .post_handle_event(
                    connection.handle(),
                    ConnectionEvent::RequestConnectionParams(PendingConnectionParams::new_link_layer(
                        Default::default(),
                    )),
                )
                .unwrap();
            let ConnectionEvent::RequestConnectionParams(token) = embassy_futures::block_on(connection.next()) else {
                panic!("token")
            };
            manager.disconnected(connection.handle(), Status::SUCCESS).unwrap();
            manager
                .connect(
                    connection.handle(),
                    Address::random([1; 6]),
                    LeConnRole::Central,
                    ConnParams::new(),
                )
                .unwrap();
            let Poll::Ready(replacement) = manager.poll_accept(LeConnRole::Central, &[], None) else {
                panic!("replacement")
            };
            let result = if accept {
                once(pin!(token.accept(None, stack)))
            } else {
                once(pin!(token.reject(stack)))
            };
            assert!(matches!(
                result,
                Poll::Ready(Err(BleHostError::BleHost(Error::Disconnected)))
            ));
            assert!(replacement.is_connected());
            assert_eq!(stack.controller.calls.get(), 0);
            assert!(manager.fault.check().is_ok());
        });
    }
}

#[test]
/// An unpolled reply releases its external token without poisoning the host or submitting bytes.
fn unpolled_reply_cancellation_releases_owner_without_host_failure() {
    with_link(|stack, connection| {
        let manager = &stack.host_state.connections;
        manager
            .post_handle_event(
                connection.handle(),
                ConnectionEvent::RequestConnectionParams(PendingConnectionParams::new(Default::default(), 1)),
            )
            .unwrap();
        let ConnectionEvent::RequestConnectionParams(token) = embassy_futures::block_on(connection.next()) else {
            panic!("token")
        };
        drop(connection);
        drop(token.reject(stack));
        assert!(manager.poll_disconnecting(None).is_ready());
        assert!(manager.fault.check().is_ok());
        assert_eq!(stack.controller.calls.get(), 0);
    });
}

#[test]
/// All already-pending split workers receive independent fatal wakes and perform no later I/O.
fn active_split_workers_all_wake_on_host_failure() {
    with_link(|stack, _connection| {
        let (mut rx, mut control, mut tx) = stack.runner().split();
        let wakes = [
            Arc::new(CountWake::default()),
            Arc::new(CountWake::default()),
            Arc::new(CountWake::default()),
        ];
        let wakers = wakes.each_ref().map(|w| Waker::from(w.clone()));
        let mut rx = pin!(rx.run());
        let mut control = pin!(control.run());
        let mut tx = pin!(tx.run());
        assert!(rx.as_mut().poll(&mut Context::from_waker(&wakers[0])).is_pending());
        assert!(control.as_mut().poll(&mut Context::from_waker(&wakers[1])).is_pending());
        assert!(tx.as_mut().poll(&mut Context::from_waker(&wakers[2])).is_pending());
        let calls = stack.controller.calls.get();
        assert_eq!(calls, 2, "RX and initial Reset each reached the driver once");
        stack.host_state.connections.fault.fail();
        assert!(wakes.iter().all(|w| w.0.load(Ordering::SeqCst) > 0));
        assert!(matches!(
            once(rx.as_mut()),
            Poll::Ready(Err(BleHostError::BleHost(Error::InvalidState)))
        ));
        assert!(matches!(
            once(control.as_mut()),
            Poll::Ready(Err(BleHostError::BleHost(Error::InvalidState)))
        ));
        assert!(matches!(
            once(tx.as_mut()),
            Poll::Ready(Err(BleHostError::BleHost(Error::InvalidState)))
        ));
        assert_eq!(stack.controller.calls.get(), calls);
    });
}

#[test]
/// The shared command/transport guard replaces a Ready result if that poll latched failure.
fn ready_driver_failure_is_observed_before_next_operation() {
    let fault = crate::lifetime::HostFault::new();
    let polls = Cell::new(0);
    let work = async {
        polls.set(polls.get() + 1);
        fault.fail();
        Ok::<_, BleHostError<Infallible>>(())
    };
    assert!(matches!(
        once(pin!(fault.run(core::pin::pin!(work)))),
        Poll::Ready(Err(BleHostError::BleHost(Error::InvalidState)))
    ));
    let later = async {
        polls.set(polls.get() + 1);
        Ok::<_, BleHostError<Infallible>>(())
    };
    assert!(matches!(
        once(pin!(fault.run(core::pin::pin!(later)))),
        Poll::Ready(Err(BleHostError::BleHost(Error::InvalidState)))
    ));
    assert_eq!(polls.get(), 1);
}

/// Completes startup queries and holds Disconnect at its actual command polling boundary.
#[derive(Default)]
struct DisconnectController {
    /// Counts each attempted Disconnect command poll, including resumption.
    polls: Cell<usize>,
    /// Permits a successful close command when true; defaults to a pending admission.
    ready: Cell<bool>,
}
impl embedded_io::ErrorType for DisconnectController {
    /// No simulated transport errors.
    type Error = Infallible;
}
impl Controller for DisconnectController {
    /// Owned receive scratch; no incoming packets are synthesized here.
    type Buffer<'a> = [u8; 259];
    /// Returns zeroed scratch without device access.
    fn alloc_buf(&self) -> Result<Self::Buffer<'_>, Self::Error> {
        Ok([0; 259])
    }
    /// Never returns; tests apply actual manager ingress transitions explicitly.
    async fn read<'a>(&self, _: &'a mut Self::Buffer<'_>) -> Result<ControllerToHostPacket<'a>, Self::Error> {
        core::future::pending().await
    }
    /// Panics on unexpected ACL traffic; never returns.
    async fn write_acl_data(&self, _: &AclPacket<'_>) -> Result<(), Self::Error> {
        panic!("unexpected ACL")
    }
    /// Panics on unexpected SCO traffic; never returns.
    async fn write_sync_data(&self, _: &bt_hci::data::SyncPacket<'_>) -> Result<(), Self::Error> {
        panic!("unexpected SCO")
    }
    /// Panics on unexpected ISO traffic; never returns.
    async fn write_iso_data(&self, _: &bt_hci::data::IsoPacket<'_>) -> Result<(), Self::Error> {
        panic!("unexpected ISO")
    }
}
impl<C: SyncCmd> ControllerCmdSync<C> for DisconnectController {
    /// Returns typed startup data for `cmd`; Disconnect counts each poll and returns success only when ready is set. Fixture shape/return decode errors panic.
    async fn exec(&self, cmd: &C) -> Result<C::Return, cmd::Error<Self::Error>> {
        if C::OPCODE.to_raw() == 0x0406 {
            assert_eq!(cmd.params().size(), 3);
            return poll_fn(|_| {
                self.polls.set(self.polls.get() + 1);
                if self.ready.get() {
                    Poll::Ready(Ok(C::Return::from_hci_bytes(&[0; 8]).unwrap().0))
                } else {
                    Poll::Pending
                }
            })
            .await;
        }
        let result = match C::OPCODE.to_raw() {
            0x2002 => [251, 0, 1, 0, 0, 0, 0, 0],
            0x200f => [2, 0, 0, 0, 0, 0, 0, 0],
            0x1009 => [1, 2, 3, 4, 5, 6, 0, 0],
            0x2018 => [0x5a; 8],
            _ => [0; 8],
        };
        Ok(C::Return::from_hci_bytes(&result).unwrap().0)
    }
}
impl<C: AsyncCmd> ControllerCmdAsync<C> for DisconnectController {
    /// Panics on unexpected asynchronous HCI commands; never returns.
    async fn exec(&self, _: &C) -> Result<(), cmd::Error<Self::Error>> {
        panic!("unexpected async command")
    }
}

#[test]
/// A real control worker must stop before repolling a pending close after same-handle replacement.
fn control_disconnect_cannot_retarget_reused_handle() {
    for retain_old_owner in [false, true] {
        let mut resources = HostResources::<DefaultPacketPool, 2, 1>::new();
        let stack = crate::new(DisconnectController::default(), &mut resources).build();
        let manager = &stack.host_state.connections;
        let handle = ConnHandle::new(1);
        let peer = Address::random([7; 6]);
        manager
            .connect(handle, peer, LeConnRole::Central, ConnParams::new())
            .unwrap();
        let Poll::Ready(old) = manager.poll_accept(LeConnRole::Central, &[], None) else {
            panic!("old link")
        };
        old.disconnect();
        let old = retain_old_owner.then_some(old);
        let mut control = ControlRunner { host: stack.host() };
        let mut run = pin!(control.run());
        let wake = Arc::new(CountWake::default());
        let waker = Waker::from(wake.clone());
        assert!(run.as_mut().poll(&mut Context::from_waker(&waker)).is_pending());
        assert_eq!(stack.controller.polls.get(), 1);
        manager.disconnected(handle, Status::SUCCESS).unwrap();
        assert!(
            wake.0.load(Ordering::SeqCst) > 0,
            "terminal change must wake pending close"
        );
        manager
            .connect(handle, peer, LeConnRole::Central, ConnParams::new())
            .unwrap();
        let Poll::Ready(replacement) = manager.poll_accept(LeConnRole::Central, &[], None) else {
            panic!("new link")
        };
        let result = once(run.as_mut());
        assert_eq!(
            stack.controller.polls.get(),
            1,
            "old Disconnect was repolled against replacement"
        );
        assert!(matches!(
            result,
            Poll::Ready(Err(BleHostError::BleHost(Error::InvalidState)))
        ));
        assert!(replacement.is_connected());
        drop(old);
    }
}

#[test]
/// Terminal work before its first poll is inert; retained close tokens cannot acknowledge replacement links.
fn disconnect_token_terminal_before_poll_is_inert_and_confirmation_stays_original() {
    with_link(|stack, old| {
        let manager = &stack.host_state.connections;
        old.disconnect();
        let Poll::Ready(Ok(request)) = manager.poll_disconnecting(None) else {
            panic!("request")
        };
        manager.disconnected(old.handle(), Status::SUCCESS).unwrap();
        manager
            .connect(old.handle(), old.peer_address(), LeConnRole::Central, ConnParams::new())
            .unwrap();
        let Poll::Ready(replacement) = manager.poll_accept(LeConnRole::Central, &[], None) else {
            panic!("replacement")
        };
        assert!(matches!(
            once(pin!(request.send(&stack.host()))),
            Poll::Ready(Err(BleHostError::BleHost(Error::Disconnected)))
        ));
        assert_eq!(stack.controller.calls.get(), 0);
        assert!(manager.fault.check().is_ok());
        replacement.disconnect();
        request.confirm();
        assert!(matches!(
            manager.state(replacement.index()),
            crate::connection_manager::ConnectionState::DisconnectRequest(_)
        ));
    });
}

#[test]
/// Close-token count exhaustion preserves the pending request; dropping a token returns capacity exactly once.
fn disconnect_token_retention_exhaustion_and_release_are_transactional() {
    with_link(|stack, old| {
        let manager = &stack.host_state.connections;
        let leases: std::vec::Vec<_> = (0..255)
            .map(|_| manager.retain_io(usize::from(old.index())).unwrap())
            .collect();
        old.disconnect();
        assert!(matches!(
            manager.poll_disconnecting(None),
            Poll::Ready(Err(Error::NoPermits))
        ));
        assert!(matches!(
            manager.state(old.index()),
            crate::connection_manager::ConnectionState::DisconnectRequest(_)
        ));
        drop(leases);
        let Poll::Ready(Ok(request)) = manager.poll_disconnecting(None) else {
            panic!("request")
        };
        drop(request);
        let Poll::Ready(Ok(request)) = manager.poll_disconnecting(None) else {
            panic!("released request")
        };
        request.confirm();
        assert!(manager.poll_disconnecting(None).is_pending());
        assert!(manager.fault.check().is_ok());
    });
}

#[test]
/// Canceling a polled internal close faults the host; dropping never-polled work does not.
fn disconnect_token_cancellation_obeys_submission_boundary() {
    with_link(|stack, old| {
        let manager = &stack.host_state.connections;
        old.disconnect();
        let Poll::Ready(Ok(request)) = manager.poll_disconnecting(None) else {
            panic!("request")
        };
        drop(request.send(&stack.host()));
        assert!(manager.fault.check().is_ok());
        assert_eq!(stack.controller.calls.get(), 0);
        let host = stack.host();
        {
            let mut send = pin!(request.send(&host));
            assert!(once(send.as_mut()).is_pending());
        }
        assert_eq!(manager.fault.check(), Err(Error::InvalidState));
        assert_eq!(stack.controller.calls.get(), 1);
    });
}

#[test]
/// A close token rejects a different host before command polling and does not poison either host.
fn disconnect_token_rejects_crossed_stack_before_io() {
    with_link(|stack, old| {
        old.disconnect();
        let Poll::Ready(Ok(request)) = stack.host_state.connections.poll_disconnecting(None) else {
            panic!("request")
        };
        with_link(|other, _other_link| {
            assert!(matches!(
                once(pin!(request.send(&other.host()))),
                Poll::Ready(Err(BleHostError::BleHost(Error::InvalidValue)))
            ));
            assert_eq!(other.controller.calls.get(), 0);
            assert!(other.host_state.connections.fault.check().is_ok());
        });
        assert!(stack.host_state.connections.fault.check().is_ok());
    });
}

#[test]
/// A successful real control-worker close advances only its retained request and keeps the host healthy.
fn control_disconnect_success_preserves_in_flight_close() {
    let mut resources = HostResources::<DefaultPacketPool, 1, 1>::new();
    let stack = crate::new(DisconnectController::default(), &mut resources).build();
    let manager = &stack.host_state.connections;
    manager
        .connect(
            ConnHandle::new(1),
            Address::random([7; 6]),
            LeConnRole::Central,
            ConnParams::new(),
        )
        .unwrap();
    let Poll::Ready(old) = manager.poll_accept(LeConnRole::Central, &[], None) else {
        panic!("link")
    };
    old.disconnect();
    stack.controller.ready.set(true);
    let mut control = ControlRunner { host: stack.host() };
    let mut run = pin!(control.run());
    assert!(once(run.as_mut()).is_pending());
    assert_eq!(stack.controller.polls.get(), 1);
    assert!(matches!(
        manager.state(old.index()),
        crate::connection_manager::ConnectionState::Disconnecting(_)
    ));
    assert!(manager.fault.check().is_ok());
}

#[test]
/// A close token alone pins terminal storage; its drop permits reuse without an owner cycle.
fn disconnect_token_alone_retains_then_releases_slot() {
    with_link(|stack, old| {
        let manager = &stack.host_state.connections;
        manager
            .connect(
                ConnHandle::new(2),
                Address::random([2; 6]),
                LeConnRole::Central,
                ConnParams::new(),
            )
            .unwrap();
        let Poll::Ready(other) = manager.poll_accept(LeConnRole::Central, &[], None) else {
            panic!("other")
        };
        old.disconnect();
        let Poll::Ready(Ok(request)) = manager.poll_disconnecting(None) else {
            panic!("request")
        };
        manager.disconnected(old.handle(), Status::SUCCESS).unwrap();
        drop(old);
        assert_eq!(
            manager.connect(
                ConnHandle::new(1),
                Address::random([1; 6]),
                LeConnRole::Central,
                ConnParams::new()
            ),
            Err(Error::NotFound)
        );
        drop(request);
        manager
            .connect(
                ConnHandle::new(1),
                Address::random([1; 6]),
                LeConnRole::Central,
                ConnParams::new(),
            )
            .unwrap();
        assert!(other.is_connected());
    });
}

/// Guard metadata must not embed another copy of a large caller-owned suspended future.
#[test]
fn guard_metadata_does_not_duplicate_suspended_payload() {
    let fault = crate::lifetime::HostFault::new();
    let changed = futures_intrusive::sync::LocalManualResetEvent::new(false);
    let child = || async {
        let payload = [0xA5_u8; 4096];
        core::future::pending::<()>().await;
        core::hint::black_box(payload);
        Ok::<(), BleHostError<Infallible>>(())
    };
    assert!(core::mem::size_of_val(&child()) >= 4096);
    assert!(
        core::mem::size_of_val(&fault.run(core::pin::pin!(child()))) < 512,
        "run embeds child storage"
    );
    assert!(
        core::mem::size_of_val(&fault.worker(core::pin::pin!(child()))) < 512,
        "worker embeds child storage"
    );
    assert!(
        core::mem::size_of_val(&fault.link(&changed, || true, core::pin::pin!(child()))) < 512,
        "link embeds child storage"
    );
}

/// Affine child whose destructor witnesses cleanup at the immediate caller boundary.
struct GuardChild<'a> {
    /// Number of destructor calls; exactly one owner may release the child.
    drops: &'a Cell<u32>,
    /// 0 suspends, 1 succeeds, 2 errors, 3 latches failure while returning success.
    mode: u8,
    /// Shared permanent failure latch used by the synchronous Ready callback.
    fault: &'a crate::lifetime::HostFault,
}
impl Future for GuardChild<'_> {
    /// Unit completion or a deliberate host error; no controller operations occur.
    type Output = Result<(), BleHostError<Infallible>>;
    /// Returns the configured result; mode0 never completes and mode3 fails the host before Ready.
    fn poll(self: core::pin::Pin<&mut Self>, _: &mut core::task::Context<'_>) -> Poll<Self::Output> {
        match self.mode {
            0 => Poll::Pending,
            1 => Poll::Ready(Ok(())),
            2 => Poll::Ready(Err(Error::Busy.into())),
            _ => {
                self.fault.fail();
                Poll::Ready(Ok(()))
            }
        }
    }
}
impl Drop for GuardChild<'_> {
    /// Records exactly one release of the owned child, including cancellation before polling.
    fn drop(&mut self) {
        self.drops.set(self.drops.get() + 1);
    }
}

/// Immediate caller-owned pins drop children before completion or cancellation returns control.
#[test]
fn pinned_guard_callers_release_children_on_every_exit() {
    for guard in 0..3 {
        for mode in 0..4 {
            for poll in [false, true] {
                let fault = crate::lifetime::HostFault::new();
                let changed = futures_intrusive::sync::LocalManualResetEvent::new(false);
                let drops = Cell::new(0);
                let child = GuardChild {
                    drops: &drops,
                    mode,
                    fault: &fault,
                };
                let mut caller = std::boxed::Box::pin(async {
                    match guard {
                        0 => fault.run(pin!(child)).await,
                        1 => fault.worker(pin!(child)).await,
                        _ => fault.link(&changed, || true, pin!(child)).await,
                    }
                });
                if poll {
                    let result = once(caller.as_mut());
                    assert_eq!(result.is_pending(), mode == 0);
                    assert_eq!(drops.get(), u32::from(mode != 0));
                    if mode == 3 {
                        assert!(matches!(
                            result,
                            Poll::Ready(Err(BleHostError::BleHost(Error::InvalidState)))
                        ));
                    }
                }
                drop(caller);
                assert_eq!(drops.get(), 1);
                if !poll {
                    assert!(fault.check().is_ok());
                } else if mode == 0 && guard != 0 {
                    assert!(fault.check().is_err());
                }
            }
        }
    }
}
