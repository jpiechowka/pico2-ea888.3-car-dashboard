//! Browser runtime with one immutable configuration and one acquisition lifetime.
use dashboard_config::ValidatedConfig;
use dashboard_core::acquisition::{DashboardSnapshot, OwnedAcquisitionModel};
use dashboard_core::control_session::ControlSession;
use dashboard_core::render::FpsMode;
use dashboard_core::view_state::{WarningPage, WarningPager};

pub(crate) struct Simulation {
    pub(crate) model: OwnedAcquisitionModel,
    pub(crate) session: ControlSession,
    pub(crate) now_ms: u64,
    pager: WarningPager,
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
        } else {
            self.model.tick(now_ms);
        }
        self.now_ms = self.now_ms.max(now_ms);
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
        self.warning = self.pager.select(
            self.model.configuration().expect("simulation config"),
            &snapshot,
            self.session.view_id(),
            self.now_ms,
            (self.now_ms / 500).is_multiple_of(2),
        );
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
