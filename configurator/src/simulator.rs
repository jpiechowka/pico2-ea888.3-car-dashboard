//! Deterministic single-owner adapter for the same gestures and dispatcher as firmware.
//! Presentation completion is synthetic; this module does not render screen pixels.
use std::num::NonZeroUsize;
use std::sync::atomic::{AtomicUsize, Ordering};

use dashboard_config::{ButtonId, ControlMap, ValidatedConfig};
use dashboard_core::acquisition::DashboardSnapshot;
use dashboard_core::control_session::ControlSession;
use dashboard_core::controls::ControlGestures;
use dashboard_core::input_model::EncoderBatch;
use dashboard_core::input_trace::Observation;
use dashboard_core::logging::{Level, LogStore};
use embassy_sync::blocking_mutex::raw::NoopRawMutex;

use crate::simulation::Simulation;
static NEXT_SOURCE_OWNER: AtomicUsize = AtomicUsize::new(1);
fn issue_source_owner() -> Option<NonZeroUsize> {
    NEXT_SOURCE_OWNER
        .try_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_add(1))
        .ok()
        .and_then(NonZeroUsize::new)
}

/// Opaque semantic source identity. Content, counters and cache IDs cannot construct it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SourceSessionKey {
    owner: NonZeroUsize,
    generation: u64,
}
mod scenarios;
pub use scenarios::*;

struct Inspection {
    session: ControlSession,
    gestures: ControlGestures,
    map: ControlMap,
    now_ms: u64,
    ignored: u32,
    outcome: Option<&'static str>,
}

pub const SAMPLE_START_MS: u64 = 118_000;

pub struct ControlSimulator {
    simulation: Simulation,
    map: ControlMap,
    gestures: ControlGestures,
    logs: LogStore<NoopRawMutex>,
    ignored: u32,
    reset_pending: bool,
    invalid: bool,
    inspection: Option<Inspection>,
    source_epoch: Option<u64>,
    source_owner: Option<NonZeroUsize>,
    scenario: Scenario,
    scenario_seeded: bool,
    reset_requested: u64,
    reset_requests: dashboard_core::acquisition::ResetRequests,
    reset_token: Option<u64>,
    control_outcome: Option<&'static str>,
    held: [bool; 4],
    outcomes: [Option<InputOutcome>; 8],
    completed_frame: Option<dashboard_core::display::pipeline::FrameToken>,
}

impl ControlSimulator {
    pub fn cache_exhaustion_status(&self) -> dashboard_core::cache_diagnostics::CacheExhaustionStatus {
        self.simulation.model.cache_exhaustion_status()
    }

    pub fn new(config: ValidatedConfig) -> Self { Self::with_source(config, issue_source_owner()) }

    fn with_source(
        config: ValidatedConfig,
        source_owner: Option<NonZeroUsize>,
    ) -> Self {
        let map = config.config().controls;
        let mut simulator = Self {
            simulation: Simulation::new(config, SAMPLE_START_MS),
            map,
            gestures: ControlGestures::new(),
            logs: LogStore::new(),
            ignored: 0,
            reset_pending: false,
            invalid: false,
            inspection: None,
            source_epoch: source_owner.map(|_| 0),
            source_owner,
            scenario: Scenario::Normal,
            scenario_seeded: false,
            reset_requested: 0,
            reset_requests: dashboard_core::acquisition::ResetRequests::default(),
            reset_token: None,
            control_outcome: None,
            held: [false; 4],
            outcomes: [None; 8],
            completed_frame: None,
        };
        for index in 0..32 {
            simulator.logs.push(
                Level::Info,
                SAMPLE_START_MS - (32 - index) * 1000,
                format_args!("Sample log {index}"),
            );
        }
        simulator.sample([false; 4], EncoderBatch::default());
        simulator
    }

    pub fn restart(
        &mut self,
        config: ValidatedConfig,
    ) {
        let Some(epoch) = self.source_epoch.and_then(|e| e.checked_add(1)) else {
            self.source_epoch = None;
            return;
        };
        let Some(owner) = issue_source_owner() else {
            self.source_epoch = None;
            return;
        };
        let scenario = self.scenario;
        let render_transport = self.simulation.render_transport;
        // The reset transport issuer survives source restarts, like the dock's
        // frame issuer. Old delivered acknowledgments must never alias a new request.
        let reset_requests = std::mem::take(&mut self.reset_requests);
        let reset_requested = self.reset_requested;
        *self = Self::with_source(config, Some(owner));
        self.simulation.render_transport = render_transport;
        self.reset_requests = reset_requests;
        self.reset_requested = reset_requested;
        self.source_epoch = Some(epoch);
        self.scenario = scenario;
    }

    /// Candidate bindings are inspection only; the validated layout remains authoritative.
    pub fn inspect_invalid(
        &mut self,
        map: ControlMap,
    ) {
        if self.inspection.as_ref().is_some_and(|i| i.map == map) {
            return;
        }
        self.cancel_inputs();
        self.invalid = true;
        let mut inspection = Inspection {
            session: ControlSession::new(self.config(), self.session().fps(), self.session().boost_psi()),
            gestures: ControlGestures::new(),
            map,
            now_ms: self.now_ms(),
            ignored: 0,
            outcome: None,
        };
        inspection
            .gestures
            .update(&map, inspection.session.page(), [false; 4], inspection.now_ms);
        self.inspection = Some(inspection);
    }

    /// End unchanged-config inspection without sampling candidate side effects
    /// or replacing acquisition, pending presentation, history or warning state.
    pub fn resume_accepted_bindings(&mut self) {
        self.cancel_inputs();
        self.inspection = None;
        self.invalid = false;
    }

    pub const fn session(&self) -> &ControlSession { &self.simulation.session }

    /// The live browser must never treat summary updates as pixel completion.
    /// Compatibility summary-only host probes may still use new() directly.
    pub fn require_frame_completion(&mut self) { self.simulation.render_transport = true; }

    pub fn led_duty(&self) -> dashboard_core::status_led::LedDuty { self.simulation.led.duty() }

    pub const fn now_ms(&self) -> u64 { self.simulation.now_ms }

    pub fn ignored(&self) -> u32 { self.inspection.as_ref().map_or(self.ignored, |i| i.ignored) }

    pub fn display_session(&self) -> &ControlSession { self.inspection.as_ref().map_or(self.session(), |i| &i.session) }

    pub fn display_map(&self) -> &ControlMap { self.inspection.as_ref().map_or(&self.map, |i| &i.map) }

    pub fn display_now_ms(&self) -> u64 { self.inspection.as_ref().map_or(self.now_ms(), |i| i.now_ms) }

    pub fn inspection_outcome(&self) -> Option<&'static str> { self.inspection.as_ref().and_then(|i| i.outcome) }

    pub const fn control_outcome(&self) -> Option<&'static str> { self.control_outcome }

    pub const fn source_epoch(&self) -> Option<u64> { self.source_epoch }

    pub fn source_key(&self) -> Option<SourceSessionKey> {
        Some(SourceSessionKey {
            owner: self.source_owner?,
            generation: self.source_epoch?,
        })
    }

    pub const fn input_outcomes(&self) -> [Option<InputOutcome>; 8] { self.outcomes }

    pub const fn completed_frame(&self) -> Option<dashboard_core::display::pipeline::FrameToken> {
        self.completed_frame
    }

    pub const fn pending_reset_token(&self) -> Option<u64> { self.reset_token }

    pub fn log_page(&self) -> dashboard_core::logging_view::LogPage {
        dashboard_core::logging_view::LogPage::capture(&self.logs, self.display_session().log_anchor())
    }

    pub fn acknowledge_warnings(&mut self) {
        if let Some(inspection) = &mut self.inspection {
            inspection.outcome = Some("Inspection only; warning acknowledgment not dispatched");
        } else {
            self.simulation.request_acknowledgment();
        }
    }

    /// Lifecycle cancellation is not a sampled release edge.
    pub fn cancel_inputs(&mut self) {
        self.held = [false; 4];
        self.gestures.reset();
        self.gestures.update(
            &self.map,
            self.simulation.session.page(),
            [false; 4],
            self.simulation.now_ms,
        );
        if let Some(i) = &mut self.inspection {
            i.gestures.reset();
            i.gestures.update(&i.map, i.session.page(), [false; 4], i.now_ms);
        }
    }

    pub fn physical_at(
        &mut self,
        pressed: [bool; 4],
        encoder: EncoderBatch,
        at_ms: u64,
    ) {
        if at_ms < self.display_now_ms() || self.source_epoch.is_none() {
            return;
        }
        self.held = pressed;
        if let Some(i) = &mut self.inspection {
            i.now_ms = at_ms;
        } else {
            self.simulation.now_ms = at_ms;
        }
        self.sample(pressed, encoder);
    }

    pub const fn invalid(&self) -> bool { self.invalid }

    pub const fn reset_pending(&self) -> bool { self.reset_pending }

    pub fn config(&self) -> &ValidatedConfig { self.simulation.configuration() }

    pub(crate) fn presentation_epoch(&self) -> Option<u64> { self.simulation.presentation_epoch }

    fn sample(
        &mut self,
        pressed: [bool; 4],
        encoder: EncoderBatch,
    ) {
        if let Some(i) = &mut self.inspection {
            i.session.expire_feedback(i.now_ms);
            let buttons = i.gestures.update(&i.map, i.session.page(), pressed, i.now_ms);
            let effects = i.session.apply(
                self.simulation.configuration(),
                &i.map,
                buttons,
                encoder,
                &self.logs,
                i.now_ms,
            );
            i.ignored = i.ignored.saturating_add(effects.ignored);
            if effects.acknowledge_requested {
                i.outcome = Some("Inspection only; warning acknowledgment not dispatched");
            }
            if effects.reset_requested {
                i.outcome = Some("Inspection only; reset not dispatched");
            }
            i.session.backlight_mut().frame_unchanged(i.now_ms);
            return;
        }
        self.simulation.model.tick(self.simulation.now_ms);
        self.simulation.session.expire_feedback(self.simulation.now_ms);
        let mut buttons = self.gestures.update(
            &self.map,
            self.simulation.session.page(),
            pressed,
            self.simulation.now_ms,
        );
        let mut encoder = encoder;
        if self.reset_requested.checked_add(1).is_none() {
            let old = buttons.actions.len();
            buttons
                .actions
                .retain(|event| event.action != dashboard_config::ControlAction::ResetStatistics);
            let discarded = old - buttons.actions.len();
            buttons.discarded = buttons.discarded.saturating_add(discarded as u32);
            let encoder_reset = self.map.screens[self.session().page().index()].encoder_click
                == dashboard_config::ControlAction::ResetStatistics
                && encoder.presses != 0;
            if encoder_reset {
                buttons.discarded = buttons.discarded.saturating_add(u32::from(encoder.presses));
                encoder.presses = 0;
                encoder.button = None;
            }
            if discarded != 0 || encoder_reset {
                self.control_outcome = Some("Statistics reset identity exhausted; request rejected");
            }
        }
        let effects = self.simulation.session.apply(
            self.simulation.model.configuration().expect("simulation config"),
            &self.map,
            buttons,
            encoder,
            &self.logs,
            self.simulation.now_ms,
        );
        self.ignored = self.ignored.saturating_add(effects.ignored);
        if effects.reset_requested
            && let Some(next) = self.reset_requested.checked_add(1)
        {
            self.reset_requested = self.reset_requests.request();
            self.reset_token = Some(next);
            self.reset_pending = true;
        }
        if effects.acknowledge_requested {
            self.simulation.request_acknowledgment();
        }
        self.simulation.retry();
        // Summary feedback completes only its backlight model, never warning identity. Off feedback starts here;
        // a later expired popup still requires a separate clear presentation.
        if !self.simulation.render_transport {
            self.simulation
                .session
                .backlight_mut()
                .frame_unchanged(self.simulation.now_ms);
        }
        self.simulation.prepare(effects.view_changed);
    }

    /// Stateful sampling uses real warning dwell. None advances time without readings.
    pub fn advance_to(
        &mut self,
        now_ms: u64,
        values: Option<[f32; 8]>,
    ) {
        self.simulation.advance_to(now_ms, values);
        self.simulation.session.expire_feedback(self.simulation.now_ms);
        // Same synthetic summary completion used by the existing controls UI.
        if !self.simulation.render_transport {
            self.simulation
                .session
                .backlight_mut()
                .frame_unchanged(self.simulation.now_ms);
        }
        self.simulation.update_led();
    }

    pub(crate) fn warning_page(&self) -> Option<dashboard_core::view_state::WarningPage> { self.simulation.warning }

    pub fn completed_warning(&self) -> dashboard_core::status_led::PresentedWarning {
        self.simulation.presented.warning()
    }

    pub fn pending_warning_command(&self) -> Option<dashboard_core::warning_state::WarningCommand> {
        self.simulation.requests.pending()
    }

    pub(crate) fn presented_scene(&self) -> dashboard_core::status_led::PresentedScene { self.simulation.scene() }

    pub(crate) fn blink_on(&self) -> bool { self.simulation.blink_on() }

    pub(crate) fn accept_rendered(
        &mut self,
        token: dashboard_core::display::pipeline::FrameToken,
    ) -> bool {
        if !self.simulation.presented.submit(token, self.simulation.scene()) {
            return false;
        }
        self.simulation.render_transport = true;
        self.simulation.session.backlight_mut().frame_submitted(token);
        true
    }

    pub(crate) fn can_accept_rendered(
        &self,
        token: dashboard_core::display::pipeline::FrameToken,
    ) -> bool {
        self.simulation.presented.can_submit(token)
    }

    pub fn complete_frame(
        &mut self,
        token: dashboard_core::display::pipeline::FrameToken,
    ) -> bool {
        if self.invalid || !self.simulation.presented.complete(token) {
            return false;
        }
        self.simulation
            .session
            .backlight_mut()
            .frame_completed(token, self.simulation.now_ms);
        self.simulation.update_led();
        self.completed_frame = Some(token);
        true
    }

    /// Explicit producer step allows deterministic command/result delay and sample races.
    pub fn producer_step(
        &mut self,
        values: Option<[f32; 8]>,
    ) {
        self.simulation.advance_to(self.simulation.now_ms, values);
    }

    /// Explicit fault injection uses the same qualified owner route as firmware.
    pub fn inject_fault(
        &mut self,
        sensor: dashboard_core::samples::SensorId,
        quality: dashboard_core::samples::Quality,
        now_ms: u64,
    ) {
        self.simulation.model.mark_fault(sensor, quality, now_ms);
        self.simulation.advance_to(now_ms, None);
    }

    pub fn snapshot(&self) -> DashboardSnapshot { self.simulation.snapshot() }

    pub fn advance(
        &mut self,
        milliseconds: u64,
    ) {
        if let Some(at) = self.display_now_ms().checked_add(milliseconds) {
            self.physical_at(self.held, EncoderBatch::default(), at);
        }
    }

    fn press(
        &mut self,
        button: ButtonId,
        duration_ms: u64,
    ) {
        self.advance(50);
        let mut pressed = [false; 4];
        pressed[button.index()] = true;
        self.physical_at(
            pressed,
            EncoderBatch::default(),
            self.display_now_ms().saturating_add(1),
        );
        self.physical_at(
            pressed,
            EncoderBatch::default(),
            self.display_now_ms().saturating_add(50),
        );
        self.physical_at(
            pressed,
            EncoderBatch::default(),
            self.display_now_ms().saturating_add(duration_ms.saturating_sub(50)),
        );
        self.physical_at(
            [false; 4],
            EncoderBatch::default(),
            self.display_now_ms().saturating_add(1),
        );
        self.advance(50);
        // Let a newly selected page capture a stable released baseline.
        self.advance(50);
    }

    pub fn tap(
        &mut self,
        button: ButtonId,
    ) {
        self.press(button, 100);
    }

    pub fn hold(
        &mut self,
        button: ButtonId,
    ) {
        self.press(
            button,
            if button == ButtonId::Y {
                2000
            } else {
                u64::from(self.display_map().hold_ms.clamp(400, 1500))
            },
        );
    }

    pub fn recover(&mut self) { self.hold(ButtonId::Y); }

    /// Submit one bounded encoder batch through the shared control dispatcher.
    pub fn encoder(
        &mut self,
        delta: i32,
        presses: u8,
    ) {
        self.advance(1);
        let observation = self
            .display_now_ms()
            .checked_mul(1000)
            .and_then(|now| Observation::new(now, now));
        self.sample(
            self.held,
            EncoderBatch {
                delta,
                presses,
                motion: (delta != 0).then_some(observation).flatten(),
                button: (presses != 0).then_some(observation).flatten().map(|mut o| {
                    o.events = u32::from(presses);
                    o
                }),
            },
        );
    }

    pub fn rotate(
        &mut self,
        delta: i32,
    ) {
        self.encoder(delta, 0);
    }

    pub fn click(&mut self) { self.encoder(0, 1); }

    pub fn acknowledge_reset(&mut self) {
        if let Some(token) = self.reset_token {
            self.acknowledge_reset_token(token);
        }
    }

    pub fn acknowledge_reset_token(
        &mut self,
        token: u64,
    ) -> bool {
        if self.invalid || self.reset_token != Some(token) || !self.reset_requests.acknowledge(token) {
            return false;
        }
        if !self.simulation.model.reset_statistics(token) {
            return false;
        }
        self.simulation.session.statistics_acknowledged(self.simulation.now_ms);
        self.reset_pending = false;
        self.reset_token = None;
        self.simulation.prepare(false);
        true
    }

    pub fn summary(&self) -> String {
        let s = self.display_session();
        format!(
            "{} · view {} · {} · PWM {} · {} · {} · popup {:?} · logs {:?} · ignored {} · {} ms{}",
            s.page().label(),
            s.view_id(),
            s.backlight().brightness().label(),
            s.backlight().compare(),
            s.fps().label(),
            if s.boost_psi() { "PSI" } else { "bar" },
            s.popup(),
            s.log_anchor(),
            self.ignored,
            self.display_now_ms(),
            if self.reset_pending {
                " · statistics reset awaiting acknowledgment"
            } else {
                ""
            }
        )
    }
}

#[cfg(test)]
mod lifecycle_limits {
    use super::*;
    #[test]
    fn exhausted_source_rejects_replacement_before_publishing_config_or_frame() {
        let mut sim = ControlSimulator::new(dashboard_config::validate(dashboard_config::default_config()).unwrap());
        let before = sim.snapshot().warning_run_id();
        sim.source_epoch = Some(u64::MAX);
        assert_eq!(sim.set_scenario(Scenario::Missing), Err(SimulationError::Exhausted));
        assert_eq!(sim.snapshot().warning_run_id(), before);
        sim.restart(sim.config().clone());
        assert_eq!(sim.source_key(), None);
        let mut preview = crate::preview::Preview::new();
        let token = dashboard_core::display::pipeline::FrameToken {
            buffer: dashboard_core::display::pipeline::BufferId::A,
            sequence: 1,
        };
        assert!(preview.render_simulation_submitted(&mut sim, token).is_err());
        assert!(!sim.complete_frame(token));
    }
    #[test]
    fn exhausted_reset_counter_cannot_publish_phantom_session_request() {
        let mut sim = ControlSimulator::new(dashboard_config::validate(dashboard_config::default_config()).unwrap());
        sim.reset_requested = u64::MAX;
        sim.tap(ButtonId::B);
        sim.simulation.session.statistics_acknowledged(sim.now_ms());
        assert!(sim.session().popup().is_none());
        assert!(!sim.reset_pending());
        sim.tap(ButtonId::Y);
        sim.recover();
        assert_eq!(sim.session().page(), dashboard_config::ControlPage::Dashboard);
    }
}
