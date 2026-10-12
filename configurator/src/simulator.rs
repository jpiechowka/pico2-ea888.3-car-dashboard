//! Deterministic single-owner adapter for the same gestures and dispatcher as firmware.
//! Presentation completion is synthetic; this module does not render screen pixels.
use dashboard_config::{ButtonId, ControlMap, ValidatedConfig};
use dashboard_core::control_session::ControlSession;
use dashboard_core::controls::ControlGestures;
use dashboard_core::input_model::EncoderBatch;
use dashboard_core::input_trace::Observation;
use dashboard_core::logging::{Level, LogStore};
use dashboard_core::render::FpsMode;
use embassy_sync::blocking_mutex::raw::NoopRawMutex;

pub const SAMPLE_START_MS: u64 = 118_000;

pub struct ControlSimulator {
    config: ValidatedConfig,
    map: ControlMap,
    session: ControlSession,
    gestures: ControlGestures,
    logs: LogStore<NoopRawMutex>,
    now_ms: u64,
    ignored: u32,
    reset_pending: bool,
    invalid: bool,
}

impl ControlSimulator {
    pub fn cache_exhaustion_status(&self) -> dashboard_core::cache_diagnostics::CacheExhaustionStatus {
        dashboard_core::cache_diagnostics::CacheExhaustionStatus::from_config(self.config.cache_identity_status())
    }

    pub fn new(config: ValidatedConfig) -> Self {
        let session = ControlSession::new(&config, FpsMode::Off, false);
        let map = config.config().controls;
        let mut simulator = Self {
            config,
            map,
            session,
            gestures: ControlGestures::new(),
            logs: LogStore::new(),
            now_ms: SAMPLE_START_MS,
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

    pub const fn session(&self) -> &ControlSession { &self.session }

    pub const fn now_ms(&self) -> u64 { self.now_ms }

    pub const fn ignored(&self) -> u32 { self.ignored }

    pub const fn invalid(&self) -> bool { self.invalid }

    pub const fn reset_pending(&self) -> bool { self.reset_pending }

    pub const fn config(&self) -> &ValidatedConfig { &self.config }

    fn sample(
        &mut self,
        pressed: [bool; 4],
        encoder: EncoderBatch,
    ) {
        self.session.expire_feedback(self.now_ms);
        let buttons = self
            .gestures
            .update(&self.map, self.session.page(), pressed, self.now_ms);
        let effects = self
            .session
            .apply(&self.config, &self.map, buttons, encoder, &self.logs, self.now_ms);
        self.ignored = self.ignored.saturating_add(effects.ignored);
        self.reset_pending |= effects.reset_requested;
        // Treat the summary presentation as completed. Off feedback starts here;
        // a later expired popup still requires a separate clear presentation.
        self.session.backlight_mut().frame_unchanged(self.now_ms);
    }

    pub fn advance(
        &mut self,
        milliseconds: u64,
    ) {
        self.now_ms = self.now_ms.saturating_add(milliseconds);
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
        self.now_ms = self.now_ms.saturating_add(1);
        self.sample(pressed, EncoderBatch::default());
        self.now_ms = self.now_ms.saturating_add(50);
        self.sample(pressed, EncoderBatch::default());
        self.now_ms = self.now_ms.saturating_add(duration_ms.saturating_sub(50));
        self.sample(pressed, EncoderBatch::default());
        self.now_ms = self.now_ms.saturating_add(1);
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

    fn encoder(
        &mut self,
        delta: i32,
        presses: u8,
    ) {
        self.advance(1);
        let observation = self.now_ms.checked_mul(1000).and_then(|now| Observation::new(now, now));
        self.sample(
            [false; 4],
            EncoderBatch {
                delta,
                presses,
                motion: (delta != 0).then_some(observation).flatten(),
                button: (presses != 0).then_some(observation).flatten(),
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
            self.now_ms = self.now_ms.saturating_add(1);
            self.session.statistics_acknowledged(self.now_ms);
            self.reset_pending = false;
        }
    }

    pub fn summary(&self) -> String {
        let s = &self.session;
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
            self.now_ms,
            if self.reset_pending {
                " · statistics reset awaiting acknowledgment"
            } else {
                ""
            }
        )
    }
}
