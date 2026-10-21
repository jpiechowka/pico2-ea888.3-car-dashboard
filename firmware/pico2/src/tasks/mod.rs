//! Acquisition/display/maintenance owners and coherent cross-core observations.
use core::cell::Cell;

use dashboard_pico2::diagnostic_values::WorkObservation;
use embassy_executor::Executor;
use embassy_sync::blocking_mutex::Mutex;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use static_cell::StaticCell;

/// Core1 sample publication and source-specific acquisition state.
pub mod acquisition;
#[cfg(feature = "ble-obd")]
/// Optional radio/host ownership and adapter-identification prototype.
pub mod ble;
/// Affine framebuffer request/start/completion handoff.
pub mod flush;
/// Generation-checked park/write/resume coordinator.
pub mod maintenance;
/// Independent progress supervision and finite watchdog maintenance tickets.
pub mod recovery;
#[cfg(feature = "te-trace")]
/// Optional software-timestamped panel TE observations.
pub mod te;

pub use acquisition::{DASHBOARD_VALUES, acquisition_task};
pub use flush::{
    BUFFER_SWAPS,
    BUFFER_WAITS,
    COMPLETED_FRAMES,
    FLUSH_BUFFER_IDX,
    FLUSH_REQUESTS,
    display_flush_task,
    wait_completed,
    wait_started,
};

/// One-time allocation for the core1 thread-mode executor.
pub static EXECUTOR_CORE1: StaticCell<Executor> = StaticCell::new();
/// Fixed 32 KiB core1 stack, including the HAL stack-guard reservation.
pub static CORE1_STACK: StaticCell<embassy_rp::multicore::Stack<{ CORE1_STACK_BYTES as usize }>> = StaticCell::new();
/// Coherent work/cycle/time observation published under one cross-core lock.
pub static CORE1_OBSERVATION: Mutex<CriticalSectionRawMutex, Cell<WorkObservation>> =
    Mutex::new(Cell::new(WorkObservation::EMPTY));
/// Bytes in the actual core1 stack allocation and linked admission policy.
pub const CORE1_STACK_BYTES: u32 = dashboard_pico2::memory::CORE1_STACK_BYTES;
// Keep the linked frame-admission policy synchronized with the actual stack.
/// Compile-time agreement with the externally reviewed 32 KiB admission bound.
const _: () = assert!(CORE1_STACK_BYTES == 32768);
