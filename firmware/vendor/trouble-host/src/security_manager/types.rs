//! SMP command encodings, pairing-feature admission and human-readable security metadata.
//!
//! Wire layouts follow Bluetooth Core Vol 3, Part H, §§3.5–3.6
//! ([Security Manager specification](https://www.bluetooth.com/wp-content/uploads/Files/Specification/HTML/Core-61/out/en/host/security-manager-specification.html)).
//! Raw AuthReq/distribution bytes retain reserved bits; feature decoding validates I/O, OOB and key size.

use core::fmt::{Display, Formatter};

use super::constants::ENCRYPTION_KEY_SIZE_128_BITS;
use crate::codec::{Decode, Encode, Type};
use crate::security_manager::crypto::IoCap;
use crate::{Error, IoCapabilities};

/// SMP failure reason; `Success` is an internal sentinel and cannot decode from a failure PDU.
// ([Vol 3] Part H, Section 3.5.5).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Reason {
    /// Pairing success
    Success,
    /// The user input of passkey failed, for example, the user cancelled the operation.
    PasskeyEntryFailed,
    /// The OOB data is not available.
    OobNotAvailable,
    /// The pairing procedure cannot be performed as authentication requirements cannot be met due to IO capabilities of one or both devices.
    AuthenticationRequirements,
    /// The confirm value does not match the calculated compare value.
    ConfirmValueFailed,
    /// Pairing is not supported by the device.
    PairingNotSupported,
    /// The resultant encryption key size is not long enough for the security requirements of this device.
    EncryptionKeySize,
    /// The SMP command received is not supported on this device.
    CommandNotSupported,
    #[allow(clippy::enum_variant_names)]
    /// Pairing failed due to an unspecified reason.
    UnspecifiedReason,
    /// Pairing or authentication procedure is disallowed because too little time has elapsed since last pairing request or security request.
    RepeatedAttempts,
    /// The Invalid Parameters error code indicates that the command length is invalid or that a parameter is outside of the specified range.
    InvalidParameters,
    /// Indicates to the remote device that the DHKey Check value received doesn’t match the one calculated by the local device.
    DHKeyCheckFailed,
    /// Indicates that the confirm values in the numeric comparison protocol do not match.
    NumericComparisonFailed,
    /// Indicates that the pairing over the LE transport failed due to a Pairing Request sent over the BR/EDR transport in progress.
    BrEdrPairingInProgress,
    /// Indicates that the BR/EDR Link Key generated on the BR/EDR transport cannot be used to derive and distribute keys for the LE transport or the LE LTK generated on the LE transport cannot be used to derive a key for the BR/EDR transport.
    GenerationNotAllowed,
    /// Indicates that the device chose not to accept a distributed key.
    KeyRejected,
    /// Indicates that the device is not ready to perform a pairing procedure.
    Busy,
}

impl TryFrom<u8> for Reason {
    /// InvalidValue for zero or any unassigned failure code.
    type Error = Error;
    /// Returns the failure represented by `val` (1 through 16); zero and unassigned codes are invalid.
    fn try_from(val: u8) -> Result<Self, Error> {
        Ok(match val {
            // Do not convert 0 to Success
            0x01 => Self::PasskeyEntryFailed,
            0x02 => Self::OobNotAvailable,
            0x03 => Self::AuthenticationRequirements,
            0x04 => Self::ConfirmValueFailed,
            0x05 => Self::PairingNotSupported,
            0x06 => Self::EncryptionKeySize,
            0x07 => Self::CommandNotSupported,
            0x08 => Self::UnspecifiedReason,
            0x09 => Self::RepeatedAttempts,
            0x0a => Self::InvalidParameters,
            0x0b => Self::DHKeyCheckFailed,
            0x0c => Self::NumericComparisonFailed,
            0x0d => Self::BrEdrPairingInProgress,
            0x0e => Self::GenerationNotAllowed,
            0x0f => Self::KeyRejected,
            0x10 => Self::Busy,
            _ => return Err(Error::InvalidValue),
        })
    }
}

impl From<Reason> for u8 {
    /// Returns the assigned SMP code for `val`, including zero for the internal success sentinel.
    fn from(val: Reason) -> u8 {
        match val {
            Reason::Success => 0x00,
            Reason::PasskeyEntryFailed => 0x01,
            Reason::OobNotAvailable => 0x02,
            Reason::AuthenticationRequirements => 0x03,
            Reason::ConfirmValueFailed => 0x04,
            Reason::PairingNotSupported => 0x05,
            Reason::EncryptionKeySize => 0x06,
            Reason::CommandNotSupported => 0x07,
            Reason::UnspecifiedReason => 0x08,
            Reason::RepeatedAttempts => 0x09,
            Reason::InvalidParameters => 0x0a,
            Reason::DHKeyCheckFailed => 0x0b,
            Reason::NumericComparisonFailed => 0x0c,
            Reason::BrEdrPairingInProgress => 0x0d,
            Reason::GenerationNotAllowed => 0x0e,
            Reason::KeyRejected => 0x0f,
            Reason::Busy => 0x10,
        }
    }
}

impl AsRef<str> for Reason {
    /// Returns a static diagnostic explanation of this outcome, without allocating.
    fn as_ref(&self) -> &str {
        match self {
            Reason::Success => "Success",
            Reason::PasskeyEntryFailed => "Passkey entry canceled or failed",
            Reason::OobNotAvailable => "Out-of-band data not available",
            Reason::AuthenticationRequirements => "Authentication requirements not met",
            Reason::ConfirmValueFailed => "Confirm value does not match",
            Reason::PairingNotSupported => "Device do not support pairing",
            Reason::EncryptionKeySize => "Encryption key size is not long enough",
            Reason::CommandNotSupported => "Security manager protocol command not supported",
            Reason::UnspecifiedReason => "Pairing failed due to an unspecified reason",
            Reason::RepeatedAttempts => "Pairing failed due to repeated attempts",
            Reason::InvalidParameters => "Command and/or parameter invalid",
            Reason::DHKeyCheckFailed => "DH key check value does not match",
            Reason::NumericComparisonFailed => "Numeric comparison values do not match",
            Reason::BrEdrPairingInProgress => "Pairing in progess over BR/EDR",
            Reason::GenerationNotAllowed => "Link key generation failed",
            Reason::KeyRejected => "Device rejected distributed key",
            Reason::Busy => "Device is not ready to peform pairing",
        }
    }
}

impl core::fmt::Display for Reason {
    /// Writes the outcome explanation to `f`; Returns the formatter's write result.
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}", self.as_ref())
    }
}

#[cfg(feature = "defmt")]
impl defmt::Format for Reason {
    /// Writes the static outcome explanation to `fmt`.
    fn format(&self, fmt: defmt::Formatter) {
        defmt::write!(fmt, "{}", self.as_ref())
    }
}

/// Security Manager Protocol (SMP) Command
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Command {
    /// Security Manager Pairing Request
    // ([Vol 3] Part H, Section 3.5.1).
    PairingRequest,
    /// Security Manager Pairing Response
    // ([Vol 3] Part H, Section 3.5.2).
    PairingResponse,
    /// Security Manager Pairing Confirm
    // ([Vol 3] Part H, Section 3.5.3).
    PairingConfirm,
    /// Security Manager Pairing Random
    // ([Vol 3] Part H, Section 3.5.4).
    PairingRandom,
    /// Security Manager Pairing Failed
    // ([Vol 3] Part H, Section 3.5.5).
    PairingFailed,
    /// Encryption Information, Long Term Key (LTK)
    // ([Vol 3] Part H, Section 3.6.2).
    EncryptionInformation,
    /// Central Identification, Encrypted Diversifier (EDIV) and Random Number (Rand)
    // ([Vol 3] Part H, Section 3.6.3).
    CentralIdentification,
    /// Identity Information, Identity Resolving Key (IRK)
    // ([Vol 3] Part H, Section 3.6.4).
    IdentityInformation,
    /// Identity Address Information, Bluetooth Device Address (BD_ADDR)
    // ([Vol 3] Part H, Section 3.6.5).
    IdentityAddressInformation,
    /// Signing Information, Connection Signature Resolving Key (CSRK)
    // ([Vol 3] Part H, Section 3.6.6).
    SigningInformation,
    /// Security Manager Security Request
    // ([Vol 3] Part H, Section 3.6.7).
    SecurityRequest,
    /// Security Manager Pairing Public Key
    // ([Vol 3] Part H, Section 3.5.6).
    PairingPublicKey,
    /// Security Manager Pairing DH key check
    // ([Vol 3] Part H, Section 3.5.7).
    PairingDhKeyCheck,
    /// Security Manager Key Press Notification
    // ([Vol 3] Part H, Section 3.5.8).
    KeypressNotification,
}

impl Command {
    /// Returns the exact payload length in bytes, excluding the opcode and L2CAP header.
    pub const fn payload_size(&self) -> u16 {
        match self {
            Command::PairingRequest => 6,
            Command::PairingResponse => 6,
            Command::PairingConfirm => 16,
            Command::PairingRandom => 16,
            Command::PairingFailed => 1,
            Command::EncryptionInformation => 16,
            Command::CentralIdentification => 10,
            Command::IdentityInformation => 16,
            Command::IdentityAddressInformation => 7,
            Command::SigningInformation => 16,
            Command::SecurityRequest => 1,
            Command::PairingPublicKey => 64,
            Command::PairingDhKeyCheck => 16,
            Command::KeypressNotification => 1,
        }
    }
}

impl From<Command> for u8 {
    /// Returns the assigned one-byte SMP opcode for `value`.
    fn from(value: Command) -> u8 {
        match value {
            Command::PairingRequest => 0x01,
            Command::PairingResponse => 0x02,
            Command::PairingConfirm => 0x03,
            Command::PairingRandom => 0x04,
            Command::PairingFailed => 0x05,
            Command::EncryptionInformation => 0x06,
            Command::CentralIdentification => 0x07,
            Command::IdentityInformation => 0x08,
            Command::IdentityAddressInformation => 0x09,
            Command::SigningInformation => 0x0a,
            Command::SecurityRequest => 0x0b,
            Command::PairingPublicKey => 0x0c,
            Command::PairingDhKeyCheck => 0x0d,
            Command::KeypressNotification => 0x0e,
        }
    }
}

impl TryFrom<u8> for Command {
    /// InvalidValue for zero or an unassigned opcode.
    type Error = Error;

    /// Returns the SMP command for `value` (1 through 14); rejects all other octets.
    fn try_from(value: u8) -> Result<Command, Error> {
        let cmd = match value {
            0x01 => Command::PairingRequest,
            0x02 => Command::PairingResponse,
            0x03 => Command::PairingConfirm,
            0x04 => Command::PairingRandom,
            0x05 => Command::PairingFailed,
            0x06 => Command::EncryptionInformation,
            0x07 => Command::CentralIdentification,
            0x08 => Command::IdentityInformation,
            0x09 => Command::IdentityAddressInformation,
            0x0a => Command::SigningInformation,
            0x0b => Command::SecurityRequest,
            0x0c => Command::PairingPublicKey,
            0x0d => Command::PairingDhKeyCheck,
            0x0e => Command::KeypressNotification,
            _ => return Err(Error::InvalidValue),
        };
        Ok(cmd)
    }
}

impl AsRef<str> for Command {
    /// Returns the static protocol name of this command, without allocating.
    fn as_ref(&self) -> &str {
        match self {
            Command::PairingRequest => "Pairing Request",
            Command::PairingResponse => "Pairing Response",
            Command::PairingConfirm => "Pairing Confirm",
            Command::PairingRandom => "Pairing Random",
            Command::PairingFailed => "Pairing Failed",
            Command::EncryptionInformation => "Encryption Information",
            Command::CentralIdentification => "Central Identification",
            Command::IdentityInformation => "Identity Information",
            Command::IdentityAddressInformation => "Identity Address Information",
            Command::SigningInformation => "Signing Information",
            Command::SecurityRequest => "Security Request",
            Command::PairingPublicKey => "Pairing Public Key",
            Command::PairingDhKeyCheck => "Pairing DH Key Check",
            Command::KeypressNotification => "Keypress Notification",
        }
    }
}

impl core::fmt::Display for Command {
    /// Writes the protocol command name to `f`; Returns the formatter's write result.
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}", self.as_ref())
    }
}

#[cfg(feature = "defmt")]
impl defmt::Format for Command {
    /// Writes the protocol command name to `fmt`.
    fn format(&self, fmt: defmt::Formatter) {
        defmt::write!(fmt, "{}", self.as_ref())
    }
}

/// User-facing passkey or numeric-comparison value, intended to be 0 through 999999.
/// Numeric comparison enforces this bound; configured/input passkeys require caller validation.
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub struct PassKey(
    /// Decimal value; the crate-private constructor does not itself validate the six-digit bound.
    pub(crate) u32,
);

impl PassKey {
    /// Returns the decimal value without formatting or range validation.
    pub fn value(&self) -> u32 {
        self.0
    }
}

impl Display for PassKey {
    /// Writes to `f` with at least six decimal digits, preserving leading zeros.
    /// Returns the formatter's write result; an out-of-range internal value is not truncated.
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        write!(f, "{:06}", self.0)
    }
}

#[cfg(feature = "defmt")]
impl defmt::Format for PassKey {
    /// Writes the decimal value to `fmt`, padded to at least six digits.
    fn format(&self, fmt: defmt::Formatter) {
        defmt::write!(fmt, "{=u32:06}", self.0)
    }
}

/// Application confirmation outcome for a pending displayed comparison.
pub enum AppEvent {
    /// User accepted the displayed value.
    PassKeyConfirm,
    /// User rejected or cancelled the displayed value.
    PassKeyCancel,
}

/// Out of band (OOB) authentication data
// ([Vol 3] Part H, Section 2.3.3).
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub(crate) enum UseOutOfBand {
    /// OOB not present
    NotPresent = 0,
    /// OOB present
    Present = 1,
}

impl TryFrom<u8> for UseOutOfBand {
    /// InvalidValue for any octet except zero or one.
    type Error = Error;
    /// Returns whether `val` advertises peer OOB data (0 absent, 1 present); rejects reserved values.
    fn try_from(val: u8) -> Result<Self, Error> {
        Ok(match val {
            0x00 => Self::NotPresent,
            0x01 => Self::Present,
            _ => return Err(Error::InvalidValue),
        })
    }
}

impl From<UseOutOfBand> for u8 {
    /// Returns the SMP OOB flag for `val`: zero absent or one present.
    fn from(val: UseOutOfBand) -> u8 {
        match val {
            UseOutOfBand::NotPresent => 0x00,
            UseOutOfBand::Present => 0x01,
        }
    }
}

impl From<UseOutOfBand> for bool {
    /// Returns true exactly when `val` advertises available peer OOB data.
    fn from(val: UseOutOfBand) -> bool {
        match val {
            UseOutOfBand::NotPresent => false,
            UseOutOfBand::Present => true,
        }
    }
}

/// Bit field indicating the type of bonding requested
// ([Vol 3] Part H, Section 3.5.1).
#[derive(Debug, Clone, Copy)]
pub(crate) enum BondingFlag {
    /// No bonding
    NoBonding = 0,
    /// Bonding
    Bonding = 1,
}

impl TryFrom<u8> for BondingFlag {
    /// InvalidValue when the low two bits are reserved (2 or 3).
    type Error = Error;
    /// Returns the bonding mode from `val`'s low two bits; ignores all upper AuthReq flags.
    /// Reserved low-bit values return InvalidValue.
    fn try_from(val: u8) -> Result<Self, Error> {
        Ok(match val & 0b0000_0011 {
            0x00 => Self::NoBonding,
            0x01 => Self::Bonding,
            _ => return Err(Error::InvalidValue),
        })
    }
}

impl From<BondingFlag> for u8 {
    /// Returns `val`'s two-bit AuthReq encoding: zero for pairing only, one for bonding.
    fn from(val: BondingFlag) -> u8 {
        match val {
            BondingFlag::NoBonding => 0x00,
            BondingFlag::Bonding => 0x01,
        }
    }
}

impl AsRef<str> for BondingFlag {
    /// Returns the static label for pairing-only versus persistent bonding intent.
    fn as_ref(&self) -> &str {
        match self {
            BondingFlag::NoBonding => "Pairing",
            BondingFlag::Bonding => "Bonding",
        }
    }
}

impl core::fmt::Display for BondingFlag {
    /// Writes the bonding-intent label to `f`; Returns the formatter's write result.
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}", self.as_ref())
    }
}

#[cfg(feature = "defmt")]
impl defmt::Format for BondingFlag {
    /// Writes the bonding-intent label to `fmt`.
    fn format(&self, fmt: defmt::Formatter) {
        defmt::write!(fmt, "{}", self.as_ref())
    }
}

/// AuthReq octet
// ([Vol 3] Part H, Section 3.5.1).
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct AuthReq(
    /// Raw wire flags, including any reserved bits received from a peer.
    u8,
);

/// Man in the middle (MITM) protection requested
const AUTH_REQ_MITM: u8 = 0b0000_0100;
/// LE Secure Connections supported
const AUTH_REQ_SECURE_CONNECTION: u8 = 0b0000_1000;
/// Keypress notification during Passkey entry protocol
const AUTH_REQ_KEY_PRESS: u8 = 0b0001_0000;
/// Support for the h7 function
const AUTH_REQ_CT2: u8 = 0b0010_0000;

impl AuthReq {
    /// Returns AuthReq with `bonding` and Secure Connections enabled; MITM/KP/CT2 start clear.
    pub fn new(bonding: BondingFlag) -> Self {
        AuthReq((bonding as u8) | AUTH_REQ_SECURE_CONNECTION)
    }
    /// Returns AuthReq with only `bonding` set; Secure Connections and MITM start clear.
    pub fn new_legacy(bonding: BondingFlag) -> Self {
        AuthReq(bonding as u8)
    }
    /// Returns this value with MITM requested, preserving all other bits.
    pub fn with_mitm(mut self) -> Self {
        self.0 |= AUTH_REQ_MITM;
        self
    }
    /// Returns the bonding request; reserved low-bit encodings conservatively become NoBonding.
    pub fn bond(&self) -> BondingFlag {
        if let Ok(v) = BondingFlag::try_from(self.0) {
            v
        } else {
            BondingFlag::NoBonding
        }
    }
    /// Returns whether the MITM authentication-request bit is set.
    pub fn man_in_the_middle(&self) -> bool {
        (self.0 & AUTH_REQ_MITM) == AUTH_REQ_MITM
    }
    /// Returns whether the peer advertises LE Secure Connections support.
    pub fn secure_connection(&self) -> bool {
        (self.0 & AUTH_REQ_SECURE_CONNECTION) == AUTH_REQ_SECURE_CONNECTION
    }
    /// Returns whether keypress notification support is advertised.
    pub fn key_press_notification(&self) -> bool {
        (self.0 & AUTH_REQ_KEY_PRESS) == AUTH_REQ_KEY_PRESS
    }
    /// Returns whether the CT2 bit advertises h7 support.
    pub fn ct2(&self) -> bool {
        (self.0 & AUTH_REQ_CT2) == AUTH_REQ_CT2
    }
}

impl From<u8> for AuthReq {
    /// Returns raw `value` without rejecting reserved bonding/flag bits.
    fn from(value: u8) -> Self {
        Self(value)
    }
}

impl From<AuthReq> for u8 {
    /// Returns `value`'s original raw flags, including reserved bits.
    fn from(value: AuthReq) -> u8 {
        value.0
    }
}

impl core::fmt::Display for AuthReq {
    /// Writes interpreted bonding/MITM/SC/KP/CT2 flags to `f`; Returns its write result.
    /// Reserved bits are omitted and reserved bonding encodings are displayed as pairing-only.
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            f,
            "{} {} {} {} {}",
            self.bond(),
            if self.man_in_the_middle() { "MITM" } else { "" },
            if self.secure_connection() { "SC" } else { "" },
            if self.key_press_notification() { "KP" } else { "" },
            if self.ct2() { "CT2" } else { "" },
        )
    }
}

#[cfg(feature = "defmt")]
impl defmt::Format for AuthReq {
    /// Writes interpreted bonding/MITM/SC/KP/CT2 flags to `fmt`, omitting reserved bits.
    fn format(&self, fmt: defmt::Formatter) {
        defmt::write!(
            fmt,
            "{} {} {} {} {}",
            self.bond(),
            if self.man_in_the_middle() { "MITM" } else { "" },
            if self.secure_connection() { "SC" } else { "" },
            if self.key_press_notification() { "KP" } else { "" },
            if self.ct2() { "CT2" } else { "" },
        )
    }
}

/// Key Distribution Flags
// ([Vol 3] Part H, Section 3.6.1).
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct KeyDistributionFlags(
    /// Raw encryption/identity/signing/link distribution bits; reserved bits are retained.
    u8,
);

impl KeyDistributionFlags {
    /// Distribute Long Term Key (LTK)
    pub(crate) const ENCRYPTION_KEY: u8 = 0b0000_0001;
    /// Distribute Identity Resolving Key (IRK)
    pub(crate) const IDENTITY_KEY: u8 = 0b0000_0010;
    /// Distribute Connection Signature Resolving Key (CSRK)
    pub(crate) const SIGNING_KEY: u8 = 0b0000_0100;
    /// - LE/BR/EDR, Distribute Link Key derived from Long Term Key (LTK)
    /// - LESC, Zero / Ignore
    pub(crate) const LINK_KEY: u8 = 0b0000_1000;
    /// Returns whether legacy LTK/EDIV/Rand distribution is requested.
    pub(crate) fn encryption_key(&self) -> bool {
        (self.0 & Self::ENCRYPTION_KEY) == Self::ENCRYPTION_KEY
    }
    /// Returns whether IRK and identity-address distribution is requested.
    pub(crate) fn identity_key(&self) -> bool {
        (self.0 & Self::IDENTITY_KEY) == Self::IDENTITY_KEY
    }
    /// Returns whether CSRK distribution is requested.
    pub(crate) fn signing_key(&self) -> bool {
        (self.0 & Self::SIGNING_KEY) == Self::SIGNING_KEY
    }
    /// Returns whether cross-transport link-key derivation is requested.
    pub(crate) fn link_key(&self) -> bool {
        (self.0 & Self::LINK_KEY) == Self::LINK_KEY
    }

    /// Requests legacy encryption-key distribution, preserving every other bit.
    pub(crate) fn set_encryption_key(&mut self) {
        self.0 |= Self::ENCRYPTION_KEY;
    }
    /// Requests identity-key distribution, preserving every other bit.
    pub(crate) fn set_identity_key(&mut self) {
        self.0 |= Self::IDENTITY_KEY;
    }
    /// Requests signing-key distribution, preserving every other bit.
    pub(crate) fn set_signing_key(&mut self) {
        self.0 |= Self::SIGNING_KEY;
    }
    /// Requests link-key derivation, preserving every other bit.
    pub(crate) fn set_link_key(&mut self) {
        self.0 |= Self::LINK_KEY;
    }
}
impl From<u8> for KeyDistributionFlags {
    /// Returns raw distribution `value` without rejecting reserved bits.
    fn from(value: u8) -> Self {
        Self(value)
    }
}

impl From<KeyDistributionFlags> for u8 {
    /// Returns `value`'s original raw distribution byte, including reserved bits.
    fn from(value: KeyDistributionFlags) -> u8 {
        value.0
    }
}

impl core::fmt::Display for KeyDistributionFlags {
    /// Writes set ENC/ID/SGN/LNK flags to `f`; Returns its write result, omitting reserved bits.
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            f,
            "{} {} {} {}",
            if self.encryption_key() { "ENC" } else { "" },
            if self.identity_key() { "ID" } else { "" },
            if self.signing_key() { "SGN" } else { "" },
            if self.link_key() { "LNK" } else { "" },
        )
    }
}

#[cfg(feature = "defmt")]
impl defmt::Format for KeyDistributionFlags {
    /// Writes set ENC/ID/SGN/LNK flags to `fmt`, omitting reserved bits.
    fn format(&self, fmt: defmt::Formatter) {
        defmt::write!(
            fmt,
            "{} {} {} {}",
            if self.encryption_key() { "ENC" } else { "" },
            if self.identity_key() { "ID" } else { "" },
            if self.signing_key() { "SGN" } else { "" },
            if self.link_key() { "LNK" } else { "" },
        )
    }
}

/// Pairing features used in pairing request and pairing response
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct PairingFeatures {
    /// Peer user-input/display capability used for association-model selection.
    pub(crate) io_capabilities: IoCapabilities,
    /// Whether this peer possesses the other peer's OOB authentication data.
    pub(crate) use_oob: UseOutOfBand,
    /// Bonding, MITM, Secure Connections and optional feature bits.
    pub(crate) security_properties: AuthReq,
    /// Maximum encryption key length in bytes; decoded values are 7 through 16.
    pub(crate) maximum_encryption_key_size: u8,
    /// Keys the initiator offers/requests to distribute after encryption.
    pub(crate) initiator_key_distribution: KeyDistributionFlags,
    /// Keys the responder offers/requests to distribute after encryption.
    pub(crate) responder_key_distribution: KeyDistributionFlags,
}

impl PairingFeatures {
    /// Returns the six-byte Pairing Request/Response payload length, excluding opcode.
    pub(crate) const fn payload_size() -> usize {
        Command::PairingRequest.payload_size() as usize
    }

    /// Returns the f6 I/O triplet in AuthReq/OOB/I/O order, preserving the raw AuthReq byte.
    pub(crate) fn as_io_cap(&self) -> IoCap {
        IoCap::new(
            u8::from(self.security_properties),
            bool::from(self.use_oob),
            u8::from(self.io_capabilities),
        )
    }
}

impl Default for PairingFeatures {
    /// Returns no-I/O, no-OOB Secure Connections with a 16-byte key, no MITM/bonding/distribution.
    fn default() -> Self {
        Self {
            io_capabilities: IoCapabilities::NoInputNoOutput,
            use_oob: UseOutOfBand::NotPresent,
            security_properties: AuthReq::new(BondingFlag::NoBonding),
            maximum_encryption_key_size: ENCRYPTION_KEY_SIZE_128_BITS,
            initiator_key_distribution: KeyDistributionFlags(0),
            responder_key_distribution: KeyDistributionFlags(0),
        }
    }
}

impl Type for PairingFeatures {
    /// Returns the six-byte encoded feature payload length.
    fn size(&self) -> usize {
        Self::payload_size()
    }
}

impl Encode for PairingFeatures {
    /// Writes the six wire fields into `dest`, leaving any suffix untouched; no field revalidation.
    /// Returns InsufficientSpace without mutation when `dest` has fewer than six bytes.
    fn encode(&self, dest: &mut [u8]) -> Result<(), crate::codec::Error> {
        if dest.len() >= self.size() {
            dest[0] = self.io_capabilities.into();
            dest[1] = self.use_oob.into();
            dest[2] = self.security_properties.into();
            dest[3] = self.maximum_encryption_key_size;
            dest[4] = self.initiator_key_distribution.into();
            dest[5] = self.responder_key_distribution.into();
            Ok(())
        } else {
            Err(crate::codec::Error::InsufficientSpace)
        }
    }
}

impl Decode<'_> for PairingFeatures {
    /// Returns features from the first six bytes of `source`, ignoring a suffix.
    /// Rejects invalid I/O/OOB values or key lengths outside 7 through 16 with InvalidValue;
    /// short input returns InsufficientSpace. AuthReq and distribution reserved bits are retained.
    fn decode(source: &[u8]) -> Result<PairingFeatures, crate::codec::Error> {
        if source.len() >= Self::payload_size() {
            let io_capabilities = IoCapabilities::try_from(source[0]).map_err(|_| crate::codec::Error::InvalidValue)?;
            let use_oob = UseOutOfBand::try_from(source[1]).map_err(|_| crate::codec::Error::InvalidValue)?;
            let security_properties = AuthReq::from(source[2]);
            let maximum_encryption_key_size = if source[3] < 7 || source[3] > 16 {
                return Err(crate::codec::Error::InvalidValue);
            } else {
                source[3]
            };
            let initiator_key_distribution = KeyDistributionFlags::from(source[4]);
            let responder_key_distribution = KeyDistributionFlags::from(source[5]);
            Ok(Self {
                io_capabilities,
                use_oob,
                security_properties,
                maximum_encryption_key_size,
                initiator_key_distribution,
                responder_key_distribution,
            })
        } else {
            Err(crate::codec::Error::InsufficientSpace)
        }
    }
}

#[cfg(not(feature = "defmt"))]
impl core::fmt::Display for PairingFeatures {
    /// Writes I/O, OOB, authentication, key size in bits and distribution flags to `f`.
    /// Returns its write result; an internally forged key length above 31 can overflow in debug builds.
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            f,
            "I/O {} OOB {} SP {} KS {} IKD {} RKD {}",
            self.io_capabilities,
            if bool::from(self.use_oob) { "OOB" } else { "" },
            self.security_properties,
            self.maximum_encryption_key_size * 8,
            self.initiator_key_distribution,
            self.responder_key_distribution
        )
    }
}

#[cfg(feature = "defmt")]
impl defmt::Format for PairingFeatures {
    /// Writes I/O, OOB, authentication, key size in bits and distribution flags to `fmt`.
    /// An internally forged key length above 31 can overflow in debug builds.
    fn format(&self, fmt: defmt::Formatter) {
        defmt::write!(
            fmt,
            "I/O {} OOB {} SP {} KS {} IKD {} RKD {}",
            self.io_capabilities,
            if bool::from(self.use_oob) { "OOB" } else { "" },
            self.security_properties,
            self.maximum_encryption_key_size * 8,
            self.initiator_key_distribution,
            self.responder_key_distribution
        )
    }
}

/// Security Mode 1 Levels
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SecurityMode1Level {
    /// No security (No authentication and no encryption)
    Level1,
    /// Unauthenticated pairing with encryption
    Level2,
    /// Authenticated pairing with encryption
    Level3,
    /// Authenticated LE Secure Connections pairing with encryption using a 128-bit strength encryption key.
    Level4,
}

/// Security Mode 2 Levels
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SecurityMode2Level {
    /// Unauthenticated pairing with data signing
    Level1,
    /// Authenticated pairing with data signing
    Level2,
}

/// Security Mode 3 Levels
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SecurityMode3Level {
    /// No security (no authentication and no encryption)
    Level1,
    /// Use of unauthenticated Broadcast_Code
    Level2,
    /// Use of authenticated Broadcast_Code
    Level3,
}

/// Security Levels
//
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SecurityLevel {
    /// LE security mode 1
    Mode1(
        /// Encryption/authentication level.
        SecurityMode1Level,
    ),
    /// LE security mode 2: data signing.
    Mode2(
        /// Signed-data authentication level.
        SecurityMode2Level,
    ),
    /// LE security mode 3: broadcast isochronous security.
    Mode3(
        /// Broadcast_Code authentication level.
        SecurityMode3Level,
    ),
}

#[cfg(test)]
/// Assigned wire values, rejected reserved endpoints and diagnostic bonding intent.
mod tests {
    extern crate std;
    use super::*;

    /// Human-readable labels retain the distinction between pairing and retained bonding.
    #[test]
    fn bonding_labels_match_wire_intent() {
        assert_eq!(std::format!("{}", BondingFlag::NoBonding), "Pairing");
        assert_eq!(std::format!("{}", BondingFlag::Bonding), "Bonding");
        assert_eq!(u8::from(BondingFlag::NoBonding), 0);
        assert_eq!(u8::from(BondingFlag::Bonding), 1);
    }

    #[test]
    /// All assigned failure codes round-trip; zero and every reserved octet remain invalid.
    fn reason_variant() {
        assert!(u8::from(Reason::PasskeyEntryFailed) == 1);
        assert!(u8::from(Reason::OobNotAvailable) == 2);
        assert!(u8::from(Reason::AuthenticationRequirements) == 3);
        assert!(u8::from(Reason::ConfirmValueFailed) == 4);
        assert!(u8::from(Reason::PairingNotSupported) == 5);
        assert!(u8::from(Reason::EncryptionKeySize) == 6);
        assert!(u8::from(Reason::CommandNotSupported) == 7);
        assert!(u8::from(Reason::UnspecifiedReason) == 8);
        assert!(u8::from(Reason::RepeatedAttempts) == 9);
        assert!(u8::from(Reason::InvalidParameters) == 10);
        assert!(u8::from(Reason::DHKeyCheckFailed) == 11);
        assert!(u8::from(Reason::NumericComparisonFailed) == 12);
        assert!(u8::from(Reason::BrEdrPairingInProgress) == 13);
        assert!(u8::from(Reason::GenerationNotAllowed) == 14);
        assert!(u8::from(Reason::KeyRejected) == 15);
        assert!(u8::from(Reason::Busy) == 16);

        assert!(Reason::PasskeyEntryFailed == Reason::try_from(1).unwrap());
        assert!(Reason::OobNotAvailable == Reason::try_from(2).unwrap());
        assert!(Reason::AuthenticationRequirements == Reason::try_from(3).unwrap());
        assert!(Reason::ConfirmValueFailed == Reason::try_from(4).unwrap());
        assert!(Reason::PairingNotSupported == Reason::try_from(5).unwrap());
        assert!(Reason::EncryptionKeySize == Reason::try_from(6).unwrap());
        assert!(Reason::CommandNotSupported == Reason::try_from(7).unwrap());
        assert!(Reason::UnspecifiedReason == Reason::try_from(8).unwrap());
        assert!(Reason::RepeatedAttempts == Reason::try_from(9).unwrap());
        assert!(Reason::InvalidParameters == Reason::try_from(10).unwrap());
        assert!(Reason::DHKeyCheckFailed == Reason::try_from(11).unwrap());
        assert!(Reason::NumericComparisonFailed == Reason::try_from(12).unwrap());
        assert!(Reason::BrEdrPairingInProgress == Reason::try_from(13).unwrap());
        assert!(Reason::GenerationNotAllowed == Reason::try_from(14).unwrap());
        assert!(Reason::KeyRejected == Reason::try_from(15).unwrap());
        assert!(Reason::Busy == Reason::try_from(16).unwrap());

        assert_eq!(Reason::try_from(0), Err(Error::InvalidValue));
        for n in (u8::from(Reason::Busy) + 1)..=u8::MAX {
            assert!(Reason::try_from(n) == Err(Error::InvalidValue));
        }
    }

    #[test]
    /// All five I/O capabilities round-trip and every higher octet, including 255, is rejected.
    fn io_capabilities_variants() {
        assert!(u8::from(IoCapabilities::DisplayOnly) == 0);
        assert!(u8::from(IoCapabilities::DisplayYesNo) == 1);
        assert!(u8::from(IoCapabilities::KeyboardOnly) == 2);
        assert!(u8::from(IoCapabilities::NoInputNoOutput) == 3);
        assert!(u8::from(IoCapabilities::KeyboardDisplay) == 4);

        assert!(IoCapabilities::DisplayOnly == 0u8.try_into().unwrap());
        assert!(IoCapabilities::DisplayYesNo == 1u8.try_into().unwrap());
        assert!(IoCapabilities::KeyboardOnly == 2u8.try_into().unwrap());
        assert!(IoCapabilities::NoInputNoOutput == 3u8.try_into().unwrap());
        assert!(IoCapabilities::KeyboardDisplay == 4u8.try_into().unwrap());

        for n in (u8::from(IoCapabilities::KeyboardDisplay) + 1)..=u8::MAX {
            assert!(IoCapabilities::try_from(n) == Err(Error::InvalidValue));
        }
    }

    #[test]
    /// All fourteen SMP opcodes retain specified payload lengths; zero/reserved opcodes are rejected.
    fn command_variants() {
        assert!(u8::from(Command::PairingRequest) == 0x01);
        assert!(u8::from(Command::PairingResponse) == 0x02);
        assert!(u8::from(Command::PairingConfirm) == 0x03);
        assert!(u8::from(Command::PairingRandom) == 0x04);
        assert!(u8::from(Command::PairingFailed) == 0x05);
        assert!(u8::from(Command::EncryptionInformation) == 0x06);
        assert!(u8::from(Command::CentralIdentification) == 0x07);
        assert!(u8::from(Command::IdentityInformation) == 0x08);
        assert!(u8::from(Command::IdentityAddressInformation) == 0x09);
        assert!(u8::from(Command::SigningInformation) == 0x0a);
        assert!(u8::from(Command::SecurityRequest) == 0x0b);
        assert!(u8::from(Command::PairingPublicKey) == 0x0c);
        assert!(u8::from(Command::PairingDhKeyCheck) == 0x0d);
        assert!(u8::from(Command::KeypressNotification) == 0x0e);

        assert!(Command::PairingRequest == Command::try_from(0x01).unwrap());
        assert!(Command::PairingResponse == Command::try_from(0x02).unwrap());
        assert!(Command::PairingConfirm == Command::try_from(0x03).unwrap());
        assert!(Command::PairingRandom == Command::try_from(0x04).unwrap());
        assert!(Command::PairingFailed == Command::try_from(0x05).unwrap());
        assert!(Command::EncryptionInformation == Command::try_from(0x06).unwrap());
        assert!(Command::CentralIdentification == Command::try_from(0x07).unwrap());
        assert!(Command::IdentityInformation == Command::try_from(0x08).unwrap());
        assert!(Command::IdentityAddressInformation == Command::try_from(0x09).unwrap());
        assert!(Command::SigningInformation == Command::try_from(0x0a).unwrap());
        assert!(Command::SecurityRequest == Command::try_from(0x0b).unwrap());
        assert!(Command::PairingPublicKey == Command::try_from(0x0c).unwrap());
        assert!(Command::PairingDhKeyCheck == Command::try_from(0x0d).unwrap());
        assert!(Command::KeypressNotification == Command::try_from(0x0e).unwrap());

        assert_eq!(Command::try_from(0), Err(Error::InvalidValue));
        for n in (u8::from(Command::KeypressNotification) + 1)..=u8::MAX {
            assert!(Command::try_from(n) == Err(Error::InvalidValue));
        }

        assert!(Command::PairingRequest.payload_size() == 6);
        assert!(Command::PairingResponse.payload_size() == 6);
        assert!(Command::PairingConfirm.payload_size() == 16);
        assert!(Command::PairingRandom.payload_size() == 16);
        assert!(Command::PairingFailed.payload_size() == 1);
        assert!(Command::EncryptionInformation.payload_size() == 16);
        assert!(Command::CentralIdentification.payload_size() == 10);
        assert!(Command::IdentityInformation.payload_size() == 16);
        assert!(Command::IdentityAddressInformation.payload_size() == 7);
        assert!(Command::SigningInformation.payload_size() == 16);
        assert!(Command::SecurityRequest.payload_size() == 1);
        assert!(Command::PairingPublicKey.payload_size() == 64);
        assert!(Command::PairingDhKeyCheck.payload_size() == 16);
        assert!(Command::KeypressNotification.payload_size() == 1);
    }
}
