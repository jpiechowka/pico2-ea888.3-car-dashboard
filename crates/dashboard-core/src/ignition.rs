//! Four independent signed ignition-correction observations, with no ECU producer.
//! The degree range is a display representation bound, not a vehicle safety limit.
use crate::samples::{DataMode, Quality, SampleError, Source};

/// Inclusive local presentation age in milliseconds; not a qualified polling rate.
pub const FRESHNESS_MS: u64 = 500;

/// Cylinder number in display order; neither firing order nor physical position.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Cylinder {
    /// Top-left cylinder number one.
    One,
    /// Top-right cylinder number two.
    Two,
    /// Bottom-left cylinder number three.
    Three,
    /// Bottom-right cylinder number four.
    Four,
}
impl Cylinder {
    /// Complete stable order for the four independently qualified observations.
    pub const ALL: [Self; 4] = [Self::One, Self::Two, Self::Three, Self::Four];

    /// Returns the bounded zero-based observation index in `0..4`.
    pub const fn index(self) -> usize { self as usize }
}

/// Compact admitted provenance; no variant represents a qualified vehicle source.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum IgnitionSource {
    /// No accepted synthetic candidate yet.
    Unknown,
    /// Synthetic input admitted only while the owner is in Demo mode.
    Demo,
}

/// Unvalidated candidate; admission independently checks source, order and degrees.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct IgnitionSample {
    /// Independent destination cylinder number.
    pub cylinder: Cylinder,
    /// Signed correction degrees; admitted finite range is `-327.68..=327.67`.
    pub degrees: f32,
    /// Original receive/capture time in monotonic milliseconds, including zero.
    pub received_at_ms: u64,
    /// Nonzero strictly increasing per-cylinder producer sequence.
    pub sequence: u64,
    /// Candidate provenance; only `Source::Demo` in `DataMode::Demo` is admitted.
    pub source: Source,
}

/// Fixed 32-byte state: last-good value plus separate status and ordering metadata.
#[derive(Clone, Copy, Debug, PartialEq)]
struct CylinderObservation {
    /// Last-good monotonic receive milliseconds, meaningful only with `has_value`.
    received_at_ms: u64,
    /// Last accepted candidate/fault time; clock reversal never rewinds this field.
    status_at_ms: u64,
    /// Last consumed typed candidate sequence, including numeric failures.
    sequence: u64,
    /// Last-good finite signed degrees; inaccessible without value qualification.
    degrees: f32,
    /// Stored status; fresh values additionally require a checked current age.
    quality: Quality,
    /// Compact source retained through numeric failures and explicit faults.
    source: IgnitionSource,
    /// Explicit presence preserves receive time zero and the full u64 range.
    has_value: bool,
}
impl CylinderObservation {
    /// Returns empty storage whose status reflects explicit `mode` availability.
    const fn empty(mode: DataMode) -> Self {
        Self {
            received_at_ms: 0,
            status_at_ms: 0,
            sequence: 0,
            degrees: 0.0,
            quality: match mode {
                DataMode::Demo => Quality::Unknown,
                DataMode::Vehicle => Quality::Unsupported,
            },
            source: IgnitionSource::Unknown,
            has_value: false,
        }
    }
}

/// Coherent 128-byte cylinder group; readers cannot mutate or fabricate observations.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct IgnitionSnapshot {
    /// Independent observations in cylinder-number order, copied with acquisition state.
    observations: [CylinderObservation; 4],
}
impl IgnitionSnapshot {
    /// Vehicle absence, with no source, measurement or implicit zero reading.
    pub const EMPTY: Self = Self::for_mode(DataMode::Vehicle);

    /// Returns empty group storage for an owner's immutable source `mode`.
    pub(crate) const fn for_mode(mode: DataMode) -> Self {
        Self {
            observations: [CylinderObservation::empty(mode); 4],
        }
    }

    /// Returns signed degrees for `cylinder` only in Demo `mode` with admitted
    /// provenance and valid checked age at monotonic `now_ms`; otherwise `None`.
    pub fn value_at(
        &self,
        cylinder: Cylinder,
        mode: DataMode,
        now_ms: u64,
    ) -> Option<f32> {
        let observation = self.observations[cylinder.index()];
        (mode == DataMode::Demo
            && observation.source == IgnitionSource::Demo
            && self.quality_at(cylinder, now_ms) == Quality::Valid)
            .then_some(observation.degrees)
    }

    /// Returns effective `cylinder` quality at monotonic `now_ms` milliseconds.
    /// Valid values expire after 500 ms inclusive; a clock before receive/status
    /// time is InvalidPayload. Reads never refresh or mutate retained observations.
    pub fn quality_at(
        &self,
        cylinder: Cylinder,
        now_ms: u64,
    ) -> Quality {
        let observation = self.observations[cylinder.index()];
        if observation.quality != Quality::Valid {
            return observation.quality;
        }
        if !observation.has_value || now_ms < observation.status_at_ms {
            return Quality::InvalidPayload;
        }
        match now_ms.checked_sub(observation.received_at_ms) {
            None => Quality::InvalidPayload,
            Some(age) if age > FRESHNESS_MS => Quality::Stale,
            Some(_) => Quality::Valid,
        }
    }

    /// Returns admitted provenance for `cylinder`, retained even during faults.
    pub const fn source(
        &self,
        cylinder: Cylinder,
    ) -> IgnitionSource {
        self.observations[cylinder.index()].source
    }

    /// Returns last-good receive milliseconds for `cylinder`, or None before a
    /// good sample. Faults and statistics resets do not refresh this timestamp.
    pub const fn received_at_ms(
        &self,
        cylinder: Cylinder,
    ) -> Option<u64> {
        let observation = self.observations[cylinder.index()];
        if observation.has_value {
            Some(observation.received_at_ms)
        } else {
            None
        }
    }

    /// Returns last candidate/fault milliseconds for `cylinder`; zero is also the
    /// initial timestamp and does not imply that a measurement exists.
    pub const fn status_at_ms(
        &self,
        cylinder: Cylinder,
    ) -> u64 {
        self.observations[cylinder.index()].status_at_ms
    }

    /// Returns the last consumed candidate sequence for `cylinder`; zero means none.
    pub const fn sequence(
        &self,
        cylinder: Cylinder,
    ) -> u64 {
        self.observations[cylinder.index()].sequence
    }

    /// Checks `sample` against `mode`, monotonic `now_ms` and retained identity.
    /// Returns source/future/order errors without mutation; numeric checking is
    /// separate because a typed numeric failure must publish a fault.
    pub(crate) fn check_sample(
        &self,
        sample: IgnitionSample,
        mode: DataMode,
        now_ms: u64,
    ) -> Result<(), SampleError> {
        let previous = self.observations[sample.cylinder.index()];
        if sample.received_at_ms > now_ms {
            return Err(SampleError::Future);
        }
        if mode != DataMode::Demo || sample.source != Source::Demo {
            return Err(SampleError::Source);
        }
        if sample.sequence <= previous.sequence
            || (previous.has_value && sample.received_at_ms <= previous.received_at_ms)
            || sample.received_at_ms < previous.status_at_ms
        {
            return Err(SampleError::OutOfOrder);
        }
        Ok(())
    }

    /// Consumes already source/time/order-checked `sample` after outer counter
    /// preflight. Returns numeric validity; invalid degrees hide but retain the
    /// last-good value/time and consume the candidate's sequence/status timestamp.
    pub(crate) fn accept(
        &mut self,
        sample: IgnitionSample,
    ) -> bool {
        let observation = &mut self.observations[sample.cylinder.index()];
        let valid = valid_degrees(sample.degrees);
        if valid {
            observation.degrees = sample.degrees;
            observation.received_at_ms = sample.received_at_ms;
            observation.has_value = true;
        }
        observation.status_at_ms = sample.received_at_ms;
        observation.sequence = sample.sequence;
        observation.source = IgnitionSource::Demo;
        observation.quality = if valid { Quality::Valid } else { Quality::InvalidPayload };
        valid
    }

    /// Returns whether non-Valid/non-Stale `quality` changes `cylinder` at
    /// non-regressed `at_ms`. Used before outer sequence preflight and mutation.
    pub(crate) fn accepts_fault(
        &self,
        cylinder: Cylinder,
        quality: Quality,
        at_ms: u64,
    ) -> bool {
        let previous = self.observations[cylinder.index()];
        !matches!(quality, Quality::Valid | Quality::Stale)
            && at_ms >= previous.status_at_ms
            && (previous.quality != quality || previous.status_at_ms != at_ms)
    }

    /// Applies previously checked fault `quality` to `cylinder` at `at_ms`
    /// milliseconds. Caller preflights outer sequence; last-good source/time stay.
    pub(crate) fn fault(
        &mut self,
        cylinder: Cylinder,
        quality: Quality,
        at_ms: u64,
    ) {
        let observation = &mut self.observations[cylinder.index()];
        observation.quality = quality;
        observation.status_at_ms = at_ms;
    }

    /// Returns whether any stored valid observation needs sticky reversal invalidation.
    pub(crate) fn has_valid_observation(&self) -> bool {
        self.observations
            .iter()
            .any(|observation| observation.quality == Quality::Valid)
    }

    /// Invalidates stored valid samples after outer counter preflight on owner
    /// reversal. `watermark_ms` is the greatest previous owner time; no status
    /// rewinds and old samples cannot revive merely as the reader clock catches up.
    pub(crate) fn invalidate_clock(
        &mut self,
        watermark_ms: u64,
    ) {
        for observation in &mut self.observations {
            if observation.quality == Quality::Valid {
                observation.quality = Quality::InvalidPayload;
                observation.status_at_ms = observation.status_at_ms.max(watermark_ms);
            }
        }
    }
}

/// Returns whether `degrees` is finite and fits the signed hundredth-degree
/// representation range. This neither decodes a PID nor qualifies vehicle support.
pub(crate) fn valid_degrees(degrees: f32) -> bool {
    let (minimum, maximum) = dashboard_config::WidgetChannel::IgnitionRetard.domain();
    degrees.is_finite() && (minimum..=maximum).contains(&degrees)
}

/// Preserves the agreed single-observation layout on host, Wasm and ARM.
const _: () = assert!(core::mem::size_of::<CylinderObservation>() == 32);
/// Preserves the complete publication/owner allocation budget for all four cylinders.
const _: () = assert!(core::mem::size_of::<IgnitionSnapshot>() == 128);
