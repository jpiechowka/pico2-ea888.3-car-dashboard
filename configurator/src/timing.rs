//! Monotonic playback and bounded browser CPU/presentation-opportunity timing.
//! Browser timestamps use [High Resolution Time](https://www.w3.org/TR/hr-time-3/);
//! a later animation callback measures presentation opportunity, never panel scanout.
use std::sync::atomic::{AtomicUsize, Ordering};

use serde::Serialize;

use crate::events::DiagnosticCount;

/// Process-local owner issuer; checked allocation never wraps to an older owner.
static NEXT_TRACE_OWNER: AtomicUsize = AtomicUsize::new(1);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
/// Permanently exhausted capability counter; existing valid permits may still finish.
pub enum TraceExhaustion {
    /// No distinct capture owner can be issued.
    OwnerIdentity,
    /// This capture cannot issue another distinct recording run.
    RunIdentity,
    /// The current run cannot count another outstanding permit.
    InFlight,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
/// Admission state of the capture; stopping retains samples and outstanding permits.
pub enum TraceStatus {
    /// No open recording interval; valid late evidence can still fall inside the run.
    Stopped,
    /// The current run accepts timestamps from its start onward.
    Recording,
    /// No further admission or restart; stores the checked counter that exhausted.
    Exhausted(
        /// Counter whose exhaustion permanently closed admission.
        TraceExhaustion,
    ),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// Trace admission or completion rejection, independent of rendering success.
pub enum TraceError {
    /// A capability counter exhausted without wrapping.
    Exhausted(
        /// Counter that could not admit a new owner/run/permit.
        TraceExhaustion,
    ),
    /// Timestamp is outside the run, nonfinite, or no run has started.
    OutsideRun,
    /// Clock/duration is invalid or the sample's temporal ordering is inconsistent.
    InvalidTime,
    /// Permit belongs to another owner/run, was consumed, or has no outstanding slot.
    StalePermit,
}

/// Private owner/run capability. Finish or abandon exactly once; never clone it.
#[derive(Debug)]
pub struct TracePermit {
    /// Capture owner admitted by the checked issuer.
    owner: usize,
    /// Recording run that issued this permit.
    run: u64,
    /// Exact finite input timestamp in milliseconds that completion must echo.
    input_ms: f64,
    /// Whether finish/abandon already released the outstanding slot.
    consumed: bool,
}

#[derive(Clone, Debug, Serialize)]
/// One browser render observation; timestamps share the browser performance origin.
pub struct TraceSample {
    /// Input observation in milliseconds, matching the issuing permit exactly.
    pub input_ms: f64,
    /// Canvas submission timestamp in milliseconds, at or after input.
    pub canvas_ms: f64,
    /// Later animation opportunity timestamp in milliseconds, at or after submission.
    pub presentation_boundary_ms: f64,
    /// Finite nonnegative CPU/render elapsed milliseconds, not device frame time.
    pub render_ms: f64,
    /// Number of configured widgets for this observation.
    pub widgets: usize,
    /// Number of configured views for this observation.
    pub views: usize,
    /// Whether synthetic warning preview was enabled.
    pub warning_preview: bool,
}

/// Bounded browser CPU/opportunity evidence. Serialization borrows data and
/// cannot duplicate the live owner; there is deliberately no Clone/Deserialize.
#[derive(Serialize)]
pub struct TraceCapture {
    /// Distinct process-local owner, absent after issuer exhaustion; never serialized.
    #[serde(skip)]
    owner: Option<usize>,
    /// Checked run identifier; zero means no run has started.
    #[serde(rename = "id")]
    run: u64,
    /// Current evidence admission state.
    status: TraceStatus,
    /// Current run's inclusive start in monotonic browser milliseconds.
    start_ms: f64,
    /// Inclusive terminal boundary when stopped/exhausted, in browser milliseconds.
    end_ms: f64,
    /// Largest accepted start/stop/input timestamp; run boundaries cannot move backward.
    high_water_ms: f64,
    /// Number of issued, unconsumed permits in the current run.
    in_flight: u32,
    /// At most 512 accepted render observations in insertion order.
    samples: Vec<TraceSample>,
    /// At most 512 `(start_ms, duration_ms)` long-task observations.
    long_tasks: Vec<(f64, f64)>,
    /// Saturating count of otherwise valid observations omitted at either capacity.
    dropped: DiagnosticCount,
}

impl Default for TraceCapture {
    /// Returns a stopped empty capture, or exhausted capture if the global issuer is full.
    fn default() -> Self { Self::with_issuer(&NEXT_TRACE_OWNER) }
}

impl TraceCapture {
    /// Claim one checked ID from `issuer`; returns a stopped capture or permanent
    /// owner-exhausted state without incrementing a saturated issuer.
    fn with_issuer(issuer: &AtomicUsize) -> Self {
        let owner = issuer
            .try_update(Ordering::Relaxed, Ordering::Relaxed, |id| id.checked_add(1))
            .ok();
        Self {
            owner,
            run: 0,
            status: if owner.is_some() {
                TraceStatus::Stopped
            } else {
                TraceStatus::Exhausted(TraceExhaustion::OwnerIdentity)
            },
            start_ms: 0.0,
            end_ms: 0.0,
            high_water_ms: 0.0,
            in_flight: 0,
            samples: Vec::with_capacity(512),
            long_tasks: Vec::with_capacity(512),
            dropped: DiagnosticCount::default(),
        }
    }

    /// Returns the current admission state without changing retained evidence.
    pub fn status(&self) -> TraceStatus { self.status }

    /// Returns the count of unconsumed permits for this run.
    pub fn in_flight(&self) -> u32 { self.in_flight }

    /// Returns the at-most-512 retained render observations in insertion order.
    pub fn samples(&self) -> &[TraceSample] { &self.samples }

    /// Returns retained `(start_ms, duration_ms)` browser long tasks in insertion order.
    pub fn long_tasks(&self) -> &[(f64, f64)] { &self.long_tasks }

    /// Returns the combined saturating capacity-drop counter for both evidence lists.
    pub fn dropped(&self) -> DiagnosticCount { self.dropped }

    /// Returns the exhaustion error if permanently closed, otherwise `Ok(())`.
    fn check_status(&self) -> Result<(), TraceError> {
        match self.status {
            TraceStatus::Exhausted(cause) => Err(TraceError::Exhausted(cause)),
            _ => Ok(()),
        }
    }

    /// Close admission at the latest accepted time using `cause`; returns its error.
    /// Retains the current run, evidence and outstanding permits for late completion.
    fn exhaust(
        &mut self,
        cause: TraceExhaustion,
    ) -> TraceError {
        self.status = TraceStatus::Exhausted(cause);
        self.end_ms = self.high_water_ms;
        TraceError::Exhausted(cause)
    }

    /// Start a fresh run at finite nonnegative `now_ms`, clamped to the prior time
    /// high-water mark. Returns `Ok(())` after clearing evidence and old permits.
    /// Invalid time preserves state; counter exhaustion closes admission permanently.
    /// A new run makes all previously issued permits stale, even if still held.
    pub fn start(
        &mut self,
        now_ms: f64,
    ) -> Result<(), TraceError> {
        self.check_status()?;
        if !now_ms.is_finite() || now_ms < 0.0 {
            return Err(TraceError::InvalidTime);
        }
        self.high_water_ms = self.high_water_ms.max(now_ms);
        let Some(run) = self.run.checked_add(1) else {
            return Err(self.exhaust(TraceExhaustion::RunIdentity));
        };
        self.run = run;
        self.status = TraceStatus::Recording;
        self.start_ms = self.high_water_ms;
        self.end_ms = self.start_ms;
        self.in_flight = 0;
        self.samples.clear();
        self.long_tasks.clear();
        self.dropped = DiagnosticCount::default();
        Ok(())
    }

    /// Close the run at finite nonnegative `now_ms`, clamped to the time high-water
    /// mark. Returns `Ok(())` while retaining evidence and permits; invalid time or
    /// prior exhaustion returns an error without changing state.
    pub fn stop(
        &mut self,
        now_ms: f64,
    ) -> Result<(), TraceError> {
        self.check_status()?;
        if !now_ms.is_finite() || now_ms < 0.0 {
            return Err(TraceError::InvalidTime);
        }
        self.high_water_ms = self.high_water_ms.max(now_ms);
        self.end_ms = self.high_water_ms;
        self.status = TraceStatus::Stopped;
        Ok(())
    }

    /// Returns whether finite `time` in browser milliseconds lies in an existing
    /// run's inclusive interval; a recording run has no upper boundary yet.
    fn includes(
        &self,
        time: f64,
    ) -> bool {
        self.run != 0
            && time.is_finite()
            && time >= self.start_ms
            && (self.status == TraceStatus::Recording || time <= self.end_ms)
    }

    /// Returns a unique consumable permit for `input_ms` inside the current run.
    /// Admission also permits late input timestamps inside a stopped interval.
    /// Outside/nonfinite input returns `OutsideRun`; checked count exhaustion closes
    /// admission. Dropping a permit does not release its slot: finish or abandon it.
    pub fn begin(
        &mut self,
        input_ms: f64,
    ) -> Result<TracePermit, TraceError> {
        self.check_status()?;
        if !self.includes(input_ms) {
            return Err(TraceError::OutsideRun);
        }
        self.high_water_ms = self.high_water_ms.max(input_ms);
        let Some(in_flight) = self.in_flight.checked_add(1) else {
            return Err(self.exhaust(TraceExhaustion::InFlight));
        };
        self.in_flight = in_flight;
        // A recording run can exist only after checked owner admission.
        let owner = self
            .owner
            .ok_or(TraceError::Exhausted(TraceExhaustion::OwnerIdentity))?;
        Ok(TracePermit {
            owner,
            run: self.run,
            input_ms,
            consumed: false,
        })
    }

    /// Consume `permit` and decrement its outstanding slot, returning `Ok(())`.
    /// Returns `StalePermit` without mutation for a foreign, obsolete or consumed
    /// permit. Current permits remain consumable after stopping/exhaustion.
    fn complete(
        &mut self,
        permit: &mut TracePermit,
    ) -> Result<(), TraceError> {
        if self.owner != Some(permit.owner) || self.run != permit.run || permit.consumed || self.in_flight == 0 {
            return Err(TraceError::StalePermit);
        }
        permit.consumed = true;
        self.in_flight -= 1;
        Ok(())
    }

    /// Cancellation/error path for a still-live run. Late completion is stale.
    /// Consumes `permit` without recording evidence; returns `StalePermit` if its
    /// owner/run/slot is no longer valid. Does not require active recording.
    pub fn abandon(
        &mut self,
        permit: &mut TracePermit,
    ) -> Result<(), TraceError> {
        self.complete(permit)
    }

    /// Consume `permit`, then validate and record `sample`; returns `Ok(())` for
    /// accepted or capacity-dropped evidence. Invalid time returns `InvalidTime`
    /// after consuming the permit; stale permits return before mutation. Timestamp
    /// equality/order and nonnegative finite duration are checked, not view counts.
    /// After 512 samples, increment the drop counter instead of extending storage.
    pub fn finish(
        &mut self,
        permit: &mut TracePermit,
        sample: TraceSample,
    ) -> Result<(), TraceError> {
        self.complete(permit)?;
        if sample.input_ms != permit.input_ms
            || !self.includes(sample.input_ms)
            || !sample.canvas_ms.is_finite()
            || sample.canvas_ms < sample.input_ms
            || !sample.presentation_boundary_ms.is_finite()
            || sample.presentation_boundary_ms < sample.canvas_ms
            || !sample.render_ms.is_finite()
            || sample.render_ms < 0.0
        {
            return Err(TraceError::InvalidTime);
        }
        if self.samples.len() == 512 {
            self.dropped.increment();
        } else {
            self.samples.push(sample);
        }
        Ok(())
    }

    /// Record finite nonnegative `duration_ms` for in-run `start_ms`; returns
    /// `Ok(())` even when the 512-entry limit drops it. Outside-run/nonfinite start,
    /// invalid duration or exhausted admission returns an error without insertion.
    pub fn record_long_task(
        &mut self,
        start_ms: f64,
        duration_ms: f64,
    ) -> Result<(), TraceError> {
        self.check_status()?;
        if !self.includes(start_ms) {
            return Err(TraceError::OutsideRun);
        }
        if !duration_ms.is_finite() || duration_ms < 0.0 {
            return Err(TraceError::InvalidTime);
        }
        if self.long_tasks.len() == 512 {
            self.dropped.increment();
        } else {
            self.long_tasks.push((start_ms, duration_ms));
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug)]
/// Millisecond playback clock that never moves backward within one instance.
pub struct Playback {
    /// Last emitted playback position in whole milliseconds, saturating at `u64::MAX`.
    sample: u64,
    /// Optional `(browser_start_ms, playback_start_ms)` while running.
    anchor: Option<(f64, u64)>,
}

impl Playback {
    /// Returns a paused clock positioned at `sample` whole playback milliseconds.
    pub const fn new(sample: u64) -> Self { Self { sample, anchor: None } }

    /// Anchor a paused clock at finite nonnegative browser `now_ms`; invalid time
    /// or an already-running clock leaves state unchanged.
    pub fn start(
        &mut self,
        now_ms: f64,
    ) {
        if self.anchor.is_none() && now_ms.is_finite() && now_ms >= 0.0 {
            self.anchor = Some((now_ms, self.sample));
        }
    }

    /// Returns the latest playback milliseconds at browser `now_ms`, advancing by
    /// truncated elapsed milliseconds with saturation. Paused, nonfinite or backward
    /// observations retain the previous sample instead of reversing playback.
    pub fn sample(
        &mut self,
        now_ms: f64,
    ) -> u64 {
        if let Some((anchor, base)) = self.anchor
            && now_ms.is_finite()
            && now_ms >= anchor
        {
            self.sample = self.sample.max(base.saturating_add((now_ms - anchor) as u64));
        }
        self.sample
    }

    /// Sample browser `now_ms`, clear the running anchor and return playback milliseconds.
    /// Invalid/backward time preserves the last sample but still pauses playback.
    pub fn pause(
        &mut self,
        now_ms: f64,
    ) -> u64 {
        let sample = self.sample(now_ms);
        self.anchor = None;
        sample
    }
}

#[cfg(test)]
/// Counter-exhaustion regressions using private state to reach otherwise impractical limits.
mod tests {
    use super::*;

    /// Returns a valid one-widget trace at input/submission/opportunity times 1/2/3 ms.
    fn sample() -> TraceSample {
        TraceSample {
            input_ms: 1.0,
            canvas_ms: 2.0,
            presentation_boundary_ms: 3.0,
            render_ms: 1.0,
            widgets: 1,
            views: 1,
            warning_preview: false,
        }
    }

    #[test]
    /// Assert a last live permit survives run-ID exhaustion, while all new starts fail.
    fn timing_last_valid_permit_finishes_after_run_id_exhaustion() {
        let mut trace = TraceCapture::default();
        trace.start(0.0).unwrap();
        trace.run = u64::MAX;
        let mut permit = trace.begin(1.0).unwrap();
        assert_eq!(
            trace.start(2.0),
            Err(TraceError::Exhausted(TraceExhaustion::RunIdentity))
        );
        assert_eq!(trace.status(), TraceStatus::Exhausted(TraceExhaustion::RunIdentity));
        assert!(trace.begin(1.0).is_err());
        trace.finish(&mut permit, sample()).unwrap();
        assert_eq!(trace.in_flight(), 0);
        assert_eq!(trace.samples().len(), 1);
        assert!(trace.start(4.0).is_err());
    }

    #[test]
    /// Assert in-flight saturation closes admission but still permits prior completion.
    fn timing_last_in_flight_slot_finishes_after_checked_admission_exhaustion() {
        let mut trace = TraceCapture::default();
        trace.start(0.0).unwrap();
        trace.in_flight = u32::MAX - 1;
        let mut permit = trace.begin(1.0).unwrap();
        assert_eq!(trace.in_flight(), u32::MAX);
        assert_eq!(
            trace.begin(1.0).unwrap_err(),
            TraceError::Exhausted(TraceExhaustion::InFlight)
        );
        trace.finish(&mut permit, sample()).unwrap();
        assert_eq!(trace.in_flight(), u32::MAX - 1);
        assert_eq!(trace.status(), TraceStatus::Exhausted(TraceExhaustion::InFlight));
    }

    #[test]
    /// Assert owner allocation stops before wrap and never aliases an old owner.
    fn timing_private_owner_issuer_exhaustion_never_wraps_or_mints_aliases() {
        let issuer = AtomicUsize::new(usize::MAX - 1);
        let live = TraceCapture::with_issuer(&issuer);
        assert_eq!(live.status(), TraceStatus::Stopped);
        let mut exhausted = TraceCapture::with_issuer(&issuer);
        assert_eq!(
            exhausted.start(0.0),
            Err(TraceError::Exhausted(TraceExhaustion::OwnerIdentity))
        );
        assert_eq!(issuer.load(Ordering::Relaxed), usize::MAX);
    }
}
