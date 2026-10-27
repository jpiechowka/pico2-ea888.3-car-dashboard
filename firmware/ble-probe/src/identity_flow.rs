//! Portable explicit identity-trial sequencing; hardware adapters own link/CoC I/O.

use core::future::Future;

use dashboard_core::ble::canyonero::{
    DecodeStatus,
    Epoch,
    Event as SessionEvent,
    InfoRequest,
    Session,
    Stage,
    TransportDomain,
    TransportFault,
};
use dashboard_core::discovery::identity::{ChannelParameters, Update, VENDOR_UUIDS};
use dashboard_core::discovery::{FailureReason, InfoValue, InspectionItem, Payload, ReadResult, Service};
use embassy_futures::select::{Either, select};

/// Fresh bounded endpoint collector; never initialized from saved capture handles.
#[derive(Default)]
pub struct FreshEndpoints {
    /// Unique fresh FFF1 declaration, absent until observed.
    service: Option<Service>,
    /// Value handles indexed by the four scoped UUIDs; zero means not observed.
    handles: [u16; 4],
    /// Sticky duplicate, property or handle relationship fault.
    invalid: bool,
}

impl FreshEndpoints {
    /// Borrows fresh inventory `item`, copying only scoped endpoint fields; never
    /// retains the input, consumes its publication owner or issues an I/O request.
    pub fn observe(
        &mut self,
        item: &InspectionItem,
    ) {
        match item {
            InspectionItem::Service(service) if service.uuid.short() == Some(0xFFF1) => {
                if self.service.is_some() || service.start == 0 || service.end < service.start {
                    self.invalid = true;
                } else {
                    self.service = Some(*service);
                }
            }
            InspectionItem::Characteristic(row) => {
                let Some(service) = self.service else {
                    return;
                };
                if row.service_start != service.start {
                    return;
                }
                let Some(index) = VENDOR_UUIDS.iter().position(|&uuid| row.uuid.short() == Some(uuid)) else {
                    return;
                };
                if self.handles[index] != 0
                    || row.properties & 2 == 0
                    || row.declaration <= service.start
                    || row.handle <= row.declaration
                    || row.handle > service.end
                    || self.handles.contains(&row.handle)
                {
                    self.invalid = true;
                } else {
                    self.handles[index] = row.handle;
                }
            }
            _ => {}
        }
    }

    /// Returns admitted fresh service/handle evidence, or a missing endpoint error.
    pub fn finish(self) -> Result<(u16, [u16; 4]), FailureReason> {
        if self.invalid {
            return Err(FailureReason::InvalidValue);
        }
        let service = self.service.ok_or(FailureReason::NotFound)?;
        if self.handles.contains(&0) {
            return Err(FailureReason::NotFound);
        }
        Ok((service.start, self.handles))
    }
}

/// Monotonic time seam shared by real firmware and deterministic flow regressions.
pub trait Clock {
    /// Returns boot-relative monotonic milliseconds; backwards values fault the session.
    fn now_ms(&self) -> u64;
    /// Waits until absolute `deadline_ms`; returns no earlier than that monotonic time.
    async fn until(
        &self,
        deadline_ms: u64,
    );
}

/// Four explicitly scoped read-only vendor operations, after full standard inventory.
pub trait VendorClient {
    /// Reads fresh `handle` into `bytes`; returns copied count at most its capacity.
    /// A full buffer is conservatively truncated evidence. Errors preserve ATT codes.
    /// Errors or cancellation may leave partial bytes; only successful returned lengths qualify.
    async fn read(
        &self,
        handle: u16,
        bytes: &mut [u8],
    ) -> Result<usize, FailureReason>;
}

/// Narrow identity-only transport seam; no arbitrary outbound command is accepted.
pub trait Transport {
    /// Affine complete receive-SDU owner; borrowing does not copy its packet allocation.
    type Sdu: AsRef<[u8]>;
    /// Opens the single configured candidate129 channel and returns actual parameters.
    /// Errors preserve L2CAP refusal codes; callers abandon the entire link on failure.
    async fn open(&mut self) -> Result<ChannelParameters, TransportFault>;
    /// Submits `request` exactly once as one complete SDU; returns local completion only.
    /// Cancellation/errors may have partial effects and require full link cleanup.
    async fn send(
        &mut self,
        request: InfoRequest,
    ) -> Result<(), TransportFault>;
    /// Returns one owned SDU or a terminal transport error; no copying receive API.
    async fn receive(&mut self) -> Result<Self::Sdu, TransportFault>;
}

/// Publication state retained outside cancelable work, with no duplicate receive payload.
#[derive(Default)]
pub struct Publication {
    /// Original session start awaiting acknowledgment before any connection work.
    pending_start_ms: Option<u64>,
    /// Final vendor storage, filled in place while unpublished; one read may be in flight.
    vendor: Option<InfoValue>,
    /// True only after a read returned or timed out, until reliable publication completes.
    /// Cancellation during the read leaves this false, so partial bytes never escape.
    vendor_ready: bool,
    /// Actual opened-channel snapshot awaiting acknowledgment before INFO submission.
    pending_channel: Option<ChannelParameters>,
    /// Actual successful local INFO submission time, retained across publication cancellation.
    pending_submitted_ms: Option<u64>,
    /// Prefix bytes already acknowledged by the reliable cross-core emitter.
    raw_bytes: u16,
    /// Received SDU length awaiting reliable metadata publication, if cancellation intervenes.
    pending_sdu: Option<u16>,
}

impl Publication {
    /// Returns publication state retaining boot-relative `started_ms` until Start reaches the queue.
    pub fn new(started_ms: u64) -> Self {
        Self {
            pending_start_ms: Some(started_ms),
            ..Self::default()
        }
    }

    /// Sends the pending Start through reliable `emit`; cancellation preserves it for cleanup.
    async fn publish_start(
        &mut self,
        emit: &impl AsyncFn(Update),
    ) {
        if let Some(at_ms) = self.pending_start_ms {
            emit(Update::Start { at_ms }).await;
            self.pending_start_ms = None;
        }
    }
}

/// Publishes retained Start before polling connection `future`, within absolute `deadline_ms`.
/// `clock` supplies monotonic time; reliable `emit` acknowledges queue acceptance. On timeout
/// or owner cancellation, `publication` retains an unaccepted Start for terminal flush.
/// Returns the connection result unchanged, or Timeout; dropping work requires owner cleanup.
pub async fn connect<T>(
    clock: &impl Clock,
    deadline_ms: u64,
    publication: &mut Publication,
    emit: &impl AsyncFn(Update),
    future: impl Future<Output = T>,
) -> Result<T, FailureReason> {
    before(clock, deadline_ms, async {
        publication.publish_start(emit).await;
        future.await
    })
    .await
}

/// Returns the source-configured local offer without inventing observed values.
/// One initial credit matches the stack's bounded backed-credit policy.
pub const fn offered_parameters() -> ChannelParameters {
    ChannelParameters {
        offered_mtu: 245,
        offered_mps: 247,
        offered_credits: 1,
        local_mtu: None,
        local_mps: None,
        peer_mtu: None,
        peer_mps: None,
        local_cid: None,
        peer_cid: None,
        local_credits: None,
        peer_credits: None,
    }
}

/// Races `future` with absolute `deadline_ms` using `clock`; returns Timeout if the timer wins.
/// Timer is polled first, including equality. A completed future's value is never discarded
/// by a later clock sample: callers retain its effects and check their Session deadline
/// before advancing authority. Owned losing futures drop before return. If `future` is
/// a pinned borrow, its caller must end the retained owner's scope before cleanup or
/// another operation; dropping this borrow alone does not cancel the owner's work.
pub async fn before<T>(
    clock: &impl Clock,
    deadline_ms: u64,
    future: impl Future<Output = T>,
) -> Result<T, FailureReason> {
    if clock.now_ms() >= deadline_ms {
        return Err(FailureReason::Timeout);
    }
    match select(clock.until(deadline_ms), future).await {
        Either::First(()) => Err(FailureReason::Timeout),
        Either::Second(value) => Ok(value),
    }
}

/// Returns transport failure preserving observed ATT/HCI code in `reason` and `partial_send`.
/// Local classifications have no invented numeric code.
pub fn transport_fault(
    reason: FailureReason,
    partial_send: bool,
) -> TransportFault {
    let (domain, code) = match reason {
        FailureReason::Att(code) => (TransportDomain::Att, Some(u16::from(code))),
        FailureReason::Hci(code) | FailureReason::DisconnectCode(code) => (TransportDomain::Hci, Some(u16::from(code))),
        _ => (TransportDomain::Local, None),
    };
    TransportFault {
        domain,
        code,
        partial_send,
    }
}

/// Records owner `reason` in `session` for `epoch` at current `clock` time, preserving first fault.
/// An expired whole-stage deadline is recorded as TimedOut before a local transport fault.
pub fn fail(
    session: &mut Session,
    epoch: Epoch,
    clock: &impl Clock,
    reason: FailureReason,
) {
    let now = clock.now_ms();
    let _ = session.event(epoch, now, SessionEvent::Tick);
    let _ = session.event(
        epoch,
        now,
        SessionEvent::TransportFailed(transport_fault(reason, false)),
    );
}

/// Publishes the current `session` stage/fault at `clock` time through `emit`.
/// Finished is represented as Closing here; the owner's subsequent End owns terminal status.
pub async fn publish_stage(
    session: &Session,
    clock: &impl Clock,
    emit: &impl AsyncFn(Update),
) {
    emit(Update::Stage {
        stage: if session.stage() == Stage::Finished {
            Stage::Closing
        } else {
            session.stage()
        },
        at_ms: clock.now_ms(),
        fault: session.fault(),
    })
    .await;
}

/// Publishes pending Start, vendor/channel/send observations, SDU metadata and retained `session`
/// bytes through reliable `emit`, in that order. Flush before newer cleanup Stage timestamps.
/// Updates `publication` only after each completion. Cancellation can resume from
/// the same offset; no SDU is copied into a second carry buffer. Returns after all
/// retained bytes have reached the cross-core queue, not after storage persistence.
pub async fn publish_pending(
    publication: &mut Publication,
    session: &Session,
    emit: &impl AsyncFn(Update),
) {
    publication.publish_start(emit).await;
    if publication.vendor_ready
        && let Some(value) = publication.vendor
    {
        emit(Update::Vendor(value)).await;
        publication.vendor_ready = false;
        publication.vendor = None;
    }
    if let Some(parameters) = publication.pending_channel {
        emit(Update::Channel(parameters)).await;
        publication.pending_channel = None;
    }
    if let Some(at_ms) = publication.pending_submitted_ms {
        emit(Update::RequestSubmitted { at_ms }).await;
        publication.pending_submitted_ms = None;
    }
    if let Some(length) = publication.pending_sdu {
        emit(Update::Sdu { length }).await;
        publication.pending_sdu = None;
    }
    while usize::from(publication.raw_bytes) < session.decoder().raw().len() {
        let start = usize::from(publication.raw_bytes);
        let end = (start + 64).min(session.decoder().raw().len());
        emit(Update::Chunk {
            offset: publication.raw_bytes,
            bytes: Payload::new(&session.decoder().raw()[start..end]),
        })
        .await;
        publication.raw_bytes = end as u16;
    }
}

/// Flushes retained `publication` observations from `session` through `emit` before
/// publishing the current stage at `clock` time. This order preserves an earlier
/// successful submission timestamp during cleanup. Cancellation remains resumable.
pub async fn publish_progress(
    publication: &mut Publication,
    session: &Session,
    clock: &impl Clock,
    emit: &impl AsyncFn(Update),
) {
    publish_pending(publication, session, emit).await;
    publish_stage(session, clock, emit).await;
}

/// Runs scoped reads, candidate channel, one INFO submission and bounded reply assembly.
/// `session` must be in TransportParameters after complete fresh standard inventory;
/// `endpoints` comes only from FreshEndpoints. `clock`, `client`, `transport`
/// and reliable `emit` are I/O seams; `publication` survives cancellation for cleanup flush.
/// Returns whether vendor reads were protected/unavailable/truncated, or a terminal local
/// classification. First detailed transport/protocol fault is retained in session.
/// Caller must cancel/drop all client/channel work and confirm full normal link cleanup.
pub async fn run_connected(
    session: &mut Session,
    clock: &impl Clock,
    client: &impl VendorClient,
    transport: &mut impl Transport,
    endpoints: (u16, [u16; 4]),
    publication: &mut Publication,
    emit: &impl AsyncFn(Update),
) -> Result<bool, FailureReason> {
    if session.stage() != Stage::TransportParameters {
        return Err(FailureReason::InvalidState);
    }
    let epoch = session.epoch().ok_or(FailureReason::InvalidState)?;
    let (service_start, handles) = endpoints;
    let mut limited = false;
    for (uuid, handle) in VENDOR_UUIDS.into_iter().zip(handles) {
        let started = clock.now_ms();
        let deadline = session.deadline_ms().ok_or(FailureReason::InvalidState)?;
        let read_deadline = started.saturating_add(3_000).min(deadline);
        publication.vendor_ready = false;
        let observation = publication.vendor.insert(InfoValue {
            service_start,
            handle,
            uuid,
            value: Payload::empty(),
            result: ReadResult::Error,
            failure: None,
            elapsed_ms: 0,
        });
        let read = before(clock, read_deadline, client.read(handle, &mut observation.value.bytes)).await;
        let (result, failure) = match read {
            Ok(Ok(len)) if len <= observation.value.bytes.len() => {
                observation.value.len = len as u8;
                observation.value.truncated = len == observation.value.bytes.len();
                observation.value.bytes[len..].fill(0);
                limited |= observation.value.truncated;
                (ReadResult::Ok, None)
            }
            Ok(Err(FailureReason::Att(code))) => {
                limited = true;
                let result = crate::errors::device_info_att_result(trouble_host::att::AttErrorCode::new(code));
                (result, Some(FailureReason::Att(code)))
            }
            Ok(Ok(_)) => (ReadResult::Error, Some(FailureReason::InvalidValue)),
            Ok(Err(reason)) | Err(reason) => (ReadResult::Error, Some(reason)),
        };
        if failure.is_some() {
            observation.value.bytes.fill(0);
        }
        observation.result = result;
        observation.failure = failure;
        observation.elapsed_ms = clock.now_ms().saturating_sub(started).min(u64::from(u32::MAX)) as u32;
        publication.vendor_ready = true;
        let late = clock.now_ms() >= read_deadline;
        // A timed-out value is evidence even when the whole-stage deadline expired.
        if result == ReadResult::Error {
            fail(session, epoch, clock, failure.unwrap_or(FailureReason::Other));
        } else if late {
            fail(session, epoch, clock, FailureReason::Timeout);
        }
        let publish_deadline = session.deadline_ms().ok_or(FailureReason::InvalidState)?;
        if let Err(reason) = before(clock, publish_deadline, publish_pending(publication, session, emit)).await {
            fail(session, epoch, clock, reason);
            return Err(reason);
        }
        if result == ReadResult::Error {
            return Err(failure.unwrap_or(FailureReason::Other));
        }
        if late {
            return Err(FailureReason::Timeout);
        }
    }
    if session
        .event(epoch, clock.now_ms(), SessionEvent::ParametersReady)
        .is_err()
    {
        return Err(FailureReason::Timeout);
    }
    let deadline = session.deadline_ms().ok_or(FailureReason::InvalidState)?;
    let channel = before(clock, deadline, async {
        publish_stage(session, clock, emit).await;
        emit(Update::Channel(offered_parameters())).await;
        transport.open().await
    })
    .await;
    let channel = match channel {
        Ok(Ok(channel)) => channel,
        Ok(Err(error)) => {
            let _ = session.event(epoch, clock.now_ms(), SessionEvent::TransportFailed(error));
            return Err(FailureReason::Other);
        }
        Err(reason) => {
            fail(session, epoch, clock, reason);
            return Err(reason);
        }
    };
    publication.pending_channel = Some(channel);
    if session
        .event(epoch, clock.now_ms(), SessionEvent::ChannelOpened)
        .is_err()
    {
        return Err(FailureReason::Timeout);
    }
    let deadline = session.deadline_ms().ok_or(FailureReason::InvalidState)?;
    let send = before(clock, deadline, async {
        publish_pending(publication, session, emit).await;
        publish_stage(session, clock, emit).await;
        let request = session
            .take_request(epoch, clock.now_ms())
            .map_err(|_| transport_fault(FailureReason::InvalidState, false))?;
        transport.send(request).await?;
        publication.pending_submitted_ms = Some(clock.now_ms());
        Ok(())
    })
    .await;
    match send {
        Ok(Ok(())) => {}
        Ok(Err(error)) => {
            let _ = session.event(epoch, clock.now_ms(), SessionEvent::TransportFailed(error));
            return Err(FailureReason::Other);
        }
        Err(reason) => {
            fail(session, epoch, clock, reason);
            return Err(reason);
        }
    }
    if session.event(epoch, clock.now_ms(), SessionEvent::InfoSent).is_err() {
        return Err(FailureReason::Timeout);
    }
    let deadline = session.deadline_ms().ok_or(FailureReason::InvalidState)?;
    let publication_result = before(clock, deadline, async {
        publish_pending(publication, session, emit).await;
        publish_stage(session, clock, emit).await;
    })
    .await;
    if let Err(reason) = publication_result {
        fail(session, epoch, clock, reason);
        return Err(reason);
    }
    loop {
        let deadline = session.deadline_ms().ok_or(FailureReason::InvalidState)?;
        let received = before(clock, deadline, transport.receive()).await;
        let sdu = match received {
            Ok(Ok(sdu)) => sdu,
            Ok(Err(error)) => {
                let _ = session.event(epoch, clock.now_ms(), SessionEvent::TransportFailed(error));
                return Err(FailureReason::Other);
            }
            Err(reason) => {
                fail(session, epoch, clock, reason);
                return Err(reason);
            }
        };
        let bytes = sdu.as_ref();
        publication.pending_sdu = Some(bytes.len().min(usize::from(u16::MAX)) as u16);
        let parsed = session.receive(epoch, clock.now_ms(), bytes);
        // The decoder owns the retained prefix before any publication can suspend.
        drop(sdu);
        let deadline = session.deadline_ms().ok_or(FailureReason::InvalidState)?;
        if let Err(reason) = before(clock, deadline, publish_pending(publication, session, emit)).await {
            fail(session, epoch, clock, reason);
            return Err(reason);
        }
        match parsed {
            Ok(DecodeStatus::NeedMore) => {}
            Ok(DecodeStatus::Complete) => return Ok(limited),
            Err(_) => return Err(FailureReason::InvalidValue),
        }
    }
}
