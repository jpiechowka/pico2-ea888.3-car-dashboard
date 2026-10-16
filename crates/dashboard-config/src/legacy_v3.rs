//! Frozen host-only v3 wire contract, including its action and rule vocabularies.
use serde::{Deserialize, Serialize};

use crate::legacy::{
    LegacyView,
    OldDisplayOptions,
    OldGauge,
    OldWarningPresentation,
    OldWarningSettings,
    upgrade_views,
};
use crate::*;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct LegacyConfigV3 {
    version: u16,
    catalogue_version: u16,
    boot_view: u8,
    views: heapless::Vec<LegacyView, MAX_VIEWS>,
    widgets: heapless::Vec<LegacyWidgetV3, MAX_WIDGETS>,
    sources: [ChannelSource; 8],
    warnings: OldWarningPresentation,
    fuel: Fuel,
    warning_colors: WarningColors,
    controls: LegacyControlMap,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct LegacyWidgetV3 {
    id: u16,
    view: u8,
    channel: Channel,
    placement: Placement,
    label: heapless::String<24>,
    display: OldDisplayOptions,
    warning: OldWarningSettings,
    gauge: OldGauge,
    colors: WidgetColors,
}
impl LegacyConfigV3 {
    pub(crate) fn upgrade(self) -> Result<ValidatedConfig, ConfigError> {
        if self.version != 3 {
            return Err(ConfigError::Version);
        }
        let mut widgets = heapless::Vec::new();
        for w in self.widgets {
            widgets
                .push(Widget {
                    id: w.id,
                    view: w.view,
                    channel: w.channel,
                    placement: w.placement,
                    label: w.label,
                    advanced: AdvancedDisplay::for_channel(w.channel, w.display.afr),
                    display: w.display.into(),
                    warning: w.warning.into(),
                    gauge: w.gauge.into(),
                    colors: w.colors,
                })
                .map_err(|_| ConfigError::Payload)?;
        }
        validate(Config {
            version: VERSION,
            catalogue_version: self.catalogue_version,
            boot_view: self.boot_view,
            views: upgrade_views(self.views),
            widgets,
            sources: self.sources,
            warnings: self.warnings.into(),
            fuel: self.fuel,
            warning_colors: self.warning_colors,
            controls: self.controls.into(),
            history_seconds: [120; 8],
            reduced_motion: false,
            led: LedSettings::default(),
            orientation: Orientation::Landscape,
        })
        .map_err(ConfigError::Validation)
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum LegacyControlAction {
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
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LegacyButtonBinding {
    pub tap: LegacyControlAction,
    pub hold: LegacyControlAction,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LegacyScreenBindings {
    pub buttons: [LegacyButtonBinding; 4],
    pub encoder_rotation: RotationAction,
    pub encoder_click: LegacyControlAction,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LegacyControlMap {
    pub hold_ms: u16,
    pub screens: [LegacyScreenBindings; 3],
}
impl From<LegacyControlAction> for ControlAction {
    fn from(value: LegacyControlAction) -> Self {
        match value {
            LegacyControlAction::None => Self::None,
            LegacyControlAction::NextScreen => Self::NextScreen,
            LegacyControlAction::PreviousScreen => Self::PreviousScreen,
            LegacyControlAction::Dashboard => Self::Dashboard,
            LegacyControlAction::Debug => Self::Debug,
            LegacyControlAction::Logs => Self::Logs,
            LegacyControlAction::NextView => Self::NextView,
            LegacyControlAction::PreviousView => Self::PreviousView,
            LegacyControlAction::ToggleBoostUnits => Self::ToggleBoostUnits,
            LegacyControlAction::CycleFps => Self::CycleFps,
            LegacyControlAction::ResetStatistics => Self::ResetStatistics,
            LegacyControlAction::CycleBrightness => Self::CycleBrightness,
            LegacyControlAction::ToggleBacklight => Self::ToggleBacklight,
            LegacyControlAction::BrightnessUp => Self::BrightnessUp,
            LegacyControlAction::BrightnessDown => Self::BrightnessDown,
            LegacyControlAction::ScrollOlder => Self::ScrollOlder,
            LegacyControlAction::ScrollNewer => Self::ScrollNewer,
            LegacyControlAction::LogsLive => Self::LogsLive,
        }
    }
}
impl From<LegacyControlMap> for ControlMap {
    fn from(value: LegacyControlMap) -> Self {
        Self {
            hold_ms: value.hold_ms,
            screens: value.screens.map(|s| {
                ScreenBindings {
                    buttons: s.buttons.map(|b| {
                        ButtonBinding {
                            tap: b.tap.into(),
                            hold: b.hold.into(),
                        }
                    }),
                    encoder_rotation: s.encoder_rotation,
                    encoder_click: s.encoder_click.into(),
                }
            }),
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn frozen_v3_default_and_maximum_are_canonical() {
        for (bytes, expected_widgets, json) in [
            (
                include_bytes!("../tests/fixtures/v3-default.eav").as_slice(),
                8,
                Some(include_str!("../tests/fixtures/v3-default.json")),
            ),
            (include_bytes!("../tests/fixtures/v3-maximum.eav").as_slice(), 64, None),
        ] {
            let (c, rest) = postcard::take_from_bytes::<LegacyConfigV3>(&bytes[12..]).unwrap();
            assert!(rest.is_empty());
            crate::codec::compare_canonical(&c, &bytes[12..]).unwrap();
            assert_eq!(c.widgets.len(), expected_widgets);
            if let Some(json) = json {
                assert_eq!(serde_json::to_string_pretty(&c).unwrap(), json.trim_end());
            }
            c.upgrade().unwrap();
        }
    }
}
