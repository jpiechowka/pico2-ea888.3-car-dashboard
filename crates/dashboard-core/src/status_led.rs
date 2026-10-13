//! Active colors, independent of the display board's active-low GPIO wiring.
use crate::dashboard::Overlay;
use crate::display::pipeline::FrameToken;

/// Immutable identity of the warning page whose pixels completed transmission.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PresentedWarning {
    pub rules: u64,
    pub event_serial: u64,
    pub run_id: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PresentedScene {
    pub overlay: Option<Overlay>,
    pub warning: PresentedWarning,
    pub page: crate::pages::Page,
    pub eligible: bool,
    pub attention: bool,
}
impl PresentedScene {
    pub const EMPTY: Self = Self {
        overlay: None,
        warning: PresentedWarning {
            rules: 0,
            event_serial: 0,
            run_id: 0,
        },
        page: crate::pages::Page::Dashboard,
        eligible: false,
        attention: false,
    };

    pub const fn legacy(overlay: Option<Overlay>) -> Self { Self { overlay, ..Self::EMPTY } }

    pub fn capture(
        config: &dashboard_config::ValidatedConfig,
        snapshot: &crate::acquisition::DashboardSnapshot,
        page: crate::pages::Page,
        view: u8,
        overlay: Option<Overlay>,
        now: u64,
    ) -> Self {
        let mut state = Self {
            page,
            overlay,
            ..Self::EMPTY
        };
        for (i, widget) in config.config().widgets.iter().enumerate() {
            if config.config().warnings.scope == dashboard_config::WarningScope::CurrentView && widget.view != view {
                continue;
            }
            let e = snapshot.warning_eligibility(crate::alarms::RuleIndex::new(i).unwrap(), now);
            state.eligible |= e.visible();
            state.attention |= e.attention();
        }
        if let Some(Overlay::Warnings(warning)) = overlay {
            state.warning = PresentedWarning {
                rules: warning.rules,
                event_serial: snapshot.warning_event_serial(),
                run_id: snapshot.warning_run_id().unwrap_or(0),
            };
        }
        state
    }
}
impl From<Option<Overlay>> for PresentedScene {
    fn from(overlay: Option<Overlay>) -> Self { Self::legacy(overlay) }
}
impl Default for PresentedScene {
    fn default() -> Self { Self::EMPTY }
}

/// Track the one in-flight request separately from the completed scene while
/// rendering the other buffer. No shared GPIO owner or extra task is required.
#[derive(Default)]
pub struct PresentedLed {
    pending: Option<(FrameToken, PresentedScene)>,
    visible: PresentedScene,
    last_submitted: u64,
}

impl PresentedLed {
    pub const fn new() -> Self {
        Self {
            pending: None,
            visible: PresentedScene::EMPTY,
            last_submitted: 0,
        }
    }

    pub fn submit(
        &mut self,
        token: FrameToken,
        state: impl Into<PresentedScene>,
    ) -> bool {
        if self.pending.is_some() || token.sequence <= self.last_submitted {
            return false;
        }
        self.last_submitted = token.sequence;
        self.pending = Some((token, state.into()));
        true
    }

    pub fn complete(
        &mut self,
        token: FrameToken,
    ) -> bool {
        if let Some((expected, overlay)) = self.pending
            && expected == token
        {
            self.visible = overlay;
            self.pending = None;
            true
        } else {
            false
        }
    }

    pub fn unchanged(
        &mut self,
        state: impl Into<PresentedScene>,
    ) -> bool {
        if self.pending.is_some() {
            return false;
        }
        let state = state.into();
        if self.visible != state {
            return false;
        }
        true
    }

    pub const fn warning(&self) -> PresentedWarning { self.visible.warning }

    pub const fn visible(&self) -> PresentedScene { self.visible }

    pub fn equivalent(
        &self,
        state: PresentedScene,
    ) -> bool {
        self.pending.is_none() && self.visible == state
    }

    pub fn output(
        &self,
        backlight_on: bool,
    ) -> StatusLed {
        StatusLed::for_overlay(self.visible.overlay, backlight_on)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StatusLed {
    Off,
    Green,
    Red,
}

impl StatusLed {
    pub fn for_overlay(
        visible: Option<Overlay>,
        backlight_on: bool,
    ) -> Self {
        if backlight_on && let Some(phase) = visible.and_then(Overlay::warning_phase) {
            if phase { Self::Red } else { Self::Off }
        } else {
            Self::Green
        }
    }
}
