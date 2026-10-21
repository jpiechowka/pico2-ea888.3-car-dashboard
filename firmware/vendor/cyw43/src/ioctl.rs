//! Single outstanding firmware IOCTL shared between Control and Runner on one serialized executor.
//! Control supplies a drop guard to clear raw-buffer state before its borrowed buffer expires.
use core::cell::{Cell, RefCell};
use core::future::{Future, poll_fn};
use core::task::{Poll, Waker};

use embassy_sync::waitqueue::WakerRegistration;

use crate::consts::Ioctl;

#[derive(Clone, Copy, PartialEq, Eq)]
/// CDC command-direction flag encoded in the firmware request header.
pub enum IoctlType {
    /// Fetch firmware state into the shared request/response buffer.
    Get = 0,
    /// Apply buffer contents as firmware state.
    Set = 2,
}

#[derive(Clone, Copy)]
/// Raw-buffer request handed to the runner; valid only under the active Control borrow.
pub struct PendingIoctl {
    /// Request bytes and response capacity; caller retains storage until completion/cancellation.
    pub buf: *mut [u8],
    /// Get/set CDC flags.
    pub kind: IoctlType,
    /// Firmware command identifier.
    pub cmd: Ioctl,
    /// Firmware interface number, encoded by the runner into CDC flags.
    pub iface: u32,
}

#[derive(Clone, Copy)]
/// Executor-local request lifecycle; Done also represents idle/canceled.
enum IoctlStateInner {
    /// Awaiting runner ownership of the request.
    Pending(
        /// Request descriptor and borrowed storage.
        PendingIoctl,
    ),
    /// Runner consumed the request; a matching reply may still write the buffer.
    Sent {
        /// Borrowed response storage, cleared by the caller cancellation guard.
        buf: *mut [u8],
    },
    /// Finished or canceled; no borrowed buffer pointer remains.
    Done {
        /// Response byte count; zero also represents initial idle/canceled state.
        resp_len: usize,
    },
}

/// One wake registration for each side of the IOCTL handoff.
struct Wakers {
    /// Caller waiting for a response length.
    control: WakerRegistration,
    /// Runner waiting for a pending command.
    runner: WakerRegistration,
}

impl Wakers {
    /// Returns empty caller/runner wake registrations.
    const fn new() -> Self {
        Self {
            control: WakerRegistration::new(),
            runner: WakerRegistration::new(),
        }
    }
}

/// Interior-mutability handoff for one request; concurrent callers are not admitted here.
pub struct IoctlState {
    /// Current request phase; raw buffer lifetime is enforced by Control rather than this Cell.
    state: Cell<IoctlStateInner>,
    /// Executor-local waiter registrations; reentrant mutable borrowing panics.
    wakers: RefCell<Wakers>,
}

impl IoctlState {
    /// Returns idle state with response length zero and no registered waiters.
    pub const fn new() -> Self {
        Self {
            state: Cell::new(IoctlStateInner::Done { resp_len: 0 }),
            wakers: RefCell::new(Wakers::new()),
        }
    }

    /// Wakes and clears the registered response waiter, if any.
    fn wake_control(&self) {
        self.wakers.borrow_mut().control.wake();
    }

    /// Registers `waker` for Control response completion, replacing any prior waiter.
    fn register_control(&self, waker: &Waker) {
        self.wakers.borrow_mut().control.register(waker);
    }

    /// Wakes and clears the runner registration, if any.
    fn wake_runner(&self) {
        self.wakers.borrow_mut().runner.wake();
    }

    /// Registers `waker` for the next pending request, replacing any prior waiter.
    fn register_runner(&self, waker: &Waker) {
        self.wakers.borrow_mut().runner.register(waker);
    }

    /// Returns a future resolving to the stored response count when Done; initial/canceled returns zero.
    /// Waiting itself does not cancel or clear a pending buffer.
    pub fn wait_complete(&self) -> impl Future<Output = usize> + '_ {
        poll_fn(|cx| {
            if let IoctlStateInner::Done { resp_len } = self.state.get() {
                Poll::Ready(resp_len)
            } else {
                self.register_control(cx.waker());
                Poll::Pending
            }
        })
    }

    /// Returns a future that consumes Pending into Sent and yields its raw request descriptor.
    /// Dropping after readiness requires external cancellation/lifetime coordination.
    pub fn wait_pending(&self) -> impl Future<Output = PendingIoctl> + '_ {
        poll_fn(|cx| {
            if let IoctlStateInner::Pending(pending) = self.state.get() {
                self.state.set(IoctlStateInner::Sent { buf: pending.buf });
                Poll::Ready(pending)
            } else {
                self.register_runner(cx.waker());
                Poll::Pending
            }
        })
    }

    /// Clears any raw buffer pointer by marking Done(0); sends no controller cancel and wakes no waiter.
    pub fn cancel_ioctl(&self) {
        self.state.set(IoctlStateInner::Done { resp_len: 0 });
    }

    /// Publishes `kind`/`cmd`/`iface` using borrowed `buf`, then returns the received byte count.
    /// Caller must serialize IOCTLs and install cancellation cleanup before awaiting: this function
    /// itself has no drop guard and retains a raw buffer pointer while pending/sent.
    pub async fn do_ioctl(&self, kind: IoctlType, cmd: Ioctl, iface: u32, buf: &mut [u8]) -> usize {
        self.state
            .set(IoctlStateInner::Pending(PendingIoctl { buf, kind, cmd, iface }));
        self.wake_runner();
        self.wait_complete().await
    }

    /// Copies matching `response` into the sent buffer, marks Done and wakes Control.
    /// Panics if response exceeds capacity; unsolicited/canceled replies are logged and ignored.
    pub fn ioctl_done(&self, response: &[u8]) {
        if let IoctlStateInner::Sent { buf } = self.state.get() {
            trace!("IOCTL response: {} bytes", response.len());

            // TODO fix this
            (unsafe { &mut *buf }[..response.len()]).copy_from_slice(response);

            self.state.set(IoctlStateInner::Done {
                resp_len: response.len(),
            });
            self.wake_control();
        } else {
            warn!("IOCTL Response but no pending Ioctl");
        }
    }
}
