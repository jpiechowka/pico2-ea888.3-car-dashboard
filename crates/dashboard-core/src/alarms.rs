//! Source-scoped policies. No vehicle alarm is enabled without an explicit rule.
//! Dwell advances on new measurements, never on repeated display frames.
use crate::acquisition::DashboardSnapshot;
use crate::samples::{DataMode, SampleMeta, SensorId, Source};

/// Per-widget immutable policy binding, episode memory and acknowledgment fencing.
mod configured;
pub(crate) use configured::RuleEngineState;
pub use configured::{RuleAlerts, RuleEngine, RuleIndex};

/// Compact, already source/quality/time-checked view; no history copy for policy evaluation.
pub(crate) struct Inputs<'a> {
    /// Source/sequence/capture metadata corresponding to each qualified value.
    pub metadata: &'a [SampleMeta; 8],
    /// Already freshness/source/mode/timestamp-qualified canonical channel values.
    pub values: [Option<f32>; 8],
    /// Immutable per-channel freshness milliseconds, capped again for contextual rules.
    pub freshness: &'a [u16; 8],
}

/// Canonical-unit hysteresis conditions, independently qualified by source/context.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Limit {
    /// Activate at/above on, clear at/below off; not permitted for raw lambda.
    High {
        /// Inclusive activation threshold; greater than off.
        on: f32,
        /// Inclusive clear threshold; less than on.
        off: f32,
    },
    /// Activate at/below on, clear at/above off; not permitted for raw lambda.
    Low {
        /// Inclusive activation threshold; less than off.
        on: f32,
        /// Inclusive clear threshold; greater than on.
        off: f32,
    },
    /// Activate outside inclusive outer bounds, clear inside inclusive inner bounds.
    OutsideWindow {
        /// Low activation edge, below low_off.
        low_on: f32,
        /// Low clear edge, no greater than high_off.
        low_off: f32,
        /// High activation edge, above high_off.
        high_on: f32,
        /// High clear edge, no less than low_off.
        high_off: f32,
    },
    /// Activate inside inclusive inner bounds, clear outside inclusive outer bounds.
    InsideWindow {
        /// Low activation edge, above low_off and no greater than high_on.
        low_on: f32,
        /// Low clear edge, below low_on.
        low_off: f32,
        /// High activation edge, below high_off and no less than low_on.
        high_on: f32,
        /// High clear edge, above high_on.
        high_off: f32,
    },
    /// Absolute lambda deviation from a qualified explicit target under load.
    LambdaDeviation {
        /// Inclusive deviation activation threshold, greater than off.
        on: f32,
        /// Inclusive nonnegative deviation clear threshold.
        off: f32,
        /// Inclusive required operating load percentage in 0..=100.
        min_load: f32,
    },
}
/// Invalid rule construction/admission; no partially constructed engine is returned.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PolicyError {
    /// Nonfinite, inverted or channel-inappropriate hysteresis parameters.
    Threshold,
    /// Provenance cannot supply the rule's canonical channel/mode.
    Source,
    /// On/off dwell is zero or greater than 30,000 milliseconds.
    Dwell,
    /// Human-supplied evidence/reference text is empty after trimming.
    Evidence,
    /// A per-channel rule was inserted into another channel's array slot.
    Slot,
}

/// Validated source-scoped rule; a reference string does not itself prove ECU qualification.
#[derive(Clone, Copy, Debug)]
pub struct Rule {
    /// Canonical channel evaluated by this rule.
    sensor: SensorId,
    /// Exact provenance required for every admitted measurement.
    source: Source,
    /// Canonical-unit activation/clear hysteresis.
    limit: Limit,
    /// Consecutive qualified-measurement activation dwell in milliseconds.
    on_ms: u64,
    /// Consecutive qualified-measurement clear dwell in milliseconds.
    off_ms: u64,
    /// Nonempty source/user-policy reference retained for inspection, not authenticated.
    reference: &'static str,
}
impl Rule {
    /// Returns whether lambda/ECU-voltage/EGT needs independently verified operating context.
    const fn contextual(self) -> bool { matches!(self.sensor, SensorId::Lambda | SensorId::Battery | SensorId::Egt) }

    /// Returns a rule for `sensor` from exact `source`, with `limit` hysteresis,
    /// `on_ms`/`off_ms` dwell in 1..=30000, and nonblank `reference` evidence text.
    /// Rejects mismatched source, nonfinite/misordered thresholds, wrong lambda
    /// mode, invalid dwell or empty reference; does not qualify a vehicle threshold.
    pub fn new(
        sensor: SensorId,
        source: Source,
        limit: Limit,
        on_ms: u64,
        off_ms: u64,
        reference: &'static str,
    ) -> Result<Self, PolicyError> {
        let mode = if source == Source::Demo {
            DataMode::Demo
        } else {
            DataMode::Vehicle
        };
        if !source.matches(sensor, mode) {
            return Err(PolicyError::Source);
        }
        let valid = match limit {
            Limit::High { on, off } => on.is_finite() && off.is_finite() && on > off && sensor != SensorId::Lambda,
            Limit::Low { on, off } => on.is_finite() && off.is_finite() && on < off && sensor != SensorId::Lambda,
            Limit::OutsideWindow {
                low_on,
                low_off,
                high_on,
                high_off,
            } => {
                [low_on, low_off, high_on, high_off].iter().all(|v| v.is_finite())
                    && low_on < low_off
                    && low_off <= high_off
                    && high_off < high_on
                    && sensor != SensorId::Lambda
            }
            Limit::InsideWindow {
                low_on,
                low_off,
                high_on,
                high_off,
            } => {
                [low_on, low_off, high_on, high_off].iter().all(|v| v.is_finite())
                    && low_off < low_on
                    && low_on <= high_on
                    && high_on < high_off
                    && sensor != SensorId::Lambda
            }
            Limit::LambdaDeviation { on, off, min_load } => {
                on.is_finite()
                    && off.is_finite()
                    && on > off
                    && off >= 0.0
                    && min_load.is_finite()
                    && (0.0..=100.0).contains(&min_load)
                    && sensor == SensorId::Lambda
            }
        };
        if !valid {
            return Err(PolicyError::Threshold);
        }
        if on_ms == 0 || off_ms == 0 || on_ms > 30_000 || off_ms > 30_000 {
            return Err(PolicyError::Dwell);
        }
        if reference.trim().is_empty() {
            return Err(PolicyError::Evidence);
        }
        Ok(Self {
            sensor,
            source,
            limit,
            on_ms,
            off_ms,
            reference,
        })
    }

    /// Returns the original untrimmed reference text supplied when constructing the rule.
    pub const fn reference(&self) -> &'static str { self.reference }
}

/// EGT measurement semantics; only measured manifold context admits a legacy EGT rule.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EgtLocation {
    /// No qualified location/model information.
    Unknown,
    /// Qualified physical measurement at the exhaust manifold.
    MeasuredManifold,
    /// Catalyst-associated temperature, not interchangeable with manifold measurement.
    Catalyst,
    /// Computed temperature rather than the required physical manifold sensor.
    Modeled,
}

/// Context is explicitly bound to the exact measured sample and source. The
/// acquisition owner must establish the operating inputs from verified signals.
#[derive(Clone, Copy, Debug)]
pub struct Context {
    /// Exact provenance of the associated canonical sample.
    pub source: Source,
    /// Associated sample's per-channel producer sequence.
    pub sequence: u64,
    /// Context observation time in monotonic milliseconds; source sample skew <=250 ms.
    pub received_at_ms: u64,
    /// Verified engine-running signal, absent when unknown.
    pub running: Option<bool>,
    /// Verified warm operating state required for lambda deviation policy.
    pub warm: Option<bool>,
    /// Verified finite operating load in 0..=100 percent, if known.
    pub load_percent: Option<f32>,
    /// Verified fuel-cut state; lambda rules require explicit false.
    pub fuel_cut: Option<bool>,
    /// Qualified dimensionless target lambda; admission requires 0.01..=2.0.
    pub target_lambda: Option<f32>,
    /// Physical/model semantics of an EGT sample.
    pub egt_location: EgtLocation,
}

/// Raw decision state; remembered latch/acknowledgment/snooze is tracked separately.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    /// No enabled rule in this slot/mode.
    Disabled,
    /// Enabled rule lacks current qualified measurement/context continuity.
    Unavailable,
    /// Qualified condition is not active and is not in activation dwell.
    Normal,
    /// Qualified activation dwell is accumulating from distinct measurements.
    Pending,
    /// Qualified active condition, including clear dwell until it completes.
    Active,
}

/// Copyable legacy per-channel policy publication with independent expiry guards.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Alerts {
    /// Raw engine status in canonical channel order.
    states: [Status; 8],
    /// Inclusive freshness/context deadlines in monotonic milliseconds.
    expires: [u64; 8],
    /// Last non-reversed engine evaluation time; readers reject earlier times.
    evaluated_at: u64,
}
impl Alerts {
    /// All slots disabled with no measurements or timing evidence required.
    pub const DISABLED: Self = Self {
        states: [Status::Disabled; 8],
        expires: [0; 8],
        evaluated_at: 0,
    };

    /// Rendering may suppress expired alarms but must never advance dwell.
    /// Returns a copy qualified against coherent `samples` at monotonic `now`
    /// milliseconds; does not modify the acquisition engine or sample metadata.
    pub fn at(
        mut self,
        samples: &DashboardSnapshot,
        now: u64,
    ) -> Self {
        for id in SensorId::ALL {
            let i = id as usize;
            self.states[i] = self.status_at(samples, id, now);
        }
        self
    }

    /// Returns status for `id` using current `samples` at `now` milliseconds;
    /// a reversed clock, expired deadline or absent value makes enabled rules
    /// Unavailable. Disabled slots remain Disabled regardless of data.
    pub(crate) fn status_at(
        &self,
        samples: &DashboardSnapshot,
        id: SensorId,
        now: u64,
    ) -> Status {
        let status = self.states[id as usize];
        if status != Status::Disabled
            && (now < self.evaluated_at || now > self.expires[id as usize] || samples.value_at(id, now).is_none())
        {
            Status::Unavailable
        } else {
            status
        }
    }

    /// Returns whether stored channel `id` status is Active; does not refresh expiry.
    pub const fn active(
        self,
        id: SensorId,
    ) -> bool {
        matches!(self.states[id as usize], Status::Active)
    }

    /// Returns stored raw status for `id`, without present-time qualification.
    pub const fn status(
        self,
        id: SensorId,
    ) -> Status {
        self.states[id as usize]
    }

    /// Returns the first stored Active channel in fixed legacy display priority,
    /// or `None`; caller qualifies expiry first. This is not calibrated severity.
    pub fn priority(self) -> Option<SensorId> {
        [
            SensorId::Water,
            SensorId::Oil,
            SensorId::Dsg,
            SensorId::Egt,
            SensorId::Lambda,
            SensorId::Battery,
            SensorId::Boost,
            SensorId::Iat,
        ]
        .into_iter()
        .find(|&id| self.active(id))
    }
}

// A compact state per rule: four timestamps/counters plus flag bits. No
// sentinel timestamp is reserved, so u64::MAX remains representable.
/// Compact dwell/episode substrate shared by legacy and configured policies.
#[derive(Clone, Copy)]
struct State {
    /// Last consumed channel sequence, meaningful when SEEN is set.
    sequence: u64,
    /// Last consumed sample timestamp in milliseconds, meaningful when SEEN is set.
    at: u64,
    /// Last qualified context timestamp, meaningful when CONTEXT is set.
    context_at: u64,
    /// Beginning of current activation/clear dwell in milliseconds, when PENDING.
    since: u64,
    /// Validity/dwell/raw and remembered episode flags; no timestamp sentinel required.
    flags: u16,
}
impl Default for State {
    /// Returns unseen, invalid and inactive state with no remembered episode.
    fn default() -> Self { Self::EMPTY }
}
/// A sample identity has been consumed, including rejected-context samples.
const SEEN: u16 = 1;
/// Prior qualified contextual timestamp is available for continuity checks.
const CONTEXT: u16 = 2;
/// Current raw decision has completed activation dwell.
const ACTIVE: u16 = 4;
/// A qualifying activation/clear dwell is in progress.
const PENDING: u16 = 8;
/// Raw decision currently has qualified measurement/context inputs.
const VALID: u16 = 16;
/// Configured episode remains remembered until qualified clear plus acknowledgment.
const LATCHED: u16 = 32;
/// Current episode has received a qualified acknowledgment.
const ACKNOWLEDGED: u16 = 64;
/// Current/remembered episode completed qualified clear dwell.
const CLEARED: u16 = 128;
/// Pending dwell is clearing rather than activating.
const PENDING_CLEAR: u16 = 256;

impl State {
    /// Unseen state; all raw and remembered flags clear.
    const EMPTY: Self = Self {
        sequence: 0,
        at: 0,
        context_at: 0,
        since: 0,
        flags: 0,
    };

    /// Returns last consumed `(sequence, timestamp_ms)`, or `None` before consumption.
    fn seen(&self) -> Option<(u64, u64)> { (self.flags & SEEN != 0).then_some((self.sequence, self.at)) }

    /// Records consumed `sequence` at monotonic sample `at` milliseconds; callers
    /// establish ordering before invoking this unchecked identity assignment.
    fn record(
        &mut self,
        sequence: u64,
        at: u64,
    ) {
        self.flags |= SEEN;
        self.sequence = sequence;
        self.at = at;
    }

    /// Consumes a newer source-matching value from `samples` for `rule` so rejected
    /// context cannot later backdate dwell. `_now` is intentionally unused because
    /// value qualification already happened upstream; no raw status is promoted.
    fn consume(
        &mut self,
        rule: Rule,
        samples: &Inputs<'_>,
        _now: u64,
    ) {
        let meta = samples.metadata[rule.sensor as usize];
        if meta.source == rule.source
            && samples.values[rule.sensor as usize].is_some()
            && let Some(at) = meta.received_at_ms
            && self.seen().is_none_or(|(seq, time)| meta.sequence > seq && at >= time)
        {
            self.record(meta.sequence, at);
        }
    }

    /// Clears raw validity/activation/dwell while retaining consumed identity and
    /// configured remembered episode flags for explicit acknowledgment/clear.
    fn invalidate(&mut self) { self.flags &= !(ACTIVE | PENDING | VALID | PENDING_CLEAR); }

    /// Returns Unavailable, Active, Pending or Normal in precedence order from flags.
    fn status(&self) -> Status {
        if self.flags & VALID == 0 {
            Status::Unavailable
        } else if self.flags & ACTIVE != 0 {
            Status::Active
        } else if self.flags & PENDING != 0 {
            Status::Pending
        } else {
            Status::Normal
        }
    }

    /// Evaluates `rule` with qualified `samples`, optional verified `context`, and
    /// monotonic `now` milliseconds; returns updated raw status and writes `expires`
    /// only on a newly qualified measurement. Replays never advance dwell; source/
    /// context failure and time/gap violations invalidate raw state. Joint dwell
    /// spans later first timestamp through earlier latest timestamp; remembered
    /// latch clear may qualify after raw freshness was lost.
    fn advance(
        &mut self,
        rule: Rule,
        samples: &Inputs<'_>,
        context: Option<Context>,
        now: u64,
        expires: &mut u64,
    ) -> Status {
        let Some(value) = AlarmEngine::value(rule, samples, context, now) else {
            // Consume invalid context too: repairing it cannot backdate dwell.
            self.consume(rule, samples, now);
            self.invalidate();
            return Status::Unavailable;
        };
        let channel = rule.sensor as usize;
        let meta = samples.metadata[channel];
        let at = meta.received_at_ms.unwrap(); // checked by value_at
        if let Some((sequence, previous_at)) = self.seen() {
            if meta.sequence < sequence || at < previous_at || (meta.sequence == sequence && at != previous_at) {
                self.invalidate();
                return Status::Unavailable;
            }
            if meta.sequence == sequence {
                return self.status();
            }
            let max_gap = (if rule.contextual() {
                500
            } else {
                rule.sensor.stale_after_ms()
            })
            .min(u64::from(samples.freshness[channel]));
            if at - previous_at > max_gap {
                self.invalidate();
            }
        }
        self.record(meta.sequence, at);
        let context_at = if rule.contextual() {
            context.map(|c| c.received_at_ms)
        } else {
            None
        };
        if let Some(current) = context_at {
            if self.flags & CONTEXT != 0 {
                if current < self.context_at {
                    self.invalidate();
                    return Status::Unavailable;
                }
                if current - self.context_at > u64::from(samples.freshness[channel]).min(500) {
                    self.invalidate();
                }
            }
            self.context_at = current;
            self.flags |= CONTEXT;
        } else {
            self.flags &= !CONTEXT;
        }
        self.flags |= VALID;
        *expires = at
            .saturating_add(rule.sensor.stale_after_ms().min(u64::from(samples.freshness[channel])))
            .min(context_at.map_or(u64::MAX, |at| {
                at.saturating_add(u64::from(samples.freshness[channel]).min(500))
            }));
        let condition_at = at.max(context_at.unwrap_or(at));
        let (trigger, clear) = match rule.limit {
            Limit::High { on, off } | Limit::LambdaDeviation { on, off, .. } => (value >= on, value <= off),
            Limit::Low { on, off } => (value <= on, value >= off),
            Limit::OutsideWindow {
                low_on,
                low_off,
                high_on,
                high_off,
            } => {
                (
                    value <= low_on || value >= high_on,
                    value >= low_off && value <= high_off,
                )
            }
            Limit::InsideWindow {
                low_on,
                low_off,
                high_on,
                high_off,
            } => {
                (
                    value >= low_on && value <= high_on,
                    value <= low_off || value >= high_off,
                )
            }
        };
        let active = self.flags & ACTIVE != 0;
        // An uncleared episode may qualify an off dwell after raw freshness was
        // lost. This uses the same admitted samples and pending timestamp, without
        // manufacturing raw Active or skipping the next on dwell.
        let clearing = active || (self.flags & (LATCHED | CLEARED) == LATCHED && clear);
        if if clearing { clear } else { trigger } {
            if self.flags & PENDING == 0 || (self.flags & PENDING_CLEAR != 0) != clearing {
                self.since = condition_at;
                self.flags |= PENDING;
                if clearing {
                    self.flags |= PENDING_CLEAR;
                } else {
                    self.flags &= !PENDING_CLEAR;
                }
            }
            let supported_until = at.min(context_at.unwrap_or(at));
            if supported_until.saturating_sub(self.since) >= if clearing { rule.off_ms } else { rule.on_ms } {
                if clearing {
                    self.flags &= !ACTIVE;
                    self.flags |= CLEARED;
                } else {
                    self.flags |= ACTIVE;
                    self.flags &= !CLEARED;
                }
                self.flags &= !(PENDING | PENDING_CLEAR);
            }
        } else {
            self.flags &= !PENDING;
        }
        self.status()
    }
}

/// The acquisition owner must submit every accepted sample and fault before
/// latest-value publication; sequence gaps cannot identify omitted observations.
pub struct AlarmEngine {
    /// Immutable optional rules in matching channel slots.
    rules: [Option<Rule>; 8],
    /// Single-owner per-rule consumption and dwell state.
    states: [State; 8],
    /// Last published raw statuses and expiry deadlines.
    alerts: Alerts,
    /// Greatest admitted evaluation clock in milliseconds.
    last_ms: u64,
    /// Whether at least one rule exists; permits cheap no-policy acquisition path.
    enabled: bool,
}
impl AlarmEngine {
    /// Empty constant state used by the construction-only legacy Vehicle base.
    pub(crate) const DISABLED: Self = Self {
        rules: [None; 8],
        states: [State::EMPTY; 8],
        alerts: Alerts::DISABLED,
        last_ms: 0,
        enabled: false,
    };

    /// Returns an engine for validated `rules` in canonical channel order, with
    /// present rules initially Unavailable; returns Slot if any rule is misplaced.
    pub fn new(rules: [Option<Rule>; 8]) -> Result<Self, PolicyError> {
        for (i, rule) in rules.iter().enumerate() {
            if rule.is_some_and(|r| r.sensor as usize != i) {
                return Err(PolicyError::Slot);
            }
        }
        Ok(Self {
            rules,
            states: [State::default(); 8],
            alerts: Alerts {
                states: rules.map(|rule| {
                    if rule.is_some() {
                        Status::Unavailable
                    } else {
                        Status::Disabled
                    }
                }),
                ..Alerts::DISABLED
            },
            last_ms: 0,
            enabled: rules.iter().any(Option::is_some),
        })
    }

    /// Returns raw published alerts; readers must qualify their expiry at display time.
    pub const fn snapshot(&self) -> Alerts { self.alerts }

    /// Returns whether any rule exists, not whether current measurements qualify it.
    pub(crate) const fn enabled(&self) -> bool { self.enabled }

    /// Returns the canonical policy value for `rule` from qualified `samples` at
    /// `now` milliseconds and optional `context`, or `None` on admission failure.
    /// Context must match source/sequence, age <=min(freshness,500 ms), skew <=250 ms.
    /// Lambda returns absolute target deviation after load/warm/running/fuel-cut
    /// checks; voltage requires running and EGT requires measured-manifold semantics.
    fn value(
        rule: Rule,
        samples: &Inputs<'_>,
        context: Option<Context>,
        now: u64,
    ) -> Option<f32> {
        let id = rule.sensor;
        let meta = samples.metadata[id as usize];
        if meta.source != rule.source {
            return None;
        }
        let value = samples.values[id as usize].filter(|v| v.is_finite())?;
        if !rule.contextual() {
            return Some(value);
        }
        let context = context?;
        if context.source != meta.source
            || context.sequence != meta.sequence
            || now.checked_sub(context.received_at_ms)? > u64::from(samples.freshness[id as usize]).min(500)
            || context.received_at_ms.abs_diff(meta.received_at_ms?) > 250
        {
            return None;
        }
        match id {
            SensorId::Battery if context.running == Some(true) => Some(value),
            SensorId::Egt if context.egt_location == EgtLocation::MeasuredManifold => Some(value),
            SensorId::Lambda => {
                let Limit::LambdaDeviation { min_load, .. } = rule.limit else {
                    return None;
                };
                let load = context.load_percent?;
                let target = context.target_lambda?;
                if context.running != Some(true)
                    || context.warm != Some(true)
                    || context.fuel_cut != Some(false)
                    || !load.is_finite()
                    || !(min_load..=100.0).contains(&load)
                    || !target.is_finite()
                    || !(0.01..=2.0).contains(&target)
                    || value <= 0.0
                {
                    return None;
                }
                Some((value - target).abs())
            }
            _ => None,
        }
    }

    /// Evaluates coherent `samples` with per-channel verified `contexts` at `now`
    /// milliseconds and returns updated alerts. Every admitted sample/fault must
    /// reach the owner before coalescing; repeated snapshots cannot advance dwell.
    pub fn update(
        &mut self,
        samples: &DashboardSnapshot,
        contexts: &[Option<Context>; 8],
        now: u64,
    ) -> Alerts {
        let input = Inputs {
            metadata: &samples.metadata,
            values: core::array::from_fn(|i| samples.value_at(SensorId::ALL[i], now)),
            freshness: &samples.freshness,
        };
        self.update_inputs(&input, contexts, now)
    }

    /// Evaluates already-qualified `samples` and verified `contexts` at monotonic
    /// `now` milliseconds, returning alerts. Clock reversal consumes qualifying
    /// identities and invalidates raw states while retaining the greatest clock;
    /// otherwise distinct measurements drive hysteresis/dwell and expiry.
    pub(crate) fn update_inputs(
        &mut self,
        samples: &Inputs<'_>,
        contexts: &[Option<Context>; 8],
        now: u64,
    ) -> Alerts {
        if now < self.last_ms {
            for (i, state) in self.states.iter_mut().enumerate() {
                if let Some(rule) = self.rules[i] {
                    state.consume(rule, samples, now);
                }
                state.invalidate();
                self.alerts.states[i] = if self.rules[i].is_some() {
                    Status::Unavailable
                } else {
                    Status::Disabled
                };
            }
            return self.alerts;
        }
        self.last_ms = now;
        self.alerts.evaluated_at = now;
        for (i, rule) in self.rules.iter().enumerate() {
            if let Some(rule) = rule {
                self.alerts.states[i] = self.states[i].advance(
                    *rule,
                    samples,
                    contexts[rule.sensor as usize],
                    now,
                    &mut self.alerts.expires[i],
                );
            }
        }
        self.alerts
    }
}

/// Synthetic UI exercise only. These values are not vehicle recommendations.
/// Engine oil and transmission oil deliberately have separate policy entries.
/// Returns source-Demo rules with 1000 ms on/500 ms off dwell; boost is disabled.
/// Panics if these fixed fixtures cease satisfying rule construction invariants.
pub fn demo_rules() -> [Option<Rule>; 8] {
    use SensorId::*;
    let mut rules = [None; 8];
    for (id, limit) in [
        (Water, Limit::High { on: 110.0, off: 105.0 }),
        (Oil, Limit::High { on: 130.0, off: 125.0 }),
        (Dsg, Limit::High { on: 120.0, off: 115.0 }),
        (Iat, Limit::High { on: 60.0, off: 55.0 }),
        (
            Egt,
            Limit::High {
                on: 1050.0,
                off: 1000.0,
            },
        ),
        (Battery, Limit::Low { on: 11.5, off: 12.0 }),
        (
            Lambda,
            Limit::LambdaDeviation {
                on: 0.15,
                off: 0.05,
                min_load: 60.0,
            },
        ),
    ] {
        rules[id as usize] =
            Some(Rule::new(id, Source::Demo, limit, 1000, 500, "synthetic UI fixture").expect("invalid demo rule"));
    }
    rules
}

/// Returns synthetic running/warm/load80/target1 context bound to `samples` only
/// for Demo source/mode entries with receive times; other channels return `None`.
pub fn demo_contexts(samples: &DashboardSnapshot) -> [Option<Context>; 8] {
    demo_contexts_for(samples.mode, &samples.metadata)
}

/// Returns per-channel synthetic context from explicit `mode` and `metadata`,
/// preserving each sample's sequence/time; requires Demo mode/source and a timestamp.
pub(crate) fn demo_contexts_for(
    mode: DataMode,
    metadata: &[SampleMeta; 8],
) -> [Option<Context>; 8] {
    core::array::from_fn(|i| {
        let meta = metadata[i];
        if mode != DataMode::Demo || meta.source != Source::Demo {
            return None;
        }
        Some(Context {
            source: Source::Demo,
            sequence: meta.sequence,
            received_at_ms: meta.received_at_ms?,
            running: Some(true),
            warm: Some(true),
            load_percent: Some(80.0),
            fuel_cut: Some(false),
            target_lambda: Some(1.0),
            egt_location: EgtLocation::MeasuredManifold,
        })
    })
}

#[cfg(test)]
/// Regression for constant construction state matching the original constructors.
mod construction_tests {
    use super::*;
    /// Returns unit after comparing constant/default empty state and disabled
    /// engine fields; panics if construction optimization changes runtime initialization.
    #[test]
    fn constant_state_and_disabled_engine_match_original_default_fields() {
        let original = State::default();
        for state in [original, State::EMPTY] {
            assert_eq!(
                (state.sequence, state.at, state.context_at, state.since, state.flags),
                (0, 0, 0, 0, 0)
            );
        }
        let original = AlarmEngine::new([None; 8]).unwrap();
        for engine in [original, AlarmEngine::DISABLED] {
            assert!(engine.rules.iter().all(Option::is_none));
            assert_eq!(engine.alerts, Alerts::DISABLED);
            assert_eq!(engine.last_ms, 0);
            assert!(!engine.enabled);
            for state in engine.states {
                assert_eq!(
                    (state.sequence, state.at, state.context_at, state.since, state.flags),
                    (0, 0, 0, 0, 0)
                );
            }
        }
    }
}
