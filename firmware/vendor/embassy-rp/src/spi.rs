//! Serial Peripheral Interface
//!
//! Eight-bit bus transfers; external chip select belongs to the caller. Async cancellation aborts
//! DMA but does not undo clocked bytes, drain FIFOs or deassert CS. Frequency derives from clk_peri.
use core::marker::PhantomData;

use embassy_embedded_hal::SetConfig;
use embassy_futures::join::join;
use embassy_hal_internal::{Peri, PeripheralType};
pub use embedded_hal_02::spi::{Phase, Polarity};

use crate::dma::{Channel, ChannelInstance};
use crate::gpio::{AnyPin, Pin as GpioPin, SealedPin as _};
use crate::{dma, interrupt, pac, peripherals};

/// SPI errors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[non_exhaustive]
pub enum Error {
    // No errors for now
}

/// SPI configuration.
#[non_exhaustive]
#[derive(Clone)]
pub struct Config {
    /// Requested maximum SCK frequency in Hz; hardware divisors round the actual rate down.
    pub frequency: u32,
    /// Phase.
    pub phase: Phase,
    /// Polarity.
    pub polarity: Polarity,
}

impl Default for Config {
    /// Returns 1 MHz, first-edge capture and idle-low configuration.
    fn default() -> Self {
        Self {
            frequency: 1_000_000,
            phase: Phase::CaptureOnFirstTransition,
            polarity: Polarity::IdleLow,
        }
    }
}

/// SPI driver.
pub struct Spi<'d, T: Instance, M: Mode> {
    /// Exclusive hardware SPI owner.
    inner: Peri<'d, T>,
    /// TX DMA owner; async methods require this channel.
    tx_dma: Option<Channel<'d>>,
    /// RX DMA owner; async read/transfer methods panic when absent.
    rx_dma: Option<Channel<'d>>,
    /// Retains peripheral lifetime and blocking/async mode in the type.
    phantom: PhantomData<(&'d mut T, M)>,
}

/// Pure prescaler arithmetic shared by constructors and rate changes.
#[path = "spi_divider.rs"]
mod divider;

/// Returns (CPSDVSR, SCR) for `freq` Hz using current peripheral-clock metadata.
/// Panics for unavailable/zero clocks, zero request or a rate below the hardware minimum.
fn calc_prescs(freq: u32) -> (u8, u8) { divider::calc_prescs(crate::clocks::clk_peri_freq(), freq) }

impl<'d, T: Instance, M: Mode> Spi<'d, T, M> {
    /// Returns enabled `inner` configured by `config`, consuming optional `clk`/`mosi`/`miso`/`cs`
    /// pins and `tx_dma`/`rx_dma` channels. Pins use function 1, Schmitt input and no pulls;
    /// omitted pins stay unconfigured. Zero/unavailable clocks and rates below the hardware minimum panic.
    fn new_inner(
        inner: Peri<'d, T>,
        clk: Option<Peri<'d, AnyPin>>,
        mosi: Option<Peri<'d, AnyPin>>,
        miso: Option<Peri<'d, AnyPin>>,
        cs: Option<Peri<'d, AnyPin>>,
        tx_dma: Option<Channel<'d>>,
        rx_dma: Option<Channel<'d>>,
        config: Config,
    ) -> Self {
        Self::apply_config(&inner, &config);

        let p = inner.regs();

        // Always enable DREQ signals -- harmless if DMA is not listening
        p.dmacr().write(|reg| {
            reg.set_rxdmae(true);
            reg.set_txdmae(true);
        });

        // finally, enable.
        p.cr1().write(|w| w.set_sse(true));

        if let Some(pin) = &clk {
            pin.gpio().ctrl().write(|w| w.set_funcsel(1));
            pin.pad_ctrl().write(|w| {
                #[cfg(feature = "_rp235x")]
                w.set_iso(false);
                w.set_schmitt(true);
                w.set_slewfast(false);
                w.set_ie(true);
                w.set_od(false);
                w.set_pue(false);
                w.set_pde(false);
            });
        }
        if let Some(pin) = &mosi {
            pin.gpio().ctrl().write(|w| w.set_funcsel(1));
            pin.pad_ctrl().write(|w| {
                #[cfg(feature = "_rp235x")]
                w.set_iso(false);
                w.set_schmitt(true);
                w.set_slewfast(false);
                w.set_ie(true);
                w.set_od(false);
                w.set_pue(false);
                w.set_pde(false);
            });
        }
        if let Some(pin) = &miso {
            pin.gpio().ctrl().write(|w| w.set_funcsel(1));
            pin.pad_ctrl().write(|w| {
                #[cfg(feature = "_rp235x")]
                w.set_iso(false);
                w.set_schmitt(true);
                w.set_slewfast(false);
                w.set_ie(true);
                w.set_od(false);
                w.set_pue(false);
                w.set_pde(false);
            });
        }
        if let Some(pin) = &cs {
            pin.gpio().ctrl().write(|w| w.set_funcsel(1));
            pin.pad_ctrl().write(|w| {
                #[cfg(feature = "_rp235x")]
                w.set_iso(false);
                w.set_schmitt(true);
                w.set_slewfast(false);
                w.set_ie(true);
                w.set_od(false);
                w.set_pue(false);
                w.set_pde(false);
            });
        }
        Self {
            inner,
            tx_dma,
            rx_dma,
            phantom: PhantomData,
        }
    }

    /// Applies eight-bit phase/polarity/frequency from `config` to `inner`; invalid rates can panic.
    ///
    /// Driver should be disabled before making changes and re-enabled after the modifications
    /// are applied.
    fn apply_config(
        inner: &Peri<'d, T>,
        config: &Config,
    ) {
        let p = inner.regs();
        let (presc, postdiv) = calc_prescs(config.frequency);

        p.cpsr().write(|w| w.set_cpsdvsr(presc));
        p.cr0().write(|w| {
            w.set_dss(0b0111); // 8bit
            w.set_spo(config.polarity == Polarity::IdleHigh);
            w.set_sph(config.phase == Phase::CaptureOnSecondTransition);
            w.set_scr(postdiv);
        });
    }

    /// Sends `data`, discards RX bytes and returns success after BUSY clears.
    /// Waits have no timeout; no external chip select is changed.
    pub fn blocking_write(
        &mut self,
        data: &[u8],
    ) -> Result<(), Error> {
        let p = self.inner.regs();
        for &b in data {
            while !p.sr().read().tnf() {}
            p.dr().write(|w| w.set_data(b as _));
            while !p.sr().read().rne() {}
            let _ = p.dr().read();
        }
        self.flush()?;
        Ok(())
    }

    /// Exchanges `data` in place and returns success after BUSY clears.
    /// Waits have no timeout; hardware stalls can leave a received prefix.
    pub fn blocking_transfer_in_place(
        &mut self,
        data: &mut [u8],
    ) -> Result<(), Error> {
        let p = self.inner.regs();
        for b in data {
            while !p.sr().read().tnf() {}
            p.dr().write(|w| w.set_data(*b as _));
            while !p.sr().read().rne() {}
            *b = p.dr().read().data() as u8;
        }
        self.flush()?;
        Ok(())
    }

    /// Fills `data` while transmitting zeros; returns success after BUSY clears, without timeout.
    pub fn blocking_read(
        &mut self,
        data: &mut [u8],
    ) -> Result<(), Error> {
        let p = self.inner.regs();
        for b in data {
            while !p.sr().read().tnf() {}
            p.dr().write(|w| w.set_data(0));
            while !p.sr().read().rne() {}
            *b = p.dr().read().data() as u8;
        }
        self.flush()?;
        Ok(())
    }

    /// Exchanges max(`read`.len(), `write`.len()) bytes, zero-padding TX and discarding excess RX.
    /// Returns success after BUSY clears; waits have no timeout.
    pub fn blocking_transfer(
        &mut self,
        read: &mut [u8],
        write: &[u8],
    ) -> Result<(), Error> {
        let p = self.inner.regs();
        let len = read.len().max(write.len());
        for i in 0..len {
            let wb = write.get(i).copied().unwrap_or(0);
            while !p.sr().read().tnf() {}
            p.dr().write(|w| w.set_data(wb as _));
            while !p.sr().read().rne() {}
            let rb = p.dr().read().data() as u8;
            if let Some(r) = read.get_mut(i) {
                *r = rb;
            }
        }
        self.flush()?;
        Ok(())
    }

    /// Spins until hardware BUSY clears and returns success; no timeout or RX FIFO drain.
    pub fn flush(&mut self) -> Result<(), Error> {
        let p = self.inner.regs();
        while p.sr().read().bsy() {}
        Ok(())
    }

    /// Sets requested `freq` Hz by disabling/reconfiguring/enabling SPI.
    /// Caller must first finish transfers; zero/unavailable clocks and rates below the hardware minimum panic.
    pub fn set_frequency(
        &mut self,
        freq: u32,
    ) {
        let (presc, postdiv) = calc_prescs(freq);
        let p = self.inner.regs();
        // disable
        p.cr1().write(|w| w.set_sse(false));

        // change stuff
        p.cpsr().write(|w| w.set_cpsdvsr(presc));
        p.cr0().modify(|w| {
            w.set_scr(postdiv);
        });

        // enable
        p.cr1().write(|w| w.set_sse(true));
    }

    /// Applies `config` by disabling/reconfiguring/enabling SPI; does not await active transfers.
    /// Invalid frequency can panic after the peripheral has been disabled.
    pub fn set_config(
        &mut self,
        config: &Config,
    ) {
        let p = self.inner.regs();

        // disable
        p.cr1().write(|w| w.set_sse(false));

        // change stuff
        Self::apply_config(&self.inner, config);

        // enable
        p.cr1().write(|w| w.set_sse(true));
    }
}

impl<'d, T: Instance> Spi<'d, T, Blocking> {
    /// Returns blocking `inner` using `clk`, `mosi`, `miso` and `config`; chip select stays external.
    pub fn new_blocking(
        inner: Peri<'d, T>,
        clk: Peri<'d, impl ClkPin<T> + 'd>,
        mosi: Peri<'d, impl MosiPin<T> + 'd>,
        miso: Peri<'d, impl MisoPin<T> + 'd>,
        config: Config,
    ) -> Self {
        Self::new_inner(
            inner,
            Some(clk.into()),
            Some(mosi.into()),
            Some(miso.into()),
            None,
            None,
            None,
            config,
        )
    }

    /// Returns blocking transmit-only `inner` using `clk`, `mosi` and `config`; no MISO pin is configured.
    pub fn new_blocking_txonly(
        inner: Peri<'d, T>,
        clk: Peri<'d, impl ClkPin<T> + 'd>,
        mosi: Peri<'d, impl MosiPin<T> + 'd>,
        config: Config,
    ) -> Self {
        Self::new_inner(
            inner,
            Some(clk.into()),
            Some(mosi.into()),
            None,
            None,
            None,
            None,
            config,
        )
    }

    /// Returns transmit-only `inner` using `mosi` and `config`, with no external SCK/MISO configured.
    pub fn new_blocking_txonly_nosck(
        inner: Peri<'d, T>,
        mosi: Peri<'d, impl MosiPin<T> + 'd>,
        config: Config,
    ) -> Self {
        Self::new_inner(inner, None, Some(mosi.into()), None, None, None, None, config)
    }

    /// Returns blocking receive-only `inner` using `clk`, `miso` and `config`; reads clock dummy zeros.
    pub fn new_blocking_rxonly(
        inner: Peri<'d, T>,
        clk: Peri<'d, impl ClkPin<T> + 'd>,
        miso: Peri<'d, impl MisoPin<T> + 'd>,
        config: Config,
    ) -> Self {
        Self::new_inner(
            inner,
            Some(clk.into()),
            None,
            Some(miso.into()),
            None,
            None,
            None,
            config,
        )
    }
}

impl<'d, T: Instance> Spi<'d, T, Async> {
    /// Returns DMA `inner` with `clk`, `mosi`, `miso` and `config`.
    /// Consumes `tx_dma`/`rx_dma`; `irq` binds both completion handlers and enables their IRQs.
    pub fn new<TxDma: ChannelInstance, RxDma: ChannelInstance>(
        inner: Peri<'d, T>,
        clk: Peri<'d, impl ClkPin<T> + 'd>,
        mosi: Peri<'d, impl MosiPin<T> + 'd>,
        miso: Peri<'d, impl MisoPin<T> + 'd>,
        tx_dma: Peri<'d, TxDma>,
        rx_dma: Peri<'d, RxDma>,
        irq: impl interrupt::typelevel::Binding<TxDma::Interrupt, dma::InterruptHandler<TxDma>>
        + interrupt::typelevel::Binding<RxDma::Interrupt, dma::InterruptHandler<RxDma>>
        + 'd,
        config: Config,
    ) -> Self {
        let tx_dma_ch = dma::Channel::new(tx_dma, irq);
        let rx_dma_ch = dma::Channel::new(rx_dma, irq);
        Self::new_inner(
            inner,
            Some(clk.into()),
            Some(mosi.into()),
            Some(miso.into()),
            None,
            Some(tx_dma_ch),
            Some(rx_dma_ch),
            config,
        )
    }

    /// Returns DMA transmit-only `inner` using `clk`, `mosi`, `tx_dma` and `config`.
    /// `irq` binds TX completion; async read/transfer methods panic because no RX DMA exists.
    pub fn new_txonly<TxDma: ChannelInstance>(
        inner: Peri<'d, T>,
        clk: Peri<'d, impl ClkPin<T> + 'd>,
        mosi: Peri<'d, impl MosiPin<T> + 'd>,
        tx_dma: Peri<'d, TxDma>,
        irq: impl interrupt::typelevel::Binding<TxDma::Interrupt, dma::InterruptHandler<TxDma>> + 'd,
        config: Config,
    ) -> Self {
        let tx_dma_ch = dma::Channel::new(tx_dma, irq);
        Self::new_inner(
            inner,
            Some(clk.into()),
            Some(mosi.into()),
            None,
            None,
            Some(tx_dma_ch),
            None,
            config,
        )
    }

    /// Returns DMA transmit-only `inner` using `mosi`, `tx_dma` and `config`, without external SCK.
    /// `irq` binds TX completion; async read/transfer methods panic because no RX DMA exists.
    pub fn new_txonly_nosck<TxDma: ChannelInstance>(
        inner: Peri<'d, T>,
        mosi: Peri<'d, impl MosiPin<T> + 'd>,
        tx_dma: Peri<'d, TxDma>,
        irq: impl interrupt::typelevel::Binding<TxDma::Interrupt, dma::InterruptHandler<TxDma>> + 'd,
        config: Config,
    ) -> Self {
        let tx_dma_ch = dma::Channel::new(tx_dma, irq);
        Self::new_inner(
            inner,
            None,
            Some(mosi.into()),
            None,
            None,
            Some(tx_dma_ch),
            None,
            config,
        )
    }

    /// Returns DMA receive-only `inner` using `clk`, `miso` and `config`.
    /// `tx_dma` clocks zero bytes; `rx_dma` receives them. `irq` binds both completion handlers.
    pub fn new_rxonly<TxDma: ChannelInstance, RxDma: ChannelInstance>(
        inner: Peri<'d, T>,
        clk: Peri<'d, impl ClkPin<T> + 'd>,
        miso: Peri<'d, impl MisoPin<T> + 'd>,
        tx_dma: Peri<'d, TxDma>,
        rx_dma: Peri<'d, RxDma>,
        irq: impl interrupt::typelevel::Binding<TxDma::Interrupt, dma::InterruptHandler<TxDma>>
        + interrupt::typelevel::Binding<RxDma::Interrupt, dma::InterruptHandler<RxDma>>
        + 'd,
        config: Config,
    ) -> Self {
        let tx_dma_ch = dma::Channel::new(tx_dma, irq);
        let rx_dma_ch = dma::Channel::new(rx_dma, irq);
        Self::new_inner(
            inner,
            Some(clk.into()),
            None,
            Some(miso.into()),
            None,
            Some(tx_dma_ch),
            Some(rx_dma_ch),
            config,
        )
    }

    /// Sends `buffer` through DMA, waits for SPI idle, drains RX and clears overrun.
    /// Returns success; hardware waits are unbounded. Cancellation aborts DMA but skips final
    /// FIFO/idle cleanup, so it does not establish a reusable transaction boundary.
    pub async fn write(
        &mut self,
        buffer: &[u8],
    ) -> Result<(), Error> {
        let tx_transfer = unsafe {
            // If we don't assign future to a variable, the data register pointer
            // is held across an await and makes the future non-Send.
            self.tx_dma
                .as_mut()
                .unwrap()
                .write(buffer, self.inner.regs().dr().as_ptr() as *mut _, T::TX_DREQ, false)
        };
        tx_transfer.await;

        let p = self.inner.regs();
        while p.sr().read().bsy() {}

        // clear RX FIFO contents to prevent stale reads
        while p.sr().read().rne() {
            let _: u16 = p.dr().read().data();
        }
        // clear RX overrun interrupt
        p.icr().write(|w| w.set_roric(true));

        Ok(())
    }

    /// Fills `buffer` via RX DMA while TX DMA clocks zeros; returns after both transfers complete.
    /// Panics without RX DMA. Cancellation aborts DMA and may leave partial output/FIFO data.
    pub async fn read(
        &mut self,
        buffer: &mut [u8],
    ) -> Result<(), Error> {
        // Start RX first. Transfer starts when TX starts, if RX
        // is not started yet we might lose bytes.
        let rx_transfer = unsafe {
            // If we don't assign future to a variable, the data register pointer
            // is held across an await and makes the future non-Send.
            self.rx_dma
                .as_mut()
                .unwrap()
                .read(self.inner.regs().dr().as_ptr() as *const _, buffer, T::RX_DREQ, false)
        };

        let tx_transfer = unsafe {
            // If we don't assign future to a variable, the data register pointer
            // is held across an await and makes the future non-Send.
            self.tx_dma.as_mut().unwrap().write_zeros(
                buffer.len(),
                self.inner.regs().dr().as_ptr() as *mut u8,
                T::TX_DREQ,
            )
        };
        join(tx_transfer, rx_transfer).await;
        Ok(())
    }

    /// Exchanges `tx_buffer` into `rx_buffer`, zero-padding short TX and discarding excess RX.
    /// Returns success after DMA completion; TX-longer cleanup also waits for SPI idle.
    /// Panics without RX DMA; cancellation can leave partial output and pending peripheral bytes.
    pub async fn transfer(
        &mut self,
        rx_buffer: &mut [u8],
        tx_buffer: &[u8],
    ) -> Result<(), Error> {
        self.transfer_inner(rx_buffer, tx_buffer).await
    }

    /// Exchanges `words` in place; returns after TX/RX DMA completion.
    /// TX consumes each byte before corresponding RX writes it. Cancellation leaves partial output.
    pub async fn transfer_in_place(
        &mut self,
        words: &mut [u8],
    ) -> Result<(), Error> {
        self.transfer_inner(words, words).await
    }

    /// Exchanges raw `tx`/`rx` spans and returns success after required DMA/overflow cleanup.
    /// Private callers retain both spans for this future; exact in-place aliasing is supported.
    /// Requires both DMA channels; dropping aborts them without completing peripheral cleanup.
    async fn transfer_inner(
        &mut self,
        rx: *mut [u8],
        tx: *const [u8],
    ) -> Result<(), Error> {
        // Start RX first. Transfer starts when TX starts, if RX
        // is not started yet we might lose bytes.
        let rx_transfer = unsafe {
            // If we don't assign future to a variable, the data register pointer
            // is held across an await and makes the future non-Send.
            self.rx_dma
                .as_mut()
                .unwrap()
                .read(self.inner.regs().dr().as_ptr() as *const _, rx, T::RX_DREQ, false)
        };

        let tx_ch = self.tx_dma.as_mut().unwrap();
        // If we don't assign future to a variable, the data register pointer
        // is held across an await and makes the future non-Send.
        let tx_transfer = async {
            let p = self.inner.regs();
            unsafe {
                tx_ch.write(tx, p.dr().as_ptr() as *mut _, T::TX_DREQ, false).await;

                if rx.len() > tx.len() {
                    let write_bytes_len = rx.len() - tx.len();
                    // write dummy data
                    // this will disable incrementation of the buffers
                    tx_ch
                        .write_zeros(write_bytes_len, p.dr().as_ptr() as *mut u8, T::TX_DREQ)
                        .await
                }
            }
        };
        join(tx_transfer, rx_transfer).await;

        // if tx > rx we should clear any overflow of the FIFO SPI buffer
        if tx.len() > rx.len() {
            let p = self.inner.regs();
            while p.sr().read().bsy() {}

            // clear RX FIFO contents to prevent stale reads
            while p.sr().read().rne() {
                let _: u16 = p.dr().read().data();
            }
            // clear RX overrun interrupt
            p.icr().write(|w| w.set_roric(true));
        }

        Ok(())
    }
}

/// Restricts mode markers to blocking and async.
trait SealedMode {}

/// Per-instance register and DMA request metadata.
trait SealedInstance {
    /// DMA pacing source for TX FIFO space.
    const TX_DREQ: pac::dma::vals::TreqSel;
    /// DMA pacing source for RX FIFO data.
    const RX_DREQ: pac::dma::vals::TreqSel;

    /// Returns the owned instance raw register block.
    fn regs(&self) -> pac::spi::Spi;
}

/// Mode.
#[allow(private_bounds)]
pub trait Mode: SealedMode {}

/// SPI instance trait.
#[allow(private_bounds)]
pub trait Instance: SealedInstance + PeripheralType {}

/// Defines register/DREQ metadata for each hardware SPI singleton.
macro_rules! impl_instance {
    ($type:ident, $irq:ident, $tx_dreq:expr, $rx_dreq:expr) => {
        impl SealedInstance for peripherals::$type {
            /// DMA pacing source for RX FIFO data.
            const RX_DREQ: pac::dma::vals::TreqSel = $rx_dreq;
            /// DMA pacing source for TX FIFO space.
            const TX_DREQ: pac::dma::vals::TreqSel = $tx_dreq;

            /// Returns this singleton raw SPI register block.
            fn regs(&self) -> pac::spi::Spi { pac::$type }
        }
        impl Instance for peripherals::$type {}
    };
}

impl_instance!(
    SPI0,
    Spi0,
    pac::dma::vals::TreqSel::SPI0_TX,
    pac::dma::vals::TreqSel::SPI0_RX
);
impl_instance!(
    SPI1,
    Spi1,
    pac::dma::vals::TreqSel::SPI1_TX,
    pac::dma::vals::TreqSel::SPI1_RX
);

/// CLK pin.
pub trait ClkPin<T: Instance>: GpioPin {}
/// CS pin.
pub trait CsPin<T: Instance>: GpioPin {}
/// MOSI pin.
pub trait MosiPin<T: Instance>: GpioPin {}
/// MISO pin.
pub trait MisoPin<T: Instance>: GpioPin {}

/// Marks GPIO support for one instance-specific SPI signal alternate function.
macro_rules! impl_pin {
    ($pin:ident, $instance:ident, $function:ident) => {
        impl $function<peripherals::$instance> for peripherals::$pin {}
    };
}

impl_pin!(PIN_0, SPI0, MisoPin);
impl_pin!(PIN_1, SPI0, CsPin);
impl_pin!(PIN_2, SPI0, ClkPin);
impl_pin!(PIN_3, SPI0, MosiPin);
impl_pin!(PIN_4, SPI0, MisoPin);
impl_pin!(PIN_5, SPI0, CsPin);
impl_pin!(PIN_6, SPI0, ClkPin);
impl_pin!(PIN_7, SPI0, MosiPin);
impl_pin!(PIN_8, SPI1, MisoPin);
impl_pin!(PIN_9, SPI1, CsPin);
impl_pin!(PIN_10, SPI1, ClkPin);
impl_pin!(PIN_11, SPI1, MosiPin);
impl_pin!(PIN_12, SPI1, MisoPin);
impl_pin!(PIN_13, SPI1, CsPin);
impl_pin!(PIN_14, SPI1, ClkPin);
impl_pin!(PIN_15, SPI1, MosiPin);
impl_pin!(PIN_16, SPI0, MisoPin);
impl_pin!(PIN_17, SPI0, CsPin);
impl_pin!(PIN_18, SPI0, ClkPin);
impl_pin!(PIN_19, SPI0, MosiPin);
impl_pin!(PIN_20, SPI0, MisoPin);
impl_pin!(PIN_21, SPI0, CsPin);
impl_pin!(PIN_22, SPI0, ClkPin);
impl_pin!(PIN_23, SPI0, MosiPin);
impl_pin!(PIN_24, SPI1, MisoPin);
impl_pin!(PIN_25, SPI1, CsPin);
impl_pin!(PIN_26, SPI1, ClkPin);
impl_pin!(PIN_27, SPI1, MosiPin);
impl_pin!(PIN_28, SPI1, MisoPin);
impl_pin!(PIN_29, SPI1, CsPin);
#[cfg(feature = "rp235xb")]
impl_pin!(PIN_30, SPI1, ClkPin);
#[cfg(feature = "rp235xb")]
impl_pin!(PIN_31, SPI1, MosiPin);
#[cfg(feature = "rp235xb")]
impl_pin!(PIN_32, SPI0, MisoPin);
#[cfg(feature = "rp235xb")]
impl_pin!(PIN_33, SPI0, CsPin);
#[cfg(feature = "rp235xb")]
impl_pin!(PIN_34, SPI0, ClkPin);
#[cfg(feature = "rp235xb")]
impl_pin!(PIN_35, SPI0, MosiPin);
#[cfg(feature = "rp235xb")]
impl_pin!(PIN_36, SPI0, MisoPin);
#[cfg(feature = "rp235xb")]
impl_pin!(PIN_37, SPI0, CsPin);
#[cfg(feature = "rp235xb")]
impl_pin!(PIN_38, SPI0, ClkPin);
#[cfg(feature = "rp235xb")]
impl_pin!(PIN_39, SPI0, MosiPin);
#[cfg(feature = "rp235xb")]
impl_pin!(PIN_40, SPI1, MisoPin);
#[cfg(feature = "rp235xb")]
impl_pin!(PIN_41, SPI1, CsPin);
#[cfg(feature = "rp235xb")]
impl_pin!(PIN_42, SPI1, ClkPin);
#[cfg(feature = "rp235xb")]
impl_pin!(PIN_43, SPI1, MosiPin);
#[cfg(feature = "rp235xb")]
impl_pin!(PIN_44, SPI1, MisoPin);
#[cfg(feature = "rp235xb")]
impl_pin!(PIN_45, SPI1, CsPin);
#[cfg(feature = "rp235xb")]
impl_pin!(PIN_46, SPI1, ClkPin);
#[cfg(feature = "rp235xb")]
impl_pin!(PIN_47, SPI1, MosiPin);

/// Seals an existing blocking/async marker as a valid driver mode.
macro_rules! impl_mode {
    ($name:ident) => {
        impl SealedMode for $name {}
        impl Mode for $name {}
    };
}

/// Blocking mode.
pub struct Blocking;
/// Async mode.
pub struct Async;

impl_mode!(Blocking);
impl_mode!(Async);

// ====================

impl<'d, T: Instance, M: Mode> embedded_hal_02::blocking::spi::Transfer<u8> for Spi<'d, T, M> {
    /// Uninhabited driver error; runtime failure is currently a stall or panic.
    type Error = Error;

    /// Exchanges `words` in place and returns their received view after blocking completion.
    fn transfer<'w>(
        &mut self,
        words: &'w mut [u8],
    ) -> Result<&'w [u8], Self::Error> {
        self.blocking_transfer_in_place(words)?;
        Ok(words)
    }
}

impl<'d, T: Instance, M: Mode> embedded_hal_02::blocking::spi::Write<u8> for Spi<'d, T, M> {
    /// Uninhabited driver error; runtime failure is currently a stall or panic.
    type Error = Error;

    /// Sends `words` and returns after blocking idle/FIFO cleanup.
    fn write(
        &mut self,
        words: &[u8],
    ) -> Result<(), Self::Error> {
        self.blocking_write(words)
    }
}

impl embedded_hal_1::spi::Error for Error {
    /// No error value can be constructed, so this has no reachable return case.
    fn kind(&self) -> embedded_hal_1::spi::ErrorKind { match *self {} }
}

impl<'d, T: Instance, M: Mode> embedded_hal_1::spi::ErrorType for Spi<'d, T, M> {
    /// Uninhabited driver error; runtime failure is currently a stall or panic.
    type Error = Error;
}

impl<'d, T: Instance, M: Mode> embedded_hal_1::spi::SpiBus<u8> for Spi<'d, T, M> {
    /// Returns success immediately; this trait adapter performs no additional hardware wait.
    fn flush(&mut self) -> Result<(), Self::Error> { Ok(()) }

    /// Fills `words` with zero-clocked RX bytes; returns after blocking completion.
    fn read(
        &mut self,
        words: &mut [u8],
    ) -> Result<(), Self::Error> {
        self.blocking_transfer(words, &[])
    }

    /// Sends `words` and returns after blocking idle/FIFO cleanup.
    fn write(
        &mut self,
        words: &[u8],
    ) -> Result<(), Self::Error> {
        self.blocking_write(words)
    }

    /// Exchanges `write` into `read` with length padding/discard; returns after blocking completion.
    fn transfer(
        &mut self,
        read: &mut [u8],
        write: &[u8],
    ) -> Result<(), Self::Error> {
        self.blocking_transfer(read, write)
    }

    /// Exchanges `words` in place; returns after blocking completion.
    fn transfer_in_place(
        &mut self,
        words: &mut [u8],
    ) -> Result<(), Self::Error> {
        self.blocking_transfer_in_place(words)
    }
}

impl<'d, T: Instance> embedded_hal_async::spi::SpiBus<u8> for Spi<'d, T, Async> {
    /// Returns success immediately; does not wait for DMA-cancellation remnants or SPI idle.
    async fn flush(&mut self) -> Result<(), Self::Error> { Ok(()) }

    /// Sends `words`; returns the DMA-write result after idle/FIFO cleanup.
    async fn write(
        &mut self,
        words: &[u8],
    ) -> Result<(), Self::Error> {
        self.write(words).await
    }

    /// Fills `words` using zero-clocked DMA; returns the read result with partial-output cancellation.
    async fn read(
        &mut self,
        words: &mut [u8],
    ) -> Result<(), Self::Error> {
        self.read(words).await
    }

    /// Exchanges `write` into `read`; returns the DMA result with partial-output cancellation.
    async fn transfer(
        &mut self,
        read: &mut [u8],
        write: &[u8],
    ) -> Result<(), Self::Error> {
        self.transfer(read, write).await
    }

    /// Exchanges `words` in place; returns the DMA result with partial-output cancellation.
    async fn transfer_in_place(
        &mut self,
        words: &mut [u8],
    ) -> Result<(), Self::Error> {
        self.transfer_in_place(words).await
    }
}

impl<'d, T: Instance, M: Mode> SetConfig for Spi<'d, T, M> {
    /// Eight-bit phase/polarity/requested-rate configuration.
    type Config = Config;
    /// No returned configuration error is produced; invalid rates can panic.
    type ConfigError = ();

    /// Applies `config` and returns success; inherits idle precondition and panic behavior.
    fn set_config(
        &mut self,
        config: &Self::Config,
    ) -> Result<(), ()> {
        self.set_config(config);

        Ok(())
    }
}
