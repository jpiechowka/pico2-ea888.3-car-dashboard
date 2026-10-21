#![no_std]
#![allow(dead_code)]
#![allow(unused_variables)]
#![allow(clippy::needless_lifetimes)]
#![doc = include_str!(concat!("../", env!("CARGO_PKG_README")))]
#![warn(missing_docs)]

#[cfg(all(feature = "discovery-no-security", feature = "security"))]
compile_error!(
    "discovery-no-security forbids the security feature; build the BLE probe separately from pairing-enabled firmware"
);

use core::cell::{Cell, RefCell};
use core::mem::{ManuallyDrop, MaybeUninit};

use advertise::AdvertisementDataError;
use bt_hci::cmd::le::LeReadMinimumSupportedConnectionInterval;
#[cfg(feature = "shorter-connection-intervals")]
use bt_hci::cmd::le::LeSetHostFeatureV2;
use bt_hci::cmd::status::ReadRssi;
use bt_hci::cmd::{AsyncCmd, SyncCmd};
use bt_hci::param::{AddrKind, BdAddr, ConnHandle};
use bt_hci::FromHciBytesError;
use embassy_time::Duration;
#[cfg(feature = "security")]
use heapless::{Vec, VecView};

use crate::att::AttErrorCode;
use crate::channel_manager::ChannelStorage;
use crate::connection::Connection;
use crate::connection_manager::ConnectionStorage;
#[cfg(feature = "security")]
pub use crate::security_manager::{
    BondInformation, IdentityResolvingKey, LongTermKey, OobData, Reason as PairingFailedReason,
};
pub use crate::types::capabilities::IoCapabilities;

/// Feature-selected logging and formatting macros.
mod fmt;

#[cfg(not(any(feature = "central", feature = "peripheral")))]
compile_error!("Must enable at least one of the `central` or `peripheral` features");

/// ATT opcodes, typed PDUs and bounded wire codecs.
pub mod att;
#[cfg(feature = "central")]
/// Initiator procedures and exclusive connection cancellation.
pub mod central;
/// Fixed dynamic-channel slots, references, credit accounting and signaling.
mod channel_manager;
/// Internal fixed/variable byte codecs.
mod codec;
/// Single-procedure admission and cancellation acknowledgment.
mod command;
/// Generated bounded host configuration constants.
pub mod config;
/// Connection slots, retained ACL ownership and event queues.
mod connection_manager;
/// Checked read/write cursor composition.
mod cursor;
#[cfg(feature = "iso")]
/// Optional ISO/CIS command and data adapter.
pub mod iso;
#[cfg(feature = "default-packet-pool")]
/// Static affine packet allocation and per-slot backing.
mod packet_pool;
/// Owned packet length and SDU views.
mod pdu;
#[cfg(feature = "peripheral")]
/// Advertising procedures and accepted-link ownership.
pub mod peripheral;
#[cfg(feature = "security")]
/// Pairing, encryption, identity and in-memory bonds.
mod security_manager;
/// Shared BLE value and wire definitions.
pub mod types;

#[cfg(feature = "central")]
use central::*;
#[cfg(feature = "peripheral")]
use peripheral::*;

/// Legacy/extended advertising parameters, payloads and set state.
pub mod advertise;
/// Accepted connection owners and link control APIs.
pub mod connection;
#[cfg(feature = "gatt")]
/// Bounded Generic Access service/advertising helpers.
pub mod gap;
/// Dynamic-channel handles, listeners and split send/receive APIs.
pub mod l2cap;
#[cfg(feature = "scan")]
/// Advertisement report views and exclusive scanning procedures.
pub mod scan;

#[cfg(test)]
/// Native fake controller with deliberate unsupported-operation panics.
pub(crate) mod mock_controller;

/// Host runner, HCI dispatch and cancellation machinery.
pub(crate) mod host;

use host::{AdvHandleState, BleHost, HostMetrics, Runner};

pub mod prelude {
    //! Convenience include of most commonly used types.
    pub use bt_hci::controller::ExternalController;
    pub use bt_hci::param::{AddrKind, BdAddr, LeConnRole as Role, PhyKind, PhyMask};
    pub use bt_hci::uuid::*;
    #[cfg(feature = "derive")]
    pub use heapless::String as HeaplessString;
    #[cfg(feature = "derive")]
    pub use trouble_host_macros::*;

    pub use super::att::AttErrorCode;
    pub use super::{BleHostError, Controller, Error, HostResources, Packet, PacketPool, Stack, StackBuilder};
    #[cfg(feature = "peripheral")]
    pub use crate::advertise::*;
    #[cfg(feature = "gatt")]
    pub use crate::attribute::*;
    #[cfg(feature = "gatt")]
    pub use crate::attribute_server::*;
    #[cfg(feature = "central")]
    pub use crate::central::*;
    pub use crate::connection::{ConnectRateParams, *};
    #[cfg(feature = "gatt")]
    pub use crate::gap::*;
    #[cfg(feature = "gatt")]
    pub use crate::gatt::*;
    pub use crate::host::{ControlRunner, EventHandler, HostMetrics, Runner, RxRunner, TxRunner};
    #[cfg(feature = "iso")]
    pub use crate::iso::Iso;
    pub use crate::l2cap::*;
    #[cfg(feature = "default-packet-pool")]
    pub use crate::packet_pool::DefaultPacketPool;
    pub use crate::pdu::Sdu;
    #[cfg(feature = "peripheral")]
    pub use crate::peripheral::*;
    #[cfg(feature = "scan")]
    pub use crate::scan::*;
    #[cfg(feature = "security")]
    pub use crate::security_manager::{
        BondInformation, IdentityResolvingKey, LongTermKey, OobData, Reason as PairingFailedReason,
    };
    pub use crate::types::capabilities::IoCapabilities;
    #[cfg(feature = "gatt")]
    pub use crate::types::gatt_traits::{AsGatt, FixedGattValue, FromGatt};
    pub use crate::{Address, Identity};
}

#[cfg(feature = "gatt")]
/// GATT attribute table, declarations and typed value access.
pub mod attribute;
#[cfg(feature = "gatt")]
/// Server request processing, permissions and per-client CCCD state.
mod attribute_server;
#[cfg(feature = "gatt")]
/// GATT request/reply and client discovery/notification APIs.
pub mod gatt;

/// Six-byte Bluetooth address plus HCI kind; constructors store bytes without validating random-address bit patterns.
/// Equality normalizes public/identity and random/identity kind pairs. Privacy addresses can rotate;
/// their use alone does not establish anonymity or prevent tracking through other advertised data.
#[derive(Debug, Clone, Copy, Default, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Address {
    /// HCI address kind; equality compares only its low bit to normalize identity reports.
    pub kind: AddrKind,
    /// Six HCI-order bytes, least significant first; display reverses them.
    pub addr: BdAddr,
}

impl PartialEq for Address {
    /// Compare two addresses, normalizing HCI identity address types.
    ///
    /// In HCI events the controller may report a peer's address type as 0x02 (Public Identity) or
    /// 0x03 (Random Static Identity) when it resolved the peer's RPA via the resolving list. These
    /// are semantically equivalent to 0x00 (Public) and 0x01 (Random) respectively, so this
    /// implementation treats them as equal when comparing.
    /// Returns byte/kind-normalized equality with `other`; no IRK resolution occurs.
    fn eq(&self, other: &Self) -> bool {
        self.addr == other.addr && self.kind.as_raw() & 1 == other.kind.as_raw() & 1
    }
}

impl Address {
    /// Create a new address with the given kind and value.
    /// Returns `kind` and HCI-order `addr` unchanged; does not validate address subtype bits.
    pub const fn new(kind: AddrKind, addr: BdAddr) -> Self {
        Self { kind, addr }
    }

    /// Create a new random address.
    /// Returns RANDOM-kind address storing HCI-order `val` unchanged; caller chooses valid random-address bits.
    pub fn random(val: [u8; 6]) -> Self {
        Self {
            kind: AddrKind::RANDOM,
            addr: BdAddr::new(val),
        }
    }

    /// To bytes
    /// Returns raw kind followed by six most-significant-first address bytes; this differs from HCI address byte order.
    pub fn to_bytes(&self) -> [u8; 7] {
        let mut bytes = [0; 7];
        bytes[0] = self.kind.into_inner();
        let mut addr_bytes = self.addr.into_inner();
        addr_bytes.reverse();
        bytes[1..].copy_from_slice(&addr_bytes);
        bytes
    }
}

impl core::fmt::Display for Address {
    /// Writes uppercase colon-separated address bytes to `f`, omitting kind; returns formatter errors.
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let a = self.addr.into_inner();
        write!(
            f,
            "{:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}",
            a[5], a[4], a[3], a[2], a[1], a[0]
        )
    }
}

#[cfg(feature = "defmt")]
impl defmt::Format for Address {
    /// Writes the address to `fmt` in uppercase display order; kind is omitted.
    fn format(&self, fmt: defmt::Formatter) {
        let a = self.addr.into_inner();
        defmt::write!(
            fmt,
            "{:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}",
            a[5],
            a[4],
            a[3],
            a[2],
            a[1],
            a[0]
        )
    }
}

/// Identity of a peer device
///
/// Sometimes we have to save both the address and the IRK.
/// Because sometimes the peer uses the static or public address even though the IRK is sent.
/// In this case, the IRK exists but the used address is not RPA.
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Identity {
    /// Identity address (random static or public)
    pub addr: Address,

    /// Identity Resolving Key
    #[cfg(feature = "security")]
    pub irk: Option<IdentityResolvingKey>,
}

#[cfg(feature = "defmt")]
impl defmt::Format for Identity {
    /// Writes address and unredacted optional IRK to `fmt`; explicit callers must keep this out of incidental diagnostics.
    fn format(&self, fmt: defmt::Formatter) {
        defmt::write!(fmt, "Addr({}) ", self.addr);
        #[cfg(feature = "security")]
        defmt::write!(fmt, "Irk({:X})", self.irk);
    }
}

impl From<Address> for Identity {
    /// Returns identity using `addr` with no IRK.
    fn from(addr: Address) -> Self {
        Self {
            addr,
            #[cfg(feature = "security")]
            irk: None,
        }
    }
}

impl Identity {
    /// Check whether the address matches the identity.
    ///
    /// Matches if the address is an exact match (kind + addr) or if the IRK can resolve it.
    /// Returns true for normalized address equality with `address`, or successful IRK resolution of its raw bytes when security is enabled; explicit kind is not separately checked on the resolution branch.
    pub fn match_address(&self, address: &Address) -> bool {
        if self.addr == *address {
            return true;
        }
        #[cfg(feature = "security")]
        if let Some(irk) = self.irk {
            return irk.resolve_address(&address.addr);
        }
        false
    }

    /// Check whether the given identity matches current identity
    /// Returns true when `identity` matches normalized address, either IRK resolves the opposite raw address, or both IRKs match. This matching relation is broader than structural Eq.
    pub fn match_identity(&self, identity: &Identity) -> bool {
        if self.addr == identity.addr {
            return true;
        }
        #[cfg(feature = "security")]
        {
            if let Some(irk) = self.irk {
                if irk.resolve_address(&identity.addr.addr) {
                    return true;
                }
            }
            if let Some(irk) = identity.irk {
                if let Some(current_irk) = self.irk {
                    if irk == current_irk {
                        return true;
                    }
                }
                if irk.resolve_address(&self.addr.addr) {
                    return true;
                }
            }
        }
        false
    }
}

/// Errors returned by the host.
#[derive(Debug)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum BleHostError<E> {
    /// Error from the controller.
    Controller(
        /// Original transport-specific failure.
        E,
    ),
    /// Error from the host.
    BleHost(
        /// Host validation/state/protocol failure.
        Error,
    ),
}

/// How many bytes of invalid data to capture in the error variants before truncating.
pub const MAX_INVALID_DATA_LEN: usize = 16;

/// Errors related to Host.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum Error {
    /// Error encoding parameters for HCI commands.
    Hci(
        /// Controller command/status error byte.
        bt_hci::param::Error,
    ),
    /// Error decoding responses from HCI commands.
    HciDecode(
        /// Locked HCI decoder size/value failure.
        FromHciBytesError,
    ),
    /// Controller hardware fault; the code is vendor-defined, not an HCI status.
    HardwareError(
        /// Unchanged vendor-defined fault code.
        u8,
    ),
    /// Error from the Attribute Protocol.
    Att(
        /// ATT error code, including unknown raw values.
        AttErrorCode,
    ),
    #[cfg(feature = "security")]
    /// Error from the security manager
    Security(
        /// Pairing/security failure reason.
        PairingFailedReason,
    ),
    /// Insufficient space in the buffer.
    InsufficientSpace,
    /// Invalid value.
    InvalidValue,

    /// Unexpected data length.
    ///
    /// This happens if the attribute data length doesn't match the input length size,
    /// and the attribute is deemed as *not* having variable length due to the characteristic's
    /// `MAX_SIZE` and `MIN_SIZE` being defined as equal.
    UnexpectedDataLength {
        /// Expected length.
        expected: usize,
        /// Actual length.
        actual: usize,
    },

    /// Error converting from GATT value.
    CannotConstructGattValue(
        /// Up to sixteen failed input bytes, zero-padded when shorter; may contain private payload.
        [u8; MAX_INVALID_DATA_LEN],
    ),

    /// Scan config filter accept list is empty.
    ConfigFilterAcceptListIsEmpty,

    /// Unexpected GATT response.
    UnexpectedGattResponse,

    /// Received characteristic declaration data shorter than the minimum required length (5 bytes).
    MalformedCharacteristicDeclaration {
        /// Expected length.
        expected: usize,
        /// Actual length.
        actual: usize,
    },

    /// Failed to decode the data structure within a characteristic declaration attribute value.
    InvalidCharacteristicDeclarationData,

    /// Failed to finalize the packet.
    FailedToFinalize {
        /// Expected length.
        expected: usize,
        /// Actual length.
        actual: usize,
    },

    /// Codec error.
    CodecError(
        /// Underlying invalid-value/capacity codec failure.
        codec::Error,
    ),

    /// Extended advertising not supported.
    ExtendedAdvertisingNotSupported,

    /// Invalid UUID length.
    InvalidUuidLength(
        /// Observed UUID byte count; supported widths are2,4,16 in the generic UUID codec.
        usize,
    ),

    /// Error decoding advertisement data.
    Advertisement(
        /// Advertising element encoding/length failure.
        AdvertisementDataError,
    ),
    /// L2CAP credit-based connection refused by the peer.
    L2capConnectError(
        /// Peer connection-refusal result code.
        crate::types::l2cap::LeCreditConnResultCode,
    ),
    /// Invalid l2cap channel id provided.
    InvalidChannelId,
    /// No l2cap channel available.
    NoChannelAvailable,
    /// Resource not found.
    NotFound,
    /// Invalid state.
    InvalidState,
    /// Out of memory.
    OutOfMemory,
    /// Unsupported operation.
    NotSupported,
    /// L2cap channel closed.
    ChannelClosed,
    /// Operation timed out.
    Timeout,
    /// Controller is busy.
    Busy,
    /// No send permits available.
    NoPermits,
    /// Connection is disconnected.
    Disconnected,
    /// Connection limit has been reached.
    ConnectionLimitReached,
    /// GATT subscriber limit has been reached.
    ///
    /// The limit can be modified using the `gatt-client-notification-max-subscribers-N` features.
    GattSubscriberLimitReached,
    /// Resource is already in use.
    AlreadyInUse,
    /// Other error.
    Other,
}

impl<E> From<Error> for BleHostError<E> {
    /// Returns host-wrapped `value` without losing its variant/payload.
    fn from(value: Error) -> Self {
        Self::BleHost(value)
    }
}

impl From<FromHciBytesError> for Error {
    /// Returns HciDecode retaining `error`.
    fn from(error: FromHciBytesError) -> Self {
        Self::HciDecode(error)
    }
}

impl From<AttErrorCode> for Error {
    /// Returns Att retaining raw `error` semantics.
    fn from(error: AttErrorCode) -> Self {
        Self::Att(error)
    }
}

impl<E> From<bt_hci::cmd::Error<E>> for BleHostError<E> {
    /// Returns Hci for controller-status `error`, or Controller for transport I/O failure.
    fn from(error: bt_hci::cmd::Error<E>) -> Self {
        match error {
            bt_hci::cmd::Error::Hci(p) => Self::BleHost(Error::Hci(p)),
            bt_hci::cmd::Error::Io(p) => Self::Controller(p),
        }
    }
}

impl<E> From<bt_hci::param::Error> for BleHostError<E> {
    /// Returns host Hci preserving controller status `error`.
    fn from(error: bt_hci::param::Error) -> Self {
        Self::BleHost(Error::Hci(error))
    }
}

impl From<codec::Error> for Error {
    /// Returns InsufficientSpace for capacity `error`; invalid codec values retain CodecError.
    fn from(error: codec::Error) -> Self {
        match error {
            codec::Error::InsufficientSpace => Error::InsufficientSpace,
            codec::Error::InvalidValue => Error::CodecError(error),
        }
    }
}

impl<E> From<codec::Error> for BleHostError<E> {
    /// Returns host InsufficientSpace or InvalidValue from codec `error`; unlike Error conversion, invalid codec wrapper detail is flattened.
    fn from(error: codec::Error) -> Self {
        match error {
            codec::Error::InsufficientSpace => BleHostError::BleHost(Error::InsufficientSpace),
            codec::Error::InvalidValue => BleHostError::BleHost(Error::InvalidValue),
        }
    }
}

use bt_hci::cmd::controller_baseband::*;
use bt_hci::cmd::info::*;
use bt_hci::cmd::le::*;
use bt_hci::cmd::link_control::*;
use bt_hci::controller::{ControllerCmdAsync, ControllerCmdSync};

/// Trait for security-related controller commands.
///
/// When the `security` feature is enabled, this requires the controller to support
/// encryption, resolving list and address resolution HCI commands. When disabled, this is
/// automatically implemented for all controllers.
#[cfg(feature = "security")]
pub trait SecurityCmds:
    bt_hci::controller::Controller
    + ControllerCmdSync<LeLongTermKeyRequestReply>
    + ControllerCmdAsync<LeEnableEncryption>
    + ControllerCmdSync<LeAddDeviceToResolvingList>
    + ControllerCmdSync<LeRemoveDeviceFromResolvingList>
    + ControllerCmdSync<LeClearResolvingList>
    + ControllerCmdSync<LeSetAddrResolutionEnable>
    + ControllerCmdSync<LeSetResolvablePrivateAddrTimeout>
    + ControllerCmdSync<LeSetPrivacyMode>
    + ControllerCmdSync<LeRand>
{
}

#[cfg(feature = "security")]
impl<
        C: bt_hci::controller::Controller
            + ControllerCmdSync<LeLongTermKeyRequestReply>
            + ControllerCmdAsync<LeEnableEncryption>
            + ControllerCmdSync<LeAddDeviceToResolvingList>
            + ControllerCmdSync<LeRemoveDeviceFromResolvingList>
            + ControllerCmdSync<LeClearResolvingList>
            + ControllerCmdSync<LeSetAddrResolutionEnable>
            + ControllerCmdSync<LeSetResolvablePrivateAddrTimeout>
            + ControllerCmdSync<LeSetPrivacyMode>
            + ControllerCmdSync<LeRand>,
    > SecurityCmds for C
{
}

/// Auto-implemented when security is not enabled.
#[cfg(not(feature = "security"))]
pub trait SecurityCmds: bt_hci::controller::Controller {}

#[cfg(not(feature = "security"))]
impl<C: bt_hci::controller::Controller> SecurityCmds for C {}

/// Auto-implemented when the `iso` feature is enabled.
#[cfg(feature = "iso")]
pub trait IsoStreamCmds: bt_hci::controller::Controller + ControllerCmdSync<LeSetHostFeature> {}

#[cfg(feature = "iso")]
impl<C: bt_hci::controller::Controller + ControllerCmdSync<LeSetHostFeature>> IsoStreamCmds for C {}

/// Auto-implemented when `iso` is not enabled.
#[cfg(not(feature = "iso"))]
pub trait IsoStreamCmds: bt_hci::controller::Controller {}

#[cfg(not(feature = "iso"))]
impl<C: bt_hci::controller::Controller> IsoStreamCmds for C {}

/// Auto-implemented when the `subrating` feature is enabled.
#[cfg(feature = "subrating")]
pub trait SubratingCmds: bt_hci::controller::Controller + ControllerCmdSync<LeSetHostFeature> {}

#[cfg(feature = "subrating")]
impl<C: bt_hci::controller::Controller + ControllerCmdSync<LeSetHostFeature>> SubratingCmds for C {}

/// Auto-implemented when subrating is not enabled.
#[cfg(not(feature = "subrating"))]
pub trait SubratingCmds: bt_hci::controller::Controller {}

#[cfg(not(feature = "subrating"))]
impl<C: bt_hci::controller::Controller> SubratingCmds for C {}

/// Requires LE Set Host Feature V2 command support when shorter-connection-intervals is enabled.
#[cfg(feature = "shorter-connection-intervals")]
pub trait ShortConnIntervalCmds: bt_hci::controller::Controller + ControllerCmdSync<LeSetHostFeatureV2> {}

#[cfg(feature = "shorter-connection-intervals")]
impl<C: bt_hci::controller::Controller + ControllerCmdSync<LeSetHostFeatureV2>> ShortConnIntervalCmds for C {}

/// Auto-implemented when `shorter-connection-intervals` is not enabled.
#[cfg(not(feature = "shorter-connection-intervals"))]
pub trait ShortConnIntervalCmds: bt_hci::controller::Controller {}

#[cfg(not(feature = "shorter-connection-intervals"))]
impl<C: bt_hci::controller::Controller> ShortConnIntervalCmds for C {}

/// Trait that defines the controller implementation required by the host.
///
/// The controller must implement the required commands and events to be able to be used with Trouble.
pub trait Controller:
    bt_hci::controller::Controller
    + embedded_io::ErrorType<Error: crate::fmt::Format>
    + ControllerCmdSync<LeReadBufferSize>
    + ControllerCmdSync<Disconnect>
    + ControllerCmdSync<SetEventMask>
    + ControllerCmdSync<SetEventMaskPage2>
    + ControllerCmdSync<LeSetEventMask>
    + ControllerCmdSync<LeSetRandomAddr>
    + ControllerCmdSync<HostBufferSize>
    + ControllerCmdAsync<LeConnUpdate>
    + ControllerCmdSync<LeReadFilterAcceptListSize>
    + ControllerCmdSync<SetControllerToHostFlowControl>
    + ControllerCmdSync<Reset>
    + ControllerCmdSync<ReadRssi>
    + ControllerCmdSync<LeCreateConnCancel>
    + ControllerCmdSync<LeSetScanEnable>
    + ControllerCmdSync<LeSetExtScanEnable>
    + ControllerCmdAsync<LeCreateConn>
    + ControllerCmdSync<LeClearFilterAcceptList>
    + ControllerCmdSync<LeAddDeviceToFilterAcceptList>
    + ControllerCmdSync<LeSetAdvEnable>
    + for<'t> ControllerCmdSync<LeSetExtAdvEnable<'t>>
    + for<'t> ControllerCmdSync<HostNumberOfCompletedPackets<'t>>
    + ControllerCmdSync<LeReadBufferSize>
    + ControllerCmdSync<LeSetAdvData>
    + ControllerCmdSync<LeSetAdvParams>
    + ControllerCmdSync<LeSetAdvEnable>
    + ControllerCmdSync<LeSetScanResponseData>
    + ControllerCmdSync<ReadBdAddr>
    + SecurityCmds
    + SubratingCmds
    + IsoStreamCmds
    + ShortConnIntervalCmds
{
}

impl<
        C: bt_hci::controller::Controller
            + embedded_io::ErrorType<Error: crate::fmt::Format>
            + ControllerCmdSync<LeReadBufferSize>
            + ControllerCmdSync<Disconnect>
            + ControllerCmdSync<SetEventMask>
            + ControllerCmdSync<SetEventMaskPage2>
            + ControllerCmdSync<LeSetEventMask>
            + ControllerCmdSync<LeSetRandomAddr>
            + ControllerCmdSync<HostBufferSize>
            + ControllerCmdAsync<LeConnUpdate>
            + ControllerCmdSync<LeReadFilterAcceptListSize>
            + ControllerCmdSync<LeClearFilterAcceptList>
            + ControllerCmdSync<LeAddDeviceToFilterAcceptList>
            + ControllerCmdSync<SetControllerToHostFlowControl>
            + ControllerCmdSync<Reset>
            + ControllerCmdSync<ReadRssi>
            + ControllerCmdSync<LeSetScanEnable>
            + ControllerCmdSync<LeSetExtScanEnable>
            + ControllerCmdSync<LeCreateConnCancel>
            + ControllerCmdAsync<LeCreateConn>
            + ControllerCmdSync<LeSetAdvEnable>
            + for<'t> ControllerCmdSync<LeSetExtAdvEnable<'t>>
            + for<'t> ControllerCmdSync<HostNumberOfCompletedPackets<'t>>
            + ControllerCmdSync<LeReadBufferSize>
            + ControllerCmdSync<LeSetAdvData>
            + ControllerCmdSync<LeSetAdvParams>
            + ControllerCmdSync<LeSetAdvEnable>
            + ControllerCmdSync<LeSetScanResponseData>
            + ControllerCmdSync<ReadBdAddr>
            + SecurityCmds
            + SubratingCmds
            + IsoStreamCmds
            + ShortConnIntervalCmds,
    > Controller for C
{
}

/// A Packet is a byte buffer for packet data.
/// AsRef/AsMut expose initialized backing bytes; logical PDU length is tracked separately by the host.
/// Implementations must preserve ordinary Rust slice uniqueness and return their owned allocation on Drop as applicable.
pub trait Packet: Sized + AsRef<[u8]> + AsMut<[u8]> {}

/// A Packet Pool that can allocate packets of the desired size.
///
/// MTU is backing byte capacity, used for whole reassembly and outbound headers/payloads.
/// Each allocation must expose at least MTU initialized bytes with exclusive mutable access.
/// Allocation/Drop must be usable from the host executor without awaiting; capacity is finite for the default pool.
/// GATT callers require MTU in 27..=65535 (four header bytes plus minimum ATT MTU23);
/// smaller/larger custom capacities are not centrally validated and can underflow or truncate in constructors.
pub trait PacketPool: 'static {
    /// Packet type provided by this pool.
    type Packet: Packet;

    /// The maximum size a packet can have.
    const MTU: usize;

    /// Returns an owned initialized buffer with at least MTU bytes, or None on exhaustion. Allocation is shared across TX/RX/reassembly; no priority or wait is supplied by this trait.
    fn allocate() -> Option<Self::Packet>;

    /// Returns total allocation capacity in packets, not the currently free count.
    fn capacity() -> usize;
}

/// HostResources holds the resources used by the host.
///
/// The l2cap packet pool is used by the host to handle inbound data, by allocating space for
/// incoming packets and dispatching to the appropriate connection and channel.
/// Callers keep CONNS at most 256 and CHANNELS at most 64: local indices are u8 and
/// LE dynamic CIDs span 0x0040..=0x007f. These generic bounds are not centrally validated.
pub struct HostResources<
    P: PacketPool,
    const CONNS: usize,
    const CHANNELS: usize,
    const ADV_SETS: usize = 1,
    const BONDS: usize = 10,
> {
    /// Manually dropped host state whose internal lifetime is narrowed during exclusive resource borrowing; never read through its nominal static type.
    state: MaybeUninit<ManuallyDrop<host::HostState<'static, P>>>,
    /// Fixed initialized-on-build link slots with bounded inbound queues/reassembly.
    connections: MaybeUninit<RefCell<[ConnectionStorage<P::Packet>; CONNS]>>,
    /// Fixed initialized-on-build dynamic-channel slots and queued SDUs.
    channels: MaybeUninit<RefCell<[ChannelStorage<P::Packet>; CHANNELS]>>,
    /// Fixed advertising-set observation storage initialized at host construction.
    advertise_handles: MaybeUninit<RefCell<[AdvHandleState; ADV_SETS]>>,
    #[cfg(feature = "security")]
    /// In-memory bounded bond vector initialized empty at host construction; no persistence is implied.
    bond_storage: MaybeUninit<RefCell<Vec<BondInformation, BONDS>>>,
}

impl<P: PacketPool, const CONNS: usize, const CHANNELS: usize, const ADV_SETS: usize, const BONDS: usize> Default
    for HostResources<P, CONNS, CHANNELS, ADV_SETS, BONDS>
{
    /// Returns uninitialized resource backing via new; controller/table initialization occurs only in the free new function.
    fn default() -> Self {
        Self::new()
    }
}

impl<P: PacketPool, const CONNS: usize, const CHANNELS: usize, const ADV_SETS: usize, const BONDS: usize>
    HostResources<P, CONNS, CHANNELS, ADV_SETS, BONDS>
{
    /// Create a new instance of host resources.
    /// Returns uninitialized fixed backing storage with no allocation, controller operation or packet ownership.
    pub const fn new() -> Self {
        Self {
            state: MaybeUninit::uninit(),
            connections: MaybeUninit::uninit(),
            channels: MaybeUninit::uninit(),
            advertise_handles: MaybeUninit::uninit(),
            #[cfg(feature = "security")]
            bond_storage: MaybeUninit::uninit(),
        }
    }
}

/// Create a new instance of the BLE host using the provided controller implementation and
/// the resource configuration
/// Returns a builder owning `controller` and exclusively borrowing `resources`; initializes fixed tables and empty bonds without controller I/O. Previous live borrows prevent reuse; dropping the builder or stack releases queued/partial packets before reuse. Forgotten owners may leak resources.
pub fn new<
    'resources,
    C: Controller,
    P: PacketPool,
    const CONNS: usize,
    const CHANNELS: usize,
    const ADV_SETS: usize,
    const BONDS: usize,
>(
    controller: C,
    resources: &'resources mut HostResources<P, CONNS, CHANNELS, ADV_SETS, BONDS>,
) -> StackBuilder<'resources, C, P> {
    let connections: &'resources RefCell<[ConnectionStorage<P::Packet>]> = resources
        .connections
        .write(RefCell::new([const { ConnectionStorage::new() }; CONNS]));

    let channels: &'resources RefCell<[ChannelStorage<P::Packet>]> = resources
        .channels
        .write(RefCell::new([const { ChannelStorage::new() }; CHANNELS]));

    let advertise_handles: &'resources RefCell<[AdvHandleState]> = resources
        .advertise_handles
        .write(RefCell::new([AdvHandleState::None; ADV_SETS]));

    #[cfg(feature = "security")]
    let bond_storage: &'resources RefCell<VecView<BondInformation>> =
        resources.bond_storage.write(RefCell::new(Vec::new()));

    // SAFETY: Narrows the host field's lifetime from `'static` to `'resources`. Sound because:
    // - HostState is covariant in 'd so the types differ only in a lifetime (identical layout).
    // - The returned StackBuilder/Stack exclusively borrows the HostResources for 'resources, preventing re-entry into
    //   this function while the narrowed-lifetime data is live.
    // - The `state` field is private, MaybeUninit (no auto-drop), and only ever accessed by this function (which
    //   overwrites via write()), so the narrowed lifetime can never be observed through the original `'static` type —
    //   even if the StackBuilder/Stack is forgotten.
    let host_state: &'resources mut MaybeUninit<ManuallyDrop<host::HostState<'resources, P>>> =
        unsafe { core::mem::transmute(&mut resources.state) };

    let host_state: &'resources mut ManuallyDrop<host::HostState<'resources, P>> =
        host_state.write(ManuallyDrop::new(host::HostState::new(
            connections,
            channels,
            advertise_handles,
            #[cfg(feature = "security")]
            bond_storage,
        )));

    StackBuilder {
        host_state: Some(host_state),
        controller: Some(controller),
    }
}

/// Contains the host stack
pub struct Stack<'stack, C, P: PacketPool> {
    /// Exclusive owner of initialized host state; Drop runs its destructor once after borrowed handles expire.
    host_state: &'stack mut ManuallyDrop<host::HostState<'stack, P>>,
    /// Owned HCI driver moved from builder; host operations borrow it.
    controller: C,
    /// One-shot runner admission latch; dropping a runner does not clear it.
    runner_taken: Cell<bool>,
}

impl<C, P: PacketPool> Stack<'_, C, P> {
    /// True only when no central create/cancel is pending and every connection
    /// slot has reached terminal Disconnected state. Keep the runner alive until
    /// this is true; a local disconnect request alone is not completion.
    /// Returns whether initiator admission is idle and every link is terminally disconnected; retained owners/queued work may still exist.
    pub fn connections_idle(&self) -> bool {
        self.host_state.connect_command_state.is_idle() && self.host_state.connections.all_disconnected()
    }
}

impl<'stack, C, P: PacketPool> Drop for Stack<'stack, C, P> {
    /// Drops initialized host state exactly once after borrowed handles expire; connection queues/partial ACLs are released before channel queues/partial SDUs. Does not issue controller disconnect/reset commands.
    fn drop(&mut self) {
        // SAFETY: host was fully initialized in new() and has not been dropped.
        // Stack is the sole owner responsible for dropping BleHost.
        // All shared &HostState references (in Runner, Central, etc.) have already
        // been dropped (reverse drop order), so no aliasing conflict.
        unsafe { ManuallyDrop::drop(self.host_state) }
    }
}

/// Builder for configuring the BLE stack before use.
///
/// Call [`build()`](StackBuilder::build) to finalize configuration and obtain the [`Stack`].
pub struct StackBuilder<'stack, C, P: PacketPool> {
    /// Initialized state ownership before build; None after transfer prevents builder double-drop.
    pub(crate) host_state: Option<&'stack mut ManuallyDrop<host::HostState<'stack, P>>>,
    /// Controller ownership before build; taken exactly once with host state.
    controller: Option<C>,
}

impl<'stack, C, P: PacketPool> Drop for StackBuilder<'stack, C, P> {
    /// Drops initialized host state only if build did not transfer it, releasing queued/partial packets; controller ownership drops normally.
    fn drop(&mut self) {
        if let Some(host_state) = &mut self.host_state {
            // SAFETY: host was fully initialized in new() and has not been dropped.
            // `build()` was never called, leaving StackBuilder as the sole owner
            // responsible for dropping BleHost.
            unsafe { ManuallyDrop::drop(host_state) }
        }
    }
}

impl<'stack, C: Controller, P: PacketPool> StackBuilder<'stack, C, P> {
    /// Returns mutable initialized builder state; internal access after ownership transfer would panic.
    fn host_state(&mut self) -> &mut host::HostState<'stack, P> {
        self.host_state.as_mut().unwrap()
    }

    /// Register an L2CAP SPSM (Simplified Protocol/Service Multiplexer) for accepting incoming connections.
    /// Returns this builder after registering `spsm` in 1..=255; out-of-range values are ignored. Reserved service admission also depends on allow-reserved-l2cap-psu.
    pub fn register_l2cap_spsm(mut self, spsm: u16) -> Self {
        self.host_state().channels.register_spsm(spsm);
        self
    }

    /// Set the random address used by this host.
    /// Returns this builder after storing `address` and security local identity. No random-kind/bit validation or controller I/O occurs; caller supplies a valid random address for startup.
    pub fn set_random_address(mut self, address: Address) -> Self {
        self.host_state().address.replace(address);
        #[cfg(feature = "security")]
        self.host_state()
            .connections
            .security_manager
            .set_local_address(address);
        self
    }

    /// Enable BLE address privacy with the given Identity Resolving Key (IRK).
    ///
    /// When privacy is enabled, Resolvable Private Addresses (RPAs) are generated
    /// that rotate periodically, reducing stable-address exposure while allowing bonded peers to
    /// resolve the device's identity.
    ///
    /// The IRK should be persisted across reboots so bonded peers can continue to resolve
    /// our RPAs. Generate a new IRK using a CSPRNG for first-time setup.
    ///
    /// After bonds are added or removed (either directly or via pairing), the controller's
    /// resolving list is updated automatically the next time advertising, scanning, and
    /// connecting are all idle. Applications should ensure periodic idle windows to allow
    /// resolving list updates to take effect.
    #[cfg(feature = "security")]
    /// Returns this builder after storing local `irk`; startup/control later apply controller privacy. Persist keys externally if peer resolution must survive reboot.
    pub fn enable_privacy(mut self, irk: IdentityResolvingKey) -> Self {
        self.host_state().connections.security_manager.set_local_irk(irk);
        self
    }

    /// Set the RPA (Resolvable Private Address) rotation timeout.
    ///
    /// New RPAs will be generated after this duration. Note that host generated RPAs
    /// (used for active scanning of and connecting to unbonded devices) will only rotate
    /// when scanning, connection initiation, and legacy advertising are all idle.
    ///
    /// Default is 900 seconds (15 minutes) per the BLE specification.
    #[cfg(feature = "security")]
    /// Returns this builder after storing `timeout` without validation; use 1..=3600 whole seconds. Startup command encoding can panic outside representable HCI duration and local expiration is separate.
    pub fn set_rpa_timeout(mut self, timeout: Duration) -> Self {
        self.host_state().rpa_timeout.set(timeout);
        self
    }

    /// Set the IO capabilities used by the security manager.
    ///
    /// Only relevant if the feature `security` is enabled.
    #[cfg(feature = "security")]
    /// Returns this builder after storing `io_capabilities` for future pairing feature exchange.
    pub fn set_io_capabilities(mut self, io_capabilities: IoCapabilities) -> Self {
        self.host_state()
            .connections
            .security_manager
            .set_io_capabilities(io_capabilities);
        self
    }

    /// Configures the Secure Connections peripheral display passkey; other display paths generate random values.
    /// Caller must supply 0..=999999; this API does not validate the range. None restores random generation.
    #[cfg(feature = "security")]
    /// Returns this builder after storing optional `passkey`; only the Secure Connections peripheral display path uses the override, with caller-owned six-digit validation.
    pub fn set_passkey(mut self, passkey: Option<u32>) -> Self {
        self.host_state().connections.security_manager.set_passkey(passkey);
        self
    }

    /// Enable or disable secure connections only mode.
    ///
    /// When enabled, legacy pairing is rejected even if the `legacy-pairing` feature is compiled in.
    /// This matches the BLE spec's "Secure Connections Only Mode" (Vol 3, Part C, Section 10.2.4).
    ///
    /// Only relevant if the feature `legacy-pairing` is enabled.
    #[cfg(feature = "legacy-pairing")]
    /// Returns this builder after storing `enabled`; true prevents legacy downgrade in subsequent pairing.
    pub fn set_secure_connections_only(mut self, enabled: bool) -> Self {
        self.host_state()
            .connections
            .security_manager
            .set_secure_connections_only(enabled);
        self
    }

    /// Finalize configuration and return the stack.
    ///
    /// Use the returned [`Stack`] for runtime operations: obtain a [`Runner`] via
    /// [`Stack::runner()`], and [`Central`] or
    /// [`Peripheral`] handles via [`Stack::central()`] and
    /// [`Stack::peripheral()`].
    /// Returns a stack by transferring initialized host state/controller exactly once; no controller I/O starts and runner admission is initially unused.
    pub fn build(mut self) -> Stack<'stack, C, P> {
        Stack {
            host_state: self.host_state.take().unwrap(),
            controller: self.controller.take().unwrap(),
            runner_taken: Cell::new(false),
        }
    }
}

impl<'stack, C, P: PacketPool> Stack<'stack, C, P> {
    /// Returns a borrowed lightweight controller/state view with this stack ownership domain.
    fn host(&self) -> BleHost<'_, C, P> {
        BleHost::new(&self.controller, self.host_state)
    }
}

impl<'stack, C: Controller, P: PacketPool> Stack<'stack, C, P> {
    /// Obtain a [`Runner`] to drive the BLE host.
    ///
    /// The runner must be polled (e.g. via [`Runner::run()`]) to drive the BLE host.
    /// Returns the sole runner; panics on every subsequent call, even if the previous runner was dropped. Poll all workers for host progress.
    pub fn runner(&self) -> Runner<'_, C, P> {
        assert!(
            !self.runner_taken.replace(true),
            "runner() can only be called once per Stack"
        );
        Runner::new(self.host())
    }

    /// Obtain a [`Central`] handle for the central BLE role.
    ///
    /// This is a lightweight handle that can be created multiple times.
    /// Concurrent connect operations are serialized internally.
    #[cfg(feature = "central")]
    /// Returns a borrowed central-role view; multiple views share one initiator admission state.
    pub fn central(&self) -> Central<'_, C, P> {
        Central::new(self.host())
    }

    /// Obtain a [`Peripheral`] handle for the peripheral BLE role.
    ///
    /// This is a lightweight handle that can be created multiple times.
    /// Concurrent advertise operations are serialized internally.
    #[cfg(feature = "peripheral")]
    /// Returns a borrowed peripheral-role view; multiple views share advertiser admission and all owned sets.
    pub fn peripheral(&self) -> Peripheral<'_, C, P> {
        Peripheral::new(self.host())
    }

    /// Obtain an [`Iso`](iso::Iso) handle for isochronous-stream (CIS/BIS) HCI commands and data.
    ///
    /// This is a lightweight handle that can be created multiple times.
    #[cfg(feature = "iso")]
    /// Returns a borrowed ISO HCI view; command support does not itself establish a stream.
    pub fn iso(&self) -> iso::Iso<'_, C, P> {
        iso::Iso::new(self.host())
    }

    /// Set the IO capabilities used by the security manager.
    ///
    /// Only relevant if the feature `security` is enabled.
    #[cfg(feature = "security")]
    /// Stores `io_capabilities` for future pairing setup; does not renegotiate an existing machine.
    pub fn set_io_capabilities(&self, io_capabilities: IoCapabilities) {
        self.host_state
            .connections
            .security_manager
            .set_io_capabilities(io_capabilities);
    }

    /// Configures the Secure Connections peripheral display passkey; other display paths generate random values.
    /// Caller must supply 0..=999999; this API does not validate the range. None restores random generation.
    #[cfg(feature = "security")]
    /// Stores optional `passkey` for the Secure Connections peripheral display path; caller enforces 0..=999999 and None restores generated values.
    pub fn set_passkey(&self, passkey: Option<u32>) {
        self.host_state.connections.security_manager.set_passkey(passkey);
    }

    /// Enable or disable secure connections only mode.
    ///
    /// When enabled, legacy pairing is rejected even if the `legacy-pairing` feature is compiled in.
    /// This matches the BLE spec's "Secure Connections Only Mode" (Vol 3, Part C, Section 10.2.4).
    ///
    /// Only relevant if the feature `legacy-pairing` is enabled.
    #[cfg(feature = "legacy-pairing")]
    /// Stores `enabled` for future legacy-downgrade admission; does not terminate an existing pairing procedure.
    pub fn set_secure_connections_only(&self, enabled: bool) {
        self.host_state
            .connections
            .security_manager
            .set_secure_connections_only(enabled);
    }

    /// Set the RPA (Resolvable Private Address) rotation timeout.
    ///
    /// Updates the stored timeout. If the host is already initialized, also sends
    /// the `LeSetResolvablePrivateAddrTimeout` HCI command to the controller.
    /// If called before initialization (e.g. during pre-server setup), the value
    /// will be used when the controller is initialized.
    ///
    /// Caller must use 1..=3600 whole seconds. This API currently stores first and does not validate that range;
    /// command encoding truncates subsecond precision, casts seconds to u32 and can panic if the HCI u16 duration overflows.
    #[cfg(feature = "security")]
    /// Stores `timeout` before optional controller update and returns its result; before initialization returns Ok with deferred application. Failure/cancellation preserves the new stored duration and does not change the current local expiration deadline.
    pub async fn set_rpa_timeout(&self, timeout: Duration) -> Result<(), BleHostError<C::Error>>
    where
        C: ControllerCmdSync<LeSetResolvablePrivateAddrTimeout>,
    {
        self.host().rpa_timeout().set(timeout);
        if self.host().is_initialized() {
            self.host()
                .command(LeSetResolvablePrivateAddrTimeout::new(
                    bt_hci::param::Duration::from_secs(timeout.as_secs() as u32),
                ))
                .await?;
        }
        Ok(())
    }

    /// Run a HCI command and return the response.
    ///
    /// The caller owns coordination with higher-level procedures. In particular,
    /// do not mix raw advertising commands with the high-level `Peripheral` API:
    /// its advertisers own all advertising sets and cleanup disables all sets.
    /// Waits for initialization, submits `cmd` and returns its synchronous response or controller/HCI error. Caller coordinates raw commands with host-owned procedures; cancellation after submission is transport-specific.
    pub async fn command<T>(&self, cmd: T) -> Result<T::Return, BleHostError<C::Error>>
    where
        T: SyncCmd,
        C: ControllerCmdSync<T>,
    {
        self.host().command(cmd).await
    }

    /// Run an async HCI command where the response will generate an event later.
    /// Waits for initialization, submits `cmd` and returns command-status success/error; later procedure completion arrives through RX events.
    pub async fn async_command<T>(&self, cmd: T) -> Result<(), BleHostError<C::Error>>
    where
        T: AsyncCmd,
        C: ControllerCmdAsync<T>,
    {
        self.host().async_command(cmd).await
    }

    /// Read the minimum supported connection interval from the controller.
    /// Returns the controller minimum-interval response after initialization, or HCI/transport failure; does not change a connection.
    pub async fn read_minimum_supported_connection_interval(
        &self,
    ) -> Result<<LeReadMinimumSupportedConnectionInterval as SyncCmd>::Return, BleHostError<C::Error>>
    where
        C: ControllerCmdSync<LeReadMinimumSupportedConnectionInterval>,
    {
        self.host()
            .command(LeReadMinimumSupportedConnectionInterval::new())
            .await
    }

    /// Read current host metrics
    /// Returns `f` result under a shared host-metrics borrow; reentrant mutation panics.
    pub fn metrics<F: FnOnce(&HostMetrics) -> R, R>(&self, f: F) -> R {
        self.host().metrics(f)
    }

    /// Log status information of the host
    /// Logs host and manager state; `verbose` includes terminal/unused slots. Current connection diagnostics omit IRKs; no controller commands are sent.
    pub fn log_status(&self, verbose: bool) {
        self.host().log_status(verbose);
    }

    #[cfg(feature = "security")]
    /// Generate local OOB data for LESC pairing.
    ///
    /// The returned data should be transferred to the peer device via an out-of-band
    /// channel (NFC, QR code, etc.) before pairing begins.
    /// Returns fresh local Secure Connections random/confirm material using current key/RNG; provide to the peer outside BLE. Does not begin pairing or persist a bond.
    pub fn get_local_oob_data(&self) -> OobData {
        self.host_state.connections.security_manager.get_local_oob_data()
    }

    #[cfg(feature = "security")]
    /// Get the local address configured on the security manager.
    /// Returns configured/initialized security local identity, or None before one is supplied or observed.
    pub fn get_local_address(&self) -> Option<Address> {
        self.host_state.connections.security_manager.get_local_address()
    }

    /// Check whether BLE address privacy is enabled.
    #[cfg(feature = "security")]
    /// Returns whether a local IRK exists; controller privacy setup success is a separate observation.
    pub fn is_privacy_enabled(&self) -> bool {
        self.host().is_privacy_enabled()
    }

    #[cfg(feature = "security")]
    /// Add bond information for a peer device.
    ///
    /// After bonds are added or removed (either directly or via pairing), the controller's
    /// resolving list is updated automatically the next time advertising, scanning, and
    /// connecting are all idle. Applications should ensure periodic idle windows to allow
    /// resolving list updates to take effect.
    /// Stores/replaces `bond_information` and returns bounded-store admission result; success schedules an idle-only controller list update. No external persistence or immediate controller completion.
    pub fn add_bond_information(&self, bond_information: BondInformation) -> Result<(), Error> {
        let identity = bond_information.identity;
        let result = self
            .host()
            .connections()
            .security_manager
            .add_bond_information(bond_information);
        #[cfg(feature = "security")]
        if result.is_ok() {
            self.host()
                .resolving_list_state()
                .borrow_mut()
                .push(crate::host::ResolvingListUpdate::Add(identity));
        }
        result
    }

    #[cfg(feature = "security")]
    /// Remove a bonded device.
    ///
    /// After bonds are added or removed (either directly or via pairing), the controller's
    /// resolving list is updated automatically the next time advertising, scanning, and
    /// connecting are all idle. Applications should ensure periodic idle windows to allow
    /// resolving list updates to take effect.
    /// Removes the record matching `identity`, returning store result; success schedules controller list removal at an idle window. Does not erase external persistence.
    pub fn remove_bond_information(&self, identity: Identity) -> Result<(), Error> {
        let result = self
            .host()
            .connections()
            .security_manager
            .remove_bond_information(identity);
        #[cfg(feature = "security")]
        if result.is_ok() {
            self.host()
                .resolving_list_state()
                .borrow_mut()
                .push(crate::host::ResolvingListUpdate::Remove(identity));
        }
        result
    }

    #[cfg(feature = "security")]
    /// Access bonded devices
    /// Returns callback `f` result while borrowing all stored bonds; reentrant bond mutation panics and key material must not be logged accidentally.
    pub fn with_bond_information<R>(&self, f: impl FnOnce(&[BondInformation]) -> R) -> R {
        f(&self.host_state.connections.security_manager.get_bond_information())
    }

    /// Get a connection by its peer address
    /// Returns a retained accepted link matching `peer_address` through identity/address resolution, or None; reference-count exhaustion panics.
    pub fn get_connection_by_peer_address(&self, peer_address: Address) -> Option<Connection<'_, P>> {
        self.host_state.connections.get_connection_by_peer_address(peer_address)
    }

    /// Get a connection by its handle
    /// Returns a retained accepted link for current controller `handle`, or None; numeric handles can be reused after disconnect and reference exhaustion panics.
    pub fn get_connected_handle(&self, handle: ConnHandle) -> Option<Connection<'_, P>> {
        self.host_state.connections.get_connected_handle(handle)
    }

    /// Iterate over all currently connected connections.
    /// Returns an iterator retaining each accepted link as yielded; dropping the last owner may request disconnect.
    pub fn connections(&self) -> connection_manager::ConnectedIter<'_, P> {
        self.host_state.connections.connections()
    }
}

/// Returns HCI duration for `d`, flooring to US-microsecond units; locked conversion panics if the u16 quotient overflows. US must be nonzero.
pub(crate) fn bt_hci_duration<const US: u32>(d: Duration) -> bt_hci::param::Duration<US> {
    bt_hci::param::Duration::from_micros(d.as_micros())
}

/// Returns extended HCI duration for `d`, flooring to US-microsecond units; locked conversion panics if the u24 quotient overflows. US must be nonzero.
pub(crate) fn bt_hci_ext_duration<const US: u16>(d: Duration) -> bt_hci::param::ExtDuration<US> {
    bt_hci::param::ExtDuration::from_micros(d.as_micros())
}

// Re-export our version of embassy-sync for the macros
/// Dependency reexports referenced by generated GATT code; uses this crate's resolved embassy-sync version.
pub mod __export {
    pub use embassy_sync;
}
