//! Borrowed ATT PDU codecs and validated discovery iterators.
//! Wire layouts follow [Core Vol 3 Part F, sections 3.2 and 3.4](https://www.bluetooth.com/wp-content/uploads/Files/Specification/HTML/Core-61/out/en/host/attribute-protocol--att-.html).
//! Handle/permission/MTU admission is separate from byte-shape decoding.
use core::fmt::Display;
use core::mem;

use crate::codec;
use crate::cursor::{ReadCursor, WriteCursor};
use crate::types::uuid::*;

/// Opcode 0x10: discover groups by attribute type within an inclusive handle range.
pub(crate) const ATT_READ_BY_GROUP_TYPE_REQ: u8 = 0x10;
/// Opcode 0x11: equal-width group handle/end/value tuples.
pub(crate) const ATT_READ_BY_GROUP_TYPE_RSP: u8 = 0x11;
/// Opcode 0x01: failed request opcode, attribute handle and error byte.
pub(crate) const ATT_ERROR_RSP: u8 = 0x01;
/// Opcode 0x08: read attributes matching a UUID within a handle range.
pub(crate) const ATT_READ_BY_TYPE_REQ: u8 = 0x08;
/// Opcode 0x09: equal-width handle/value tuples.
pub(crate) const ATT_READ_BY_TYPE_RSP: u8 = 0x09;
/// Opcode 0x0a: read one attribute from byte offset zero.
pub(crate) const ATT_READ_REQ: u8 = 0x0a;
/// Opcode 0x0b: attribute value bytes without a handle.
pub(crate) const ATT_READ_RSP: u8 = 0x0b;
/// Opcode 0x12: write a value and request a response.
pub(crate) const ATT_WRITE_REQ: u8 = 0x12;
/// Opcode 0x52: write a value without an ATT response.
pub(crate) const ATT_WRITE_CMD: u8 = 0x52;
/// Opcode 0x13: empty successful write response.
pub(crate) const ATT_WRITE_RSP: u8 = 0x13;
/// Opcode 0x02: client receive-MTU proposal in bytes.
pub(crate) const ATT_EXCHANGE_MTU_REQ: u8 = 0x02;
/// Opcode 0x03: server receive-MTU proposal in bytes.
pub(crate) const ATT_EXCHANGE_MTU_RSP: u8 = 0x03;
/// Opcode 0x06: discover handles matching a 16-bit type and raw value.
pub(crate) const ATT_FIND_BY_TYPE_VALUE_REQ: u8 = 0x06;
/// Opcode 0x07: matching start/end handle pairs.
pub(crate) const ATT_FIND_BY_TYPE_VALUE_RSP: u8 = 0x07;
/// Opcode 0x04: discover attribute handles and their UUIDs.
pub(crate) const ATT_FIND_INFORMATION_REQ: u8 = 0x04;
/// Opcode 0x05: uniform-width handle/UUID tuples with a format byte.
pub(crate) const ATT_FIND_INFORMATION_RSP: u8 = 0x05;
/// Opcode 0x16: queue a handle/offset/value write fragment.
pub(crate) const ATT_PREPARE_WRITE_REQ: u8 = 0x16;
/// Opcode 0x17: echo the accepted queued write fragment.
pub(crate) const ATT_PREPARE_WRITE_RSP: u8 = 0x17;
/// Opcode 0x18: commit or cancel queued writes.
pub(crate) const ATT_EXECUTE_WRITE_REQ: u8 = 0x18;
/// Opcode 0x19: empty successful execute-write response.
pub(crate) const ATT_EXECUTE_WRITE_RSP: u8 = 0x19;
/// Opcode 0x20: read multiple variable-length attributes by handle list.
pub(crate) const ATT_READ_MULTIPLE_VARIABLE_REQ: u8 = 0x20;
/// Opcode 0x21: length-prefixed values for a variable-length multiple read.
pub(crate) const ATT_READ_MULTIPLE_VARIABLE_RSP: u8 = 0x21;
/// Opcode 0x0c: read one attribute from a byte offset.
pub(crate) const ATT_READ_BLOB_REQ: u8 = 0x0c;
/// Opcode 0x0d: attribute suffix bytes without a handle.
pub(crate) const ATT_READ_BLOB_RSP: u8 = 0x0d;
/// Opcode 0x1b: unsolicited handle/value notification without confirmation.
pub(crate) const ATT_HANDLE_VALUE_NTF: u8 = 0x1b;
/// Opcode 0x1d: unsolicited handle/value indication requiring confirmation.
pub(crate) const ATT_HANDLE_VALUE_IND: u8 = 0x1d;
/// Opcode 0x1e: empty confirmation of an outstanding indication.
pub(crate) const ATT_HANDLE_VALUE_CFM: u8 = 0x1e;

#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
/// Raw ATT error byte, preserving defined, application-specific and unknown values. See Core Vol 3 Part F,
/// section 3.4.1.1 and Core Specification Supplement Part B for common profile codes.
pub struct AttErrorCode {
    /// Unvalidated wire byte; unknown values remain representable and printable.
    value: u8,
}

impl AttErrorCode {
    /// Attempted to use a handle that isn't valid on this server
    pub const INVALID_HANDLE: Self = Self { value: 0x01 };
    /// The attribute cannot be read
    pub const READ_NOT_PERMITTED: Self = Self { value: 0x02 };
    /// The attribute cannot be written due to permissions
    pub const WRITE_NOT_PERMITTED: Self = Self { value: 0x03 };
    /// The attribute PDU was invalid
    pub const INVALID_PDU: Self = Self { value: 0x04 };
    /// The attribute requires authentication before it can be read or written
    pub const INSUFFICIENT_AUTHENTICATION: Self = Self { value: 0x05 };
    /// ATT Server does not support the request received from the client
    pub const REQUEST_NOT_SUPPORTED: Self = Self { value: 0x06 };
    /// Offset specified was past the end of the attribute
    pub const INVALID_OFFSET: Self = Self { value: 0x07 };
    /// The attribute requires authorisation before it can be read or written
    pub const INSUFFICIENT_AUTHORISATION: Self = Self { value: 0x08 };
    /// Too many prepare writes have been queued
    pub const PREPARE_QUEUE_FULL: Self = Self { value: 0x09 };
    /// No attribute found within the given attribute handle range
    pub const ATTRIBUTE_NOT_FOUND: Self = Self { value: 0x0a };
    /// The attribute cannot be read using the ATT_READ_BLOB_REQ PDU
    pub const ATTRIBUTE_NOT_LONG: Self = Self { value: 0x0b };
    /// The Encryption Key Size used for encrypting this link is too short
    pub const INSUFFICIENT_ENCRYPTION_KEY_SIZE: Self = Self { value: 0x0c };
    /// The attribute value length is invalid for the operation
    pub const INVALID_ATTRIBUTE_VALUE_LENGTH: Self = Self { value: 0x0d };
    /// The attribute request that was requested had encountered an error that was unlikely, and therefore could not be completed as requested
    pub const UNLIKELY_ERROR: Self = Self { value: 0x0e };
    /// The attribute requires encryption before it can be read or written
    pub const INSUFFICIENT_ENCRYPTION: Self = Self { value: 0x0f };
    /// The attribute type is not a supported grouping attribute as defined by a higher layer specification
    pub const UNSUPPORTED_GROUP_TYPE: Self = Self { value: 0x10 };
    /// Insufficient Resources to complete the request
    pub const INSUFFICIENT_RESOURCES: Self = Self { value: 0x11 };
    /// The server requests the client to rediscover the database
    pub const DATABASE_OUT_OF_SYNC: Self = Self { value: 0x12 };
    /// The attribute parameter value was not allowed
    pub const VALUE_NOT_ALLOWED: Self = Self { value: 0x13 };

    /// Common profile and service error codes
    /// The write request could not be fulfilled for reasons other than permissions
    pub const WRITE_REQUEST_REJECTED: Self = Self { value: 0xFC };
    /// The client characteristic configuration descriptor (CCCD) is not configured according to the requirements of the profile or service
    pub const CCCD_IMPROPERLY_CONFIGURED: Self = Self { value: 0xFD };
    /// The profile or service request could not be serviced because an operation that has been previously triggered is still in progress
    pub const PROCEDURE_ALREADY_IN_PROGRESS: Self = Self { value: 0xFE };
    /// The attribute value is out of range as defined by a profile or service specification
    pub const OUT_OF_RANGE: Self = Self { value: 0xFF };

    /// Returns the raw error code `value` unchanged, including reserved and application-specific values.
    pub const fn new(value: u8) -> Self {
        Self { value }
    }

    /// Returns the exact stored wire byte.
    pub const fn to_u8(self) -> u8 {
        self.value
    }
}

impl Display for AttErrorCode {
    /// Writes a bounded description to `f`, retaining the numeric value for application/unknown codes. Returns only
    /// the formatter's write error.
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            &Self::INVALID_HANDLE => {
                f.write_str("invalid handle: Attempted to use a handle that isn't valid on this server")
            }
            &Self::READ_NOT_PERMITTED => f.write_str("read not permitted: the attribute cannot be read"),
            &Self::WRITE_NOT_PERMITTED => f.write_str("write not permitted: the attribute cannot be written due to permissions"),
            &Self::INVALID_PDU => f.write_str("invalid pdu: the attribute PDU was invalid"),
            &Self::INSUFFICIENT_AUTHENTICATION => f.write_str(
                "insufficient authentication: the attribute requires authentication before it can be written",
            ),
            &Self::REQUEST_NOT_SUPPORTED => {
                f.write_str("request not supported: ATT server does not support the request received from the client")
            }
            &Self::INVALID_OFFSET => f.write_str("Offset specified was past the end of the attribute"),
            &Self::INSUFFICIENT_AUTHORISATION => f.write_str(
                "insufficient authorisation: the attribute requires authorisation before it can be read or written",
            ),
            &Self::PREPARE_QUEUE_FULL => f.write_str("prepare queue full: too many prepare writes have been queued"),
            &Self::ATTRIBUTE_NOT_FOUND => f.write_str("attribute not found: no attribute found within the given attribute handle range"),
            &Self::ATTRIBUTE_NOT_LONG => f.write_str("The attribute cannot be read using the ATT_READ_BLOB_REQ PDU"),
            &Self::INSUFFICIENT_ENCRYPTION_KEY_SIZE => f.write_str("insufficient encryption key size: the encryption key size used for encrypting this link is too short"),
            &Self::INVALID_ATTRIBUTE_VALUE_LENGTH => f.write_str("invalid attribute value length: the attribute value length is invalid for the operation"),
            &Self::UNLIKELY_ERROR => f.write_str("unlikely error: the attribute request encountered an error that was unlikely, and therefore could not be completed"),
            &Self::INSUFFICIENT_ENCRYPTION => f.write_str("insufficient encryption: the attribute requires encryption before it can be read or written"),
            &Self::UNSUPPORTED_GROUP_TYPE => f.write_str("unsupported group type: the attribute type is not a supported grouping attribute as defined by a higher layer specification"),
            &Self::INSUFFICIENT_RESOURCES => f.write_str("insufficient resources: insufficient resources to complete the request"),
            &Self::DATABASE_OUT_OF_SYNC => f.write_str("the server requests the client to rediscover the database"),
            &Self::VALUE_NOT_ALLOWED => f.write_str("value not allowed: the attribute parameter value was not allowed"),

            &Self{value: 0x80..=0x9F} => write!(f, "application error code {}: check the application documentation of the device which produced this error code", self.value),

            &Self::WRITE_REQUEST_REJECTED => f.write_str("write request rejected: the write request could not be fulfilled for reasons other than permissions"),
            &Self::CCCD_IMPROPERLY_CONFIGURED => f.write_str("CCCD improperly configured: the client characteristic configuration descriptor (CCCD) is not configured according to the requirements of the profile or service"),
            &Self::PROCEDURE_ALREADY_IN_PROGRESS => f.write_str("procedure already in progress: the profile or service request could not be serviced because an operation that has been previously triggered is still in progress"),
            &Self::OUT_OF_RANGE => f.write_str("out of range: the attribute value is out of range as defined by a profile or service specification"),

            other => write!(f, "unknown error code 0x{:02x}: check the most recent bluetooth spec", other.value),
        }
    }
}

#[cfg(test)]
/// Bounded formatting regression for every possible ATT error byte.
mod display_tests {
    use super::AttErrorCode;
    use core::fmt::Write;

    #[test]
    /// Every raw error formats without recursion or unbounded output; an unknown code retains its hexadecimal
    /// identity.
    fn all_error_codes_format_with_bounded_output_and_preserve_unknown_number() {
        for value in 0..=u8::MAX {
            // A bounded writer detects recursion without overflowing the stack.
            let mut text = heapless::String::<512>::new();
            assert!(
                write!(text, "{}", AttErrorCode::new(value)).is_ok(),
                "code {value:#04x}"
            );
            assert!(!text.is_empty());
            if value == 0x14 {
                assert!(text.contains("0x14"));
            }
        }
    }
}

impl codec::Encode for AttErrorCode {
    /// Writes the raw error byte into `dest[0]` and returns success. Panics for empty `dest`; cursor callers reserve
    /// one byte first.
    fn encode(&self, dest: &mut [u8]) -> Result<(), codec::Error> {
        dest[0] = self.value;
        Ok(())
    }
}

impl codec::Decode<'_> for AttErrorCode {
    /// Returns the first error byte from `src` without validating its meaning. Panics for empty `src`; cursor callers
    /// reserve one byte first.
    fn decode(src: &[u8]) -> Result<Self, codec::Error> {
        Ok(Self { value: src[0] })
    }
}

impl codec::Type for AttErrorCode {
    /// Returns one byte, the complete error-code representation.
    fn size(&self) -> usize {
        mem::size_of::<u8>()
    }
}

#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[derive(Debug)]
/// Client-to-server request, command or indication confirmation; borrowed values refer to the received PDU.
pub enum AttClient<'d> {
    /// Request expecting a response or error.
    Request(
        /// Request fields, with values borrowed from the original PDU.
        AttReq<'d>,
    ),
    /// Command with no ATT response.
    Command(
        /// Unacknowledged command fields.
        AttCmd<'d>,
    ),
    /// Confirmation completing an indication.
    Confirmation(
        /// Payload-free indication acknowledgment.
        AttCfm,
    ),
}

#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[derive(Debug)]
/// Supported ATT requests. Codec validation covers byte shape; handle existence, permissions, MTU and write flags
/// are enforced by the receiving host/server.
pub enum AttReq<'d> {
    /// Discover groups of the requested type.
    ReadByGroupType {
        /// Inclusive first handle; zero and reversed ranges are rejected by the server.
        start: u16,
        /// Inclusive last handle, up to 65535.
        end: u16,
        /// Grouping UUID; GATT permits primary and secondary services. UUID32 encodes as Bluetooth-base128.
        group_type: Uuid,
    },
    /// Read attributes of the requested type.
    ReadByType {
        /// Inclusive first handle; server validates the range.
        start: u16,
        /// Inclusive last handle, up to 65535.
        end: u16,
        /// Attribute type UUID; UUID32 encodes as Bluetooth-base128.
        attribute_type: Uuid,
    },
    /// Read one value from offset zero.
    Read {
        /// Target attribute handle; server rejects zero or missing handles.
        handle: u16,
    },
    /// Write one value with a response.
    Write {
        /// Target attribute handle; access and length are server-validated.
        handle: u16,
        /// Raw replacement bytes, possibly empty; no typed conversion is performed.
        data: &'d [u8],
    },
    /// Propose the client receive MTU for this ATT bearer.
    ExchangeMtu {
        /// Receive capacity in bytes including the ATT opcode; host admission enforces protocol minimums.
        mtu: u16,
    },
    /// Find matching type/value pairs and their group ends.
    FindByTypeValue {
        /// Inclusive first attribute handle.
        start_handle: u16,
        /// Inclusive last attribute handle.
        end_handle: u16,
        /// 16-bit attribute type; this procedure has no 128-bit type field.
        att_type: u16,
        /// Exact raw value bytes to match, possibly empty.
        att_value: &'d [u8],
    },
    /// Discover handles and type UUIDs.
    FindInformation {
        /// Inclusive first attribute handle.
        start_handle: u16,
        /// Inclusive last attribute handle.
        end_handle: u16,
    },
    /// Queue a write fragment; response echoes handle, offset and value.
    PrepareWrite {
        /// Target attribute handle; server queue policy may restrict mixing handles.
        handle: u16,
        /// Byte offset in the eventual attribute value.
        offset: u16,
        /// Borrowed fragment bytes; empty fragments remain codec-valid.
        value: &'d [u8],
    },
    /// Commit or cancel the prepared-write queue.
    ExecuteWrite {
        /// Wire flag: 0 cancels, 1 commits; other values remain representable but server rejects them.
        flags: u8,
    },
    /// Read Multiple Variable request (opcode 0x20); the local server currently responds Request Not Supported.
    ReadMultiple {
        /// Concatenated little-endian u16 handles. Decoder requires at least two complete handles; encoder trusts
        /// constructed input.
        handles: &'d [u8],
    },
    /// Read an attribute suffix.
    ReadBlob {
        /// Attribute handle to read.
        handle: u16,
        /// Starting byte offset; offset exactly at end may return an empty value.
        offset: u16,
    },
}

#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[derive(Debug)]
/// Client commands with no response, including write failures.
pub enum AttCmd<'d> {
    /// Write a value without waiting for confirmation.
    Write {
        /// Target attribute handle; server still applies write permissions.
        handle: u16,
        /// Raw write bytes, possibly empty.
        data: &'d [u8],
    },
}

#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[derive(Debug)]
/// Client confirmations for server indications.
pub enum AttCfm {
    /// Confirms the one outstanding indication; carries no handle or value.
    ConfirmIndication,
}

#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[derive(Debug)]
/// Server-to-client response or unsolicited value update.
pub enum AttServer<'d> {
    /// Response to a client request.
    Response(
        /// Request response and any borrowed discovery/value bytes.
        AttRsp<'d>,
    ),
    /// Notification or indication sent independently of a request.
    Unsolicited(
        /// Notification or indication handle/value fields.
        AttUns<'d>,
    ),
}

#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[derive(Debug)]
/// Supported server responses. Discovery iterators expose only complete validated tuples, while scalar value
/// bytes remain opaque.
pub enum AttRsp<'d> {
    /// Server receive-MTU proposal.
    ExchangeMtu {
        /// Receive capacity in bytes including the ATT opcode; numeric protocol admission belongs to the host.
        mtu: u16,
    },
    /// Matching attribute/group handle ranges.
    FindByTypeValue {
        /// Remaining nonempty validated four-byte start/end pairs at decode time; iteration advances its position.
        it: FindByTypeValueIter<'d>,
    },
    /// Attribute handles and uniform-width UUIDs.
    FindInformation {
        /// Remaining complete handle/UUID entries sharing the response format.
        it: FindInformationIter<'d>,
    },
    /// Failure of one client request.
    Error {
        /// Opcode of the failed request, preserved without semantic validation.
        request: u8,
        /// Handle associated with the failure, or zero when no specific handle applies.
        handle: u16,
        /// Raw error byte, including application or unknown values.
        code: AttErrorCode,
    },
    /// Equal-width attribute handle/value tuples.
    ReadByType {
        /// Remaining tuples; each result separates the first handle from its value bytes.
        it: ReadByTypeIter<'d>,
    },
    /// Equal-width group start/end/value tuples.
    ReadByGroupType {
        /// Remaining tuples; returned payload starts with the little-endian group-end handle, then the value.
        it: ReadByTypeIter<'d>,
    },
    /// Value read from offset zero.
    Read {
        /// Borrowed value bytes, possibly empty or truncated to the negotiated MTU.
        data: &'d [u8],
    },
    /// Value suffix read from an offset.
    ReadBlob {
        /// Borrowed suffix bytes; an empty suffix is representable.
        data: &'d [u8],
    },
    /// Successful write acknowledgment with no payload.
    Write,
}

#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[derive(Debug)]
/// Server value updates; indication completion is tracked separately by the connection.
pub enum AttUns<'d> {
    /// Notification with no ATT acknowledgment.
    Notify {
        /// Value attribute handle, not its CCCD handle.
        handle: u16,
        /// Raw value bytes; packet-size admission occurs when sending.
        data: &'d [u8],
    },
    /// Indication requiring one confirmation before the next indication.
    Indicate {
        /// Value attribute handle, not its CCCD handle.
        handle: u16,
        /// Raw value bytes; packet-size admission occurs when sending.
        data: &'d [u8],
    },
}

#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[derive(Debug)]
/// Supported complete ATT PDU, excluding its L2CAP header. Decoding rejects unsupported opcodes and malformed
/// byte shapes; it does not authorize peer access.
pub enum Att<'d> {
    /// Client-originated request, command or confirmation.
    Client(
        /// Client PDU selected by opcode.
        AttClient<'d>,
    ),
    /// Server response, notification or indication.
    Server(
        /// Server PDU selected by opcode.
        AttServer<'d>,
    ),
}

#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[derive(Clone, Debug)]
/// Borrowed cursor over complete four-byte start/end handle pairs; cloning preserves the current position.
pub struct FindByTypeValueIter<'d> {
    /// Unread validated response tuples; no PDU header is retained.
    cursor: ReadCursor<'d>,
}

impl FindByTypeValueIter<'_> {
    #[allow(clippy::should_implement_trait)]
    /// Advances one tuple and returns `Some(Ok((start, end)))`, or `None` when fewer than four bytes remain. Cursor
    /// errors are returned inside `Some`; validated construction prevents truncated tuples.
    pub fn next(&mut self) -> Option<Result<(u16, u16), crate::Error>> {
        if self.cursor.available() >= 4 {
            let res = (|| {
                let handle: u16 = self.cursor.read()?;
                let end: u16 = self.cursor.read()?;
                Ok((handle, end))
            })();
            Some(res)
        } else {
            None
        }
    }
}

#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[derive(Clone, Debug)]
/// Borrowed cursor over equal-width handle/payload tuples; group responses retain the group-end handle in the
/// payload.
pub struct ReadByTypeIter<'d> {
    /// Total bytes per tuple including the first two-byte handle; at least two, or four for group responses.
    item_len: usize,
    /// Unread validated complete tuples; clones start at the same current position.
    cursor: ReadCursor<'d>,
}

impl<'d> ReadByTypeIter<'d> {
    #[allow(clippy::should_implement_trait)]
    /// Advances one tuple and returns its handle and borrowed payload, or `None` when exhausted. Group payload begins
    /// with the end handle. Cursor errors propagate inside `Some`; decoder validation guarantees complete tuples.
    pub fn next(&mut self) -> Option<Result<(u16, &'d [u8]), crate::Error>> {
        if self.cursor.available() >= self.item_len {
            let res = (|| {
                let handle: u16 = self.cursor.read()?;
                let item = self.cursor.slice(self.item_len - 2)?;
                Ok((handle, item))
            })();
            Some(res)
        } else {
            None
        }
    }
}

#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[derive(Debug, Copy, Clone)]
/// Find Information wire format, excluding unsupported UUID32 fields.
enum FindInformationUuidFormat {
    /// Format 1: each UUID occupies two little-endian bytes.
    Uuid16 = 1,
    /// Format 2: each UUID occupies sixteen little-endian bytes.
    Uuid128 = 2,
}

impl FindInformationUuidFormat {
    /// Returns the UUID field width in bytes, excluding the two-byte handle.
    fn num_bytes(self) -> usize {
        match self {
            Self::Uuid16 => 2,
            Self::Uuid128 => 16,
        }
    }

    /// Returns the UUID format for wire byte `format`, or InvalidValue unless it is 1 or 2.
    fn from(format: u8) -> Result<Self, codec::Error> {
        match format {
            1 => Ok(Self::Uuid16),
            2 => Ok(Self::Uuid128),
            _ => Err(codec::Error::InvalidValue),
        }
    }
}

#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[derive(Clone, Debug)]
/// Borrowed cursor over validated handle/UUID entries of one uniform width.
pub struct FindInformationIter<'d> {
    /// Wire format controlling two- or sixteen-byte UUID decoding.
    format: FindInformationUuidFormat,
    /// Unread complete entries; cloning does not advance the original.
    cursor: ReadCursor<'d>,
}

impl<'d> FindInformationIter<'d> {
    #[allow(clippy::should_implement_trait)]
    /// Advances one entry and returns its handle and decoded UUID, or `None` at exhaustion. Cursor/UUID errors
    /// propagate inside `Some`; validated construction prevents incomplete entries.
    pub fn next(&mut self) -> Option<Result<(u16, Uuid), crate::Error>> {
        let uuid_len = self.format.num_bytes();

        if self.cursor.available() >= 2 + uuid_len {
            let res = (|| {
                let handle: u16 = self.cursor.read()?;
                let uuid = Uuid::try_from(self.cursor.slice(uuid_len)?)?;
                Ok((handle, uuid))
            })();
            Some(res)
        } else {
            None
        }
    }
}

impl<'d> AttServer<'d> {
    /// Returns complete selected server-PDU bytes including its opcode, excluding L2CAP framing.
    fn size(&self) -> usize {
        match self {
            Self::Response(rsp) => rsp.size(),
            Self::Unsolicited(uns) => uns.size(),
        }
    }

    /// Encodes the selected server PDU into the prefix of `dest`. Returns codec failure for insufficient space;
    /// preceding bytes may already be written.
    fn encode(&self, dest: &mut [u8]) -> Result<(), codec::Error> {
        match self {
            Self::Response(rsp) => rsp.encode(dest),
            Self::Unsolicited(uns) => uns.encode(dest),
        }
    }

    /// Decodes server `opcode` with payload cursor `r`, returning a response or unsolicited update. Returns
    /// InvalidValue/InsufficientSpace for unsupported opcode or malformed payload; values borrow the cursor input.
    fn decode_with_opcode(opcode: u8, r: ReadCursor<'d>) -> Result<Self, codec::Error> {
        let decoded = match opcode {
            ATT_HANDLE_VALUE_NTF | ATT_HANDLE_VALUE_IND => Self::Unsolicited(AttUns::decode_with_opcode(opcode, r)?),
            _ => Self::Response(AttRsp::decode_with_opcode(opcode, r)?),
        };
        Ok(decoded)
    }
}

impl<'d> AttRsp<'d> {
    /// Returns opcode/header bytes plus remaining iterator entries or value bytes. Consumed iterator entries are
    /// excluded.
    fn size(&self) -> usize {
        1 + match self {
            Self::ExchangeMtu { mtu: u16 } => 2,
            Self::FindByTypeValue { it } => it.cursor.available(),
            Self::FindInformation { it } => 1 + it.cursor.available(), // 1 for format byte
            Self::Error { .. } => 4,
            Self::Read { data } => data.len(),
            Self::ReadBlob { data } => data.len(),
            Self::ReadByType { it } => 1 + it.cursor.available(),
            Self::ReadByGroupType { it } => 1 + it.cursor.available(),
            Self::Write => 0,
        }
    }

    /// Writes opcode and response into `dest`, returning insufficient-space errors after possible prefix writes.
    /// Discovery iteration uses a clone and preserves the source position; only remaining tuples are encoded.
    fn encode(&self, dest: &mut [u8]) -> Result<(), codec::Error> {
        let mut w = WriteCursor::new(dest);
        match self {
            Self::ExchangeMtu { mtu } => {
                w.write(ATT_EXCHANGE_MTU_RSP)?;
                w.write(*mtu)?;
            }
            Self::FindByTypeValue { it } => {
                w.write(ATT_FIND_BY_TYPE_VALUE_RSP)?;
                let mut it = it.clone();
                while let Some(Ok((start, end))) = it.next() {
                    w.write(start)?;
                    w.write(end)?;
                }
            }
            Self::FindInformation { it } => {
                w.write(ATT_FIND_INFORMATION_RSP)?;
                w.write(it.format as u8)?;
                let mut it = it.clone();
                while let Some(Ok((handle, uuid))) = it.next() {
                    w.write(handle)?;
                    w.append(uuid.as_raw())?;
                }
            }
            Self::Error { request, handle, code } => {
                w.write(ATT_ERROR_RSP)?;
                w.write(*request)?;
                w.write(*handle)?;
                w.write(*code)?;
            }
            Self::ReadByType { it } => {
                w.write(ATT_READ_BY_TYPE_RSP)?;
                w.write(it.item_len as u8)?;
                let mut it = it.clone();
                while let Some(Ok((handle, item))) = it.next() {
                    w.write(handle)?;
                    w.append(item)?;
                }
            }
            Self::ReadByGroupType { it } => {
                w.write(ATT_READ_BY_GROUP_TYPE_RSP)?;
                w.write(it.item_len as u8)?;
                let mut it = it.clone();
                while let Some(Ok((handle, item))) = it.next() {
                    w.write(handle)?;
                    w.append(item)?;
                }
            }
            Self::Read { data } => {
                w.write(ATT_READ_RSP)?;
                w.append(data)?;
            }
            Self::ReadBlob { data } => {
                w.write(ATT_READ_BLOB_RSP)?;
                w.append(data)?;
            }
            Self::Write => {
                w.write(ATT_WRITE_RSP)?;
            }
        }
        Ok(())
    }

    /// Decodes response `opcode` from payload cursor `r`. Returns borrowed values or validated nonempty complete
    /// tuple lists; rejects unsupported formats/opcodes, wrong fixed lengths and truncated tuples.
    fn decode_with_opcode(opcode: u8, mut r: ReadCursor<'d>) -> Result<Self, codec::Error> {
        match opcode {
            ATT_FIND_BY_TYPE_VALUE_RSP => {
                validate_tuple_list(r.available(), 4, 4)?;
                Ok(Self::FindByTypeValue {
                    it: FindByTypeValueIter { cursor: r },
                })
            }
            ATT_FIND_INFORMATION_RSP => {
                let format = FindInformationUuidFormat::from(r.read()?)?;
                validate_tuple_list(r.available(), 2 + format.num_bytes(), 4)?;
                Ok(Self::FindInformation {
                    it: FindInformationIter { format, cursor: r },
                })
            }
            ATT_EXCHANGE_MTU_RSP => {
                if r.available() != 2 {
                    return Err(codec::Error::InvalidValue);
                }
                let mtu: u16 = r.read()?;
                Ok(Self::ExchangeMtu { mtu })
            }
            ATT_ERROR_RSP => {
                // ATT 3.4.1.1: request opcode, two-byte handle, one-byte error code.
                if r.available() != 4 {
                    return Err(codec::Error::InvalidValue);
                }
                let request = r.read()?;
                let handle = r.read()?;
                let code = r.read()?;
                Ok(Self::Error { request, handle, code })
            }
            ATT_READ_RSP => Ok(Self::Read { data: r.remaining() }),
            ATT_READ_BLOB_RSP => Ok(Self::ReadBlob { data: r.remaining() }),
            ATT_READ_BY_TYPE_RSP => {
                let item_len: u8 = r.read()?;
                validate_tuple_list(r.available(), item_len as usize, 2)?;
                Ok(Self::ReadByType {
                    it: ReadByTypeIter {
                        item_len: item_len as usize,
                        cursor: r,
                    },
                })
            }
            ATT_READ_BY_GROUP_TYPE_RSP => {
                let item_len: u8 = r.read()?;
                validate_tuple_list(r.available(), item_len as usize, 4)?;
                Ok(Self::ReadByGroupType {
                    it: ReadByTypeIter {
                        item_len: item_len as usize,
                        cursor: r,
                    },
                })
            }
            ATT_WRITE_RSP if r.available() == 0 => Ok(Self::Write),
            _ => Err(codec::Error::InvalidValue),
        }
    }
}

// Discovery responses contain at least one complete tuple; no tuple spans PDUs.
// Validate before exposing an iterator so malformed lengths cannot underflow or
// turn a truncated tail into successful enumeration.
/// Validates `available` bytes as a nonempty list of `item_len`-byte tuples, each at least `minimum` bytes.
/// Returns InvalidValue for undersized/zero tuple width, empty payload or an incomplete tail.
fn validate_tuple_list(available: usize, item_len: usize, minimum: usize) -> Result<(), codec::Error> {
    if item_len < minimum || available == 0 || !available.is_multiple_of(item_len) {
        Err(codec::Error::InvalidValue)
    } else {
        Ok(())
    }
}

impl<'d> AttUns<'d> {
    /// Returns opcode, two-byte handle and raw value byte count.
    fn size(&self) -> usize {
        1 + match self {
            Self::Notify { data, .. } => 2 + data.len(),
            Self::Indicate { data, .. } => 2 + data.len(),
        }
    }

    /// Writes notification/indication opcode, handle and value into `dest`. Returns insufficient-space errors after
    /// possible prefix mutation; no MTU/permission checks occur here.
    fn encode(&self, dest: &mut [u8]) -> Result<(), codec::Error> {
        let mut w = WriteCursor::new(dest);
        match self {
            Self::Notify { handle, data } => {
                w.write(ATT_HANDLE_VALUE_NTF)?;
                w.write(*handle)?;
                w.append(data)?;
            }
            Self::Indicate { handle, data } => {
                w.write(ATT_HANDLE_VALUE_IND)?;
                w.write(*handle)?;
                w.append(data)?;
            }
        }
        Ok(())
    }

    /// Decodes notification/indication `opcode` from payload cursor `r`, returning a borrowed value after its handle.
    /// Rejects other opcodes and payloads shorter than the two-byte handle; empty values are accepted.
    fn decode_with_opcode(opcode: u8, mut r: ReadCursor<'d>) -> Result<Self, codec::Error> {
        match opcode {
            ATT_HANDLE_VALUE_NTF => {
                let handle = r.read()?;
                Ok(Self::Notify {
                    handle,
                    data: r.remaining(),
                })
            }
            ATT_HANDLE_VALUE_IND => {
                let handle = r.read()?;
                Ok(Self::Indicate {
                    handle,
                    data: r.remaining(),
                })
            }
            _ => Err(codec::Error::InvalidValue),
        }
    }
}

impl<'d> AttClient<'d> {
    /// Returns complete request/command/confirmation bytes including opcode.
    fn size(&self) -> usize {
        match self {
            Self::Request(req) => req.size(),
            Self::Command(cmd) => cmd.size(),
            Self::Confirmation(cfm) => cfm.size(),
        }
    }

    /// Encodes the selected client PDU into `dest`, returning codec space errors after possible prefix writes;
    /// server-side semantic admission is separate.
    fn encode(&self, dest: &mut [u8]) -> Result<(), codec::Error> {
        match self {
            Self::Request(req) => req.encode(dest),
            Self::Command(cmd) => cmd.encode(dest),
            Self::Confirmation(cfm) => cfm.encode(dest),
        }
    }

    /// Decodes client `opcode` from payload cursor `r`, returning a request, write command or confirmation. Rejects
    /// unsupported opcodes and malformed payload lengths.
    fn decode_with_opcode(opcode: u8, r: ReadCursor<'d>) -> Result<Self, codec::Error> {
        let decoded = match opcode {
            ATT_WRITE_CMD => Self::Command(AttCmd::decode_with_opcode(opcode, r)?),
            ATT_HANDLE_VALUE_CFM => Self::Confirmation(AttCfm::decode_with_opcode(opcode, r)?),
            _ => Self::Request(AttReq::decode_with_opcode(opcode, r)?),
        };
        Ok(decoded)
    }
}

impl<'d> AttReq<'d> {
    /// Returns encoded request bytes including opcode; UUID32 type fields expand to sixteen bytes.
    fn size(&self) -> usize {
        1 + match self {
            Self::ExchangeMtu { .. } => 2,
            Self::FindByTypeValue {
                start_handle,
                end_handle,
                att_type,
                att_value,
            } => 6 + att_value.len(),
            Self::FindInformation {
                start_handle,
                end_handle,
            } => 4,
            Self::ReadByType {
                start,
                end,
                attribute_type,
            } => 4 + attribute_type.att_bytes().1,
            Self::ReadByGroupType { group_type, .. } => 4 + group_type.att_bytes().1,
            Self::Read { .. } => 2,
            Self::ReadBlob { .. } => 4, // handle (2 bytes) + offset (2 bytes)
            Self::Write { handle, data } => 2 + data.len(),
            Self::PrepareWrite { value, .. } => 4 + value.len(),
            Self::ExecuteWrite { .. } => 1,
            Self::ReadMultiple { handles } => handles.len(),
        }
    }
    /// Encodes request fields into `dest`, returning insufficient-space errors after possible prefix writes. UUID32
    /// expands to Bluetooth-base128; constructed handles, flags, MTU and raw handle-list shape are not semantically
    /// validated.
    fn encode(&self, dest: &mut [u8]) -> Result<(), codec::Error> {
        let mut w = WriteCursor::new(dest);
        match self {
            Self::ExchangeMtu { mtu } => {
                w.write(ATT_EXCHANGE_MTU_REQ)?;
                w.write(*mtu)?;
            }
            Self::FindByTypeValue {
                start_handle,
                end_handle,
                att_type,
                att_value,
            } => {
                w.write(ATT_FIND_BY_TYPE_VALUE_REQ)?;
                w.write(*start_handle)?;
                w.write(*end_handle)?;
                w.write(*att_type)?;
                w.append(att_value)?;
            }
            Self::FindInformation {
                start_handle,
                end_handle,
            } => {
                w.write(ATT_FIND_INFORMATION_REQ)?;
                w.write(*start_handle)?;
                w.write(*end_handle)?;
            }
            Self::ReadByType {
                start,
                end,
                attribute_type,
            } => {
                w.write(ATT_READ_BY_TYPE_REQ)?;
                w.write(*start)?;
                w.write(*end)?;
                let (bytes, len) = attribute_type.att_bytes();
                w.append(&bytes[..len])?;
            }
            Self::ReadByGroupType { start, end, group_type } => {
                w.write(ATT_READ_BY_GROUP_TYPE_REQ)?;
                w.write(*start)?;
                w.write(*end)?;
                let (bytes, len) = group_type.att_bytes();
                w.append(&bytes[..len])?;
            }
            Self::Read { handle } => {
                w.write(ATT_READ_REQ)?;
                w.write(*handle)?;
            }
            Self::ReadBlob { handle, offset } => {
                w.write(ATT_READ_BLOB_REQ)?;
                w.write(*handle)?;
                w.write(*offset)?;
            }
            Self::Write { handle, data } => {
                w.write(ATT_WRITE_REQ)?;
                w.write(*handle)?;
                w.append(data)?;
            }
            Self::PrepareWrite { handle, offset, value } => {
                w.write(ATT_PREPARE_WRITE_REQ)?;
                w.write(*handle)?;
                w.write(*offset)?;
                w.append(value)?;
            }
            Self::ExecuteWrite { flags } => {
                w.write(ATT_EXECUTE_WRITE_REQ)?;
                w.write(*flags)?;
            }
            Self::ReadMultiple { handles } => {
                w.write(ATT_READ_MULTIPLE_VARIABLE_REQ)?;
                w.append(handles)?;
            }
        }
        Ok(())
    }

    /// Decodes request `opcode` from payload cursor `r`, returning borrowed values after validating fixed prefixes
    /// and UUID/list widths. Returns InvalidValue or InsufficientSpace on malformed bytes; MTU, handle ranges,
    /// permissions and execute flags remain server concerns.
    fn decode_with_opcode(opcode: u8, r: ReadCursor<'d>) -> Result<Self, codec::Error> {
        let payload = r.remaining();
        // Admit each fixed prefix before indexing; variable values may be empty.
        let minimum = match opcode {
            ATT_READ_BY_GROUP_TYPE_REQ
            | ATT_READ_BY_TYPE_REQ
            | ATT_FIND_INFORMATION_REQ
            | ATT_PREPARE_WRITE_REQ
            | ATT_READ_BLOB_REQ => 4,
            ATT_READ_REQ | ATT_WRITE_REQ | ATT_EXCHANGE_MTU_REQ => 2,
            ATT_FIND_BY_TYPE_VALUE_REQ => 6,
            ATT_EXECUTE_WRITE_REQ => 1,
            _ => 0,
        };
        if payload.len() < minimum {
            return Err(codec::Error::InvalidValue);
        }
        let exact = match opcode {
            ATT_READ_REQ | ATT_EXCHANGE_MTU_REQ => Some(2),
            ATT_FIND_INFORMATION_REQ | ATT_READ_BLOB_REQ => Some(4),
            ATT_EXECUTE_WRITE_REQ => Some(1),
            _ => None,
        };
        if exact.is_some_and(|len| payload.len() != len) {
            return Err(codec::Error::InvalidValue);
        }
        match opcode {
            ATT_READ_BY_GROUP_TYPE_REQ => {
                let start_handle = (payload[0] as u16) + ((payload[1] as u16) << 8);
                let end_handle = (payload[2] as u16) + ((payload[3] as u16) << 8);

                let group_type = if payload.len() == 6 {
                    Uuid::Uuid16([payload[4], payload[5]])
                } else if payload.len() == 20 {
                    let uuid = payload[4..20].try_into().map_err(|_| codec::Error::InvalidValue)?;
                    Uuid::Uuid128(uuid)
                } else {
                    return Err(codec::Error::InvalidValue);
                };

                Ok(Self::ReadByGroupType {
                    start: start_handle,
                    end: end_handle,
                    group_type,
                })
            }
            ATT_READ_BY_TYPE_REQ => {
                let start_handle = (payload[0] as u16) + ((payload[1] as u16) << 8);
                let end_handle = (payload[2] as u16) + ((payload[3] as u16) << 8);

                let attribute_type = if payload.len() == 6 {
                    Uuid::Uuid16([payload[4], payload[5]])
                } else if payload.len() == 20 {
                    let uuid = payload[4..20].try_into().map_err(|_| codec::Error::InvalidValue)?;
                    Uuid::Uuid128(uuid)
                } else {
                    return Err(codec::Error::InvalidValue);
                };

                Ok(Self::ReadByType {
                    start: start_handle,
                    end: end_handle,
                    attribute_type,
                })
            }
            ATT_READ_REQ => {
                let handle = (payload[0] as u16) + ((payload[1] as u16) << 8);

                Ok(Self::Read { handle })
            }
            ATT_WRITE_REQ => {
                let handle = (payload[0] as u16) + ((payload[1] as u16) << 8);
                let data = &payload[2..];

                Ok(Self::Write { handle, data })
            }
            ATT_EXCHANGE_MTU_REQ => {
                let mtu = (payload[0] as u16) + ((payload[1] as u16) << 8);
                Ok(Self::ExchangeMtu { mtu })
            }
            ATT_FIND_BY_TYPE_VALUE_REQ => {
                let start_handle = (payload[0] as u16) + ((payload[1] as u16) << 8);
                let end_handle = (payload[2] as u16) + ((payload[3] as u16) << 8);
                let att_type = (payload[4] as u16) + ((payload[5] as u16) << 8);
                let att_value = &payload[6..];

                Ok(Self::FindByTypeValue {
                    start_handle,
                    end_handle,
                    att_type,
                    att_value,
                })
            }
            ATT_FIND_INFORMATION_REQ => {
                let start_handle = (payload[0] as u16) + ((payload[1] as u16) << 8);
                let end_handle = (payload[2] as u16) + ((payload[3] as u16) << 8);

                Ok(Self::FindInformation {
                    start_handle,
                    end_handle,
                })
            }
            ATT_PREPARE_WRITE_REQ => {
                let handle = (payload[0] as u16) + ((payload[1] as u16) << 8);
                let offset = (payload[2] as u16) + ((payload[3] as u16) << 8);
                Ok(Self::PrepareWrite {
                    handle,
                    offset,
                    value: &payload[4..],
                })
            }
            ATT_EXECUTE_WRITE_REQ => {
                let flags = payload[0];
                Ok(Self::ExecuteWrite { flags })
            }
            ATT_READ_MULTIPLE_VARIABLE_REQ if payload.len() >= 4 && payload.len().is_multiple_of(2) => {
                Ok(Self::ReadMultiple { handles: payload })
            }
            ATT_READ_BLOB_REQ => {
                let handle = (payload[0] as u16) + ((payload[1] as u16) << 8);
                let offset = (payload[2] as u16) + ((payload[3] as u16) << 8);
                Ok(Self::ReadBlob { handle, offset })
            }
            code => {
                warn!("[att] unknown opcode {:x}", code);
                Err(codec::Error::InvalidValue)
            }
        }
    }
}

impl<'d> AttCmd<'d> {
    /// Returns write-command opcode, two-byte handle and value byte count.
    fn size(&self) -> usize {
        1 + match self {
            Self::Write { handle, data } => 2 + data.len(),
        }
    }

    /// Writes the command into `dest`, returning insufficient-space errors after possible prefix writes. Empty values
    /// are accepted; no permission/MTU checks occur.
    fn encode(&self, dest: &mut [u8]) -> Result<(), codec::Error> {
        let mut w = WriteCursor::new(dest);
        match self {
            Self::Write { handle, data } => {
                w.write(ATT_WRITE_CMD)?;
                w.write(*handle)?;
                w.append(data)?;
            }
        }
        Ok(())
    }

    /// Decodes command `opcode` from payload cursor `r`, returning a write command for 0x52 with at least a handle.
    /// Rejects unsupported opcodes or short payloads.
    fn decode_with_opcode(opcode: u8, r: ReadCursor<'d>) -> Result<Self, codec::Error> {
        let payload = r.remaining();
        match opcode {
            ATT_WRITE_CMD => {
                if payload.len() < 2 {
                    return Err(codec::Error::InvalidValue);
                }
                let handle = (payload[0] as u16) + ((payload[1] as u16) << 8);
                let data = &payload[2..];

                Ok(Self::Write { handle, data })
            }
            code => {
                warn!("[att] unknown opcode {:x}", code);
                Err(codec::Error::InvalidValue)
            }
        }
    }
}

impl AttCfm {
    /// Returns one opcode byte; confirmation has no payload.
    fn size(&self) -> usize {
        1
    }

    /// Writes the confirmation opcode into `dest`, returning a codec space error for an empty slice.
    fn encode(&self, dest: &mut [u8]) -> Result<(), codec::Error> {
        let mut w = WriteCursor::new(dest);
        match self {
            Self::ConfirmIndication => {
                w.write(ATT_HANDLE_VALUE_CFM)?;
            }
        }
        Ok(())
    }

    /// Returns indication confirmation only for `opcode` 0x1e with empty payload cursor `r`; otherwise InvalidValue.
    fn decode_with_opcode(opcode: u8, r: ReadCursor<'_>) -> Result<Self, codec::Error> {
        let payload = r.remaining();
        match opcode {
            ATT_HANDLE_VALUE_CFM if payload.is_empty() => Ok(Self::ConfirmIndication),
            code => {
                warn!("[att] unknown opcode {:x}", code);
                Err(codec::Error::InvalidValue)
            }
        }
    }
}

impl<'d> Att<'d> {
    /// Returns complete ATT PDU bytes, including opcode and excluding L2CAP framing; discovery responses count only
    /// remaining iterator entries.
    pub fn size(&self) -> usize {
        match self {
            Self::Client(client) => client.size(),
            Self::Server(server) => server.size(),
        }
    }

    /// Writes this PDU into the prefix of `dest`, returning codec space errors after possible prefix mutation. Does
    /// not validate semantic handle/MTU/flag values; leaves bytes beyond size untouched.
    pub fn encode(&self, dest: &mut [u8]) -> Result<(), codec::Error> {
        match self {
            Self::Client(client) => client.encode(dest),
            Self::Server(server) => server.encode(dest),
        }
    }

    /// Decodes complete ATT `data`, returning borrowed payloads or codec errors for empty, unsupported or malformed
    /// PDUs. Fixed layouts reject trailing bytes; variable value bytes remain opaque. Opcode parity selects
    /// client/server dispatch.
    pub fn decode(data: &'d [u8]) -> Result<Att<'d>, codec::Error> {
        let mut r = ReadCursor::new(data);
        let opcode: u8 = r.read()?;
        if opcode.is_multiple_of(2) {
            let client = AttClient::decode_with_opcode(opcode, r)?;
            Ok(Att::Client(client))
        } else {
            let server = AttServer::decode_with_opcode(opcode, r)?;
            Ok(Att::Server(server))
        }
    }
}

impl From<codec::Error> for AttErrorCode {
    /// Returns INVALID_PDU for any codec failure `e`; detailed codec cause is deliberately discarded.
    fn from(e: codec::Error) -> Self {
        AttErrorCode::INVALID_PDU
    }
}

impl codec::Type for Att<'_> {
    /// Returns complete ATT wire bytes via the inherent size method, excluding L2CAP framing.
    fn size(&self) -> usize {
        Self::size(self)
    }
}

impl codec::Encode for Att<'_> {
    /// Encodes into `dest` using the inherent PDU encoder, returning its space errors and possible partial-prefix
    /// mutation.
    fn encode(&self, dest: &mut [u8]) -> Result<(), codec::Error> {
        Self::encode(self, dest)
    }
}

impl<'d> codec::Decode<'d> for Att<'d> {
    /// Returns the decoded PDU borrowing `data`, or the inherent decoder's malformed/unsupported-PDU error.
    fn decode(data: &'d [u8]) -> Result<Self, codec::Error> {
        Self::decode(data)
    }
}

#[cfg(test)]
/// Wire-shape, bounded decoding, UUID expansion and encode/decode regression coverage.
mod tuple_tests {
    extern crate std;
    use super::*;
    use std::vec;

    /// Decodes valid `wire`, checks exact re-encoding and rejects each truncated destination length.
    fn request_round_trip(wire: &[u8]) {
        let pdu = Att::decode(wire).unwrap();
        assert_eq!(pdu.size(), wire.len());
        let mut encoded = vec![0; pdu.size()];
        pdu.encode(&mut encoded).unwrap();
        assert_eq!(encoded, wire);
        for len in 0..wire.len() {
            assert!(pdu.encode(&mut encoded[..len]).is_err());
        }
    }

    /// Prepare Write preserves handle, byte offset and empty/nonempty fragments through the public codec.
    #[test]
    fn prepare_write_public_codec_round_trips() {
        request_round_trip(&[ATT_PREPARE_WRITE_REQ, 1, 0, 0xff, 0, 7, 8]);
        request_round_trip(&[ATT_PREPARE_WRITE_REQ, 1, 0, 0, 0]);
    }

    /// Execute Write preserves cancel and commit flags through the public codec.
    #[test]
    fn execute_write_public_codec_round_trips() {
        request_round_trip(&[ATT_EXECUTE_WRITE_REQ, 0]);
        request_round_trip(&[ATT_EXECUTE_WRITE_REQ, 1]);
    }

    /// Read Multiple Variable preserves complete little-endian handle pairs through the public codec.
    #[test]
    fn read_multiple_public_codec_round_trips() {
        request_round_trip(&[ATT_READ_MULTIPLE_VARIABLE_REQ, 1, 0, 0xff, 0xff]);
    }

    /// Checks exact Bluetooth-base expansion of `request`'s UUID32 and subsequent parser admission.
    fn expanded_uuid32_request(request: AttReq<'_>) {
        let pdu = Att::Client(AttClient::Request(request));
        let mut bytes = vec![0; pdu.size()];
        pdu.encode(&mut bytes).unwrap();
        assert_eq!(bytes.len(), 21);
        assert_eq!(&bytes[1..5], &[1, 0, 0xff, 0xff]);
        assert_eq!(
            &bytes[5..],
            &[0xfb, 0x34, 0x9b, 0x5f, 0x80, 0, 0, 0x80, 0, 0x10, 0, 0, 0x78, 0x56, 0x34, 0x12]
        );
        assert!(Att::decode(&bytes).is_ok());
    }

    /// Read By Type emits the 128-bit equivalent of UUID32 and remains decodable by the ATT parser.
    #[test]
    fn uuid32_read_by_type_uses_bluetooth_base_uuid() {
        expanded_uuid32_request(AttReq::ReadByType {
            start: 1,
            end: u16::MAX,
            attribute_type: Uuid::from(0x12345678u32),
        });
    }

    /// Read By Group Type emits the 128-bit equivalent of UUID32 rather than an invalid four-byte field.
    #[test]
    fn uuid32_read_by_group_type_uses_bluetooth_base_uuid() {
        expanded_uuid32_request(AttReq::ReadByGroupType {
            start: 1,
            end: u16::MAX,
            group_type: Uuid::from(0x12345678u32),
        });
    }

    #[test]
    /// Fixed-layout PDUs reject extra bytes; valid MTU/write/error responses preserve encoded size and bytes.
    fn fixed_pdus_reject_trailing_bytes_and_response_sizes_round_trip() {
        for pdu in [
            &[0x02, 23, 0][..],
            &[0x03, 23, 0],
            &[0x04, 1, 0, 0xff, 0xff],
            &[0x0a, 1, 0],
            &[0x0c, 1, 0, 0, 0],
            &[0x13],
            &[0x18, 1],
            &[0x1e],
        ] {
            assert!(Att::decode(pdu).is_ok(), "{pdu:?}");
            let mut extra = pdu.to_vec();
            extra.push(0);
            assert!(Att::decode(&extra).is_err(), "accepted trailing byte: {extra:?}");
        }
        for pdu in [
            &[0x07, 1, 0, 3, 0][..],
            &[0x05, 1, 1, 0, 0, 0x18],
            &[0x09, 4, 1, 0, 2, 0][..],
            &[0x11, 6, 1, 0, 3, 0, 0, 0x18],
        ] {
            let decoded = Att::decode(pdu).unwrap();
            let mut encoded = vec![0; decoded.size()];
            decoded.encode(&mut encoded).unwrap();
            assert_eq!(encoded.as_slice(), pdu);
        }
    }

    #[test]
    /// Exercises every opcode and bounded short payload to ensure malformed data returns an error without panicking.
    fn all_opcodes_and_bounded_payloads_decode_without_panicking() {
        /// Calls iterator callback `next` until exhaustion; panics if it yields more than `length` items,
        /// bounding malformed-response iteration by the input byte count.
        fn exhaust<T>(length: usize, mut next: impl FnMut() -> Option<T>) {
            for _ in 0..=length {
                if next().is_none() {
                    return;
                }
            }
            panic!("iterator did not terminate within input length");
        }
        for opcode in 0..=u8::MAX {
            for length in 0..=64 {
                for seed in [0u32, 1, 0x12345678, u32::MAX] {
                    let mut bytes = [0; 64];
                    let mut state = seed;
                    for byte in &mut bytes[..length] {
                        state = state.wrapping_mul(1664525).wrapping_add(1013904223);
                        *byte = (state >> 24) as u8;
                    }
                    bytes[0] = opcode;
                    let result = std::panic::catch_unwind(|| match Att::decode(&bytes[..length]) {
                        Ok(Att::Server(AttServer::Response(AttRsp::FindByTypeValue { mut it }))) => {
                            exhaust(length, || it.next());
                        }
                        Ok(Att::Server(AttServer::Response(AttRsp::FindInformation { mut it }))) => {
                            exhaust(length, || it.next());
                        }
                        Ok(Att::Server(AttServer::Response(
                            AttRsp::ReadByType { mut it } | AttRsp::ReadByGroupType { mut it },
                        ))) => {
                            exhaust(length, || it.next());
                        }
                        _ => {}
                    });
                    assert!(result.is_ok(), "opcode={opcode:#x} length={length} seed={seed:#x}");
                }
            }
        }
    }

    #[test]
    /// Request prefix truncation is rejected while complete prefixes and empty writable values remain accepted.
    fn guarded_request_prefixes_preserve_valid_requests_and_empty_values() {
        for bytes in [
            &[0x10, 1, 0, 9, 0, 3, 0x28][..],
            &[0x08, 1, 0, 9, 0, 3, 0x28][..],
            &[0x0a, 1, 0][..],
            &[0x12, 1, 0][..],
            &[0x02, 23, 0][..],
            &[0x06, 1, 0, 9, 0, 0, 0x28][..],
            &[0x04, 1, 0, 9, 0][..],
            &[0x16, 1, 0, 2, 0][..],
            &[0x0c, 1, 0, 2, 0][..],
            &[0x18, 1][..],
            &[0x52, 1, 0][..],
        ] {
            for length in 0..bytes.len() {
                assert!(
                    Att::decode(&bytes[..length]).is_err(),
                    "accepted incomplete prefix {bytes:?} at {length}"
                );
            }
            match (bytes[0], Att::decode(bytes).unwrap()) {
                (
                    0x10,
                    Att::Client(AttClient::Request(AttReq::ReadByGroupType {
                        start: 1,
                        end: 9,
                        group_type,
                    })),
                ) => assert_eq!(group_type, Uuid::new_short(0x2803)),
                (
                    0x08,
                    Att::Client(AttClient::Request(AttReq::ReadByType {
                        start: 1,
                        end: 9,
                        attribute_type,
                    })),
                ) => assert_eq!(attribute_type, Uuid::new_short(0x2803)),
                (0x0a, Att::Client(AttClient::Request(AttReq::Read { handle: 1 }))) => {}
                (0x12, Att::Client(AttClient::Request(AttReq::Write { handle: 1, data }))) => assert!(data.is_empty()),
                (0x02, Att::Client(AttClient::Request(AttReq::ExchangeMtu { mtu: 23 }))) => {}
                (
                    0x06,
                    Att::Client(AttClient::Request(AttReq::FindByTypeValue {
                        start_handle: 1,
                        end_handle: 9,
                        att_type: 0x2800,
                        att_value,
                    })),
                ) => assert!(att_value.is_empty()),
                (
                    0x04,
                    Att::Client(AttClient::Request(AttReq::FindInformation {
                        start_handle: 1,
                        end_handle: 9,
                    })),
                ) => {}
                (
                    0x16,
                    Att::Client(AttClient::Request(AttReq::PrepareWrite {
                        handle: 1,
                        offset: 2,
                        value,
                    })),
                ) => assert!(value.is_empty()),
                (0x0c, Att::Client(AttClient::Request(AttReq::ReadBlob { handle: 1, offset: 2 }))) => {}
                (0x18, Att::Client(AttClient::Request(AttReq::ExecuteWrite { flags: 1 }))) => {}
                (0x52, Att::Client(AttClient::Command(AttCmd::Write { handle: 1, data }))) => assert!(data.is_empty()),
                _ => panic!("valid request changed: {bytes:?}"),
            }
        }
        let bytes = [0x08, 1, 0, 9, 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16];
        assert!(
            matches!(Att::decode(&bytes), Ok(Att::Client(AttClient::Request(AttReq::ReadByType {
            start: 1, end: 9, attribute_type: Uuid::Uuid128(uuid),
        }))) if uuid == bytes[5..])
        );
    }

    #[test]
    /// A group request preserves all sixteen UUID bytes after decoding.
    fn read_by_group_type_request_preserves_128_bit_uuid() {
        let bytes = [0x10, 1, 0, 9, 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16];
        assert!(
            matches!(Att::decode(&bytes), Ok(Att::Client(AttClient::Request(AttReq::ReadByGroupType {
            start: 1, end: 9, group_type: Uuid::Uuid128(uuid),
        }))) if uuid == bytes[5..])
        );
    }

    #[test]
    /// Error responses require the exact four-byte payload after opcode, including complete handle and code.
    fn error_response_requires_complete_fixed_payload() {
        for bytes in [
            &[][..],
            &[0x01][..],
            &[0x01, 0x08][..],
            &[0x01, 0x08, 0][..],
            &[0x01, 0x08, 0, 0][..],
            &[0x01, 0x08, 0, 0, 0x06, 0][..],
        ] {
            assert!(Att::decode(bytes).is_err(), "accepted malformed error {bytes:?}");
        }
        for code in 0..=u8::MAX {
            let bytes = [0x01, 0x08, 0, 0, code];
            assert!(
                matches!(Att::decode(&bytes), Ok(Att::Server(AttServer::Response(AttRsp::Error {
                request: 0x08, handle: 0, code: received,
            }))) if received.to_u8() == code)
            );
        }
    }

    #[test]
    /// Read tuple lengths must include their mandatory handle fields before iterator construction.
    fn read_tuple_lengths_must_include_their_handles() {
        for (opcode, minimum) in [(0x09, 2), (0x11, 4)] {
            for length in 0..minimum {
                let bytes = [opcode, length, 1, 0, 2, 0];
                assert!(
                    matches!(Att::decode(&bytes), Err(codec::Error::InvalidValue)),
                    "{bytes:?}"
                );
            }
        }
    }

    #[test]
    /// Read tuple responses reject empty lists and incomplete final entries.
    fn read_tuple_lists_require_complete_nonempty_entries() {
        for (opcode, length) in [(0x09, 7), (0x09, 21), (0x11, 6), (0x11, 20)] {
            assert!(Att::decode(&[opcode, length]).is_err());
            for tail in 1..length as usize {
                for complete in [0, 1] {
                    let mut bytes = vec![0; 2 + complete * length as usize + tail];
                    bytes[0] = opcode;
                    bytes[1] = length;
                    assert!(
                        matches!(Att::decode(&bytes), Err(codec::Error::InvalidValue)),
                        "{bytes:?}"
                    );
                }
            }
        }
    }

    #[test]
    /// Find-by-type responses require complete nonempty start/end handle pairs.
    fn find_by_type_lists_require_complete_nonempty_handle_ranges() {
        for length in [0, 1, 2, 3, 5, 6, 7] {
            let mut bytes = vec![0; 1 + length];
            bytes[0] = 0x07;
            assert!(
                matches!(Att::decode(&bytes), Err(codec::Error::InvalidValue)),
                "{bytes:?}"
            );
        }
    }

    #[test]
    /// Find-information responses reject empty, partial and unsupported-width UUID tuples.
    fn find_information_lists_require_complete_nonempty_uuid_entries() {
        for (format, length) in [(1, 4), (2, 18)] {
            assert!(Att::decode(&[0x05, format]).is_err());
            for tail in 1..length {
                for complete in [0, 1] {
                    let mut bytes = vec![0; 2 + complete * length + tail];
                    bytes[0] = 0x05;
                    bytes[1] = format;
                    assert!(
                        matches!(Att::decode(&bytes), Err(codec::Error::InvalidValue)),
                        "{bytes:?}"
                    );
                }
            }
        }
        assert!(Att::decode(&[0x05, 3, 1, 0, 2, 0]).is_err());
    }

    #[test]
    /// Zero-byte values remain valid when each read tuple contains its handle, and iteration preserves every complete
    /// entry.
    fn read_tuples_keep_zero_length_values_and_all_complete_entries() {
        for (opcode, bytes, expected) in [
            (0x09, &[0x09, 2, 1, 0, 2, 0][..], &[(1, &[][..]), (2, &[][..])][..]),
            (0x11, &[0x11, 4, 1, 0, 2, 0][..], &[(1, &[2, 0][..])][..]),
        ] {
            let Att::Server(AttServer::Response(rsp)) = Att::decode(bytes).unwrap() else {
                panic!()
            };
            let mut it = match rsp {
                AttRsp::ReadByType { it } if opcode == 0x09 => it,
                AttRsp::ReadByGroupType { it } if opcode == 0x11 => it,
                _ => panic!(),
            };
            for &(handle, value) in expected {
                assert_eq!(it.next().unwrap().unwrap(), (handle, value));
            }
            assert!(it.next().is_none());
        }
    }

    #[test]
    /// Complete discovery tuples preserve 128-bit UUIDs, group-end handles and consecutive entries.
    fn complete_discovery_tuples_preserve_128_bit_values_and_ranges() {
        let uuid = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16];
        for (opcode, prefix) in [
            (0x09, &[0x09, 21, 2, 0, 2, 3, 0][..]),
            (0x11, &[0x11, 20, 1, 0, 5, 0][..]),
        ] {
            let mut bytes = prefix.to_vec();
            bytes.extend_from_slice(&uuid);
            let Att::Server(AttServer::Response(rsp)) = Att::decode(&bytes).unwrap() else {
                panic!()
            };
            let mut it = match rsp {
                AttRsp::ReadByType { it } if opcode == 0x09 => it,
                AttRsp::ReadByGroupType { it } if opcode == 0x11 => it,
                _ => panic!(),
            };
            assert_eq!(it.next().unwrap().unwrap().1, &bytes[4..]);
            assert!(it.next().is_none());
        }
        for format in [1, 2] {
            let value = if format == 1 { &uuid[..2] } else { &uuid[..] };
            let mut bytes = vec![0x05, format, 2, 0];
            bytes.extend_from_slice(value);
            bytes.extend_from_slice(&[3, 0]);
            bytes.extend_from_slice(value);
            let Att::Server(AttServer::Response(AttRsp::FindInformation { mut it })) = Att::decode(&bytes).unwrap()
            else {
                panic!()
            };
            assert_eq!(it.next().unwrap().unwrap(), (2, Uuid::try_from(value).unwrap()));
            assert_eq!(it.next().unwrap().unwrap(), (3, Uuid::try_from(value).unwrap()));
            assert!(it.next().is_none());
        }
        let Att::Server(AttServer::Response(AttRsp::FindByTypeValue { mut it })) =
            Att::decode(&[0x07, 1, 0, 5, 0, 6, 0, 0xff, 0xff]).unwrap()
        else {
            panic!()
        };
        assert_eq!(it.next().unwrap().unwrap(), (1, 5));
        assert_eq!(it.next().unwrap().unwrap(), (6, 0xffff));
        assert!(it.next().is_none());
    }
}
