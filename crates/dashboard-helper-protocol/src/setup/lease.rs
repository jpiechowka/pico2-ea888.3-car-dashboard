//! Private two-peer transcript admission with bounded records, receipts and one-way retirement.
use super::*;
use crate::*;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// Trusted transport direction supplied by the native adapter, not chosen from message content.
pub enum SetupDirection {
    /// Registered peer sends to its original owner.
    PeerToOwner,
    /// Original owner sends an acknowledgement/rejection to its peer.
    OwnerToPeer,
}
#[derive(Clone, PartialEq, Eq)]
/// Expected next transition; Idle/Executing defer operation deadlines to the native owner.
enum Phase {
    /// Waiting for exact nonce and expected peer identity.
    Hello,
    /// Waiting for matching owner nonce acknowledgement.
    HelloAck,
    /// Association ready for the next admitted control request.
    Idle,
    /// Waiting for owner admission; ledger is not yet Running.
    Admission(
        /// Requested exact tuple.
        SetupStepId,
    ),
    /// One ledger step is Running.
    Executing(
        /// Active exact tuple.
        SetupStepId,
    ),
    /// Finished step holds the ledger receipt barrier.
    Receipt(
        /// Tuple to acknowledge.
        SetupStepId,
    ),
    /// Bootstrap requested continuation preparation.
    Prepare(
        /// Bound continuation image digest.
        Sha256,
    ),
    /// Waiting for owner registration acknowledgement.
    Register(
        /// Prepared continuation process/image identity.
        SetupPeerIdentity,
    ),
    /// Waiting for owner result receipt.
    Result(
        /// Announced result-byte digest.
        Sha256,
    ),
    /// Waiting for exact close acknowledgement; abnormal causes are already latched.
    Close(
        /// Requested retirement reason.
        SetupCloseDisposition,
    ),
    /// Permanently retired; no sequence or slot reuse is allowed.
    Closed,
}
/// Pure transcript/ledger validator. Caller must separately verify real pipe/process/Job facts.
#[derive(Clone)]
pub struct SetupLeasePeer {
    /// Immutable attempt/context/role and secret capability binding.
    envelope: SetupLeaseEnvelope,
    /// Independently observed expected process/image identity.
    expected_peer: SetupPeerIdentity,
    /// Expected next message shape/direction.
    phase: Phase,
    /// Accepted envelope plus frame count, capped at 1024 per association.
    records: u16,
    /// Total accepted encoded envelope/frame bytes including prefixes, capped at 4 MiB.
    bytes: u64,
    /// Next exact nonwrapping frame sequence, starting at 1.
    message_no: u16,
    /// Last accepted transition time in owner-monotonic milliseconds.
    pending_since: u64,
    /// Sole continuation preparation has been acknowledged.
    prepared: bool,
    /// Sole continuation identity has been acknowledged.
    registered: bool,
    /// Result digest receipt has been acknowledged; further step admission is forbidden.
    result_ready: bool,
    /// Prepared continuation executable identity, retained for registration.
    delegated_image: Option<Sha256>,
    /// Distinct prepared continuation capability, retained for its admission envelope.
    delegated_nonce: Option<Nonce>,
    /// Registered continuation identity, matched when the second association is created.
    delegated_peer: Option<SetupPeerIdentity>,
}
impl SetupLeasePeer {
    /// Return a Hello-phase association for validated `envelope`, independent `expected_peer` and `now_ms`.
    /// Require nonzero peer PID and matching image digest; count the envelope against byte/record budgets.
    /// The caller must verify actual pipe ownership, process creation/image identity and OS cleanup ownership.
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

    /// Return accepted `(record count, framed bytes)`, including the initial envelope.
    pub fn counts(&self) -> (u16, u64) { (self.records, self.bytes) }

    /// Return whether this association is permanently retired, regardless of success or failure.
    pub fn closed(&self) -> bool { self.phase == Phase::Closed }

    /// Return the fixed ledger executor corresponding to the validated envelope slot.
    fn executor(&self) -> SetupExecutor {
        if self.envelope.peer_slot == 0 {
            SetupExecutor::Peer0
        } else {
            SetupExecutor::Peer1
        }
    }

    /// Return success after atomically advancing `frame` in `direction` at owner-relative `now_ms` with `ledger`.
    /// Validate cloned state first. On failure, retire this association and latch InvalidState in the original
    /// ledger, preserving completed/active facts; rejected candidate transitions/byte counts are discarded.
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

    /// Retire this peer and stop pending `ledger` work with OwnerLost; preserve active/completed evidence.
    pub fn owner_lost(
        &mut self,
        ledger: &mut SetupLedger,
    ) {
        self.phase = Phase::Closed;
        ledger.stop(SetupFailure::OwnerLost);
    }

    /// Return success for `result` only when envelope identity/action/context/role and `ledger` outcomes match.
    /// Does not hash transferred bytes, verify OS facts or publish output; native owner performs those steps.
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

    /// Return success after checking/advancing candidate `frame` in `direction` at `now_ms` against `ledger`.
    /// Reject closed peers, replay, context mismatch, backward time, >1024 records or >4 MiB framed bytes.
    /// Pending handshakes expire after 15000 ms (exact boundary accepted); Idle/Executing use caller deadlines.
    /// Called on clones because a rejected transition must not publish partially updated bookkeeping.
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
    /// Admitted topology; Cold forbids the second association.
    shape: DelegationShape,
    /// Retained associations, including retired ones; occupied slots are never recycled.
    peers: [Option<SetupLeasePeer>; 2],
}
impl SetupLeasePeers {
    /// Return an empty two-slot association set governed by `shape`.
    pub fn new(shape: DelegationShape) -> Self {
        Self {
            shape,
            peers: [None, None],
        }
    }

    /// Return success after registering `envelope` with independent OS `identity` at `now_ms`.
    /// Reject occupied/out-of-range slots and role/topology mismatches. Peer1 additionally requires a live
    /// registered bootstrap with exact inherited context/owner/action and prepared distinct nonce/image/identity.
    /// Errors leave retained slots unchanged; this method performs no process or pipe operations.
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

    /// Return the retained mutable association at `slot`, including a closed one; absent/out-of-range is unavailable.
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
