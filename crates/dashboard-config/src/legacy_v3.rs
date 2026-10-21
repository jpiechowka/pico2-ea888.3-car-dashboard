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

/// Version-three field order, including pre-advanced widgets and pre-acknowledgment controls.
/// Deserialization alone does not admit the configuration; consume it with `upgrade`.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct LegacyConfigV3 {
    /// Historical schema number; upgrade rejects a value different from this frozen container version.
    version: u16,
    /// Documentary catalogue revision, retained for current semantic admission.
    catalogue_version: u16,
    /// Initial view ID, retained without reindexing.
    boot_view: u8,
    /// Bounded historical view list; order and IDs survive migration.
    views: heapless::Vec<LegacyView, MAX_VIEWS>,
    /// Bounded historical widget list; migration never drops an instance to gain capacity.
    widgets: heapless::Vec<LegacyWidgetV3, MAX_WIDGETS>,
    /// Eight channel-indexed documentary source selections, retained for current validation.
    sources: [ChannelSource; 8],
    /// Historical warning presentation policy, migrated without changing selected scope or timing.
    warnings: OldWarningPresentation,
    /// Explicit fuel assumption, retained without inventing measured composition.
    fuel: Fuel,
    /// Retained popup foreground/background colors; current contrast rules still apply.
    warning_colors: WarningColors,
    /// Retained control map; current screen/recovery/reachability rules are revalidated.
    controls: LegacyControlMap,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
/// Frozen widget record; current defaults are added without changing historical field order.
struct LegacyWidgetV3 {
    /// Historical nonzero identity candidate; uniqueness is checked after migration.
    id: u16,
    /// Referenced view ID, preserved without substituting an array index.
    view: u8,
    /// Canonical measurement channel retained by migration.
    channel: Channel,
    /// Logical historical grid position and span; no automatic repacking during migration.
    placement: Placement,
    /// Historical bounded widget label; retained verbatim for current validation.
    label: heapless::String<24>,
    /// Historical numeric/history options, mapped without losing requested visibility.
    display: OldDisplayOptions,
    /// Historical warning rule; new window/latch/snooze fields receive explicit defaults.
    warning: OldWarningSettings,
    /// Historical gauge shape/range; canonical endpoints are preserved.
    gauge: OldGauge,
    /// Historical per-role palette, including dormant roles.
    colors: WidgetColors,
}
impl LegacyConfigV3 {
    /// Consume the historical candidate and return a fully validated current configuration.
    /// Rejects another version, capacity failure or current validation errors. Adds 120-second
    /// history, landscape/inherited views, default LED/advanced settings and disabled BLE.
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
            ble: BleSettings::DEFAULT,
        })
        .map_err(ConfigError::Validation)
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
/// Frozen variant indices; acknowledgment actions did not exist in version three.
pub enum LegacyControlAction {
    /// No action.
    None,
    /// Advance the screen cycle.
    NextScreen,
    /// Reverse the screen cycle.
    PreviousScreen,
    /// Select Dashboard.
    Dashboard,
    /// Select Debug.
    Debug,
    /// Select Logs.
    Logs,
    /// Advance the configured view cycle.
    NextView,
    /// Reverse the configured view cycle.
    PreviousView,
    /// Toggle the shared boost-unit preference.
    ToggleBoostUnits,
    /// Cycle the FPS overlay mode.
    CycleFps,
    /// Request accumulated-statistics reset.
    ResetStatistics,
    /// Cycle the configured backlight levels.
    CycleBrightness,
    /// Toggle the backlight state.
    ToggleBacklight,
    /// Increase backlight brightness.
    BrightnessUp,
    /// Decrease backlight brightness.
    BrightnessDown,
    /// Scroll toward older log entries.
    ScrollOlder,
    /// Scroll toward newer log entries.
    ScrollNewer,
    /// Resume following the newest log entries.
    LogsLive,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
/// Frozen tap/hold pair, converted by variant without substituting default actions.
pub struct LegacyButtonBinding {
    /// Historical short-press action.
    pub tap: LegacyControlAction,
    /// Historical hold action; current reserved-Y validation still applies.
    pub hold: LegacyControlAction,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
/// Frozen bindings for one screen; physical button ordering is unchanged.
pub struct LegacyScreenBindings {
    /// A/B/X/Y physical button order, preserved during conversion.
    pub buttons: [LegacyButtonBinding; 4],
    /// Historical rotation action, retained with later compatibility validation.
    pub encoder_rotation: RotationAction,
    /// Historical encoder click action, converted by exact variant.
    pub encoder_click: LegacyControlAction,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
/// Frozen control policy; migration preserves timing and delegates reachability admission.
pub struct LegacyControlMap {
    /// Historical remappable hold threshold in milliseconds; reserved recovery remains independent.
    pub hold_ms: u16,
    /// Dashboard/Debug/Logs binding order, preserved during conversion.
    pub screens: [LegacyScreenBindings; 3],
}
impl From<LegacyControlAction> for ControlAction {
    /// Map `value` one-to-one to the current action; returns no newly introduced acknowledgment.
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
    /// Convert `value` preserving threshold, screen order and every binding; returns an unvalidated map.
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
/// Frozen binary/JSON fixtures detect accidental historical wire-contract changes.
mod tests {
    use super::*;
    #[test]
    /// Both default and maximum records remain canonical and migrate without dropping widgets.
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
