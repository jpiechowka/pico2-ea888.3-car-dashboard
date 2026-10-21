//! Retained BLE link handles, controller parameter requests and application/security events.
//!
//! Handles keep table slots from reuse; last drop schedules asynchronous disconnection.
//! The host runner must remain active for commands/events. Command cancellation cannot undo
//! controller-side effects; APIs taking a stack require the originating stack.
//! HCI time conversions truncate to command units and panic when their integer width is exceeded.

use bt_hci::cmd::le::{
    LeConnUpdate, LeConnectionRateRequest, LeFrameSpaceUpdate, LeReadLocalSupportedFeatures, LeReadPhy,
    LeSetDataLength, LeSetPhy,
};
use bt_hci::cmd::status::ReadRssi;
use bt_hci::controller::{ControllerCmdAsync, ControllerCmdSync};
use bt_hci::param::{
    AllPhys, ConnHandle, DisconnectReason, FilterDuplicates, FrameSpaceInitiator, LeConnRole, PhyKind, PhyMask,
    PhyOptions, SpacingTypes, Status,
};
#[cfg(feature = "connection-params-update")]
use bt_hci::{
    cmd::le::{LeRemoteConnectionParameterRequestNegativeReply, LeRemoteConnectionParameterRequestReply},
    param::RemoteConnectionParamsRejectReason,
};
#[cfg(feature = "gatt")]
use embassy_sync::blocking_mutex::raw::RawMutex;
use embassy_time::Duration;

#[cfg(feature = "connection-metrics")]
pub use crate::connection_manager::Metrics as ConnectionMetrics;
use crate::connection_manager::{ConnectionManager, ConnectionState};
use crate::pdu::Pdu;
#[cfg(feature = "gatt")]
use crate::prelude::{AttributeServer, GattConnection};
#[cfg(feature = "security")]
use crate::security_manager::{BondInformation, PassKey};
use crate::types::l2cap::ConnParamUpdateRes;
use crate::{bt_hci_duration, Address, BleHostError, Error, Identity, PacketPool, Stack};

/// Security level of a connection
///
/// This describes the various security levels that are supported.
///
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum SecurityLevel {
    /// No encryption and no authentication. All connections start on this security level.
    NoEncryption,
    /// Encrypted but not authenticated communication. Does not provide MITM protection.
    Encrypted,
    /// Encrypted and authenticated security level. MITM protected.
    EncryptedAuthenticated,
}

impl SecurityLevel {
    /// Returns true for either encrypted level; this does not separately certify key length or freshness.
    pub fn encrypted(&self) -> bool {
        !matches!(self, SecurityLevel::NoEncryption)
    }

    /// Returns true only for the authenticated encryption level, representing MITM-protected pairing.
    pub fn authenticated(&self) -> bool {
        matches!(self, SecurityLevel::EncryptedAuthenticated)
    }
}

/// Connection configuration.
pub struct ConnectConfig<'d> {
    /// Scan configuration to use while connecting.
    pub scan_config: ScanConfig<'d>,
    /// Parameters to use for the connection.
    pub connect_params: RequestedConnParams,
}

/// Scan/connect configuration.
pub struct ScanConfig<'d> {
    /// True requests scan-response exchanges; false receives advertisements only.
    pub active: bool,
    /// Peer address/type accept list; empty means role-specific unrestricted scan policy.
    pub filter_accept_list: &'d [Address],
    /// Requested scan PHY bitset; controller/role APIs reject unsupported combinations.
    pub phys: PhySet,
    /// Scan start-to-start interval; encoded in 625-us HCI units by callers.
    pub interval: Duration,
    /// Listening duration per scan interval; callers/controller enforce window<=interval.
    pub window: Duration,
    /// Requested scan duration; zero is unbounded. Legacy central scanning ignores this field.
    pub timeout: Duration,
    /// Controller duplicate-report policy, not host packet deduplication.
    pub filter_duplicates: FilterDuplicates,
}

impl Default for ScanConfig<'_> {
    /// Returns active 1M scanning with empty accept list, one-second interval, one-second window, zero timeout and duplicate filtering disabled. Role-specific callers determine timeout enforcement.
    fn default() -> Self {
        Self {
            active: true,
            filter_accept_list: &[],
            phys: PhySet::M1,
            interval: Duration::from_secs(1),
            window: Duration::from_secs(1),
            timeout: Duration::from_secs(0),
            filter_duplicates: FilterDuplicates::Disabled,
        }
    }
}

/// Requested scan PHY bitset; controller/role APIs reject unsupported combinations.
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[derive(Eq, PartialEq, Copy, Clone)]
#[repr(u8)]
pub enum PhySet {
    /// 1Mbps phy
    M1 = 1,
    /// 2Mbps phy
    M2 = 2,
    /// 1Mbps + 2Mbps phys
    M1M2 = 3,
    /// Coded phy (125kbps, S=8)
    Coded = 4,
    /// 1Mbps and Coded phys
    M1Coded = 5,
    /// 2Mbps and Coded phys
    M2Coded = 6,
    /// 1Mbps, 2Mbps and Coded phys
    M1M2Coded = 7,
}

/// Requested parameters for a connection.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct RequestedConnParams {
    /// Minimum requested event spacing; quantized to the selected HCI command timebase.
    pub min_connection_interval: Duration,
    /// Maximum requested event spacing; ordinary interval validation caps this at 4 seconds.
    pub max_connection_interval: Duration,
    /// Maximum skipped connection events; ordinary requested parameters require less than 500.
    pub max_latency: u16,
    /// Minimum event-length scheduling hint; quantized to 625 us, zero means no minimum hint.
    pub min_event_length: Duration,
    /// Maximum event-length scheduling hint; quantized to 625 us, zero means no maximum hint.
    pub max_event_length: Duration,
    /// Link supervision duration; command conversion uses 10-ms HCI units.
    pub supervision_timeout: Duration,
}

impl RequestedConnParams {
    /// Returns whether interval ordering/range (7.5 ms..=4 s), latency<500, event-length ordering and timeout inequalities hold. Timeout must be 100 ms..=32 s and strictly exceed twice (latency+1)*maximum interval. Does not check HCI quantization or event-length integer encodability.
    pub fn is_valid(&self) -> bool {
        self.min_connection_interval <= self.max_connection_interval
            && self.min_connection_interval >= Duration::from_micros(7_500)
            && self.max_connection_interval <= Duration::from_secs(4)
            && self.max_latency < 500
            && self.min_event_length <= self.max_event_length
            && self.supervision_timeout >= Duration::from_millis(100)
            && self.supervision_timeout <= Duration::from_millis(32_000)
            && self.supervision_timeout.as_micros()
                > 2 * u64::from(self.max_latency + 1) * self.max_connection_interval.as_micros()
    }
}

/// Current parameters for a connection.
#[derive(Default, Debug, Clone, Copy)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct ConnParams {
    /// Connection interval.
    pub conn_interval: Duration,
    /// Permitted skipped connection events reported by the controller.
    pub peripheral_latency: u16,
    /// Link supervision duration; command conversion uses 10-ms HCI units.
    pub supervision_timeout: Duration,
}

/// Connection subrating parameters
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct SubratingParams {
    /// Subrate factor: only every `subrate_factor`-th connection event is used.
    pub subrate_factor: u16,
    /// Number of connection events to stay awake for after a non-empty packet.
    pub continuation_number: u16,
}

/// Connection rate parameters.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct ConnectRateParams {
    /// Minimum requested event spacing; quantized to the selected HCI command timebase.
    pub min_connection_interval: Duration,
    /// Maximum requested event spacing; ordinary interval validation caps this at 4 seconds.
    pub max_connection_interval: Duration,
    /// Minimum requested connection-event divisor; controller validates the range.
    pub subrate_min: u16,
    /// Maximum requested connection-event divisor; controller validates range/order.
    pub subrate_max: u16,
    /// Maximum skipped connection events; ordinary requested parameters require less than 500.
    pub max_latency: u16,
    /// Number of continuation events allowed for subrate processing.
    pub continuation_number: u16,
    /// Link supervision duration; command conversion uses 10-ms HCI units.
    pub supervision_timeout: Duration,
    /// Minimum event-length hint, quantized to 625-us HCI units.
    pub min_ce_length: Duration,
    /// Maximum event-length hint, quantized to 625-us HCI units.
    pub max_ce_length: Duration,
}

/// A connection event.
#[derive(Debug)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum ConnectionEvent {
    /// Connection disconnected.
    Disconnected {
        /// The reason (status code) for the disconnect.
        reason: Status,
    },
    /// The phy settings was updated for this connection.
    PhyUpdated {
        /// The TX phy.
        tx_phy: PhyKind,
        /// The RX phy.
        rx_phy: PhyKind,
    },
    /// Successful controller connection interval/latency/timeout update.
    ConnectionParamsUpdated {
        /// Connection interval.
        conn_interval: Duration,
        /// Permitted skipped connection events reported by the controller.
        peripheral_latency: u16,
        /// Link supervision duration; command conversion uses 10-ms HCI units.
        supervision_timeout: Duration,
    },
    /// The subrating was updated for this connection.
    SubratingParamsUpdated {
        /// Subrate factor: only every `subrate_factor`-th connection event is used.
        subrate_factor: u16,
        /// Peripheral latency, in subrated connection events.
        peripheral_latency: u16,
        /// Number of connection events to stay awake for after a non-empty packet.
        continuation_number: u16,
        /// Link supervision duration; command conversion uses 10-ms HCI units.
        supervision_timeout: Duration,
    },
    /// The data length was changed for this connection.
    DataLengthUpdated {
        /// Controller-reported maximum TX payload octets.
        max_tx_octets: u16,
        /// Controller-reported maximum TX packet airtime in microseconds.
        max_tx_time: u16,
        /// Controller-reported maximum RX payload octets.
        max_rx_octets: u16,
        /// Controller-reported maximum RX packet airtime in microseconds.
        max_rx_time: u16,
    },
    /// The frame space was updated for this connection.
    FrameSpaceUpdated {
        /// The negotiated frame space value.
        frame_space: Duration,
        /// Who initiated the frame space update.
        initiator: FrameSpaceInitiator,
        /// PHYs affected.
        phys: PhyMask,
        /// Spacing types affected.
        spacing_types: SpacingTypes,
    },
    /// Connection rate has been changed.
    ConnectionRateChanged {
        /// Connection interval.
        conn_interval: Duration,
        /// Subrate factor.
        subrate_factor: u16,
        /// Permitted skipped connection events reported by the controller.
        peripheral_latency: u16,
        /// Additional active connection events retained after a nonempty packet.
        continuation_number: u16,
        /// Link supervision duration; command conversion uses 10-ms HCI units.
        supervision_timeout: Duration,
    },
    /// A request to change the connection parameters.
    ///
    /// Respond through [`ConnectionParamsRequest`]; dropping the token sends no reply.
    /// Acceptance requires `central` or `connection-params-update`; rejection is always available.
    RequestConnectionParams(
        /// Owned request token; application must accept/reject it or the peer can time out.
        ConnectionParamsRequest,
    ),
    #[cfg(feature = "security")]
    /// Request to display a pass key
    PassKeyDisplay(
        /// Numeric value to display through the application UI.
        PassKey,
    ),
    #[cfg(feature = "security")]
    /// Request to display and confirm a pass key
    PassKeyConfirm(
        /// Numeric value requiring explicit user comparison/confirmation.
        PassKey,
    ),
    #[cfg(feature = "security")]
    /// Request to make the user input the pass key
    PassKeyInput,
    #[cfg(feature = "security")]
    /// Pairing completed
    PairingComplete {
        /// Security level established by this pairing result.
        security_level: SecurityLevel,
        /// Bond information if the devices create a bond with this pairing.
        bond: Option<BondInformation>,
    },
    #[cfg(feature = "security")]
    /// Pairing failed; a previously encrypted/bonded state is not implied.
    PairingFailed(
        /// Exact local/peer/protocol failure reported by the security manager.
        Error,
    ),
    #[cfg(feature = "security")]
    /// The peer has lost its bond (received pairing request for a bonded peer).
    BondLost,
    #[cfg(feature = "security")]
    /// The link is now encrypted. Fires once per encryption-enable transition,
    /// for both fresh pairings and resumed bonded sessions. For pairings, this
    /// fires alongside `PairingComplete`.
    Encrypted {
        /// Security level achieved by the encryption.
        security_level: SecurityLevel,
        /// Bond information if encryption was achieved using a stored bond.
        bond: Option<BondInformation>,
    },
    #[cfg(feature = "security")]
    /// OOB data is requested during pairing. Respond with [`Connection::provide_oob_data()`].
    OobRequest,
}

/// A connection parameters update request
#[derive(Debug)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct ConnectionParamsRequest {
    /// Peer proposal, retained so acceptance can omit an override.
    params: RequestedConnParams,
    /// Link handle on the originating stack; not an independently retained Connection owner.
    handle: ConnHandle,
    /// True once an accept/reject future starts, not proof that its reply completed.
    responded: bool,
    /// Some carries the L2CAP request identifier to echo; None selects an HCI Link Layer response.
    l2cap_identifier: Option<u8>,
}

impl ConnectionParamsRequest {
    /// Returns an unanswered L2CAP request carrying `params`, link `handle` and response `l2cap_identifier`; identifier validation occurs on response submission.
    pub(crate) fn new(params: RequestedConnParams, handle: ConnHandle, l2cap_identifier: u8) -> Self {
        Self {
            params,
            handle,
            responded: false,
            l2cap_identifier: Some(l2cap_identifier),
        }
    }

    #[cfg(feature = "connection-params-update")]
    /// Returns an unanswered HCI Link Layer request carrying `params` and link `handle`; no L2CAP identifier is used.
    pub(crate) fn new_link_layer(params: RequestedConnParams, handle: ConnHandle) -> Self {
        Self {
            params,
            handle,
            responded: false,
            l2cap_identifier: None,
        }
    }

    /// Returns the peer-requested parameters retained by this response token.
    pub fn params(&self) -> &RequestedConnParams {
        &self.params
    }
    #[cfg(any(feature = "central", feature = "connection-params-update"))]
    /// Accepts using override `params` or the peer proposal through originating `stack`. Returns controller-update then success-signal result; invalid parameters are rejected instead. Marks responded before awaiting; cancellation can omit the reply. Unknown link maps to Disconnected; other update errors attempt rejection before propagating the original error.
    async fn accept_l2cap<C, P: PacketPool>(
        mut self,
        params: Option<&RequestedConnParams>,
        stack: &Stack<'_, C, P>,
    ) -> Result<(), BleHostError<C::Error>>
    where
        C: crate::Controller,
    {
        self.responded = true;
        let identifier = self.l2cap_identifier.ok_or(Error::InvalidState)?;

        let params = params.unwrap_or(&self.params);
        if !params.is_valid() {
            return self.reject_l2cap(stack).await;
        }

        match stack
            .host()
            .async_command(into_le_conn_update(self.handle, params))
            .await
        {
            Ok(()) => {
                let param = ConnParamUpdateRes { result: 0 };
                stack
                    .host()
                    .send_conn_param_update_res(self.handle, identifier, &param)
                    .await
            }
            Err(BleHostError::BleHost(crate::Error::Hci(bt_hci::param::Error::UNKNOWN_CONN_IDENTIFIER))) => {
                Err(crate::Error::Disconnected.into())
            }
            Err(e) => {
                info!("Connection parameters request procedure failed");
                if let Err(e) = self.reject_l2cap(stack).await {
                    warn!("Failed to reject ConnParamRequest after failure");
                }
                Err(e)
            }
        }
    }

    /// Rejects through originating `stack`, returning signal submission result or InvalidState for a non-L2CAP token. Marks responded before I/O; cancellation does not automatically retry the reply.
    async fn reject_l2cap<C, P: PacketPool>(mut self, stack: &Stack<'_, C, P>) -> Result<(), BleHostError<C::Error>>
    where
        C: crate::Controller,
    {
        self.responded = true;
        let identifier = self.l2cap_identifier.ok_or(Error::InvalidState)?;
        let param = ConnParamUpdateRes { result: 1 };
        stack
            .host()
            .send_conn_param_update_res(self.handle, identifier, &param)
            .await
    }
}

#[cfg(not(feature = "connection-params-update"))]
impl ConnectionParamsRequest {
    /// Accept an L2CAP request using the supplied or peer-requested parameters.
    #[cfg(feature = "central")]
    /// Accepts through originating `stack`, using optional override `params` (None keeps the proposal). Returns controller/signal errors; invalid proposals send rejection. Cancellation after first poll may leave an unanswered peer request or completed controller update.
    pub async fn accept<C: crate::Controller, P: PacketPool>(
        self,
        params: Option<&RequestedConnParams>,
        stack: &Stack<'_, C, P>,
    ) -> Result<(), BleHostError<C::Error>> {
        self.accept_l2cap(params, stack).await
    }

    /// Rejects through originating `stack`, returning L2CAP submission result; cancellation after first poll may leave the peer unanswered.
    pub async fn reject<C: crate::Controller, P: PacketPool>(
        self,
        stack: &Stack<'_, C, P>,
    ) -> Result<(), BleHostError<C::Error>> {
        self.reject_l2cap(stack).await
    }
}

#[cfg(feature = "connection-params-update")]
impl ConnectionParamsRequest {
    /// Accepts through originating `stack`, using optional override `params` (None keeps the proposal). Returns HCI reply or L2CAP update/reply result according to request origin; invalid parameters send rejection. Unknown handle maps to Disconnected. Quantization/width rules apply, and cancellation does not undo controller effects or guarantee a peer reply.
    pub async fn accept<C, P: PacketPool>(
        mut self,
        params: Option<&RequestedConnParams>,
        stack: &Stack<'_, C, P>,
    ) -> Result<(), BleHostError<C::Error>>
    where
        C: crate::Controller
            + ControllerCmdAsync<LeRemoteConnectionParameterRequestReply>
            + ControllerCmdAsync<LeRemoteConnectionParameterRequestNegativeReply>,
    {
        if self.l2cap_identifier.is_some() {
            return self.accept_l2cap(params, stack).await;
        }
        self.responded = true;
        let params = params.unwrap_or(&self.params);
        if !params.is_valid() {
            return self.reject(stack).await;
        }

        // An HCI remote request needs its reply, not a separate local update.
        let interval_min: bt_hci::param::Duration<1_250> = bt_hci_duration(params.min_connection_interval);
        let interval_max: bt_hci::param::Duration<1_250> = bt_hci_duration(params.max_connection_interval);
        let timeout: bt_hci::param::Duration<10_000> = bt_hci_duration(params.supervision_timeout);
        stack
            .host()
            .async_command(LeRemoteConnectionParameterRequestReply::new(
                self.handle,
                interval_min,
                interval_max,
                params.max_latency,
                timeout,
                bt_hci_duration(params.min_event_length),
                bt_hci_duration(params.max_event_length),
            ))
            .await
            .map_err(|error| match error {
                BleHostError::BleHost(Error::Hci(bt_hci::param::Error::UNKNOWN_CONN_IDENTIFIER)) => {
                    Error::Disconnected.into()
                }
                other => other,
            })
    }

    /// Rejects through originating `stack` using HCI UnacceptableConnParameters or L2CAP result1. Returns transport/controller result; cancellation after first poll does not retry the response.
    pub async fn reject<C, P: PacketPool>(mut self, stack: &Stack<'_, C, P>) -> Result<(), BleHostError<C::Error>>
    where
        C: crate::Controller + ControllerCmdAsync<LeRemoteConnectionParameterRequestNegativeReply>,
    {
        self.responded = true;
        if self.l2cap_identifier.is_some() {
            self.reject_l2cap(stack).await
        } else {
            stack
                .host()
                .async_command(LeRemoteConnectionParameterRequestNegativeReply::new(
                    self.handle,
                    RemoteConnectionParamsRejectReason::UnacceptableConnParameters,
                ))
                .await
        }
    }
}

impl Drop for ConnectionParamsRequest {
    /// Logs an unanswered-token diagnostic; does not send a default rejection or retry an interrupted response.
    fn drop(&mut self) {
        if !self.responded {
            error!("ConnParamRequest dropped without being acccepted/rejected");
        }
    }
}

impl Default for RequestedConnParams {
    /// Returns 80 ms fixed interval, zero peripheral latency/event-length hints and 8 s supervision timeout.
    fn default() -> Self {
        Self {
            min_connection_interval: Duration::from_millis(80),
            max_connection_interval: Duration::from_millis(80),
            max_latency: 0,
            min_event_length: Duration::from_secs(0),
            max_event_length: Duration::from_secs(0),
            supervision_timeout: Duration::from_secs(8),
        }
    }
}

impl ConnParams {
    /// Returns zero/unavailable controller observations before a successful connection event populates them.
    pub(crate) const fn new() -> Self {
        Self {
            conn_interval: Duration::from_ticks(0),
            peripheral_latency: 0,
            supervision_timeout: Duration::from_ticks(0),
        }
    }
}

/// Handle to a BLE connection.
///
/// Last drop requests asynchronous disconnection; the runner/controller must complete it.
pub struct Connection<'stack, P: PacketPool> {
    /// Manager-local retained slot, preventing reuse until all owners drop.
    index: u8,
    /// Originating table; its identity is part of connection equality.
    manager: &'stack ConnectionManager<'stack, P>,
}

impl<P: PacketPool> core::fmt::Debug for Connection<'_, P> {
    /// Writes the retained slot index to `f`; returns formatter success/error without exposing peer/security data.
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_tuple("Connection").field(&self.index).finish()
    }
}

#[cfg(feature = "defmt")]
impl<P: PacketPool> defmt::Format for Connection<'_, P> {
    /// Writes retained slot index through defmt formatter `f`.
    fn format(&self, f: defmt::Formatter) {
        defmt::write!(f, "Connection({})", self.index)
    }
}

impl<P: PacketPool> PartialEq for Connection<'_, P> {
    /// Returns true only when `other` has the same manager identity and retained slot index.
    fn eq(&self, other: &Self) -> bool {
        self.index == other.index && core::ptr::eq(self.manager, other.manager)
    }
}

impl<P: PacketPool> Eq for Connection<'_, P> {}

impl<P: PacketPool> Clone for Connection<'_, P> {
    /// Returns another retained link owner; panics before mutation if the manager reference counter is full.
    fn clone(&self) -> Self {
        self.manager.inc_ref(self.index);
        Connection::new(self.index, self.manager)
    }
}

impl<P: PacketPool> Drop for Connection<'_, P> {
    /// Releases one link owner; last release requests deferred disconnection rather than waiting for the controller.
    fn drop(&mut self) {
        self.manager.dec_ref(self.index);
    }
}

impl<'stack, P: PacketPool> Connection<'stack, P> {
    /// Returns an owner for `index` in `manager`; caller must already have retained one reference for this handle.
    pub(crate) fn new(index: u8, manager: &'stack ConnectionManager<'stack, P>) -> Self {
        Self { index, manager }
    }

    /// Returns the originating connection table, whose identity disambiguates equal controller handles in different stacks.
    pub(crate) fn manager(&self) -> &ConnectionManager<'stack, P> {
        self.manager
    }

    /// Returns Ok if `stack` owns this handle's manager, otherwise InvalidValue before controller I/O.
    pub(crate) fn check_stack<T>(&self, stack: &Stack<'_, T, P>) -> Result<(), Error> {
        if core::ptr::eq(self.manager, stack.host().connections()) {
            Ok(())
        } else {
            Err(Error::InvalidValue)
        }
    }

    /// Returns this manager-local retained slot index, not the controller connection handle.
    pub(crate) fn index(&self) -> u8 {
        self.index
    }

    /// Stores negotiated `mtu` bytes; zero resets the unexchanged sentinel. Internal callers enforce protocol bounds.
    pub(crate) fn set_att_mtu(&self, mtu: u16) {
        self.manager.set_att_mtu(self.index, mtu);
    }

    /// Returns negotiated ATT MTU bytes, or the default 23-byte ATT MTU before exchange.
    pub(crate) fn get_att_mtu(&self) -> u16 {
        self.manager.get_att_mtu(self.index)
    }

    /// Returns whether a nonzero negotiated ATT MTU has been stored.
    pub(crate) fn is_att_mtu_exchanged(&self) -> bool {
        self.manager.is_att_mtu_exchanged(self.index)
    }

    /// Sets inbound dynamic-channel admission to `listening`; duplicate true admission panics before changing the existing listener.
    pub(crate) fn set_l2cap_listening(&self, listening: bool) {
        self.manager.set_l2cap_listening(self.index, listening);
    }

    /// Queues owned `pdu` with retained slot identity, waiting for shared capacity. Returns Disconnected/NoPermits before admission; cancellation drops packet/lease. Completion means queued, not sent; later disconnection discards it.
    pub(crate) async fn send(&self, pdu: Pdu<P::Packet>) -> Result<(), Error> {
        self.manager.send(self.index, pdu).await
    }

    /// Returns Ok after queueing owned `pdu`, Disconnected/NoPermits on admission failure, or OutOfMemory if full. Errors drop the packet; retained slot identity prevents reuse from redirecting bytes. Success does not guarantee transmission.
    pub(crate) fn try_send(&self, pdu: Pdu<P::Packet>) -> Result<(), Error> {
        self.manager.try_send(self.index, pdu)
    }

    /// Queues owned `event`, waiting for this link event queue capacity. Cancellation before enqueue drops it; internal readiness-to-insertion invariant violations panic.
    pub(crate) async fn post_event(&self, event: ConnectionEvent) {
        self.manager.post_event(self.index, event).await
    }

    /// Returns the next queued link event, waiting without a deadline. Cancellation while empty consumes nothing; one receiver waker is supported. Queue exhaustion after disconnect can wait forever; use retained terminal status for cleanup decisions.
    pub async fn next(&self) -> ConnectionEvent {
        self.manager.next(self.index).await
    }

    #[cfg(feature = "gatt")]
    /// Returns the next server-side ATT packet, waiting without checking link state. Cancellation while empty consumes nothing; caller concurrently watches connection events.
    pub(crate) async fn next_gatt(&self) -> Pdu<P::Packet> {
        self.manager.next_gatt(self.index).await
    }

    #[cfg(feature = "gatt")]
    /// Returns the next client ATT response, or None once the link is no longer Connecting/Connected. Cancellation while empty consumes nothing; one response waiter is retained.
    pub(crate) async fn next_gatt_client(&self) -> Option<Pdu<P::Packet>> {
        self.manager.next_gatt_client(self.index).await
    }

    #[cfg(feature = "gatt")]
    /// Returns Ok after exclusively reserving the link indication slot, or Disconnected; waits while another indication owns it. Pending cancellation reserves nothing; successful acquisition requires explicit release.
    pub(crate) async fn acquire_indication_slot(&self) -> Result<(), Error> {
        self.manager.acquire_indication_slot(self.index).await
    }

    #[cfg(feature = "gatt")]
    /// Releases indication ownership, clears its confirmation latch and wakes the next slot waiter.
    pub(crate) fn release_indication_slot(&self) {
        self.manager.release_indication_slot(self.index)
    }

    #[cfg(feature = "gatt")]
    /// Returns Ok after consuming the confirmation latch, or Disconnected. Pending cancellation retains the indication slot and unconsumed confirmation; caller must release ownership.
    pub(crate) async fn wait_indication_confirmation(&self) -> Result<(), Error> {
        self.manager.wait_indication_confirmation(self.index).await
    }

    /// Returns the current local lifecycle phase without controller I/O.
    pub(crate) fn state(&self) -> ConnectionState {
        self.manager.state(self.index)
    }

    /// Returns true only in application-accepted Connected state; Connecting and local disconnect phases are false.
    pub fn is_connected(&self) -> bool {
        self.manager.is_connected(self.index)
    }

    /// Returns true only after terminal controller disconnection; local requested/in-flight disconnect remains false.
    pub fn is_disconnected(&self) -> bool {
        self.manager.is_disconnected(self.index)
    }

    /// Returns terminal controller reason even if event queue was full; None before terminal notification, including a local close request.
    pub fn disconnect_reason(&self) -> Option<Status> {
        self.manager.connection(self.index).disconnect_reason
    }

    /// Returns the first ACL ingress error triggering local close, cloned from retained state; None if none recorded. It survives terminal disconnect until slot reuse.
    pub fn local_disconnect_error(&self) -> Option<Error> {
        self.manager.connection(self.index).local_disconnect_error.clone()
    }

    /// Returns the controller connection handle retained by this owner; the numeric value alone does not prove the link is still live.
    pub fn handle(&self) -> ConnHandle {
        self.manager.handle(self.index)
    }

    /// Returns negotiated ATT MTU bytes, or 23 before negotiation; independent of dynamic-channel SDU MTUs.
    pub fn att_mtu(&self) -> u16 {
        self.get_att_mtu()
    }

    /// Returns the link role reported at establishment; does not query current controller state.
    pub fn role(&self) -> LeConnRole {
        self.manager.role(self.index)
    }

    /// Returns the retained peer identity address/type; this is not a separate snapshot of the on-air private address.
    pub fn peer_address(&self) -> Address {
        self.manager.peer_address(self.index)
    }

    /// Returns the retained peer identity address plus optional resolving key; peer_address exposes the address component.
    pub fn peer_identity(&self) -> Identity {
        self.manager.peer_identity(self.index)
    }

    /// Returns the latest retained connection interval/latency/timeout observation; no controller read is issued.
    pub fn params(&self) -> ConnParams {
        self.manager.params(self.index)
    }

    /// The subrating currently in effect, or `None` if the connection is not subrated.
    #[cfg(feature = "subrating")]
    /// Returns retained subrating when factor>1, otherwise None; an interval change resets this observation.
    pub fn subrating_params(&self) -> Option<SubratingParams> {
        self.manager.subrating_params(self.index)
    }

    /// Requests role-appropriate pairing/encryption for an unencrypted accepted link. Returns Ok without starting work if already encrypted; otherwise returns admission/security errors, Disconnected, or NotSupported without security feature. Completion/failure is reported asynchronously.
    pub fn request_security(&self) -> Result<(), Error> {
        self.manager.request_security(self.index, true)
    }

    /// Check if the peer is a bonded device.
    #[cfg(feature = "security")]
    /// Returns whether stored peer bond information exists with its bonded flag set; does not prove current link encryption.
    pub fn is_bonded_peer(&self) -> bool {
        self.manager.is_bonded_peer(self.index)
    }

    /// Try to enable encryption with a bonded peer if it is not yet established
    #[cfg(feature = "security")]
    /// Returns Ok when already encrypted or the shared pairing/encryption procedure completes; otherwise returns link/security errors. Cancellation stops waiting but does not cancel the shared procedure.
    pub(crate) async fn try_enable_encryption(&self) -> Result<(), Error> {
        self.manager.try_enable_encryption(self.index).await
    }

    /// Returns the accepted link security level, or Disconnected; without security feature returns NoEncryption for live links.
    pub fn security_level(&self) -> Result<SecurityLevel, Error> {
        self.manager.get_security_level(self.index)
    }

    /// Get the negotiated encryption key length for this connection
    #[cfg(feature = "legacy-pairing")]
    /// Returns the retained negotiated encryption-key byte length, or Disconnected; inspect security_level separately before interpreting unencrypted state.
    pub fn encryption_key_len(&self) -> Result<u8, Error> {
        self.manager.get_encryption_key_len(self.index)
    }

    /// Returns the accepted link bonding preference, or Disconnected; without security support returns false. Preference is sampled by pairing setup.
    pub fn bondable(&self) -> Result<bool, Error> {
        self.manager.get_bondable(self.index)
    }

    /// Stores `bondable` for future pairing setup; returns Disconnected or NotSupported without security. Default is false; setting it after pairing starts does not revise negotiated flags. Bond persistence remains the application responsibility.
    pub fn set_bondable(&self, bondable: bool) -> Result<(), Error> {
        self.manager.set_bondable(self.index, bondable)
    }

    /// Confirms the displayed numeric value through the security state machine. Returns phase/queue errors, Disconnected, or NotSupported without security; does not itself complete pairing.
    pub fn pass_key_confirm(&self) -> Result<(), Error> {
        self.manager.pass_key_confirm(self.index, true)
    }

    /// Rejects the displayed numeric value through the security state machine. Returns phase/queue errors, Disconnected, or NotSupported without security.
    pub fn pass_key_cancel(&self) -> Result<(), Error> {
        self.manager.pass_key_confirm(self.index, false)
    }

    /// Supplies numeric `pass_key` to the active input phase; caller must enforce 0..=999999 (not checked here). Returns security phase/queue errors, Disconnected, or NotSupported without security. No controller operation is awaited.
    pub fn pass_key_input(&self, pass_key: u32) -> Result<(), Error> {
        self.manager.pass_key_input(self.index, pass_key)
    }

    /// Set whether OOB data is available for this connection.
    ///
    /// LESC selects OOB if either peer advertises it; legacy requires both peers.
    ///
    /// This must be set before pairing is initiated.
    #[cfg(feature = "security")]
    /// Stores `available` for pairing capability setup; returns Disconnected if the link is not accepted. Set before pairing; selection depends on negotiated security mode and both peers flags.
    pub fn set_oob_available(&self, available: bool) -> Result<(), Error> {
        self.manager.set_oob_available(self.index, available)
    }

    /// Provide OOB data during pairing.
    ///
    /// Call this when [`ConnectionEvent::OobRequest`] is received. Both the local
    /// and peer OOB data must be provided.
    #[cfg(feature = "security")]
    /// Supplies `local_oob` and `peer_oob` after OobRequest. LESC ignores local confirm; local random is used when the peer advertised OOB. An all-zero peer random+confirm means absent and returns Security(OobNotAvailable) when this side advertised OOB; otherwise supplied peer confirm is verified. Returns state/crypto/queue errors or Disconnected; does not persist the resulting bond.
    pub fn provide_oob_data(
        &self,
        local_oob: crate::security_manager::OobData,
        peer_oob: crate::security_manager::OobData,
    ) -> Result<(), Error> {
        self.manager.provide_oob_data(self.index, local_oob, peer_oob)
    }

    /// Requests RemoteUserTerminatedConn disconnection and wakes the host runner; returns before command submission or terminal controller event. Repeated requests outside live phases do nothing.
    pub fn disconnect(&self) {
        self.manager
            .request_disconnect(self.index, DisconnectReason::RemoteUserTerminatedConn);
    }

    /// Read metrics for this connection
    #[cfg(feature = "connection-metrics")]
    /// Returns `f` applied to a borrowed metrics snapshot. Callback runs under an immutable table borrow; reentrant mutation can panic.
    pub fn metrics<F: FnOnce(&ConnectionMetrics) -> R, R>(&self, f: F) -> R {
        self.manager.metrics(self.index, f)
    }

    #[cfg(feature = "att-queued-writes")]
    /// Queues `value` at byte `offset` for ATT `handle`; returns INVALID_HANDLE for zero or PREPARE_QUEUE_FULL on capacity/noncontiguous/different-handle input. Rejected chunks preserve prior state; execution/attribute validation happens elsewhere.
    pub(crate) fn prepare_write(&self, handle: u16, offset: u16, value: &[u8]) -> Result<(), crate::att::AttErrorCode> {
        self.manager.prepare_write(self.index, handle, offset, value)
    }

    #[cfg(feature = "att-queued-writes")]
    /// Returns callback `f` result while immutably borrowing the queued-write state; reentrant table mutation can panic.
    pub(crate) fn with_prepare_write<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&crate::connection_manager::PrepareWriteState) -> R,
    {
        self.manager.with_prepare_write(self.index, f)
    }

    #[cfg(feature = "att-queued-writes")]
    /// Releases queued-write handle/length; retained backing bytes are not erased.
    pub(crate) fn clear_prepare_write(&self) {
        self.manager.clear_prepare_write(self.index)
    }

    /// Reads link RSSI through originating `stack`; returns signed dBm or raw 127 unavailable sentinel, with controller/transport errors propagated. Cancellation may leave an issued command pending in the host.
    /// Returns InvalidValue before I/O if stack belongs to a different host.
    pub async fn rssi<T>(&self, stack: &Stack<'_, T, P>) -> Result<i8, BleHostError<T::Error>>
    where
        T: ControllerCmdSync<ReadRssi>,
    {
        self.check_stack(stack)?;
        let handle = self.handle();
        let ret = stack.host().command(ReadRssi::new(handle)).await?;
        Ok(ret.rssi)
    }

    /// Requests `phy` for both directions through originating `stack`; coded variants select S8/S2 preference. Returns controller/transport result; negotiated PHY arrives in an event and may differ. Cancellation does not undo an issued request.
    /// Returns InvalidValue before I/O if stack belongs to a different host.
    pub async fn set_phy<T>(&self, stack: &Stack<'_, T, P>, phy: PhyKind) -> Result<(), BleHostError<T::Error>>
    where
        T: ControllerCmdAsync<LeSetPhy>,
    {
        self.check_stack(stack)?;
        let all_phys = AllPhys::new()
            .set_has_no_rx_phy_preference(false)
            .set_has_no_tx_phy_preference(false);
        let mut mask = PhyMask::new()
            .set_le_coded_phy(false)
            .set_le_1m_phy(false)
            .set_le_2m_phy(false);
        let mut options = PhyOptions::default();
        match phy {
            PhyKind::Le2M => {
                mask = mask.set_le_2m_phy(true);
            }
            PhyKind::Le1M => {
                mask = mask.set_le_1m_phy(true);
            }
            PhyKind::LeCoded => {
                mask = mask.set_le_coded_phy(true);
                options = PhyOptions::S8CodingPreferred;
            }
            PhyKind::LeCodedS2 => {
                mask = mask.set_le_coded_phy(true);
                options = PhyOptions::S2CodingPreferred;
            }
        }
        stack
            .host()
            .async_command(LeSetPhy::new(self.handle(), all_phys, mask, mask, options))
            .await?;
        Ok(())
    }

    /// Reads through originating `stack`, returning (TX PHY, RX PHY) or controller/transport error; this is a controller observation, not requested preference.
    /// Returns InvalidValue before I/O if stack belongs to a different host.
    pub async fn read_phy<T>(&self, stack: &Stack<'_, T, P>) -> Result<(PhyKind, PhyKind), BleHostError<T::Error>>
    where
        T: ControllerCmdSync<LeReadPhy>,
    {
        self.check_stack(stack)?;
        let res = stack.host().command(LeReadPhy::new(self.handle())).await?;
        Ok((res.tx_phy, res.rx_phy))
    }

    /// Requests TX payload `length` octets and `time_us` microseconds through originating `stack`. Returns InvalidValue when length>27 lacks local extension support; other ranges are controller-validated. Unknown handle maps to Disconnected; success is command acceptance, with negotiated values reported asynchronously.
    /// Returns InvalidValue before I/O if stack belongs to a different host.
    pub async fn update_data_length<T>(
        &self,
        stack: &Stack<'_, T, P>,
        length: u16,
        time_us: u16,
    ) -> Result<(), BleHostError<T::Error>>
    where
        T: ControllerCmdSync<LeSetDataLength> + ControllerCmdSync<LeReadLocalSupportedFeatures>,
    {
        self.check_stack(stack)?;
        let handle = self.handle();
        // First, check the local supported features to ensure that the connection update is supported.
        let features = stack.host().command(LeReadLocalSupportedFeatures::new()).await?;
        if length <= 27 || features.supports_le_data_packet_length_extension() {
            match stack
                .host()
                .command(LeSetDataLength::new(handle, length, time_us))
                .await
            {
                Ok(_) => Ok(()),
                Err(BleHostError::BleHost(crate::Error::Hci(bt_hci::param::Error::UNKNOWN_CONN_IDENTIFIER))) => {
                    Err(crate::Error::Disconnected.into())
                }
                Err(e) => Err(e),
            }
        } else {
            Err(BleHostError::BleHost(Error::InvalidValue))
        }
    }

    /// Requests `params` through originating `stack`, selecting HCI update or peripheral L2CAP fallback from role/features. Returns controller/signaling errors and maps unknown handle to Disconnected; does not call params.is_valid. Duration conversions truncate/panic on width overflow. Success may only mean signaling submission; cancellation may leave the request active.
    /// Returns InvalidValue before I/O if stack belongs to a different host.
    pub async fn update_connection_params<T>(
        &self,
        stack: &Stack<'_, T, P>,
        params: &RequestedConnParams,
    ) -> Result<(), BleHostError<T::Error>>
    where
        T: ControllerCmdAsync<LeConnUpdate> + ControllerCmdSync<LeReadLocalSupportedFeatures>,
    {
        self.check_stack(stack)?;
        let handle = self.handle();
        #[cfg(any(feature = "central", feature = "connection-params-update"))]
        {
            // First, check the local supported features to ensure that the connection update is supported.
            let features = stack.host().command(LeReadLocalSupportedFeatures::new()).await?;
            if features.supports_conn_parameters_request_procedure() || self.role() == LeConnRole::Central {
                match stack.host().async_command(into_le_conn_update(handle, params)).await {
                    Ok(_) => return Ok(()),
                    Err(BleHostError::BleHost(crate::Error::Hci(bt_hci::param::Error::UNKNOWN_CONN_IDENTIFIER))) => {
                        return Err(crate::Error::Disconnected.into());
                    }
                    Err(BleHostError::BleHost(crate::Error::Hci(bt_hci::param::Error::UNSUPPORTED_REMOTE_FEATURE))) => {
                        // We tried to send the request as a periperhal but the remote central does not support procedure.
                        // Use the L2CAP signaling method below instead.
                        // This code path should never be reached when acting as a central. If a bugged controller implementation
                        // returns this error code we transmit an invalid L2CAP signal which then is rejected by the remote.
                    }
                    Err(e) => return Err(e),
                }
            }
        }

        if self.role() == LeConnRole::Peripheral || cfg!(feature = "connection-params-update") {
            use crate::types::l2cap::ConnParamUpdateReq;
            // Use L2CAP signaling to update connection parameters
            debug!(
                "Connection parameters request procedure not supported, use l2cap connection parameter update req instead"
            );
            let interval_min: bt_hci::param::Duration<1_250> = bt_hci_duration(params.min_connection_interval);
            let interval_max: bt_hci::param::Duration<1_250> = bt_hci_duration(params.max_connection_interval);
            let timeout: bt_hci::param::Duration<10_000> = bt_hci_duration(params.supervision_timeout);
            let param = ConnParamUpdateReq {
                interval_min: interval_min.as_u16(),
                interval_max: interval_max.as_u16(),
                latency: params.max_latency,
                timeout: timeout.as_u16(),
            };
            stack.host().send_conn_param_update_req(handle, &param).await?;
        }
        Ok(())
    }

    /// Requests spacing bounds `frame_space_min`/`frame_space_max` for `phys` and `spacing_types` through originating `stack`. Returns command result, mapping unknown handle to Disconnected. Values are quantized to HCI units; width overflow panics and controller validates ranges. Actual negotiation is reported asynchronously.
    /// Returns InvalidValue before I/O if stack belongs to a different host.
    pub async fn update_frame_space<T>(
        &self,
        stack: &Stack<'_, T, P>,
        frame_space_min: Duration,
        frame_space_max: Duration,
        phys: PhyMask,
        spacing_types: SpacingTypes,
    ) -> Result<(), BleHostError<T::Error>>
    where
        T: ControllerCmdSync<LeFrameSpaceUpdate>,
    {
        self.check_stack(stack)?;
        let handle = self.handle();
        let frame_space_min_dur = bt_hci_duration(frame_space_min);
        let frame_space_max_dur = bt_hci_duration(frame_space_max);
        match stack
            .host()
            .command(LeFrameSpaceUpdate::new(
                handle,
                frame_space_min_dur,
                frame_space_max_dur,
                phys,
                spacing_types,
            ))
            .await
        {
            Ok(_) => Ok(()),
            Err(BleHostError::BleHost(crate::Error::Hci(bt_hci::param::Error::UNKNOWN_CONN_IDENTIFIER))) => {
                Err(crate::Error::Disconnected.into())
            }
            Err(e) => Err(e),
        }
    }

    /// Requests `conn_rate_params` through originating `stack`, returning command result and mapping unknown handle to Disconnected. Duration conversion quantizes and may panic on width overflow; controller validates ranges. Success precedes the resulting connection-rate event.
    /// Returns InvalidValue before I/O if stack belongs to a different host.
    pub async fn request_connection_rate<T>(
        &self,
        stack: &Stack<'_, T, P>,
        conn_rate_params: &ConnectRateParams,
    ) -> Result<(), BleHostError<T::Error>>
    where
        T: ControllerCmdSync<LeConnectionRateRequest>,
    {
        self.check_stack(stack)?;
        let handle = self.handle();
        let min_interval = bt_hci_duration(conn_rate_params.min_connection_interval);
        let max_interval = bt_hci_duration(conn_rate_params.max_connection_interval);
        let timeout = bt_hci_duration(conn_rate_params.supervision_timeout);
        let min_ce = bt_hci_duration(conn_rate_params.min_ce_length);
        let max_ce = bt_hci_duration(conn_rate_params.max_ce_length);
        match stack
            .host()
            .command(LeConnectionRateRequest::new(
                handle,
                min_interval,
                max_interval,
                conn_rate_params.subrate_min,
                conn_rate_params.subrate_max,
                conn_rate_params.max_latency,
                conn_rate_params.continuation_number,
                timeout,
                min_ce,
                max_ce,
            ))
            .await
        {
            Ok(_) => Ok(()),
            Err(BleHostError::BleHost(crate::Error::Hci(bt_hci::param::Error::UNKNOWN_CONN_IDENTIFIER))) => {
                Err(crate::Error::Disconnected.into())
            }
            Err(e) => Err(e),
        }
    }

    /// Transform BLE connection into a `GattConnection`
    #[cfg(feature = "gatt")]
    /// Consumes this link owner and registers it with `server`, returning a GATT wrapper or server capacity/state error. Failure drops the consumed owner and can request disconnection if it was the last reference.
    pub fn with_attribute_server<'values, 'server, M: RawMutex, const ATT_MAX: usize, const CONN_MAX: usize>(
        self,
        server: &'server AttributeServer<'values, M, P, ATT_MAX, CONN_MAX>,
    ) -> Result<GattConnection<'stack, 'server, P>, Error> {
        GattConnection::try_new(self, server)
    }
}

#[cfg(any(feature = "central", feature = "connection-params-update"))]
/// Returns the HCI update command for `handle` and `params`, quantizing intervals to 1.25 ms, timeout to 10 ms and event-length hints to 625 us. Does not validate policy; unrepresentable integer durations panic.
fn into_le_conn_update(handle: ConnHandle, params: &RequestedConnParams) -> LeConnUpdate {
    LeConnUpdate::new(
        handle,
        bt_hci_duration(params.min_connection_interval),
        bt_hci_duration(params.max_connection_interval),
        params.max_latency,
        bt_hci_duration(params.supervision_timeout),
        bt_hci_duration(params.min_event_length),
        bt_hci_duration(params.max_event_length),
    )
}

#[cfg(test)]
/// Connection parameter admission boundaries independent of controller behavior.
mod tests {
    use super::*;

    /// Defaults, interval endpoints and the strict supervision inequality agree with admission.
    #[test]
    fn requested_parameters_enforce_interval_latency_and_timeout_bounds() {
        assert!(RequestedConnParams::default().is_valid());
        let mut params = RequestedConnParams {
            min_connection_interval: Duration::from_micros(7_500),
            max_connection_interval: Duration::from_micros(7_500),
            supervision_timeout: Duration::from_millis(100),
            ..Default::default()
        };
        assert!(params.is_valid());
        params.min_connection_interval = Duration::from_micros(7_499);
        assert!(!params.is_valid());
        params.min_connection_interval = Duration::from_secs(4);
        params.max_connection_interval = Duration::from_secs(4);
        params.supervision_timeout = Duration::from_secs(8);
        assert!(!params.is_valid()); // equality is not a safe supervision margin
        params.supervision_timeout = Duration::from_millis(8_001);
        assert!(params.is_valid());
        params.max_connection_interval = Duration::from_micros(4_000_001);
        assert!(!params.is_valid());
        params = RequestedConnParams {
            min_connection_interval: Duration::from_micros(7_500),
            max_connection_interval: Duration::from_micros(7_500),
            max_latency: 499,
            supervision_timeout: Duration::from_secs(32),
            ..Default::default()
        };
        assert!(params.is_valid());
        params.max_latency = 500;
        assert!(!params.is_valid());
        params.max_latency = 0;
        params.supervision_timeout = Duration::from_micros(32_000_001);
        assert!(!params.is_valid());
        params.supervision_timeout = Duration::from_millis(99);
        assert!(!params.is_valid());
        params.supervision_timeout = Duration::from_millis(100);
        params.min_event_length = Duration::from_micros(1);
        assert!(!params.is_valid());
        params.max_event_length = Duration::from_micros(1);
        assert!(params.is_valid());
    }
}
