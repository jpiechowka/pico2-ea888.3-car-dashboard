//! Widget indices and channel indices are deliberately distinct types.
use dashboard_config::{Applicability, LimitMode, MAX_WIDGETS, ValidatedConfig};

use super::*;
use crate::warning_state::{CommandResult, WarningCommand, WarningEligibility};

/// Valid index into the fixed 64-widget/rule storage, not a canonical channel index.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RuleIndex(
    /// Widget/rule index in 0..MAX_WIDGETS; construction bounds it.
    u8,
);
impl RuleIndex {
    /// Returns a bounded rule index for `index` <MAX_WIDGETS, otherwise `None`.
    pub const fn new(index: usize) -> Option<Self> {
        if index < MAX_WIDGETS {
            Some(Self(index as u8))
        } else {
            None
        }
    }

    /// Returns the validated zero-based widget/rule array index.
    pub const fn index(self) -> usize { self.0 as usize }
}

/// Coherent configured-rule publication; raw validity and remembered episodes differ.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RuleAlerts {
    /// Raw state per configured widget, disabled for unused slots.
    states: [Status; MAX_WIDGETS],
    /// Inclusive raw-validity deadlines in monotonic milliseconds.
    expires: [u64; MAX_WIDGETS],
    // Two 3-bit channel IDs per byte; avoids copying configuration in Watch.
    /// Two channel nibbles per byte; low three bits identify canonical channel.
    channels: [u8; MAX_WIDGETS / 2],
    /// Latest non-reversed raw-policy evaluation time in milliseconds.
    evaluated_at: u64,
    /// Remembered episode bits awaiting qualified clear and acknowledgment.
    latched: u64,
    /// Episode bits with qualified completed clear dwell.
    cleared: u64,
    /// Episode bits with accepted acknowledgment.
    acknowledged: u64,
    /// Episode bits with nonzero producer-managed snooze deadlines.
    snoozed: u64,
    /// Saturating activation watermark; MAX is terminal, not an issued episode ID.
    pub(crate) event_serial: u64,
    /// Last command result exposed to presentation, including rejected commands.
    pub(crate) result: CommandResult,
}
impl RuleAlerts {
    /// All rules disabled, no episodes, no command result and event watermark zero.
    pub const DISABLED: Self = Self {
        states: [Status::Disabled; MAX_WIDGETS],
        expires: [0; MAX_WIDGETS],
        channels: [0; MAX_WIDGETS / 2],
        evaluated_at: 0,
        latched: 0,
        cleared: 0,
        acknowledged: 0,
        snoozed: 0,
        event_serial: 0,
        result: CommandResult::EMPTY,
    };

    /// Returns the packed canonical channel for widget `index`; unused slots map
    /// to Boost but remain Disabled and have no held value.
    pub(crate) fn channel(
        &self,
        index: RuleIndex,
    ) -> SensorId {
        let i = index.index();
        SensorId::ALL[((self.channels[i / 2] >> ((i % 2) * 4)) & 7) as usize]
    }

    /// Returns raw status qualified with `samples` at `now` milliseconds plus
    /// remembered flags for widget `index`. Reader does not advance/expire snoozes;
    /// producer ticking owns those deadlines.
    pub(crate) fn eligibility(
        &self,
        index: RuleIndex,
        samples: &DashboardSnapshot,
        now: u64,
    ) -> WarningEligibility {
        let bit = 1u64 << index.index();
        WarningEligibility {
            status: self.status_at(index, samples, now),
            latched: self.latched & bit != 0,
            cleared: self.cleared & bit != 0,
            acknowledged: self.acknowledged & bit != 0,
            snoozed: self.snoozed & bit != 0,
        }
    }

    /// Returns widget `index` status qualified by coherent `samples` at `now`
    /// milliseconds. Reversed time, expiry or absent channel value makes enabled
    /// status Unavailable; Disabled remains Disabled and dwell is never advanced.
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

/// One configured episode's raw dwell, activation identity and snooze deadline.
#[derive(Clone, Copy)]
struct EpisodeState {
    /// Shared source/context/dwell state with remembered flags.
    raw: State,
    /// Nonzero activation identity, zero before activation or at terminal exhaustion.
    activation_serial: u64,
    /// Exclusive monotonic millisecond snooze expiry; zero means not snoozed.
    snooze_until_ms: u64,
}
impl Default for EpisodeState {
    /// Returns an empty unseen episode with no activation or snooze.
    fn default() -> Self { Self::EMPTY }
}
impl EpisodeState {
    /// Empty constant episode for allocation-free fixed array initialization.
    const EMPTY: Self = Self {
        raw: State::EMPTY,
        activation_serial: 0,
        snooze_until_ms: 0,
    };
}

/// Mutable policy state inseparable from its enclosing immutable configuration.
pub(crate) struct RuleEngineState {
    /// Per-widget raw decisions and remembered episodes.
    states: [EpisodeState; MAX_WIDGETS],
    /// Coherent published raw status/masks and command result.
    alerts: RuleAlerts,
    /// Exact source binding per canonical channel, absent when unqualified/disabled.
    sources: [Option<Source>; 8],
    /// Configured/applicable rule mask; source/context may still be unavailable.
    enabled: u64,
    /// Greatest accepted raw-evaluation time in milliseconds.
    last_ms: u64,
    /// Newest admitted command, used for exact idempotence and stale rejection.
    last_command: WarningCommand,
    /// Result cached for an exact repeat of last_command.
    last_result: CommandResult,
}
impl RuleEngineState {
    /// Empty construction storage; requires one immutable-policy initialization.
    pub(crate) const EMPTY: Self = Self {
        states: [EpisodeState::EMPTY; MAX_WIDGETS],
        alerts: RuleAlerts::DISABLED,
        sources: [None; 8],
        enabled: 0,
        last_ms: 0,
        last_command: WarningCommand {
            request_id: 0,
            run_id: 0,
            observed_event_serial: 0,
            rules: 0,
        },
        last_result: CommandResult::EMPTY,
    };

    /// Returns empty configured state bound to `config` rules applicable in `mode`
    /// with exact channel `sources`; caller preserves that configuration binding.
    pub(crate) fn new(
        config: &ValidatedConfig,
        mode: DataMode,
        sources: [Option<Source>; 8],
    ) -> Self {
        let mut result = Self::EMPTY;
        result.initialize_bound_policy(config, mode, sources);
        result
    }

    // Construction only: mutate the already-present empty engine. The owner
    // supplies the configuration from its immutable stored handle.
    /// Initializes fresh empty storage from immutable `config`, explicit `mode`
    /// and exact `sources`. Enables Demo rules or VehicleAndDemo applicability;
    /// enabled slots begin Unavailable. Construction-only: masks/channels use OR,
    /// so rerunning this on existing state is not a supported configuration change.
    pub(crate) fn initialize_bound_policy(
        &mut self,
        config: &ValidatedConfig,
        mode: DataMode,
        sources: [Option<Source>; 8],
    ) {
        self.sources = sources;
        for (i, widget) in config.config().widgets.iter().enumerate() {
            self.alerts.channels[i / 2] |= (widget.channel.index() as u8) << ((i % 2) * 4);
            if widget.warning.enabled
                && (mode == DataMode::Demo || widget.warning.applicability == Applicability::VehicleAndDemo)
            {
                self.enabled |= 1 << i;
                self.alerts.states[i] = Status::Unavailable;
            }
        }
    }

    /// Returns the current publication; does not tick raw freshness or snooze expiry.
    pub(crate) const fn snapshot(&self) -> RuleAlerts { self.alerts }

    /// Returns whether any applicable configured rule exists, independent of data availability.
    pub(crate) const fn enabled(&self) -> bool { self.enabled != 0 }

    /// Returns source-bound canonical rule for widget `i` in immutable `config`,
    /// or `None` without an admitted source. Panics if i is outside configured
    /// widgets; internal enabled-mask callers guarantee the bound and validation.
    fn rule(
        &self,
        config: &ValidatedConfig,
        i: usize,
    ) -> Option<Rule> {
        let widget = &config.config().widgets[i];
        let source = self.sources[widget.channel.index()]?;
        let warning = &widget.warning;
        let limit = match warning.mode {
            LimitMode::OutsideWindow => {
                Limit::OutsideWindow {
                    low_on: warning.trigger,
                    low_off: warning.clear,
                    high_on: warning.advanced.trigger_high,
                    high_off: warning.advanced.clear_high,
                }
            }
            LimitMode::InsideWindow => {
                Limit::InsideWindow {
                    low_on: warning.trigger,
                    low_off: warning.clear,
                    high_on: warning.advanced.trigger_high,
                    high_off: warning.advanced.clear_high,
                }
            }
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

    /// Evaluates qualified `samples` and verified optional `contexts` at monotonic
    /// `now` milliseconds against immutable `config`. New raw activations get
    /// checked-by-saturation episode IDs, clear acknowledgment, optionally latch,
    /// and interrupt every snooze. Reversal invalidates raw decisions/snoozes,
    /// preserving remembered episodes; MAX serial keeps visibility but disables ack.
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
            let episode = &mut self.states[i];
            let state = &mut episode.raw;
            let was_active = state.flags & ACTIVE != 0;
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
            let status = self.alerts.states[i];
            if status == Status::Active && !was_active {
                self.alerts.event_serial = self.alerts.event_serial.saturating_add(1);
                // MAX is a terminal watermark, never an issued episode identity.
                episode.activation_serial = if self.alerts.event_serial == u64::MAX {
                    0
                } else {
                    self.alerts.event_serial
                };
                state.flags &= !(ACKNOWLEDGED | CLEARED);
                if config.config().widgets[i].warning.advanced.latch {
                    state.flags |= LATCHED;
                }
                // Every new episode interrupts all snoozes, including out-of-view rules.
                for item in &mut self.states {
                    item.snooze_until_ms = 0;
                }
            } else if state.flags & (CLEARED | ACKNOWLEDGED) == CLEARED | ACKNOWLEDGED {
                state.flags &= !LATCHED;
            }
        }
        if backwards {
            for item in &mut self.states {
                item.snooze_until_ms = 0;
            }
        }
        self.refresh_masks();
    }

    /// Rebuilds publication masks from episode flags/deadlines without advancing time.
    fn refresh_masks(&mut self) {
        self.alerts.latched = 0;
        self.alerts.cleared = 0;
        self.alerts.acknowledged = 0;
        self.alerts.snoozed = 0;
        for (i, item) in self.states.iter().enumerate() {
            let bit = 1u64 << i;
            if item.raw.flags & CLEARED != 0 {
                self.alerts.cleared |= bit;
            }
            if item.raw.flags & LATCHED != 0 {
                self.alerts.latched |= bit;
            }
            if item.raw.flags & ACKNOWLEDGED != 0 {
                self.alerts.acknowledged |= bit;
            }
            if item.snooze_until_ms != 0 {
                self.alerts.snoozed |= bit;
            }
        }
    }

    /// Returns warning event/request exhaustion causes at terminal MAX counters;
    /// reporting does not change visibility, restore identities or retry commands.
    pub(crate) fn exhaustion_status(&self) -> crate::cache_diagnostics::CacheExhaustionStatus {
        use crate::cache_diagnostics::{CacheExhaustionCause, CacheExhaustionStatus};
        let mut status = CacheExhaustionStatus::EMPTY;
        if self.alerts.event_serial == u64::MAX {
            status = status.union(CacheExhaustionStatus::from_cause(
                CacheExhaustionCause::WarningEventSerial,
            ));
        }
        if self.last_command.request_id == u64::MAX {
            status = status.union(CacheExhaustionStatus::from_cause(
                CacheExhaustionCause::WarningRequestId,
            ));
        }
        status
    }

    /// Clears expired snoozes at monotonic `now` milliseconds, or every snooze
    /// on reversal relative to last raw evaluation, then refreshes masks. Does not
    /// clear remembered latches or promote unavailable raw measurements.
    pub(crate) fn expire(
        &mut self,
        now: u64,
    ) {
        for item in &mut self.states {
            if now < self.last_ms || now >= item.snooze_until_ms {
                item.snooze_until_ms = 0;
            }
        }
        self.refresh_masks();
    }

    /// Applies fenced acknowledgment `command` using immutable `config`, current
    /// optional `run_id` and monotonic `now` milliseconds. Returns accepted/rejected
    /// masks; wrong run/time, zero/stale ID or exhausted event serial rejects.
    /// Exact repeats return the cached result without extending snooze. Only
    /// observed active/latched episodes qualify; acknowledgment plus prior clear
    /// removes latches, and configured snooze deadlines saturate without wrapping.
    pub(crate) fn apply_command(
        &mut self,
        config: &ValidatedConfig,
        command: WarningCommand,
        run_id: Option<u64>,
        now: u64,
    ) -> CommandResult {
        let mut result = CommandResult::rejected(command);
        if now < self.last_ms
            || run_id != Some(command.run_id)
            || command.request_id == 0
            || self.alerts.event_serial == u64::MAX
        {
            self.alerts.result = result;
            return result;
        }
        if command.request_id == self.last_command.request_id && command == self.last_command {
            self.alerts.result = self.last_result;
            return self.last_result;
        }
        if command.request_id <= self.last_command.request_id {
            self.alerts.result = result;
            return result;
        }
        for (i, item) in self.states.iter_mut().enumerate() {
            let bit = 1u64 << i;
            if command.rules & bit == 0
                || item.activation_serial == 0
                || item.activation_serial > command.observed_event_serial
                || (self.alerts.states[i] != Status::Active && item.raw.flags & LATCHED == 0)
            {
                continue;
            }
            item.raw.flags |= ACKNOWLEDGED;
            if item.raw.flags & CLEARED != 0 {
                item.raw.flags &= !LATCHED;
            }
            let seconds = config.config().widgets[i].warning.advanced.snooze_s;
            if seconds != 0 {
                item.snooze_until_ms = now.saturating_add(u64::from(seconds) * 1000);
            }
            result.accepted |= bit;
            result.rejected &= !bit;
        }
        self.last_command = command;
        self.last_result = result;
        self.alerts.result = result;
        self.refresh_masks();
        result
    }
}

/// Borrowed policy facade. Its configuration cannot be replaced independently of state.
pub struct RuleEngine<'a> {
    /// Immutable validated document governing this state's entire lifetime.
    config: &'a ValidatedConfig,
    /// Mutable episodes whose widget indices/sources refer to the bound document.
    state: RuleEngineState,
}
impl<'a> RuleEngine<'a> {
    /// Returns a facade bound to `config`, explicit `mode` and exact channel
    /// `sources`; construction alone enables no qualified measurement/ECU traffic.
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

    /// Returns current raw alerts/episode flags, without present-time qualification.
    pub const fn snapshot(&self) -> RuleAlerts { self.state.snapshot() }

    /// Returns the same immutable validated configuration used during construction.
    pub const fn configuration(&self) -> &ValidatedConfig { self.config }
}

// Measured traced ARM candidate passes unchanged memory and named stack gates.
/// Upper bound preserving reviewed owner storage/headroom across pointer widths.
const _: () = assert!(core::mem::size_of::<RuleEngine<'_>>() <= 4376);
/// Upper bound on coherent publication copied through shared transport.
const _: () = assert!(core::mem::size_of::<RuleAlerts>() <= 680);

#[cfg(test)]
/// Constant-construction and terminal warning-identity regression fixtures.
mod tests {
    use super::*;
    /// Returns unit after comparing empty episode/engine constants to original
    /// field values; panics if construction changes counters, flags or bindings.
    #[test]
    fn constant_episode_and_rule_storage_match_original_empty_fields() {
        for episode in [EpisodeState::default(), EpisodeState::EMPTY] {
            assert_eq!(
                (
                    episode.raw.sequence,
                    episode.raw.at,
                    episode.raw.context_at,
                    episode.raw.since,
                    episode.raw.flags,
                    episode.activation_serial,
                    episode.snooze_until_ms
                ),
                (0, 0, 0, 0, 0, 0, 0)
            );
        }
        let engine = RuleEngineState::EMPTY;
        assert_eq!(engine.alerts, RuleAlerts::DISABLED);
        assert_eq!(engine.sources, [None; 8]);
        assert_eq!((engine.enabled, engine.last_ms), (0, 0));
        assert_eq!(
            (
                engine.last_command.request_id,
                engine.last_command.run_id,
                engine.last_command.observed_event_serial,
                engine.last_command.rules
            ),
            (0, 0, 0, 0)
        );
        assert_eq!(engine.last_result, CommandResult::EMPTY);
        for episode in engine.states {
            assert_eq!(
                (
                    episode.raw.sequence,
                    episode.raw.at,
                    episode.raw.context_at,
                    episode.raw.since,
                    episode.raw.flags,
                    episode.activation_serial,
                    episode.snooze_until_ms
                ),
                (0, 0, 0, 0, 0, 0, 0)
            );
        }
    }
    /// Returns unit after final serial overflow retains visible latch and rejects
    /// acknowledgment without issuing MAX; panics on identity reuse or lost warning.
    #[test]
    fn event_exhaustion_preserves_visible_latch_and_disables_acknowledgment() {
        let mut c = dashboard_config::default_config();
        c.widgets.truncate(1);
        let w = &mut c.widgets[0].warning;
        w.enabled = true;
        w.trigger = 2.;
        w.clear = 1.;
        w.on_ms = 100;
        w.advanced.latch = true;
        c.controls.screens[0].buttons[0].tap = dashboard_config::ControlAction::AcknowledgeWarnings;
        let c = dashboard_config::validate(c).unwrap();
        let mut engine = RuleEngineState::new(&c, DataMode::Demo, [Some(Source::Demo); 8]);
        engine.alerts.event_serial = u64::MAX - 1;
        for at in [0, 100] {
            let mut metadata = [SampleMeta::UNKNOWN; 8];
            metadata[0] = SampleMeta {
                source: Source::Demo,
                sequence: at + 1,
                received_at_ms: Some(at),
                status_at_ms: at,
                quality: crate::samples::Quality::Valid,
            };
            engine.update_inputs(
                &c,
                &Inputs {
                    metadata: &metadata,
                    values: [Some(3.); 8],
                    freshness: &[500; 8],
                },
                &[None; 8],
                at,
            );
        }
        assert_eq!(engine.alerts.event_serial, u64::MAX);
        assert_eq!(
            engine.states[0].activation_serial, 0,
            "exhaustion is not an issued serial"
        );
        assert_eq!(engine.alerts.states[0], Status::Active);
        assert_eq!(engine.alerts.latched, 1);
        let command = WarningCommand {
            request_id: 1,
            run_id: 1,
            observed_event_serial: u64::MAX,
            rules: 1,
        };
        assert_eq!(engine.apply_command(&c, command, Some(1), 100).rejected, 1);
        assert_eq!(engine.alerts.acknowledged, 0);
        assert_eq!(engine.alerts.latched, 1);
        let mut seen = crate::cache_diagnostics::CacheExhaustionSeen::new();
        assert_eq!(seen.observe(engine.exhaustion_status()).causes().count(), 1);
        assert_eq!(seen.observe(engine.exhaustion_status()).causes().count(), 0);
    }
}
