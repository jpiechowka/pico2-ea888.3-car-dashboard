//! Core0 initializes once before core1 starts. Only immutable references escape.
use dashboard_pico2::view_config::{BootConfig, load_at_boot};
use static_cell::StaticCell;

static CONFIGURATION: StaticCell<BootConfig> = StaticCell::new();
static EMBEDDED: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/dashboard.eav"));
pub const SHA256: &str = env!("DASHBOARD_CONFIG_SHA256");

#[inline(never)]
pub fn initialize() -> &'static BootConfig { CONFIGURATION.init_with(|| load_at_boot(EMBEDDED)) }
