//! Offline controller boundary regressions; no hardware transport.
#[cfg(test)]
/// Public-controller command ownership, cancellation, framing and Reset boundary fixtures.
mod tests {
    extern crate std;
    use std::cell::{Cell, RefCell};
    use std::collections::VecDeque;
    use std::convert::Infallible;
    use std::future::{poll_fn, Future};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use std::task::{Context, Poll, Waker};
    use std::vec;
    use std::vec::Vec;

    use crate::cmd::info::ReadBdAddr;
    use crate::controller::{Controller, ControllerCmdSync, ExternalController};
    use crate::transport::{PacketKind, PacketToController, PacketToHost, Transport};
    use crate::ReadHciError;
    #[derive(Debug)]
    /// Collapses synthetic transport/parser failures without hiding whether receive failed.
    enum Error {
        /// Exact malformed-value rejection reported by HCI decoding.
        InvalidValue,
        /// Other synthetic input/parser failure.
        Decode,
    }
    impl std::fmt::Display for Error {
        /// Formats the fixture failure into `f`; no packet data is logged.
        /// Returns formatter success or the error from `f`.
        fn fmt(
            &self,
            f: &mut std::fmt::Formatter<'_>,
        ) -> std::fmt::Result {
            write!(f, "fixture decode")
        }
    }
    impl std::error::Error for Error {}
    impl embedded_io::Error for Error {
        /// Classifies the synthetic failure as Other.
        /// Returns the non-specific Other I/O classification.
        fn kind(&self) -> embedded_io::ErrorKind { embedded_io::ErrorKind::Other }
    }
    impl From<ReadHciError<Infallible>> for Error {
        /// Converts actual HCI parsing failure into the fixture error.
        /// Returns InvalidValue for that exact `error`; other HCI read failures become Decode.
        fn from(error: ReadHciError<Infallible>) -> Self {
            match error {
                ReadHciError::InvalidValue => Self::InvalidValue,
                _ => Self::Decode,
            }
        }
    }
    #[derive(Default)]
    /// Bounded-by-test in-memory transport, manually polled with no devices or background task.
    struct Offline {
        /// Complete incoming HCI packets in controller-to-host order.
        packets: RefCell<VecDeque<Vec<u8>>>,
        /// Number of completed mock transport submissions.
        writes: Cell<usize>,
        /// Forces write futures to suspend before publication when true.
        block_writes: Cell<bool>,
    }
    impl embedded_io::ErrorType for &Offline {
        /// Synthetic parser/transport error type.
        type Error = Error;
    }
    impl Transport for &Offline {
        /// Waits for one injected packet and decodes it into caller `rx`; empty input stays Pending.
        /// Returns the decoded packet borrowing `rx`, or the parser error; the queued fixture is consumed first.
        async fn read<'a, P: PacketToHost<'a>>(
            &self,
            rx: &'a mut [u8],
        ) -> Result<P, Error> {
            let bytes = poll_fn(|_| {
                self.packets
                    .borrow_mut()
                    .pop_front()
                    .map(Poll::Ready)
                    .unwrap_or(Poll::Pending)
            })
            .await;
            let mut input = bytes.as_slice();
            let kind = PacketKind::read(&mut input)?;
            P::read_hci(kind, &mut input, rx).map_err(Into::into)
        }

        /// Suspends while blocked, otherwise counts submission of `_val` without I/O.
        /// Returns success after incrementing the count; cancellation while blocked changes no count.
        async fn write<T: PacketToController>(
            &self,
            _val: &T,
        ) -> Result<(), Error> {
            poll_fn(|_| {
                if self.block_writes.get() {
                    Poll::Pending
                } else {
                    Poll::Ready(())
                }
            })
            .await;
            self.writes.set(self.writes.get() + 1);
            Ok(())
        }
    }
    #[derive(Default)]
    /// Counts executor wake notifications independently for each registered task.
    struct Wakes(
        /// Relaxed fixture scheduling count; SeqCst ordering simplifies assertions.
        AtomicUsize,
    );
    impl std::task::Wake for Wakes {
        /// Records one notification without polling any future reentrantly.
        fn wake(self: Arc<Self>) { self.0.fetch_add(1, Ordering::SeqCst); }
    }
    /// Builds an exact ReadBdAddr completion; `extra` adds a seventh return byte and updates outer length.
    fn completed(extra: bool) -> Vec<u8> {
        let mut v = vec![4, 0x0E, 10, 1, 0x09, 0x10, 0, 1, 2, 3, 4, 5, 6];
        if extra {
            v[2] += 1;
            v.push(7);
        }
        v
    }
    #[test]
    /// A completion must wake a same-opcode waiter after the owner actually releases its slot.
    fn completion_slot_release_must_wake_same_opcode_waiter() {
        let transport = Offline::default();
        let controller = ExternalController::<_, 2>::new(&transport);
        let cmd = ReadBdAddr::new();
        let mut first = std::pin::pin!(controller.exec(&cmd));
        let mut second = std::pin::pin!(controller.exec(&cmd));
        let first_w = Waker::noop();
        let count = Arc::new(Wakes::default());
        let second_w = Waker::from(count.clone());
        let mut first_cx = Context::from_waker(first_w);
        let mut second_cx = Context::from_waker(&second_w);
        assert!(first.as_mut().poll(&mut first_cx).is_pending());
        assert_eq!(transport.writes.get(), 1);
        assert!(second.as_mut().poll(&mut second_cx).is_pending());
        transport.packets.borrow_mut().push_back(completed(false));
        let mut buf = controller.alloc_buf().unwrap();
        let mut rx = std::pin::pin!(controller.read(&mut buf));
        assert!(rx.as_mut().poll(&mut first_cx).is_pending());
        assert!(count.0.load(Ordering::SeqCst) > 0);
        assert!(second.as_mut().poll(&mut second_cx).is_pending());
        count.0.store(0, Ordering::SeqCst);
        assert!(matches!(first.as_mut().poll(&mut first_cx), Poll::Ready(Ok(_))));
        let wakes = count.0.load(Ordering::SeqCst);
        // An unsolicited manual poll proves the second command could now submit.
        assert!(second.as_mut().poll(&mut second_cx).is_pending());
        assert_eq!(transport.writes.get(), 2);
        assert!(
            wakes > 0,
            "slot became available without waking the blocked same-opcode command"
        );
    }
    #[test]
    /// An intact HCI envelope with excess return bytes must fail before slicing caller storage.
    fn oversized_completion_must_return_error_without_panicking() {
        let transport = Offline::default();
        let controller = ExternalController::<_, 2>::new(&transport);
        let cmd = ReadBdAddr::new();
        let mut command = std::pin::pin!(controller.exec(&cmd));
        let mut cx = Context::from_waker(Waker::noop());
        assert!(command.as_mut().poll(&mut cx).is_pending());
        transport.packets.borrow_mut().push_back(completed(true));
        let mut buf = controller.alloc_buf().unwrap();
        let mut rx = std::pin::pin!(controller.read(&mut buf));
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| rx.as_mut().poll(&mut cx)));
        assert!(matches!(outcome, Ok(Poll::Ready(Err(Error::InvalidValue)))));
        assert!(command.as_mut().poll(&mut cx).is_pending());
        assert_eq!(controller.slots.permits.get(), 0);
    }

    /// Processes all queued mock HCI responses; empty transport suspends instead of fabricating a packet.
    /// Uses `controller` to dispatch queued completions; panics on allocation failure or a non-Pending poll.
    fn pump<const N: usize>(controller: &ExternalController<&Offline, N>) {
        let mut buf = controller.alloc_buf().unwrap();
        let mut rx = std::pin::pin!(controller.read(&mut buf));
        let mut cx = Context::from_waker(Waker::noop());
        assert!(rx.as_mut().poll(&mut cx).is_pending());
    }
    #[test]
    /// A repeated controller completion cannot replace the first unread command result.
    fn duplicate_completion_does_not_replace_unconsumed_result() {
        let transport = Offline::default();
        let controller = ExternalController::<_, 2>::new(&transport);
        let cmd = ReadBdAddr::new();
        let mut command = std::pin::pin!(controller.exec(&cmd));
        let mut cx = Context::from_waker(Waker::noop());
        assert!(command.as_mut().poll(&mut cx).is_pending());
        transport.packets.borrow_mut().push_back(completed(false));
        let mut duplicate = completed(false);
        duplicate[7] = 99;
        transport.packets.borrow_mut().push_back(duplicate);
        pump(&controller);
        assert!(matches!(
            command.as_mut().poll(&mut cx),
            Poll::Ready(Ok(crate::param::BdAddr([1, 2, 3, 4, 5, 6])))
        ));
    }
    #[test]
    /// Late replies consume canceled ownership before a replacement command can submit.
    fn canceled_command_quarantines_opcode_until_late_completion() {
        let transport = Offline::default();
        let controller = ExternalController::<_, 2>::new(&transport);
        // An unsolicited completion establishes two controller credits without a pending owner.
        let mut credits = completed(false);
        credits[3] = 2;
        transport.packets.borrow_mut().push_back(credits);
        pump(&controller);
        let cmd = ReadBdAddr::new();
        let mut cx = Context::from_waker(Waker::noop());
        let mut old = std::boxed::Box::pin(controller.exec(&cmd));
        assert!(old.as_mut().poll(&mut cx).is_pending());
        drop(old);
        let mut next = std::pin::pin!(controller.exec(&cmd));
        assert!(next.as_mut().poll(&mut cx).is_pending());
        assert_eq!(
            transport.writes.get(),
            1,
            "replacement command reused an unresolved canceled opcode"
        );
        transport.packets.borrow_mut().push_back(completed(false));
        pump(&controller);
        assert!(next.as_mut().poll(&mut cx).is_pending());
        assert_eq!(transport.writes.get(), 2);
        let mut fresh = completed(false);
        fresh[7] = 42;
        transport.packets.borrow_mut().push_back(fresh);
        pump(&controller);
        assert!(matches!(
            next.as_mut().poll(&mut cx),
            Poll::Ready(Ok(crate::param::BdAddr([42, 2, 3, 4, 5, 6])))
        ));
    }
    #[test]
    /// Waiting for an occupied slot must leave unsubmitted controller capacity available.
    fn canceled_slot_waiter_refunds_reserved_controller_credit() {
        let transport = Offline::default();
        let controller = ExternalController::<_, 1>::new(&transport);
        let mut credits = completed(false);
        credits[3] = 2;
        transport.packets.borrow_mut().push_back(credits);
        pump(&controller);
        let cmd = ReadBdAddr::new();
        let mut cx = Context::from_waker(Waker::noop());
        let mut first = std::pin::pin!(controller.exec(&cmd));
        assert!(first.as_mut().poll(&mut cx).is_pending());
        let mut waiter = std::boxed::Box::pin(controller.exec(&cmd));
        assert!(waiter.as_mut().poll(&mut cx).is_pending());
        drop(waiter);
        // With no matching completion, raw state observes the credit must still belong to the controller.
        assert_eq!(
            controller.slots.permits.get(),
            1,
            "canceled slot waiter leaked its reserved credit"
        );
    }

    #[test]
    /// Distinct task waiters all wake; canceled nodes disappear and absolute credits are not duplicated.
    fn all_slot_waiters_wake_and_canceled_nodes_do_not_leak_credits() {
        for cancel_waiter in [false, true] {
            let transport = Offline::default();
            let controller = ExternalController::<_, 2>::new(&transport);
            let mut credits = completed(false);
            credits[3] = 3;
            transport.packets.borrow_mut().push_back(credits);
            pump(&controller);
            let cmd = ReadBdAddr::new();
            let mut cx = Context::from_waker(Waker::noop());
            let mut owner = std::pin::pin!(controller.exec(&cmd));
            assert!(owner.as_mut().poll(&mut cx).is_pending());
            let a = Arc::new(Wakes::default());
            let b = Arc::new(Wakes::default());
            let wa = Waker::from(a.clone());
            let wb = Waker::from(b.clone());
            let mut ca = Context::from_waker(&wa);
            let mut cb = Context::from_waker(&wb);
            let mut first = Some(std::boxed::Box::pin(controller.exec(&cmd)));
            let mut second = std::pin::pin!(controller.exec(&cmd));
            assert!(first.as_mut().unwrap().as_mut().poll(&mut ca).is_pending());
            assert!(second.as_mut().poll(&mut cb).is_pending());
            assert_eq!(
                controller.slots.permits.get(),
                2,
                "unsubmitted slot waiters must not retain controller credits"
            );
            if cancel_waiter {
                drop(first.take());
            }
            let mut response = completed(false);
            response[3] = 3;
            transport.packets.borrow_mut().push_back(response);
            pump(&controller);
            assert!(matches!(owner.as_mut().poll(&mut cx), Poll::Ready(Ok(_))));
            assert_eq!(a.0.load(Ordering::SeqCst) > 0, !cancel_waiter);
            assert!(b.0.load(Ordering::SeqCst) > 0);
            assert_eq!(
                controller.slots.permits.get(),
                3,
                "absolute credit reports must not double-count held reservations"
            );
            assert!(second.as_mut().poll(&mut cb).is_pending());
            assert_eq!(transport.writes.get(), 2);
            transport.packets.borrow_mut().push_back(completed(false));
            pump(&controller);
            assert!(matches!(second.as_mut().poll(&mut cb), Poll::Ready(Ok(_))));
            if let Some(mut first) = first {
                assert!(first.as_mut().poll(&mut ca).is_pending());
                assert_eq!(transport.writes.get(), 3);
            }
        }
    }
    #[test]
    /// Reset uses an available slot even when ordinary commands wait for controller credit.
    fn reset_with_zero_credits_passes_blocked_normal_admission() {
        use crate::cmd::controller_baseband::Reset;
        for abandoned in [false, true] {
            let transport = Offline::default();
            let controller = ExternalController::<_, 2>::new(&transport);
            let cmd = ReadBdAddr::new();
            let mut cx = Context::from_waker(Waker::noop());
            let mut old = Some(std::boxed::Box::pin(controller.exec(&cmd)));
            assert!(old.as_mut().unwrap().as_mut().poll(&mut cx).is_pending());
            if abandoned {
                drop(old.take());
            }
            let mut waiter = std::pin::pin!(controller.exec(&cmd));
            assert!(waiter.as_mut().poll(&mut cx).is_pending());
            let reset = Reset::new();
            let mut reset_command = std::pin::pin!(controller.exec(&reset));
            assert!(reset_command.as_mut().poll(&mut cx).is_pending());
            assert_eq!(transport.writes.get(), 2);
            transport
                .packets
                .borrow_mut()
                .push_back(vec![4, 0x0E, 4, 1, 3, 0x0C, 0]);
            pump(&controller);
            if let Some(mut old) = old {
                assert!(matches!(
                    old.as_mut().poll(&mut cx),
                    Poll::Ready(Err(crate::cmd::Error::Hci(crate::param::Error::CONTROLLER_BUSY)))
                ));
            }
            assert!(matches!(reset_command.as_mut().poll(&mut cx), Poll::Ready(Ok(()))));
            assert!(waiter.as_mut().poll(&mut cx).is_pending());
            assert_eq!(transport.writes.get(), 3);
        }
    }
    #[test]
    /// Reset cannot steal an unresolved opcode when every fixed command slot is quarantined.
    fn all_abandoned_slots_block_reset_until_response_or_reconstruction() {
        use crate::cmd::controller_baseband::Reset;
        let transport = Offline::default();
        let controller = ExternalController::<_, 1>::new(&transport);
        let cmd = ReadBdAddr::new();
        let mut cx = Context::from_waker(Waker::noop());
        let mut old = std::boxed::Box::pin(controller.exec(&cmd));
        assert!(old.as_mut().poll(&mut cx).is_pending());
        drop(old);
        let reset = Reset::new();
        let mut reset_command = std::pin::pin!(controller.exec(&reset));
        assert!(reset_command.as_mut().poll(&mut cx).is_pending());
        assert_eq!(transport.writes.get(), 1);
        transport.packets.borrow_mut().push_back(completed(false));
        pump(&controller);
        assert!(reset_command.as_mut().poll(&mut cx).is_pending());
        assert_eq!(transport.writes.get(), 2);
    }
    #[test]
    /// A repeated Reset result cannot repeat its broadcast against a newer command.
    fn duplicate_reset_does_not_fail_a_new_command() {
        use crate::cmd::controller_baseband::Reset;
        let transport = Offline::default();
        let controller = ExternalController::<_, 2>::new(&transport);
        let mut cx = Context::from_waker(Waker::noop());
        let reset = Reset::new();
        let mut reset_command = std::pin::pin!(controller.exec(&reset));
        assert!(reset_command.as_mut().poll(&mut cx).is_pending());
        transport
            .packets
            .borrow_mut()
            .push_back(vec![4, 0x0E, 4, 1, 3, 0x0C, 0]);
        pump(&controller);
        let cmd = ReadBdAddr::new();
        let mut fresh = std::pin::pin!(controller.exec(&cmd));
        assert!(fresh.as_mut().poll(&mut cx).is_pending());
        transport
            .packets
            .borrow_mut()
            .push_back(vec![4, 0x0E, 4, 1, 3, 0x0C, 0]);
        pump(&controller);
        assert!(
            fresh.as_mut().poll(&mut cx).is_pending(),
            "duplicate Reset completed a newer command"
        );
        transport.packets.borrow_mut().push_back(completed(false));
        pump(&controller);
        assert!(matches!(fresh.as_mut().poll(&mut cx), Poll::Ready(Ok(_))));
    }

    impl crate::transport::blocking::Transport for &Offline {
        /// Decodes one queued packet into `rx`, or returns Busy when empty.
        fn read<'a, P: PacketToHost<'a>>(
            &self,
            rx: &'a mut [u8],
        ) -> Result<P, crate::controller::blocking::TryError<Error>> {
            let bytes = self
                .packets
                .borrow_mut()
                .pop_front()
                .ok_or(crate::controller::blocking::TryError::Busy)?;
            let mut input = bytes.as_slice();
            let kind = PacketKind::read(&mut input)?;
            Ok(P::read_hci(kind, &mut input, rx)?)
        }

        /// Counts `_tx` submission without accessing any device.
        /// Returns success after the count increment; this fixture has no blocking or failure branch.
        fn write<P: PacketToController>(
            &self,
            _tx: &P,
        ) -> Result<(), crate::controller::blocking::TryError<Error>> {
            self.writes.set(self.writes.get() + 1);
            Ok(())
        }
    }
    #[test]
    /// An uncertain canceled transport write retains opcode ownership until explicit reset.
    fn cancel_during_transport_write_keeps_opcode_quarantined() {
        let transport = Offline::default();
        let controller = ExternalController::<_, 2>::new(&transport);
        let mut credits = completed(false);
        credits[3] = 2;
        transport.packets.borrow_mut().push_back(credits);
        pump(&controller);
        let cmd = ReadBdAddr::new();
        let mut cx = Context::from_waker(Waker::noop());
        transport.block_writes.set(true);
        let mut old = std::boxed::Box::pin(controller.exec(&cmd));
        assert!(old.as_mut().poll(&mut cx).is_pending());
        assert_eq!(transport.writes.get(), 0);
        drop(old);
        transport.block_writes.set(false);
        let mut next = std::pin::pin!(controller.exec(&cmd));
        assert!(next.as_mut().poll(&mut cx).is_pending());
        assert_eq!(transport.writes.get(), 0);
        // Generic Controller cannot know that this fixture did not submit, so Reset is required here.
        let reset = crate::cmd::controller_baseband::Reset::new();
        let mut reset_command = std::pin::pin!(controller.exec(&reset));
        assert!(reset_command.as_mut().poll(&mut cx).is_pending());
        assert_eq!(transport.writes.get(), 1);
        transport
            .packets
            .borrow_mut()
            .push_back(vec![4, 0x0E, 4, 1, 3, 0x0C, 0]);
        pump(&controller);
        assert!(matches!(reset_command.as_mut().poll(&mut cx), Poll::Ready(Ok(()))));
        assert!(next.as_mut().poll(&mut cx).is_pending());
        assert_eq!(transport.writes.get(), 2);
    }
    #[test]
    /// Blocking receive rejects an oversized return before signaling the owner or publishing credits.
    fn oversized_blocking_reply_changes_no_slot_or_credit() {
        let transport = Offline::default();
        let controller = ExternalController::<_, 2>::new(&transport);
        let cmd = ReadBdAddr::new();
        let mut command = std::pin::pin!(controller.exec(&cmd));
        let mut cx = Context::from_waker(Waker::noop());
        assert!(command.as_mut().poll(&mut cx).is_pending());
        let mut malformed = completed(true);
        malformed[3] = 9;
        transport.packets.borrow_mut().push_back(malformed);
        let mut buf = Controller::alloc_buf(&controller).unwrap();
        assert!(matches!(
            crate::controller::blocking::Controller::try_read(&controller, &mut buf),
            Err(crate::controller::blocking::TryError::Error(_))
        ));
        assert_eq!(controller.slots.permits.get(), 0);
        assert!(command.as_mut().poll(&mut cx).is_pending());
        transport.packets.borrow_mut().push_back(completed(false));
        pump(&controller);
        assert!(matches!(
            command.as_mut().poll(&mut cx),
            Poll::Ready(Ok(crate::param::BdAddr([1, 2, 3, 4, 5, 6])))
        ));
    }
    #[test]
    /// Unmatched completions leave the owner pending; a legal short failure keeps its exact HCI error.
    fn error_status_and_unknown_opcode_do_not_fabricate_success() {
        let transport = Offline::default();
        let controller = ExternalController::<_, 2>::new(&transport);
        let cmd = ReadBdAddr::new();
        let mut command = std::pin::pin!(controller.exec(&cmd));
        let mut cx = Context::from_waker(Waker::noop());
        assert!(command.as_mut().poll(&mut cx).is_pending());
        let mut foreign = completed(false);
        foreign[4] = 8;
        transport.packets.borrow_mut().push_back(foreign);
        pump(&controller);
        assert!(command.as_mut().poll(&mut cx).is_pending());
        // Status-only error response is legal even though the command's success body is six bytes.
        transport
            .packets
            .borrow_mut()
            .push_back(vec![4, 0x0E, 4, 1, 9, 0x10, 1]);
        pump(&controller);
        assert!(matches!(
            command.as_mut().poll(&mut cx),
            Poll::Ready(Err(crate::cmd::Error::Hci(crate::param::Error::UNKNOWN_CMD)))
        ));
    }

    #[test]
    /// Canceled ownership retains its return-length bound without retaining the old caller pointer.
    fn oversized_abandoned_reply_does_not_release_quarantine() {
        let transport = Offline::default();
        let controller = ExternalController::<_, 2>::new(&transport);
        let cmd = ReadBdAddr::new();
        let mut cx = Context::from_waker(Waker::noop());
        let mut old = std::boxed::Box::pin(controller.exec(&cmd));
        assert!(old.as_mut().poll(&mut cx).is_pending());
        drop(old);
        transport.packets.borrow_mut().push_back(completed(true));
        let mut buf = Controller::alloc_buf(&controller).unwrap();
        assert!(matches!(
            crate::controller::blocking::Controller::try_read(&controller, &mut buf),
            Err(crate::controller::blocking::TryError::Error(_))
        ));
        assert_eq!(controller.slots.permits.get(), 0);
        transport.packets.borrow_mut().push_back(completed(false));
        pump(&controller);
        let mut next = std::pin::pin!(controller.exec(&cmd));
        assert!(next.as_mut().poll(&mut cx).is_pending());
        assert_eq!(transport.writes.get(), 2);
    }
    #[test]
    /// Malformed Reset framing cannot mutate other commands or the credit count.
    fn malformed_reset_cannot_broadcast_or_publish_credits() {
        use crate::cmd::controller_baseband::Reset;
        let transport = Offline::default();
        let controller = ExternalController::<_, 2>::new(&transport);
        let cmd = ReadBdAddr::new();
        let mut cx = Context::from_waker(Waker::noop());
        let mut first = std::pin::pin!(controller.exec(&cmd));
        assert!(first.as_mut().poll(&mut cx).is_pending());
        let reset = Reset::new();
        let mut reset_command = std::pin::pin!(controller.exec(&reset));
        assert!(reset_command.as_mut().poll(&mut cx).is_pending());
        transport
            .packets
            .borrow_mut()
            .push_back(vec![4, 0x0E, 5, 9, 3, 0x0C, 0, 0]);
        let mut buf = Controller::alloc_buf(&controller).unwrap();
        assert!(matches!(
            crate::controller::blocking::Controller::try_read(&controller, &mut buf),
            Err(crate::controller::blocking::TryError::Error(_))
        ));
        assert!(first.as_mut().poll(&mut cx).is_pending());
        assert!(reset_command.as_mut().poll(&mut cx).is_pending());
        assert_eq!(controller.slots.permits.get(), 0);
    }
    #[test]
    /// Async commands have zero-byte return storage and complete from a valid CommandStatus.
    fn async_command_rejects_nonempty_return_and_accepts_status() {
        let transport = Offline::default();
        let controller = ExternalController::<_, 2>::new(&transport);
        let cmd = crate::cmd::le::LeEnableEncryption::new(crate::param::ConnHandle::new(7), [0; 8], 0, [0; 16]);
        let mut command = std::pin::pin!(crate::controller::ControllerCmdAsync::exec(&controller, &cmd));
        let mut cx = Context::from_waker(Waker::noop());
        assert!(command.as_mut().poll(&mut cx).is_pending());
        transport
            .packets
            .borrow_mut()
            .push_back(vec![4, 0x0E, 5, 1, 0x19, 0x20, 0, 1]);
        let mut buf = Controller::alloc_buf(&controller).unwrap();
        assert!(matches!(
            crate::controller::blocking::Controller::try_read(&controller, &mut buf),
            Err(crate::controller::blocking::TryError::Error(_))
        ));
        assert!(command.as_mut().poll(&mut cx).is_pending());
        transport
            .packets
            .borrow_mut()
            .push_back(vec![4, 0x0F, 4, 0, 1, 0x19, 0x20]);
        pump(&controller);
        assert!(matches!(command.as_mut().poll(&mut cx), Poll::Ready(Ok(()))));
    }

    #[test]
    /// Only successful Reset establishes the end of other unresolved command lifetimes.
    fn failed_reset_does_not_clear_abandoned_command() {
        let transport = Offline::default();
        let controller = ExternalController::<_, 2>::new(&transport);
        let cmd = ReadBdAddr::new();
        let mut cx = Context::from_waker(Waker::noop());
        let mut old = std::boxed::Box::pin(controller.exec(&cmd));
        assert!(old.as_mut().poll(&mut cx).is_pending());
        drop(old);
        let reset = crate::cmd::controller_baseband::Reset::new();
        let mut reset_command = std::pin::pin!(controller.exec(&reset));
        assert!(reset_command.as_mut().poll(&mut cx).is_pending());
        transport
            .packets
            .borrow_mut()
            .push_back(vec![4, 0x0E, 4, 1, 3, 0x0C, 1]);
        pump(&controller);
        assert!(matches!(
            reset_command.as_mut().poll(&mut cx),
            Poll::Ready(Err(crate::cmd::Error::Hci(crate::param::Error::UNKNOWN_CMD)))
        ));
        let mut next = std::pin::pin!(controller.exec(&cmd));
        assert!(next.as_mut().poll(&mut cx).is_pending());
        assert_eq!(
            transport.writes.get(),
            2,
            "failed Reset released an unresolved old opcode"
        );
        transport.packets.borrow_mut().push_back(completed(false));
        pump(&controller);
        assert!(next.as_mut().poll(&mut cx).is_pending());
        assert_eq!(transport.writes.get(), 3);
    }

    #[test]
    /// A matched completion replaces the remaining allowance, including a lower value or zero.
    fn matched_completion_can_reduce_command_allowance() {
        for allowance in [1, 0] {
            let transport = Offline::default();
            let controller = ExternalController::<_, 4>::new(&transport);
            let mut initial = completed(false);
            initial[3] = 3;
            transport.packets.borrow_mut().push_back(initial);
            pump(&controller);
            let cmd = ReadBdAddr::new();
            let mut cx = Context::from_waker(Waker::noop());
            let mut owner = std::pin::pin!(controller.exec(&cmd));
            assert!(owner.as_mut().poll(&mut cx).is_pending());
            let mut response = completed(false);
            response[3] = allowance;
            transport.packets.borrow_mut().push_back(response);
            pump(&controller);
            assert!(matches!(owner.as_mut().poll(&mut cx), Poll::Ready(Ok(_))));
            let mut first = std::pin::pin!(controller.exec(&cmd));
            let other = crate::cmd::info::ReadLocalVersionInformation::new();
            let mut second = std::pin::pin!(controller.exec(&other));
            assert!(first.as_mut().poll(&mut cx).is_pending());
            assert!(second.as_mut().poll(&mut cx).is_pending());
            assert_eq!(
                transport.writes.get(),
                1 + usize::from(allowance),
                "a lower completion allowance must stop excess transport submissions"
            );
        }
    }

    /// Sends an opcode-zero credit report through actual async or blocking receive.
    /// `status_event` selects CommandStatus instead of the status-free CommandComplete layout.
    /// Injects `allowance` into `transport` and pumps `controller`; `blocking` selects try_read instead
    /// of polling the async reader. Panics on allocation or receive failure; no physical I/O occurs.
    fn report_allowance(
        controller: &ExternalController<&Offline, 4>,
        transport: &Offline,
        allowance: u8,
        status_event: bool,
        blocking: bool,
    ) {
        let bytes = if status_event {
            vec![4, 0x0F, 4, 0, allowance, 0, 0]
        } else {
            vec![4, 0x0E, 3, allowance, 0, 0]
        };
        transport.packets.borrow_mut().push_back(bytes);
        let mut buf = controller.alloc_buf().unwrap();
        if blocking {
            match crate::controller::blocking::Controller::try_read(controller, &mut buf) {
                Ok(_) | Err(crate::controller::blocking::TryError::Busy) => {}
                Err(crate::controller::blocking::TryError::Error(error)) => panic!("{error}"),
            }
        } else {
            let mut rx = std::pin::pin!(controller.read(&mut buf));
            let mut cx = Context::from_waker(Waker::noop());
            assert!(!matches!(rx.as_mut().poll(&mut cx), Poll::Ready(Err(_))));
        }
    }

    #[test]
    /// Both event layouts and receive APIs honor stop/resume reports and wake every credit waiter.
    fn opcode_zero_allowance_replaces_credits_and_wakes_waiters() {
        for status_event in [true, false] {
            for blocking in [false, true] {
                let transport = Offline::default();
                let controller = ExternalController::<_, 4>::new(&transport);
                report_allowance(&controller, &transport, 0, status_event, blocking);
                let cmd = ReadBdAddr::new();
                let other = crate::cmd::info::ReadLocalVersionInformation::new();
                let a = Arc::new(Wakes::default());
                let b = Arc::new(Wakes::default());
                let wa = Waker::from(a.clone());
                let wb = Waker::from(b.clone());
                let mut ca = Context::from_waker(&wa);
                let mut cb = Context::from_waker(&wb);
                let mut first = std::boxed::Box::pin(controller.exec(&cmd));
                let mut second = std::pin::pin!(controller.exec(&other));
                assert!(first.as_mut().poll(&mut ca).is_pending());
                assert!(second.as_mut().poll(&mut cb).is_pending());
                assert_eq!(transport.writes.get(), 0, "zero credit must stop both receive paths");
                report_allowance(&controller, &transport, 1, status_event, blocking);
                assert!(a.0.load(Ordering::SeqCst) > 0);
                assert!(b.0.load(Ordering::SeqCst) > 0);
                // Revocation after wake must recheck allowance, without panicking or submitting.
                report_allowance(&controller, &transport, 0, status_event, blocking);
                assert!(first.as_mut().poll(&mut ca).is_pending());
                assert!(second.as_mut().poll(&mut cb).is_pending());
                assert_eq!(transport.writes.get(), 0);
                report_allowance(&controller, &transport, 1, status_event, blocking);
                drop(first);
                assert!(second.as_mut().poll(&mut cb).is_pending());
                assert_eq!(
                    transport.writes.get(),
                    1,
                    "canceled notified waiter must not consume credit"
                );
            }
        }
    }
}
