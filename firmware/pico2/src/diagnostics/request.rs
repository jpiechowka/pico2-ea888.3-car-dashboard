use super::{CanFrame, FrameError};

/// Only non-mutating diagnostic services. No arbitrary service byte, session
/// change, security access, actuator control or ECU write constructor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Request {
    Obd(u8),
    ReadDid(u16),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReplyError {
    Length,
    Service,
    Identifier,
}
#[derive(Debug, PartialEq, Eq)]
pub enum Reply<'a> {
    Positive(&'a [u8]),
    Negative(u8),
    Pending,
}
impl Request {
    pub const fn service(self) -> u8 {
        match self {
            Self::Obd(_) => 1,
            Self::ReadDid(_) => 0x22,
        }
    }

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
