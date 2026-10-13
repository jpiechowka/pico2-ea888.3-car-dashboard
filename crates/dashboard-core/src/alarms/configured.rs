//! Widget indices and channel indices are deliberately distinct types.
use dashboard_config::{Applicability, LimitMode, MAX_WIDGETS, ValidatedConfig};

use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RuleIndex(u8);
impl RuleIndex {
    pub const fn new(index: usize) -> Option<Self> {
        if index < MAX_WIDGETS {
            Some(Self(index as u8))
        } else {
            None
        }
    }

    pub const fn index(self) -> usize { self.0 as usize }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RuleAlerts {
    states: [Status; MAX_WIDGETS],
    expires: [u64; MAX_WIDGETS],
    // Two 3-bit channel IDs per byte; avoids copying configuration in Watch.
    channels: [u8; MAX_WIDGETS / 2],
    evaluated_at: u64,
}
impl RuleAlerts {
    pub const DISABLED: Self = Self {
        states: [Status::Disabled; MAX_WIDGETS],
        expires: [0; MAX_WIDGETS],
        channels: [0; MAX_WIDGETS / 2],
        evaluated_at: 0,
    };

    pub(crate) fn status_at(
        &self,
        index: RuleIndex,
        samples: &DashboardSnapshot,
        now: u64,
    ) -> Status {
        let i = index.index();
        let channel = (self.channels[i / 2] >> ((i % 2) * 4)) & 7;
        let status = self.states[i];
        if status != Status::Disabled
            && (now < self.evaluated_at
                || now > self.expires[i]
                || samples.value_at(SensorId::ALL[channel as usize], now).is_none())
        {
            Status::Unavailable
        } else {
            status
        }
    }
}

pub(crate) struct RuleEngineState {
    states: [State; MAX_WIDGETS],
    alerts: RuleAlerts,
    sources: [Option<Source>; 8],
    enabled: u64,
    last_ms: u64,
}
impl RuleEngineState {
    pub(crate) fn new(
        config: &ValidatedConfig,
        mode: DataMode,
        sources: [Option<Source>; 8],
    ) -> Self {
        let mut result = Self {
            states: [State::default(); MAX_WIDGETS],
            alerts: RuleAlerts::DISABLED,
            sources,
            enabled: 0,
            last_ms: 0,
        };
        for (i, widget) in config.config().widgets.iter().enumerate() {
            result.alerts.channels[i / 2] |= (widget.channel.index() as u8) << ((i % 2) * 4);
            if widget.warning.enabled
                && (mode == DataMode::Demo || widget.warning.applicability == Applicability::VehicleAndDemo)
            {
                result.enabled |= 1 << i;
                result.alerts.states[i] = Status::Unavailable;
            }
        }
        result
    }

    pub(crate) const fn snapshot(&self) -> RuleAlerts { self.alerts }

    pub(crate) const fn enabled(&self) -> bool { self.enabled != 0 }

    fn rule(
        &self,
        config: &ValidatedConfig,
        i: usize,
    ) -> Option<Rule> {
        let widget = &config.config().widgets[i];
        let source = self.sources[widget.channel.index()]?;
        let warning = &widget.warning;
        let limit = match warning.mode {
            LimitMode::OutsideWindow | LimitMode::InsideWindow => return None,
            LimitMode::High => {
                Limit::High {
                    on: warning.trigger,
                    off: warning.clear,
                }
            }
            LimitMode::Low => {
                Limit::Low {
                    on: warning.trigger,
                    off: warning.clear,
                }
            }
            LimitMode::LambdaDeviation => {
                Limit::LambdaDeviation {
                    on: warning.trigger,
                    off: warning.clear,
                    min_load: warning.min_load_percent,
                }
            }
        };
        // The immutable validated document establishes numeric invariants;
        // admission still binds every measurement to this channel/source.
        Some(Rule {
            sensor: SensorId::from_channel(widget.channel),
            source,
            limit,
            on_ms: warning.on_ms.into(),
            off_ms: warning.off_ms.into(),
            reference: "configured user threshold",
        })
    }

    pub(crate) fn update_inputs(
        &mut self,
        config: &ValidatedConfig,
        samples: &Inputs<'_>,
        contexts: &[Option<Context>; 8],
        now: u64,
    ) {
        let backwards = now < self.last_ms;
        if !backwards {
            self.last_ms = now;
            self.alerts.evaluated_at = now;
        }
        let mut enabled = self.enabled;
        while enabled != 0 {
            let i = enabled.trailing_zeros() as usize;
            enabled &= enabled - 1;
            let Some(rule) = self.rule(config, i) else {
                continue;
            };
            let state = &mut self.states[i];
            self.alerts.states[i] = if backwards {
                state.consume(rule, samples, now);
                state.invalidate();
                Status::Unavailable
            } else {
                state.advance(
                    rule,
                    samples,
                    contexts[rule.sensor as usize],
                    now,
                    &mut self.alerts.expires[i],
                )
            };
        }
    }
}

/// Borrowed policy facade. Its configuration cannot be replaced independently of state.
pub struct RuleEngine<'a> {
    config: &'a ValidatedConfig,
    state: RuleEngineState,
}
impl<'a> RuleEngine<'a> {
    pub fn new(
        config: &'a ValidatedConfig,
        mode: DataMode,
        sources: [Option<Source>; 8],
    ) -> Self {
        Self {
            config,
            state: RuleEngineState::new(config, mode, sources),
        }
    }

    pub const fn snapshot(&self) -> RuleAlerts { self.state.snapshot() }

    pub const fn configuration(&self) -> &ValidatedConfig { self.config }
}

const _: () = assert!(core::mem::size_of::<RuleEngine<'_>>() <= 4096);
const _: () = assert!(core::mem::size_of::<RuleAlerts>() <= 640);
