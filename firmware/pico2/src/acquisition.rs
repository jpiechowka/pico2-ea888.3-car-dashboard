//! Single-owner measurement statistics, published as bounded coherent snapshots.
use crate::sensor_state::{PeakMode, SensorSnapshot, SensorState};

pub const SENSOR_COUNT: usize = 8;

#[derive(Clone, Copy)]
#[repr(usize)]
pub enum SensorId {
    Boost,
    Oil,
    Water,
    Dsg,
    Iat,
    Egt,
    Battery,
    Afr,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DashboardSnapshot {
    pub sensors: [SensorSnapshot; SENSOR_COUNT],
    pub sequence: u64,
    pub reset_generation: u64,
}

impl DashboardSnapshot {
    pub const EMPTY: Self = Self {
        sensors: [SensorSnapshot::EMPTY; SENSOR_COUNT],
        sequence: 0,
        reset_generation: 0,
    };

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
    sequence: u64,
    reset_generation: u64,
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
            sequence: 0,
            reset_generation: 0,
        }
    }

    pub fn ingest_batch(
        &mut self,
        values: [f32; SENSOR_COUNT],
        captured_ms: u64,
    ) -> bool {
        const MODES: [PeakMode; SENSOR_COUNT] = [
            PeakMode::None,
            PeakMode::Maximum,
            PeakMode::Maximum,
            PeakMode::Maximum,
            PeakMode::Maximum,
            PeakMode::Maximum,
            PeakMode::Range,
            PeakMode::None,
        ];
        // Exhaustion is terminal, and must fail before partially changing data.
        let next_sequence = self.sequence.checked_add(1).expect("measurement sequence exhausted");
        let mut accepted = false;
        for ((state, value), mode) in self.sensors.iter_mut().zip(values).zip(MODES) {
            accepted |= state.ingest(value, captured_ms, mode);
        }
        if accepted {
            self.sequence = next_sequence;
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
            sequence: self.sequence,
            reset_generation: self.reset_generation,
        }
    }
}
