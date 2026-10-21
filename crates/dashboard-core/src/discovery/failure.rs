//! Small, owned failure evidence with a bounded rejected declaration prefix.
use super::{Characteristic, ProbeError, Uuid};

/// Local ingress failure and controller-confirmed termination are separate evidence.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ConnectionFailureContext {
    /// First local ingress failure; separate from any later disconnect status.
    pub local_error: Option<FailureReason>,
    /// Raw controller HCI disconnect reason, or absent without terminal evidence.
    pub disconnect_reason: Option<u8>,
}

/// Operation whose failure was observed; a stage label alone does not establish causality.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FailureStage {
    /// Link creation and selected-peer identity verification.
    Connect,
    /// ATT exchange-MTU procedure.
    Mtu,
    /// Primary-service enumeration.
    Services,
    /// Characteristic-declaration enumeration.
    Characteristics,
    /// Descriptor enumeration between characteristic boundaries.
    Descriptors,
    /// Allowlisted standard-information characteristic reads.
    DeviceInfo,
    /// Asynchronous connection events or termination evidence.
    ConnectionEvents,
    /// Concurrent ATT/GATT receive task.
    ClientTask,
    /// Whole-inspection deadline or outer lifecycle.
    Inspection,
}
impl FailureStage {
    /// Returns `self`'s diagnostic stage label for display/export.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Connect => "connect",
            Self::Mtu => "MTU exchange",
            Self::Services => "primary services",
            Self::Characteristics => "characteristics",
            Self::Descriptors => "descriptors",
            Self::DeviceInfo => "standard information reads",
            Self::ConnectionEvents => "connection events",
            Self::ClientTask => "GATT receive task",
            Self::Inspection => "whole inspection",
        }
    }
}

/// Bounded local/protocol failure classification retaining raw codes where available.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FailureReason {
    /// Controller transport I/O failed without a more specific raw code.
    Controller,
    /// HCI hardware-error event.
    Hardware(
        /// Raw controller hardware-error code.
        u8,
    ),
    /// The operation's deadline expired.
    Timeout,
    /// Connection ended without a retained raw reason.
    Disconnected,
    /// Connection ended with an HCI reason.
    DisconnectCode(
        /// Raw HCI disconnection reason.
        u8,
    ),
    /// A bounded buffer, queue or host resource could not admit more data.
    Capacity,
    /// Remote ATT error response.
    Att(
        /// Raw ATT error code.
        u8,
    ),
    /// HCI command/status failure.
    Hci(
        /// Raw HCI status code.
        u8,
    ),
    /// Controller event framing could not be decoded.
    HciDecode,
    /// ATT response did not match the pending operation.
    UnexpectedResponse,
    /// Decoded data violated the receiving API's value contract.
    InvalidValue,
    /// ATT payload encoding was malformed.
    Codec,
    /// Operation was incompatible with the host's current state.
    InvalidState,
    /// Required host resource was absent.
    NotFound,
    /// Host did not implement the requested operation.
    Unsupported,
    /// Host resource was already in use.
    Busy,
    /// Unclassified host error; no invented protocol code is attached.
    Other,
    /// Characteristic declaration was neither 5 nor 19 bytes long.
    DeclarationLength,
    /// Declaration handle violated service bounds or monotonic ordering.
    DeclarationOrder,
    /// Characteristic value handle did not follow its declaration within the service.
    ValueHandle,
    /// Connected address/type differed from the explicitly selected peer.
    PeerMismatch,
}
impl FailureReason {
    /// Returns `self`'s classification label; raw protocol codes remain separately available.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Controller => "controller I/O",
            Self::Hardware(_) => "controller hardware event",
            Self::Timeout => "deadline expired",
            Self::Disconnected => "disconnected",
            Self::DisconnectCode(_) => "disconnect HCI",
            Self::Capacity => "buffer / resource capacity",
            Self::Att(_) => "ATT error",
            Self::Hci(_) => "HCI status",
            Self::HciDecode => "HCI decode",
            Self::UnexpectedResponse => "unexpected ATT response",
            Self::InvalidValue => "invalid data",
            Self::Codec => "malformed ATT encoding",
            Self::InvalidState => "invalid host state",
            Self::NotFound => "host resource not found",
            Self::Unsupported => "unsupported host operation",
            Self::Busy => "host resource busy",
            Self::Other => "other host error",
            Self::DeclarationLength => "invalid declaration length",
            Self::DeclarationOrder => "invalid declaration order",
            Self::ValueHandle => "invalid value handle",
            Self::PeerMismatch => "connected peer mismatch",
        }
    }

    /// Returns `self`'s retained ATT/HCI/disconnect/hardware code, or `None` for local classifications.
    pub const fn code(self) -> Option<u8> {
        match self {
            Self::Att(code) | Self::Hci(code) | Self::DisconnectCode(code) | Self::Hardware(code) => Some(code),
            _ => None,
        }
    }

    /// Returns the coarse UI error for `self`; unmapped host/declaration failures become discovery errors.
    pub const fn error(self) -> ProbeError {
        match self {
            Self::Controller | Self::Hardware(_) => ProbeError::Transport,
            Self::Timeout => ProbeError::Timeout,
            Self::Disconnected | Self::DisconnectCode(_) => ProbeError::Disconnected,
            Self::Capacity => ProbeError::Capacity,
            Self::PeerMismatch => ProbeError::InvalidPeer,
            _ => ProbeError::Discovery,
        }
    }
}

/// Rejected ATT declaration with its original length and bounded wire prefix.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RejectedDeclaration {
    /// Declaration attribute handle returned by the remote peer.
    pub handle: u16,
    /// Original wire length in bytes, saturated at `u16::MAX` for synthetic oversized input.
    length: u16,
    /// First 19 wire bytes, zero-padded beyond the retained prefix.
    prefix: [u8; 19],
}
impl RejectedDeclaration {
    /// Returns rejected evidence for declaration `handle` and raw `bytes`, retaining
    /// the first 19 bytes. ATT lengths fit u16; longer synthetic lengths saturate.
    pub fn from_wire(
        handle: u16,
        bytes: &[u8],
    ) -> Self {
        let mut prefix = [0; 19];
        let count = bytes.len().min(prefix.len());
        prefix[..count].copy_from_slice(&bytes[..count]);
        Self {
            handle,
            length: bytes.len().try_into().unwrap_or(u16::MAX),
            prefix,
        }
    }

    /// Returns `self`'s original wire length in bytes, possibly saturated at `u16::MAX`.
    pub const fn length(&self) -> u16 { self.length }

    /// Returns `self`'s retained prefix of `min(original length, 19)` bytes.
    pub fn bytes(&self) -> &[u8] { &self.prefix[..usize::from(self.length).min(self.prefix.len())] }

    /// Returns the little-endian value-handle candidate in `self`'s bytes 1..3,
    /// or `None` when fewer than three bytes were observed. Does not validate that handle.
    pub fn value_handle(&self) -> Option<u16> {
        (self.length >= 3).then(|| u16::from_le_bytes([self.prefix[1], self.prefix[2]]))
    }

    /// Reconstructs evidence for `handle`, original byte `length` and retained `bytes`.
    /// Returns `None` unless the prefix has exactly `min(length, 19)` bytes;
    /// successful reconstruction preserves the original length without inventing tail data.
    pub(crate) fn from_prefix(
        handle: u16,
        length: u16,
        bytes: &[u8],
    ) -> Option<Self> {
        if bytes.len() != usize::from(length).min(19) {
            return None;
        }
        let mut value = Self::from_wire(handle, bytes);
        value.length = length;
        Some(value)
    }
}

/// First operation failure with optional query, ordering and timing evidence.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InspectionFailure {
    /// Operation stage where the failure was observed.
    pub stage: FailureStage,
    /// Local/protocol failure classification, including a raw code when known.
    pub reason: FailureReason,
    /// Application query range, NOT the handle from a remote ATT Error Response.
    pub range: Option<(u16, u16)>,
    /// Last accepted declaration/value in this service, NOT the failed request cursor.
    pub last_accepted: Option<(u16, u16)>,
    /// First tuple rejected by the application guard; absent for lower-layer errors.
    pub rejected: Option<RejectedDeclaration>,
    /// Operation duration in milliseconds, or unavailable when not measured.
    pub elapsed_ms: Option<u32>,
}
impl InspectionFailure {
    /// Returns failure evidence for `stage` and `reason` with no query, tuple or timing context.
    pub const fn new(
        stage: FailureStage,
        reason: FailureReason,
    ) -> Self {
        Self {
            stage,
            reason,
            range: None,
            last_accepted: None,
            rejected: None,
            elapsed_ms: None,
        }
    }

    /// Returns `self`'s UI error. Timeout and peer mismatch retain their categories;
    /// other connect-stage failures become `Connect`, with remaining stages mapped by reason.
    pub const fn error(self) -> ProbeError {
        match self.reason {
            FailureReason::Timeout | FailureReason::PeerMismatch => self.reason.error(),
            _ if matches!(self.stage, FailureStage::Connect) => ProbeError::Connect,
            _ => self.reason.error(),
        }
    }
}

/// Validate a characteristic declaration returned by the real ATT client.
/// A later declaration must follow the previous value, within the selected service.
/// `start`/`end` are inclusive service handles; `previous_value` is the last accepted
/// value handle (zero before the first); `declaration` is this tuple's attribute handle.
/// `bytes` contains properties, little-endian value handle, and a 16- or 128-bit UUID.
/// Returns a characteristic only for 5/19-byte declarations strictly after `start`
/// and `previous_value`, before `end`, with a later value handle no greater than `end`.
/// Returns the first length/order/value-handle violation without mutating inventory.
pub fn characteristic_from_wire(
    start: u16,
    end: u16,
    previous_value: u16,
    declaration: u16,
    bytes: &[u8],
) -> Result<Characteristic, FailureReason> {
    if !matches!(bytes.len(), 5 | 19) {
        return Err(FailureReason::DeclarationLength);
    }
    if start == 0 || end < start || declaration <= start || declaration <= previous_value || declaration >= end {
        return Err(FailureReason::DeclarationOrder);
    }
    let handle = u16::from_le_bytes([bytes[1], bytes[2]]);
    if handle <= declaration || handle > end {
        return Err(FailureReason::ValueHandle);
    }
    Ok(Characteristic {
        service_start: start,
        declaration,
        handle,
        properties: bytes[0],
        uuid: Uuid::from_wire(&bytes[3..]).ok_or(FailureReason::DeclarationLength)?,
    })
}
