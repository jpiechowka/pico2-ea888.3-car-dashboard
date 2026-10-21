//! CYW43 SPI command framing, power sequencing and cached backplane/status access.
//! Low-level transport owns CS and cancellation cleanup; this adapter exposes no recoverable bus error.
use aligned::{A4, Aligned};
use embassy_futures::yield_now;
use embassy_time::Timer;
use embedded_hal_1::digital::OutputPin;
use futures::FutureExt;

use crate::consts::*;
use crate::runner::{BusType, SealedBus};
use crate::util::{slice8_mut, slice32_mut, slice32_ref};

/// Custom Spi Trait that _only_ supports the bus operation of the cyw43
/// Implementors are expected to hold the CS pin low during an operation.
pub trait SpiBusCyw43 {
    /// Issues a write command on the bus
    /// `write` begins with a command word followed by payload words; returns the trailing device status.
    /// The implementation must preserve buffer ownership until hardware access ends, including cancellation.
    async fn cmd_write(&mut self, write: &[u32]) -> u32;

    /// Issues a read command on the bus
    /// `write` is a command word; returns trailing device status after filling `read`.
    /// The implementation must preserve buffer ownership until hardware access ends, including cancellation.
    /// `read` will contain the response of the device
    /// Backplane reads have a response delay that produces one extra unspecified word at the beginning of `read`.
    /// Callers that want to read `n` word from the backplane, have to provide a slice that is `n+1` words long.
    async fn cmd_read(&mut self, write: u32, read: &mut [u32]) -> u32;

    /// Wait for events from the Device. A typical implementation would wait for the IRQ pin to be high.
    /// The default yields once, then returns without inspecting hardware, resulting in active polling.
    async fn wait_for_event(&mut self) {
        yield_now().await;
    }
}

/// SPI runner adapter owning radio power, transport and cached register state.
pub struct SpiBus<PWR, SPI> {
    /// Last fully programmed window base; 0xAAAAAAAA forces first-time programming.
    backplane_window: u32,
    /// Active-high radio regulator-enable output, exclusively owned.
    pwr: PWR,
    /// Transport handling command clocks, response delay and chip select.
    spi: SPI,
    /// Latest trailing status; zero means no cached nonzero status for the next status read.
    status: u32,
}

impl<PWR, SPI> SpiBus<PWR, SPI>
where
    PWR: OutputPin,
    SPI: SpiBusCyw43,
{
    /// Returns an uninitialized adapter owning `pwr` and `spi`, with invalid window and empty status caches.
    pub(crate) fn new(pwr: PWR, spi: SPI) -> Self {
        Self {
            backplane_window: 0xAAAA_AAAA,
            pwr,
            spi,
            status: 0,
        }
    }

    /// Selects the window for `addr` and returns its `len`-byte register value (internal callers use 1,2,4).
    /// Four-byte access sets the hardware word flag; keeps that window selected.
    async fn backplane_readn(&mut self, addr: u32, len: u32) -> u32 {
        trace!("backplane_readn addr = {:08x} len = {}", addr, len);

        self.backplane_set_window(addr).await;

        let mut bus_addr = addr & BACKPLANE_ADDRESS_MASK;
        if len == 4 {
            bus_addr |= BACKPLANE_ADDRESS_32BIT_FLAG;
        }

        let val = self.readn(FUNC_BACKPLANE, bus_addr, len).await;

        trace!("backplane_readn addr = {:08x} len = {} val = {:08x}", addr, len, val);

        return val;
    }

    /// Writes low `len` bytes of `val` at backplane `addr` (internal len is 1,2,4), keeping its window selected.
    async fn backplane_writen(&mut self, addr: u32, val: u32, len: u32) {
        trace!("backplane_writen addr = {:08x} len = {} val = {:08x}", addr, len, val);

        self.backplane_set_window(addr).await;

        let mut bus_addr = addr & BACKPLANE_ADDRESS_MASK;
        if len == 4 {
            bus_addr |= BACKPLANE_ADDRESS_32BIT_FLAG;
        }
        self.writen(FUNC_BACKPLANE, bus_addr, val, len).await;
    }

    /// Programs changed window bytes for `addr`, updating the cache only after all writes finish.
    /// Cancellation can leave hardware partially changed while the cache still names the old window.
    async fn backplane_set_window(&mut self, addr: u32) {
        let new_window = addr & !BACKPLANE_ADDRESS_MASK;

        if (new_window >> 24) as u8 != (self.backplane_window >> 24) as u8 {
            self.write8(
                FUNC_BACKPLANE,
                REG_BACKPLANE_BACKPLANE_ADDRESS_HIGH,
                (new_window >> 24) as u8,
            )
            .await;
        }
        if (new_window >> 16) as u8 != (self.backplane_window >> 16) as u8 {
            self.write8(
                FUNC_BACKPLANE,
                REG_BACKPLANE_BACKPLANE_ADDRESS_MID,
                (new_window >> 16) as u8,
            )
            .await;
        }
        if (new_window >> 8) as u8 != (self.backplane_window >> 8) as u8 {
            self.write8(
                FUNC_BACKPLANE,
                REG_BACKPLANE_BACKPLANE_ADDRESS_LOW,
                (new_window >> 8) as u8,
            )
            .await;
        }
        self.backplane_window = new_window;
    }

    /// Returns a `len`-byte value from `func`/`addr`, skipping the extra backplane response word.
    /// Internal len is 1,2,4; updates trailing-status cache without checking error flags.
    async fn readn(&mut self, func: u32, addr: u32, len: u32) -> u32 {
        let cmd = cmd_word(READ, INC_ADDR, func, addr, len);
        let mut buf = [0; 2];
        // if we are reading from the backplane, we need an extra word for the response delay
        let len = if func == FUNC_BACKPLANE { 2 } else { 1 };

        self.status = self.spi.cmd_read(cmd, &mut buf[..len]).await;

        // if we read from the backplane, the result is in the second word, after the response delay
        if func == FUNC_BACKPLANE { buf[1] } else { buf[0] }
    }

    /// Sends low `len` bytes of `val` to `func`/`addr` (internal len 1,2,4), caching trailing status.
    async fn writen(&mut self, func: u32, addr: u32, val: u32, len: u32) {
        let cmd = cmd_word(WRITE, INC_ADDR, func, addr, len);

        self.status = self.spi.cmd_write(&[cmd, val]).await;
    }

    /// Returns a halfword-swapped 32-bit value from `func`/`addr` for pre-configuration bus testing.
    /// Does not support backplane response delay; caches returned trailing status.
    async fn read32_swapped(&mut self, func: u32, addr: u32) -> u32 {
        let cmd = cmd_word(READ, INC_ADDR, func, addr, 4);
        let cmd = swap16(cmd);
        let mut buf = [0; 1];

        self.status = self.spi.cmd_read(cmd, &mut buf).await;

        swap16(buf[0])
    }

    /// Writes halfword-swapped command and `val` to `func`/`addr` during initial bus-mode setup.
    async fn write32_swapped(&mut self, func: u32, addr: u32, val: u32) {
        let cmd = cmd_word(WRITE, INC_ADDR, func, addr, 4);
        let buf = [swap16(cmd), swap16(val)];

        self.status = self.spi.cmd_write(&buf).await;
    }
}

impl<PWR, SPI> SealedBus for SpiBus<PWR, SPI>
where
    PWR: OutputPin,
    SPI: SpiBusCyw43,
{
    /// Selects SPI status/interrupt paths in the runner.
    const TYPE: BusType = BusType::Spi;

    /// Power-cycles the radio (20 ms low, 250 ms high), tests bus words and configures framing/IRQs.
    /// `bluetooth_enabled` adds the F1 interrupt. Pin failures/test mismatches panic; initial FEEDBEAD polling
    /// has no timeout. Cancellation leaves partially initialized power/bus state.
    async fn init(&mut self, bluetooth_enabled: bool) {
        // Reset
        trace!("WL_REG off/on");
        self.pwr.set_low().unwrap();
        Timer::after_millis(20).await;
        self.pwr.set_high().unwrap();
        Timer::after_millis(250).await;

        trace!("read REG_BUS_TEST_RO");
        while self
            .read32_swapped(FUNC_BUS, REG_BUS_TEST_RO)
            .inspect(|v| trace!("{:#x}", v))
            .await
            != FEEDBEAD
        {}

        trace!("write REG_BUS_TEST_RW");
        self.write32_swapped(FUNC_BUS, REG_BUS_TEST_RW, TEST_PATTERN).await;
        let val = self.read32_swapped(FUNC_BUS, REG_BUS_TEST_RW).await;
        trace!("{:#x}", val);
        assert_eq!(val, TEST_PATTERN);

        trace!("read REG_BUS_CTRL");
        let val = self.read32_swapped(FUNC_BUS, REG_BUS_CTRL).await;
        trace!("{:#010b}", (val & 0xff));

        // 32-bit word length, little endian (which is the default endianess).
        // TODO: C library is uint32_t val = WORD_LENGTH_32 | HIGH_SPEED_MODE| ENDIAN_BIG | INTERRUPT_POLARITY_HIGH | WAKE_UP | 0x4 << (8 * SPI_RESPONSE_DELAY) | INTR_WITH_STATUS << (8 * SPI_STATUS_ENABLE);
        trace!("write REG_BUS_CTRL");
        self.write32_swapped(
            FUNC_BUS,
            REG_BUS_CTRL,
            WORD_LENGTH_32
                | HIGH_SPEED
                | INTERRUPT_POLARITY_HIGH
                | WAKE_UP
                | 0x4 << (8 * REG_BUS_RESPONSE_DELAY)
                | STATUS_ENABLE << (8 * REG_BUS_STATUS_ENABLE)
                | INTR_WITH_STATUS << (8 * REG_BUS_STATUS_ENABLE),
        )
        .await;

        trace!("read REG_BUS_CTRL");
        let val = self.read8(FUNC_BUS, REG_BUS_CTRL).await;
        trace!("{:#b}", val);

        // TODO: C doesn't do this? i doubt it messes anything up
        trace!("read REG_BUS_TEST_RO");
        let val = self.read32(FUNC_BUS, REG_BUS_TEST_RO).await;
        trace!("{:#x}", val);
        assert_eq!(val, FEEDBEAD);

        // TODO: C doesn't do this? i doubt it messes anything up
        trace!("read REG_BUS_TEST_RW");
        let val = self.read32(FUNC_BUS, REG_BUS_TEST_RW).await;
        trace!("{:#x}", val);
        assert_eq!(val, TEST_PATTERN);

        trace!("write SPI_RESP_DELAY_F1 CYW43_BACKPLANE_READ_PAD_LEN_BYTES");
        self.write8(FUNC_BUS, SPI_RESP_DELAY_F1, WHD_BUS_SPI_BACKPLANE_READ_PADD_SIZE)
            .await;

        // TODO: Make sure error interrupt bits are clear?
        // cyw43_write_reg_u8(self, BUS_FUNCTION, SPI_INTERRUPT_REGISTER, DATA_UNAVAILABLE | COMMAND_ERROR | DATA_ERROR | F1_OVERFLOW) != 0)
        trace!("Make sure error interrupt bits are clear");
        self.write8(
            FUNC_BUS,
            REG_BUS_INTERRUPT,
            (IRQ_DATA_UNAVAILABLE | IRQ_COMMAND_ERROR | IRQ_DATA_ERROR | IRQ_F1_OVERFLOW) as u8,
        )
        .await;

        // Enable a selection of interrupts
        // TODO: why not all of these F2_F3_FIFO_RD_UNDERFLOW | F2_F3_FIFO_WR_OVERFLOW | COMMAND_ERROR | DATA_ERROR | F2_PACKET_AVAILABLE | F1_OVERFLOW | F1_INTR
        trace!("enable a selection of interrupts");
        let mut val = IRQ_F2_F3_FIFO_RD_UNDERFLOW
            | IRQ_F2_F3_FIFO_WR_OVERFLOW
            | IRQ_COMMAND_ERROR
            | IRQ_DATA_ERROR
            | IRQ_F2_PACKET_AVAILABLE
            | IRQ_F1_OVERFLOW;
        if bluetooth_enabled {
            val = val | IRQ_F1_INTR;
        }
        self.write16(FUNC_BUS, REG_BUS_INTERRUPT_ENABLE, val).await;
    }

    /// Fills logical WLAN `buf` bytes and caches status; internal caller supplies initialized whole-word backing.
    /// Transport clocks rounded-up words but the command carries exact byte length.
    async fn wlan_read(&mut self, buf: &mut Aligned<A4, [u8]>) {
        let len_in_u8 = buf.len() as u32;
        let buf = slice32_mut(buf);

        let cmd = cmd_word(READ, INC_ADDR, FUNC_WLAN, 0, len_in_u8);
        let len_in_u32 = (len_in_u8 as usize + 3) / 4;

        self.status = self.spi.cmd_read(cmd, &mut buf[..len_in_u32]).await;
    }

    /// Sends WLAN `buf` rounded to whole words, copying into a 513-word command buffer; caches status.
    /// Internal callers supply initialized trailing bytes and at most 2048 payload bytes; larger input panics.
    async fn wlan_write(&mut self, buf: &Aligned<A4, [u8]>) {
        let buf = slice32_ref(buf);
        let cmd = cmd_word(WRITE, INC_ADDR, FUNC_WLAN, 0, buf.len() as u32 * 4);
        //TODO try to remove copy?
        let mut cmd_buf = [0_u32; 513];
        cmd_buf[0] = cmd;
        cmd_buf[1..][..buf.len()].copy_from_slice(buf);

        self.status = self.spi.cmd_write(&cmd_buf[..buf.len() + 1]).await;
    }

    /// Copies backplane `addr` into `data` in at most 64-byte window-safe chunks, skipping response delay.
    /// Panics for unaligned addr; cancellation preserves a completed prefix. No status-error propagation.
    #[allow(unused)]
    async fn bp_read(&mut self, mut addr: u32, mut data: &mut [u8]) {
        trace!("bp_read addr = {:08x}", addr);

        // It seems the HW force-aligns the addr
        // to 2 if data.len() >= 2
        // to 4 if data.len() >= 4
        // To simplify, enforce 4-align for now.
        assert!(addr % 4 == 0);

        // Backplane read buffer has one extra word for the response delay.
        let mut buf = [0u32; BACKPLANE_MAX_TRANSFER_SIZE / 4 + 1];

        while !data.is_empty() {
            // Ensure transfer doesn't cross a window boundary.
            let window_offs = addr & BACKPLANE_ADDRESS_MASK;
            let window_remaining = BACKPLANE_WINDOW_SIZE - window_offs as usize;

            let len = data.len().min(BACKPLANE_MAX_TRANSFER_SIZE).min(window_remaining);

            self.backplane_set_window(addr).await;

            let cmd = cmd_word(READ, INC_ADDR, FUNC_BACKPLANE, window_offs, len as u32);

            // round `buf` to word boundary, add one extra word for the response delay
            self.status = self.spi.cmd_read(cmd, &mut buf[..(len + 3) / 4 + 1]).await;

            // when writing out the data, we skip the response-delay byte
            data[..len].copy_from_slice(&slice8_mut(&mut buf[1..])[..len]);

            // Advance ptr.
            addr += len as u32;
            data = &mut data[len..];
        }
    }

    /// Sends logical `data` bytes at four-byte-aligned backplane `addr`, in at most 64-byte window-safe chunks.
    /// Scratch storage supplies rounded transport words; command length excludes padding. Cancellation may deliver a prefix.
    async fn bp_write(&mut self, mut addr: u32, mut data: &[u8]) {
        trace!("bp_write addr = {:08x}", addr);

        // It seems the HW force-aligns the addr
        // to 2 if data.len() >= 2
        // to 4 if data.len() >= 4
        // To simplify, enforce 4-align for now.
        assert!(addr % 4 == 0);

        let mut buf = [0u32; BACKPLANE_MAX_TRANSFER_SIZE / 4 + 1];

        while !data.is_empty() {
            // Ensure transfer doesn't cross a window boundary.
            let window_offs = addr & BACKPLANE_ADDRESS_MASK;
            let window_remaining = BACKPLANE_WINDOW_SIZE - window_offs as usize;

            let len = data.len().min(BACKPLANE_MAX_TRANSFER_SIZE).min(window_remaining);
            slice8_mut(&mut buf[1..])[..len].copy_from_slice(&data[..len]);

            self.backplane_set_window(addr).await;

            let cmd = cmd_word(WRITE, INC_ADDR, FUNC_BACKPLANE, window_offs, len as u32);
            buf[0] = cmd;

            self.status = self.spi.cmd_write(&buf[..(len + 3) / 4 + 1]).await;

            // Advance ptr.
            addr += len as u32;
            data = &data[len..];
        }
    }

    /// Returns the low 8 bits from backplane register `addr`, retaining its selected window.
    async fn bp_read8(&mut self, addr: u32) -> u8 {
        self.backplane_readn(addr, 1).await as u8
    }

    /// Writes 8-bit `val` at backplane `addr`, retaining its selected window and caching status.
    async fn bp_write8(&mut self, addr: u32, val: u8) {
        self.backplane_writen(addr, val as u32, 1).await
    }

    /// Returns the low 16 bits from backplane register `addr`, retaining its selected window.
    async fn bp_read16(&mut self, addr: u32) -> u16 {
        self.backplane_readn(addr, 2).await as u16
    }

    #[allow(unused)]
    /// Writes 16-bit `val` at backplane `addr`, retaining its selected window and caching status.
    async fn bp_write16(&mut self, addr: u32, val: u16) {
        self.backplane_writen(addr, val as u32, 2).await
    }

    /// Copies backplane `addr` into `data` in at most 64-byte window-safe chunks, skipping response delay.
    /// Panics for unaligned addr; cancellation preserves a completed prefix. No status-error propagation.
    #[allow(unused)]
    /// Returns the low 32 bits from backplane register `addr`, retaining its selected window.
    async fn bp_read32(&mut self, addr: u32) -> u32 {
        self.backplane_readn(addr, 4).await
    }

    /// Writes 32-bit `val` at backplane `addr`, retaining its selected window and caching status.
    async fn bp_write32(&mut self, addr: u32, val: u32) {
        self.backplane_writen(addr, val, 4).await
    }

    /// Returns 8-bit data at `func`/`addr`, caching trailing status.
    async fn read8(&mut self, func: u32, addr: u32) -> u8 {
        self.readn(func, addr, 1).await as u8
    }

    /// Writes 8-bit `val` at `func`/`addr` and caches trailing device status.
    async fn write8(&mut self, func: u32, addr: u32, val: u8) {
        self.writen(func, addr, val as u32, 1).await
    }

    /// Returns 16-bit data at `func`/`addr`, caching trailing status.
    async fn read16(&mut self, func: u32, addr: u32) -> u16 {
        self.readn(func, addr, 2).await as u16
    }

    #[allow(unused)]
    /// Writes 16-bit `val` at `func`/`addr` and caches trailing device status.
    async fn write16(&mut self, func: u32, addr: u32, val: u16) {
        self.writen(func, addr, val as u32, 2).await
    }

    /// Returns 32-bit data at `func`/`addr`; a status-register read consumes cached nonzero status first.
    /// A zero cache causes a fresh bus read, so zero is both a valid status and cache-empty sentinel.
    async fn read32(&mut self, func: u32, addr: u32) -> u32 {
        if func == FUNC_BUS && addr == SPI_STATUS_REGISTER && self.status != 0 {
            let status = self.status;
            self.status = 0;

            status
        } else {
            self.readn(func, addr, 4).await
        }
    }

    #[allow(unused)]
    /// Writes 32-bit `val` at `func`/`addr` and caches trailing device status.
    async fn write32(&mut self, func: u32, addr: u32, val: u32) {
        self.writen(func, addr, val, 4).await
    }

    /// Delegates transport IRQ waiting, or its yield-based polling fallback.
    async fn wait_for_event(&mut self) {
        self.spi.wait_for_event().await;
    }
}

/// Returns `x` with its low/high 16-bit halves exchanged; byte order within halves is unchanged.
fn swap16(x: u32) -> u32 {
    x.rotate_left(16)
}

/// Returns SPI framing for `write`, increment flag `incr`, two-bit `func`, 17-bit `addr` and 11-bit byte `len`.
/// Truncates out-of-field bits rather than validating them; internal callers bound their addresses/counts.
fn cmd_word(write: bool, incr: bool, func: u32, addr: u32, len: u32) -> u32 {
    (write as u32) << 31 | (incr as u32) << 30 | (func & 0b11) << 28 | (addr & 0x1FFFF) << 11 | (len & 0x7FF)
}
