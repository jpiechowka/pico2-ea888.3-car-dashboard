//! Actual probe telemetry projection with hardware admission/sampling unavailable.

extern crate self as dashboard_pico2;

/// Host-only type boundary for otherwise unused hardware telemetry entrypoints.
pub mod memory {
    /// Linker-memory observation shape; host tests never fabricate an observation.
    pub struct MemoryStats {
        /// Static allocation in bytes, unavailable through this host stub.
        pub static_ram: u32,
        /// Physical memory bytes, unavailable through this host stub.
        pub ram_total: u32,
    }
    impl MemoryStats {
        /// Never returns; panics if a test attempts hardware/linker memory admission.
        pub fn collect() -> Self { panic!("hardware memory collection unavailable") }
    }
    /// Never returns; panics if a test interprets a host pointer as an RP2350 sample.
    pub fn stack_pointer() -> u32 { panic!("hardware stack sampling unavailable") }
}

/// Production observation lock, freshness projection and completion snapshot.
#[allow(dead_code)]
#[path = "../../firmware/ble-probe/src/metrics.rs"]
mod metrics;
