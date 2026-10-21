//! One storage owner, one bounded flash operation per ticket. Never cancel run().
use core::cell::RefCell;
use core::sync::atomic::{AtomicBool, Ordering};

use dashboard_core::health::MaintenanceTicket;
use dashboard_core::maintenance::{Command, Gate, Owner, Request};
use dashboard_pico2::flash_guard;
use embassy_sync::blocking_mutex::Mutex;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::Channel;
use embassy_time::{Instant, Timer};

use crate::bridge;

/// Shared generation/phase barrier requiring UI, encoder and radio acknowledgement.
static GATE: Mutex<CriticalSectionRawMutex, RefCell<Gate>> = Mutex::new(RefCell::new(Gate::with_radio(true)));
/// Stops core1 from admitting another frame while core0 drains queued transfers.
pub static FRAME_INHIBIT: AtomicBool = AtomicBool::new(false);
/// Core1 acknowledged observing frame inhibition before acquiring another buffer.
pub static UI_QUIET: AtomicBool = AtomicBool::new(false);
/// Sole storage owner's request to the watchdog supervisor.
pub enum HealthRequest {
    /// Grant a finite ticket after ordinary health/display checks.
    Begin,
    /// Return the unique ticket after all owners resume.
    End(
        /// Move-only supervisor grant retained across mutation and restoration.
        MaintenanceTicket,
    ),
}
/// Supervisor acknowledgement paired with a storage-owner request.
pub enum HealthReply {
    /// Exactly one entry feed occurred and mutation preparation may begin.
    Begun(
        /// Finite grant; feeds remain suspended while it is outstanding.
        MaintenanceTicket,
    ),
    /// Matching ticket ended and ordinary progress policy resumes.
    Ended,
}
/// One outstanding request; another requester violates exclusive storage ownership.
pub static HEALTH_REQUEST: Channel<CriticalSectionRawMutex, HealthRequest, 1> = Channel::new();
/// One acknowledgement awaiting the storage task's receive.
static HEALTH_REPLY: Channel<CriticalSectionRawMutex, HealthReply, 1> = Channel::new();

/// Publishes supervisor `reply`; panics if an earlier acknowledgement is unconsumed.
pub fn reply(reply: HealthReply) {
    assert!(HEALTH_REPLY.try_send(reply).is_ok());
}
/// Returns current Park/Resume generation or None outside the active barrier.
fn command() -> Option<Command> { GATE.lock(|g| g.borrow().command()) }
/// Records `success` for `owner` at exact `command` generation/phase; the shared
/// gate checks stale, duplicate and failed acknowledgements.
fn acknowledge(
    command: Command,
    owner: Owner,
    success: bool,
) {
    GATE.lock(|g| {
        g.borrow_mut().acknowledge(command, owner, success);
    });
}
/// Returns whether `request` still owns the write phase at current milliseconds;
/// invalid timing may latch a shared gate failure instead of authorizing mutation.
pub fn writing(request: Request) -> bool {
    GATE.lock(|g| g.borrow_mut().writing(request, Instant::now().as_millis()).is_ok())
}

/// The UI owner invokes this before taking a framebuffer; radio only after its
/// complete PIO/DMA bus transaction; encoder only between complete I2C iterations.
/// `owner` identifies the participant, `last` suppresses duplicate resumes, and
/// `ready` checks its live hardware/interrupt preconditions. Parks until Resume,
/// updates `last`, then acknowledges; do not cancel while the owner is parked.
async fn boundary(
    owner: Owner,
    last: &mut Option<Request>,
    ready: fn() -> bool,
) {
    let Some(current) = command() else {
        return;
    };
    let request = match current {
        Command::Park(r) | Command::Resume(r) => r,
    };
    if *last == Some(request) {
        return;
    }
    while command() == Some(Command::Park(request)) {
        let valid = Instant::now()
            .as_millis()
            .checked_sub(request.started_ms())
            .is_some_and(|elapsed| elapsed <= dashboard_core::maintenance::PREPARE_MS);
        if ready() {
            acknowledge(Command::Park(request), owner, valid);
        } else if !valid {
            acknowledge(Command::Park(request), owner, false);
        }
        Timer::after_millis(1).await;
    }
    *last = Some(request);
    acknowledge(Command::Resume(request), owner, true);
}
/// Parks the core1 UI using `last` to suppress repeats; requires the core1 FIFO
/// lockout IRQ to remain available. Call before taking a framebuffer.
pub async fn ui_boundary(last: &mut Option<Request>) {
    boundary(Owner::Acquisition, last, flash_guard::core1_ready).await;
}
/// Parks the encoder between full I2C iterations, tracking resumed requests in
/// `last`; admission requires unmasked IRQs and idle/empty I2C0.
pub async fn input_boundary(last: &mut Option<Request>) {
    boundary(Owner::Input, last, || {
        flash_guard::interrupt_ready() && flash_guard::i2c_idle()
    })
    .await;
}
/// Parks radio after a complete bus transaction, tracking generations in `last`;
/// its caller must retain bus ownership and supply only quiescent boundaries.
pub async fn radio_boundary(last: &mut Option<Request>) {
    boundary(Owner::Radio, last, flash_guard::interrupt_ready).await;
}

#[derive(Debug)]
/// Failed display drain, owner/deadline barrier, or synchronous flash operation.
pub enum Error<E> {
    /// Display/UI did not quiesce within the 400 ms preparation window.
    Drain,
    /// Shared phase, ownership or elapsed-time admission failure.
    Gate(
        /// Original barrier rejection, including post-operation expiry.
        dashboard_core::maintenance::Error,
    ),
    /// Caller-specific flash mutation failure.
    Operation(
        /// Unmodified operation error after entering the write phase.
        E,
    ),
}

/// Drains display, acquires one watchdog ticket, parks owners, and invokes
/// synchronous `operation` once with the admitted Request. Returns its result or
/// admission error only after restoration and fresh owner progress; no mutation
/// occurs on Drain. Post-write expiry supersedes an operation error. Never cancel:
/// resume failure holds the ticket and awaits watchdog reset without feeding.
/// Panics on overlapping admission or broken request/reply/generation invariants.
pub async fn run<E>(operation: impl FnOnce(Request) -> Result<(), E>) -> Result<(), Error<E>> {
    assert!(
        !FRAME_INHIBIT.swap(true, Ordering::SeqCst),
        "overlapping flash admission"
    );
    let start = Instant::now().as_millis();
    while !UI_QUIET.load(Ordering::SeqCst) || !bridge::READY.is_empty() || bridge::DISPLAY_ACTIVE.load(Ordering::SeqCst)
    {
        if Instant::now().as_millis().saturating_sub(start) > 400 {
            FRAME_INHIBIT.store(false, Ordering::SeqCst);
            return Err(Error::Drain);
        }
        Timer::after_millis(1).await;
    }
    assert!(HEALTH_REQUEST.try_send(HealthRequest::Begin).is_ok());
    let HealthReply::Begun(ticket) = HEALTH_REPLY.receive().await else {
        panic!("wrong maintenance grant")
    };
    let request = GATE
        .lock(|g| g.borrow_mut().begin(&ticket))
        .expect("maintenance admission");
    let result = loop {
        match GATE.lock(|g| g.borrow_mut().ready(Instant::now().as_millis())) {
            Ok(true) => {
                if let Err(e) = GATE.lock(|g| g.borrow_mut().begin_write(Instant::now().as_millis())) {
                    break Err(Error::Gate(e));
                }
                let result = operation(request).map_err(Error::Operation);
                if let Err(e) = GATE.lock(|g| g.borrow_mut().writing(request, Instant::now().as_millis())) {
                    break Err(Error::Gate(e));
                }
                break result;
            }
            Ok(false) => Timer::after_millis(1).await,
            Err(e) => break Err(Error::Gate(e)),
        }
    };
    GATE.lock(|g| g.borrow_mut().release(request))
        .expect("maintenance owner");
    loop {
        match GATE.lock(|g| g.borrow_mut().resumed(Instant::now().as_millis())) {
            Ok(true) => break,
            Ok(false) => Timer::after_millis(1).await,
            Err(e) => {
                defmt::error!(
                    "probe.flash resume failed {:?}; watchdog recovery",
                    defmt::Debug2Format(&e)
                );
                core::future::pending::<()>().await;
            }
        }
    }
    GATE.lock(|g| g.borrow_mut().finish(request, Instant::now().as_millis()))
        .expect("maintenance deadline");
    assert!(HEALTH_REQUEST.try_send(HealthRequest::End(ticket)).is_ok());
    assert!(matches!(HEALTH_REPLY.receive().await, HealthReply::Ended));
    let previous = progress();
    FRAME_INHIBIT.store(false, Ordering::SeqCst);
    // ProgressMonitor also enforces this. Wait for actual new useful owner work
    // before another journal page may request a watchdog exception.
    while progress().iter().zip(previous).any(|(now, old)| *now == old) {
        Timer::after_millis(1).await;
    }
    result
}
/// Returns independent wrapping encoder/UI/button liveness sequences; each must
/// change after restoration before another journal operation can request a ticket.
fn progress() -> [u32; 3] {
    [
        bridge::ENCODER_PROGRESS.load(Ordering::Relaxed),
        bridge::UI_PROGRESS.load(Ordering::Relaxed),
        bridge::INPUT_PROGRESS.load(Ordering::Relaxed),
    ]
}
