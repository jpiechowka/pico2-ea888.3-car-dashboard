//! Multicore support
//!
//! Starts core1 on RP2040/RP235x Arm targets through the ROM FIFO handshake.
//! The stack and entry closure transfer permanently to core1; FIFO pause/resume requires the runtime IRQ.
//!
//! The entrypoint for core1 can be any function that never returns, including closures.
//!
//! Enable the `critical-section-impl` feature in embassy-rp when sharing data across cores using
//! the `embassy-sync` primitives and `CriticalSectionRawMutex`.
//!
//! Stack alignment follows [AAPCS32](https://github.com/ARM-software/abi-aa/blob/main/aapcs32/aapcs32.rst#621-the-stack).
//! ROM launch protocol: [Raspberry Pi SDK](https://github.com/raspberrypi/pico-sdk/blob/master/src/rp2_common/pico_multicore/multicore.c).
//!
//! # Usage
//!
//! ```no_run
//! use core::ptr::addr_of_mut;
//!
//! use embassy_executor::Executor;
//! use embassy_rp::multicore::Stack;
//! use static_cell::StaticCell;
//!
//! static mut CORE1_STACK: Stack<4096> = Stack::new();
//! static EXECUTOR0: StaticCell<Executor> = StaticCell::new();
//! static EXECUTOR1: StaticCell<Executor> = StaticCell::new();
//!
//! # // workaround weird error: `main` function not found in crate `rust_out`
//! # let _ = ();
//!
//! #[embassy_executor::task]
//! async fn core0_task() {
//!     // ...
//! }
//!
//! #[embassy_executor::task]
//! async fn core1_task() {
//!     // ...
//! }
//!
//! #[cortex_m_rt::entry]
//! fn main() -> ! {
//!     let p = embassy_rp::init(Default::default());
//!
//!     embassy_rp::multicore::spawn_core1(
//!         p.CORE1,
//!         unsafe { &mut *addr_of_mut!(CORE1_STACK) },
//!         move || {
//!             let executor1 = EXECUTOR1.init(Executor::new());
//!             executor1.run(|spawner| spawner.spawn(core1_task().unwrap()));
//!         },
//!     );
//!
//!     let executor0 = EXECUTOR0.init(Executor::new());
//!     executor0.run(|spawner| spawner.spawn(core0_task().unwrap()))
//! }
//! ```

use core::mem::ManuallyDrop;
use core::sync::atomic::{AtomicBool, Ordering, compiler_fence};

#[cfg(all(feature = "rt", any(feature = "rp2040", feature = "_rp235x")))]
use cortex_m::interrupt::InterruptNumber;
#[cfg(all(feature = "rt", any(feature = "rp2040", feature = "_rp235x")))]
use cortex_m::peripheral::NVIC;

use crate::interrupt::InterruptExt;
use crate::peripherals::CORE1;
use crate::{Peri, gpio, install_stack_guard, interrupt, pac};

/// Host-testable byte-to-word launch layout validation.
#[path = "multicore_stack.rs"]
mod stack_layout;

/// FIFO request/acknowledgment parking core1 with interrupts disabled.
const PAUSE_TOKEN: u32 = 0xDEADBEEF;
/// FIFO request/acknowledgment releasing the parked core1.
const RESUME_TOKEN: u32 = !0xDEADBEEF;
#[cfg(all(feature = "rt", any(feature = "rp2040", feature = "_rp235x")))]
/// Upper 16-bit FIFO tag; low 16 bits identify the interrupt to pend remotely.
pub(crate) const PEND_IRQ_TOKEN: u32 = 0xCAFE0000;

/// Release-published after core1 copies the closure; acquire gates pause/resume requests.
static IS_CORE1_INIT: AtomicBool = AtomicBool::new(false);

/// Represents a particular CPU core (SIO_CPUID)
#[derive(Debug, PartialEq, Eq, Clone, Copy, Hash)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[repr(u8)]
pub enum CoreId {
    /// Core 0
    Core0 = 0x0,
    /// Core 1
    Core1 = 0x1,
}

/// Returns the executing core identity from SIO CPUID (zero is core0).
pub fn current_core() -> CoreId {
    if pac::SIO.cpuid().read() == 0 {
        CoreId::Core0
    } else {
        CoreId::Core1
    }
}

/// Installs the core-local guard at `stack_bottom`, configures RP235x exclusive accesses and GPIO.
/// Traps if the RP2040 MPU is already configured.
///
/// # Safety
/// Call only on newly launched core1, with `stack_bottom` the permanent stack allocation base.
/// The compiler-generated frame must already fit before this Rust function installs the guard.
#[inline(always)]
unsafe fn core1_setup(stack_bottom: *mut usize) {
    if install_stack_guard(stack_bottom).is_err() {
        // currently only happens if the MPU was already set up, which
        // would indicate that the core is already in use from outside
        // embassy, somehow. trap if so since we can't deal with that.
        cortex_m::asm::udf();
    }

    #[cfg(feature = "_rp235x")]
    crate::enable_actlr_extexclall();

    unsafe {
        gpio::init();
    }
}

/// 32-byte-aligned permanent stack allocation with `SIZE` bytes of advertised capacity.
/// Launch rounds the top down to 8-byte alignment and reserves 8 bytes for argument words.
/// RP2040 MPU protection consumes the lowest 32 bytes; RP235x MSPLIM uses the exact bottom.
/// The creator must budget startup/closure, interrupts and application frames independently.
#[repr(C, align(32))]
pub struct Stack<const SIZE: usize> {
    /// Exclusive stack bytes; tail padding of the aligned struct is not included in capacity.
    pub mem: [u8; SIZE],
}

impl<const SIZE: usize> Stack<SIZE> {
    /// Returns a zero-filled `SIZE`-byte stack; launch validates its bookkeeping minimum separately.
    pub const fn new() -> Stack<SIZE> { Stack { mem: [0_u8; SIZE] } }
}

#[cfg(all(feature = "rt", any(feature = "rp2040", feature = "_rp235x")))]
#[derive(Clone, Copy)]
/// Runtime interrupt number decoded from a FIFO pend request.
struct Irq(
    /// NVIC interrupt number carried in the low 16 token bits.
    u16,
);
#[cfg(all(feature = "rt", any(feature = "rp2040", feature = "_rp235x")))]
unsafe impl InterruptNumber for Irq {
    /// Returns `self`'s decoded NVIC interrupt number.
    fn number(self) -> u16 { self.0 }
}

#[cfg(all(feature = "rt", feature = "rp2040"))]
#[interrupt]
/// Handles RP2040 pause/resume and remote pend tokens; pause blocks until the matching resume.
/// # Safety
/// runtime IRQ entry on the initialized core; FIFO ownership belongs to this protocol.
unsafe fn SIO_IRQ_PROC1() {
    let sio = pac::SIO;
    // Clear IRQ
    sio.fifo().st().write(|w| w.set_wof(false));

    while sio.fifo().st().read().vld() {
        let fifo_read = fifo_read_wfe();
        if fifo_read == PAUSE_TOKEN {
            // Pause CORE1 execution and disable interrupts
            cortex_m::interrupt::disable();
            // Signal to CORE0 that execution is paused
            fifo_write(PAUSE_TOKEN);
            // Wait for `resume` signal from CORE0
            while fifo_read_wfe() != RESUME_TOKEN {
                cortex_m::asm::nop();
            }
            cortex_m::interrupt::enable();
            // Signal to CORE0 that execution is resumed
            fifo_write(RESUME_TOKEN);
        } else if fifo_read & 0xFFFF0000 == PEND_IRQ_TOKEN {
            // Pend the IRQ to wake up interrupt executors.
            let irq = Irq((fifo_read & 0xFFFF) as u16);
            NVIC::pend(irq);
        }
    }
}

#[cfg(all(feature = "rt", feature = "_rp235x"))]
// The pause acknowledgement permits core0 to disable XIP. Both the generated
// interrupt trampoline and this body must stay in SRAM, including the parked
// FIFO/WFE loop. All callees in that interval are inline; inspect the linked
// optimized instructions when upgrading the compiler or HAL.
#[unsafe(link_section = ".data.ram_func")]
#[interrupt]
/// SRAM interrupt entry for RP235x FIFO lockout and remote interrupt requests.
/// # Safety
/// runtime IRQ entry; SRAM placement must include the generated trampoline.
unsafe fn SIO_IRQ_FIFO() { sio_fifo_ram(); }

// cortex-m-rt's interrupt macro rejects inline attributes. Use a normal private
// helper for the non-inlining boundary, with the trampoline in SRAM as well.
#[cfg(all(feature = "rt", feature = "_rp235x"))]
#[unsafe(link_section = ".data.ram_func")]
#[inline(never)]
/// Parks core1 in SRAM after pause acknowledgment, then reenables interrupts on resume.
/// # Safety
/// invoked only by its FIFO IRQ; no concurrent consumer may steal protocol tokens.
unsafe fn sio_fifo_ram() {
    let sio = pac::SIO;
    // Clear IRQ
    sio.fifo().st().write(|w| w.set_wof(false));

    while sio.fifo().st().read().vld() {
        let fifo_read = fifo_read_wfe();
        if fifo_read == PAUSE_TOKEN {
            // Pause CORE1 execution and disable interrupts
            cortex_m::interrupt::disable();
            // Signal to CORE0 that execution is paused
            fifo_write(PAUSE_TOKEN);
            // Wait for `resume` signal from CORE0
            while fifo_read_wfe() != RESUME_TOKEN {
                cortex_m::asm::nop();
            }
            cortex_m::interrupt::enable();
            // Signal to CORE0 that execution is resumed
            fifo_write(RESUME_TOKEN);
        } else if fifo_read & 0xFFFF0000 == PEND_IRQ_TOKEN {
            // Pend the IRQ to wake up interrupt executors.
            let irq = Irq((fifo_read & 0xFFFF) as u16);
            let mut nvic: NVIC = core::mem::transmute(());
            nvic.request(irq);
        }
    }
}

/// Transfers `entry` to core1 using exclusive `_core1` and permanent `stack` storage.
/// Returns only after core1 copies the closure; `entry` must never return. FIFO launch waits
/// have no deadline; more than 16 mismatched responses panic. Stack sizes below 8 bytes panic
/// before core reset or pointer arithmetic. The usable top is rounded down to 8-byte alignment.
///
/// Eight bytes cover only launch argument writes. Budget the startup prologue, captured `entry`,
/// interrupt frames and application calls separately: the guard is installed after Rust startup
/// begins, and cannot protect its preceding compiler-generated prologue. Do not share the FIFO
/// with another protocol. Enable the runtime FIFO IRQ for flash pause/resume coordination.
pub fn spawn_core1<F, const SIZE: usize>(
    _core1: Peri<'static, CORE1>,
    stack: &'static mut Stack<SIZE>,
    entry: F,
) where
    F: FnOnce() -> bad::Never + Send + 'static,
{
    // The first two ignored `u64` parameters are there to take up all of the registers,
    // which means that the rest of the arguments are taken from the stack,
    // where we're able to put them from core 0.
    /// Takes the closure through `entry`, installs the guard at `stack_bottom`, acknowledges
    /// ownership transfer and invokes the closure; never returns. Two ignored ABI arguments occupy
    /// r0-r3 so the pointers are read from the launch stack. Raw pointers come only from spawn.
    extern "C" fn core1_startup<F: FnOnce() -> bad::Never>(
        _: u64,
        _: u64,
        entry: *mut ManuallyDrop<F>,
        stack_bottom: *mut usize,
    ) -> ! {
        unsafe { core1_setup(stack_bottom) };

        let entry = unsafe { ManuallyDrop::take(&mut *entry) };

        // make sure the preceding read doesn't get reordered past the following fifo write
        compiler_fence(Ordering::SeqCst);

        // Signal that it's safe for core 0 to get rid of the original value now.
        fifo_write(1);

        IS_CORE1_INIT.store(true, Ordering::Release);
        // Enable fifo interrupt on CORE1 for `pause` functionality.
        #[cfg(feature = "rp2040")]
        unsafe {
            interrupt::SIO_IRQ_PROC1.enable()
        };
        #[cfg(feature = "_rp235x")]
        unsafe {
            interrupt::SIO_IRQ_FIFO.enable()
        };

        // Enable FPU
        #[cfg(all(feature = "_rp235x", has_fpu))]
        unsafe {
            let p = cortex_m::Peripherals::steal();
            p.SCB.cpacr.modify(|cpacr| cpacr | (3 << 20) | (3 << 22));
        }

        entry()
    }

    // Reject invalid bookkeeping before resetting core1 or forming any stack pointers.
    let stack_words = stack_layout::stack_words(stack.mem.len());

    // Reset the core
    let psm = pac::PSM;
    psm.frce_off().modify(|w| w.set_proc1(true));
    while !psm.frce_off().read().proc1() {
        cortex_m::asm::nop();
    }
    psm.frce_off().modify(|w| w.set_proc1(false));

    // The ARM AAPCS ABI requires 8-byte stack alignment.
    // #[align] on `struct Stack` ensures the bottom is aligned, but the top could still be
    // unaligned if the user chooses a stack size that's not multiple of 8.
    // So, we round down to the next multiple of 8.
    let mem = unsafe { core::slice::from_raw_parts_mut(stack.mem.as_mut_ptr() as *mut usize, stack_words) };

    // Set up the stack
    let mut stack_ptr = unsafe { mem.as_mut_ptr().add(mem.len()) };

    // We don't want to drop this, since it's getting moved to the other core.
    let mut entry = ManuallyDrop::new(entry);

    // Push the arguments to `core1_startup` onto the stack.
    unsafe {
        // Push `stack_bottom`.
        stack_ptr = stack_ptr.sub(1);
        stack_ptr.cast::<*mut usize>().write(mem.as_mut_ptr());

        // Push `entry`.
        stack_ptr = stack_ptr.sub(1);
        stack_ptr.cast::<*mut ManuallyDrop<F>>().write(&mut entry);
    }

    // Make sure the compiler does not reorder the stack writes after to after the
    // below FIFO writes, which would result in them not being seen by the second
    // core.
    //
    // From the compiler perspective, this doesn't guarantee that the second core
    // actually sees those writes. However, we know that the RP2040 doesn't have
    // memory caches, and writes happen in-order.
    compiler_fence(Ordering::Release);

    let p = unsafe { cortex_m::Peripherals::steal() };
    let vector_table = p.SCB.vtor.read();

    // After reset, core 1 is waiting to receive commands over FIFO.
    // This is the sequence to have it jump to some code.
    let cmd_seq = [
        0,
        0,
        1,
        vector_table as usize,
        stack_ptr as usize,
        core1_startup::<F> as *const () as usize,
    ];

    let mut seq = 0;
    let mut fails = 0;
    loop {
        let cmd = cmd_seq[seq] as u32;
        if cmd == 0 {
            fifo_drain();
            cortex_m::asm::sev();
        }
        fifo_write(cmd);

        let response = fifo_read();
        if cmd == response {
            seq += 1;
        } else {
            seq = 0;
            fails += 1;
            if fails > 16 {
                // The second core isn't responding, and isn't going to take the entrypoint
                panic!("CORE1 not responding");
            }
        }
        if seq >= cmd_seq.len() {
            break;
        }
    }

    // Wait until the other core has copied `entry` before returning.
    fifo_read();

    // Enable fifo interrupt on CORE0 for `pend irq` functionality.
    #[cfg(all(feature = "rp2040", feature = "executor-interrupt"))]
    unsafe {
        interrupt::SIO_IRQ_PROC1.enable()
    };
    #[cfg(all(feature = "_rp235x", feature = "executor-interrupt"))]
    unsafe {
        interrupt::SIO_IRQ_FIFO.enable()
    };
}

/// Requests core1 park and waits for its acknowledgment; no-op before initialization.
/// No timeout: core1 must service the FIFO IRQ and callers must serialize pause/resume pairs.
/// RP235x acknowledgment comes from SRAM code, allowing the caller to suspend XIP afterward.
pub fn pause_core1() {
    if IS_CORE1_INIT.load(Ordering::Acquire) {
        fifo_write(PAUSE_TOKEN);
        // Wait for CORE1 to signal it has paused execution.
        while fifo_read() != PAUSE_TOKEN {}
    }
}

/// Releases a previously parked core1 and waits for its acknowledgment; no-op before init.
/// Calling without a matching pause can wait forever because the normal IRQ ignores resume tokens.
pub fn resume_core1() {
    if IS_CORE1_INIT.load(Ordering::Acquire) {
        fifo_write(RESUME_TOKEN);
        // Wait for CORE1 to signal it has resumed execution.
        while fifo_read() != RESUME_TOKEN {}
    }
}

/// Sends `value` to the other core and emits SEV; spins without timeout while FIFO is full.
#[inline(always)]
pub(crate) fn fifo_write(value: u32) {
    let sio = pac::SIO;
    // Wait for the FIFO to have enough space
    while !sio.fifo().st().read().rdy() {
        cortex_m::asm::nop();
    }
    sio.fifo().wr().write_value(value);
    // Fire off an event to the other core.
    // This is required as the other core may be `wfe` (waiting for event)
    cortex_m::asm::sev();
}

/// Returns the next FIFO word, spinning without timeout until one is available.
#[inline(always)]
fn fifo_read() -> u32 {
    let sio = pac::SIO;
    // Wait until FIFO has data
    while !sio.fifo().st().read().vld() {
        cortex_m::asm::nop();
    }
    sio.fifo().rd().read()
}

/// Returns the next FIFO word, sleeping with WFE while empty; requires peer event signaling.
#[inline(always)]
#[allow(unused)]
fn fifo_read_wfe() -> u32 {
    let sio = pac::SIO;
    // Wait until FIFO has data
    while !sio.fifo().st().read().vld() {
        cortex_m::asm::wfe();
    }
    sio.fifo().rd().read()
}

/// Discards all currently readable FIFO words; concurrent arrivals may extend the loop.
#[inline(always)]
fn fifo_drain() {
    let sio = pac::SIO;
    while sio.fifo().st().read().vld() {
        let _ = sio.fifo().rd().read();
    }
}

/// Projects the never type through a function output to express the stable closure bound.
/// Pattern: <https://github.com/nvzqz/bad-rs/blob/master/src/never.rs>.
mod bad {
    /// Uninhabited output required from the core1 entry closure.
    pub(crate) type Never = <F as HasOutput>::Output;

    /// Extracts a function pointer's output type.
    pub trait HasOutput {
        /// Return type selected by the implementing function pointer.
        type Output;
    }

    impl<O> HasOutput for fn() -> O {
        /// The function pointer's exact output type.
        type Output = O;
    }

    /// Nonreturning function pointer used solely for associated-type projection.
    type F = fn() -> !;
}
