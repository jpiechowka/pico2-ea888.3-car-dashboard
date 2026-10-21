//! Compact physical control bindings and their independent validation.
use serde::{Deserialize, Serialize};

/// Stable three-screen control-map order; dashboard view selection is a separate state.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ControlPage {
    #[default]
    /// Configured widget views and warning presentation.
    Dashboard,
    /// Diagnostic measurements and brightness controls.
    Debug,
    /// Bounded runtime event history.
    Logs,
}
impl ControlPage {
    /// Complete order used by binding arrays and cyclic navigation.
    pub const ALL: [Self; 3] = [Self::Dashboard, Self::Debug, Self::Logs];

    /// Return the fixed screen name used by controls/editor diagnostics.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Dashboard => "Dashboard",
            Self::Debug => "Debug",
            Self::Logs => "Logs",
        }
    }

    /// Return the index in the three-element screen-binding array.
    pub const fn index(self) -> usize { self as usize }

    /// Return the next screen, wrapping Logs to Dashboard.
    pub const fn next(self) -> Self {
        match self {
            Self::Dashboard => Self::Debug,
            Self::Debug => Self::Logs,
            Self::Logs => Self::Dashboard,
        }
    }

    /// Return the previous screen, wrapping Dashboard to Logs.
    pub const fn previous(self) -> Self {
        match self {
            Self::Dashboard => Self::Logs,
            Self::Debug => Self::Dashboard,
            Self::Logs => Self::Debug,
        }
    }

    /// Return the next screen; retained compatibility name for the three-screen cycle.
    pub const fn toggle(self) -> Self { self.next() }
}

/// Physical PIM715 button identity, preserved across screen mounting changes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ButtonId {
    /// Board A button; index zero.
    A,
    /// Board B button; index one.
    B,
    /// Board X button; index two.
    X,
    /// Board Y button; its two-second recovery hold is not remappable.
    Y,
}
impl ButtonId {
    /// Stable physical-button array order.
    pub const ALL: [Self; 4] = [Self::A, Self::B, Self::X, Self::Y];

    /// Return the printed physical button label.
    pub const fn label(self) -> &'static str {
        match self {
            Self::A => "A",
            Self::B => "B",
            Self::X => "X",
            Self::Y => "Y",
        }
    }

    /// Return the zero-based index in four-element button-binding arrays.
    pub const fn index(self) -> usize { self as usize }
}

/// Closed semantic input actions; screen compatibility is validated independently of gesture timing.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ControlAction {
    /// No remappable action; reserved Y recovery still operates.
    None,
    /// Move forward through Dashboard, Debug and Logs.
    NextScreen,
    /// Move backward through the screen cycle.
    PreviousScreen,
    /// Select Dashboard while retaining its selected view.
    Dashboard,
    /// Select Debug.
    Debug,
    /// Select Logs.
    Logs,
    /// Select the next configured Dashboard view with wrapping.
    NextView,
    /// Select the previous configured Dashboard view with wrapping.
    PreviousView,
    /// Toggle the physical-following boost bar/PSI mode; fixed unit choices stay fixed.
    ToggleBoostUnits,
    /// Cycle Dashboard FPS header presentation.
    CycleFps,
    /// Request statistics/history reset through runtime acknowledgment handling.
    ResetStatistics,
    /// Cycle Dashboard Full, Dim and Off, respecting completed shutdown feedback/clear frames.
    CycleBrightness,
    /// Toggle Debug backlight state.
    ToggleBacklight,
    /// Increase Debug brightness by the runtime step.
    BrightnessUp,
    /// Decrease Debug brightness by the runtime step.
    BrightnessDown,
    /// Move toward older Logs entries.
    ScrollOlder,
    /// Move toward newer Logs entries.
    ScrollNewer,
    /// Resume live Logs following.
    LogsLive,
    /// Acknowledge/snooze eligible warnings according to their configured runtime policy.
    AcknowledgeWarnings,
}
impl ControlAction {
    /// Stable explicit catalogue; never derive serialization from a menu index.
    pub const ALL: [Self; 19] = [
        Self::None,
        Self::NextScreen,
        Self::PreviousScreen,
        Self::Dashboard,
        Self::Debug,
        Self::Logs,
        Self::NextView,
        Self::PreviousView,
        Self::ToggleBoostUnits,
        Self::CycleFps,
        Self::ResetStatistics,
        Self::CycleBrightness,
        Self::ToggleBacklight,
        Self::BrightnessUp,
        Self::BrightnessDown,
        Self::ScrollOlder,
        Self::ScrollNewer,
        Self::LogsLive,
        Self::AcknowledgeWarnings,
    ];

    /// Return the fixed action-picker label.
    pub const fn label(self) -> &'static str {
        match self {
            Self::None => "No action",
            Self::NextScreen => "Next screen",
            Self::PreviousScreen => "Previous screen",
            Self::Dashboard => "Go to Dashboard",
            Self::Debug => "Go to Debug",
            Self::Logs => "Go to Logs",
            Self::NextView => "Next view",
            Self::PreviousView => "Previous view",
            Self::ToggleBoostUnits => "Toggle bar / PSI",
            Self::CycleFps => "Cycle FPS display",
            Self::ResetStatistics => "Reset statistics",
            Self::CycleBrightness => "Cycle brightness",
            Self::ToggleBacklight => "Toggle backlight",
            Self::BrightnessUp => "Brightness up",
            Self::BrightnessDown => "Brightness down",
            Self::ScrollOlder => "Older logs",
            Self::ScrollNewer => "Newer logs",
            Self::LogsLive => "Return to live logs",
            Self::AcknowledgeWarnings => "Acknowledge warnings",
        }
    }

    /// Return whether this action is compatible with `page`; does not check reachability.
    pub const fn allowed_on(
        self,
        page: ControlPage,
    ) -> bool {
        match self {
            Self::None | Self::NextScreen | Self::PreviousScreen | Self::Dashboard | Self::Debug | Self::Logs => true,
            Self::NextView
            | Self::PreviousView
            | Self::ToggleBoostUnits
            | Self::CycleFps
            | Self::AcknowledgeWarnings
            | Self::ResetStatistics
            | Self::CycleBrightness => matches!(page, ControlPage::Dashboard),
            Self::ToggleBacklight | Self::BrightnessUp | Self::BrightnessDown => matches!(page, ControlPage::Debug),
            Self::ScrollOlder | Self::ScrollNewer | Self::LogsLive => matches!(page, ControlPage::Logs),
        }
    }

    /// Return the screen reached from `page` by a navigation action, else `None`.
    /// Pure mapping only; execution and recovery brightness policy are separate.
    pub const fn destination(
        self,
        page: ControlPage,
    ) -> Option<ControlPage> {
        match self {
            Self::NextScreen => Some(page.next()),
            Self::PreviousScreen => Some(page.previous()),
            Self::Dashboard => Some(ControlPage::Dashboard),
            Self::Debug => Some(ControlPage::Debug),
            Self::Logs => Some(ControlPage::Logs),
            _ => None,
        }
    }
}

/// Meaning of encoder rotation; direction/detent filtering is handled by runtime input adapters.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum RotationAction {
    /// Ignore encoder rotation on this screen.
    None,
    /// Select Dashboard views.
    Views,
    /// Adjust Debug brightness.
    Brightness,
    /// Scroll Logs.
    Logs,
}
impl RotationAction {
    /// Complete stable rotation-action picker order.
    pub const ALL: [Self; 4] = [Self::None, Self::Views, Self::Brightness, Self::Logs];

    /// Return the fixed human-readable rotation-action label.
    pub const fn label(self) -> &'static str {
        match self {
            Self::None => "No action",
            Self::Views => "Select views",
            Self::Brightness => "Adjust brightness",
            Self::Logs => "Scroll logs",
        }
    }

    /// Return whether this rotation action is compatible with `page`.
    pub const fn allowed_on(
        self,
        page: ControlPage,
    ) -> bool {
        match self {
            Self::None => true,
            Self::Views => matches!(page, ControlPage::Dashboard),
            Self::Brightness => matches!(page, ControlPage::Debug),
            Self::Logs => matches!(page, ControlPage::Logs),
        }
    }
}

/// Tap/hold actions for one physical button; runtime suppresses tap after a fired hold.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ButtonBinding {
    /// Short-press action.
    pub tap: ControlAction,
    /// Remappable hold action; Y must remain `None` for reserved recovery.
    pub hold: ControlAction,
}
/// Binding set for one screen, independent of panel mounting.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScreenBindings {
    /// A/B/X/Y physical button order.
    pub buttons: [ButtonBinding; 4],
    /// Encoder detent meaning.
    pub encoder_rotation: RotationAction,
    /// Encoder click action; cannot be required for onboard-only navigation.
    pub encoder_click: ControlAction,
}
/// Complete input map; admission ensures every screen and view navigation remain reachable onboard.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ControlMap {
    /// Remappable hold threshold in milliseconds, 400..=1,500; Y recovery stays two seconds.
    pub hold_ms: u16,
    /// Dashboard/Debug/Logs binding order.
    pub screens: [ScreenBindings; 3],
}
impl Default for ControlMap {
    /// Return 800 ms holds and the complete default onboard/encoder map with reserved Y recovery.
    fn default() -> Self {
        use ControlAction::*;
        /// Return a physical binding from `tap` and `hold` without compatibility validation.
        const fn binding(
            tap: ControlAction,
            hold: ControlAction,
        ) -> ButtonBinding {
            ButtonBinding { tap, hold }
        }
        Self {
            hold_ms: 800,
            screens: [
                ScreenBindings {
                    buttons: [
                        binding(ToggleBoostUnits, PreviousView),
                        binding(ResetStatistics, NextView),
                        binding(CycleFps, CycleBrightness),
                        binding(NextScreen, None),
                    ],
                    encoder_rotation: RotationAction::Views,
                    encoder_click: CycleBrightness,
                },
                ScreenBindings {
                    buttons: [
                        binding(None, BrightnessDown),
                        binding(None, BrightnessUp),
                        binding(None, ToggleBacklight),
                        binding(NextScreen, None),
                    ],
                    encoder_rotation: RotationAction::Brightness,
                    encoder_click: ToggleBacklight,
                },
                ScreenBindings {
                    buttons: [
                        binding(None, ScrollOlder),
                        binding(None, ScrollNewer),
                        binding(None, LogsLive),
                        binding(NextScreen, None),
                    ],
                    encoder_rotation: RotationAction::Logs,
                    encoder_click: None,
                },
            ],
        }
    }
}

/// Specific control-map admission failure with enough coordinates for editor repair.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ControlError {
    /// Hold threshold lies outside 400..=1,500 ms.
    HoldDuration,
    /// Attempt to remap the reserved Y hold.
    ReservedRecovery {
        /// Screen carrying the prohibited mapping.
        page: ControlPage,
    },
    /// Physical button action is incompatible with its screen.
    IncompatibleButton {
        /// Screen containing the binding.
        page: ControlPage,
        /// Physical board button.
        button: ButtonId,
        /// `true` selects hold; `false` selects tap.
        hold: bool,
    },
    /// Encoder click action is incompatible with its screen.
    IncompatibleEncoderClick {
        /// Screen containing the click binding.
        page: ControlPage,
    },
    /// Encoder rotation action is incompatible with its screen.
    IncompatibleEncoderRotation {
        /// Screen containing the rotation binding.
        page: ControlPage,
    },
    /// No onboard-button path reaches a screen from Dashboard.
    UnreachableScreen {
        /// Unreachable destination; encoder routes do not satisfy onboard reachability.
        page: ControlPage,
    },
    /// Dashboard has no onboard tap/hold selecting next or previous view.
    MissingViewNavigation,
}

/// Return success for valid `map`, else the first timing/binding/reachability failure.
/// No state changes or input events occur; ordering matches the internal multi-error visitor.
pub fn validate_controls(map: &ControlMap) -> Result<(), ControlError> {
    let mut first = None;
    let _ = visit_controls(map, |error| {
        first = Some(error);
        core::ops::ControlFlow::Break(())
    });
    first.map_or(Ok(()), Err)
}

/// Traverse `map`, reporting independent errors through `emit` until it requests break.
/// Returns `Continue(())` after all checks. Reachability uses onboard buttons plus
/// immutable recovery edges, deliberately excluding the optional encoder. Three bounded
/// graph passes cover all screens; view navigation must be assigned on Dashboard itself.
pub(crate) fn visit_controls(
    map: &ControlMap,
    mut emit: impl FnMut(ControlError) -> core::ops::ControlFlow<()>,
) -> core::ops::ControlFlow<()> {
    if !(400..=1500).contains(&map.hold_ms) {
        emit(ControlError::HoldDuration)?;
    }
    for page in ControlPage::ALL {
        let screen = &map.screens[page.index()];
        if screen.buttons[ButtonId::Y.index()].hold != ControlAction::None {
            emit(ControlError::ReservedRecovery { page })?;
        }
        for button in ButtonId::ALL {
            let binding = screen.buttons[button.index()];
            for (action, hold) in [(binding.tap, false), (binding.hold, true)] {
                if !action.allowed_on(page) {
                    emit(ControlError::IncompatibleButton { page, button, hold })?;
                }
            }
        }
        if !screen.encoder_click.allowed_on(page) {
            emit(ControlError::IncompatibleEncoderClick { page })?;
        }
        if !screen.encoder_rotation.allowed_on(page) {
            emit(ControlError::IncompatibleEncoderRotation { page })?;
        }
    }
    // Every page has an immutable Y recovery edge back to Dashboard.
    let mut edges = [1u8 << ControlPage::Dashboard.index(); 3];
    for page in ControlPage::ALL {
        for binding in map.screens[page.index()].buttons {
            for action in [binding.tap, binding.hold] {
                if let Some(destination) = action.destination(page) {
                    edges[page.index()] |= 1 << destination.index();
                }
            }
        }
    }
    let mut reachable = 1u8 << ControlPage::Dashboard.index();
    for _ in 0..3 {
        for page in ControlPage::ALL {
            if reachable & (1 << page.index()) != 0 {
                reachable |= edges[page.index()];
            }
        }
    }
    for page in ControlPage::ALL {
        if reachable & (1 << page.index()) == 0 {
            emit(ControlError::UnreachableScreen { page })?;
        }
    }
    if !map.screens[ControlPage::Dashboard.index()]
        .buttons
        .iter()
        .any(|binding| {
            [binding.tap, binding.hold]
                .iter()
                .any(|action| matches!(action, ControlAction::NextView | ControlAction::PreviousView))
        })
    {
        emit(ControlError::MissingViewNavigation)?;
    }
    core::ops::ControlFlow::Continue(())
}
