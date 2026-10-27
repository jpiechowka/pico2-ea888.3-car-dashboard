//! Single-owner measurement statistics, published as bounded coherent snapshots.
use core::num::NonZeroU32;
use core::sync::atomic::{AtomicU32, Ordering};

use dashboard_config::{SourceResolution, ValidatedConfig, resolve_source};

use crate::alarms::{
    AlarmEngine,
    Alerts,
    Context,
    Inputs,
    RuleAlerts,
    RuleEngineState,
    RuleIndex,
    Status,
    demo_contexts_for,
    demo_rules,
};
use crate::cache_diagnostics::{CacheExhaustionCause, CacheExhaustionStatus};
use crate::ignition::{Cylinder, IgnitionSample, IgnitionSnapshot, valid_degrees};
use crate::peak_hold::{PeakHolds, PeakSnapshot};
pub use crate::samples::SensorId;
use crate::samples::{DataMode, Quality, Sample, SampleError, SampleMeta, Source};
use crate::sensor_state::{PeakMode, SensorSnapshot, SensorState};
use crate::warning_state::{CommandResult, WarningCommand, WarningEligibility};

#[cfg(not(target_has_atomic = "32"))]
compile_error!("graph cache identities require 32-bit atomics");
/// Process-wide non-reusing run-origin issuer; relaxed atomics provide uniqueness only.
struct OriginIssuer(
    /// Next origin to issue; `u32::MAX` is permanent exhaustion.
    AtomicU32,
);
impl OriginIssuer {
    /// Returns an issuer starting at nonzero `next`; internal callers must not
    /// supply zero because a successful issuance asserts a nonzero identity.
    const fn new(next: u32) -> Self { Self(AtomicU32::new(next)) }

    /// Returns a unique origin and atomically advances the next value, or `Err(())`
    /// at sticky exhaustion. Panics if constructed at zero; does not synchronize data.
    fn issue(&self) -> Result<NonZeroU32, ()> {
        self.0
            .try_update(Ordering::Relaxed, Ordering::Relaxed, |next| next.checked_add(1))
            .map(|next| NonZeroU32::new(next).expect("issuer starts at one"))
            .map_err(|_| ())
    }
}
/// Shared issuer across all borrowed/owned acquisition models in this process/boot.
static ORIGIN_ISSUER: OriginIssuer = OriginIssuer::new(1);

/// Number of canonical channels, indexed in [`SensorId::ALL`] order.
pub const SENSOR_COUNT: usize = 8;
/// Per-channel presentation age caps in milliseconds, copied from shared schema.
const DEFAULT_FRESHNESS: [u16; SENSOR_COUNT] = {
    let mut caps = [0; SENSOR_COUNT];
    let mut i = 0;
    while i < SENSOR_COUNT {
        caps[i] = SensorId::ALL[i].channel().freshness_cap();
        i += 1;
    }
    caps
};
/// Empty channel statistics, with channel-specific trend thresholds for EGT/volts/lambda.
const SENSOR_BASE: [SensorState; SENSOR_COUNT] = [
    SensorState::new(),
    SensorState::new(),
    SensorState::new(),
    SensorState::new(),
    SensorState::new(),
    SensorState::with_trend_threshold(5.0),
    SensorState::with_trend_threshold(0.05),
    SensorState::with_trend_threshold(0.1),
];

/// UI-side reset identity tracking; only the newest request can acknowledge success.
#[derive(Default)]
pub struct ResetRequests {
    /// Greatest issued nonzero reset request ID; zero before first request.
    requested: u64,
    /// Greatest accepted acknowledgment, initially zero.
    acknowledged: u64,
}

impl ResetRequests {
    /// Returns a fresh request ID and records it as newest; panics before mutation
    /// when the counter is exhausted, preventing reset-identity reuse.
    pub fn request(&mut self) -> u64 {
        self.requested = self.requested.checked_add(1).expect("reset generation exhausted");
        self.requested
    }

    /// Show success once, only after the latest actual request is acknowledged.
    /// Returns true for exact latest `generation` not already acknowledged;
    /// stale, zero, future or duplicate replies return false without mutation.
    pub fn acknowledge(
        &mut self,
        generation: u64,
    ) -> bool {
        if generation != self.requested || generation <= self.acknowledged {
            return false;
        }
        self.acknowledged = generation;
        true
    }
}

/// Private origin and checked counter; absence disables transport deduplication.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PublicationIdentity {
    /// Non-reused acquisition origin, ordering independent owner instances.
    origin: NonZeroU32,
    /// Nonzero checked publication revision, independent of raw sample sequence.
    revision: core::num::NonZeroU64,
    /// Exhaustion causes participate even if their change did not advance publication.
    diagnostics: CacheExhaustionStatus,
}

/// Coherent acquisition publication; consumers evaluate freshness at their own
/// monotonic time and never advance producer warning dwell by redrawing.
#[derive(Clone, Copy, Debug)]
pub struct DashboardSnapshot {
    /// Independently qualified synthetic cylinder corrections; Vehicle is unsupported.
    pub ignition: IgnitionSnapshot,
    /// Last-good canonical values/statistics by channel; not inherently fresh.
    pub sensors: [SensorSnapshot; SENSOR_COUNT],
    /// Source, sample identity and quality corresponding to each channel's statistics.
    pub metadata: [SampleMeta; SENSOR_COUNT],
    /// Explicit vehicle/synthetic source admission mode.
    pub mode: DataMode,
    /// Checked raw sample/fault publication count; timing-only changes do not advance it.
    pub sequence: u64,
    /// Latest applied statistics-reset request, zero before any reset.
    pub reset_generation: u64,
    /// Legacy per-channel alarm state; disabled for configured owners.
    pub(crate) alerts: Alerts,
    /// Configured per-widget episodes and command acknowledgment state.
    pub(crate) rule_alerts: RuleAlerts,
    /// Configured sample age limits in milliseconds, additionally capped per channel.
    pub(crate) freshness: [u16; SENSOR_COUNT],
    /// Bounded union of exhausted identity domains affecting this publication.
    cache_exhaustion: CacheExhaustionStatus,
    /// Unique run origin, absent after issuer exhaustion or on EMPTY.
    origin: Option<NonZeroU32>,
    /// Checked publication revision; zero is sticky exhausted/untracked sentinel.
    publication: u64,
    /// Per-widget held raw extrema; producer ticking owns expiry.
    peaks: PeakSnapshot,
}

impl DashboardSnapshot {
    /// Vehicle-mode absence with alarms disabled and no trusted transport/cache identity.
    pub const EMPTY: Self = Self {
        ignition: IgnitionSnapshot::EMPTY,
        sensors: [SensorSnapshot::EMPTY; SENSOR_COUNT],
        metadata: [SampleMeta::UNKNOWN; SENSOR_COUNT],
        mode: DataMode::Vehicle,
        sequence: 0,
        reset_generation: 0,
        alerts: Alerts::DISABLED,
        rule_alerts: RuleAlerts::DISABLED,
        freshness: DEFAULT_FRESHNESS,
        cache_exhaustion: CacheExhaustionStatus::EMPTY,
        origin: None,
        publication: 0,
        peaks: PeakSnapshot::EMPTY,
    };

    /// Transport equality includes diagnostic-only transitions without advancing
    /// the raw sample sequence (or warning dwell).
    /// Returns exact deduplication proof, or `None` without an origin/nonzero revision.
    pub fn publication_identity(&self) -> Option<PublicationIdentity> {
        Some(PublicationIdentity {
            origin: self.origin?,
            revision: core::num::NonZeroU64::new(self.publication)?,
            diagnostics: self.cache_exhaustion,
        })
    }

    /// Independent of measurement sequence and reset generation. None means
    /// exhaustion: publish/accept conservatively forever, without ID reuse.
    /// Returns the nonzero revision when available, including the final `u64::MAX` value.
    pub fn publication_revision(&self) -> Option<u64> { core::num::NonZeroU64::new(self.publication).map(|r| r.get()) }

    /// Returns acquisition-run identity for warning commands, or `None` when untracked.
    pub fn warning_run_id(&self) -> Option<u64> { self.origin.map(|v| u64::from(v.get())) }

    /// MAX is terminal identity exhaustion; acknowledgment is disabled.
    /// Returns the configured engine's event watermark; zero precedes any activation.
    pub const fn warning_event_serial(&self) -> u64 { self.rule_alerts.event_serial }

    /// Returns the last configured warning-command result, or EMPTY before any result.
    pub const fn warning_command_result(&self) -> CommandResult { self.rule_alerts.result }

    /// Returns effective raw status and remembered episode flags for widget
    /// `index` at monotonic `now` milliseconds; does not expire producer snoozes.
    pub fn warning_eligibility(
        &self,
        index: RuleIndex,
        now: u64,
    ) -> WarningEligibility {
        self.rule_alerts.eligibility(index, self, now)
    }

    /// Returns fresh active cylinder bits for grouped rule `index` at `now`
    /// milliseconds. Scalar/disabled/pending/unavailable rules return zero.
    pub fn ignition_warning_mask_at(
        &self,
        index: RuleIndex,
        now: u64,
    ) -> u8 {
        self.rule_alerts.ignition_mask_at(index, self, now)
    }

    /// Returns remembered cylinder membership of a latched grouped `index`,
    /// including unavailable members. These bits never qualify a live value.
    pub fn ignition_warning_latched_mask(
        &self,
        index: RuleIndex,
    ) -> u8 {
        self.rule_alerts.ignition_latched_mask(index)
    }

    /// Returns held canonical extremum for widget `index` only if its channel
    /// is currently value-qualified at `now` milliseconds; otherwise `None`.
    /// Producer tick owns hold deadlines; this read does not recompute their expiry.
    pub fn peak_at(
        &self,
        index: RuleIndex,
        now: u64,
    ) -> Option<f32> {
        self.value_at(self.rule_alerts.channel(index)?, now)?;
        self.peaks.value(index)
    }

    /// Returns raw held extrema/presence; callers separately apply channel freshness.
    pub const fn peaks(&self) -> &PeakSnapshot { &self.peaks }

    /// Returns bounded exhaustion metadata without changing rendering correctness.
    pub const fn cache_exhaustion_status(&self) -> CacheExhaustionStatus { self.cache_exhaustion }

    /// Returns legacy alerts qualified at `now_ms` milliseconds, suppressing
    /// stale/future/unavailable samples without advancing alarm dwell.
    pub fn alerts_at(
        &self,
        now_ms: u64,
    ) -> Alerts {
        self.alerts.at(self, now_ms)
    }

    /// Returns raw configured status for widget `index` at `now_ms` milliseconds;
    /// expired/reversed/unavailable data becomes Unavailable, disabled stays Disabled.
    pub fn rule_status_at(
        &self,
        index: RuleIndex,
        now_ms: u64,
    ) -> Status {
        self.rule_alerts.status_at(index, self, now_ms)
    }

    /// Returns whether widget `index` has qualified raw Active status at `now_ms`
    /// milliseconds; remembered latches alone do not count as active measurements.
    pub fn rule_active_at(
        &self,
        index: RuleIndex,
        now_ms: u64,
    ) -> bool {
        self.rule_status_at(index, now_ms) == Status::Active
    }

    /// Returns whether legacy channel `id` is raw Active at `now_ms` milliseconds.
    pub fn alarm_active_at(
        &self,
        id: SensorId,
        now_ms: u64,
    ) -> bool {
        self.alerts.status_at(self, id, now_ms) == crate::alarms::Status::Active
    }

    /// Returns effective quality for channel `id` at monotonic `now_ms`
    /// milliseconds using configured and channel age caps; does not validate provenance.
    pub fn quality(
        &self,
        id: SensorId,
        now_ms: u64,
    ) -> Quality {
        self.metadata[id as usize].quality_with_freshness(id, now_ms, u64::from(self.freshness[id as usize]))
    }

    /// Returns canonical value for `id` at monotonic `now_ms` milliseconds only
    /// when finite, source/mode-qualified, timestamp-consistent and fresh; else `None`.
    pub fn value_at(
        &self,
        id: SensorId,
        now_ms: u64,
    ) -> Option<f32> {
        let sensor = self.sensor(id);
        self.metadata[id as usize].value_with_freshness(
            id,
            self.mode,
            sensor.value(),
            sensor.sampled_at_ms(),
            now_ms,
            u64::from(self.freshness[id as usize]),
        )
    }

    /// Returns retained statistics for `id`; use [`Self::value_at`] for live values.
    pub fn sensor(
        &self,
        id: SensorId,
    ) -> &SensorSnapshot {
        &self.sensors[id as usize]
    }

    /// Replaces this snapshot with `incoming` and returns true unless both have
    /// identity proof and incoming is older or exactly duplicate. Equal revision
    /// with changed diagnostic metadata is accepted. Missing proof on either side
    /// conservatively accepts, so exhaustion never freezes state or reuses an ID.
    pub fn accept(
        &mut self,
        incoming: Self,
    ) -> bool {
        if let (Some(current), Some(next)) = (self.publication_identity(), incoming.publication_identity())
            && (next.origin < current.origin
                || (next.origin == current.origin && (next.revision < current.revision || next == current)))
        {
            return false;
        }
        *self = incoming;
        true
    }
}

// Runtime transport proof, like graph proof, is not semantic snapshot data.
impl PartialEq for DashboardSnapshot {
    /// Returns semantic equality with `other`, excluding origin/publication proofs
    /// but including quality, exhaustion metadata, alarms, held peaks and statistics.
    fn eq(
        &self,
        other: &Self,
    ) -> bool {
        self.ignition == other.ignition
            && self.sensors == other.sensors
            && self.metadata == other.metadata
            && self.mode == other.mode
            && self.sequence == other.sequence
            && self.reset_generation == other.reset_generation
            && self.alerts == other.alerts
            && self.rule_alerts == other.rule_alerts
            && self.freshness == other.freshness
            && self.cache_exhaustion == other.cache_exhaustion
            && self.peaks == other.peaks
    }
}

// Alternatives share storage; boxing is unavailable on the MCU. Legacy is
// retained solely for existing protocol/pixel replay fixtures.
#[allow(clippy::large_enum_variant)]
/// Exclusive policy variant; inline storage avoids allocation on firmware.
enum Alarms {
    /// Historical per-channel replay/demo policy.
    Legacy(
        /// Mutable engine with immutable built-in/source-scoped rules.
        AlarmEngine,
    ),
    /// Per-widget policy bound to the enclosing owner's validated configuration.
    Configured(
        /// Mutable episode state; its configuration handle lives in the owner.
        RuleEngineState,
    ),
}
impl Alarms {
    /// Returns whether any policy slot is enabled, independent of source availability.
    fn enabled(&self) -> bool {
        match self {
            Self::Legacy(e) => e.enabled(),
            Self::Configured(e) => e.enabled(),
        }
    }

    /// Applies source/time-checked `inputs`, verified optional `contexts`, and
    /// monotonic `now` milliseconds to the active variant. `config` must be the
    /// immutable configured handle; panics if absent for a configured engine.
    /// `ignition` carries independent group observations and outer raw sequence;
    /// legacy policy ignores it, while configured group rules qualify it separately.
    fn update_inputs(
        &mut self,
        config: Option<&ValidatedConfig>,
        inputs: &Inputs<'_>,
        contexts: &[Option<Context>; 8],
        now: u64,
        ignition: (&IgnitionSnapshot, u64),
    ) {
        match self {
            Self::Legacy(e) => {
                e.update_inputs(inputs, contexts, now);
            }
            Self::Configured(e) => {
                e.update_inputs(
                    config.expect("configured policy has immutable handle"),
                    inputs,
                    contexts,
                    now,
                    ignition,
                )
            }
        }
    }
}

/// Indivisible mutable state owned alongside an immutable configuration handle.
struct AcquisitionState {
    /// Single-owner independent cylinder observations, without history or scalar alarms.
    ignition: IgnitionSnapshot,
    /// Single-owner per-channel statistics and sampling rings.
    sensors: [SensorState; SENSOR_COUNT],
    /// Per-channel accepted provenance, ordering and quality metadata.
    metadata: [SampleMeta; SENSOR_COUNT],
    /// Explicit source-admission mode, fixed during owner construction.
    mode: DataMode,
    /// Checked raw sample/fault publication counter; panic at exhaustion.
    sequence: u64,
    /// Greatest applied statistics-reset request ID.
    reset_generation: u64,
    /// Legacy or configured mutable policy storage, fixed during construction.
    alarms: Alarms,
    /// Optional verified operating context attached to each channel's latest sample.
    contexts: [Option<Context>; SENSOR_COUNT],
    /// Exact configured source per channel; `None` disables typed configured admission.
    expected_sources: [Option<Source>; SENSOR_COUNT],
    /// Configured sample freshness milliseconds, bounded again at read time.
    freshness: [u16; SENSOR_COUNT],
    /// Non-reused acquisition identity; absent permanently after issuer exhaustion.
    origin: Option<NonZeroU32>,
    /// Checked publication revision, with sticky zero on exhaustion.
    publication: u64,
    /// Raw per-widget extrema retained before snapshot coalescing.
    peaks: PeakHolds,
    /// Greatest observed mutation/tick time in milliseconds for peak reversal handling.
    last_ms: u64,
}

impl AcquisitionState {
    /// Flash-constructible configured storage before immutable policy initialization.
    const CONFIGURED_BASE: Self = Self::base(Alarms::Configured(RuleEngineState::EMPTY));
    /// Flash-constructible legacy vehicle storage, with all alarms disabled.
    const LEGACY_BASE: Self = Self::base(Alarms::Legacy(AlarmEngine::DISABLED));

    // Valid flash-backed construction storage. No origin or dynamic policy work
    // occurs until the enclosing owner has installed its immutable handle.
    /// Returns empty construction storage with selected `alarms` variant; no origin
    /// is issued until the enclosing immutable configuration binding is installed.
    const fn base(alarms: Alarms) -> Self {
        Self {
            ignition: IgnitionSnapshot::EMPTY,
            sensors: SENSOR_BASE,
            metadata: [SampleMeta::UNKNOWN; SENSOR_COUNT],
            mode: DataMode::Vehicle,
            sequence: 0,
            reset_generation: 0,
            alarms,
            contexts: [None; SENSOR_COUNT],
            expected_sources: [None; SENSOR_COUNT],
            freshness: DEFAULT_FRESHNESS,
            origin: None,
            publication: 1,
            peaks: PeakHolds::EMPTY,
            last_ms: 0,
        }
    }

    /// Advances transport publication revision; overflow becomes sticky zero so
    /// consumers fall back to unconditional acceptance without affecting samples.
    fn publish_change(&mut self) {
        // Zero is sticky exhaustion; every subsequent snapshot is untracked.
        if self.publication != 0 {
            self.publication = self.publication.checked_add(1).unwrap_or(0);
        }
    }

    /// Invalidates grouped policy support before new observations replace gap
    /// evidence. Untouched cylinders are qualified at `now` milliseconds. An
    /// optional typed `replacement` qualifies its prior support at the incoming
    /// capture time, preserving capture-contiguous evidence despite delivery lag.
    /// Caller must preflight all required counters before invoking this mutation.
    fn prepare_ignition(
        &mut self,
        now: u64,
        replacement: Option<(Cylinder, u64)>,
    ) {
        let Alarms::Configured(engine) = &mut self.alarms else {
            return;
        };
        let mut unavailable = 0u8;
        for cylinder in Cylinder::ALL {
            let at = replacement
                .filter(|(replaced, _)| *replaced == cylinder)
                .map_or(now, |(_, captured)| captured);
            if self.ignition.value_at(cylinder, self.mode, at).is_none() {
                unavailable |= 1 << cylinder.index();
            }
        }
        engine.invalidate_ignition_support(unavailable);
    }

    /// Invalidates existing history of channel `index` when its last-good sample
    /// is stale or future-dated relative to `now` milliseconds. Panics for index >=8;
    /// internal callers use the canonical channel arrays.
    fn prepare_history(
        &mut self,
        index: usize,
        now: u64,
    ) {
        if self.metadata[index].received_at_ms.is_some_and(|at| {
            now.checked_sub(at)
                .is_none_or(|age| age > u64::from(self.freshness[index]))
        }) && self.sensors[index].history().count() != 0
        {
            self.sensors[index].invalidate_history(now);
        }
    }

    /// Advances history/highlight/held-peak/snooze expiry at `now` milliseconds
    /// using immutable optional `config`; reversed time clears held peaks and
    /// invalidates alarm freshness and makes existing valid cylinder observations
    /// sticky InvalidPayload, without inventing measurements. Cylinder invalidation
    /// preflights the raw counter before any mutation and panics on exhaustion. Publication
    /// advances only for changed presentation state. Configured variant requires config.
    fn tick(
        &mut self,
        config: Option<&ValidatedConfig>,
        now: u64,
    ) {
        let mut changed = false;
        if now < self.last_ms {
            if self.ignition.has_valid_observation() {
                let next = self.sequence.checked_add(1).expect("measurement sequence exhausted");
                self.ignition.invalidate_clock(self.last_ms);
                self.sequence = next;
                changed = true;
            }
            changed |= self.peaks.snapshot().valid_mask() != 0;
            self.peaks.clear();
        }
        self.last_ms = self.last_ms.max(now);
        if let Some(config) = config {
            let fresh = core::array::from_fn(|i| {
                self.metadata[i].quality_with_freshness(SensorId::ALL[i], now, u64::from(self.freshness[i]))
                    == Quality::Valid
            });
            changed |= self.peaks.expire(config, fresh, now);
        }
        for i in 0..SENSOR_COUNT {
            let sensor = &self.sensors[i];
            let before = (
                sensor.history().newest_ordinal(),
                sensor.history().validity(),
                sensor.graph_revision(),
                sensor.is_new_peak,
            );
            self.prepare_history(i, now);
            self.sensors[i].tick(now);
            let sensor = &self.sensors[i];
            changed |= before
                != (
                    sensor.history().newest_ordinal(),
                    sensor.history().validity(),
                    sensor.graph_revision(),
                    sensor.is_new_peak,
                );
        }
        if changed {
            self.publish_change();
        }
        self.update_alarms(config, now);
        if let Alarms::Configured(engine) = &mut self.alarms {
            let before = engine.snapshot();
            engine.expire(now);
            if before != engine.snapshot() {
                self.publish_change();
            }
        }
    }

    // Build the final policy directly. Creating a legacy owner and replacing
    // its alarms kept several complete sampling-ring copies live on ARM.
    /// Returns fresh state in `mode` with exact `expected_sources`, millisecond
    /// `freshness` caps and chosen `alarms`. Issues one origin; exhaustion leaves
    /// identity absent while all numeric/state operations remain available.
    fn with_policy(
        mode: DataMode,
        expected_sources: [Option<Source>; SENSOR_COUNT],
        freshness: [u16; SENSOR_COUNT],
        alarms: Alarms,
    ) -> Self {
        let origin = ORIGIN_ISSUER.issue().ok();
        Self {
            ignition: IgnitionSnapshot::for_mode(mode),
            sensors: SENSOR_BASE,
            metadata: [SampleMeta::UNKNOWN; SENSOR_COUNT],
            mode,
            sequence: 0,
            reset_generation: 0,
            alarms,
            contexts: [None; SENSOR_COUNT],
            expected_sources,
            freshness,
            origin,
            publication: 1,
            peaks: PeakHolds::EMPTY,
            last_ms: 0,
        }
    }

    /// Admits typed `sample` at monotonic `now_ms` using immutable optional `config`,
    /// with no operating context. Returns the contextual admission result; rules
    /// requiring context remain unavailable rather than inferring engine conditions.
    fn ingest_sample(
        &mut self,
        config: Option<&ValidatedConfig>,
        sample: Sample,
        now_ms: u64,
    ) -> Result<(), SampleError> {
        self.ingest_contextual(config, sample, None, now_ms)
    }

    /// Context must come from verified operating signals. Absent context keeps
    /// contextual rules unavailable; no inference from voltage or lambda alone.
    /// Checks typed `sample` at `now_ms` milliseconds against immutable `config`
    /// and optional verified `context`. Returns future/source/order errors before
    /// mutation. Invalid numeric input consumes sample/status sequence, clears graph/
    /// peak validity and updates alarms, returning InvalidValue while retaining
    /// last-good values/statistics. Panics before mutation on raw sequence exhaustion.
    fn ingest_contextual(
        &mut self,
        config: Option<&ValidatedConfig>,
        sample: Sample,
        context: Option<Context>,
        now_ms: u64,
    ) -> Result<(), SampleError> {
        let sensor = sample.value.sensor();
        let index = sensor as usize;
        let previous = self.metadata[index];
        if sample.received_at_ms > now_ms {
            return Err(SampleError::Future);
        }
        if !sample.source.matches(sensor, self.mode)
            || (config.is_some() && self.expected_sources[index] != Some(sample.source))
            || (previous.source != Source::Unknown && previous.source != sample.source)
        {
            return Err(SampleError::Source);
        }
        if sample.sequence <= previous.sequence
            || previous.received_at_ms.is_some_and(|at| sample.received_at_ms <= at)
            || sample.received_at_ms < previous.status_at_ms
        {
            return Err(SampleError::OutOfOrder);
        }
        let next = self.sequence.checked_add(1).expect("measurement sequence exhausted");
        let value = sample.value.value();
        // Shared statistics already reject nonfinite/unrepresentable values.
        // Protocol-specific ranges belong to the decoder, not an alarm rule.
        self.prepare_history(index, sample.received_at_ms);
        let valid = self.sensors[index].ingest(value, sample.received_at_ms, PEAK_MODES[index]);
        if !valid {
            self.sensors[index].invalidate_history(sample.received_at_ms);
        }
        self.metadata[index] = SampleMeta {
            received_at_ms: if valid {
                Some(sample.received_at_ms)
            } else {
                previous.received_at_ms
            },
            status_at_ms: sample.received_at_ms,
            sequence: sample.sequence,
            source: sample.source,
            quality: if valid { Quality::Valid } else { Quality::InvalidPayload },
        };
        self.sequence = next;
        self.publish_change();
        self.contexts[index] = context;
        if let Some(config) = config {
            if previous
                .received_at_ms
                .is_some_and(|at| sample.received_at_ms.saturating_sub(at) > u64::from(self.freshness[index]))
            {
                self.peaks.clear_channel(config, sensor);
            }
            if valid
                && now_ms >= self.last_ms
                && self.metadata[index].quality_with_freshness(sensor, now_ms, u64::from(self.freshness[index]))
                    == Quality::Valid
            {
                self.peaks.sample(config, sensor, value, sample.received_at_ms);
            } else {
                self.peaks.clear_channel(config, sensor);
            }
        }
        self.last_ms = self.last_ms.max(now_ms);
        self.update_alarms(config, now_ms);
        if valid { Ok(()) } else { Err(SampleError::InvalidValue) }
    }

    /// Publishes non-Valid/non-Stale `quality` for `sensor` at `at_ms` milliseconds,
    /// using immutable `config` to clear matching holds/update alarms. Returns true
    /// when changed; rejects regressed/identical status or unsupported quality.
    /// Retains last-good statistics/source; panics before mutation on sequence exhaustion.
    fn mark_fault(
        &mut self,
        config: Option<&ValidatedConfig>,
        sensor: SensorId,
        quality: Quality,
        at_ms: u64,
    ) -> bool {
        let meta = &mut self.metadata[sensor as usize];
        if matches!(quality, Quality::Valid | Quality::Stale)
            || at_ms < meta.status_at_ms
            || (meta.quality == quality && meta.status_at_ms == at_ms)
        {
            return false;
        }
        let next = self.sequence.checked_add(1).expect("measurement sequence exhausted");
        meta.quality = quality;
        meta.status_at_ms = at_ms;
        self.sequence = next;
        self.publish_change();
        self.sensors[sensor as usize].invalidate_history(at_ms);
        self.contexts[sensor as usize] = None;
        if let Some(config) = config {
            self.peaks.clear_channel(config, sensor);
        }
        self.last_ms = self.last_ms.max(at_ms);
        self.update_alarms(config, at_ms);
        true
    }

    /// Admits Demo `values` in canonical channel order at `captured_ms`, using
    /// immutable optional `config`. Returns whether any channel accepted a newer
    /// finite/bounded sample; Vehicle mode returns false. Invalid/stale entries
    /// leave their channel unchanged. Preflights global/all channel counters and
    /// panics on exhaustion before partial mutation, then updates synthetic contexts.
    fn ingest_batch(
        &mut self,
        config: Option<&ValidatedConfig>,
        values: [f32; SENSOR_COUNT],
        captured_ms: u64,
    ) -> bool {
        // Synthetic/demo fixture helper. Vehicle ingestion requires typed,
        // source-checked samples. The last canonical channel is lambda.
        if self.mode != DataMode::Demo {
            return false;
        }
        // Exhaustion is terminal, and must fail before partially changing data.
        let next_sequence = self.sequence.checked_add(1).expect("measurement sequence exhausted");
        let channel_sequences = self
            .metadata
            .map(|meta| meta.sequence.checked_add(1).expect("channel sequence exhausted"));
        let mut accepted = false;
        for i in 0..SENSOR_COUNT {
            if captured_ms > self.metadata[i].status_at_ms && crate::extrema::valid_sample(values[i]) {
                self.prepare_history(i, captured_ms);
            }
            let state = &mut self.sensors[i];
            let value = values[i];
            let mode = PEAK_MODES[i];
            if captured_ms >= self.metadata[i].status_at_ms && state.ingest(value, captured_ms, mode) {
                accepted = true;
                if let Some(config) = config {
                    if self.metadata[i]
                        .received_at_ms
                        .is_some_and(|at| captured_ms.saturating_sub(at) > u64::from(self.freshness[i]))
                    {
                        self.peaks.clear_channel(config, SensorId::ALL[i]);
                    }
                    if captured_ms >= self.last_ms {
                        self.peaks.sample(config, SensorId::ALL[i], value, captured_ms);
                    } else {
                        self.peaks.clear_channel(config, SensorId::ALL[i]);
                    }
                }
                self.metadata[i] = SampleMeta {
                    received_at_ms: Some(captured_ms),
                    status_at_ms: captured_ms,
                    sequence: channel_sequences[i],
                    source: Source::Demo,
                    quality: Quality::Valid,
                };
            }
        }
        if accepted {
            self.last_ms = self.last_ms.max(captured_ms);
            self.sequence = next_sequence;
            self.publish_change();
            self.contexts = demo_contexts_for(self.mode, &self.metadata);
            self.update_alarms(config, captured_ms);
        }
        accepted
    }

    /// Acknowledged in the next published snapshot. Replays cannot reset twice.
    /// Applies newer `generation`, clearing derived statistics and held peaks while
    /// retaining raw values/quality/source. Returns true when applied, false for
    /// zero/older/duplicate generations; alarm episode state is not reset.
    fn reset_statistics(
        &mut self,
        generation: u64,
    ) -> bool {
        if generation <= self.reset_generation {
            return false;
        }
        for state in &mut self.sensors {
            state.reset_statistics();
        }
        self.peaks.clear();
        self.reset_generation = generation;
        self.publish_change();
        true
    }

    /// Returns the union of issuer, graph and warning exhaustion plus immutable
    /// optional `config` identity status; reading does not restore or consume IDs.
    fn cache_exhaustion_status(
        &self,
        config: Option<&ValidatedConfig>,
    ) -> CacheExhaustionStatus {
        let mut status = config.map_or(CacheExhaustionStatus::EMPTY, |config| {
            CacheExhaustionStatus::from_config(config.cache_identity_status())
        });
        if self.origin.is_none() {
            status = status.union(CacheExhaustionStatus::from_cause(
                CacheExhaustionCause::AcquisitionIssuer,
            ));
        }
        if let Alarms::Configured(engine) = &self.alarms {
            status = status.union(engine.exhaustion_status());
        }
        for (index, sensor) in self.sensors.iter().enumerate() {
            if sensor.graph_revision().is_none() {
                status = status.union(CacheExhaustionStatus::from_cause(CacheExhaustionCause::GraphRevision(
                    SensorId::ALL[index].channel(),
                )));
            }
        }
        status
    }

    // Keep publication scratch out of the async acquisition poll frame, which
    // is also resident while the much larger Vehicle constructor runs.
    #[inline(never)]
    /// Returns a coherent copy with immutable optional `config` exhaustion metadata
    /// and exact graph proofs where available. Does not tick freshness or hold expiry.
    fn snapshot(
        &self,
        config: Option<&ValidatedConfig>,
    ) -> DashboardSnapshot {
        DashboardSnapshot {
            ignition: self.ignition,
            sensors: core::array::from_fn(|i| {
                self.sensors[i].tracked_snapshot(self.origin, SensorId::ALL[i].channel())
            }),
            metadata: self.metadata,
            mode: self.mode,
            sequence: self.sequence,
            reset_generation: self.reset_generation,
            alerts: match &self.alarms {
                Alarms::Legacy(e) => e.snapshot(),
                _ => Alerts::DISABLED,
            },
            rule_alerts: match &self.alarms {
                Alarms::Configured(e) => e.snapshot(),
                _ => RuleAlerts::DISABLED,
            },
            freshness: self.freshness,
            cache_exhaustion: self.cache_exhaustion_status(config),
            origin: self.origin,
            publication: self.publication,
            peaks: self.peaks.snapshot(),
        }
    }

    /// Re-evaluates all enabled policy inputs at `now_ms` milliseconds from current
    /// qualified values/contexts, using immutable `config` for configured rules.
    /// Advances publication only for changed alerts; repeated samples cannot advance dwell.
    fn update_alarms(
        &mut self,
        config: Option<&ValidatedConfig>,
        now_ms: u64,
    ) {
        if !self.alarms.enabled() {
            return;
        }
        let values = core::array::from_fn(|i| {
            self.metadata[i].value_with_freshness(
                SensorId::ALL[i],
                self.mode,
                self.sensors[i].value(),
                self.sensors[i].sampled_at_ms(),
                now_ms,
                u64::from(self.freshness[i]),
            )
        });
        let before = match &self.alarms {
            Alarms::Legacy(e) => (e.snapshot(), RuleAlerts::DISABLED),
            Alarms::Configured(e) => (Alerts::DISABLED, e.snapshot()),
        };
        self.alarms.update_inputs(
            config,
            &Inputs {
                metadata: &self.metadata,
                values,
                freshness: &self.freshness,
            },
            &self.contexts,
            now_ms,
            (&self.ignition, self.sequence),
        );
        let after = match &self.alarms {
            Alarms::Legacy(e) => (e.snapshot(), RuleAlerts::DISABLED),
            Alarms::Configured(e) => (Alerts::DISABLED, e.snapshot()),
        };
        if before != after {
            self.publish_change();
        }
    }
}

/// Restricts configuration handles to immutable borrows and owned validated boxes.
mod sealed {
    /// Private marker preventing externally switchable configuration providers.
    pub trait Sealed {}
    impl Sealed for &dashboard_config::ValidatedConfig {}
    #[cfg(feature = "alloc")]
    impl Sealed for alloc::boxed::Box<dashboard_config::ValidatedConfig> {}
}

/// An immutable configuration binding; only borrowed and boxed validated documents qualify.
pub trait ConfigurationHandle: sealed::Sealed {
    /// Returns the same immutable validated document for this handle's lifetime.
    fn configuration(&self) -> &ValidatedConfig;
}
impl ConfigurationHandle for &ValidatedConfig {
    /// Returns the borrowed validated document without copying or rebinding it.
    fn configuration(&self) -> &ValidatedConfig { self }
}
#[cfg(feature = "alloc")]
impl ConfigurationHandle for alloc::boxed::Box<ValidatedConfig> {
    /// Returns the boxed validated document as a shared borrow.
    fn configuration(&self) -> &ValidatedConfig { self }
}

/// Measurement state and its immutable configuration have one indivisible lifetime.
///
/// Storage cannot be named or constructed by applications.
/// ```compile_fail
/// use dashboard_core::acquisition::AcquisitionState;
/// ```
/// ```compile_fail
/// use dashboard_core::alarms::RuleEngineState;
/// ```
/// The config getter provides only an immutable borrow.
/// ```compile_fail
/// use dashboard_core::acquisition::AcquisitionModel;
/// let config = dashboard_config::validate(dashboard_config::default_config()).unwrap();
/// let mut owner = AcquisitionModel::demo_configured(&config);
/// let _: &mut dashboard_config::ValidatedConfig = owner.configuration().unwrap();
/// ```
/// ```compile_fail
/// use dashboard_core::acquisition::AcquisitionModel;
/// let mut owner = AcquisitionModel::demo();
/// let _ = &mut owner.state;
/// ```
/// Neither the configuration handle nor state can be replaced independently.
/// ```compile_fail
/// use dashboard_core::acquisition::AcquisitionModel;
/// let mut owner = AcquisitionModel::demo();
/// owner.configuration = None;
/// ```
/// External handles cannot switch configurations behind the owner.
/// ```compile_fail
/// use dashboard_core::acquisition::ConfigurationHandle;
/// struct Switching(dashboard_config::ValidatedConfig);
/// impl ConfigurationHandle for Switching {
///     fn configuration(&self) -> &dashboard_config::ValidatedConfig { &self.0 }
/// }
/// ```
/// No operation accepts a replacement configuration.
/// Construction phases are not application APIs.
/// ```compile_fail
/// use dashboard_core::acquisition::AcquisitionModel;
/// let _ = AcquisitionModel::session_base::<false>(None);
/// ```
/// ```compile_fail
/// use dashboard_core::acquisition::AcquisitionModel;
/// let mut owner = AcquisitionModel::new();
/// owner.initialize_for_session(0x7e8);
/// ```
/// ```compile_fail
/// use dashboard_core::acquisition::AcquisitionModel;
/// let config = dashboard_config::validate(dashboard_config::default_config()).unwrap();
/// let mut owner = AcquisitionModel::demo();
/// owner.ingest_batch(&config, [0.; 8], 0);
/// ```
/// ```compile_fail
/// use dashboard_core::acquisition::AcquisitionModel;
/// let config = dashboard_config::validate(dashboard_config::default_config()).unwrap();
/// let mut owner = AcquisitionModel::demo();
/// owner.tick(&config, 0);
/// ```
pub struct AcquisitionOwner<C: ConfigurationHandle> {
    /// Immutable policy binding, absent only for legacy source/replay owners.
    configuration: Option<C>,
    /// Mutable sampling/quality/policy state that cannot be replaced independently.
    state: AcquisitionState,
}
/// Firmware-friendly owner borrowing an immutable validated configuration when configured.
pub type AcquisitionModel<'cfg> = AcquisitionOwner<&'cfg ValidatedConfig>;
#[cfg(feature = "alloc")]
/// Allocating host/browser owner with the same state implementation and boxed configuration.
pub type OwnedAcquisitionModel = AcquisitionOwner<alloc::boxed::Box<ValidatedConfig>>;

impl Default for AcquisitionModel<'_> {
    /// Returns an empty legacy Vehicle owner with alarms disabled.
    fn default() -> Self { Self::new() }
}
impl<'cfg> AcquisitionModel<'cfg> {
    /// Returns an empty Vehicle owner with generic typed-source checks and no
    /// configured source policy or enabled alarms; does not issue ECU requests.
    pub fn new() -> Self {
        let mut owner = Self::session_base::<false>(None);
        owner.initialize_for_session(0);
        owner
    }

    /// Returns an empty synthetic owner with legacy demo alarm fixtures; no data
    /// is generated until explicitly ingested.
    pub fn demo() -> Self { Self::legacy(DataMode::Demo, demo_rules()) }

    /// Returns unconfigured state in `mode` using per-channel `rules`; panics for
    /// slot-mismatched rules. Issuer exhaustion disables proof, not data handling.
    fn legacy(
        mode: DataMode,
        rules: [Option<crate::alarms::Rule>; 8],
    ) -> Self {
        Self {
            configuration: None,
            state: AcquisitionState::with_policy(
                mode,
                [None; SENSOR_COUNT],
                DEFAULT_FRESHNESS,
                Alarms::Legacy(AlarmEngine::new(rules).expect("valid legacy rules")),
            ),
        }
    }

    /// Returns a Vehicle owner immutably bound to `config`, resolving allowed
    /// sources for response CAN ID `ecu` and request ID ecu-8. Invalid/unqualified
    /// routes resolve absent, not live support. Performs no transport operation.
    pub fn configured(
        config: &'cfg ValidatedConfig,
        ecu: u16,
    ) -> Self {
        let mut owner = Self::session_base::<true>(Some(config));
        owner.initialize_for_session(ecu);
        owner
    }

    // Diagnostics construction only; neither base nor finishing is a public
    // acquisition operation. None remains valid issuer exhaustion after finish.
    #[inline(always)]
    /// Returns construction-only empty storage bound to `config`. Panics unless
    /// CONFIGURED matches config presence; caller must initialize once before use.
    pub(crate) fn session_base<const CONFIGURED: bool>(config: Option<&'cfg ValidatedConfig>) -> Self {
        assert!(
            CONFIGURED == config.is_some(),
            "construction variant matches bound configuration"
        );
        Self {
            configuration: config,
            state: const {
                if CONFIGURED {
                    AcquisitionState::CONFIGURED_BASE
                } else {
                    AcquisitionState::LEGACY_BASE
                }
            },
        }
    }

    /// Finishes freshly constructed session storage for responder `ecu`, issuing
    /// one origin and binding configured sources when present. Construction-only:
    /// must not be rerun on a populated owner's sampling/policy state.
    pub(crate) fn initialize_for_session(
        &mut self,
        ecu: u16,
    ) {
        if let Some(config) = self.configuration {
            let sources = Self::configured_sources(config, ecu);
            self.initialize_bound_policy(DataMode::Vehicle, sources);
        } else {
            self.state.origin = ORIGIN_ISSUER.issue().ok();
        }
    }

    /// Returns exact admitted OBD sources from immutable `config` for response
    /// `ecu` and checked ecu-8 request; disabled, candidate, non-OBD or mismatched
    /// routes become `None` rather than inheriting a generic PID.
    fn configured_sources(
        config: &ValidatedConfig,
        ecu: u16,
    ) -> [Option<Source>; SENSOR_COUNT] {
        SensorId::ALL.map(|id| {
            ecu.checked_sub(8).and_then(|request| {
                match resolve_source(config, id.channel(), request, ecu) {
                    SourceResolution::Obd { pid, .. } => Some(Source::Obd { ecu, pid }),
                    _ => None,
                }
            })
        })
    }
}
impl<C: ConfigurationHandle> AcquisitionOwner<C> {
    /// Returns a fresh owner holding immutable `configuration` in `mode` with
    /// supplied per-channel `sources`; initializes policy/history without an
    /// intermediate legacy state allocation.
    fn with_configuration(
        configuration: C,
        mode: DataMode,
        sources: [Option<Source>; SENSOR_COUNT],
    ) -> Self {
        let mut owner = Self {
            configuration: Some(configuration),
            state: AcquisitionState::CONFIGURED_BASE,
        };
        owner.initialize_bound_policy(mode, sources);
        owner
    }

    /// Initializes fresh configured storage for `mode` and exact `sources`, using
    /// its already installed immutable handle. Panics for absent configuration or
    /// wrong policy variant; must run only once during construction.
    fn initialize_bound_policy(
        &mut self,
        mode: DataMode,
        sources: [Option<Source>; SENSOR_COUNT],
    ) {
        let config = self
            .configuration
            .as_ref()
            .expect("configured policy has immutable handle")
            .configuration();
        self.state.mode = mode;
        self.state.ignition = IgnitionSnapshot::for_mode(mode);
        self.state.expected_sources = sources;
        self.state.freshness = config.config().sources.map(|source| source.freshness_ms);
        let Alarms::Configured(engine) = &mut self.state.alarms else {
            unreachable!("configured construction base")
        };
        engine.initialize_bound_policy(config, mode, sources);
        self.state.origin = ORIGIN_ISSUER.issue().ok();
        for (sensor, seconds) in self.state.sensors.iter_mut().zip(config.config().history_seconds) {
            sensor.set_history_duration(seconds);
        }
    }

    /// Returns a synthetic owner bound to immutable `config` (borrowed or boxed)
    /// with Demo sources for all channels; preserves the configuration's policy/history.
    pub fn demo_configured(config: C) -> Self {
        Self::with_configuration(config, DataMode::Demo, [Some(Source::Demo); SENSOR_COUNT])
    }

    /// Returns the immutable bound document, or `None` for legacy owners.
    pub fn configuration(&self) -> Option<&ValidatedConfig> {
        self.configuration.as_ref().map(ConfigurationHandle::configuration)
    }

    /// Admits signed-degree `sample` at monotonic `now_ms` only in Demo mode.
    /// Source/future/order failures preserve all state. Invalid degrees publish
    /// InvalidPayload and consume candidate identity, retaining hidden last-good
    /// data. Returns numeric admission status; panics before mutation on outer
    /// sequence exhaustion. Updates grouped warning policy without adding scalar
    /// measurements, history or peak holds.
    pub fn ingest_ignition(
        &mut self,
        sample: IgnitionSample,
        now_ms: u64,
    ) -> Result<(), SampleError> {
        self.state.ignition.check_sample(sample, self.state.mode, now_ms)?;
        let next = self
            .state
            .sequence
            .checked_add(1)
            .expect("measurement sequence exhausted");
        self.state
            .prepare_ignition(now_ms, Some((sample.cylinder, sample.received_at_ms)));
        let valid = self.state.ignition.accept(sample);
        self.state.sequence = next;
        self.state.last_ms = self.state.last_ms.max(now_ms);
        self.state.publish_change();
        self.state.update_alarms(
            self.configuration.as_ref().map(ConfigurationHandle::configuration),
            now_ms,
        );
        if valid { Ok(()) } else { Err(SampleError::InvalidValue) }
    }

    /// Admits four independent signed-degree Demo `values` in cylinder-number
    /// order at `captured_ms` milliseconds. Returns whether any entry changed;
    /// Vehicle, invalid numeric entries and duplicate/regressed times are ignored.
    /// Preflights the outer counter and every accepted cylinder counter before
    /// any mutation, panicking on exhaustion. One batch advances publication once.
    pub fn ingest_ignition_demo(
        &mut self,
        values: [f32; 4],
        captured_ms: u64,
    ) -> bool {
        if self.state.mode != DataMode::Demo {
            return false;
        }
        let mut accepted = 0u8;
        for cylinder in Cylinder::ALL {
            if valid_degrees(values[cylinder.index()])
                && self
                    .state
                    .ignition
                    .received_at_ms(cylinder)
                    .is_none_or(|at| captured_ms > at)
                && captured_ms >= self.state.ignition.status_at_ms(cylinder)
            {
                self.state
                    .ignition
                    .sequence(cylinder)
                    .checked_add(1)
                    .expect("cylinder sequence exhausted");
                accepted |= 1 << cylinder.index();
            }
        }
        if accepted == 0 {
            return false;
        }
        let next = self
            .state
            .sequence
            .checked_add(1)
            .expect("measurement sequence exhausted");
        self.state.prepare_ignition(captured_ms, None);
        for cylinder in Cylinder::ALL {
            if accepted & (1 << cylinder.index()) != 0 {
                self.state.ignition.accept(IgnitionSample {
                    cylinder,
                    degrees: values[cylinder.index()],
                    received_at_ms: captured_ms,
                    sequence: self.state.ignition.sequence(cylinder) + 1,
                    source: Source::Demo,
                });
            }
        }
        self.state.sequence = next;
        self.state.last_ms = self.state.last_ms.max(captured_ms);
        self.state.publish_change();
        self.state.update_alarms(
            self.configuration.as_ref().map(ConfigurationHandle::configuration),
            captured_ms,
        );
        true
    }

    /// Marks `cylinder` with unavailable `quality` at non-regressed monotonic
    /// `at_ms`, preserving hidden last-good data and source. Returns false for
    /// Valid/Stale or duplicate/older faults. Panics before mutation on outer
    /// sequence exhaustion; never creates a valid value or a scalar warning.
    pub fn mark_ignition_fault(
        &mut self,
        cylinder: Cylinder,
        quality: Quality,
        at_ms: u64,
    ) -> bool {
        if !self.state.ignition.accepts_fault(cylinder, quality, at_ms) {
            return false;
        }
        let next = self
            .state
            .sequence
            .checked_add(1)
            .expect("measurement sequence exhausted");
        self.state.ignition.fault(cylinder, quality, at_ms);
        self.state.sequence = next;
        self.state.last_ms = self.state.last_ms.max(at_ms);
        self.state.publish_change();
        self.state.update_alarms(
            self.configuration.as_ref().map(ConfigurationHandle::configuration),
            at_ms,
        );
        true
    }

    /// Admits typed `sample` at monotonic `now_ms` milliseconds without operating
    /// context. Returns success or sample error. Future/source/order rejection
    /// preserves state; invalid numeric input publishes InvalidPayload and clears
    /// graph/holds while retaining last-good statistics. Panics on sequence exhaustion.
    pub fn ingest_sample(
        &mut self,
        sample: Sample,
        now_ms: u64,
    ) -> Result<(), SampleError> {
        self.state.ingest_sample(
            self.configuration.as_ref().map(ConfigurationHandle::configuration),
            sample,
            now_ms,
        )
    }

    /// Context must come from verified operating signals, with matching source and time.
    /// Admits `sample` with optional `context` at monotonic `now_ms` milliseconds.
    /// Returns the same admission/error semantics as [`Self::ingest_sample`]; context
    /// qualification belongs to the rule engine and is not inferred from the value.
    pub fn ingest_contextual(
        &mut self,
        sample: Sample,
        context: Option<Context>,
        now_ms: u64,
    ) -> Result<(), SampleError> {
        self.state.ingest_contextual(
            self.configuration.as_ref().map(ConfigurationHandle::configuration),
            sample,
            context,
            now_ms,
        )
    }

    /// In Demo mode, ingests canonical `values` in sensor order at `captured_ms`
    /// milliseconds; returns whether any valid newer entry was accepted. Vehicle
    /// mode rejects batches. Invalid entries are ignored, unlike typed invalid-value
    /// fault publication. Panics before mutation if any required sequence exhausts.
    pub fn ingest_batch(
        &mut self,
        values: [f32; SENSOR_COUNT],
        captured_ms: u64,
    ) -> bool {
        self.state.ingest_batch(
            self.configuration.as_ref().map(ConfigurationHandle::configuration),
            values,
            captured_ms,
        )
    }

    /// Marks `sensor` with fault `quality` at monotonic `at_ms` milliseconds,
    /// preserving last-good statistics but clearing history/holds/context. Returns
    /// false for Valid/Stale, regressed or duplicate statuses; panics on sequence exhaustion.
    pub fn mark_fault(
        &mut self,
        sensor: SensorId,
        quality: Quality,
        at_ms: u64,
    ) -> bool {
        self.state.mark_fault(
            self.configuration.as_ref().map(ConfigurationHandle::configuration),
            sensor,
            quality,
            at_ms,
        )
    }

    /// Advance freshness and invalidate expired dwell without inventing measurements.
    /// Uses monotonic `now_ms` milliseconds; clock reversal clears held peaks and
    /// graph continuity and makes policy data unavailable rather than rewinding dwell.
    /// Existing valid cylinder observations become sticky InvalidPayload until a new
    /// sample is accepted; sequence exhaustion panics before reversal mutation.
    pub fn tick(
        &mut self,
        now_ms: u64,
    ) {
        self.state.tick(
            self.configuration.as_ref().map(ConfigurationHandle::configuration),
            now_ms,
        );
    }

    /// Call after admitting all iteration inputs, then tick timed expiries and publish.
    /// Applies fenced `command` at monotonic `now_ms` milliseconds after updating
    /// alarm qualification; returns accepted/rejected rule masks. Legacy owners
    /// reject every command; configured commands require current run/event identity.
    pub fn apply_warning_command(
        &mut self,
        command: WarningCommand,
        now_ms: u64,
    ) -> CommandResult {
        let config = self.configuration.as_ref().map(ConfigurationHandle::configuration);
        self.state.update_alarms(config, now_ms);
        let run = self.state.origin.map(|v| u64::from(v.get()));
        let (result, changed) = match (&mut self.state.alarms, config) {
            (Alarms::Configured(engine), Some(config)) => {
                let before = engine.snapshot();
                let result = engine.apply_command(config, command, run, now_ms);
                (result, before != engine.snapshot())
            }
            _ => (CommandResult::rejected(command), false),
        };
        if changed {
            self.state.publish_change();
        }
        result
    }

    /// Applies newer reset `generation` and returns true, clearing derived
    /// statistics/holds while retaining current values and alarm episodes. Returns
    /// false for zero/duplicate/older requests; acknowledgment appears in snapshots.
    pub fn reset_statistics(
        &mut self,
        generation: u64,
    ) -> bool {
        self.state.reset_statistics(generation)
    }

    /// Returns current bounded exhaustion causes without issuing IDs or mutating state.
    pub fn cache_exhaustion_status(&self) -> CacheExhaustionStatus {
        self.state.cache_exhaustion_status(self.configuration())
    }

    #[inline(never)]
    /// Returns a coherent presentation copy; caller ticks timed state before
    /// publication and readers still qualify freshness against their current clock.
    pub fn snapshot(&self) -> DashboardSnapshot { self.state.snapshot(self.configuration()) }
}

/// Legacy highlight policy: temperature maxima, voltage range, none for boost/lambda.
const PEAK_MODES: [PeakMode; SENSOR_COUNT] = [
    PeakMode::None,
    PeakMode::Maximum,
    PeakMode::Maximum,
    PeakMode::Maximum,
    PeakMode::Maximum,
    PeakMode::Maximum,
    PeakMode::Range,
    PeakMode::None,
];

#[cfg(test)]
/// Construction, identity exhaustion and retained-render correctness regressions.
mod cache_tests {
    extern crate std;
    use std::boxed::Box;

    use super::*;
    use crate::cache_diagnostics::CacheExhaustionSeen;
    use crate::colors::BLACK;
    use crate::dashboard::{DashboardCache, DashboardScene};
    use crate::display::{FrameBuffer, St7789Renderer};
    use crate::render::FpsMode;
    use crate::vehicle_status::VehicleStatus;

    /// Returns unit after checking construction variant/handle agreement and fresh
    /// initialization; panics if mismatched variants are accepted or bindings drift.
    #[test]
    fn fixed_construction_variant_matches_only_its_bound_configuration() {
        let config = dashboard_config::validate(dashboard_config::default_config()).unwrap();
        for configured in [false, true] {
            let result = std::panic::catch_unwind(|| {
                if configured {
                    AcquisitionModel::session_base::<true>(None)
                } else {
                    AcquisitionModel::session_base::<false>(Some(&config))
                }
            });
            assert!(result.is_err());
        }
        let mut legacy = AcquisitionModel::session_base::<false>(None);
        let mut configured = AcquisitionModel::session_base::<true>(Some(&config));
        assert!(legacy.state.origin.is_none() && configured.state.origin.is_none());
        assert!(matches!(legacy.state.alarms, Alarms::Legacy(_)));
        assert!(matches!(configured.state.alarms, Alarms::Configured(_)));
        legacy.initialize_for_session(0x7E8);
        configured.initialize_for_session(0x7E8);
        assert!(legacy.state.origin.is_some() && configured.state.origin.is_some());
        assert!(legacy.configuration().is_none());
        assert!(core::ptr::eq(configured.configuration().unwrap(), &config));
        assert_eq!(legacy.state.mode, DataMode::Vehicle);
        assert_eq!(configured.state.mode, DataMode::Vehicle);
    }

    /// Returns unit after comparing initial sources/history/quality across owner
    /// constructors; panics if configured initialization diverges from its policy.
    #[test]
    fn constructor_initial_fields_preserve_history_policy_and_empty_runtime_state() {
        let mut raw = dashboard_config::default_config();
        raw.history_seconds[dashboard_config::Channel::Lambda.index()] = 10;
        raw.widgets[0].warning.enabled = true;
        raw.widgets[0].warning.applicability = dashboard_config::Applicability::DemoOnly;
        let config = dashboard_config::validate(raw).unwrap();
        let configured = AcquisitionModel::configured(&config, 0x7E8);
        let demo = AcquisitionModel::demo_configured(&config);
        let legacy = AcquisitionModel::new();
        for owner in [&configured, &demo, &legacy] {
            let state = &owner.state;
            assert_eq!(state.metadata, [SampleMeta::UNKNOWN; 8]);
            assert_eq!(
                (state.sequence, state.reset_generation, state.publication, state.last_ms),
                (0, 0, 1, 0)
            );
            assert!(state.contexts.iter().all(Option::is_none));
            assert!(state.origin.is_some());
            for id in SensorId::ALL {
                let sensor = &state.sensors[id as usize];
                let changed = owner.configuration.is_some() && id == SensorId::Lambda;
                assert_eq!(sensor.graph_revision(), Some(u32::from(changed)));
                assert_eq!(
                    sensor.snapshot().history().duration_seconds(),
                    if changed { 10 } else { 120 }
                );
            }
        }
        assert_eq!(legacy.state.freshness, DEFAULT_FRESHNESS);
        assert_eq!(legacy.state.expected_sources, [None; 8]);
        assert_eq!(configured.state.mode, DataMode::Vehicle);
        assert_eq!(demo.state.mode, DataMode::Demo);
        assert_eq!(
            configured.state.freshness,
            config.config().sources.map(|s| s.freshness_ms)
        );
        assert_eq!(demo.state.expected_sources, [Some(Source::Demo); 8]);
        assert_eq!(
            configured.state.expected_sources,
            [
                None,
                Some(Source::Obd { ecu: 0x7E8, pid: 0x5C }),
                Some(Source::Obd { ecu: 0x7E8, pid: 0x05 }),
                None,
                Some(Source::Obd { ecu: 0x7E8, pid: 0x0F }),
                None,
                Some(Source::Obd { ecu: 0x7E8, pid: 0x42 }),
                None
            ]
        );
        let rule = RuleIndex::new(0).unwrap();
        let Alarms::Configured(vehicle_rules) = &configured.state.alarms else {
            panic!("configured variant")
        };
        let Alarms::Configured(demo_rules) = &demo.state.alarms else {
            panic!("configured variant")
        };
        assert!(!vehicle_rules.enabled());
        assert!(demo_rules.enabled());
        assert_eq!(vehicle_rules.snapshot().channel(rule), Some(SensorId::Boost));
        assert_eq!(demo_rules.snapshot().channel(rule), Some(SensorId::Boost));
        assert_eq!(configured.snapshot().warning_command_result(), CommandResult::EMPTY);
        assert_eq!(demo.snapshot().warning_event_serial(), 0);
    }

    /// Returns unit after publication overflow disables deduplication while samples,
    /// ticks and reset acknowledgments continue; panics on ID reuse or frozen state.
    #[test]
    fn publication_exhaustion_disables_deduplication_without_freezing_state() {
        let mut model = AcquisitionModel::demo();
        model.ingest_batch([1.0; 8], 0);
        model.tick(2_000);
        model.state.publication = u64::MAX - 1;
        model.state.publish_change();
        let mut displayed = model.snapshot();
        assert_eq!(displayed.publication_revision(), Some(u64::MAX));
        model.tick(6_000);
        let exhausted = model.snapshot();
        assert_eq!(exhausted.publication_revision(), None);
        assert_eq!(exhausted.publication_identity(), None);
        assert_eq!(exhausted.sequence, displayed.sequence);
        assert!(displayed.accept(exhausted));
        assert!(displayed.accept(exhausted)); // Explicit conservative fallback.
        model.ingest_batch([2.0; 8], 6_001);
        assert!(displayed.accept(model.snapshot()));
        assert_eq!(displayed.sensors[1].maximum(), Some(2.0));
        model.reset_statistics(1);
        assert!(displayed.accept(model.snapshot()));
        assert_eq!(displayed.reset_generation, 1);
        assert_eq!(displayed.publication_identity(), None);
    }

    #[cfg(feature = "alloc")]
    /// Returns unit after exhausted borrowed/owned models retain their distinct
    /// immutable policies and coherent samples; panics on configuration aliasing.
    #[test]
    fn exhausted_origins_preserve_owned_and_borrowed_policy_binding() {
        use dashboard_config::{default_config, validate};
        let mut raw = default_config();
        raw.widgets[0].warning.enabled = true;
        raw.widgets[0].warning.trigger = 1.5;
        raw.widgets[0].warning.clear = 1.0;
        raw.widgets[0].warning.on_ms = 500;
        let original = validate(raw.clone()).unwrap();
        let mut borrowed = AcquisitionModel::demo_configured(&original);
        let mut owned = OwnedAcquisitionModel::demo_configured(Box::new(original.clone()));
        raw.widgets[0].warning.trigger = 3.0;
        raw.widgets[0].warning.clear = 2.5;
        let mut replacement = OwnedAcquisitionModel::demo_configured(Box::new(validate(raw).unwrap()));
        let exhausted = OriginIssuer::new(u32::MAX);
        borrowed.state.origin = exhausted.issue().ok();
        owned.state.origin = exhausted.issue().ok();
        replacement.state.origin = exhausted.issue().ok();
        let rule = RuleIndex::new(0).unwrap();
        for (at, expected) in [(0, Status::Pending), (250, Status::Pending), (500, Status::Active)] {
            let values = [2.0, 90.0, 95.0, 80.0, 30.0, 700.0, 13.8, 1.0];
            borrowed.ingest_batch(values, at);
            owned.ingest_batch(values, at);
            replacement.ingest_batch(values, at);
            assert_eq!(owned.snapshot(), borrowed.snapshot());
            assert_eq!(owned.snapshot().rule_status_at(rule, at), expected);
            assert_eq!(replacement.snapshot().rule_status_at(rule, at), Status::Normal);
            assert!(
                owned
                    .snapshot()
                    .sensors
                    .iter()
                    .all(|s| s.graph_cache_identity().is_none())
            );
        }
        assert_eq!(
            owned.cache_exhaustion_status(),
            CacheExhaustionStatus::from_cause(CacheExhaustionCause::AcquisitionIssuer)
        );
        assert_eq!(exhausted.issue(), Err(()));
    }

    /// Returns unit after the final issued origin remains valid and exhaustion
    /// diagnostics stay sticky through resets/invalid data; panics on false reporting.
    #[test]
    fn actual_origin_failure_is_sticky_and_last_success_has_no_diagnostic() {
        let issuer = OriginIssuer::new(u32::MAX - 1);
        let mut model = AcquisitionModel::demo();
        model.state.origin = issuer.issue().ok();
        assert!(model.state.origin.is_some());
        assert_eq!(model.snapshot().cache_exhaustion_status(), CacheExhaustionStatus::EMPTY);
        model.state.origin = issuer.issue().ok();
        assert_eq!(issuer.issue(), Err(()));
        let exhausted = CacheExhaustionStatus::from_cause(CacheExhaustionCause::AcquisitionIssuer);
        assert_eq!(model.snapshot().cache_exhaustion_status(), exhausted);
        model.reset_statistics(1);
        model.ingest_batch([f32::NAN; 8], 0);
        assert_eq!(model.snapshot().cache_exhaustion_status(), exhausted);
        assert_eq!(
            DashboardSnapshot::EMPTY.cache_exhaustion_status(),
            CacheExhaustionStatus::EMPTY
        );
    }

    /// Returns unit after graph-proof exhaustion publishes diagnostics without a
    /// new sample and both retained framebuffers match full redraws; panics on reuse.
    #[test]
    fn exhausted_revision_publishes_without_sample_and_never_freezes_either_buffer() {
        let mut model = AcquisitionModel::demo();
        model.ingest_batch([1.0, 90.0, 95.0, 80.0, 30.0, 750.0, 13.8, 1.0], 0);
        model.state.sensors[1].force_revision_for_test(u32::MAX - 1);
        model.state.sensors[1].reset_graph();
        let before = model.snapshot();
        assert!(before.sensors[1].graph_cache_identity().is_some());
        assert_eq!(before.cache_exhaustion_status(), CacheExhaustionStatus::EMPTY);
        model.state.sensors[1].reset_graph();
        let failed = model.snapshot();
        assert_eq!(failed.sequence, before.sequence);
        assert_eq!(failed.reset_generation, before.reset_generation);
        assert_ne!(failed.publication_identity(), before.publication_identity());
        let mut accepted = before;
        let mut seen = CacheExhaustionSeen::new();
        assert_eq!(seen.observe(failed.cache_exhaustion_status()).causes().count(), 1);
        assert!(accepted.accept(failed));
        let mut cache = DashboardCache::new();
        let mut frames = [Box::new(FrameBuffer::new()), Box::new(FrameBuffer::new())];
        let mut full = Box::new(FrameBuffer::new());
        for step in 1..5 {
            model.ingest_batch(
                [1.0, 90.0 + step as f32 * 2.0, 95.0, 80.0, 30.0, 750.0, 13.8, 1.0],
                step * 2000,
            );
            let snapshot = model.snapshot();
            assert_eq!(snapshot.sensors[1].graph_cache_identity(), None);
            // Sink has already dropped the one report. Drawing is independent.
            assert_eq!(seen.observe(snapshot.cache_exhaustion_status()).causes().count(), 0);
            let scene = DashboardScene {
                readings: None,
                layout: None,
                measurements: &snapshot,
                now_ms: step * 2000,
                status: VehicleStatus::Demo,
                blink_on: false,
                boost_psi: false,
                fps_mode: FpsMode::Off,
                fps_instant: 0.0,
                fps_average: 0.0,
                backgrounds: [BLACK; 8],
                overlay: None,
            };
            scene.draw_full(&mut St7789Renderer::new(&mut full));
            for (buffer, frame) in frames.iter_mut().enumerate() {
                cache.draw(&mut St7789Renderer::new(frame), buffer, &scene);
                assert!(frame.as_ref() == full.as_ref());
            }
        }
        model.reset_statistics(1);
        assert_eq!(
            model.snapshot().cache_exhaustion_status(),
            failed.cache_exhaustion_status()
        );
        model.state.sensors[5].force_revision_for_test(u32::MAX);
        model.state.sensors[5].reset_graph();
        assert_eq!(
            seen.observe(model.snapshot().cache_exhaustion_status()).causes().next(),
            Some(CacheExhaustionCause::GraphRevision(dashboard_config::Channel::Egt))
        );
    }
}

#[cfg(test)]
/// Warning-run exhaustion and borrowed/boxed command-path equivalence regressions.
mod warning_tests {
    use super::*;
    /// Returns one validated latched boost rule with 100 ms dwell and reachable
    /// acknowledgment; panics if the fixture no longer satisfies shared validation.
    fn config() -> ValidatedConfig {
        let mut c = dashboard_config::default_config();
        c.widgets.truncate(1);
        let w = &mut c.widgets[0].warning;
        w.enabled = true;
        w.trigger = 2.;
        w.clear = 1.;
        w.on_ms = 100;
        w.advanced.latch = true;
        c.controls.screens[0].buttons[0].tap = dashboard_config::ControlAction::AcknowledgeWarnings;
        dashboard_config::validate(c).unwrap()
    }
    /// Returns unit after an exhausted run retains warning visibility but rejects
    /// acknowledgment identity; panics if untracked commands alter the episode.
    #[test]
    fn exhausted_run_keeps_warning_visibility_and_rejects_commands() {
        let c = config();
        let mut m = AcquisitionModel::demo_configured(&c);
        m.state.origin = None;
        for at in [0, 100] {
            m.ingest_batch([3., 90., 95., 80., 30., 700., 13.8, 1.], at);
        }
        assert_eq!(m.snapshot().warning_run_id(), None);
        assert_eq!(
            m.apply_warning_command(
                WarningCommand {
                    request_id: 1,
                    run_id: 0,
                    observed_event_serial: 1,
                    rules: 1
                },
                100
            )
            .rejected,
            1
        );
        let e = m.snapshot().warning_eligibility(RuleIndex::new(0).unwrap(), 100);
        assert!(e.visible());
        assert!(!e.acknowledged);
    }
    #[cfg(feature = "alloc")]
    /// Returns unit after borrowed/boxed owners apply equivalent run-adjusted
    /// commands and episode eligibility; panics on facade behavior divergence.
    #[test]
    fn owned_alias_uses_the_same_command_and_episode_implementation() {
        let c = config();
        let mut borrowed = AcquisitionModel::demo_configured(&c);
        let mut owned = OwnedAcquisitionModel::demo_configured(alloc::boxed::Box::new(config()));
        for at in [0, 100] {
            let values = [3., 90., 95., 80., 30., 700., 13.8, 1.];
            borrowed.ingest_batch(values, at);
            owned.ingest_batch(values, at);
        }
        let b = borrowed.snapshot();
        let o = owned.snapshot();
        let command = WarningCommand {
            request_id: 1,
            run_id: b.warning_run_id().unwrap(),
            observed_event_serial: 1,
            rules: 1,
        };
        assert_eq!(
            borrowed.apply_warning_command(command, 100),
            owned.apply_warning_command(
                WarningCommand {
                    run_id: o.warning_run_id().unwrap(),
                    ..command
                },
                100
            )
        );
        assert_eq!(
            borrowed.snapshot().warning_eligibility(RuleIndex::new(0).unwrap(), 100),
            owned.snapshot().warning_eligibility(RuleIndex::new(0).unwrap(), 100)
        );
    }
}

#[cfg(test)]
/// Terminal raw-sequence guards at each cylinder mutation boundary.
mod ignition_sequence_tests {
    extern crate std;
    use std::panic::{AssertUnwindSafe, catch_unwind};

    use super::*;

    /// Catches exhausted outer counters changing a cylinder, publication or clock
    /// watermark before the panic, including a reversed tick's sticky fault.
    #[test]
    fn all_cylinder_mutations_preflight_outer_sequence() {
        for operation in 0..4 {
            let mut owner = AcquisitionModel::demo();
            owner.ingest_ignition_demo([-1.0; 4], 100);
            owner.state.sequence = u64::MAX;
            let before = owner.snapshot();
            let before_time = owner.state.last_ms;
            assert!(
                catch_unwind(AssertUnwindSafe(|| {
                    match operation {
                        0 => {
                            let _ = owner.ingest_ignition(
                                IgnitionSample {
                                    cylinder: Cylinder::One,
                                    degrees: f32::NAN,
                                    received_at_ms: 101,
                                    sequence: 2,
                                    source: Source::Demo,
                                },
                                101,
                            );
                        }
                        1 => {
                            owner.ingest_ignition_demo([-2.0; 4], 101);
                        }
                        2 => {
                            owner.mark_ignition_fault(Cylinder::Four, Quality::TransportError, 101);
                        }
                        _ => {
                            owner.tick(99);
                        }
                    }
                }))
                .is_err()
            );
            assert_eq!(owner.snapshot(), before);
            assert_eq!(owner.snapshot().publication_identity(), before.publication_identity());
            assert_eq!(owner.state.last_ms, before_time);
        }
    }

    /// Catches using raw sequence as publication identity or dropping grouped
    /// changes after publication exhaustion; numeric acquisition must remain live.
    #[test]
    fn exhausted_publication_identity_does_not_freeze_cylinders() {
        let mut owner = AcquisitionModel::demo();
        owner.state.publication = u64::MAX;
        owner.ingest_ignition_demo([0.0, -1.0, -2.0, -3.0], 0);
        let mut displayed = owner.snapshot();
        assert_eq!(displayed.publication_identity(), None);
        owner.ingest_ignition_demo([0.0, -1.0, -2.0, -4.0], 1);
        assert!(displayed.accept(owner.snapshot()));
        assert_eq!(
            displayed.ignition.value_at(Cylinder::Four, DataMode::Demo, 1),
            Some(-4.0)
        );
    }
}
