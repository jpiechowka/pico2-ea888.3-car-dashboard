//! Bounded dashboard configuration, migration, validation and startup-asset contracts.
//! Decode into [`ValidatedConfig`] before runtime publication; deserializing [`Config`]
//! alone does not validate editable values. JSON support requires the `json` feature.
#![no_std]

#[cfg(feature = "json")]
extern crate alloc;

mod advanced;
mod ble;
pub use ble::*;
mod appearance;
mod cache_identity;
mod catalog;
mod led;
#[cfg(feature = "json")]
mod legacy_v5;
pub use cache_identity::{CacheIdentityStatus, ConfigCacheIdentity};
mod codec;
pub mod controls;
mod decode_in_place;
mod defaults;
mod fuel;
#[cfg(feature = "json")]
mod legacy;
#[cfg(feature = "json")]
mod legacy_v3;
#[cfg(feature = "json")]
mod legacy_v4;
mod model;
mod orientation;
pub mod startup;
pub mod startup_asset;
mod validate;

pub use advanced::*;
pub use appearance::*;
pub use catalog::*;
pub use codec::*;
pub use controls::*;
pub use defaults::{default_config, default_widget};
pub use led::*;
pub use model::*;
pub use orientation::*;
pub use validate::*;

/// Startup owns this result before publishing an immutable configuration to
/// either executor. The fault survives even though a complete default loads.
pub struct BootConfig {
    /// Admitted input or the complete reviewed default after decode/validation failure.
    pub config: ValidatedConfig,
    /// Original input failure; `None` means the supplied bytes were admitted.
    pub fault: Option<ConfigError>,
}

// Config storage plus proof/fault metadata; whole-image checks run separately.
#[cfg(target_pointer_width = "32")]
/// Compile-time 32-bit storage ceiling; excludes call-stack and executor allocations.
const _: () = assert!(core::mem::size_of::<BootConfig>() <= 14_620);

/// Decode `bytes` and return boot-owned configuration plus the retained input fault.
/// Malformed, unsupported or invalid input selects the entire default; no partial
/// configuration is published. Panics only if the reviewed default fails validation.
#[inline(never)]
pub fn load_at_boot(bytes: &[u8]) -> BootConfig {
    match decode(bytes) {
        Ok(config) => BootConfig { config, fault: None },
        Err(fault) => {
            BootConfig {
                config: validate(default_config()).expect("reviewed default configuration"),
                fault: Some(fault),
            }
        }
    }
}
