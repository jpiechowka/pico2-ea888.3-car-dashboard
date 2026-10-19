use core::cell::Cell;

use dashboard_pico2::diagnostic_values::WorkObservation;
use embassy_executor::Executor;
use embassy_sync::blocking_mutex::Mutex;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use static_cell::StaticCell;

pub mod acquisition;
#[cfg(feature = "ble-obd")]
pub mod ble;
pub mod flush;
pub mod maintenance;
pub mod recovery;
#[cfg(feature = "te-trace")]
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

pub static EXECUTOR_CORE1: StaticCell<Executor> = StaticCell::new();
pub static CORE1_STACK: StaticCell<embassy_rp::multicore::Stack<{ CORE1_STACK_BYTES as usize }>> = StaticCell::new();
pub static CORE1_OBSERVATION: Mutex<CriticalSectionRawMutex, Cell<WorkObservation>> =
    Mutex::new(Cell::new(WorkObservation::EMPTY));
pub const CORE1_STACK_BYTES: u32 = dashboard_pico2::memory::CORE1_STACK_BYTES;
// Keep the linked frame-admission policy synchronized with the actual stack.
const _: () = assert!(CORE1_STACK_BYTES == 32768);
