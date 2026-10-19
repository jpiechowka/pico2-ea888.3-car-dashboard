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

static GATE: Mutex<CriticalSectionRawMutex, RefCell<Gate>> = Mutex::new(RefCell::new(Gate::with_radio(true)));
pub static FRAME_INHIBIT: AtomicBool = AtomicBool::new(false);
pub static UI_QUIET: AtomicBool = AtomicBool::new(false);
pub enum HealthRequest {
    Begin,
    End(MaintenanceTicket),
}
pub enum HealthReply {
    Begun(MaintenanceTicket),
    Ended,
}
pub static HEALTH_REQUEST: Channel<CriticalSectionRawMutex, HealthRequest, 1> = Channel::new();
static HEALTH_REPLY: Channel<CriticalSectionRawMutex, HealthReply, 1> = Channel::new();

pub fn reply(reply: HealthReply) {
    assert!(HEALTH_REPLY.try_send(reply).is_ok());
}
fn command() -> Option<Command> { GATE.lock(|g| g.borrow().command()) }
fn acknowledge(
    command: Command,
    owner: Owner,
    success: bool,
) {
    GATE.lock(|g| {
        g.borrow_mut().acknowledge(command, owner, success);
    });
}
pub fn writing(request: Request) -> bool {
    GATE.lock(|g| g.borrow_mut().writing(request, Instant::now().as_millis()).is_ok())
}

/// The UI owner invokes this before taking a framebuffer; radio only after its
/// complete PIO/DMA bus transaction; encoder only between complete I2C iterations.
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
pub async fn ui_boundary(last: &mut Option<Request>) {
    boundary(Owner::Acquisition, last, flash_guard::core1_ready).await;
}
pub async fn input_boundary(last: &mut Option<Request>) {
    boundary(Owner::Input, last, || {
        flash_guard::interrupt_ready() && flash_guard::i2c_idle()
    })
    .await;
}
pub async fn radio_boundary(last: &mut Option<Request>) {
    boundary(Owner::Radio, last, flash_guard::interrupt_ready).await;
}

#[derive(Debug)]
pub enum Error<E> {
    Drain,
    Gate(dashboard_core::maintenance::Error),
    Operation(E),
}

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
fn progress() -> [u32; 3] {
    [
        bridge::ENCODER_PROGRESS.load(Ordering::Relaxed),
        bridge::UI_PROGRESS.load(Ordering::Relaxed),
        bridge::INPUT_PROGRESS.load(Ordering::Relaxed),
    ]
}
