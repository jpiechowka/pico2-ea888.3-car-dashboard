use super::extrema::{Extrema, valid_sample};
use super::history::{History, HistoryData, HistoryView};
use crate::config::{HISTORY_SIZE, TREND_THRESHOLD};

/// Exact graph proof, issued only by the acquisition owner. Channel participates
/// because public snapshot arrays may move a copied sensor to another slot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GraphCacheIdentity {
    origin: core::num::NonZeroU32,
    revision: u32,
    channel: dashboard_config::Channel,
}

const AVG_BUFFER_SIZE: usize = 60;

pub const AVG_SAMPLE_PERIOD_MS: u64 = 5_000;

pub const GRAPH_HISTORY_SIZE: usize = 60;

pub const GRAPH_SAMPLE_PERIOD_MS: u64 = 2_000;

pub const PEAK_HOLD_MS: u64 = 750;

const TREND_SAMPLE_PERIOD_MS: u64 = 200;
const TREND_WINDOW_MS: u64 = 2_000;

#[derive(Clone, Copy)]
pub enum PeakMode {
    None,
    Maximum,
    Range,
}

/// Presentation data only. Sampling rings and accumulation stay with the
/// acquisition owner; a coalesced snapshot already includes every accepted peak.
#[derive(Clone, Copy, Debug)]
pub struct SensorSnapshot {
    value: Option<f32>,
    minimum: Option<f32>,
    maximum: Option<f32>,
    average: Option<f32>,
    trend: Option<bool>,
    sampled_at_ms: Option<u64>,
    peak_until_ms: Option<u64>,
    graph: HistoryData,
    graph_identity: Option<GraphCacheIdentity>,
}

// Runtime cache proof does not change semantic snapshot equality.
impl PartialEq for SensorSnapshot {
    fn eq(
        &self,
        other: &Self,
    ) -> bool {
        self.value == other.value
            && self.minimum == other.minimum
            && self.maximum == other.maximum
            && self.average == other.average
            && self.trend == other.trend
            && self.sampled_at_ms == other.sampled_at_ms
            && self.peak_until_ms == other.peak_until_ms
            && self.graph == other.graph
    }
}

impl SensorSnapshot {
    pub const EMPTY: Self = Self {
        value: None,
        minimum: None,
        maximum: None,
        average: None,
        trend: None,
        sampled_at_ms: None,
        peak_until_ms: None,
        graph: HistoryData::EMPTY,
        graph_identity: None,
    };

    pub const fn graph_cache_identity(&self) -> Option<GraphCacheIdentity> { self.graph_identity }

    pub fn value(&self) -> Option<f32> { self.value }

    pub fn minimum(&self) -> Option<f32> { self.minimum }

    pub fn maximum(&self) -> Option<f32> { self.maximum }

    /// Join accepted extrema from an earlier snapshot of the same source/run.
    /// Used by bounded browser replay when older history points leave its window.
    /// The caller owns source/time/reset boundaries. No reading or freshness is
    /// synthesized, and firmware acquisition remains the statistics owner.
    pub fn retain_extrema_from(
        &mut self,
        earlier: &Self,
    ) {
        if let Some(min) = earlier.minimum.filter(|v| valid_sample(*v)) {
            self.minimum = Some(self.minimum.map_or(min, |value| value.min(min)));
        }
        if let Some(max) = earlier.maximum.filter(|v| valid_sample(*v)) {
            self.maximum = Some(self.maximum.map_or(max, |value| value.max(max)));
        }
    }

    pub fn get_average(&self) -> Option<f32> { self.average }

    pub fn sampled_at_ms(&self) -> Option<u64> { self.sampled_at_ms }

    pub fn get_trend_at(
        &self,
        now_ms: u64,
    ) -> Option<bool> {
        let age = now_ms.checked_sub(self.sampled_at_ms?)?;
        if age <= TREND_WINDOW_MS { self.trend } else { None }
    }

    pub fn is_new_peak_at(
        &self,
        now_ms: u64,
    ) -> bool {
        self.sampled_at_ms.is_some_and(|at| at <= now_ms) && self.peak_until_ms.is_some_and(|until| now_ms < until)
    }

    pub fn history(&self) -> HistoryView<'_> { self.graph.view() }
}

pub struct SensorState {
    value: Option<f32>,
    extrema: Extrema,
    history: [f32; HISTORY_SIZE],
    history_time_ms: [u64; HISTORY_SIZE],
    history_index: usize,
    history_count: usize,
    trend_last_sample_ms: Option<u64>,
    trend_threshold: f32,
    last_sample_ms: Option<u64>,
    last_tick_ms: Option<u64>,
    peak_until_ms: Option<u64>,

    pub is_new_peak: bool,

    avg_buffer: [f32; AVG_BUFFER_SIZE],
    avg_index: usize,
    avg_count: usize,
    avg_sum: f32,
    avg_last_sample_ms: Option<u64>,

    graph: History,
}

impl SensorState {
    #[cfg(test)]
    pub(crate) fn force_revision_for_test(
        &mut self,
        revision: u32,
    ) {
        self.graph.force_revision(revision);
    }

    pub(crate) const fn graph_revision(&self) -> Option<u32> { self.graph.revision() }

    pub const fn new() -> Self {
        Self {
            value: None,
            extrema: Extrema::new(),
            history: [0.0; HISTORY_SIZE],
            history_time_ms: [0; HISTORY_SIZE],
            history_index: 0,
            history_count: 0,
            trend_last_sample_ms: None,
            trend_threshold: TREND_THRESHOLD,
            last_sample_ms: None,
            last_tick_ms: None,
            peak_until_ms: None,
            is_new_peak: false,
            avg_buffer: [0.0; AVG_BUFFER_SIZE],
            avg_index: 0,
            avg_count: 0,
            avg_sum: 0.0,
            avg_last_sample_ms: None,
            graph: History::new(120),
        }
    }

    pub const fn value(&self) -> Option<f32> { self.value }

    pub const fn sampled_at_ms(&self) -> Option<u64> { self.last_sample_ms }

    pub fn snapshot(&self) -> SensorSnapshot { self.snapshot_with_identity(None) }

    pub(crate) fn tracked_snapshot(
        &self,
        origin: Option<core::num::NonZeroU32>,
        channel: dashboard_config::Channel,
    ) -> SensorSnapshot {
        let identity = origin.zip(self.graph.revision()).map(|(origin, revision)| {
            GraphCacheIdentity {
                origin,
                revision,
                channel,
            }
        });
        self.snapshot_with_identity(identity)
    }

    fn snapshot_with_identity(
        &self,
        graph_identity: Option<GraphCacheIdentity>,
    ) -> SensorSnapshot {
        SensorSnapshot {
            value: self.value,
            minimum: self.minimum(),
            maximum: self.maximum(),
            average: self.get_average(),
            trend: self.get_trend(),
            sampled_at_ms: self.last_sample_ms,
            peak_until_ms: self.peak_until_ms,
            graph: self.graph.data(),
            graph_identity,
        }
    }

    pub fn minimum(&self) -> Option<f32> { self.extrema.min() }

    pub fn maximum(&self) -> Option<f32> { self.extrema.max() }

    /// Apply all measurement-derived state through the same acceptance gate.
    pub fn ingest(
        &mut self,
        value: f32,
        sampled_at_ms: u64,
        peak_mode: PeakMode,
    ) -> bool {
        let peak = match peak_mode {
            PeakMode::None => false,
            PeakMode::Maximum => self.extrema.would_raise(value),
            PeakMode::Range => self.extrema.would_extend(value),
        };
        self.update_at(value, peak, sampled_at_ms)
    }

    pub const fn with_trend_threshold(threshold: f32) -> Self {
        let mut state = Self::new();
        if threshold.is_finite() && threshold >= 0.0 {
            state.trend_threshold = threshold;
        }
        state
    }

    /// Returns false for an invalid value or a duplicate/out-of-order measurement.
    pub fn update_at(
        &mut self,
        value: f32,
        is_max_updated: bool,
        sampled_at_ms: u64,
    ) -> bool {
        if !valid_sample(value) || self.last_sample_ms.is_some_and(|last| sampled_at_ms <= last) {
            return false;
        }
        self.last_sample_ms = Some(sampled_at_ms);
        self.value = Some(value);
        self.extrema.record(value);

        if is_max_updated {
            let until = sampled_at_ms.saturating_add(PEAK_HOLD_MS);
            self.peak_until_ms = Some(until);
            self.is_new_peak = self.last_tick_ms.is_none_or(|now| now < until);
        }

        if sample_due(self.trend_last_sample_ms, sampled_at_ms, TREND_SAMPLE_PERIOD_MS) {
            self.history[self.history_index] = value;
            self.history_time_ms[self.history_index] = sampled_at_ms;
            self.history_index = (self.history_index + 1) % HISTORY_SIZE;
            self.history_count = self.history_count.saturating_add(1).min(HISTORY_SIZE);
            self.trend_last_sample_ms = Some(sampled_at_ms);
        }

        if sample_due(self.avg_last_sample_ms, sampled_at_ms, AVG_SAMPLE_PERIOD_MS) {
            self.add_avg_sample(value);
            self.avg_last_sample_ms = Some(sampled_at_ms);
        }

        self.graph.observe(value, sampled_at_ms);
        true
    }

    pub fn tick(
        &mut self,
        now_ms: u64,
    ) {
        self.graph.advance(now_ms);
        if self.last_tick_ms.is_some_and(|last| now_ms < last) {
            return;
        }
        self.last_tick_ms = Some(now_ms);
        if self.peak_until_ms.is_some_and(|until| now_ms >= until) {
            self.peak_until_ms = None;
            self.is_new_peak = false;
        }
    }

    fn add_avg_sample(
        &mut self,
        value: f32,
    ) {
        if self.avg_count >= AVG_BUFFER_SIZE {
            self.avg_sum -= self.avg_buffer[self.avg_index];
        } else {
            self.avg_count += 1;
        }

        self.avg_buffer[self.avg_index] = value;
        self.avg_sum += value;
        self.avg_index = (self.avg_index + 1) % AVG_BUFFER_SIZE;
    }

    pub fn get_average(&self) -> Option<f32> {
        if self.avg_count == 0 {
            None
        } else {
            Some(self.avg_sum / self.avg_count as f32)
        }
    }

    pub fn reset_average(&mut self) {
        self.avg_buffer = [0.0; AVG_BUFFER_SIZE];
        self.avg_index = 0;
        self.avg_count = 0;
        self.avg_sum = 0.0;
        self.avg_last_sample_ms = None;
    }

    pub fn history(&self) -> HistoryView<'_> { self.graph.view() }

    pub fn set_history_duration(
        &mut self,
        seconds: u16,
    ) {
        self.graph.set_duration(seconds);
    }

    pub fn invalidate_history(
        &mut self,
        now_ms: u64,
    ) {
        self.graph.invalidate(now_ms);
    }

    pub fn reset_graph(&mut self) { self.graph.clear(); }

    pub fn reset_peak(&mut self) {
        self.is_new_peak = false;
        self.peak_until_ms = None;
    }

    pub fn reset_trend(&mut self) {
        self.history_count = 0;
        self.history_index = 0;
        self.trend_last_sample_ms = None;
    }

    /// Keep the accepted current measurement; restart derived statistics.
    pub fn reset_statistics(&mut self) {
        self.reset_average();
        self.reset_graph();
        self.reset_peak();
        self.reset_trend();
        self.extrema = Extrema::new();
        if let Some(value) = self.value {
            self.extrema.reset_to(value);
        }
    }

    pub fn get_trend(&self) -> Option<bool> {
        let latest = self.last_sample_ms?;
        let mut recent = (0.0f32, 0usize);
        let mut older = (0.0f32, 0usize);
        for i in 0..self.history_count {
            let age = latest.saturating_sub(self.history_time_ms[i]);
            if age <= TREND_WINDOW_MS {
                recent.0 += self.history[i];
                recent.1 += 1;
            } else if age <= 2 * TREND_WINDOW_MS {
                older.0 += self.history[i];
                older.1 += 1;
            }
        }
        if recent.1 == 0 || older.1 == 0 {
            None
        } else {
            let diff = recent.0 / recent.1 as f32 - older.0 / older.1 as f32;
            (diff.abs() > self.trend_threshold).then_some(diff > 0.0)
        }
    }
}

fn sample_due(
    last_ms: Option<u64>,
    sampled_at_ms: u64,
    period_ms: u64,
) -> bool {
    last_ms.is_none_or(|last| sampled_at_ms - last >= period_ms)
}

impl Default for SensorState {
    fn default() -> Self { Self::new() }
}
