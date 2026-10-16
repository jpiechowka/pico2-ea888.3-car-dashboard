#![no_std]

#[cfg(feature = "json")]
extern crate alloc;

mod advanced;
mod appearance;
mod cache_identity;
mod catalog;
mod led;
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
    pub config: ValidatedConfig,
    pub fault: Option<ConfigError>,
}

// Config storage plus proof/fault metadata; whole-image checks run separately.
#[cfg(target_pointer_width = "32")]
const _: () = assert!(core::mem::size_of::<BootConfig>() <= 14_604);

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
