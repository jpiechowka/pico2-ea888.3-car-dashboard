//! Fixed synthetic fixtures and injection outcomes. No vehicle qualification.
use dashboard_config::{Channel, LimitMode};
use dashboard_core::samples::{Quality, Quantity, Sample, SampleError, SensorId, Source};

use super::ControlSimulator;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
/// Explicit deterministic fixture selection; dropped completion/reset-delay behavior is applied by the browser
/// transport adapter.
pub enum Scenario {
    #[default]
    /// Shared synthetic batch generator, including explicit demo operating context.
    Normal,
    /// Bounded elapsed schedule targeting one configured warning witness at a time.
    Warnings,
    /// Alternate channel domain minima/maxima every two simulated seconds.
    MinMax,
    /// Same typed extreme schedule, presented as a text-width inspection fixture.
    WidthStress,
    /// Advance control/time without publishing any sample.
    Missing,
    /// Seed once, then advance time without refreshing readings.
    Stale,
    /// Seed once, then inject coolant transport-error quality.
    SourceFault,
    /// Normal typed readings while the UI defers statistics-reset acknowledgement.
    DelayedResetAck,
    /// Normal typed readings while the UI withholds display completion.
    DroppedCompletion,
}
impl Scenario {
    /// Stable scenario selector order.
    pub const ALL: [Self; 9] = [
        Self::Normal,
        Self::Warnings,
        Self::MinMax,
        Self::WidthStress,
        Self::Missing,
        Self::Stale,
        Self::SourceFault,
        Self::DelayedResetAck,
        Self::DroppedCompletion,
    ];

    /// Returns the visible synthetic-scenario label.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Normal => "Normal demo",
            Self::Warnings => "Warnings",
            Self::MinMax => "Channel minima/maxima",
            Self::WidthStress => "Width stress",
            Self::Missing => "Missing data",
            Self::Stale => "Stale data",
            Self::SourceFault => "Source fault",
            Self::DelayedResetAck => "Delayed reset acknowledgment",
            Self::DroppedCompletion => "Dropped display completion",
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// Scenario/startup action refusal, distinct from per-sensor admission results.
pub enum SimulationError {
    /// Captured project dashboard failed decode or differs from this accepted owner.
    InvalidProject,
    /// Required source generation or owner identity cannot be issued.
    Exhausted,
    /// Requested scenario time precedes accepted simulation time.
    Backdated,
    /// Unapplied/invalid inspection has no accepted source-mutation authority.
    InspectionOnly,
}
#[derive(Clone, Copy, Debug, PartialEq)]
/// Explicit typed synthetic injection; raw samples retain their supplied provenance for shared-gate testing.
pub enum SimulationInput {
    /// Construct a Demo sample with next checked sequence for one canonical channel.
    Channel {
        /// Channel selecting canonical quantity/unit and metadata sequence.
        channel: Channel,
        /// Canonical bar, Celsius, volts or dimensionless lambda value; shared admission checks validity.
        value: f32,
        /// Synthetic receipt milliseconds; the observation clock advances to at least this time.
        at_ms: u64,
    },
    /// Submit an exact sample against current simulation time, including future/source/sequence rejection cases.
    Sample(
        /// Supplied quantity, receipt timestamp, sequence and source.
        Sample,
    ),
    /// Mark quality unavailable/error without fabricating a numeric reading.
    Fault {
        /// Affected sensor metadata slot.
        sensor: SensorId,
        /// Shared fault-quality observation.
        quality: Quality,
        /// Fault observation milliseconds; backdated owner updates may be rejected.
        at_ms: u64,
    },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// Local input result, preserving parsing/policy failure separately from shared sample admission.
pub enum InputOutcome {
    /// Shared owner admitted the sample or fault observation.
    Accepted,
    /// Shared owner refused sample/fault admission.
    Rejected(
        /// Value, timestamp, ordering or provenance rejection; faults map refusal to OutOfOrder.
        SampleError,
    ),
    /// Trimmed text could not parse as f32.
    Parse,
    /// Ordinary input parsed as infinity or NaN.
    NotFinite,
    /// Ordinary input falls outside the channel's synthetic testing domain.
    TestingRange,
    /// Input exceeds the 64-byte UTF-8 text allowance.
    TooLong,
    /// Required time/sample/source identity cannot advance safely.
    Exhausted,
    /// Candidate/invalid inspection cannot dispatch accepted acquisition input.
    InspectionOnly,
}

/// Retains only bounded text. An oversized visible edit revokes dispatch of
/// the previous value until a subsequent bounded edit is observed.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BoundedText<const N: usize> {
    /// Last admitted text, at most N UTF-8 bytes; oversize edits retain it only for display, not dispatch.
    text: String,
    /// Current visible edit exceeded N, revoking dispatch of retained text.
    blocked: bool,
}
impl<const N: usize> BoundedText<N> {
    /// Observes `text`; returns true after replacing retained bytes within N, false after blocking an oversized edit.
    /// An oversized edit leaves prior text intact but admitted() unavailable; a later bounded edit clears the block.
    pub fn observe(
        &mut self,
        text: &str,
    ) -> bool {
        self.blocked = text.len() > N;
        if self.blocked {
            return false;
        }
        self.text.clear();
        self.text.push_str(text);
        true
    }

    /// Returns last bounded text for display, even when a newer oversized edit revoked its dispatch authority.
    pub fn text(&self) -> &str { &self.text }

    /// Returns whether the most recently observed edit exceeded the byte bound.
    pub const fn is_blocked(&self) -> bool { self.blocked }

    /// Returns retained text only while the latest edit is bounded, otherwise None; empty initial text is admitted.
    pub fn admitted(&self) -> Option<&str> { (!self.blocked).then_some(self.text.as_str()) }
}
impl InputOutcome {
    /// Returns a fixed explanatory label without embedding potentially private/raw input text.
    pub fn label(self) -> &'static str {
        match self {
            Self::Accepted => "Accepted synthetic sample",
            Self::Rejected(SampleError::InvalidValue) => "Rejected by shared gate: invalid value",
            Self::Rejected(SampleError::OutOfOrder) => "Rejected by shared gate: out of order",
            Self::Rejected(SampleError::Future) => "Rejected by shared gate: future timestamp",
            Self::Rejected(SampleError::Source) => "Rejected by shared gate: source mismatch",
            Self::Parse => "Enter a Rust decimal number",
            Self::NotFinite => "Ordinary input must be finite",
            Self::TestingRange => "Input outside synthetic testing range",
            Self::TooLong => "Input exceeds 64 UTF-8 bytes",
            Self::Exhausted => "Simulation identity exhausted",
            Self::InspectionOnly => "Inspection only; input not dispatched",
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
/// Availability of one synthetic warning witness, never a vehicle operating recommendation.
pub enum WarningFixture {
    /// Missing widget or disabled warning rule.
    Disabled,
    /// Finite candidate inside the channel test domain; actual warning still needs shared context/dwell admission.
    Available(
        /// Canonical channel-unit witness value.
        f32,
    ),
    /// Required context or a finite bounded witness is unavailable.
    Unavailable(
        /// Static reason shown beside the synthetic scenario.
        &'static str,
    ),
}
#[derive(Clone, Copy, Debug)]
/// Immutable UI projection joining accepted acquisition evidence with current displayed control/inspection state.
pub struct SimulatorProjection {
    /// Selected synthetic fixture.
    pub scenario: Scenario,
    /// Local source generation or None at exhaustion; not full callback authority.
    pub source_epoch: Option<u64>,
    /// Accepted acquisition milliseconds, not necessarily inspection display time.
    pub now_ms: u64,
    /// Coherent accepted acquisition publication.
    pub snapshot: dashboard_core::acquisition::DashboardSnapshot,
    /// Displayed accepted/inspection control page.
    pub page: dashboard_config::ControlPage,
    /// Displayed accepted/inspection view ID.
    pub view: u8,
    /// Candidate/invalid inspection is active and cannot dispatch accepted source effects.
    pub invalid: bool,
    /// Exact pending statistics-reset token, or None.
    pub reset_token: Option<u64>,
    /// Accepted completed warning identity, not a candidate warning projection.
    pub completed_warning: dashboard_core::status_led::PresentedWarning,
    /// Accepted completed-scene logical RGB duty.
    pub led: dashboard_core::status_led::LedDuty,
    /// Latest per-channel input attempt results; absent until recorded.
    pub outcomes: [Option<InputOutcome>; 8],
}

/// Checked callback identity, separate from source and optimization cache identities.
pub struct CallbackGate {
    /// Checked callback lifetime, None permanently after overflow.
    epoch: Option<u64>,
}
#[derive(Clone, Copy)]
/// Captured callback epoch and exact source, optionally bound to accepted editor lineage.
pub struct CallbackToken {
    /// Gate lifetime at capture.
    epoch: u64,
    /// Opaque simulator owner/generation authority.
    source: super::SourceSessionKey,
    /// Accepted editor configuration lineage when captured for browser work; None for source-only callbacks.
    editor: Option<crate::editor::AcceptedConfigKey>,
}
impl Default for CallbackGate {
    /// Returns an epoch-zero callback gate.
    fn default() -> Self { Self::new() }
}
impl CallbackGate {
    /// Returns a live epoch-zero gate with no callback allocation or scheduling.
    pub const fn new() -> Self { Self { epoch: Some(0) } }

    /// Returns a callback capability for available `source`, or None if source/gate identity is exhausted.
    /// Does not schedule work or retire prior captures in the same epoch.
    pub fn capture(
        &self,
        source: Option<super::SourceSessionKey>,
    ) -> Option<CallbackToken> {
        Some(CallbackToken {
            epoch: self.epoch?,
            source: source?,
            editor: None,
        })
    }

    /// Returns a capability bound to exact `source` and accepted `editor` lineage, or None for unavailable source/gate
    /// identity.
    pub fn capture_for_editor(
        &self,
        source: Option<super::SourceSessionKey>,
        editor: crate::editor::AcceptedConfigKey,
    ) -> Option<CallbackToken> {
        let mut token = self.capture(source)?;
        token.editor = Some(editor);
        Some(token)
    }

    /// Returns whether `token` matches current gate, exact `source` and accepted `editor` lineage; source-only tokens
    /// fail.
    pub fn is_current_for_editor(
        &self,
        token: CallbackToken,
        source: Option<super::SourceSessionKey>,
        editor: crate::editor::AcceptedConfigKey,
    ) -> bool {
        token.editor == Some(editor) && self.is_current(token, source)
    }

    /// Returns whether `token` matches this gate and exact available `source`; missing identities never compare as
    /// authority.
    pub fn is_current(
        &self,
        token: CallbackToken,
        source: Option<super::SourceSessionKey>,
    ) -> bool {
        self.epoch == Some(token.epoch) && source == Some(token.source)
    }

    /// Invalidates previous callbacks with checked increment; exhaustion permanently disables capture/admission.
    pub fn retire(&mut self) { self.epoch = self.epoch.and_then(|e| e.checked_add(1)); }
}

/// Returns typed canonical quantity for `channel` containing `value`; performs no range/finite validation.
fn quantity(
    channel: Channel,
    value: f32,
) -> Quantity {
    match channel {
        Channel::Boost => Quantity::BoostBar(value),
        Channel::Oil => Quantity::EngineOilC(value),
        Channel::Water => Quantity::CoolantC(value),
        Channel::Dsg => Quantity::TransmissionOilC(value),
        Channel::Iat => Quantity::IntakeC(value),
        Channel::Egt => Quantity::ExhaustC(value),
        Channel::Battery => Quantity::ModuleVolts(value),
        Channel::Lambda => Quantity::Lambda(value),
    }
}
impl ControlSimulator {
    /// Returns accepted acquisition/LED/reset evidence plus displayed page/view/inspection state without advancing
    /// time.
    pub fn projection(&self) -> SimulatorProjection {
        SimulatorProjection {
            scenario: self.scenario,
            source_epoch: self.source_epoch,
            now_ms: self.now_ms(),
            snapshot: self.snapshot(),
            page: self.display_session().page(),
            view: self.display_session().view_id(),
            invalid: self.invalid(),
            reset_token: self.reset_token,
            completed_warning: self.completed_warning(),
            led: self.led_duty(),
            outcomes: self.outcomes,
        }
    }

    /// Restarts the accepted owner under fresh identity for `scenario`; returns InspectionOnly or Exhausted if not
    /// admissible. Resets samples/control history even for the same scenario, retaining reset issuer and
    /// explicit-render mode. Generation exhaustion refuses before replacement; owner exhaustion retires source
    /// authority while retaining old data.
    pub fn set_scenario(
        &mut self,
        scenario: Scenario,
    ) -> Result<(), SimulationError> {
        if self.invalid {
            return Err(SimulationError::InspectionOnly);
        }
        if self.source_epoch.and_then(|e| e.checked_add(1)).is_none() {
            return Err(SimulationError::Exhausted);
        }
        self.restart(self.config().clone());
        if self.source_key().is_none() {
            return Err(SimulationError::Exhausted);
        }
        self.scenario = scenario;
        Ok(())
    }

    /// Parses at-most-64-byte `text` for `channel`, requiring finite value inside its test domain, and injects at next
    /// millisecond. Returns parse/policy/shared-gate result; parse/policy failures are recorded per channel, while
    /// early time/identity/inspection refusal can return without replacing its prior outcome. No real vehicle
    /// measurement is created.
    pub fn custom_input(
        &mut self,
        channel: Channel,
        text: &str,
    ) -> InputOutcome {
        let result = if text.len() > 64 {
            InputOutcome::TooLong
        } else {
            match text.trim().parse::<f32>() {
                Err(_) => InputOutcome::Parse,
                Ok(value) if !value.is_finite() => InputOutcome::NotFinite,
                Ok(value) if !(channel.domain().0..=channel.domain().1).contains(&value) => InputOutcome::TestingRange,
                Ok(value) => {
                    let Some(at_ms) = self.now_ms().checked_add(1) else {
                        return InputOutcome::Exhausted;
                    };
                    return self.inject(SimulationInput::Channel { channel, value, at_ms });
                }
            }
        };
        self.outcomes[channel.index()] = Some(result);
        result
    }

    /// Parses at-most-64-byte `text` for `channel` and injects at next millisecond, intentionally retaining
    /// nonfinite/out-of-domain values for shared-gate rejection tests. Returns
    /// parse/length/exhaustion/inspection/shared admission outcome. Early time/identity/inspection refusal may
    /// leave the previous per-channel outcome unchanged.
    pub fn diagnostic_input(
        &mut self,
        channel: Channel,
        text: &str,
    ) -> InputOutcome {
        let result = if text.len() > 64 {
            InputOutcome::TooLong
        } else {
            match text.trim().parse::<f32>() {
                Err(_) => InputOutcome::Parse,
                Ok(value) => {
                    let Some(at_ms) = self.now_ms().checked_add(1) else {
                        return InputOutcome::Exhausted;
                    };
                    return self.inject(SimulationInput::Channel { channel, value, at_ms });
                }
            }
        };
        self.outcomes[channel.index()] = Some(result);
        result
    }

    /// Injects typed `input` through accepted acquisition and returns the shared admission outcome, without UI
    /// test-domain filtering. Inspection/exhausted source returns before dispatch. Channel input creates checked
    /// Demo sequence and advances time to at least its receipt; exact Sample uses current time. Fault/sample
    /// rejection still ticks nondecreasing acquisition time and records outcome.
    pub fn inject(
        &mut self,
        input: SimulationInput,
    ) -> InputOutcome {
        if self.invalid {
            return InputOutcome::InspectionOnly;
        }
        if self.source_epoch.is_none() {
            return InputOutcome::Exhausted;
        }
        let (sample, now) = match input {
            SimulationInput::Channel { channel, value, at_ms } => {
                let Some(sequence) = self.snapshot().metadata[channel.index()].sequence.checked_add(1) else {
                    return InputOutcome::Exhausted;
                };
                (
                    Sample {
                        value: quantity(channel, value),
                        received_at_ms: at_ms,
                        sequence,
                        source: Source::Demo,
                    },
                    self.now_ms().max(at_ms),
                )
            }
            SimulationInput::Sample(sample) => (sample, self.now_ms()),
            SimulationInput::Fault { sensor, quality, at_ms } => {
                let accepted = self.simulation.model.mark_fault(sensor, quality, at_ms);
                self.advance_acquisition(self.now_ms().max(at_ms), None);
                let outcome = if accepted {
                    InputOutcome::Accepted
                } else {
                    InputOutcome::Rejected(SampleError::OutOfOrder)
                };
                self.outcomes[sensor as usize] = Some(outcome);
                return outcome;
            }
        };
        let channel = sample.value.sensor() as usize;
        let outcome = match self.simulation.model.ingest_sample(sample, now) {
            Ok(()) => InputOutcome::Accepted,
            Err(error) => InputOutcome::Rejected(error),
        };
        // A rejected sample cannot roll back the simulation clock or evaluate old dwell.
        self.advance_acquisition(now, None);
        self.outcomes[channel] = Some(outcome);
        outcome
    }

    /// Returns a canonical warning witness for widget `index`, Disabled for missing/disabled rules, or Unavailable for
    /// unsupported context/nonfinite/out-of-domain witness. This suggests test input only; it does not bypass
    /// source/context/dwell guards.
    pub fn warning_fixture(
        &self,
        index: usize,
    ) -> WarningFixture {
        let Some(widget) = self.config().config().widgets.get(index) else {
            return WarningFixture::Disabled;
        };
        let w = &widget.warning;
        if !w.enabled {
            return WarningFixture::Disabled;
        }
        if matches!(widget.channel, Channel::Battery | Channel::Lambda | Channel::Egt) {
            return WarningFixture::Unavailable("Required operating context is not supplied by this typed scenario");
        }
        let (low, high) = widget.channel.domain();
        let witness = match w.mode {
            LimitMode::High => high,
            LimitMode::Low => low,
            LimitMode::OutsideWindow => {
                if low <= w.trigger {
                    low
                } else {
                    high
                }
            }
            LimitMode::InsideWindow => (w.trigger + w.advanced.trigger_high) / 2.,
            LimitMode::LambdaDeviation => {
                return WarningFixture::Unavailable("Required lambda target/load context unavailable");
            }
        };
        if witness.is_finite() && (low..=high).contains(&witness) {
            WarningFixture::Available(witness)
        } else {
            WarningFixture::Unavailable("No finite witness in the synthetic testing range")
        }
    }

    /// Advances selected fixture to `at_ms` milliseconds with at most eight typed ingests and one control sample,
    /// independent of skips. Returns InspectionOnly/Exhausted/Backdated before effects. Normal uses shared demo
    /// batch context; others use typed fixtures, with missing/stale/fault behavior and a deterministic
    /// warning-witness schedule. Success does not imply every ingest was admitted.
    pub fn scenario_step(
        &mut self,
        at_ms: u64,
    ) -> Result<(), SimulationError> {
        if self.invalid {
            return Err(SimulationError::InspectionOnly);
        }
        if self.source_epoch.is_none() {
            return Err(SimulationError::Exhausted);
        }
        if at_ms < self.now_ms() {
            return Err(SimulationError::Backdated);
        }
        let elapsed = at_ms.saturating_sub(super::SAMPLE_START_MS);
        let mut values = dashboard_core::demo::values(elapsed);
        let ingest = match self.scenario {
            Scenario::Missing => false,
            Scenario::Stale => !self.scenario_seeded,
            Scenario::SourceFault if self.scenario_seeded => {
                self.inject(SimulationInput::Fault {
                    sensor: SensorId::Water,
                    quality: Quality::TransportError,
                    at_ms,
                });
                false
            }
            Scenario::MinMax | Scenario::WidthStress => {
                values = Channel::ALL.map(|c| {
                    let (lo, hi) = c.domain();
                    if (elapsed / 2000).is_multiple_of(2) { lo } else { hi }
                });
                true
            }
            Scenario::Warnings => {
                // Select one rule in a bounded deterministic elapsed schedule. Conflicting
                // rules on one channel are never promised to activate simultaneously.
                let widgets = &self.config().config().widgets;
                let cycle: u64 = widgets
                    .iter()
                    .map(|w| u64::from(w.warning.on_ms) + u64::from(w.warning.off_ms) + 4000)
                    .sum();
                let mut phase = if cycle == 0 { 0 } else { elapsed % cycle };
                for (i, w) in widgets.iter().enumerate() {
                    let span = u64::from(w.warning.on_ms) + u64::from(w.warning.off_ms) + 4000;
                    if phase < span {
                        if let WarningFixture::Available(value) = self.warning_fixture(i) {
                            values[w.channel.index()] = value;
                        }
                        break;
                    }
                    phase -= span;
                }
                true
            }
            _ => true,
        };
        if ingest && self.scenario == Scenario::Normal {
            // Established shared Demo batch semantics include explicitly synthetic
            // operating context; custom/diagnostic/witness inputs remain typed.
            self.advance_to(at_ms, Some(values));
        } else if ingest {
            for channel in Channel::ALL {
                self.inject(SimulationInput::Channel {
                    channel,
                    value: values[channel.index()],
                    at_ms,
                });
            }
        }
        self.scenario_seeded = true;
        self.physical_at(self.held, dashboard_core::input_model::EncoderBatch::default(), at_ms);
        Ok(())
    }
}

#[cfg(test)]
/// Callback retirement at the counter boundary never revives previously captured authority.
mod callback_limits {
    use super::*;
    #[test]
    /// Gate exhaustion rejects old token and every subsequent capture, including repeated retire calls.
    fn exhausted_callback_epoch_never_reissues_an_old_capability() {
        let sim = ControlSimulator::new(dashboard_config::validate(dashboard_config::default_config()).unwrap());
        let mut gate = CallbackGate { epoch: Some(u64::MAX) };
        let token = gate.capture(sim.source_key()).unwrap();
        gate.retire();
        assert!(!gate.is_current(token, sim.source_key()));
        assert!(gate.capture(sim.source_key()).is_none());
        gate.retire();
        assert!(gate.capture(sim.source_key()).is_none());
    }
}
