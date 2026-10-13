//! Browser preferences and pure manual workspace navigation, never firmware state.
use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

pub const MAX_PREFERENCES_BYTES: usize = 4096;
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Workspace {
    #[default]
    Views,
    Device,
    DataWarnings,
    Diagnostics,
    BuildFlash,
}
impl Workspace {
    pub const ALL: [Self; 5] = [
        Self::Views,
        Self::Device,
        Self::DataWarnings,
        Self::Diagnostics,
        Self::BuildFlash,
    ];

    pub const fn index(self) -> usize {
        match self {
            Self::Views => 0,
            Self::Device => 1,
            Self::DataWarnings => 2,
            Self::Diagnostics => 3,
            Self::BuildFlash => 4,
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Views => "Views",
            Self::Device => "Device",
            Self::DataWarnings => "Data & warnings",
            Self::Diagnostics => "Diagnostics",
            Self::BuildFlash => "Build & Flash",
        }
    }

    pub const fn id(self) -> &'static str {
        match self {
            Self::Views => "views",
            Self::Device => "device",
            Self::DataWarnings => "data-warnings",
            Self::Diagnostics => "diagnostics",
            Self::BuildFlash => "build-flash",
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum GroupId {
    Layout,
    Readout,
    Gauge,
    HistoryStatistics,
    Warnings,
    Appearance,
    Bindings,
    Indicator,
    Channels,
    Sources,
    Fuel,
    CombinedWarnings,
    Simulation,
    Files,
    Measurements,
}
impl GroupId {
    pub const fn id(self) -> &'static str {
        match self {
            Self::Layout => "layout",
            Self::Readout => "readout",
            Self::Gauge => "gauge",
            Self::HistoryStatistics => "history-statistics",
            Self::Warnings => "warnings",
            Self::Appearance => "appearance",
            Self::Bindings => "bindings",
            Self::Indicator => "indicator",
            Self::Channels => "channels",
            Self::Sources => "sources",
            Self::Fuel => "fuel",
            Self::CombinedWarnings => "combined-warnings",
            Self::Simulation => "simulation",
            Self::Files => "files",
            Self::Measurements => "measurements",
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Layout => "Layout",
            Self::Readout => "Readout",
            Self::Gauge => "Gauge",
            Self::HistoryStatistics => "History & statistics",
            Self::Warnings => "Warnings",
            Self::Appearance => "Appearance",
            Self::Bindings => "Buttons & encoder",
            Self::Indicator => "RGB indicator",
            Self::Channels => "Channel history",
            Self::Sources => "Channel sources",
            Self::Fuel => "Fuel & AFR",
            Self::CombinedWarnings => "Combined warnings",
            Self::Simulation => "Timing simulation",
            Self::Files => "Configuration files",
            Self::Measurements => "Measurements",
        }
    }
}
#[derive(Clone, Copy, Debug)]
pub enum TabKey {
    Previous,
    Next,
    First,
    Last,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkspaceState {
    active: Workspace,
    focused: Workspace,
    expanded: BTreeSet<GroupId>,
}
impl Default for WorkspaceState {
    fn default() -> Self {
        Self {
            active: Workspace::Views,
            focused: Workspace::Views,
            expanded: [
                GroupId::Layout,
                GroupId::Readout,
                GroupId::Bindings,
                GroupId::Channels,
                GroupId::Files,
            ]
            .into_iter()
            .collect(),
        }
    }
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Preferences {
    version: u16,
    active: Workspace,
    expanded: Vec<GroupId>,
}
impl WorkspaceState {
    pub fn active(&self) -> Workspace { self.active }

    pub fn focused(&self) -> Workspace { self.focused }

    pub fn expanded(
        &self,
        group: GroupId,
    ) -> bool {
        self.expanded.contains(&group)
    }

    pub fn focus(
        &mut self,
        key: TabKey,
    ) {
        let index = self.focused.index();
        self.focused = Workspace::ALL[match key {
            TabKey::Previous => (index + 4) % 5,
            TabKey::Next => (index + 1) % 5,
            TabKey::First => 0,
            TabKey::Last => 4,
        }];
    }

    pub fn activate_focused(&mut self) { self.active = self.focused; }

    pub fn activate(
        &mut self,
        workspace: Workspace,
    ) {
        self.active = workspace;
        self.focused = workspace;
    }

    pub fn open(
        &mut self,
        workspace: Workspace,
        group: GroupId,
    ) {
        self.activate(workspace);
        self.expanded.insert(group);
    }

    pub fn set_expanded(
        &mut self,
        group: GroupId,
        expanded: bool,
    ) {
        if expanded {
            self.expanded.insert(group);
        } else {
            self.expanded.remove(&group);
        }
    }

    pub fn preferences(&self) -> Result<String, &'static str> {
        let text = serde_json::to_string(&Preferences {
            version: 1,
            active: self.active,
            expanded: self.expanded.iter().copied().collect(),
        })
        .map_err(|_| "Workspace preferences could not be encoded")?;
        if text.len() > MAX_PREFERENCES_BYTES {
            return Err("Workspace preferences exceed 4 KiB");
        }
        Ok(text)
    }

    pub fn from_preferences(text: Option<&str>) -> (Self, Option<&'static str>) {
        let Some(text) = text else {
            return (Self::default(), None);
        };
        let fail = || (Self::default(), Some("Workspace preferences are invalid; using Views"));
        if text.len() > MAX_PREFERENCES_BYTES {
            return fail();
        }
        let Ok(pref) = serde_json::from_str::<Preferences>(text) else {
            return fail();
        };
        let expanded = pref.expanded.iter().copied().collect::<BTreeSet<_>>();
        if pref.version != 1 || expanded.len() != pref.expanded.len() {
            return fail();
        }
        (
            Self {
                active: pref.active,
                focused: pref.active,
                expanded,
            },
            None,
        )
    }
}
