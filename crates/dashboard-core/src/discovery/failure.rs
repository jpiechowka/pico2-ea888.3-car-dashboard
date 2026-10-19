//! Small, owned failure evidence with a bounded rejected declaration prefix.
use super::{Characteristic, ProbeError, Uuid};

/// Local ingress failure and controller-confirmed termination are separate evidence.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ConnectionFailureContext {
    pub local_error: Option<FailureReason>,
    pub disconnect_reason: Option<u8>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FailureStage {
    Connect,
    Mtu,
    Services,
    Characteristics,
    Descriptors,
    DeviceInfo,
    ConnectionEvents,
    ClientTask,
    Inspection,
}
impl FailureStage {
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FailureReason {
    Controller,
    Hardware(u8),
    Timeout,
    Disconnected,
    DisconnectCode(u8),
    Capacity,
    Att(u8),
    Hci(u8),
    HciDecode,
    UnexpectedResponse,
    InvalidValue,
    Codec,
    InvalidState,
    NotFound,
    Unsupported,
    Busy,
    Other,
    DeclarationLength,
    DeclarationOrder,
    ValueHandle,
    PeerMismatch,
}
impl FailureReason {
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

    pub const fn code(self) -> Option<u8> {
        match self {
            Self::Att(code) | Self::Hci(code) | Self::DisconnectCode(code) | Self::Hardware(code) => Some(code),
            _ => None,
        }
    }

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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RejectedDeclaration {
    pub handle: u16,
    length: u16,
    prefix: [u8; 19],
}
impl RejectedDeclaration {
    /// ATT declaration values fit u16. Longer synthetic inputs saturate the length.
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

    pub const fn length(&self) -> u16 { self.length }

    pub fn bytes(&self) -> &[u8] { &self.prefix[..usize::from(self.length).min(self.prefix.len())] }

    pub fn value_handle(&self) -> Option<u16> {
        (self.length >= 3).then(|| u16::from_le_bytes([self.prefix[1], self.prefix[2]]))
    }

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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InspectionFailure {
    pub stage: FailureStage,
    pub reason: FailureReason,
    /// Application query range, NOT the handle from a remote ATT Error Response.
    pub range: Option<(u16, u16)>,
    /// Last accepted declaration/value in this service, NOT the failed request cursor.
    pub last_accepted: Option<(u16, u16)>,
    /// First tuple rejected by the application guard; absent for lower-layer errors.
    pub rejected: Option<RejectedDeclaration>,
    pub elapsed_ms: Option<u32>,
}
impl InspectionFailure {
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
