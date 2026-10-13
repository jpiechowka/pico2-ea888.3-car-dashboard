//! Single render-owner control state; adapters own hardware and reset acknowledgment.
use dashboard_config::{ButtonId, ControlAction, ControlMap, RotationAction, ValidatedConfig};
use embassy_sync::blocking_mutex::raw::RawMutex;

use crate::backlight::Backlight;
use crate::controls::ControlBatch;
use crate::input_model::EncoderBatch;
use crate::input_trace::Observation;
use crate::logging::LogStore;
use crate::pages::Page;
use crate::popup::Popup;
use crate::render::FpsMode;
use crate::view_state::ViewSelection;

#[derive(Default, Debug)]
pub struct ControlEffects {
    pub page_changed: bool,
    pub view_changed: bool,
    pub units_changed: bool,
    pub fps_changed: bool,
    pub recovered: bool,
    pub reset_requested: bool,
    pub acknowledge_requested: bool,
    pub observations: Option<Observation>,
    pub reset_observation: Option<Observation>,
    pub ignored: u32,
}

fn count(o: Option<Observation>) -> u32 { o.map_or(0, |o| o.events.max(1)) }
fn valid(o: Observation) -> bool { o.events != 0 && o.observed_us <= o.accepted_us }
// Input sources are independently timestamped. Transaction aggregation is
// commutative, unlike the chronological publisher's Observation::merge.
fn aggregate(
    target: &mut Option<Observation>,
    o: Observation,
) -> bool {
    if !valid(o) {
        return false;
    }
    if let Some(old) = target {
        let Some(events) = old.events.checked_add(o.events) else {
            return false;
        };
        old.observed_us = old.observed_us.min(o.observed_us);
        old.accepted_us = old.accepted_us.max(o.accepted_us);
        old.events = events;
    } else {
        *target = Some(o);
    }
    true
}

pub struct ControlSession {
    page: Page,
    views: ViewSelection,
    backlight: Backlight,
    fps: FpsMode,
    boost_psi: bool,
    popup: Option<Popup>,
    log_anchor: Option<u64>,
    last_ms: u64,
    reset_pending: bool,
}
impl ControlSession {
    pub fn new(
        config: &ValidatedConfig,
        fps: FpsMode,
        boost_psi: bool,
    ) -> Self {
        Self {
            page: Page::Dashboard,
            views: ViewSelection::new(config),
            backlight: Backlight::new(),
            fps,
            boost_psi,
            popup: None,
            log_anchor: None,
            last_ms: 0,
            reset_pending: false,
        }
    }

    pub const fn page(&self) -> Page { self.page }

    pub const fn view_selection(&self) -> &ViewSelection { &self.views }

    pub const fn view_id(&self) -> u8 { self.views.id() }

    pub const fn backlight(&self) -> &Backlight { &self.backlight }

    pub fn backlight_mut(&mut self) -> &mut Backlight { &mut self.backlight }

    pub const fn fps(&self) -> FpsMode { self.fps }

    pub const fn boost_psi(&self) -> bool { self.boost_psi }

    pub const fn popup(&self) -> Option<Popup> { self.popup }

    pub const fn log_anchor(&self) -> Option<u64> { self.log_anchor }

    fn feedback(
        &mut self,
        popup: Popup,
    ) {
        self.backlight.interrupt_feedback();
        self.popup = Some(popup);
    }

    fn navigate(
        &mut self,
        page: Page,
    ) {
        self.backlight.interrupt_feedback();
        self.popup = None;
        self.log_anchor = None;
        self.page = page;
    }

    pub fn expire_feedback(
        &mut self,
        now_ms: u64,
    ) {
        if now_ms < self.last_ms {
            return;
        }
        self.last_ms = now_ms;
        if matches!(
            self.popup,
            Some(Popup::Warnings(
                _,
                crate::warning_state::WarningFeedback::Pending | crate::warning_state::WarningFeedback::Busy
            ))
        ) {
            return;
        }
        if let Some(popup) = self.popup
            && self.backlight.expire_popup(popup, now_ms)
        {
            self.popup = None;
        }
    }

    pub fn statistics_acknowledged(
        &mut self,
        now_ms: u64,
    ) {
        if now_ms < self.last_ms || !self.reset_pending {
            return;
        }
        self.last_ms = now_ms;
        self.reset_pending = false;
        if self.page == Page::Dashboard {
            self.feedback(Popup::Reset(now_ms));
        }
    }

    pub fn warning_feedback(
        &mut self,
        feedback: crate::warning_state::WarningFeedback,
        now: u64,
    ) {
        if now < self.last_ms {
            return;
        }
        self.last_ms = now;
        if self.page == Page::Dashboard {
            self.feedback(Popup::Warnings(now, feedback));
        }
    }

    pub fn overlay(
        &self,
        config: &ValidatedConfig,
    ) -> Option<crate::dashboard::Overlay> {
        use crate::dashboard::Overlay;
        self.popup.map(|popup| {
            match popup {
                Popup::Reset(_) => Overlay::Reset,
                Popup::Warnings(_, feedback) => {
                    Overlay::View(crate::view_state::PopupText::new(feedback.label()).unwrap())
                }
                Popup::Fps(_) => Overlay::Fps(self.fps),
                Popup::BoostUnit(_) => Overlay::BoostUnit(self.boost_psi),
                Popup::Brightness(_, b) => Overlay::Brightness(b),
                Popup::View(_) => {
                    Overlay::View(
                        crate::view_state::PopupText::new(
                            &config
                                .config()
                                .views
                                .iter()
                                .find(|v| v.id == self.view_id())
                                .expect("validated selected view")
                                .name,
                        )
                        .unwrap(),
                    )
                }
            }
        })
    }

    fn action<M: RawMutex>(
        &mut self,
        config: &ValidatedConfig,
        action: ControlAction,
        logs: &LogStore<M>,
        now: u64,
        effects: &mut ControlEffects,
    ) -> bool {
        if !action.allowed_on(self.page) {
            return false;
        }
        if let Some(destination) = action.destination(self.page) {
            if destination == self.page {
                return false;
            }
            self.navigate(destination);
            effects.page_changed = true;
            return true;
        }
        match action {
            ControlAction::AcknowledgeWarnings => effects.acknowledge_requested = true,
            ControlAction::NextView | ControlAction::PreviousView => {
                if !self.views.rotate(
                    config,
                    self.page,
                    if action == ControlAction::NextView { 1 } else { -1 },
                ) {
                    return false;
                }
                effects.view_changed = true;
                self.feedback(Popup::View(now));
            }
            ControlAction::ToggleBoostUnits => {
                self.boost_psi = !self.boost_psi;
                effects.units_changed = true;
                self.feedback(Popup::BoostUnit(now));
            }
            ControlAction::CycleFps => {
                self.fps = self.fps.next();
                effects.fps_changed = true;
                self.feedback(Popup::Fps(now));
            }
            ControlAction::ResetStatistics => {
                self.reset_pending = true;
                effects.reset_requested = true;
            }
            ControlAction::CycleBrightness
            | ControlAction::ToggleBacklight
            | ControlAction::BrightnessUp
            | ControlAction::BrightnessDown => {
                let (page, delta, presses) = match action {
                    ControlAction::CycleBrightness => (Page::Dashboard, 0, 1),
                    ControlAction::ToggleBacklight => (Page::Debug, 0, 1),
                    ControlAction::BrightnessUp => (Page::Debug, 1, 0),
                    _ => (Page::Debug, -1, 0),
                };
                let effect = self.backlight.encoder(page, delta, presses, now);
                if !(effect.motion_applied || effect.press_applied) {
                    return false;
                }
                self.popup = effect.popup;
            }
            ControlAction::ScrollOlder | ControlAction::ScrollNewer => {
                let old = self.log_anchor;
                if !logs.scroll(
                    &mut self.log_anchor,
                    if action == ControlAction::ScrollOlder { -1 } else { 1 },
                ) || self.log_anchor == old
                {
                    return false;
                }
            }
            ControlAction::LogsLive => {
                if self.log_anchor.take().is_none() {
                    return false;
                }
            }
            _ => return false,
        }
        true
    }

    /// All mappings and precedence use the page captured at transaction entry.
    #[allow(clippy::too_many_arguments)]
    pub fn apply<M: RawMutex>(
        &mut self,
        config: &ValidatedConfig,
        map: &ControlMap,
        buttons: ControlBatch,
        encoder: EncoderBatch,
        logs: &LogStore<M>,
        now_ms: u64,
    ) -> ControlEffects {
        let mut result = ControlEffects {
            ignored: buttons.discarded,
            ..Default::default()
        };
        let motion_count = count(encoder.motion).max(u32::from(encoder.delta != 0));
        let click_count = count(encoder.button).max(u32::from(encoder.presses));
        let button_count = buttons.actions.iter().fold(count(buttons.recovery), |n, e| {
            n.saturating_add(e.observation.events.max(1))
        });
        let total = button_count.saturating_add(motion_count).saturating_add(click_count);
        if now_ms < self.last_ms {
            result.ignored = result.ignored.saturating_add(total);
            return result;
        }
        let Some(_) = now_ms.checked_mul(1000) else {
            result.ignored = result.ignored.saturating_add(total);
            return result;
        };
        // The caller supplies a millisecond bucket, while producers retain
        // microsecond precision. All instants within this bucket are eligible.
        let valid_before = |o: Observation| valid(o) && o.accepted_us / 1000 <= now_ms;
        self.last_ms = now_ms;
        if let Some(o) = buttons.recovery
            && valid_before(o)
        {
            result.page_changed = self.page != Page::Dashboard;
            self.navigate(Page::Dashboard);
            self.backlight.wake_full();
            result.recovered = true;
            result.observations = Some(o);
            let discarded = buttons
                .actions
                .iter()
                .fold(0u32, |n, e| n.saturating_add(e.observation.events.max(1)));
            result.ignored = result
                .ignored
                .saturating_add(discarded)
                .saturating_add(motion_count)
                .saturating_add(click_count);
            return result;
        }
        let page = self.page;
        let click = map.screens[page.index()].encoder_click;
        let click_valid = encoder.presses > 0
            && encoder.presses <= 8
            && encoder
                .button
                .is_some_and(|o| valid_before(o) && o.events == u32::from(encoder.presses))
            && click.allowed_on(page);
        // Button index order is explicit even for manually constructed batches.
        let transition = ButtonId::ALL.iter().find_map(|id| {
            buttons.actions.iter().find(|e| {
                e.button == *id && valid_before(e.observation) && e.action.destination(page).is_some_and(|p| p != page)
            })
        });
        if let Some(event) = transition {
            self.navigate(event.action.destination(page).unwrap());
            result.page_changed = true;
            result.observations = Some(event.observation);
            let discarded = buttons
                .actions
                .iter()
                .filter(|e| !core::ptr::eq(*e, event))
                .fold(count(buttons.recovery), |n, e| {
                    n.saturating_add(e.observation.events.max(1))
                });
            result.ignored = result
                .ignored
                .saturating_add(discarded)
                .saturating_add(motion_count)
                .saturating_add(click_count);
            return result;
        }
        if click_valid && let Some(destination) = click.destination(page).filter(|p| *p != page) {
            self.navigate(destination);
            result.page_changed = true;
            let mut o = encoder.button.unwrap();
            o.events = 1;
            result.observations = Some(o);
            result.ignored = result
                .ignored
                .saturating_add(button_count)
                .saturating_add(motion_count)
                .saturating_add(click_count.saturating_sub(1));
            return result;
        }
        result.ignored = result.ignored.saturating_add(count(buttons.recovery));
        for id in ButtonId::ALL {
            for e in buttons.actions.iter().filter(|e| e.button == id) {
                let target = if e.action == ControlAction::ResetStatistics {
                    &mut result.reset_observation
                } else {
                    &mut result.observations
                };
                let mut candidate = *target;
                if valid_before(e.observation)
                    && aggregate(&mut candidate, e.observation)
                    && self.action(config, e.action, logs, now_ms, &mut result)
                {
                    if e.action == ControlAction::ResetStatistics {
                        result.reset_observation = candidate;
                    } else {
                        result.observations = candidate;
                    }
                } else {
                    result.ignored = result.ignored.saturating_add(e.observation.events.max(1));
                }
            }
        }
        let rotation = map.screens[page.index()].encoder_rotation;
        let mut candidate = result.observations;
        let motion_valid = encoder.delta != 0
            && (-128..=128).contains(&encoder.delta)
            && rotation.allowed_on(page)
            && encoder
                .motion
                .is_some_and(|o| valid_before(o) && aggregate(&mut candidate, o));
        let mut changed = false;
        if motion_valid {
            changed = match rotation {
                RotationAction::Views => {
                    let changed = self.views.rotate(config, page, encoder.delta);
                    if changed {
                        result.view_changed = true;
                        self.feedback(Popup::View(now_ms));
                    }
                    changed
                }
                RotationAction::Brightness => {
                    let e = self.backlight.encoder(Page::Debug, encoder.delta, 0, now_ms);
                    if e.motion_applied {
                        self.popup = e.popup;
                    }
                    e.motion_applied
                }
                RotationAction::Logs => {
                    let old = self.log_anchor;
                    logs.scroll(&mut self.log_anchor, encoder.delta) && old != self.log_anchor
                }
                RotationAction::None => false,
            };
        }
        if changed {
            result.observations = candidate;
        } else {
            result.ignored = result.ignored.saturating_add(motion_count);
        }
        if click_valid {
            let mut o = encoder.button.unwrap();
            o.events = 1;
            for _ in 0..encoder.presses {
                let mut candidate = if click == ControlAction::ResetStatistics {
                    result.reset_observation
                } else {
                    result.observations
                };
                if aggregate(&mut candidate, o) && self.action(config, click, logs, now_ms, &mut result) {
                    if click == ControlAction::ResetStatistics {
                        result.reset_observation = candidate;
                    } else {
                        result.observations = candidate;
                    }
                } else {
                    result.ignored = result.ignored.saturating_add(1);
                }
            }
        } else {
            result.ignored = result.ignored.saturating_add(click_count);
        }
        result
    }
}
