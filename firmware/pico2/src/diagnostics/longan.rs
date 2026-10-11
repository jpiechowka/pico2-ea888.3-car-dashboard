//! RX12 records are not self-framing. Only a qualified configuration transition
//! can establish the initial phase; loss/errors invalidate the entire epoch.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BridgeError {
    Unsynchronized,
    AlreadySynchronized,
    Identifier,
    EpochExhausted,
}

/// Unqualified metadata: DLC, frame kind and integrity are absent on this wire.
/// Deliberately has no From/Into<CanFrame> conversion.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RawRecord {
    id: u16,
    data: [u8; 8],
    epoch: u64,
}
impl RawRecord {
    pub const fn id(&self) -> u16 { self.id }

    pub const fn data(&self) -> &[u8; 8] { &self.data }

    pub const fn epoch(&self) -> u64 { self.epoch }
}

pub struct Rx12 {
    bytes: [u8; 12],
    len: usize,
    synchronized: bool,
    epoch: u64,
    exhausted: bool,
}
impl Default for Rx12 {
    fn default() -> Self { Self::new() }
}
impl Rx12 {
    pub const fn new() -> Self {
        Self {
            bytes: [0; 12],
            len: 0,
            synchronized: false,
            epoch: 0,
            exhausted: false,
        }
    }

    pub const fn epoch(&self) -> u64 { self.epoch }

    /// Whether all bytes already consumed belong to complete qualified records.
    /// This says nothing about bytes still queued in the port or adapter.
    pub const fn at_boundary(&self) -> bool { self.synchronized && self.len == 0 }

    /// Caller must have consumed the complete, qualified ENTER DATA MODE
    /// transition. Silence, plausible identifiers and fixed byte counts are not
    /// evidence of this boundary. Retain following bytes from the same RX chunk.
    pub fn confirm_boundary(&mut self) -> Result<u64, BridgeError> {
        if self.exhausted {
            return Err(BridgeError::EpochExhausted);
        }
        if self.synchronized {
            return Err(BridgeError::AlreadySynchronized);
        }
        self.advance_epoch()?;
        self.len = 0;
        self.synchronized = true;
        Ok(self.epoch)
    }

    fn advance_epoch(&mut self) -> Result<(), BridgeError> {
        let Some(next) = self.epoch.checked_add(1) else {
            self.exhausted = true;
            return Err(BridgeError::EpochExhausted);
        };
        self.epoch = next;
        Ok(())
    }

    pub fn fault(&mut self) {
        self.synchronized = false;
        self.len = 0;
        let _ = self.advance_epoch(); // exhausted remains permanently closed
    }

    pub fn push(
        &mut self,
        byte: u8,
    ) -> Result<Option<RawRecord>, BridgeError> {
        if self.exhausted {
            return Err(BridgeError::EpochExhausted);
        }
        if !self.synchronized {
            return Err(BridgeError::Unsynchronized);
        }
        self.bytes[self.len] = byte;
        self.len += 1;
        if self.len != self.bytes.len() {
            return Ok(None);
        }
        self.len = 0;
        let id = u32::from_be_bytes(self.bytes[..4].try_into().unwrap());
        if id > 0x7FF {
            self.fault();
            return Err(BridgeError::Identifier);
        }
        Ok(Some(RawRecord {
            id: id as u16,
            data: self.bytes[4..].try_into().unwrap(),
            epoch: self.epoch,
        }))
    }
}
