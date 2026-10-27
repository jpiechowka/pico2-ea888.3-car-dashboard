//! Bounded observations; the lock covers timer reads and value updates, never
//! future polling, rendering or awaited peripheral transactions.
use core::cell::RefCell;
use core::future::Future;

use dashboard_core::discovery::{MemoryUse, ProbeDiagnostics};
use dashboard_core::stack_samples::StackSamples;
use dashboard_core::task_work::WorkWindow;
use dashboard_pico2::memory::{MemoryStats, stack_pointer};
use embassy_sync::blocking_mutex::Mutex;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_time::Instant;

/// Cross-core counters and sampled windows; all fields share one lock.
struct Observations {
    /// Wrapping successful HAL frame-transfer count.
    completed: u32,
    /// Per-core bounded wall-time polling-work windows, not DWT cycle counts.
    work: [WorkWindow; 2],
    /// Per-core admitted stack ranges and maximum observed samples.
    stacks: [Option<StackSamples>; 2],
    /// Total admitted stack-region bytes for each core.
    stack_bytes: [u32; 2],
    /// Latest RAM/stack observations; work/fps are resolved at snapshot use.
    values: ProbeDiagnostics,
}
impl Observations {
    /// Fills final `destination` diagnostics at boot-relative `at_us` and returns
    /// the paired completion count/time. Callers retain the observation lock;
    /// missing/future/stale work remains unavailable and old destination fields
    /// are fully replaced. Borrows work windows without copying accumulators.
    fn snapshot_into(
        &self,
        at_us: u64,
        destination: &mut ProbeDiagnostics,
    ) -> Snapshot {
        *destination = self.values;
        destination.work = [self.work[0].percent_tenths(at_us), self.work[1].percent_tenths(at_us)];
        Snapshot {
            at_us,
            completed: self.completed,
        }
    }
}
/// Short-lock shared telemetry; polling and rendering never occur inside it.
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

/// Initializes telemetry on core0 before task polling. `core1_bottom` and
/// `core1_bytes` describe the actual stack allocation; invalid/overflowing bounds
/// remain unavailable. Static RAM and core0 bounds come from linker symbols.
pub fn init(
    core1_bottom: u32,
    core1_bytes: u32,
) {
    // Core0 invokes this after HAL init and before either executor starts work.
    let ram = MemoryStats::collect();
    unsafe extern "C" {
        /// End of linked static main-SRAM allocations.
        static __sheap: u8;
        /// Linker-selected initial core0 stack top.
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
/// Returns the wrapping successful transfer count under the observation lock.
pub fn completed() -> u32 { OBSERVATIONS.lock(|cell| cell.borrow().completed) }

/// Coherent completion-count/time pair associated with in-place diagnostics.
pub struct Snapshot {
    /// Boot-relative microseconds sampled under the same lock as completed count.
    pub at_us: u64,
    /// Wrapping successful HAL transfer count at this snapshot time.
    pub completed: u32,
}
/// Fills final `destination` diagnostics and returns its completion count/time
/// pair under one lock. Resolves work freshness at the same timestamp without
/// treating missing/stale windows as zero; FPS is subsequently caller-derived.
pub fn snapshot_into(destination: &mut ProbeDiagnostics) -> Snapshot {
    OBSERVATIONS.lock(|cell| {
        let data = cell.borrow();
        // Counting and timestamping under the same lock excludes future completions.
        let at_us = Instant::now().as_micros();
        data.snapshot_into(at_us, destination)
    })
}

/// Returns a wrapper polling `future` and recording each poll duration/stack
/// sample for `core` (0 or 1). Propagates its output unchanged; suspended time is
/// excluded. Panics for an out-of-range core index when a poll is recorded.
/// Stack observations sample poll returns, not continuous physical high-water.
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

#[cfg(test)]
/// Deterministic projections use the same production lock and freshness method.
mod tests {
    use super::*;

    /// Returns isolated synthetic observations with one loaded and one idle core.
    fn observations() -> Observations {
        let mut work = [WorkWindow::new(); 2];
        work[0].record(0, 100_000);
        work[0].record(900_000, 1_000_000);
        work[1].record(0, 0);
        work[1].record(1_000_000, 1_000_000);
        Observations {
            completed: u32::MAX,
            work,
            stacks: [None, None],
            stack_bytes: [0; 2],
            values: ProbeDiagnostics {
                ram: Some(MemoryUse { used: 123, total: 456 }),
                ..ProbeDiagnostics::default()
            },
        }
    }

    /// Count/time and diagnostics come from one locked observation; replacement
    /// clears old FPS/stack data and preserves known zero separately from absent.
    #[test]
    fn snapshot_projection_is_coherent_and_replaces_unavailable_fields() {
        let lock = Mutex::<CriticalSectionRawMutex, _>::new(RefCell::new(observations()));
        let mut destination = ProbeDiagnostics {
            fps: Some(dashboard_core::display::telemetry::FrameRate {
                current: 1.0,
                average: 2.0,
            }),
            work: [Some(999); 2],
            ram: None,
            stacks: [Some(MemoryUse { used: 1, total: 2 }); 2],
        };
        let snapshot = lock.lock(|cell| cell.borrow().snapshot_into(1_000_000, &mut destination));
        assert_eq!((snapshot.at_us, snapshot.completed), (1_000_000, u32::MAX));
        assert_eq!(destination.work, [Some(200), Some(0)]);
        assert_eq!(destination.ram, Some(MemoryUse { used: 123, total: 456 }));
        assert_eq!(destination.stacks, [None; 2]);
        assert_eq!(destination.fps, None);
        lock.lock(|cell| {
            let mut data = cell.borrow_mut();
            data.completed = data.completed.wrapping_add(1);
            data.values.ram = None;
        });
        let snapshot = lock.lock(|cell| cell.borrow().snapshot_into(3_000_001, &mut destination));
        assert_eq!((snapshot.at_us, snapshot.completed), (3_000_001, 0));
        assert_eq!(destination.work, [None; 2]);
        assert_eq!(destination.ram, None);
    }

    /// Future, stale and reset work windows never project fabricated zero load.
    #[test]
    fn snapshot_projection_preserves_work_freshness_boundaries() {
        let mut data = observations();
        let mut destination = ProbeDiagnostics::default();
        for (at_us, expected) in [
            (999_999, [None; 2]),
            (1_000_000, [Some(200), Some(0)]),
            (3_000_000, [Some(200), Some(0)]),
            (3_000_001, [None; 2]),
        ] {
            data.snapshot_into(at_us, &mut destination);
            assert_eq!(destination.work, expected);
        }
        data.work[0].record(999_999, 1_000_001);
        data.snapshot_into(1_000_001, &mut destination);
        assert_eq!(destination.work, [None, Some(0)]);
    }
}
