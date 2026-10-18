#![no_std]

pub use dashboard_core::*;
pub mod clock_profile;
#[path = "drivers/st7789.rs"]
pub mod panel;
#[path = "drivers/watchdog.rs"]
pub mod recovery_watchdog;
pub mod seesaw;

mod profiling {
    pub mod cpu_cycles;
    pub mod memory;
}
pub use profiling::{cpu_cycles, memory};
