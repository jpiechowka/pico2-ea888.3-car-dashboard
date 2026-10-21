//! Reference-counted LE credit-based channels, backed by the stack runner and packet pool.
//! Every stack argument must identify the originating stack; numeric channel indices are local to that stack.
//! Last-owner drop requests asynchronous disconnection; it is not a peer acknowledgment or a radio stop barrier.
/// Returns Ok only when `manager` belongs to supplied `stack`; rejects equal indices in unrelated stacks before I/O or ownership transfer.
fn check_stack<T: Controller, P: PacketPool>(
    manager: &ChannelManager<'_, P>,
    stack: &Stack<'_, T, P>,
) -> Result<(), Error> {
    if core::ptr::eq(manager, stack.host().channels()) {
        Ok(())
    } else {
        Err(Error::InvalidValue)
    }
}

use core::future::{poll_fn, Future};
use core::task::Poll;

use bt_hci::controller::{blocking, Controller};
use bt_hci::param::ConnHandle;

pub use crate::channel_manager::CreditFlowPolicy;
#[cfg(feature = "channel-metrics")]
pub use crate::channel_manager::Metrics as ChannelMetrics;
use crate::channel_manager::{ChannelIndex, ChannelManager};
use crate::connection::Connection;
use crate::host::BleHost;
use crate::pdu::Sdu;
pub use crate::types::l2cap::LeCreditConnResultCode;
use crate::{BleHostError, Error, PacketPool, Stack};

pub(crate) mod sar;

/// Handle representing an L2CAP channel.
pub struct L2capChannel<'d, P: PacketPool> {
    /// Stable slot index retained by this handle's manager reference count.
    index: ChannelIndex,
    /// Originating stack's channel state; must outlive every handle and active operation.
    manager: &'d ChannelManager<'d, P>,
}

/// Handle representing an L2CAP channel write endpoint.
pub struct L2capChannelWriter<'d, P: PacketPool> {
    /// Stable slot index retained by this handle's manager reference count.
    index: ChannelIndex,
    /// Originating stack's channel state; must outlive every handle and active operation.
    manager: &'d ChannelManager<'d, P>,
}

/// Receive half of an L2CAP channel; owns one manager reference.
pub struct L2capChannelReader<'d, P: PacketPool> {
    /// Stable slot index retained by this handle's manager reference count.
    index: ChannelIndex,
    /// Originating stack's channel state; must outlive every handle and active operation.
    manager: &'d ChannelManager<'d, P>,
}

/// Retained channel reference for metrics; keeps its slot from reuse until dropped.
pub struct L2capChannelRef<'d, P: PacketPool> {
    /// Stable slot index retained by this handle's manager reference count.
    index: ChannelIndex,
    /// Originating stack's channel state; must outlive every handle and active operation.
    manager: &'d ChannelManager<'d, P>,
}

#[cfg(feature = "defmt")]
impl<P: PacketPool> defmt::Format for L2capChannel<'_, P> {
    /// Writes slot identity and manager-state metadata through `f`.
    fn format(&self, f: defmt::Formatter<'_>) {
        defmt::write!(f, "{}, ", self.index);
        self.manager.print(self.index, f);
    }
}

impl<P: PacketPool> Drop for L2capChannel<'_, P> {
    /// Releases one reference; last release schedules channel disconnection without waiting for the peer.
    fn drop(&mut self) {
        self.manager.dec_ref(self.index);
    }
}

impl<P: PacketPool> Drop for L2capChannelRef<'_, P> {
    /// Releases one reference; last release schedules channel disconnection without waiting for the peer.
    fn drop(&mut self) {
        self.manager.dec_ref(self.index);
    }
}

#[cfg(feature = "defmt")]
impl<P: PacketPool> defmt::Format for L2capChannelWriter<'_, P> {
    /// Writes slot identity and manager-state metadata through `f`.
    fn format(&self, f: defmt::Formatter<'_>) {
        defmt::write!(f, "{}, ", self.index);
        self.manager.print(self.index, f);
    }
}

impl<P: PacketPool> Drop for L2capChannelWriter<'_, P> {
    /// Releases one reference; last release schedules channel disconnection without waiting for the peer.
    fn drop(&mut self) {
        self.manager.dec_ref(self.index);
    }
}

#[cfg(feature = "defmt")]
impl<P: PacketPool> defmt::Format for L2capChannelReader<'_, P> {
    /// Writes slot identity and manager-state metadata through `f`.
    fn format(&self, f: defmt::Formatter<'_>) {
        defmt::write!(f, "{}, ", self.index);
        self.manager.print(self.index, f);
    }
}

impl<P: PacketPool> Drop for L2capChannelReader<'_, P> {
    /// Releases one reference; last release schedules channel disconnection without waiting for the peer.
    fn drop(&mut self) {
        self.manager.dec_ref(self.index);
    }
}

/// A pending incoming L2CAP connection that can be inspected, then accepted or rejected.
///
/// Dropping this without calling [`accept`](Self::accept) or [`reject`](Self::reject) will
/// silently free the channel slot (the peer will time out).
pub struct L2capPendingConnection<'d, P: PacketPool> {
    /// Owned pending-slot reference, taken when acceptance/rejection assumes cleanup.
    index: Option<ChannelIndex>,
    /// Originating stack's channel state; must outlive every handle and active operation.
    manager: &'d ChannelManager<'d, P>,
    /// Originating connection handle snapshot; does not itself retain a Connection owner.
    conn: ConnHandle,
}

impl<'d, P: PacketPool> L2capPendingConnection<'d, P> {
    /// Returns a pending token for `index` in `manager` on `conn`; caller has already incremented its reference.
    pub(crate) fn new(index: ChannelIndex, manager: &'d ChannelManager<'d, P>, conn: ConnHandle) -> Self {
        Self {
            index: Some(index),
            manager,
            conn,
        }
    }

    /// Returns the consumed pending slot index, transferring its reference and cleanup obligation to the caller.
    pub(crate) fn into_index(mut self) -> ChannelIndex {
        self.index.take().unwrap()
    }

    /// Returns the peer-requested Simplified Protocol/Service Multiplexer value.
    pub fn spsm(&self) -> u16 {
        self.manager.spsm(self.index.unwrap())
    }

    /// Returns the peer receive MTU in bytes before local acceptance.
    pub fn mtu(&self) -> u16 {
        self.manager.peer_mtu(self.index.unwrap())
    }

    /// Accepts through originating `stack` using local receive/credit `config`; returns the established channel.
    /// Invalid config, stale state or signaling failure returns an error. Cancellation releases the pending reference
    /// and may request disconnection if success already reached the peer; the runner must remain active.
    /// Returns InvalidValue before mutation when stack belongs to a different host.
    pub async fn accept<T: Controller>(
        self,
        stack: &'d Stack<'_, T, P>,
        config: &L2capChannelConfig,
    ) -> Result<L2capChannel<'d, P>, BleHostError<T::Error>> {
        check_stack(self.manager, stack)?;
        let index = self.into_index();
        stack
            .host()
            .channels()
            .accept_pending(index, config, stack.host())
            .await
    }

    /// Sends rejection `result` through originating `stack`, returning the signaling result.
    /// Local slot/reference cleanup runs on success, error or cancellation; an interrupted reply may not reach the peer.
    /// Returns InvalidValue before mutation when stack belongs to a different host.
    pub async fn reject<T: Controller>(
        mut self,
        stack: &Stack<'_, T, P>,
        result: LeCreditConnResultCode,
    ) -> Result<(), BleHostError<T::Error>> {
        check_stack(self.manager, stack)?;
        let index = self.index.take().unwrap();
        stack
            .host()
            .channels()
            .reject_pending(index, result, stack.host())
            .await
    }
}

impl<P: PacketPool> Drop for L2capPendingConnection<'_, P> {
    /// Releases an unconsumed pending token; no automatic rejection response is sent.
    fn drop(&mut self) {
        if let Some(index) = self.index.take() {
            error!("L2capPendingConnection dropped without being accepted or rejected");
            self.manager.drop_pending(index);
        }
    }
}

/// Configuration for an L2CAP channel.
#[derive(Default)]
pub struct L2capChannelConfig {
    /// Local receive SDU byte limit, at least 23; None selects packet-pool MTU minus six.
    /// Complete receive storage must hold the declared SDU; oversize reassembly is rejected.
    pub mtu: Option<u16>,
    /// Local K-frame payload limit in bytes, 23..=65533 and at most pool MTU minus four.
    /// None selects pool MTU minus four; one K-frame consumes one channel credit.
    pub mps: Option<u16>,
    /// Flow control policy for connection oriented channels.
    pub flow_policy: CreditFlowPolicy,
    /// Initial receive credits advertised to the peer; zero is allowed.
    /// None selects min(receive queue capacity, total packet-pool slots), narrowed to u16.
    pub initial_credits: Option<u16>,
}

impl<'d, P: PacketPool> L2capChannel<'d, P> {
    /// Returns a channel owning the already-retained `index` in `manager`; does not increment the count.
    pub(crate) fn new(index: ChannelIndex, manager: &'d ChannelManager<'d, P>) -> Self {
        Self { index, manager }
    }

    /// Requests asynchronous channel disconnection and wakes the runner; returns before peer acknowledgment.
    pub fn disconnect(&mut self) {
        self.manager.disconnect(self.index);
    }

    /// Returns the channel Simplified Protocol/Service Multiplexer value.
    pub fn spsm(&self) -> u16 {
        self.manager.spsm(self.index)
    }

    /// Returns this endpoint's configured receive SDU limit in bytes.
    pub fn mtu(&self) -> u16 {
        self.manager.mtu(self.index)
    }

    /// Returns this endpoint's configured receive K-frame payload limit in bytes.
    pub fn mps(&self) -> u16 {
        self.manager.mps(self.index)
    }

    /// Returns the peer receive SDU limit in bytes, which bounds sends.
    pub fn peer_mtu(&self) -> u16 {
        self.manager.peer_mtu(self.index)
    }

    /// Returns the peer receive K-frame payload limit in bytes, which controls segmentation.
    pub fn peer_mps(&self) -> u16 {
        self.manager.peer_mps(self.index)
    }

    /// Sends SDU bytes `buf` through originating `stack`; returns Ok after controller writes, not peer delivery.
    /// Allocates one scratch packet; returns OutOfMemory, size/channel errors or controller errors as applicable.
    /// Waits for channel/ACL credits without a deadline. Cancellation/error may leave a transmitted prefix;
    /// unused grants are refunded but completed fragments cannot be recalled or blindly retried.
    /// Returns InvalidValue before mutation when stack belongs to a different host.
    pub async fn send<T: Controller>(
        &mut self,
        stack: &Stack<'_, T, P>,
        buf: &[u8],
    ) -> Result<(), BleHostError<T::Error>> {
        check_stack(self.manager, stack)?;
        let mut p_buf = P::allocate().ok_or(Error::OutOfMemory)?;
        stack
            .host()
            .channels()
            .send(self.index, buf, p_buf.as_mut(), stack.host())
            .await
    }

    /// Attempts SDU bytes `buf` through originating `stack`, returning Ok after all controller writes.
    /// Returns pool/size/channel/controller errors, Busy for backpressure, or NoPermits for impossible ACL demand.
    /// A controller error/Busy can follow a transmitted prefix; unused credits are refunded, not sent data.
    /// Returns InvalidValue before mutation when stack belongs to a different host.
    pub fn try_send<T: Controller + blocking::Controller>(
        &mut self,
        stack: &Stack<'_, T, P>,
        buf: &[u8],
    ) -> Result<(), BleHostError<T::Error>> {
        check_stack(self.manager, stack)?;
        let mut p_buf = P::allocate().ok_or(Error::OutOfMemory)?;
        stack
            .host()
            .channels()
            .try_send(self.index, buf, p_buf.as_mut(), stack.host())
    }

    /// Receives via originating `stack`, copies min(SDU length, `buf` length) bytes and returns that copied count.
    /// A short buffer silently discards the SDU tail; untouched output tail remains unchanged. Use receive_sdu to retain all bytes.
    /// Waits without a deadline. After dequeue, credit-signaling error/cancellation loses the SDU and may leave buf modified.
    /// Returns InvalidValue before mutation when stack belongs to a different host.
    pub async fn receive<T: Controller>(
        &mut self,
        stack: &Stack<'_, T, P>,
        buf: &mut [u8],
    ) -> Result<usize, BleHostError<T::Error>> {
        check_stack(self.manager, stack)?;
        stack.host().channels().receive(self.index, buf, stack.host()).await
    }

    /// Returns the next owned SDU via originating `stack` after receive-credit processing.
    /// Closed-channel or signaling errors are returned. Cancellation after dequeue drops the SDU; before dequeue it consumes none.
    /// Returns InvalidValue before mutation when stack belongs to a different host.
    pub async fn receive_sdu<T: Controller>(
        &mut self,
        stack: &Stack<'_, T, P>,
    ) -> Result<Sdu<P::Packet>, BleHostError<T::Error>> {
        check_stack(self.manager, stack)?;
        stack.host().channels().receive_sdu(self.index, stack.host()).await
    }

    /// Returns `f` applied to the borrowed channel metrics; reentering mutable manager operations in f can panic.
    #[cfg(feature = "channel-metrics")]
    pub fn metrics<F: FnOnce(&ChannelMetrics) -> R, R>(&self, f: F) -> R {
        self.manager.metrics(self.index, f)
    }

    /// Returns a listener for `connection` on originating `stack`, retaining a cloned connection owner.
    /// SPSMs must already be registered on the stack. Panics before changing listener state for a duplicate
    /// listener, foreign stack or exhausted connection reference count. Dropping a listener releases admission.
    pub fn listen<T: Controller>(
        stack: &'d Stack<'_, T, P>,
        connection: &Connection<'d, P>,
    ) -> L2capChannelListener<'d, T, P> {
        connection
            .check_stack(stack)
            .expect("connection belongs to another stack");
        let conn = connection.clone();
        conn.set_l2cap_listening(true);
        L2capChannelListener {
            conn,
            host: stack.host(),
        }
    }

    /// Opens `spsm` on `connection` through its originating `stack` with receive/credit `config`, returning a channel.
    /// Returns local validation/resource, peer rejection, disconnect or controller errors; response wait has a 30-second RTX timeout.
    /// Initial controller-send waits are outside that timeout; cancellation releases the slot/reference and may schedule disconnect.
    /// Returns InvalidValue before mutation when stack belongs to a different host.
    pub async fn create<T: Controller>(
        stack: &'d Stack<'_, T, P>,
        connection: &Connection<'_, P>,
        spsm: u16,
        config: &L2capChannelConfig,
    ) -> Result<Self, BleHostError<T::Error>> {
        connection.check_stack(stack)?;
        stack
            .host()
            .channels()
            .create(connection.handle(), spsm, config, stack.host())
            .await
    }

    /// Returns independently owned write/read halves; consuming this handle leaves two retained references.
    /// Adds one net reference; panics if already at the u8 reference-count limit.
    pub fn split(self) -> (L2capChannelWriter<'d, P>, L2capChannelReader<'d, P>) {
        self.manager.inc_ref(self.index);
        let halves = (
            L2capChannelWriter {
                index: self.index,
                manager: self.manager,
            },
            L2capChannelReader {
                index: self.index,
                manager: self.manager,
            },
        );
        // Transfer the original reference to one half; the increment owns the other.
        core::mem::forget(self);
        halves
    }

    /// Returns one channel consuming `writer` and `reader`, releasing their separate references.
    /// Panics if manager identity or slot differs; transfers one reference and releases the other.
    pub fn merge(writer: L2capChannelWriter<'d, P>, reader: L2capChannelReader<'d, P>) -> Self {
        // A channel will not be reused unless the refcount is 0, so the index could
        // never be stale.
        assert!(
            core::ptr::eq(writer.manager, reader.manager),
            "channel halves belong to different stacks"
        );
        assert_eq!(writer.index, reader.index);

        let manager = writer.manager;
        let index = writer.index;
        // Reuse the writer's reference; reader drop releases the second owner.
        core::mem::forget(writer);
        Self { index, manager }
    }
}

impl<'d, P: PacketPool> L2capChannelReader<'d, P> {
    /// Requests asynchronous channel disconnection and wakes the runner; returns before peer acknowledgment.
    pub fn disconnect(&mut self) {
        self.manager.disconnect(self.index);
    }

    /// Returns this endpoint's configured receive SDU limit in bytes.
    pub fn mtu(&self) -> u16 {
        self.manager.mtu(self.index)
    }

    /// Returns this endpoint's configured receive K-frame payload limit in bytes.
    pub fn mps(&self) -> u16 {
        self.manager.mps(self.index)
    }

    /// Receives via originating `stack`, copies min(SDU length, `buf` length) bytes and returns that copied count.
    /// A short buffer silently discards the SDU tail; untouched output tail remains unchanged. Use receive_sdu to retain all bytes.
    /// Waits without a deadline. After dequeue, credit-signaling error/cancellation loses the SDU and may leave buf modified.
    /// Returns InvalidValue before mutation when stack belongs to a different host.
    pub async fn receive<T: Controller>(
        &mut self,
        stack: &Stack<'_, T, P>,
        buf: &mut [u8],
    ) -> Result<usize, BleHostError<T::Error>> {
        check_stack(self.manager, stack)?;
        stack.host().channels().receive(self.index, buf, stack.host()).await
    }

    /// Returns the next owned SDU via originating `stack` after receive-credit processing.
    /// Closed-channel or signaling errors are returned. Cancellation after dequeue drops the SDU; before dequeue it consumes none.
    /// Returns InvalidValue before mutation when stack belongs to a different host.
    pub async fn receive_sdu<T: Controller>(
        &mut self,
        stack: &Stack<'_, T, P>,
    ) -> Result<Sdu<P::Packet>, BleHostError<T::Error>> {
        check_stack(self.manager, stack)?;
        stack.host().channels().receive_sdu(self.index, stack.host()).await
    }

    /// Returns `f` applied to the borrowed channel metrics; reentering mutable manager operations in f can panic.
    #[cfg(feature = "channel-metrics")]
    pub fn metrics<F: FnOnce(&ChannelMetrics) -> R, R>(&self, f: F) -> R {
        self.manager.metrics(self.index, f)
    }

    /// Returns another retained metrics reference; panics at the u8 reference-count limit.
    pub fn channel_ref(&mut self) -> L2capChannelRef<'d, P> {
        self.manager.inc_ref(self.index);
        L2capChannelRef {
            index: self.index,
            manager: self.manager,
        }
    }
}

impl<'d, P: PacketPool> L2capChannelRef<'d, P> {
    #[cfg(feature = "channel-metrics")]
    /// Returns `f` applied to the borrowed channel metrics; reentering mutable manager operations in f can panic.
    pub fn metrics<F: FnOnce(&ChannelMetrics) -> R, R>(&self, f: F) -> R {
        self.manager.metrics(self.index, f)
    }
}

impl<'d, P: PacketPool> L2capChannelWriter<'d, P> {
    /// Requests asynchronous channel disconnection and wakes the runner; returns before peer acknowledgment.
    pub fn disconnect(&mut self) {
        self.manager.disconnect(self.index);
    }

    /// Returns the peer receive SDU limit in bytes, which bounds sends.
    pub fn peer_mtu(&self) -> u16 {
        self.manager.peer_mtu(self.index)
    }

    /// Returns the peer receive K-frame payload limit in bytes, which controls segmentation.
    pub fn peer_mps(&self) -> u16 {
        self.manager.peer_mps(self.index)
    }

    /// Sends SDU bytes `buf` through originating `stack`; returns Ok after controller writes, not peer delivery.
    /// Allocates one scratch packet; returns OutOfMemory, size/channel errors or controller errors as applicable.
    /// Waits for channel/ACL credits without a deadline. Cancellation/error may leave a transmitted prefix;
    /// unused grants are refunded but completed fragments cannot be recalled or blindly retried.
    /// Returns InvalidValue before mutation when stack belongs to a different host.
    pub async fn send<T: Controller>(
        &mut self,
        stack: &Stack<'_, T, P>,
        buf: &[u8],
    ) -> Result<(), BleHostError<T::Error>> {
        check_stack(self.manager, stack)?;
        let mut p_buf = P::allocate().ok_or(Error::OutOfMemory)?;
        stack
            .host()
            .channels()
            .send(self.index, buf, p_buf.as_mut(), stack.host())
            .await
    }

    /// Attempts SDU bytes `buf` through originating `stack`, returning Ok after all controller writes.
    /// Returns pool/size/channel/controller errors, Busy for backpressure, or NoPermits for impossible ACL demand.
    /// A controller error/Busy can follow a transmitted prefix; unused credits are refunded, not sent data.
    /// Returns InvalidValue before mutation when stack belongs to a different host.
    pub fn try_send<T: Controller + blocking::Controller>(
        &mut self,
        stack: &Stack<'_, T, P>,
        buf: &[u8],
    ) -> Result<(), BleHostError<T::Error>> {
        check_stack(self.manager, stack)?;
        let mut p_buf = P::allocate().ok_or(Error::OutOfMemory)?;
        stack
            .host()
            .channels()
            .try_send(self.index, buf, p_buf.as_mut(), stack.host())
    }

    /// Returns `f` applied to the borrowed channel metrics; reentering mutable manager operations in f can panic.
    #[cfg(feature = "channel-metrics")]
    pub fn metrics<F: FnOnce(&ChannelMetrics) -> R, R>(&self, f: F) -> R {
        self.manager.metrics(self.index, f)
    }

    /// Returns another retained metrics reference; panics at the u8 reference-count limit.
    pub fn channel_ref(&mut self) -> L2capChannelRef<'d, P> {
        self.manager.inc_ref(self.index);
        L2capChannelRef {
            index: self.index,
            manager: self.manager,
        }
    }
}

/// A listener for incoming L2CAP connection requests on a specific connection.
///
/// Created by [`L2capChannel::listen`]. When dropped, clears the listening flag
/// on the retained connection. While that exact link remains Connected, rejects requests not
/// yet returned by [`next`](Self::next) or [`accept`](Self::accept); after close it cannot act on a reused handle.
pub struct L2capChannelListener<'d, T: Controller, P: PacketPool> {
    /// Retained link owner whose shared listening flag is cleared on listener drop.
    conn: Connection<'d, P>,
    /// Originating host used for pending-request lookup, acceptance and best-effort rejection.
    host: BleHost<'d, T, P>,
}

impl<'d, T: Controller, P: PacketPool> L2capChannelListener<'d, T, P> {
    /// Returns the next owned pending request or Disconnected when its link is gone; waits without a deadline.
    /// Cancellation before a Ready result retains no request reference. Only one active waiter is supported;
    /// requests already owned by earlier pending tokens are skipped. Each poll checks the retained link slot
    /// before handle-based lookup, so handle reuse cannot redirect this listener. Terminal channel cleanup
    /// wakes the waiter; local close may remain asleep until that cleanup or another channel event.
    pub async fn next(&self) -> Result<L2capPendingConnection<'d, P>, Error> {
        let mut pending = core::pin::pin!(self
            .host
            .channels()
            .next_pending(self.conn.handle(), self.host.connections()));
        poll_fn(|cx| {
            // Check the retained slot before every poll: a terminal handle can be
            // reused by another peer while this listener/future is still owned.
            if !self.conn.is_connected() {
                return Poll::Ready(Err(Error::Disconnected));
            }
            pending.as_mut().poll(cx)
        })
        .await
    }

    /// Waits for a request and accepts local receive/credit `config`, returning its channel or admission/signaling error.
    /// Cancellation follows next/accept ownership cleanup and may schedule disconnect after partial success.
    pub async fn accept(&self, config: &L2capChannelConfig) -> Result<L2capChannel<'d, P>, BleHostError<T::Error>> {
        let pending = self.next().await?;
        let index = pending.into_index();
        self.host.channels().accept_pending(index, config, self.host).await
    }

    /// Returns the retained connection owner by reference; borrowing does not add another count.
    pub fn connection(&self) -> &Connection<'d, P> {
        &self.conn
    }
}

impl<T: Controller, P: PacketPool> Drop for L2capChannelListener<'_, T, P> {
    /// Clears the retained slot's listening flag; only while that exact link is Connected, best-effort rejects unclaimed requests. A closed old listener never acts on another slot reusing its handle.
    fn drop(&mut self) {
        self.conn.set_l2cap_listening(false);
        if self.conn.is_connected() {
            self.host
                .channels()
                .reject_all_pending(self.conn.handle(), self.host.connections());
        }
    }
}
