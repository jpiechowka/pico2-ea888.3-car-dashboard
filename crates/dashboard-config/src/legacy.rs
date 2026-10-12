//! Host-only v1 importer. Firmware consumes v2 and fails closed on old images.
use serde::{Deserialize, Serialize};

use crate::*;

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct LegacyConfig {
    version: u16,
    catalogue_version: u16,
    boot_view: u8,
    views: heapless::Vec<View, MAX_VIEWS>,
    widgets: heapless::Vec<LegacyWidget, MAX_WIDGETS>,
    sources: [ChannelSource; 8],
    warnings: WarningPresentation,
    fuel: Fuel,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct LegacyWidget {
    id: u16,
    view: u8,
    channel: Channel,
    placement: Placement,
    label: heapless::String<24>,
    display: DisplayOptions,
    warning: WarningSettings,
}
impl LegacyConfig {
    pub(crate) fn upgrade(self) -> Result<ValidatedConfig, ConfigError> {
        if self.version != 1 {
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
                    display: w.display,
                    warning: w.warning,
                    gauge: Gauge::for_channel(w.channel),
                    colors: WidgetColors::default(),
                })
                .map_err(|_| ConfigError::Payload)?;
        }
        validate(Config {
            version: VERSION,
            catalogue_version: self.catalogue_version,
            boot_view: self.boot_view,
            views: self.views,
            widgets,
            sources: self.sources,
            warnings: self.warnings,
            fuel: self.fuel,
            warning_colors: WarningColors::default(),
        })
        .map_err(ConfigError::Validation)
    }
}
