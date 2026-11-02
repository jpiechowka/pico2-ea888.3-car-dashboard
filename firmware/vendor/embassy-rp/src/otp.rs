//! RP2350 OTP read/program access. Programming is irreversible and subject to boot-ROM access policy.
//! See [SDK boot-ROM API](https://github.com/raspberrypi/pico-sdk/blob/master/src/rp2_common/pico_bootrom/include/pico/bootrom.h).
//! All safe access uses nonblocking boot lock 2, shared with the Pico SDK. Contention
//! returns [`Error::Busy`] without accessing OTP. Unsafe raw ROM/PAC callers must
//! honor the same exclusion; programming temporarily disables the direct DATA port.

// Credit: taken from `rp-hal` (also licensed Apache+MIT)
// https://github.com/rp-rs/rp-hal/blob/main/rp235x-hal/src/rom_data.rs

#[cfg(test)]
use tests::otp_access;

#[cfg(not(test))]
use crate::rom_data::otp_access;

/// SDK `BOOTROM_LOCK_OTP`, distinct from the SIO locks used by critical sections.
#[cfg(not(test))]
const OTP_BOOT_LOCK: usize = 2;

/// Exclusive OTP access until scope exit; acquisition never waits or masks interrupts.
struct OtpGuard;

impl OtpGuard {
    /// Returns a new owner, or Busy without OTP effects if another caller owns the lock.
    fn try_acquire() -> Result<Self, Error> {
        #[cfg(not(test))]
        let claimed = crate::pac::BOOTRAM.bootlock(OTP_BOOT_LOCK).read() != 0;
        #[cfg(test)]
        let claimed = tests::try_claim();
        if !claimed {
            return Err(Error::Busy);
        }
        core::sync::atomic::fence(core::sync::atomic::Ordering::Acquire);
        Ok(Self)
    }
}

impl Drop for OtpGuard {
    /// Releases this owner's lock after all access; target abort/reset does not run Drop.
    fn drop(&mut self) {
        core::sync::atomic::fence(core::sync::atomic::Ordering::Release);
        #[cfg(not(test))]
        crate::pac::BOOTRAM.bootlock(OTP_BOOT_LOCK).write_value(0);
        #[cfg(test)]
        tests::release();
    }
}

/// Returns one word read at `address`; tests replace only the hardware boundary.
/// # Safety
/// `address` must designate an accessible, aligned OTP register while the caller owns OtpGuard.
unsafe fn read_word(address: *const u32) -> u32 {
    #[cfg(not(test))]
    unsafe {
        address.read_volatile()
    }
    #[cfg(test)]
    tests::read_word(address)
}

#[cfg(test)]
#[path = "otp_tests.rs"]
mod tests;

/// The ways in which we can fail to access OTP
#[derive(Debug, Clone)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum Error {
    /// Another OTP operation owns the shared boot lock; no OTP read/program was attempted.
    Busy,
    /// The user passed an invalid index to a function.
    InvalidIndex,
    /// The hardware refused to let us read this word, probably due to
    /// read or write lock set earlier in the boot process.
    InvalidPermissions,
    /// Modification is impossible based on current state; e.g.
    /// attempted to clear an OTP bit.
    UnsupportedModification,
    /// Value being written is bigger than 24 bits allowed for raw writes.
    Overflow,
    /// An unexpected failure that contains the exact return code
    UnexpectedFailure(
        /// Unmapped signed boot-ROM return code, preserved exactly.
        i32,
    ),
}

/// OTP read address, using automatic Error Correction.
///
/// A 32-bit read returns the ECC-corrected data for two neighbouring rows, or
/// all-ones on permission failure. Only the first 8 KiB is populated.
pub const OTP_DATA_BASE: *const u32 = 0x4013_0000 as *const u32;

/// OTP read address, without using any automatic Error Correction.
///
/// A 32-bit read returns 24-bits of raw data from the OTP word.
pub const OTP_DATA_RAW_BASE: *const u32 = 0x4013_4000 as *const u32;

/// How many pages in OTP (post error-correction)
pub const NUM_PAGES: usize = 64;

/// How many rows in one page in OTP (post error-correction)
pub const NUM_ROWS_PER_PAGE: usize = 64;

/// How many rows in OTP (post error-correction)
pub const NUM_ROWS: usize = NUM_PAGES * NUM_ROWS_PER_PAGE;

/// 24-bit mask for raw writes
pub const RAW_WRITE_BIT_MASK: u32 = 0x00FF_FFFF;

/// Returns the 16-bit ECC-corrected value at `row` in 0..4096; index/permission/Busy errors propagate.
pub fn read_ecc_word(row: usize) -> Result<u16, Error> {
    if row >= NUM_ROWS {
        return Err(Error::InvalidIndex);
    }
    let _guard = OtpGuard::try_acquire()?;
    // Check permissions under the same lock as the ECC read, without reacquisition.
    let _ = unsafe { read_raw_locked(row) }?;
    // One 32-bit read gets us two rows
    let offset = row >> 1;
    // # Safety
    //
    // The offset is in range and the guard excludes programming for both reads.
    let value = unsafe { read_word(OTP_DATA_BASE.add(offset)) };
    if (row & 1) == 0 {
        Ok(value as u16)
    } else {
        Ok((value >> 16) as u16)
    }
}

/// Returns raw 24-bit OTP contents at `row` in 0..4096; all-ones readback becomes InvalidPermissions.
///
/// You get the 24-bit raw value in the lower part of the 32-bit result.
/// Busy means the shared OTP lock was occupied; no register was read.
pub fn read_raw_word(row: usize) -> Result<u32, Error> {
    if row >= NUM_ROWS {
        return Err(Error::InvalidIndex);
    }
    let _guard = OtpGuard::try_acquire()?;
    // SAFETY: Bounds were checked and the guard excludes programming.
    unsafe { read_raw_locked(row) }
}

/// Returns raw data for `row`, or InvalidPermissions for all-ones readback.
/// # Safety
/// `row` must be below NUM_ROWS and the caller must retain OtpGuard through the read.
unsafe fn read_raw_locked(row: usize) -> Result<u32, Error> {
    let value = unsafe { read_word(OTP_DATA_RAW_BASE.add(row)) };
    if value == 0xFFFF_FFFF {
        Err(Error::InvalidPermissions)
    } else {
        Ok(value)
    }
}
/// Programs 24-bit `data` at `row` in 0..4096; returns Ok on ROM success, or overflow/index/access/ROM error.
/// Only supported one-way bit changes are permitted; errors do not imply reversible or transactional programming.
/// Busy returns before programming; other programming errors can follow irreversible partial effects.
pub fn write_raw_word(
    row: usize,
    data: u32,
) -> Result<(), Error> {
    if data > RAW_WRITE_BIT_MASK {
        return Err(Error::Overflow);
    }
    if row >= NUM_ROWS {
        return Err(Error::InvalidIndex);
    }
    let row_with_write_bit = row | 0x00010000;
    let _guard = OtpGuard::try_acquire()?;
    // A typed local guarantees the ROM's four-byte alignment, including after inlining.
    let mut wire = data.to_le();
    // # Safety
    //
    // Bounds, alignment and length are valid; the guard serializes access until ROM returns.
    let result = unsafe { otp_access(core::ptr::from_mut(&mut wire).cast(), 4, row_with_write_bit as u32) };
    program_result(result)
}

/// Programs 16-bit `data` plus ECC at `row` in 0..4096; returns Ok or index/access/ROM error.
/// Existing programmed bits cannot be cleared; failure does not provide rollback.
/// Busy returns before programming when another operation owns the shared lock.
pub fn write_ecc_word(
    row: usize,
    data: u16,
) -> Result<(), Error> {
    if row >= NUM_ROWS {
        return Err(Error::InvalidIndex);
    }
    let row_with_write_and_ecc_bit = row | 0x00030000;
    let _guard = OtpGuard::try_acquire()?;
    let mut wire = data.to_le();

    // # Safety
    //
    // Bounds, two-byte typed alignment and length are valid; the guard serializes access.
    let result = unsafe {
        otp_access(
            core::ptr::from_mut(&mut wire).cast(),
            2,
            row_with_write_and_ecc_bit as u32,
        )
    };
    program_result(result)
}

/// Returns success or the access/programming error for signed ROM `result`; no rollback is implied.
fn program_result(result: i32) -> Result<(), Error> {
    if result == 0 {
        Ok(())
    } else {
        // 5.4.3. API Function Return Codes
        let error = match result {
            -4 => Error::InvalidPermissions,
            -18 => Error::UnsupportedModification,
            _ => Error::UnexpectedFailure(result),
        };
        Err(error)
    }
}

/// Returns the 64-bit factory chip ID from rows 0..4, least-significant row first, or a read error.
/// Each row is independently locked; Busy propagates and no multirow snapshot is promised.
pub fn get_chipid() -> Result<u64, Error> {
    let w0 = read_ecc_word(0x000)?.to_be_bytes();
    let w1 = read_ecc_word(0x001)?.to_be_bytes();
    let w2 = read_ecc_word(0x002)?.to_be_bytes();
    let w3 = read_ecc_word(0x003)?.to_be_bytes();

    Ok(u64::from_be_bytes([
        w3[0], w3[1], w2[0], w2[1], w1[0], w1[1], w0[0], w0[1],
    ]))
}

/// Returns the 128-bit factory random value from rows 4..12, least-significant row first, or a read error.
/// Each row is independently locked; Busy propagates and no multirow snapshot is promised.
///
/// This ID is not exposed through the USB PICOBOOT GET_INFO command
/// or the ROM get_sys_info() API. However note that the USB PICOBOOT OTP
/// access point can read the entirety of page 0, so this value is not
/// meaningfully private unless the USB PICOBOOT interface is disabled via the
/// DISABLE_BOOTSEL_USB_PICOBOOT_IFC flag in BOOT_FLAGS0
pub fn get_private_random_number() -> Result<u128, Error> {
    let w0 = read_ecc_word(0x004)?.to_be_bytes();
    let w1 = read_ecc_word(0x005)?.to_be_bytes();
    let w2 = read_ecc_word(0x006)?.to_be_bytes();
    let w3 = read_ecc_word(0x007)?.to_be_bytes();
    let w4 = read_ecc_word(0x008)?.to_be_bytes();
    let w5 = read_ecc_word(0x009)?.to_be_bytes();
    let w6 = read_ecc_word(0x00A)?.to_be_bytes();
    let w7 = read_ecc_word(0x00B)?.to_be_bytes();

    Ok(u128::from_be_bytes([
        w7[0], w7[1], w6[0], w6[1], w5[0], w5[1], w4[0], w4[1], w3[0], w3[1], w2[0], w2[1], w1[0], w1[1], w0[0], w0[1],
    ]))
}
