//! Core0 initializes once before core1 starts. Only immutable references escape.
use dashboard_pico2::view_config::{BootConfig, ConfigSlot};
use static_cell::ConstStaticCell;

/// One-time runtime parse/fallback slot; no mutable reference escapes initialization.
static CONFIGURATION: ConstStaticCell<ConfigSlot> = ConstStaticCell::new(ConfigSlot::new());
// Keep the exact runtime-consumed bytes inspectable in the final linked image.
#[used]
#[unsafe(no_mangle)]
#[unsafe(link_section = ".dashboard_config")]
/// Exact build-admitted compact configuration bytes retained for linked-image checks.
static DASHBOARD_EMBEDDED_CONFIG: [u8; include_bytes!(concat!(env!("OUT_DIR"), "/dashboard.eav")).len()] =
    *include_bytes!(concat!(env!("OUT_DIR"), "/dashboard.eav"));
/// Hex SHA-256 of the embedded configuration snapshot, provided by the build script.
pub const SHA256: &str = env!("DASHBOARD_CONFIG_SHA256");

#[inline(never)]
/// Returns the immutable parsed boot configuration, with shared explicit fallback
/// diagnostics on invalid bytes. Panics if called twice; call before core1 starts.
pub fn initialize() -> &'static BootConfig { CONFIGURATION.take().load_at_boot(&DASHBOARD_EMBEDDED_CONFIG) }
