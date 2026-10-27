//! Browser runtime with one immutable configuration and one acquisition lifetime.
use dashboard_config::ValidatedConfig;
use dashboard_core::acquisition::{DashboardSnapshot, OwnedAcquisitionModel};
use dashboard_core::control_session::ControlSession;
use dashboard_core::render::FpsMode;
use dashboard_core::status_led::{LedController, PresentedLed, PresentedScene};
use dashboard_core::view_state::{WarningPage, WarningPager};
use dashboard_core::warning_state::{WarningCommand, WarningFeedback, WarningRequests};
use embassy_sync::blocking_mutex::raw::NoopRawMutex;
use embassy_sync::channel::Channel;

/// Captured startup presentation replay sharing acquisition but retaining only converted artwork.
pub(crate) struct StartupReplay {
    /// Captured timings, orientations, theme and warning policy.
    pub(crate) profile: dashboard_core::startup::StartupProfile,
    /// Completion-driven shared startup phase machine.
    pub(crate) state: dashboard_core::startup::Startup,
    /// Immutable converted pixels; absent for procedural themes, no original source payload.
    pub(crate) artwork: Option<dashboard_config::startup_asset::OwnedAsset>,
}
/// Single-thread browser acquisition/control owner, with capacity-one warning handoff and completed-scene LED state.
pub(crate) struct Simulation {
    /// Optional captured startup replay; completed replay retains its exit evidence.
    pub(crate) startup: Option<StartupReplay>,
    /// Shared configured synthetic acquisition, statistics and qualified warning state.
    pub(crate) model: OwnedAcquisitionModel,
    /// Accepted controls/page/view/backlight session.
    pub(crate) session: ControlSession,
    /// Monotonic retained simulation milliseconds, not browser wall time.
    pub(crate) now_ms: u64,
    /// Warning-page ordering/dwell paused outside visible Dashboard warning presentation.
    pager: WarningPager,
    /// Logical RGB duty driven by completed scene and current backlight.
    pub(crate) led: LedController,
    /// Submitted/completed scene identities used for warning acknowledgement and LED visibility.
    pub(crate) presented: PresentedLed,
    /// Checked warning command issuer and pending/result correlation.
    pub(crate) requests: WarningRequests,
    /// Single-thread capacity-one producer handoff; no blocking sends/receives.
    queue: Channel<NoopRawMutex, WarningCommand, 1>,
    /// Blink origin in simulation milliseconds.
    start_ms: u64,
    /// True requires explicit accepted-pixel completion instead of summary-only backlight completion.
    pub(crate) render_transport: bool,
    /// Last prepared page/view pair for continuity invalidation.
    visible: (dashboard_core::pages::Page, u8),
    /// Checked presentation continuity epoch; None permanently disables cache continuity after overflow.
    pub(crate) presentation_epoch: Option<u64>,
    /// Current eligible warning page, absent during startup, feedback or non-Dashboard pages.
    pub(crate) warning: Option<WarningPage>,
}
impl Simulation {
    /// Returns a new shared synthetic owner for `config` at `now_ms` milliseconds, without startup replay or initial
    /// samples. Starts Dashboard controls, empty warning handoff and epoch zero; actual canvas completion remains
    /// caller-managed.
    pub(crate) fn new(
        config: ValidatedConfig,
        now_ms: u64,
    ) -> Self {
        let session = ControlSession::new(&config, FpsMode::Off, false);
        let visible = (session.page(), session.view_id());
        Self {
            startup: None,
            visible,
            presentation_epoch: Some(0),
            model: OwnedAcquisitionModel::demo_configured(Box::new(config)),
            session,
            now_ms,
            pager: WarningPager::new(),
            led: LedController::new(),
            presented: PresentedLed::new(),
            requests: WarningRequests::new(),
            queue: Channel::new(),
            start_ms: now_ms,
            render_transport: false,
            warning: None,
        }
    }

    /// Returns the retained validated configuration; panics if the configured-model construction invariant is broken.
    pub(crate) fn configuration(&self) -> &ValidatedConfig {
        self.model
            .configuration()
            .expect("simulation always has a configuration")
    }

    /// Processes at most one canonical Channel::ALL `values` batch, one shared synthetic cylinder batch and one
    /// queued warning command at `now_ms` milliseconds, then ticks state and prepares presentation. None advances
    /// timers/results without inventing samples. Backdated sample admission follows the shared gate; retained time
    /// never decreases and no elapsed-proportional replay runs.
    pub(crate) fn advance_to(
        &mut self,
        now_ms: u64,
        values: Option<[f32; 8]>,
    ) {
        // At most one real sample per call. No replay proportional to elapsed time.
        // Preserve the shared model's rejection semantics for backdated readings.
        if let Some(values) = values {
            self.model.ingest_batch(values, now_ms);
            self.model
                .ingest_ignition_demo(dashboard_core::demo::ignition_values(now_ms), now_ms);
        }
        if let Ok(command) = self.queue.try_receive() {
            self.model.apply_warning_command(command, now_ms);
        }
        self.model.tick(now_ms);
        self.advance_startup(now_ms);
        self.now_ms = self.now_ms.max(now_ms);
        self.observe_result();
        self.session.expire_feedback(self.now_ms);
        self.prepare(false);
    }

    /// Captures `project` startup/profile and converted artwork, resets controls/paging and retires pending
    /// presentation. Caller must ensure the project's dashboard matches this owner. Acquisition/history/warning
    /// episodes remain intact; checked continuity exhaustion becomes None rather than recycling identity.
    pub(crate) fn replay_startup(
        &mut self,
        project: &dashboard_config_tools::project::Project,
    ) {
        self.session = ControlSession::new(self.configuration(), FpsMode::Off, false);
        self.pager = WarningPager::new();
        self.warning = None;
        self.visible = (self.session.page(), self.session.view_id());
        self.presentation_epoch = self.presentation_epoch.and_then(|n| n.checked_add(1));
        self.presented.retire_pending();
        self.startup = Some(StartupReplay {
            profile: project.profile(),
            state: dashboard_core::startup::Startup::new(project.profile(), self.now_ms),
            artwork: project.artwork().map(|a| a.owned_asset()),
        });
        self.prepare(false);
    }

    /// Returns whether a captured replay remains in an active startup phase.
    pub(crate) fn startup_active(&self) -> bool { self.startup.as_ref().is_some_and(|s| s.state.active()) }

    /// Advances captured startup at `now` milliseconds using current qualified attention and no injected global fault.
    /// Phase changes retire continuity; this does not acknowledge a pending frame or advance acquisition.
    fn advance_startup(
        &mut self,
        now: u64,
    ) {
        let attention = PresentedScene::capture(
            self.configuration(),
            &self.snapshot(),
            self.session.page(),
            self.session.view_id(),
            None,
            now,
        )
        .attention;
        if let Some(startup) = &mut self.startup {
            let phase = startup.state.phase();
            startup.state.advance(startup.profile, now, false, attention);
            if phase != startup.state.phase() {
                self.presentation_epoch = self.presentation_epoch.and_then(|n| n.checked_add(1));
            }
        }
    }

    /// Prepares warning paging and LED state at retained time; sticky `view_changed` also retires same-batch round-trip
    /// continuity. Startup, button feedback and non-Dashboard pages pause warning paging. Epoch overflow
    /// permanently disables continuity.
    pub(crate) fn prepare(
        &mut self,
        view_changed: bool,
    ) {
        // Accepted view actions can return to the starting view in one batch.
        // Preserve their sticky effect as well as transitions between calls.
        // Exhaustion permanently disables continuity.
        let visible = (self.session.page(), self.session.view_id());
        if view_changed || self.visible != visible {
            self.presentation_epoch = self.presentation_epoch.and_then(|epoch| epoch.checked_add(1));
            self.visible = visible;
        }
        self.advance_startup(self.now_ms);
        let snapshot = self.model.snapshot();
        self.warning = if !self.startup_active()
            && self.session.page() == dashboard_core::pages::Page::Dashboard
            && self.session.popup().is_none()
        {
            self.pager.select(
                self.model.configuration().expect("simulation config"),
                &snapshot,
                self.session.view_id(),
                self.now_ms,
                self.blink_on(),
            )
        } else {
            self.pager.pause(self.now_ms);
            None
        };
        self.update_led();
    }

    /// Updates logical RGB duty from last completed scene, backlight, reduced-motion policy and retained time; no
    /// hardware I/O.
    pub(crate) fn update_led(&mut self) {
        let config = self.model.configuration().expect("simulation config").config();
        self.led.update(
            &config.led,
            self.presented.visible(),
            self.session.backlight().compare() != 0,
            self.now_ms,
            config.reduced_motion,
        );
    }

    /// Returns the shared blink phase at saturating elapsed milliseconds since this owner's origin.
    pub(crate) fn blink_on(&self) -> bool {
        dashboard_core::animations::blink_on(self.now_ms.saturating_sub(self.start_ms))
    }

    /// Requests acknowledgement only for the completed visible warning identity, publishes immediate request feedback
    /// and retries handoff. A full queue retains the pending request for retry; this does not directly mutate
    /// acquisition warning state.
    pub(crate) fn request_acknowledgment(&mut self) {
        let result = self.requests.request_presented(self.presented.warning());
        self.session
            .warning_feedback(WarningFeedback::requested(result), self.now_ms);
        self.retry();
    }

    /// Tries to enqueue the same pending warning command without blocking; a full queue leaves ownership pending for
    /// later retry.
    pub(crate) fn retry(&self) {
        if let Some(command) = self.requests.pending() {
            let _ = self.queue.try_send(command);
        }
    }

    /// Consumes a matching published warning-command result and updates control feedback at retained time; unrelated
    /// results do nothing.
    pub(crate) fn observe_result(&mut self) {
        if let Some(feedback) = self.requests.observe(&self.model.snapshot()) {
            self.session.warning_feedback(feedback, self.now_ms);
        }
    }

    /// Returns immutable current scene metadata, with startup warning-LED policy while replay is active; does not mark
    /// it completed.
    pub(crate) fn scene(&self) -> PresentedScene {
        let overlay = dashboard_core::dashboard::Overlay::configured_for_page(
            self.session.page(),
            self.session.overlay(self.configuration()),
            self.warning,
        );
        let scene = PresentedScene::capture(
            self.configuration(),
            &self.snapshot(),
            self.session.page(),
            self.session.view_id(),
            overlay,
            self.now_ms,
        );
        if let Some(startup) = self.startup.as_ref().filter(|s| s.state.active()) {
            scene.startup(startup.profile.warning_led)
        } else {
            scene
        }
    }

    /// Returns one coherent acquisition publication, including sample quality, statistics and warning command result.
    pub(crate) fn snapshot(&self) -> DashboardSnapshot { self.model.snapshot() }
}

#[cfg(test)]
/// Presentation continuity remains permanently unavailable after identity exhaustion.
mod tests {
    use dashboard_core::pages::Page;

    use super::*;
    #[test]
    /// Visibility changes at MAX retire continuity to None and subsequent changes never recycle an old epoch.
    fn exhausted_visibility_epoch_never_recycles_old_continuity() {
        let mut simulation = Simulation::new(
            dashboard_config::validate(dashboard_config::default_config()).unwrap(),
            0,
        );
        simulation.presentation_epoch = Some(u64::MAX);
        simulation.prepare(true);
        assert_eq!(simulation.presentation_epoch, None);
        simulation.visible = (Page::Logs, 1);
        simulation.prepare(false);
        assert_eq!(simulation.presentation_epoch, None);
    }
}

#[cfg(test)]
/// Shared warning handoff ordering, retry identity and episode-sensitive acknowledgement regressions.
mod warning_transport_tests {
    use dashboard_core::status_led::PresentedWarning;

    use super::*;
    #[test]
    /// A full one-slot queue retains command identity; result publication needs no sample and duplicate delivery is
    /// idempotent.
    fn capacity_one_queue_keeps_identity_and_publishes_result_without_samples() {
        let mut raw = dashboard_config::default_config();
        raw.widgets.truncate(1);
        raw.widgets[0].warning.enabled = true;
        raw.widgets[0].warning.trigger = 2.;
        raw.widgets[0].warning.clear = 1.;
        raw.widgets[0].warning.on_ms = 1;
        let mut s = Simulation::new(dashboard_config::validate(raw).unwrap(), 0);
        s.advance_to(0, Some([3., 90., 95., 80., 30., 700., 13.8, 1.]));
        s.advance_to(1, Some([3., 90., 95., 80., 30., 700., 13.8, 1.]));
        let before = s.snapshot();
        let cmd = s
            .requests
            .request_presented(PresentedWarning {
                rules: 1,
                event_serial: before.warning_event_serial(),
                run_id: before.warning_run_id().unwrap(),
            })
            .unwrap();
        s.retry();
        s.retry();
        assert_eq!(s.queue.try_receive().unwrap(), cmd);
        assert!(s.queue.try_receive().is_err());
        s.retry();
        s.advance_to(2, None);
        let after = s.snapshot();
        assert_eq!(after.sequence, before.sequence);
        assert_ne!(after.publication_revision(), before.publication_revision());
        assert_eq!(after.warning_command_result().request_id, cmd.request_id);
        assert_eq!(after.warning_command_result().accepted, 1);
        assert!(s.requests.pending().is_none());
        // Delayed transport duplicate cannot reapply snooze or create a result revision.
        s.queue.try_send(cmd).unwrap();
        s.advance_to(2, None);
        assert_eq!(s.snapshot().publication_revision(), after.publication_revision());
    }
    #[test]
    /// Delayed acknowledgement accepts only the old still-current episode while a new warning episode remains
    /// unsnoozed.
    fn new_episode_command_and_snooze_deadline_publish_coherent_state() {
        use dashboard_core::alarms::RuleIndex;
        let mut raw = dashboard_config::default_config();
        raw.widgets.truncate(2);
        raw.widgets[1] =
            dashboard_config::default_widget(dashboard_config::Channel::Oil, 2, 1, raw.widgets[1].placement);
        raw.controls.screens[0].buttons[0].tap = dashboard_config::ControlAction::AcknowledgeWarnings;
        for w in &mut raw.widgets {
            w.warning.enabled = true;
            w.warning.on_ms = 1;
            w.warning.off_ms = 1;
            w.warning.advanced.snooze_s = 5;
        }
        let mut s = Simulation::new(dashboard_config::validate(raw).unwrap(), 0);
        let values = |oil| [3., oil, 95., 80., 30., 700., 13.8, 1.];
        s.advance_to(0, Some(values(150.)));
        s.advance_to(1, Some(values(150.)));
        let initial = s.snapshot();
        let page = PresentedWarning {
            rules: 3,
            event_serial: initial.warning_event_serial(),
            run_id: initial.warning_run_id().unwrap(),
        };
        let first = s.requests.request_presented(page).unwrap();
        s.retry();
        s.advance_to(1, None);
        assert_eq!(s.snapshot().warning_command_result().accepted, 3);
        for now in [100, 101] {
            s.advance_to(now, Some(values(90.)));
        }
        for now in (250..=4750).step_by(250) {
            s.advance_to(now, Some(values(90.)));
        }
        s.advance_to(5000, Some(values(150.)));
        let command = s.requests.request_presented(page).unwrap();
        assert!(command.request_id > first.request_id);
        s.retry();
        s.advance_to(5001, Some(values(150.)));
        let after = s.snapshot();
        assert_eq!(after.warning_command_result().request_id, command.request_id);
        assert_eq!(
            (
                after.warning_command_result().accepted,
                after.warning_command_result().rejected
            ),
            (1, 2)
        );
        assert!(after.warning_event_serial() > page.event_serial);
        assert!(after.warning_eligibility(RuleIndex::new(0).unwrap(), 5001).snoozed);
        let newcomer = after.warning_eligibility(RuleIndex::new(1).unwrap(), 5001);
        assert!(newcomer.attention());
        assert!(!newcomer.acknowledged);
        assert!(!newcomer.snoozed);
    }
}
