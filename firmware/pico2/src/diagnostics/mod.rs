//! Bounded, hardware-independent classical CAN diagnostic transport.
pub mod isotp;
pub mod longan;
pub mod obd;
pub mod request;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FrameError {
    Identifier,
    Length,
}

/// Standard CAN data frame with a known DLC. No remote or extended frames.
/// Longan RX12 does not carry enough information to construct this implicitly.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CanFrame {
    id: u16,
    data: [u8; 8],
    len: u8,
}
impl CanFrame {
    pub fn new(
        id: u16,
        bytes: &[u8],
    ) -> Result<Self, FrameError> {
        if id > 0x7FF {
            return Err(FrameError::Identifier);
        }
        if bytes.len() > 8 {
            return Err(FrameError::Length);
        }
        let mut data = [0; 8];
        data[..bytes.len()].copy_from_slice(bytes);
        Ok(Self {
            id,
            data,
            len: bytes.len() as u8,
        })
    }

    pub const fn id(&self) -> u16 { self.id }

    pub fn data(&self) -> &[u8] { &self.data[..self.len as usize] }

    /// The published Longan firmware always transmits DLC8. Reject shorter
    /// frames rather than changing their semantics by silently adding padding.
    pub fn longan_tx(&self) -> Result<[u8; 14], FrameError> {
        if self.len != 8 {
            return Err(FrameError::Length);
        }
        let mut bytes = [0; 14];
        bytes[..4].copy_from_slice(&u32::from(self.id).to_be_bytes());
        bytes[6..].copy_from_slice(&self.data);
        Ok(bytes)
    }
}
