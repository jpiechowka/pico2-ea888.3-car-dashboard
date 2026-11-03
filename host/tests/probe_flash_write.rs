//! Production RAM-write admission boundaries; no forged slices, MMIO or flash operations.

#[path = "../../firmware/vendor/embassy-rp/src/flash_write.rs"]
/// The same pure admission helper used before the HAL enters flash exclusion.
mod flash_write;

use flash_write::{PAGE_BYTES, RamWriteError, check_ram_write};

/// Complete pages and multipage inputs fit the intentionally narrower lower512KiB SRAM range.
#[test]
fn full_pages_require_complete_source_range_but_not_source_alignment() {
    for (offset, length, address) in [
        (0, PAGE_BYTES, 0x2000_0000),
        (256, PAGE_BYTES, 0x2000_0001),
        (0x1F_FF00, PAGE_BYTES, 0x2007_FF00),
        (0, 8192, 0x2007_E000),
        (0, 0x80000, 0x2000_0000),
    ] {
        assert_eq!(check_ram_write(offset, length, address), Ok(()));
    }
}

/// Empty, flash-backed, peripheral, upper-bank, crossing and overflowing source extents reject.
#[test]
fn invalid_source_ranges_and_empty_writes_are_rejected() {
    for (length, address) in [
        (0, 0x2000_0000),
        (PAGE_BYTES, 0x1000_0000),
        (PAGE_BYTES, 0x1FFF_FFFF),
        (PAGE_BYTES, 0x2007_FF01),
        (PAGE_BYTES, 0x2008_0000),
        (PAGE_BYTES, 0x4000_0000),
        (PAGE_BYTES, usize::MAX - 255),
        (0x80100, 0x2000_0000),
        (usize::MAX & !(PAGE_BYTES - 1), 0x2000_0000),
    ] {
        assert_eq!(check_ram_write(0, length, address), Err(RamWriteError::OutOfBounds));
    }
}

/// Partial pages or unaligned destination offsets never reach the no-padding path.
#[test]
fn destination_alignment_and_partial_lengths_are_rejected() {
    for offset in [1, 255, 257, u32::MAX] {
        assert_eq!(
            check_ram_write(offset, PAGE_BYTES, 0x2000_0000),
            Err(RamWriteError::Unaligned)
        );
    }
    for length in [1, 255, 257, 8191, usize::MAX] {
        assert_eq!(check_ram_write(0, length, 0x2000_0000), Err(RamWriteError::Unaligned));
    }
}
