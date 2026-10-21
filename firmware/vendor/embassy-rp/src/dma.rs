//! Direct Memory Access (DMA), using IRQ0 and one wake registration per channel.
//! Transfers start during construction, not on first poll; dropping issues abort and busy-waits.
//! Raw buffer APIs require the caller to preserve DMA access/aliasing until hardware completion.
use core::future::Future;
use core::marker::PhantomData;
use core::pin::Pin;
use core::sync::atomic::{Ordering, compiler_fence};
use core::task::{Context, Poll};

use embassy_hal_internal::interrupt::InterruptExt;
use embassy_hal_internal::{Peri, PeripheralType};
use embassy_sync::waitqueue::AtomicWaker;
use pac::dma::vals::DataSize;

use crate::interrupt::typelevel::Interrupt;
use crate::pac::dma::vals;
use crate::{RegExt, interrupt, pac, peripherals};

/// DMA interrupt handler.
pub struct InterruptHandler<T: ChannelInstance> {
    /// Binds the handler to its channel singleton without storing hardware state.
    _phantom: PhantomData<T>,
}

impl<T: ChannelInstance> interrupt::typelevel::Handler<T::Interrupt> for InterruptHandler<T> {
    /// Clears this channel's IRQ0 completion and wakes its waiter; panics on an AHB error.
    /// # Safety
    /// called only for the bound DMA interrupt with initialized channel state.
    unsafe fn on_interrupt() {
        let channel = T::number() as usize;
        let ctrl_trig = pac::DMA.ch(channel).ctrl_trig().read();
        if ctrl_trig.ahb_error() {
            panic!("DMA: error on DMA_0 channel {}", channel);
        }

        let ints0 = pac::DMA.ints(0).read();
        if ints0 & (1 << channel) != 0 {
            pac::DMA.ints(0).write_value(1 << channel);

            CHANNEL_WAKERS[channel].wake();
        }
    }
}

/// Sets IRQ0 priority P3; channel constructors enable delivery.
/// # Safety
/// call during HAL initialization before concurrent DMA interrupt configuration.
pub(crate) unsafe fn init() { interrupt::DMA_IRQ_0.set_priority(interrupt::Priority::P3); }

/// DMA channel driver.
pub struct Channel<'d> {
    /// Physical channel index, below `CHANNEL_COUNT`.
    number: u8,
    /// Retains the exclusive peripheral borrow for the channel driver's lifetime.
    phantom: PhantomData<&'d ()>,
}

impl<'d> Channel<'d> {
    /// Returns an exclusive driver consuming `_ch` and requiring `_irq` to bind its handler.
    /// Enables this channel on IRQ0 and enables the shared interrupt on the current core.
    pub fn new<T: ChannelInstance>(
        _ch: Peri<'d, T>,
        _irq: impl interrupt::typelevel::Binding<T::Interrupt, InterruptHandler<T>> + 'd,
    ) -> Self {
        let number = T::number();

        // Enable interrupt for this channel
        pac::DMA.inte(0).write_set(|v| *v = 1 << number);
        unsafe { T::Interrupt::enable() };

        Self {
            number,
            phantom: PhantomData,
        }
    }

    /// Returns the physical channel index (0..12 on RP2040, 0..16 on RP235x).
    fn number(&self) -> u8 { self.number }

    /// Returns this channel's raw register block; writes must preserve transfer ownership.
    #[cfg(feature = "unstable-pac")]
    pub fn regs(&self) -> pac::dma::Channel { pac::DMA.ch(self.number as _) }

    /// Returns this channel's raw register block; writes must preserve transfer ownership.
    #[cfg(not(feature = "unstable-pac"))]
    fn regs(&self) -> pac::dma::Channel { pac::DMA.ch(self.number as _) }

    /// Returns the live next DMA write address in bytes; it may advance immediately afterward.
    pub fn write_addr(&self) -> u32 { self.regs().write_addr().read() }

    /// Returns a shorter exclusive channel borrow; the original cannot be used while it lives.
    pub fn reborrow(&mut self) -> Channel<'_> {
        Channel {
            number: self.number,
            phantom: PhantomData,
        }
    }

    /// Starts `len` transfers from `from` to `to`, each of width `data_size`; `incr_read` and
    /// `incr_write` control address advance, `dreq` selects pacing and `bswap` swaps bytes per word.
    /// Counts are cast to32 bits (RP2040) or masked to28 bits (RP235x); no bounds validation occurs.
    ///
    /// # Safety
    /// Both pointers must be aligned, DMA-accessible and valid for all requested accesses. The
    /// channel must be idle and exclusively owned; buffers/peripherals must remain reserved until
    /// completion/abort. `len` must fit the hardware counter and address spans must not wrap.
    unsafe fn configure(
        &self,
        from: *const u32,
        to: *mut u32,
        len: usize,
        data_size: DataSize,
        incr_read: bool,
        incr_write: bool,
        dreq: vals::TreqSel,
        bswap: bool,
    ) {
        let p = self.regs();

        p.read_addr().write_value(from as u32);
        p.write_addr().write_value(to as u32);
        #[cfg(feature = "rp2040")]
        p.trans_count().write(|w| {
            *w = len as u32;
        });
        #[cfg(feature = "_rp235x")]
        p.trans_count().write(|w| {
            w.set_mode(0.into());
            w.set_count(len as u32);
        });

        compiler_fence(Ordering::SeqCst);

        p.ctrl_trig().write(|w| {
            w.set_treq_sel(dreq);
            w.set_data_size(data_size);
            w.set_incr_read(incr_read);
            w.set_incr_write(incr_write);
            w.set_chain_to(self.number());
            w.set_bswap(bswap);
            w.set_en(true);
        });

        compiler_fence(Ordering::SeqCst);
    }

    /// Starts fixed-address `from` reads into incrementing `to`, paced by `dreq`.
    /// `bswap` swaps bytes within each word. Returns the already-active transfer lease.
    ///
    /// # Safety
    /// `from` must be a readable DMA peripheral register; `to` must be aligned, DMA-accessible,
    /// exclusively writable and alive until completion/abort, even if this lease is forgotten.
    /// The element count must fit the hardware counter; peripheral width/pacing must match `W`.
    pub unsafe fn read<'a, W: Word>(
        &'a mut self,
        from: *const W,
        to: *mut [W],
        dreq: vals::TreqSel,
        bswap: bool,
    ) -> Transfer<'a> {
        self.configure(
            from as *const u32,
            to as *mut W as *mut u32,
            to.len(),
            W::size(),
            false,
            true,
            dreq,
            bswap,
        );
        Transfer::new(self.reborrow())
    }

    /// Starts `len` fixed-address reads of `from` paced by `dreq`, discarding data in static RAM.
    /// Returns the active transfer lease; no byte swap is applied.
    ///
    /// # Safety
    /// `from` must remain a DMA-readable register supporting width `W` and `dreq` for the entire
    /// transfer. `len` must fit the hardware counter; another owner must not reconfigure the register.
    pub unsafe fn read_discard<'a, W: Word>(
        &'a mut self,
        from: *mut W,
        len: usize,
        dreq: vals::TreqSel,
    ) -> Transfer<'a> {
        // static mut so that this is allocated in RAM.
        /// Shared RAM sink; contents are never read by software.
        static mut DUMMY: u32 = 0;

        self.configure(
            from as *mut u32,
            core::ptr::addr_of_mut!(DUMMY) as *mut u32,
            len,
            W::size(),
            false,
            false,
            dreq,
            false,
        );
        Transfer::new(self.reborrow())
    }

    /// Starts incrementing `from` reads into fixed-address `to`, paced by `dreq`.
    /// `bswap` swaps bytes within each word. Returns the already-active transfer lease.
    ///
    /// # Safety
    /// `from` must remain aligned, DMA-readable and unmodified until completion/abort, even if the
    /// lease is forgotten. `to` must be a DMA-writable peripheral matching `W`/`dreq`. The source
    /// element count must fit the hardware counter; neither address span may wrap.
    pub unsafe fn write<'a, W: Word>(
        &'a mut self,
        from: *const [W],
        to: *mut W,
        dreq: vals::TreqSel,
        bswap: bool,
    ) -> Transfer<'a> {
        self.configure(
            from as *const W as *const u32,
            to as *mut u32,
            from.len(),
            W::size(),
            true,
            false,
            dreq,
            bswap,
        );
        Transfer::new(self.reborrow())
    }

    /// Starts `count` zero-word writes to fixed-address `to`, paced by `dreq`.
    /// Returns the active transfer lease with a static RAM source.
    ///
    /// # Safety
    /// `to` must remain an exclusively configured DMA-writable peripheral matching `W`/`dreq`
    /// throughout transfer. `count` must fit the hardware counter.
    pub unsafe fn write_zeros<'a, W: Word>(
        &'a mut self,
        count: usize,
        to: *mut W,
        dreq: vals::TreqSel,
    ) -> Transfer<'a> {
        // static mut so that this is allocated in RAM.
        /// Immutable-in-practice zero source placed in RAM for DMA accessibility.
        static mut DUMMY: u32 = 0;

        self.configure(
            core::ptr::addr_of_mut!(DUMMY) as *const u32,
            to as *mut u32,
            count,
            W::size(),
            false,
            false,
            dreq,
            false,
        );
        Transfer::new(self.reborrow())
    }

    /// Starts an unpaced copy of `from` into `to`; returns the active channel lease.
    /// Panics before starting if the element counts differ.
    ///
    /// # Safety
    /// Both slices must be DMA-accessible and alive through completion/abort; `from` must remain
    /// unmodified and `to` exclusively reserved. Their borrows are not tied to the returned lifetime.
    /// The element count must fit the hardware counter and address spans must not wrap.
    pub unsafe fn copy<'a, W: Word>(
        &'a mut self,
        from: &[W],
        to: &mut [W],
    ) -> Transfer<'a> {
        let from_len = from.len();
        let to_len = to.len();
        assert_eq!(from_len, to_len);
        self.configure(
            from.as_ptr() as *const u32,
            to.as_mut_ptr() as *mut u32,
            from_len,
            W::size(),
            true,
            true,
            vals::TreqSel::PERMANENT,
            false,
        );
        Transfer::new(self.reborrow())
    }
}

/// Already-started DMA operation retaining the channel borrow. Polling observes BUSY; drop aborts.
/// Completion describes DMA, not a downstream peripheral FIFO or physical wire becoming idle.
#[must_use = "futures do nothing unless you `.await` or poll them"]
pub struct Transfer<'a> {
    /// Exclusive channel borrow held until completion or synchronous abort on drop.
    channel: Channel<'a>,
}

impl<'a> Transfer<'a> {
    /// Returns a lease for `channel`, which the caller has already configured and started.
    fn new(channel: Channel<'a>) -> Self { Self { channel } }
}

impl<'a> Drop for Transfer<'a> {
    /// Issues channel abort and spins until BUSY clears; no timeout or downstream FIFO drain.
    fn drop(&mut self) {
        let p = self.channel.regs();
        pac::DMA
            .chan_abort()
            .modify(|m| m.set_chan_abort(1 << self.channel.number()));
        while p.ctrl_trig().read().busy() {}
    }
}

impl<'a> Unpin for Transfer<'a> {}
impl<'a> Future for Transfer<'a> {
    /// DMA BUSY has cleared; no byte count or bus error is returned.
    type Output = ();

    /// Registers `cx` for this channel and returns ready when `self` observes BUSY clear.
    fn poll(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Self::Output> {
        // We need to register/re-register the waker for each poll because any
        // calls to wake will deregister the waker.
        CHANNEL_WAKERS[self.channel.number() as usize].register(cx.waker());

        if self.channel.regs().ctrl_trig().read().busy() {
            Poll::Pending
        } else {
            Poll::Ready(())
        }
    }
}

#[cfg(feature = "rp2040")]
/// Number of physical RP2040 DMA channels.
pub(crate) const CHANNEL_COUNT: usize = 12;
#[cfg(feature = "_rp235x")]
/// Number of physical RP235x DMA channels.
pub(crate) const CHANNEL_COUNT: usize = 16;
/// One completion waiter per physical channel, shared with the IRQ0 handler.
static CHANNEL_WAKERS: [AtomicWaker; CHANNEL_COUNT] = [const { AtomicWaker::new() }; CHANNEL_COUNT];

/// Restricts channel instances to HAL-owned peripheral singletons.
trait SealedChannelInstance {}
/// Restricts DMA widths to supported unsigned integer words.
trait SealedWord {}

/// DMA channel instance trait.
#[allow(private_bounds)]
pub trait ChannelInstance: PeripheralType + SealedChannelInstance + Sized + 'static {
    /// The interrupt type for this DMA channel.
    type Interrupt: interrupt::typelevel::Interrupt;

    /// Returns the singleton's physical DMA channel index.
    fn number() -> u8;

    /// Returns the raw register block for this singleton's physical channel.
    fn regs() -> pac::dma::Channel { pac::DMA.ch(Self::number() as _) }
}

/// DMA word.
#[allow(private_bounds)]
pub trait Word: SealedWord {
    /// Returns the hardware transfer-width selector for this word type.
    fn size() -> vals::DataSize;
}

impl SealedWord for u8 {}
impl Word for u8 {
    /// Returns the 8-bit hardware transfer selector.
    fn size() -> vals::DataSize { vals::DataSize::SIZE_BYTE }
}

impl SealedWord for u16 {}
impl Word for u16 {
    /// Returns the 16-bit hardware transfer selector.
    fn size() -> vals::DataSize { vals::DataSize::SIZE_HALFWORD }
}

impl SealedWord for u32 {}
impl Word for u32 {
    /// Returns the 32-bit hardware transfer selector.
    fn size() -> vals::DataSize { vals::DataSize::SIZE_WORD }
}

/// Implements channel number/IRQ metadata for one peripheral singleton.
macro_rules! channel {
    ($name:ident, $num:expr, $irq:ident) => {
        impl SealedChannelInstance for peripherals::$name {}
        impl ChannelInstance for peripherals::$name {
            /// IRQ0 shared by every generated channel handler.
            type Interrupt = interrupt::typelevel::$irq;

            /// Returns this singleton's fixed physical channel index.
            fn number() -> u8 { $num }
        }
    };
}

channel!(DMA_CH0, 0, DMA_IRQ_0);
channel!(DMA_CH1, 1, DMA_IRQ_0);
channel!(DMA_CH2, 2, DMA_IRQ_0);
channel!(DMA_CH3, 3, DMA_IRQ_0);
channel!(DMA_CH4, 4, DMA_IRQ_0);
channel!(DMA_CH5, 5, DMA_IRQ_0);
channel!(DMA_CH6, 6, DMA_IRQ_0);
channel!(DMA_CH7, 7, DMA_IRQ_0);
channel!(DMA_CH8, 8, DMA_IRQ_0);
channel!(DMA_CH9, 9, DMA_IRQ_0);
channel!(DMA_CH10, 10, DMA_IRQ_0);
channel!(DMA_CH11, 11, DMA_IRQ_0);
#[cfg(feature = "_rp235x")]
channel!(DMA_CH12, 12, DMA_IRQ_0);
#[cfg(feature = "_rp235x")]
channel!(DMA_CH13, 13, DMA_IRQ_0);
#[cfg(feature = "_rp235x")]
channel!(DMA_CH14, 14, DMA_IRQ_0);
#[cfg(feature = "_rp235x")]
channel!(DMA_CH15, 15, DMA_IRQ_0);
