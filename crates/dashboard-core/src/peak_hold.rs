//! Canonical per-widget timed extrema, updated before latest-value coalescing.
use dashboard_config::{MAX_WIDGETS, PeakSelection, ValidatedConfig};

use crate::alarms::RuleIndex;
use crate::samples::SensorId;

/// Copyable per-widget held extrema in each widget channel's canonical units.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PeakSnapshot {
    /// Raw held values; only entries with a set validity bit may be presented.
    values: [f32; MAX_WIDGETS],
    /// Validity bits indexed by configured widget/rule index.
    valid: u64,
}
impl PeakSnapshot {
    /// No held values; zero backing storage is not measurement evidence.
    pub const EMPTY: Self = Self {
        values: [0.0; MAX_WIDGETS],
        valid: 0,
    };

    /// Returns the held canonical value for widget `index`, or `None` if absent.
    /// Freshness is enforced by acquisition before publication and by snapshot readers.
    pub fn value(
        &self,
        index: RuleIndex,
    ) -> Option<f32> {
        (self.valid & (1u64 << index.index()) != 0).then_some(self.values[index.index()])
    }

    /// Returns the widget-indexed presence mask, independent of numeric values.
    pub const fn valid_mask(&self) -> u64 { self.valid }
}
/// Acquisition-owned timed extrema before snapshot coalescing; no rendering clock.
pub(crate) struct PeakHolds {
    /// Exclusive expiry timestamps in monotonic milliseconds, meaningful for valid bits.
    until_ms: [u64; MAX_WIDGETS],
    /// Current published values/presence, sharing widget indices with configuration.
    snapshot: PeakSnapshot,
}
impl PeakHolds {
    /// Empty holds with no valid deadlines or measurements.
    pub const EMPTY: Self = Self {
        until_ms: [0; MAX_WIDGETS],
        snapshot: PeakSnapshot::EMPTY,
    };

    /// Returns a copy of current held values without advancing expiry.
    pub const fn snapshot(&self) -> PeakSnapshot { self.snapshot }

    /// Invalidates all holds; old values/deadlines remain inaccessible behind the mask.
    pub fn clear(&mut self) { self.snapshot.valid = 0; }

    /// Invalidates all widgets in immutable `config` that use canonical `channel`.
    pub fn clear_channel(
        &mut self,
        config: &ValidatedConfig,
        channel: SensorId,
    ) {
        for (i, w) in config.config().widgets.iter().enumerate() {
            if w.channel.scalar() == Some(channel.channel()) {
                self.snapshot.valid &= !(1u64 << i);
            }
        }
    }

    /// Applies fresh canonical `value` for `channel` at monotonic `at` milliseconds
    /// to matching enabled widgets in `config`. Replaces missing/expired holds or
    /// strictly better extrema and restarts their saturating deadline; equality
    /// does not extend a live hold. Caller validates value/time/source and binds
    /// the same configuration for the entire owner's lifetime.
    pub fn sample(
        &mut self,
        config: &ValidatedConfig,
        channel: SensorId,
        value: f32,
        at: u64,
    ) {
        for (i, w) in config.config().widgets.iter().enumerate() {
            if w.channel.scalar() != Some(channel.channel()) || w.advanced.peak_mode == PeakSelection::Off {
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

    /// Clears holds at/after their deadline at monotonic `now` milliseconds or
    /// when their channel's `fresh` entry is false, using immutable `config`.
    /// Returns whether validity changed; does not fabricate a replacement peak.
    pub fn expire(
        &mut self,
        config: &ValidatedConfig,
        fresh: [bool; 8],
        now: u64,
    ) -> bool {
        let before = self.snapshot.valid;
        for (i, w) in config.config().widgets.iter().enumerate() {
            if now >= self.until_ms[i] || w.channel.scalar().is_none_or(|channel| !fresh[channel.index()]) {
                self.snapshot.valid &= !(1u64 << i);
            }
        }
        before != self.snapshot.valid
    }
}
/// Fixed acquisition-side memory budget for 64 widget values and deadlines.
const _: () = assert!(core::mem::size_of::<PeakHolds>() == 776);
/// Fixed publication size prevents per-widget hold state from growing silently.
const _: () = assert!(core::mem::size_of::<PeakSnapshot>() == 264);
