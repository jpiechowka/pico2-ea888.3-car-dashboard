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

pub(crate) struct Simulation {
    pub(crate) model: OwnedAcquisitionModel,
    pub(crate) session: ControlSession,
    pub(crate) now_ms: u64,
    pager: WarningPager,
    pub(crate) led: LedController,
    pub(crate) presented: PresentedLed,
    pub(crate) requests: WarningRequests,
    queue: Channel<NoopRawMutex, WarningCommand, 1>,
    start_ms: u64,
    pub(crate) render_transport: bool,
    visible: (dashboard_core::pages::Page, u8),
    pub(crate) presentation_epoch: Option<u64>,
    pub(crate) warning: Option<WarningPage>,
}
impl Simulation {
    pub(crate) fn new(
        config: ValidatedConfig,
        now_ms: u64,
    ) -> Self {
        let session = ControlSession::new(&config, FpsMode::Off, false);
        let visible = (session.page(), session.view_id());
        Self {
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

    pub(crate) fn configuration(&self) -> &ValidatedConfig {
        self.model
            .configuration()
            .expect("simulation always has a configuration")
    }

    pub(crate) fn advance_to(
        &mut self,
        now_ms: u64,
        values: Option<[f32; 8]>,
    ) {
        // At most one real sample per call. No replay proportional to elapsed time.
        // Preserve the shared model's rejection semantics for backdated readings.
        if let Some(values) = values {
            self.model.ingest_batch(values, now_ms);
        }
        if let Ok(command) = self.queue.try_receive() {
            self.model.apply_warning_command(command, now_ms);
        }
        self.model.tick(now_ms);
        self.now_ms = self.now_ms.max(now_ms);
        self.observe_result();
        self.session.expire_feedback(self.now_ms);
        self.prepare(false);
    }

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
        let snapshot = self.model.snapshot();
        self.warning =
            if self.session.page() == dashboard_core::pages::Page::Dashboard && self.session.popup().is_none() {
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

    pub(crate) fn blink_on(&self) -> bool {
        dashboard_core::animations::blink_on(self.now_ms.saturating_sub(self.start_ms))
    }

    pub(crate) fn request_acknowledgment(&mut self) {
        let result = self.requests.request_presented(self.presented.warning());
        self.session
            .warning_feedback(WarningFeedback::requested(result), self.now_ms);
        self.retry();
    }

    pub(crate) fn retry(&self) {
        if let Some(command) = self.requests.pending() {
            let _ = self.queue.try_send(command);
        }
    }

    pub(crate) fn observe_result(&mut self) {
        if let Some(feedback) = self.requests.observe(&self.model.snapshot()) {
            self.session.warning_feedback(feedback, self.now_ms);
        }
    }

    pub(crate) fn scene(&self) -> PresentedScene {
        let overlay = dashboard_core::dashboard::Overlay::configured_for_page(
            self.session.page(),
            self.session.overlay(self.configuration()),
            self.warning,
        );
        PresentedScene::capture(
            self.configuration(),
            &self.snapshot(),
            self.session.page(),
            self.session.view_id(),
            overlay,
            self.now_ms,
        )
    }

    pub(crate) fn snapshot(&self) -> DashboardSnapshot { self.model.snapshot() }
}

#[cfg(test)]
mod tests {
    use dashboard_core::pages::Page;

    use super::*;
    #[test]
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
mod warning_transport_tests {
    use dashboard_core::status_led::PresentedWarning;

    use super::*;
    #[test]
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
