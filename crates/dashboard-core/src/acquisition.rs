//! Single-owner measurement statistics, published as bounded coherent snapshots.
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
pub use crate::samples::SensorId;
use crate::samples::{DataMode, Quality, Sample, SampleError, SampleMeta, Source};
use crate::sensor_state::{PeakMode, SensorSnapshot, SensorState};

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
    };

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
}

impl Default for AcquisitionModel<'_> {
    fn default() -> Self { Self::new() }
}

impl<'a> AcquisitionModel<'a> {
    pub fn new() -> Self {
        Self {
            sensors: [
                SensorState::new(),
                SensorState::new(),
                SensorState::new(),
                SensorState::new(),
                SensorState::new(),
                SensorState::with_trend_threshold(5.0),
                SensorState::with_trend_threshold(0.05),
                SensorState::with_trend_threshold(0.1),
            ],
            metadata: [SampleMeta::UNKNOWN; SENSOR_COUNT],
            mode: DataMode::Vehicle,
            sequence: 0,
            reset_generation: 0,
            alarms: Alarms::Legacy(AlarmEngine::new([None; 8]).expect("empty alarm rules")),
            contexts: [None; SENSOR_COUNT],
            configuration: None,
            expected_sources: [None; SENSOR_COUNT],
            freshness: DEFAULT_FRESHNESS,
        }
    }

    pub fn demo() -> Self {
        Self {
            mode: DataMode::Demo,
            alarms: Alarms::Legacy(AlarmEngine::new(demo_rules()).expect("demo alarm rules")),
            ..Self::new()
        }
    }

    pub fn configured(
        config: &'a ValidatedConfig,
        ecu: u16,
    ) -> Self {
        let mut model = Self::new();
        model.configuration = Some(config);
        for id in SensorId::ALL {
            let index = id as usize;
            model.freshness[index] = config.config().sources[index].freshness_ms;
            model.expected_sources[index] = ecu.checked_sub(8).and_then(|request| {
                match resolve_source(config, id.channel(), request, ecu) {
                    SourceResolution::Obd { pid, .. } => Some(Source::Obd { ecu, pid }),
                    _ => None,
                }
            });
        }
        model.alarms = Alarms::Configured(RuleEngine::new(config, model.mode, model.expected_sources));
        model
    }

    pub fn demo_configured(config: &'a ValidatedConfig) -> Self {
        let mut model = Self::demo();
        model.configuration = Some(config);
        model.freshness = config.config().sources.map(|source| source.freshness_ms);
        model.expected_sources = [Some(Source::Demo); SENSOR_COUNT];
        model.alarms = Alarms::Configured(RuleEngine::new(config, model.mode, model.expected_sources));
        model
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

    pub fn snapshot(&self) -> DashboardSnapshot {
        DashboardSnapshot {
            sensors: core::array::from_fn(|i| self.sensors[i].snapshot()),
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
