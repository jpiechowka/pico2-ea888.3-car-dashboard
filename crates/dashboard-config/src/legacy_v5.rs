//! Frozen host-only v5 container; child wire contracts are unchanged.
use heapless::Vec;
use serde::{Deserialize, Serialize};

use crate::*;
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct LegacyConfigV5 {
    pub version: u16,
    pub catalogue_version: u16,
    pub boot_view: u8,
    pub views: Vec<View, MAX_VIEWS>,
    pub widgets: Vec<Widget, MAX_WIDGETS>,
    pub sources: [ChannelSource; 8],
    pub warnings: WarningPresentation,
    pub fuel: Fuel,
    pub warning_colors: crate::WarningColors,
    pub controls: crate::ControlMap,
    pub history_seconds: [u16; 8],
    pub reduced_motion: bool,
    pub led: crate::LedSettings,
    pub orientation: crate::Orientation,
}

impl LegacyConfigV5 {
    pub(crate) fn upgrade(self) -> Result<ValidatedConfig, ConfigError> {
        if self.version != 5 {
            return Err(ConfigError::Version);
        }
        validate(Config {
            version: VERSION,
            catalogue_version: self.catalogue_version,
            boot_view: self.boot_view,
            views: self.views,
            widgets: self.widgets,
            sources: self.sources,
            warnings: self.warnings,
            fuel: self.fuel,
            warning_colors: self.warning_colors,
            controls: self.controls,
            history_seconds: self.history_seconds,
            reduced_motion: self.reduced_motion,
            led: self.led,
            orientation: self.orientation,
            ble: BleSettings::DEFAULT,
        })
        .map_err(ConfigError::Validation)
    }
}
