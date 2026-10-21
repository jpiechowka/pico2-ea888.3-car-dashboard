//! Settings flash address admission against its exact reserved two-sector window.
use dashboard_host_validation::settings::region::{BASE, Range};

/// Exhausts every journal offset for read/program/erase and rejects empty, overflow and escaping ranges.
#[test]
fn every_journal_byte_is_confined_and_program_erase_alignment_is_checked() {
    for offset in 0..=8192 {
        let range = Range::read(offset, 1);
        assert_eq!(range.is_some(), offset < 8192);
        if let Some(range) = range {
            assert_eq!((range.start(), range.end()), (BASE + offset, BASE + offset + 1));
        }
        assert_eq!(Range::program(offset).is_some(), offset <= 7936 && offset % 256 == 0);
        assert_eq!(Range::erase(offset).is_some(), offset == 0 || offset == 4096);
    }
    assert_eq!(Range::read(0, 8192).unwrap().end(), 0x20_0000);
    for (offset, len) in [(0, 0), (8192, 0), (0, 8193), (u32::MAX, 1), (1, usize::MAX)] {
        assert!(Range::read(offset, len).is_none());
    }
    assert!(Range::program(u32::MAX).is_none());
    assert!(Range::erase(u32::MAX).is_none());
}
