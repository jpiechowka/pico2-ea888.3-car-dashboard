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
    RuleEngine,
    RuleIndex,
    Status,
    demo_contexts_for,
    demo_rules,
};
use crate::cache_diagnostics::{CacheExhaustionCause, CacheExhaustionStatus};
pub use crate::samples::SensorId;
use crate::samples::{DataMode, Quality, Sample, SampleError, SampleMeta, Source};
use crate::sensor_state::{PeakMode, SensorSnapshot, SensorState};

#[cfg(not(target_has_atomic = "32"))]
compile_error!("graph cache identities require 32-bit atomics");
struct OriginIssuer(AtomicU32);
impl OriginIssuer {
    const fn new(next: u32) -> Self { Self(AtomicU32::new(next)) }

    fn issue(&self) -> Result<NonZeroU32, ()> {
        self.0
            .try_update(Ordering::Relaxed, Ordering::Relaxed, |next| next.checked_add(1))
            .map(|next| NonZeroU32::new(next).expect("issuer starts at one"))
            .map_err(|_| ())
    }
}
static ORIGIN_ISSUER: OriginIssuer = OriginIssuer::new(1);

pub const SENSOR_COUNT: usize = 8;
const DEFAULT_FRESHNESS: [u16; SENSOR_COUNT] = {
    let mut caps = [0; SENSOR_COUNT];
    let mut i = 0;
    while i < SENSOR_COUNT {
        caps[i] = SensorId::ALL[i].channel().freshness_cap();
        i += 1;
    }
    caps
};

#[derive(Default)]
pub struct ResetRequests {
    requested: u64,
    acknowledged: u64,
}

impl ResetRequests {
    pub fn request(&mut self) -> u64 {
        self.requested = self.requested.checked_add(1).expect("reset generation exhausted");
        self.requested
    }

    /// Show success once, only after the latest actual request is acknowledged.
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

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DashboardSnapshot {
    pub sensors: [SensorSnapshot; SENSOR_COUNT],
    pub metadata: [SampleMeta; SENSOR_COUNT],
    pub mode: DataMode,
    pub sequence: u64,
    pub reset_generation: u64,
    pub(crate) alerts: Alerts,
    pub(crate) rule_alerts: RuleAlerts,
    pub(crate) freshness: [u16; SENSOR_COUNT],
    cache_exhaustion: CacheExhaustionStatus,
}

impl DashboardSnapshot {
    pub const EMPTY: Self = Self {
        sensors: [SensorSnapshot::EMPTY; SENSOR_COUNT],
        metadata: [SampleMeta::UNKNOWN; SENSOR_COUNT],
        mode: DataMode::Vehicle,
        sequence: 0,
        reset_generation: 0,
        alerts: Alerts::DISABLED,
        rule_alerts: RuleAlerts::DISABLED,
        freshness: DEFAULT_FRESHNESS,
        cache_exhaustion: CacheExhaustionStatus::EMPTY,
    };

    /// Transport equality includes diagnostic-only transitions without advancing
    /// the raw sample sequence (or warning dwell).
    pub const fn publication_identity(&self) -> (u64, u64, CacheExhaustionStatus) {
        (self.sequence, self.reset_generation, self.cache_exhaustion)
    }

    pub const fn cache_exhaustion_status(&self) -> CacheExhaustionStatus { self.cache_exhaustion }

    pub fn alerts_at(
        &self,
        now_ms: u64,
    ) -> Alerts {
        self.alerts.at(self, now_ms)
    }

    pub fn rule_status_at(
        &self,
        index: RuleIndex,
        now_ms: u64,
    ) -> Status {
        self.rule_alerts.status_at(index, self, now_ms)
    }

    pub fn rule_active_at(
        &self,
        index: RuleIndex,
        now_ms: u64,
    ) -> bool {
        self.rule_status_at(index, now_ms) == Status::Active
    }

    pub fn alarm_active_at(
        &self,
        id: SensorId,
        now_ms: u64,
    ) -> bool {
        self.alerts.status_at(self, id, now_ms) == crate::alarms::Status::Active
    }

    pub fn quality(
        &self,
        id: SensorId,
        now_ms: u64,
    ) -> Quality {
        self.metadata[id as usize].quality_with_freshness(id, now_ms, u64::from(self.freshness[id as usize]))
    }

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

    pub fn sensor(
        &self,
        id: SensorId,
    ) -> &SensorSnapshot {
        &self.sensors[id as usize]
    }

    pub fn accept(
        &mut self,
        incoming: Self,
    ) -> bool {
        if incoming.sequence < self.sequence
            || incoming.reset_generation < self.reset_generation
            || (incoming.sequence == self.sequence && incoming.reset_generation == self.reset_generation)
        {
            return false;
        }
        *self = incoming;
        true
    }
}

// Alternatives share storage; boxing is unavailable on the MCU. Legacy is
// retained solely for existing protocol/pixel replay fixtures.
#[allow(clippy::large_enum_variant)]
enum Alarms<'a> {
    Legacy(AlarmEngine),
    Configured(RuleEngine<'a>),
}
impl Alarms<'_> {
    fn enabled(&self) -> bool {
        match self {
            Self::Legacy(e) => e.enabled(),
            Self::Configured(e) => e.enabled(),
        }
    }

    fn update_inputs(
        &mut self,
        inputs: &Inputs<'_>,
        contexts: &[Option<Context>; 8],
        now: u64,
    ) {
        match self {
            Self::Legacy(e) => {
                e.update_inputs(inputs, contexts, now);
            }
            Self::Configured(e) => e.update_inputs(inputs, contexts, now),
        }
    }
}

pub struct AcquisitionModel<'a> {
    sensors: [SensorState; SENSOR_COUNT],
    metadata: [SampleMeta; SENSOR_COUNT],
    mode: DataMode,
    sequence: u64,
    reset_generation: u64,
    alarms: Alarms<'a>,
    contexts: [Option<Context>; SENSOR_COUNT],
    configuration: Option<&'a ValidatedConfig>,
    expected_sources: [Option<Source>; SENSOR_COUNT],
    freshness: [u16; SENSOR_COUNT],
    origin: Option<NonZeroU32>,
}

impl Default for AcquisitionModel<'_> {
    fn default() -> Self { Self::new() }
}

impl<'a> AcquisitionModel<'a> {
    // Build the final policy directly. Creating a legacy owner and replacing
    // its alarms kept several complete sampling-ring copies live on ARM.
    fn with_policy(
        mode: DataMode,
        configuration: Option<&'a ValidatedConfig>,
        expected_sources: [Option<Source>; SENSOR_COUNT],
        freshness: [u16; SENSOR_COUNT],
        alarms: Alarms<'a>,
    ) -> Self {
        let origin = ORIGIN_ISSUER.issue().ok();
        Self {
            // A flash template avoids separate runtime scratch arrays for each
            // sampling ring when this large owner is constructed on core 1.
            sensors: const {
                [
                    SensorState::new(),
                    SensorState::new(),
                    SensorState::new(),
                    SensorState::new(),
                    SensorState::new(),
                    SensorState::with_trend_threshold(5.0),
                    SensorState::with_trend_threshold(0.05),
                    SensorState::with_trend_threshold(0.1),
                ]
            },
            metadata: [SampleMeta::UNKNOWN; SENSOR_COUNT],
            mode,
            sequence: 0,
            reset_generation: 0,
            alarms,
            contexts: [None; SENSOR_COUNT],
            configuration,
            expected_sources,
            freshness,
            origin,
        }
    }

    pub fn new() -> Self {
        Self::with_policy(
            DataMode::Vehicle,
            None,
            [None; SENSOR_COUNT],
            DEFAULT_FRESHNESS,
            Alarms::Legacy(AlarmEngine::new([None; 8]).expect("empty alarm rules")),
        )
    }

    pub fn demo() -> Self {
        Self::with_policy(
            DataMode::Demo,
            None,
            [None; SENSOR_COUNT],
            DEFAULT_FRESHNESS,
            Alarms::Legacy(AlarmEngine::new(demo_rules()).expect("demo alarm rules")),
        )
    }

    pub fn configured(
        config: &'a ValidatedConfig,
        ecu: u16,
    ) -> Self {
        let sources = SensorId::ALL.map(|id| {
            ecu.checked_sub(8).and_then(|request| {
                match resolve_source(config, id.channel(), request, ecu) {
                    SourceResolution::Obd { pid, .. } => Some(Source::Obd { ecu, pid }),
                    _ => None,
                }
            })
        });
        Self::with_policy(
            DataMode::Vehicle,
            Some(config),
            sources,
            config.config().sources.map(|source| source.freshness_ms),
            Alarms::Configured(RuleEngine::new(config, DataMode::Vehicle, sources)),
        )
    }

    pub fn demo_configured(config: &'a ValidatedConfig) -> Self {
        let sources = [Some(Source::Demo); SENSOR_COUNT];
        Self::with_policy(
            DataMode::Demo,
            Some(config),
            sources,
            config.config().sources.map(|source| source.freshness_ms),
            Alarms::Configured(RuleEngine::new(config, DataMode::Demo, sources)),
        )
    }

    pub const fn configuration(&self) -> Option<&'a ValidatedConfig> { self.configuration }

    pub fn ingest_sample(
        &mut self,
        sample: Sample,
        now_ms: u64,
    ) -> Result<(), SampleError> {
        self.ingest_contextual(sample, None, now_ms)
    }

    /// Context must come from verified operating signals. Absent context keeps
    /// contextual rules unavailable; no inference from voltage or lambda alone.
    pub fn ingest_contextual(
        &mut self,
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
            || (self.configuration.is_some() && self.expected_sources[index] != Some(sample.source))
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
        let valid = self.sensors[index].ingest(value, sample.received_at_ms, PEAK_MODES[index]);
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
        self.contexts[index] = context;
        self.update_alarms(now_ms);
        if valid { Ok(()) } else { Err(SampleError::InvalidValue) }
    }

    pub fn mark_fault(
        &mut self,
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
        self.contexts[sensor as usize] = None;
        self.update_alarms(at_ms);
        true
    }

    pub fn ingest_batch(
        &mut self,
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
        for (i, ((state, value), mode)) in self.sensors.iter_mut().zip(values).zip(PEAK_MODES).enumerate() {
            if captured_ms >= self.metadata[i].status_at_ms && state.ingest(value, captured_ms, mode) {
                accepted = true;
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
            self.sequence = next_sequence;
            self.contexts = demo_contexts_for(self.mode, &self.metadata);
            self.update_alarms(captured_ms);
        }
        accepted
    }

    /// Acknowledged in the next published snapshot. Replays cannot reset twice.
    pub fn reset_statistics(
        &mut self,
        generation: u64,
    ) -> bool {
        if generation <= self.reset_generation {
            return false;
        }
        for state in &mut self.sensors {
            state.reset_statistics();
        }
        self.reset_generation = generation;
        true
    }

    pub fn cache_exhaustion_status(&self) -> CacheExhaustionStatus {
        let mut status = self.configuration.map_or(CacheExhaustionStatus::EMPTY, |config| {
            CacheExhaustionStatus::from_config(config.cache_identity_status())
        });
        if self.origin.is_none() {
            status = status.union(CacheExhaustionStatus::from_cause(
                CacheExhaustionCause::AcquisitionIssuer,
            ));
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
    pub fn snapshot(&self) -> DashboardSnapshot {
        DashboardSnapshot {
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
            cache_exhaustion: self.cache_exhaustion_status(),
        }
    }

    fn update_alarms(
        &mut self,
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
        self.alarms.update_inputs(
            &Inputs {
                metadata: &self.metadata,
                values,
                freshness: &self.freshness,
            },
            &self.contexts,
            now_ms,
        );
    }
}

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

    #[test]
    fn actual_origin_failure_is_sticky_and_last_success_has_no_diagnostic() {
        let issuer = OriginIssuer::new(u32::MAX - 1);
        let mut model = AcquisitionModel::demo();
        model.origin = issuer.issue().ok();
        assert!(model.origin.is_some());
        assert_eq!(model.snapshot().cache_exhaustion_status(), CacheExhaustionStatus::EMPTY);
        model.origin = issuer.issue().ok();
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

    #[test]
    fn exhausted_revision_publishes_without_sample_and_never_freezes_either_buffer() {
        let mut model = AcquisitionModel::demo();
        model.ingest_batch([1.0, 90.0, 95.0, 80.0, 30.0, 750.0, 13.8, 1.0], 0);
        model.sensors[1].force_revision_for_test(u32::MAX - 1);
        model.sensors[1].reset_graph();
        let before = model.snapshot();
        assert!(before.sensors[1].graph_cache_identity().is_some());
        assert_eq!(before.cache_exhaustion_status(), CacheExhaustionStatus::EMPTY);
        model.sensors[1].reset_graph();
        let failed = model.snapshot();
        assert_eq!(failed.sequence, before.sequence);
        assert_eq!(failed.reset_generation, before.reset_generation);
        assert_ne!(failed.publication_identity(), before.publication_identity());
        let mut accepted = before;
        let mut seen = CacheExhaustionSeen::new();
        assert_eq!(seen.observe(failed.cache_exhaustion_status()).causes().count(), 1);
        assert!(!accepted.accept(failed));
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
        model.sensors[5].force_revision_for_test(u32::MAX);
        model.sensors[5].reset_graph();
        assert_eq!(
            seen.observe(model.snapshot().cache_exhaustion_status()).causes().next(),
            Some(CacheExhaustionCause::GraphRevision(dashboard_config::Channel::Egt))
        );
    }
}
