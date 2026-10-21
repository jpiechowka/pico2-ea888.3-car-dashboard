//! RX12 records are not self-framing. Only a qualified configuration transition
//! can establish the initial phase; loss/errors invalidate the entire epoch.
//! [Manufacturer wire protocol](https://docs.longan-labs.cc/1030001/#working-mode).
/// Stream-phase/epoch admission failure; no heuristic resynchronization occurs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BridgeError {
    /// No fully qualified mode-transition boundary has been established.
    Unsynchronized,
    /// Boundary confirmation attempted while a stream is already synchronized.
    AlreadySynchronized,
    /// Big-endian record identifier exceeds standard 11-bit CAN; stream invalidated.
    Identifier,
    /// Epoch counter exhausted; the parser remains permanently closed.
    EpochExhausted,
}

/// Unqualified metadata: DLC, frame kind and integrity are absent on this wire.
/// Deliberately has no `From`/`Into<CanFrame>` conversion.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RawRecord {
    /// Standard-range ID parsed from the wire's 32-bit field; flags still unknown.
    id: u16,
    /// Eight received bytes; absent DLC means tail bytes may not be CAN payload.
    data: [u8; 8],
    /// Parser epoch at record assembly, for rejecting observations after known loss.
    epoch: u64,
}
impl RawRecord {
    /// Returns parsed standard-range identifier without claiming frame-kind metadata.
    pub const fn id(&self) -> u16 { self.id }

    /// Returns all eight raw bytes; true DLC and any short-frame tail remain unknown.
    pub const fn data(&self) -> &[u8; 8] { &self.data }

    /// Returns the qualified parser epoch in which the complete record was assembled.
    pub const fn epoch(&self) -> u64 { self.epoch }
}

/// Incremental fixed-record assembler gated by externally qualified mode transitions.
pub struct Rx12 {
    /// Partially filled ID/data record; bytes beyond len are not current evidence.
    bytes: [u8; 12],
    /// Current record prefix length, zero through eleven between pushes.
    len: usize,
    /// Whether the caller established a qualified stream boundary since last fault.
    synchronized: bool,
    /// Checked generation advanced by confirmations and faults, initially zero.
    epoch: u64,
    /// Sticky flag preventing recovery once epoch addition would overflow.
    exhausted: bool,
}
impl Default for Rx12 {
    /// Returns an unsynchronized empty parser at epoch zero.
    fn default() -> Self { Self::new() }
}
impl Rx12 {
    /// Returns an unsynchronized parser; no record is admitted before confirmation.
    pub const fn new() -> Self {
        Self {
            bytes: [0; 12],
            len: 0,
            synchronized: false,
            epoch: 0,
            exhausted: false,
        }
    }

    /// Returns current epoch, including invalidated epochs with no qualified stream.
    pub const fn epoch(&self) -> u64 { self.epoch }

    /// Whether all bytes already consumed belong to complete qualified records.
    /// This says nothing about bytes still queued in the port or adapter.
    /// Returns true only when synchronized and no partial record is retained.
    pub const fn at_boundary(&self) -> bool { self.synchronized && self.len == 0 }

    /// Caller must have consumed the complete, qualified ENTER DATA MODE
    /// transition. Silence, plausible identifiers and fixed byte counts are not
    /// evidence of this boundary. Retain following bytes from the same RX chunk.
    /// Returns a new non-reused epoch and opens assembly, or already-synchronized/
    /// exhaustion error. It cannot verify the caller's external evidence.
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

    /// Increments epoch and returns success, or latches EpochExhausted without wrapping.
    fn advance_epoch(&mut self) -> Result<(), BridgeError> {
        let Some(next) = self.epoch.checked_add(1) else {
            self.exhausted = true;
            return Err(BridgeError::EpochExhausted);
        };
        self.epoch = next;
        Ok(())
    }

    /// Discards a partial record, closes assembly and advances epoch; overflow
    /// permanently closes the parser. Known byte loss must call this immediately.
    pub fn fault(&mut self) {
        self.synchronized = false;
        self.len = 0;
        let _ = self.advance_epoch(); // exhausted remains permanently closed
    }

    /// Consumes one wire `byte`; returns `None` for a partial record, `Some` after
    /// twelve bytes, or phase/epoch/identifier error. Invalid IDs close the stream;
    /// plausible IDs cannot detect undetected loss/insertion or establish integrity.
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
