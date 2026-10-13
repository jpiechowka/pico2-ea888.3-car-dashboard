//! Deterministic single-owner adapter for the same gestures and dispatcher as firmware.
//! Presentation completion is synthetic; this module does not render screen pixels.
use dashboard_config::{ButtonId, ControlMap, ValidatedConfig};
use dashboard_core::acquisition::DashboardSnapshot;
use dashboard_core::control_session::ControlSession;
use dashboard_core::controls::ControlGestures;
use dashboard_core::input_model::EncoderBatch;
use dashboard_core::input_trace::Observation;
use dashboard_core::logging::{Level, LogStore};
use embassy_sync::blocking_mutex::raw::NoopRawMutex;

use crate::simulation::Simulation;

pub const SAMPLE_START_MS: u64 = 118_000;

pub struct ControlSimulator {
    simulation: Simulation,
    map: ControlMap,
    gestures: ControlGestures,
    logs: LogStore<NoopRawMutex>,
    ignored: u32,
    reset_pending: bool,
    invalid: bool,
}

impl ControlSimulator {
    pub fn cache_exhaustion_status(&self) -> dashboard_core::cache_diagnostics::CacheExhaustionStatus {
        self.simulation.model.cache_exhaustion_status()
    }

    pub fn new(config: ValidatedConfig) -> Self {
        let map = config.config().controls;
        let mut simulator = Self {
            simulation: Simulation::new(config, SAMPLE_START_MS),
            map,
            gestures: ControlGestures::new(),
            logs: LogStore::new(),
            ignored: 0,
            reset_pending: false,
            invalid: false,
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
        *self = Self::new(config);
    }

    /// Candidate bindings are inspection only; the validated layout remains authoritative.
    pub fn inspect_invalid(
        &mut self,
        map: ControlMap,
    ) {
        self.map = map;
        self.invalid = true;
        self.gestures.reset();
        self.sample([false; 4], EncoderBatch::default());
    }

    /// End unchanged-config inspection without sampling candidate side effects
    /// or replacing acquisition, pending presentation, history or warning state.
    pub fn resume_accepted_bindings(&mut self) {
        self.map = self.simulation.configuration().config().controls;
        self.gestures.reset();
        self.invalid = false;
    }

    pub const fn session(&self) -> &ControlSession { &self.simulation.session }

    pub fn led_duty(&self) -> dashboard_core::status_led::LedDuty { self.simulation.led.duty() }

    pub const fn now_ms(&self) -> u64 { self.simulation.now_ms }

    pub const fn ignored(&self) -> u32 { self.ignored }

    pub const fn invalid(&self) -> bool { self.invalid }

    pub const fn reset_pending(&self) -> bool { self.reset_pending }

    pub fn config(&self) -> &ValidatedConfig { self.simulation.configuration() }

    pub(crate) fn presentation_epoch(&self) -> Option<u64> { self.simulation.presentation_epoch }

    fn sample(
        &mut self,
        pressed: [bool; 4],
        encoder: EncoderBatch,
    ) {
        self.simulation.model.tick(self.simulation.now_ms);
        self.simulation.session.expire_feedback(self.simulation.now_ms);
        let buttons = self.gestures.update(
            &self.map,
            self.simulation.session.page(),
            pressed,
            self.simulation.now_ms,
        );
        let effects = self.simulation.session.apply(
            self.simulation.model.configuration().expect("simulation config"),
            &self.map,
            buttons,
            encoder,
            &self.logs,
            self.simulation.now_ms,
        );
        self.ignored = self.ignored.saturating_add(effects.ignored);
        self.reset_pending |= effects.reset_requested;
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
        if !self.simulation.presented.complete(token) {
            return false;
        }
        self.simulation
            .session
            .backlight_mut()
            .frame_completed(token, self.simulation.now_ms);
        self.simulation.update_led();
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
        self.simulation.now_ms = self.simulation.now_ms.saturating_add(milliseconds);
        self.sample([false; 4], EncoderBatch::default());
    }

    fn press(
        &mut self,
        button: ButtonId,
        duration_ms: u64,
    ) {
        self.advance(50);
        let mut pressed = [false; 4];
        pressed[button.index()] = true;
        self.simulation.now_ms = self.simulation.now_ms.saturating_add(1);
        self.sample(pressed, EncoderBatch::default());
        self.simulation.now_ms = self.simulation.now_ms.saturating_add(50);
        self.sample(pressed, EncoderBatch::default());
        self.simulation.now_ms = self.simulation.now_ms.saturating_add(duration_ms.saturating_sub(50));
        self.sample(pressed, EncoderBatch::default());
        self.simulation.now_ms = self.simulation.now_ms.saturating_add(1);
        self.sample([false; 4], EncoderBatch::default());
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
                u64::from(self.map.hold_ms.clamp(400, 1500))
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
            .simulation
            .now_ms
            .checked_mul(1000)
            .and_then(|now| Observation::new(now, now));
        self.sample(
            [false; 4],
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
        if self.reset_pending {
            let generation = self
                .simulation
                .model
                .snapshot()
                .reset_generation
                .checked_add(1)
                .expect("reset generation exhausted");
            self.simulation.model.reset_statistics(generation);
            self.simulation.now_ms = self.simulation.now_ms.saturating_add(1);
            self.simulation.session.statistics_acknowledged(self.simulation.now_ms);
            self.reset_pending = false;
        }
    }

    pub fn summary(&self) -> String {
        let s = &self.simulation.session;
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
            self.simulation.now_ms,
            if self.reset_pending {
                " · statistics reset awaiting acknowledgment"
            } else {
                ""
            }
        )
    }
}
