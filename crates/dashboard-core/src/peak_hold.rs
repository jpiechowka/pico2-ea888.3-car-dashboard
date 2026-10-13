//! Canonical per-widget timed extrema, updated before latest-value coalescing.
use dashboard_config::{MAX_WIDGETS, PeakSelection, ValidatedConfig};

use crate::alarms::RuleIndex;
use crate::samples::SensorId;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PeakSnapshot {
    values: [f32; MAX_WIDGETS],
    valid: u64,
}
impl PeakSnapshot {
    pub const EMPTY: Self = Self {
        values: [0.0; MAX_WIDGETS],
        valid: 0,
    };

    pub fn value(
        &self,
        index: RuleIndex,
    ) -> Option<f32> {
        (self.valid & (1u64 << index.index()) != 0).then_some(self.values[index.index()])
    }

    pub const fn valid_mask(&self) -> u64 { self.valid }
}
pub(crate) struct PeakHolds {
    until_ms: [u64; MAX_WIDGETS],
    snapshot: PeakSnapshot,
}
impl PeakHolds {
    pub const EMPTY: Self = Self {
        until_ms: [0; MAX_WIDGETS],
        snapshot: PeakSnapshot::EMPTY,
    };

    pub const fn snapshot(&self) -> PeakSnapshot { self.snapshot }

    pub fn clear(&mut self) { self.snapshot.valid = 0; }

    pub fn clear_channel(
        &mut self,
        config: &ValidatedConfig,
        channel: SensorId,
    ) {
        for (i, w) in config.config().widgets.iter().enumerate() {
            if w.channel == channel.channel() {
                self.snapshot.valid &= !(1u64 << i);
            }
        }
    }

    pub fn sample(
        &mut self,
        config: &ValidatedConfig,
        channel: SensorId,
        value: f32,
        at: u64,
    ) {
        for (i, w) in config.config().widgets.iter().enumerate() {
            if w.channel != channel.channel() || w.advanced.peak_mode == PeakSelection::Off {
                continue;
            }
            let bit = 1u64 << i;
            let replace = self.snapshot.valid & bit == 0
                || at >= self.until_ms[i]
                || match w.advanced.peak_mode {
                    PeakSelection::Maximum => value > self.snapshot.values[i],
                    PeakSelection::Minimum => value < self.snapshot.values[i],
                    PeakSelection::Off => false,
                };
            if replace {
                self.snapshot.values[i] = value;
                self.until_ms[i] = at.saturating_add(u64::from(w.advanced.peak_hold_ms));
                self.snapshot.valid |= bit;
            }
        }
    }

    pub fn expire(
        &mut self,
        config: &ValidatedConfig,
        fresh: [bool; 8],
        now: u64,
    ) -> bool {
        let before = self.snapshot.valid;
        for (i, w) in config.config().widgets.iter().enumerate() {
            if now >= self.until_ms[i] || !fresh[w.channel.index()] {
                self.snapshot.valid &= !(1u64 << i);
            }
        }
        before != self.snapshot.valid
    }
}
const _: () = assert!(core::mem::size_of::<PeakHolds>() == 776);
const _: () = assert!(core::mem::size_of::<PeakSnapshot>() == 264);
