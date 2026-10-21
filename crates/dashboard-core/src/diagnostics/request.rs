//! Typed read-service requests and response echo checking before payload decoding.
use super::{CanFrame, FrameError};

/// Only non-mutating diagnostic services. No arbitrary service byte, session
/// change, security access, actuator control or ECU write constructor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Request {
    /// OBD current-data service 01; available constructor does not qualify a PID.
    Obd(
        /// Requested one-byte PID.
        u8,
    ),
    /// UDS ReadDataByIdentifier service 22; no manufacturer DID enabled implicitly.
    ReadDid(
        /// Requested 16-bit DID, serialized most significant byte first.
        u16,
    ),
}
/// Response framing mismatch before any service-specific data-width validation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReplyError {
    /// Missing positive prefix or negative reply not exactly three bytes.
    Length,
    /// Positive/negative service echo does not match the request.
    Service,
    /// Positive PID/DID echo differs from the requested identifier.
    Identifier,
}
/// Matched response kind; only a separate exact-width decoder can produce a sample.
#[derive(Debug, PartialEq, Eq)]
pub enum Reply<'a> {
    /// Matched positive service and PID/DID prefix.
    Positive(
        /// Borrowed remaining payload, possibly empty and still unvalidated.
        &'a [u8],
    ),
    /// Matched negative response other than response-pending.
    Negative(
        /// Original negative-response code without reinterpretation.
        u8,
    ),
    /// NRC 0x78; the owning client must bound retries and the absolute deadline.
    Pending,
}
impl Request {
    /// Returns service byte 0x01 for OBD or 0x22 for ReadDataByIdentifier.
    pub const fn service(self) -> u8 {
        match self {
            Self::Obd(_) => 1,
            Self::ReadDid(_) => 0x22,
        }
    }

    /// Returns an eight-byte zero-padded ISO-TP single frame addressed to standard
    /// CAN `id`, or identifier error. Does not send the request or qualify its route.
    pub fn frame(
        self,
        id: u16,
    ) -> Result<CanFrame, FrameError> {
        let data = match self {
            Self::Obd(pid) => [2, 1, pid, 0, 0, 0, 0, 0],
            Self::ReadDid(did) => {
                let [hi, lo] = did.to_be_bytes();
                [3, 0x22, hi, lo, 0, 0, 0, 0]
            }
        };
        CanFrame::new(id, &data)
    }

    /// Matches reassembled service payload `bytes` to this request and returns
    /// positive borrowed data, a negative code or Pending. Returns prefix/echo
    /// errors; positive data width remains the decoder's responsibility. Negative
    /// replies must be exactly three bytes; arbitrary positive trailing data is retained.
    pub fn reply(
        self,
        bytes: &[u8],
    ) -> Result<Reply<'_>, ReplyError> {
        let Some(&service) = bytes.first() else {
            return Err(ReplyError::Length);
        };
        if service == 0x7F {
            if bytes.len() != 3 {
                return Err(ReplyError::Length);
            }
            if bytes[1] != self.service() {
                return Err(ReplyError::Service);
            }
            return Ok(if bytes[2] == 0x78 {
                Reply::Pending
            } else {
                Reply::Negative(bytes[2])
            });
        }
        if service != self.service() + 0x40 {
            return Err(ReplyError::Service);
        }
        let (prefix, len) = match self {
            Self::Obd(pid) => ([pid, 0], 1),
            Self::ReadDid(did) => (did.to_be_bytes(), 2),
        };
        if bytes.len() < 1 + len {
            return Err(ReplyError::Length);
        }
        if bytes[1..=len] != prefix[..len] {
            return Err(ReplyError::Identifier);
        }
        // Service-specific decoders, or the caller's exact DID schema, must
        // validate this data width. Matching an echo alone is never a sample.
        Ok(Reply::Positive(&bytes[1 + len..]))
    }
}
