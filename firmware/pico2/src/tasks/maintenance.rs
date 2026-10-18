//! One main-loop initiator. Never cancel `run`: it retains the health permit
//! until every linked owner acknowledges resumption, including rejected saves.
use core::cell::RefCell;

use dashboard_pico2::maintenance::{Command, Gate, Owner, Request};
use embassy_sync::blocking_mutex::Mutex;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_time::{Instant, Timer};

static GATE: Mutex<CriticalSectionRawMutex, RefCell<Gate>> =
    Mutex::new(RefCell::new(Gate::with_radio(cfg!(feature = "ble-obd"))));

pub fn command() -> Option<Command> { GATE.lock(|gate| gate.borrow().command()) }
pub fn acknowledge(
    command: Command,
    owner: Owner,
    success: bool,
) {
    GATE.lock(|gate| {
        gate.borrow_mut().acknowledge(command, owner, success);
    });
}

pub fn interrupt_ready() -> bool {
    use cortex_m::peripheral::SCB;
    use cortex_m::peripheral::scb::VectActive;
    use cortex_m::register::{basepri, faultmask, primask};
    SCB::vect_active() == VectActive::ThreadMode
        && primask::read().is_active()
        && faultmask::read().is_active()
        && basepri::read() == 0
}

pub fn core1_ready() -> bool {
    embassy_rp::pac::SIO.cpuid().read() == 1
        && interrupt_ready()
        && cortex_m::peripheral::NVIC::is_enabled(embassy_rp::pac::Interrupt::SIO_IRQ_FIFO)
}

pub fn i2c_idle() -> bool {
    let status = embassy_rp::pac::I2C0.ic_status().read();
    !status.activity() && !status.mst_activity() && status.tfe() && !status.rfne()
}

/// Called only between complete encoder iterations. In particular an error
/// return need not imply STOP has finished. Observe idle without cancelling IO.
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
pub enum SaveError<E> {
    Preparation(dashboard_pico2::maintenance::Error),
    Operation(E),
}

pub fn writing(request: Request) -> bool {
    GATE.lock(|gate| gate.borrow_mut().writing(request, Instant::now().as_millis()).is_ok())
}

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
                crate::log_fault!("Resume: {:?}", error);
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
