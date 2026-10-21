//! Completed-scene warning evidence and configurable RGB duty, independent of
//! the display board's active-low GPIO wiring.
use crate::dashboard::Overlay;
use crate::display::pipeline::FrameToken;

/// Immutable identity of the warning page whose pixels completed transmission.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PresentedWarning {
    /// Widget/rule mask whose warning text was included in the frame; zero means none.
    pub rules: u64,
    /// Producer event frontier captured with those pixels, not the latest live frontier.
    pub event_serial: u64,
    /// Warning producer run identity; zero means unavailable/unbound.
    pub run_id: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// Immutable LED/acknowledgment metadata captured for one rendered scene.
/// Only matching transfer completion promotes it to the visible scene; completion is not scanout proof.
pub struct PresentedScene {
    /// Overlay encoded in the frame, including any frozen warning blink phase.
    pub overlay: Option<Overlay>,
    /// Acknowledgment evidence for warning text present in the frame.
    pub warning: PresentedWarning,
    /// Screen whose background-warning policy applies.
    pub page: crate::pages::Page,
    /// Any scoped visible warning, including snoozed or latched episodes.
    pub eligible: bool,
    /// Any scoped warning needing unsnoozed attention; distinct from LED eligibility.
    pub attention: bool,
}
impl PresentedScene {
    /// Dashboard scene without overlay, acknowledgment evidence or warning eligibility.
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

    /// Returns Dashboard compatibility metadata for `overlay`, without configured eligibility/evidence.
    pub const fn legacy(overlay: Option<Overlay>) -> Self { Self { overlay, ..Self::EMPTY } }

    /// Startup may indicate current warnings but never presents their text.
    /// Returns a scene stripped of overlay/acknowledgment evidence; `warning_led`
    /// gates preservation of the input scene's eligibility and attention flags.
    pub const fn startup(
        self,
        warning_led: bool,
    ) -> Self {
        Self {
            eligible: warning_led && self.eligible,
            attention: warning_led && self.attention,
            ..Self::EMPTY
        }
    }

    /// Returns metadata for validated `config`, coherent `snapshot`, screen `page`,
    /// stable view ID `view`, rendered `overlay` and freshness clock `now` in ms.
    /// Scope filters eligibility by view; only a configured Warnings overlay carries
    /// acknowledgment bits. Caller must match snapshot/config/overlay to the actual
    /// rendered frame; this function does not verify pixels or complete a transfer.
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
    /// Returns legacy Dashboard metadata for `overlay` without acknowledgment evidence.
    fn from(overlay: Option<Overlay>) -> Self { Self::legacy(overlay) }
}
impl Default for PresentedScene {
    /// Returns [`Self::EMPTY`].
    fn default() -> Self { Self::EMPTY }
}

/// Track the one in-flight request separately from the completed scene while
/// rendering the other buffer. No shared GPIO owner or extra task is required.
#[derive(Default)]
pub struct PresentedLed {
    /// Sole submitted frame token and immutable metadata awaiting completion.
    pending: Option<(FrameToken, PresentedScene)>,
    /// Metadata of the last exactly matched completed transfer.
    visible: PresentedScene,
    /// Highest admitted sequence, retained even when a pending source is retired.
    last_submitted: u64,
}

impl PresentedLed {
    /// Returns empty completed metadata with no in-flight frame and sequence high-water zero.
    pub const fn new() -> Self {
        Self {
            pending: None,
            visible: PresentedScene::EMPTY,
            last_submitted: 0,
        }
    }

    /// Returns `true` after retaining `state` for `token` if idle and its sequence
    /// exceeds all prior submissions. Returns `false` without mutation otherwise;
    /// completed metadata remains unchanged until exact-token completion.
    pub fn submit(
        &mut self,
        token: FrameToken,
        state: impl Into<PresentedScene>,
    ) -> bool {
        if !self.can_submit(token) {
            return false;
        }
        self.last_submitted = token.sequence;
        self.pending = Some((token, state.into()));
        true
    }

    /// A read-only transport gate lets browser rendering preserve its last
    /// submitted buffers when submission is already known to be impossible.
    /// Returns whether `token` has a strictly newer sequence and no frame is pending.
    pub const fn can_submit(
        &self,
        token: FrameToken,
    ) -> bool {
        self.pending.is_none() && token.sequence > self.last_submitted
    }

    /// Returns `true` and promotes pending metadata only when `token` matches both
    /// buffer and sequence. Stale, duplicate or mismatched completions return `false`
    /// without mutation. Caller must have already established transport completion.
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

    /// Returns whether `state` exactly matches completed metadata while no frame
    /// is pending. Performs no promotion; caller separately proves unchanged pixels.
    pub fn unchanged(
        &mut self,
        state: impl Into<PresentedScene>,
    ) -> bool {
        if self.pending.is_some() {
            return false;
        }
        let state = state.into();
        self.visible == state
    }

    /// An explicit synthetic source replacement retires delivery, not visible metadata.
    /// Sequence high-water is preserved so the retired token cannot be resubmitted.
    pub fn retire_pending(&mut self) { self.pending = None; }

    /// Returns warning acknowledgment evidence from the last completed scene only.
    pub const fn warning(&self) -> PresentedWarning { self.visible.warning }

    /// Returns complete metadata from the last matching transfer completion.
    pub const fn visible(&self) -> PresentedScene { self.visible }

    /// Returns whether `state` matches completed metadata with no pending transfer.
    pub fn equivalent(
        &self,
        state: PresentedScene,
    ) -> bool {
        self.pending.is_none() && self.visible == state
    }

    /// Returns the legacy default-policy red/green/off projection of completed
    /// overlay metadata; `backlight_on` gates default visible-warning indication.
    pub fn output(
        &self,
        backlight_on: bool,
    ) -> StatusLed {
        StatusLed::for_overlay(self.visible.overlay, backlight_on)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// Legacy single-color projection; configurable RGB output uses [`LedDuty`].
pub enum StatusLed {
    /// No emitted light, including the off phase of a visible warning blink.
    Off,
    /// Normal policy indication; not vehicle-health certification.
    Green,
    /// On phase of an indicated warning.
    Red,
}

impl StatusLed {
    /// Returns default-policy color for completed overlay `visible` and actual
    /// `backlight_on`. Uses a fresh epoch at zero; configurable RGB animations
    /// require a persistent [`LedController`] instead.
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
    /// Red emitted-light duty in `0..=10000`, before electrical polarity inversion.
    pub red: u16,
    /// Green emitted-light duty in `0..=10000`, before electrical polarity inversion.
    pub green: u16,
    /// Blue emitted-light duty in `0..=10000`, before electrical polarity inversion.
    pub blue: u16,
}

/// One animation epoch follows completed normal/warning selection, never repaint cadence.
#[derive(Default)]
pub struct LedController {
    /// Millisecond start of the current normal/warning selection's animation.
    epoch: u64,
    /// Last accepted update clock; backward clocks cannot change output or selection.
    last_ms: u64,
    /// Most recently computed emitted-light duty.
    duty: LedDuty,
    /// Whether the warning pattern is selected after screen/backlight policy gates.
    warning: bool,
    /// Whether the first update established an epoch and monotonic clock guard.
    initialized: bool,
}
impl LedController {
    /// Returns an uninitialized controller with zero emitted light until its first update.
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

    /// Returns the last computed duty without advancing animation time.
    pub const fn duty(&self) -> LedDuty { self.duty }

    /// Returns and stores emitted-light duty using `settings`, completed `scene`,
    /// actual `backlight_on`, monotonic `now_ms` and `reduced_motion`.
    /// Backward time returns the previous duty without mutation. Normal/warning
    /// selection changes restart the epoch; repaint or settings changes alone do not.
    /// Snoozed eligibility still selects warning policy. A committed overlay phase
    /// controls synchronized warning blink when lit; reduced motion makes breathing
    /// solid but preserves explicit blinking. Invalid raw periods/duties/brightness
    /// clamp to `200..=10000` ms, `10..=90` percent and at most 100 percent respectively.
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
