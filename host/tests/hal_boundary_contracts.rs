//! Execute the vendored HAL's production arithmetic/admission helpers on the host.
//! These regressions cover bounds and callback ordering; they do not execute registers,
//! assembly, core launch or flash/XIP maintenance on physical hardware.

#[path = "../../firmware/vendor/embassy-rp/src/executor_start.rs"]
/// Interrupt executor publication ordering and duplicate/panicking initialization.
mod executor_start;
#[path = "../../firmware/vendor/embassy-rp/src/flash_read.rs"]
/// Empty and oversized RP2040 unique-ID reads are rejected before hardware access.
mod flash_read;
#[path = "../../firmware/vendor/embassy-rp/src/multicore_stack.rs"]
/// Core1 launch bookkeeping stays inside the caller's supplied allocation.
mod multicore_stack;
#[path = "../../firmware/vendor/embassy-rp/src/pio/batch.rs"]
/// Repeated PIO enable/disable requests preserve the latest value and unselected bits.
mod pio_batch;
#[path = "../../firmware/vendor/embassy-rp/src/spi_divider.rs"]
/// SPI prescaler arithmetic across valid profiles, zero and integer limits.
mod spi_divider;
