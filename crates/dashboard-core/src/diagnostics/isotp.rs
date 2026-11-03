//! Normal-addressed classical CAN ISO-TP receive path. One responder and epoch
//! per bounded transaction. Unsupported CAN FD/extended-length forms fail closed.
//! Framing terminology follows the [Linux ISO-TP documentation](https://docs.kernel.org/networking/iso15765-2.html).
use heapless::Vec;

use super::CanFrame;

/// Maximum reassembled application bytes; larger first frames produce overflow FC.
pub const MAX_PAYLOAD: usize = 256;
/// Consecutive-frame/FC-completion allowance in milliseconds, capped by the total deadline.
pub const INTER_FRAME_MS: u64 = 1_000;
/// Consecutive frames permitted after each completed CTS transmission.
const BLOCK_SIZE: u8 = 4;
/// Padded CTS: four-frame blocks and five-millisecond minimum separation.
const CTS: [u8; 8] = [0x30, BLOCK_SIZE, 5, 0, 0, 0, 0, 0];
/// Padded overflow FC for a first frame exceeding local storage.
const OVERFLOW: [u8; 8] = [0x32, 0, 0, 0, 0, 0, 0, 0];
/// Owned, allocation-free reassembled application bytes, excluding ISO-TP PCI/padding.
pub type Payload = Vec<u8, MAX_PAYLOAD>;

/// Transaction admission or reassembly failure; receive failures abort buffered state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IsoError {
    /// A transaction is already active; begin leaves it unchanged.
    Busy,
    /// Responder identifier exceeds the eleven-bit CAN range.
    Identifier,
    /// Deadline precedes start, or an active clock moved backwards.
    Clock,
    /// Time passed the total or current inter-frame deadline.
    Timeout,
    /// Missing PCI, unsupported length form, or insufficient declared CAN data.
    Length,
    /// PCI type is unsupported or invalid in the current phase.
    FrameType,
    /// Consecutive-frame four-bit sequence does not match the expected value.
    Sequence,
    /// A consecutive frame arrived before FC completion, or completion was unexpected.
    FlowControl,
    /// Reserved inactive-transaction error; current entry points ignore inactive frames.
    Inactive,
}

/// Receive outcome; FC bytes must be transmitted and acknowledged through `flow_control_sent`.
#[derive(Debug, PartialEq, Eq)]
// A response owns its bounded 256-byte storage; no allocator or live receiver
// borrow crosses publication. Host layout and target RAM checks cover the cost.
#[allow(clippy::large_enum_variant)]
pub enum RxEvent {
    /// Receiver idle, different responder, or different epoch; no payload accepted.
    Ignored,
    /// Accepted consecutive data; more frames are required within the current block.
    Pending,
    /// Receiver pauses until the caller confirms local transport completion of this CTS.
    FlowControl(
        /// Eight CAN data bytes, including PCI and padding.
        [u8; 8],
    ),
    /// Receiver is aborted; caller may transmit this overflow indication.
    Overflow(
        /// Eight CAN data bytes for overflow FC.
        [u8; 8],
    ),
    /// Receiver is idle and ownership of the complete response moves to the caller.
    Complete(
        /// Reassembled application bytes without padding.
        Payload,
    ),
}

/// Receive outcome borrowing completed bytes from the receiver instead of moving its256-byte buffer.
/// A live completion borrow prevents resetting/reusing that receiver; other events carry no payload.
#[derive(Debug, PartialEq, Eq)]
pub enum RxEventRef<'a> {
    /// Receiver idle, different responder or different epoch; no completed bytes are exposed.
    Ignored,
    /// Accepted data requires more frames within the current block.
    Pending,
    /// Receiver awaits caller-confirmed local CTS completion before accepting consecutive frames.
    FlowControl(
        /// Eight CAN data bytes, including PCI and padding.
        [u8; 8],
    ),
    /// Receiver aborted the oversized response; the caller decides whether to transmit overflow FC.
    Overflow(
        /// Eight CAN data bytes for overflow FC.
        [u8; 8],
    ),
    /// Receiver is idle; completed bytes remain borrowed until the caller releases this event.
    Complete(
        /// One through256 reassembled application bytes, excluding PCI and padding.
        &'a [u8],
    ),
}

/// Small parser result shared by owned and borrowed entry points; never contains a payload copy.
enum Progress {
    /// No admitted response data.
    Ignored,
    /// More response data is needed.
    Pending,
    /// The fixed CTS is required before more consecutive data.
    FlowControl,
    /// The oversized first frame aborted reassembly.
    Overflow,
    /// The receiver is idle with one complete payload retained internally.
    Complete,
}
/// Single-response reassembly phase; FC completion explicitly gates each block.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Phase {
    /// No transaction; incoming frames are ignored.
    Idle,
    /// Waiting for a single or first frame.
    First,
    /// CTS requested but not yet confirmed transmitted.
    AwaitFlowControl,
    /// Receiving an authorized block of consecutive frames.
    Consecutive,
}

/// One bounded classical-CAN response, with caller-supplied responder/epoch and monotonic time.
/// No I/O occurs here. Abandoning the receiver does not cancel outstanding adapter traffic.
pub struct IsoTpRx {
    /// Current reassembly and FC authorization state.
    phase: Phase,
    /// Expected eleven-bit responder identifier.
    id: u16,
    /// Opaque caller generation; compared exactly, with no reserved value.
    epoch: u64,
    /// Last accepted timeout-check timestamp in milliseconds.
    last_ms: u64,
    /// Inclusive absolute transaction deadline in milliseconds.
    deadline_ms: u64,
    /// Inclusive deadline for initial response, FC completion or next consecutive frame.
    frame_deadline_ms: u64,
    /// First-frame application length, at most MAX_PAYLOAD after admission.
    total: usize,
    /// Expected consecutive-frame sequence, wrapping modulo sixteen from one.
    next_sequence: u8,
    /// Frames still permitted by the last completed CTS, zero while awaiting CTS.
    block_remaining: u8,
    /// Accepted prefix, discarded on abort/begin; completion is borrowed or moved by the chosen API.
    payload: Payload,
}
impl Default for IsoTpRx {
    /// Returns an idle receiver with empty storage, as `new`.
    fn default() -> Self { Self::new() }
}
impl IsoTpRx {
    /// Returns an idle receiver; no responder or clock is admitted until `begin`.
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

    /// Returns whether a transaction is waiting for response data or FC completion.
    pub fn active(&self) -> bool { self.phase != Phase::Idle }

    /// Discards buffered payload and enters idle; does not transmit cancellation traffic.
    pub fn abort(&mut self) {
        self.phase = Phase::Idle;
        self.payload.clear();
    }

    /// Admits eleven-bit `id` and opaque `epoch` at monotonic `now_ms`, with inclusive
    /// absolute `deadline_ms` (milliseconds). Returns success after resetting reassembly,
    /// or Busy/Identifier/Clock without changing state. Equal start/deadline is allowed.
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

    /// Checks active deadlines against monotonic `now_ms` in milliseconds. Returns success
    /// while time is within both inclusive limits (or idle); backwards/late time aborts
    /// and returns Clock/Timeout. Successful active checks update the clock even for foreign frames.
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

    /// Called only after the caller's transport confirms complete local FC submission,
    /// not merely application queueing. UART callers must wait until bytes leave TX;
    /// other transports must use their corresponding local completion boundary.
    /// Local completion alone does not prove CAN transmission or peer processing.
    /// `now_ms` is monotonic completion time in milliseconds. Returns success after
    /// opening a four-frame block and renewing its deadline, capped by the total deadline;
    /// timeout, reversal or an unexpected phase aborts and returns the corresponding error.
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

    /// Processes known-DLC `frame` from caller generation `epoch` at monotonic `now_ms`
    /// in milliseconds. Returns an event or aborting parse/deadline error. Foreign/idle
    /// frames are ignored only after timeout checking; they never extend a deadline.
    /// Accepted PCI lengths determine payload bytes; extra CAN padding is ignored.
    pub fn on_frame(
        &mut self,
        frame: &CanFrame,
        epoch: u64,
        now_ms: u64,
    ) -> Result<RxEvent, IsoError> {
        Ok(match self.advance(frame, epoch, now_ms)? {
            Progress::Ignored => RxEvent::Ignored,
            Progress::Pending => RxEvent::Pending,
            Progress::FlowControl => RxEvent::FlowControl(CTS),
            Progress::Overflow => RxEvent::Overflow(OVERFLOW),
            Progress::Complete => RxEvent::Complete(core::mem::take(&mut self.payload)),
        })
    }

    /// Processes `frame` for exact caller `epoch` at monotonic `now_ms` milliseconds using the
    /// same parser, inclusive deadlines and failure effects as [`Self::on_frame`]. Returns a
    /// small event borrowing complete bytes, or an aborting parse/deadline error. Ignored frames
    /// never expose a prior completion. Begin/abort clear retained bytes before subsequent use.
    /// The receiver cannot be mutated while a returned completion is still used:
    ///
    /// ```compile_fail
    /// use dashboard_core::diagnostics::{CanFrame, isotp::{IsoTpRx, RxEventRef}};
    /// let mut receiver = IsoTpRx::new();
    /// receiver.begin(0x7E8, 1, 0, 100).unwrap();
    /// let frame = CanFrame::new(0x7E8, &[1, 42]).unwrap();
    /// let RxEventRef::Complete(bytes) = receiver.on_frame_ref(&frame, 1, 0).unwrap() else { panic!() };
    /// receiver.abort();
    /// assert_eq!(bytes, &[42]);
    /// ```
    pub fn on_frame_ref(
        &mut self,
        frame: &CanFrame,
        epoch: u64,
        now_ms: u64,
    ) -> Result<RxEventRef<'_>, IsoError> {
        Ok(match self.advance(frame, epoch, now_ms)? {
            Progress::Ignored => RxEventRef::Ignored,
            Progress::Pending => RxEventRef::Pending,
            Progress::FlowControl => RxEventRef::FlowControl(CTS),
            Progress::Overflow => RxEventRef::Overflow(OVERFLOW),
            Progress::Complete => RxEventRef::Complete(&self.payload),
        })
    }

    /// Admits `frame` against `epoch`/monotonic `now_ms`, returning shared parser progress.
    /// Timeout checks precede foreign/idle rejection; parse failures abort retained payload.
    fn advance(
        &mut self,
        frame: &CanFrame,
        epoch: u64,
        now_ms: u64,
    ) -> Result<Progress, IsoError> {
        self.poll_timeout(now_ms)?;
        if !self.active() || frame.id() != self.id || epoch != self.epoch {
            return Ok(Progress::Ignored);
        }
        let result = self.receive(frame.data(), now_ms);
        if result.is_err() {
            self.abort();
        }
        result
    }

    /// Decodes admitted CAN `bytes` at checked `now_ms` in milliseconds. Returns the next
    /// reassembly/FC/completion event or a structural error for the caller to abort.
    /// Oversize first frames abort here and return an overflow event, not an error.
    fn receive(
        &mut self,
        bytes: &[u8],
        now_ms: u64,
    ) -> Result<Progress, IsoError> {
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
                    return Ok(Progress::Overflow);
                }
                self.payload
                    .extend_from_slice(&bytes[2..])
                    .map_err(|_| IsoError::Length)?;
                self.phase = Phase::AwaitFlowControl;
                self.frame_deadline_ms = now_ms.saturating_add(INTER_FRAME_MS).min(self.deadline_ms);
                Ok(Progress::FlowControl)
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
                    Ok(Progress::FlowControl)
                } else {
                    Ok(Progress::Pending)
                }
            }
            _ => Err(IsoError::FrameType),
        }
    }

    /// Enters idle and returns Complete with payload retained for the selected public API.
    /// This helper never returns Err and never moves the payload array.
    fn complete(&mut self) -> Result<Progress, IsoError> {
        self.phase = Phase::Idle;
        Ok(Progress::Complete)
    }
}

#[cfg(test)]
/// Borrowed/owned parser parity and retained-buffer lifetime regressions, using synthetic CAN only.
mod tests {
    use super::*;

    /// Returns a synthetic fixed-responder frame for `bytes`; invalid test DLC panics.
    fn frame(bytes: &[u8]) -> CanFrame { CanFrame::new(0x7E8, bytes).unwrap() }

    /// Converts borrowed `event` to an owned test observation for exact legacy API comparison.
    /// Returns identical tags/bytes; no production collector uses this copying helper.
    fn owned(event: RxEventRef<'_>) -> RxEvent {
        match event {
            RxEventRef::Ignored => RxEvent::Ignored,
            RxEventRef::Pending => RxEvent::Pending,
            RxEventRef::FlowControl(bytes) => RxEvent::FlowControl(bytes),
            RxEventRef::Overflow(bytes) => RxEvent::Overflow(bytes),
            RxEventRef::Complete(bytes) => RxEvent::Complete(Payload::from_slice(bytes).unwrap()),
        }
    }

    /// Every legal application length keeps exact owned events, FC blocks and full256-byte support.
    #[test]
    fn borrowed_and_owned_paths_match_all_payload_lengths() {
        for length in 1..=MAX_PAYLOAD {
            let expected: Payload = (0..length).map(|n| n as u8).collect();
            let mut borrowed = IsoTpRx::new();
            let mut legacy = IsoTpRx::new();
            borrowed.begin(0x7E8, 1, 0, 5000).unwrap();
            legacy.begin(0x7E8, 1, 0, 5000).unwrap();
            let mut first = [0; 8];
            let mut consumed = if length <= 7 {
                first[0] = length as u8;
                first[1..=length].copy_from_slice(&expected);
                length
            } else {
                first[..2].copy_from_slice(&[0x10 | (length >> 8) as u8, length as u8]);
                first[2..].copy_from_slice(&expected[..6]);
                6
            };
            let first = frame(&first);
            let mut event = owned(borrowed.on_frame_ref(&first, 1, 0).unwrap());
            assert_eq!(event, legacy.on_frame(&first, 1, 0).unwrap());
            let mut sequence = 1u8;
            while consumed < length {
                if matches!(event, RxEvent::FlowControl(_)) {
                    borrowed.flow_control_sent(0).unwrap();
                    legacy.flow_control_sent(0).unwrap();
                }
                let count = (length - consumed).min(7);
                let mut next = [0; 8];
                next[0] = 0x20 | sequence;
                next[1..=count].copy_from_slice(&expected[consumed..consumed + count]);
                let next = frame(&next);
                event = owned(borrowed.on_frame_ref(&next, 1, 0).unwrap());
                assert_eq!(event, legacy.on_frame(&next, 1, 0).unwrap());
                consumed += count;
                sequence = (sequence + 1) & 15;
            }
            assert_eq!(event, RxEvent::Complete(expected));
            assert!(!borrowed.active() && !legacy.active());
            assert_eq!(borrowed.on_frame_ref(&frame(&[1, 99]), 1, 0), Ok(RxEventRef::Ignored));
        }
    }

    /// Foreign data, overflow and timeout never expose retained completion; restart clears old bytes.
    #[test]
    fn borrowed_rejection_abort_and_restart_never_republish_old_completion() {
        let mut rx = IsoTpRx::new();
        rx.begin(0x7E8, 1, 10, 20).unwrap();
        assert_eq!(rx.on_frame_ref(&frame(&[1, 9]), 2, 11), Ok(RxEventRef::Ignored));
        let foreign = CanFrame::new(0x7E9, &[1, 9]).unwrap();
        assert_eq!(rx.on_frame_ref(&foreign, 1, 12), Ok(RxEventRef::Ignored));
        assert_eq!(
            rx.on_frame_ref(&frame(&[2, 8, 9]), 1, 20),
            Ok(RxEventRef::Complete(&[8, 9]))
        );
        rx.abort();
        assert!(rx.payload.is_empty());
        assert_eq!(rx.on_frame_ref(&frame(&[1, 7]), 1, 21), Ok(RxEventRef::Ignored));
        rx.begin(0x7E8, 2, 30, 40).unwrap();
        assert_eq!(rx.on_frame_ref(&frame(&[1, 7]), 2, 41), Err(IsoError::Timeout));
        assert!(rx.payload.is_empty());
        rx.begin(0x7E8, 3, 50, 60).unwrap();
        assert_eq!(
            rx.on_frame_ref(&frame(&[0x11, 1, 0, 0, 0, 0, 0, 0]), 3, 50),
            Ok(RxEventRef::Overflow(OVERFLOW))
        );
        assert!(rx.payload.is_empty());
        rx.begin(0x7E8, 4, 70, 80).unwrap();
        assert_eq!(rx.on_frame_ref(&frame(&[1, 6]), 4, 70), Ok(RxEventRef::Complete(&[6])));
        rx.begin(0x7E8, 5, 90, 100).unwrap();
        assert!(rx.payload.is_empty());
        assert_eq!(rx.on_frame_ref(&frame(&[1, 5]), 5, 90), Ok(RxEventRef::Complete(&[5])));
    }
}
