//! Hardware-facing adapters shared by dashboard and the standalone BLE probe.
//! Pure state, rendering and protocol models are re-exported from dashboard-core.
#![no_std]

pub use dashboard_core::*;
/// Build-selected clock configuration and emitted identity metadata.
pub mod clock_profile;
/// Release-only checked flash ownership and physical-region admission.
pub mod flash_guard;
#[path = "drivers/st7789.rs"]
/// Retained SPI/DMA panel owner shared by both images.
pub mod panel;
#[path = "drivers/watchdog.rs"]
/// RP2350 watchdog reset mask and retained boot/failure evidence.
pub mod recovery_watchdog;
/// Bounded Adafruit 5880 register transport and identity checks.
pub mod seesaw;

/// Hardware cycle/stack observations exported without duplicating implementations.
mod profiling {
    /// Per-core DWT progress checks and checked elapsed-cycle arithmetic.
    pub mod cpu_cycles;
    /// Linked main-SRAM bounds and instantaneous stack samples.
    pub mod memory;
}
pub use profiling::{cpu_cycles, memory};
