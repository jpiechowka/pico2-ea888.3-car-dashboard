//! Single-executor LE credit-channel admission, segmentation, queues and deferred disconnection.
//!
//! Local CIDs map directly to retained slots starting at 0x40. Refcounts prevent reuse while
//! handles or handshakes own a slot; RefCell/NoopRawMutex do not provide cross-thread exclusion.
//! Credits count K-frames, not SDUs or controller ACL fragments. See
//! [Bluetooth L2CAP](https://www.bluetooth.com/wp-content/uploads/Files/Specification/HTML/Core-60/out/en/host/logical-link-control-and-adaptation-protocol-specification.html).

use core::cell::{Ref, RefCell, RefMut};
use core::future::poll_fn;
use core::task::{Context, Poll};

use bt_hci::controller::{blocking, Controller};
use bt_hci::param::{ConnHandle, LeConnRole};
use bt_hci::{FromHciBytes, WriteHci};
use embassy_sync::blocking_mutex::raw::NoopRawMutex;
use embassy_sync::channel::Channel;
use embassy_sync::waitqueue::WakerRegistration;
use embassy_time::with_timeout;

use crate::connection_manager::ConnectionManager;
use crate::cursor::WriteCursor;
use crate::host::{BleHost, OnDrop};
#[cfg(not(feature = "l2cap-sdu-reassembly-optimization"))]
use crate::l2cap::sar::PacketReassembly;
use crate::l2cap::{L2capChannel, L2capPendingConnection};
use crate::pdu::{Pdu, Sdu};
use crate::prelude::{ConnectionEvent, ConnectionParamsRequest, L2capChannelConfig};
use crate::types::l2cap::{
    CommandRejectRes, ConnParamUpdateReq, ConnParamUpdateRes, DisconnectionReq, DisconnectionRes, L2capHeader,
    L2capSignal, L2capSignalCode, L2capSignalHeader, LeCreditConnReq, LeCreditConnRes, LeCreditConnResultCode,
    LeCreditFlowInd, L2CAP_CID_LE_U_SIGNAL,
};
use crate::{config, BleHostError, Error, PacketPool};

/// First dynamic local CID; slot index is CID minus 0x40.
const BASE_ID: u16 = 0x40;

#[cfg(all(test, feature = "default-packet-pool"))]
#[path = "channel_regression_tests.rs"]
/// Actual manager/stack regressions for handshake lifetime, credits, malformed frames and identity.
mod regression_tests;

/// BT Core Spec Vol 3, Part A, Section 6.2.1: RTX signaling timeout.
const L2CAP_RTX_TIMEOUT: embassy_time::Duration = embassy_time::Duration::from_secs(30);

/// Shared request-identifier sequence, single waiter registrations and permanently registered service bitmap.
struct State {
    /// Next signaling identifier; zero is skipped and 255 wraps to 1.
    next_req_id: u8,
    /// Single waiter for incoming pending channels or link loss.
    accept_waker: WakerRegistration,
    /// Single waiter for outgoing handshake completion or link loss.
    create_waker: WakerRegistration,
    /// Host runner waiter for deferred channel close signaling.
    disconnect_waker: WakerRegistration,
    /// Bitmask tracking which SPSMs (0x0001..=0x00FF) have active listeners.
    registered_spsms: [u32; 8],
}

/// Borrowed dynamic-channel table and signaling state for one host executor; references prevent slot reuse, not concurrent access.
pub struct ChannelManager<'d, P: PacketPool> {
    /// Identifier/listener/waker state; runtime borrow checked within one executor.
    state: RefCell<State>,
    /// Host-owned fixed table; handles borrow this manager for its full lifetime.
    channels: &'d RefCell<[ChannelStorage<P::Packet>]>,
}

/// Bounded inbound SDU queue; None is a best-effort close marker. P owns packet storage; QLEN counts queue entries.
pub(crate) struct PacketChannel<P, const QLEN: usize> {
    /// Single-executor bounded packet queue; None marks closure when capacity permits.
    chan: Channel<NoopRawMutex, Option<Pdu<P>>, QLEN>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
/// Manager-local slot identity; never valid for a different ChannelManager.
pub struct ChannelIndex(
    /// Zero-based slot, derived from a validated local CID or table allocation.
    u8,
);

impl<P, const QLEN: usize> PacketChannel<P, QLEN> {
    /// Returns an empty queue with no close marker; capacity is QLEN.
    pub(crate) const fn new() -> Self {
        Self { chan: Channel::new() }
    }

    /// Returns Ok after enqueueing a close marker, or Err(()) if full; existing packets remain queued and failure does not wake a receiver.
    pub fn close(&self) -> Result<(), ()> {
        self.chan.try_send(None).map_err(|_| ())
    }

    /// Queues owned `pdu`, waiting for capacity. Cancellation before enqueue drops the packet; a completed send transfers ownership to the receiver.
    pub async fn send(&self, pdu: Pdu<P>) {
        self.chan.send(Some(pdu)).await;
    }

    /// Returns Ok after queueing owned `pdu`, or OutOfMemory if full; rejected packet storage is dropped.
    pub fn try_send(&self, pdu: Pdu<P>) -> Result<(), Error> {
        self.chan.try_send(Some(pdu)).map_err(|_| Error::OutOfMemory)
    }

    /// Returns the next packet or None close marker, consuming it. Empty queues register `cx` for one receiver and return Pending.
    pub fn poll_receive(&self, cx: &mut Context<'_>) -> Poll<Option<Pdu<P>>> {
        self.chan.poll_receive(cx)
    }

    /// Drops all queued packets/markers and wakes a blocked sender when the queue was full; does not enqueue a close marker.
    pub fn clear(&self) {
        self.chan.clear()
    }
}

impl State {
    /// Permanently registers `spsm` in 1..=255; values outside that bitmap are ignored.
    fn register_spsm(&mut self, spsm: u16) {
        if (1..=255).contains(&spsm) {
            let idx = spsm as usize / 32;
            let bit = spsm as usize % 32;
            self.registered_spsms[idx] |= 1 << bit;
        }
    }

    /// Returns bitmap membership for `spsm` in 1..=255; reserved values return the allow-reserved-l2cap-psu feature setting.
    fn is_spsm_registered(&self, spsm: u16) -> bool {
        if !(1..=255).contains(&spsm) {
            // When the allow-reserved-l2ca-psu feature is enable, treat all reserved PSUs as registered
            return cfg!(feature = "allow-reserved-l2cap-psu");
        }
        let idx = spsm as usize / 32;
        let bit = spsm as usize % 32;
        self.registered_spsms[idx] & (1 << bit) != 0
    }

    /// Returns the next nonzero signaling identifier, wrapping 255 to 1; does not reserve identifiers against outstanding requests.
    fn next_request_id(&mut self) -> u8 {
        // 0 is an invalid identifier
        if self.next_req_id == 0 {
            self.next_req_id += 1;
        }
        let next = self.next_req_id;
        self.next_req_id = self.next_req_id.wrapping_add(1);
        next
    }
}

impl<P: PacketPool> Drop for ChannelManager<'_, P> {
    /// Resets borrowed slots, releasing queued/partial SDU allocations before backing resources can be reused. Borrowed channel owners cannot outlive this manager; no peer disconnect is sent.
    fn drop(&mut self) {
        for storage in self.channels.borrow_mut().iter_mut() {
            *storage = ChannelStorage::new();
        }
    }
}

impl<'d, P: PacketPool> ChannelManager<'d, P> {
    /// Returns a manager borrowing `channels` without resetting their slots; caller supplies initialized storage owned by this host. Signaling sequence/listeners start empty.
    pub fn new(channels: &'d RefCell<[ChannelStorage<P::Packet>]>) -> Self {
        Self {
            state: RefCell::new(State {
                next_req_id: 0,
                accept_waker: WakerRegistration::new(),
                create_waker: WakerRegistration::new(),
                disconnect_waker: WakerRegistration::new(),
                registered_spsms: [0; 8],
            }),
            channels,
        }
    }

    /// Returns a shared borrow of local `index`; panics for an invalid index or conflicting mutable table borrow.
    fn channel(&self, index: ChannelIndex) -> Ref<'_, ChannelStorage<P::Packet>> {
        Ref::map(self.channels.borrow(), |x| &x[index.0 as usize])
    }

    /// Returns an exclusive borrow of local `index`; panics for an invalid index or any conflicting table borrow.
    fn channel_mut(&self, index: ChannelIndex) -> RefMut<'_, ChannelStorage<P::Packet>> {
        RefMut::map(self.channels.borrow_mut(), |x| &mut x[index.0 as usize])
    }

    /// Permanently registers service `spsm` in 1..=255; reserved services depend on the feature policy.
    pub(crate) fn register_spsm(&self, spsm: u16) {
        self.state.borrow_mut().register_spsm(spsm);
    }

    /// Returns the next nonzero, wrapping signaling identifier under the shared state borrow.
    fn next_request_id(&self) -> u8 {
        self.state.borrow_mut().next_request_id()
    }

    /// Returns the registered service multiplexer stored at local `index`; zero may describe an unconfigured/closed slot. Invalid indices or borrow conflicts panic.
    pub(crate) fn spsm(&self, index: ChannelIndex) -> u16 {
        self.channel(index).spsm
    }

    /// Returns the local accepted SDU byte ceiling stored at local `index`; zero may describe an unconfigured/closed slot. Invalid indices or borrow conflicts panic.
    pub(crate) fn mtu(&self, index: ChannelIndex) -> u16 {
        self.channel(index).mtu
    }

    /// Returns the local accepted K-frame payload byte ceiling stored at local `index`; zero may describe an unconfigured/closed slot. Invalid indices or borrow conflicts panic.
    pub(crate) fn mps(&self, index: ChannelIndex) -> u16 {
        self.channel(index).mps
    }

    /// Returns the peer accepted SDU byte ceiling stored at local `index`; zero may describe an unconfigured/closed slot. Invalid indices or borrow conflicts panic.
    pub(crate) fn peer_mtu(&self, index: ChannelIndex) -> u16 {
        self.channel(index).peer_mtu
    }

    /// Returns the peer accepted K-frame payload byte ceiling stored at local `index`; zero may describe an unconfigured/closed slot. Invalid indices or borrow conflicts panic.
    pub(crate) fn peer_mps(&self, index: ChannelIndex) -> u16 {
        self.channel(index).peer_mps
    }

    /// Requests deferred disconnection of connected local `index`, queues a best-effort receive-close marker and wakes the host runner. Other states are unchanged; peer acknowledgment is not awaited.
    pub(crate) fn disconnect(&self, index: ChannelIndex) {
        self.channel_mut(index).disconnect();
        self.state.borrow_mut().disconnect_waker.wake();
    }

    /// Requests deferred close only when local `channel` CID is in range and belongs to `conn`; unknown/mismatched channels are ignored.
    pub(crate) fn disconnect_by_cid(&self, conn: ConnHandle, channel: u16) {
        if channel >= BASE_ID && usize::from(channel - BASE_ID) < self.channels.as_ptr().len() {
            let index = ChannelIndex((channel - BASE_ID) as u8);
            let mut storage = self.channel_mut(index);
            if storage.conn != Some(conn) {
                return;
            }
            storage.disconnect();
            self.state.borrow_mut().disconnect_waker.wake();
        }
    }

    /// Checks `actual` bytes against the value selected by `limit` for connected local `channel` on `conn`; `label` names the diagnostic length. Returns InvalidChannelId for wrong ownership/state, or InvalidValue after requesting/waking disconnect on excess.
    fn check_len(
        &self,
        conn: ConnHandle,
        channel: u16,
        actual: u16,
        limit: impl FnOnce(&ChannelStorage<P::Packet>) -> u16,
        label: &str,
    ) -> Result<(), Error> {
        if channel < BASE_ID || usize::from(channel - BASE_ID) >= self.channels.as_ptr().len() {
            return Err(Error::InvalidChannelId);
        }
        let index = ChannelIndex((channel - BASE_ID) as u8);
        let mut storage = self.channel_mut(index);
        if storage.conn != Some(conn) || storage.state != ChannelState::Connected {
            return Err(Error::InvalidChannelId);
        }
        let max = limit(&storage);
        storage
            .check_len(actual, max, label)
            .inspect_err(|_| self.state.borrow_mut().disconnect_waker.wake())
    }

    /// Returns admission of `sdu_len` bytes against local SDU MTU for `channel` on `conn`; invalid identity/state fails and excess requests disconnection.
    pub(crate) fn check_sdu_len(&self, conn: ConnHandle, channel: u16, sdu_len: u16) -> Result<(), Error> {
        self.check_len(conn, channel, sdu_len, |s| s.mtu, "SDU")
    }

    /// Returns admission of `pdu_len` bytes against local K-frame MPS for `channel` on `conn`; invalid identity/state fails and excess requests disconnection.
    pub(crate) fn check_pdu_len(&self, conn: ConnHandle, channel: u16, pdu_len: u16) -> Result<(), Error> {
        self.check_len(conn, channel, pdu_len, |s| s.mps, "PDU")
    }

    /// Closes all slots owned by lost link `conn`, clears partial reassembly/metrics and wakes handshake waiters; existing reference counts remain. Returns Ok; queued data is retained until reuse, with a best-effort close marker.
    pub(crate) fn disconnected(&self, conn: ConnHandle) -> Result<(), Error> {
        for storage in self.channels.borrow_mut().iter_mut() {
            if Some(conn) == storage.conn {
                let _ = storage.inbound.close();
                #[cfg(not(feature = "l2cap-sdu-reassembly-optimization"))]
                storage.reassembly.clear();
                #[cfg(feature = "channel-metrics")]
                storage.metrics.reset();
                storage.close();
            }
        }
        let mut state = self.state.borrow_mut();
        state.accept_waker.wake();
        state.create_waker.wake();
        Ok(())
    }

    /// Returns a free zero-reference slot for `conn`, rejecting duplicate active `peer_cid` and table exhaustion. Clears old inbound/reassembly storage, assigns local CID, then calls `f` to initialize state. A panicking `f` can leave partial slot changes; table indices must fit the configured u8 identity domain.
    fn alloc<F: FnOnce(&mut ChannelStorage<P::Packet>)>(
        &self,
        conn: ConnHandle,
        peer_cid: Option<u16>,
        f: F,
    ) -> Result<ChannelIndex, Error> {
        // Check that the peer CID isn't already in use on this connection.
        let mut channels = self.channels.borrow_mut();
        if let Some(peer_cid) = peer_cid {
            let in_use = channels.iter().any(|s| {
                s.conn == Some(conn) && s.peer_cid == peer_cid && !matches!(s.state, ChannelState::Disconnected)
            });
            if in_use {
                return Err(Error::L2capConnectError(LeCreditConnResultCode::ScidAlreadyAllocated));
            }
        }

        let idx = channels
            .iter()
            .position(|s| s.state == ChannelState::Disconnected && s.refcount == 0)
            .ok_or(Error::NoChannelAvailable)?;

        let storage = &mut channels[idx];
        storage.inbound.clear();
        #[cfg(not(feature = "l2cap-sdu-reassembly-optimization"))]
        storage.reassembly.clear();
        storage.conn = Some(conn);
        storage.cid = BASE_ID + idx as u16;
        storage.peer_cid = peer_cid.unwrap_or(0);
        f(storage);

        Ok(ChannelIndex(idx as u8))
    }

    /// Returns a retained pending channel for `conn`, waiting while `connections` reports the link alive; returns Disconnected after link loss. Uses one accept waker and skips requests already owned by pending tokens. Pending cancellation owns no slot.
    pub(crate) async fn next_pending(
        &'d self,
        conn: ConnHandle,
        connections: &ConnectionManager<'_, P>,
    ) -> Result<L2capPendingConnection<'d, P>, Error> {
        poll_fn(|cx| {
            let mut state = self.state.borrow_mut();
            state.accept_waker.register(cx.waker());
            core::mem::drop(state);

            let mut channels = self.channels.borrow_mut();
            for (idx, chan) in channels.iter_mut().enumerate() {
                match chan.state {
                    ChannelState::PeerConnecting(_) if chan.conn == Some(conn) && chan.refcount == 0 => {
                        chan.inc_ref();
                        let index = ChannelIndex(idx as u8);
                        return Poll::Ready(Ok(L2capPendingConnection::new(index, self, conn)));
                    }
                    _ => {}
                }
            }
            if !connections.is_handle_connected(conn) {
                return Poll::Ready(Err(Error::Disconnected));
            }
            Poll::Pending
        })
        .await
    }

    /// Best-effort rejects every unclaimed peer request on `conn` using `manager` outbound capacity, then closes each slot even when queueing fails. Already retained pending tokens remain owned by their callers.
    pub(crate) fn reject_all_pending(&self, conn: ConnHandle, manager: &ConnectionManager<'_, P>) {
        let mut channels = self.channels.borrow_mut();
        for chan in channels.iter_mut() {
            if chan.conn == Some(conn) && chan.refcount == 0 {
                if let ChannelState::PeerConnecting(identifier) = chan.state {
                    if let Err(e) = Self::try_send_signal(
                        conn,
                        identifier,
                        &LeCreditConnRes::reject(LeCreditConnResultCode::NoResources),
                        manager,
                    ) {
                        warn!(
                            "[l2cap] error rejecting pending channel connect request {:?}: {:?}",
                            identifier, e
                        );
                    }
                    chan.close();
                }
            }
        }
    }

    /// Accepts retained local `index` using `config` limits/credits and originating `ble` transport. Returns its channel after a success response is submitted; rejects bad MTU/MPS or packet capacity. Marks Connected before awaiting I/O; error/cancellation releases the pending reference and requests disconnect if the handshake may have escaped.
    pub(crate) async fn accept_pending<T: Controller>(
        &'d self,
        index: ChannelIndex,
        config: &L2capChannelConfig,
        ble: BleHost<'d, T, P>,
    ) -> Result<L2capChannel<'d, P>, BleHostError<T::Error>> {
        // The pending handle transferred its reference to this operation.
        // Keep it owned across validation and the first awaited signal send.
        let ondrop = OnDrop::new(|| self.drop_pending(index));
        let L2capChannelConfig {
            mtu,
            mps,
            flow_policy,
            initial_credits,
        } = config;

        let mtu = mtu.unwrap_or(P::MTU as u16 - 6);
        let mps = mps.unwrap_or(P::MTU as u16 - 4);
        if mps > P::MTU as u16 - 4 {
            return Err(Error::InsufficientSpace.into());
        }
        if !valid_credit_parameters(mtu, mps) {
            return Err(Error::InvalidValue.into());
        }

        let (conn, req_id, mps, mtu, cid, credits) = {
            let mut chan = self.channel_mut(index);
            let req_id = match chan.state {
                ChannelState::PeerConnecting(req_id) => req_id,
                _ => return Err(Error::NotFound.into()),
            };
            chan.mtu = mtu;
            chan.mps = mps;
            chan.flow_control = CreditFlowControl::new(
                *flow_policy,
                initial_credits.unwrap_or(config::L2CAP_RX_QUEUE_SIZE.min(P::capacity()) as u16),
            );
            chan.state = ChannelState::Connected;
            let conn = chan.conn.unwrap();
            (
                conn,
                req_id,
                chan.mps,
                chan.mtu,
                chan.cid,
                chan.flow_control.available(),
            )
        };

        let mut tx = [0; 18];
        ble.l2cap_signal(
            conn,
            req_id,
            &LeCreditConnRes {
                mps,
                dcid: cid,
                mtu,
                credits,
                result: LeCreditConnResultCode::Success,
            },
            &mut tx[..],
        )
        .await?;
        ondrop.defuse();
        Ok(L2capChannel::new(index, self))
    }

    /// Rejects retained local `index` with wire `result` through originating `ble`; returns signal submission success/error. Closes locally before I/O, and releases its reference on all exits including cancellation; peer receipt is not guaranteed.
    pub(crate) async fn reject_pending<T: Controller>(
        &self,
        index: ChannelIndex,
        result: LeCreditConnResultCode,
        ble: BleHost<'_, T, P>,
    ) -> Result<(), BleHostError<T::Error>> {
        let _ondrop = OnDrop::new(|| self.drop_pending(index));
        let (conn, req_id) = {
            let mut chan = self.channel_mut(index);
            let req_id = match chan.state {
                ChannelState::PeerConnecting(req_id) => req_id,
                _ => return Err(Error::NotFound.into()),
            };
            let conn = chan.conn.unwrap();
            chan.close();
            (conn, req_id)
        };

        let mut tx = [0; 18];
        ble.l2cap_signal(conn, req_id, &LeCreditConnRes::reject(result), &mut tx[..])
            .await?;
        Ok(())
    }

    /// Releases one handshake-owned reference at `index`; closes unfinished handshakes or requests disconnect when an established channel loses its last owner. Panics on reference underflow.
    pub(crate) fn drop_pending(&self, index: ChannelIndex) {
        let mut chan = self.channel_mut(index);
        chan.refcount = unwrap!(chan.refcount.checked_sub(1), "bug: dropping a channel with refcount 0");
        if matches!(
            chan.state,
            ChannelState::PeerConnecting(_) | ChannelState::Connecting(_) | ChannelState::ConnectFailed(_)
        ) {
            chan.close();
        } else if chan.refcount == 0 {
            // A successful handshake may have reached the peer before cancellation.
            chan.disconnect();
            self.state.borrow_mut().disconnect_waker.wake();
        }
    }

    /// Creates service `spsm` on `conn` using local `config` and originating `ble`. Returns a retained channel or allocation/config/controller/peer error. Only the response wait has a 30-second timeout; initial request I/O may wait longer. Error/cancellation releases its slot reference, closing incomplete handshakes or requesting established-channel disconnect.
    pub(crate) async fn create<T: Controller>(
        &'d self,
        conn: ConnHandle,
        spsm: u16,
        config: &L2capChannelConfig,
        ble: BleHost<'_, T, P>,
    ) -> Result<L2capChannel<'d, P>, BleHostError<T::Error>> {
        let L2capChannelConfig {
            mtu,
            mps,
            flow_policy,
            initial_credits,
        } = config;

        let req_id = self.next_request_id();
        let mut credits = 0;
        let mut cid: u16 = 0;

        let mtu = mtu.unwrap_or(P::MTU as u16 - 6);
        let mps = mps.unwrap_or(P::MTU as u16 - 4);
        if mps > P::MTU as u16 - 4 {
            return Err(Error::InsufficientSpace.into());
        }
        if !valid_credit_parameters(mtu, mps) {
            return Err(Error::InvalidValue.into());
        }

        // Allocate space for our new channel.
        let idx = self.alloc(conn, None, |storage| {
            cid = storage.cid;
            credits = initial_credits.unwrap_or(config::L2CAP_RX_QUEUE_SIZE.min(P::capacity()) as u16);
            storage.spsm = spsm;
            storage.mtu = mtu;
            storage.mps = mps;
            storage.flow_control = CreditFlowControl::new(*flow_policy, credits);
            storage.state = ChannelState::Connecting(req_id);
            // Retain the slot even if a response or link loss closes it before
            // this future resumes. Its cleanup must never touch a reused slot.
            storage.inc_ref();
        })?;
        // Install before the first await: controller backpressure and startup
        // can suspend the initial request just as they suspend its response.
        let ondrop = OnDrop::new(|| self.drop_pending(idx));

        let mut tx = [0; 18];
        // Send the initial connect request.
        let command = LeCreditConnReq {
            spsm,
            mps,
            scid: cid,
            mtu,
            credits,
        };
        ble.l2cap_signal(conn, req_id, &command, &mut tx[..]).await?;

        // Wait until a response is accepted.
        let result = with_timeout(
            L2CAP_RTX_TIMEOUT,
            poll_fn(|cx| self.poll_created(conn, idx, ble, Some(cx))),
        )
        .await
        .unwrap_or_else(|_| {
            warn!("[l2cap] RTX timeout waiting for LE Credit Based Connection Response");
            Err(BleHostError::BleHost(Error::Timeout))
        })?;
        ondrop.defuse();
        Ok(result)
    }

    /// Polls outgoing slot `idx` on `conn` against originating `ble`; optional `cx` registers the single create waiter. Returns the retained channel on success, link/peer error on failure, otherwise Pending. Success requires exactly one handshake reference or panics; caller transfers/releases that reference.
    fn poll_created<T: Controller>(
        &'d self,
        conn: ConnHandle,
        idx: ChannelIndex,
        ble: BleHost<'_, T, P>,
        cx: Option<&mut Context<'_>>,
    ) -> Poll<Result<L2capChannel<'d, P>, BleHostError<T::Error>>> {
        if let Some(cx) = cx {
            self.state.borrow_mut().create_waker.register(cx.waker());
        }
        let mut storage = self.channel_mut(idx);
        // Check if we've been disconnected while waiting
        if !ble.connections().is_handle_connected(conn) || storage.conn != Some(conn) {
            return Poll::Ready(Err(Error::Disconnected.into()));
        }

        match storage.state {
            ChannelState::ConnectFailed(result) => {
                storage.state = ChannelState::Disconnected;
                return Poll::Ready(Err(Error::L2capConnectError(result).into()));
            }
            ChannelState::Disconnected | ChannelState::Disconnecting | ChannelState::PeerDisconnecting(_) => {
                return Poll::Ready(Err(Error::Disconnected.into()));
            }
            ChannelState::Connected => {
                if storage.refcount != 1 {
                    core::mem::drop(storage);
                    self.log_status(true);
                    panic!("unexpected refcount");
                }
                // Transfer the create future's reference to the returned channel.
                return Poll::Ready(Ok(L2capChannel::new(idx, self)));
            }
            _ => {}
        }
        Poll::Pending
    }

    /// Consumes one incoming K-frame credit for connected local `channel` on `conn`; `credits` is currently ignored. Returns OutOfMemory when no local credit remains, NotFound for wrong state/link, or InvalidChannelId outside the table. Used when host-level reassembly owns packet dispatch.
    pub(crate) fn received(&self, conn: ConnHandle, channel: u16, credits: u16) -> Result<(), Error> {
        if channel < BASE_ID || usize::from(channel - BASE_ID) >= self.channels.as_ptr().len() {
            return Err(Error::InvalidChannelId);
        }

        let index = ChannelIndex((channel - BASE_ID) as u8);
        let mut storage = self.channel_mut(index);
        match storage.state {
            ChannelState::Connected if channel == storage.cid && storage.conn == Some(conn) => {
                if storage.flow_control.available() == 0 {
                    #[cfg(feature = "channel-metrics")]
                    storage.metrics.blocked_receive();
                    // NOTE: This will trigger closing of the link, which might be a bit
                    // too strict. But it should be controllable via the credits given,
                    // which the remote should respect.
                    debug!("[l2cap][cid = {}] no credits available", channel);
                    return Err(Error::OutOfMemory);
                }
                storage.flow_control.confirm_received(1);
                #[cfg(feature = "channel-metrics")]
                storage.metrics.received(1);
                return Ok(());
            }
            _ => {}
        }
        Err(Error::NotFound)
    }

    /// Consumes owned `pdu` for local `channel` on `conn`, returning queue/reassembly/identity errors. Normal mode consumes one credit and assembles K-frames; optimized mode receives an already assembled SDU. Malformed lengths request disconnect; full inbound queue drops the completed SDU. Packet ownership is retained only by assembly or a successful queue insertion.
    pub(crate) fn dispatch(&self, conn: ConnHandle, channel: u16, pdu: Pdu<P::Packet>) -> Result<(), Error> {
        if channel < BASE_ID || usize::from(channel - BASE_ID) >= self.channels.as_ptr().len() {
            return Err(Error::InvalidChannelId);
        }

        let index = ChannelIndex((channel - BASE_ID) as u8);
        #[allow(unused_mut)]
        let mut storage = self.channel_mut(index);

        let mut sdu = None;
        match storage.state {
            ChannelState::Connected if channel == storage.cid && storage.conn == Some(conn) => {
                // Reassembly and accounting is already done
                #[cfg(feature = "l2cap-sdu-reassembly-optimization")]
                sdu.replace(pdu);

                // Reassembly is done in the channel
                #[cfg(not(feature = "l2cap-sdu-reassembly-optimization"))]
                {
                    if storage.flow_control.available() == 0 {
                        #[cfg(feature = "channel-metrics")]
                        storage.metrics.blocked_receive();
                        // NOTE: This will trigger closing of the link, which might be a bit
                        // too strict. But it should be controllable via the credits given,
                        // which the remote should respect.
                        debug!("[l2cap][cid = {}] no credits available", channel);
                        return Err(Error::OutOfMemory);
                    }

                    storage.flow_control.confirm_received(1);

                    #[cfg(feature = "channel-metrics")]
                    storage.metrics.received(1);
                    if !storage.reassembly.in_progress() {
                        let Some(first) = pdu.as_ref().get(..2) else {
                            storage.disconnect();
                            self.state.borrow_mut().disconnect_waker.wake();
                            return Err(Error::InvalidValue);
                        };
                        let sdu_len: u16 = u16::from_le_bytes([first[0], first[1]]);

                        storage
                            .check_sdu_len(sdu_len)
                            .inspect_err(|_| self.state.borrow_mut().disconnect_waker.wake())?;

                        let len = pdu.len() - 2;

                        let mut packet = pdu.into_inner();
                        packet.as_mut().rotate_left(2);

                        // A complete fragment
                        if sdu_len as usize == len {
                            sdu.replace(Pdu::new(packet, sdu_len as usize));
                        } else {
                            // Need another fragment
                            if let Err(error) = storage.reassembly.init_with_written(channel, sdu_len, packet, len) {
                                storage.disconnect();
                                self.state.borrow_mut().disconnect_waker.wake();
                                return Err(error);
                            }
                        }
                    } else {
                        match storage.reassembly.update(pdu.as_ref()) {
                            Ok(Some((_, pdu))) => {
                                sdu.replace(pdu);
                            }
                            Ok(None) => {}
                            Err(e) => {
                                storage.disconnect();
                                self.state.borrow_mut().disconnect_waker.wake();
                                return Err(e);
                            }
                        }
                    }
                }
            }
            _ => return Err(Error::InvalidChannelId),
        }

        if let Some(sdu) = sdu {
            storage.inbound.try_send(sdu)?;
        }

        Ok(())
    }

    /// Processes concatenated signaling `data` from `conn` using `manager` link state/outbound queue. Returns framing/rejection-queue errors; most per-command errors are logged and processing continues. Header extents are checked before mutation; malformed payloads can follow earlier successful commands. Identifier zero is skipped.
    pub(crate) fn signal(
        &self,
        conn: ConnHandle,
        data: &[u8],
        manager: &ConnectionManager<'_, P>,
    ) -> Result<(), Error> {
        // Validate that all signal headers fit within the PDU before processing.
        {
            let mut remaining = data;
            while !remaining.is_empty() {
                let (header, rest) = L2capSignalHeader::from_hci_bytes(remaining)?;
                if header.length as usize > rest.len() {
                    if header.identifier != 0 {
                        Self::try_send_signal(conn, header.identifier, &CommandRejectRes { reason: 0 }, manager)?;
                    }
                    return Err(Error::InvalidValue);
                }
                remaining = &rest[header.length as usize..];
            }
        }

        let mut data = data;
        while !data.is_empty() {
            let (header, remaining) = L2capSignalHeader::from_hci_bytes(data)?;
            let (signal_data, next) = remaining.split_at(header.length as usize);
            if header.identifier == 0 {
                data = next;
                continue;
            }
            let result = (|| match header.code {
                L2capSignalCode::LE_CREDIT_CONN_REQ => {
                    let req = LeCreditConnReq::from_hci_bytes_complete(signal_data)?;
                    self.handle_connect_request(conn, header.identifier, &req, manager)
                }
                L2capSignalCode::LE_CREDIT_CONN_RES => {
                    let res = LeCreditConnRes::from_hci_bytes_complete(signal_data)?;
                    self.handle_connect_response(conn, header.identifier, &res)
                }
                L2capSignalCode::LE_CREDIT_FLOW_IND => {
                    let req = LeCreditFlowInd::from_hci_bytes_complete(signal_data)?;
                    //trace!("[l2cap] credit flow: {:?}", req);
                    self.handle_credit_flow(conn, &req)
                }
                L2capSignalCode::COMMAND_REJECT_RES => {
                    let (_reject, _) = CommandRejectRes::from_hci_bytes(signal_data)?;
                    Ok(())
                }
                L2capSignalCode::DISCONNECTION_REQ => {
                    let req = DisconnectionReq::from_hci_bytes_complete(signal_data)?;
                    debug!("[l2cap][conn = {:?}, cid = {}] disconnect request", conn, req.dcid);
                    self.handle_disconnect_request(conn, header.identifier, req.dcid, req.scid, manager)
                }
                L2capSignalCode::DISCONNECTION_RES => {
                    let res = DisconnectionRes::from_hci_bytes_complete(signal_data)?;
                    debug!("[l2cap][conn = {:?}, cid = {}] disconnect response", conn, res.scid);
                    self.handle_disconnect_response(conn, res.scid, res.dcid)
                }
                L2capSignalCode::CONN_PARAM_UPDATE_REQ => {
                    if manager.role_by_handle(conn) != Some(LeConnRole::Central) {
                        warn!(
                            "[l2cap][conn = {:?}] rejecting connection param update request: not Central",
                            conn
                        );
                        Self::try_send_signal(conn, header.identifier, &CommandRejectRes { reason: 0 }, manager)?;
                        return Ok(());
                    }

                    let req = ConnParamUpdateReq::from_hci_bytes_complete(signal_data)?;
                    debug!("[l2cap][conn = {:?}] connection param update request: {:?}", conn, req);
                    let interval_min: bt_hci::param::Duration<1_250> =
                        bt_hci::param::Duration::from_u16(req.interval_min);
                    let interval_max: bt_hci::param::Duration<1_250> =
                        bt_hci::param::Duration::from_u16(req.interval_max);
                    let timeout: bt_hci::param::Duration<10_000> = bt_hci::param::Duration::from_u16(req.timeout);

                    use embassy_time::Duration;
                    let params = crate::prelude::RequestedConnParams {
                        min_connection_interval: Duration::from_micros(interval_min.as_micros()),
                        max_connection_interval: Duration::from_micros(interval_max.as_micros()),
                        max_latency: req.latency,
                        supervision_timeout: Duration::from_micros(timeout.as_micros()),
                        ..Default::default()
                    };

                    if !params.is_valid() {
                        warn!(
                            "[l2cap][conn = {:?}] rejecting connection param update request with invalid parameters",
                            conn
                        );
                        Self::try_send_signal(conn, header.identifier, &ConnParamUpdateRes { result: 1 }, manager)?;
                    } else {
                        let req = ConnectionParamsRequest::new(params, conn, header.identifier);

                        if manager
                            .post_handle_event(conn, ConnectionEvent::RequestConnectionParams(req))
                            .is_err()
                        {
                            Self::try_send_signal(conn, header.identifier, &ConnParamUpdateRes { result: 1 }, manager)?;
                        }
                    }
                    Ok(())
                }
                L2capSignalCode::CONN_PARAM_UPDATE_RES => {
                    let res = ConnParamUpdateRes::from_hci_bytes_complete(signal_data)?;
                    debug!(
                        "[l2cap][conn = {:?}] connection param update response: {}",
                        conn, res.result,
                    );
                    Ok(())
                }
                _ => {
                    warn!("[l2cap][conn = {:?}] unsupported signal: {:?}", conn, header.code);
                    Self::try_send_signal(conn, header.identifier, &CommandRejectRes { reason: 0 }, manager)?;
                    Err(Error::NotSupported)
                }
            })();
            if let Err(e) = result {
                warn!("[l2cap][conn = {:?}] error processing signal: {:?}", conn, e);
                if matches!(e, Error::HciDecode(_)) {
                    Self::try_send_signal(conn, header.identifier, &CommandRejectRes { reason: 0 }, manager)?;
                }
            }
            data = next;
        }
        Ok(())
    }

    /// Admits peer `req` on `conn` with response `identifier`, using `manager` listener state and rejection queue. Returns Ok for queued acceptance-to-user or rejection; invalid CID/parameters/service, absent listener and capacity failures become wire rejection codes. Outbound allocation failures propagate.
    fn handle_connect_request(
        &self,
        conn: ConnHandle,
        identifier: u8,
        req: &LeCreditConnReq,
        manager: &ConnectionManager<'_, P>,
    ) -> Result<(), Error> {
        let result = {
            let mut state = self.state.borrow_mut();
            if !(0x40..=0x7f).contains(&req.scid) {
                Err(LeCreditConnResultCode::InvalidSourceId)
            } else if !valid_credit_parameters(req.mtu, req.mps) {
                Err(LeCreditConnResultCode::UnacceptableParameters)
            } else if !state.is_spsm_registered(req.spsm) {
                Err(LeCreditConnResultCode::SpsmNotSupported)
            } else if !manager.is_l2cap_listening(conn) {
                Err(LeCreditConnResultCode::NoResources)
            } else {
                match self.alloc(conn, Some(req.scid), |storage| {
                    storage.conn = Some(conn);
                    storage.spsm = req.spsm;
                    storage.peer_credits = req.credits;
                    storage.peer_mps = req.mps;
                    storage.peer_mtu = req.mtu;
                    storage.state = ChannelState::PeerConnecting(identifier);
                }) {
                    Ok(_) => {
                        state.accept_waker.wake();
                        Ok(())
                    }
                    Err(Error::L2capConnectError(result)) => Err(result),
                    Err(Error::NoChannelAvailable) => Err(LeCreditConnResultCode::NoResources),
                    Err(e) => return Err(e),
                }
            }
        };

        match result {
            Ok(()) => Ok(()),
            Err(result) => {
                debug!(
                    "[l2cap][conn = {:?}] rejecting connection for SPSM 0x{:04x}: {:?}",
                    conn, req.spsm, result
                );
                Self::try_send_signal(conn, identifier, &LeCreditConnRes::reject(result), manager)
            }
        }
    }

    /// Matches peer `res` by `conn` and `identifier`; returns success after publishing Connected, or peer/validation errors. Unknown identifiers close all Connecting slots on that link; successful replies require legal unique peer CID and MTU/MPS. Wakes the create waiter on every matched/fallback completion.
    fn handle_connect_response(&self, conn: ConnHandle, identifier: u8, res: &LeCreditConnRes) -> Result<(), Error> {
        let mut channels = self.channels.borrow_mut();
        let duplicate_cid = channels
            .iter()
            .any(|s| s.conn == Some(conn) && s.peer_cid == res.dcid && s.state != ChannelState::Disconnected);

        // Find the channel matching this response identifier
        let matched = channels.iter_mut().find(|storage| {
            matches!(storage.state, ChannelState::Connecting(req_id) if identifier == req_id && Some(conn) == storage.conn)
        });

        let Some(storage) = matched else {
            // No channel matched this identifier. Clean up any Connecting channels
            // on this connection — the peer has sent a response (with a wrong identifier),
            // so the connection attempt has effectively failed.
            for storage in channels.iter_mut() {
                if let ChannelState::Connecting(_) = storage.state {
                    if Some(conn) == storage.conn {
                        debug!(
                            "[l2cap][handle_connect_response][link = {}] cleaning up channel with mismatched identifier {}",
                            conn.raw(),
                            identifier
                        );
                        storage.state = ChannelState::Disconnected;
                    }
                }
            }
            self.state.borrow_mut().create_waker.wake();
            return Err(Error::NotFound);
        };

        match res.result {
            LeCreditConnResultCode::Success => {
                if !(0x40..=0x7f).contains(&res.dcid) || !valid_credit_parameters(res.mtu, res.mps) || duplicate_cid {
                    storage.close();
                    self.state.borrow_mut().create_waker.wake();
                    return Err(Error::InvalidValue);
                }
                storage.peer_cid = res.dcid;
                storage.peer_credits = res.credits;
                storage.peer_mps = res.mps;
                storage.peer_mtu = res.mtu;
                storage.state = ChannelState::Connected;
                self.state.borrow_mut().create_waker.wake();
                Ok(())
            }
            other => {
                warn!("Channel open request failed: {:?}", other);
                storage.state = ChannelState::ConnectFailed(other);
                self.state.borrow_mut().create_waker.wake();
                Err(Error::L2capConnectError(other))
            }
        }
    }

    /// Adds peer `req` credits to the matching connected peer CID on `conn` and wakes its sender. Returns NotFound without a match; u16 overflow requests disconnect and returns InvalidValue.
    fn handle_credit_flow(&self, conn: ConnHandle, req: &LeCreditFlowInd) -> Result<(), Error> {
        for storage in self.channels.borrow_mut().iter_mut() {
            match storage.state {
                ChannelState::Connected if storage.peer_cid == req.cid && Some(conn) == storage.conn => {
                    trace!(
                        "[l2cap][handle_credit_flow][cid = {}] {} += {} credits",
                        req.cid,
                        storage.peer_credits,
                        req.credits
                    );
                    match storage.peer_credits.checked_add(req.credits) {
                        Some(credits) => {
                            storage.peer_credits = credits;
                            storage.credit_waker.wake();
                        }
                        None => {
                            warn!(
                                "[l2cap][cid = {}] credit overflow ({} + {}), disconnecting channel",
                                req.cid, storage.peer_credits, req.credits
                            );
                            storage.disconnect();
                            self.state.borrow_mut().disconnect_waker.wake();
                            return Err(Error::InvalidValue);
                        }
                    }
                    return Ok(());
                }
                _ => {}
            }
        }
        //    trace!("[l2cap][handle_credit_flow] peer channel {} not found", req.cid);
        Err(Error::NotFound)
    }

    /// Handles peer close of local `cid`/`peer_cid` on `conn`; stores `identifier` for the deferred response and wakes the runner. Returns Ok or an outbound rejection error through `manager` if the channel tuple is unknown.
    fn handle_disconnect_request(
        &self,
        conn: ConnHandle,
        identifier: u8,
        cid: u16,
        peer_cid: u16,
        manager: &ConnectionManager<'_, P>,
    ) -> Result<(), Error> {
        for storage in self.channels.borrow_mut().iter_mut() {
            if cid == storage.cid
                && storage.peer_cid == peer_cid
                && storage.conn == Some(conn)
                && matches!(storage.state, ChannelState::Connected | ChannelState::Disconnecting)
            {
                storage.state = ChannelState::PeerDisconnecting(identifier);
                let _ = storage.inbound.close();
                self.state.borrow_mut().disconnect_waker.wake();
                return Ok(());
            }
        }
        warn!(
            "[l2cap][conn = {:?}, cid = {}] disconnect request for unknown channel",
            conn, cid
        );
        Self::try_send_signal(conn, identifier, &CommandRejectRes { reason: 2 }, manager)
    }

    /// Closes the matching Disconnecting local `cid`/`peer_cid` on `conn`; returns Ok even if no slot matches. Does not clear retained queue/reassembly storage here.
    fn handle_disconnect_response(&self, conn: ConnHandle, cid: u16, peer_cid: u16) -> Result<(), Error> {
        for storage in self.channels.borrow_mut().iter_mut() {
            if storage.state == ChannelState::Disconnecting
                && cid == storage.cid
                && storage.peer_cid == peer_cid
                && storage.conn == Some(conn)
            {
                storage.close();
                break;
            }
        }
        Ok(())
    }

    /// Returns one owned SDU from local `chan`, then replenishes receive credits through originating `ble`. Closed channels fail. Once dequeued, an error/cancellation during credit signaling drops the SDU; no retry delivery is retained.
    pub(crate) async fn receive_sdu<T: Controller>(
        &self,
        chan: ChannelIndex,
        ble: BleHost<'_, T, P>,
    ) -> Result<Sdu<P::Packet>, BleHostError<T::Error>> {
        let pdu = self.receive_pdu(ble.connections(), chan).await?;
        let mut p_buf: [u8; 16] = [0; 16];
        self.flow_control(chan, ble, &mut p_buf).await?;
        Ok(Sdu::from_pdu(pdu))
    }

    /// Copies one SDU prefix into `buf` from local `chan`, then replenishes credits through originating `ble`. Returns copied bytes; excess SDU bytes are discarded and untouched output tail is preserved. A later error/cancellation can leave the copied prefix modified without returning its length.
    pub(crate) async fn receive<T: Controller>(
        &self,
        chan: ChannelIndex,
        buf: &mut [u8],
        ble: BleHost<'_, T, P>,
    ) -> Result<usize, BleHostError<T::Error>> {
        let pdu = self.receive_pdu(ble.connections(), chan).await?;

        let to_copy = pdu.len().min(buf.len());
        // info!("[host] received a pdu of len {}, copying {} bytes", pdu.len(), to_copy);
        buf[..to_copy].copy_from_slice(&pdu.as_ref()[..to_copy]);

        let mut p_buf: [u8; 16] = [0; 16];
        self.flow_control(chan, ble, &mut p_buf).await?;
        Ok(to_copy)
    }

    /// Returns the next owned SDU for local `chan`, or ChannelClosed if no longer Connected/close marker dequeued. `ble` is retained in the signature but not consulted. Cancellation while empty consumes nothing; only one receiver waker is retained.
    async fn receive_pdu<'m>(
        &self,
        ble: &'m ConnectionManager<'d, P>,
        chan: ChannelIndex,
    ) -> Result<Pdu<P::Packet>, Error> {
        poll_fn(|cx| {
            let chan = self.channel(chan);
            if chan.state == ChannelState::Connected {
                let conn = chan.conn.unwrap();
                match chan.inbound.poll_receive(cx) {
                    Poll::Ready(Some(pdu)) => Poll::Ready(Ok(pdu)),
                    Poll::Ready(None) => Poll::Ready(Err(Error::ChannelClosed)),
                    Poll::Pending => Poll::Pending,
                }
            } else {
                Poll::Ready(Err(Error::ChannelClosed))
            }
        })
        .await
    }

    /// Segments `buf` into K-frames for local `index` through originating `ble`, using `p_buf` as reusable encoding scratch (at least MPS+4 bytes). Returns MTU/encoding/link/controller errors; waits for all SDU credits first. Error/cancellation can leave a transmitted prefix; unsent reserved credits are refunded, but sent bytes are not recalled.
    pub(crate) async fn send<T: Controller>(
        &self,
        index: ChannelIndex,
        buf: &[u8],
        p_buf: &mut [u8],
        ble: BleHost<'_, T, P>,
    ) -> Result<(), BleHostError<T::Error>> {
        let (conn, mps, mtu, peer_cid) = self.connected_channel_params(index)?;
        if buf.len() > mtu as usize {
            return Err(Error::InsufficientSpace.into());
        }
        // The number of packets we'll need to send for this payload
        let n_packets = sdu_credits(buf.len() as u16, mps);
        // info!("[host] sending {} LE K frames, len {}, mps {}", n_packets, len, mps);

        let mut grant = poll_fn(|cx| self.poll_request_to_send(index, n_packets, Some(cx))).await?;

        // Segment using mps
        let (first, remaining) = buf.split_at(buf.len().min(mps as usize - 2));

        let len = encode(first, &mut p_buf[..], peer_cid, Some(buf.len() as u16))?;
        ble.l2cap_pdu(conn).await?.send(&p_buf[..len]).await?;
        grant.confirm(1);

        let chunks = remaining.chunks(mps as usize);

        for chunk in chunks {
            let len = encode(chunk, &mut p_buf[..], peer_cid, None)?;
            ble.l2cap_pdu(conn).await?.send(&p_buf[..len]).await?;
            grant.confirm(1);
        }
        Ok(())
    }

    /// Attempts complete `buf` segmentation for local `index` through originating `ble`, using `p_buf` scratch of at least MPS+4 bytes. Returns Busy when credits/ACL ownership are unavailable, or MTU/encoding/controller errors. Controller failure after admission can expose a prefix; unused channel credits are refunded.
    pub(crate) fn try_send<T: Controller + blocking::Controller>(
        &self,
        index: ChannelIndex,
        buf: &[u8],
        p_buf: &mut [u8],
        ble: BleHost<'_, T, P>,
    ) -> Result<(), BleHostError<T::Error>> {
        let (conn, mps, mtu, peer_cid) = self.connected_channel_params(index)?;
        if buf.len() > mtu as usize {
            return Err(Error::InsufficientSpace.into());
        }

        // The number of packets we'll need to send for this payload
        let sdu_len = buf.len() as u16;
        let n_packets = sdu_credits(sdu_len, mps);

        let mut grant = match self.poll_request_to_send(index, n_packets, None) {
            Poll::Ready(res) => res?,
            Poll::Pending => {
                return Err(Error::Busy.into());
            }
        };

        // Pre-request
        let mut sender = ble.try_l2cap_sdu(conn, sdu_len, mps)?;

        // Segment using mps
        let (first, remaining) = buf.split_at(buf.len().min(mps as usize - 2));

        let len = encode(first, &mut p_buf[..], peer_cid, Some(buf.len() as u16))?;
        sender.try_send(&p_buf[..len])?;
        grant.confirm(1);

        let chunks = remaining.chunks(mps as usize);
        let num_chunks = chunks.len();

        for (i, chunk) in chunks.enumerate() {
            let len = encode(chunk, &mut p_buf[..], peer_cid, None)?;
            sender.try_send(&p_buf[..len])?;
            grant.confirm(1);
        }
        Ok(())
    }

    /// Encodes `signal` with nonzero protocol `identifier` for `conn` into a pool packet and queues it through `manager`. Returns allocation, encoding or outbound-state/capacity errors; success means queued, not transmitted.
    fn try_send_signal<D: L2capSignal>(
        conn: ConnHandle,
        identifier: u8,
        signal: &D,
        manager: &ConnectionManager<'_, P>,
    ) -> Result<(), Error> {
        let signal_header = L2capSignalHeader {
            code: D::code(),
            identifier,
            length: signal.size() as u16,
        };
        let l2cap = L2capHeader {
            channel: L2CAP_CID_LE_U_SIGNAL,
            length: signal_header.size() as u16 + signal_header.length,
        };
        let mut buf = P::allocate().ok_or(Error::OutOfMemory)?;
        let mut w = WriteCursor::new(buf.as_mut());
        w.write_hci(&l2cap)?;
        w.write_hci(&signal_header)?;
        w.write_hci(signal)?;
        let len = w.len();
        manager.try_outbound(conn, Pdu::new(buf, len))
    }

    /// Sends `param` link-update request for `handle` through `host` with a fresh identifier. Returns transport/encoding result without waiting for the peer response; cancellation may leave a transmitted prefix.
    pub(crate) async fn send_conn_param_update_req<T: Controller>(
        &self,
        handle: ConnHandle,
        host: &BleHost<'_, T, P>,
        param: &ConnParamUpdateReq,
    ) -> Result<(), BleHostError<T::Error>> {
        let identifier = self.next_request_id();
        let mut tx = [0; 16];
        host.l2cap_signal(handle, identifier, param, &mut tx[..]).await
    }

    /// Sends `param` response for `handle` and nonzero request `identifier` through `host`. Returns InvalidValue for zero or transport/encoding result; cancellation may leave a transmitted prefix.
    pub(crate) async fn send_conn_param_update_res<T: Controller>(
        &self,
        handle: ConnHandle,
        identifier: u8,
        host: &BleHost<'_, T, P>,
        param: &ConnParamUpdateRes,
    ) -> Result<(), BleHostError<T::Error>> {
        if identifier == 0 {
            return Err(Error::InvalidValue.into());
        }
        let mut tx = [0; 16];
        host.l2cap_signal(handle, identifier, param, &mut tx[..]).await
    }

    /// Returns (link handle, peer MPS bytes, peer MTU bytes, peer CID) for local `index`, or ChannelClosed. Invalid index/borrow conflict and a Connected slot without a link panic.
    fn connected_channel_params(&self, index: ChannelIndex) -> Result<(ConnHandle, u16, u16, u16), Error> {
        let chan = self.channel(index);
        if chan.state == ChannelState::Connected {
            return Ok((chan.conn.unwrap(), chan.peer_mps, chan.peer_mtu, chan.peer_cid));
        }
        //trace!("[l2cap][connected_channel_params] channel {} closed", index);
        Err(Error::ChannelClosed)
    }

    // Check the current state of flow control and send flow indications if
    // our policy says so.
    /// Replenishes local `index` according to its policy through originating `ble`, encoding into `p_buf`. Returns NotFound after closure or transport error. Accounting changes only after successful submission; cancellation/error after a partial send can leave peer/local credit knowledge different.
    async fn flow_control<T: Controller>(
        &self,
        index: ChannelIndex,
        ble: BleHost<'_, T, P>,
        p_buf: &mut [u8],
    ) -> Result<(), BleHostError<T::Error>> {
        let (conn, cid, credits) = {
            let mut chan = self.channel_mut(index);
            if chan.state == ChannelState::Connected {
                (chan.conn.unwrap(), chan.cid, chan.flow_control.process())
            } else {
                debug!("[l2cap][flow_control_process] channel {:?} not found", index);
                return Err(Error::NotFound.into());
            }
        };

        if let Some(credits) = credits {
            let identifier = self.next_request_id();
            let signal = LeCreditFlowInd { cid, credits };
            // info!("[host] sending credit flow {} credits on cid {}", credits, cid);

            // Reuse packet buffer for signalling data to save the extra TX buffer
            ble.l2cap_signal(conn, identifier, &signal, p_buf).await?;

            let mut chan = self.channel_mut(index);
            if chan.state == ChannelState::Connected {
                chan.flow_control.confirm_granted(credits);
                return Ok(());
            } else {
                debug!("[l2cap][flow_control_grant] channel {:?} not found", index);
                return Err(Error::NotFound.into());
            }
        }
        Ok(())
    }

    /// Reserves `credits` K-frames at local `index`; optional `cx` replaces its sender waker. Returns a refund-on-drop grant, Pending for insufficient peer credits, or NotFound when not Connected. Sent metrics count reservation, including subsequently refunded credits.
    fn poll_request_to_send(
        &self,
        index: ChannelIndex,
        credits: u16,
        cx: Option<&mut Context<'_>>,
    ) -> Poll<Result<CreditGrant<'_, P::Packet>, Error>> {
        let mut chan = self.channel_mut(index);
        if chan.state == ChannelState::Connected {
            if let Some(cx) = cx {
                chan.credit_waker.register(cx.waker());
            }
            if credits <= chan.peer_credits {
                chan.peer_credits -= credits;
                #[cfg(feature = "channel-metrics")]
                chan.metrics.sent(credits as usize);
                return Poll::Ready(Ok(CreditGrant::new(self.channels, &self.state, index, credits)));
            } else {
                #[cfg(feature = "channel-metrics")]
                chan.metrics.blocked_send();
                return Poll::Pending;
            }
        }
        debug!("[l2cap][pool_request_to_send] channel index {:?} not found", index);
        Poll::Ready(Err(Error::NotFound))
    }

    /// Returns the first locally/peer-requested disconnect, or Pending; optional `cx` registers the runner waiter. Returned request borrows table state without adding a channel reference; runner must send/confirm before polling another.
    pub(crate) fn poll_disconnecting<'m>(&'m self, cx: Option<&mut Context<'_>>) -> Poll<DisconnectRequest<'m, P>> {
        let mut state = self.state.borrow_mut();
        if let Some(cx) = cx {
            state.disconnect_waker.register(cx.waker());
        }
        for (idx, storage) in self.channels.borrow().iter().enumerate() {
            match storage.state {
                ChannelState::Disconnecting | ChannelState::PeerDisconnecting(_) => {
                    return Poll::Ready(DisconnectRequest {
                        index: ChannelIndex(idx as u8),
                        handle: storage.conn.unwrap(),
                        state: &self.state,
                        channels: self.channels,
                    });
                }
                _ => {}
            }
        }
        Poll::Pending
    }

    /// Retains local `index`; panics if its u8 reference count is already 255 or index/borrow is invalid.
    pub(crate) fn inc_ref(&self, index: ChannelIndex) {
        self.channel_mut(index).inc_ref();
    }

    /// Releases local `index`; zero remaining references request disconnect and wake the runner. Panics on underflow or invalid index/borrow.
    pub(crate) fn dec_ref(&self, index: ChannelIndex) {
        let mut chan = self.channel_mut(index);
        chan.refcount = unwrap!(
            chan.refcount.checked_sub(1),
            "bug: dropping a channel (i = {}) with refcount 0",
            index.0
        );
        if chan.refcount == 0 {
            chan.disconnect();
            self.state.borrow_mut().disconnect_waker.wake();
        }
    }

    /// Logs all slots if `verbose`, otherwise only non-Disconnected slots; retains an immutable table borrow during formatting.
    pub(crate) fn log_status(&self, verbose: bool) {
        log_status(&self.channels.borrow(), verbose);
    }

    #[cfg(feature = "defmt")]
    /// Formats local `index` into defmt `f`; invalid index or conflicting mutable table borrow panics.
    pub(crate) fn print(&self, index: ChannelIndex, f: defmt::Formatter) {
        use defmt::Format;
        self.channel(index).format(f);
    }

    #[cfg(feature = "channel-metrics")]
    /// Returns callback `f` result for metrics at local `index`, holding an immutable table borrow. Reentrant mutation of channel state from `f` panics.
    pub(crate) fn metrics<F: FnOnce(&Metrics) -> R, R>(&self, index: ChannelIndex, f: F) -> R {
        f(&self.channel(index).metrics)
    }
}

/// Logs `channels` with slot indices, including closed slots only when `verbose`; does not mutate state.
fn log_status<P>(channels: &[ChannelStorage<P>], verbose: bool) {
    for (idx, storage) in channels.iter().enumerate() {
        if verbose || storage.state != ChannelState::Disconnected {
            debug!("[l2cap][idx = {}] {:?}", idx, storage);
        }
    }
}

/// Runner-owned borrowed description of a pending channel disconnect; send/confirm does not wait for peer acknowledgment.
pub struct DisconnectRequest<'a, P: PacketPool> {
    /// Local slot selected by the runner poll.
    index: ChannelIndex,
    /// Captured link identity checked before signaling.
    handle: ConnHandle,
    /// Shared allocator for fresh local disconnect request identifiers.
    state: &'a RefCell<State>,
    /// Borrowed slot table; this request adds no reference count.
    channels: &'a RefCell<[ChannelStorage<P::Packet>]>,
}

impl<'a, P: PacketPool> DisconnectRequest<'a, P> {
    /// Returns the link handle captured when this disconnect request was polled.
    pub fn handle(&self) -> ConnHandle {
        self.handle
    }

    /// Submits the current slot close request/response through originating `host`; returns transport result, or Ok if state no longer needs signaling. Panics if the captured link identity changed. Cancellation leaves the request pending; partial wire effects remain possible.
    pub async fn send<T: Controller>(&self, host: &BleHost<'_, T, P>) -> Result<(), BleHostError<T::Error>> {
        let (state, conn, our_cid, peer_cid) = {
            let state = self.channels.borrow();
            let chan = &state[self.index.0 as usize];
            (chan.state.clone(), chan.conn, chan.cid, chan.peer_cid)
        };

        let mut tx = [0; 18];
        match state {
            ChannelState::PeerDisconnecting(identifier) => {
                assert_eq!(Some(self.handle), conn);
                host.l2cap_signal(
                    self.handle,
                    identifier,
                    &DisconnectionRes {
                        dcid: our_cid,
                        scid: peer_cid,
                    },
                    &mut tx[..],
                )
                .await?;
            }
            ChannelState::Disconnecting => {
                let identifier = self.state.borrow_mut().next_request_id();
                assert_eq!(Some(self.handle), conn);
                host.l2cap_signal(
                    self.handle,
                    identifier,
                    &DisconnectionReq {
                        dcid: peer_cid,
                        scid: our_cid,
                    },
                    &mut tx[..],
                )
                .await?;
            }
            _ => {}
        }
        Ok(())
    }

    /// Marks the slot Disconnected after runner submission; leaves its other fields, queues and references until cleanup/reuse. This is not a peer acknowledgment.
    pub fn confirm(self) {
        self.channels.borrow_mut()[self.index.0 as usize].state = ChannelState::Disconnected;
    }
}

/// Returns K-frame credits for `sdu_len` payload bytes plus the first frame's two-byte SDU prefix.
/// `mps` is the negotiated K-frame payload ceiling; panics below the legal 23-byte minimum.
/// Widening before addition preserves the prefix for SDU lengths 65534 and 65535.
fn sdu_credits(sdu_len: u16, mps: u16) -> u16 {
    core::assert!(mps >= 23);
    (u32::from(sdu_len) + 2).div_ceil(u32::from(mps)) as u16
}

// Core Vol 3, Part A, sections 4.22/4.23: LE credit channel limits.
/// Returns whether `mtu` is at least 23 SDU bytes and `mps` is 23..=65533 K-frame payload bytes (Core Vol3 PartA4.22/4.23).
fn valid_credit_parameters(mtu: u16, mps: u16) -> bool {
    mtu >= 23 && (23..=65533).contains(&mps)
}

/// Encodes K-frame `data` for `peer_cid` into `packet`; optional `header` supplies first-frame SDU length. Returns bytes written or InsufficientSpace with a possibly modified prefix. Internal callers bound payload to MPS<=65533 so u16 length plus the optional two bytes fits.
fn encode(data: &[u8], packet: &mut [u8], peer_cid: u16, header: Option<u16>) -> Result<usize, Error> {
    let mut w = WriteCursor::new(packet);
    if header.is_some() {
        w.write(2 + data.len() as u16)?;
    } else {
        w.write(data.len() as u16)?;
    }
    w.write(peer_cid)?;

    if let Some(len) = header {
        w.write(len)?;
    }

    w.append(data)?;
    Ok(w.len())
}

/// One reusable dynamic-channel slot; P owns queued/reassembly packet memory. Reuse requires Disconnected and zero retained references.
pub struct ChannelStorage<P> {
    /// Handshake/close phase controlling admission and reuse.
    state: ChannelState,
    /// Owning link handle; None after full close.
    conn: Option<ConnHandle>,
    /// Local dynamic CID (0x40+slot), or zero when cleared.
    cid: u16,
    /// Service protocol multiplexer, or zero before configuration/after close.
    spsm: u16,
    /// Local maximum accepted K-frame payload bytes, or zero when unconfigured.
    mps: u16,
    /// Local maximum accepted SDU payload bytes, or zero when unconfigured.
    mtu: u16,
    /// Advertised local receive credits and pending replenishment.
    flow_control: CreditFlowControl,
    /// Retained channel/half/metrics/pending owners; zero permits disconnected slot reuse.
    refcount: u8,

    /// Peer dynamic CID, or zero until accepted/after close.
    peer_cid: u16,
    /// Peer K-frame payload byte ceiling; legal negotiated range 23..=65533.
    peer_mps: u16,
    /// Peer SDU payload byte ceiling; legal negotiated minimum 23.
    peer_mtu: u16,
    /// Unreserved outgoing K-frame credits currently granted by the peer.
    peer_credits: u16,
    /// Single sender waiting for peer credits or a local refund.
    credit_waker: WakerRegistration,

    /// Completed SDUs plus best-effort closure marker; allocation clears retained contents.
    inbound: PacketChannel<P, { config::L2CAP_RX_QUEUE_SIZE }>,
    #[cfg(not(feature = "l2cap-sdu-reassembly-optimization"))]
    /// Partial nonoptimized SDU; link loss and slot allocation release retained packet memory.
    reassembly: PacketReassembly<P>,

    #[cfg(feature = "channel-metrics")]
    /// Wrapping counters for reservations, accepted frames and blocked attempts.
    metrics: Metrics,
}

/// Metrics for this channel
#[cfg(feature = "channel-metrics")]
#[derive(Debug)]
pub struct Metrics {
    /// Wrapping count of reserved outgoing K-frames, including later refunds.
    pub num_sent: usize,
    /// Wrapping count of accepted incoming K-frames.
    pub num_received: usize,
    /// Wrapping count of insufficient-credit polls, including repeated polls.
    pub blocked_send: usize,
    /// Wrapping count of incoming frames rejected for exhausted credits.
    pub blocked_receive: usize,
}

#[cfg(feature = "channel-metrics")]
impl Metrics {
    /// Returns zeroed wrapping channel counters.
    pub(crate) const fn new() -> Self {
        Self {
            num_sent: 0,
            num_received: 0,
            blocked_send: 0,
            blocked_receive: 0,
        }
    }
    /// Adds `num` reserved K-frames with usize wrapping; refunds do not subtract from this counter.
    pub(crate) fn sent(&mut self, num: usize) {
        self.num_sent = self.num_sent.wrapping_add(num);
    }

    /// Adds `num` accepted K-frames with usize wrapping.
    pub(crate) fn received(&mut self, num: usize) {
        self.num_received = self.num_received.wrapping_add(num);
    }

    /// Increments the insufficient-credit poll count with usize wrapping; repeated polls count separately.
    pub(crate) fn blocked_send(&mut self) {
        self.blocked_send = self.blocked_send.wrapping_add(1);
    }

    /// Increments the incoming-without-credit count with usize wrapping.
    pub(crate) fn blocked_receive(&mut self) {
        self.blocked_receive = self.blocked_receive.wrapping_add(1);
    }

    /// Resets all channel counters to zero on link/channel cleanup.
    pub(crate) fn reset(&mut self) {
        *self = Self::new();
    }
}

#[cfg(feature = "channel-metrics")]
#[cfg(feature = "defmt")]
impl defmt::Format for Metrics {
    /// Writes all four metrics counters to defmt formatter `f`.
    fn format(&self, f: defmt::Formatter<'_>) {
        defmt::write!(
            f,
            "sent = {}, recvd = {}, blocked send = {}, blocked receive = {}",
            self.num_sent,
            self.num_received,
            self.blocked_send,
            self.blocked_receive,
        );
    }
}

impl<P> core::fmt::Debug for ChannelStorage<P> {
    /// Writes state/identities/limits/credits/reference count to `f`; returns formatter failures. Packet payloads are omitted.
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let mut d = f.debug_struct("ChannelStorage");
        let d = d
            .field("state", &self.state)
            .field("conn", &self.conn)
            .field("cid", &self.cid)
            .field("peer_cid", &self.peer_cid)
            .field("mps", &self.mps)
            .field("mtu", &self.mtu)
            .field("peer_mps", &self.peer_mps)
            .field("peer_mtu", &self.peer_mtu)
            .field("peer_credits", &self.peer_credits)
            .field("available", &self.flow_control.available())
            .field("refcount", &self.refcount);
        #[cfg(feature = "channel-metrics")]
        let d = d.field("metrics", &self.metrics);
        d.finish()
    }
}

#[cfg(feature = "defmt")]
impl<P> defmt::Format for ChannelStorage<P> {
    /// Writes channel state/identities/limits/credits and optional metrics to defmt `f`; packet payloads are omitted.
    fn format(&self, f: defmt::Formatter<'_>) {
        defmt::write!(
            f,
            "state = {}, c = {}, cid = {}, peer = {}, mps = {}, mtu = {}, peer_mps = {}, peer_mtu = {}, cred out {}, cred in = {}, ref = {}",
            self.state,
            self.conn,
            self.cid,
            self.peer_cid,
            self.mps,
            self.mtu,
            self.peer_mps,
            self.peer_mtu,
            self.peer_credits,
            self.flow_control.available(),
            self.refcount,
        );
        #[cfg(feature = "channel-metrics")]
        defmt::write!(f, ", {}", self.metrics);
    }
}

impl<P> ChannelStorage<P> {
    /// Returns an unowned Disconnected slot with zero identities/limits/credits and empty inbound/reassembly storage.
    pub(crate) const fn new() -> ChannelStorage<P> {
        ChannelStorage {
            state: ChannelState::Disconnected,
            conn: None,
            cid: 0,
            mps: 0,
            mtu: 0,
            spsm: 0,

            flow_control: CreditFlowControl::new(CreditFlowPolicy::Every(1), 0),
            peer_cid: 0,
            peer_mps: 0,
            peer_mtu: 0,
            peer_credits: 0,
            credit_waker: WakerRegistration::new(),
            refcount: 0,
            inbound: PacketChannel::new(),
            #[cfg(not(feature = "l2cap-sdu-reassembly-optimization"))]
            reassembly: PacketReassembly::new(),
            #[cfg(feature = "channel-metrics")]
            metrics: Metrics::new(),
        }
    }

    /// Adds one retained owner; panics before mutation if u8 reference count is already 255.
    fn inc_ref(&mut self) {
        self.refcount = unwrap!(self.refcount.checked_add(1), "Too many references to the same channel");
    }

    /// Returns Ok if `actual` bytes fit `limit`; otherwise logs `label`, requests disconnect and returns InvalidValue. Caller must wake the manager disconnect waiter.
    fn check_len(&mut self, actual: u16, limit: u16, label: &str) -> Result<(), Error> {
        if actual > limit {
            warn!(
                "[l2cap][cid = {}] received {} length {} exceeds {}, disconnecting channel",
                self.cid, label, actual, limit
            );
            self.disconnect();
            return Err(Error::InvalidValue);
        }
        Ok(())
    }

    /// Returns admission of `sdu_len` bytes against this slot MTU; excess requests disconnect. Caller must wake the manager on failure.
    pub(crate) fn check_sdu_len(&mut self, sdu_len: u16) -> Result<(), Error> {
        self.check_len(sdu_len, self.mtu, "SDU")
    }

    /// Transitions Connected to Disconnecting, queues a best-effort close marker and resets metrics. Other states are unchanged; caller wakes the runner. Full queues can reject the marker.
    fn disconnect(&mut self) {
        if self.state == ChannelState::Connected {
            self.state = ChannelState::Disconnecting;
            let _ = self.inbound.close();
            #[cfg(feature = "channel-metrics")]
            self.metrics.reset();
        }
    }

    /// Resets state, link/CIDs/service/limits and credit accounting; preserves references, waker, inbound queue, reassembly and metrics for caller-managed cleanup.
    fn close(&mut self) {
        self.state = ChannelState::Disconnected;
        self.cid = 0;
        self.conn = None;
        self.mps = 0;
        self.mtu = 0;
        self.spsm = 0;
        self.peer_cid = 0;
        self.peer_mps = 0;
        self.peer_mtu = 0;
        self.flow_control = CreditFlowControl::new(CreditFlowPolicy::Every(1), 0);
        self.peer_credits = 0;
    }
}

#[derive(Debug, PartialEq, Clone)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
/// Local handshake/close state; Disconnected permits reuse only when reference count reaches zero.
pub enum ChannelState {
    /// Locally closed; reuse additionally requires zero references.
    Disconnected,
    /// Outgoing request submitted or waiting for controller capacity.
    Connecting(
        /// Nonzero request identifier expected in the response.
        u8,
    ),
    /// Incoming request awaiting application acceptance/rejection.
    PeerConnecting(
        /// Peer request identifier to echo in the response.
        u8,
    ),
    /// Handshake accepted locally; data/credit operations are admitted.
    Connected,
    /// Outgoing handshake received a negative peer result.
    ConnectFailed(
        /// Exact peer connection result delivered to the creator.
        LeCreditConnResultCode,
    ),
    /// Peer requested closure; runner must send the response.
    PeerDisconnecting(
        /// Peer disconnect identifier to echo.
        u8,
    ),
    /// Local close requested; runner must submit disconnect signaling.
    Disconnecting,
}

/// Control how credits are issued by the receiving end.
#[derive(Clone, Copy, Debug)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum CreditFlowPolicy {
    /// Issue credits for every N messages received
    Every(
        /// Consumed K-frame threshold; zero triggers even with no consumed credits.
        u16,
    ),
    /// Issue credits when below a threshold
    MinThreshold(
        /// Replenish when available credits are strictly below this value; zero never triggers.
        u16,
    ),
}

impl Default for CreditFlowPolicy {
    /// Returns Every(1), replenishing consumed credits after each receive-side flow-control check.
    fn default() -> Self {
        Self::Every(1)
    }
}

#[derive(Debug)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
/// Local receive-credit balance and consumed-but-not-yet-replenished count; arithmetic saturates at u16 bounds.
pub(crate) struct CreditFlowControl {
    /// Condition for sending all accumulated replenishment credits.
    policy: CreditFlowPolicy,
    /// Available incoming K-frame credits already advertised to the peer.
    credits: u16,
    /// Consumed K-frame credits awaiting successful replenishment submission.
    received: u16,
}

impl CreditFlowControl {
    /// Returns accounting with grant `policy`, `initial_credits` offered to the peer, and zero consumed credits.
    const fn new(policy: CreditFlowPolicy, initial_credits: u16) -> Self {
        Self {
            policy,
            credits: initial_credits,
            received: 0,
        }
    }
    /// Returns credits currently advertised as available for incoming K-frames.
    fn available(&self) -> u16 {
        self.credits
    }

    /// Consumes `n` advertised credits and accumulates consumed credits using saturating subtraction/addition; does not itself validate peer compliance.
    fn confirm_received(&mut self, n: u16) {
        self.credits = self.credits.saturating_sub(n);
        self.received = self.received.saturating_add(n);
    }

    // Confirm that we've granted amount credits
    /// Confirms `amount` replenishment after signal submission, subtracting pending consumption and adding available credits with saturation.
    fn confirm_granted(&mut self, amount: u16) {
        self.received = self.received.saturating_sub(amount);
        self.credits = self.credits.saturating_add(amount);
    }

    // Check if policy says we should grant more credits
    /// Returns all consumed credits when policy triggers, otherwise None; may return Some(0) for zero thresholds/counts and does not mutate accounting.
    fn process(&mut self) -> Option<u16> {
        match self.policy {
            CreditFlowPolicy::Every(count) => {
                if self.received >= count {
                    Some(self.received)
                } else {
                    None
                }
            }
            CreditFlowPolicy::MinThreshold(threshold) => {
                if self.credits < threshold {
                    Some(self.received)
                } else {
                    None
                }
            }
        }
    }
}

/// Reserved outgoing K-frame credits; confirms submitted frames and refunds the remainder to a still-connected slot on drop.
pub struct CreditGrant<'reference, P> {
    /// Runner wake registration used if a canceled reservation reveals credit overflow.
    state: &'reference RefCell<State>,
    /// Originating table used to refund a still-connected slot.
    channels: &'reference RefCell<[ChannelStorage<P>]>,
    /// Manager-local slot whose credits were reserved.
    index: ChannelIndex,
    /// Unconfirmed reserved credits refundable on drop.
    credits: u16,
}

impl<'reference, P> CreditGrant<'reference, P> {
    /// Returns ownership of already-debited `credits` for local `index` in `channels`, using `state` to wake deferred close on refund overflow.
    /// Does not debit the table itself or retain the channel slot.
    fn new(
        channels: &'reference RefCell<[ChannelStorage<P>]>,
        state: &'reference RefCell<State>,
        index: ChannelIndex,
        credits: u16,
    ) -> Self {
        Self {
            channels,
            state,
            index,
            credits,
        }
    }

    /// Subtracts submitted `sent` K-frames from refundable credits, saturating at zero.
    pub(crate) fn confirm(&mut self, sent: u16) {
        self.credits = self.credits.saturating_sub(sent);
    }

    /// Returns reserved K-frame credits not yet confirmed as submitted.
    pub(crate) fn remaining(&self) -> u16 {
        self.credits
    }

    /// Discards all refund entitlement without changing the channel balance.
    fn done(&mut self) {
        self.credits = 0;
    }
}

impl<P> Drop for CreditGrant<'_, P> {
    /// Refunds outstanding credits only while Connected and wakes its sender; overflow requests disconnect and wakes the runner. Invalid slot/borrow conflicts panic.
    fn drop(&mut self) {
        if self.credits > 0 {
            let mut channels = self.channels.borrow_mut();
            let chan = &mut channels[self.index.0 as usize];
            if chan.state == ChannelState::Connected {
                if let Some(credits) = chan.peer_credits.checked_add(self.credits) {
                    chan.peer_credits = credits;
                    chan.credit_waker.wake();
                } else {
                    // New peer grants may have arrived while this reservation was pending.
                    chan.disconnect();
                    self.state.borrow_mut().disconnect_waker.wake();
                }
            }
            // make it an assert?
            //        warn!("[l2cap][credit grant drop] channel {} not found", self.index);
        }
    }
}

#[cfg(test)]
/// Actual host/channel lifetime regression; no controller or hardware traffic.
mod tests {
    extern crate std;

    use bt_hci::param::{AddrKind, BdAddr, LeConnRole, Status};

    use super::*;
    use crate::mock_controller::MockController;
    use crate::prelude::{ConnParams, DefaultPacketPool};
    use crate::{Address, HostResources};

    /// Builder cancellation and full stack teardown release queued SDUs and partial reassembly before repeated resource reuse.
    #[test]
    fn teardown_releases_channel_packets_before_resource_reuse() {
        let available = || {
            let mut packets = std::vec::Vec::new();
            while let Some(packet) = DefaultPacketPool::allocate() {
                packets.push(packet);
            }
            packets.len()
        };
        let before = available();
        let mut resources = HostResources::<DefaultPacketPool, 1, 1>::new();
        for build in [false, true, false, true] {
            let builder = crate::new(MockController::new(), &mut resources);
            {
                let state = builder.host_state.as_ref().unwrap();
                let slot = state.channels.channel_mut(ChannelIndex(0));
                slot.inbound
                    .try_send(Pdu::new(DefaultPacketPool::allocate().unwrap(), 1))
                    .unwrap();
                #[cfg(not(feature = "l2cap-sdu-reassembly-optimization"))]
                {
                    let mut slot = slot;
                    slot.reassembly
                        .init(0x40, 10, DefaultPacketPool::allocate().unwrap())
                        .unwrap();
                }
                let owned = if cfg!(feature = "l2cap-sdu-reassembly-optimization") {
                    1
                } else {
                    2
                };
                assert_eq!(available(), before - owned);
            }
            if build {
                drop(builder.build());
            } else {
                drop(builder);
            }
            assert_eq!(available(), before);
        }
    }

    #[test]
    /// Link loss during an outgoing channel handshake produces Disconnected and cleanup releases its retained slot reference.
    fn channel_refcount() {
        let mut resources: HostResources<DefaultPacketPool, 2, 2> = HostResources::new();
        let ble = MockController::new();

        let builder = crate::new(ble, &mut resources);
        let ble = BleHost::new(
            builder.controller.as_ref().unwrap(),
            builder.host_state.as_ref().unwrap(),
        );

        let conn = ConnHandle::new(33);
        ble.connections()
            .connect(
                conn,
                Address::new(AddrKind::PUBLIC, BdAddr::new([0; 6])),
                LeConnRole::Central,
                ConnParams::new(),
            )
            .unwrap();
        let idx = ble
            .channels()
            .alloc(conn, None, |storage| {
                storage.state = ChannelState::Connecting(42);
                storage.inc_ref();
            })
            .unwrap();

        let chan = ble.channels().poll_created(conn, idx, ble, None);
        assert!(matches!(chan, Poll::Pending));

        ble.connections().disconnected(conn, Status::UNSPECIFIED).unwrap();
        ble.channels().disconnected(conn).unwrap();

        let chan = ble.channels().poll_created(conn, idx, ble, None);
        assert!(matches!(
            chan,
            Poll::Ready(Err(BleHostError::BleHost(Error::Disconnected)))
        ));
        ble.channels().drop_pending(idx);
        assert_eq!(ble.channels().channel(idx).refcount, 0);
    }
}
