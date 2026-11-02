//! Host-only OTP hardware boundary doubles; no ROM or MMIO operation is executed.
extern crate std;

use core::cell::RefCell;
use std::vec::Vec;

use super::*;

/// Per-test-thread replacement for OTP data, ROM, and lock hardware.
#[derive(Default)]
struct Hardware {
    /// Whether the shared OTP boot lock is held.
    locked: bool,
    /// Counts attempted hardware lock claims.
    claims: usize,
    /// Counts releases of successfully claimed locks.
    releases: usize,
    /// Counts actual raw/ECC register reads.
    reads: usize,
    /// Ordered register addresses requested by the public read paths.
    addresses: Vec<usize>,
    /// Counts admitted ROM invocations.
    programs: usize,
    /// Optional one-shot callback invoked inside simulated programming.
    during_program: Option<fn()>,
    /// Raw permission/value response.
    raw: u32,
    /// Packed two-row ECC response.
    ecc: u32,
    /// Signed ROM result injected by a test.
    result: i32,
    /// Last program bytes, copied from the caller's live buffer.
    bytes: Vec<u8>,
    /// Last program command bits.
    flags: u32,
}

std::thread_local! {
    /// Isolates deterministic hardware state between concurrent host tests.
    static HARDWARE: RefCell<Hardware> = RefCell::new(Hardware::default());
}

/// Resets this test's hardware state to unlocked, successful, zero-valued reads.
fn reset() { HARDWARE.with(|hardware| *hardware.borrow_mut() = Hardware::default()); }

/// Atomically models claim-by-read; returns false without changing an existing owner.
pub(super) fn try_claim() -> bool {
    HARDWARE.with(|hardware| {
        let mut hardware = hardware.borrow_mut();
        hardware.claims += 1;
        if hardware.locked {
            false
        } else {
            hardware.locked = true;
            true
        }
    })
}

/// Models releasing the current owner and counts releases for error/unwind checks.
pub(super) fn release() {
    HARDWARE.with(|hardware| {
        let mut hardware = hardware.borrow_mut();
        assert!(hardware.locked, "released an unowned OTP lock");
        hardware.locked = false;
        hardware.releases += 1;
    });
}

/// Returns injected register data at `address`, recording the real caller's access.
pub(super) fn read_word(address: *const u32) -> u32 {
    HARDWARE.with(|hardware| {
        let mut hardware = hardware.borrow_mut();
        assert!(hardware.locked, "OTP register access escaped the guard");
        hardware.reads += 1;
        hardware.addresses.push(address as usize);
        if address as usize >= OTP_DATA_RAW_BASE as usize {
            hardware.raw
        } else {
            hardware.ecc
        }
    })
}

/// Records `len` bytes and `flags`, runs an optional nested caller, and returns the injected ROM code.
/// # Safety
/// `buffer` must reference at least `len` initialized readable bytes for this call.
pub(super) unsafe fn otp_access(
    buffer: *mut u8,
    len: u32,
    flags: u32,
) -> i32 {
    assert_eq!((buffer as usize) % len as usize, 0, "ROM transfer buffer is misaligned");
    let callback = HARDWARE.with(|hardware| {
        let mut hardware = hardware.borrow_mut();
        assert!(hardware.locked, "OTP programming escaped the guard");
        hardware.programs += 1;
        hardware.bytes = unsafe { core::slice::from_raw_parts(buffer, len as usize) }.to_vec();
        hardware.flags = flags;
        hardware.during_program.take()
    });
    if let Some(callback) = callback {
        callback();
    }
    HARDWARE.with(|hardware| hardware.borrow().result)
}

/// A held OTP lock must reject every public primitive without any read/program effect.
#[test]
fn held_lock_rejects_all_access_without_effects() {
    reset();
    HARDWARE.with(|hardware| hardware.borrow_mut().locked = true);
    assert!(matches!(read_raw_word(0), Err(Error::Busy)));
    assert!(matches!(read_ecc_word(0), Err(Error::Busy)));
    assert!(matches!(write_raw_word(0, 1), Err(Error::Busy)));
    assert!(matches!(write_ecc_word(0, 1), Err(Error::Busy)));
    assert!(matches!(get_chipid(), Err(Error::Busy)));
    assert!(matches!(get_private_random_number(), Err(Error::Busy)));
    HARDWARE.with(|hardware| {
        let hardware = hardware.borrow();
        assert_eq!(hardware.reads, 0);
        assert_eq!(hardware.programs, 0);
        assert!(hardware.locked);
        assert_eq!(hardware.releases, 0);
    });
}

/// Reentrant callers during programming must fail without touching the unavailable DATA port.
#[test]
fn programming_excludes_nested_reads_and_writes() {
    reset();
    HARDWARE.with(|hardware| {
        hardware.borrow_mut().during_program = Some(|| {
            assert!(matches!(read_raw_word(2), Err(Error::Busy)));
            assert!(matches!(read_ecc_word(3), Err(Error::Busy)));
            assert!(matches!(write_raw_word(4, 1), Err(Error::Busy)));
            assert!(matches!(write_ecc_word(5, 1), Err(Error::Busy)));
        })
    });
    write_raw_word(1, 1).unwrap();
    HARDWARE.with(|hardware| {
        let hardware = hardware.borrow();
        assert_eq!(hardware.reads, 0);
        assert_eq!(hardware.programs, 1);
        assert!(!hardware.locked);
    });
    assert_eq!(read_raw_word(1).unwrap(), 0);
}

/// Bound checks remain before hardware access and retain their existing error categories.
#[test]
fn invalid_arguments_have_no_hardware_effects() {
    reset();
    assert!(matches!(read_raw_word(4096), Err(Error::InvalidIndex)));
    assert!(matches!(read_ecc_word(4096), Err(Error::InvalidIndex)));
    assert!(matches!(write_raw_word(4096, 1), Err(Error::InvalidIndex)));
    assert!(matches!(write_ecc_word(4096, 1), Err(Error::InvalidIndex)));
    assert!(matches!(write_raw_word(0, 0x0100_0000), Err(Error::Overflow)));
    HARDWARE.with(|hardware| {
        let hardware = hardware.borrow();
        assert_eq!(hardware.reads, 0);
        assert_eq!(hardware.programs, 0);
        assert_eq!(hardware.claims, 0);
    });
}

/// ECC permission and paired-word reads share one acquisition; denied reads skip the ECC port.
#[test]
fn ecc_permission_probe_and_half_selection_share_one_owner() {
    reset();
    HARDWARE.with(|hardware| hardware.borrow_mut().ecc = 0x1234_ABCD);
    assert_eq!(read_ecc_word(4094).unwrap(), 0xABCD);
    assert_eq!(read_ecc_word(4095).unwrap(), 0x1234);
    HARDWARE.with(|hardware| {
        let mut hardware = hardware.borrow_mut();
        assert_eq!(hardware.claims, 2);
        assert_eq!(hardware.releases, 2);
        assert_eq!(hardware.reads, 4);
        assert_eq!(hardware.addresses, [0x4013_7FF8, 0x4013_1FFC, 0x4013_7FFC, 0x4013_1FFC]);
        hardware.raw = 0xFFFF_FFFF;
    });
    assert!(matches!(read_ecc_word(0), Err(Error::InvalidPermissions)));
    HARDWARE.with(|hardware| {
        let hardware = hardware.borrow();
        assert_eq!(hardware.reads, 5);
        assert_eq!(hardware.releases, 3);
        assert!(!hardware.locked);
    });
}

/// Public raw/ECC programs preserve exact wire bytes, transfer sizes and high-row command bits.
#[test]
fn aligned_program_buffers_preserve_little_endian_wire() {
    reset();
    write_raw_word(4095, 0x00AB_CDEF).unwrap();
    HARDWARE.with(|hardware| {
        let hardware = hardware.borrow();
        assert_eq!(hardware.bytes, [0xEF, 0xCD, 0xAB, 0x00]);
        assert_eq!(hardware.flags, 0x0001_0FFF);
    });
    write_ecc_word(4095, 0x1234).unwrap();
    HARDWARE.with(|hardware| {
        let hardware = hardware.borrow();
        assert_eq!(hardware.bytes, [0x34, 0x12]);
        assert_eq!(hardware.flags, 0x0003_0FFF);
        assert_eq!(hardware.releases, 2);
    });
}

/// Every ROM outcome retains its category/code and releases the lock for a subsequent caller.
#[test]
fn rom_errors_release_and_preserve_mapping() {
    for code in [0, -4, -18, -11, -19, 123] {
        for ecc in [false, true] {
            reset();
            HARDWARE.with(|hardware| hardware.borrow_mut().result = code);
            let result = if ecc {
                write_ecc_word(0, 1)
            } else {
                write_raw_word(0, 1)
            };
            match code {
                0 => assert!(result.is_ok()),
                -4 => assert!(matches!(result, Err(Error::InvalidPermissions))),
                -18 => assert!(matches!(result, Err(Error::UnsupportedModification))),
                other => assert!(matches!(result, Err(Error::UnexpectedFailure(actual)) if actual == other)),
            }
            assert_eq!(read_raw_word(0).unwrap(), 0);
            HARDWARE.with(|hardware| assert_eq!(hardware.borrow().releases, 2));
        }
    }
}

/// Host unwind releases the owner; this does not promise cleanup on target panic-abort/reset.
#[test]
fn host_unwind_releases_program_owner() {
    reset();
    HARDWARE.with(|hardware| hardware.borrow_mut().during_program = Some(|| panic!("injected ROM boundary panic")));
    let result = std::panic::catch_unwind(|| write_ecc_word(0, 1));
    assert!(result.is_err());
    HARDWARE.with(|hardware| {
        let hardware = hardware.borrow();
        assert!(!hardware.locked);
        assert_eq!(hardware.releases, 1);
    });
    assert_eq!(read_raw_word(0).unwrap(), 0);
}
