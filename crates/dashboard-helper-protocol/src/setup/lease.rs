use super::*;
use crate::*;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SetupDirection {
    PeerToOwner,
    OwnerToPeer,
}
#[derive(Clone, PartialEq, Eq)]
enum Phase {
    Hello,
    HelloAck,
    Idle,
    Admission(SetupStepId),
    Executing(SetupStepId),
    Receipt(SetupStepId),
    Prepare(Sha256),
    Register(SetupPeerIdentity),
    Result(Sha256),
    Close(SetupCloseDisposition),
    Closed,
}
/// Pure transcript/ledger validator. Caller must separately verify real pipe/process/Job facts.
#[derive(Clone)]
pub struct SetupLeasePeer {
    envelope: SetupLeaseEnvelope,
    expected_peer: SetupPeerIdentity,
    phase: Phase,
    records: u16,
    bytes: u64,
    message_no: u16,
    pending_since: u64,
    prepared: bool,
    registered: bool,
    result_ready: bool,
    delegated_image: Option<Sha256>,
    delegated_nonce: Option<Nonce>,
    delegated_peer: Option<SetupPeerIdentity>,
}
impl SetupLeasePeer {
    pub fn new(
        envelope: SetupLeaseEnvelope,
        expected_peer: SetupPeerIdentity,
        now_ms: u64,
    ) -> Result<Self, Error> {
        let bytes = encode_private(&envelope)?.len() as u64;
        if expected_peer.pid == 0 || expected_peer.image_sha256 != envelope.expected_peer_image_sha256 {
            return Err(Error::Schema);
        }
        Ok(Self {
            envelope,
            expected_peer,
            phase: Phase::Hello,
            records: 1,
            bytes,
            message_no: 1,
            pending_since: now_ms,
            prepared: false,
            registered: false,
            result_ready: false,
            delegated_image: None,
            delegated_nonce: None,
            delegated_peer: None,
        })
    }

    pub fn counts(&self) -> (u16, u64) { (self.records, self.bytes) }

    pub fn closed(&self) -> bool { self.phase == Phase::Closed }

    fn executor(&self) -> SetupExecutor {
        if self.envelope.peer_slot == 0 {
            SetupExecutor::Peer0
        } else {
            SetupExecutor::Peer1
        }
    }

    /// A failed transcript retires the association and latches stop, without changing completed facts.
    pub fn accept(
        &mut self,
        direction: SetupDirection,
        frame: &SetupLeaseFrame,
        now_ms: u64,
        ledger: &mut SetupLedger,
    ) -> Result<(), Error> {
        let mut candidate = self.clone();
        let mut next_ledger = ledger.clone();
        match candidate.advance(direction, frame, now_ms, &mut next_ledger) {
            Ok(()) => {
                *self = candidate;
                *ledger = next_ledger;
                Ok(())
            }
            Err(e) => {
                self.phase = Phase::Closed;
                ledger.stop(SetupFailure::InvalidState);
                Err(e)
            }
        }
    }

    pub fn owner_lost(
        &mut self,
        ledger: &mut SetupLedger,
    ) {
        self.phase = Phase::Closed;
        ledger.stop(SetupFailure::OwnerLost);
    }

    pub fn validate_result(
        &self,
        result: &SetupContinuationResult,
        ledger: &SetupLedger,
    ) -> Result<(), Error> {
        if result.attempt_id != self.envelope.attempt_id
            || result.context != self.envelope.context
            || result.action != self.envelope.action
            || result.role != self.envelope.role
            || result.peer_slot != self.envelope.peer_slot
        {
            return Err(Error::Schema);
        }
        ledger.validate_result(result)
    }

    fn advance(
        &mut self,
        direction: SetupDirection,
        frame: &SetupLeaseFrame,
        now_ms: u64,
        ledger: &mut SetupLedger,
    ) -> Result<(), Error> {
        if self.closed() {
            return Err(Error::Closed);
        }
        let size = encode_private(frame)?.len() as u64;
        if self.records >= 1024 || self.bytes.checked_add(size).is_none_or(|b| b > 4 * 1024 * 1024) {
            return Err(Error::Bounds);
        }
        if frame.attempt_id != self.envelope.attempt_id
            || frame.context_sha256 != self.envelope.context.context_sha256
            || frame.peer_slot != self.envelope.peer_slot
            || frame.message_no != self.message_no
        {
            return Err(Error::Schema);
        }
        if now_ms < self.pending_since {
            return Err(Error::InvalidState);
        }
        if !matches!(self.phase, Phase::Idle | Phase::Executing(_))
            && now_ms.checked_sub(self.pending_since).is_none_or(|d| d > 15000)
        {
            return Err(Error::InvalidState);
        }
        use SetupDirection::*;
        use SetupLeaseMessage::*;
        let next = match (&self.phase, direction, &frame.body) {
            (Phase::Hello, PeerToOwner, Hello { nonce, peer })
                if nonce == &self.envelope.nonce && peer == &self.expected_peer =>
            {
                Phase::HelloAck
            }
            (Phase::HelloAck, OwnerToPeer, HelloAck { nonce }) if nonce == &self.envelope.nonce => Phase::Idle,
            (Phase::Idle, PeerToOwner, AdmitStep { step }) if !self.result_ready => {
                let mut checked = ledger.clone();
                checked.start_next(self.executor(), *step)?;
                Phase::Admission(*step)
            }
            (Phase::Admission(want), OwnerToPeer, StepAdmitted { step }) if step == want => {
                ledger.start_next(self.executor(), *step)?;
                Phase::Executing(*step)
            }
            (Phase::Executing(want), PeerToOwner, StepFinished { outcome }) if outcome.step == *want => {
                ledger.finish_active(outcome.clone())?;
                ledger.hold_receipt(*want);
                Phase::Receipt(*want)
            }
            (Phase::Receipt(want), OwnerToPeer, StepReceived { step }) if step == want => {
                ledger.acknowledge_receipt(*step)?;
                Phase::Idle
            }
            (Phase::Idle, PeerToOwner, PrepareContinuation { image_sha256 })
                if self.envelope.role == SetupRole::Bootstrap && !self.prepared =>
            {
                Phase::Prepare(image_sha256.clone())
            }
            (Phase::Prepare(image), OwnerToPeer, ContinuationPrepared { peer_slot: 1, nonce })
                if nonce != &self.envelope.nonce =>
            {
                self.prepared = true;
                self.delegated_image = Some(image.clone());
                self.delegated_nonce = Some(nonce.clone());
                Phase::Idle
            }
            (Phase::Idle, PeerToOwner, RegisterContinuation { peer })
                if self.prepared && !self.registered && self.delegated_image.as_ref() == Some(&peer.image_sha256) =>
            {
                Phase::Register(peer.clone())
            }
            (Phase::Register(peer), OwnerToPeer, ContinuationRegistered { peer_slot: 1 }) => {
                self.registered = true;
                self.delegated_peer = Some(peer.clone());
                Phase::Idle
            }
            (Phase::Idle, PeerToOwner, ResultReady { result_sha256, .. })
                if !self.result_ready
                    && ledger
                        .assigned_outcomes(self.executor())
                        .iter()
                        .all(|o| o.state.terminal()) =>
            {
                Phase::Result(result_sha256.clone())
            }
            (Phase::Result(want), OwnerToPeer, ResultReceived { result_sha256 }) if result_sha256 == want => {
                self.result_ready = true;
                Phase::Idle
            }
            (Phase::Idle, PeerToOwner, Close { disposition })
                if self.result_ready || *disposition != SetupCloseDisposition::Completed =>
            {
                // Retiring a peer must also stop its executable work immediately;
                // the acknowledgement only confirms receipt, not the stop itself.
                match disposition {
                    SetupCloseDisposition::Completed => {}
                    SetupCloseDisposition::Failed => ledger.stop(SetupFailure::CommandFailed),
                    SetupCloseDisposition::Cancelled => ledger.stop(SetupFailure::Cancelled),
                    SetupCloseDisposition::CleanupFatal => ledger.stop(SetupFailure::OwnershipCleanupFatal),
                }
                Phase::Close(*disposition)
            }
            (Phase::Close(want), OwnerToPeer, Closed { disposition }) if disposition == want => Phase::Closed,
            (_, OwnerToPeer, Rejected { failure }) if !matches!(self.phase, Phase::Idle | Phase::Executing(_)) => {
                ledger.stop(*failure);
                Phase::Closed
            }
            _ => return Err(Error::InvalidState),
        };
        self.phase = next;
        self.pending_since = now_ms;
        self.records += 1;
        self.bytes += size;
        self.message_no = self.message_no.checked_add(1).ok_or(Error::Bounds)?;
        Ok(())
    }
}
/// Exactly two retained slots, never recycled during an attempt.
pub struct SetupLeasePeers {
    shape: DelegationShape,
    peers: [Option<SetupLeasePeer>; 2],
}
impl SetupLeasePeers {
    pub fn new(shape: DelegationShape) -> Self {
        Self {
            shape,
            peers: [None, None],
        }
    }

    pub fn register(
        &mut self,
        envelope: SetupLeaseEnvelope,
        identity: SetupPeerIdentity,
        now_ms: u64,
    ) -> Result<(), Error> {
        let slot = usize::from(envelope.peer_slot);
        if slot > 1 || self.peers[slot].is_some() {
            return Err(Error::Bounds);
        }
        match (self.shape, slot, envelope.role) {
            (DelegationShape::Cold, 0, SetupRole::Continuation)
            | (DelegationShape::Native, 0, SetupRole::Bootstrap) => {}
            (DelegationShape::Native, 1, SetupRole::Continuation) => {
                let parent = self.peers[0].as_ref().ok_or(Error::InvalidState)?;
                if !parent.registered
                    || parent.closed()
                    || parent.envelope.attempt_id != envelope.attempt_id
                    || parent.envelope.action != envelope.action
                    || parent.envelope.context != envelope.context
                    || parent.envelope.owner != envelope.owner
                    || parent.envelope.nonce == envelope.nonce
                    || parent.delegated_nonce.as_ref() != Some(&envelope.nonce)
                    || parent.delegated_peer.as_ref() != Some(&identity)
                    || parent.delegated_image.as_ref() != Some(&envelope.expected_peer_image_sha256)
                {
                    return Err(Error::InvalidState);
                }
            }
            _ => return Err(Error::Schema),
        }
        self.peers[slot] = Some(SetupLeasePeer::new(envelope, identity, now_ms)?);
        Ok(())
    }

    pub fn peer_mut(
        &mut self,
        slot: u8,
    ) -> Result<&mut SetupLeasePeer, Error> {
        self.peers
            .get_mut(usize::from(slot))
            .and_then(Option::as_mut)
            .ok_or(Error::NotFoundOrUnavailable)
    }
}
