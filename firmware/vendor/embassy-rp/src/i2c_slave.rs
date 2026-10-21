//! Interrupt-driven I2C target with bounded FIFO draining and clock stretching.
//!
//! Reads require a prompt response; cancelling a response can stall the controller
//! until [`I2cSlave::reset`]. Listening also consumes bytes before completion, so
//! cancellation does not restore the stream. No operation has a built-in timeout.
//! Register behavior follows the [RP2040 I2C chapter](https://datasheets.raspberrypi.com/rp2040/rp2040-datasheet.pdf).
use core::future;
use core::marker::PhantomData;
use core::task::Poll;

use pac::i2c;

use crate::i2c::{AbortReason, FIFO_SIZE, Instance, InterruptHandler, SclPin, SdaPin, set_up_i2c_pin};
use crate::interrupt::typelevel::{Binding, Interrupt};
use crate::{Peri, pac};

/// I2C error
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[non_exhaustive]
pub enum Error {
    /// I2C abort with error
    Abort(
        /// Decoded hardware abort source.
        AbortReason,
    ),
    /// User passed in a response buffer that was 0 length
    InvalidResponseBufferLength,
    /// User passed in a command buffer that was 0 length.
    InvalidListenBufferLength,
    /// The response buffer length was too short to contain the message
    ///
    /// The length parameter will always be the length of the buffer, and is
    /// provided as a convenience for matching alongside `Command::Write`.
    PartialWrite(
        /// Bytes copied, equal to destination capacity.
        usize,
    ),
    /// The response buffer length was too short to contain the message
    ///
    /// The length parameter will always be the length of the buffer, and is
    /// provided as a convenience for matching alongside `Command::GeneralCall`.
    PartialGeneralCall(
        /// Bytes copied, equal to destination capacity.
        usize,
    ),
}

/// Pure admission/lookahead tests avoid peripheral registers.
#[cfg(test)]
mod tests {
    use core::future::Future;
    use core::pin::Pin;
    use core::task::Context;

    use super::*;

    /// Empty admission must retain the next transaction byte without MMIO.
    #[test]
    fn empty_listen_is_rejected_without_consuming_pending_data() {
        let mut driver = I2cSlave::<crate::peripherals::I2C0> {
            phantom: PhantomData,
            pending_byte: Some(0xA5),
            config: Config::default(),
        };
        let mut empty = [];
        let mut cx = Context::from_waker(core::task::Waker::noop());
        {
            let mut future = core::pin::pin!(driver.listen(&mut empty));
            assert_eq!(
                Pin::as_mut(&mut future).poll(&mut cx),
                Poll::Ready(Err(Error::InvalidListenBufferLength))
            );
        }
        assert_eq!(driver.pending_byte, Some(0xA5));
    }

    /// No-space destinations retain lookahead, then a later one-byte read consumes it once.
    #[test]
    fn pending_byte_survives_empty_or_full_destination() {
        let mut driver = I2cSlave::<crate::peripherals::I2C0> {
            phantom: PhantomData,
            pending_byte: Some(0xA5),
            config: Config::default(),
        };
        driver.drain_fifo(&mut [], &mut 0);
        assert_eq!(driver.pending_byte, Some(0xA5));
        let mut buffer = [0u8];
        let mut offset = 1;
        driver.drain_fifo(&mut buffer, &mut offset);
        assert_eq!(driver.pending_byte, Some(0xA5));
        assert_eq!(buffer, [0]);
        offset = 0;
        driver.drain_fifo(&mut buffer, &mut offset);
        assert_eq!((buffer, offset, driver.pending_byte), ([0xA5], 1, None));
    }
}

/// Received command
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum Command {
    /// General Call
    GeneralCall(
        /// General-call bytes copied into the listen buffer.
        usize,
    ),
    /// Read
    Read,
    /// Write+read
    WriteRead(
        /// Bytes copied before the repeated-start read request.
        usize,
    ),
    /// Write
    Write(
        /// Write bytes copied before STOP.
        usize,
    ),
}

/// Possible responses to responding to a read
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum ReadStatus {
    /// Transaction Complete, controller naked our last byte
    Done,
    /// Transaction Incomplete, controller trying to read more bytes than were provided
    NeedMoreBytes,
    /// Transaction Complete, but controller stopped reading bytes before we ran out
    LeftoverBytes(
        /// Hardware TX-flush count, excluding response bytes never queued.
        u16,
    ),
}

/// Slave Configuration
#[non_exhaustive]
#[derive(Copy, Clone)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct Config {
    /// Nonzero target address written to SAR; this driver leaves 7-bit addressing selected.
    pub addr: u16,
    /// Control if the peripheral should ack to and report general calls.
    pub general_call: bool,
    /// Enable internal pullup on SDA.
    ///
    /// Using external pullup resistors is recommended for I2C. If you do
    /// have external pullups you should not enable this.
    pub sda_pullup: bool,
    /// Enable internal pullup on SCL.
    ///
    /// Using external pullup resistors is recommended for I2C. If you do
    /// have external pullups you should not enable this.
    pub scl_pullup: bool,
}

impl Default for Config {
    /// Returns address 0x55 with general-call acknowledgement and both internal pullups enabled.
    fn default() -> Self {
        Self {
            addr: 0x55,
            general_call: true,
            sda_pullup: true,
            scl_pullup: true,
        }
    }
}

/// I2CSlave driver.
pub struct I2cSlave<'d, T: Instance> {
    /// Exclusive peripheral lifetime; construction consumes its token.
    phantom: PhantomData<&'d mut T>,
    /// First byte consumed from the following transaction, or `None` when no lookahead is held.
    pending_byte: Option<u8>,
    /// Settings reapplied by reset, including address and general-call policy.
    config: Config,
}

impl<'d, T: Instance> I2cSlave<'d, T> {
    /// Returns a target owning `_peri`, with `scl`/`sda` configured from `config`.
    /// `_irq` proves the interrupt-handler binding; construction enables that IRQ.
    /// Panics for address zero; resets the peripheral and discards existing traffic.
    pub fn new(
        _peri: Peri<'d, T>,
        scl: Peri<'d, impl SclPin<T>>,
        sda: Peri<'d, impl SdaPin<T>>,
        _irq: impl Binding<T::Interrupt, InterruptHandler<T>>,
        config: Config,
    ) -> Self {
        assert!(config.addr != 0);

        // Configure SCL & SDA pins
        set_up_i2c_pin(&scl, config.scl_pullup);
        set_up_i2c_pin(&sda, config.sda_pullup);

        let mut ret = Self {
            phantom: PhantomData,
            pending_byte: None,
            config,
        };

        ret.reset();

        ret
    }

    /// Reset the i2c peripheral. If you cancel a respond_to_read, you may stall the bus.
    /// You can recover the bus by calling this function, but doing so will almost certainly cause
    /// an i/o error in the master.
    pub fn reset(&mut self) {
        self.pending_byte = None;
        let p = T::regs();

        let reset = T::reset();
        crate::reset::reset(reset);
        crate::reset::unreset_wait(reset);

        p.ic_enable().write(|w| w.set_enable(false));

        p.ic_sar().write(|w| w.set_ic_sar(self.config.addr));
        p.ic_con().modify(|w| {
            w.set_master_mode(false);
            w.set_ic_slave_disable(false);
            w.set_tx_empty_ctrl(true);
            w.set_rx_fifo_full_hld_ctrl(true);

            // This typically makes no sense for a slave, but it is used to
            // tune spike suppression, according to the datasheet.
            w.set_speed(pac::i2c::vals::Speed::FAST);

            // Generate stop interrupts for general calls
            // This also causes stop interrupts for other devices on the bus but those will not be
            // propagated up to the application.
            w.set_stop_det_ifaddressed(!self.config.general_call);
        });
        p.ic_ack_general_call()
            .write(|w| w.set_ack_gen_call(self.config.general_call));

        // Set FIFO watermarks to 1 to make things simpler. This is encoded
        // by a register value of 0. Rx watermark should never change, but Tx watermark will be
        // adjusted in operation.
        p.ic_tx_tl().write(|w| w.set_tx_tl(0));
        p.ic_rx_tl().write(|w| w.set_rx_tl(0));

        // Clear interrupts
        p.ic_clr_intr().read();

        // Enable I2C block
        p.ic_enable().write(|w| w.set_enable(true));

        // mask everything initially
        p.ic_intr_mask().write_value(i2c::regs::IcIntrMask(0));
        T::Interrupt::unpend();
        unsafe { T::Interrupt::enable() };
    }

    /// Returns the ready value from `f`, registering the waker before each check.
    /// Calls `g` after every pending poll to arm interrupts; cancellation leaves hardware state intact.
    #[inline(always)]
    async fn wait_on<F, U, G>(
        &mut self,
        mut f: F,
        mut g: G,
    ) -> U
    where
        F: FnMut(&mut Self) -> Poll<U>,
        G: FnMut(&mut Self),
    {
        future::poll_fn(|cx| {
            // Register prior to checking the condition
            T::waker().register(cx.waker());
            let r = f(self);

            if r.is_pending() {
                g(self);
            }

            r
        })
        .await
    }

    /// Copies pending/FIFO bytes into `buffer` starting at `offset`, advancing `offset`.
    /// Full or empty destinations preserve lookahead; the first byte of a following
    /// transaction is retained. An offset at/beyond capacity performs no MMIO.
    #[inline(always)]
    fn drain_fifo(
        &mut self,
        buffer: &mut [u8],
        offset: &mut usize,
    ) {
        if *offset >= buffer.len() {
            return;
        }
        let p = T::regs();

        if let Some(pending) = self.pending_byte.take() {
            buffer[*offset] = pending;
            *offset += 1;
        }

        for b in &mut buffer[*offset..] {
            if !p.ic_status().read().rfne() {
                break;
            }

            let dat = p.ic_data_cmd().read();
            if *offset != 0 && dat.first_data_byte() {
                // The RP2040 state machine will keep placing bytes into the
                // FIFO, even if they are part of a subsequent write transaction.
                //
                // Unfortunately merely reading ic_data_cmd will consume that
                // byte, the first byte of the next transaction, so we need
                // to store it elsewhere
                self.pending_byte = Some(dat.dat());
                break;
            }

            *b = dat.dat();
            *offset += 1;
        }
    }

    /// Returns the next controller command, copying write/general-call bytes into `buffer`.
    /// Empty `buffer` is rejected without consuming lookahead; filling it returns a
    /// partial-write error even if STOP is already observed, so allow spare capacity.
    /// Pure reads leave bytes untouched. Cancellation loses this call's byte count
    /// and may leave data consumed; neither timeout nor automatic reset is provided.
    pub async fn listen(
        &mut self,
        buffer: &mut [u8],
    ) -> Result<Command, Error> {
        if buffer.is_empty() {
            return Err(Error::InvalidListenBufferLength);
        }
        let p = T::regs();

        // set rx fifo watermark to 1 byte
        p.ic_rx_tl().write(|w| w.set_rx_tl(0));

        let mut len = 0;
        self.wait_on(
            |me| {
                let stat = p.ic_raw_intr_stat().read();
                trace!("ls:{:013b} len:{}", stat.0, len);

                if p.ic_rxflr().read().rxflr() > 0 || me.pending_byte.is_some() {
                    me.drain_fifo(buffer, &mut len);
                    // we're receiving data, set rx fifo watermark to 12 bytes (3/4 full) to reduce interrupt noise
                    p.ic_rx_tl().write(|w| w.set_rx_tl(11));
                }

                if buffer.len() == len {
                    if stat.gen_call() {
                        return Poll::Ready(Err(Error::PartialGeneralCall(buffer.len())));
                    } else {
                        return Poll::Ready(Err(Error::PartialWrite(buffer.len())));
                    }
                }
                trace!("len:{}, pend:{:?}", len, me.pending_byte);
                if me.pending_byte.is_some() {
                    warn!("pending")
                }

                if stat.restart_det() && stat.rd_req() {
                    p.ic_clr_restart_det().read();
                    Poll::Ready(Ok(Command::WriteRead(len)))
                } else if stat.gen_call() && stat.stop_det() && len > 0 {
                    p.ic_clr_gen_call().read();
                    p.ic_clr_stop_det().read();
                    Poll::Ready(Ok(Command::GeneralCall(len)))
                } else if stat.stop_det() && len > 0 {
                    p.ic_clr_stop_det().read();
                    Poll::Ready(Ok(Command::Write(len)))
                } else if stat.rd_req() {
                    p.ic_clr_stop_det().read();
                    p.ic_clr_restart_det().read();
                    p.ic_clr_gen_call().read();
                    Poll::Ready(Ok(Command::Read))
                } else if stat.stop_det() {
                    // clear stuck stop bit
                    // This can happen if the SDA/SCL pullups are enabled after calling this func
                    p.ic_clr_stop_det().read();
                    Poll::Pending
                } else {
                    Poll::Pending
                }
            },
            |_me| {
                p.ic_intr_mask().write(|w| {
                    w.set_m_stop_det(true);
                    w.set_m_restart_det(true);
                    w.set_m_gen_call(true);
                    w.set_m_rd_req(true);
                    w.set_m_rx_full(true);
                });
            },
        )
        .await
    }

    /// Queues `buffer` for the pending controller read and returns its completion status.
    /// Empty input returns `InvalidResponseBufferLength`; hardware aborts return `Abort`
    /// except TX leftovers, reported as `LeftoverBytes` with the hardware flush count.
    /// That count excludes later chunks never queued. Cancellation may leave SCL
    /// stretched; call `reset` to recover, disrupting the controller's transfer.
    pub async fn respond_to_read(
        &mut self,
        buffer: &[u8],
    ) -> Result<ReadStatus, Error> {
        let p = T::regs();

        if buffer.is_empty() {
            return Err(Error::InvalidResponseBufferLength);
        }

        let mut chunks = buffer.chunks(FIFO_SIZE as usize);

        self.wait_on(
            |me| {
                let stat = p.ic_raw_intr_stat().read();
                trace!("rs:{:013b}", stat.0);

                if stat.tx_abrt() {
                    if let Err(abort_reason) = me.read_and_clear_abort_reason() {
                        if let Error::Abort(AbortReason::TxNotEmpty(bytes)) = abort_reason {
                            p.ic_clr_intr().read();
                            return Poll::Ready(Ok(ReadStatus::LeftoverBytes(bytes)));
                        } else {
                            return Poll::Ready(Err(abort_reason));
                        }
                    }
                }

                if let Some(chunk) = chunks.next() {
                    for byte in chunk {
                        p.ic_clr_rd_req().read();
                        p.ic_data_cmd().write(|w| w.set_dat(*byte));
                    }

                    Poll::Pending
                } else if stat.rx_done() {
                    p.ic_clr_rx_done().read();
                    Poll::Ready(Ok(ReadStatus::Done))
                } else if stat.rd_req() && stat.tx_empty() {
                    Poll::Ready(Ok(ReadStatus::NeedMoreBytes))
                } else {
                    Poll::Pending
                }
            },
            |_me| {
                p.ic_intr_mask().write(|w| {
                    w.set_m_rx_done(true);
                    w.set_m_tx_empty(true);
                    w.set_m_tx_abrt(true);
                })
            },
        )
        .await
    }

    /// Sends repeated `fill` bytes until the controller stops, returning `Ok(())`.
    /// Returns non-leftover hardware aborts; cancellation can stall the bus as for `respond_to_read`.
    pub async fn respond_till_stop(
        &mut self,
        fill: u8,
    ) -> Result<(), Error> {
        // Send fill bytes a full fifo at a time, to reduce interrupt noise.
        // This does mean we'll almost certainly abort the write, but since these are fill bytes,
        // we don't care.
        let buff = [fill; FIFO_SIZE as usize];
        loop {
            match self.respond_to_read(&buff).await {
                Ok(ReadStatus::NeedMoreBytes) => (),
                Ok(ReadStatus::LeftoverBytes(_)) => break Ok(()),
                Ok(_) => break Ok(()),
                Err(e) => break Err(e),
            }
        }
    }

    /// Sends `buffer`, then repeats `fill` if the controller requests more bytes.
    /// Returns the original completion/leftover status, or `Done` after filler stops.
    /// Empty input and hardware aborts propagate; cancellation may require `reset`.
    pub async fn respond_and_fill(
        &mut self,
        buffer: &[u8],
        fill: u8,
    ) -> Result<ReadStatus, Error> {
        let resp_stat = self.respond_to_read(buffer).await?;

        if resp_stat == ReadStatus::NeedMoreBytes {
            self.respond_till_stop(fill).await?;
            Ok(ReadStatus::Done)
        } else {
            Ok(resp_stat)
        }
    }

    /// Returns `Ok(())` without an abort, otherwise clears the latched cause and returns it.
    /// Address NACK takes precedence over arbitration loss, then TX-flush count, then raw bits.
    #[inline(always)]
    fn read_and_clear_abort_reason(&mut self) -> Result<(), Error> {
        let p = T::regs();
        let abort_reason = p.ic_tx_abrt_source().read();

        if abort_reason.0 != 0 {
            // Note clearing the abort flag also clears the reason, and this
            // instance of flag is clear-on-read! Note also the
            // IC_CLR_TX_ABRT register always reads as 0.
            p.ic_clr_tx_abrt().read();

            let reason = if abort_reason.abrt_7b_addr_noack()
                | abort_reason.abrt_10addr1_noack()
                | abort_reason.abrt_10addr2_noack()
            {
                AbortReason::NoAcknowledge
            } else if abort_reason.arb_lost() {
                AbortReason::ArbitrationLoss
            } else if abort_reason.tx_flush_cnt() > 0 {
                AbortReason::TxNotEmpty(abort_reason.tx_flush_cnt())
            } else {
                AbortReason::Other(abort_reason.0)
            };

            Err(Error::Abort(reason))
        } else {
            Ok(())
        }
    }
}
