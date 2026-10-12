//! Source-scoped policies. No vehicle alarm is enabled without an explicit rule.
//! Dwell advances on new measurements, never on repeated display frames.
use crate::acquisition::DashboardSnapshot;
use crate::samples::{DataMode, SampleMeta, SensorId, Source};

/// Compact, already source/quality/time-checked view; no history copy for policy evaluation.
pub(crate) struct Inputs<'a> {
    pub metadata: &'a [SampleMeta; 8],
    pub values: [Option<f32>; 8],
    pub freshness: &'a [u16; 8],
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Limit {
    High { on: f32, off: f32 },
    Low { on: f32, off: f32 },
    LambdaDeviation { on: f32, off: f32, min_load: f32 },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PolicyError {
    Threshold,
    Source,
    Dwell,
    Evidence,
    Slot,
}

#[derive(Clone, Copy, Debug)]
pub struct Rule {
    sensor: SensorId,
    source: Source,
    limit: Limit,
    on_ms: u64,
    off_ms: u64,
    reference: &'static str,
}
impl Rule {
    const fn contextual(self) -> bool { matches!(self.sensor, SensorId::Lambda | SensorId::Battery | SensorId::Egt) }

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

    pub const fn reference(&self) -> &'static str { self.reference }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EgtLocation {
    Unknown,
    MeasuredManifold,
    Catalyst,
    Modeled,
}

/// Context is explicitly bound to the exact measured sample and source. The
/// acquisition owner must establish the operating inputs from verified signals.
#[derive(Clone, Copy, Debug)]
pub struct Context {
    pub source: Source,
    pub sequence: u64,
    pub received_at_ms: u64,
    pub running: Option<bool>,
    pub warm: Option<bool>,
    pub load_percent: Option<f32>,
    pub fuel_cut: Option<bool>,
    pub target_lambda: Option<f32>,
    pub egt_location: EgtLocation,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Disabled,
    Unavailable,
    Normal,
    Pending,
    Active,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Alerts {
    states: [Status; 8],
    expires: [u64; 8],
    evaluated_at: u64,
}
impl Alerts {
    pub const DISABLED: Self = Self {
        states: [Status::Disabled; 8],
        expires: [0; 8],
        evaluated_at: 0,
    };

    /// Rendering may suppress expired alarms but must never advance dwell.
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

    pub const fn active(
        self,
        id: SensorId,
    ) -> bool {
        matches!(self.states[id as usize], Status::Active)
    }

    pub const fn status(
        self,
        id: SensorId,
    ) -> Status {
        self.states[id as usize]
    }

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

#[derive(Clone, Copy, Default)]
struct State {
    seen: Option<(u64, u64)>,
    context_at: Option<u64>,
    active: bool,
    since: Option<u64>,
    valid: bool,
}
impl State {
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
            && self.seen.is_none_or(|(seq, time)| meta.sequence > seq && at >= time)
        {
            self.seen = Some((meta.sequence, at));
        }
    }

    fn invalidate(&mut self) {
        self.active = false;
        self.since = None;
        self.valid = false;
    }

    fn status(self) -> Status {
        if !self.valid {
            Status::Unavailable
        } else if self.active {
            Status::Active
        } else if self.since.is_some() {
            Status::Pending
        } else {
            Status::Normal
        }
    }
}

/// The acquisition owner must submit every accepted sample and fault before
/// latest-value publication; sequence gaps cannot identify omitted observations.
pub struct AlarmEngine {
    rules: [Option<Rule>; 8],
    states: [State; 8],
    alerts: Alerts,
    last_ms: u64,
    enabled: bool,
}
impl AlarmEngine {
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

    pub const fn snapshot(&self) -> Alerts { self.alerts }

    pub(crate) const fn enabled(&self) -> bool { self.enabled }

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
        for (i, context) in contexts.iter().enumerate() {
            let Some(rule) = self.rules[i] else {
                continue;
            };
            let state = &mut self.states[i];
            let Some(value) = Self::value(rule, samples, *context, now) else {
                // A valid measurement observed with invalid context is consumed.
                // Repairing its context later cannot resurrect or backdate dwell.
                state.consume(rule, samples, now);
                state.invalidate();
                self.alerts.states[i] = Status::Unavailable;
                continue;
            };
            let meta = samples.metadata[i];
            let at = meta.received_at_ms.unwrap(); // checked by value_at
            if let Some((sequence, previous_at)) = state.seen {
                if meta.sequence < sequence || at < previous_at || (meta.sequence == sequence && at != previous_at) {
                    state.invalidate();
                    self.alerts.states[i] = Status::Unavailable;
                    continue;
                }
                if meta.sequence == sequence {
                    self.alerts.states[i] = state.status();
                    continue;
                }
                let max_gap = (if rule.contextual() {
                    500
                } else {
                    rule.sensor.stale_after_ms()
                })
                .min(u64::from(samples.freshness[i]));
                if at - previous_at > max_gap {
                    state.invalidate();
                }
            }
            state.seen = Some((meta.sequence, at));
            let context_at = if rule.contextual() {
                context.map(|c| c.received_at_ms)
            } else {
                None
            };
            if let (Some(previous), Some(current)) = (state.context_at, context_at) {
                if current < previous {
                    state.invalidate();
                    self.alerts.states[i] = Status::Unavailable;
                    continue;
                }
                if current - previous > u64::from(samples.freshness[i]).min(500) {
                    state.invalidate();
                }
            }
            state.context_at = context_at;
            state.valid = true;
            self.alerts.expires[i] = at
                .saturating_add(rule.sensor.stale_after_ms().min(u64::from(samples.freshness[i])))
                .min(context_at.map_or(u64::MAX, |at| {
                    at.saturating_add(u64::from(samples.freshness[i]).min(500))
                }));
            // Both the measurement and its operating context must support the
            // entire dwell. Skew may never backdate the condition's start.
            let condition_at = at.max(context_at.unwrap_or(at));
            let (trigger, clear) = match rule.limit {
                Limit::High { on, off } | Limit::LambdaDeviation { on, off, .. } => (value >= on, value <= off),
                Limit::Low { on, off } => (value <= on, value >= off),
            };
            if if state.active { clear } else { trigger } {
                let since = *state.since.get_or_insert(condition_at);
                let supported_until = at.min(context_at.unwrap_or(at));
                if supported_until.saturating_sub(since) >= if state.active { rule.off_ms } else { rule.on_ms } {
                    state.active = !state.active;
                    state.since = None;
                }
            } else {
                state.since = None;
            }
            self.alerts.states[i] = state.status();
        }
        self.alerts
    }
}

/// Synthetic UI exercise only. These values are not vehicle recommendations.
/// Engine oil and transmission oil deliberately have separate policy entries.
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

pub fn demo_contexts(samples: &DashboardSnapshot) -> [Option<Context>; 8] {
    demo_contexts_for(samples.mode, &samples.metadata)
}

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
