//! Single-channel accepted values, elapsed-time sampling and immutable graph snapshots.
use super::extrema::{Extrema, valid_sample};
use super::finite_optional::FiniteOptional;
use super::history::{History, HistoryData, HistoryView};
use crate::config::{HISTORY_SIZE, TREND_THRESHOLD};

/// Exact graph proof, issued only by the acquisition owner. Channel participates
/// because public snapshot arrays may move a copied sensor to another slot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GraphCacheIdentity {
    /// Non-reused acquisition run origin, absent identities disable reuse entirely.
    origin: core::num::NonZeroU32,
    /// Checked graph content/time revision within this owner.
    revision: u32,
    /// Channel provenance independent of the snapshot's array location.
    channel: dashboard_config::Channel,
}

/// Number of admitted five-second average samples retained.
const AVG_BUFFER_SIZE: usize = 60;

/// Minimum accepted-sample spacing for the rolling average, in milliseconds.
pub const AVG_SAMPLE_PERIOD_MS: u64 = 5_000;

/// Legacy graph capacity, matching the canonical history's sixty buckets.
pub const GRAPH_HISTORY_SIZE: usize = 60;

/// Legacy default 120-second graph bucket period; configured duration may differ.
pub const GRAPH_SAMPLE_PERIOD_MS: u64 = 2_000;

/// Duration of the legacy new-extremum highlight, in milliseconds.
pub const PEAK_HOLD_MS: u64 = 750;

/// Minimum spacing of accepted trend samples, in milliseconds.
const TREND_SAMPLE_PERIOD_MS: u64 = 200;
/// Width of each recent/older trend comparison window and snapshot trend age limit.
const TREND_WINDOW_MS: u64 = 2_000;

/// Which extension of lifetime extrema triggers the legacy highlight.
#[derive(Clone, Copy)]
pub enum PeakMode {
    /// No new-peak highlights, while numeric extrema still update.
    None,
    /// A value strictly above an existing maximum highlights.
    Maximum,
    /// A value strictly outside either existing bound highlights.
    Range,
}

/// Presentation data only. Sampling rings and accumulation stay with the
/// acquisition owner; a coalesced snapshot already includes every accepted peak.
#[derive(Clone, Copy, Debug)]
pub struct SensorSnapshot {
    /// Last accepted canonical value, or absent before first sample.
    value: FiniteOptional,
    /// Minimum since statistics reset, including retained current value at reset.
    minimum: FiniteOptional,
    /// Maximum since statistics reset, including retained current value at reset.
    maximum: FiniteOptional,
    /// Mean of up to sixty five-second samples, not a time-weighted interpolation.
    average: FiniteOptional,
    /// Significant rising/falling trend; `None` for insufficient/flat comparison.
    trend: Option<bool>,
    /// Original last-good sample timestamp in monotonic milliseconds.
    sampled_at_ms: Option<u64>,
    /// Exclusive legacy highlight expiry, with saturating millisecond addition.
    peak_until_ms: Option<u64>,
    /// Fixed canonical graph data and explicit gaps.
    graph: HistoryData,
    /// Optional acquisition-issued graph proof; absence requires conservative redraw.
    graph_identity: Option<GraphCacheIdentity>,
}

// Runtime cache proof does not change semantic snapshot equality.
impl PartialEq for SensorSnapshot {
    /// Returns semantic equality with `other`, excluding runtime graph-cache identity.
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
    /// No accepted value/statistics; empty graph and unavailable cache identity.
    pub const EMPTY: Self = Self {
        value: FiniteOptional::NONE,
        minimum: FiniteOptional::NONE,
        maximum: FiniteOptional::NONE,
        average: FiniteOptional::NONE,
        trend: None,
        sampled_at_ms: None,
        peak_until_ms: None,
        graph: HistoryData::EMPTY,
        graph_identity: None,
    };

    /// Returns exact owner/channel/revision proof, or `None` for untracked/exhausted data.
    pub const fn graph_cache_identity(&self) -> Option<GraphCacheIdentity> { self.graph_identity }

    /// Returns retained canonical value without source/freshness qualification.
    pub fn value(&self) -> Option<f32> { self.value.get() }

    /// Returns retained minimum, or `None` before accepted data.
    pub fn minimum(&self) -> Option<f32> { self.minimum.get() }

    /// Returns retained maximum, or `None` before accepted data.
    pub fn maximum(&self) -> Option<f32> { self.maximum.get() }

    /// Join accepted extrema from an earlier snapshot of the same source/run.
    /// Used by bounded browser replay when older history points leave its window.
    /// The caller owns source/time/reset boundaries. No reading or freshness is
    /// synthesized, and firmware acquisition remains the statistics owner.
    /// Merges valid extrema from `earlier` without changing the current reading,
    /// timestamp, graph, average or cache proof.
    /// Panics if the private finite-storage invariant is violated; admitted extrema cannot do so.
    pub fn retain_extrema_from(
        &mut self,
        earlier: &Self,
    ) {
        if let Some(min) = earlier.minimum().filter(|v| valid_sample(*v)) {
            self.minimum = FiniteOptional::try_from_option(Some(self.minimum().map_or(min, |value| value.min(min))))
                .expect("snapshot minimum must remain finite");
        }
        if let Some(max) = earlier.maximum().filter(|v| valid_sample(*v)) {
            self.maximum = FiniteOptional::try_from_option(Some(self.maximum().map_or(max, |value| value.max(max))))
                .expect("snapshot maximum must remain finite");
        }
    }

    /// Returns the captured rolling mean in canonical units, or `None` without samples.
    pub fn get_average(&self) -> Option<f32> { self.average.get() }

    /// Returns original last-good sample time in monotonic milliseconds.
    pub fn sampled_at_ms(&self) -> Option<u64> { self.sampled_at_ms }

    /// Returns captured rising/falling trend at monotonic `now_ms` milliseconds,
    /// or `None` for no trend, reversed time, or last sample older than two seconds.
    pub fn get_trend_at(
        &self,
        now_ms: u64,
    ) -> Option<bool> {
        let age = now_ms.checked_sub(self.sampled_at_ms?)?;
        if age <= TREND_WINDOW_MS { self.trend } else { None }
    }

    /// Returns whether a legacy new-peak highlight remains live at `now_ms`
    /// milliseconds, requiring last sample <= now < expiry. No freshness proof added.
    pub fn is_new_peak_at(
        &self,
        now_ms: u64,
    ) -> bool {
        self.sampled_at_ms.is_some_and(|at| at <= now_ms) && self.peak_until_ms.is_some_and(|until| now_ms < until)
    }

    /// Returns immutable chronological graph points and gaps from this snapshot.
    pub fn history(&self) -> HistoryView<'_> { self.graph.view() }
}

/// Single acquisition owner's statistics; timers never invent measurements.
pub struct SensorState {
    /// Last accepted canonical value, independent of outer source/quality metadata.
    value: Option<f32>,
    /// Accepted min/max since reset.
    extrema: Extrema,
    /// Trend sample ring in insertion order modulo its capacity.
    history: [f32; HISTORY_SIZE],
    /// Original trend sample timestamps in milliseconds, parallel to `history`.
    history_time_ms: [u64; HISTORY_SIZE],
    /// Next trend write slot.
    history_index: usize,
    /// Initialized trend slots, capped at HISTORY_SIZE.
    history_count: usize,
    /// Timestamp of last trend admission for 200 ms decimation.
    trend_last_sample_ms: Option<u64>,
    /// Nonnegative significant difference between window means, in canonical units.
    trend_threshold: f32,
    /// Last accepted raw sample time; strictly increases across ingestion.
    last_sample_ms: Option<u64>,
    /// Latest non-reversed timer observation for highlight expiry.
    last_tick_ms: Option<u64>,
    /// Exclusive millisecond deadline for legacy new-extremum highlight.
    peak_until_ms: Option<u64>,

    /// Cached legacy highlight state; snapshot readers also evaluate timestamp expiry.
    pub is_new_peak: bool,

    /// Five-second-decimated canonical samples for the rolling arithmetic mean.
    avg_buffer: [f32; AVG_BUFFER_SIZE],
    /// Next average write/eviction slot.
    avg_index: usize,
    /// Initialized average slots, at most sixty.
    avg_count: usize,
    /// Running sum of initialized average slots; finite sample bound limits magnitude.
    avg_sum: f32,
    /// Timestamp of last average admission, absent after reset.
    avg_last_sample_ms: Option<u64>,

    /// Independent elapsed-time bucket history with gaps and checked revision.
    graph: History,
}

impl SensorState {
    #[cfg(test)]
    /// Forces graph `revision` for exhaustion fixtures without altering graph contents.
    pub(crate) fn force_revision_for_test(
        &mut self,
        revision: u32,
    ) {
        self.graph.force_revision(revision);
    }

    /// Returns checked graph revision, or sticky `None` after exhaustion.
    pub(crate) const fn graph_revision(&self) -> Option<u32> { self.graph.revision() }

    /// Returns empty statistics with default trend threshold and 120-second graph.
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

    /// Returns last accepted canonical value without source/freshness admission.
    pub const fn value(&self) -> Option<f32> { self.value }

    /// Returns last accepted sample's monotonic millisecond timestamp.
    pub const fn sampled_at_ms(&self) -> Option<u64> { self.last_sample_ms }

    /// Returns a coherent presentation copy without graph-cache proof.
    /// Panics if the private finite-value/statistics invariant is violated.
    pub fn snapshot(&self) -> SensorSnapshot { self.snapshot_with_identity(None) }

    /// Returns a presentation copy with exact graph identity from optional
    /// acquisition `origin`, current graph revision and `channel`; either missing
    /// origin/revision disables proof while preserving numeric data.
    /// Panics if the private finite-value/statistics invariant is violated.
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

    /// Returns all presentation fields with supplied `graph_identity`; caller
    /// guarantees any identity corresponds to these current graph contents.
    /// Panics on nonfinite numeric state instead of silently replacing it with absence.
    /// Raw/extrema admission bounds values to +/-1,000,000; the mean uses at most
    /// sixty samples and checks the finite invariant again at this copy boundary.
    fn snapshot_with_identity(
        &self,
        graph_identity: Option<GraphCacheIdentity>,
    ) -> SensorSnapshot {
        SensorSnapshot {
            value: FiniteOptional::try_from_option(self.value).expect("accepted snapshot value must be finite"),
            minimum: FiniteOptional::try_from_option(self.minimum()).expect("snapshot minimum must be finite"),
            maximum: FiniteOptional::try_from_option(self.maximum()).expect("snapshot maximum must be finite"),
            average: FiniteOptional::try_from_option(self.get_average()).expect("snapshot average must be finite"),
            trend: self.get_trend(),
            sampled_at_ms: self.last_sample_ms,
            peak_until_ms: self.peak_until_ms,
            graph: self.graph.data(),
            graph_identity,
        }
    }

    /// Returns minimum since reset, or `None` before a valid sample.
    pub fn minimum(&self) -> Option<f32> { self.extrema.min() }

    /// Returns maximum since reset, or `None` before a valid sample.
    pub fn maximum(&self) -> Option<f32> { self.extrema.max() }

    /// Apply all measurement-derived state through the same acceptance gate.
    /// Returns whether canonical `value` at `sampled_at_ms` milliseconds is finite,
    /// within +/-1,000,000 and strictly newer. `peak_mode` selects highlight policy;
    /// first valid data initializes extrema without producing a highlight.
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

    /// Returns empty state with significant trend difference `threshold` in
    /// canonical units; negative/nonfinite input retains the default threshold.
    pub const fn with_trend_threshold(threshold: f32) -> Self {
        let mut state = Self::new();
        if threshold.is_finite() && threshold >= 0.0 {
            state.trend_threshold = threshold;
        }
        state
    }

    /// Returns false for an invalid value or a duplicate/out-of-order measurement.
    /// On success, stores canonical `value` at `sampled_at_ms` milliseconds and
    /// updates extrema, graph and due trend/average samples. `is_max_updated`
    /// requests a 750 ms highlight; caller determines its semantic validity.
    /// No catch-up samples are synthesized. Rejection leaves all state unchanged.
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

    /// Advances graph gaps at `now_ms` milliseconds and expires highlights. Graph
    /// clock reversal clears/reanchors history; highlight timing ignores reversed
    /// ticks. Does not insert values into averages, trends or extrema.
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

    /// Inserts already validated canonical `value` into the sixty-slot average,
    /// evicting the oldest when full. Caller enforces five-second sample spacing.
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

    /// Returns arithmetic mean of admitted average samples, or `None` when empty;
    /// elapsed gaps are not filled or weighted as repeated measurements.
    pub fn get_average(&self) -> Option<f32> {
        if self.avg_count == 0 {
            None
        } else {
            Some(self.avg_sum / self.avg_count as f32)
        }
    }

    /// Clears only the rolling average and its sampling clock, preserving raw data.
    pub fn reset_average(&mut self) {
        self.avg_buffer = [0.0; AVG_BUFFER_SIZE];
        self.avg_index = 0;
        self.avg_count = 0;
        self.avg_sum = 0.0;
        self.avg_last_sample_ms = None;
    }

    /// Returns current immutable graph values, timestamps and explicit gaps.
    pub fn history(&self) -> HistoryView<'_> { self.graph.view() }

    /// Sets graph duration to `seconds`; changed duration clears/reanchors graph
    /// only. Panics unless 10..=900; statistics and raw values remain untouched.
    pub fn set_history_duration(
        &mut self,
        seconds: u16,
    ) {
        self.graph.set_duration(seconds);
    }

    /// Creates a graph gap at monotonic event `now_ms` and its preceding retained
    /// bucket, without altering last-good numeric statistics or their timestamp.
    pub fn invalidate_history(
        &mut self,
        now_ms: u64,
    ) {
        self.graph.invalidate(now_ms);
    }

    /// Clears graph validity/duplicate history while retaining its duration/clock.
    pub fn reset_graph(&mut self) { self.graph.clear(); }

    /// Clears the legacy new-extremum highlight and its expiry deadline.
    pub fn reset_peak(&mut self) {
        self.is_new_peak = false;
        self.peak_until_ms = None;
    }

    /// Empties trend sampling history; next accepted sample can seed a new window.
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

    /// Returns true for significantly rising or false for falling window means,
    /// or `None` without both windows or when difference <=threshold. Windows
    /// are ages 0..=2 seconds and >2..=4 seconds relative to last accepted sample;
    /// callers separately expire that sample against the current clock.
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

/// Returns whether no `last_ms` exists or `sampled_at_ms` is at least `period_ms`
/// later; all are milliseconds. Caller guarantees sampled time >= last, avoiding
/// unsigned underflow. Only one sample is taken regardless of missed periods.
fn sample_due(
    last_ms: Option<u64>,
    sampled_at_ms: u64,
    period_ms: u64,
) -> bool {
    last_ms.is_none_or(|last| sampled_at_ms - last >= period_ms)
}

impl Default for SensorState {
    /// Returns empty state with the default trend threshold and graph duration.
    fn default() -> Self { Self::new() }
}
