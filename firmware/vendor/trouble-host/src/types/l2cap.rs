//! L2CAP fixed wire records and signal identifiers. Native-layout HCI codecs require a little-endian target.
//! Layouts follow [Bluetooth Core Vol 3 Part A](https://www.bluetooth.com/wp-content/uploads/Files/Specification/HTML/Core-61/out/en/host/logical-link-control-and-adaptation-protocol-specification.html).
//! Bit-valid decoding alone does not admit protocol ranges, credit totals or connection state.
use bt_hci::{FixedSizeValue, WriteHci};

/// Fixed ATT channel identifier.
pub(crate) const L2CAP_CID_ATT: u16 = 0x0004;
/// LE signaling channel identifier.
pub(crate) const L2CAP_CID_LE_U_SIGNAL: u16 = 0x0005;
/// LE Security Manager channel identifier.
pub(crate) const L2CAP_CID_LE_U_SECURITY_MANAGER: u16 = 0x0006;
/// First dynamically assignable channel identifier.
pub(crate) const L2CAP_CID_DYN_START: u16 = 0x0040;

#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[derive(Debug, Clone, Copy)]
#[repr(C)]
/// Basic header preceding the L2CAP payload; four bytes with no padding.
pub struct L2capHeader {
    /// Byte length of the payload after this header.
    pub length: u16,
    /// Destination L2CAP channel identifier.
    pub channel: u16,
}

unsafe impl FixedSizeValue for L2capHeader {
    /// Returns true for any bit pattern in `data`; byte count is checked by the HCI decoder.
    fn is_valid(data: &[u8]) -> bool {
        true
    }
}

#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[repr(C)]
#[derive(Debug, Clone, Copy)]
/// Four-byte signaling header; the payload follows this record.
pub struct L2capSignalHeader {
    /// Operation selector; unknown byte values remain representable.
    pub code: L2capSignalCode,
    /// Transaction identifier echoed in the corresponding response; zero is invalid on wire.
    pub identifier: u8,
    /// Byte length of the payload after this header.
    pub length: u16,
}

unsafe impl FixedSizeValue for L2capSignalHeader {
    /// Returns true for any bit pattern in `data`; byte count is checked by the HCI decoder.
    fn is_valid(data: &[u8]) -> bool {
        true
    }
}

#[cfg(not(feature = "defmt"))]
/// Fixed-size LE signal payload with its channel and operation discriminator.
pub trait L2capSignal: WriteHci + FixedSizeValue + core::fmt::Debug {
    /// Returns the fixed LE signaling channel identifier.
    fn channel() -> u16 {
        L2CAP_CID_LE_U_SIGNAL
    }
    /// Returns the operation discriminator for this payload type.
    fn code() -> L2capSignalCode;
}

#[cfg(feature = "defmt")]
/// Fixed-size LE signal payload with its channel and operation discriminator.
pub trait L2capSignal: WriteHci + FixedSizeValue + defmt::Format {
    /// Returns the fixed LE signaling channel identifier.
    fn channel() -> u16 {
        L2CAP_CID_LE_U_SIGNAL
    }
    /// Returns the operation discriminator for this payload type.
    fn code() -> L2capSignalCode;
}

#[derive(Clone, Copy, PartialEq)]
#[repr(transparent)]
/// Extensible signaling discriminator; unknown values are preserved for command rejection.
pub struct L2capSignalCode(#[doc = "Raw operation code, including unrecognized values."] pub u8);

unsafe impl FixedSizeValue for L2capSignalCode {
    /// Returns true for every `_data` byte pattern; unknown signal codes are representable.
    fn is_valid(_data: &[u8]) -> bool {
        true
    }
}

impl core::fmt::Debug for L2capSignalCode {
    /// Writes a known operation name or Unknown(hex) to `f`; returns formatting errors.
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match *self {
            Self::COMMAND_REJECT_RES => write!(f, "CommandRejectRes"),
            Self::CONNECTION_REQ => write!(f, "ConnectionReq"),
            Self::CONNECTION_RES => write!(f, "ConnectionRes"),
            Self::CONFIGURATION_REQ => write!(f, "ConfigurationReq"),
            Self::CONFIGURATION_RES => write!(f, "ConfigurationRes"),
            Self::DISCONNECTION_REQ => write!(f, "DisconnectionReq"),
            Self::DISCONNECTION_RES => write!(f, "DisconnectionRes"),
            Self::ECHO_REQ => write!(f, "EchoReq"),
            Self::ECHO_RES => write!(f, "EchoRes"),
            Self::INFORMATION_REQ => write!(f, "InformationReq"),
            Self::INFORMATION_RES => write!(f, "InformationRes"),
            Self::CONN_PARAM_UPDATE_REQ => write!(f, "ConnParamUpdateReq"),
            Self::CONN_PARAM_UPDATE_RES => write!(f, "ConnParamUpdateRes"),
            Self::LE_CREDIT_CONN_REQ => write!(f, "LeCreditConnReq"),
            Self::LE_CREDIT_CONN_RES => write!(f, "LeCreditConnRes"),
            Self::LE_CREDIT_FLOW_IND => write!(f, "LeCreditFlowInd"),
            Self::CREDIT_CONN_REQ => write!(f, "CreditConnReq"),
            Self::CREDIT_CONN_RES => write!(f, "CreditConnRes"),
            Self::CREDIT_CONN_RECONFIG_REQ => write!(f, "CreditConnReconfigReq"),
            Self::CREDIT_CONN_RECONFIG_RES => write!(f, "CreditConnReconfigRes"),
            _ => write!(f, "Unknown(0x{:02x})", self.0),
        }
    }
}

#[cfg(feature = "defmt")]
impl defmt::Format for L2capSignalCode {
    /// Writes a known operation name or raw unknown value through formatter `f`.
    fn format(&self, f: defmt::Formatter<'_>) {
        match *self {
            Self::COMMAND_REJECT_RES => defmt::write!(f, "CommandRejectRes"),
            Self::CONNECTION_REQ => defmt::write!(f, "ConnectionReq"),
            Self::CONNECTION_RES => defmt::write!(f, "ConnectionRes"),
            Self::CONFIGURATION_REQ => defmt::write!(f, "ConfigurationReq"),
            Self::CONFIGURATION_RES => defmt::write!(f, "ConfigurationRes"),
            Self::DISCONNECTION_REQ => defmt::write!(f, "DisconnectionReq"),
            Self::DISCONNECTION_RES => defmt::write!(f, "DisconnectionRes"),
            Self::ECHO_REQ => defmt::write!(f, "EchoReq"),
            Self::ECHO_RES => defmt::write!(f, "EchoRes"),
            Self::INFORMATION_REQ => defmt::write!(f, "InformationReq"),
            Self::INFORMATION_RES => defmt::write!(f, "InformationRes"),
            Self::CONN_PARAM_UPDATE_REQ => defmt::write!(f, "ConnParamUpdateReq"),
            Self::CONN_PARAM_UPDATE_RES => defmt::write!(f, "ConnParamUpdateRes"),
            Self::LE_CREDIT_CONN_REQ => defmt::write!(f, "LeCreditConnReq"),
            Self::LE_CREDIT_CONN_RES => defmt::write!(f, "LeCreditConnRes"),
            Self::LE_CREDIT_FLOW_IND => defmt::write!(f, "LeCreditFlowInd"),
            Self::CREDIT_CONN_REQ => defmt::write!(f, "CreditConnReq"),
            Self::CREDIT_CONN_RES => defmt::write!(f, "CreditConnRes"),
            Self::CREDIT_CONN_RECONFIG_REQ => defmt::write!(f, "CreditConnReconfigReq"),
            Self::CREDIT_CONN_RECONFIG_RES => defmt::write!(f, "CreditConnReconfigRes"),
            _ => defmt::write!(f, "Unknown(0x{:02x})", self.0),
        }
    }
}

impl From<u8> for L2capSignalCode {
    /// Returns the raw signal code `val`, without filtering unknown values.
    fn from(val: u8) -> Self {
        Self(val)
    }
}

#[allow(non_upper_case_globals)]
impl L2capSignalCode {
    /// Reject an unsupported or invalid signaling command.
    pub const COMMAND_REJECT_RES: Self = Self(0x01);
    /// BR/EDR channel connection request (not LE credit mode).
    pub const CONNECTION_REQ: Self = Self(0x02);
    /// BR/EDR channel connection response.
    pub const CONNECTION_RES: Self = Self(0x03);
    /// Negotiate BR/EDR channel configuration.
    pub const CONFIGURATION_REQ: Self = Self(0x04);
    /// Respond to BR/EDR channel configuration.
    pub const CONFIGURATION_RES: Self = Self(0x05);
    /// Request channel disconnection.
    pub const DISCONNECTION_REQ: Self = Self(0x06);
    /// Acknowledge channel disconnection.
    pub const DISCONNECTION_RES: Self = Self(0x07);
    /// BR/EDR signaling echo request.
    pub const ECHO_REQ: Self = Self(0x08);
    /// BR/EDR signaling echo response.
    pub const ECHO_RES: Self = Self(0x09);
    /// Request BR/EDR L2CAP capabilities.
    pub const INFORMATION_REQ: Self = Self(0x0A);
    /// Return BR/EDR L2CAP capabilities.
    pub const INFORMATION_RES: Self = Self(0x0B);
    /// Peripheral request for LE connection timing changes.
    pub const CONN_PARAM_UPDATE_REQ: Self = Self(0x12);
    /// Central admission response to LE timing changes.
    pub const CONN_PARAM_UPDATE_RES: Self = Self(0x13);
    /// Create one LE credit-based channel.
    pub const LE_CREDIT_CONN_REQ: Self = Self(0x14);
    /// Admit or refuse one LE credit-based channel.
    pub const LE_CREDIT_CONN_RES: Self = Self(0x15);
    /// Grant additional K-frame send credits.
    pub const LE_CREDIT_FLOW_IND: Self = Self(0x16);
    /// Create enhanced credit-based channels.
    pub const CREDIT_CONN_REQ: Self = Self(0x17);
    /// Respond to enhanced credit-based channel creation.
    pub const CREDIT_CONN_RES: Self = Self(0x18);
    /// Request enhanced credit channel MTU/MPS changes.
    pub const CREDIT_CONN_RECONFIG_REQ: Self = Self(0x19);
    /// Respond to enhanced credit channel reconfiguration.
    pub const CREDIT_CONN_RECONFIG_RES: Self = Self(0x1A);
}

#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[repr(C)]
#[derive(Debug, Clone, Copy)]
/// LE credit-based channel creation request; numeric protocol validation is performed by the channel manager.
pub struct LeCreditConnReq {
    /// Requested protocol/service multiplexer.
    pub spsm: u16,
    /// Source endpoint CID of the request initiator.
    pub scid: u16,
    /// Maximum receivable SDU size in bytes.
    pub mtu: u16,
    /// Maximum receivable K-frame payload size in bytes.
    pub mps: u16,
    /// K-frame receive credits granted to the peer; each credit permits one frame.
    pub credits: u16,
}

unsafe impl FixedSizeValue for LeCreditConnReq {
    /// Returns true for any bit pattern in `data`; byte count is checked by the HCI decoder.
    fn is_valid(data: &[u8]) -> bool {
        true
    }
}

impl L2capSignal for LeCreditConnReq {
    /// Returns the signal discriminator paired with this record layout.
    fn code() -> L2capSignalCode {
        L2capSignalCode::LE_CREDIT_CONN_REQ
    }
}

#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u16)]
/// Result code for an LE Credit Based Connection Response.
pub enum LeCreditConnResultCode {
    /// Connection successful.
    Success = 0x0000,
    /// Connection refused — LE_PSM not supported.
    SpsmNotSupported = 0x0002,
    /// Connection refused — no resources available.
    NoResources = 0x0004,
    /// Connection refused — insufficient authentication.
    InsufficientAuthentication = 0x0005,
    /// Connection refused — insufficient authorization.
    InsufficientAuthorization = 0x0006,
    /// Connection refused — encryption key size too short.
    EncryptionKeyTooShort = 0x0007,
    /// Connection refused — insufficient encryption.
    InsufficientEncryption = 0x0008,
    /// Connection refused — invalid Source CID.
    InvalidSourceId = 0x0009,
    /// Connection refused — Source CID already allocated.
    ScidAlreadyAllocated = 0x000A,
    /// Connection refused — unacceptable parameters.
    UnacceptableParameters = 0x000B,
}

#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[repr(C)]
#[derive(Debug, Clone, Copy)]
/// LE credit-based channel response; only the result discriminant is validated during bit decoding.
pub struct LeCreditConnRes {
    /// Destination endpoint CID assigned by the responder; zero on rejection.
    pub dcid: u16,
    /// Maximum receivable SDU size in bytes.
    pub mtu: u16,
    /// Maximum receivable K-frame payload size in bytes.
    pub mps: u16,
    /// K-frame receive credits granted to the peer; each credit permits one frame.
    pub credits: u16,
    /// Connection success or explicit refusal code.
    pub result: LeCreditConnResultCode,
}

impl LeCreditConnRes {
    /// Returns a zero-capacity response carrying `result`; callers must supply a refusal code, not Success.
    pub const fn reject(result: LeCreditConnResultCode) -> Self {
        Self {
            dcid: 0,
            mtu: 0,
            mps: 0,
            credits: 0,
            result,
        }
    }
}

impl L2capSignal for LeCreditConnRes {
    /// Returns the signal discriminator paired with this record layout.
    fn code() -> L2capSignalCode {
        L2capSignalCode::LE_CREDIT_CONN_RES
    }
}

unsafe impl FixedSizeValue for LeCreditConnRes {
    /// Returns whether `data` contains a valid native-endian result discriminant; rejects truncated result bytes.
    /// Other numeric fields remain unchecked until channel admission.
    fn is_valid(data: &[u8]) -> bool {
        let offset = core::mem::offset_of!(Self, result);
        let Some(&[first, second]) = data.get(offset..offset + 2) else {
            return false;
        };
        // FixedSizeValue decodes with read_unaligned, so validate the native
        // enum representation before bt-hci constructs the enclosing value.
        matches!(u16::from_ne_bytes([first, second]), 0x0000 | 0x0002 | 0x0004..=0x000b)
    }
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
/// Additional K-frame credits granted to the peer for an established channel.
pub struct LeCreditFlowInd {
    /// Granting device endpoint CID, interpreted as the peer destination CID by the receiver.
    pub cid: u16,
    /// K-frame receive credits granted to the peer; each credit permits one frame.
    pub credits: u16,
}

unsafe impl FixedSizeValue for LeCreditFlowInd {
    /// Returns true for any bit pattern in `data`; byte count is checked by the HCI decoder.
    fn is_valid(data: &[u8]) -> bool {
        true
    }
}

impl L2capSignal for LeCreditFlowInd {
    /// Returns the signal discriminator paired with this record layout.
    fn code() -> L2capSignalCode {
        L2capSignalCode::LE_CREDIT_FLOW_IND
    }
}

#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[repr(C)]
#[derive(Debug, Clone, Copy)]
/// Minimal command-rejection response; optional reason-specific data is not represented.
pub struct CommandRejectRes {
    /// Command reject reason; zero means unsupported command.
    pub reason: u16,
    // TODO: Optional fields pub data: u16,
}

unsafe impl FixedSizeValue for CommandRejectRes {
    /// Returns true for any bit pattern in `data`; byte count is checked by the HCI decoder.
    fn is_valid(data: &[u8]) -> bool {
        true
    }
}

impl L2capSignal for CommandRejectRes {
    /// Returns the signal discriminator paired with this record layout.
    fn code() -> L2capSignalCode {
        L2capSignalCode::COMMAND_REJECT_RES
    }
}

#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[repr(C)]
#[derive(Debug, Clone, Copy)]
/// Request to disconnect a channel using the peer and local endpoint IDs.
pub struct DisconnectionReq {
    /// Destination endpoint CID assigned by the responder; zero on rejection.
    pub dcid: u16,
    /// Source endpoint CID of the request initiator.
    pub scid: u16,
}

unsafe impl FixedSizeValue for DisconnectionReq {
    /// Returns true for any bit pattern in `data`; byte count is checked by the HCI decoder.
    fn is_valid(data: &[u8]) -> bool {
        true
    }
}

impl L2capSignal for DisconnectionReq {
    /// Returns the signal discriminator paired with this record layout.
    fn code() -> L2capSignalCode {
        L2capSignalCode::DISCONNECTION_REQ
    }
}

#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[repr(C)]
#[derive(Debug, Clone, Copy)]
/// Acknowledgment echoing endpoint IDs from a disconnection request.
pub struct DisconnectionRes {
    /// Destination endpoint CID assigned by the responder; zero on rejection.
    pub dcid: u16,
    /// Source endpoint CID of the request initiator.
    pub scid: u16,
}

unsafe impl FixedSizeValue for DisconnectionRes {
    /// Returns true for any bit pattern in `data`; byte count is checked by the HCI decoder.
    fn is_valid(data: &[u8]) -> bool {
        true
    }
}

impl L2capSignal for DisconnectionRes {
    /// Returns the signal discriminator paired with this record layout.
    fn code() -> L2capSignalCode {
        L2capSignalCode::DISCONNECTION_RES
    }
}

#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[repr(C)]
#[derive(Debug, Clone, Copy)]
/// Peripheral request for connection timing; this record does not validate allowed ranges.
pub struct ConnParamUpdateReq {
    /// Minimum requested connection interval in 1.25 ms units.
    pub interval_min: u16,
    /// Maximum requested connection interval in 1.25 ms units.
    pub interval_max: u16,
    /// Maximum peripheral latency in connection events.
    pub latency: u16,
    /// Supervision timeout in 10 ms units.
    pub timeout: u16,
}

unsafe impl FixedSizeValue for ConnParamUpdateReq {
    /// Returns true for any bit pattern in `data`; byte count is checked by the HCI decoder.
    fn is_valid(data: &[u8]) -> bool {
        true
    }
}

impl L2capSignal for ConnParamUpdateReq {
    /// Returns the signal discriminator paired with this record layout.
    fn code() -> L2capSignalCode {
        L2capSignalCode::CONN_PARAM_UPDATE_REQ
    }
}

#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[repr(C)]
#[derive(Debug, Clone, Copy)]
/// Central response accepting or rejecting the requested connection timing.
pub struct ConnParamUpdateRes {
    /// Zero accepts the parameters, one rejects them; other values are reserved.
    pub result: u16,
}

unsafe impl FixedSizeValue for ConnParamUpdateRes {
    /// Returns true for any bit pattern in `data`; byte count is checked by the HCI decoder.
    fn is_valid(data: &[u8]) -> bool {
        true
    }
}

impl L2capSignal for ConnParamUpdateRes {
    /// Returns the signal discriminator paired with this record layout.
    fn code() -> L2capSignalCode {
        L2capSignalCode::CONN_PARAM_UPDATE_RES
    }
}

#[cfg(test)]
/// Invalid discriminants must be rejected before constructing the repr(u16) enum.
mod tests {
    use bt_hci::{FromHciBytes, FromHciBytesError};

    use super::*;

    /// Exhausts all u16 discriminants and decodes only valid response representations.
    #[test]
    fn credit_response_validates_every_result_before_enum_decode() {
        for result in 0u16..=u16::MAX {
            let mut bytes = [0x40, 0, 23, 0, 23, 0, 1, 0, 0, 0];
            bytes[8..10].copy_from_slice(&result.to_le_bytes());
            let valid = [0, 2, 4, 5, 6, 7, 8, 9, 10, 11].contains(&result);
            // Fail before decoding: an invalid enum representation must never
            // be constructed, including when this regression catches a bug.
            assert_eq!(LeCreditConnRes::is_valid(&bytes), valid, "result {result:#06x}");
            if valid {
                let response = LeCreditConnRes::from_hci_bytes_complete(&bytes).unwrap();
                assert_eq!(response.result as u16, result);
                assert_eq!(
                    (response.dcid, response.mtu, response.mps, response.credits),
                    (64, 23, 23, 1)
                );
            } else {
                assert!(matches!(
                    LeCreditConnRes::from_hci_bytes_complete(&bytes),
                    Err(FromHciBytesError::InvalidValue)
                ));
            }
        }
    }

    /// Truncated records fail, prefix decoding preserves the suffix, and complete decoding rejects it.
    #[test]
    fn credit_response_decoder_handles_truncation_and_remaining_bytes() {
        let bytes = [0x40, 0, 23, 0, 23, 0, 1, 0, 0, 0, 0xa5];
        for length in 0..10 {
            assert!(matches!(
                LeCreditConnRes::from_hci_bytes_complete(&bytes[..length]),
                Err(FromHciBytesError::InvalidSize)
            ));
        }
        let (response, rest) = LeCreditConnRes::from_hci_bytes(&bytes).unwrap();
        assert_eq!(response.result, LeCreditConnResultCode::Success);
        assert_eq!(rest, [0xa5]);
        assert!(LeCreditConnRes::from_hci_bytes_complete(&bytes).is_err());
    }
}
