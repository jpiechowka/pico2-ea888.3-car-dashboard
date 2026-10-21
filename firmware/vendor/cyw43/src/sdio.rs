//! SDIO CMD52/CMD53 transport with 64-byte blocks and cached backplane windows.
//! Transport failures are currently logged/swallowed (CMD52 reads use 0xff); no transactional rollback.
//! Cancellation can leave partially changed device/window state. The selected Pico 2 W route uses SPI.
//! Backplane byte transfers retain a 1024-byte aligned scratch array in their async future.
use core::slice;

use aligned::{A4, Aligned};
use embassy_time::{Duration, Timer};

use crate::consts::*;
use crate::runner::{BusType, SealedBus};
use crate::util::{aligned_mut, aligned_ref, try_until};

// macro_rules! ALIGN_UINT {
//     ($val:expr, $align:expr) => {
//         ((($val) + ($align) - 1) & !(($align) - 1))
//     };
// }
//
// macro_rules! WRITE_BYTES_PAD {
//     ($len:expr) => {
//         ALIGN_UINT!($len, 64)
//     };
// }

#[derive(Clone, Copy, PartialEq, Debug)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[cfg_attr(feature = "log", derive(derive_more::Display))]
/// CMD53 count interpretation.
enum Mode {
    /// Count represents 64-byte blocks.
    Block,
    /// Count represents bytes.
    Byte,
}

#[derive(Clone, Copy, PartialEq, Debug)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[cfg_attr(feature = "log", derive(derive_more::Display))]
/// Width of an individual backplane register access.
enum Word {
    /// One-byte CMD52 access.
    U8,
    /// Two-byte little-endian CMD53 access.
    U16,
    /// Four-byte access with the backplane 32-bit flag.
    U32,
}

/// Transfer block length in bytes, matching configured function block-size registers.
const BLOCK_SIZE: usize = BACKPLANE_MAX_TRANSFER_SIZE;

/// Returns incrementing CMD53 argument for `write`, `func`, masked 17-bit `addr`, `mode` and byte `len`.
/// Callers bound func to 0..=7 and count to nine bits; Block requires a whole number of 64-byte blocks.
/// Fields other than address are not masked or validated here.
fn cmd53_arg(write: bool, func: u32, addr: u32, mode: Mode, len: usize) -> u32 {
    let (len, block_mode) = match mode {
        Mode::Block => (len / BLOCK_SIZE, 1u32),
        Mode::Byte => (len, 0u32),
    };

    let op_code = 1;

    (write as u32) << 31 | func << 28 | block_mode << 27 | op_code << 26 | (addr & 0x1ffff) << 9 | len as u32
}

/// SDIO command operations required by the CYW43 transport, using SIZE-byte aligned blocks.
/// Implementors own bus framing, response checks and cancellation cleanup; logical slice lengths exclude padding.
pub trait SdioBusCyw43<const SIZE: usize> {
    /// Transport/controller error; the higher bus adapter currently logs or substitutes failures.
    type Error: core::fmt::Debug;

    /// Configures four-bit bus clock to requested `frequency` Hz; returns Ok or the transport configuration error.
    fn set_bus_to_high_speed(&mut self, frequency: u32) -> Result<(), Self::Error>;

    /// Issues raw command `arg`; returns its response word or transport error.
    async fn cmd52(&mut self, arg: u32) -> Result<u16, Self::Error>;

    /// Reads raw command `arg` into complete `blocks`; returns Ok or a transport error with possible partial writes.
    async fn cmd53_block_read(&mut self, arg: u32, blocks: &mut [Aligned<A4, [u8; SIZE]>]) -> Result<(), Self::Error>;

    /// Reads raw command `arg` into logical `buffer` bytes; returns Ok or an error with possible partial output.
    async fn cmd53_byte_read(&mut self, arg: u32, buffer: &mut Aligned<A4, [u8]>) -> Result<(), Self::Error>;

    /// Sends complete `blocks` with raw command `arg`; returns Ok or a transport error after possible partial delivery.
    async fn cmd53_block_write(&mut self, arg: u32, blocks: &[Aligned<A4, [u8; SIZE]>]) -> Result<(), Self::Error>;

    /// Sends logical `buffer` bytes with raw command `arg`; returns Ok or an error after possible partial delivery.
    async fn cmd53_byte_write(&mut self, arg: u32, buffer: &Aligned<A4, [u8]>) -> Result<(), Self::Error>;

    /// Waits for a transport-defined wake event; default delays 800 ms and does not inspect an IRQ pin.
    async fn wait_for_event(&mut self) {
        Timer::after_millis(800).await;
    }
}

impl<const SIZE: usize, T: SdioBusCyw43<SIZE>> SdioBusCyw43<SIZE> for &mut T {
    /// Preserves the underlying SDIO implementation error type.
    type Error = T::Error;

    /// Passes requested `frequency` Hz through; returns the underlying configuration result.
    fn set_bus_to_high_speed(&mut self, frequency: u32) -> Result<(), Self::Error> {
        T::set_bus_to_high_speed(self, frequency)
    }

    /// Forwards raw `arg`; returns the underlying response/error unchanged.
    async fn cmd52(&mut self, arg: u32) -> Result<u16, Self::Error> {
        T::cmd52(self, arg).await
    }

    /// Forwards `arg` and mutable `blocks`; returns the underlying partial-effect read result.
    async fn cmd53_block_read(&mut self, arg: u32, blocks: &mut [Aligned<A4, [u8; SIZE]>]) -> Result<(), Self::Error> {
        T::cmd53_block_read(self, arg, blocks).await
    }

    /// Forwards `arg` and logical `buffer`; returns the underlying partial-effect read result.
    async fn cmd53_byte_read(&mut self, arg: u32, buffer: &mut Aligned<A4, [u8]>) -> Result<(), Self::Error> {
        T::cmd53_byte_read(self, arg, buffer).await
    }

    /// Forwards `arg` and `blocks`; returns the underlying transmission result.
    async fn cmd53_block_write(&mut self, arg: u32, blocks: &[Aligned<A4, [u8; SIZE]>]) -> Result<(), Self::Error> {
        T::cmd53_block_write(self, arg, blocks).await
    }

    /// Forwards `arg` and logical `buffer`; returns the underlying transmission result.
    async fn cmd53_byte_write(&mut self, arg: u32, buffer: &Aligned<A4, [u8]>) -> Result<(), Self::Error> {
        T::cmd53_byte_write(self, arg, buffer).await
    }

    /// Delegates the underlying transport event wait and cancellation behavior.
    async fn wait_for_event(&mut self) {
        T::wait_for_event(self).await
    }
}

/// Runner adapter owning an SDIO transport and its cached backplane address window.
pub struct SdioBus<SDIO> {
    /// Last fully requested window base; 0xAAAAAAAA forces initial register programming.
    backplane_window: u32,
    /// Exclusively owned low-level command transport.
    sdio: SDIO,
}

impl<SDIO> SdioBus<SDIO>
where
    SDIO: SdioBusCyw43<BLOCK_SIZE>,
{
    /// Returns an uninitialized adapter owning `sdio`, with an invalidated window cache.
    pub(crate) fn new(sdio: SDIO) -> Self {
        Self {
            backplane_window: 0xAAAA_AAAA,
            sdio,
        }
    }

    /// Selects the window containing `addr`, returns a `word`-width register value and restores the chip-common window.
    /// Narrow transport failures become 0xff or partially initialized zero-buffer data.
    async fn backplane_readn(&mut self, addr: u32, word: Word) -> u32 {
        trace!("backplane_readn addr = {:08x} len = {}", addr, word);

        self.backplane_set_window(addr).await;

        let mut bus_addr = addr & BACKPLANE_ADDRESS_MASK;
        if word == Word::U32 {
            bus_addr |= BACKPLANE_ADDRESS_32BIT_FLAG;
        }

        let val = match word {
            Word::U8 => self.read8(FUNC_BACKPLANE, bus_addr).await as u32,
            Word::U16 => self.read16(FUNC_BACKPLANE, bus_addr).await as u32,
            Word::U32 => self.read32(FUNC_BACKPLANE, bus_addr).await,
        };

        trace!("backplane_readn addr = {:08x} len = {} val = {:08x}", addr, word, val);

        self.backplane_set_window(CHIPCOMMON_BASE_ADDRESS).await;

        return val;
    }

    /// Writes `val` at backplane `addr` with `word` width, then restores the chip-common window.
    /// Panics when val does not fit selected width; underlying transport errors are swallowed.
    async fn backplane_writen(&mut self, addr: u32, val: u32, word: Word) {
        trace!("backplane_writen addr = {:08x} len = {} val = {:08x}", addr, word, val);

        self.backplane_set_window(addr).await;

        let mut bus_addr = addr & BACKPLANE_ADDRESS_MASK;
        if word == Word::U32 {
            bus_addr |= BACKPLANE_ADDRESS_32BIT_FLAG;
        }

        let _ = match word {
            Word::U8 => self.write8(FUNC_BACKPLANE, bus_addr, val.try_into().unwrap()).await,
            Word::U16 => self.write16(FUNC_BACKPLANE, bus_addr, val.try_into().unwrap()).await,
            Word::U32 => self.write32(FUNC_BACKPLANE, bus_addr, val).await,
        };

        self.backplane_set_window(CHIPCOMMON_BASE_ADDRESS).await;
    }

    /// Programs changed address bytes for the window containing `addr`, then updates the cache.
    /// Swallowed transport failures or cancellation can leave cache/device state inconsistent; no rollback.
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

    /// Issues byte `val` for read/write `write` at `func` and masked 17-bit `addr`.
    /// Returns the low response byte, or 0xff on transport failure; response error bits are not validated.
    async fn cmd52(&mut self, write: bool, func: u32, addr: u32, val: u8) -> u8 {
        let arg: u32 = (write as u32) << 31 | func << 28 | (addr & 0x1ffff) << 9 | (val as u32 & 0xff);

        let result = self.sdio.cmd52(arg).await.unwrap_or(u16::MAX) as u8;

        // trace!("cmd52: 0x{:08x} 0x{:08x}", arg, result);

        result
    }

    /// Sends logical `buf` bytes to incrementing `func`/`addr`, whole blocks followed by byte tail.
    /// Empty input does nothing; errors are logged and later phases still run. Internal callers bound count/address arithmetic.
    async fn cmd53_write(&mut self, func: u32, mut addr: u32, buf: &Aligned<A4, [u8]>) {
        let byte_part = buf.len() % BLOCK_SIZE;
        let block_part = buf.len() - byte_part;

        if block_part > 0 {
            let buf = &buf[..block_part];

            if self
                .sdio
                .cmd53_block_write(cmd53_arg(true, func, addr, Mode::Block, buf.len()), unsafe {
                    slice::from_raw_parts(buf.as_ptr() as *mut _, size_of_val(buf) / BLOCK_SIZE)
                })
                .await
                .is_err()
            {
                debug!("cmd53 block read failed");
            }

            addr += block_part as u32;
        }

        if byte_part > 0 {
            let buf = &buf[block_part..];

            if self
                .sdio
                .cmd53_byte_write(cmd53_arg(true, func, addr, Mode::Byte, buf.len()), buf)
                .await
                .is_err()
            {
                debug!("cmd53 byte read failed (size: {})", size_of_val(buf));
            }
        }
    }

    /// Reads logical `buf` bytes from incrementing `func`/`addr`, whole blocks then byte tail.
    /// Empty input does nothing; errors are logged and can leave partial output before the next phase.
    async fn cmd53_read(&mut self, func: u32, mut addr: u32, buf: &mut Aligned<A4, [u8]>) {
        let byte_part = buf.len() % BLOCK_SIZE;
        let block_part = buf.len() - byte_part;

        if block_part > 0 {
            let buf = &mut buf[..block_part];

            if self
                .sdio
                .cmd53_block_read(cmd53_arg(false, func, addr, Mode::Block, buf.len()), unsafe {
                    slice::from_raw_parts_mut(buf.as_mut_ptr() as *mut _, size_of_val(buf) / BLOCK_SIZE)
                })
                .await
                .is_err()
            {
                debug!("cmd53 block read failed");
            }

            addr += block_part as u32;
        }

        if byte_part > 0 {
            let buf = &mut buf[block_part..];

            if self
                .sdio
                .cmd53_byte_read(cmd53_arg(false, func, addr, Mode::Byte, buf.len()), buf)
                .await
                .is_err()
            {
                debug!("cmd53 byte read failed (size: {})", size_of_val(buf));
            }
        }
    }
}

impl<SDIO> SealedBus for SdioBus<SDIO>
where
    SDIO: SdioBusCyw43<64>,
{
    /// Selects the runner SDIO mailbox/status initialization path.
    const TYPE: BusType = BusType::Sdio;

    /// Initializes four-bit SDIO, 64-byte blocks, interrupts and 25/50 MHz bus; `_bluetooth_enabled` is ignored.
    /// Returns early after exhausted 500-attempt polls without a status error; clock-setting failures panic.
    /// Each attempt may itself stall indefinitely. Cancellation leaves partial device setup.
    async fn init(&mut self, _bluetooth_enabled: bool) {
        // whd_bus_sdio_init

        // set up backplane
        if !try_until(
            async || {
                self.write8(FUNC_BUS, SDIOD_CCCR_IOEN, SDIO_FUNC_ENABLE_1 as u8).await;

                self.read8(FUNC_BUS, SDIOD_CCCR_IOEN).await as u32 == SDIO_FUNC_ENABLE_1
            },
            Duration::from_millis(500),
        )
        .await
        {
            debug!("timeout while setting up the backplane");
            return;
        }

        debug!("backplane is up");

        // Read the bus width and set to 4 bits (1-bit bus is not currently supported)
        let reg = self.read8(FUNC_BUS, SDIOD_CCCR_BICTRL).await as u32;

        self.write8(
            FUNC_BUS,
            SDIOD_CCCR_BICTRL,
            ((reg & !BUS_SD_DATA_WIDTH_MASK) | BUS_SD_DATA_WIDTH_4BIT) as u8,
        )
        .await;

        // Set the block size
        if !try_until(
            async || {
                self.write8(FUNC_BUS, SDIOD_CCCR_BLKSIZE_0, SDIO_64B_BLOCK as u8).await;

                self.read8(FUNC_BUS, SDIOD_CCCR_BLKSIZE_0).await as u32 == SDIO_64B_BLOCK
            },
            Duration::from_millis(500),
        )
        .await
        {
            debug!("timeout while setting block size");
            return;
        }

        self.write8(FUNC_BUS, SDIOD_CCCR_BLKSIZE_0, SDIO_64B_BLOCK as u8).await;
        self.write8(FUNC_BUS, SDIOD_CCCR_F1BLKSIZE_0, SDIO_64B_BLOCK as u8)
            .await;
        self.write8(FUNC_BUS, SDIOD_CCCR_F2BLKSIZE_0, SDIO_64B_BLOCK as u8)
            .await;
        self.write8(FUNC_BUS, SDIOD_CCCR_F2BLKSIZE_1, 0).await;

        // Enable/Disable Client interrupts
        self.write8(
            FUNC_BUS,
            SDIOD_CCCR_INTEN,
            (INTR_CTL_MASTER_EN | INTR_CTL_FUNC1_EN | INTR_CTL_FUNC2_EN) as u8,
        )
        .await;

        self.sdio.set_bus_to_high_speed(25_000_000).unwrap();

        // enable more than 25MHz bus
        let reg = self.read8(FUNC_BUS, SDIOD_CCCR_SPEED_CONTROL).await as u32;
        if reg & 1 != 0 {
            self.write8(FUNC_BUS, SDIOD_CCCR_SPEED_CONTROL, (reg | SDIO_SPEED_EHS) as u8)
                .await;

            self.sdio.set_bus_to_high_speed(50_000_000).unwrap();
        }

        // Wait till the backplane is ready
        if !try_until(
            async || self.read8(FUNC_BUS, SDIOD_CCCR_IORDY).await as u32 & SDIO_FUNC_READY_1 != 0,
            Duration::from_millis(500),
        )
        .await
        {
            debug!("timeout while waiting for backplane to be ready");
            return;
        }
    }

    /// Reads logical `buf` bytes from WLAN function address zero; errors may leave partial output.
    async fn wlan_read(&mut self, buf: &mut Aligned<A4, [u8]>) {
        self.cmd53_read(FUNC_WLAN, 0, buf).await;
    }

    /// Sends logical `buf` bytes to WLAN function address zero; transport failures are logged only.
    async fn wlan_write(&mut self, buf: &Aligned<A4, [u8]>) {
        self.cmd53_write(FUNC_WLAN, 0, buf).await;
    }

    /// Reads `data` from four-byte-aligned backplane `addr` in bounded window-safe chunks.
    /// Requires aligned data pointer, uses 1024 initialized scratch bytes and copies only logical bytes.
    /// Errors preserve unread caller bytes while any completed prefix remains; cancellation can lose in-flight scratch data.
    #[allow(unused)]
    async fn bp_read(&mut self, mut addr: u32, mut data: &mut [u8]) {
        trace!("bp_read addr = {:08x}, len = {}", addr, data.len());

        // It seems the HW force-aligns the addr
        // to 2 if data.len() >= 2
        // to 4 if data.len() >= 4
        // To simplify, enforce 4-align for now.
        assert!(addr % 4 == 0);
        assert!(data.as_ptr() as u32 % 4 == 0);

        // Only owned, initialized scratch padding may be exposed to an aligned bus view.
        let mut scratch = Aligned::<A4, _>([0u8; BLOCK_BUFFER_SIZE]);
        let scratch: &mut Aligned<A4, [u8]> = &mut scratch;

        while !data.is_empty() {
            // Ensure transfer doesn't cross a window boundary.
            let window_offs = addr & BACKPLANE_ADDRESS_MASK;
            let window_remaining = BACKPLANE_WINDOW_SIZE - window_offs as usize;

            let len = data.len().min(BLOCK_BUFFER_SIZE).min(window_remaining);
            scratch[..len].copy_from_slice(&data[..len]);
            let buf = &mut scratch[..len];

            self.backplane_set_window(addr).await;

            self.cmd53_read(FUNC_BACKPLANE, addr & BACKPLANE_ADDRESS_MASK as u32, buf)
                .await;
            data[..len].copy_from_slice(buf);

            // Advance ptr.
            addr += len as u32;
            data = &mut data[len..];
        }

        self.backplane_set_window(CHIPCOMMON_BASE_ADDRESS).await;
    }

    /// Sends `data` at four-byte-aligned backplane `addr` using 1024-byte initialized scratch/window-safe chunks.
    /// Requires aligned source pointer; no padding bytes are sent. No verification or error return; cancellation may deliver a prefix.
    async fn bp_write(&mut self, mut addr: u32, data: &[u8]) {
        trace!("bp_write addr = {:08x}, len = {}", addr, data.len());

        // It seems the HW force-aligns the addr
        // to 2 if data.len() >= 2
        // to 4 if data.len() >= 4
        // To simplify, enforce 4-align for now.
        assert!(addr % 4 == 0);
        assert!(data.as_ptr() as u32 % 4 == 0);

        let mut data = data;
        let mut scratch = Aligned::<A4, _>([0u8; BLOCK_BUFFER_SIZE]);
        let scratch: &mut Aligned<A4, [u8]> = &mut scratch;

        while !data.is_empty() {
            // Ensure transfer doesn't cross a window boundary.
            let window_offs = addr & BACKPLANE_ADDRESS_MASK;
            let window_remaining = BACKPLANE_WINDOW_SIZE - window_offs as usize;

            let len = data.len().min(BLOCK_BUFFER_SIZE).min(window_remaining);
            scratch[..len].copy_from_slice(&data[..len]);
            let buf = &scratch[..len];

            self.backplane_set_window(addr).await;

            self.cmd53_write(FUNC_BACKPLANE, addr & BACKPLANE_ADDRESS_MASK as u32, buf)
                .await;

            // Advance ptr.
            addr += len as u32;
            data = &data[len..];
        }

        self.backplane_set_window(CHIPCOMMON_BASE_ADDRESS).await;

        // TODO: implement verify download
    }

    /// Returns the 8-bit register at backplane `addr`, restoring the chip-common window afterward.
    async fn bp_read8(&mut self, addr: u32) -> u8 {
        self.backplane_readn(addr, Word::U8).await as u8
    }

    /// Writes 8-bit `val` at backplane `addr`, restoring the chip-common window; transport errors are swallowed.
    async fn bp_write8(&mut self, addr: u32, val: u8) {
        self.backplane_writen(addr, val as u32, Word::U8).await
    }

    /// Returns the 16-bit register at backplane `addr`, restoring the chip-common window afterward.
    async fn bp_read16(&mut self, addr: u32) -> u16 {
        self.backplane_readn(addr, Word::U16).await as u16
    }

    #[allow(unused)]
    /// Writes 16-bit `val` at backplane `addr`, restoring the chip-common window; transport errors are swallowed.
    async fn bp_write16(&mut self, addr: u32, val: u16) {
        self.backplane_writen(addr, val as u32, Word::U16).await
    }

    #[allow(unused)]
    /// Returns the 32-bit register at backplane `addr`, restoring the chip-common window afterward.
    async fn bp_read32(&mut self, addr: u32) -> u32 {
        self.backplane_readn(addr, Word::U32).await
    }

    /// Writes 32-bit `val` at backplane `addr`, restoring the chip-common window; transport errors are swallowed.
    async fn bp_write32(&mut self, addr: u32, val: u32) {
        self.backplane_writen(addr, val, Word::U32).await
    }

    /// Returns 8-bit data from `func`/`addr`; transport failure yields 0xff.
    async fn read8(&mut self, func: u32, addr: u32) -> u8 {
        self.cmd52(false, func, addr, 0).await.into()
    }

    /// Writes 8-bit `val` at `func`/`addr`; no transport error is propagated.
    async fn write8(&mut self, func: u32, addr: u32, val: u8) {
        self.cmd52(true, func, addr, val).await;
    }

    /// Returns 16-bit data from `func`/`addr`; buffer starts zero and failures can return partial data.
    async fn read16(&mut self, func: u32, addr: u32) -> u16 {
        let mut bytes = Aligned::<A4, _>([0u8; 2]);
        self.cmd53_read(func, addr, &mut bytes).await;

        u16::from_le_bytes(*bytes)
    }

    #[allow(unused)]
    /// Writes 16-bit `val` at `func`/`addr`; no transport error is propagated.
    async fn write16(&mut self, func: u32, addr: u32, val: u16) {
        self.cmd53_write(func, addr, &Aligned::<A4, _>(val.to_le_bytes())).await;
    }

    /// Returns 32-bit data from `func`/`addr`; buffer starts zero and failures can return partial data.
    async fn read32(&mut self, func: u32, addr: u32) -> u32 {
        let mut val = [0u32];
        self.cmd53_read(func, addr, &mut aligned_mut(&mut val)).await;

        val[0]
    }

    #[allow(unused)]
    /// Writes 32-bit `val` at `func`/`addr`; no transport error is propagated.
    async fn write32(&mut self, func: u32, addr: u32, val: u32) {
        self.cmd53_write(func, addr, &aligned_ref(&[val])).await;
    }

    /// Delays 10 ms before runner polling; does not call the low-level IRQ/event wait.
    async fn wait_for_event(&mut self) {
        Timer::after(Duration::from_millis(10)).await;
        // self.sdio.wait_for_event().await;
    }
}

#[cfg(test)]
/// Logical SDIO lengths and caller-owned tail preservation, using an immediate bus model.
mod tests {
    extern crate std;
    use super::*;
    use embassy_futures::block_on;
    use std::vec::Vec;

    /// Records payload commands without touching real SDIO registers.
    #[derive(Default)]
    struct MockSdio {
        /// CMD53 arguments and exact logical payload lengths, in invocation order.
        commands: Vec<(u32, usize)>,
        /// All transmitted payload bytes, excluding CMD52 window-register writes.
        writes: Vec<u8>,
        /// Read commands modify one byte before returning an error when set.
        fail_reads: bool,
    }

    impl SdioBusCyw43<BLOCK_SIZE> for MockSdio {
        /// Unit marks an injected partial-read failure.
        type Error = ();
        /// Accepts `frequency` without hardware effects; returns Ok.
        fn set_bus_to_high_speed(&mut self, frequency: u32) -> Result<(), Self::Error> {
            let _ = frequency;
            Ok(())
        }
        /// Returns the data byte in `arg`, sufficient for window-write acknowledgment.
        async fn cmd52(&mut self, arg: u32) -> Result<u16, Self::Error> {
            Ok((arg & 0xff) as u16)
        }
        /// Records `arg`, fills all `blocks` with a marker and returns Ok.
        async fn cmd53_block_read(
            &mut self,
            arg: u32,
            blocks: &mut [Aligned<A4, [u8; BLOCK_SIZE]>],
        ) -> Result<(), Self::Error> {
            self.commands.push((arg, blocks.len() * BLOCK_SIZE));
            if self.fail_reads {
                blocks[0][0] = 0x42;
                return Err(());
            }
            for block in blocks {
                block.fill(0x42);
            }
            Ok(())
        }
        /// Records `arg`, fills logical `buffer` bytes and returns Ok.
        async fn cmd53_byte_read(&mut self, arg: u32, buffer: &mut Aligned<A4, [u8]>) -> Result<(), Self::Error> {
            self.commands.push((arg, buffer.len()));
            if self.fail_reads {
                *buffer.first_mut().unwrap() = 0x42;
                return Err(());
            }
            buffer.fill(0x42);
            Ok(())
        }
        /// Records `arg` and exact `blocks` bytes; returns Ok.
        async fn cmd53_block_write(
            &mut self,
            arg: u32,
            blocks: &[Aligned<A4, [u8; BLOCK_SIZE]>],
        ) -> Result<(), Self::Error> {
            self.commands.push((arg, blocks.len() * BLOCK_SIZE));
            for block in blocks {
                self.writes.extend_from_slice(&block[..]);
            }
            Ok(())
        }
        /// Records `arg` and logical `buffer` bytes; returns Ok.
        async fn cmd53_byte_write(&mut self, arg: u32, buffer: &Aligned<A4, [u8]>) -> Result<(), Self::Error> {
            self.commands.push((arg, buffer.len()));
            self.writes.extend_from_slice(buffer);
            Ok(())
        }
    }

    /// CMD53 byte counts exclude Aligned DST padding at every byte/block boundary.
    #[test]
    fn cmd53_uses_logical_lengths_without_padding_bytes() {
        for len in [0, 1, 2, 3, 4, 63, 64, 65, 1023, 1024, 1025] {
            let backing = Aligned::<A4, _>([0xa5; 1028]);
            let backing: &Aligned<A4, [u8]> = &backing;
            let mut bus = SdioBus::new(MockSdio::default());
            block_on(bus.cmd53_write(FUNC_BACKPLANE, 0, &backing[..len]));
            assert_eq!(bus.sdio.writes.len(), len);
            for (arg, payload) in &bus.sdio.commands {
                let count = (arg & 0x1ff) as usize;
                assert_eq!(count * if arg & (1 << 27) != 0 { BLOCK_SIZE } else { 1 }, *payload);
            }
        }
    }

    /// Backplane transfers neither transmit nor overwrite bytes beyond a short caller slice, even across windows.
    #[test]
    fn backplane_preserves_caller_tail_and_window_boundaries() {
        for (addr, len) in [
            (0, 0),
            (0, 1),
            (0, 4),
            (0, 5),
            (0, 63),
            (0, 64),
            (0, 65),
            (0, 1024),
            (0, 1025),
            (0x7ffc, 4),
            (0x7ffc, 5),
        ] {
            let mut backing = Aligned::<A4, _>([0xa5; 1036]);
            let mut bus = SdioBus::new(MockSdio::default());
            block_on(bus.bp_write(addr, &backing[4..4 + len]));
            assert_eq!(bus.sdio.writes.len(), len);
            block_on(bus.bp_read(addr, &mut backing[4..4 + len]));
            assert!(backing[4..4 + len].iter().all(|byte| *byte == 0x42));
            assert!(
                backing[..4]
                    .iter()
                    .chain(backing[4 + len..].iter())
                    .all(|byte| *byte == 0xa5)
            );
            for (arg, payload) in &bus.sdio.commands {
                let offset = ((arg >> 9) & 0x1ffff) as usize;
                assert!((offset & BACKPLANE_ADDRESS_MASK as usize) + payload <= BACKPLANE_WINDOW_SIZE);
            }
        }
    }

    /// A failed block read preserves unread caller bytes while the subsequent byte phase still runs.
    #[test]
    fn partial_read_failure_preserves_unwritten_bytes() {
        let mut backing = Aligned::<A4, _>([0xa5; 68]);
        let mut bus = SdioBus::new(MockSdio {
            fail_reads: true,
            ..MockSdio::default()
        });
        block_on(bus.bp_read(0, &mut backing[..65]));
        assert_eq!(backing[0], 0x42);
        assert_eq!(backing[64], 0x42);
        assert!(
            backing[1..64]
                .iter()
                .chain(backing[65..].iter())
                .all(|byte| *byte == 0xa5)
        );
        assert_eq!(bus.sdio.commands.len(), 2);
    }
}
