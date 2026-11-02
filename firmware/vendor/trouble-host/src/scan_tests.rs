//! Actual scanner-future admission and cleanup regressions, without controller hardware.
extern crate std;

use core::cell::{Cell, RefCell};
use core::future::Future;
use core::pin::pin;
use core::task::{Context, Poll, Waker};

use bt_hci::cmd::{self, AsyncCmd, SyncCmd};
use bt_hci::controller::{Controller, ControllerCmdAsync, ControllerCmdSync};
use bt_hci::{FromHciBytes, WriteHci};
use embassy_futures::block_on;

use crate::connection::ScanConfig;
use crate::host::InitialState;
use crate::prelude::DefaultPacketPool;
use crate::scan::Scanner;
use crate::HostResources;

/// Inert HCI boundary retaining emitted commands; an optional opcode injects a controller refusal.
#[derive(Default)]
struct ScanController {
    /// Commands emitted by production scanner/control code, including failed commands.
    commands: RefCell<std::vec::Vec<(u16, std::vec::Vec<u8>)>>,
    /// Opcode rejected with Command Disallowed; None accepts setup commands.
    fail: Cell<Option<u16>>,
}

impl embedded_io::ErrorType for ScanController {
    /// This fixture has no transport I/O; refusals use HCI status.
    type Error = core::convert::Infallible;
}

impl Controller for ScanController {
    /// Owned HCI input scratch, unused because receive remains pending.
    type Buffer<'a> = [u8; 259];
    /// Returns initialized input scratch without a device.
    fn alloc_buf(&self) -> Result<Self::Buffer<'_>, Self::Error> {
        Ok([0; 259])
    }
    /// Waits indefinitely without changing `_buf`; never returns, tests drive command state explicitly.
    async fn read<'a>(
        &self,
        _buf: &'a mut Self::Buffer<'_>,
    ) -> Result<bt_hci::ControllerToHostPacket<'a>, Self::Error> {
        core::future::pending().await
    }
    /// Panics without returning on unexpected ACL traffic; scanning must never send it.
    async fn write_acl_data(&self, _: &bt_hci::data::AclPacket<'_>) -> Result<(), Self::Error> {
        unreachable!()
    }
    /// Panics without returning on unexpected synchronous data.
    async fn write_sync_data(&self, _: &bt_hci::data::SyncPacket<'_>) -> Result<(), Self::Error> {
        unreachable!()
    }
    /// Panics without returning on unexpected ISO data.
    async fn write_iso_data(&self, _: &bt_hci::data::IsoPacket<'_>) -> Result<(), Self::Error> {
        unreachable!()
    }
}

impl<C: SyncCmd> ControllerCmdSync<C> for ScanController {
    /// Records serialized `command` and returns unit success or injected HCI refusal; unsupported return types panic.
    async fn exec(&self, command: &C) -> Result<C::Return, cmd::Error<Self::Error>> {
        let opcode = C::OPCODE.to_raw();
        let mut bytes = std::vec![0; command.params().size()];
        command.params().write_hci(&mut bytes[..]).unwrap();
        self.commands.borrow_mut().push((opcode, bytes));
        if self.fail.get() == Some(opcode) {
            return Err(cmd::Error::Hci(bt_hci::param::Error::CMD_DISALLOWED));
        }
        Ok(C::Return::from_hci_bytes(&[]).unwrap().0)
    }
}

impl<C: AsyncCmd> ControllerCmdAsync<C> for ScanController {
    /// Panics without returning on asynchronous commands; scanner setup uses synchronous commands only.
    async fn exec(&self, _: &C) -> Result<(), cmd::Error<Self::Error>> {
        unreachable!()
    }
}

/// Publishes a finite controller ACL capability in `stack` to bypass unrelated boot commands.
fn initialized(stack: &crate::Stack<'_, ScanController, DefaultPacketPool>) {
    stack
        .host()
        .state
        .initialized
        .init(InitialState {
            acl_max: 251,
            acl_total: 1,
        })
        .ok()
        .unwrap();
}

#[test]
/// A canceled unadmitted legacy/extended waiter must leave an existing scan and its cancellation kind untouched.
fn dropped_scan_waiter_cannot_cancel_the_active_session() {
    for active_extended in [false, true] {
        for waiting_extended in [false, true] {
            let mut resources = HostResources::<DefaultPacketPool, 1, 1>::new();
            let stack = crate::new(ScanController::default(), &mut resources).build();
            initialized(&stack);
            let mut central = stack.central();
            let mut scanner = Scanner::new(&mut central);
            let config = ScanConfig::default();
            let mut context = Context::from_waker(Waker::noop());
            // Keep the real successful session alive while a second scanner waits for its admission.
            let legacy = if active_extended {
                None
            } else {
                Some(block_on(scanner.scan(&config)).unwrap())
            };
            let extended = if active_extended {
                Some(block_on(scanner.scan_ext(&config)).unwrap())
            } else {
                None
            };
            let emitted = stack.controller.commands.borrow().len();
            if waiting_extended {
                let mut pending = pin!(scanner.scan_ext(&config));
                assert!(pending.as_mut().poll(&mut context).is_pending());
            } else {
                let mut pending = pin!(scanner.scan(&config));
                assert!(pending.as_mut().poll(&mut context).is_pending());
            }
            assert!(
                stack.host().scan_command_state().is_active(),
                "unadmitted waiter canceled owner"
            );
            assert_eq!(stack.controller.commands.borrow().len(), emitted);
            drop(legacy);
            drop(extended);
            assert_eq!(
                stack.host().scan_command_state().poll_cancelled(&mut context),
                Poll::Ready(active_extended)
            );
        }
    }
}

#[test]
/// An admitted setup error retains the correct disable request, and a subsequent canceled waiter cannot replace it.
fn setup_failure_retains_original_cleanup_context() {
    for extended in [false, true] {
        let mut resources = HostResources::<DefaultPacketPool, 1, 1>::new();
        let stack = crate::new(ScanController::default(), &mut resources).build();
        initialized(&stack);
        stack.controller.fail.set(Some(if extended { 0x2041 } else { 0x200b }));
        let mut central = stack.central();
        let mut scanner = Scanner::new(&mut central);
        let config = ScanConfig::default();
        if extended {
            assert!(block_on(scanner.scan_ext(&config)).is_err());
        } else {
            assert!(block_on(scanner.scan(&config)).is_err());
        }
        let mut context = Context::from_waker(Waker::noop());
        assert_eq!(
            stack.host().scan_command_state().poll_cancelled(&mut context),
            Poll::Ready(extended)
        );
        {
            let mut waiter = pin!(scanner.scan(&config));
            assert!(waiter.as_mut().poll(&mut context).is_pending());
        }
        assert_eq!(
            stack.host().scan_command_state().poll_cancelled(&mut context),
            Poll::Ready(extended)
        );
        assert!(stack
            .controller
            .commands
            .borrow()
            .iter()
            .all(|(opcode, _)| ![0x200c, 0x2042].contains(opcode)));
    }
}

#[test]
/// Canceling admitted setup before initialization must still request cleanup, unlike canceling an admission waiter.
fn cancellation_after_admission_retains_cleanup() {
    for extended in [false, true] {
        let mut resources = HostResources::<DefaultPacketPool, 1, 1>::new();
        let stack = crate::new(ScanController::default(), &mut resources).build();
        let mut central = stack.central();
        let mut scanner = Scanner::new(&mut central);
        let config = ScanConfig::default();
        let mut context = Context::from_waker(Waker::noop());
        if extended {
            let mut setup = pin!(scanner.scan_ext(&config));
            assert!(setup.as_mut().poll(&mut context).is_pending());
        } else {
            let mut setup = pin!(scanner.scan(&config));
            assert!(setup.as_mut().poll(&mut context).is_pending());
        }
        assert_eq!(
            stack.host().scan_command_state().poll_cancelled(&mut context),
            Poll::Ready(extended)
        );
        assert!(stack.controller.commands.borrow().is_empty());
    }
}

#[test]
/// Natural timeout retains lease admission until explicit cleanup; a canceled stop cannot prematurely admit a replacement.
fn timeout_and_stop_preserve_admission_until_cleanup_acknowledgment() {
    let mut resources = HostResources::<DefaultPacketPool, 1, 1>::new();
    let stack = crate::new(ScanController::default(), &mut resources).build();
    initialized(&stack);
    let mut central = stack.central();
    let mut scanner = Scanner::new(&mut central);
    let config = ScanConfig::default();
    let mut active = block_on(scanner.scan_ext(&config)).unwrap();
    let mut context = Context::from_waker(Waker::noop());
    stack.host().scan_timeout().signal(());
    assert!(core::pin::Pin::new(&mut active).poll(&mut context).is_ready());
    {
        let mut waiter = pin!(scanner.scan(&config));
        assert!(waiter.as_mut().poll(&mut context).is_pending());
    }
    assert!(stack.host().scan_command_state().is_active());
    {
        let mut stop = pin!(active.stop());
        assert!(stop.as_mut().poll(&mut context).is_pending());
    }
    assert!(!stack.host().scan_command_state().is_idle());
    let mut replacement = pin!(scanner.scan(&config));
    assert!(replacement.as_mut().poll(&mut context).is_pending());
    // Simulates the successful disable acknowledgment consumed by ControlRunner.
    stack.host().scan_command_state().canceled();
    assert!(matches!(replacement.as_mut().poll(&mut context), Poll::Ready(Ok(_))));
}
