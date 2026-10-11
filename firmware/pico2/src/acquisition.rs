//! Single-owner measurement statistics, published as bounded coherent snapshots.
use crate::alarms::{AlarmEngine, Alerts, Context, Inputs, demo_contexts_for, demo_rules};
pub use crate::samples::SensorId;
use crate::samples::{DataMode, Quality, Sample, SampleError, SampleMeta, Source};
use crate::sensor_state::{PeakMode, SensorSnapshot, SensorState};

pub const SENSOR_COUNT: usize = 8;

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
}

impl DashboardSnapshot {
    pub const EMPTY: Self = Self {
        sensors: [SensorSnapshot::EMPTY; SENSOR_COUNT],
        metadata: [SampleMeta::UNKNOWN; SENSOR_COUNT],
        mode: DataMode::Vehicle,
        sequence: 0,
        reset_generation: 0,
        alerts: Alerts::DISABLED,
    };

    pub fn alerts_at(
        &self,
        now_ms: u64,
    ) -> Alerts {
        self.alerts.at(self, now_ms)
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
        self.metadata[id as usize].quality_at(id, now_ms)
    }

    pub fn value_at(
        &self,
        id: SensorId,
        now_ms: u64,
    ) -> Option<f32> {
        let sensor = self.sensor(id);
        self.metadata[id as usize].value_at(id, self.mode, sensor.value(), sensor.sampled_at_ms(), now_ms)
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

pub struct AcquisitionModel {
    sensors: [SensorState; SENSOR_COUNT],
    metadata: [SampleMeta; SENSOR_COUNT],
    mode: DataMode,
    sequence: u64,
    reset_generation: u64,
    alarms: AlarmEngine,
    contexts: [Option<Context>; SENSOR_COUNT],
}

impl Default for AcquisitionModel {
    fn default() -> Self { Self::new() }
}

impl AcquisitionModel {
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
            alarms: AlarmEngine::new([None; 8]).expect("empty alarm rules"),
            contexts: [None; SENSOR_COUNT],
        }
    }

    pub fn demo() -> Self {
        Self {
            mode: DataMode::Demo,
            alarms: AlarmEngine::new(demo_rules()).expect("demo alarm rules"),
            ..Self::new()
        }
    }

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
            alerts: self.alarms.snapshot(),
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
            self.metadata[i].value_at(
                SensorId::ALL[i],
                self.mode,
                self.sensors[i].value(),
                self.sensors[i].sampled_at_ms(),
                now_ms,
            )
        });
        self.alarms.update_inputs(
            &Inputs {
                metadata: &self.metadata,
                values,
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
