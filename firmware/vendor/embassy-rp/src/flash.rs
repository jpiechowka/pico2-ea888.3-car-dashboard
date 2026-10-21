//! Flash driver.
//!
//! `FLASH_SIZE` is declared byte capacity, not detected physical capacity. Mutating operations
//! require application maintenance ownership of both cores, DMA and XIP clients. RP235x CS0
//! restoration failures park in SRAM; an independently armed watchdog supplies the recovery bound.
#[cfg(feature = "_rp235x")]
#[path = "flash_probe.rs"]
/// Bounded QMI direct-mode JEDEC transaction shared with host regression tests.
mod probe;
/// Length admission for the RP2040 assembly receive loop.
#[cfg(feature = "rp2040")]
#[path = "flash_read.rs"]
mod read_layout;
use core::future::Future;
use core::marker::PhantomData;
use core::pin::Pin;
use core::task::{Context, Poll};

use embassy_hal_internal::{Peri, PeripheralType};
use embedded_storage::nor_flash::{
    ErrorType,
    MultiwriteNorFlash,
    NorFlash,
    NorFlashError,
    NorFlashErrorKind,
    ReadNorFlash,
    check_erase,
    check_read,
    check_write,
};

use crate::peripherals::FLASH;
use crate::{dma, interrupt, pac};

/// Flash base address.
pub const FLASH_BASE: *const u32 = 0x10000000 as _;

/// Address for xip setup function set up by the 235x bootrom.
#[cfg(feature = "_rp235x")]
pub const BOOTRAM_BASE: *const u32 = 0x400E0000 as _;

/// If running from RAM, we might have no boot2. Use bootrom `flash_enter_cmd_xip` instead.
// TODO: when run-from-ram is set, completely skip the "pause cores and jumpp to RAM" dance.
pub const USE_BOOT2: bool = !cfg!(feature = "run-from-ram") | cfg!(feature = "_rp235x");

// **NOTE**:
//
// These limitations are currently enforced because of using the
// RP2040 boot-rom flash functions, that are optimized for flash compatibility
// rather than performance.
/// Flash page size.
pub const PAGE_SIZE: usize = 256;
/// Flash write size.
pub const WRITE_SIZE: usize = 1;
/// Flash read size.
pub const READ_SIZE: usize = 1;
/// Flash erase size.
pub const ERASE_SIZE: usize = 4096;
/// Flash DMA read size.
pub const ASYNC_READ_SIZE: usize = 4;

/// Validation/core-admission errors; ROM calls do not report program/erase status here.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum Error {
    /// Operation using a location not in flash.
    OutOfBounds,
    /// Unaligned operation or using unaligned buffers.
    Unaligned,
    /// Accessed from the wrong core.
    InvalidCore,
    /// Other error
    Other,
}

impl From<NorFlashErrorKind> for Error {
    /// Returns local bounds/alignment classification for `e`, collapsing other kinds to Other.
    fn from(e: NorFlashErrorKind) -> Self {
        match e {
            NorFlashErrorKind::NotAligned => Self::Unaligned,
            NorFlashErrorKind::OutOfBounds => Self::OutOfBounds,
            _ => Self::Other,
        }
    }
}

impl NorFlashError for Error {
    /// Returns the storage-trait category; wrong-core failures map to Other.
    fn kind(&self) -> NorFlashErrorKind {
        match self {
            Self::OutOfBounds => NorFlashErrorKind::OutOfBounds,
            Self::Unaligned => NorFlashErrorKind::NotAligned,
            _ => NorFlashErrorKind::Other,
        }
    }
}

/// Already-started streaming read retaining flash/channel/output borrows.
/// Drop stops the XIP stream, orders outstanding reads, then synchronously aborts DMA.
#[must_use = "futures do nothing unless you `.await` or poll them"]
pub struct BackgroundRead<'a, 'd, T: Instance, const FLASH_SIZE: usize> {
    /// Exclusive flash ownership through stream shutdown.
    flash: PhantomData<&'a mut Flash<'d, T, Async, FLASH_SIZE>>,
    /// Active receiver; its destructor aborts DMA after the stream is stopped.
    transfer: dma::Transfer<'a>,
}

impl<'a, 'd, T: Instance, const FLASH_SIZE: usize> Future for BackgroundRead<'a, 'd, T, FLASH_SIZE> {
    /// DMA completion without a separate byte count or controller status.
    type Output = ();

    /// Polls `self`'s DMA using `cx`; returns ready when its BUSY bit clears.
    fn poll(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Self::Output> {
        Pin::new(&mut self.transfer).poll(cx)
    }
}

impl<'a, 'd, T: Instance, const FLASH_SIZE: usize> Drop for BackgroundRead<'a, 'd, T, FLASH_SIZE> {
    /// Stops stream requests and synchronizes an in-flight XIP read before the DMA field drops.
    fn drop(&mut self) {
        if pac::XIP_CTRL.stream_ctr().read().0 == 0 {
            return;
        }
        pac::XIP_CTRL
            .stream_ctr()
            .write_value(pac::xip_ctrl::regs::StreamCtr(0));
        core::sync::atomic::compiler_fence(core::sync::atomic::Ordering::SeqCst);
        // Errata RP2040-E8: Perform an uncached read to make sure there's not a transfer in
        // flight that might effect an address written to start a new transfer.  This stalls
        // until after any transfer is complete, so the address will not change anymore.
        #[cfg(feature = "rp2040")]
        /// Uncached flash alias used as a stream-shutdown ordering barrier.
        const XIP_NOCACHE_NOALLOC_BASE: *const u32 = 0x13000000 as *const _;
        #[cfg(feature = "_rp235x")]
        /// Uncached flash alias used as a stream-shutdown ordering barrier.
        const XIP_NOCACHE_NOALLOC_BASE: *const u32 = 0x14000000 as *const _;
        unsafe {
            core::ptr::read_volatile(XIP_NOCACHE_NOALLOC_BASE);
        }
        core::sync::atomic::compiler_fence(core::sync::atomic::Ordering::SeqCst);
    }
}

/// Exclusive flash owner; construction does not probe physical capacity or safe write regions.
pub struct Flash<'d, T: Instance, M: Mode, const FLASH_SIZE: usize> {
    /// Async-mode DMA receiver; absent in blocking mode.
    dma: Option<dma::Channel<'d>>,
    /// Retains peripheral ownership and compile-time mode selection.
    phantom: PhantomData<(&'d mut T, M)>,
}

impl<'d, T: Instance, M: Mode, const FLASH_SIZE: usize> Flash<'d, T, M, FLASH_SIZE> {
    /// Copies XIP bytes at flash-relative byte `offset` into `bytes`.
    /// Returns validation errors before copying; byte alignment suffices. Declared capacity must
    /// match the physical mapping and no concurrent flash mutation may invalidate the read.
    pub fn blocking_read(
        &mut self,
        offset: u32,
        bytes: &mut [u8],
    ) -> Result<(), Error> {
        trace!(
            "Reading from 0x{:x} to 0x{:x}",
            FLASH_BASE as u32 + offset,
            FLASH_BASE as u32 + offset + bytes.len() as u32
        );
        check_read(self, offset, bytes.len())?;

        let flash_data = unsafe { core::slice::from_raw_parts((FLASH_BASE as u32 + offset) as *const u8, bytes.len()) };

        bytes.copy_from_slice(flash_data);
        Ok(())
    }

    /// Returns declared `FLASH_SIZE` in bytes; no chip-capacity probe is performed.
    pub fn capacity(&self) -> usize { FLASH_SIZE }

    /// Erases flash-relative byte range from `from` (inclusive) to `to` (exclusive), with sector-aligned endpoints.
    /// Returns bounds/alignment/wrong-core errors. Blocks with peer parked and interrupts masked;
    /// ROM completion has no reported device status, verification or power-loss rollback.
    pub fn blocking_erase(
        &mut self,
        from: u32,
        to: u32,
    ) -> Result<(), Error> {
        check_erase(self, from, to)?;

        trace!(
            "Erasing from 0x{:x} to 0x{:x}",
            FLASH_BASE as u32 + from,
            FLASH_BASE as u32 + to
        );

        let len = to - from;

        unsafe { in_ram(|| ram_helpers::flash_range_erase(from, len))? };

        Ok(())
    }

    /// Programs `bytes` at flash-relative byte `offset`, padding partial pages with 0xFF.
    /// Returns validation/wrong-core errors; earlier programmed pages are not rolled back.
    /// Does not erase: callers must obey chip programming rules. Flash-backed input is copied
    /// through SRAM pages; addresses at/above 0x20000000 are assumed safe to use directly.
    pub fn blocking_write(
        &mut self,
        offset: u32,
        bytes: &[u8],
    ) -> Result<(), Error> {
        check_write(self, offset, bytes.len())?;

        trace!("Writing {:?} bytes to 0x{:x}", bytes.len(), FLASH_BASE as u32 + offset);

        let end_offset = offset as usize + bytes.len();

        let padded_offset = (offset as *const u8).align_offset(PAGE_SIZE);
        let start_padding = core::cmp::min(padded_offset, bytes.len());

        // Pad in the beginning
        if start_padding > 0 {
            let start = PAGE_SIZE - padded_offset;
            let end = start + start_padding;

            let mut pad_buf = [0xFF_u8; PAGE_SIZE];
            pad_buf[start..end].copy_from_slice(&bytes[..start_padding]);

            let unaligned_offset = offset as usize - start;

            unsafe { in_ram(|| ram_helpers::flash_range_program(unaligned_offset as u32, &pad_buf))? }
        }

        let remaining_len = bytes.len() - start_padding;
        let end_padding = start_padding + PAGE_SIZE * (remaining_len / PAGE_SIZE);

        // Write aligned slice of length in multiples of 256 bytes
        // If the remaining bytes to be written is more than a full page.
        if remaining_len >= PAGE_SIZE {
            let mut aligned_offset = if start_padding > 0 {
                offset as usize + padded_offset
            } else {
                offset as usize
            };

            if bytes.as_ptr() as usize >= 0x2000_0000 {
                let aligned_data = &bytes[start_padding..end_padding];

                unsafe { in_ram(|| ram_helpers::flash_range_program(aligned_offset as u32, aligned_data))? }
            } else {
                for chunk in bytes[start_padding..end_padding].chunks_exact(PAGE_SIZE) {
                    let mut ram_buf = [0xFF_u8; PAGE_SIZE];
                    ram_buf.copy_from_slice(chunk);
                    unsafe { in_ram(|| ram_helpers::flash_range_program(aligned_offset as u32, &ram_buf))? }
                    aligned_offset += PAGE_SIZE;
                }
            }
        }

        // Pad in the end
        let rem_offset = (end_offset as *const u8).align_offset(PAGE_SIZE);
        let rem_padding = remaining_len % PAGE_SIZE;
        if rem_padding > 0 {
            let mut pad_buf = [0xFF_u8; PAGE_SIZE];
            pad_buf[..rem_padding].copy_from_slice(&bytes[end_padding..]);

            let unaligned_offset = end_offset - (PAGE_SIZE - rem_offset);

            unsafe { in_ram(|| ram_helpers::flash_range_program(unaligned_offset as u32, &pad_buf))? }
        }

        Ok(())
    }

    /// Fills `uid` with bytes from optional chip command 0x4B. Returns admission/length errors.
    /// Command support and requested identifier length must match the physical flash part.
    /// Empty buffers succeed without hardware access or core admission checks; lengths above65532 return OutOfBounds
    /// because the SSI counter must also receive four dummy bytes.
    #[cfg(feature = "rp2040")]
    pub fn blocking_unique_id(
        &mut self,
        uid: &mut [u8],
    ) -> Result<(), Error> {
        read_layout::read_unique_id(uid, Error::OutOfBounds, |uid| unsafe {
            in_ram(|| ram_helpers::flash_unique_id(uid))
        })
    }

    /// Returns the 24-bit JEDEC manufacturer/type/capacity ID, or wrong-core admission error.
    ///
    /// The local RP2350 implementation is limited to CS0 flash with no CS1
    /// memory and the boot-ROM XIP format. The application checks this contract
    /// before calling. See vendor/README.md and the linked RAM-code gate.
    pub fn blocking_jedec_id(&mut self) -> Result<u32, Error> {
        let mut jedec = None;
        unsafe {
            in_ram(|| {
                jedec.replace(ram_helpers::flash_jedec_id());
            })?;
        };
        Ok(jedec.unwrap())
    }
}

impl<'d, T: Instance, const FLASH_SIZE: usize> Flash<'d, T, Blocking, FLASH_SIZE> {
    /// Returns a blocking driver consuming `_flash`; chip/maintenance safety is not probed.
    pub fn new_blocking(_flash: Peri<'d, T>) -> Self {
        Self {
            dma: None,
            phantom: PhantomData,
        }
    }
}

impl<'d, T: Instance, const FLASH_SIZE: usize> Flash<'d, T, Async, FLASH_SIZE> {
    /// Returns an async driver consuming `_flash` and `dma`; `irq` binds DMA completion handling.
    /// Enables that channel interrupt without reading flash-chip metadata.
    pub fn new<D: dma::ChannelInstance>(
        _flash: Peri<'d, T>,
        dma: Peri<'d, D>,
        irq: impl interrupt::typelevel::Binding<D::Interrupt, dma::InterruptHandler<D>> + 'd,
    ) -> Self {
        Self {
            dma: Some(dma::Channel::new(dma, irq)),
            phantom: PhantomData,
        }
    }

    /// Starts a stream at flash-relative byte `offset` into aligned `data` words.
    /// Returns the already-active DMA lease or bounds/alignment error before starting.
    /// Offset must be four-byte aligned; the output stays borrowed through completion or abort.
    pub fn background_read<'a>(
        &'a mut self,
        offset: u32,
        data: &'a mut [u32],
    ) -> Result<BackgroundRead<'a, 'd, T, FLASH_SIZE>, Error> {
        trace!(
            "Reading in background from 0x{:x} to 0x{:x}",
            FLASH_BASE as u32 + offset,
            FLASH_BASE as u32 + offset + (data.len() * 4) as u32
        );
        // Can't use check_read because we need to enforce 4-byte alignment
        let offset = offset as usize;
        let length = data.len() * 4;
        if length > self.capacity() || offset > self.capacity() - length {
            return Err(Error::OutOfBounds);
        }
        if offset % 4 != 0 {
            return Err(Error::Unaligned);
        }

        while !pac::XIP_CTRL.stat().read().fifo_empty() {
            pac::XIP_CTRL.stream_fifo().read();
        }

        pac::XIP_CTRL
            .stream_addr()
            .write_value(pac::xip_ctrl::regs::StreamAddr(FLASH_BASE as u32 + offset as u32));
        pac::XIP_CTRL
            .stream_ctr()
            .write_value(pac::xip_ctrl::regs::StreamCtr(data.len() as u32));

        // Use the XIP AUX bus port, rather than the FIFO register access (e.x.
        // pac::XIP_CTRL.stream_fifo().as_ptr()) to avoid DMA stalling on
        // general XIP access.
        #[cfg(feature = "rp2040")]
        /// Dedicated stream bus port avoids contention on the ordinary XIP register bus.
        const XIP_AUX_BASE: *const u32 = 0x50400000 as *const _;
        #[cfg(feature = "_rp235x")]
        /// Dedicated stream bus port avoids contention on the ordinary XIP register bus.
        const XIP_AUX_BASE: *const u32 = 0x50500000 as *const _;
        let transfer = unsafe {
            self.dma
                .as_mut()
                .unwrap()
                .read(XIP_AUX_BASE, data, pac::dma::vals::TreqSel::XIP_STREAM, false)
        };

        Ok(BackgroundRead {
            flash: PhantomData,
            transfer,
        })
    }

    /// Reads flash-relative byte `offset` into `bytes`; returns bounds/alignment errors.
    /// Offset/length require four-byte alignment; an unaligned destination uses 256-byte stack
    /// chunks. Bounds are checked per chunk, so errors/cancellation can leave a prefix copied.
    /// Dropping an in-flight read stops the XIP stream and aborts its DMA receiver.
    pub async fn read(
        &mut self,
        offset: u32,
        bytes: &mut [u8],
    ) -> Result<(), Error> {
        use core::mem::MaybeUninit;

        // Checked early to simplify address validity checks
        if bytes.len() % 4 != 0 {
            return Err(Error::Unaligned);
        }

        // If the destination address is already aligned, then we can just DMA directly
        if (bytes.as_ptr() as u32) % 4 == 0 {
            // Safety: alignment and size have been checked for compatibility
            let buf: &mut [u32] =
                unsafe { core::slice::from_raw_parts_mut(bytes.as_mut_ptr() as *mut u32, bytes.len() / 4) };
            self.background_read(offset, buf)?.await;
            return Ok(());
        }

        // Destination address is unaligned, so use an intermediate buffer
        /// Maximum bytes copied through the aligned stack scratch buffer.
        const REALIGN_CHUNK: usize = PAGE_SIZE;
        // Safety: MaybeUninit requires no initialization
        let mut buf: [MaybeUninit<u32>; REALIGN_CHUNK / 4] = unsafe { MaybeUninit::uninit().assume_init() };
        let mut chunk_offset: usize = 0;
        while chunk_offset < bytes.len() {
            let chunk_size = (bytes.len() - chunk_offset).min(REALIGN_CHUNK);
            let buf = &mut buf[..(chunk_size / 4)];

            // Safety: this is written to completely by DMA before any reads
            let buf = unsafe { &mut *(buf as *mut [MaybeUninit<u32>] as *mut [u32]) };
            self.background_read(offset + chunk_offset as u32, buf)?.await;

            // Safety: [u8] has more relaxed alignment and size requirements than [u32], so this is just aliasing
            let buf = unsafe { core::slice::from_raw_parts(buf.as_ptr() as *const _, buf.len() * 4) };
            bytes[chunk_offset..(chunk_offset + chunk_size)].copy_from_slice(&buf[..chunk_size]);

            chunk_offset += chunk_size;
        }

        Ok(())
    }
}

impl<'d, T: Instance, M: Mode, const FLASH_SIZE: usize> ErrorType for Flash<'d, T, M, FLASH_SIZE> {
    /// Validation/core-admission error used by the storage traits.
    type Error = Error;
}

impl<'d, T: Instance, M: Mode, const FLASH_SIZE: usize> ReadNorFlash for Flash<'d, T, M, FLASH_SIZE> {
    /// Blocking reads accept byte-aligned offsets and lengths.
    const READ_SIZE: usize = READ_SIZE;

    /// Reads `bytes` at flash-relative `offset`; returns blocking validation errors.
    fn read(
        &mut self,
        offset: u32,
        bytes: &mut [u8],
    ) -> Result<(), Self::Error> {
        self.blocking_read(offset, bytes)
    }

    /// Returns declared byte capacity without probing the chip.
    fn capacity(&self) -> usize { self.capacity() }
}

impl<'d, T: Instance, M: Mode, const FLASH_SIZE: usize> MultiwriteNorFlash for Flash<'d, T, M, FLASH_SIZE> {}

impl<'d, T: Instance, const FLASH_SIZE: usize> embedded_storage_async::nor_flash::MultiwriteNorFlash
    for Flash<'d, T, Async, FLASH_SIZE>
{
}

impl<'d, T: Instance, M: Mode, const FLASH_SIZE: usize> NorFlash for Flash<'d, T, M, FLASH_SIZE> {
    /// Erase endpoints use 4096-byte sectors.
    const ERASE_SIZE: usize = ERASE_SIZE;
    /// Byte-aligned public writes are internally padded to ROM pages.
    const WRITE_SIZE: usize = WRITE_SIZE;

    /// Erases sector-aligned range `from` to `to` (exclusive); returns blocking validation/core errors.
    fn erase(
        &mut self,
        from: u32,
        to: u32,
    ) -> Result<(), Self::Error> {
        self.blocking_erase(from, to)
    }

    /// Programs `bytes` at `offset`; returns blocking validation/core errors.
    fn write(
        &mut self,
        offset: u32,
        bytes: &[u8],
    ) -> Result<(), Self::Error> {
        self.blocking_write(offset, bytes)
    }
}

impl<'d, T: Instance, const FLASH_SIZE: usize> embedded_storage_async::nor_flash::ReadNorFlash
    for Flash<'d, T, Async, FLASH_SIZE>
{
    /// DMA offsets and lengths are multiples of four bytes.
    const READ_SIZE: usize = ASYNC_READ_SIZE;

    /// Reads `bytes` at `offset`; returns DMA read validation errors, possibly after a copied prefix.
    async fn read(
        &mut self,
        offset: u32,
        bytes: &mut [u8],
    ) -> Result<(), Self::Error> {
        self.read(offset, bytes).await
    }

    /// Returns declared byte capacity without probing the chip.
    fn capacity(&self) -> usize { self.capacity() }
}

impl<'d, T: Instance, const FLASH_SIZE: usize> embedded_storage_async::nor_flash::NorFlash
    for Flash<'d, T, Async, FLASH_SIZE>
{
    /// Erase endpoints use 4096-byte sectors.
    const ERASE_SIZE: usize = ERASE_SIZE;
    /// Byte-aligned public writes are internally padded to ROM pages.
    const WRITE_SIZE: usize = WRITE_SIZE;

    /// Erases `from` to `to` (exclusive) synchronously on polling; returns blocking validation/core errors.
    async fn erase(
        &mut self,
        from: u32,
        to: u32,
    ) -> Result<(), Self::Error> {
        self.blocking_erase(from, to)
    }

    /// Programs `bytes` at `offset` synchronously on polling; returns blocking validation/core errors.
    async fn write(
        &mut self,
        offset: u32,
        bytes: &[u8],
    ) -> Result<(), Self::Error> {
        self.blocking_write(offset, bytes)
    }
}

#[allow(dead_code)]
/// ROM call tables, RAM-only XIP-off bodies and checked QMI restoration.
mod ram_helpers {
    use super::*;
    use crate::rom_data;

    #[repr(C)]
    /// ROM/RAM callees resolved before XIP is disabled; C field order matches RP2040 assembly.
    struct FlashFunctionPointers<'a> {
        /// ROM flash pad/peripheral initialization entry.
        connect_internal_flash: unsafe extern "C" fn() -> (),
        /// ROM transition from XIP to direct flash command mode.
        flash_exit_xip: unsafe extern "C" fn() -> (),
        /// Optional erase entry; None skips erasing in the combined RAM body.
        flash_range_erase: Option<unsafe extern "C" fn(addr: u32, count: usize, block_size: u32, block_cmd: u8) -> ()>,
        /// Optional program entry; None skips programming in the combined RAM body.
        flash_range_program: Option<unsafe extern "C" fn(addr: u32, data: *const u8, count: usize) -> ()>,
        /// ROM cache flush after command completion.
        flash_flush_cache: unsafe extern "C" fn() -> (),
        /// ROM fallback or borrowed SRAM boot setup restoring XIP access.
        flash_enter_cmd_xip: unsafe extern "C" fn() -> (),
        /// Keeps copied SRAM boot code alive while its function pointer is used.
        phantom: PhantomData<&'a ()>,
    }

    #[allow(unused)]
    /// Returns ROM entries, enabling erase/program only when `erase`/`write` is true.
    fn flash_function_pointers(
        erase: bool,
        write: bool,
    ) -> FlashFunctionPointers<'static> {
        FlashFunctionPointers {
            connect_internal_flash: rom_data::connect_internal_flash::ptr(),
            flash_exit_xip: rom_data::flash_exit_xip::ptr(),
            flash_range_erase: if erase {
                Some(rom_data::flash_range_erase::ptr())
            } else {
                None
            },
            flash_range_program: if write {
                Some(rom_data::flash_range_program::ptr())
            } else {
                None
            },
            flash_flush_cache: rom_data::flash_flush_cache::ptr(),
            flash_enter_cmd_xip: rom_data::flash_enter_cmd_xip::ptr(),
            phantom: PhantomData,
        }
    }

    #[allow(unused)]
    /// # Safety
    ///
    /// `boot2` must contain a valid 2nd stage boot loader which can be called to re-initialize XIP mode
    /// Returns ROM entries selected by `erase`/`write`, using borrowed `boot2` for XIP restoration.
    unsafe fn flash_function_pointers_with_boot2(
        erase: bool,
        write: bool,
        boot2: &[u32; 64],
    ) -> FlashFunctionPointers<'_> {
        let boot2_fn_ptr = (boot2 as *const u32 as *const u8).offset(1);
        let boot2_fn: unsafe extern "C" fn() -> () = core::mem::transmute(boot2_fn_ptr);
        FlashFunctionPointers {
            connect_internal_flash: rom_data::connect_internal_flash::ptr(),
            flash_exit_xip: rom_data::flash_exit_xip::ptr(),
            flash_range_erase: if erase {
                Some(rom_data::flash_range_erase::ptr())
            } else {
                None
            },
            flash_range_program: if write {
                Some(rom_data::flash_range_program::ptr())
            } else {
                None
            },
            flash_flush_cache: rom_data::flash_flush_cache::ptr(),
            flash_enter_cmd_xip: boot2_fn,
            phantom: PhantomData,
        }
    }

    /// Erase a flash range starting at `addr` with length `len`.
    ///
    /// `addr` and `len` must be multiples of 4096
    ///
    /// If `USE_BOOT2` is `true`, a copy of the 2nd stage boot loader
    /// is used to re-initialize the XIP engine after flashing.
    ///
    /// # Safety
    ///
    /// Nothing must access flash while this is running.
    /// Usually this means:
    ///   - interrupts must be disabled
    ///   - 2nd core must be running code from RAM or ROM with interrupts disabled
    ///   - DMA must not access flash memory
    ///
    /// `addr` and `len` parameters must be valid and are not checked.
    pub unsafe fn flash_range_erase(
        addr: u32,
        len: u32,
    ) {
        let mut boot2 = [0u32; 256 / 4];
        let ptrs = if USE_BOOT2 {
            #[cfg(feature = "rp2040")]
            rom_data::memcpy44(&mut boot2 as *mut _, FLASH_BASE, 256);
            #[cfg(feature = "_rp235x")]
            core::ptr::copy_nonoverlapping(BOOTRAM_BASE as *const u8, boot2.as_mut_ptr() as *mut u8, 256);
            flash_function_pointers_with_boot2(true, false, &boot2)
        } else {
            flash_function_pointers(true, false)
        };

        core::sync::atomic::compiler_fence(core::sync::atomic::Ordering::SeqCst);

        write_flash_inner(addr, len, None, &ptrs as *const FlashFunctionPointers);
    }

    /// Erase and rewrite a flash range starting at `addr` with data `data`.
    ///
    /// `addr` and `data.len()` must be multiples of 4096
    ///
    /// If `USE_BOOT2` is `true`, a copy of the 2nd stage boot loader
    /// is used to re-initialize the XIP engine after flashing.
    ///
    /// # Safety
    ///
    /// Nothing must access flash while this is running.
    /// Usually this means:
    ///   - interrupts must be disabled
    ///   - 2nd core must be running code from RAM or ROM with interrupts disabled
    ///   - DMA must not access flash memory
    ///
    /// `addr` and SRAM `data` must cover valid ranges; neither is checked.
    pub unsafe fn flash_range_erase_and_program(
        addr: u32,
        data: &[u8],
    ) {
        let mut boot2 = [0u32; 256 / 4];
        let ptrs = if USE_BOOT2 {
            #[cfg(feature = "rp2040")]
            rom_data::memcpy44(&mut boot2 as *mut _, FLASH_BASE, 256);
            #[cfg(feature = "_rp235x")]
            core::ptr::copy_nonoverlapping(BOOTRAM_BASE as *const u8, (boot2).as_mut_ptr() as *mut u8, 256);
            flash_function_pointers_with_boot2(true, true, &boot2)
        } else {
            flash_function_pointers(true, true)
        };

        core::sync::atomic::compiler_fence(core::sync::atomic::Ordering::SeqCst);

        write_flash_inner(
            addr,
            data.len() as u32,
            Some(data),
            &ptrs as *const FlashFunctionPointers,
        );
    }

    /// Write a flash range starting at `addr` with data `data`.
    ///
    /// `addr` and `data.len()` must be multiples of 256
    ///
    /// If `USE_BOOT2` is `true`, a copy of the 2nd stage boot loader
    /// is used to re-initialize the XIP engine after flashing.
    ///
    /// # Safety
    ///
    /// Nothing must access flash while this is running.
    /// Usually this means:
    ///   - interrupts must be disabled
    ///   - 2nd core must be running code from RAM or ROM with interrupts disabled
    ///   - DMA must not access flash memory
    ///
    /// `addr` and SRAM `data` must cover valid ranges; neither is checked.
    pub unsafe fn flash_range_program(
        addr: u32,
        data: &[u8],
    ) {
        let mut boot2 = [0u32; 256 / 4];
        let ptrs = if USE_BOOT2 {
            #[cfg(feature = "rp2040")]
            rom_data::memcpy44(&mut boot2 as *mut _, FLASH_BASE, 256);
            #[cfg(feature = "_rp235x")]
            core::ptr::copy_nonoverlapping(BOOTRAM_BASE as *const u8, boot2.as_mut_ptr() as *mut u8, 256);
            flash_function_pointers_with_boot2(false, true, &boot2)
        } else {
            flash_function_pointers(false, true)
        };

        core::sync::atomic::compiler_fence(core::sync::atomic::Ordering::SeqCst);

        write_flash_inner(
            addr,
            data.len() as u32,
            Some(data),
            &ptrs as *const FlashFunctionPointers,
        );
    }

    /// # Safety
    ///
    /// Nothing must access flash while this is running.
    /// Usually this means:
    ///   - interrupts must be disabled
    ///   - 2nd core must be running code from RAM or ROM with interrupts disabled
    ///   - DMA must not access flash memory
    /// `addr` is flash-relative bytes, `len` is the count, `data` is optional SRAM input and `ptrs`
    /// an SRAM call table. Erase requires 4096-byte alignment; program requires 256-byte alignment.
    /// Enabled program calls require `data` covering `len`; all addresses/callees must remain valid.
    #[inline(never)]
    #[unsafe(link_section = ".data.ram_func")]
    #[cfg(feature = "rp2040")]
    unsafe fn write_flash_inner(
        addr: u32,
        len: u32,
        data: Option<&[u8]>,
        ptrs: *const FlashFunctionPointers,
    ) {
        #[cfg(target_arch = "arm")]
        core::arch::asm!(
            "mov r8, r0",
            "mov r9, r2",
            "mov r10, r1",
            "ldr r4, [{ptrs}, #0]",
            "blx r4", // connect_internal_flash()

            "ldr r4, [{ptrs}, #4]",
            "blx r4", // flash_exit_xip()

            "mov r0, r8", // r0 = addr
            "mov r1, r10", // r1 = len
            "movs r2, #1",
            "lsls r2, r2, #31", // r2 = 1 << 31
            "movs r3, #0", // r3 = 0
            "ldr r4, [{ptrs}, #8]",
            "cmp r4, #0",
            "beq 2f",
            "blx r4", // flash_range_erase(addr, len, 1 << 31, 0)
            "2:",

            "mov r0, r8", // r0 = addr
            "mov r1, r9", // r0 = data
            "mov r2, r10", // r2 = len
            "ldr r4, [{ptrs}, #12]",
            "cmp r4, #0",
            "beq 2f",
            "blx r4", // flash_range_program(addr, data, len);
            "2:",

            "ldr r4, [{ptrs}, #16]",
            "blx r4", // flash_flush_cache();

            "ldr r4, [{ptrs}, #20]",
            "blx r4", // flash_enter_cmd_xip();
            ptrs = in(reg) ptrs,
            // Registers r8-r15 are not allocated automatically,
            // so assign them manually. We need to use them as
            // otherwise there are not enough registers available.
            in("r0") addr,
            in("r2") data.map(|d| d.as_ptr()).unwrap_or(core::ptr::null()),
            in("r1") len,
            out("r3") _,
            out("r4") _,
            lateout("r8") _,
            lateout("r9") _,
            lateout("r10") _,
            clobber_abi("C"),
        );
    }

    /// # Safety
    ///
    /// Nothing must access flash while this is running.
    /// Usually this means:
    ///   - interrupts must be disabled
    ///   - 2nd core must be running code from RAM or ROM with interrupts disabled
    ///   - DMA must not access flash memory
    /// `addr` is flash-relative bytes, `len` is the count, `data` is optional SRAM input and `ptrs`
    /// an SRAM call table. Erase requires 4096-byte alignment; program requires 256-byte alignment.
    /// Enabled program calls require `data` covering `len`; all addresses/callees must remain valid.
    #[inline(never)]
    #[unsafe(link_section = ".data.ram_func")]
    #[cfg(feature = "_rp235x")]
    unsafe fn write_flash_inner(
        addr: u32,
        len: u32,
        data: Option<&[u8]>,
        ptrs: *const FlashFunctionPointers,
    ) {
        let saved = XipState::capture();
        let data = data.map(|d| d.as_ptr()).unwrap_or(core::ptr::null());
        ((*ptrs).connect_internal_flash)();
        ((*ptrs).flash_exit_xip)();
        if (*ptrs).flash_range_erase.is_some() {
            ((*ptrs).flash_range_erase.unwrap())(addr, len as usize, 1 << 31, 0);
        }
        if (*ptrs).flash_range_program.is_some() {
            ((*ptrs).flash_range_program.unwrap())(addr, data as *const _, len as usize);
        }
        ((*ptrs).flash_flush_cache)();
        ((*ptrs).flash_enter_cmd_xip)();
        saved.restore();
    }

    // CS1/PSRAM is deliberately outside the local application's contract.
    // Inline these helpers into RAM functions; final instruction bytes and
    // transitive calls are verified in every deployable optimized artifact.
    #[cfg(feature = "_rp235x")]
    /// CS0 QMI and six pad settings retained in SRAM across ROM flash operations.
    struct XipState {
        /// Original CS0 divisor/delay timing.
        timing: pac::qmi::regs::Timing,
        /// Expected boot-restored CS0 read format.
        rfmt: pac::qmi::regs::Rfmt,
        /// Expected boot-restored CS0 read opcode.
        rcmd: pac::qmi::regs::Rcmd,
        /// Preserved high control bits; direct-mode enables/chip selects remain clear.
        direct: pac::qmi::regs::DirectCsr,
        /// Original six QSPI electrical pad control words.
        pads: [pac::pads::regs::GpioCtrl; 6],
    }
    #[cfg(feature = "_rp235x")]
    impl XipState {
        #[inline(always)]
        /// Returns the current CS0/read/pad state before command-mode entry.
        fn capture() -> Self {
            Self {
                // Original CS0 divisor/delay timing.
                timing: pac::QMI.mem(0).timing().read(),
                // Expected boot-restored CS0 read format.
                rfmt: pac::QMI.mem(0).rfmt().read(),
                // Expected boot-restored CS0 read opcode.
                rcmd: pac::QMI.mem(0).rcmd().read(),
                // Preserved high control bits; direct-mode enables/chip selects remain clear.
                direct: pac::qmi::regs::DirectCsr(pac::QMI.direct_csr().read().0 & 0xFFC0_0000),
                // Keep all six accesses explicit. Every supported optimization
                // still requires a current-byte RAM dependency review.
                pads: [
                    pac::PADS_QSPI.gpio(0).read(),
                    pac::PADS_QSPI.gpio(1).read(),
                    pac::PADS_QSPI.gpio(2).read(),
                    pac::PADS_QSPI.gpio(3).read(),
                    pac::PADS_QSPI.gpio(4).read(),
                    pac::PADS_QSPI.gpio(5).read(),
                ],
            }
        }

        #[inline(always)]
        /// Restores timing/pads from `self` after matching boot read format/opcode.
        /// Mismatch/readback failure parks in SRAM without returning to uncertain XIP.
        fn restore(self) {
            // Never apply fast timing to a different read command/format.
            // No panic/return into flash on an unexpected boot2 restoration.
            if pac::QMI.mem(0).rfmt().read() != self.rfmt || pac::QMI.mem(0).rcmd().read() != self.rcmd {
                fail_stop();
            }
            pac::QMI.mem(0).timing().write_value(self.timing);
            // EN, ASSERT_CS and AUTO_CS must remain clear, including when EN=0.
            pac::QMI.direct_csr().write_value(self.direct);
            pac::PADS_QSPI.gpio(0).write_value(self.pads[0]);
            pac::PADS_QSPI.gpio(1).write_value(self.pads[1]);
            pac::PADS_QSPI.gpio(2).write_value(self.pads[2]);
            pac::PADS_QSPI.gpio(3).write_value(self.pads[3]);
            pac::PADS_QSPI.gpio(4).write_value(self.pads[4]);
            pac::PADS_QSPI.gpio(5).write_value(self.pads[5]);
            cortex_m::asm::dsb();
            let direct = pac::QMI.direct_csr().read();
            if pac::QMI.mem(0).timing().read() != self.timing
                || direct.0 & 0xFFC0_00CF != self.direct.0
                || !direct.txempty()
                || !direct.rxempty()
            {
                fail_stop();
            }
            if pac::PADS_QSPI.gpio(0).read() != self.pads[0]
                || pac::PADS_QSPI.gpio(1).read() != self.pads[1]
                || pac::PADS_QSPI.gpio(2).read() != self.pads[2]
                || pac::PADS_QSPI.gpio(3).read() != self.pads[3]
                || pac::PADS_QSPI.gpio(4).read() != self.pads[4]
                || pac::PADS_QSPI.gpio(5).read() != self.pads[5]
            {
                fail_stop();
            }
            cortex_m::asm::dsb();
            cortex_m::asm::isb();
            core::sync::atomic::compiler_fence(core::sync::atomic::Ordering::SeqCst);
        }
    }

    #[cfg(feature = "_rp235x")]
    #[inline(always)]
    /// Never returns; SRAM-only WFE leaves recovery to the independently armed watchdog.
    fn fail_stop() -> ! {
        // No panic, logging or flash access. The armed watchdog is the bound.
        loop {
            cortex_m::asm::wfe();
        }
    }

    #[cfg(feature = "_rp235x")]
    /// Direct QMI adapter for the bounded read-only JEDEC transaction.
    struct Probe;
    #[cfg(feature = "_rp235x")]
    impl super::probe::Io for Probe {
        #[inline(always)]
        /// Writes direct-mode control `value` under exclusive XIP-off ownership.
        fn control(
            &mut self,
            value: u32,
        ) {
            pac::QMI.direct_csr().write_value(pac::qmi::regs::DirectCsr(value));
        }

        #[inline(always)]
        /// Returns the current QMI direct-mode status/control word.
        fn status(&mut self) -> u32 { pac::QMI.direct_csr().read().0 }

        #[inline(always)]
        /// Queues eight-bit `byte` in the direct transmit FIFO.
        fn write(
            &mut self,
            byte: u8,
        ) {
            pac::QMI
                .direct_tx()
                .write_value(pac::qmi::regs::DirectTx(u32::from(byte)));
        }

        #[inline(always)]
        /// Returns and removes the low byte of the direct receive FIFO.
        fn read(&mut self) -> u8 { pac::QMI.direct_rx().read().0 as u8 }
    }

    /// Fixed read-only 9F transaction. Resolve ROM functions and copy the boot
    /// setup into SRAM before entering the non-inlined XIP-off function. Returns the 24-bit ID.
    /// # Safety
    /// Caller must quiesce all other XIP users and preserve the CS0-only boot-XIP contract.
    #[cfg(feature = "_rp235x")]
    pub unsafe fn flash_jedec_id() -> u32 {
        let mut boot2 = [0u32; 64];
        core::ptr::copy_nonoverlapping(BOOTRAM_BASE, boot2.as_mut_ptr(), 64);
        let ptrs = flash_function_pointers_with_boot2(false, false, &boot2);
        core::sync::atomic::compiler_fence(core::sync::atomic::Ordering::SeqCst);
        jedec_id_inner(&ptrs)
    }

    #[cfg(feature = "_rp235x")]
    #[inline(never)]
    #[unsafe(link_section = ".data.ram_func")]
    /// Returns the 24-bit JEDEC value using SRAM `ptrs`; timeout/restoration failure parks.
    /// # Safety
    /// Callees/data must remain SRAM/ROM accessible; all other XIP users must be quiescent.
    unsafe fn jedec_id_inner(ptrs: &FlashFunctionPointers<'_>) -> u32 {
        let saved = XipState::capture();
        (ptrs.connect_internal_flash)();
        (ptrs.flash_exit_xip)();
        let id = match super::probe::read_id(&mut Probe, 100_000) {
            Some(id) => id,
            None => fail_stop(),
        };
        (ptrs.flash_flush_cache)();
        (ptrs.flash_enter_cmd_xip)();
        saved.restore();
        id
    }

    #[repr(C)]
    /// RP2040 SSI descriptor whose C field layout is consumed by inline assembly.
    struct FlashCommand {
        /// SRAM command/address bytes; requires nonzero length.
        cmd_addr: *const u8,
        /// Number of bytes emitted before dummy/data reads.
        cmd_addr_len: u32,
        /// Received dummy bytes discarded before output.
        dummy_len: u32,
        /// SRAM output pointer, writable for data_len bytes.
        data: *mut u8,
        /// Positive receive byte count; assembly loop is not zero-safe.
        data_len: u32,
    }

    /// Fills `out` with unique-ID bytes using command 0x4B.
    ///
    /// Not all SPI flashes implement this command, so check the JEDEC
    /// ID before relying on it. The Winbond parts commonly seen on
    /// RP2040 devboards (JEDEC=0xEF7015) support an 8-byte unique ID;
    /// <https://forums.raspberrypi.com/viewtopic.php?t=331949> suggests
    /// that LCSC (Zetta) parts have a 16-byte unique ID (which is
    /// *not* unique in just its first 8 bytes),
    /// JEDEC=0xBA6015. Macronix and Spansion parts do not have a
    /// unique ID.
    ///
    /// The returned bytes are relatively predictable and should be
    /// salted and hashed before use if that is an issue (e.g. for MAC
    /// addresses).
    ///
    /// # Safety
    /// `out` must contain 1..=65532 bytes; assembly RX stores before decrementing its counter,
    /// and the16-bit SSI receive count also includes four dummy bytes.
    ///
    /// Nothing must access flash while this is running.
    /// Usually this means:
    ///   - interrupts must be disabled
    ///   - 2nd core must be running code from RAM or ROM with interrupts disabled
    ///   - DMA must not access flash memory
    ///
    /// Credit: taken from `rp2040-flash` (also licensed Apache+MIT)
    #[cfg(feature = "rp2040")]
    pub unsafe fn flash_unique_id(out: &mut [u8]) {
        let mut boot2 = [0u32; 256 / 4];
        let ptrs = if USE_BOOT2 {
            rom_data::memcpy44(&mut boot2 as *mut _, FLASH_BASE, 256);
            flash_function_pointers_with_boot2(false, false, &boot2)
        } else {
            flash_function_pointers(false, false)
        };

        // 4B - read unique ID
        let cmd = [0x4B];
        read_flash(&cmd[..], 4, out, &ptrs as *const FlashFunctionPointers);
    }

    /// Returns the 24-bit JEDEC identifier using command 0x9F.
    ///
    /// This is the three-byte manufacturer-and-model identifier
    /// commonly used to check before using manufacturer-specific SPI
    /// flash features, e.g. 0xEF7015 for Winbond W25Q16JV.
    ///
    /// # Safety
    ///
    /// Nothing must access flash while this is running.
    /// Usually this means:
    ///   - interrupts must be disabled
    ///   - 2nd core must be running code from RAM or ROM with interrupts disabled
    ///   - DMA must not access flash memory
    ///
    /// Credit: taken from `rp2040-flash` (also licensed Apache+MIT)
    #[cfg(feature = "rp2040")]
    pub unsafe fn flash_jedec_id() -> u32 {
        let mut boot2 = [0u32; 256 / 4];
        let ptrs = if USE_BOOT2 {
            rom_data::memcpy44(&mut boot2 as *mut _, FLASH_BASE, 256);
            flash_function_pointers_with_boot2(false, false, &boot2)
        } else {
            flash_function_pointers(false, false)
        };

        let mut id = [0u8; 4];
        // 9F - read JEDEC ID
        let cmd = [0x9F];
        read_flash(&cmd[..], 0, &mut id[1..4], &ptrs as *const FlashFunctionPointers);
        u32::from_be_bytes(id)
    }

    #[cfg(feature = "rp2040")]
    /// Sends SRAM `cmd_addr`, skips `dummy_len` bytes, fills `out`, then restores XIP via `ptrs`.
    /// # Safety
    /// XIP users must be quiescent; `ptrs` and buffers must be SRAM/ROM resident. Command/output
    /// must be nonempty and total receive count fit SSI CTRLR1. No timeout is implemented.
    unsafe fn read_flash(
        cmd_addr: &[u8],
        dummy_len: u32,
        out: &mut [u8],
        ptrs: *const FlashFunctionPointers,
    ) {
        read_flash_inner(
            FlashCommand {
                cmd_addr: cmd_addr.as_ptr(),
                cmd_addr_len: cmd_addr.len() as u32,
                dummy_len,
                data: out.as_mut_ptr(),
                data_len: out.len() as u32,
            },
            ptrs,
        );
    }

    /// Issue a generic SPI flash read command
    ///
    /// # Arguments
    ///
    /// * `cmd` - `FlashCommand` structure
    /// * `ptrs` - Flash function pointers as per `write_flash_inner`
    ///
    /// # Safety
    /// All pointers/callees must remain SRAM/ROM accessible with other XIP users quiescent.
    /// `cmd` requires nonzero command/data lengths and a receive count fitting SSI CTRLR1.
    ///
    /// Credit: taken from `rp2040-flash` (also licensed Apache+MIT)
    #[inline(never)]
    #[unsafe(link_section = ".data.ram_func")]
    #[cfg(feature = "rp2040")]
    unsafe fn read_flash_inner(
        cmd: FlashCommand,
        ptrs: *const FlashFunctionPointers,
    ) {
        #[cfg(target_arch = "arm")]
        core::arch::asm!(
            "mov r10, r0", // cmd
            "mov r5, r1", // ptrs

            "ldr r4, [r5, #0]",
            "blx r4", // connect_internal_flash()

            "ldr r4, [r5, #4]",
            "blx r4", // flash_exit_xip()


            "movs r4, #0x18",
            "lsls r4, r4, #24", // 0x18000000, SSI, RP2040 datasheet 4.10.13

            // Disable, write 0 to SSIENR
            "movs r0, #0",
            "str r0, [r4, #8]", // SSIENR

            // Write ctrlr0
            "movs r0, #0x3",
            "lsls r0, r0, #8", // TMOD=0x300
            "ldr r1, [r4, #0]", // CTRLR0
            "orrs r1, r0",
            "str r1, [r4, #0]",

            // Write ctrlr1 with len-1
            "mov r3, r10", // cmd
            "ldr r0, [r3, #8]", // dummy_len
            "ldr r1, [r3, #16]", // data_len
            "add r0, r1",
            "subs r0, #1",
            "str r0, [r4, #0x04]", // CTRLR1

            // Enable, write 1 to ssienr
            "movs r0, #1",
            "str r0, [r4, #8]", // SSIENR

            // Write cmd/addr phase to DR
            "mov r2, r4",
            "adds r2, 0x60", // &DR
            "ldr r0, [r3, #0]", // cmd_addr
            "ldr r1, [r3, #4]", // cmd_addr_len
            "3:",
            "ldrb r3, [r0]",
            "strb r3, [r2]", // DR
            "adds r0, #1",
            "subs r1, #1",
            "bne 3b",

            // Skip any dummy cycles
            "mov r3, r10", // cmd
            "ldr r1, [r3, #8]", // dummy_len
            "cmp r1, #0",
            "beq 9f",
            "4:",
            "ldr r3, [r4, #0x28]", // SR
            "movs r2, #0x8",
            "tst r3, r2", // SR.RFNE
            "beq 4b",

            "mov r2, r4",
            "adds r2, 0x60", // &DR
            "ldrb r3, [r2]", // DR
            "subs r1, #1",
            "bne 4b",

            // Read RX fifo
            "9:",
            "mov r2, r10", // cmd
            "ldr r0, [r2, #12]", // data
            "ldr r1, [r2, #16]", // data_len

            "2:",
            "ldr r3, [r4, #0x28]", // SR
            "movs r2, #0x8",
            "tst r3, r2", // SR.RFNE
            "beq 2b",

            "mov r2, r4",
            "adds r2, 0x60", // &DR
            "ldrb r3, [r2]", // DR
            "strb r3, [r0]",
            "adds r0, #1",
            "subs r1, #1",
            "bne 2b",

            // Disable, write 0 to ssienr
            "movs r0, #0",
            "str r0, [r4, #8]", // SSIENR

            // Write 0 to CTRLR1 (returning to its default value)
            //
            // flash_enter_cmd_xip does NOT do this, and everything goes
            // wrong unless we do it here
            "str r0, [r4, #4]", // CTRLR1

            "ldr r4, [r5, #20]",
            "blx r4", // flash_enter_cmd_xip();

            in("r0") &cmd as *const FlashCommand,
            in("r1") ptrs,
            out("r2") _,
            out("r3") _,
            out("r4") _,
            out("r5") _,
            // Registers r8-r10 are used to store values
            // from r0-r2 in registers not clobbered by
            // function calls.
            // The values can't be passed in using r8-r10 directly
            // due to https://github.com/rust-lang/rust/issues/99071
            out("r10") _,
            clobber_abi("C"),
        );
    }
}

/// Runs `operation` after pausing core1, entering a critical section and draining observed XIP DMA.
/// Returns InvalidCore without side effects off core0, otherwise success after resuming core1.
/// Waits have no timeout; panic/nonreturning operation leaves core1 parked.
///
/// # Safety
/// `operation` must enter SRAM/ROM code before disabling XIP and restore it before returning.
/// Caller must exclude new DMA/XIP consumers; the address scan is not a general DMA lock.
/// Call with peer FIFO interrupts serviceable and outside locks needed for peer pause.
pub(crate) unsafe fn in_ram(operation: impl FnOnce()) -> Result<(), Error> {
    // Make sure we're running on CORE0
    let core_id: u32 = pac::SIO.cpuid().read();
    if core_id != 0 {
        return Err(Error::InvalidCore);
    }

    // Make sure CORE1 is paused during the entire duration of the RAM function
    crate::multicore::pause_core1();

    critical_section::with(|_| {
        // Wait for all DMA channels in flash to finish before ram operation
        /// Lower SRAM address; active DMA reads below this boundary are awaited.
        const SRAM_LOWER: u32 = 0x2000_0000;
        for n in 0..crate::dma::CHANNEL_COUNT {
            let ch = crate::pac::DMA.ch(n);
            while ch.read_addr().read() < SRAM_LOWER && ch.ctrl_trig().read().busy() {}
        }
        // Wait for completion of any background reads
        while pac::XIP_CTRL.stream_ctr().read().0 > 0 {}

        // Run our flash operation in RAM
        operation();
    });

    // Resume CORE1 execution
    crate::multicore::resume_core1();
    Ok(())
}

/// Restricts flash ownership to the HAL flash singleton.
trait SealedInstance {}
/// Restricts mode markers to the two implemented driver modes.
trait SealedMode {}

/// Flash instance.
#[allow(private_bounds)]
pub trait Instance: SealedInstance + PeripheralType {}
/// Flash mode.
#[allow(private_bounds)]
pub trait Mode: SealedMode {}

impl SealedInstance for FLASH {}
impl Instance for FLASH {}

/// Implements sealed mode traits for an existing marker type.
macro_rules! impl_mode {
    ($name:ident) => {
        impl SealedMode for $name {}
        impl Mode for $name {}
    };
}

/// Flash blocking mode.
pub struct Blocking;
/// Flash async mode.
pub struct Async;

impl_mode!(Blocking);
impl_mode!(Async);
