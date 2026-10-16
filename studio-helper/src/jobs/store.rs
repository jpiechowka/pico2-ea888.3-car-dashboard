use std::collections::VecDeque;
use std::time::{Instant, SystemTime};

use dashboard_helper_protocol::setup::*;
use dashboard_helper_protocol::*;

use super::logs::LogInput;
use super::time::utc_millis;

#[derive(Clone)]
struct Job {
    summary: JobSummary,
    records: VecDeque<Event>,
    next: u64,
    dropped: u64,
    start: Instant,
    elapsed: u64,
    pending: VecDeque<LogInput>,
    setup: Option<SetupLedger>,
}
/// Single serialized state owner: all mutation requires exclusive access. Native
/// adapters send control directly; the lossy queue carries only sanitized logs.
pub struct JobStore {
    session: SessionId,
    jobs: VecDeque<Job>,
    tombstones: VecDeque<JobId>,
}
impl JobStore {
    pub fn new(session: SessionId) -> Self {
        Self {
            session,
            jobs: VecDeque::new(),
            tombstones: VecDeque::new(),
        }
    }

    fn index(
        &self,
        id: &JobId,
    ) -> Result<usize, Error> {
        self.jobs
            .iter()
            .position(|j| j.summary.job_id == *id)
            .ok_or(if self.tombstones.contains(id) {
                Error::Expired
            } else {
                Error::NotFoundOrUnavailable
            })
    }

    fn change(
        &mut self,
        id: &JobId,
        f: impl FnOnce(&mut Job) -> Result<(), Error>,
    ) -> Result<(), Error> {
        let i = self.index(id)?;
        if self.jobs[i].summary.events_closed {
            return Err(Error::Closed);
        }
        let current = &self.jobs[i];
        let mut candidate = Job {
            summary: current.summary.clone(),
            records: VecDeque::new(),
            next: current.next,
            dropped: current.dropped,
            start: current.start,
            elapsed: current.elapsed,
            pending: current.pending.clone(),
            setup: current.setup.clone(),
        };
        f(&mut candidate)?;
        candidate.summary.validate()?;
        self.jobs[i].summary = candidate.summary;
        self.jobs[i].setup = candidate.setup;
        self.jobs[i].elapsed = candidate.elapsed;
        Ok(())
    }

    pub fn start(
        &mut self,
        id: JobId,
        operation: OperationKind,
        retry_of: Option<JobId>,
    ) -> Result<JobSummary, Error> {
        self.start_core(id, operation, retry_of, None)
    }

    pub fn start_setup(
        &mut self,
        id: JobId,
        plan: SetupPlan,
        retry_of: Option<JobId>,
    ) -> Result<JobSummary, Error> {
        self.start_core(id, OperationKind::Setup, retry_of, Some(SetupLedger::new(plan)))
    }

    fn start_core(
        &mut self,
        id: JobId,
        operation: OperationKind,
        retry_of: Option<JobId>,
        setup: Option<SetupLedger>,
    ) -> Result<JobSummary, Error> {
        if self.jobs.iter().any(|j| !j.summary.events_closed) {
            return Err(Error::Busy);
        }
        if (operation == OperationKind::Setup) != setup.is_some() {
            return Err(Error::InvalidState);
        }
        if self.jobs.iter().any(|j| j.summary.job_id == id) || self.tombstones.contains(&id) {
            return Err(Error::InvalidState);
        }
        if let Some(previous) = &retry_of {
            let i = self.index(previous)?;
            if self.jobs[i].summary.operation != operation {
                return Err(Error::InvalidState);
            }
        }
        let summary = JobSummary {
            session_id: self.session.clone(),
            job_id: id,
            operation,
            stages: BoundedVec::new(
                operation
                    .layout()
                    .into_iter()
                    .map(|stage| {
                        StageSummary {
                            stage,
                            state: StageState::Pending,
                            reason: None,
                            started_ms: None,
                            ended_ms: None,
                            tools: Default::default(),
                        }
                    })
                    .collect(),
            )?,
            cancel_requested: false,
            raw_exit: None,
            write_outcome: WriteOutcome::NotStarted,
            artifact_id: None,
            device_id: None,
            retry_of,
            terminal_cause: None,
            final_next: None,
            events_closed: false,
            running_firmware_observed: false,
            output_incomplete: false,
            output_dropped_chunks: 0,
            output_dropped_bytes: 0,
            anomalies: Default::default(),
            active_setup_step: None,
        };
        let mut job = Job {
            summary,
            records: VecDeque::new(),
            next: 1,
            dropped: 0,
            start: Instant::now(),
            elapsed: 0,
            pending: VecDeque::new(),
            setup,
        };
        if job.setup.is_some() {
            job.project_setup()?;
        }
        job.summary.validate()?;
        let summary = job.summary.clone();
        if self.jobs.len() == 16 {
            let old = self.jobs.pop_front().ok_or(Error::InvalidState)?;
            if self.tombstones.len() == 16 {
                self.tombstones.pop_front();
            }
            self.tombstones.push_back(old.summary.job_id);
        }
        self.jobs.push_back(job);
        Ok(summary)
    }

    pub fn transition(
        &mut self,
        id: &JobId,
        stage: StageId,
        state: StageState,
        reason: Option<Text<512>>,
    ) -> Result<(), Error> {
        self.change(id, |j| {
            if j.summary.operation == OperationKind::Setup {
                return Err(Error::InvalidState);
            }
            let i = j
                .summary
                .stages
                .iter()
                .position(|s| s.stage == stage)
                .ok_or(Error::InvalidState)?;
            let old = j.summary.stages[i].state;
            match (old, state) {
                (StageState::Pending, StageState::Running)
                    if !j.summary.cancel_requested
                        && j.summary.stages[..i]
                            .iter()
                            .all(|s| matches!(s.state, StageState::Succeeded | StageState::Skipped))
                        && !j.summary.stages.iter().any(|s| s.state == StageState::Running) => {}
                (StageState::Pending, StageState::Skipped) if reason.is_some() => {}
                (StageState::Running, StageState::Succeeded) => {}
                (StageState::Running, StageState::Failed | StageState::Cancelled) if reason.is_some() => {}
                _ => return Err(Error::InvalidState),
            }
            let elapsed = j.elapsed();
            let mut stages = j.summary.stages.clone().into_vec();
            stages[i].state = state;
            stages[i].reason = reason;
            if state == StageState::Running {
                stages[i].started_ms = Some(elapsed);
            } else {
                stages[i].ended_ms = Some(elapsed);
            }
            if matches!(state, StageState::Failed | StageState::Cancelled) {
                for s in &mut stages[i + 1..] {
                    if s.state == StageState::Pending {
                        s.state = StageState::Skipped;
                        s.reason = Some(Text::new("previous stage interrupted")?);
                        s.ended_ms = Some(elapsed);
                    }
                }
            }
            j.summary.stages = BoundedVec::new(stages)?;
            match (stage, state) {
                (StageId::Firmware(FirmwareStage::Transfer), StageState::Running) => {
                    j.summary.write_outcome = WriteOutcome::InProgress
                }
                (StageId::Firmware(FirmwareStage::Transfer), StageState::Succeeded) => {
                    j.summary.write_outcome = WriteOutcome::TransferredUnverified
                }
                (StageId::Firmware(FirmwareStage::Transfer), StageState::Failed | StageState::Cancelled) => {
                    j.summary.write_outcome = WriteOutcome::Unknown
                }
                (StageId::Firmware(FirmwareStage::Verification), StageState::Succeeded) => {
                    j.summary.write_outcome = WriteOutcome::Verified
                }
                _ => {}
            }
            Ok(())
        })
    }

    pub fn append(
        &mut self,
        id: &JobId,
        input: LogInput,
    ) -> Result<(), Error> {
        let i = self.index(id)?;
        let current = &self.jobs[i];
        if current.summary.events_closed {
            return Err(Error::Closed);
        }
        // Prepare only bounded new fragments and summary, never copy retained history.
        let mut delta = Job {
            summary: current.summary.clone(),
            records: VecDeque::new(),
            next: current.next,
            dropped: 0,
            start: current.start,
            elapsed: current.elapsed,
            pending: VecDeque::new(),
            setup: None,
        };
        delta.append(input)?;
        delta.summary.validate()?;
        let current = &mut self.jobs[i];
        for event in delta.records {
            if current.records.len() == 1024 {
                current.records.pop_front();
                current.dropped = checked_count(current.dropped, 1, &mut delta.summary.anomalies.counter_overflow);
            }
            current.records.push_back(event);
        }
        current.next = delta.next;
        current.elapsed = delta.elapsed;
        current.summary = delta.summary;
        Ok(())
    }

    /// Nonblocking bounded log ingress; control operations never depend on this capacity.
    pub fn enqueue(
        &mut self,
        id: &JobId,
        input: LogInput,
    ) -> Result<bool, Error> {
        let i = self.index(id)?;
        let j = &mut self.jobs[i];
        if j.summary.events_closed {
            return Err(Error::Closed);
        }
        j.validate_input(&input)?;
        if j.pending.len() == 256 {
            j.loss(1, input.text.len() as u64);
            return Ok(false);
        }
        j.pending.push_back(input);
        Ok(true)
    }

    pub fn drain(
        &mut self,
        id: &JobId,
    ) -> Result<(), Error> {
        let i = self.index(id)?;
        if self.jobs[i].summary.events_closed {
            return Err(Error::Closed);
        }
        while let Some(input) = self.jobs[i].pending.front().cloned() {
            self.append(id, input)?;
            self.jobs[i].pending.pop_front();
        }
        Ok(())
    }

    pub fn poll(
        &self,
        id: &JobId,
        request: PollRequest,
    ) -> Result<EventBatch, Error> {
        request.validate()?;
        let j = &self.jobs[self.index(id)?];
        let batch = EventBatch {
            session_id: self.session.clone(),
            job_id: id.clone(),
            oldest: j.records.front().map_or(j.next, |r| r.sequence),
            next: j.next,
            dropped: j.dropped,
            records: BoundedVec::new(
                j.records
                    .iter()
                    .filter(|r| r.sequence > request.after)
                    .take(usize::from(request.limit))
                    .cloned()
                    .collect(),
            )?,
            summary: j.summary.clone(),
        };
        batch.validate()?;
        Ok(batch)
    }

    pub fn cancel(
        &mut self,
        id: &JobId,
    ) -> Result<(), Error> {
        self.change(id, |j| {
            j.summary.cancel_requested = true;
            if let Some(setup) = &mut j.setup {
                setup.stop(SetupFailure::Cancelled);
                j.project_setup()?;
            }
            Ok(())
        })
    }

    /// Account only observed sanitized chunks dropped by the bounded process channel.
    pub fn account_log_loss(
        &mut self,
        id: &JobId,
        loss: super::ingress::LogLoss,
    ) -> Result<(), Error> {
        self.change(id, |j| {
            j.loss(loss.chunks, loss.bytes);
            j.summary.anomalies.counter_overflow |= loss.counter_overflow;
            Ok(())
        })
    }

    /// Trusted selection result, captured before transfer and retained as immutable history.
    pub fn bind_flash(
        &mut self,
        id: &JobId,
        artifact: ArtifactId,
        device: DeviceId,
    ) -> Result<(), Error> {
        self.change(id, |j| {
            if j.summary.operation != OperationKind::Flash
                || j.summary.write_outcome != WriteOutcome::NotStarted
                || j.summary.artifact_id.is_some()
                || j.summary.device_id.is_some()
            {
                return Err(Error::InvalidState);
            }
            j.summary.artifact_id = Some(artifact);
            j.summary.device_id = Some(device);
            Ok(())
        })
    }

    /// An unjoined run remains active and cannot seal; retain the failure separately.
    pub fn note_cleanup_fatal(
        &mut self,
        id: &JobId,
        output_incomplete: bool,
    ) -> Result<(), Error> {
        self.change(id, |j| {
            j.summary.terminal_cause = Some(TerminalCause::OwnershipCleanupFatal);
            j.summary.output_incomplete |= output_incomplete;
            if j.summary.write_outcome == WriteOutcome::InProgress {
                j.summary.write_outcome = WriteOutcome::Unknown;
            }
            if let Some(setup) = &mut j.setup {
                setup.stop(SetupFailure::OwnershipCleanupFatal);
                j.project_setup()?;
            } else {
                let mut stages = j.summary.stages.clone().into_vec();
                let elapsed = j.elapsed();
                for stage in &mut stages {
                    if stage.state == StageState::Pending {
                        stage.state = StageState::Skipped;
                        stage.reason = Some(Text::new("process ownership cleanup failed")?);
                        stage.ended_ms = Some(elapsed);
                    }
                }
                j.summary.stages = BoundedVec::new(stages)?;
            }
            Ok(())
        })
    }

    pub fn finish(
        &mut self,
        id: &JobId,
        cause: TerminalCause,
        closure: OutputClosure,
        artifact: Option<ArtifactId>,
        exit: Option<RawExit>,
    ) -> Result<(), Error> {
        self.change(id, |j| {
            if !closure.workers_joined || !closure.tree_empty || !j.pending.is_empty() {
                return Err(Error::InvalidState);
            }
            if (j.summary.terminal_cause == Some(TerminalCause::OwnershipCleanupFatal)
                || j.setup
                    .as_ref()
                    .is_some_and(|setup| setup.stop_cause() == Some(SetupFailure::OwnershipCleanupFatal)))
                && cause != TerminalCause::OwnershipCleanupFatal
            {
                return Err(Error::InvalidState);
            }
            if artifact
                .as_ref()
                .is_some_and(|a| j.summary.artifact_id.as_ref().is_some_and(|old| old != a))
            {
                return Err(Error::InvalidState);
            }
            if let Some(setup) = &mut j.setup {
                if cause == TerminalCause::Succeeded && setup.stop_cause().is_some() {
                    return Err(Error::InvalidState);
                }
                if setup.active().is_some() {
                    return Err(Error::InvalidState);
                }
                if cause != TerminalCause::Succeeded {
                    setup.stop(if cause == TerminalCause::Cancelled {
                        SetupFailure::Cancelled
                    } else if cause == TerminalCause::OwnershipCleanupFatal {
                        SetupFailure::OwnershipCleanupFatal
                    } else {
                        SetupFailure::CommandFailed
                    });
                }
                j.project_setup()?;
            }
            if let Some(e) = exit {
                e.validate()?;
            }
            if cause == TerminalCause::Succeeded
                && (j.summary.cancel_requested
                    || j.summary.stages.iter().any(|s| {
                        s.state != StageState::Succeeded
                            && !(j.summary.operation == OperationKind::Setup && s.state == StageState::Skipped)
                    })
                    || (j.summary.operation == OperationKind::Build && artifact.is_none())
                    || (j.summary.operation == OperationKind::Flash
                        && j.summary.write_outcome != WriteOutcome::Verified))
            {
                return Err(Error::InvalidState);
            }
            if cause == TerminalCause::Cancelled && !j.summary.cancel_requested {
                return Err(Error::InvalidState);
            }
            let mut stages = j.summary.stages.clone().into_vec();
            let elapsed = j.elapsed();
            for s in &mut stages {
                if s.state == StageState::Pending {
                    s.state = StageState::Skipped;
                    s.reason = Some(Text::new("operation interrupted")?);
                    s.ended_ms = Some(elapsed);
                } else if s.state == StageState::Running {
                    s.state = if cause == TerminalCause::Cancelled {
                        StageState::Cancelled
                    } else {
                        StageState::Failed
                    };
                    s.reason = Some(Text::new("operation terminated at producer boundary")?);
                    s.ended_ms = Some(elapsed);
                }
            }
            if j.summary.write_outcome == WriteOutcome::InProgress {
                j.summary.write_outcome = WriteOutcome::Unknown;
            }
            j.summary.stages = BoundedVec::new(stages)?;
            j.loss(closure.discarded_chunks, closure.discarded_bytes);
            j.summary.output_incomplete |= closure.output_incomplete;
            if artifact.is_some() {
                j.summary.artifact_id = artifact;
            }
            j.summary.raw_exit = exit;
            j.summary.terminal_cause = Some(cause);
            j.summary.final_next = Some(j.next);
            j.summary.events_closed = true;
            Ok(())
        })
    }

    pub fn setup_start_next(
        &mut self,
        id: &JobId,
        executor: SetupExecutor,
        step: SetupStepId,
    ) -> Result<(), Error> {
        self.change(id, |j| {
            j.setup
                .as_mut()
                .ok_or(Error::InvalidState)?
                .start_next(executor, step)?;
            j.project_setup()
        })
    }

    /// Apply private lease control to this owner's only ledger and publish its projection.
    /// Protocol rejection deliberately retains the stop latch/retired peer, never a success.
    pub fn setup_accept(
        &mut self,
        id: &JobId,
        peer: &mut SetupLeasePeer,
        direction: SetupDirection,
        frame: &SetupLeaseFrame,
        now_ms: u64,
    ) -> Result<(), Error> {
        let mut candidate = peer.clone();
        let mut accepted = Ok(());
        self.change(id, |j| {
            accepted = candidate.accept(direction, frame, now_ms, j.setup.as_mut().ok_or(Error::InvalidState)?);
            j.project_setup()
        })?;
        *peer = candidate;
        accepted
    }

    pub fn setup_validate_result(
        &self,
        id: &JobId,
        peer: &SetupLeasePeer,
        result: &SetupContinuationResult,
    ) -> Result<(), Error> {
        let ledger = self.jobs[self.index(id)?].setup.as_ref().ok_or(Error::InvalidState)?;
        peer.validate_result(result, ledger)
    }

    pub fn setup_owner_lost(
        &mut self,
        id: &JobId,
        peer: &mut SetupLeasePeer,
    ) -> Result<(), Error> {
        let mut candidate = peer.clone();
        self.change(id, |j| {
            candidate.owner_lost(j.setup.as_mut().ok_or(Error::InvalidState)?);
            j.project_setup()
        })?;
        *peer = candidate;
        Ok(())
    }

    pub fn setup_finish_active(
        &mut self,
        id: &JobId,
        outcome: SetupStepOutcome,
    ) -> Result<(), Error> {
        self.change(id, |j| {
            j.setup.as_mut().ok_or(Error::InvalidState)?.finish_active(outcome)?;
            j.project_setup()
        })
    }

    pub fn setup_skip_next(
        &mut self,
        id: &JobId,
        executor: SetupExecutor,
        step: SetupStepId,
    ) -> Result<(), Error> {
        self.change(id, |j| {
            j.setup
                .as_mut()
                .ok_or(Error::InvalidState)?
                .skip_next_unnecessary(executor, step)?;
            j.project_setup()
        })
    }

    pub fn summaries(&self) -> Vec<JobSummary> { self.jobs.iter().map(|j| j.summary.clone()).collect() }
}
impl Job {
    fn project_setup(&mut self) -> Result<(), Error> {
        let elapsed = self.elapsed();
        let ledger = self.setup.as_ref().ok_or(Error::InvalidState)?;
        // Every ledger path is authoritative, including step outcomes and rejected
        // acknowledgements. Reporting this failure does not assert output closure.
        if ledger.stop_cause() == Some(SetupFailure::OwnershipCleanupFatal) {
            self.summary.terminal_cause = Some(TerminalCause::OwnershipCleanupFatal);
        }
        let mut stages = self.summary.stages.clone().into_vec();
        for (s, p) in stages.iter_mut().zip(ledger.projection()) {
            s.state = p.state;
            s.reason = if p.failure.is_some() {
                Some(Text::new("setup interrupted")?)
            } else if p.state == StageState::Skipped {
                Some(Text::new("not assigned or unnecessary")?)
            } else {
                None
            };
            if s.state == StageState::Running && s.started_ms.is_none() {
                s.started_ms = Some(elapsed);
            }
            if s.state.terminal() && s.ended_ms.is_none() {
                s.ended_ms = Some(elapsed);
            }
            s.tools = BoundedVec::new(ledger.tool_projection(p.stage))?;
        }
        self.summary.active_setup_step = ledger.active();
        self.summary.stages = BoundedVec::new(stages)?;
        Ok(())
    }

    fn elapsed(&mut self) -> u64 {
        self.elapsed = super::time::monotonic_elapsed(
            self.elapsed,
            super::time::checked_millis(self.start.elapsed().as_millis()),
            &mut self.summary.anomalies.elapsed_anomaly,
        );
        self.elapsed
    }

    fn loss(
        &mut self,
        chunks: u64,
        bytes: u64,
    ) {
        self.summary.output_dropped_chunks = checked_count(
            self.summary.output_dropped_chunks,
            chunks,
            &mut self.summary.anomalies.counter_overflow,
        );
        self.summary.output_dropped_bytes = checked_count(
            self.summary.output_dropped_bytes,
            bytes,
            &mut self.summary.anomalies.counter_overflow,
        );
    }

    fn validate_input(
        &self,
        input: &LogInput,
    ) -> Result<(), Error> {
        if input.text.len() > 1024 || !self.summary.operation.layout().contains(&input.stage) {
            return Err(Error::Bounds);
        }
        Ok(())
    }

    fn append(
        &mut self,
        input: LogInput,
    ) -> Result<(), Error> {
        self.validate_input(&input)?;
        let mut remaining = input.text.as_str();
        let mut first = true;
        while first || !remaining.is_empty() {
            first = false;
            if self.next == u64::MAX {
                self.summary.anomalies.event_sequence_exhausted = true;
                self.loss(1, remaining.len() as u64);
                break;
            }
            let utc_ms = utc_millis(SystemTime::now());
            if utc_ms.is_none() {
                self.summary.anomalies.utc_unavailable = true;
            }
            let mut e = Event {
                sequence: self.next,
                utc_ms,
                elapsed_ms: self.elapsed(),
                stage: input.stage,
                kind: if input.progress {
                    EventKind::Progress
                } else {
                    EventKind::Log
                },
                level: Level::Info,
                source: input.source,
                text: Text::new("")?,
                progress: None,
                tool: None,
                setup_step: None,
            };
            // Measure the real envelope once, then count serde_json's string escaping
            // in one scalar pass. Final validation guards against serializer drift.
            let budget = Event::MAX_BYTES.checked_sub(encode(&e)?.len()).ok_or(Error::Bounds)?;
            let end = json_text_prefix(remaining, budget);
            if end == 0 && !remaining.is_empty() {
                return Err(Error::Bounds);
            }
            e.text = Text::new(&remaining[..end])?;
            e.validate()?;
            self.next = self.next.checked_add(1).ok_or(Error::SequenceExhausted)?;
            if self.next == u64::MAX {
                self.summary.anomalies.event_sequence_exhausted = true;
            }
            if self.records.len() == 1024 {
                self.records.pop_front();
                self.dropped = checked_count(self.dropped, 1, &mut self.summary.anomalies.counter_overflow);
            }
            self.records.push_back(e);
            remaining = &remaining[end..];
        }
        Ok(())
    }
}
fn json_text_prefix(
    text: &str,
    budget: usize,
) -> usize {
    let mut used = 0;
    let mut end = 0;
    for (offset, scalar) in text.char_indices() {
        let bytes = match scalar {
            '"' | '\\' | '\u{08}' | '\u{0c}' | '\n' | '\r' | '\t' => 2,
            '\u{00}'..='\u{1f}' => 6,
            _ => scalar.len_utf8(),
        };
        if used + bytes > budget {
            break;
        }
        used += bytes;
        end = offset + scalar.len_utf8();
    }
    end
}
fn checked_count(
    a: u64,
    b: u64,
    overflow: &mut bool,
) -> u64 {
    a.checked_add(b).unwrap_or_else(|| {
        *overflow = true;
        u64::MAX
    })
}
/// Internal producer facts; never deserialized from HTTP. The native owner proves
/// process-tree and output closure before sealing an operation.
pub struct OutputClosure {
    pub workers_joined: bool,
    pub tree_empty: bool,
    pub output_incomplete: bool,
    pub discarded_chunks: u64,
    pub discarded_bytes: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exhausted_sequence_keeps_mutable_summary_until_real_close() {
        let mut s = JobStore::new(SessionId::new("a".repeat(32)).unwrap());
        let id = JobId::new("b".repeat(32)).unwrap();
        s.start(id.clone(), OperationKind::DeviceScan, None).unwrap();
        s.jobs[0].next = u64::MAX - 1;
        let stage = StageId::Probe(ProbeStage::DeviceDetection);
        let input = LogInput {
            stage,
            source: Source::Stdout,
            text: "x".into(),
            progress: false,
        };
        s.append(&id, input.clone()).unwrap();
        s.append(&id, input).unwrap();
        assert_eq!(s.jobs[0].records.len(), 1);
        assert_eq!(s.jobs[0].next, u64::MAX);
        assert!(!s.summaries()[0].events_closed);
        s.transition(&id, stage, StageState::Running, None).unwrap();
        s.transition(&id, stage, StageState::Succeeded, None).unwrap();
        s.finish(
            &id,
            TerminalCause::Succeeded,
            OutputClosure {
                workers_joined: true,
                tree_empty: true,
                output_incomplete: false,
                discarded_chunks: 0,
                discarded_bytes: 0,
            },
            None,
            None,
        )
        .unwrap();
        let p = s
            .poll(
                &id,
                PollRequest {
                    after: u64::MAX,
                    limit: 128,
                },
            )
            .unwrap();
        assert!(p.records.is_empty());
        assert_eq!(p.summary.final_next, Some(u64::MAX));
    }
    #[test]
    fn checked_drop_count_does_not_wrap() {
        let mut flag = false;
        assert_eq!(checked_count(u64::MAX, 1, &mut flag), u64::MAX);
        assert!(flag);
    }
}
