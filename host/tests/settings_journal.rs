//! Two-sector settings journal fault injection, commit integrity and unknown-format preservation.
use dashboard_host_validation::render::FpsMode;
use dashboard_host_validation::settings::{self, Error, PAGE_SIZE, SECTOR_SIZE, STORAGE_SIZE, Settings, Storage};

/// Byte-addressable NOR model; partial-prefix/bit tests are software fault models, not electrical guarantees.
#[derive(Clone)]
struct Flash {
    /// Reserved journal bytes, initially erased to 0xFF.
    bytes: [u8; STORAGE_SIZE],
    /// Zero-based next program/erase operation number; reads are counted separately.
    operations: usize,
    /// Optional failing operation and number of leading bytes changed before error.
    cut: Option<(usize, usize)>,
    /// Fail every read before modifying its destination.
    read_error: bool,
    /// Report program success without changing storage, exercising readback verification.
    silent_write: bool,
    /// Report erase success without changing storage, exercising erase verification.
    silent_erase: bool,
    /// Zero-based next read attempt number, including failed reads.
    reads: usize,
    /// Fail exactly this numbered read in addition to the global read-error flag.
    fail_read: Option<usize>,
}
impl Flash {
    /// Returns an erased journal with empty counters and all fault injection disabled.
    fn blank() -> Self {
        Self {
            bytes: [255; STORAGE_SIZE],
            operations: 0,
            cut: None,
            read_error: false,
            silent_write: false,
            silent_erase: false,
            reads: 0,
            fail_read: None,
        }
    }
}
impl Storage for Flash {
    /// Injected storage failure; the selected operation identifies its cause.
    type Error = ();

    /// Copies bytes at journal-relative `offset` into `out`, counting the attempt.
    /// Returns `Err(())` before copying on injected failure, otherwise `Ok(())`;
    /// panics if the production caller supplies an escaping range.
    fn read(
        &mut self,
        offset: u32,
        out: &mut [u8],
    ) -> Result<(), ()> {
        let fail = self.read_error || self.fail_read == Some(self.reads);
        self.reads += 1;
        if fail {
            return Err(());
        }
        out.copy_from_slice(&self.bytes[offset as usize..offset as usize + out.len()]);
        Ok(())
    }

    /// Programs page `data` at journal-relative `offset` using NOR bitwise AND.
    /// Returns an injected error after the configured prefix, or `Ok(())`, which
    /// may deliberately hide a silent write. Counts every attempt; panics for
    /// unaligned/out-of-range or previously programmed pages.
    fn program(
        &mut self,
        offset: u32,
        data: &[u8; PAGE_SIZE],
    ) -> Result<(), ()> {
        assert!((offset as usize).is_multiple_of(PAGE_SIZE));
        let at = offset as usize;
        assert!(
            self.bytes[at..at + PAGE_SIZE].iter().all(|b| *b == 255),
            "page reprogrammed"
        );
        let cut = self.cut.filter(|(op, _)| *op == self.operations).map(|(_, n)| n);
        self.operations += 1;
        if !self.silent_write {
            for (a, b) in self.bytes[at..at + cut.unwrap_or(PAGE_SIZE).min(PAGE_SIZE)]
                .iter_mut()
                .zip(data)
            {
                *a &= *b;
            }
        }
        if cut.is_some() { Err(()) } else { Ok(()) }
    }

    /// Erases one sector at journal-relative `offset`; counts the attempt.
    /// Returns an injected error after its prefix, otherwise `Ok(())` even when
    /// silent erase is enabled. Panics for unaligned or escaping ranges.
    fn erase(
        &mut self,
        offset: u32,
    ) -> Result<(), ()> {
        assert!((offset as usize).is_multiple_of(SECTOR_SIZE));
        let cut = self.cut.filter(|(op, _)| *op == self.operations).map(|(_, n)| n);
        self.operations += 1;
        if !self.silent_erase {
            self.bytes[offset as usize..offset as usize + cut.unwrap_or(SECTOR_SIZE).min(SECTOR_SIZE)].fill(255);
        }
        if cut.is_some() { Err(()) } else { Ok(()) }
    }
}
/// Returns deterministic valid settings from `n`: brightness modulo 101, restore 75%,
/// odd values select PSI and FPS is Combined; every u8 input is admitted.
fn choice(n: u8) -> Settings { Settings::new(n % 101, 75, !n.is_multiple_of(2), FpsMode::Combined).unwrap() }

/// Checks standard CRC vectors, every byte-valued brightness boundary and durable default preferences.
#[test]
fn settings_ranges_default_and_crc_check_vector() {
    assert_eq!(settings::crc32(b"123456789"), 0xCBF43926);
    assert_eq!(settings::crc32(b""), 0);
    for b in 0..=255 {
        assert_eq!(Settings::new(b, 100, false, FpsMode::Off).is_some(), b <= 100);
        assert_eq!(
            Settings::new(0, b, false, FpsMode::Off).is_some(),
            (1..=100).contains(&b)
        );
    }
    assert_eq!(Settings::default().brightness(), 100);
    assert_eq!(Settings::default().restore_brightness(), 100);
    assert!(!Settings::default().boost_psi());
    assert_eq!(Settings::default().fps(), FpsMode::Off);
}

/// Blank load, repeated unchanged saves and multiple sector rollovers preserve monotonic committed generation.
#[test]
fn blank_roundtrip_unchanged_and_multi_sector_rollovers() {
    let mut flash = Flash::blank();
    assert_eq!(settings::load(&mut flash).unwrap().latest, None);
    for n in 1..=45 {
        let record = settings::save(&mut flash, choice(n)).unwrap();
        assert_eq!(record.generation, u64::from(n));
        assert_eq!(settings::load(&mut flash).unwrap().latest, Some(record));
        let operations = flash.operations;
        assert_eq!(settings::save(&mut flash, choice(n)).unwrap(), record);
        assert_eq!(flash.operations, operations, "unchanged settings wrote flash");
    }
}

/// Every data/commit-page cut leaves either the prior record or a fully committed successor and allows later recovery.
#[test]
fn every_page_cut_preserves_previous_or_fully_committed_new_record() {
    let mut base = Flash::blank();
    let previous = settings::save(&mut base, choice(1)).unwrap();
    for operation in 0..2 {
        for byte in 0..=PAGE_SIZE {
            let mut flash = base.clone();
            flash.operations = 0;
            flash.cut = Some((operation, byte));
            assert!(settings::save(&mut flash, choice(2)).is_err());
            let latest = settings::load(&mut flash).unwrap().latest.unwrap();
            assert!(latest == previous || (latest.generation == 2 && latest.settings == choice(2)));
            flash.cut = None;
            let recovered = settings::save(&mut flash, choice(3)).unwrap();
            assert_eq!(recovered.settings, choice(3));
            assert_eq!(recovered.generation, latest.generation + 1);
        }
    }
}

/// Every erase-prefix failure during rollover preserves the newest record in the other sector.
#[test]
fn every_rollover_erase_cut_preserves_the_latest_other_sector() {
    let mut base = Flash::blank();
    let mut previous = None;
    for n in 1..=16 {
        previous = Some(settings::save(&mut base, choice(n)).unwrap());
    }
    // Both banks have data. The seventeenth value must erase only the older one.
    for byte in 0..=SECTOR_SIZE {
        let mut flash = base.clone();
        flash.operations = 0;
        flash.cut = Some((0, byte));
        assert!(settings::save(&mut flash, choice(17)).is_err());
        assert_eq!(settings::load(&mut flash).unwrap().latest, previous);
    }
}

/// Flipping any bit of a newer data/commit pair rejects it and falls back to the prior valid record.
#[test]
fn every_single_corrupt_bit_in_a_record_pair_is_rejected() {
    let mut base = Flash::blank();
    settings::save(&mut base, choice(1)).unwrap();
    let previous = settings::load(&mut base).unwrap().latest;
    settings::save(&mut base, choice(2)).unwrap();
    for byte in 512..1024 {
        for bit in 0..8 {
            let mut flash = base.clone();
            flash.bytes[byte] ^= 1 << bit;
            let scan = settings::load(&mut flash).unwrap();
            assert_eq!(scan.latest, previous, "byte {byte}, bit {bit}");
            assert_eq!(scan.invalid_records, 1);
        }
    }
}

/// Read failure prevents mutation; silent program/erase must fail verification instead of reporting successful
/// persistence.
#[test]
fn read_errors_and_silent_program_or_erase_failures_never_report_saved() {
    let mut flash = Flash::blank();
    flash.read_error = true;
    assert_eq!(settings::load(&mut flash), Err(Error::Storage(())));
    assert_eq!(settings::save(&mut flash, choice(1)), Err(Error::Storage(())));
    assert_eq!(flash.operations, 0);
    flash.read_error = false;
    flash.silent_write = true;
    assert_eq!(settings::save(&mut flash, choice(1)), Err(Error::Readback));
    flash.silent_write = false;
    for n in 1..=16 {
        settings::save(&mut flash, choice(n)).unwrap();
    }
    let previous = settings::load(&mut flash).unwrap().latest;
    flash.silent_erase = true;
    assert_eq!(settings::save(&mut flash, choice(17)), Err(Error::Readback));
    assert_eq!(settings::load(&mut flash).unwrap().latest, previous);
}

/// Duplicate committed generations block writes as ambiguous; a wholly corrupt journal can start a new generation 1.
#[test]
fn duplicated_generation_blocks_mutation_and_all_corrupt_records_fall_back() {
    let mut flash = Flash::blank();
    settings::save(&mut flash, choice(1)).unwrap();
    flash.bytes.copy_within(0..512, 512);
    assert_eq!(settings::load(&mut flash), Err(Error::Ambiguous));
    let operations = flash.operations;
    assert_eq!(settings::save(&mut flash, choice(2)), Err(Error::Ambiguous));
    assert_eq!(flash.operations, operations);
    flash.bytes.fill(0);
    let scan = settings::load(&mut flash).unwrap();
    assert_eq!(scan.latest, None);
    assert_eq!(scan.invalid_records, 16);
    assert_eq!(settings::save(&mut flash, choice(3)).unwrap().generation, 1);
}

/// Repairs the first data/commit envelope in `flash` after deliberate mutation,
/// recomputing CRCs and copying generation/digest bindings so semantic guards
/// are tested independently of accidental CRC failure.
fn repair_envelope(flash: &mut Flash) {
    let crc = settings::crc32(&flash.bytes[..252]);
    flash.bytes[252..256].copy_from_slice(&crc.to_le_bytes());
    flash.bytes.copy_within(8..16, 260);
    flash.bytes[268..272].copy_from_slice(&crc.to_le_bytes());
    let crc = settings::crc32(&flash.bytes[256..272]);
    flash.bytes[272..276].copy_from_slice(&crc.to_le_bytes());
}

/// Structurally intact future schema/length records block writes; maximum generation cannot wrap and zero is invalid.
#[test]
fn unsupported_schema_and_length_block_saves_and_generation_never_wraps() {
    let mut base = Flash::blank();
    settings::save(&mut base, choice(1)).unwrap();
    let mut future = base.clone();
    future.bytes[4..8].copy_from_slice(&[2, 0, 20, 0]);
    future.bytes[24..36].fill(0xA5);
    repair_envelope(&mut future);
    assert_eq!(settings::load(&mut future), Err(Error::Unsupported));
    assert_eq!(settings::save(&mut future, choice(2)), Err(Error::Unsupported));
    assert_eq!(future.operations, base.operations);
    for byte in [4, 5, 6, 7] {
        let mut flash = base.clone();
        flash.bytes[byte] ^= 1;
        repair_envelope(&mut flash);
        assert_eq!(settings::load(&mut flash), Err(Error::Unsupported));
        assert_eq!(settings::save(&mut flash, choice(2)), Err(Error::Unsupported));
        assert_eq!(flash.operations, base.operations);
    }
    let mut flash = base.clone();
    flash.bytes[8..16].copy_from_slice(&u64::MAX.to_le_bytes());
    repair_envelope(&mut flash);
    assert_eq!(settings::save(&mut flash, choice(2)), Err(Error::Exhausted));
    assert_eq!(flash.operations, base.operations);
    flash.bytes[8..16].fill(0);
    repair_envelope(&mut flash);
    assert_eq!(settings::load(&mut flash).unwrap().latest, None);
}

/// Repaired CRCs do not admit invalid setting bytes or a commit page copied from another record.
#[test]
fn intact_crc_cannot_authorize_invalid_values_or_a_foreign_commit() {
    let mut base = Flash::blank();
    settings::save(&mut base, choice(1)).unwrap();
    for (offset, limit) in [(16, 100), (17, 100), (18, 1), (19, 3)] {
        for value in 0..=255 {
            let mut flash = base.clone();
            flash.bytes[offset] = value;
            repair_envelope(&mut flash);
            assert_eq!(
                settings::load(&mut flash).unwrap().latest.is_some(),
                value <= limit && !(offset == 17 && value == 0)
            );
        }
    }
    settings::save(&mut base, choice(2)).unwrap();
    base.bytes.copy_within(256..512, 768);
    assert_eq!(settings::load(&mut base).unwrap().latest.unwrap().generation, 1);
}

/// Every save-path read failure and deterministic torn NOR bit pattern yields only the old or fully committed new
/// record.
#[test]
fn arbitrary_torn_bit_patterns_and_every_read_failure_remain_recoverable() {
    let mut base = Flash::blank();
    let old = settings::save(&mut base, choice(1)).unwrap();
    let mut complete = base.clone();
    complete.reads = 0;
    let new = settings::save(&mut complete, choice(2)).unwrap();
    for read in 0..complete.reads {
        let mut flash = base.clone();
        flash.reads = 0;
        flash.fail_read = Some(read);
        assert_eq!(settings::save(&mut flash, choice(2)), Err(Error::Storage(())));
        flash.fail_read = None;
        let latest = settings::load(&mut flash).unwrap().latest.unwrap();
        assert!(latest == old || latest == new);
    }
    let mut random = 0x1A38_D27Bu32;
    for _ in 0..1024 {
        let mut flash = base.clone();
        for byte in 512..1024 {
            random = random.wrapping_mul(1664525).wrapping_add(1013904223);
            flash.bytes[byte] = complete.bytes[byte] | (random >> 24) as u8;
        }
        let latest = settings::load(&mut flash).unwrap().latest.unwrap();
        assert!(latest == old || latest == new);
    }
}
