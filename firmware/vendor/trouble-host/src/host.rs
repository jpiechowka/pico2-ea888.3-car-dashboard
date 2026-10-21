//! Single-executor HCI dispatch, controller initialization, cancellation and outbound fragmentation.
//! RX, control and TX must all progress; callbacks borrow receive bytes only during dispatch.
//! Runner failure/cancellation does not reset the controller or reconstruct already-published host state.
//! Controller buffer credits are completion accounting, not application delivery acknowledgments.
#[cfg(all(test, feature = "peripheral", feature = "default-packet-pool"))]
#[path = "host_advertise_tests.rs"]
/// Advertising serialization, admission and control-loop cleanup fixtures.
mod advertise_tests;
#[cfg(all(test, feature = "default-packet-pool"))]
#[path = "host_channel_tests.rs"]
/// Dynamic-channel and parameter-request state/transport regressions.
mod channel_tests;
#[cfg(all(test, feature = "gatt", feature = "default-packet-pool"))]
#[path = "host_mtu_tests.rs"]
/// ATT MTU/correlation and retained terminal-diagnostic regressions.
mod mtu_tests;
#[cfg(all(test, not(feature = "gatt"), feature = "default-packet-pool"))]
#[path = "host_no_gatt_tests.rs"]
/// ATT behavior with server/client GATT disabled.
mod no_gatt_tests;
#[cfg(all(test, feature = "gatt", feature = "default-packet-pool"))]
#[path = "host_regression_tests.rs"]
/// Ingress resource failure and canceled establishment regressions.
mod regression_tests;
#[cfg(all(test, feature = "central", feature = "gatt", feature = "default-packet-pool"))]
#[path = "host_wire_tests.rs"]
/// Raw HCI parsing through actual receive dispatch.
mod wire_tests;
#[cfg(feature = "security")]
use core::cell::Cell;
use core::cell::RefCell;
use core::future::{poll_fn, Future};
use core::mem::MaybeUninit;
use core::task::{Context, Poll};

use bt_hci::cmd::controller_baseband::{
    HostBufferSize, HostNumberOfCompletedPackets, Reset, SetControllerToHostFlowControl, SetEventMask,
    SetEventMaskPage2,
};
use bt_hci::cmd::info::ReadBdAddr;
#[cfg(feature = "subrating")]
use bt_hci::cmd::le::LeSetHostFeature;
#[cfg(feature = "shorter-connection-intervals")]
use bt_hci::cmd::le::LeSetHostFeatureV2;
#[cfg(feature = "security")]
use bt_hci::cmd::le::{
    LeAddDeviceToResolvingList, LeClearResolvingList, LeRand, LeRemoveDeviceFromResolvingList,
    LeSetAddrResolutionEnable, LeSetPrivacyMode, LeSetResolvablePrivateAddrTimeout,
};
use bt_hci::cmd::le::{
    LeConnUpdate, LeCreateConnCancel, LeReadBufferSize, LeReadFilterAcceptListSize, LeSetAdvEnable, LeSetEventMask,
    LeSetExtAdvEnable, LeSetExtScanEnable, LeSetRandomAddr, LeSetScanEnable,
};
use bt_hci::cmd::link_control::Disconnect;
use bt_hci::cmd::{self, AsyncCmd, SyncCmd};
use bt_hci::controller::{blocking, Controller, ControllerCmdAsync, ControllerCmdSync};
#[cfg(feature = "iso")]
use bt_hci::data::IsoPacket;
use bt_hci::data::{AclBroadcastFlag, AclPacket, AclPacketBoundary};
#[cfg(feature = "scan")]
use bt_hci::event::le::LeAdvertisingReport;
#[cfg(feature = "scan")]
use bt_hci::event::le::LeExtendedAdvertisingReport;
#[cfg(feature = "subrating")]
use bt_hci::event::le::LeSubrateChange;
use bt_hci::event::le::{
    LeAdvertisingSetTerminated, LeConnectionComplete, LeConnectionRateChange, LeConnectionUpdateComplete,
    LeDataLengthChange, LeEnhancedConnectionComplete, LeEventKind, LeEventPacket, LeFrameSpaceUpdateComplete,
    LePhyUpdateComplete, LeRemoteConnectionParameterRequest,
};
#[cfg(feature = "iso")]
use bt_hci::event::le::{LeCisEstablished, LeCisRequest};
use bt_hci::event::{DisconnectionComplete, EventKind, HardwareError, NumberOfCompletedPackets, Vendor};
#[cfg(feature = "security")]
use bt_hci::param::BdAddr;
#[cfg(feature = "scan")]
use bt_hci::param::FilterDuplicates;
use bt_hci::param::{
    AddrKind, AdvHandle, AdvSet, ConnHandle, DisconnectReason, EventMask, EventMaskPage2, LeConnRole, LeEventMask,
    Status,
};
use bt_hci::{ControllerToHostPacket, FromHciBytes, WriteHci};
use embassy_futures::select::{select3, select5, Either3, Either5};
#[cfg(any(feature = "scan", feature = "security"))]
use embassy_sync::blocking_mutex::raw::NoopRawMutex;
#[cfg(feature = "security")]
use embassy_sync::mutex::Mutex;
use embassy_sync::once_lock::OnceLock;
#[cfg(feature = "scan")]
use embassy_sync::signal::Signal;
use embassy_sync::waitqueue::WakerRegistration;
use embassy_time::Duration;
#[cfg(all(feature = "security", feature = "central"))]
use embassy_time::{Instant, Timer};
use futures::pin_mut;

use crate::att::{AttClient, AttServer};
use crate::channel_manager::{ChannelManager, ChannelStorage};
use crate::command::CommandState;
use crate::connection::{ConnParams, ConnectionEvent};
#[cfg(feature = "security")]
use crate::connection_manager::ResolvablePrivateAddrs;
use crate::connection_manager::{AclSendLock, ConnectionManager, ConnectionStorage};
use crate::cursor::WriteCursor;
use crate::pdu::Pdu;
#[cfg(feature = "connection-params-update")]
use crate::prelude::{ConnectionParamsRequest, RequestedConnParams};
#[cfg(feature = "security")]
use crate::security_manager::SecurityEventData;
use crate::types::l2cap::{
    ConnParamUpdateReq, ConnParamUpdateRes, L2capHeader, L2capSignal, L2capSignalHeader, L2CAP_CID_ATT,
    L2CAP_CID_DYN_START, L2CAP_CID_LE_U_SECURITY_MANAGER, L2CAP_CID_LE_U_SIGNAL,
};
use crate::{att, Address, BleHostError, Error, PacketPool};

#[cfg(feature = "security")]
#[derive(Clone, Copy)]
/// Deferred controller privacy-list mutation; coalescing multiple updates requires a full bond-store snapshot.
pub(crate) enum ResolvingListUpdate {
    /// Clear and rebuild the resolving list from current local IRK and stored bonds.
    FullSync,
    /// Insert/update one peer with an IRK.
    Add(
        /// Peer address/key copied from the bond store.
        crate::Identity,
    ),
    /// Remove one peer address from the controller list.
    Remove(
        /// Peer identity whose address selects the entry.
        crate::Identity,
    ),
}

#[cfg(feature = "security")]
/// One coalescing update slot and one control-loop waiter; protected by the surrounding RefCell.
pub(crate) struct ResolvingListSignal {
    /// Pending update, or None after consumption/clear.
    state: Option<ResolvingListUpdate>,
    /// Single control-loop notification waiter.
    waker: WakerRegistration,
}

#[cfg(feature = "security")]
impl ResolvingListSignal {
    /// Returns an empty update slot with no registered waiter.
    const fn new() -> Self {
        Self {
            state: None,
            waker: WakerRegistration::new(),
        }
    }

    /// Push an update and wake the control loop.
    /// Queues `update`, promoting any existing pending update to FullSync, and wakes the control loop.
    pub(crate) fn push(&mut self, update: ResolvingListUpdate) {
        self.state = Some(match self.state {
            Some(_) => ResolvingListUpdate::FullSync,
            None => update,
        });
        self.waker.wake();
    }

    /// Discard any pending update.
    /// Drops pending update without waking or changing controller state.
    pub(crate) fn clear(&mut self) {
        self.state = None;
    }

    /// Poll for a pending update. Takes the value on Ready.
    /// Registers `cx` and returns/consumes one pending update, otherwise Pending; pending cancellation consumes nothing.
    fn poll_changed(&mut self, cx: &mut Context<'_>) -> Poll<ResolvingListUpdate> {
        self.waker.register(cx.waker());
        match self.state.take() {
            Some(state) => Poll::Ready(state),
            None => Poll::Pending,
        }
    }
}

/// Bounded borrowed connection/channel/advertiser state plus single-executor command admission and initialization publication.
/// Field drop order releases connection queues and I/O retention before channel packets; borrowed owners cannot outlive the stack.
pub(crate) struct HostState<'d, P: PacketPool> {
    /// One-time controller ACL capacity publication; later controller reset does not replace it.
    initialized: OnceLock<InitialState>,
    /// Wrapping host event/error counters under runtime borrow checking.
    metrics: RefCell<HostMetrics>,
    /// Configured random/public identity override; None uses controller public address selection.
    pub(crate) address: Option<Address>,
    /// Connection slots, security state and retained outbound queue.
    pub(crate) connections: ConnectionManager<'d, P>,
    /// Dynamic channel slots and their bounded queues/credit state.
    pub(crate) channels: ChannelManager<'d, P>,
    /// Per-set advertising/termination observations.
    pub(crate) advertise_state: AdvState<'d>,
    /// Exclusive advertiser admission/cancellation; Boolean context selects extended commands.
    pub(crate) advertise_command_state: CommandState<bool>,
    /// Exclusive initiator admission/cancellation; completion or terminal close settles canceled attempts.
    pub(crate) connect_command_state: CommandState<bool>,
    /// Exclusive scan admission/cancellation; Boolean context selects extended commands.
    pub(crate) scan_command_state: CommandState<bool>,
    #[cfg(feature = "security")]
    /// Single-executor async gate serializing operation admission against privacy mutations.
    pub(crate) command_request_gate: Mutex<NoopRawMutex, ()>,
    #[cfg(feature = "security")]
    /// Configured private-address rotation duration; defaults to 900 seconds.
    pub(crate) rpa_timeout: Cell<embassy_time::Duration>,
    #[cfg(all(feature = "security", feature = "central"))]
    /// Monotonic local RPA expiration; zero requests initial rotation when eligible.
    pub(crate) rpa_expires_at: Cell<Instant>,
    #[cfg(feature = "security")]
    /// Coalescing bond-store privacy update signal.
    pub(crate) resolving_list_state: RefCell<ResolvingListSignal>,
    #[cfg(feature = "scan")]
    /// Latched extended ScanTimeout event; independent of application timeout policy.
    pub(crate) scan_timeout: Signal<NoopRawMutex, ()>,
}

impl<'d, P: PacketPool> HostState<'d, P> {
    /// Returns uninitialized host state borrowing `connections`, `channels`, `advertise_handles` and optional `bond_storage`; tables are not cleared, default privacy timeout is 900 seconds.
    pub(crate) fn new(
        connections: &'d RefCell<[ConnectionStorage<P::Packet>]>,
        channels: &'d RefCell<[ChannelStorage<P::Packet>]>,
        advertise_handles: &'d RefCell<[AdvHandleState]>,
        #[cfg(feature = "security")] bond_storage: &'d RefCell<heapless::VecView<crate::BondInformation>>,
    ) -> Self {
        Self {
            address: None,
            initialized: OnceLock::new(),
            metrics: RefCell::new(HostMetrics::default()),
            connections: ConnectionManager::new(
                connections,
                #[cfg(feature = "security")]
                bond_storage,
            ),
            channels: ChannelManager::new(channels),
            advertise_state: AdvState::new(advertise_handles),
            advertise_command_state: CommandState::new(),
            scan_command_state: CommandState::new(),
            connect_command_state: CommandState::new(),
            #[cfg(feature = "security")]
            command_request_gate: Mutex::new(()),
            #[cfg(feature = "security")]
            rpa_timeout: Cell::new(embassy_time::Duration::from_secs(900)),
            #[cfg(all(feature = "security", feature = "central"))]
            rpa_expires_at: Cell::new(Instant::from_ticks(0)),
            #[cfg(feature = "security")]
            resolving_list_state: RefCell::new(ResolvingListSignal::new()),
            #[cfg(feature = "scan")]
            scan_timeout: Signal::new(),
        }
    }
}

/// A BLE Host.
///
/// The BleHost holds the runtime state of the host, and is the entry point
/// for all interactions with the controller.
///
/// The host performs connection management, l2cap channel management, and
/// multiplexes events and data across connections and l2cap channels.
pub(crate) struct BleHost<'d, T, P: PacketPool> {
    /// Borrowed HCI transport; runners share its interior-synchronized command/data interface.
    controller: &'d T,
    /// Borrowed single-executor host state; copying a handle does not duplicate resources.
    state: &'d HostState<'d, P>,
}

impl<'d, T, P: PacketPool> Clone for BleHost<'d, T, P> {
    /// Returns another borrowed handle to the same controller/state.
    fn clone(&self) -> Self {
        *self
    }
}

impl<'d, T, P: PacketPool> Copy for BleHost<'d, T, P> {}

#[derive(Clone, Copy)]
/// Controller ACL capacity published once after control startup queries.
pub(crate) struct InitialState {
    /// Maximum ACL data bytes per HCI packet, excluding HCI header; zero denies sending.
    acl_max: usize,
    /// Controller ACL packet count copied into each newly established link budget.
    acl_total: usize,
}

#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[derive(Clone, Copy, Debug)]
/// Host observation of an extended advertising set, separate from command cancellation acknowledgment.
pub(crate) enum AdvHandleState {
    /// Unused slot; counts as complete.
    None,
    /// Controller set currently observed advertising.
    Advertising(
        /// Controller advertising-set identifier.
        AdvHandle,
    ),
    /// Controller termination observed for this set.
    Terminated(
        /// Controller advertising-set identifier retained for diagnostics.
        AdvHandle,
    ),
}

/// Borrowed advertising-set slots and a single completion waiter.
pub(crate) struct AdvState<'d> {
    /// Fixed host set capacity; None slots count as complete.
    handles: &'d RefCell<[AdvHandleState]>,
    /// Single waiter for all sets becoming terminated/unused.
    waker: RefCell<WakerRegistration>,
}

impl<'d> AdvState<'d> {
    /// Returns a view of `handles` without clearing their states; starts with no waiter.
    pub(crate) fn new(handles: &'d RefCell<[AdvHandleState]>) -> Self {
        Self {
            handles,
            waker: RefCell::new(WakerRegistration::new()),
        }
    }

    /// Marks all sets unused and wakes completion; does not send controller disable commands.
    pub(crate) fn reset(&self) {
        for entry in self.handles.borrow_mut().iter_mut() {
            *entry = AdvHandleState::None;
        }
        self.waker.borrow_mut().wake();
    }

    // Terminate handle
    /// Marks all matching active `handle` slots terminated and wakes completion, even when no slot matched.
    pub(crate) fn terminate(&self, handle: AdvHandle) {
        for entry in self.handles.borrow_mut().iter_mut() {
            match entry {
                AdvHandleState::Advertising(h) if *h == handle => {
                    *entry = AdvHandleState::Terminated(handle);
                }
                _ => {}
            }
        }
        self.waker.borrow_mut().wake();
    }

    /// Returns fixed host advertising-set capacity.
    pub(crate) fn len(&self) -> usize {
        self.handles.as_ptr().len()
    }

    /// Replaces all observations with active handles from `sets`; panics before mutation if sets exceed capacity. Does not issue controller commands.
    pub(crate) fn start(&self, sets: &[AdvSet]) {
        assert!(sets.len() <= self.handles.as_ptr().len());
        let mut handles = self.handles.borrow_mut();
        for handle in handles.iter_mut() {
            *handle = AdvHandleState::None;
        }

        for (idx, entry) in sets.iter().enumerate() {
            handles[idx] = AdvHandleState::Advertising(entry.adv_handle);
        }
    }

    /// Waits until every slot is unused/terminated; zero slots complete immediately. Pending cancellation consumes no state; one waiter is supported.
    pub async fn wait(&self) {
        poll_fn(|cx| {
            self.waker.borrow_mut().register(cx.waker());

            let mut terminated = 0;
            for entry in self.handles.borrow().iter() {
                match entry {
                    AdvHandleState::Terminated(_) => {
                        terminated += 1;
                    }
                    AdvHandleState::None => {
                        terminated += 1;
                    }
                    _ => {}
                }
            }
            if terminated == self.handles.as_ptr().len() {
                Poll::Ready(())
            } else {
                Poll::Pending
            }
        })
        .await;
    }
}

/// Host metrics
#[derive(Default, Clone)]
pub struct HostMetrics {
    /// How many connect events have been received.
    pub connect_events: u32,
    /// How many disconnect events have been received.
    pub disconnect_events: u32,
    /// How many errors processing received data.
    pub rx_errors: u32,
}

impl<'d, T, P> BleHost<'d, T, P>
where
    P: PacketPool,
{
    /// Create a new instance of the BLE host.
    ///
    /// The host requires a HCI driver (a particular HCI-compatible controller implementing the required traits), and
    /// a reference to resources that are created outside the host but which the host is the only accessor of.
    /// Returns a borrowed host view over `controller` and `state`; no initialization or I/O occurs.
    pub(crate) fn new(controller: &'d T, state: &'d HostState<'d, P>) -> Self {
        Self { controller, state }
    }

    /// Returns the configured address override; None is distinct from the controller public-address query result.
    pub(crate) fn address(&self) -> Option<Address> {
        self.state.address
    }

    /// Returns shared connection-procedure admission/cancellation state.
    pub(crate) fn connect_command_state(&self) -> &CommandState<bool> {
        &self.state.connect_command_state
    }

    /// Returns shared advertising-procedure admission/cancellation state.
    pub(crate) fn advertise_command_state(&self) -> &CommandState<bool> {
        &self.state.advertise_command_state
    }

    /// Returns shared scan-procedure admission/cancellation state.
    pub(crate) fn scan_command_state(&self) -> &'d CommandState<bool> {
        &self.state.scan_command_state
    }

    /// Returns the originating connection manager and its fixed-slot ownership domain.
    pub(crate) fn connections(&self) -> &'d ConnectionManager<'d, P> {
        &self.state.connections
    }

    /// Returns the originating dynamic-channel manager.
    pub(crate) fn channels(&self) -> &'d ChannelManager<'d, P> {
        &self.state.channels
    }

    /// Returns host advertising-set completion observations.
    pub(crate) fn advertise_state(&self) -> &AdvState<'d> {
        &self.state.advertise_state
    }

    #[cfg(feature = "scan")]
    /// Returns the extended scan timeout signal; caller controls consumption/reset.
    pub(crate) fn scan_timeout(&self) -> &'d Signal<NoopRawMutex, ()> {
        &self.state.scan_timeout
    }

    #[cfg(feature = "security")]
    /// Returns the coalescing pending privacy-list update state.
    pub(crate) fn resolving_list_state(&self) -> &RefCell<ResolvingListSignal> {
        &self.state.resolving_list_state
    }

    #[cfg(feature = "security")]
    /// Returns the configured local private-address rotation duration cell.
    pub(crate) fn rpa_timeout(&self) -> &Cell<Duration> {
        &self.state.rpa_timeout
    }
}

impl<'d, T, P> BleHost<'d, T, P>
where
    T: Controller,
    P: PacketPool,
{
    /// Poll whether any command should be cancelled or the resolving list should be synced.
    /// Polls control work with `cx` in priority order: connect, advertise, scan cancellation; eligible RPA rotation; then idle-only list update. Returns one action or Pending; resolving updates are consumed on Ready.
    fn poll_cancelled(&self, cx: &mut Context<'_>) -> Poll<CancelledCommandState> {
        // Not every branch below survives every feature combination.
        let _ = cx;

        #[cfg(feature = "central")]
        if let Poll::Ready(ctx) = self.state.connect_command_state.poll_cancelled(cx) {
            return Poll::Ready(CancelledCommandState::Connect(ctx));
        }
        #[cfg(feature = "peripheral")]
        if let Poll::Ready(ctx) = self.state.advertise_command_state.poll_cancelled(cx) {
            return Poll::Ready(CancelledCommandState::Advertise(ctx));
        }
        #[cfg(feature = "scan")]
        if let Poll::Ready(ctx) = self.state.scan_command_state.poll_cancelled(cx) {
            return Poll::Ready(CancelledCommandState::Scan(ctx));
        }

        #[cfg(all(feature = "security", feature = "central"))]
        if self.is_privacy_enabled() && self.is_rpa_rotation_ready() {
            return Poll::Ready(CancelledCommandState::RotateRpa);
        }

        #[cfg(feature = "security")]
        if self.state.connect_command_state.is_idle()
            && self.state.advertise_command_state.is_idle()
            && self.state.scan_command_state.is_idle()
        {
            if let Poll::Ready(update) = self.state.resolving_list_state.borrow_mut().poll_changed(cx) {
                return Poll::Ready(CancelledCommandState::SyncResolvingList(update));
            }
        }

        Poll::Pending
    }

    /// Check whether BLE address privacy is enabled.
    #[cfg(feature = "security")]
    /// Returns whether a local IRK is configured; does not prove controller privacy setup succeeded.
    pub(crate) fn is_privacy_enabled(&self) -> bool {
        self.state.connections.security_manager.get_local_irk().is_some()
    }

    /// Get the appropriate own address kind based on the host address and privacy state.
    /// Returns configured/default own-address kind, choosing RPA-with-random/public fallback when local privacy is enabled.
    pub(crate) fn own_addr_kind(&self) -> AddrKind {
        #[cfg(feature = "security")]
        if self.is_privacy_enabled() {
            return if self.state.address.is_some() {
                AddrKind::RESOLVABLE_PRIVATE_OR_RANDOM
            } else {
                AddrKind::RESOLVABLE_PRIVATE_OR_PUBLIC
            };
        }
        self.state.address.map(|a| a.kind).unwrap_or(AddrKind::PUBLIC)
    }

    /// Atomically mark an address-using procedure active relative to RPA rotation.
    /// Waits for exclusive `state` admission carrying `ctx`; security builds serialize admission against RPA/list mutation. Cancellation while waiting releases the gate and acquires no completed request.
    pub(crate) async fn request_operation<CTX: Clone + Copy>(&self, state: &CommandState<CTX>, ctx: CTX) {
        #[cfg(feature = "security")]
        let _guard = self.state.command_request_gate.lock().await;
        state.request(ctx).await;
    }

    #[cfg(all(feature = "security", feature = "central"))]
    /// Returns true when local privacy is enabled, expiration passed, initiator/scanner idle and no active legacy advertiser; extended advertising does not block this global-address update.
    fn is_rpa_rotation_ready(&self) -> bool {
        self.is_privacy_enabled()
            && self.state.connect_command_state.is_idle()
            && !self.state.advertise_command_state.is_active_with(|extended| !extended)
            && self.state.scan_command_state.is_idle()
            && Instant::now() >= self.state.rpa_expires_at.get()
    }

    #[cfg(all(feature = "security", feature = "central"))]
    /// Waits for the sampled/current RPA deadline; if procedures still block rotation, remains pending and relies on the separate command-state polling branch to wake control.
    async fn wait_for_rpa_expiration(&self) {
        while Instant::now() < self.state.rpa_expires_at.get() {
            Timer::at(self.state.rpa_expires_at.get()).await
        }
        if !self.is_rpa_rotation_ready() {
            // A command is still active, so rely on poll_cancelled to wake the runner loop instead
            core::future::pending::<()>().await;
        }
    }

    #[cfg(all(feature = "security", feature = "central"))]
    /// Returns Ok after eligible private-address generation/controller update and deadline renewal, or no-op if ineligible. RNG/HCI failures propagate; cancellation can leave controller state changed before deadline publication.
    async fn rotate_rpa(&self) -> Result<(), BleHostError<T::Error>>
    where
        T: ControllerCmdSync<LeSetRandomAddr>,
    {
        let _guard = self.state.command_request_gate.lock().await;
        if !self.is_rpa_rotation_ready() {
            return Ok(());
        }
        let Some(rpa) = self.state.connections.security_manager.generate_local_rpa()? else {
            return Ok(());
        };
        LeSetRandomAddr::new(rpa).exec(self.controller).await?;
        self.state
            .rpa_expires_at
            .set(Instant::now() + self.state.rpa_timeout.get());
        trace!("[host] rotated private address");
        Ok(())
    }

    /// Sync the controller's resolving list based on a pending update.
    #[cfg(feature = "security")]
    /// Applies `update` under operation gate only when all procedures are idle; otherwise returns Ok without replay. Disables resolution, applies best-effort entries, then attempts re-enable; command errors may leave partial state. Cancellation has no async restoration guard.
    pub(crate) async fn sync_resolving_list(&self, update: ResolvingListUpdate) -> Result<(), BleHostError<T::Error>>
    where
        T: ControllerCmdSync<LeClearResolvingList>
            + ControllerCmdSync<LeSetAddrResolutionEnable>
            + ControllerCmdSync<LeRemoveDeviceFromResolvingList>
            + ControllerCmdSync<LeAddDeviceToResolvingList>
            + ControllerCmdSync<LeSetPrivacyMode>,
        T::Error: crate::fmt::Format,
    {
        let _guard = self.state.command_request_gate.lock().await;
        if !(self.state.connect_command_state.is_idle()
            && self.state.advertise_command_state.is_idle()
            && self.state.scan_command_state.is_idle())
        {
            return Ok(());
        }

        let local_irk = self.state.connections.security_manager.get_local_irk();
        let local_irk_bytes = local_irk.map(|k| k.to_le_bytes()).unwrap_or_default();

        // Disable address resolution while modifying the list
        LeSetAddrResolutionEnable::new(false).exec(self.controller).await?;

        let res = match update {
            ResolvingListUpdate::FullSync => self.full_resolving_list_sync(local_irk).await,
            ResolvingListUpdate::Add(identity) => {
                if let Some(peer_irk) = identity.irk {
                    let peer_addr_kind = identity.addr.kind;
                    let peer_irk_bytes = peer_irk.to_le_bytes();

                    debug!("[host] incremental resolving list add");

                    // Remove first in case this is an update (device already in list)
                    let _ = LeRemoveDeviceFromResolvingList::new(peer_addr_kind, identity.addr.addr)
                        .exec(self.controller)
                        .await;

                    if let Err(e) = LeAddDeviceToResolvingList::new(
                        peer_addr_kind,
                        identity.addr.addr,
                        peer_irk_bytes,
                        local_irk_bytes,
                    )
                    .exec(self.controller)
                    .await
                    {
                        warn!("[host] failed to add device to resolving list: {:?}", e);
                    }

                    if let Err(e) =
                        LeSetPrivacyMode::new(peer_addr_kind, identity.addr.addr, bt_hci::param::PrivacyMode::Device)
                            .exec(self.controller)
                            .await
                    {
                        warn!("[host] failed to set privacy mode: {:?}", e);
                    }
                }
                Ok(())
            }
            ResolvingListUpdate::Remove(identity) => {
                let peer_addr_kind = identity.addr.kind;

                debug!("[host] incremental resolving list remove");

                if let Err(e) = LeRemoveDeviceFromResolvingList::new(peer_addr_kind, identity.addr.addr)
                    .exec(self.controller)
                    .await
                {
                    warn!("[host] failed to remove device from resolving list: {:?}", e);
                }
                Ok(())
            }
        };

        // The normal completion path attempts to re-enable address resolution; cancellation does not.
        if let Err(e) = LeSetAddrResolutionEnable::new(true).exec(self.controller).await {
            warn!("[host] failed to re-enable address resolution: {:?}", e);
            return res.and(Err(e.into()));
        }

        res
    }

    /// Full clear-and-rebuild of the resolving list.
    #[cfg(feature = "security")]
    /// Rebuilds the list after clear, using `local_irk` and current bonds. Returns clear failure; default/peer insertion and privacy-mode failures are logged, with peer insertion stopping the loop. No atomic rollback.
    async fn full_resolving_list_sync(
        &self,
        local_irk: Option<crate::security_manager::IdentityResolvingKey>,
    ) -> Result<(), BleHostError<T::Error>>
    where
        T: ControllerCmdSync<LeClearResolvingList>
            + ControllerCmdSync<LeAddDeviceToResolvingList>
            + ControllerCmdSync<LeSetPrivacyMode>,
        T::Error: crate::fmt::Format,
    {
        debug!("[host] full resolving list sync");

        // Clear resolving list
        LeClearResolvingList::new().exec(self.controller).await?;

        let local_irk_bytes = local_irk.map(|k| k.to_le_bytes()).unwrap_or_default();

        // Add a default entry with just the local IRK so the controller can generate RPAs
        // even when no peers are bonded yet. Only needed when we have our own IRK.
        if local_irk.is_some() {
            if let Err(e) =
                LeAddDeviceToResolvingList::new(AddrKind::PUBLIC, BdAddr::default(), [0u8; 16], local_irk_bytes)
                    .exec(self.controller)
                    .await
            {
                warn!("[host] failed to add default resolving list entry: {:?}", e);
            }
        }

        // Add entries for all bonded peers with IRKs
        let mut i = 0;
        while let Some(bond) = self.state.connections.security_manager.get_bond(i) {
            i += 1;
            if let Some(peer_irk) = bond.identity.irk {
                let peer_addr_kind = bond.identity.addr.kind;
                let peer_irk_bytes = peer_irk.to_le_bytes();
                if let Err(e) = LeAddDeviceToResolvingList::new(
                    peer_addr_kind,
                    bond.identity.addr.addr,
                    peer_irk_bytes,
                    local_irk_bytes,
                )
                .exec(self.controller)
                .await
                {
                    warn!("[host] failed to add device to resolving list: {:?}", e);
                    break;
                }
                // Set device privacy mode so we can receive both RPAs and identity addresses
                if let Err(e) = LeSetPrivacyMode::new(
                    peer_addr_kind,
                    bond.identity.addr.addr,
                    bt_hci::param::PrivacyMode::Device,
                )
                .exec(self.controller)
                .await
                {
                    warn!("[host] failed to set privacy mode: {:?}", e);
                }
            }
        }

        debug!("[host] resolving list synced");
        Ok(())
    }

    /// Returns true if the host has been initialized by the Runner.
    pub(crate) fn is_initialized(&self) -> bool {
        self.state.initialized.try_get().is_some()
    }

    /// Waits for the one-time ACL-capacity publication; cancellation consumes no state and no timeout is imposed.
    pub(crate) async fn wait_initialized(&self) {
        let _ = self.state.initialized.get().await;
    }

    /// Run a HCI command and return the response.
    /// Waits for initialization, executes `cmd` and returns its synchronous response or controller/HCI error. Cancellation semantics after submission are transport-specific.
    pub(crate) async fn command<C>(&self, cmd: C) -> Result<C::Return, BleHostError<T::Error>>
    where
        C: SyncCmd,
        T: ControllerCmdSync<C>,
    {
        let _ = self.state.initialized.get().await;
        let ret = cmd.exec(self.controller).await?;
        Ok(ret)
    }

    /// Run an async HCI command where the response will generate an event later.
    /// Waits for initialization, submits `cmd` and returns command-status success/error; the later procedure-completion event remains separate. Cancellation after submission does not undo the procedure.
    pub(crate) async fn async_command<C>(&self, cmd: C) -> Result<(), BleHostError<T::Error>>
    where
        C: AsyncCmd,
        T: ControllerCmdAsync<C>,
    {
        let _ = self.state.initialized.get().await;
        cmd.exec(self.controller).await?;
        Ok(())
    }

    #[cfg(feature = "iso")]
    /// Submits borrowed ISO `packet` directly without initialization wait; returns the controller transport result with its cancellation semantics.
    pub(crate) async fn write_iso_data(&self, packet: &bt_hci::data::IsoPacket<'_>) -> Result<(), T::Error> {
        self.controller.write_iso_data(packet).await
    }

    /// Consumes `status`, `handle`, `peer_addr`, `role`, `params` and optional `resolvable_addrs`; returns false only when a successful link cannot get a slot. Canceled central successes are quarantined for disconnect; other successes increment wrapping connect metrics.
    fn handle_connection(
        &self,
        status: Status,
        handle: ConnHandle,
        peer_addr: Address,
        role: LeConnRole,
        params: ConnParams,
        #[cfg(feature = "security")] resolvable_addrs: ResolvablePrivateAddrs,
    ) -> bool {
        match status.to_result() {
            Ok(_) => {
                if let Err(err) = self.state.connections.connect_with_rpas(
                    handle,
                    peer_addr,
                    role,
                    params,
                    #[cfg(feature = "security")]
                    resolvable_addrs,
                ) {
                    warn!("Error establishing connection: {:?}", err);
                    return false;
                } else {
                    if role == LeConnRole::Central && !self.state.connect_command_state.is_active() {
                        self.state
                            .connections
                            .request_handle_disconnect(handle, DisconnectReason::RemoteUserTerminatedConn);
                        self.state.connect_command_state.canceled();
                        return true;
                    }
                    #[cfg(feature = "defmt")]
                    debug!(
                        "[host] connection with handle {:?} established to {}",
                        handle, peer_addr
                    );

                    #[cfg(feature = "log")]
                    debug!(
                        "[host] connection with handle {:?} established to {}",
                        handle, peer_addr
                    );
                    let mut m = self.state.metrics.borrow_mut();
                    m.connect_events = m.connect_events.wrapping_add(1);
                }
            }
            Err(e) => {
                self.handle_connection_error(e);
            }
        }
        true
    }

    /// Routes `error`: advertising timeout clears set observations; other failures settle the pending initiator command with that status.
    fn handle_connection_error(&self, error: bt_hci::param::Error) {
        if error == bt_hci::param::Error::ADV_TIMEOUT {
            self.state.advertise_state.reset();
        } else {
            warn!("Error connection complete event: {:?}", error);
            self.state.connect_command_state.failed(error);
        }
    }

    /// Validates `data` length against nonzero `expected_len`; returns true after handling failure status, false on success, or HciDecode. Failed trailing fields are deliberately not decoded; internal expected_len must be positive.
    fn handle_failed_connection_complete(&self, data: &[u8], expected_len: usize) -> Result<bool, Error> {
        if data.len() != expected_len {
            return Err(Error::HciDecode(bt_hci::FromHciBytesError::InvalidSize));
        }
        // HCI 4.5/Table 4.1: all fields are sent on failure, but only Status
        // is valid. Do not reject undefined Role/AddressType/ClockAccuracy.
        if let Err(error) = Status::new(data[0]).to_result() {
            self.handle_connection_error(error);
            return Ok(true);
        }
        Ok(false)
    }

    /// Validates `event` success before terminating connection/channel state and incrementing wrapping metrics. Returns HCI failure unchanged; unknown successful handles still count as events.
    fn handle_disconnection_complete(&self, event: DisconnectionComplete) -> Result<(), Error> {
        // A failed event does not terminate the link and its reason is invalid.
        event.status.to_result().map_err(Error::Hci)?;
        info!(
            "[host] disconnection event on handle {}, reason: {:?}",
            event.handle.raw(),
            event.reason
        );
        let _ = self.state.connections.disconnected(event.handle, event.reason);
        let _ = self.state.channels.disconnected(event.handle);
        let mut metrics = self.state.metrics.borrow_mut();
        metrics.disconnect_events = metrics.disconnect_events.wrapping_add(1);
        Ok(())
    }

    /// Processes borrowed `acl` through `event_handler`; returns ingress errors after recording first local cause and requesting close, except NotSupported. Already-closing slots do not gain a new cause.
    fn handle_acl(&self, acl: AclPacket<'_>, event_handler: &dyn EventHandler) -> Result<(), Error> {
        let result = self.handle_acl_inner(acl, event_handler);
        if let Some(error) = result
            .as_ref()
            .err()
            .filter(|error| !matches!(error, Error::NotSupported))
        {
            self.state
                .connections
                .record_local_disconnect_error(acl.handle(), error);
            // Lost/malformed ACL bytes invalidate the stream, including failures
            // before ATT decoding (pool exhaustion and fragment reassembly).
            self.state
                .connections
                .request_handle_disconnect(acl.handle(), DisconnectReason::RemoteUserTerminatedConn);
        }
        result
    }

    /// Reassembles/routes borrowed `acl` using `event_handler` for security effects. Returns partial success while waiting, or decode/state/credit/capacity errors. ATT ingress validates correlation and queues confirmations; dynamic optimization shares one per-link assembly.
    fn handle_acl_inner(&self, acl: AclPacket<'_>, event_handler: &dyn EventHandler) -> Result<(), Error> {
        self.state.connections.received(acl.handle())?;
        let handle = acl.handle();
        let (header, pdu) = match acl.boundary_flag() {
            AclPacketBoundary::FirstFlushable => {
                let (header, data) = L2capHeader::from_hci_bytes(acl.data())?;

                // Ignore channels we don't support
                if header.channel < L2CAP_CID_DYN_START
                    && !(&[L2CAP_CID_LE_U_SIGNAL, L2CAP_CID_ATT, L2CAP_CID_LE_U_SECURITY_MANAGER]
                        .contains(&header.channel))
                {
                    // Apple devices probe this channel id even if it's outside the dynamic range.
                    if header.channel == 0x3A {
                        info!(
                            "[host] unsupported l2cap channel id {} (Apple devices which always probe this channel, \
                             so this is safe to ignore).",
                            header.channel
                        );
                    } else {
                        warn!("[host] unsupported l2cap channel id {}", header.channel);
                    }
                    return Err(Error::NotSupported);
                }

                // Fast-path for complete signalling packets
                if header.channel == L2CAP_CID_LE_U_SIGNAL && data.len() == header.length as usize {
                    return self.state.channels.signal(acl.handle(), data, &self.state.connections);
                }

                trace!(
                    "[host] inbound l2cap header channel = {}, fragment len = {}, total = {}",
                    header.channel,
                    data.len(),
                    header.length
                );

                // We must be prepared to receive fragments.
                if header.length as usize != data.len() {
                    // Dynamic channels can be optimized.
                    #[cfg(feature = "l2cap-sdu-reassembly-optimization")]
                    if header.channel >= L2CAP_CID_DYN_START {
                        // This is the start of the frame, so make sure to adjust the credits.
                        self.state.channels.received(handle, header.channel, 1)?;
                        self.state
                            .channels
                            .check_pdu_len(handle, header.channel, header.length)?;

                        self.state
                            .connections
                            .reassembly(acl.handle(), |p| {
                                let r = if !p.in_progress() {
                                    // Init the new assembly assuming the length of the SDU.
                                    let (first, payload) = data.split_at_checked(2).ok_or(Error::InvalidValue)?;
                                    let len: u16 = u16::from_le_bytes([first[0], first[1]]);
                                    self.state.channels.check_sdu_len(handle, header.channel, len)?;
                                    let Some(packet) = P::allocate() else {
                                        warn!("[host] no memory for packets on channel {}", header.channel);
                                        return Err(Error::OutOfMemory);
                                    };
                                    p.init(header.channel, len, packet)?;
                                    p.update(payload)?
                                } else {
                                    p.update(data)?
                                };
                                // Something is wrong if assembly was finished since we've not received the last
                                // fragment.
                                if r.is_some() {
                                    Err(Error::InvalidState)
                                } else {
                                    Ok(())
                                }
                            })
                            .inspect_err(|_| self.state.channels.disconnect_by_cid(handle, header.channel))?;
                        return Ok(());
                    }

                    // For dynamic channels, validate PDU length against MPS before reassembly.
                    if header.channel >= L2CAP_CID_DYN_START {
                        self.state
                            .channels
                            .check_pdu_len(handle, header.channel, header.length)?;
                    }

                    let Some(packet) = P::allocate() else {
                        warn!("[host] no memory for packets on channel {}", header.channel);
                        return Err(Error::OutOfMemory);
                    };
                    self.state.connections.reassembly(acl.handle(), |p| {
                        p.init(header.channel, header.length, packet)?;
                        let r = p.update(data)?;
                        if r.is_some() {
                            Err(Error::InvalidState)
                        } else {
                            Ok(())
                        }
                    })?;
                    return Ok(());
                } else {
                    #[allow(unused_mut)]
                    let mut result = None;

                    // For dynamic channels, validate PDU length against MPS and handle
                    // credit accounting and SDU reassembly before HCI reassembly.
                    #[cfg(feature = "l2cap-sdu-reassembly-optimization")]
                    if header.channel >= L2CAP_CID_DYN_START {
                        // This is a complete L2CAP K-frame, so make sure to adjust the credits.
                        self.state.channels.received(handle, header.channel, 1)?;
                        self.state
                            .channels
                            .check_pdu_len(handle, header.channel, header.length)?;

                        if let Some((state, pdu)) = self
                            .state
                            .connections
                            .reassembly(acl.handle(), |p| {
                                if !p.in_progress() {
                                    let (first, payload) = data.split_at_checked(2).ok_or(Error::InvalidValue)?;
                                    let len: u16 = u16::from_le_bytes([first[0], first[1]]);
                                    self.state.channels.check_sdu_len(handle, header.channel, len)?;
                                    let Some(packet) = P::allocate() else {
                                        warn!("[host] no memory for packets on channel {}", header.channel);
                                        return Err(Error::OutOfMemory);
                                    };
                                    p.init(header.channel, len, packet)?;
                                    p.update(payload)
                                } else {
                                    p.update(data)
                                }
                            })
                            .inspect_err(|_| self.state.channels.disconnect_by_cid(handle, header.channel))?
                        {
                            result.replace((state, pdu));
                        } else {
                            return Ok(());
                        }
                    }

                    if let Some((state, pdu)) = result {
                        (state, pdu)
                    } else {
                        // For dynamic channels, validate PDU length against MPS before reassembly.
                        if header.channel >= L2CAP_CID_DYN_START {
                            self.state
                                .channels
                                .check_pdu_len(handle, header.channel, header.length)?;
                        }

                        let Some(packet) = P::allocate() else {
                            warn!("[host] no memory for packets on channel {}", header.channel);
                            return Err(Error::OutOfMemory);
                        };
                        let result = self.state.connections.reassembly(acl.handle(), |p| {
                            p.init(header.channel, header.length, packet)?;
                            p.update(data)
                        })?;
                        let Some((state, pdu)) = result else {
                            return Err(Error::InvalidState);
                        };
                        (state, pdu)
                    }
                }
            }
            // Next (potentially last) in a fragment
            AclPacketBoundary::Continuing => {
                trace!("[host] inbound l2cap len = {}", acl.data().len(),);
                // Get the existing fragment
                if let Some((header, p)) = self.state.connections.reassembly(acl.handle(), |p| {
                    if !p.in_progress() {
                        warn!(
                            "[host] unexpected continuation fragment of length {} for handle {}: {:?}",
                            acl.data().len(),
                            acl.handle().raw(),
                            p
                        );
                        return Err(Error::InvalidState);
                    }
                    p.update(acl.data())
                })? {
                    (header, p)
                } else {
                    // Do not process yet
                    return Ok(());
                }
            }
            other => {
                warn!("Unexpected boundary flag: {:?}!", other);
                return Err(Error::InvalidValue);
            }
        };

        match header.channel {
            L2CAP_CID_ATT => {
                if pdu.as_ref().is_empty() {
                    return Err(Error::InvalidValue);
                }
                if matches!(pdu.as_ref()[0], att::ATT_EXCHANGE_MTU_REQ | att::ATT_EXCHANGE_MTU_RSP)
                    && pdu.as_ref().len() != 3
                {
                    return Err(Error::InvalidValue);
                }
                // Handle ATT MTU exchange here since it doesn't strictly require
                // gatt to be enabled.
                let a = att::Att::decode(pdu.as_ref());
                if let Ok(att::Att::Client(AttClient::Request(att::AttReq::ExchangeMtu { mtu }))) = a {
                    let negotiated = self.state.connections.exchange_att_mtu(acl.handle(), mtu);

                    // Advertise our receive capacity, independently of the negotiated minimum.
                    let rsp = att::Att::Server(AttServer::Response(att::AttRsp::ExchangeMtu {
                        mtu: self.state.connections.default_att_mtu(),
                    }));
                    let l2cap = L2capHeader {
                        channel: L2CAP_CID_ATT,
                        length: 3,
                    };

                    let mut packet = pdu.into_inner();
                    let mut w = WriteCursor::new(packet.as_mut());
                    w.write_hci(&l2cap)?;
                    w.write(rsp)?;

                    debug!("[host] agreed att MTU of {}", negotiated);
                    let len = w.len();
                    self.state
                        .connections
                        .try_outbound(acl.handle(), Pdu::new(packet, len))?;
                } else if let Ok(att::Att::Server(AttServer::Response(att::AttRsp::ExchangeMtu { mtu }))) = a {
                    #[cfg(feature = "gatt")]
                    self.state.connections.accept_att_response(acl.handle(), pdu.as_ref())?;
                    debug!("[host] remote agreed att MTU of {}", mtu);
                    self.state.connections.exchange_att_mtu(acl.handle(), mtu);
                    #[cfg(feature = "gatt")]
                    self.state.connections.post_gatt_client(acl.handle(), pdu)?;
                } else {
                    #[cfg(feature = "gatt")]
                    match a {
                        Ok(att::Att::Client(AttClient::Confirmation(_))) => {
                            self.state.connections.signal_indication_confirmation(acl.handle());
                        }
                        Ok(att::Att::Client(_)) => {
                            self.state.connections.post_gatt(acl.handle(), pdu)?;
                        }
                        Ok(att::Att::Server(AttServer::Unsolicited(att::AttUns::Indicate { .. }))) => {
                            // Per BLE spec, ATT_HANDLE_VALUE_CFM must always be sent in response
                            // to an indication, regardless of whether a GATT client task is running.
                            let cfm = att::Att::Client(AttClient::Confirmation(crate::att::AttCfm::ConfirmIndication));
                            let l2cap = L2capHeader {
                                channel: L2CAP_CID_ATT,
                                length: cfm.size() as u16,
                            };
                            let mut buf = P::allocate().ok_or(Error::OutOfMemory)?;
                            let mut w = WriteCursor::new(buf.as_mut());
                            w.write_hci(&l2cap)?;
                            w.write(cfm)?;
                            let len = w.len();
                            self.state.connections.try_outbound(acl.handle(), Pdu::new(buf, len))?;

                            // Confirming a value does not permit losing it locally.
                            self.state.connections.post_gatt_client(acl.handle(), pdu)?;
                        }
                        Ok(att::Att::Server(AttServer::Response(_))) => {
                            self.state.connections.accept_att_response(acl.handle(), pdu.as_ref())?;
                            self.state.connections.post_gatt_client(acl.handle(), pdu)?;
                        }
                        Ok(att::Att::Server(AttServer::Unsolicited(_))) => {
                            self.state.connections.post_gatt_client(acl.handle(), pdu)?;
                        }
                        Err(e) => {
                            warn!("Error decoding attribute payload: {:?}", e);
                            return Err(Error::InvalidValue);
                        }
                    }
                    #[cfg(not(feature = "gatt"))]
                    {
                        if let Ok(att::Att::Client(AttClient::Command(_) | AttClient::Confirmation(_))) = a {
                            // Commands and confirmations never elicit an error response.
                            return Ok(());
                        } else if let Ok(att::Att::Client(AttClient::Request(_))) = a {
                            drop(a);

                            let opcode = pdu.as_ref()[0];
                            let rsp = att::Att::Server(AttServer::Response(att::AttRsp::Error {
                                request: opcode,
                                handle: 0,
                                code: att::AttErrorCode::REQUEST_NOT_SUPPORTED,
                            }));

                            let mut packet = pdu.into_inner();
                            let mut w = WriteCursor::new(packet.as_mut());

                            let l2cap = L2capHeader {
                                channel: L2CAP_CID_ATT,
                                length: rsp.size() as u16,
                            };

                            w.write_hci(&l2cap)?;
                            w.write(rsp)?;

                            let len = w.len();
                            self.state
                                .connections
                                .try_outbound(acl.handle(), Pdu::new(packet, len))?;
                            warn!("[host] got attribute request but 'gatt' feature is not enabled.");
                            return Ok(());
                        } else {
                            warn!("Got unsupported ATT: {:?}", a);
                            return Err(Error::NotSupported);
                        }
                    }
                }
            }
            L2CAP_CID_LE_U_SIGNAL => {
                self.state
                    .channels
                    .signal(handle, pdu.as_ref(), &self.state.connections)?;
            }
            L2CAP_CID_LE_U_SECURITY_MANAGER => {
                self.state
                    .connections
                    .handle_security_channel(acl.handle(), pdu, event_handler)?;
            }
            other if other >= L2CAP_CID_DYN_START => match self.state.channels.dispatch(handle, header.channel, pdu) {
                Ok(_) => {}
                Err(e) => {
                    warn!("Error dispatching l2cap packet to channel: {:?}", e);
                    return Err(e);
                }
            },
            chan => {
                debug!(
                    "[host] conn {:?} attempted to use unsupported l2cap channel {}, ignoring",
                    acl.handle(),
                    chan
                );
                return Ok(());
            }
        }
        Ok(())
    }

    // Send l2cap signal payload
    /// Encodes `signal` and `identifier` into caller scratch `p_buf` for current `conn`, then sends under exclusive ACL ownership. Returns encode/admission/controller errors; cancellation may leave a partially submitted PDU.
    pub(crate) async fn l2cap_signal<D: L2capSignal>(
        &self,
        conn: ConnHandle,
        identifier: u8,
        signal: &D,
        p_buf: &mut [u8],
    ) -> Result<(), BleHostError<T::Error>> {
        // trace!(
        //    "[l2cap] sending control signal (req = {}) signal: {:?}",
        //    identifier,
        //    signal
        //);
        let header = L2capSignalHeader {
            identifier,
            code: D::code(),
            length: signal.size() as u16,
        };
        let l2cap = L2capHeader {
            channel: D::channel(),
            length: header.size() as u16 + header.length,
        };

        let mut w = WriteCursor::new(p_buf);
        w.write_hci(&l2cap)?;
        w.write_hci(&header)?;
        w.write_hci(signal)?;

        let mut sender = self.l2cap_pdu(conn).await?;
        sender.send(w.finish()).await?;

        Ok(())
    }

    // Request to send a single L2CAP PDU of pdu_len to the HCI controller for a connection.
    //
    // This function will acquire the connection ACL send lock and the returned sender will handle ACL fragmentation.
    //
    // This function cannot be used to send an SDU split among multiple k-frames.
    /// Returns a sender for current live `handle`, capturing slot identity before any await. Waits for initialization/ACL ownership; NotFound, Disconnected and NoPermits propagate. Cancellation releases retained identity without controller rollback.
    pub(crate) async fn l2cap_pdu(&self, handle: ConnHandle) -> Result<L2capSender<'_, T, P>, BleHostError<T::Error>> {
        let lease = self.state.connections.retain_handle(handle)?;
        self.l2cap_pdu_for_lease(&lease).await
    }

    /// Returns an ACL sender retaining the exact slot in `lease`, waiting for initialization/serialization; closed slots return Disconnected, count/zero-buffer admission returns NoPermits. Cancellation releases only newly acquired ownership.
    async fn l2cap_pdu_for_lease(
        &self,
        lease: &crate::connection_manager::IoLease<'d, P>,
    ) -> Result<L2capSender<'d, T, P>, BleHostError<T::Error>> {
        let initial_state = self.state.initialized.get().await;
        let acl_max = initial_state.acl_max as u16;
        if acl_max == 0 {
            return Err(Error::NoPermits.into());
        }
        let acl_send_lock = poll_fn(|cx| lease.poll_acquire(Some(cx))).await?;
        let handle = lease.handle();
        Ok(L2capSender {
            controller: self.controller,
            handle,
            acl_send_lock,
            fragment_size: acl_max,
            max_fragments: initial_state.acl_total,
        })
    }

    // Request to send an L2CAP SDU of sdu_len to the HCI controller for a connection.
    //
    // This function will acquire the connection ACL send lock and the returned sender will handle ACL fragmentation.
    /// Returns an immediate sender for live `handle` if `sdu_len` plus prefix at nonzero `mps` fits current ACL credits/capacity; returns NoPermits/Busy/link errors. Internal caller supplies validated MPS; zero would divide by zero.
    pub(crate) fn try_l2cap_sdu(
        &self,
        handle: ConnHandle,
        sdu_len: u16,
        mps: u16,
    ) -> Result<L2capSender<'_, T, P>, BleHostError<T::Error>> {
        let initial_state = self.state.initialized.try_get().ok_or(Error::NoPermits)?;
        let acl_max = initial_state.acl_max;
        if acl_max == 0 {
            return Err(Error::NoPermits.into());
        }

        let acl_send_lock = match self.state.connections.poll_acquire_acl_send_lock(handle, None) {
            Poll::Ready(res) => res?,
            Poll::Pending => {
                return Err(Error::Busy.into());
            }
        };

        /// Four-byte channel/length header repeated for each K-frame.
        const L2CAP_BASIC_HEADER_SIZE: usize = 4; // L2CAP Basic Header added to each k-frame
        /// Two-byte SDU length prepended only to the first K-frame.
        const L2CAP_SDU_LEN_SIZE: usize = 2; // L2CAP SDU Length field added to first k-frame

        let mps = usize::from(mps);
        let len = usize::from(sdu_len) + L2CAP_SDU_LEN_SIZE;
        let full_k_frame_len = mps + L2CAP_BASIC_HEADER_SIZE;
        let full_k_frames = len / mps;
        let last_k_frame_len = if len.is_multiple_of(mps) {
            0
        } else {
            len % mps + L2CAP_BASIC_HEADER_SIZE
        };

        let n_acl = full_k_frames * full_k_frame_len.div_ceil(acl_max) + last_k_frame_len.div_ceil(acl_max);
        if n_acl > initial_state.acl_total {
            return Err(Error::NoPermits.into());
        } else if !acl_send_lock.has_link_credits(n_acl) {
            return Err(Error::Busy.into());
        }

        Ok(L2capSender {
            controller: self.controller,
            handle,
            acl_send_lock,
            fragment_size: acl_max as u16,
            max_fragments: initial_state.acl_total,
        })
    }

    /// Sends `param` for live `handle` through the channel signaling path; returns admission/encoding/controller errors and may partially submit before cancellation.
    pub(crate) async fn send_conn_param_update_req(
        &self,
        handle: ConnHandle,
        param: &ConnParamUpdateReq,
    ) -> Result<(), BleHostError<T::Error>> {
        self.state
            .channels
            .send_conn_param_update_req(handle, self, param)
            .await
    }

    /// Sends `param` under peer `identifier` for live `handle`; returns admission/encoding/controller errors with ordinary partial-send cancellation semantics.
    pub(crate) async fn send_conn_param_update_res(
        &self,
        handle: ConnHandle,
        identifier: u8,
        param: &ConnParamUpdateRes,
    ) -> Result<(), BleHostError<T::Error>> {
        self.state
            .channels
            .send_conn_param_update_res(handle, identifier, self, param)
            .await
    }

    /// Read current host metrics
    /// Returns callback `f` result under a shared metrics borrow; reentrant metrics mutation panics.
    pub(crate) fn metrics<F: FnOnce(&HostMetrics) -> R, R>(&self, f: F) -> R {
        let m = self.state.metrics.borrow();
        f(&m)
    }

    /// Log status information of the host
    /// Logs host counters and manager state; `verbose` includes per-link/per-channel diagnostics. Logging features control emission; borrow conflicts panic.
    pub(crate) fn log_status(&self, verbose: bool) {
        let m = self.state.metrics.borrow();
        debug!("[host] connect events: {}", m.connect_events);
        debug!("[host] disconnect events: {}", m.disconnect_events);
        debug!("[host] rx errors: {}", m.rx_errors);
        self.state.connections.log_status(verbose);
        self.state.channels.log_status(verbose);
    }
}

/// Runs the host with the given controller.
pub struct Runner<'d, C, P: PacketPool> {
    /// Receive dispatcher sharing this host state.
    rx: RxRunner<'d, C, P>,
    /// Controller initialization/cancellation/security worker.
    control: ControlRunner<'d, C, P>,
    /// Queued outbound PDU worker.
    tx: TxRunner<'d, C, P>,
}

/// The receiver part of the host runner.
pub struct RxRunner<'d, C, P: PacketPool> {
    /// Shared controller/state view; this worker owns no separate connection table.
    host: BleHost<'d, C, P>,
}

/// The control part of the host runner.
pub struct ControlRunner<'d, C, P: PacketPool> {
    /// Shared controller/state view; this worker owns no separate connection table.
    host: BleHost<'d, C, P>,
}

/// The transmit part of the host runner.
pub struct TxRunner<'d, C, P: PacketPool> {
    /// Shared controller/state view; this worker owns no separate connection table.
    host: BleHost<'d, C, P>,
}

/// Event handler.
/// Synchronous receive-loop callbacks; borrowed packets/iterators expire after the callback. Default handlers discard events; blocking work stalls all receive dispatch.
pub trait EventHandler {
    /// Handle vendor events
    /// Receives borrowed `vendor` event; default drops it.
    fn on_vendor(&self, vendor: &Vendor) {}
    /// Handle advertising reports
    #[cfg(feature = "scan")]
    /// Receives borrowed legacy `reports`; per-report decode errors remain in the iterator. Default discards it.
    fn on_adv_reports(&self, reports: bt_hci::param::LeAdvReportsIter) {}
    /// Handle a complete legacy advertising-report batch, including its borrowed bytes.
    /// The default preserves handlers that only implement the iterator callback.
    #[cfg(feature = "scan")]
    /// Receives complete borrowed legacy `reports`, preserving raw batch bytes; default delegates its iterator to on_adv_reports.
    fn on_adv_report_data(&self, reports: bt_hci::param::LeAdvReports<'_>) {
        self.on_adv_reports(reports.iter());
    }
    /// Handle extended advertising reports
    #[cfg(feature = "scan")]
    /// Receives borrowed extended `reports`; default discards them and does not assemble chained advertising payloads.
    fn on_ext_adv_reports(&self, reports: bt_hci::param::LeExtAdvReportsIter) {}
    /// Reports `_num_completed` controller-completed ACL packets for one decoded handle entry; the handle itself is not passed. Default ignores it. Counts represent reclaimed HCI buffer credits, may include flushed packets, and do not establish peer/application receipt or exact connection-event timing.
    /// See [HCI flow control and completion events](https://www.bluetooth.com/wp-content/uploads/Files/Specification/HTML/Core-60/out/en/host-controller-interface/host-controller-interface-functional-specification.html), sections4.1/4.3/7.7.19.
    fn on_packets_completed(&self, _num_completed: usize) {}

    /// Handle an LE CIS Request event
    #[cfg(feature = "iso")]
    /// Receives borrowed `_event` requesting CIS establishment; default performs no acceptance/rejection.
    fn on_cis_request(&self, _event: &LeCisRequest) {}
    /// Handle an LE CIS Established event
    #[cfg(feature = "iso")]
    /// Receives borrowed `_event` carrying CIS establishment status; default ignores it.
    fn on_cis_established(&self, _event: &LeCisEstablished) {}
    /// Handle an incoming HCI ISO data packet
    #[cfg(feature = "iso")]
    /// Receives borrowed ISO `_packet`; default drops its payload.
    fn on_iso_data(&self, _packet: &IsoPacket<'_>) {}
}

/// Default callback implementation discarding optional events.
struct DummyHandler;
impl EventHandler for DummyHandler {}

impl<'d, C: Controller, P: PacketPool> Runner<'d, C, P> {
    /// Returns receive/control/transmit runners borrowing the same `host`; no tasks or controller operations start.
    pub(crate) fn new(host: BleHost<'d, C, P>) -> Self {
        Self {
            rx: RxRunner { host },
            control: ControlRunner { host },
            tx: TxRunner { host },
        }
    }

    /// Split the runner into separate independent async tasks
    /// Returns independently pollable RX/control/TX workers. All must progress on the same executor; splitting does not add synchronization.
    pub fn split(self) -> (RxRunner<'d, C, P>, ControlRunner<'d, C, P>, TxRunner<'d, C, P>) {
        (self.rx, self.control, self.tx)
    }

    /// Run the host.
    /// Runs RX/control/TX together with discarded optional events; returns the first worker error and cancels the others. Cancellation/failure does not reset controller state; rebuild/recovery belongs to the owner.
    pub async fn run(&mut self) -> Result<(), BleHostError<C::Error>>
    where
        C: ControllerCmdSync<Disconnect>
            + ControllerCmdSync<SetEventMask>
            + ControllerCmdSync<SetEventMaskPage2>
            + ControllerCmdSync<LeSetEventMask>
            + ControllerCmdSync<LeSetRandomAddr>
            + ControllerCmdSync<HostBufferSize>
            + ControllerCmdAsync<LeConnUpdate>
            + ControllerCmdSync<LeReadFilterAcceptListSize>
            + ControllerCmdSync<SetControllerToHostFlowControl>
            + ControllerCmdSync<Reset>
            + ControllerCmdSync<LeCreateConnCancel>
            + ControllerCmdSync<LeSetScanEnable>
            + ControllerCmdSync<LeSetExtScanEnable>
            + ControllerCmdSync<LeSetAdvEnable>
            + for<'t> ControllerCmdSync<LeSetExtAdvEnable<'t>>
            + for<'t> ControllerCmdSync<HostNumberOfCompletedPackets<'t>>
            + ControllerCmdSync<LeReadBufferSize>
            + ControllerCmdSync<ReadBdAddr>
            + crate::SecurityCmds
            + crate::IsoStreamCmds
            + crate::SubratingCmds
            + crate::ShortConnIntervalCmds,
        C::Error: crate::fmt::Format,
    {
        let dummy = DummyHandler;
        self.run_with_handler(&dummy).await
    }

    /// Run the host with a vendor event handler for custom events.
    /// Runs all three workers using borrowed `event_handler`; returns the first completed worker result and cancels siblings. Handler callbacks run synchronously in RX; cancellation requires owner-managed controller recovery.
    pub async fn run_with_handler<E: EventHandler>(&mut self, event_handler: &E) -> Result<(), BleHostError<C::Error>>
    where
        C: ControllerCmdSync<Disconnect>
            + ControllerCmdSync<SetEventMask>
            + ControllerCmdSync<SetEventMaskPage2>
            + ControllerCmdSync<LeSetEventMask>
            + ControllerCmdSync<LeSetRandomAddr>
            + ControllerCmdSync<LeReadFilterAcceptListSize>
            + ControllerCmdSync<HostBufferSize>
            + ControllerCmdAsync<LeConnUpdate>
            + ControllerCmdSync<SetControllerToHostFlowControl>
            + ControllerCmdSync<LeSetAdvEnable>
            + for<'t> ControllerCmdSync<LeSetExtAdvEnable<'t>>
            + for<'t> ControllerCmdSync<HostNumberOfCompletedPackets<'t>>
            + ControllerCmdSync<LeSetScanEnable>
            + ControllerCmdSync<LeSetExtScanEnable>
            + ControllerCmdSync<Reset>
            + ControllerCmdSync<LeCreateConnCancel>
            + ControllerCmdSync<LeReadBufferSize>
            + ControllerCmdSync<ReadBdAddr>
            + crate::SecurityCmds
            + crate::IsoStreamCmds
            + crate::SubratingCmds
            + crate::ShortConnIntervalCmds,
        C::Error: crate::fmt::Format,
    {
        let control_fut = self.control.run();
        let rx_fut = self.rx.run_with_handler(event_handler);
        let tx_fut = self.tx.run();
        pin_mut!(control_fut, rx_fut, tx_fut);
        match select3(&mut tx_fut, &mut rx_fut, &mut control_fut).await {
            Either3::First(result) => {
                trace!("[host] tx_fut exit");
                result
            }
            Either3::Second(result) => {
                trace!("[host] rx_fut exit");
                result
            }
            Either3::Third(result) => {
                trace!("[host] control_fut exit");
                result
            }
        }
    }
}

impl<'d, C: Controller, P: PacketPool> RxRunner<'d, C, P> {
    /// Run the receive loop that polls the controller for events.
    /// Runs receive dispatch with discarded optional events; returns transport/allocation/envelope/hardware/security errors. ACL processing errors normally count and request close while the loop continues. Cancellation stops receiving without resetting links.
    pub async fn run(&mut self) -> Result<(), BleHostError<C::Error>>
    where
        C: ControllerCmdSync<Disconnect>,
    {
        let dummy = DummyHandler;
        self.run_with_handler(&dummy).await
    }

    /// Dispatches borrowed receive bytes to `event_handler`; returns terminal transport/decode/hardware/security errors, while ACL errors request close and increment metrics. Unknown events are ignored. Callback blocking/panic affects this loop; cancellation performs no controller cleanup.
    pub async fn run_with_handler<E: EventHandler>(&mut self, event_handler: &E) -> Result<(), BleHostError<C::Error>>
    where
        C: ControllerCmdSync<Disconnect>,
    {
        let host = &self.host;
        // use embassy_time::Instant;
        // let mut last = Instant::now();
        loop {
            // Task handling receiving data from the controller.
            let mut rx = host.controller.alloc_buf().map_err(BleHostError::Controller)?;
            // let now = Instant::now();
            // let elapsed = (now - last).as_millis();
            // if elapsed >= 1 {
            //     trace!("[host] time since last poll was {} us", elapsed);
            // }
            let result = host.controller.read(&mut rx).await;
            // last = Instant::now();
            //        trace!("[host] polling took {} ms", (polled - started).as_millis());
            match result {
                Ok(ControllerToHostPacket::Acl(acl)) => match host.handle_acl(acl, event_handler) {
                    Ok(_) => {}
                    Err(e) => {
                        warn!(
                            "[host] encountered error processing ACL data for {:?}: {:?}",
                            acl.handle(),
                            e
                        );

                        match e {
                            Error::InvalidState | Error::Disconnected => {
                                warn!("[host] requesting {:?} to be disconnected", acl.handle());
                                host.state.connections.log_status(true);
                                host.state.connections.request_handle_disconnect(
                                    acl.handle(),
                                    DisconnectReason::RemoteUserTerminatedConn,
                                );
                            }
                            _ => {}
                        }

                        let mut m = host.state.metrics.borrow_mut();
                        m.rx_errors = m.rx_errors.wrapping_add(1);
                    }
                },
                Ok(ControllerToHostPacket::Event(event)) => {
                    match event.kind {
                        EventKind::Le => {
                            let event = match LeEventPacket::from_hci_bytes_complete(event.data) {
                                Ok(event) => event,
                                // The envelope decoder only validates the subevent byte.
                                // Unsupported subevents must be ignored (HCI 5.4.4).
                                Err(bt_hci::FromHciBytesError::InvalidValue) => continue,
                                Err(error) => return Err(Error::HciDecode(error).into()),
                            };
                            match event.kind {
                                LeEventKind::LeConnectionComplete => {
                                    // Fixed fields after the subevent: 18 octets.
                                    if host.handle_failed_connection_complete(event.data, 18)? {
                                        continue;
                                    }
                                    let e = LeConnectionComplete::from_hci_bytes_complete(event.data)
                                        .map_err(Error::from)?;
                                    if !host.handle_connection(
                                        e.status,
                                        e.handle,
                                        Address::new(e.peer_addr_kind, e.peer_addr),
                                        e.role,
                                        ConnParams {
                                            conn_interval: Duration::from_micros(e.conn_interval.as_micros()),
                                            peripheral_latency: e.peripheral_latency,
                                            supervision_timeout: Duration::from_micros(
                                                e.supervision_timeout.as_micros(),
                                            ),
                                        },
                                        #[cfg(feature = "security")]
                                        ResolvablePrivateAddrs::none(),
                                    ) {
                                        // RX cannot await an HCI command whose response it
                                        // must itself read. No slot to quarantine this handle:
                                        // fail the host so the owner resets the controller.
                                        return Err(Error::OutOfMemory.into());
                                    }
                                }
                                LeEventKind::LeEnhancedConnectionComplete => {
                                    // v1 adds two six-octet resolvable addresses.
                                    if host.handle_failed_connection_complete(event.data, 30)? {
                                        continue;
                                    }
                                    let e = LeEnhancedConnectionComplete::from_hci_bytes_complete(event.data)
                                        .map_err(Error::from)?;
                                    if !host.handle_connection(
                                        e.status,
                                        e.handle,
                                        Address::new(e.peer_addr_kind, e.peer_addr),
                                        e.role,
                                        ConnParams {
                                            conn_interval: Duration::from_micros(e.conn_interval.as_micros()),
                                            peripheral_latency: e.peripheral_latency,
                                            supervision_timeout: Duration::from_micros(
                                                e.supervision_timeout.as_micros(),
                                            ),
                                        },
                                        #[cfg(feature = "security")]
                                        ResolvablePrivateAddrs {
                                            local: Some(e.local_resolvable_private_addr).filter(|a| *a.raw() != [0; 6]),
                                            peer: Some(e.peer_resolvable_private_addr).filter(|a| *a.raw() != [0; 6]),
                                        },
                                    ) {
                                        // RX cannot await an HCI command whose response it
                                        // must itself read. No slot to quarantine this handle:
                                        // fail the host so the owner resets the controller.
                                        return Err(Error::OutOfMemory.into());
                                    }
                                }
                                LeEventKind::LeScanTimeout => {
                                    bt_hci::event::le::LeScanTimeout::from_hci_bytes_complete(event.data)
                                        .map_err(Error::from)?;
                                    #[cfg(feature = "scan")]
                                    host.state.scan_timeout.signal(());
                                }
                                LeEventKind::LeAdvertisingSetTerminated => {
                                    let set = LeAdvertisingSetTerminated::from_hci_bytes_complete(event.data)
                                        .map_err(Error::from)?;
                                    host.state.advertise_state.terminate(set.adv_handle);
                                }
                                LeEventKind::LeExtendedAdvertisingReport => {
                                    #[cfg(feature = "scan")]
                                    {
                                        let data = LeExtendedAdvertisingReport::from_hci_bytes_complete(event.data)
                                            .map_err(Error::from)?;
                                        event_handler.on_ext_adv_reports(data.reports.iter());
                                    }
                                }
                                LeEventKind::LeAdvertisingReport => {
                                    #[cfg(feature = "scan")]
                                    {
                                        let data = LeAdvertisingReport::from_hci_bytes_complete(event.data)
                                            .map_err(Error::from)?;
                                        event_handler.on_adv_report_data(data.reports);
                                    }
                                }
                                LeEventKind::LeLongTermKeyRequest => {
                                    host.state.connections.handle_security_hci_le_event(event)?;
                                }
                                LeEventKind::LePhyUpdateComplete => {
                                    let event = LePhyUpdateComplete::from_hci_bytes_complete(event.data)
                                        .map_err(Error::from)?;
                                    if let Err(e) = event.status.to_result() {
                                        warn!("[host] error updating phy for {:?}: {:?}", event.handle, e);
                                    } else {
                                        let _ = host.state.connections.post_handle_event(
                                            event.handle,
                                            ConnectionEvent::PhyUpdated {
                                                tx_phy: event.tx_phy,
                                                rx_phy: event.rx_phy,
                                            },
                                        );
                                    }
                                }
                                LeEventKind::LeConnectionUpdateComplete => {
                                    let event = LeConnectionUpdateComplete::from_hci_bytes_complete(event.data)
                                        .map_err(Error::from)?;
                                    if let Err(e) = event.status.to_result() {
                                        warn!(
                                            "[host] error updating connection parameters for {:?}: {:?}",
                                            event.handle, e
                                        );
                                    } else {
                                        let _ = host.state.connections.post_handle_event(
                                            event.handle,
                                            ConnectionEvent::ConnectionParamsUpdated {
                                                conn_interval: Duration::from_micros(event.conn_interval.as_micros()),
                                                peripheral_latency: event.peripheral_latency,
                                                supervision_timeout: Duration::from_micros(
                                                    event.supervision_timeout.as_micros(),
                                                ),
                                            },
                                        );
                                    }
                                }
                                LeEventKind::LeDataLengthChange => {
                                    let event =
                                        LeDataLengthChange::from_hci_bytes_complete(event.data).map_err(Error::from)?;
                                    let _ = host.state.connections.post_handle_event(
                                        event.handle,
                                        ConnectionEvent::DataLengthUpdated {
                                            max_tx_octets: event.max_tx_octets,
                                            max_tx_time: event.max_tx_time,
                                            max_rx_octets: event.max_rx_octets,
                                            max_rx_time: event.max_rx_time,
                                        },
                                    );
                                }
                                LeEventKind::LeFrameSpaceUpdateComplete => {
                                    let event = LeFrameSpaceUpdateComplete::from_hci_bytes_complete(event.data)
                                        .map_err(Error::from)?;
                                    if let Err(e) = event.status.to_result() {
                                        warn!("[host] error updating frame space for {:?}: {:?}", event.handle, e);
                                    } else {
                                        let _ = host.state.connections.post_handle_event(
                                            event.handle,
                                            ConnectionEvent::FrameSpaceUpdated {
                                                frame_space: Duration::from_micros(event.frame_space.as_micros()),
                                                initiator: event.initiator,
                                                phys: event.phys,
                                                spacing_types: event.spacing_types,
                                            },
                                        );
                                    }
                                }
                                #[cfg(feature = "subrating")]
                                LeEventKind::LeSubrateChange => {
                                    let event =
                                        LeSubrateChange::from_hci_bytes_complete(event.data).map_err(Error::from)?;
                                    if let Err(e) = event.status.to_result() {
                                        warn!("[host] error in subrate change for {:?}: {:?}", event.handle, e);
                                    } else {
                                        let _ = host.state.connections.post_handle_event(
                                            event.handle,
                                            ConnectionEvent::SubratingParamsUpdated {
                                                subrate_factor: event.subrate_factor,
                                                peripheral_latency: event.peripheral_latency,
                                                continuation_number: event.continuation_number,
                                                supervision_timeout: Duration::from_micros(
                                                    event.supervision_timeout.as_micros(),
                                                ),
                                            },
                                        );
                                    }
                                }
                                LeEventKind::LeConnectionRateChange => {
                                    let event = LeConnectionRateChange::from_hci_bytes_complete(event.data)
                                        .map_err(Error::from)?;
                                    if let Err(e) = event.status.to_result() {
                                        warn!("[host] error in connection rate change for {:?}: {:?}", event.handle, e);
                                    } else {
                                        let _ = host.state.connections.post_handle_event(
                                            event.handle,
                                            ConnectionEvent::ConnectionRateChanged {
                                                conn_interval: Duration::from_micros(event.conn_interval.as_micros()),
                                                subrate_factor: event.subrate_factor,
                                                peripheral_latency: event.peripheral_latency,
                                                continuation_number: event.continuation_number,
                                                supervision_timeout: Duration::from_micros(
                                                    event.supervision_timeout.as_micros(),
                                                ),
                                            },
                                        );
                                    }
                                }
                                LeEventKind::LeRemoteConnectionParameterRequest => {
                                    let event = LeRemoteConnectionParameterRequest::from_hci_bytes_complete(event.data)
                                        .map_err(Error::from)?;
                                    #[cfg(not(feature = "connection-params-update"))]
                                    let _ = event;
                                    #[cfg(feature = "connection-params-update")]
                                    let req = ConnectionParamsRequest::new_link_layer(
                                        RequestedConnParams {
                                            min_connection_interval: Duration::from_micros(
                                                event.interval_min.as_micros(),
                                            ),
                                            max_connection_interval: Duration::from_micros(
                                                event.interval_max.as_micros(),
                                            ),
                                            max_latency: event.max_latency,
                                            supervision_timeout: Duration::from_micros(event.timeout.as_micros()),
                                            ..Default::default()
                                        },
                                        event.handle,
                                    );
                                    #[cfg(feature = "connection-params-update")]
                                    if let Err(error) = host
                                        .state
                                        .connections
                                        .post_handle_event(event.handle, ConnectionEvent::RequestConnectionParams(req))
                                    {
                                        // The peer is awaiting a reply. A full application queue
                                        // must not silently strand a live controller procedure.
                                        host.connections().record_local_disconnect_error(event.handle, &error);
                                        host.connections().request_handle_disconnect(
                                            event.handle,
                                            DisconnectReason::RemoteUserTerminatedConn,
                                        );
                                    }
                                }
                                #[cfg(feature = "iso")]
                                LeEventKind::LeCisRequest => {
                                    let e = LeCisRequest::from_hci_bytes_complete(event.data).map_err(Error::from)?;
                                    event_handler.on_cis_request(&e);
                                }
                                #[cfg(feature = "iso")]
                                LeEventKind::LeCisEstablished => {
                                    let e =
                                        LeCisEstablished::from_hci_bytes_complete(event.data).map_err(Error::from)?;
                                    event_handler.on_cis_established(&e);
                                }
                                _ => {
                                    warn!("Unknown LE event!");
                                }
                            }
                        }
                        EventKind::DisconnectionComplete => {
                            let e = DisconnectionComplete::from_hci_bytes_complete(event.data).map_err(Error::from)?;
                            host.handle_disconnection_complete(e)?;
                        }
                        EventKind::NumberOfCompletedPackets => {
                            let c =
                                NumberOfCompletedPackets::from_hci_bytes_complete(event.data).map_err(Error::from)?;
                            // Explicitly ignoring for now
                            for entry in c.completed_packets.iter() {
                                match (entry.handle(), entry.num_completed_packets()) {
                                    (Ok(handle), Ok(completed)) => {
                                        let _ = host.state.connections.confirm_sent(handle, completed as usize);
                                        event_handler.on_packets_completed(completed as usize);
                                    }
                                    (Ok(handle), Err(e)) => {
                                        warn!("[host] error processing completed packets for {:?}: {:?}", handle, e);
                                    }
                                    _ => {}
                                }
                            }
                        }
                        EventKind::HardwareError => {
                            let fault = HardwareError::from_hci_bytes_complete(event.data).map_err(Error::from)?;
                            return Err(Error::HardwareError(fault.hardware_code).into());
                        }
                        EventKind::Vendor => {
                            let vendor = Vendor::from_hci_bytes_complete(event.data).map_err(Error::from)?;
                            event_handler.on_vendor(&vendor);
                        }
                        EventKind::EncryptionChangeV1
                        | EventKind::EncryptionChangeV2
                        | EventKind::EncryptionKeyRefreshComplete => {
                            host.state.connections.handle_security_hci_event(event)?;
                        }
                        // Ignore
                        _ => {}
                    }
                }
                #[cfg(feature = "iso")]
                Ok(ControllerToHostPacket::Iso(packet)) => {
                    event_handler.on_iso_data(&packet);
                }
                // Ignore
                Ok(_) => {}
                Err(e) => {
                    return Err(BleHostError::Controller(e));
                }
            }
        }
    }
}

/// One prioritized control-loop action, consumed after the select branch wins.
enum CancelledCommandState {
    #[cfg(feature = "central")]
    /// Cancel the active initiator procedure.
    Connect(
        /// Stored extended-procedure flag; current cancellation command ignores it.
        bool,
    ),
    #[cfg(feature = "peripheral")]
    /// Disable active advertising.
    Advertise(
        /// True selects extended advertising disable; false selects legacy.
        bool,
    ),
    #[cfg(feature = "scan")]
    /// Disable active scanning.
    Scan(
        /// True selects extended scan disable; false selects legacy.
        bool,
    ),
    #[cfg(feature = "security")]
    /// Apply an idle-only privacy-list update.
    SyncResolvingList(
        /// Consumed/coalesced pending list mutation.
        ResolvingListUpdate,
    ),
    #[cfg(all(feature = "security", feature = "central"))]
    /// Generate/update an eligible expired private address.
    RotateRpa,
}

impl<'d, C: Controller, P: PacketPool> ControlRunner<'d, C, P> {
    /// Run the control loop for the host
    /// Resets/configures the controller, publishes ACL capacity, then services close/cancel/privacy/security work. Returns unhandled controller/host errors; optional unsupported capabilities are logged. RX must progress for command replies. Cancellation may leave partially applied controller state; rerunning does not reset the once-published host tables.
    pub async fn run(&mut self) -> Result<(), BleHostError<C::Error>>
    where
        C: ControllerCmdSync<Disconnect>
            + ControllerCmdSync<SetEventMask>
            + ControllerCmdSync<SetEventMaskPage2>
            + ControllerCmdSync<LeSetEventMask>
            + ControllerCmdSync<LeSetRandomAddr>
            + ControllerCmdSync<HostBufferSize>
            + ControllerCmdAsync<LeConnUpdate>
            + ControllerCmdSync<LeReadFilterAcceptListSize>
            + ControllerCmdSync<SetControllerToHostFlowControl>
            + ControllerCmdSync<Reset>
            + ControllerCmdSync<LeCreateConnCancel>
            + ControllerCmdSync<LeSetAdvEnable>
            + for<'t> ControllerCmdSync<LeSetExtAdvEnable<'t>>
            + ControllerCmdSync<LeSetScanEnable>
            + ControllerCmdSync<LeSetExtScanEnable>
            + for<'t> ControllerCmdSync<HostNumberOfCompletedPackets<'t>>
            + ControllerCmdSync<LeReadBufferSize>
            + ControllerCmdSync<ReadBdAddr>
            + crate::SecurityCmds
            + crate::IsoStreamCmds
            + crate::SubratingCmds
            + crate::ShortConnIntervalCmds,
        C::Error: crate::fmt::Format,
    {
        let host = &self.host;
        Reset::new().exec(host.controller).await?;

        #[cfg(feature = "security")]
        {
            let mut seed = [0u8; 32];
            for chunk in seed.chunks_mut(8) {
                let bytes: [u8; 8] = LeRand::new().exec(host.controller).await?;
                chunk.copy_from_slice(&bytes);
            }
            host.state.connections.security_manager.set_random_generator_seed(seed);
        }

        {
            let addr = host.state.address.map(|a| a.addr);

            #[cfg(all(feature = "security", feature = "central"))]
            let addr = host.state.connections.security_manager.generate_local_rpa()?.or(addr);

            if let Some(addr) = addr {
                LeSetRandomAddr::new(addr).exec(host.controller).await?;

                #[cfg(all(feature = "security", feature = "central"))]
                if host.is_privacy_enabled() {
                    host.state
                        .rpa_expires_at
                        .set(Instant::now() + host.state.rpa_timeout.get());
                }
            }
        }

        SetEventMask::new(
            EventMask::new()
                .enable_le_meta(true)
                .enable_conn_request(true)
                .enable_conn_complete(true)
                .enable_hardware_error(true)
                .enable_disconnection_complete(true)
                .enable_encryption_change_v1(true)
                .enable_encryption_key_refresh_complete(true),
        )
        .exec(host.controller)
        .await?;

        if let Err(e) = SetEventMaskPage2::new(EventMaskPage2::new().enable_encryption_change_v2(true))
            .exec(host.controller)
            .await
        {
            match e {
                cmd::Error::Hci(bt_hci::param::Error::UNKNOWN_CMD) => warn!("set event mask page 2 is not supported"),
                e => Err(e)?,
            }
        }

        let mask = LeEventMask::new()
            .enable_le_conn_complete(true)
            .enable_le_enhanced_conn_complete_v1(true)
            .enable_le_conn_update_complete(true)
            .enable_le_adv_set_terminated(true)
            .enable_le_adv_report(true)
            .enable_le_scan_timeout(true)
            .enable_le_ext_adv_report(true)
            .enable_le_long_term_key_request(true)
            .enable_le_phy_update_complete(true)
            .enable_le_data_length_change(true);

        #[cfg(feature = "subrating")]
        let mask = mask.enable_le_subrate_change(true);

        #[cfg(feature = "iso")]
        let mask = mask.enable_le_cis_established_v1(true).enable_le_cis_request(true);

        #[cfg(feature = "connection-params-update")]
        let mask = mask.enable_le_remote_conn_parameter_request(true);

        #[cfg(feature = "shorter-connection-intervals")]
        let mask = mask.enable_le_connection_rate_change(true);

        LeSetEventMask::new(mask).exec(host.controller).await?;

        // Without the Connection Isochronous Stream (Host Support) bit set, a peer central is not allowed to
        // create or accept a CIS.
        #[cfg(feature = "iso")]
        {
            /// Host feature bit32 advertising connected isochronous stream support.
            const LE_FEATURE_CIS_HOST: u8 = 32;
            if let Err(e) = LeSetHostFeature::new(LE_FEATURE_CIS_HOST, 1)
                .exec(host.controller)
                .await
            {
                match e {
                    cmd::Error::Hci(bt_hci::param::Error::UNSUPPORTED | bt_hci::param::Error::UNKNOWN_CMD) => {
                        warn!("[host] connection isochronous streams are not supported")
                    }
                    e => Err(e)?,
                }
            }
        }

        // Without the Connection Subrating (Host Support) bit set, a peer central is not allowed to
        // start the Connection Subrate Update procedure on us.
        #[cfg(feature = "subrating")]
        {
            /// Host feature bit38 permitting connection subrating procedures.
            const LE_FEATURE_CONN_SUBRATING_HOST: u8 = 38;
            if let Err(e) = LeSetHostFeature::new(LE_FEATURE_CONN_SUBRATING_HOST, 1)
                .exec(host.controller)
                .await
            {
                match e {
                    cmd::Error::Hci(bt_hci::param::Error::UNSUPPORTED | bt_hci::param::Error::UNKNOWN_CMD) => {
                        warn!("[host] connection subrating is not supported")
                    }
                    e => Err(e)?,
                }
            }
        }

        // Without the Shorter Connection Intervals (Host Support) bit set, a peer central is not allowed to
        // start the Connection Rate Change procedure on us.
        #[cfg(feature = "shorter-connection-intervals")]
        {
            /// Host feature bit73 permitting shorter connection intervals.
            const LE_FEATURE_SHORTER_CONNECTION_INTERVALS_HOST: u16 = 73;
            if let Err(e) = LeSetHostFeatureV2::new(LE_FEATURE_SHORTER_CONNECTION_INTERVALS_HOST, 1)
                .exec(host.controller)
                .await
            {
                match e {
                    cmd::Error::Hci(bt_hci::param::Error::UNSUPPORTED | bt_hci::param::Error::UNKNOWN_CMD) => {
                        warn!("[host] shorter connection intervals are not supported")
                    }
                    e => Err(e)?,
                }
            }
        }

        info!(
            "[host] using packet pool with MTU {} capacity {}",
            P::MTU,
            P::capacity(),
        );

        let ret = LeReadFilterAcceptListSize::new().exec(host.controller).await?;
        info!("[host] filter accept list size: {}", ret);

        let ret = LeReadBufferSize::new().exec(host.controller).await?;
        info!(
            "[host] setting txq to {}, fragmenting at {}",
            ret.total_num_le_acl_data_packets as usize, ret.le_acl_data_packet_length as usize
        );
        host.state
            .connections
            .set_link_credits(ret.total_num_le_acl_data_packets as usize);

        {
            /// Advertised host receive ACL payload capacity, excluding HCI header; controller-to-host flow control remains disabled.
            const ACL_LEN: u16 = 255;
            /// Advertised host ACL packet count; no completed-packet return loop is enabled here.
            const ACL_N: u16 = 1;
            info!(
                "[host] configuring host buffers ({} packets of size {})",
                ACL_N, ACL_LEN,
            );
            if let Err(_e) = HostBufferSize::new(ACL_LEN, 0, ACL_N, 0).exec(host.controller).await {
                warn!("[host] error configuring host buffers (continuing)");
            }
        }

        // #[cfg(feature = "controller-host-flow-control")]
        // {
        // info!("[host] enabling flow control");
        // SetControllerToHostFlowControl::new(ControllerToHostFlowControl::AclOnSyncOff)
        // .exec(&host.controller)
        // .await?;
        // }

        let _ = host.state.initialized.init(InitialState {
            acl_max: ret.le_acl_data_packet_length as usize,
            acl_total: ret.total_num_le_acl_data_packets as usize,
        });
        info!("[host] initialized");

        let device_address = host.command(ReadBdAddr::new()).await?;
        if *device_address.raw() != [0, 0, 0, 0, 0, 0] {
            let device_address = Address::new(AddrKind::PUBLIC, device_address);
            info!("[host] Device Address {}", device_address);
            if host.state.address.is_none() {
                #[cfg(feature = "security")]
                host.state
                    .connections
                    .security_manager
                    .set_local_address(device_address);
            }
        }

        // Set default RPA timeout in controller
        #[cfg(feature = "security")]
        {
            let timeout_secs = host.state.rpa_timeout.get().as_secs();
            LeSetResolvablePrivateAddrTimeout::new(bt_hci::param::Duration::from_secs(timeout_secs as u32))
                .exec(host.controller)
                .await?;
            info!("[host] RPA timeout set to {}s", timeout_secs);
        }

        // Initialize privacy: sync resolving list
        #[cfg(feature = "security")]
        if host.is_privacy_enabled() {
            host.state.resolving_list_state.borrow_mut().clear();
            host.sync_resolving_list(ResolvingListUpdate::FullSync).await?;
            info!("[host] privacy initialized");
        }

        loop {
            match select5(
                poll_fn(|cx| host.state.connections.poll_disconnecting(Some(cx))),
                poll_fn(|cx| host.state.channels.poll_disconnecting(Some(cx))),
                poll_fn(|cx| host.poll_cancelled(cx)),
                #[cfg(feature = "security")]
                {
                    host.state.connections.poll_security_events()
                },
                #[cfg(not(feature = "security"))]
                {
                    core::future::pending::<()>()
                },
                #[cfg(all(feature = "security", feature = "central"))]
                {
                    host.wait_for_rpa_expiration()
                },
                #[cfg(not(all(feature = "security", feature = "central")))]
                {
                    core::future::pending::<()>()
                },
            )
            .await
            {
                Either5::First(request) => {
                    trace!("[host] poll disconnecting links");
                    match host.command(Disconnect::new(request.handle(), request.reason())).await {
                        Ok(_) => {}
                        Err(BleHostError::BleHost(Error::Hci(bt_hci::param::Error::UNKNOWN_CONN_IDENTIFIER))) => {}
                        Err(BleHostError::BleHost(Error::NotFound)) => {}
                        Err(BleHostError::BleHost(Error::Disconnected)) => {}
                        Err(e) => {
                            return Err(e);
                        }
                    }
                    request.confirm();
                }
                Either5::Second(request) => {
                    trace!("[host] poll disconnecting channels");
                    match request.send(host).await {
                        Ok(_) => {}
                        Err(BleHostError::BleHost(Error::Hci(bt_hci::param::Error::UNKNOWN_CONN_IDENTIFIER))) => {}
                        Err(BleHostError::BleHost(Error::NotFound)) => {}
                        Err(BleHostError::BleHost(Error::Disconnected)) => {}
                        Err(e) => {
                            return Err(e);
                        }
                    }
                    request.confirm();
                }
                Either5::Third(action) => {
                    match action {
                        #[cfg(feature = "central")]
                        CancelledCommandState::Connect(_) => {
                            trace!("[host] cancel connection create");
                            host.state.connect_command_state.cancel_started();
                            if host.state.connections.cancel_unaccepted_central() {
                                host.state.connect_command_state.canceled();
                            } else {
                                // Command Complete does not complete the procedure.
                                // LE Connection Complete must settle the old attempt.
                                host.command(LeCreateConnCancel::new()).await?;
                            }
                        }
                        #[cfg(feature = "peripheral")]
                        CancelledCommandState::Advertise(ext) => {
                            trace!("[host] disabling advertising");
                            #[cfg(feature = "extended-advertising")]
                            if ext {
                                host.command(LeSetExtAdvEnable::new(false, &[])).await?
                            } else {
                                host.command(LeSetAdvEnable::new(false)).await?
                            }
                            #[cfg(not(feature = "extended-advertising"))]
                            {
                                // Extended advertising cannot have been started
                                // (`advertise_ext` is compiled out), so `ext` is
                                // always false here.
                                let _ = ext;
                                host.command(LeSetAdvEnable::new(false)).await?
                            }
                            host.state.advertise_command_state.canceled();
                        }
                        #[cfg(feature = "scan")]
                        CancelledCommandState::Scan(ext) => {
                            trace!("[host] disabling scanning");
                            if ext {
                                // TODO: A bit opinionated but not more than before
                                host.command(LeSetExtScanEnable::new(
                                    false,
                                    FilterDuplicates::Disabled,
                                    bt_hci::param::Duration::from_secs(0),
                                    bt_hci::param::Duration::from_secs(0),
                                ))
                                .await?;
                            } else {
                                host.command(LeSetScanEnable::new(false, false)).await?;
                            }
                            host.state.scan_command_state.canceled();
                        }
                        #[cfg(feature = "security")]
                        CancelledCommandState::SyncResolvingList(update) => {
                            host.sync_resolving_list(update).await?;
                        }
                        #[cfg(all(feature = "security", feature = "central"))]
                        CancelledCommandState::RotateRpa => {
                            host.rotate_rpa().await?;
                        }
                    }
                }
                Either5::Fourth(request) => {
                    #[cfg(feature = "security")]
                    {
                        let event_data = request.unwrap_or(SecurityEventData::Timeout);
                        host.state.connections.handle_security_event(host, event_data).await?;
                    }
                }
                Either5::Fifth(()) => {
                    #[cfg(all(feature = "security", feature = "central"))]
                    {
                        host.rotate_rpa().await?;
                    }
                }
            }
        }
    }
}

impl<'d, C: Controller, P: PacketPool> TxRunner<'d, C, P> {
    /// Run the transmit loop for the host.
    /// Waits for initialization, drains retained outbound PDUs and sends them under exact-slot ACL locks. Closed-link packets are discarded; returns other errors to terminate. Cancellation drops the current PDU/leases but cannot retract submitted fragments.
    pub async fn run(&mut self) -> Result<(), BleHostError<C::Error>> {
        let host = &self.host;
        host.state.initialized.get().await;
        loop {
            let (lease, pdu) = host.state.connections.outbound().await;
            match host.l2cap_pdu_for_lease(&lease).await {
                Ok(mut sender) => match sender.send(pdu.as_ref()).await {
                    Ok(()) => {}
                    Err(BleHostError::BleHost(Error::NotFound)) | Err(BleHostError::BleHost(Error::Disconnected)) => {
                        warn!("[host] link disconnected while sending outbound pdu (ignored)");
                    }
                    Err(e) => {
                        warn!("[host] error sending outbound pdu");
                        return Err(e);
                    }
                },
                Err(BleHostError::BleHost(Error::NotFound)) => {
                    warn!("[host] unable to send data to disconnected host (ignored)");
                }
                Err(BleHostError::BleHost(Error::Disconnected)) => {
                    warn!("[host] unable to send data to disconnected host (ignored)");
                }
                Err(e) => {
                    warn!("[host] error requesting sending outbound pdu");
                    return Err(e);
                }
            }
        }
    }
}

/// Exclusive ACL-fragment sender tied to a retained slot; dropping releases lock/credits but cannot retract submitted bytes.
pub struct L2capSender<'a, T: Controller, P: PacketPool> {
    /// Shared HCI transport used for outgoing ACL fragments.
    pub(crate) controller: &'a T,
    /// Controller handle captured for the retained local connection.
    pub(crate) handle: ConnHandle,
    /// Affine serialization/identity lease spanning every fragment of this send.
    pub(crate) acl_send_lock: AclSendLock<'a, P>,
    /// Positive ACL data byte limit from controller initialization.
    pub(crate) fragment_size: u16,
    /// Controller packet budget limiting immediate whole-SDU admission.
    pub(crate) max_fragments: usize,
}

impl<'a, T: Controller, P: PacketPool> L2capSender<'a, T, P> {
    /// Fragments borrowed `pdu`, reserves all required ACL credits, and returns Ok after controller acceptance. NoPermits/Busy/link/transport errors can follow earlier submitted fragments; unused grants refund on drop. Empty input sends nothing; internal fragment_size must be positive.
    pub(crate) fn try_send(&mut self, pdu: &[u8]) -> Result<(), BleHostError<T::Error>>
    where
        T: blocking::Controller,
    {
        let n_acl = pdu.chunks(self.fragment_size as usize).len();
        if n_acl > self.max_fragments {
            return Err(Error::NoPermits.into());
        }

        let mut grant = match self.acl_send_lock.poll_request_to_send(n_acl, None) {
            Poll::Ready(res) => res?,
            Poll::Pending => return Err(Error::Busy.into()),
        };
        let mut pbf = AclPacketBoundary::FirstNonFlushable;
        // info!(
        //    "[host] fragmenting PDU of size {} into {} sized fragments",
        //    pdu.len(),
        //    self.fragment_size
        //);
        for chunk in pdu.chunks(self.fragment_size as usize) {
            let acl = AclPacket::new(self.handle, pbf, AclBroadcastFlag::PointToPoint, chunk);
            match self.controller.try_write_acl_data(&acl) {
                Ok(_result) => {
                    grant.confirm(1);
                    trace!("[host] sent acl packet len = {}", chunk.len());
                }
                Err(blocking::TryError::Busy) => {
                    warn!("hci: acl data send busy");
                    return Err(Error::Busy.into());
                }
                Err(blocking::TryError::Error(e)) => return Err(BleHostError::Controller(e)),
            }
            pbf = AclPacketBoundary::Continuing;
        }
        Ok(())
    }

    /// Fragments borrowed `pdu`, waiting for one credit per fragment, and returns after transport acceptance or link/transport failure. Checks retained-link liveness before every transport poll and wakes on close; cancellation refunds only unconfirmed credits. Generic transports may already have partial effects when Pending; accepted/queued bytes cannot be retracted and peer delivery is not guaranteed. Selected CYW transport publishes nothing before its capacity wait completes. Empty input sends nothing.
    pub(crate) async fn send(&mut self, pdu: &[u8]) -> Result<(), BleHostError<T::Error>> {
        // info!(
        //    "[host] fragmenting PDU of size {} into {} sized fragments",
        //    pdu.len(),
        //    self.fragment_size
        //);
        let mut pbf = AclPacketBoundary::FirstNonFlushable;
        for chunk in pdu.chunks(self.fragment_size as usize) {
            let mut grant = poll_fn(|cx| self.acl_send_lock.poll_request_to_send(1, Some(cx))).await?;
            let acl = AclPacket::new(self.handle, pbf, AclBroadcastFlag::PointToPoint, chunk);
            let mut write = core::pin::pin!(self.controller.write_acl_data(&acl));
            poll_fn(|cx| {
                self.acl_send_lock.check_live(cx)?;
                write.as_mut().poll(cx).map_err(BleHostError::Controller)
            })
            .await?;
            grant.confirm(1);
            pbf = AclPacketBoundary::Continuing;
            trace!("[host] sent acl packet len = {}", chunk.len());
        }
        Ok(())
    }
}

/// A type to delay the drop handler invocation.
#[must_use = "to delay the drop handler invocation to the end of the scope"]
pub struct OnDrop<F: FnOnce()> {
    /// Initialized one-shot cleanup closure; defuse forgets the entire guard and its captures.
    f: MaybeUninit<F>,
}

impl<F: FnOnce()> OnDrop<F> {
    /// Create a new instance.
    /// Returns an armed guard owning `f`; Drop invokes it once unless defused.
    pub fn new(f: F) -> Self {
        Self { f: MaybeUninit::new(f) }
    }

    /// Prevent drop handler from running.
    /// Consumes/forgets the guard so cleanup and captured-value destructors do not run.
    pub fn defuse(self) {
        core::mem::forget(self)
    }
}

impl<F: FnOnce()> Drop for OnDrop<F> {
    /// Moves the initialized closure out and invokes it exactly once; a panic propagates and can abort if already unwinding.
    fn drop(&mut self) {
        // SAFETY: new initializes f; Drop runs once, while defuse forgets the
        // entire guard. MaybeUninit does not separately drop the moved closure.
        unsafe { self.f.as_ptr().read()() }
    }
}
