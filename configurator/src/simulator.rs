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

use crate::json_draft::JsonPreviewProof;
use crate::simulation::Simulation;
/// Process-local nonzero source-owner issuer; checked increment permanently refuses reuse at exhaustion.
static NEXT_SOURCE_OWNER: AtomicUsize = AtomicUsize::new(1);
/// Returns a fresh nonzero owner or None at atomic counter exhaustion; never wraps or reclaims dropped owners.
fn issue_source_owner() -> Option<NonZeroUsize> {
    NEXT_SOURCE_OWNER
        .try_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_add(1))
        .ok()
        .and_then(NonZeroUsize::new)
}

/// Opaque semantic source identity. Content, counters and cache IDs cannot construct it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SourceSessionKey {
    /// Process-unique issued source owner, not a configuration content hash.
    owner: NonZeroUsize,
    /// Checked lifetime generation within the simulator, retired for source/startup replacement.
    generation: u64,
}
mod scenarios;
pub use scenarios::*;

/// Separate candidate controls/time without authority over accepted acquisition, statistics or completed warnings.
struct Inspection {
    /// Sealed valid JSON candidate, or None for invalid-map inspection against accepted config.
    candidate: Option<JsonPreviewProof>,
    /// Isolated page/view/feedback/backlight session.
    session: ControlSession,
    /// Isolated debounce/hold state, reset on lifecycle cancellation.
    gestures: ControlGestures,
    /// Candidate bindings retained for read-only behavior inspection.
    map: ControlMap,
    /// Inspection-relative monotonic milliseconds, initially accepted owner's current time.
    now_ms: u64,
    /// Saturating ignored-action count for this inspection session.
    ignored: u32,
    /// Last blocked reset/acknowledgement explanation, or None before one occurs.
    outcome: Option<&'static str>,
}

/// Initial synthetic milliseconds, providing a stable origin for fixtures/log history without claiming boot time.
pub const SAMPLE_START_MS: u64 = 118_000;

/// Persistent synthetic acquisition/control session with explicit frame completion and isolated invalid/candidate
/// inspection.
pub struct ControlSimulator {
    /// Accepted shared acquisition, controls, warning handoff and presented LED owner.
    simulation: Simulation,
    /// Accepted per-screen bindings.
    map: ControlMap,
    /// Accepted debounce/hold state.
    gestures: ControlGestures,
    /// Shared bounded synthetic log store; no physical device log stream.
    logs: LogStore<NoopRawMutex>,
    /// Saturating accepted-session ignored-action count.
    ignored: u32,
    /// Statistics reset awaits an exact producer acknowledgement.
    reset_pending: bool,
    /// Inspection-only mode blocks accepted injections/completion.
    invalid: bool,
    /// Independent candidate control session, absent during accepted operation.
    inspection: Option<Inspection>,
    /// Checked source generation; None refuses identity-bound work permanently.
    source_epoch: Option<u64>,
    /// Process-unique source issuer identity; None when construction exhausted.
    source_owner: Option<NonZeroUsize>,
    /// Selected deterministic source fixture.
    scenario: Scenario,
    /// Scenario has emitted its initial sample, controlling stale/fault fixtures.
    scenario_seeded: bool,
    /// Last statistics reset request number retained across source restarts.
    reset_requested: u64,
    /// Shared statistics reset issuer/acknowledgement correlation retained across source restarts.
    reset_requests: dashboard_core::acquisition::ResetRequests,
    /// Exact currently pending reset token, or None.
    reset_token: Option<u64>,
    /// Last accepted control refusal explanation, independent of sample outcomes.
    control_outcome: Option<&'static str>,
    /// Current physical-equivalent A/B/X/Y button levels.
    held: [bool; 4],
    /// Latest per-channel typed input result, in SensorId/Channel order.
    outcomes: [Option<InputOutcome>; 8],
    /// Last exact synthetically completed frame, absent after source/startup replacement.
    completed_frame: Option<dashboard_core::display::pipeline::FrameToken>,
}

impl ControlSimulator {
    /// Returns cache-exhaustion facts from the persistent accepted acquisition owner.
    pub fn cache_exhaustion_status(&self) -> dashboard_core::cache_diagnostics::CacheExhaustionStatus {
        self.simulation.model.cache_exhaustion_status()
    }

    /// Returns a new synthetic owner for `config`, attempting a fresh source identity and seeding bounded sample logs.
    /// Identity exhaustion leaves source_key unavailable; it never aliases another owner's pending pixels.
    pub fn new(config: ValidatedConfig) -> Self { Self::with_source(config, issue_source_owner()) }

    /// Returns an owner for `config` at SAMPLE_START_MS using optional issued `source_owner`; None makes identity
    /// unavailable. Seeds 32 synthetic log records and released control baseline, without ingesting vehicle
    /// samples.
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

    /// Replaces acquisition/control state with `config` under fresh checked source identity, retaining scenario,
    /// render-transport mode and reset issuer history. If generation/owner issuance exhausts, retires source
    /// authority and leaves old config/state.
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

    /// Replays `project` startup only when its decoded dashboard equals this accepted config; returns
    /// inspection/project/exhaustion errors. Retires old delivery identity before resetting presentation/gestures;
    /// acquisition/history remain and only converted artwork is retained.
    pub fn restart_project(
        &mut self,
        project: &dashboard_config_tools::project::Project,
    ) -> Result<(), SimulationError> {
        if self.invalid {
            return Err(SimulationError::InspectionOnly);
        }
        let config =
            dashboard_config::decode(project.dashboard_bytes()).map_err(|_| SimulationError::InvalidProject)?;
        if &config != self.config() {
            return Err(SimulationError::InvalidProject);
        }
        self.retire_startup_delivery()?;
        self.simulation.replay_startup(project);
        self.gestures = ControlGestures::new();
        self.gestures
            .update_startup(&self.map, self.session().page(), self.held, self.now_ms());
        Ok(())
    }

    /// Returns success after issuing a fresh source owner/generation and retiring pending/completed presentation.
    /// Returns Exhausted before mutating this simulator if either identity cannot be issued; acquisition remains
    /// untouched.
    fn retire_startup_delivery(&mut self) -> Result<(), SimulationError> {
        let epoch = self
            .source_epoch
            .and_then(|e| e.checked_add(1))
            .ok_or(SimulationError::Exhausted)?;
        let owner = issue_source_owner().ok_or(SimulationError::Exhausted)?;
        // Retire presentation callbacks without replacing acquisition, history or warning episodes.
        self.source_epoch = Some(epoch);
        self.source_owner = Some(owner);
        self.completed_frame = None;
        self.simulation.presented.retire_pending();
        Ok(())
    }

    /// Cancel a captured startup after a project edit, retaining acquisition and visible metadata.
    /// Retires delivery identity (or disables it on exhaustion), clears startup and suppresses held inputs before
    /// preparing controls.
    pub fn cancel_startup_replay(&mut self) {
        if self.retire_startup_delivery().is_err() {
            self.source_epoch = None;
        }
        self.simulation.startup = None;
        self.gestures.suppress_held();
        self.simulation.prepare(true);
    }

    /// Returns current captured startup phase, or Dashboard when no replay is installed.
    pub fn startup_phase(&self) -> dashboard_core::startup::Phase {
        self.simulation
            .startup
            .as_ref()
            .map_or(dashboard_core::startup::Phase::Dashboard, |s| s.state.phase())
    }

    /// Returns retained replay exit reason, or None while active/no replay; no physical boot evidence is implied.
    pub fn startup_exit(&self) -> Option<dashboard_core::startup::Exit> {
        self.simulation.startup.as_ref().and_then(|s| s.state.exit())
    }

    /// Returns active replay profile, phase, phase-relative elapsed milliseconds and borrowed converted asset, or None
    /// when inactive.
    pub(crate) fn startup_frame(
        &self
    ) -> Option<(
        dashboard_core::startup::StartupProfile,
        dashboard_core::startup::Phase,
        u64,
        Option<dashboard_config::startup_asset::Asset<'_>>,
    )> {
        let startup = self.simulation.startup.as_ref().filter(|s| s.state.active())?;
        Some((
            startup.profile,
            startup.state.phase(),
            startup.state.elapsed(self.now_ms()),
            startup.artwork.as_ref().map(|asset| asset.as_asset()),
        ))
    }

    /// Ends any startup replay with a synthetic global fault, suppresses held inputs and retires presentation
    /// continuity. Does not inject a sensor fault or alter accepted configuration.
    pub fn startup_fault(&mut self) {
        if let Some(startup) = &mut self.simulation.startup {
            startup
                .state
                .finish_at(dashboard_core::startup::Exit::Fault, self.simulation.now_ms);
        }
        self.gestures.suppress_held();
        self.simulation.prepare(true);
    }

    /// Installs read-only candidate `map` controls against accepted layout/acquisition, cancelling input edges.
    /// Repeating the same invalid map preserves inspection time/session; it never grants sample/reset/warning
    /// authority.
    pub fn inspect_invalid(
        &mut self,
        map: ControlMap,
    ) {
        if self
            .inspection
            .as_ref()
            .is_some_and(|i| i.candidate.is_none() && i.map == map)
        {
            return;
        }
        self.begin_inspection(map, None);
    }

    /// Installs valid unapplied `proof` as a separate inspection session sharing its exact sealed projection with
    /// candidate pixels. Repeating the same projection preserves session state; replacement cancels input edges
    /// without replacing acquisition.
    pub fn inspect_candidate(
        &mut self,
        proof: JsonPreviewProof,
    ) {
        if self.inspection.as_ref().is_some_and(|i| {
            i.candidate
                .as_ref()
                .is_some_and(|p| p.projection() == proof.projection())
        }) {
            return;
        }
        self.begin_inspection(proof.config().config().controls, Some(proof));
    }

    /// Returns installed valid candidate proof, or None for accepted operation/invalid-map inspection.
    pub fn inspection_candidate(&self) -> Option<&JsonPreviewProof> {
        self.inspection.as_ref().and_then(|i| i.candidate.as_ref())
    }

    /// Creates independent controls/time using `map` and optional valid `candidate`, seeded from accepted FPS/unit
    /// mode. None uses accepted layout; cancels input edges and sets inspection-only before publishing the new
    /// session.
    fn begin_inspection(
        &mut self,
        map: ControlMap,
        candidate: Option<JsonPreviewProof>,
    ) {
        self.cancel_inputs();
        self.invalid = true;
        let mut inspection = Inspection {
            session: ControlSession::new(
                candidate.as_ref().map_or(self.config(), |p| p.config()),
                self.session().fps(),
                self.session().boost_psi(),
            ),
            candidate,
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

    /// Returns accepted control session even while a separate inspection session is displayed.
    pub const fn session(&self) -> &ControlSession { &self.simulation.session }

    /// The live browser must never treat summary updates as pixel completion.
    /// Compatibility summary-only host probes may still use new() directly.
    pub fn require_frame_completion(&mut self) { self.simulation.render_transport = true; }

    /// Returns synthetic RGB PWM duty from accepted completed-scene policy; not physical LED or vehicle-health
    /// evidence.
    pub fn led_duty(&self) -> dashboard_core::status_led::LedDuty { self.simulation.led.duty() }

    /// Returns accepted acquisition/control milliseconds; inspection can have a different displayed clock.
    pub const fn now_ms(&self) -> u64 { self.simulation.now_ms }

    /// Returns saturating ignored-action count of the displayed session.
    pub fn ignored(&self) -> u32 { self.inspection.as_ref().map_or(self.ignored, |i| i.ignored) }

    /// Returns isolated inspection controls when installed, otherwise accepted controls.
    pub fn display_session(&self) -> &ControlSession { self.inspection.as_ref().map_or(self.session(), |i| &i.session) }

    /// Returns bindings used by the displayed session, including inspectable invalid candidate mappings.
    pub fn display_map(&self) -> &ControlMap { self.inspection.as_ref().map_or(&self.map, |i| &i.map) }

    /// Returns displayed-session milliseconds, isolated from accepted time during inspection.
    pub fn display_now_ms(&self) -> u64 { self.inspection.as_ref().map_or(self.now_ms(), |i| i.now_ms) }

    /// Returns the last inspection-only blocked-action reason, or None before rejection/outside inspection.
    pub fn inspection_outcome(&self) -> Option<&'static str> { self.inspection.as_ref().and_then(|i| i.outcome) }

    /// Returns the retained accepted-control refusal reason, or None before one occurs.
    pub const fn control_outcome(&self) -> Option<&'static str> { self.control_outcome }

    /// Returns local generation, or None after exhaustion; generation alone is insufficient callback authority.
    pub const fn source_epoch(&self) -> Option<u64> { self.source_epoch }

    /// Returns exact source owner/generation capability, or None if either identity is unavailable/exhausted.
    pub fn source_key(&self) -> Option<SourceSessionKey> {
        Some(SourceSessionKey {
            owner: self.source_owner?,
            generation: self.source_epoch?,
        })
    }

    /// Returns last recorded input outcomes in channel order; None entries have no recorded attempt.
    pub const fn input_outcomes(&self) -> [Option<InputOutcome>; 8] { self.outcomes }

    /// Returns last synthetically completed token, or None before completion/after delivery retirement.
    pub const fn completed_frame(&self) -> Option<dashboard_core::display::pipeline::FrameToken> {
        self.completed_frame
    }

    /// Returns the exact pending statistics-reset acknowledgement token, or None when no request awaits its producer.
    pub const fn pending_reset_token(&self) -> Option<u64> { self.reset_token }

    /// Returns a bounded synthetic log page at the displayed session's anchor.
    pub fn log_page(&self) -> dashboard_core::logging_view::LogPage {
        self.log_page_at(self.display_session().log_anchor())
    }

    /// Returns a bounded log window at `anchor`, with None selecting the latest page; does not mutate the log/session
    /// cursor.
    pub fn log_page_at(
        &self,
        anchor: Option<u64>,
    ) -> dashboard_core::logging_view::LogPage {
        dashboard_core::logging_view::LogPage::capture(&self.logs, anchor)
    }

    /// Requests acknowledgement of accepted completed warning identity, or records an inspection-only refusal without
    /// dispatch.
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

    /// Samples A/B/X/Y levels `pressed` and bounded `encoder` batch at displayed `at_ms` milliseconds.
    /// Ignores backward time or exhausted source authority; otherwise advances only the displayed session and
    /// dispatches gestures. This does not inject sensor samples; inspection effects remain isolated.
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

    /// Returns whether read-only invalid/unapplied inspection blocks accepted source actions and frame completion.
    pub const fn invalid(&self) -> bool { self.invalid }

    /// Returns whether accepted statistics reset still awaits exact producer acknowledgement.
    pub const fn reset_pending(&self) -> bool { self.reset_pending }

    /// Returns immutable accepted acquisition configuration, never the candidate inspection layout.
    pub fn config(&self) -> &ValidatedConfig { self.simulation.configuration() }

    /// Returns accepted presentation-continuity epoch, or None after exhaustion; not a source-ownership capability.
    pub(crate) fn presentation_epoch(&self) -> Option<u64> { self.simulation.presentation_epoch }

    /// Dispatches `pressed` levels and `encoder` at the already-selected session time, using shared gestures/control
    /// logic. Inspection records blocked reset/ack intent only; accepted mode ticks warnings, retires startup
    /// inputs, correlates reset requests and prepares presentation. Caller performs time/source admission;
    /// summary-only completion never acknowledges warnings.
    fn sample(
        &mut self,
        pressed: [bool; 4],
        encoder: EncoderBatch,
    ) {
        if let Some(i) = &mut self.inspection {
            i.session.expire_feedback(i.now_ms);
            let buttons = i.gestures.update(&i.map, i.session.page(), pressed, i.now_ms);
            let effects = i.session.apply(
                i.candidate
                    .as_ref()
                    .map_or(self.simulation.configuration(), |p| p.config()),
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
        let was_startup = self.simulation.startup_active();
        let mut buttons = if was_startup {
            self.gestures.update_startup(
                &self.map,
                self.simulation.session.page(),
                pressed,
                self.simulation.now_ms,
            )
        } else {
            self.gestures.update(
                &self.map,
                self.simulation.session.page(),
                pressed,
                self.simulation.now_ms,
            )
        };
        let mut encoder = encoder;
        if let Some(startup) = &mut self.simulation.startup {
            startup
                .state
                .consume_inputs(&mut self.gestures, &mut buttons, &mut encoder, self.simulation.now_ms);
        }
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
        if was_startup && !self.simulation.startup_active() {
            self.gestures.suppress_held();
        }
    }

    /// Advances accepted acquisition at `now_ms` with optional canonical `values`; None ticks time/results without
    /// readings. Suppresses held controls when startup exits so its gesture cannot leak into Dashboard.
    fn advance_acquisition(
        &mut self,
        now_ms: u64,
        values: Option<[f32; 8]>,
    ) {
        let was_startup = self.simulation.startup_active();
        self.simulation.advance_to(now_ms, values);
        if was_startup && !self.simulation.startup_active() {
            self.gestures.suppress_held();
        }
    }

    /// Advances accepted acquisition to `now_ms` with optional canonical Channel::ALL `values` and the shared
    /// synthetic cylinder generator at that time, preserving actual
    /// warning dwell. None ticks time/results without samples; backdated admission follows shared gates. Updates
    /// feedback/LED and summary-only backlight completion when enabled; low-level caller must enforce
    /// inspection/source policy before using this route.
    pub fn advance_to(
        &mut self,
        now_ms: u64,
        values: Option<[f32; 8]>,
    ) {
        self.advance_acquisition(now_ms, values);
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

    /// Returns currently prepared eligible warning page, or None while suppressed/not active; not completion evidence.
    pub(crate) fn warning_page(&self) -> Option<dashboard_core::view_state::WarningPage> { self.simulation.warning }

    /// Returns warning identity of the completed visible scene, including its empty-rule sentinel when no warning is
    /// visible.
    pub fn completed_warning(&self) -> dashboard_core::status_led::PresentedWarning {
        self.simulation.presented.warning()
    }

    /// Returns pending acknowledgement command until matching producer result, or None when no request remains.
    pub fn pending_warning_command(&self) -> Option<dashboard_core::warning_state::WarningCommand> {
        self.simulation.requests.pending()
    }

    /// Returns immutable scene metadata to bind to a render; this does not itself claim completion.
    pub(crate) fn presented_scene(&self) -> dashboard_core::status_led::PresentedScene { self.simulation.scene() }

    /// Returns shared accepted-session blink phase relative to its source origin.
    pub(crate) fn blink_on(&self) -> bool { self.simulation.blink_on() }

    /// Returns whether `token` can bind current immutable scene, enabling explicit transport and notifying
    /// startup/backlight submission. False leaves state unchanged; private preview caller first checks source
    /// identity and excludes inspection.
    pub(crate) fn accept_rendered(
        &mut self,
        token: dashboard_core::display::pipeline::FrameToken,
    ) -> bool {
        if !self.simulation.presented.submit(token, self.simulation.scene()) {
            return false;
        }
        self.simulation.render_transport = true;
        if let Some(startup) = &mut self.simulation.startup {
            startup.state.submitted(token);
        }
        self.simulation.session.backlight_mut().frame_submitted(token);
        true
    }

    /// Returns whether presented-scene ownership can accept `token`; caller separately checks source and inspection
    /// authority.
    pub(crate) fn can_accept_rendered(
        &self,
        token: dashboard_core::display::pipeline::FrameToken,
    ) -> bool {
        self.simulation.presented.can_submit(token)
    }

    /// Returns true only when accepted mode completes exact submitted `token`; updates startup/backlight/LED and
    /// completed identity. False leaves those states unchanged. Caller must first prove the consumer accepted the
    /// bound pixels; no physical scanout is implied.
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
        if let Some(startup) = &mut self.simulation.startup {
            startup.state.completed(token, self.simulation.now_ms);
        }
        self.simulation.update_led();
        self.completed_frame = Some(token);
        true
    }

    /// Processes optional canonical `values` and queued warning command at current accepted time; None permits
    /// result-only publication. Low-level deterministic producer hook does not perform browser inspection
    /// admission.
    pub fn producer_step(
        &mut self,
        values: Option<[f32; 8]>,
    ) {
        self.advance_acquisition(self.simulation.now_ms, values);
    }

    /// Offers `quality` fault for `sensor` at `now_ms` through the shared owner, then advances timers without readings.
    /// Shared owner may reject stale faults; this low-level hook does not return admission or apply browser inspection
    /// policy.
    pub fn inject_fault(
        &mut self,
        sensor: dashboard_core::samples::SensorId,
        quality: dashboard_core::samples::Quality,
        now_ms: u64,
    ) {
        self.simulation.model.mark_fault(sensor, quality, now_ms);
        self.advance_acquisition(now_ms, None);
    }

    /// Returns a coherent accepted acquisition publication, never candidate-generated samples.
    pub fn snapshot(&self) -> DashboardSnapshot { self.simulation.snapshot() }

    /// Advances displayed physical-control time by `milliseconds` with held buttons and no encoder input; overflow is a
    /// no-op.
    pub fn advance(
        &mut self,
        milliseconds: u64,
    ) {
        if let Some(at) = self.display_now_ms().checked_add(milliseconds) {
            self.physical_at(self.held, EncoderBatch::default(), at);
        }
    }

    /// Simulates debounced `button` press for `duration_ms` plus release/baseline samples, with 50-ms settling margins.
    /// Saturating timestamp additions cannot wrap; source/inspection policy follows physical_at.
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

    /// Simulates a 100-ms debounced tap of `button` followed by stable release; uses displayed-session bindings.
    pub fn tap(
        &mut self,
        button: ButtonId,
    ) {
        self.press(button, 100);
    }

    /// Simulates `button` hold at configured 400..1500-ms threshold, except reserved Y always lasts two seconds.
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

    /// Simulates reserved two-second Y recovery to Dashboard/Full, including debounced press/release margins.
    pub fn recover(&mut self) { self.hold(ButtonId::Y); }

    /// Dispatches one bounded encoder batch of signed `delta` detents and `presses` clicks after one simulated
    /// millisecond. Converts observation time to microseconds with checked arithmetic; missing representable timing
    /// remains unavailable.
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

    /// Dispatches signed `delta` encoder detents through the displayed session without click events.
    pub fn rotate(
        &mut self,
        delta: i32,
    ) {
        self.encoder(delta, 0);
    }

    /// Dispatches one encoder click through the displayed session without rotation.
    pub fn click(&mut self) { self.encoder(0, 1); }

    /// Attempts acknowledgement of the current reset token when present; no pending request is a no-op.
    pub fn acknowledge_reset(&mut self) {
        if let Some(token) = self.reset_token {
            self.acknowledge_reset_token(token);
        }
    }

    /// Returns true after accepted-mode exact pending `token` acknowledgement resets shared statistics and publishes
    /// feedback. False rejects inspection/stale/duplicate tokens or model refusal; issuer acknowledgement precedes
    /// the model call.
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

    /// Returns displayed synthetic screen/view/brightness/control/time status plus pending reset note; no physical
    /// measurements.
    pub fn summary(&self) -> String {
        let s = self.display_session();
        let screen = match self.startup_phase() {
            dashboard_core::startup::Phase::Status => "Startup status",
            dashboard_core::startup::Phase::Splash => "Startup splash",
            dashboard_core::startup::Phase::Dashboard => s.page().label(),
        };
        format!(
            "{} · view {} · {} · PWM {} · {} · {} · popup {:?} · logs {:?} · ignored {} · {} ms{}",
            screen,
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
/// Source/reset exhaustion refuses aliasing or phantom requests while preserving recovery/control observability.
mod lifecycle_limits {
    use super::*;
    /// Exhausted source identity cannot authorize unsubmitted scratch bytes or invoke a pixel consumer.
    #[test]
    fn exhausted_source_cannot_consume_unsubmitted_pixels() {
        let config = dashboard_config::validate(dashboard_config::default_config()).unwrap();
        let sim = ControlSimulator::with_source(config, None);
        let preview = crate::preview::Preview::new();
        let token = dashboard_core::display::pipeline::FrameToken {
            buffer: dashboard_core::display::pipeline::BufferId::A,
            sequence: 1,
        };
        assert!(preview.pending_pixels(&sim, token).is_none());
        let mut consumed = false;
        assert!(
            crate::event_adapters::consume_simulation_pixels(&sim, &preview, token, |_| {
                consumed = true;
                Ok(())
            })
            .is_err()
        );
        assert!(!consumed);
    }
    #[test]
    /// Exhausted replacement cannot publish new config, source authority or frame completion.
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
    /// Reset counter exhaustion cannot create phantom feedback/request and still permits reserved recovery.
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
