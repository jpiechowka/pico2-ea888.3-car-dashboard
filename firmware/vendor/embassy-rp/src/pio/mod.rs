//! Four state machines per PIO block, shared 32-word instruction RAM and per-machine FIFOs.
//!
//! FIFO/DMA completion means words moved, not program or wire completion. Pending FIFO
//! futures mask their IRQ on drop; DMA drop aborts/waits for DMA but preserves PIO state.
//! Programs/pins are shared across machines: retain instruction reservations and coordinate
//! RP235x GPIOBASE changes for the whole block. See the
//! [Pico SDK PIO contract](https://www.raspberrypi.com/documentation/pico-sdk/hardware.html#hardware_pio).
use core::future::Future;
use core::marker::PhantomData;
use core::pin::Pin as FuturePin;
use core::sync::atomic::{AtomicU8, AtomicU32, Ordering};
use core::task::{Context, Poll};

use embassy_hal_internal::{Peri, PeripheralType};
use embassy_sync::waitqueue::AtomicWaker;
use fixed::FixedU32;
use fixed::types::extra::U8;
use pio::{Program, SideSet, Wrap};

use crate::dma::{self, Transfer, Word};
use crate::gpio::{self, AnyPin, Drive, Level, Pull, SealedPin, SlewRate};
use crate::interrupt::typelevel::{Binding, Handler, Interrupt};
use crate::relocate::RelocatedProgram;
use crate::{RegExt, pac, peripherals};

/// Pure enable-mask update and application for batched state-machine control.
mod batch;

/// Unsafe injected instruction helpers and their shift/FIFO preconditions.
mod instr;

#[doc(inline)]
pub use pio as program;

/// Wakers for interrupts and FIFOs.
pub struct Wakers(
    /// RX waiters0..4, TX waiters4..8 and IRQ flag waiters8..12.
    [AtomicWaker; 12],
);

impl Wakers {
    /// Returns the four RX-not-empty waiters indexed by state machine.
    #[inline(always)]
    fn fifo_in(&self) -> &[AtomicWaker] { &self.0[0..4] }

    /// Returns the four TX-not-full waiters indexed by state machine.
    #[inline(always)]
    fn fifo_out(&self) -> &[AtomicWaker] { &self.0[4..8] }

    /// Returns the four CPU-visible PIO IRQ flag waiters (flags 0..=3).
    #[inline(always)]
    fn irq(&self) -> &[AtomicWaker] { &self.0[8..12] }
}

/// FIFO config.
#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[repr(u8)]
pub enum FifoJoin {
    /// Both TX and RX fifo is enabled
    #[default]
    Duplex,
    /// Rx fifo twice as deep. TX fifo disabled
    RxOnly,
    /// Tx fifo twice as deep. RX fifo disabled
    TxOnly,
    /// Enable random writes (`FJOIN_RX_PUT`) from the state machine (through ISR),
    /// and random reads from the system (using [`StateMachine::get_rxf_entry`]).
    #[cfg(feature = "_rp235x")]
    RxAsStatus,
    /// Enable random reads (`FJOIN_RX_GET`) from the state machine (through OSR),
    /// and random writes from the system (using [`StateMachine::set_rxf_entry`]).
    #[cfg(feature = "_rp235x")]
    RxAsControl,
    /// FJOIN_RX_PUT | FJOIN_RX_GET: RX can be used as a scratch register,
    /// not accessible from the CPU
    #[cfg(feature = "_rp235x")]
    PioScratch,
}

/// Shift direction.
#[allow(missing_docs)]
#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[repr(u8)]
pub enum ShiftDirection {
    #[default]
    /// Shifts toward bit 0; serial order depends on the instruction and configured threshold.
    Right = 1,
    /// Shifts toward bit 31; serial order depends on the instruction and configured threshold.
    Left = 0,
}

/// Pin direction.
#[allow(missing_docs)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[repr(u8)]
pub enum Direction {
    /// Releases PIO output enable for the selected pins.
    In = 0,
    /// Enables PIO output drivers for the selected pins.
    Out = 1,
}

/// Which fifo level to use in status check.
#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[repr(u8)]
pub enum StatusSource {
    #[default]
    /// All-ones if TX FIFO level < N, otherwise all-zeroes.
    TxFifoLevel = 0,
    /// All-ones if RX FIFO level < N, otherwise all-zeroes.
    RxFifoLevel = 1,
    /// All-ones if the indexed IRQ flag is raised, otherwise all-zeroes
    #[cfg(feature = "_rp235x")]
    Irq = 2,
}

/// IRQ0 RX-not-empty bit for SM0; shift by the machine index.
const RXNEMPTY_MASK: u32 = 1 << 0;
/// IRQ0 TX-not-full bit for SM0; shift by the machine index.
const TXNFULL_MASK: u32 = 1 << 4;
/// IRQ0 flag 0 bit; shift by the CPU-visible IRQ flag index 0..=3.
const SMIRQ_MASK: u32 = 1 << 8;

/// Interrupt handler for PIO.
pub struct InterruptHandler<PIO: Instance> {
    /// PIO instance identity without runtime storage.
    _pio: PhantomData<PIO>,
}

impl<PIO: Instance> Handler<PIO::Interrupt> for InterruptHandler<PIO> {
    /// Wakes asserted FIFO/IRQ waiters and masks those IRQ0 sources until repolled.
    ///
    /// # Safety
    /// Run only for this PIO instance with initialized wakers and exclusive IRQ0 ownership.
    unsafe fn on_interrupt() {
        let ints = PIO::PIO.irqs(0).ints().read().0;
        for bit in 0..12 {
            if ints & (1 << bit) != 0 {
                PIO::wakers().0[bit].wake();
            }
        }
        PIO::PIO.irqs(0).inte().write_clear(|m| m.0 = ints);
    }
}

/// Future that waits for TX-FIFO to become writable
#[must_use = "futures do nothing unless you `.await` or poll them"]
pub struct FifoOutFuture<'a, 'd, PIO: Instance, const SM: usize> {
    /// Exclusive TX handle retained until enqueue or cancellation.
    sm_tx: &'a mut StateMachineTx<'d, PIO, SM>,
    /// Word to enqueue exactly once after space becomes available.
    value: u32,
}

impl<'a, 'd, PIO: Instance, const SM: usize> FifoOutFuture<'a, 'd, PIO, SM> {
    /// Returns an unpolled future borrowing `sm` and retaining `value` until TX space is available.
    pub fn new(
        sm: &'a mut StateMachineTx<'d, PIO, SM>,
        value: u32,
    ) -> Self {
        FifoOutFuture { sm_tx: sm, value }
    }
}

impl<'a, 'd, PIO: Instance, const SM: usize> Future for FifoOutFuture<'a, 'd, PIO, SM> {
    /// Unit after one word enters the TX FIFO; not proof that the program transmitted it.
    type Output = ();

    /// Returns Ready after `self` queues one word; otherwise registers `cx` for TX space.
    fn poll(
        self: FuturePin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Self::Output> {
        // debug!("Poll {},{}", PIO::PIO_NO, SM);
        let value = self.value;
        if self.get_mut().sm_tx.try_push(value) {
            Poll::Ready(())
        } else {
            PIO::wakers().fifo_out()[SM].register(cx.waker());
            PIO::PIO.irqs(0).inte().write_set(|m| {
                m.0 = TXNFULL_MASK << SM;
            });
            // debug!("Pending");
            Poll::Pending
        }
    }
}

impl<'a, 'd, PIO: Instance, const SM: usize> Drop for FifoOutFuture<'a, 'd, PIO, SM> {
    /// Masks TX-not-full IRQ; already queued words are not removed.
    fn drop(&mut self) {
        PIO::PIO.irqs(0).inte().write_clear(|m| {
            m.0 = TXNFULL_MASK << SM;
        });
    }
}

/// Future that waits for RX-FIFO to become readable.
#[must_use = "futures do nothing unless you `.await` or poll them"]
pub struct FifoInFuture<'a, 'd, PIO: Instance, const SM: usize> {
    /// Exclusive RX consumer retained until one word is popped or cancelled.
    sm_rx: &'a mut StateMachineRx<'d, PIO, SM>,
}

impl<'a, 'd, PIO: Instance, const SM: usize> FifoInFuture<'a, 'd, PIO, SM> {
    /// Returns an unpolled future borrowing `sm`; no RX word is consumed until polling succeeds.
    pub fn new(sm: &'a mut StateMachineRx<'d, PIO, SM>) -> Self { FifoInFuture { sm_rx: sm } }
}

impl<'a, 'd, PIO: Instance, const SM: usize> Future for FifoInFuture<'a, 'd, PIO, SM> {
    /// One word removed from the RX FIFO.
    type Output = u32;

    /// Returns Ready with one word popped by `self`, or registers `cx` for RX data and returns Pending.
    fn poll(
        mut self: FuturePin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Self::Output> {
        // debug!("Poll {},{}", PIO::PIO_NO, SM);
        if let Some(v) = self.sm_rx.try_pull() {
            Poll::Ready(v)
        } else {
            PIO::wakers().fifo_in()[SM].register(cx.waker());
            PIO::PIO.irqs(0).inte().write_set(|m| {
                m.0 = RXNEMPTY_MASK << SM;
            });
            // debug!("Pending");
            Poll::Pending
        }
    }
}

impl<'a, 'd, PIO: Instance, const SM: usize> Drop for FifoInFuture<'a, 'd, PIO, SM> {
    /// Masks RX-not-empty IRQ; pending cancellation consumes no word.
    fn drop(&mut self) {
        PIO::PIO.irqs(0).inte().write_clear(|m| {
            m.0 = RXNEMPTY_MASK << SM;
        });
    }
}

/// Future that waits for IRQ
#[must_use = "futures do nothing unless you `.await` or poll them"]
pub struct IrqFuture<'a, 'd, PIO: Instance> {
    /// Exclusive IRQ borrow; the actual flag index is stored separately.
    pio: PhantomData<&'a mut Irq<'d, PIO, 0>>,
    /// CPU-visible PIO flag index 0..=3.
    irq_no: u8,
}

impl<'a, 'd, PIO: Instance> Future for IrqFuture<'a, 'd, PIO> {
    /// Unit after the selected IRQ flag is observed and cleared.
    type Output = ();

    /// Returns Ready after `self` observes/clears its IRQ flag, or registers `cx` and returns Pending.
    fn poll(
        self: FuturePin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Self::Output> {
        // debug!("Poll {},{}", PIO::PIO_NO, SM);

        // Check if IRQ flag is already set
        if PIO::PIO.irq().read().0 & (1 << self.irq_no) != 0 {
            PIO::PIO.irq().write(|m| m.0 = 1 << self.irq_no);
            return Poll::Ready(());
        }

        PIO::wakers().irq()[self.irq_no as usize].register(cx.waker());
        PIO::PIO.irqs(0).inte().write_set(|m| {
            m.0 = SMIRQ_MASK << self.irq_no;
        });
        Poll::Pending
    }
}

impl<'a, 'd, PIO: Instance> Drop for IrqFuture<'a, 'd, PIO> {
    /// Masks this IRQ source without clearing a still-pending PIO flag.
    fn drop(&mut self) {
        PIO::PIO.irqs(0).inte().write_clear(|m| {
            m.0 = SMIRQ_MASK << self.irq_no;
        });
    }
}

/// Returns the shared physical GPIO window base for this PIO instance.
fn pio_pin_base<PIO: Instance>() -> u8 {
    #[cfg(feature = "rp2040")]
    {
        0
    }
    #[cfg(feature = "_rp235x")]
    {
        if PIO::PIO.gpiobase().read().gpiobase() { 16 } else { 0 }
    }
}

/// Returns the bypass bit for physical `pin` in window `base`; panics outside the 32-pin window.
fn input_sync_bypass_mask(
    pin: u8,
    base: u8,
) -> u32 {
    let relative = pin.checked_sub(base).expect("pin below PIO GPIOBASE");
    assert!(relative < 32, "pin above PIO GPIOBASE window");
    1 << relative
}

/// Returns the physical side-set pin count from encoded `bits` and `optional` enable-bit policy.
/// Panics if optional enable is requested with no encoded bit.
#[cfg(feature = "_rp235x")]
fn side_set_pin_count(
    bits: u8,
    optional: bool,
) -> u8 {
    bits.checked_sub(u8::from(optional))
        .expect("optional side-set requires an enable bit")
}

/// Returns a common GPIO window base and relative JMP selector for `ranges` and explicit `jmp`.
/// None leaves JMP at relative 0. Panics if no 32-pin window contains all selected physical pins.
#[cfg(feature = "_rp235x")]
fn pin_window(
    ranges: [core::ops::Range<u8>; 4],
    jmp: Option<u8>,
) -> (u8, u8) {
    let mut low_ok = true;
    let mut high_ok = true;
    for pin in ranges.into_iter().flatten().chain(jmp) {
        low_ok &= pin < 32;
        high_ok &= (16..48).contains(&pin);
    }
    assert!(low_ok || high_ok, "PIO pins must share a32-pin GPIOBASE window");
    let base = if low_ok { 0 } else { 16 };
    (base, jmp.map_or(0, |pin| pin - base))
}

/// Type representing a PIO pin.
pub struct Pin<'l, PIO: Instance> {
    /// Physical GPIO token; registration lifetime is owned by Common and the state machines.
    pin: Peri<'l, AnyPin>,
    /// PIO identity owning this pin registration.
    pio: PhantomData<PIO>,
}

impl<'l, PIO: Instance> Pin<'l, PIO> {
    /// Sets pad drive `strength`; current rating is a configuration, not an electrical load guarantee.
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

    /// Sets output transition policy to `slew_rate`; does not change state-machine timing.
    #[inline]
    pub fn set_slew_rate(
        &mut self,
        slew_rate: SlewRate,
    ) {
        self.pin.pad_ctrl().modify(|w| {
            w.set_slewfast(slew_rate == SlewRate::Fast);
        });
    }

    /// Sets weak resistor policy `pull`, enabling at most one pull direction.
    #[inline]
    pub fn set_pull(
        &mut self,
        pull: Pull,
    ) {
        self.pin.pad_ctrl().modify(|w| {
            w.set_pue(pull == Pull::Up);
            w.set_pde(pull == Pull::Down);
        });
    }

    /// Sets input Schmitt hysteresis to `enable`; does not alter synchronizer bypass.
    #[inline]
    pub fn set_schmitt(
        &mut self,
        enable: bool,
    ) {
        self.pin.pad_ctrl().modify(|w| {
            w.set_schmitt(enable);
        });
    }

    /// Sets output logic inversion to `invert`, independently of output-enable polarity.
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

    /// Sets output-enable inversion to `invert`, independently of pin data.
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

    /// Sets `bypass` for this PIO input; bypass requires externally synchronous timing to avoid metastability.
    /// Panics outside the current GPIOBASE window; configure a high window before bypassing its pins.
    pub fn set_input_sync_bypass(
        &mut self,
        bypass: bool,
    ) {
        let mask = input_sync_bypass_mask(self.pin(), pio_pin_base::<PIO>());
        if bypass {
            PIO::PIO.input_sync_bypass().write_set(|w| *w = mask);
        } else {
            PIO::PIO.input_sync_bypass().write_clear(|w| *w = mask);
        }
    }

    /// Returns the physical BANK0 GPIO number, independent of the PIO GPIOBASE window.
    pub fn pin(&self) -> u8 { self.pin._pin() }
}

/// Type representing a state machine RX FIFO.
pub struct StateMachineRx<'d, PIO: Instance, const SM: usize> {
    /// PIO lifetime/instance identity; this handle owns one RX consumer.
    pio: PhantomData<&'d mut PIO>,
}

impl<'d, PIO: Instance, const SM: usize> StateMachineRx<'d, PIO, SM> {
    /// Returns whether RX has no word available; no FIFO read occurs.
    pub fn empty(&self) -> bool { PIO::PIO.fstat().read().rxempty() & (1u8 << SM) != 0 }

    /// Returns whether RX has no free slot under the current join mode.
    pub fn full(&self) -> bool { PIO::PIO.fstat().read().rxfull() & (1u8 << SM) != 0 }

    /// Returns the current RX word count (normally 0..4 or 0..8 when joined).
    pub fn level(&self) -> u8 { (PIO::PIO.flevel().read().0 >> (SM * 8 + 4)) as u8 & 0x0F }

    /// Returns and clears the sticky RX-full stall flag, not the current execution state.
    pub fn stalled(&self) -> bool {
        let fdebug = PIO::PIO.fdebug();
        let ret = fdebug.read().rxstall() & (1 << SM) != 0;
        if ret {
            fdebug.write(|w| w.set_rxstall(1 << SM));
        }
        ret
    }

    /// Returns and clears the sticky CPU-read-on-empty RX flag; an empty read returns unspecified data.
    pub fn underflowed(&self) -> bool {
        let fdebug = PIO::PIO.fdebug();
        let ret = fdebug.read().rxunder() & (1 << SM) != 0;
        if ret {
            fdebug.write(|w| w.set_rxunder(1 << SM));
        }
        ret
    }

    /// Pops and returns one RX word without checking availability; empty reads return unspecified data and set
    /// underflow.
    pub fn pull(&mut self) -> u32 { PIO::PIO.rxf(SM).read() }

    /// Returns Some(popped word) when RX is nonempty, otherwise None without reading it.
    pub fn try_pull(&mut self) -> Option<u32> {
        if self.empty() {
            return None;
        }
        Some(self.pull())
    }

    /// Returns a future consuming one RX word when available; dropping it masks its IRQ without consuming data.
    pub fn wait_pull<'a>(&'a mut self) -> FifoInFuture<'a, 'd, PIO, SM> { FifoInFuture::new(self) }

    /// Returns this instance/machine RX DMA request selector.
    fn dreq() -> crate::pac::dma::vals::TreqSel { crate::pac::dma::vals::TreqSel::from(PIO::PIO_NO * 8 + SM as u8 + 4) }

    /// Starts DMA `ch` from RX into `data` and returns its transfer guard; `bswap` reverses bytes within each W.
    /// Length is in W elements. Guard drop aborts/waits for DMA; received prefix and PIO state remain. No timeout.
    pub fn dma_pull<'a, W: Word>(
        &'a mut self,
        ch: &'a mut dma::Channel<'_>,
        data: &'a mut [W],
        bswap: bool,
    ) -> Transfer<'a> {
        unsafe { ch.read(PIO::PIO.rxf(SM).as_ptr() as *const W, data, Self::dreq(), bswap) }
    }

    /// Starts DMA `ch` discarding `len` RX words and returns its transfer guard.
    /// The W parameter does not change the u32 FIFO read width here; drop aborts/waits without resetting PIO.
    pub fn dma_pull_discard<'a, W: Word>(
        &'a mut self,
        ch: &'a mut dma::Channel<'_>,
        len: usize,
    ) -> Transfer<'a> {
        unsafe { ch.read_discard(PIO::PIO.rxf(SM).as_ptr(), len, Self::dreq()) }
    }
}

/// Type representing a state machine TX FIFO.
pub struct StateMachineTx<'d, PIO: Instance, const SM: usize> {
    /// PIO lifetime/instance identity; this handle owns one TX producer.
    pio: PhantomData<&'d mut PIO>,
}

impl<'d, PIO: Instance, const SM: usize> StateMachineTx<'d, PIO, SM> {
    /// Returns whether TX has no queued word; OSR/program activity may continue.
    pub fn empty(&self) -> bool { PIO::PIO.fstat().read().txempty() & (1u8 << SM) != 0 }

    /// Returns whether TX has no free slot under the current join mode.
    pub fn full(&self) -> bool { PIO::PIO.fstat().read().txfull() & (1u8 << SM) != 0 }

    /// Returns the current TX word count (normally 0..4 or 0..8 when joined).
    pub fn level(&self) -> u8 { (PIO::PIO.flevel().read().0 >> (SM * 8)) as u8 & 0x0F }

    /// Returns and clears the sticky TX-empty stall flag, not the current execution state.
    pub fn stalled(&self) -> bool {
        let fdebug = PIO::PIO.fdebug();
        let ret = fdebug.read().txstall() & (1 << SM) != 0;
        if ret {
            fdebug.write(|w| w.set_txstall(1 << SM));
        }
        ret
    }

    /// Returns and clears the sticky CPU-write-on-full TX flag; the overflowing word was discarded.
    pub fn overflowed(&self) -> bool {
        let fdebug = PIO::PIO.fdebug();
        let ret = fdebug.read().txover() & (1 << SM) != 0;
        if ret {
            fdebug.write(|w| w.set_txover(1 << SM));
        }
        ret
    }

    /// Writes `v` without checking TX space; a full FIFO discards it and sets overflow.
    pub fn push(
        &mut self,
        v: u32,
    ) {
        PIO::PIO.txf(SM).write_value(v);
    }

    /// Returns true after queuing `v`, or false without a write if TX is full; does not wait for execution.
    pub fn try_push(
        &mut self,
        v: u32,
    ) -> bool {
        if self.full() {
            return false;
        }
        self.push(v);
        true
    }

    /// Returns a future queuing `value` when TX space exists; pending cancellation does not queue it.
    pub fn wait_push<'a>(
        &'a mut self,
        value: u32,
    ) -> FifoOutFuture<'a, 'd, PIO, SM> {
        FifoOutFuture::new(self, value)
    }

    /// Returns this instance/machine TX DMA request selector.
    fn dreq() -> crate::pac::dma::vals::TreqSel { crate::pac::dma::vals::TreqSel::from(PIO::PIO_NO * 8 + SM as u8) }

    /// Starts DMA `ch` from `data` into TX and returns its transfer guard; `bswap` reverses bytes within each W.
    /// Completion means FIFO admission, not execution. Drop aborts/waits for DMA; queued words remain.
    pub fn dma_push<'a, W: Word>(
        &'a mut self,
        ch: &'a mut dma::Channel<'_>,
        data: &'a [W],
        bswap: bool,
    ) -> Transfer<'a> {
        unsafe { ch.write(data, PIO::PIO.txf(SM).as_ptr() as *mut W, Self::dreq(), bswap) }
    }

    /// Starts DMA `ch` writing `len` zero W elements into TX and returns its guard.
    /// Drop aborts/waits for DMA; already queued words remain and may execute afterward.
    pub fn dma_push_zeros<'a, W: Word>(
        &'a mut self,
        ch: &'a mut dma::Channel<'_>,
        len: usize,
    ) -> Transfer<'a> {
        unsafe { ch.write_zeros(len, PIO::PIO.txf(SM).as_ptr() as *mut W, Self::dreq()) }
    }
}

/// A type representing a single PIO state machine.
pub struct StateMachine<'d, PIO: Instance, const SM: usize> {
    /// Exclusively owned CPU/DMA RX FIFO interface.
    rx: StateMachineRx<'d, PIO, SM>,
    /// Exclusively owned CPU/DMA TX FIFO interface.
    tx: StateMachineTx<'d, PIO, SM>,
}

impl<'d, PIO: Instance, const SM: usize> Drop for StateMachine<'d, PIO, SM> {
    /// Disables this machine, then releases its pin-registration reference; FIFO/program state remains.
    fn drop(&mut self) {
        PIO::PIO.ctrl().write_clear(|w| w.set_sm_enable(1 << SM));
        on_pio_drop::<PIO>();
    }
}

/// Panics unless `pins` are strictly consecutive physical GPIOs; empty/singleton slices pass, wraparound does not.
fn assert_consecutive<PIO: Instance>(pins: &[&Pin<PIO>]) {
    for (p1, p2) in pins.iter().zip(pins.iter().skip(1)) {
        // purposely does not allow wrap-around because we can't claim pins 30 and 31.
        assert!(p1.pin() + 1 == p2.pin(), "pins must be consecutive");
    }
}

/// PIO Execution config.
#[derive(Clone, Copy, Default, Debug)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[non_exhaustive]
pub struct ExecConfig {
    /// If true, the MSB of the Delay/Side-set instruction field is used as side-set enable, rather than a side-set
    /// data bit.
    pub side_en: bool,
    /// If true, side-set data is asserted to pin directions, instead of pin values.
    pub side_pindir: bool,
    /// Physical BANK0 pin tested by JMP PIN; translated relative to GPIOBASE when applied.
    pub jmp_pin: u8,
    /// After reaching this address, execution is wrapped to wrap_bottom.
    pub wrap_top: u8,
    /// After reaching wrap_top, execution is wrapped to this address.
    pub wrap_bottom: u8,
}

/// PIO shift register config for input or output.
#[derive(Clone, Copy, Default, Debug)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct ShiftConfig {
    /// Shifted-bit threshold 0..=32: zero and 32 encode32; applies to ISR autopush or OSR autopull.
    pub threshold: u8,
    /// Shift direction.
    pub direction: ShiftDirection,
    /// For output: Pull automatically output shift register is emptied.
    /// For input: Push automatically when the input shift register is filled.
    pub auto_fill: bool,
}

/// PIO pin config.
#[derive(Clone, Copy, Default, Debug)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct PinConfig {
    /// The number of MSBs of the Delay/Side-set instruction field which are used for side-set.
    pub sideset_count: u8,
    /// The number of pins asserted by a SET. In the range 0 to 5 inclusive.
    pub set_count: u8,
    /// The number of pins asserted by an OUT PINS, OUT PINDIRS or MOV PINS instruction. In the range 0 to 32
    /// inclusive.
    pub out_count: u8,
    /// The pin which is mapped to the least-significant bit of a state machine's IN data bus.
    pub in_base: u8,
    /// The lowest-numbered pin that will be affected by a side-set operation.
    pub sideset_base: u8,
    /// The lowest-numbered pin that will be affected by a SET PINS or SET PINDIRS instruction.
    pub set_base: u8,
    /// The lowest-numbered pin that will be affected by an OUT PINS, OUT PINDIRS or MOV PINS instruction.
    pub out_base: u8,
}

/// Comparison level or IRQ index for the MOV x, STATUS instruction.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[cfg(feature = "_rp235x")]
pub enum StatusN {
    /// Local IRQ flag index or FIFO comparison threshold, according to StatusSource.
    This(
        /// IRQ index 0..=7 when StatusSource::Irq; This also carries FIFO comparison levels.
        u8,
    ),
    /// IRQ flag in the next lower PIO block
    Lower(
        /// IRQ index 0..=7 when StatusSource::Irq; This also carries FIFO comparison levels.
        u8,
    ),
    /// IRQ flag in the next higher PIO block
    Higher(
        /// IRQ index 0..=7 when StatusSource::Irq; This also carries FIFO comparison levels.
        u8,
    ),
}

#[cfg(feature = "_rp235x")]
impl Default for StatusN {
    /// Returns the local IRQ/status selector with comparison/index 0.
    fn default() -> Self { Self::This(0) }
}

#[cfg(feature = "_rp235x")]
impl Into<crate::pac::pio::vals::ExecctrlStatusN> for StatusN {
    /// Returns the raw STATUS_N selector encoded from `self`; IRQ indices must be0..=7.
    /// Out-of-range values are not validated and may overflow or select reserved hardware encodings.
    fn into(self) -> crate::pac::pio::vals::ExecctrlStatusN {
        let x = match self {
            StatusN::This(n) => n,
            StatusN::Lower(n) => n + 0x08,
            StatusN::Higher(n) => n + 0x10,
        };

        crate::pac::pio::vals::ExecctrlStatusN(x)
    }
}

/// PIO config.
#[derive(Clone, Copy, Debug)]
pub struct Config<'d, PIO: Instance> {
    /// System-clock divider1..=65536 in 16.8 fixed point; fractional values introduce cycle jitter.
    pub clock_divider: FixedU32<U8>,
    /// Which data bit to use for inline OUT enable.
    pub out_en_sel: u8,
    /// Use a bit of OUT data as an auxiliary write enable When used in conjunction with OUT_STICKY.
    pub inline_out_en: bool,
    /// Continuously assert the most recent OUT/SET to the pins.
    pub out_sticky: bool,
    /// Which source to use for checking status.
    pub status_sel: StatusSource,
    /// Status comparison level.
    #[cfg(feature = "rp2040")]
    pub status_n: u8,
    // This cfg probably shouldn't be required, but the SVD for the 2040 doesn't have the enum
    #[cfg(feature = "_rp235x")]
    /// Status comparison level.
    pub status_n: StatusN,
    /// Stored execution fields, applied without pausing by set_config.
    exec: ExecConfig,
    /// Whether JMP_PIN was explicitly supplied as a physical GPIO instead of default relative 0.
    jmp_pin_explicit: bool,
    /// Optional instruction address to jump to when applying; None leaves PC unchanged.
    origin: Option<u8>,
    /// Configure FIFO allocation.
    pub fifo_join: FifoJoin,
    /// Input shifting config.
    pub shift_in: ShiftConfig,
    /// Output shifting config.
    pub shift_out: ShiftConfig,
    // PINCTRL
    /// Physical pin bases/counts before GPIOBASE translation.
    pins: PinConfig,
    /// Input pin count; RP235x writes its five-bit encoding (zero represents32).
    in_count: u8,
    /// Binds configuration to the PIO peripheral lifetime.
    _pio: PhantomData<&'d mut PIO>,
}

impl<'d, PIO: Instance> Default for Config<'d, PIO> {
    /// Returns divisor 1, duplex FIFOs, right shifting without auto-fill, zero pin counts and no initial jump.
    fn default() -> Self {
        Self {
            clock_divider: 1u8.into(),
            out_en_sel: Default::default(),
            inline_out_en: Default::default(),
            out_sticky: Default::default(),
            status_sel: Default::default(),
            status_n: Default::default(),
            exec: Default::default(),
            jmp_pin_explicit: false,
            origin: Default::default(),
            fifo_join: Default::default(),
            shift_in: Default::default(),
            shift_out: Default::default(),
            pins: Default::default(),
            in_count: Default::default(),
            _pio: Default::default(),
        }
    }
}

impl<'d, PIO: Instance> Config<'d, PIO> {
    /// Returns a copy of the stored execution configuration; no register access.
    pub fn get_exec(&self) -> ExecConfig { self.exec }

    /// Stores raw execution settings `e` without hardware writes.
    ///
    /// # Safety
    /// Ensure referenced pins/instructions belong to this PIO allocation, fields fit hardware widths,
    /// and later execution cannot drive unowned pins or access released instruction memory.
    /// Supplied JMP_PIN is explicitly physical, including set_exec(get_exec()) with the default 0;
    /// it must share the selected window even when the program does not execute JMP PIN.
    pub unsafe fn set_exec(
        &mut self,
        e: ExecConfig,
    ) {
        self.exec = e;
        self.jmp_pin_explicit = true;
    }

    /// Returns a copy of stored physical pin mapping; no register access.
    pub fn get_pins(&self) -> PinConfig { self.pins }

    /// Stores raw physical pin mapping `p` without hardware writes.
    ///
    /// # Safety
    /// All selected pin ranges must be owned by this PIO, fit register count widths and share a
    /// valid GPIOBASE window; coordinate other machines before applying the configuration.
    pub unsafe fn set_pins(
        &mut self,
        p: PinConfig,
    ) {
        self.pins = p;
    }

    /// Records `prog` metadata for later configuration, including jumping to the origin
    /// of the program. The state machine is not started.
    ///
    /// `side_set` sets the range of pins affected by side-sets. The range must be consecutive.
    /// Sideset pins must configured as outputs using [`StateMachine::set_pin_dirs`] to be
    /// effective.
    pub fn use_program(
        &mut self,
        prog: &LoadedProgram<'d, PIO>,
        side_set: &[&Pin<'d, PIO>],
    ) {
        assert!((prog.side_set.bits() - prog.side_set.optional() as u8) as usize == side_set.len());
        assert_consecutive(side_set);
        self.exec.side_en = prog.side_set.optional();
        self.exec.side_pindir = prog.side_set.pindirs();
        self.exec.wrap_bottom = prog.wrap.target;
        self.exec.wrap_top = prog.wrap.source;
        self.pins.sideset_count = prog.side_set.bits();
        self.pins.sideset_base = side_set.first().map_or(0, |p| p.pin());
        self.origin = Some(prog.origin);
    }

    /// Selects physical `pin` for JMP PIN; caller must configure its input direction and synchronizer policy.
    pub fn set_jmp_pin(
        &mut self,
        pin: &Pin<'d, PIO>,
    ) {
        self.exec.jmp_pin = pin.pin();
        self.jmp_pin_explicit = true;
    }

    /// Maps SET to consecutive `pins`; panics for more than 5 or nonconsecutive pins.
    /// Empty input disables SET pin writes; output direction must be configured separately.
    pub fn set_set_pins(
        &mut self,
        pins: &[&Pin<'d, PIO>],
    ) {
        assert!(pins.len() <= 5);
        assert_consecutive(pins);
        self.pins.set_base = pins.first().map_or(0, |p| p.pin());
        self.pins.set_count = pins.len() as u8;
    }

    /// Maps OUT/MOV to consecutive `pins`; panics for gaps. Caller must restrict the count to 32.
    /// Empty input disables mapped output; output directions are configured separately.
    pub fn set_out_pins(
        &mut self,
        pins: &[&Pin<'d, PIO>],
    ) {
        assert_consecutive(pins);
        self.pins.out_base = pins.first().map_or(0, |p| p.pin());
        self.pins.out_count = pins.len() as u8;
    }

    /// Maps IN from consecutive `pins`; panics for gaps. Caller must restrict the count to 32.
    /// RP2040 stores only the base; RP235x also configures the input count. Set input directions separately.
    pub fn set_in_pins(
        &mut self,
        pins: &[&Pin<'d, PIO>],
    ) {
        assert_consecutive(pins);
        self.pins.in_base = pins.first().map_or(0, |p| p.pin());
        self.in_count = pins.len() as u8;
    }
}

impl<'d, PIO: Instance + 'd, const SM: usize> StateMachine<'d, PIO, SM> {
    /// Applies `config` and optionally jumps to its program origin; does not pause or enable the machine.
    /// Panics before hardware writes for divisor outside 1..=65536, OUT selector>=32, shift threshold>32 or
    /// incompatible RP235x pin windows, including explicitly selected JMP pins. GPIOBASE is
    /// shared by all four machines: coordinate every machine before changing its window.
    pub fn set_config(
        &mut self,
        config: &Config<'d, PIO>,
    ) {
        // sm expects 0 for 65536, truncation makes that happen
        assert!(config.clock_divider <= 65536, "clkdiv must be <= 65536");
        assert!(config.clock_divider >= 1, "clkdiv must be >= 1");
        assert!(config.out_en_sel < 32, "out_en_sel must be < 32");
        // assert!(config.status_n < 32, "status_n must be < 32");
        // sm expects 0 for 32, truncation makes that happen
        assert!(config.shift_in.threshold <= 32, "shift_in.threshold must be <= 32");
        assert!(config.shift_out.threshold <= 32, "shift_out.threshold must be <= 32");
        #[cfg(feature = "_rp235x")]
        let (shift, jmp_pin) = pin_window(
            [
                config.pins.in_base..config.pins.in_base + config.in_count,
                config.pins.sideset_base
                    ..config.pins.sideset_base + side_set_pin_count(config.pins.sideset_count, config.exec.side_en),
                config.pins.set_base..config.pins.set_base + config.pins.set_count,
                config.pins.out_base..config.pins.out_base + config.pins.out_count,
            ],
            config.jmp_pin_explicit.then_some(config.exec.jmp_pin),
        );
        let sm = Self::this_sm();
        sm.clkdiv().write(|w| w.0 = config.clock_divider.to_bits() << 8);
        sm.execctrl().write(|w| {
            w.set_side_en(config.exec.side_en);
            w.set_side_pindir(config.exec.side_pindir);
            #[cfg(feature = "rp2040")]
            w.set_jmp_pin(config.exec.jmp_pin);
            #[cfg(feature = "_rp235x")]
            w.set_jmp_pin(jmp_pin);
            w.set_out_en_sel(config.out_en_sel);
            w.set_inline_out_en(config.inline_out_en);
            w.set_out_sticky(config.out_sticky);
            w.set_wrap_top(config.exec.wrap_top);
            w.set_wrap_bottom(config.exec.wrap_bottom);
            #[cfg(feature = "_rp235x")]
            w.set_status_sel(match config.status_sel {
                StatusSource::TxFifoLevel => pac::pio::vals::ExecctrlStatusSel::TXLEVEL,
                StatusSource::RxFifoLevel => pac::pio::vals::ExecctrlStatusSel::RXLEVEL,
                StatusSource::Irq => pac::pio::vals::ExecctrlStatusSel::IRQ,
            });
            #[cfg(feature = "rp2040")]
            w.set_status_sel(match config.status_sel {
                StatusSource::TxFifoLevel => pac::pio::vals::SmExecctrlStatusSel::TXLEVEL,
                StatusSource::RxFifoLevel => pac::pio::vals::SmExecctrlStatusSel::RXLEVEL,
            });
            w.set_status_n(config.status_n.into());
        });
        sm.shiftctrl().write(|w| {
            w.set_fjoin_rx(config.fifo_join == FifoJoin::RxOnly);
            w.set_fjoin_tx(config.fifo_join == FifoJoin::TxOnly);
            w.set_pull_thresh(config.shift_out.threshold);
            w.set_push_thresh(config.shift_in.threshold);
            w.set_out_shiftdir(config.shift_out.direction == ShiftDirection::Right);
            w.set_in_shiftdir(config.shift_in.direction == ShiftDirection::Right);
            w.set_autopull(config.shift_out.auto_fill);
            w.set_autopush(config.shift_in.auto_fill);

            #[cfg(feature = "_rp235x")]
            {
                w.set_fjoin_rx_get(
                    config.fifo_join == FifoJoin::RxAsControl || config.fifo_join == FifoJoin::PioScratch,
                );
                w.set_fjoin_rx_put(
                    config.fifo_join == FifoJoin::RxAsStatus || config.fifo_join == FifoJoin::PioScratch,
                );
                w.set_in_count(config.in_count);
            }
        });

        #[cfg(feature = "rp2040")]
        sm.pinctrl().write(|w| {
            w.set_sideset_count(config.pins.sideset_count);
            w.set_set_count(config.pins.set_count);
            w.set_out_count(config.pins.out_count);
            w.set_in_base(config.pins.in_base);
            w.set_sideset_base(config.pins.sideset_base);
            w.set_set_base(config.pins.set_base);
            w.set_out_base(config.pins.out_base);
        });

        #[cfg(feature = "_rp235x")]
        {
            sm.pinctrl().write(|w| {
                w.set_sideset_count(config.pins.sideset_count);
                w.set_set_count(config.pins.set_count);
                w.set_out_count(config.pins.out_count);
                w.set_in_base(config.pins.in_base.checked_sub(shift).unwrap_or_default());
                w.set_sideset_base(config.pins.sideset_base.checked_sub(shift).unwrap_or_default());
                w.set_set_base(config.pins.set_base.checked_sub(shift).unwrap_or_default());
                w.set_out_base(config.pins.out_base.checked_sub(shift).unwrap_or_default());
            });

            PIO::PIO.gpiobase().write(|w| w.set_gpiobase(shift == 16));
        }

        if let Some(origin) = config.origin {
            unsafe { self.exec_jmp(origin) }
        }
    }

    /// Returns the RX FIFO MMIO pointer; raw access must exclude competing CPU/DMA consumers.
    pub fn rx_fifo_ptr(&self) -> *mut u32 { PIO::PIO.rxf(SM).as_ptr() }

    /// Returns the TX FIFO MMIO pointer; raw access must exclude competing CPU/DMA producers.
    pub fn tx_fifo_ptr(&self) -> *mut u32 { PIO::PIO.txf(SM).as_ptr() }

    /// Returns the DMA request selector for this machine's RX FIFO.
    pub fn rx_treq(&self) -> crate::pac::dma::vals::TreqSel { StateMachineRx::<PIO, SM>::dreq() }

    /// Returns the DMA request selector for this machine's TX FIFO.
    pub fn tx_treq(&self) -> crate::pac::dma::vals::TreqSel { StateMachineTx::<PIO, SM>::dreq() }

    /// Returns the current five-bit program-counter address; execution may advance immediately afterward.
    pub fn get_addr(&self) -> u8 {
        let addr = Self::this_sm().addr();
        addr.read().addr()
    }

    /// Returns raw autopull threshold 0..=31; zero encodes32 shifted bits.
    pub fn get_tx_threshold(&self) -> u8 {
        let shiftctrl = Self::this_sm().shiftctrl();
        shiftctrl.read().pull_thresh()
    }

    /// Sets raw autopull `threshold`; zero means32 bits, values above 31 panic before writing.
    pub fn set_tx_threshold(
        &mut self,
        threshold: u8,
    ) {
        assert!(threshold <= 31);
        let shiftctrl = Self::this_sm().shiftctrl();
        shiftctrl.modify(|w| {
            w.set_pull_thresh(threshold);
        });
    }

    /// Returns raw autopush threshold 0..=31; zero encodes32 shifted bits.
    pub fn get_rx_threshold(&self) -> u8 { Self::this_sm().shiftctrl().read().push_thresh() }

    /// Sets raw autopush `threshold`; zero means32 bits, values above 31 panic before writing.
    pub fn set_rx_threshold(
        &mut self,
        threshold: u8,
    ) {
        assert!(threshold <= 31);
        let shiftctrl = Self::this_sm().shiftctrl();
        shiftctrl.modify(|w| {
            w.set_push_thresh(threshold);
        });
    }

    /// Sets both raw shift thresholds to `threshold`; zero means32 bits, values above 31 panic.
    pub fn set_thresholds(
        &mut self,
        threshold: u8,
    ) {
        assert!(threshold <= 31);
        let shiftctrl = Self::this_sm().shiftctrl();
        shiftctrl.modify(|w| {
            w.set_push_thresh(threshold);
            w.set_pull_thresh(threshold);
        });
    }

    /// Sets 16.8 `clock_divider` relative to system clock; panics outside 1..=65536.
    /// Does not restart divider phase or wait for any currently executing instruction.
    pub fn set_clock_divider(
        &mut self,
        clock_divider: FixedU32<U8>,
    ) {
        assert!(clock_divider >= 1 && clock_divider <= 65536, "clkdiv must be 1..=65536");
        let sm = Self::this_sm();
        sm.clkdiv().write(|w| w.0 = clock_divider.to_bits() << 8);
    }

    /// Returns this const-indexed machine's register view; valid handles have SM0..=3.
    #[inline(always)]
    fn this_sm() -> crate::pac::pio::StateMachine { PIO::PIO.sm(SM) }

    /// Restarts internal shift/stall/delay state; does not reset program counter or clear FIFOs.
    pub fn restart(&mut self) {
        let mask = 1u8 << SM;
        PIO::PIO.ctrl().write_set(|w| w.set_sm_restart(mask));
    }

    /// Sets this machine's execution enable to `enable`; disabling retains program counter/FIFOs.
    pub fn set_enable(
        &mut self,
        enable: bool,
    ) {
        let mask = 1u8 << SM;
        if enable {
            PIO::PIO.ctrl().write_set(|w| w.set_sm_enable(mask));
        } else {
            PIO::PIO.ctrl().write_clear(|w| w.set_sm_enable(mask));
        }
    }

    /// Returns whether this machine's execution enable bit is set, not whether it is currently stalled.
    pub fn is_enabled(&self) -> bool { PIO::PIO.ctrl().read().sm_enable() & (1u8 << SM) != 0 }

    /// Restarts this machine's fractional clock-divider phase without clearing its FIFO or program counter.
    pub fn clkdiv_restart(&mut self) {
        let mask = 1u8 << SM;
        PIO::PIO.ctrl().write_set(|w| w.set_clkdiv_restart(mask));
    }

    /// Calls `f` with execution disabled and OUT_STICKY cleared, then restores pin/execution config and enable.
    /// A panic in `f` skips restoration; FIFO/shift/instruction effects are not rolled back.
    fn with_paused(
        &mut self,
        f: impl FnOnce(&mut Self),
    ) {
        let enabled = self.is_enabled();
        self.set_enable(false);
        let pincfg = Self::this_sm().pinctrl().read();
        let execcfg = Self::this_sm().execctrl().read();
        Self::this_sm().execctrl().write_clear(|w| w.set_out_sticky(true));
        f(self);
        Self::this_sm().pinctrl().write_value(pincfg);
        Self::this_sm().execctrl().write_value(execcfg);
        self.set_enable(enabled);
    }

    /// Returns the shared physical GPIO window base (0 on RP2040,0 or 16 on RP235x).
    #[cfg(feature = "rp2040")]
    fn pin_base() -> u8 { pio_pin_base::<PIO>() }

    /// Returns the shared physical GPIO window base (0 or 16 on RP235x).
    #[cfg(feature = "_rp235x")]
    fn pin_base() -> u8 { pio_pin_base::<PIO>() }

    /// Injects SET PINDIRS for `pins` using `dir`, temporarily pausing this machine.
    /// Pins must lie in the current shared GPIOBASE window; other machines driving them remain active.
    pub fn set_pin_dirs(
        &mut self,
        dir: Direction,
        pins: &[&Pin<'d, PIO>],
    ) {
        self.with_paused(|sm| {
            for pin in pins {
                Self::this_sm().pinctrl().write(|w| {
                    w.set_set_base(pin.pin() - Self::pin_base());
                    w.set_set_count(1);
                });
                // SET PINDIRS, (dir)
                unsafe { sm.exec_instr(0b111_00000_100_00000 | dir as u16) };
            }
        });
    }

    /// Injects SET PINS for `pins` using `level`, temporarily pausing this machine.
    /// Pins must lie in the current shared GPIOBASE window; no output-direction change is made.
    pub fn set_pins(
        &mut self,
        level: Level,
        pins: &[&Pin<'d, PIO>],
    ) {
        self.with_paused(|sm| {
            for pin in pins {
                Self::this_sm().pinctrl().write(|w| {
                    w.set_set_base(pin.pin() - Self::pin_base());
                    w.set_set_count(1);
                });
                // SET PINS, (dir)
                unsafe { sm.exec_instr(0b11100_000_000_00000 | level as u16) };
            }
        });
    }

    /// Discards both FIFO contents by toggling RX join; does not clear shift registers or restart execution.
    pub fn clear_fifos(&mut self) {
        // Toggle FJOIN_RX to flush FIFOs
        let shiftctrl = Self::this_sm().shiftctrl();
        shiftctrl.modify(|w| {
            w.set_fjoin_rx(!w.fjoin_rx());
        });
        shiftctrl.modify(|w| {
            w.set_fjoin_rx(!w.fjoin_rx());
        });
    }

    /// Injects encoded `instr`; returns after the register write, without waiting for stalled execution.
    ///
    /// # Safety
    /// Ensure this instruction and its FIFO/pin/memory effects are valid for the owned program and
    /// current shift state; exclude conflicting DMA/CPU accesses and unowned pin drives.
    pub unsafe fn exec_instr(
        &mut self,
        instr: u16,
    ) {
        Self::this_sm().instr().write(|w| w.set_instr(instr));
    }

    /// Returns the exclusively borrowed RX handle; state-machine execution continues.
    pub fn rx(&mut self) -> &mut StateMachineRx<'d, PIO, SM> { &mut self.rx }

    /// Returns the exclusively borrowed TX handle; state-machine execution continues.
    pub fn tx(&mut self) -> &mut StateMachineTx<'d, PIO, SM> { &mut self.tx }

    /// Returns disjoint mutable RX/TX handles while retaining the state-machine owner.
    pub fn rx_tx(&mut self) -> (&mut StateMachineRx<'d, PIO, SM>, &mut StateMachineTx<'d, PIO, SM>) {
        (&mut self.rx, &mut self.tx)
    }

    /// Returns RX scratch entry `n` in 0..=3; use only RxAsStatus, with no competing producer. Panics outside range.
    #[cfg(feature = "_rp235x")]
    pub fn get_rxf_entry(
        &self,
        n: usize,
    ) -> u32 {
        PIO::PIO.rxf_putget(SM).putget(n).read()
    }

    /// Writes `val` to RX scratch entry `n` in 0..=3; use only RxAsControl. Panics outside range.
    #[cfg(feature = "_rp235x")]
    pub fn set_rxf_entry(
        &self,
        n: usize,
        val: u32,
    ) {
        PIO::PIO.rxf_putget(SM).putget(n).write_value(val)
    }
}

/// PIO handle.
pub struct Common<'d, PIO: Instance> {
    /// Bitmap of reserved instruction addresses0..31.
    instructions_used: u32,
    /// One of five references keeping the PIO pin registrations alive.
    pio: PhantomData<&'d mut PIO>,
}

impl<'d, PIO: Instance> Drop for Common<'d, PIO> {
    /// Releases the common pin-registration reference; instructions are not erased.
    fn drop(&mut self) { on_pio_drop::<PIO>(); }
}

/// Memory of PIO instance.
pub struct InstanceMemory<'d, PIO: Instance> {
    /// Reserved instruction-address bitmap; drop alone does not free it.
    used_mask: u32,
    /// PIO instance/lifetime identity of this instruction reservation.
    pio: PhantomData<&'d mut PIO>,
}

/// A loaded PIO program.
pub struct LoadedProgram<'d, PIO: Instance> {
    /// Memory used by program.
    pub used_memory: InstanceMemory<'d, PIO>,
    /// Program origin for loading.
    pub origin: u8,
    /// Wrap controls what to do once program is done executing.
    pub wrap: Wrap,
    /// Data for 'side' set instruction parameters.
    pub side_set: SideSet,
}

/// Errors loading a PIO program.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum LoadError {
    /// Insufficient consecutive free instruction space to load program.
    InsufficientSpace,
    /// Loading the program would overwrite an instruction address already
    /// used by another program.
    AddressInUse(
        /// Conflicting instruction address 0..31.
        usize,
    ),
}

impl<'d, PIO: Instance> Common<'d, PIO> {
    /// Loads/relocates `prog` and returns its instruction reservation, wrap and side-set metadata.
    /// Panics on incompatible RP2040 instruction version or allocation failure; does not start a machine.
    /// The reservation is not automatically freed on drop; use free_instr only after all consumers stop.
    pub fn load_program<const SIZE: usize>(
        &mut self,
        prog: &Program<SIZE>,
    ) -> LoadedProgram<'d, PIO> {
        match self.try_load_program(prog) {
            Ok(r) => r,
            Err(e) => panic!("Failed to load PIO program: {:?}", e),
        }
    }

    /// Loads/relocates `prog` and returns its reservation, or AddressInUse/InsufficientSpace.
    /// A failed candidate can overwrite unreserved instruction slots; existing reservations are preserved.
    /// RP2040 rejects non-V0 programs by panic; dropping the returned token does not free its reservation.
    pub fn try_load_program<const SIZE: usize>(
        &mut self,
        prog: &Program<SIZE>,
    ) -> Result<LoadedProgram<'d, PIO>, LoadError> {
        match prog.origin {
            Some(origin) => self.try_load_program_at(prog, origin).map_err(LoadError::AddressInUse),
            None => {
                // naively search for free space, allowing wraparound since
                // PIO does support that. with only 32 instruction slots it
                // doesn't make much sense to do anything more fancy.
                let mut origin = 0;
                while origin < 32 {
                    match self.try_load_program_at(prog, origin as _) {
                        Ok(r) => return Ok(r),
                        Err(a) => origin = a + 1,
                    }
                }
                Err(LoadError::InsufficientSpace)
            }
        }
    }

    /// Relocates `prog` to `origin` and returns its reservation or the conflicting slot.
    /// Instruction placement wraps modulo32; RP2040 panics for non-V0 code; metadata relocation follows pio.
    fn try_load_program_at<const SIZE: usize>(
        &mut self,
        prog: &Program<SIZE>,
        origin: u8,
    ) -> Result<LoadedProgram<'d, PIO>, usize> {
        #[cfg(not(feature = "_rp235x"))]
        assert!(prog.version == pio::PioVersion::V0);

        let prog = RelocatedProgram::new_with_origin(prog, origin);
        let used_memory = self.try_write_instr(prog.origin() as _, prog.code())?;
        Ok(LoadedProgram {
            used_memory,
            origin: prog.origin(),
            wrap: prog.wrap(),
            side_set: prog.side_set(),
        })
    }

    /// Writes `instrs` from `start` modulo32 and returns a reservation or first conflicting slot.
    /// Failure retains already-written free slots but does not reserve them; more than 32 words conflicts with itself.
    fn try_write_instr<I>(
        &mut self,
        start: usize,
        instrs: I,
    ) -> Result<InstanceMemory<'d, PIO>, usize>
    where
        I: Iterator<Item = u16>,
    {
        let mut used_mask = 0;
        for (i, instr) in instrs.enumerate() {
            // wrapping around the end of program memory is valid, let's make use of that.
            let addr = (i + start) % 32;
            let mask = 1 << addr;
            if (self.instructions_used | used_mask) & mask != 0 {
                return Err(addr);
            }
            PIO::PIO.instr_mem(addr).write(|w| {
                w.set_instr_mem(instr);
            });
            used_mask |= mask;
        }
        self.instructions_used |= used_mask;
        Ok(InstanceMemory {
            used_mask,
            pio: PhantomData,
        })
    }

    /// Releases reservation `instrs` without erasing instruction RAM.
    ///
    /// # Safety
    /// No running/stalled machine, injected instruction or future restart may still execute these slots.
    /// The token must belong to this common owner; coordinate all consumers before freeing it.
    pub unsafe fn free_instr(
        &mut self,
        instrs: InstanceMemory<PIO>,
    ) {
        self.instructions_used &= !instrs.used_mask;
    }

    /// Sets synchronizer bypass bits from `bypass` wherever `mask` is one; other bits remain unchanged.
    /// Bits are relative to the current GPIOBASE window. Bypassed inputs need external timing guarantees.
    pub fn set_input_sync_bypass<'a>(
        &'a mut self,
        bypass: u32,
        mask: u32,
    ) {
        // this can interfere with per-pin bypass functions. splitting the
        // modification is going to be fine since nothing that relies on
        // it can reasonably run before we finish.
        PIO::PIO.input_sync_bypass().write_set(|w| *w = mask & bypass);
        PIO::PIO.input_sync_bypass().write_clear(|w| *w = mask & !bypass);
    }

    /// Returns the bypass bitmap relative to current GPIOBASE; reading has no side effects.
    pub fn get_input_sync_bypass(&self) -> u32 { PIO::PIO.input_sync_bypass().read() }

    /// Assigns owned `pin` to this PIO, enabling input/Schmitt, normal slew and no pulls; returns its handle.
    /// Registration lasts until Common and all four StateMachines drop, even if the Pin drops sooner.
    /// Pad configuration is overwritten; output direction remains controlled by the state machines.
    pub fn make_pio_pin(
        &mut self,
        pin: Peri<'d, impl PioPin + 'd>,
    ) -> Pin<'d, PIO> {
        // enable the outputs
        pin.pad_ctrl().write(|w| w.set_od(false));
        // especially important on the 235x, where IE defaults to 0
        pin.pad_ctrl().write(|w| w.set_ie(true));

        pin.gpio().ctrl().write(|w| w.set_funcsel(PIO::FUNCSEL as _));
        pin.pad_ctrl().write(|w| {
            #[cfg(feature = "_rp235x")]
            w.set_iso(false);
            w.set_schmitt(true);
            w.set_slewfast(false);
            // TODO rp235x errata E9 recommends to not enable IE if we're not
            // going to use input. Maybe add an API for the user to enable/disable this?
            w.set_ie(true);
            w.set_od(false);
            w.set_pue(false);
            w.set_pde(false);
        });
        // we can be relaxed about this because we're &mut here and nothing is cached
        critical_section::with(|_| {
            let val = PIO::state().used_pins.load(Ordering::Relaxed);
            PIO::state()
                .used_pins
                .store(val | 1 << pin.pin_bank(), Ordering::Relaxed);
        });

        Pin {
            pin: pin.into(),
            pio: PhantomData::default(),
        }
    }
}

/// Represents multiple state machines in a single type.
pub struct PioBatch<'a, PIO: Instance> {
    /// Machines whose fractional divider phase will restart on execute.
    clkdiv_restart: u8,
    /// Machine restart mask; currently remains zero because no public setter records it.
    sm_restart: u8,
    /// Machines with a recorded enable decision; others retain hardware state.
    sm_enable_mask: u8,
    /// Latest enable bits for selected machines.
    sm_enable: u8,
    /// Shared PIO lifetime needed by the recorded machine requests.
    _pio: PhantomData<&'a PIO>,
}

impl<'a, PIO: Instance> PioBatch<'a, PIO> {
    /// Returns an empty reusable batch; creating it changes no hardware.
    pub fn new() -> Self {
        Self {
            clkdiv_restart: 0,
            sm_restart: 0,
            sm_enable_mask: 0,
            sm_enable: 0,
            _pio: PhantomData,
        }
    }

    /// Records a divider-phase restart for `_sm`; hardware changes only when execute is called.
    pub fn restart<const SM: usize>(
        &mut self,
        _sm: &mut StateMachine<'a, PIO, SM>,
    ) {
        self.clkdiv_restart |= 1 << SM;
    }

    /// Records `enable` for `_sm`; the latest request for that machine wins.
    /// No hardware change occurs until execute; other machines retain their requests.
    pub fn set_enable<const SM: usize>(
        &mut self,
        _sm: &mut StateMachine<'a, PIO, SM>,
        enable: bool,
    ) {
        batch::set_enable(&mut self.sm_enable_mask, &mut self.sm_enable, 1 << SM, enable);
    }

    /// Applies all recorded changes together, preserving unselected enable bits.
    /// Requests remain recorded for reuse; later setters can replace enable values.
    pub fn execute(&mut self) {
        PIO::PIO.ctrl().modify(|w| {
            w.set_clkdiv_restart(self.clkdiv_restart);
            w.set_sm_restart(self.sm_restart);
            w.set_sm_enable(batch::apply_enable(w.sm_enable(), self.sm_enable_mask, self.sm_enable));
        });
    }
}

/// Type representing a PIO interrupt.
pub struct Irq<'d, PIO: Instance, const N: usize> {
    /// Exclusive flag handle; does not keep pin registrations alive.
    pio: PhantomData<&'d mut PIO>,
}

impl<'d, PIO: Instance, const N: usize> Irq<'d, PIO, N> {
    /// Acknowledges this PIO IRQ flag; does not change its interrupt-enable mask.
    pub(crate) fn clear(&mut self) { PIO::PIO.irq().write(|w| w.set_irq(1 << N)); }

    /// Returns a future that consumes this flag when set; drop masks its IRQ but leaves an unconsumed flag.
    pub fn wait<'a>(&'a mut self) -> IrqFuture<'a, 'd, PIO> {
        IrqFuture {
            pio: PhantomData,
            irq_no: N as u8,
        }
    }
}

/// Interrupt flags for a PIO instance.
#[derive(Clone)]
pub struct IrqFlags<'d, PIO: Instance> {
    /// Shared flag access lifetime; clones do not keep pin registrations alive.
    pio: PhantomData<&'d mut PIO>,
}

impl<'d, PIO: Instance> IrqFlags<'d, PIO> {
    /// Returns whether `irq_no` is set without clearing it; panics for indices>=8.
    pub fn check(
        &self,
        irq_no: u8,
    ) -> bool {
        assert!(irq_no < 8);
        self.check_any(1 << irq_no)
    }

    /// Returns whether any bit in `irqs` is set; an empty mask returns false.
    pub fn check_any(
        &self,
        irqs: u8,
    ) -> bool {
        PIO::PIO.irq().read().irq() & irqs != 0
    }

    /// Returns whether every bit in `irqs` is set; an empty mask returns true.
    pub fn check_all(
        &self,
        irqs: u8,
    ) -> bool {
        PIO::PIO.irq().read().irq() & irqs == irqs
    }

    /// Clears flag `irq_no` only; panics for indices>=8.
    pub fn clear(
        &self,
        irq_no: usize,
    ) {
        assert!(irq_no < 8);
        self.clear_all(1 << irq_no);
    }

    /// Clears every selected bit in `irqs`; an empty mask has no effect.
    pub fn clear_all(
        &self,
        irqs: u8,
    ) {
        PIO::PIO.irq().write(|w| w.set_irq(irqs))
    }

    /// Forces flag `irq_no`; panics for indices>=8. Registered waiters are notified through IRQ delivery.
    pub fn set(
        &self,
        irq_no: usize,
    ) {
        assert!(irq_no < 8);
        self.set_all(1 << irq_no);
    }

    /// Forces each selected flag in `irqs`; an empty mask has no effect.
    pub fn set_all(
        &self,
        irqs: u8,
    ) {
        PIO::PIO.irq_force().write(|w| w.set_irq_force(irqs))
    }
}

/// An instance of the PIO driver.
pub struct Pio<'d, PIO: Instance> {
    /// PIO handle.
    pub common: Common<'d, PIO>,
    /// PIO IRQ flags.
    pub irq_flags: IrqFlags<'d, PIO>,
    /// IRQ0 configuration.
    pub irq0: Irq<'d, PIO, 0>,
    /// IRQ1 configuration.
    pub irq1: Irq<'d, PIO, 1>,
    /// IRQ2 configuration.
    pub irq2: Irq<'d, PIO, 2>,
    /// IRQ3 configuration.
    pub irq3: Irq<'d, PIO, 3>,
    /// State machine 0 handle.
    pub sm0: StateMachine<'d, PIO, 0>,
    /// State machine 1 handle.
    pub sm1: StateMachine<'d, PIO, 1>,
    /// State machine 2 handle.
    pub sm2: StateMachine<'d, PIO, 2>,
    /// State machine 3 handle.
    pub sm3: StateMachine<'d, PIO, 3>,
    /// Peripheral lifetime identity; ownership is carried by its split handles.
    _pio: PhantomData<&'d mut PIO>,
}

impl<'d, PIO: Instance> Pio<'d, PIO> {
    /// Consumes `_pio` and `_irq` binding proof and returns split common/machine/flag owners.
    /// Initializes software reference counts and enables IRQ0; does not reset instruction/FIFO registers.
    /// Exclusive peripheral ownership and a suitable prior hardware reset are required.
    pub fn new(
        _pio: Peri<'d, PIO>,
        _irq: impl Binding<PIO::Interrupt, InterruptHandler<PIO>>,
    ) -> Self {
        PIO::state().users.store(5, Ordering::Release);
        PIO::state().used_pins.store(0, Ordering::Release);
        PIO::Interrupt::unpend();

        unsafe { PIO::Interrupt::enable() };
        Self {
            common: Common {
                instructions_used: 0,
                pio: PhantomData,
            },
            irq_flags: IrqFlags { pio: PhantomData },
            irq0: Irq { pio: PhantomData },
            irq1: Irq { pio: PhantomData },
            irq2: Irq { pio: PhantomData },
            irq3: Irq { pio: PhantomData },
            sm0: StateMachine {
                rx: StateMachineRx { pio: PhantomData },
                tx: StateMachineTx { pio: PhantomData },
            },
            sm1: StateMachine {
                rx: StateMachineRx { pio: PhantomData },
                tx: StateMachineTx { pio: PhantomData },
            },
            sm2: StateMachine {
                rx: StateMachineRx { pio: PhantomData },
                tx: StateMachineTx { pio: PhantomData },
            },
            sm3: StateMachine {
                rx: StateMachineRx { pio: PhantomData },
                tx: StateMachineTx { pio: PhantomData },
            },
            _pio: PhantomData,
        }
    }
}

/// Two atomic words protected together by the HAL critical section, including across cores.
struct AtomicU64 {
    /// Most significant32 bits, accessed with the lower half under the same critical section.
    upper_32: AtomicU32,
    /// Least significant32 bits, accessed with the upper half under the same critical section.
    lower_32: AtomicU32,
}

impl AtomicU64 {
    /// Returns a two-word atomic cell initialized from `val`; no hardware access.
    const fn new(val: u64) -> Self {
        let upper_32 = (val >> 32) as u32;
        let lower_32 = val as u32;

        Self {
            upper_32: AtomicU32::new(upper_32),
            lower_32: AtomicU32::new(lower_32),
        }
    }

    /// Returns a coherent64-bit value under critical section using load `order`; Release/AcqRel panic.
    fn load(
        &self,
        order: Ordering,
    ) -> u64 {
        let (upper, lower) = critical_section::with(|_| (self.upper_32.load(order), self.lower_32.load(order)));

        let upper = (upper as u64) << 32;
        let lower = lower as u64;

        upper | lower
    }

    /// Stores both halves of `val` under critical section with `order`; Acquire/AcqRel panic.
    fn store(
        &self,
        val: u64,
        order: Ordering,
    ) {
        let upper_32 = (val >> 32) as u32;
        let lower_32 = val as u32;

        critical_section::with(|_| {
            self.upper_32.store(upper_32, order);
            self.lower_32.store(lower_32, order);
        });
    }
}

/// Representation of the PIO state keeping a record of which pins are assigned to
/// each PIO.
// make_pio_pin notionally takes ownership of the pin it is given, but the wrapped pin
// cannot be treated as an owned resource since dropping it would have to deconfigure
// the pin, breaking running state machines in the process. pins are also shared
// between all state machines, which makes ownership even messier to track any
// other way.
pub struct State {
    /// Live Common plus StateMachine owners; initialized to 5, final decrement releases pins.
    users: AtomicU8,
    /// Physical BANK0 pin bitmap shared by all four machines.
    used_pins: AtomicU64,
}

/// Releases one of five owning references; the last resets every registered GPIO FUNCSEL to NULL.
/// Caller must release each initialized Common/StateMachine exactly once; pad configuration remains.
fn on_pio_drop<PIO: Instance>() {
    let state = PIO::state();
    let users_state = critical_section::with(|_| {
        let val = state.users.load(Ordering::Acquire);
        state.users.store(val - 1, Ordering::Release);
        val
    });
    if users_state == 1 {
        let used_pins = state.used_pins.load(Ordering::Relaxed);
        let null = pac::io::vals::Gpio0ctrlFuncsel::NULL as _;
        for i in 0..crate::gpio::BANK0_PIN_COUNT {
            if used_pins & (1 << i) != 0 {
                pac::IO_BANK0.gpio(i).ctrl().write(|w| w.set_funcsel(null));
            }
        }
    }
}

/// Maps each PIO singleton to its register bank, mux selector and independent software state.
trait SealedInstance {
    /// Hardware PIO index used to derive DMA request selectors.
    const PIO_NO: u8;
    /// Register bank shared by this instance's four state machines.
    const PIO: &'static crate::pac::pio::Pio;
    /// GPIO mux function selecting this PIO block.
    const FUNCSEL: crate::pac::io::vals::Gpio0ctrlFuncsel;

    /// Returns this instance's independent FIFO/IRQ waiter storage.
    fn wakers() -> &'static Wakers;
    /// Returns this instance's independent pin-registration reference state.
    fn state() -> &'static State;
}

/// PIO instance.
#[allow(private_bounds)]
pub trait Instance: SealedInstance + PeripheralType + Sized + Unpin {
    /// Interrupt for this peripheral.
    type Interrupt: crate::interrupt::typelevel::Interrupt;
}

/// Binds a peripheral/register/mux/IRQ tuple and emits separate static state per PIO instance.
macro_rules! impl_pio {
    ($name:ident, $pio:expr, $pac:ident, $funcsel:ident, $irq:ident) => {
        impl SealedInstance for peripherals::$name {
            /// GPIO mux selector for this PIO instance.
            const FUNCSEL: pac::io::vals::Gpio0ctrlFuncsel = pac::io::vals::Gpio0ctrlFuncsel::$funcsel;
            /// Register bank shared by the four machines.
            const PIO: &'static pac::pio::Pio = &pac::$pac;
            /// Hardware PIO index used for DMA request selection.
            const PIO_NO: u8 = $pio;

            // Statics in trait default methods are shared across implementors.
            // Emit separate storage for each hardware PIO instance.
            /// Returns FIFO/IRQ waiter storage unique to this PIO instance.
            fn wakers() -> &'static Wakers {
                /// Four RX, four TX and four IRQ waiters for this PIO.
                static WAKERS: Wakers = Wakers([const { AtomicWaker::new() }; 12]);
                &WAKERS
            }

            /// Returns pin/reference state unique to this PIO instance.
            fn state() -> &'static State {
                /// Common/machine owners and registered physical pins.
                static STATE: State = State {
                    users: AtomicU8::new(0),
                    used_pins: AtomicU64::new(0),
                };
                &STATE
            }
        }
        impl Instance for peripherals::$name {
            /// CPU interrupt0 owned by the PIO driver.
            type Interrupt = crate::interrupt::typelevel::$irq;
        }
    };
}

impl_pio!(PIO0, 0, PIO0, PIO0_0, PIO0_IRQ_0);
impl_pio!(PIO1, 1, PIO1, PIO1_0, PIO1_IRQ_0);
#[cfg(feature = "_rp235x")]
impl_pio!(PIO2, 2, PIO2, PIO2_0, PIO2_IRQ_0);

#[cfg(test)]
/// Regressions for independent macro-generated ownership and IRQ wake storage.
mod instance_tests {
    use super::*;

    /// Checks that macro-generated PIO instances retain separate IRQ waiters and pin ownership storage.
    #[test]
    fn pio_instances_have_distinct_wakers_and_pin_ownership() {
        assert!(!core::ptr::eq(peripherals::PIO0::wakers(), peripherals::PIO1::wakers()));
        assert!(!core::ptr::eq(peripherals::PIO0::state(), peripherals::PIO1::state()));
        #[cfg(feature = "_rp235x")]
        {
            assert!(!core::ptr::eq(peripherals::PIO0::wakers(), peripherals::PIO2::wakers()));
            assert!(!core::ptr::eq(peripherals::PIO1::wakers(), peripherals::PIO2::wakers()));
            assert!(!core::ptr::eq(peripherals::PIO0::state(), peripherals::PIO2::state()));
            assert!(!core::ptr::eq(peripherals::PIO1::state(), peripherals::PIO2::state()));
        }
    }
    /// High physical pins map to relative bypass bits; out-of-window pins are rejected.
    #[test]
    fn bypass_uses_current_gpio_window() {
        assert_eq!(input_sync_bypass_mask(0, 0), 1);
        assert_eq!(input_sync_bypass_mask(31, 0), 1 << 31);
        assert_eq!(input_sync_bypass_mask(32, 16), 1 << 16);
        assert_eq!(input_sync_bypass_mask(47, 16), 1 << 31);
    }

    /// Default JMP remains relative 0; explicit physical JMP participates in window selection/translation.
    #[cfg(feature = "_rp235x")]
    #[test]
    fn explicit_jmp_uses_selected_gpio_window() {
        assert_eq!(pin_window([32..48, 0..0, 0..0, 0..0], None), (16, 0));
        assert_eq!(pin_window([0..1, 0..0, 0..0, 0..0], Some(29)), (0, 29));
        assert_eq!(pin_window([32..48, 0..0, 0..0, 0..0], Some(35)), (16, 19));
        for pin in 32..48 {
            assert_eq!(pin_window([0..0, 0..0, 0..0, 0..0], Some(pin)), (16, pin - 16));
        }
    }

    /// A physical pin below the selected window cannot accidentally select a wrapped bypass bit.
    #[test]
    #[should_panic(expected = "below PIO GPIOBASE")]
    fn bypass_rejects_pin_below_window() { let _ = input_sync_bypass_mask(15, 16); }

    /// A physical pin above the window cannot overflow or wrap the 32-bit mask.
    #[test]
    #[should_panic(expected = "above PIO GPIOBASE")]
    fn bypass_rejects_pin_above_window() { let _ = input_sync_bypass_mask(32, 0); }

    /// Explicit physical JMP0 is incompatible with output pins requiring the high window.
    #[cfg(feature = "_rp235x")]
    #[test]
    #[should_panic(expected = "share a32-pin")]
    fn explicit_low_jmp_rejects_high_window() { let _ = pin_window([32..48, 0..0, 0..0, 0..0], Some(0)); }

    /// Raw execution replacement makes its JMP field explicit, even when copied from defaults.
    #[cfg(feature = "_rp235x")]
    #[test]
    fn raw_exec_marks_default_jmp_explicit() {
        let mut config = Config::<peripherals::PIO0>::default();
        assert!(!config.jmp_pin_explicit);
        unsafe {
            config.set_exec(config.get_exec());
        }
        assert!(config.jmp_pin_explicit);
        assert_eq!(config.exec.jmp_pin, 0);
    }
    /// Optional side-set enable consumes an encoding bit, not an additional physical GPIO.
    #[cfg(feature = "_rp235x")]
    #[test]
    fn optional_side_set_keeps_pin31_in_low_window() {
        let side = SideSet::new(true, 1, false);
        let count = side_set_pin_count(side.bits(), side.optional());
        assert_eq!(count, 1);
        assert_eq!(pin_window([0..0, 31..31 + count, 0..0, 0..0], None), (0, 0));
        assert_eq!(side_set_pin_count(0, false), 0);
        assert_eq!(side_set_pin_count(5, false), 5);
        assert_eq!(side_set_pin_count(5, true), 4);
    }
}

/// PIO pin.
pub trait PioPin: gpio::Pin {}

/// Marks listed physical GPIO singletons as eligible for PIO registration.
macro_rules! impl_pio_pin {
    ($( $pin:ident, )*) => {
        $(
            impl PioPin for peripherals::$pin {}
        )*
    };
}

impl_pio_pin! {
    PIN_0,
    PIN_1,
    PIN_2,
    PIN_3,
    PIN_4,
    PIN_5,
    PIN_6,
    PIN_7,
    PIN_8,
    PIN_9,
    PIN_10,
    PIN_11,
    PIN_12,
    PIN_13,
    PIN_14,
    PIN_15,
    PIN_16,
    PIN_17,
    PIN_18,
    PIN_19,
    PIN_20,
    PIN_21,
    PIN_22,
    PIN_23,
    PIN_24,
    PIN_25,
    PIN_26,
    PIN_27,
    PIN_28,
    PIN_29,
}

#[cfg(feature = "rp235xb")]
impl_pio_pin! {
    PIN_30,
    PIN_31,
    PIN_32,
    PIN_33,
    PIN_34,
    PIN_35,
    PIN_36,
    PIN_37,
    PIN_38,
    PIN_39,
    PIN_40,
    PIN_41,
    PIN_42,
    PIN_43,
    PIN_44,
    PIN_45,
    PIN_46,
    PIN_47,
}
