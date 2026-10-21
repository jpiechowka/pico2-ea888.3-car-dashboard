//! Bounded, hardware-independent classical CAN diagnostic transport.
/// Exact incremental Longan mode handshake and command identity fencing.
pub mod bootstrap;
/// One outstanding read transaction with TX and response deadline ownership.
pub mod client;
/// Bounded normal-addressed classical CAN response reassembly.
pub mod isotp;
/// Non-self-framing RX12 records and explicitly qualified stream epochs.
pub mod longan;
/// Generic OBD capabilities and exact-width admitted quantity decoders.
pub mod obd;
/// Read-only request construction and service/identifier response matching.
pub mod request;
/// Partial UART write ownership through software flush and hardware-idle observation.
pub mod serial_tx;
/// Bounded bridge/session/acquisition service with explicit deployment qualification.
pub mod service;
/// Capability-scoped polling, source admission and coherent acquisition publication.
pub mod session;

/// Invalid classical standard-data frame representation or bridge serialization.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FrameError {
    /// Identifier exceeds the 11-bit standard CAN range.
    Identifier,
    /// Payload exceeds eight bytes or is not exactly DLC8 for Longan TX14.
    Length,
}

/// Standard CAN data frame with a known DLC. No remote or extended frames.
/// Longan RX12 does not carry enough information to construct this implicitly.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CanFrame {
    /// Standard 11-bit CAN identifier, at most 0x7FF.
    id: u16,
    /// Initialized payload storage; only the first len bytes belong to the frame.
    data: [u8; 8],
    /// Known data length code interpreted as byte length, zero through eight.
    len: u8,
}
impl CanFrame {
    /// Returns a standard data frame for 11-bit `id` and zero-to-eight `bytes`,
    /// copying the payload and zeroing unused storage. Returns identifier/length
    /// error for invalid input; does not infer remote/extended flags or send traffic.
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

    /// Returns the validated standard CAN identifier.
    pub const fn id(&self) -> u16 { self.id }

    /// Returns exactly the known DLC payload, excluding initialized unused tail bytes.
    pub fn data(&self) -> &[u8] { &self.data[..self.len as usize] }

    /// The published Longan firmware always transmits DLC8. Reject shorter
    /// frames rather than changing their semantics by silently adding padding.
    /// Returns TX14 bytes: big-endian 32-bit ID, zero EXT/RTR flags, eight data
    /// bytes; returns Length for DLC !=8. See
    /// [Longan protocol](https://docs.longan-labs.cc/1030001/#working-mode).
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
