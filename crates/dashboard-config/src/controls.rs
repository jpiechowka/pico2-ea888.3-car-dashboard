//! Compact physical control bindings and their independent validation.
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ControlPage {
    #[default]
    Dashboard,
    Debug,
    Logs,
}
impl ControlPage {
    pub const ALL: [Self; 3] = [Self::Dashboard, Self::Debug, Self::Logs];

    pub const fn label(self) -> &'static str {
        match self {
            Self::Dashboard => "Dashboard",
            Self::Debug => "Debug",
            Self::Logs => "Logs",
        }
    }

    pub const fn index(self) -> usize { self as usize }

    pub const fn next(self) -> Self {
        match self {
            Self::Dashboard => Self::Debug,
            Self::Debug => Self::Logs,
            Self::Logs => Self::Dashboard,
        }
    }

    pub const fn previous(self) -> Self {
        match self {
            Self::Dashboard => Self::Logs,
            Self::Debug => Self::Dashboard,
            Self::Logs => Self::Debug,
        }
    }

    pub const fn toggle(self) -> Self { self.next() }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ButtonId {
    A,
    B,
    X,
    Y,
}
impl ButtonId {
    pub const ALL: [Self; 4] = [Self::A, Self::B, Self::X, Self::Y];

    pub const fn label(self) -> &'static str {
        match self {
            Self::A => "A",
            Self::B => "B",
            Self::X => "X",
            Self::Y => "Y",
        }
    }

    pub const fn index(self) -> usize { self as usize }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ControlAction {
    None,
    NextScreen,
    PreviousScreen,
    Dashboard,
    Debug,
    Logs,
    NextView,
    PreviousView,
    ToggleBoostUnits,
    CycleFps,
    ResetStatistics,
    CycleBrightness,
    ToggleBacklight,
    BrightnessUp,
    BrightnessDown,
    ScrollOlder,
    ScrollNewer,
    LogsLive,
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum RotationAction {
    None,
    Views,
    Brightness,
    Logs,
}
impl RotationAction {
    pub const ALL: [Self; 4] = [Self::None, Self::Views, Self::Brightness, Self::Logs];

    pub const fn label(self) -> &'static str {
        match self {
            Self::None => "No action",
            Self::Views => "Select views",
            Self::Brightness => "Adjust brightness",
            Self::Logs => "Scroll logs",
        }
    }

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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ButtonBinding {
    pub tap: ControlAction,
    pub hold: ControlAction,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScreenBindings {
    pub buttons: [ButtonBinding; 4],
    pub encoder_rotation: RotationAction,
    pub encoder_click: ControlAction,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ControlMap {
    pub hold_ms: u16,
    pub screens: [ScreenBindings; 3],
}
impl Default for ControlMap {
    fn default() -> Self {
        use ControlAction::*;
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ControlError {
    HoldDuration,
    ReservedRecovery {
        page: ControlPage,
    },
    IncompatibleButton {
        page: ControlPage,
        button: ButtonId,
        hold: bool,
    },
    IncompatibleEncoderClick {
        page: ControlPage,
    },
    IncompatibleEncoderRotation {
        page: ControlPage,
    },
    UnreachableScreen {
        page: ControlPage,
    },
    MissingViewNavigation,
}

pub fn validate_controls(map: &ControlMap) -> Result<(), ControlError> {
    let mut first = None;
    let _ = visit_controls(map, |error| {
        first = Some(error);
        core::ops::ControlFlow::Break(())
    });
    first.map_or(Ok(()), Err)
}

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
