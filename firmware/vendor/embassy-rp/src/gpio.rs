//! Owned GPIO pins, digital levels and one interrupt waiter per pin. Input reads
//! observe SIO sampling; output queries report requested latch/direction state,
//! not electrical voltage. Pin drop disconnects the mux but retains SIO latches.
//! Register semantics: <https://datasheets.raspberrypi.com/rp2040/rp2040-datasheet.pdf>
//! section 2.19; RP235x package features select 30 or 48 bank-0 pins.
#![macro_use]
use core::convert::Infallible;
use core::future::Future;
use core::pin::Pin as FuturePin;
use core::task::{Context, Poll};

use embassy_hal_internal::{Peri, PeripheralType, impl_peripheral};
use embassy_sync::waitqueue::AtomicWaker;

use crate::interrupt::InterruptExt;
use crate::pac::SIO;
use crate::pac::common::{RW, Reg};
use crate::{RegExt, interrupt, pac, peripherals};

#[cfg(any(feature = "rp2040", feature = "rp235xa"))]
/// Number of user-bank pins on the selected package.
pub(crate) const BANK0_PIN_COUNT: usize = 30;
#[cfg(feature = "rp235xb")]
/// Number of user-bank pins on the selected package.
pub(crate) const BANK0_PIN_COUNT: usize = 48;

/// One IRQ waiter per user-bank pin; exclusive pin borrowing prevents concurrent waits.
static BANK0_WAKERS: [AtomicWaker; BANK0_PIN_COUNT] = [const { AtomicWaker::new() }; BANK0_PIN_COUNT];
#[cfg(feature = "qspi-as-gpio")]
/// Number of external flash-interface pads exposed by qspi-as-gpio.
const QSPI_PIN_COUNT: usize = 6;
#[cfg(feature = "qspi-as-gpio")]
/// One IRQ waiter per QSPI pad when those pins are available for GPIO.
static QSPI_WAKERS: [AtomicWaker; QSPI_PIN_COUNT] = [const { AtomicWaker::new() }; QSPI_PIN_COUNT];

/// Represents a digital input or output level.
#[derive(Debug, Eq, PartialEq, Clone, Copy)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum Level {
    /// Logical low.
    Low,
    /// Logical high.
    High,
}

impl From<bool> for Level {
    /// Returns High for `val` true and Low for false.
    fn from(val: bool) -> Self {
        match val {
            true => Self::High,
            false => Self::Low,
        }
    }
}

impl From<Level> for bool {
    /// Returns true for High `level`, false for Low.
    fn from(level: Level) -> bool {
        match level {
            Level::Low => false,
            Level::High => true,
        }
    }
}

/// Represents a pull setting for an input.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum Pull {
    /// No pull.
    None,
    /// Internal pull-up resistor.
    Up,
    /// Internal pull-down resistor.
    Down,
}

/// Drive strength of an output
#[derive(Debug, Eq, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum Drive {
    /// 2 mA drive.
    _2mA,
    /// 4 mA drive.
    _4mA,
    /// 8 mA drive.
    _8mA,
    /// 12 mA drive.
    _12mA,
}
/// Slew rate of an output
#[derive(Debug, Eq, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum SlewRate {
    /// Fast slew rate.
    Fast,
    /// Slow slew rate.
    Slow,
}

/// GPIO register bank; bank 0 contains up to 48 pins on RP235xB.
#[derive(Debug, Eq, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum Bank {
    /// Bank 0.
    Bank0 = 0,
    /// QSPI.
    #[cfg(feature = "qspi-as-gpio")]
    Qspi = 1,
}

/// Dormant mode config.
#[derive(Debug, Eq, PartialEq, Copy, Clone, Default)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct DormantWakeConfig {
    /// Wake on edge high.
    pub edge_high: bool,
    /// Wake on edge low.
    pub edge_low: bool,
    /// Wake on level high.
    pub level_high: bool,
    /// Wake on level low.
    pub level_low: bool,
}

/// GPIO input driver.
#[derive(Debug)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct Input<'d> {
    /// Exclusive pad ownership and shared configuration/cleanup implementation.
    pin: Flex<'d>,
}

impl<'d> Input<'d> {
    /// Returns an input owning `pin`, with output disabled and resistor selection `pull`.
    /// Reinitializes pad/mux configuration; does not preserve its previous settings.
    #[inline]
    pub fn new(
        pin: Peri<'d, impl Pin>,
        pull: Pull,
    ) -> Self {
        let mut pin = Flex::new(pin);
        pin.set_as_input();
        pin.set_pull(pull);
        Self { pin }
    }

    /// Sets input Schmitt hysteresis to `enable`, preserving other pad settings.
    #[inline]
    pub fn set_schmitt(
        &mut self,
        enable: bool,
    ) {
        self.pin.set_schmitt(enable)
    }

    /// Returns whether the sampled SIO input is high, after configured input inversion.
    #[inline]
    pub fn is_high(&self) -> bool { self.pin.is_high() }

    /// Returns whether the sampled SIO input is low, after configured input inversion.
    #[inline]
    pub fn is_low(&self) -> bool { self.pin.is_low() }

    /// Returns the sampled SIO input level, after configured input inversion.
    #[inline]
    pub fn get_level(&self) -> Level { self.pin.get_level() }

    /// Sets input inversion to `invert`; does not change the separate IRQ override.
    #[inline]
    pub fn set_inversion(
        &mut self,
        invert: bool,
    ) {
        self.pin.set_input_inversion(invert)
    }

    /// Waits for the level-high IRQ condition; an already-high input needs no new edge.
    /// Completes after IRQ handling, with no timeout. Cancellation disarms this pin's wait.
    #[inline]
    pub async fn wait_for_high(&mut self) { self.pin.wait_for_high().await; }

    /// Waits for the level-low IRQ condition; an already-low input needs no new edge.
    /// Completes after IRQ handling, with no timeout. Cancellation disarms this pin's wait.
    #[inline]
    pub async fn wait_for_low(&mut self) { self.pin.wait_for_low().await; }

    /// Waits for low-to-high edge after arming, discarding previously latched edges.
    /// Edges during setup may be missed; no timeout/debounce. Cancellation disarms this pin.
    #[inline]
    pub async fn wait_for_rising_edge(&mut self) { self.pin.wait_for_rising_edge().await; }

    /// Waits for high-to-low edge after arming, discarding previously latched edges.
    /// Edges during setup may be missed; no timeout/debounce. Cancellation disarms this pin.
    #[inline]
    pub async fn wait_for_falling_edge(&mut self) { self.pin.wait_for_falling_edge().await; }

    /// Waits for either edge after arming, discarding previously latched edges.
    /// Edges during setup may be missed; no timeout/debounce. Cancellation disarms this pin.
    #[inline]
    pub async fn wait_for_any_edge(&mut self) { self.pin.wait_for_any_edge().await; }

    /// Arms conditions from `cfg` and returns a borrowed guard that disables all this
    /// pin's dormant-wake conditions on drop; clears selected stale edge flags first.
    /// Does not enter dormant mode or preserve a previous wake configuration.
    #[inline]
    pub fn dormant_wake(
        &mut self,
        cfg: DormantWakeConfig,
    ) -> DormantWake<'_> {
        self.pin.dormant_wake(cfg)
    }

    /// Sets electrical pad isolation to `isolate`; other pad and SIO configuration is retained.
    #[cfg(feature = "_rp235x")]
    #[inline]
    pub fn set_pad_isolation(
        &mut self,
        isolate: bool,
    ) {
        self.pin.set_pad_isolation(isolate)
    }
}

/// Interrupt trigger levels.
#[derive(Debug, Eq, PartialEq, Copy, Clone)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum InterruptTrigger {
    /// Trigger on pin low.
    LevelLow,
    /// Trigger on pin high.
    LevelHigh,
    /// Trigger on high to low transition.
    EdgeLow,
    /// Trigger on low to high transition.
    EdgeHigh,
    /// Trigger on any transition.
    AnyEdge,
}

/// Enables GPIO bank IRQs at priority P3 on the calling core.
/// # Safety
/// Call during exclusive HAL initialization with the matching handlers installed.
pub(crate) unsafe fn init() {
    interrupt::IO_IRQ_BANK0.disable();
    interrupt::IO_IRQ_BANK0.set_priority(interrupt::Priority::P3);
    interrupt::IO_IRQ_BANK0.enable();

    #[cfg(feature = "qspi-as-gpio")]
    {
        interrupt::IO_IRQ_QSPI.disable();
        interrupt::IO_IRQ_QSPI.set_priority(interrupt::Priority::P3);
        interrupt::IO_IRQ_QSPI.enable();
    }
}

#[cfg(feature = "rt")]
/// Scans `bank` pending events on the current core, disarms each triggered pin and
/// wakes its entry in `wakers`; `N` must match the bank's implemented pin count.
fn irq_handler<const N: usize>(
    bank: pac::io::Io,
    wakers: &[AtomicWaker; N],
) {
    let cpu = SIO.cpuid().read() as usize;
    // There are two sets of interrupt registers, one for cpu0 and one for cpu1
    // and here we are selecting the set that belongs to the currently executing
    // cpu.
    let proc_intx: pac::io::Int = bank.int_proc(cpu);
    for pin in 0..N {
        // There are 4 raw interrupt status registers, PROCx_INTS0, PROCx_INTS1,
        // PROCx_INTS2, and PROCx_INTS3, and we are selecting the one that the
        // current pin belongs to.
        let intsx = proc_intx.ints(pin / 8);
        // The status register is divided into groups of four, one group for
        // each pin. Each group consists of four trigger levels LEVEL_LOW,
        // LEVEL_HIGH, EDGE_LOW, and EDGE_HIGH for each pin.
        let pin_group = pin % 8;
        let event = (intsx.read().0 >> (pin_group * 4)) & 0xF;

        // no more than one event can be awaited per pin at any given time, so
        // we can just clear all interrupt enables for that pin without having
        // to check which event was signalled.
        if event != 0 {
            disarm_wait(proc_intx, pin);
            wakers[pin].wake();
        }
    }
}

#[cfg(feature = "rt")]
#[interrupt]
/// Dispatches current-core user-bank GPIO events to their registered waiters.
fn IO_IRQ_BANK0() { irq_handler(pac::IO_BANK0, &BANK0_WAKERS); }

#[cfg(all(feature = "rt", feature = "qspi-as-gpio"))]
#[interrupt]
/// Dispatches current-core QSPI GPIO events to their registered waiters.
fn IO_IRQ_QSPI() { irq_handler(pac::IO_QSPI, &QSPI_WAKERS); }

/// Armed GPIO wait tied to the CPU interrupt bank that created it; drop disarms.
#[must_use = "futures do nothing unless you `.await` or poll them"]
struct InputFuture<'d> {
    /// Exclusive erased pin token retained for this driver or wait.
    pin: Peri<'d, AnyPin>,
    /// Original CPU interrupt register bank, retained if the future is polled elsewhere.
    interrupts: pac::io::Int,
}

/// Clears all four enable bits for bank-local `pin` in `interrupts`, preserving adjacent pins.
fn disarm_wait(
    interrupts: pac::io::Int,
    pin: usize,
) {
    interrupts.inte(pin / 8).write_clear(|w| {
        w.set_edge_high(pin % 8, true);
        w.set_edge_low(pin % 8, true);
        w.set_level_high(pin % 8, true);
        w.set_level_low(pin % 8, true);
    });
}

impl Drop for InputFuture<'_> {
    /// Disarms the original CPU's pin wait on completion or cancellation; stale waker may remain.
    fn drop(&mut self) { disarm_wait(self.interrupts, self.pin.pin() as usize); }
}

/// In-memory register regressions for cancellation and original-bank polling.
#[cfg(test)]
mod wait_tests {
    use super::*;

    /// Checks cancellation clears only the owned four-bit interrupt-enable group.
    #[test]
    fn cancellation_disarms_only_the_owned_pin() {
        let mut registers = [0u32; 4096];
        let interrupts = unsafe { pac::io::Int::from_ptr(registers.as_mut_ptr().cast()) };
        let wait = InputFuture {
            pin: unsafe { AnyPin::steal(10) },
            interrupts,
        };
        drop(wait);
        let clear_mask = registers[(0x3000 + 4) / 4];
        assert_eq!(clear_mask, 0xF << 8);
        assert_eq!(registers[0x3000 / 4], 0);
    }

    /// Checks readiness uses the saved bank and ignores neighboring enabled pins.
    #[test]
    fn wait_observes_its_original_interrupt_bank() {
        let mut registers = [0u32; 4096];
        let interrupts = unsafe { pac::io::Int::from_ptr(registers.as_mut_ptr().cast()) };
        let mut wait = InputFuture {
            pin: unsafe { AnyPin::steal(10) },
            interrupts,
        };
        let mut cx = Context::from_waker(core::task::Waker::noop());
        interrupts.inte(1).write_value(pac::io::regs::Int(1 << 11));
        assert!(FuturePin::new(&mut wait).poll(&mut cx).is_pending());
        // An adjacent pin does not keep our wait pending.
        interrupts.inte(1).write_value(pac::io::regs::Int(1 << 12));
        assert!(FuturePin::new(&mut wait).poll(&mut cx).is_ready());
    }
}

impl<'d> InputFuture<'d> {
    /// Returns an armed wait for `level` on `pin`; clears stale edges and retains
    /// the calling CPU's interrupt bank. IRQ delivery must remain enabled on that CPU.
    fn new(
        pin: Peri<'d, AnyPin>,
        level: InterruptTrigger,
    ) -> Self {
        let pin_group = (pin.pin() % 8) as usize;
        let interrupts = pin.int_proc();
        disarm_wait(interrupts, pin.pin() as usize);
        // first, clear the INTR register bits. without this INTR will still
        // contain reports of previous edges, causing the IRQ to fire early
        // on stale state. clearing these means that we can only detect edges
        // that occur *after* the clear happened, but since both this and the
        // alternative are fundamentally racy it's probably fine.
        // (the alternative being checking the current level and waiting for
        // its inverse, but that requires reading the current level and thus
        // missing anything that happened before the level was read.)
        pin.io().intr(pin.pin() as usize / 8).write(|w| {
            w.set_edge_high(pin_group, true);
            w.set_edge_low(pin_group, true);
        });

        // Each INTR register is divided into 8 groups, one group for each
        // pin, and each group consists of LEVEL_LOW, LEVEL_HIGH, EDGE_LOW,
        // and EDGE_HIGH.
        interrupts.inte((pin.pin() / 8) as usize).write_set(|w| {
            match level {
                InterruptTrigger::LevelHigh => {
                    w.set_level_high(pin_group, true);
                }
                InterruptTrigger::LevelLow => {
                    w.set_level_low(pin_group, true);
                }
                InterruptTrigger::EdgeHigh => {
                    w.set_edge_high(pin_group, true);
                }
                InterruptTrigger::EdgeLow => {
                    w.set_edge_low(pin_group, true);
                }
                InterruptTrigger::AnyEdge => {
                    w.set_edge_high(pin_group, true);
                    w.set_edge_low(pin_group, true);
                }
            }
        });

        Self { pin, interrupts }
    }
}

impl<'d> Future for InputFuture<'d> {
    /// Completion means the handler cleared all enable bits for this pin.
    type Output = ();

    /// Registers `cx` and returns Ready when `self`'s saved interrupt-enable group is clear.
    /// External register mutation can therefore produce readiness without a physical event.
    fn poll(
        self: FuturePin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Self::Output> {
        // We need to register/re-register the waker for each poll because any
        // calls to wake will deregister the waker.
        let waker = match self.pin.bank() {
            Bank::Bank0 => &BANK0_WAKERS[self.pin.pin() as usize],
            #[cfg(feature = "qspi-as-gpio")]
            Bank::Qspi => &QSPI_WAKERS[self.pin.pin() as usize],
        };
        waker.register(cx.waker());

        // Keep the interrupt bank of the CPU that armed this wait.
        let inte: pac::io::regs::Int = self.interrupts.inte((self.pin.pin() / 8) as usize).read();
        // The register is divided into groups of four, one group for
        // each pin. Each group consists of four trigger levels LEVEL_LOW,
        // LEVEL_HIGH, EDGE_LOW, and EDGE_HIGH for each pin.
        let pin_group = (self.pin.pin() % 8) as usize;

        // since the interrupt handler clears all INTE flags we'll check that
        // all have been cleared and unconditionally return Ready(()) if so.
        // we don't need further handshaking since only a single event wait
        // is possible for any given pin at any given time.
        if !inte.edge_high(pin_group)
            && !inte.edge_low(pin_group)
            && !inte.level_high(pin_group)
            && !inte.level_low(pin_group)
        {
            return Poll::Ready(());
        }
        Poll::Pending
    }
}

/// GPIO output driver.
#[derive(Debug)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct Output<'d> {
    /// Exclusive pad ownership and shared configuration/cleanup implementation.
    pin: Flex<'d>,
}

impl<'d> Output<'d> {
    /// Returns an output owning `pin`, writing `initial_output` before enabling SIO output.
    /// Reinitializes pad/mux; an already-enabled SIO latch can be exposed during setup.
    #[inline]
    pub fn new(
        pin: Peri<'d, impl Pin>,
        initial_output: Level,
    ) -> Self {
        let mut pin = Flex::new(pin);
        match initial_output {
            Level::High => pin.set_high(),
            Level::Low => pin.set_low(),
        }

        pin.set_as_output();
        Self { pin }
    }

    /// Programs nominal pad drive `strength`; this is not a current regulator or load measurement.
    #[inline]
    pub fn set_drive_strength(
        &mut self,
        strength: Drive,
    ) {
        self.pin.set_drive_strength(strength)
    }

    /// Programs pad transition-rate selection `slew_rate`, preserving other pad settings.
    #[inline]
    pub fn set_slew_rate(
        &mut self,
        slew_rate: SlewRate,
    ) {
        self.pin.set_slew_rate(slew_rate)
    }

    /// Sets pad output inversion to `invert`; SIO latch readback remains uninverted.
    #[inline]
    pub fn set_inversion(
        &mut self,
        invert: bool,
    ) {
        self.pin.set_output_inversion(invert)
    }

    /// Sets the SIO output latch high; physical drive depends on direction/isolation/overrides.
    #[inline]
    pub fn set_high(&mut self) { self.pin.set_high() }

    /// Sets the SIO output latch low; physical drive depends on direction/isolation/overrides.
    #[inline]
    pub fn set_low(&mut self) { self.pin.set_low() }

    /// Stores requested `level` using an atomic SIO set/clear operation.
    #[inline]
    pub fn set_level(
        &mut self,
        level: Level,
    ) {
        self.pin.set_level(level)
    }

    /// Returns whether the SIO output latch is high, without measuring the pad.
    #[inline]
    pub fn is_set_high(&self) -> bool { self.pin.is_set_high() }

    /// Returns whether the SIO output latch is low, without measuring the pad.
    #[inline]
    pub fn is_set_low(&self) -> bool { self.pin.is_set_low() }

    /// Returns the requested output latch level, without measuring the pad.
    #[inline]
    pub fn get_output_level(&self) -> Level { self.pin.get_output_level() }

    /// Atomically toggles the SIO output latch; physical output follows direction and overrides.
    #[inline]
    pub fn toggle(&mut self) { self.pin.toggle() }

    /// Sets electrical pad isolation to `isolate`; other pad and SIO configuration is retained.
    #[cfg(feature = "_rp235x")]
    #[inline]
    pub fn set_pad_isolation(
        &mut self,
        isolate: bool,
    ) {
        self.pin.set_pad_isolation(isolate)
    }
}

/// GPIO output open-drain.
#[derive(Debug)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct OutputOpenDrain<'d> {
    /// Exclusive pad ownership and shared configuration/cleanup implementation.
    pin: Flex<'d>,
}

impl<'d> OutputOpenDrain<'d> {
    /// Returns an open-drain owner of `pin`: `initial_output` Low drives low, High
    /// releases the pad. Initializes the latch low; an external/internal pull-up is separate.
    #[inline]
    pub fn new(
        pin: Peri<'d, impl Pin>,
        initial_output: Level,
    ) -> Self {
        let mut pin = Flex::new(pin);
        pin.set_low();
        match initial_output {
            Level::High => pin.set_as_input(),
            Level::Low => pin.set_as_output(),
        }
        Self { pin }
    }

    /// Enables the weak pull-up when `enable` is true, otherwise removes both pulls.
    #[inline]
    pub fn set_pullup(
        &mut self,
        enable: bool,
    ) {
        if enable {
            self.pin.set_pull(Pull::Up);
        } else {
            self.pin.set_pull(Pull::None);
        }
    }

    /// Programs nominal pad drive `strength`; this is not a current regulator or load measurement.
    #[inline]
    pub fn set_drive_strength(
        &mut self,
        strength: Drive,
    ) {
        self.pin.set_drive_strength(strength)
    }

    /// Programs pad transition-rate selection `slew_rate`, preserving other pad settings.
    #[inline]
    pub fn set_slew_rate(
        &mut self,
        slew_rate: SlewRate,
    ) {
        self.pin.set_slew_rate(slew_rate)
    }

    /// Releases the output driver; a pull-up or external device determines the measured level.
    #[inline]
    pub fn set_high(&mut self) {
        // For Open Drain High, disable the output pin.
        self.pin.set_as_input()
    }

    /// Enables the low output driver.
    #[inline]
    pub fn set_low(&mut self) {
        // For Open Drain Low, enable the output pin.
        self.pin.set_as_output()
    }

    /// Sets `level` Low to drive low, or High to release the pad.
    #[inline]
    pub fn set_level(
        &mut self,
        level: Level,
    ) {
        match level {
            Level::Low => self.set_low(),
            Level::High => self.set_high(),
        }
    }

    /// Returns whether the output driver is released; does not measure the pad.
    #[inline]
    pub fn is_set_high(&self) -> bool { !self.is_set_low() }

    /// Returns whether the low output driver is enabled; does not measure the pad.
    #[inline]
    pub fn is_set_low(&self) -> bool { self.pin.is_set_as_output() }

    /// Returns Low when driving low or High when released; does not measure the pad.
    #[inline]
    pub fn get_output_level(&self) -> Level { self.is_set_high().into() }

    /// Atomically switches between driving low and releasing the pad.
    #[inline]
    pub fn toggle(&mut self) { self.pin.toggle_set_as_output() }

    /// Returns whether the sampled SIO input is high, after configured input inversion.
    #[inline]
    pub fn is_high(&self) -> bool { self.pin.is_high() }

    /// Returns whether the sampled SIO input is low, after configured input inversion.
    #[inline]
    pub fn is_low(&self) -> bool { self.pin.is_low() }

    /// Returns the sampled SIO input level, after configured input inversion.
    #[inline]
    pub fn get_level(&self) -> Level { self.is_high().into() }

    /// Waits for the level-high IRQ condition; an already-high input needs no new edge.
    /// Completes after IRQ handling, with no timeout. Cancellation disarms this pin's wait.
    #[inline]
    pub async fn wait_for_high(&mut self) { self.pin.wait_for_high().await; }

    /// Waits for the level-low IRQ condition; an already-low input needs no new edge.
    /// Completes after IRQ handling, with no timeout. Cancellation disarms this pin's wait.
    #[inline]
    pub async fn wait_for_low(&mut self) { self.pin.wait_for_low().await; }

    /// Waits for low-to-high edge after arming, discarding previously latched edges.
    /// Edges during setup may be missed; no timeout/debounce. Cancellation disarms this pin.
    #[inline]
    pub async fn wait_for_rising_edge(&mut self) { self.pin.wait_for_rising_edge().await; }

    /// Waits for high-to-low edge after arming, discarding previously latched edges.
    /// Edges during setup may be missed; no timeout/debounce. Cancellation disarms this pin.
    #[inline]
    pub async fn wait_for_falling_edge(&mut self) { self.pin.wait_for_falling_edge().await; }

    /// Waits for either edge after arming, discarding previously latched edges.
    /// Edges during setup may be missed; no timeout/debounce. Cancellation disarms this pin.
    #[inline]
    pub async fn wait_for_any_edge(&mut self) { self.pin.wait_for_any_edge().await; }

    /// Sets electrical pad isolation to `isolate`; other pad and SIO configuration is retained.
    #[cfg(feature = "_rp235x")]
    #[inline]
    pub fn set_pad_isolation(
        &mut self,
        isolate: bool,
    ) {
        self.pin.set_pad_isolation(isolate)
    }
}

/// GPIO flexible pin.
///
/// This pin can be either an input or output pin. The output level register bit will remain
/// set while not in output mode, so the pin's level will be 'remembered' when it is not in output
/// mode.
#[derive(Debug)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct Flex<'d> {
    /// Exclusive erased pin token retained for this driver or wait.
    pin: Peri<'d, AnyPin>,
}

impl<'d> Flex<'d> {
    /// Returns a flexible owner of `pin`, selecting SIO and resetting pad/mux fields.
    /// Existing SIO output-enable and output latches are retained; this does not
    /// guarantee a disconnected input. Set direction and level explicitly.
    #[inline]
    pub fn new(pin: Peri<'d, impl Pin>) -> Self {
        pin.pad_ctrl().write(|w| {
            #[cfg(feature = "_rp235x")]
            w.set_iso(false);
            w.set_ie(true);
        });

        pin.gpio().ctrl().write(|w| {
            #[cfg(feature = "rp2040")]
            w.set_funcsel(pac::io::vals::Gpio0ctrlFuncsel::SIO_0 as _);
            #[cfg(feature = "_rp235x")]
            w.set_funcsel(pac::io::vals::Gpio0ctrlFuncsel::SIOB_PROC_0 as _);
        });

        Self { pin: pin.into() }
    }

    /// Returns this pin's mask within its 32-bit SIO register word.
    #[inline]
    fn bit(&self) -> u32 { 1 << (self.pin.pin() % 32) }

    /// Applies resistor selection `pull` and enables digital input; preserves other pad fields.
    #[inline]
    pub fn set_pull(
        &mut self,
        pull: Pull,
    ) {
        self.pin.pad_ctrl().modify(|w| {
            w.set_ie(true);
            let (pu, pd) = match pull {
                Pull::Up => (true, false),
                Pull::Down => (false, true),
                Pull::None => (false, false),
            };
            w.set_pue(pu);
            w.set_pde(pd);
        });
    }

    /// Programs nominal pad drive `strength`; this is not a current regulator or load measurement.
    #[inline]
    pub fn set_drive_strength(
        &mut self,
        strength: Drive,
    ) {
        self.pin.pad_ctrl().modify(|w| {
            w.set_drive(match strength {
                Drive::_2mA => pac::pads::vals::Drive::_2M_A,
                Drive::_4mA => pac::pads::vals::Drive::_4M_A,
                Drive::_8mA => pac::pads::vals::Drive::_8M_A,
                Drive::_12mA => pac::pads::vals::Drive::_12M_A,
            });
        });
    }

    /// Programs pad transition-rate selection `slew_rate`, preserving other pad settings.
    #[inline]
    pub fn set_slew_rate(
        &mut self,
        slew_rate: SlewRate,
    ) {
        self.pin.pad_ctrl().modify(|w| {
            w.set_slewfast(slew_rate == SlewRate::Fast);
        });
    }

    /// Sets input Schmitt hysteresis to `enable`, preserving other pad settings.
    #[inline]
    pub fn set_schmitt(
        &mut self,
        enable: bool,
    ) {
        self.pin.pad_ctrl().modify(|w| {
            w.set_schmitt(enable);
        });
    }

    /// Put the pin into input mode.
    ///
    /// The pull setting is left unchanged.
    #[inline]
    pub fn set_as_input(&mut self) { self.pin.sio_oe().value_clr().write_value(self.bit()) }

    /// Put the pin into output mode.
    ///
    /// The pin level will be whatever the retained SIO latch contains. If you want it to begin
    /// at a specific level, call `set_high`/`set_low` on the pin first.
    #[inline]
    pub fn set_as_output(&mut self) { self.pin.sio_oe().value_set().write_value(self.bit()) }

    /// Returns whether the SIO output-enable latch is set, before OE override/isolation.
    #[inline]
    fn is_set_as_output(&self) -> bool { (self.pin.sio_oe().value().read() & self.bit()) != 0 }

    /// Atomically toggles the SIO output-enable latch, retaining its output value.
    #[inline]
    pub fn toggle_set_as_output(&mut self) { self.pin.sio_oe().value_xor().write_value(self.bit()) }

    /// Sets input inversion to `invert`; does not change the separate IRQ override.
    #[inline]
    pub fn set_input_inversion(
        &mut self,
        invert: bool,
    ) {
        self.pin.gpio().ctrl().modify(|w| {
            w.set_inover(if invert {
                pac::io::vals::Inover::INVERT
            } else {
                pac::io::vals::Inover::NORMAL
            })
        });
    }

    /// Sets output-enable inversion to `invert`; direction-latch readback remains uninverted.
    #[inline]
    pub fn set_output_enable_inversion(
        &mut self,
        invert: bool,
    ) {
        self.pin.gpio().ctrl().modify(|w| {
            w.set_oeover(if invert {
                pac::io::vals::Oeover::INVERT
            } else {
                pac::io::vals::Oeover::NORMAL
            })
        })
    }

    /// Sets pad output inversion to `invert`; SIO latch readback remains uninverted.
    #[inline]
    pub fn set_output_inversion(
        &mut self,
        invert: bool,
    ) {
        self.pin.gpio().ctrl().modify(|w| {
            w.set_outover(if invert {
                pac::io::vals::Outover::INVERT
            } else {
                pac::io::vals::Outover::NORMAL
            })
        });
    }

    /// Returns whether the sampled SIO input is high, after configured input inversion.
    #[inline]
    pub fn is_high(&self) -> bool { !self.is_low() }

    /// Returns whether the sampled SIO input is low, after configured input inversion.

    #[inline]
    pub fn is_low(&self) -> bool { self.pin.sio_in().read() & self.bit() == 0 }

    /// Returns the sampled SIO input level, after configured input inversion.
    #[inline]
    pub fn get_level(&self) -> Level { self.is_high().into() }

    /// Sets the SIO output latch high; physical drive depends on direction/isolation/overrides.
    #[inline]
    pub fn set_high(&mut self) { self.pin.sio_out().value_set().write_value(self.bit()) }

    /// Sets the SIO output latch low; physical drive depends on direction/isolation/overrides.
    #[inline]
    pub fn set_low(&mut self) { self.pin.sio_out().value_clr().write_value(self.bit()) }

    /// Stores requested `level` using an atomic SIO set/clear operation.
    #[inline]
    pub fn set_level(
        &mut self,
        level: Level,
    ) {
        match level {
            Level::Low => self.set_low(),
            Level::High => self.set_high(),
        }
    }

    /// Returns whether the SIO output latch is high, without measuring the pad.
    #[inline]
    pub fn is_set_high(&self) -> bool { !self.is_set_low() }

    /// Returns whether the SIO output latch is low, without measuring the pad.
    #[inline]
    pub fn is_set_low(&self) -> bool { (self.pin.sio_out().value().read() & self.bit()) == 0 }

    /// Returns the requested output latch level, without measuring the pad.
    #[inline]
    pub fn get_output_level(&self) -> Level { self.is_set_high().into() }

    /// Atomically toggles the SIO output latch; physical output follows direction and overrides.
    #[inline]
    pub fn toggle(&mut self) { self.pin.sio_out().value_xor().write_value(self.bit()) }

    /// Waits for the level-high IRQ condition; an already-high input needs no new edge.
    /// Completes after IRQ handling, with no timeout. Cancellation disarms this pin's wait.
    #[inline]
    pub async fn wait_for_high(&mut self) { InputFuture::new(self.pin.reborrow(), InterruptTrigger::LevelHigh).await; }

    /// Waits for the level-low IRQ condition; an already-low input needs no new edge.
    /// Completes after IRQ handling, with no timeout. Cancellation disarms this pin's wait.
    #[inline]
    pub async fn wait_for_low(&mut self) { InputFuture::new(self.pin.reborrow(), InterruptTrigger::LevelLow).await; }

    /// Waits for low-to-high edge after arming, discarding previously latched edges.
    /// Edges during setup may be missed; no timeout/debounce. Cancellation disarms this pin.
    #[inline]
    pub async fn wait_for_rising_edge(&mut self) {
        InputFuture::new(self.pin.reborrow(), InterruptTrigger::EdgeHigh).await;
    }

    /// Waits for high-to-low edge after arming, discarding previously latched edges.
    /// Edges during setup may be missed; no timeout/debounce. Cancellation disarms this pin.
    #[inline]
    pub async fn wait_for_falling_edge(&mut self) {
        InputFuture::new(self.pin.reborrow(), InterruptTrigger::EdgeLow).await;
    }

    /// Waits for either edge after arming, discarding previously latched edges.
    /// Edges during setup may be missed; no timeout/debounce. Cancellation disarms this pin.
    #[inline]
    pub async fn wait_for_any_edge(&mut self) {
        InputFuture::new(self.pin.reborrow(), InterruptTrigger::AnyEdge).await;
    }

    /// Arms conditions from `cfg` and returns a borrowed guard that disables all this
    /// pin's dormant-wake conditions on drop; clears selected stale edge flags first.
    /// Does not enter dormant mode or preserve a previous wake configuration.
    #[inline]
    pub fn dormant_wake(
        &mut self,
        cfg: DormantWakeConfig,
    ) -> DormantWake<'_> {
        let idx = self.pin._pin() as usize;
        self.pin.io().intr(idx / 8).write(|w| {
            w.set_edge_high(idx % 8, cfg.edge_high);
            w.set_edge_low(idx % 8, cfg.edge_low);
        });
        self.pin.io().int_dormant_wake().inte(idx / 8).write_set(|w| {
            w.set_edge_high(idx % 8, cfg.edge_high);
            w.set_edge_low(idx % 8, cfg.edge_low);
            w.set_level_high(idx % 8, cfg.level_high);
            w.set_level_low(idx % 8, cfg.level_low);
        });
        DormantWake {
            pin: self.pin.reborrow(),
            cfg,
        }
    }

    /// Sets electrical pad isolation to `isolate`; other pad and SIO configuration is retained.
    #[cfg(feature = "_rp235x")]
    #[inline]
    pub fn set_pad_isolation(
        &mut self,
        isolate: bool,
    ) {
        self.pin.pad_ctrl().modify(|w| {
            w.set_iso(isolate);
        });
    }
}

impl<'d> Drop for Flex<'d> {
    /// Resets pad fields, disconnects the mux and clears dormant wake enables;
    /// SIO output and output-enable latches are retained.
    #[inline]
    fn drop(&mut self) {
        let idx = self.pin._pin() as usize;
        self.pin.pad_ctrl().write(|_| {});
        self.pin.gpio().ctrl().write(|w| {
            w.set_funcsel(pac::io::vals::Gpio0ctrlFuncsel::NULL as _);
            w.set_inover(pac::io::vals::Inover::NORMAL);
            w.set_outover(pac::io::vals::Outover::NORMAL);
        });
        self.pin.io().int_dormant_wake().inte(idx / 8).write_clear(|w| {
            w.set_edge_high(idx % 8, true);
            w.set_edge_low(idx % 8, true);
            w.set_level_high(idx % 8, true);
            w.set_level_low(idx % 8, true);
        });
    }
}

/// Dormant wake driver.
#[derive(Debug)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct DormantWake<'w> {
    /// Borrowed pad held until wake configuration is removed.
    pin: Peri<'w, AnyPin>,
    /// Selected edge flags to acknowledge during cleanup.
    cfg: DormantWakeConfig,
}

impl<'w> Drop for DormantWake<'w> {
    /// Clears selected latched edges and all dormant-wake enables for this pin.
    fn drop(&mut self) {
        let idx = self.pin._pin() as usize;
        self.pin.io().intr(idx / 8).write(|w| {
            w.set_edge_high(idx % 8, self.cfg.edge_high);
            w.set_edge_low(idx % 8, self.cfg.edge_low);
        });
        self.pin.io().int_dormant_wake().inte(idx / 8).write_clear(|w| {
            w.set_edge_high(idx % 8, true);
            w.set_edge_low(idx % 8, true);
            w.set_level_high(idx % 8, true);
            w.set_level_low(idx % 8, true);
        });
    }
}

/// Internal pin encoding and register selection; implementations must encode valid package pins.
pub(crate) trait SealedPin: Sized {
    /// Returns bank in bit 7 and bank-local pin index in bits 0..6.
    fn pin_bank(&self) -> u8;

    #[inline]
    /// Returns the bank-local pin number from the encoded token.
    fn _pin(&self) -> u8 { self.pin_bank() & 0x7F }

    #[inline]
    /// Returns QSPI for encoded bank one when enabled, otherwise bank zero.
    fn _bank(&self) -> Bank {
        match self.pin_bank() >> 7 {
            #[cfg(feature = "qspi-as-gpio")]
            1 => Bank::Qspi,
            _ => Bank::Bank0,
        }
    }

    /// Returns this pin's IO register bank.
    fn io(&self) -> pac::io::Io {
        match self._bank() {
            Bank::Bank0 => crate::pac::IO_BANK0,
            #[cfg(feature = "qspi-as-gpio")]
            Bank::Qspi => crate::pac::IO_QSPI,
        }
    }

    /// Returns the pin's mux/override register pair; invalid stolen indices can panic.
    fn gpio(&self) -> pac::io::Gpio { self.io().gpio(self._pin() as _) }

    /// Returns this pin's pad-control register; invalid stolen indices can panic.
    fn pad_ctrl(&self) -> Reg<pac::pads::regs::GpioCtrl, RW> {
        let block = match self._bank() {
            Bank::Bank0 => crate::pac::PADS_BANK0,
            #[cfg(feature = "qspi-as-gpio")]
            Bank::Qspi => crate::pac::PADS_QSPI,
        };
        block.gpio(self._pin() as _)
    }

    /// Returns the SIO output-latch word selected for this chip and pin.
    fn sio_out(&self) -> pac::sio::Gpio {
        if cfg!(feature = "rp2040") {
            SIO.gpio_out(self._bank() as _)
        } else {
            SIO.gpio_out((self._pin() / 32) as _)
        }
    }

    /// Returns the SIO output-enable word selected for this chip and pin.
    fn sio_oe(&self) -> pac::sio::Gpio {
        if cfg!(feature = "rp2040") {
            SIO.gpio_oe(self._bank() as _)
        } else {
            SIO.gpio_oe((self._pin() / 32) as _)
        }
    }

    /// Returns the sampled SIO input word selected for this chip and pin.
    fn sio_in(&self) -> Reg<u32, RW> {
        if cfg!(feature = "rp2040") {
            SIO.gpio_in(self._bank() as _)
        } else {
            SIO.gpio_in((self._pin() / 32) as _)
        }
    }

    /// Returns this pin bank's interrupt registers for the currently executing CPU.
    fn int_proc(&self) -> pac::io::Int {
        let proc = SIO.cpuid().read();
        self.io().int_proc(proc as _)
    }
}

/// Interface for a Pin that can be configured by an [Input] or [Output] driver, or converted to an [AnyPin].
#[allow(private_bounds)]
pub trait Pin: PeripheralType + Into<AnyPin> + SealedPin + Sized + 'static {
    /// Returns the pin number within a bank
    #[inline]
    fn pin(&self) -> u8 { self._pin() }

    /// Returns the bank of this pin
    #[inline]
    fn bank(&self) -> Bank { self._bank() }
}

/// Type-erased GPIO pin
#[derive(Debug)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct AnyPin {
    /// Bank discriminator in bit 7, bank-local pin index in bits 0..6.
    pin_bank: u8,
}

impl AnyPin {
    /// Returns a token encoded by `pin_bank` (bank in bit 7, bank-local index in bits 0..6).
    /// Does not initialize hardware or validate the encoding.
    ///
    /// # Safety
    ///
    /// The encoding must identify a supported, physically available pin on the selected
    /// chip, and no other token/driver may own it. QSPI GPIO also requires execution
    /// and memory users to be independent of the repurposed external flash interface.
    pub unsafe fn steal(pin_bank: u8) -> Peri<'static, Self> { Peri::new_unchecked(Self { pin_bank }) }
}

impl_peripheral!(AnyPin);

impl Pin for AnyPin {}
impl SealedPin for AnyPin {
    /// Returns bank in bit 7 and bank-local pin index in bits 0..6.
    fn pin_bank(&self) -> u8 { self.pin_bank }
}

// ==========================

/// Implements valid bank/index encoding and ownership-preserving type erasure for one singleton.
macro_rules! impl_pin {
    ($name:ident, $bank:expr, $pin_num:expr) => {
        impl Pin for peripherals::$name {}
        impl SealedPin for peripherals::$name {
            #[inline]
            /// Returns bank in bit 7 and bank-local pin index in bits 0..6.
            fn pin_bank(&self) -> u8 { ($bank as u8) * 128 + $pin_num }
        }

        impl From<peripherals::$name> for crate::gpio::AnyPin {
            /// Consumes `val` and returns its erased bank/index token without changing hardware.
            fn from(val: peripherals::$name) -> Self {
                Self {
                    pin_bank: val.pin_bank(),
                }
            }
        }
    };
}

impl_pin!(PIN_0, Bank::Bank0, 0);
impl_pin!(PIN_1, Bank::Bank0, 1);
impl_pin!(PIN_2, Bank::Bank0, 2);
impl_pin!(PIN_3, Bank::Bank0, 3);
impl_pin!(PIN_4, Bank::Bank0, 4);
impl_pin!(PIN_5, Bank::Bank0, 5);
impl_pin!(PIN_6, Bank::Bank0, 6);
impl_pin!(PIN_7, Bank::Bank0, 7);
impl_pin!(PIN_8, Bank::Bank0, 8);
impl_pin!(PIN_9, Bank::Bank0, 9);
impl_pin!(PIN_10, Bank::Bank0, 10);
impl_pin!(PIN_11, Bank::Bank0, 11);
impl_pin!(PIN_12, Bank::Bank0, 12);
impl_pin!(PIN_13, Bank::Bank0, 13);
impl_pin!(PIN_14, Bank::Bank0, 14);
impl_pin!(PIN_15, Bank::Bank0, 15);
impl_pin!(PIN_16, Bank::Bank0, 16);
impl_pin!(PIN_17, Bank::Bank0, 17);
impl_pin!(PIN_18, Bank::Bank0, 18);
impl_pin!(PIN_19, Bank::Bank0, 19);
impl_pin!(PIN_20, Bank::Bank0, 20);
impl_pin!(PIN_21, Bank::Bank0, 21);
impl_pin!(PIN_22, Bank::Bank0, 22);
impl_pin!(PIN_23, Bank::Bank0, 23);
impl_pin!(PIN_24, Bank::Bank0, 24);
impl_pin!(PIN_25, Bank::Bank0, 25);
impl_pin!(PIN_26, Bank::Bank0, 26);
impl_pin!(PIN_27, Bank::Bank0, 27);
impl_pin!(PIN_28, Bank::Bank0, 28);
impl_pin!(PIN_29, Bank::Bank0, 29);

#[cfg(feature = "rp235xb")]
impl_pin!(PIN_30, Bank::Bank0, 30);
#[cfg(feature = "rp235xb")]
impl_pin!(PIN_31, Bank::Bank0, 31);
#[cfg(feature = "rp235xb")]
impl_pin!(PIN_32, Bank::Bank0, 32);
#[cfg(feature = "rp235xb")]
impl_pin!(PIN_33, Bank::Bank0, 33);
#[cfg(feature = "rp235xb")]
impl_pin!(PIN_34, Bank::Bank0, 34);
#[cfg(feature = "rp235xb")]
impl_pin!(PIN_35, Bank::Bank0, 35);
#[cfg(feature = "rp235xb")]
impl_pin!(PIN_36, Bank::Bank0, 36);
#[cfg(feature = "rp235xb")]
impl_pin!(PIN_37, Bank::Bank0, 37);
#[cfg(feature = "rp235xb")]
impl_pin!(PIN_38, Bank::Bank0, 38);
#[cfg(feature = "rp235xb")]
impl_pin!(PIN_39, Bank::Bank0, 39);
#[cfg(feature = "rp235xb")]
impl_pin!(PIN_40, Bank::Bank0, 40);
#[cfg(feature = "rp235xb")]
impl_pin!(PIN_41, Bank::Bank0, 41);
#[cfg(feature = "rp235xb")]
impl_pin!(PIN_42, Bank::Bank0, 42);
#[cfg(feature = "rp235xb")]
impl_pin!(PIN_43, Bank::Bank0, 43);
#[cfg(feature = "rp235xb")]
impl_pin!(PIN_44, Bank::Bank0, 44);
#[cfg(feature = "rp235xb")]
impl_pin!(PIN_45, Bank::Bank0, 45);
#[cfg(feature = "rp235xb")]
impl_pin!(PIN_46, Bank::Bank0, 46);
#[cfg(feature = "rp235xb")]
impl_pin!(PIN_47, Bank::Bank0, 47);

// TODO rp235x bank1 as gpio support
#[cfg(feature = "qspi-as-gpio")]
impl_pin!(PIN_QSPI_SCLK, Bank::Qspi, 0);
#[cfg(feature = "qspi-as-gpio")]
impl_pin!(PIN_QSPI_SS, Bank::Qspi, 1);
#[cfg(feature = "qspi-as-gpio")]
impl_pin!(PIN_QSPI_SD0, Bank::Qspi, 2);
#[cfg(feature = "qspi-as-gpio")]
impl_pin!(PIN_QSPI_SD1, Bank::Qspi, 3);
#[cfg(feature = "qspi-as-gpio")]
impl_pin!(PIN_QSPI_SD2, Bank::Qspi, 4);
#[cfg(feature = "qspi-as-gpio")]
impl_pin!(PIN_QSPI_SD3, Bank::Qspi, 5);

// ====================

/// embedded-hal 0.2 adapters; hardware operations are infallible but not electrical verification.
mod eh02 {
    use super::*;

    impl<'d> embedded_hal_02::digital::v2::InputPin for Input<'d> {
        /// Register access does not produce recoverable errors.
        type Error = Infallible;

        /// Returns Ok with sampled high state; never returns an error.

        fn is_high(&self) -> Result<bool, Self::Error> { Ok(self.is_high()) }

        /// Returns Ok with sampled low state; never returns an error.

        fn is_low(&self) -> Result<bool, Self::Error> { Ok(self.is_low()) }
    }

    impl<'d> embedded_hal_02::digital::v2::OutputPin for Output<'d> {
        /// Register access does not produce recoverable errors.
        type Error = Infallible;

        /// Requests high (release for open drain) and returns Ok; no electrical verification.

        fn set_high(&mut self) -> Result<(), Self::Error> { Ok(self.set_high()) }

        /// Requests low and returns Ok; no electrical verification.

        fn set_low(&mut self) -> Result<(), Self::Error> { Ok(self.set_low()) }
    }

    impl<'d> embedded_hal_02::digital::v2::StatefulOutputPin for Output<'d> {
        /// Returns Ok with requested high/released state, not measured voltage.
        fn is_set_high(&self) -> Result<bool, Self::Error> { Ok(self.is_set_high()) }

        /// Returns Ok with requested low/driving state, not measured voltage.

        fn is_set_low(&self) -> Result<bool, Self::Error> { Ok(self.is_set_low()) }
    }

    impl<'d> embedded_hal_02::digital::v2::ToggleableOutputPin for Output<'d> {
        /// Register access does not produce recoverable errors.
        type Error = Infallible;

        #[inline]
        /// Toggles requested output state and returns Ok; no electrical verification.
        fn toggle(&mut self) -> Result<(), Self::Error> { Ok(self.toggle()) }
    }

    impl<'d> embedded_hal_02::digital::v2::InputPin for OutputOpenDrain<'d> {
        /// Register access does not produce recoverable errors.
        type Error = Infallible;

        /// Returns Ok with sampled high state; never returns an error.

        fn is_high(&self) -> Result<bool, Self::Error> { Ok(self.is_high()) }

        /// Returns Ok with sampled low state; never returns an error.

        fn is_low(&self) -> Result<bool, Self::Error> { Ok(self.is_low()) }
    }

    impl<'d> embedded_hal_02::digital::v2::OutputPin for OutputOpenDrain<'d> {
        /// Register access does not produce recoverable errors.
        type Error = Infallible;

        #[inline]
        /// Requests high (release for open drain) and returns Ok; no electrical verification.
        fn set_high(&mut self) -> Result<(), Self::Error> { Ok(self.set_high()) }

        #[inline]
        /// Requests low and returns Ok; no electrical verification.
        fn set_low(&mut self) -> Result<(), Self::Error> { Ok(self.set_low()) }
    }

    impl<'d> embedded_hal_02::digital::v2::StatefulOutputPin for OutputOpenDrain<'d> {
        /// Returns Ok with requested high/released state, not measured voltage.
        fn is_set_high(&self) -> Result<bool, Self::Error> { Ok(self.is_set_high()) }

        /// Returns Ok with requested low/driving state, not measured voltage.

        fn is_set_low(&self) -> Result<bool, Self::Error> { Ok(self.is_set_low()) }
    }

    impl<'d> embedded_hal_02::digital::v2::ToggleableOutputPin for OutputOpenDrain<'d> {
        /// Register access does not produce recoverable errors.
        type Error = Infallible;

        #[inline]
        /// Toggles requested output state and returns Ok; no electrical verification.
        fn toggle(&mut self) -> Result<(), Self::Error> { Ok(self.toggle()) }
    }

    impl<'d> embedded_hal_02::digital::v2::InputPin for Flex<'d> {
        /// Register access does not produce recoverable errors.
        type Error = Infallible;

        /// Returns Ok with sampled high state; never returns an error.

        fn is_high(&self) -> Result<bool, Self::Error> { Ok(self.is_high()) }

        /// Returns Ok with sampled low state; never returns an error.

        fn is_low(&self) -> Result<bool, Self::Error> { Ok(self.is_low()) }
    }

    impl<'d> embedded_hal_02::digital::v2::OutputPin for Flex<'d> {
        /// Register access does not produce recoverable errors.
        type Error = Infallible;

        /// Requests high (release for open drain) and returns Ok; no electrical verification.

        fn set_high(&mut self) -> Result<(), Self::Error> { Ok(self.set_high()) }

        /// Requests low and returns Ok; no electrical verification.

        fn set_low(&mut self) -> Result<(), Self::Error> { Ok(self.set_low()) }
    }

    impl<'d> embedded_hal_02::digital::v2::StatefulOutputPin for Flex<'d> {
        /// Returns Ok with requested high/released state, not measured voltage.
        fn is_set_high(&self) -> Result<bool, Self::Error> { Ok(self.is_set_high()) }

        /// Returns Ok with requested low/driving state, not measured voltage.

        fn is_set_low(&self) -> Result<bool, Self::Error> { Ok(self.is_set_low()) }
    }

    impl<'d> embedded_hal_02::digital::v2::ToggleableOutputPin for Flex<'d> {
        /// Register access does not produce recoverable errors.
        type Error = Infallible;

        #[inline]
        /// Toggles requested output state and returns Ok; no electrical verification.
        fn toggle(&mut self) -> Result<(), Self::Error> { Ok(self.toggle()) }
    }
}

impl<'d> embedded_hal_1::digital::ErrorType for Input<'d> {
    /// Register access does not produce recoverable errors.
    type Error = Infallible;
}

impl<'d> embedded_hal_1::digital::InputPin for Input<'d> {
    /// Returns Ok with sampled high state; never returns an error.
    fn is_high(&mut self) -> Result<bool, Self::Error> { Ok((*self).is_high()) }

    /// Returns Ok with sampled low state; never returns an error.

    fn is_low(&mut self) -> Result<bool, Self::Error> { Ok((*self).is_low()) }
}

impl<'d> embedded_hal_1::digital::ErrorType for Output<'d> {
    /// Register access does not produce recoverable errors.
    type Error = Infallible;
}

impl<'d> embedded_hal_1::digital::OutputPin for Output<'d> {
    /// Requests high (release for open drain) and returns Ok; no electrical verification.
    fn set_high(&mut self) -> Result<(), Self::Error> { Ok(self.set_high()) }

    /// Requests low and returns Ok; no electrical verification.

    fn set_low(&mut self) -> Result<(), Self::Error> { Ok(self.set_low()) }
}

impl<'d> embedded_hal_1::digital::StatefulOutputPin for Output<'d> {
    /// Returns Ok with requested high/released state, not measured voltage.
    fn is_set_high(&mut self) -> Result<bool, Self::Error> { Ok((*self).is_set_high()) }

    /// Returns Ok with requested low/driving state, not measured voltage.

    fn is_set_low(&mut self) -> Result<bool, Self::Error> { Ok((*self).is_set_low()) }
}

impl<'d> embedded_hal_1::digital::ErrorType for OutputOpenDrain<'d> {
    /// Register access does not produce recoverable errors.
    type Error = Infallible;
}

impl<'d> embedded_hal_1::digital::OutputPin for OutputOpenDrain<'d> {
    /// Requests high (release for open drain) and returns Ok; no electrical verification.
    fn set_high(&mut self) -> Result<(), Self::Error> { Ok(self.set_high()) }

    /// Requests low and returns Ok; no electrical verification.

    fn set_low(&mut self) -> Result<(), Self::Error> { Ok(self.set_low()) }
}

impl<'d> embedded_hal_1::digital::StatefulOutputPin for OutputOpenDrain<'d> {
    /// Returns Ok with requested high/released state, not measured voltage.
    fn is_set_high(&mut self) -> Result<bool, Self::Error> { Ok((*self).is_set_high()) }

    /// Returns Ok with requested low/driving state, not measured voltage.

    fn is_set_low(&mut self) -> Result<bool, Self::Error> { Ok((*self).is_set_low()) }
}

impl<'d> embedded_hal_1::digital::InputPin for OutputOpenDrain<'d> {
    /// Returns Ok with sampled high state; never returns an error.
    fn is_high(&mut self) -> Result<bool, Self::Error> { Ok((*self).is_high()) }

    /// Returns Ok with sampled low state; never returns an error.

    fn is_low(&mut self) -> Result<bool, Self::Error> { Ok((*self).is_low()) }
}

impl<'d> embedded_hal_1::digital::ErrorType for Flex<'d> {
    /// Register access does not produce recoverable errors.
    type Error = Infallible;
}

impl<'d> embedded_hal_1::digital::InputPin for Flex<'d> {
    /// Returns Ok with sampled high state; never returns an error.
    fn is_high(&mut self) -> Result<bool, Self::Error> { Ok((*self).is_high()) }

    /// Returns Ok with sampled low state; never returns an error.

    fn is_low(&mut self) -> Result<bool, Self::Error> { Ok((*self).is_low()) }
}

impl<'d> embedded_hal_1::digital::OutputPin for Flex<'d> {
    /// Requests high (release for open drain) and returns Ok; no electrical verification.
    fn set_high(&mut self) -> Result<(), Self::Error> { Ok(self.set_high()) }

    /// Requests low and returns Ok; no electrical verification.

    fn set_low(&mut self) -> Result<(), Self::Error> { Ok(self.set_low()) }
}

impl<'d> embedded_hal_1::digital::StatefulOutputPin for Flex<'d> {
    /// Returns Ok with requested high/released state, not measured voltage.
    fn is_set_high(&mut self) -> Result<bool, Self::Error> { Ok((*self).is_set_high()) }

    /// Returns Ok with requested low/driving state, not measured voltage.

    fn is_set_low(&mut self) -> Result<bool, Self::Error> { Ok((*self).is_set_low()) }
}

impl<'d> embedded_hal_async::digital::Wait for Flex<'d> {
    /// Returns Ok after the high-level IRQ; no timeout, and cancellation disarms this wait.
    async fn wait_for_high(&mut self) -> Result<(), Self::Error> {
        self.wait_for_high().await;
        Ok(())
    }

    /// Returns Ok after the low-level IRQ; no timeout, and cancellation disarms this wait.

    async fn wait_for_low(&mut self) -> Result<(), Self::Error> {
        self.wait_for_low().await;
        Ok(())
    }

    /// Returns Ok after a newly armed rising-edge IRQ; no debounce/timeout, cancellation disarms.

    async fn wait_for_rising_edge(&mut self) -> Result<(), Self::Error> {
        self.wait_for_rising_edge().await;
        Ok(())
    }

    /// Returns Ok after a newly armed falling-edge IRQ; no debounce/timeout, cancellation disarms.

    async fn wait_for_falling_edge(&mut self) -> Result<(), Self::Error> {
        self.wait_for_falling_edge().await;
        Ok(())
    }

    /// Returns Ok after either newly armed edge IRQ; no debounce/timeout, cancellation disarms.

    async fn wait_for_any_edge(&mut self) -> Result<(), Self::Error> {
        self.wait_for_any_edge().await;
        Ok(())
    }
}

impl<'d> embedded_hal_async::digital::Wait for Input<'d> {
    /// Returns Ok after the high-level IRQ; no timeout, and cancellation disarms this wait.
    async fn wait_for_high(&mut self) -> Result<(), Self::Error> {
        self.wait_for_high().await;
        Ok(())
    }

    /// Returns Ok after the low-level IRQ; no timeout, and cancellation disarms this wait.

    async fn wait_for_low(&mut self) -> Result<(), Self::Error> {
        self.wait_for_low().await;
        Ok(())
    }

    /// Returns Ok after a newly armed rising-edge IRQ; no debounce/timeout, cancellation disarms.

    async fn wait_for_rising_edge(&mut self) -> Result<(), Self::Error> {
        self.wait_for_rising_edge().await;
        Ok(())
    }

    /// Returns Ok after a newly armed falling-edge IRQ; no debounce/timeout, cancellation disarms.

    async fn wait_for_falling_edge(&mut self) -> Result<(), Self::Error> {
        self.wait_for_falling_edge().await;
        Ok(())
    }

    /// Returns Ok after either newly armed edge IRQ; no debounce/timeout, cancellation disarms.

    async fn wait_for_any_edge(&mut self) -> Result<(), Self::Error> {
        self.wait_for_any_edge().await;
        Ok(())
    }
}

impl<'d> embedded_hal_async::digital::Wait for OutputOpenDrain<'d> {
    /// Returns Ok after the high-level IRQ; no timeout, and cancellation disarms this wait.
    async fn wait_for_high(&mut self) -> Result<(), Self::Error> {
        self.wait_for_high().await;
        Ok(())
    }

    /// Returns Ok after the low-level IRQ; no timeout, and cancellation disarms this wait.

    async fn wait_for_low(&mut self) -> Result<(), Self::Error> {
        self.wait_for_low().await;
        Ok(())
    }

    /// Returns Ok after a newly armed rising-edge IRQ; no debounce/timeout, cancellation disarms.

    async fn wait_for_rising_edge(&mut self) -> Result<(), Self::Error> {
        self.wait_for_rising_edge().await;
        Ok(())
    }

    /// Returns Ok after a newly armed falling-edge IRQ; no debounce/timeout, cancellation disarms.

    async fn wait_for_falling_edge(&mut self) -> Result<(), Self::Error> {
        self.wait_for_falling_edge().await;
        Ok(())
    }

    /// Returns Ok after either newly armed edge IRQ; no debounce/timeout, cancellation disarms.

    async fn wait_for_any_edge(&mut self) -> Result<(), Self::Error> {
        self.wait_for_any_edge().await;
        Ok(())
    }
}
