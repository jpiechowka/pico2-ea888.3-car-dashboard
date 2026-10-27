//! Frozen host-only v4 container. Its widget/readout vocabulary is frozen separately.
use serde::{Deserialize, Serialize};

use crate::legacy::{LegacyView, upgrade_views};
use crate::legacy_widget_v6::{LegacyWidget, upgrade_widgets};
use crate::*;

/// Frozen version-four field order; orientation and BLE were absent from this container.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct LegacyConfigV4 {
    /// Historical schema number; upgrade rejects a value different from this frozen container version.
    version: u16,
    /// Documentary catalogue revision, retained for current semantic admission.
    catalogue_version: u16,
    /// Initial view ID, retained without reindexing.
    boot_view: u8,
    /// Bounded historical view list; order and IDs survive migration.
    views: heapless::Vec<LegacyView, MAX_VIEWS>,
    /// Bounded historical widget list; migration never drops an instance to gain capacity.
    widgets: heapless::Vec<LegacyWidget, MAX_WIDGETS>,
    /// Eight channel-indexed documentary source selections, retained for current validation.
    sources: [ChannelSource; 8],
    /// Historical warning presentation policy, migrated without changing selected scope or timing.
    warnings: WarningPresentation,
    /// Explicit fuel assumption, retained without inventing measured composition.
    fuel: Fuel,
    /// Retained popup foreground/background colors; current contrast rules still apply.
    warning_colors: WarningColors,
    /// Retained control map; current screen/recovery/reachability rules are revalidated.
    controls: ControlMap,
    /// Per-channel history seconds, retained in stable channel order.
    history_seconds: [u16; 8],
    /// Retained animation-reduction policy.
    reduced_motion: bool,
    /// Retained dashboard RGB policy; unrelated to standalone probe status.
    led: LedSettings,
}

impl LegacyConfigV4 {
    /// Consume a version-four candidate and return current semantic admission.
    /// Adds landscape/inherited views and disabled BLE; preserves every other setting.
    /// Another version or an invalid retained setting returns an error without partial acceptance.
    pub(crate) fn upgrade(self) -> Result<ValidatedConfig, ConfigError> {
        if self.version != 4 {
            return Err(ConfigError::Version);
        }
        validate(Config {
            version: VERSION,
            catalogue_version: self.catalogue_version,
            boot_view: self.boot_view,
            views: upgrade_views(self.views),
            widgets: upgrade_widgets(self.widgets),
            sources: self.sources,
            warnings: self.warnings,
            fuel: self.fuel,
            warning_colors: self.warning_colors,
            controls: self.controls,
            history_seconds: self.history_seconds,
            reduced_motion: self.reduced_motion,
            led: self.led,
            orientation: Orientation::Landscape,
            ble: BleSettings::DEFAULT,
        })
        .map_err(ConfigError::Validation)
    }
}
