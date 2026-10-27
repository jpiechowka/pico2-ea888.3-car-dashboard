//! One main-loop initiator. Never cancel `run`: it retains the health permit
//! until every linked owner acknowledges resumption, including rejected saves.
use core::cell::RefCell;

use dashboard_pico2::maintenance::{Command, Gate, Owner, Request};
use embassy_sync::blocking_mutex::Mutex;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_time::{Instant, Timer};

/// Cross-core generation/phase barrier; radio participates only in BLE builds.
static GATE: Mutex<CriticalSectionRawMutex, RefCell<Gate>> =
    Mutex::new(RefCell::new(Gate::with_radio(cfg!(feature = "ble-obd"))));

/// Returns the current Park/Resume command, or `None` outside an active request.
pub fn command() -> Option<Command> { GATE.lock(|gate| gate.borrow().command()) }
/// Records `owner`'s `success` for the exact `command` generation and phase.
/// The shared gate rejects stale/duplicate acknowledgement as defined by its state.
pub fn acknowledge(
    command: Command,
    owner: Owner,
    success: bool,
) {
    GATE.lock(|gate| {
        gate.borrow_mut().acknowledge(command, owner, success);
    });
}

/// Returns whether this core is in thread mode with all interrupt masks clear.
pub fn interrupt_ready() -> bool { dashboard_pico2::flash_guard::interrupt_ready() }

/// Returns whether core1 can receive its secure FIFO lockout interrupt.
pub fn core1_ready() -> bool { dashboard_pico2::flash_guard::core1_ready() }

/// Returns whether I2C0 has no active transaction or buffered RX/TX data.
pub fn i2c_idle() -> bool { dashboard_pico2::flash_guard::i2c_idle() }

/// Called only between complete encoder iterations. In particular an error
/// return need not imply STOP has finished. Observe idle without cancelling IO.
/// `last_resume` suppresses duplicate handling of a completed request and is
/// updated before Resume acknowledgement. Returns `true` after processing a
/// maintenance request (caller must reset input state), or `false` if none is new.
/// Do not cancel while parked: the initiator waits for the matching owner reply.
pub async fn input_boundary(last_resume: &mut Option<Request>) -> bool {
    let Some(current) = command() else {
        return false;
    };
    let request = match current {
        Command::Park(r) | Command::Resume(r) => r,
    };
    if *last_resume == Some(request) {
        return false;
    }
    while command() == Some(Command::Park(request)) {
        let now = Instant::now().as_millis();
        let valid_time = now
            .checked_sub(request.started_ms())
            .is_some_and(|ms| ms <= dashboard_pico2::maintenance::PREPARE_MS);
        if i2c_idle() && interrupt_ready() {
            acknowledge(Command::Park(request), Owner::Input, valid_time);
        } else if !valid_time {
            acknowledge(Command::Park(request), Owner::Input, false);
        }
        Timer::after_millis(1).await;
    }
    // No I2C operation was cancelled or started while the owner was parked.
    *last_resume = Some(request);
    acknowledge(Command::Resume(request), Owner::Input, true);
    true
}

#[derive(Debug)]
/// Preparation/deadline failure or the journal operation's own failure.
pub enum SaveError<E> {
    /// Barrier, phase, or elapsed-time checks prevented a valid operation.
    Preparation(
        /// Shared gate rejection, including post-operation expiry.
        dashboard_pico2::maintenance::Error,
    ),
    /// The operation ran under its permit and returned an error.
    Operation(
        /// Unmodified caller-specific journal or hardware failure.
        E,
    ),
}

/// Returns whether `request` still owns the writing phase at the current
/// boot-relative millisecond timestamp; the shared gate latches invalid timing.
pub fn writing(request: Request) -> bool {
    GATE.lock(|gate| gate.borrow_mut().writing(request, Instant::now().as_millis()).is_ok())
}

/// Parks all owners, invokes synchronous `operation` once with the admitted
/// request, then waits for every Resume acknowledgement before returning its
/// result. Preparation/elapsed-time errors supersede an operation error.
/// Never cancel this future: it retains the watchdog ticket through restoration.
/// Resume failure waits forever without feeding; protocol violations panic.
pub async fn run<E>(operation: impl FnOnce(Request) -> Result<(), E>) -> Result<(), SaveError<E>> {
    let ticket = super::recovery::begin_maintenance().await;
    let request = GATE
        .lock(|gate| gate.borrow_mut().begin(&ticket))
        .expect("overlapping maintenance");
    let result = loop {
        match GATE.lock(|gate| gate.borrow_mut().ready(Instant::now().as_millis())) {
            Ok(true) => {
                if let Err(error) = GATE.lock(|gate| gate.borrow_mut().begin_write(Instant::now().as_millis())) {
                    break Err(SaveError::Preparation(error));
                }
                let result = operation(request).map_err(SaveError::Operation);
                // A synchronous operation may return after its time budget.
                // Latch this even if it returned an ordinary storage error.
                if let Err(error) = GATE.lock(|gate| gate.borrow_mut().writing(request, Instant::now().as_millis())) {
                    break Err(SaveError::Preparation(error));
                }
                break result;
            }
            Ok(false) => Timer::after_millis(1).await,
            Err(error) => break Err(SaveError::Preparation(error)),
        }
    };
    // Mark after the last synchronous operation and before yielding: the TE
    // waiter may already have a pending edge from the IRQ-masked interval.
    #[cfg(feature = "te-trace")]
    super::te::mark_maintenance_gap();
    GATE.lock(|gate| gate.borrow_mut().release(request))
        .expect("lost maintenance owner");
    loop {
        match GATE.lock(|gate| gate.borrow_mut().resumed(Instant::now().as_millis())) {
            Ok(true) => break,
            Ok(false) => Timer::after_millis(1).await,
            Err(error) => {
                crate::log_fault!("Resume: {}", error.label());
                // Retain the permit. No new feed; the hardware watchdog is the
                // bound when an owner cannot resume or the total budget expires.
                core::future::pending::<()>().await;
            }
        }
    }
    GATE.lock(|gate| gate.borrow_mut().finish(request, Instant::now().as_millis()))
        .expect("expired maintenance");
    super::recovery::end_maintenance(ticket).await;
    result
}
