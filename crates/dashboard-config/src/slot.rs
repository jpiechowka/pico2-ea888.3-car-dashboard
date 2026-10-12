use super::{ValidatedConfig, validate_ref};
use crate::BootConfig;
use crate::codec::decode_current_into;
use crate::defaults::{empty_config, fill_default_config};

/// Initialized, unpublished storage for one bounded configuration.
///
/// Loading fills this storage directly. Only a complete, canonical and
/// semantically validated document (or the reviewed default) is published.
/// The current envelope is required even when host JSON migration is enabled.
/// Place `ConfigSlot::new()` in constant static storage for firmware; a local
/// slot still occupies a full configuration on its caller's stack.
///
/// Storage is private and the slot has no unchecked or mutable accessor:
/// ```compile_fail
/// let slot = dashboard_config::ConfigSlot::new();
/// let config = slot.storage;
/// ```
/// A published borrow prevents reloading until its last use:
/// ```compile_fail
/// let mut slot = dashboard_config::ConfigSlot::new();
/// let first = slot.load_at_boot(b"");
/// let second = slot.load_at_boot(b"");
/// assert_eq!(first.fault, second.fault);
/// ```
pub struct ConfigSlot {
    // This wrapper starts with legally initialized but unvalidated values.
    // It remains private until load_at_boot has replaced and validated every
    // field. Failed partial decoding always refills the complete default.
    storage: BootConfig,
}

impl ConfigSlot {
    pub const fn new() -> Self {
        Self {
            storage: BootConfig {
                config: ValidatedConfig {
                    config: empty_config(),
                    stoich: 0.0,
                },
                fault: None,
            },
        }
    }

    #[inline(never)]
    pub fn load_at_boot(
        &mut self,
        bytes: &[u8],
    ) -> &BootConfig {
        match decode_current_into(bytes, &mut self.storage.config.config) {
            Ok(stoich) => {
                self.storage.config.stoich = stoich;
                self.storage.fault = None;
            }
            Err(fault) => {
                fill_default_config(&mut self.storage.config.config);
                self.storage.config.stoich =
                    validate_ref(&self.storage.config.config).expect("reviewed default configuration");
                self.storage.fault = Some(fault);
            }
        }
        &self.storage
    }
}

impl Default for ConfigSlot {
    fn default() -> Self { Self::new() }
}

#[cfg(target_pointer_width = "32")]
const _: () = assert!(core::mem::size_of::<ConfigSlot>() <= 10_240);
