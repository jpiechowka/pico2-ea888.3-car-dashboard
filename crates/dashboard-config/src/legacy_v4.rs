//! Frozen host-only v4 container. Its children other than View are unchanged.
use serde::{Deserialize, Serialize};

use crate::legacy::{LegacyView, upgrade_views};
use crate::*;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct LegacyConfigV4 {
    version: u16,
    catalogue_version: u16,
    boot_view: u8,
    views: heapless::Vec<LegacyView, MAX_VIEWS>,
    widgets: heapless::Vec<Widget, MAX_WIDGETS>,
    sources: [ChannelSource; 8],
    warnings: WarningPresentation,
    fuel: Fuel,
    warning_colors: WarningColors,
    controls: ControlMap,
    history_seconds: [u16; 8],
    reduced_motion: bool,
    led: LedSettings,
}

impl LegacyConfigV4 {
    pub(crate) fn upgrade(self) -> Result<ValidatedConfig, ConfigError> {
        if self.version != 4 {
            return Err(ConfigError::Version);
        }
        validate(Config {
            version: VERSION,
            catalogue_version: self.catalogue_version,
            boot_view: self.boot_view,
            views: upgrade_views(self.views),
            widgets: self.widgets,
            sources: self.sources,
            warnings: self.warnings,
            fuel: self.fuel,
            warning_colors: self.warning_colors,
            controls: self.controls,
            history_seconds: self.history_seconds,
            reduced_motion: self.reduced_motion,
            led: self.led,
            orientation: Orientation::Landscape,
        })
        .map_err(ConfigError::Validation)
    }
}
