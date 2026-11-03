//! Portable one-shot collector sequencing; the caller retains all owners through cancellation.
use core::pin::pin;

use dashboard_core::ble::canyonero::collection::{Command, Error, Event, ReadStatus, Request, Session};
use dashboard_core::ble::canyonero::reads::ReadId;
use dashboard_core::ble::canyonero::{CleanupOutcome, Epoch};
use dashboard_core::discovery::Payload;
use dashboard_core::discovery::collection::Update;

use crate::identity_flow::{Clock, before};

/// Narrow writer for nonforgeable collection permits on the already admitted INFO channel.
pub trait Transport {
    /// Affine complete receive packet; the flow releases it before any publication await.
    type Sdu: AsRef<[u8]>;
    /// Synchronously dequeues at most one already complete SDU, returning None if empty.
    /// Must not poll a receive future, grant credits, register waiters or submit signaling.
    /// The normal awaited receive remains responsible for credit replenishment.
    fn try_receive(&mut self) -> Result<Option<Self::Sdu>, Error>;
    /// Consumes `request` once on the current owner, validating its token against `session`
    /// and current external generation immediately before I/O at `clock` time. The writer
    /// must mark_attempt before polling hardware and call sent immediately on local completion.
    /// Errors/cancellation may have partial effects; no retry or speculative CLOSE is permitted.
    /// Returns unit only after local SDU completion and session admission, or a terminal error;
    /// success does not establish a CAN acknowledgment or ECU receipt.
    async fn send(
        &mut self,
        request: Request,
        session: &mut Session,
        clock: &impl Clock,
    ) -> Result<(), Error>;
    /// Returns the next owned SDU or terminal transport failure on the same channel.
    async fn receive(&mut self) -> Result<Self::Sdu, Error>;
}

/// Small resumable publication state; raw bytes remain in the externally owned protocol session.
pub struct Publication {
    /// Original explicit authorization time, pending until the event queue accepts Start.
    start: Option<u64>,
    /// One accepted protocol observation awaiting queue acceptance.
    event: Option<Update>,
    /// Current read identity; flow-control permits never replace its row.
    row: Option<ReadId>,
    /// Whether the current row's attempted timestamp reached the queue.
    attempted: bool,
    /// Whether the current row's local completion timestamp reached the queue.
    submitted: bool,
    /// Complete row payload length awaiting ordered chunk publication, in bytes.
    row_len: u8,
    /// Current row prefix accepted by the queue, in bytes.
    row_offset: u8,
    /// Complete read results observed;28 are required for full collection success.
    rows: u8,
    /// False after any negative or nondefault-session result; later rows cannot erase it.
    all_positive: bool,
    /// Matching CLOSE received; not physical connection cleanup.
    completed: bool,
    /// First portable-flow failure, including admission and writer errors.
    failure: Option<Error>,
    /// Terminal raw header accepted by the queue; cancellation cannot duplicate it.
    terminal_header: bool,
    /// Terminal prefix accepted by the queue, in bytes.
    terminal_offset: u16,
    /// Terminal cleanup observation accepted by the queue.
    ended: bool,
}
impl Publication {
    /// Retains authorization at boot-relative `at_ms`; performs no transport or queue work.
    /// Returns fresh resumable publication state with Start pending and no observed results.
    pub const fn new(at_ms: u64) -> Self {
        Self {
            start: Some(at_ms),
            event: None,
            row: None,
            attempted: false,
            submitted: false,
            row_len: 0,
            row_offset: 0,
            rows: 0,
            all_positive: true,
            completed: false,
            failure: None,
            terminal_header: false,
            terminal_offset: 0,
            ended: false,
        }
    }

    /// Returns whether all28 positive read results and matching CLOSE were observed without flow failure.
    /// Actual link cleanup remains an independent prerequisite for UI success.
    pub const fn successful(&self) -> bool {
        self.completed && self.all_positive && self.rows as usize == ReadId::ALL.len() && self.failure.is_none()
    }
}

/// Flushes pending `publication` metadata and row bytes borrowed from `session` through reliable
/// `emit`. Queue acceptance advances offsets; cancellation preserves unaccepted work. The caller
/// must not mutate the session or issue another permit until this future completes.
pub async fn publish_pending(
    publication: &mut Publication,
    session: &Session,
    emit: &impl AsyncFn(Update),
) {
    if let Some(at_ms) = publication.start {
        emit(Update::Start { at_ms }).await;
        publication.start = None;
    }
    if let (Some(id), Some(submission)) = (publication.row, session.submission())
        && submission.command == Command::Read(id)
    {
        if !publication.attempted
            && let Some(at_ms) = submission.attempted_at_ms
        {
            emit(Update::Attempt { id, at_ms }).await;
            publication.attempted = true;
        }
        if !publication.submitted
            && let Some(at_ms) = submission.completed_at_ms
        {
            emit(Update::Submitted { id, at_ms }).await;
            publication.submitted = true;
        }
    }
    if let Some(event) = publication.event {
        emit(event).await;
        publication.event = None;
    }
    if let Some(id) = publication.row {
        while publication.row_offset < publication.row_len {
            let start = usize::from(publication.row_offset);
            let end = (start + 64).min(usize::from(publication.row_len));
            emit(Update::Chunk {
                id,
                offset: publication.row_offset,
                bytes: Payload::new(&session.evidence().1[start..end]),
            })
            .await;
            publication.row_offset = end as u8;
        }
    }
}

/// Runs only after fresh valid INFO on the same channel. `session`/`publication` must be owned
/// outside this cancelable future. `epoch` identifies the explicit operation; `clock` supplies
/// monotonic milliseconds; `admit` confirms pre-reserved worst-case capture capacity before any
/// permit within two seconds, with completion-clock reversal/equality rejected before start;
/// `emit` acknowledges queue acceptance. Returns after matching CLOSE or a terminal
/// error; caller must drop channel/link work and call finish only with actual cleanup evidence.
/// `transport` owns the already admitted INFO channel and validates every affine write permit.
pub async fn run(
    session: &mut Session,
    epoch: Epoch,
    clock: &impl Clock,
    transport: &mut impl Transport,
    publication: &mut Publication,
    admit: &impl AsyncFn() -> bool,
    emit: &impl AsyncFn(Update),
) -> Result<(), Error> {
    let result = {
        // Retain the future in one residence; its borrows end before terminal mutation.
        let work = pin!(run_inner(session, epoch, clock, transport, publication, admit, emit));
        work.await
    };
    if let Err(error) = result {
        publication.failure.get_or_insert(error);
        if error == Error::Timeout {
            let _ = session.tick(epoch, clock.now_ms());
        }
        let _ = session.cancel(epoch, clock.now_ms());
    }
    result
}

/// Executes run's admitted sequence with the same borrowed owners and bounded queue waits.
/// Any error is recorded by the outer run before return; cancellation is handled by its caller.
/// `session` retains protocol authority and raw evidence for explicit `epoch`; `clock` supplies
/// monotonic boot milliseconds. `transport` uses the existing INFO channel. `publication` retains
/// queue progress; `emit` acknowledges each accepted update. `admit` confirms reserved capacity
/// before the first permit. Returns unit on matching CLOSE or the first flow/protocol error.
async fn run_inner(
    session: &mut Session,
    epoch: Epoch,
    clock: &impl Clock,
    transport: &mut impl Transport,
    publication: &mut Publication,
    admit: &impl AsyncFn() -> bool,
    emit: &impl AsyncFn(Update),
) -> Result<(), Error> {
    let admission_started = clock.now_ms();
    let admission_deadline = admission_started.checked_add(2_000).ok_or(Error::Clock)?;
    let admitted = before(
        clock,
        admission_deadline,
        pin!(async {
            publish_pending(publication, session, emit).await;
            admit().await
        }),
    )
    .await
    .map_err(|_| Error::Timeout)?;
    // before retains a ready result even if its poll crossed the timer boundary.
    // Observe it, then refuse new authority for late or reversed completion time.
    let admitted_at = clock.now_ms();
    if admitted_at < admission_started {
        return Err(Error::Clock);
    }
    if admitted_at >= admission_deadline {
        return Err(Error::Timeout);
    }
    session.start(epoch, admitted_at, admitted)?;
    loop {
        let deadline = session.deadline_ms().ok_or(Error::State)?;
        before(clock, deadline, pin!(publish_pending(publication, session, emit)))
            .await
            .map_err(|_| Error::Timeout)?;
        if session.ready_for_request()
            && let Some(packet) = transport.try_receive()?
        {
            let rejected = session.reject_queued(epoch, clock.now_ms(), packet.as_ref());
            drop(packet);
            return Err(rejected.err().unwrap_or(Error::State));
        }
        if let Some(request) = session.next_request(epoch, clock.now_ms(), true)? {
            if let Some(submission) = session.submission()
                && let Command::Read(id) = submission.command
            {
                publication.row = Some(id);
                publication.attempted = false;
                publication.submitted = false;
                publication.row_len = 0;
                publication.row_offset = 0;
            }
            let deadline = session.deadline_ms().ok_or(Error::State)?;
            before(clock, deadline, pin!(transport.send(request, session, clock)))
                .await
                .map_err(|_| Error::Timeout)??;
            continue;
        }
        let packet = before(clock, deadline, pin!(transport.receive()))
            .await
            .map_err(|_| Error::Timeout)??;
        let observed = session.receive(epoch, clock.now_ms(), packet.as_ref());
        drop(packet);
        let at_ms = clock.now_ms();
        publication.event = match observed? {
            Event::Pending | Event::FlowControlNeeded => None,
            Event::Voltage(millivolts) => Some(Update::Voltage { millivolts, at_ms }),
            Event::Opened(handle) => Some(Update::Opened { handle, at_ms }),
            Event::Configured => Some(Update::Configured { at_ms }),
            Event::ReadComplete { id, status } => {
                publication.rows += 1;
                publication.all_positive &= status == ReadStatus::Positive;
                publication.row_len = session.evidence().1.len() as u8;
                publication.row_offset = 0;
                Some(Update::RowHeader {
                    id,
                    at_ms,
                    status,
                    length: publication.row_len,
                    pending: session.pending_count(),
                })
            }
            Event::Finished => {
                publication.completed = true;
                return Ok(());
            }
        };
    }
}

/// Flushes retained `publication` and latest raw `session` prefix before End at `clock` time.
/// `cleanup` must be the caller's actual terminal link observation; Pending emits nothing.
/// Reliable `emit` may wait beyond the operation deadline; cancellation preserves offsets.
/// This performs no I/O, never retries, and never infers peer/CAN delivery from local completion.
pub async fn finish(
    publication: &mut Publication,
    session: &Session,
    clock: &impl Clock,
    cleanup: CleanupOutcome,
    emit: &impl AsyncFn(Update),
) {
    if cleanup == CleanupOutcome::Pending || publication.ended {
        return;
    }
    publish_pending(publication, session, emit).await;
    let (kind, bytes) = session.evidence();
    if !publication.terminal_header {
        emit(Update::TerminalHeader {
            kind,
            length: bytes.len() as u16,
        })
        .await;
        publication.terminal_header = true;
    }
    while usize::from(publication.terminal_offset) < bytes.len() {
        let start = usize::from(publication.terminal_offset);
        let end = (start + 64).min(bytes.len());
        emit(Update::TerminalChunk {
            offset: publication.terminal_offset,
            bytes: Payload::new(&bytes[start..end]),
        })
        .await;
        publication.terminal_offset = end as u16;
    }
    emit(Update::End {
        at_ms: clock.now_ms(),
        completed: publication.completed,
        fault: publication.failure.or(session.fault()).or(if publication.completed {
            None
        } else {
            Some(Error::Cancelled)
        }),
        cleanup,
        last_submission: session.submission(),
    })
    .await;
    publication.ended = true;
}
