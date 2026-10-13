//! Monotonic playback and bounded browser CPU/presentation-opportunity timing.
use std::sync::atomic::{AtomicUsize, Ordering};

use serde::Serialize;

use crate::events::DiagnosticCount;

static NEXT_TRACE_OWNER: AtomicUsize = AtomicUsize::new(1);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum TraceExhaustion {
    OwnerIdentity,
    RunIdentity,
    InFlight,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum TraceStatus {
    Stopped,
    Recording,
    Exhausted(TraceExhaustion),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TraceError {
    Exhausted(TraceExhaustion),
    OutsideRun,
    InvalidTime,
    StalePermit,
}

/// Private owner/run capability. Finish or abandon exactly once; never clone it.
#[derive(Debug)]
pub struct TracePermit {
    owner: usize,
    run: u64,
    input_ms: f64,
    consumed: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct TraceSample {
    pub input_ms: f64,
    pub canvas_ms: f64,
    pub presentation_boundary_ms: f64,
    pub render_ms: f64,
    pub widgets: usize,
    pub views: usize,
    pub warning_preview: bool,
}

/// Bounded browser CPU/opportunity evidence. Serialization borrows data and
/// cannot duplicate the live owner; there is deliberately no Clone/Deserialize.
#[derive(Serialize)]
pub struct TraceCapture {
    #[serde(skip)]
    owner: Option<usize>,
    #[serde(rename = "id")]
    run: u64,
    status: TraceStatus,
    start_ms: f64,
    end_ms: f64,
    high_water_ms: f64,
    in_flight: u32,
    samples: Vec<TraceSample>,
    long_tasks: Vec<(f64, f64)>,
    dropped: DiagnosticCount,
}

impl Default for TraceCapture {
    fn default() -> Self { Self::with_issuer(&NEXT_TRACE_OWNER) }
}

impl TraceCapture {
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

    pub fn status(&self) -> TraceStatus { self.status }

    pub fn in_flight(&self) -> u32 { self.in_flight }

    pub fn samples(&self) -> &[TraceSample] { &self.samples }

    pub fn long_tasks(&self) -> &[(f64, f64)] { &self.long_tasks }

    pub fn dropped(&self) -> DiagnosticCount { self.dropped }

    fn check_status(&self) -> Result<(), TraceError> {
        match self.status {
            TraceStatus::Exhausted(cause) => Err(TraceError::Exhausted(cause)),
            _ => Ok(()),
        }
    }

    fn exhaust(
        &mut self,
        cause: TraceExhaustion,
    ) -> TraceError {
        self.status = TraceStatus::Exhausted(cause);
        self.end_ms = self.high_water_ms;
        TraceError::Exhausted(cause)
    }

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

    fn includes(
        &self,
        time: f64,
    ) -> bool {
        self.run != 0
            && time.is_finite()
            && time >= self.start_ms
            && (self.status == TraceStatus::Recording || time <= self.end_ms)
    }

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
    pub fn abandon(
        &mut self,
        permit: &mut TracePermit,
    ) -> Result<(), TraceError> {
        self.complete(permit)
    }

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
pub struct Playback {
    sample: u64,
    anchor: Option<(f64, u64)>,
}

impl Playback {
    pub const fn new(sample: u64) -> Self { Self { sample, anchor: None } }

    pub fn start(
        &mut self,
        now_ms: f64,
    ) {
        if self.anchor.is_none() && now_ms.is_finite() && now_ms >= 0.0 {
            self.anchor = Some((now_ms, self.sample));
        }
    }

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
mod tests {
    use super::*;

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
