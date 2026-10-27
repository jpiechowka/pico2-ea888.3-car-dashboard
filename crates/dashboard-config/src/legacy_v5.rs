//! Frozen host-only v5 container; widget/readout vocabulary is frozen separately.
use heapless::Vec;
use serde::{Deserialize, Serialize};

use crate::legacy_widget_v6::{LegacyWidget, upgrade_widgets};
use crate::*;
/// Frozen version-five field order; migration adds disabled BLE, Global ignition popup layout and current schema.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct LegacyConfigV5 {
    /// Historical schema number; upgrade rejects a value different from this frozen container version.
    pub version: u16,
    /// Documentary catalogue revision, retained for current semantic admission.
    pub catalogue_version: u16,
    /// Initial view ID, retained without reindexing.
    pub boot_view: u8,
    /// Bounded historical view list; order and IDs survive migration.
    pub views: Vec<View, MAX_VIEWS>,
    /// Bounded historical widget list; migration never drops an instance to gain capacity.
    pub widgets: Vec<LegacyWidget, MAX_WIDGETS>,
    /// Eight channel-indexed documentary source selections, retained for current validation.
    pub sources: [ChannelSource; 8],
    /// Historical warning presentation policy, migrated without changing selected scope or timing.
    pub warnings: WarningPresentation,
    /// Explicit fuel assumption, retained without inventing measured composition.
    pub fuel: Fuel,
    /// Retained popup foreground/background colors; current contrast rules still apply.
    pub warning_colors: crate::WarningColors,
    /// Retained control map; current screen/recovery/reachability rules are revalidated.
    pub controls: crate::ControlMap,
    /// Per-channel history seconds, retained in stable channel order.
    pub history_seconds: [u16; 8],
    /// Retained animation-reduction policy.
    pub reduced_motion: bool,
    /// Retained dashboard RGB policy; unrelated to standalone probe status.
    pub led: crate::LedSettings,
    /// Retained default panel mounting direction.
    pub orientation: crate::Orientation,
}

impl LegacyConfigV5 {
    /// Consume a version-five candidate and return current semantic admission.
    /// Preserves orientation and all retained settings; rejects other versions or invalid values.
    pub(crate) fn upgrade(self) -> Result<ValidatedConfig, ConfigError> {
        if self.version != 5 {
            return Err(ConfigError::Version);
        }
        validate(Config {
            version: VERSION,
            catalogue_version: self.catalogue_version,
            boot_view: self.boot_view,
            views: self.views,
            widgets: upgrade_widgets(self.widgets),
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
