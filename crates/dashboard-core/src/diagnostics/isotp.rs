//! Normal-addressed classical CAN ISO-TP receive path. One responder and epoch
//! per bounded transaction. Unsupported CAN FD/extended-length forms fail closed.
use heapless::Vec;

use super::CanFrame;

pub const MAX_PAYLOAD: usize = 256;
// ISO 15765-2 N_Cr; a caller may separately impose a tighter total request
// deadline, but the transport does not invent a shorter protocol timeout.
pub const INTER_FRAME_MS: u64 = 1_000;
const BLOCK_SIZE: u8 = 4;
const CTS: [u8; 8] = [0x30, BLOCK_SIZE, 5, 0, 0, 0, 0, 0];
const OVERFLOW: [u8; 8] = [0x32, 0, 0, 0, 0, 0, 0, 0];
pub type Payload = Vec<u8, MAX_PAYLOAD>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IsoError {
    Busy,
    Identifier,
    Clock,
    Timeout,
    Length,
    FrameType,
    Sequence,
    FlowControl,
    Inactive,
}
#[derive(Debug, PartialEq, Eq)]
// A response owns its bounded 256-byte storage; no allocator or live receiver
// borrow crosses publication. Host layout and target RAM checks cover the cost.
#[allow(clippy::large_enum_variant)]
pub enum RxEvent {
    Ignored,
    Pending,
    FlowControl([u8; 8]),
    Overflow([u8; 8]),
    Complete(Payload),
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum Phase {
    Idle,
    First,
    AwaitFlowControl,
    Consecutive,
}

pub struct IsoTpRx {
    phase: Phase,
    id: u16,
    epoch: u64,
    last_ms: u64,
    deadline_ms: u64,
    frame_deadline_ms: u64,
    total: usize,
    next_sequence: u8,
    block_remaining: u8,
    payload: Payload,
}
impl Default for IsoTpRx {
    fn default() -> Self { Self::new() }
}
impl IsoTpRx {
    pub const fn new() -> Self {
        Self {
            phase: Phase::Idle,
            id: 0,
            epoch: 0,
            last_ms: 0,
            deadline_ms: 0,
            frame_deadline_ms: 0,
            total: 0,
            next_sequence: 1,
            block_remaining: 0,
            payload: Vec::new(),
        }
    }

    pub fn active(&self) -> bool { self.phase != Phase::Idle }

    pub fn abort(&mut self) {
        self.phase = Phase::Idle;
        self.payload.clear();
    }

    pub fn begin(
        &mut self,
        id: u16,
        epoch: u64,
        now_ms: u64,
        deadline_ms: u64,
    ) -> Result<(), IsoError> {
        if self.active() {
            return Err(IsoError::Busy);
        }
        if id > 0x7FF {
            return Err(IsoError::Identifier);
        }
        if deadline_ms < now_ms {
            return Err(IsoError::Clock);
        }
        self.id = id;
        self.epoch = epoch;
        self.last_ms = now_ms;
        self.deadline_ms = deadline_ms;
        self.frame_deadline_ms = deadline_ms;
        self.payload.clear();
        self.total = 0;
        self.next_sequence = 1;
        self.block_remaining = 0;
        self.phase = Phase::First;
        Ok(())
    }

    pub fn poll_timeout(
        &mut self,
        now_ms: u64,
    ) -> Result<(), IsoError> {
        if !self.active() {
            return Ok(());
        }
        let error = if now_ms < self.last_ms {
            Some(IsoError::Clock)
        } else if now_ms > self.deadline_ms || now_ms > self.frame_deadline_ms {
            Some(IsoError::Timeout)
        } else {
            None
        };
        if let Some(error) = error {
            self.abort();
            return Err(error);
        }
        self.last_ms = now_ms;
        Ok(())
    }

    /// Called only after the FC bytes have left UART TX, not merely been queued.
    /// This is UART completion; Longan provides no CAN transmission ACK.
    pub fn flow_control_sent(
        &mut self,
        now_ms: u64,
    ) -> Result<(), IsoError> {
        self.poll_timeout(now_ms)?;
        if self.phase != Phase::AwaitFlowControl {
            self.abort();
            return Err(IsoError::FlowControl);
        }
        self.phase = Phase::Consecutive;
        self.block_remaining = BLOCK_SIZE;
        self.frame_deadline_ms = now_ms.saturating_add(INTER_FRAME_MS).min(self.deadline_ms);
        Ok(())
    }

    pub fn on_frame(
        &mut self,
        frame: &CanFrame,
        epoch: u64,
        now_ms: u64,
    ) -> Result<RxEvent, IsoError> {
        self.poll_timeout(now_ms)?;
        if !self.active() || frame.id() != self.id || epoch != self.epoch {
            return Ok(RxEvent::Ignored);
        }
        let result = self.receive(frame.data(), now_ms);
        if result.is_err() {
            self.abort();
        }
        result
    }

    fn receive(
        &mut self,
        bytes: &[u8],
        now_ms: u64,
    ) -> Result<RxEvent, IsoError> {
        let Some(&pci) = bytes.first() else {
            return Err(IsoError::Length);
        };
        match pci >> 4 {
            0 => {
                if self.phase != Phase::First {
                    return Err(IsoError::FrameType);
                }
                let len = usize::from(pci & 15);
                if !(1..=7).contains(&len) || bytes.len() < len + 1 {
                    return Err(IsoError::Length);
                }
                self.payload
                    .extend_from_slice(&bytes[1..=len])
                    .map_err(|_| IsoError::Length)?;
                self.complete()
            }
            1 => {
                if self.phase != Phase::First {
                    return Err(IsoError::FrameType);
                }
                if bytes.len() != 8 {
                    return Err(IsoError::Length);
                }
                self.total = (usize::from(pci & 15) << 8) | usize::from(bytes[1]);
                if self.total <= 7 {
                    return Err(IsoError::Length);
                }
                if self.total > MAX_PAYLOAD {
                    self.abort();
                    return Ok(RxEvent::Overflow(OVERFLOW));
                }
                self.payload
                    .extend_from_slice(&bytes[2..])
                    .map_err(|_| IsoError::Length)?;
                self.phase = Phase::AwaitFlowControl;
                self.frame_deadline_ms = now_ms.saturating_add(INTER_FRAME_MS).min(self.deadline_ms);
                Ok(RxEvent::FlowControl(CTS))
            }
            2 => {
                if self.phase == Phase::AwaitFlowControl {
                    return Err(IsoError::FlowControl);
                }
                if self.phase != Phase::Consecutive {
                    return Err(IsoError::FrameType);
                }
                if pci & 15 != self.next_sequence {
                    return Err(IsoError::Sequence);
                }
                let len = (self.total - self.payload.len()).min(7);
                if bytes.len() < len + 1 {
                    return Err(IsoError::Length);
                }
                self.payload
                    .extend_from_slice(&bytes[1..=len])
                    .map_err(|_| IsoError::Length)?;
                self.next_sequence = (self.next_sequence + 1) & 15;
                self.block_remaining -= 1;
                self.frame_deadline_ms = now_ms.saturating_add(INTER_FRAME_MS).min(self.deadline_ms);
                if self.payload.len() == self.total {
                    self.complete()
                } else if self.block_remaining == 0 {
                    self.phase = Phase::AwaitFlowControl;
                    Ok(RxEvent::FlowControl(CTS))
                } else {
                    Ok(RxEvent::Pending)
                }
            }
            _ => Err(IsoError::FrameType),
        }
    }

    fn complete(&mut self) -> Result<RxEvent, IsoError> {
        self.phase = Phase::Idle;
        Ok(RxEvent::Complete(core::mem::take(&mut self.payload)))
    }
}
