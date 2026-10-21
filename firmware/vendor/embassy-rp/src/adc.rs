//! ADC conversion and DMA sampling. Results are raw codes, not calibrated volts
//! or temperature. The application must serialize board-shared analog pins.
//! Conversion timing: <https://datasheets.raspberrypi.com/rp2040/rp2040-datasheet.pdf>
//! section 4.9; chip-specific channel counts are selected by Cargo features.
use core::future::{Future, poll_fn};
use core::marker::PhantomData;
use core::mem;
use core::sync::atomic::{Ordering, compiler_fence};
use core::task::Poll;

use embassy_sync::waitqueue::AtomicWaker;

use crate::gpio::{self, AnyPin, Pull, SealedPin as GpioPin};
use crate::interrupt::InterruptExt;
use crate::interrupt::typelevel::Binding;
use crate::pac::dma::vals::TreqSel;
use crate::peripherals::{ADC, ADC_TEMP_SENSOR};
use crate::{Peri, RegExt, dma, interrupt, pac, peripherals};

/// Single ADC-ready waiter, protected by exclusive driver borrowing.
static WAKER: AtomicWaker = AtomicWaker::new();

/// ADC config.
#[non_exhaustive]
#[derive(Default)]
pub struct Config {}

/// Owned analog pin or internal sensor; drop restores a fixed digital pad state or disables TS.
#[derive(Debug)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
enum Source<'p> {
    /// External input whose digital input/output are disabled while owned.
    Pin(
        /// Exclusive analog pad ownership.
        Peri<'p, AnyPin>,
    ),
    /// Internal sensor enabled until this source is dropped.
    TempSensor(
        /// Exclusive temperature-sensor token.
        Peri<'p, ADC_TEMP_SENSOR>,
    ),
}

/// ADC channel.
pub struct Channel<'p>(
    /// Source ownership and drop cleanup.
    Source<'p>,
);

impl<'p> Channel<'p> {
    /// Returns analog ownership of `pin` with `pull` enabled as requested; disables
    /// digital input/output and clears RP235x pad isolation. Pulls affect measured voltage.
    pub fn new_pin(
        pin: Peri<'p, impl AdcPin + 'p>,
        pull: Pull,
    ) -> Self {
        pin.pad_ctrl().modify(|w| {
            #[cfg(feature = "_rp235x")]
            w.set_iso(false);
            // manual says:
            //
            // > When using an ADC input shared with a GPIO pin, the pin’s
            // > digital functions must be disabled by setting IE low and OD
            // > high in the pin’s pad control register
            w.set_ie(false);
            w.set_od(true);
            w.set_pue(pull == Pull::Up);
            w.set_pde(pull == Pull::Down);
        });
        Self(Source::Pin(pin.into()))
    }

    /// Enables the sensor represented by `s` and returns its channel. Construct after
    /// the ADC driver: driver initialization resets ADC registers, including TS enable.
    pub fn new_temp_sensor(s: Peri<'p, ADC_TEMP_SENSOR>) -> Self {
        let r = pac::ADC;
        r.cs().write_set(|w| w.set_ts_en(true));
        Self(Source::TempSensor(s))
    }

    /// Returns the hardware AINSEL index for this owned pin or sensor.
    fn channel(&self) -> u8 {
        #[cfg(any(feature = "rp2040", feature = "rp235xa"))]
        /// First GPIO number in this package's consecutive ADC input range.
        const CH_OFFSET: u8 = 26;
        #[cfg(feature = "rp235xb")]
        /// First GPIO number in this package's consecutive ADC input range.
        const CH_OFFSET: u8 = 40;

        #[cfg(any(feature = "rp2040", feature = "rp235xa"))]
        /// Dedicated internal sensor index following the external input channels.
        const TS_CHAN: u8 = 4;
        #[cfg(feature = "rp235xb")]
        /// Dedicated internal sensor index following the external input channels.
        const TS_CHAN: u8 = 8;

        match &self.0 {
            // this requires adc pins to be sequential and matching the adc channels,
            // which is the case for rp2040/rp235xy
            Source::Pin(p) => p._pin() - CH_OFFSET,
            Source::TempSensor(_) => TS_CHAN,
        }
    }
}

impl<'p> Drop for Source<'p> {
    /// Restores digital input/output with pull-down for a pin, or disables the sensor;
    /// the original pad state is not restored.
    fn drop(&mut self) {
        match self {
            Source::Pin(p) => {
                p.pad_ctrl().modify(|w| {
                    w.set_ie(true);
                    w.set_od(false);
                    w.set_pue(false);
                    w.set_pde(true);
                });
            }
            Source::TempSensor(_) => {
                pac::ADC.cs().write_clear(|w| w.set_ts_en(true));
            }
        }
    }
}

/// ADC sample.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, Default)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[repr(transparent)]
pub struct Sample(
    /// FIFO code in low bits and conversion-error flag at bit 15.
    u16,
);

impl Sample {
    /// Returns true when bit 15 reports no conversion error; does not validate analog quality.
    pub fn good(&self) -> bool { self.0 < 0x8000 }

    /// Returns the stored code with error bit 15 removed; hardware provides a 12-bit result.
    pub fn value(&self) -> u16 { self.0 & !0x8000 }
}

/// ADC error.
#[derive(Debug, Eq, PartialEq, Copy, Clone)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum Error {
    /// Error converting value.
    ConversionFailed,
}

/// ADC mode.
pub trait Mode {}

/// ADC async mode.
pub struct Async;
impl Mode for Async {}

/// ADC blocking mode.
pub struct Blocking;
impl Mode for Blocking {}

/// ADC driver.
pub struct Adc<'d, M: Mode> {
    /// Retains exclusive ADC lifetime and selects blocking/async APIs.
    phantom: PhantomData<(&'d ADC, M)>,
}

impl<'d, M: Mode> Drop for Adc<'d, M> {
    /// Disables the ADC and clears its control register; borrowed channel pads are separate.
    fn drop(&mut self) {
        let r = Self::regs();
        // disable ADC. leaving it enabled comes with a ~150µA static
        // current draw. Writing CS also clears temperature-sensor enable.
        r.cs().write(|w| w.set_en(false));
    }
}

impl<'d, M: Mode> Adc<'d, M> {
    /// Returns the ADC register block; shared only through exclusive driver ownership.
    #[inline]
    fn regs() -> pac::adc::Adc { pac::ADC }

    /// Returns a reset mask containing only ADC.
    #[inline]
    fn reset() -> pac::resets::regs::Peripherals {
        let mut ret = pac::resets::regs::Peripherals::default();
        ret.set_adc(true);
        ret
    }

    /// Resets/enables ADC and waits until ready without a timeout; prior ADC configuration is lost.
    fn setup() {
        let reset = Self::reset();
        crate::reset::reset(reset);
        crate::reset::unreset_wait(reset);
        let r = Self::regs();
        // Enable ADC
        r.cs().write(|w| w.set_en(true));
        // Wait for ADC ready
        while !r.cs().read().ready() {}
    }

    /// Starts one conversion of `ch` and returns a 12-bit raw code, or ConversionFailed.
    /// Spins until ready with no timeout; input/channel ownership is retained throughout.
    pub fn blocking_read(
        &mut self,
        ch: &mut Channel,
    ) -> Result<u16, Error> {
        let r = Self::regs();
        r.cs().modify(|w| {
            w.set_ainsel(ch.channel());
            w.set_start_once(true);
            w.set_err(true);
        });
        while !r.cs().read().ready() {}
        match r.cs().read().err() {
            true => Err(Error::ConversionFailed),
            false => Ok(r.result().read().result()),
        }
    }
}

impl<'d> Adc<'d, Async> {
    /// Returns exclusive async ownership from `_inner`, resetting ADC and enabling
    /// the IRQ proven by `_irq`. `_config` currently has no settings. Initialization
    /// busy-waits without a timeout and clears any previously enabled temperature sensor.
    pub fn new(
        _inner: Peri<'d, ADC>,
        _irq: impl Binding<interrupt::typelevel::ADC_IRQ_FIFO, InterruptHandler>,
        _config: Config,
    ) -> Self {
        Self::setup();

        // Setup IRQ
        interrupt::ADC_IRQ_FIFO.unpend();
        unsafe { interrupt::ADC_IRQ_FIFO.enable() };

        Self { phantom: PhantomData }
    }

    /// Returns a future that enables the ADC IRQ and registers the single waiter until
    /// CS.READY is set. Cancellation does not disable the IRQ or stop a conversion.
    fn wait_for_ready() -> impl Future<Output = ()> {
        let r = Self::regs();

        poll_fn(move |cx| {
            WAKER.register(cx.waker());

            r.inte().write(|w| w.set_fifo(true));
            compiler_fence(Ordering::SeqCst);

            if r.cs().read().ready() {
                return Poll::Ready(());
            }
            Poll::Pending
        })
    }

    /// Starts a conversion of `ch` and returns its raw 12-bit code, or ConversionFailed.
    /// No timeout; cancellation leaves the started conversion/IRQ state in hardware.
    pub async fn read(
        &mut self,
        ch: &mut Channel<'_>,
    ) -> Result<u16, Error> {
        let r = Self::regs();
        r.cs().modify(|w| {
            w.set_ainsel(ch.channel());
            w.set_start_once(true);
            w.set_err(true);
        });
        Self::wait_for_ready().await;
        match r.cs().read().err() {
            true => Err(Error::ConversionFailed),
            false => Ok(r.result().read().result()),
        }
    }

    // Note for refactoring: we don't require the actual Channels here, just the channel numbers.
    // The public api is responsible for asserting ownership of the actual Channels.
    /// Fills `buf` using `dma_ch` with the distinct `channels` in ascending hardware
    /// index order. `fcs_err` embeds conversion failure in bit 15; byte words shift
    /// samples down by four bits. `div` is the integer ADC period divider.
    /// Returns ConversionFailed only after the full DMA completes if sticky error
    /// was set. Cancellation aborts DMA, stops ADC and clears FIFO/DREQ/shift enable;
    /// drop can busy-wait without timeout and a partial buffer remains.
    /// Panics for empty `channels` before MMIO; indices must identify owned hardware channels.
    async fn read_many_inner<W: dma::Word>(
        &mut self,
        channels: impl Iterator<Item = u8>,
        buf: &mut [W],
        fcs_err: bool,
        div: u16,
        dma_ch: &mut dma::Channel<'_>,
    ) -> Result<(), Error> {
        #[cfg(feature = "rp2040")]
        let mut rrobin = 0_u8;
        #[cfg(feature = "_rp235x")]
        let mut rrobin = 0_u16;
        for c in channels {
            rrobin |= 1 << c;
        }
        let first_ch = first_channel(u16::from(rrobin));
        if rrobin.count_ones() == 1 {
            rrobin = 0;
        }

        let r = Self::regs();
        // clear previous errors and set channel
        r.cs().modify(|w| {
            w.set_ainsel(first_ch);
            w.set_rrobin(rrobin);
            w.set_err_sticky(true); // clear previous errors
            w.set_start_many(false);
        });
        // wait for previous conversions and drain fifo. an earlier batch read may have
        // been cancelled, leaving the adc running.
        while !r.cs().read().ready() {}
        while !r.fcs().read().empty() {
            r.fifo().read();
        }

        // set up fifo for dma
        r.fcs().write(|w| {
            w.set_thresh(1);
            w.set_dreq_en(true);
            w.set_shift(mem::size_of::<W>() == 1);
            w.set_en(true);
            w.set_err(fcs_err);
        });

        // reset dma config on drop, regardless of whether it was a future being cancelled
        // or the method returning normally.
        /// Stops ADC and removes DMA/FIFO settings on completion or cancellation.
        struct ResetDmaConfig;
        impl Drop for ResetDmaConfig {
            /// Stops free-running conversions, waits for ready without timeout, then disables FIFO/DREQ/shift.
            fn drop(&mut self) {
                pac::ADC.cs().write_clear(|w| w.set_start_many(true));
                while !pac::ADC.cs().read().ready() {}
                pac::ADC.fcs().write_clear(|w| {
                    w.set_dreq_en(true);
                    w.set_shift(true);
                    w.set_en(true);
                });
            }
        }
        let auto_reset = ResetDmaConfig;

        let dma = unsafe { dma_ch.read(r.fifo().as_ptr() as *const W, buf as *mut [W], TreqSel::ADC, false) };
        // start conversions and wait for dma to finish. we can't report errors early
        // because there's no interrupt to signal them, and inspecting every element
        // of the fifo is too costly to do here.
        r.div().modify(|w| w.set_int(div));
        r.cs().write_set(|w| w.set_start_many(true));
        dma.await;
        mem::drop(auto_reset);
        // we can't report errors before the conversions have ended since no interrupt
        // exists to report them early, and since they're exceedingly rare we probably don't
        // want to anyway.
        match r.cs().read().err_sticky() {
            false => Ok(()),
            true => Err(Error::ConversionFailed),
        }
    }

    /// Fills `buf` through `dma` from the nonempty `ch` slice, ordered by ascending
    /// hardware channel index (not slice order); duplicate indices are sampled once.
    /// Panics for empty `ch` before accessing registers, including when `buf` is empty.
    /// `div` sets integer period: max(96, div + 1) ADC clocks per conversion
    /// with reset fractional divider. At 48 MHz, div 0..95 gives 500 ksample/s total.
    /// Per-channel rate is total rate divided by the number of distinct channels.
    /// Returns Ok after all writes, or ConversionFailed after completion; u16 holds
    /// 12-bit codes and u8 holds their high eight bits, with no per-sample error flag.
    /// Cancellation may leave a partial buffer, aborts DMA and stops ADC; cleanup
    /// waits for ready without timeout. This does not detect FIFO data loss.
    /// Empty `buf` still configures DMA and starts/stops ADC; it is not a hardware no-op.
    #[inline]
    pub async fn read_many_multichannel<S: AdcSample>(
        &mut self,
        ch: &mut [Channel<'_>],
        buf: &mut [S],
        div: u16,
        dma: &mut dma::Channel<'_>,
    ) -> Result<(), Error> {
        self.read_many_inner(ch.iter().map(|c| c.channel()), buf, false, div, dma)
            .await
    }

    /// Fills `buf` through `dma` from the nonempty `ch` slice, ordered by ascending
    /// hardware channel index (not slice order); duplicate indices are sampled once.
    /// Panics for empty `ch` before accessing registers, including when `buf` is empty.
    /// `div` sets integer period: max(96, div + 1) ADC clocks per conversion
    /// with reset fractional divider. At 48 MHz, div 0..95 gives 500 ksample/s total.
    /// Per-channel rate is total rate divided by the number of distinct channels.
    /// Conversion failures are carried in each Sample; overall sticky error is discarded.
    /// Cancellation may leave a partial buffer, aborts DMA and stops ADC; cleanup
    /// waits for ready without timeout. This does not detect FIFO data loss.
    /// Empty `buf` still configures DMA and starts/stops ADC; it is not a hardware no-op.
    #[inline]
    pub async fn read_many_multichannel_raw(
        &mut self,
        ch: &mut [Channel<'_>],
        buf: &mut [Sample],
        div: u16,
        dma: &mut dma::Channel<'_>,
    ) {
        // errors are reported in individual samples
        let _ = self
            .read_many_inner(
                ch.iter().map(|c| c.channel()),
                unsafe { mem::transmute::<_, &mut [u16]>(buf) },
                true,
                div,
                dma,
            )
            .await;
    }

    /// Fills `buf` through `dma` from `ch`, retaining channel and destination ownership.
    /// `div` sets integer period: max(96, div + 1) ADC clocks per conversion
    /// with reset fractional divider. At 48 MHz, div 0..95 gives 500 ksample/s total.
    /// Returns Ok after all writes, or ConversionFailed after completion; u16 holds
    /// 12-bit codes and u8 holds their high eight bits, with no per-sample error flag.
    /// Cancellation may leave a partial buffer, aborts DMA and stops ADC; cleanup
    /// waits for ready without timeout. This does not detect FIFO data loss.
    /// Empty `buf` still configures DMA and starts/stops ADC; it is not a hardware no-op.
    #[inline]
    pub async fn read_many<S: AdcSample>(
        &mut self,
        ch: &mut Channel<'_>,
        buf: &mut [S],
        div: u16,
        dma: &mut dma::Channel<'_>,
    ) -> Result<(), Error> {
        self.read_many_inner([ch.channel()].into_iter(), buf, false, div, dma)
            .await
    }

    /// Fills `buf` through `dma` from `ch`, retaining channel and destination ownership.
    /// `div` sets integer period: max(96, div + 1) ADC clocks per conversion
    /// with reset fractional divider. At 48 MHz, div 0..95 gives 500 ksample/s total.
    /// Conversion failures are carried in each Sample; overall sticky error is discarded.
    /// Cancellation may leave a partial buffer, aborts DMA and stops ADC; cleanup
    /// waits for ready without timeout. This does not detect FIFO data loss.
    /// Empty `buf` still configures DMA and starts/stops ADC; it is not a hardware no-op.
    #[inline]
    pub async fn read_many_raw(
        &mut self,
        ch: &mut Channel<'_>,
        buf: &mut [Sample],
        div: u16,
        dma: &mut dma::Channel<'_>,
    ) {
        // errors are reported in individual samples
        let _ = self
            .read_many_inner(
                [ch.channel()].into_iter(),
                unsafe { mem::transmute::<_, &mut [u16]>(buf) },
                true,
                div,
                dma,
            )
            .await;
    }
}

impl<'d> Adc<'d, Blocking> {
    /// Returns exclusive blocking ownership from `_inner`; `_config` has no settings.
    /// Resets/enables ADC and waits without timeout, clearing prior sensor enable.
    pub fn new_blocking(
        _inner: Peri<'d, ADC>,
        _config: Config,
    ) -> Self {
        Self::setup();

        Self { phantom: PhantomData }
    }
}

/// Interrupt handler.
pub struct InterruptHandler {
    /// Prevents external construction of the type-level handler marker.
    _empty: (),
}

impl interrupt::typelevel::Handler<interrupt::typelevel::ADC_IRQ_FIFO> for InterruptHandler {
    /// Disables ADC FIFO interrupt generation and wakes the registered reader.
    /// # Safety
    /// Invoke only as the bound ADC_IRQ_FIFO handler for the exclusively owned driver.
    unsafe fn on_interrupt() {
        let r = Adc::<Async>::regs();
        r.inte().write(|w| w.set_fifo(false));
        WAKER.wake();
    }
}

/// Limits DMA sample element widths to the supported u8/u16 representations.
trait SealedAdcSample: crate::dma::Word {}
/// Restricts sources to package ADC pins and the internal temperature sensor.
trait SealedAdcChannel {}

/// ADC sample.
#[allow(private_bounds)]
pub trait AdcSample: SealedAdcSample {}

impl SealedAdcSample for u16 {}
impl AdcSample for u16 {}

impl SealedAdcSample for u8 {}
impl AdcSample for u8 {}

/// ADC channel.
#[allow(private_bounds)]
pub trait AdcChannel: SealedAdcChannel {}
/// ADC pin.
pub trait AdcPin: AdcChannel + gpio::Pin {}

/// Associates a package ADC-capable pin with channel eligibility; mapping uses consecutive pin numbers.
macro_rules! impl_pin {
    ($pin:ident, $channel:expr) => {
        impl SealedAdcChannel for peripherals::$pin {}
        impl AdcChannel for peripherals::$pin {}
        impl AdcPin for peripherals::$pin {}
    };
}

#[cfg(any(feature = "rp235xa", feature = "rp2040"))]
impl_pin!(PIN_26, 0);
#[cfg(any(feature = "rp235xa", feature = "rp2040"))]
impl_pin!(PIN_27, 1);
#[cfg(any(feature = "rp235xa", feature = "rp2040"))]
impl_pin!(PIN_28, 2);
#[cfg(any(feature = "rp235xa", feature = "rp2040"))]
impl_pin!(PIN_29, 3);

#[cfg(feature = "rp235xb")]
impl_pin!(PIN_40, 0);
#[cfg(feature = "rp235xb")]
impl_pin!(PIN_41, 1);
#[cfg(feature = "rp235xb")]
impl_pin!(PIN_42, 2);
#[cfg(feature = "rp235xb")]
impl_pin!(PIN_43, 3);
#[cfg(feature = "rp235xb")]
impl_pin!(PIN_44, 4);
#[cfg(feature = "rp235xb")]
impl_pin!(PIN_45, 5);
#[cfg(feature = "rp235xb")]
impl_pin!(PIN_46, 6);
#[cfg(feature = "rp235xb")]
impl_pin!(PIN_47, 7);

impl SealedAdcChannel for peripherals::ADC_TEMP_SENSOR {}
impl AdcChannel for peripherals::ADC_TEMP_SENSOR {}

/// Returns the lowest selected hardware channel in `mask`; callers build the mask
/// only from owned ADC channels before accessing any registers. Panics for an empty mask.
fn first_channel(mask: u16) -> u8 {
    assert!(mask != 0, "at least one ADC channel is required");
    mask.trailing_zeros() as u8
}

/// Pure channel admission checks; no peripheral accesses are performed.
#[cfg(test)]
mod channel_tests {
    /// Empty selection must fail before reserved AINSEL can reach hardware.
    #[test]
    #[should_panic(expected = "at least one ADC channel")]
    fn empty_selection_is_rejected() { super::first_channel(0); }

    /// Lowest selected channel starts the ascending hardware round robin.
    #[test]
    fn selected_channel_order_is_hardware_order() {
        for index in 0..=8 {
            assert_eq!(super::first_channel(1 << index), index);
        }
        assert_eq!(super::first_channel((1 << 7) | (1 << 2)), 2);
    }
}
