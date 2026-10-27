//! Bounded source-configured identity-trial evidence; no transport I/O or vehicle data.
use super::{Characteristic, InfoValue, Payload, ReadResult, Service};
use crate::ble::canyonero::{CleanupOutcome, Fault, MAX_FRAME_BYTES, Stage, validate_frame};

/// Public-client compatibility candidate, never a GATT-discovered SPSM.
pub const CANDIDATE_SPSM: u16 = 129;
/// Public implementation pin defining the candidate and INFO primitive.
pub const SOURCE_COMMIT: &str = "78091bb5599c4f5e3002fbd5700b826a625e8b04";
/// The four explicitly scoped vendor UUIDs, in stable capture/display order.
pub const VENDOR_UUIDS: [u16; 4] = [0x0081, 0x0082, 0xFFF2, 0xFFF3];

/// Returns observation order for `stage`; this is not a serialized enum representation.
fn stage_order(stage: Stage) -> u8 {
    match stage {
        Stage::Idle => 0,
        Stage::Connecting => 1,
        Stage::Discovering => 2,
        Stage::TransportParameters => 3,
        Stage::OpeningCoc => 4,
        Stage::SendingInfo => 5,
        Stage::AwaitingInfo => 6,
        Stage::Closing => 7,
        Stage::Finished => 8,
    }
}

/// Directional CoC observations. None is unavailable, never a guessed zero/default.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ChannelParameters {
    /// Offered local receive SDU limit in bytes, 23..=245 for this tool.
    pub offered_mtu: u16,
    /// Offered local K-frame payload limit in bytes, at least23.
    pub offered_mps: u16,
    /// Offered initial receive credits; zero is a real offered value.
    pub offered_credits: u16,
    /// Actual local receive SDU limit after channel establishment, in bytes.
    pub local_mtu: Option<u16>,
    /// Actual local receive K-frame payload limit after establishment, in bytes.
    pub local_mps: Option<u16>,
    /// Peer receive SDU limit after establishment, in bytes.
    pub peer_mtu: Option<u16>,
    /// Peer receive K-frame payload limit after establishment, in bytes.
    pub peer_mps: Option<u16>,
    /// Observed local dynamic CID; absent when the stack does not expose it.
    pub local_cid: Option<u16>,
    /// Observed peer dynamic CID; absent when the stack does not expose it.
    pub peer_cid: Option<u16>,
    /// Observed remaining local receive credits at the snapshot, if exposed.
    pub local_credits: Option<u16>,
    /// Observed remaining peer receive credits at the snapshot, if exposed.
    pub peer_credits: Option<u16>,
}

impl ChannelParameters {
    /// Returns whether `self` respects local resource and Bluetooth scalar ranges.
    /// This validates representation, not interoperability or parameter provenance.
    pub fn valid(&self) -> bool {
        (23..=MAX_FRAME_BYTES as u16).contains(&self.offered_mtu)
            && (23..=247).contains(&self.offered_mps)
            && self.local_mtu.is_none_or(|v| v == self.offered_mtu)
            && self.local_mps.is_none_or(|v| v == self.offered_mps)
            && self.peer_mtu.is_none_or(|v| v >= 23)
            && self.peer_mps.is_none_or(|v| (23..=65533).contains(&v))
            && self.local_cid.is_none_or(|v| (0x40..=0x7F).contains(&v))
            && self.peer_cid.is_none_or(|v| (0x40..=0x7F).contains(&v))
    }
}

/// Small cross-core update; full raw evidence never travels as one queued event.
#[derive(Clone, Copy, Debug)]
pub enum Update {
    /// Begins the explicit identity attempt before connection, once per inspection.
    Start {
        /// Boot-relative requested start milliseconds.
        at_ms: u64,
    },
    /// Records lifecycle progress and preserves the first supplied fault.
    Stage {
        /// Current session stage; Idle is not an active attempt.
        stage: Stage,
        /// Boot-relative observation milliseconds, nondecreasing for ordinary updates.
        at_ms: u64,
        /// First session fault, including its original stage/time/domain.
        fault: Option<Fault>,
    },
    /// One bounded raw vendor read, validated against current-session declarations.
    Vendor(
        /// Read record for one of the four scoped UUIDs.
        InfoValue,
    ),
    /// Replaces directional parameters with the latest actual observation.
    Channel(
        /// Offered and optionally observed channel parameters.
        ChannelParameters,
    ),
    /// Local controller submission of the complete INFO SDU succeeded once.
    RequestSubmitted {
        /// Boot-relative local completion milliseconds; not a peer-processing acknowledgment.
        at_ms: u64,
    },
    /// Records one received complete SDU before its retained chunks are published.
    Sdu {
        /// Full observed SDU length in bytes, including zero for an empty SDU.
        length: u16,
    },
    /// Appends an exact contiguous raw stream prefix without silently discarding gaps.
    Chunk {
        /// Zero-based offset into the at-most245-byte retained prefix.
        offset: u16,
        /// One through64 exact bytes; truncated chunks are rejected.
        bytes: Payload<64>,
    },
    /// Final independent cleanup outcome, after first failure/progress publication.
    End {
        /// Boot-relative final observation milliseconds.
        at_ms: u64,
        /// Actual cleanup state; Pending is not an end observation.
        cleanup: CleanupOutcome,
    },
}

/// One explicit selected-peer identity attempt; strings are derived from raw bytes.
#[derive(Clone, Copy, Debug)]
pub struct Evidence {
    /// Requested start in boot-relative milliseconds, including failed connection attempts.
    pub started_at_ms: u64,
    /// Latest non-reversing lifecycle timestamp, in boot-relative milliseconds.
    pub updated_at_ms: u64,
    /// Final observation time in boot-relative milliseconds, absent while active.
    pub ended_at_ms: Option<u64>,
    /// Current lifecycle stage; no stage alone establishes success.
    pub stage: Stage,
    /// First session failure, preserved separately from final cleanup outcome.
    pub fault: Option<Fault>,
    /// Actual cleanup observation; submission alone remains Pending.
    pub cleanup: CleanupOutcome,
    /// Distinct vendor evidence in [`VENDOR_UUIDS`] order; not standard `InfoValue` admission.
    pub vendor_values: [Option<InfoValue>; 4],
    /// Offered and actual channel observations; absent before parameter preparation.
    pub channel: Option<ChannelParameters>,
    /// Time of successful local submission, or unavailable/not submitted.
    pub request_submitted_at_ms: Option<u64>,
    /// Total observed SDU bytes, saturated at u64::MAX, before any retained-prefix omission.
    pub observed_bytes: u64,
    /// Number of observed SDUs, saturated at u32::MAX; empty SDUs also count.
    pub sdu_count: u32,
    /// Largest observed complete SDU in bytes; zero remains valid for an empty SDU.
    pub max_sdu_bytes: u16,
    /// Exact retained stream prefix; bytes beyond `raw_len` are not evidence.
    pub(crate) raw: [u8; MAX_FRAME_BYTES],
    /// Initialized prefix length in0..=245 bytes.
    pub(crate) raw_len: u8,
}

impl Evidence {
    /// Returns an empty Connecting attempt started at boot-relative `at_ms`.
    /// Performs no connection and leaves parameters, reads and cleanup unavailable.
    pub const fn new(at_ms: u64) -> Self {
        Self {
            started_at_ms: at_ms,
            updated_at_ms: at_ms,
            ended_at_ms: None,
            stage: Stage::Connecting,
            fault: None,
            cleanup: CleanupOutcome::Pending,
            vendor_values: [None; 4],
            channel: None,
            request_submitted_at_ms: None,
            observed_bytes: 0,
            sdu_count: 0,
            max_sdu_bytes: 0,
            raw: [0; MAX_FRAME_BYTES],
            raw_len: 0,
        }
    }

    /// Returns the exact retained received prefix, at most245 bytes, without interpretation.
    pub fn raw(&self) -> &[u8] { &self.raw[..usize::from(self.raw_len)] }

    /// Returns whether observed receive bytes exceed the retained prefix, including
    /// capacity overflow or cancellation while publishing a received SDU's chunks.
    pub fn raw_incomplete(&self) -> bool { self.observed_bytes > u64::from(self.raw_len) }

    /// Returns true only for one valid INFO frame, successful request submission,
    /// completed normal cleanup and no fault or omitted receive evidence.
    /// Actual live-vs-saved and link/inventory qualification belong to the caller.
    pub fn successful(&self) -> bool {
        if self.stage != Stage::Finished
            || self.ended_at_ms.is_none()
            || self.cleanup != CleanupOutcome::Clean
            || self.fault.is_some()
            || self.request_submitted_at_ms.is_none()
            || self.raw_incomplete()
            || !self.valid(self.updated_at_ms)
            || self.channel.as_ref().is_none_or(|c| {
                c.local_mtu.is_none() || c.local_mps.is_none() || c.peer_mtu.is_none() || c.peer_mps.is_none()
            })
            || self.vendor_values.iter().any(|v| {
                v.as_ref()
                    .is_none_or(|v| v.result != ReadResult::Ok || v.failure.is_some() || v.value.truncated)
            })
        {
            return false;
        }
        validate_frame(self.raw()).is_ok()
    }

    /// Applies small `update`, returning false before mutation for invalid order,
    /// bounds, duplicate writes or timestamps. Progress observations may skip stages
    /// but never regress; a time-reversal fault retains its original time while the
    /// latest monotonic observation stays unchanged. Vendor relationships are checked
    /// by the owning inspection. First faults remain sticky. No update starts I/O.
    pub fn apply(
        &mut self,
        update: Update,
    ) -> bool {
        if self.ended_at_ms.is_some() {
            return false;
        }
        match update {
            Update::Start { .. } => return false,
            Update::Stage { stage, at_ms, fault } => {
                if stage == Stage::Idle
                    || stage == Stage::Finished
                    || stage_order(stage) < stage_order(self.stage)
                    || (at_ms < self.updated_at_ms
                        && !fault.is_some_and(|f| f.reason == crate::ble::canyonero::Reason::TimeReversed))
                    || fault.is_some_and(|f| f.at_ms > at_ms.max(self.updated_at_ms))
                {
                    return false;
                }
                self.stage = stage;
                self.updated_at_ms = at_ms.max(self.updated_at_ms);
                self.fault = self.fault.or(fault);
            }
            Update::Vendor(value) => {
                let Some(index) = VENDOR_UUIDS.iter().position(|&uuid| uuid == value.uuid) else {
                    return false;
                };
                if self.vendor_values[index].is_some()
                    || usize::from(value.value.len) > 64
                    || (value.result == ReadResult::Ok) != value.failure.is_none()
                {
                    return false;
                }
                self.vendor_values[index] = Some(value);
            }
            Update::Channel(parameters) => {
                if !parameters.valid() {
                    return false;
                }
                self.channel = Some(parameters);
            }
            Update::RequestSubmitted { at_ms } => {
                if self.request_submitted_at_ms.is_some() || at_ms < self.updated_at_ms || self.channel.is_none() {
                    return false;
                }
                self.request_submitted_at_ms = Some(at_ms);
                self.updated_at_ms = at_ms;
            }
            Update::Sdu { length } => {
                if self.request_submitted_at_ms.is_none() {
                    return false;
                }
                self.observed_bytes = self.observed_bytes.saturating_add(u64::from(length));
                self.sdu_count = self.sdu_count.saturating_add(1);
                self.max_sdu_bytes = self.max_sdu_bytes.max(length);
            }
            Update::Chunk { offset, bytes } => {
                let len = usize::from(bytes.len);
                let start = usize::from(offset);
                let end = start + len;
                if start != usize::from(self.raw_len)
                    || len == 0
                    || len > 64
                    || bytes.truncated
                    || end > MAX_FRAME_BYTES
                    || end as u64 > self.observed_bytes
                {
                    return false;
                }
                self.raw[start..end].copy_from_slice(bytes.as_slice());
                self.raw_len = end as u8;
            }
            Update::End { at_ms, cleanup } => {
                if at_ms < self.updated_at_ms || cleanup == CleanupOutcome::Pending {
                    return false;
                }
                self.updated_at_ms = at_ms;
                self.ended_at_ms = Some(at_ms);
                self.cleanup = cleanup;
                self.stage = Stage::Finished;
            }
        }
        true
    }

    /// Returns whether internal bookkeeping is valid at boot-relative capture time `at_ms`.
    /// Failed/partial attempts remain valid evidence; no success is fabricated.
    pub fn valid(
        &self,
        at_ms: u64,
    ) -> bool {
        self.started_at_ms <= self.updated_at_ms
            && self.updated_at_ms <= at_ms
            && self.stage != Stage::Idle
            && self.ended_at_ms.is_none_or(|end| end == self.updated_at_ms)
            && (self.stage == Stage::Finished) == self.ended_at_ms.is_some()
            && (self.cleanup != CleanupOutcome::Pending) == self.ended_at_ms.is_some()
            && self
                .fault
                .is_none_or(|f| f.at_ms <= self.updated_at_ms && f.stage != Stage::Idle && f.stage != Stage::Finished)
            && self.channel.as_ref().is_none_or(|c| c.valid())
            && self
                .request_submitted_at_ms
                .is_none_or(|time| self.channel.is_some() && time >= self.started_at_ms && time <= self.updated_at_ms)
            && usize::from(self.raw_len) <= MAX_FRAME_BYTES
            && u64::from(self.raw_len) <= self.observed_bytes
            && u64::from(self.max_sdu_bytes) <= self.observed_bytes
            && (self.sdu_count != 0 || (self.observed_bytes == 0 && self.raw_len == 0 && self.max_sdu_bytes == 0))
            && (self.sdu_count == 0 || self.request_submitted_at_ms.is_some())
            && self.vendor_values.iter().zip(VENDOR_UUIDS).all(|(value, uuid)| {
                value.as_ref().is_none_or(|v| {
                    v.uuid == uuid
                        && usize::from(v.value.len) <= 64
                        && (v.result == ReadResult::Ok) == v.failure.is_none()
                })
            })
    }
}

/// Returns whether vendor `value` references a unique scoped readable declaration
/// in exactly one freshly inventoried FFF1 service in `services`/`characteristics`.
/// This checks provenance only and never interprets the returned vendor bytes.
pub fn vendor_relationship(
    value: &InfoValue,
    services: &[Service],
    characteristics: &[Characteristic],
) -> bool {
    VENDOR_UUIDS.contains(&value.uuid)
        && services.iter().filter(|s| s.uuid.short() == Some(0xFFF1)).count() == 1
        && services
            .iter()
            .any(|s| s.start == value.service_start && s.uuid.short() == Some(0xFFF1))
        && characteristics
            .iter()
            .filter(|c| c.service_start == value.service_start && c.uuid.short() == Some(value.uuid))
            .count()
            == 1
        && characteristics.iter().any(|c| {
            c.service_start == value.service_start
                && c.handle == value.handle
                && c.uuid.short() == Some(value.uuid)
                && c.properties & 2 != 0
        })
}
