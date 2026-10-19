//! Bounded observations; the lock never covers polling, rendering or hardware I/O.
use core::cell::RefCell;
use core::future::Future;

use dashboard_core::discovery::{MemoryUse, ProbeDiagnostics};
use dashboard_core::stack_samples::StackSamples;
use dashboard_core::task_work::WorkWindow;
use dashboard_pico2::memory::{MemoryStats, stack_pointer};
use embassy_sync::blocking_mutex::Mutex;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_time::Instant;

struct Observations {
    completed: u32,
    work: [WorkWindow; 2],
    stacks: [Option<StackSamples>; 2],
    stack_bytes: [u32; 2],
    values: ProbeDiagnostics,
}
static OBSERVATIONS: Mutex<CriticalSectionRawMutex, RefCell<Observations>> = Mutex::new(RefCell::new(Observations {
    completed: 0,
    work: [WorkWindow::new(); 2],
    stacks: [None, None],
    stack_bytes: [0; 2],
    values: ProbeDiagnostics {
        fps: None,
        work: [None; 2],
        ram: None,
        stacks: [None; 2],
    },
}));

pub fn init(
    core1_bottom: u32,
    core1_bytes: u32,
) {
    // Core0 invokes this after HAL init and before either executor starts work.
    let ram = MemoryStats::collect();
    unsafe extern "C" {
        static __sheap: u8;
        static _stack_start: u8;
    }
    let bottom = core::ptr::addr_of!(__sheap) as u32;
    let top = core::ptr::addr_of!(_stack_start) as u32;
    OBSERVATIONS.lock(|cell| {
        let mut data = cell.borrow_mut();
        data.values.ram = Some(MemoryUse {
            used: ram.static_ram,
            total: ram.ram_total,
        });
        data.stacks[0] = StackSamples::new(bottom, top);
        data.stack_bytes[0] = top.saturating_sub(bottom);
        data.stacks[1] = core1_bottom
            .checked_add(core1_bytes)
            .and_then(|top| StackSamples::new(core1_bottom, top));
        data.stack_bytes[1] = core1_bytes;
    });
}

/// Only the display owner calls this, after successful HAL return and CS high.
pub fn frame_completed() {
    OBSERVATIONS.lock(|cell| {
        let mut data = cell.borrow_mut();
        data.completed = data.completed.wrapping_add(1);
    });
}
pub fn completed() -> u32 { OBSERVATIONS.lock(|cell| cell.borrow().completed) }

pub struct Snapshot {
    pub at_us: u64,
    pub completed: u32,
    pub values: ProbeDiagnostics,
}
pub fn snapshot() -> Snapshot {
    OBSERVATIONS.lock(|cell| {
        let data = cell.borrow();
        // Counting and timestamping under the same lock excludes future completions.
        let at_us = Instant::now().as_micros();
        let mut values = data.values;
        values.work = data.work.map(|work| work.percent_tenths(at_us));
        Snapshot {
            at_us,
            completed: data.completed,
            values,
        }
    })
}

pub fn measure<F: Future>(
    core: usize,
    future: F,
) -> impl Future<Output = F::Output> {
    dashboard_core::task_work::measure(
        future,
        || Instant::now().as_micros(),
        move |start, end| {
            let sp = stack_pointer();
            OBSERVATIONS.lock(|cell| {
                let mut data = cell.borrow_mut();
                data.work[core].record(start, end);
                data.values.stacks[core] = data.stacks[core].as_mut().and_then(|s| s.observe(sp)).map(|used| {
                    MemoryUse {
                        used,
                        total: data.stack_bytes[core],
                    }
                });
            });
        },
    )
}
