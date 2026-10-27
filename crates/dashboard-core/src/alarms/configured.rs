//! Widget indices and channel indices are deliberately distinct types.
use dashboard_config::{Applicability, LimitMode, MAX_WIDGETS, ValidatedConfig};

use super::*;
use crate::ignition::{Cylinder, FRESHNESS_MS, IgnitionSnapshot};
use crate::warning_state::{CommandResult, WarningCommand, WarningEligibility};

/// One-byte status plus group marker and current four-cylinder membership.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PublishedStatus(
    /// Raw status in bits0..2, member mask in bits3..6, group marker in bit7.
    u8,
);
impl PublishedStatus {
    /// Returns the encoded raw `status`; `group` selects grouped interpretation and `mask`
    /// supplies its bounded low-four membership bits. No scalar channel is implied.
    const fn new(
        status: Status,
        group: bool,
        mask: u8,
    ) -> Self {
        Self(status as u8 | ((mask & 15) << 3) | if group { 128 } else { 0 })
    }

    /// Returns the raw decision; impossible private encodings fail unavailable.
    const fn status(self) -> Status {
        match self.0 & 7 {
            0 => Status::Disabled,
            1 => Status::Unavailable,
            2 => Status::Normal,
            3 => Status::Pending,
            4 => Status::Active,
            _ => Status::Unavailable,
        }
    }

    /// Returns whether this slot is a group rather than a scalar/unused position.
    const fn group(self) -> bool { self.0 & 128 != 0 }

    /// Returns the published current group membership, independent of current age.
    const fn mask(self) -> u8 { (self.0 >> 3) & 15 }
}

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
    /// Packed raw state/member bits per configured widget; unused slots are disabled.
    states: [PublishedStatus; MAX_WIDGETS],
    /// Inclusive raw-validity deadlines in monotonic milliseconds.
    expires: [u64; MAX_WIDGETS],
    /// Two nibbles per byte: scalar IDs 0..7/unused8, or group episode-member masks.
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
        states: [PublishedStatus::new(Status::Disabled, false, 0); MAX_WIDGETS],
        expires: [0; MAX_WIDGETS],
        channels: [0x88; MAX_WIDGETS / 2],
        evaluated_at: 0,
        latched: 0,
        cleared: 0,
        acknowledged: 0,
        snoozed: 0,
        event_serial: 0,
        result: CommandResult::EMPTY,
    };

    /// Returns the packed scalar channel for widget `index`, or None for grouped
    /// widgets and unused positions. Disabled scalar widgets retain their identity.
    pub(crate) fn channel(
        &self,
        index: RuleIndex,
    ) -> Option<SensorId> {
        let i = index.index();
        if self.states[i].group() {
            return None;
        }
        SensorId::ALL
            .get(((self.channels[i / 2] >> ((i % 2) * 4)) & 15) as usize)
            .copied()
    }

    /// Returns current fresh group membership for `index` using coherent `samples`
    /// at `now` milliseconds; only raw Active statuses yield a nonzero mask.
    pub(crate) fn ignition_mask_at(
        &self,
        index: RuleIndex,
        samples: &DashboardSnapshot,
        now: u64,
    ) -> u8 {
        let state = self.states[index.index()];
        if !state.group() || state.status() != Status::Active || now < self.evaluated_at {
            return 0;
        }
        self.fresh_group_mask(state.mask(), samples, now)
    }

    /// Returns remembered episode members for group `index`, including faulted
    /// members; scalar/unused/unlatched slots return zero and imply no live data.
    pub(crate) fn ignition_latched_mask(
        &self,
        index: RuleIndex,
    ) -> u8 {
        let i = index.index();
        if self.states[i].group() && self.latched & (1u64 << i) != 0 {
            (self.channels[i / 2] >> ((i % 2) * 4)) & 15
        } else {
            0
        }
    }

    /// Returns low-four `mask` filtered through exact Demo/source/freshness checks
    /// in coherent `samples` at monotonic `now` milliseconds, without mutating dwell.
    fn fresh_group_mask(
        &self,
        mask: u8,
        samples: &DashboardSnapshot,
        now: u64,
    ) -> u8 {
        let mut fresh = 0;
        for cylinder in Cylinder::ALL {
            let bit = 1 << cylinder.index();
            if mask & bit != 0 && samples.ignition.value_at(cylinder, samples.mode, now).is_some() {
                fresh |= bit;
            }
        }
        fresh
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
        let published = self.states[i];
        let status = published.status();
        if published.group() {
            if status == Status::Disabled {
                return status;
            }
            let qualified = match status {
                Status::Active | Status::Pending => self.fresh_group_mask(published.mask(), samples, now) != 0,
                Status::Normal => self.fresh_group_mask(15, samples, now) == 15,
                _ => false,
            };
            return if now >= self.evaluated_at && qualified {
                status
            } else {
                Status::Unavailable
            };
        }
        if status != Status::Disabled
            && (now < self.evaluated_at
                || now > self.expires[i]
                || self
                    .channel(index)
                    .and_then(|channel| samples.value_at(channel, now))
                    .is_none())
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

impl State {
    /// Evaluates shared group `warning` from independent `ignition` observations
    /// at `now` milliseconds and owner raw `sequence`. Returns raw status/current
    /// member mask. Hysteresis uses strict negative-side trigger and inclusive
    /// clear; only capture timestamps advance shared dwell. All four cylinders
    /// must be fresh and clear to complete clearing, including a remembered latch.
    /// Writes `expires` with the inclusive support deadline in monotonic milliseconds
    /// when support is available; unavailable early returns retain the prior deadline.
    fn advance_ignition(
        &mut self,
        ignition: &IgnitionSnapshot,
        sequence: u64,
        warning: &dashboard_config::WarningSettings,
        now: u64,
        expires: &mut u64,
    ) -> (Status, u8) {
        let mut fresh = 0u8;
        let mut clear = 0u8;
        let mut hysteresis = self.group_hysteresis;
        let mut newest_active = 0u64;
        let mut newest = 0u64;
        let mut oldest = u64::MAX;
        for cylinder in Cylinder::ALL {
            let bit = 1 << cylinder.index();
            let Some(value) = ignition.value_at(cylinder, DataMode::Demo, now) else {
                hysteresis &= !bit;
                continue;
            };
            let at = ignition.received_at_ms(cylinder).expect("qualified group timestamp");
            fresh |= bit;
            newest = newest.max(at);
            oldest = oldest.min(at);
            if value >= warning.clear {
                clear |= bit;
                hysteresis &= !bit;
            }
            if value < warning.trigger {
                hysteresis |= bit;
            }
            if hysteresis & bit != 0 {
                newest_active = newest_active.max(at);
            }
        }
        if self
            .seen()
            .is_some_and(|(_, previous)| newest.checked_sub(previous).is_some_and(|gap| gap > FRESHNESS_MS))
        {
            self.invalidate();
            // A separated observation must trigger again, not inherit an old
            // cylinder's hysteresis through a period with no supporting samples.
            hysteresis = 0;
            for cylinder in Cylinder::ALL {
                if ignition
                    .value_at(cylinder, DataMode::Demo, now)
                    .is_some_and(|value| value < warning.trigger)
                {
                    hysteresis |= 1 << cylinder.index();
                }
            }
        }
        self.group_hysteresis = hysteresis;
        let all_clear = fresh == 15 && clear == 15;
        if hysteresis == 0 && fresh != 15 {
            self.invalidate();
            return (Status::Unavailable, 0);
        }
        let clearing =
            self.flags & ACTIVE != 0 && all_clear || self.flags & (LATCHED | CLEARED) == LATCHED && all_clear;
        let condition_at = if clearing { newest } else { newest_active };
        let supported_until = if clearing { oldest } else { newest_active };
        *expires = supported_until.saturating_add(FRESHNESS_MS);
        if self.seen().is_some_and(|(seen, _)| sequence < seen) {
            self.invalidate();
            return (Status::Unavailable, 0);
        }
        let distinct = self.seen().is_none_or(|(seen, _)| sequence > seen);
        if distinct {
            let previous_at = self.at;
            self.record(sequence, previous_at.max(newest));
            self.flags |= VALID;
            if clearing || hysteresis != 0 && self.flags & ACTIVE == 0 {
                if self.flags & PENDING == 0 || (self.flags & PENDING_CLEAR != 0) != clearing {
                    self.since = condition_at;
                    self.flags |= PENDING;
                    if clearing {
                        self.flags |= PENDING_CLEAR;
                    } else {
                        self.flags &= !PENDING_CLEAR;
                    }
                }
                if supported_until >= self.since
                    && supported_until - self.since >= u64::from(if clearing { warning.off_ms } else { warning.on_ms })
                {
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
                self.flags &= !(PENDING | PENDING_CLEAR);
            }
        }
        let status = self.status();
        let mask = if hysteresis != 0 {
            hysteresis
        } else if status == Status::Active || clearing && self.flags & LATCHED != 0 {
            self.group_members & fresh
        } else {
            0
        };
        (status, mask)
    }
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
    /// enabled slots begin Unavailable. Grouped rules admit Demo only and never
    /// acquire scalar identities. Construction-only; existing episodes are not reset.
    pub(crate) fn initialize_bound_policy(
        &mut self,
        config: &ValidatedConfig,
        mode: DataMode,
        sources: [Option<Source>; 8],
    ) {
        self.sources = sources;
        for (i, widget) in config.config().widgets.iter().enumerate() {
            let shift = (i % 2) * 4;
            let group = widget.channel.scalar().is_none();
            let identity = widget.channel.scalar().map_or(0, |channel| channel.index() as u8);
            self.alerts.channels[i / 2] = (self.alerts.channels[i / 2] & !(15 << shift)) | (identity << shift);
            self.alerts.states[i] = PublishedStatus::new(Status::Disabled, group, 0);
            if widget.warning.enabled
                && (mode == DataMode::Demo || widget.warning.applicability == Applicability::VehicleAndDemo)
                && (!group || mode == DataMode::Demo)
            {
                self.enabled |= 1 << i;
                self.alerts.states[i] = PublishedStatus::new(Status::Unavailable, group, 0);
            }
        }
    }

    /// Returns the current publication; does not tick raw freshness or snooze expiry.
    pub(crate) const fn snapshot(&self) -> RuleAlerts { self.alerts }

    /// Returns whether any applicable configured rule exists, independent of data availability.
    pub(crate) const fn enabled(&self) -> bool { self.enabled != 0 }

    /// Drops group hysteresis supported by `unavailable` cylinder bits before
    /// replacement observations overwrite their prior ages. Losing every active
    /// supporter or any shared-clear evidence breaks raw dwell/activation while
    /// preserving episode membership and acknowledgment. Caller preflights the
    /// impending acquisition mutation and evaluates/publishes afterward.
    pub(crate) fn invalidate_ignition_support(
        &mut self,
        unavailable: u8,
    ) {
        if unavailable == 0 {
            return;
        }
        let mut enabled = self.enabled;
        while enabled != 0 {
            let i = enabled.trailing_zeros() as usize;
            enabled &= enabled - 1;
            if !self.alerts.states[i].group() {
                continue;
            }
            let state = &mut self.states[i].raw;
            state.group_hysteresis &= !unavailable;
            if state.group_hysteresis == 0 || state.flags & PENDING_CLEAR != 0 {
                state.invalidate();
            }
        }
    }

    /// Returns source-bound canonical rule for widget `i` in immutable `config`,
    /// or `None` without an admitted source. Panics if i is outside configured
    /// widgets; internal enabled-mask callers guarantee the bound and validation.
    fn rule(
        &self,
        config: &ValidatedConfig,
        i: usize,
    ) -> Option<Rule> {
        let widget = &config.config().widgets[i];
        let channel = widget.channel.scalar()?;
        let source = self.sources[channel.index()]?;
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
            sensor: SensorId::from_channel(channel),
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
    /// `ignition` supplies independent cylinder observations and outer raw sequence;
    /// joined group members also issue new events so older acknowledgments cannot cover them.
    pub(crate) fn update_inputs(
        &mut self,
        config: &ValidatedConfig,
        samples: &Inputs<'_>,
        contexts: &[Option<Context>; 8],
        now: u64,
        ignition: (&IgnitionSnapshot, u64),
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
            let group = self.alerts.states[i].group();
            let rule = self.rule(config, i);
            if !group && rule.is_none() {
                continue;
            }
            let episode = &mut self.states[i];
            let state = &mut episode.raw;
            let was_active = state.flags & ACTIVE != 0;
            let previous_members = state.group_members;
            let (status, mask) = if backwards {
                if let Some(rule) = rule {
                    state.consume(rule, samples, now);
                }
                state.invalidate();
                state.group_hysteresis = 0;
                (Status::Unavailable, 0)
            } else if group {
                state.advance_ignition(
                    ignition.0,
                    ignition.1,
                    &config.config().widgets[i].warning,
                    now,
                    &mut self.alerts.expires[i],
                )
            } else {
                let rule = rule.expect("scalar enabled rule with source");
                (
                    state.advance(
                        rule,
                        samples,
                        contexts[rule.sensor as usize],
                        now,
                        &mut self.alerts.expires[i],
                    ),
                    0,
                )
            };
            self.alerts.states[i] = PublishedStatus::new(status, group, mask);
            let new_member = group && mask & !previous_members != 0;
            if group && status == Status::Active {
                if !was_active && state.flags & LATCHED == 0 {
                    state.group_members = 0;
                }
                state.group_members |= state.group_hysteresis;
            }
            if status == Status::Active && (!was_active || new_member) {
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
        for (i, item) in self.states.iter_mut().enumerate() {
            let bit = 1u64 << i;
            if self.alerts.states[i].group() {
                if item.raw.flags & (LATCHED | ACTIVE) == 0 {
                    item.raw.group_members = 0;
                }
                let shift = (i % 2) * 4;
                self.alerts.channels[i / 2] =
                    (self.alerts.channels[i / 2] & !(15 << shift)) | ((item.raw.group_members & 15) << shift);
            }
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
                || (self.alerts.states[i].status() != Status::Active && item.raw.flags & LATCHED == 0)
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
                (&IgnitionSnapshot::EMPTY, 0),
            );
        }
        assert_eq!(engine.alerts.event_serial, u64::MAX);
        assert_eq!(
            engine.states[0].activation_serial, 0,
            "exhaustion is not an issued serial"
        );
        assert_eq!(engine.alerts.states[0].status(), Status::Active);
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
