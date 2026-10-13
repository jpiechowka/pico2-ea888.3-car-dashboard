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
        // Compatibility projection of the shared default policy. Runtime adapters
        // retain their own LedController and consume all three channels.
        let duty = LedController::new().update(
            &dashboard_config::LedSettings::DEFAULT,
            PresentedScene::legacy(visible),
            backlight_on,
            0,
            false,
        );
        if duty.red != 0 {
            Self::Red
        } else if duty.green != 0 {
            Self::Green
        } else {
            Self::Off
        }
    }
}

/// Per-channel emitted light, before the board's active-low inversion.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LedDuty {
    pub red: u16,
    pub green: u16,
    pub blue: u16,
}

/// One animation epoch follows completed normal/warning selection, never repaint cadence.
#[derive(Default)]
pub struct LedController {
    epoch: u64,
    last_ms: u64,
    duty: LedDuty,
    warning: bool,
    initialized: bool,
}
impl LedController {
    pub const fn new() -> Self {
        Self {
            epoch: 0,
            last_ms: 0,
            duty: LedDuty {
                red: 0,
                green: 0,
                blue: 0,
            },
            warning: false,
            initialized: false,
        }
    }

    pub const fn duty(&self) -> LedDuty { self.duty }

    pub fn update(
        &mut self,
        settings: &dashboard_config::LedSettings,
        scene: PresentedScene,
        backlight_on: bool,
        now_ms: u64,
        reduced_motion: bool,
    ) -> LedDuty {
        use dashboard_config::LedPatternKind;
        if self.initialized && now_ms < self.last_ms {
            return self.duty;
        }
        let phase = scene.overlay.and_then(Overlay::warning_phase);
        let permitted = match scene.page {
            crate::pages::Page::Dashboard => scene.overlay.is_none() || phase.is_some() || settings.during_feedback,
            crate::pages::Page::Debug => settings.on_debug,
            crate::pages::Page::Logs => settings.on_logs,
        };
        let warning = (scene.eligible || phase.is_some()) && permitted && (backlight_on || settings.backlight_off);
        if !self.initialized || warning != self.warning {
            self.epoch = now_ms;
        }
        self.initialized = true;
        self.warning = warning;
        self.last_ms = now_ms;
        let pattern = if warning { settings.warning } else { settings.normal };
        // Validated callers supply these bounds; clamps also keep this pure public API total.
        let period = u64::from(pattern.period_ms.clamp(200, 10000));
        let elapsed = (now_ms - self.epoch) % period;
        let amplitude = match pattern.kind {
            LedPatternKind::Off => 0,
            LedPatternKind::Solid => 10000,
            LedPatternKind::Breathe if reduced_motion => 10000,
            LedPatternKind::Blink => {
                let on = if warning && settings.sync_warning_blink && backlight_on {
                    phase.unwrap_or(elapsed * 100 < period * u64::from(pattern.on_percent.clamp(10, 90)))
                } else {
                    elapsed * 100 < period * u64::from(pattern.on_percent.clamp(10, 90))
                };
                if on { 10000 } else { 0 }
            }
            LedPatternKind::Breathe => {
                // Triangle 0..1..0 followed by smoothstep x*x*(3-2*x).
                // Fixed scale10000, nearest integer (ties up), bounded u64 intermediates.
                let triangle = elapsed.min(period - elapsed) * 2;
                let x = (triangle * 10000 + period / 2) / period;
                ((x * x * (30000 - 2 * x) + 50000000) / 100000000) as u32
            }
        };
        let color = pattern.color.0;
        let channel = |value: u16, maximum: u32| {
            let denominator = maximum * 100;
            ((u32::from(value) * u32::from(pattern.brightness_percent.min(100)) * amplitude + denominator / 2)
                / denominator) as u16
        };
        self.duty = LedDuty {
            red: channel((color >> 11) & 31, 31),
            green: channel((color >> 5) & 63, 63),
            blue: channel(color & 31, 31),
        };
        self.duty
    }
}
