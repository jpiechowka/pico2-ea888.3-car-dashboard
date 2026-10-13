//! Fixed synthetic fixtures and injection outcomes. No vehicle qualification.
use dashboard_config::{Channel, LimitMode};
use dashboard_core::samples::{Quality, Quantity, Sample, SampleError, SensorId, Source};

use super::ControlSimulator;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Scenario {
    #[default]
    Normal,
    Warnings,
    MinMax,
    WidthStress,
    Missing,
    Stale,
    SourceFault,
    DelayedResetAck,
    DroppedCompletion,
}
impl Scenario {
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
pub enum SimulationError {
    Exhausted,
    Backdated,
    InspectionOnly,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SimulationInput {
    Channel {
        channel: Channel,
        value: f32,
        at_ms: u64,
    },
    Sample(Sample),
    Fault {
        sensor: SensorId,
        quality: Quality,
        at_ms: u64,
    },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InputOutcome {
    Accepted,
    Rejected(SampleError),
    Parse,
    NotFinite,
    TestingRange,
    TooLong,
    Exhausted,
    InspectionOnly,
}

/// Retains only bounded text. An oversized visible edit revokes dispatch of
/// the previous value until a subsequent bounded edit is observed.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BoundedText<const N: usize> {
    text: String,
    blocked: bool,
}
impl<const N: usize> BoundedText<N> {
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

    pub fn text(&self) -> &str { &self.text }

    pub const fn is_blocked(&self) -> bool { self.blocked }

    pub fn admitted(&self) -> Option<&str> { (!self.blocked).then_some(self.text.as_str()) }
}
impl InputOutcome {
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
pub enum WarningFixture {
    Disabled,
    Available(f32),
    Unavailable(&'static str),
}
#[derive(Clone, Copy, Debug)]
pub struct SimulatorProjection {
    pub scenario: Scenario,
    pub source_epoch: Option<u64>,
    pub now_ms: u64,
    pub snapshot: dashboard_core::acquisition::DashboardSnapshot,
    pub page: dashboard_config::ControlPage,
    pub view: u8,
    pub invalid: bool,
    pub reset_token: Option<u64>,
    pub completed_warning: dashboard_core::status_led::PresentedWarning,
    pub led: dashboard_core::status_led::LedDuty,
    pub outcomes: [Option<InputOutcome>; 8],
}

/// Checked callback identity, separate from source and optimization cache identities.
pub struct CallbackGate {
    epoch: Option<u64>,
}
#[derive(Clone, Copy)]
pub struct CallbackToken {
    epoch: u64,
    source: super::SourceSessionKey,
    editor: Option<crate::editor::AcceptedConfigKey>,
}
impl Default for CallbackGate {
    fn default() -> Self { Self::new() }
}
impl CallbackGate {
    pub const fn new() -> Self { Self { epoch: Some(0) } }

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

    pub fn capture_for_editor(
        &self,
        source: Option<super::SourceSessionKey>,
        editor: crate::editor::AcceptedConfigKey,
    ) -> Option<CallbackToken> {
        let mut token = self.capture(source)?;
        token.editor = Some(editor);
        Some(token)
    }

    pub fn is_current_for_editor(
        &self,
        token: CallbackToken,
        source: Option<super::SourceSessionKey>,
        editor: crate::editor::AcceptedConfigKey,
    ) -> bool {
        token.editor == Some(editor) && self.is_current(token, source)
    }

    pub fn is_current(
        &self,
        token: CallbackToken,
        source: Option<super::SourceSessionKey>,
    ) -> bool {
        self.epoch == Some(token.epoch) && source == Some(token.source)
    }

    pub fn retire(&mut self) { self.epoch = self.epoch.and_then(|e| e.checked_add(1)); }
}

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

    /// The UI testing-range policy is deliberately absent from diagnostic injection.
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
                self.simulation.advance_to(self.now_ms().max(at_ms), None);
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
        self.simulation.advance_to(now, None);
        self.outcomes[channel] = Some(outcome);
        outcome
    }

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

    /// At most eight typed ingests and one control sample, independent of skipped time.
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
mod callback_limits {
    use super::*;
    #[test]
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
