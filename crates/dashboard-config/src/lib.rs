#![no_std]

#[cfg(feature = "json")]
extern crate alloc;

mod appearance;
mod catalog;
mod codec;
mod defaults;
mod fuel;
#[cfg(feature = "json")]
mod legacy;
mod model;
mod validate;

pub use appearance::*;
pub use catalog::*;
pub use codec::*;
pub use defaults::{default_config, default_widget};
pub use model::*;
pub use validate::*;

/// Startup owns this result before publishing an immutable configuration to
/// either executor. The fault survives even though a complete default loads.
pub struct BootConfig {
    pub config: ValidatedConfig,
    pub fault: Option<ConfigError>,
}

#[cfg(target_pointer_width = "32")]
const _: () = assert!(core::mem::size_of::<BootConfig>() <= 10_240);

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
