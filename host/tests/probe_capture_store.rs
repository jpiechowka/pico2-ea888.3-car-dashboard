//! Four-slot journal fault injection and payload admission: committed-generation recovery, unknown-copy protection and
//! explicit reset.

use dashboard_core::discovery::capture_store as journal;
use embassy_futures::block_on;
use journal::*;

/// Reserved-range NOR model with operation-index failures, torn prefixes and silent mutation failures.
#[derive(Clone)]
struct Flash {
    /// Complete four-slot reservation, addressed relative to FLASH_OFFSET.
    bytes: Vec<u8>,
    /// Read/erase/program calls whose failure point has been checked.
    calls: usize,
    /// Zero-based call to fail once; none disables injected failure.
    fail_at: Option<usize>,
    /// Erase/program attempts, including failed or silent ones.
    writes: usize,
    /// Prefix bytes changed by an injected failed erase/program, clamped to operation size.
    tear: usize,
    /// Return successful erase without changing bytes.
    silent_erase: bool,
    /// Return successful program without changing bytes unless the call is the injected failure.
    silent_program: bool,
}
impl Flash {
    /// Returns erased four-slot storage with zero counters and no fault injection.
    fn new() -> Self {
        Self {
            bytes: vec![255; SLOT_BYTES * usize::from(SLOT_COUNT)],
            calls: 0,
            fail_at: None,
            writes: 0,
            tear: 0,
            silent_erase: false,
            silent_program: false,
        }
    }

    /// Returns reservation-relative index for absolute `offset` and `length`; panics below/outside the reservation or
    /// on overflow.
    fn index(
        offset: u32,
        length: usize,
    ) -> usize {
        let index = usize::try_from(offset.checked_sub(FLASH_OFFSET).expect("below reservation")).unwrap();
        assert!(
            index + length <= SLOT_BYTES * usize::from(SLOT_COUNT),
            "outside reservation"
        );
        index
    }

    /// Advances the operation counter and returns whether its previous value matches the injected failure index.
    fn fails(&mut self) -> bool {
        let fails = self.fail_at == Some(self.calls);
        self.calls += 1;
        fails
    }
}
impl Storage for Flash {
    /// Injected storage failure; assertion failures instead identify broken caller range/alignment/NOR contracts.
    type Error = ();

    /// Copies at most one page from absolute `offset` into `bytes`; returns injected failure before mutation.
    /// Invalid size/range panics, and each call advances the operation count.
    async fn read(
        &mut self,
        offset: u32,
        bytes: &mut [u8],
    ) -> Result<(), ()> {
        assert!(bytes.len() <= PAGE_BYTES);
        let i = Self::index(offset, bytes.len());
        if self.fails() {
            return Err(());
        }
        bytes.copy_from_slice(&self.bytes[i..i + bytes.len()]);
        Ok(())
    }

    /// Erases aligned sector `offset`, or its configured torn prefix on failure; returns the injected status.
    /// Counts the write attempt; silent-success mode leaves bytes unchanged, and invalid bounds/alignment panic.
    async fn erase_sector(
        &mut self,
        offset: u32,
    ) -> Result<(), ()> {
        assert_eq!(offset as usize % SECTOR_BYTES, 0);
        let i = Self::index(offset, SECTOR_BYTES);
        self.writes += 1;
        if self.fails() {
            self.bytes[i..i + self.tear.min(SECTOR_BYTES)].fill(255);
            return Err(());
        }
        if !self.silent_erase {
            self.bytes[i..i + SECTOR_BYTES].fill(255);
        }
        Ok(())
    }

    /// AND-programs `bytes` at aligned `offset`, retaining a torn prefix on failure; returns injected status.
    /// Counts attempts; silent-success mode leaves bytes unchanged. Invalid ranges/alignment or zero-to-one programming
    /// panic.
    async fn program_page(
        &mut self,
        offset: u32,
        bytes: &[u8; PAGE_BYTES],
    ) -> Result<(), ()> {
        assert_eq!(offset as usize % PAGE_BYTES, 0);
        let i = Self::index(offset, PAGE_BYTES);
        self.writes += 1;
        let failed = self.fails();
        if self.silent_program && !failed {
            return Ok(());
        }
        let length = if failed { self.tear.min(PAGE_BYTES) } else { PAGE_BYTES };
        for (old, &new) in self.bytes[i..i + length].iter_mut().zip(bytes) {
            assert_eq!(*old & new, new, "program tried to set a zero bit");
            *old &= new;
        }
        if failed { Err(()) } else { Ok(()) }
    }
}

// Keep envelope checksums valid while testing semantic rejection. CRC itself has
// separate published check vectors and a frozen independently generated fixture.
/// Repairs `page` checksum over its protected bytes 0..32 and 36..256; panics if shorter than one page.
fn fix_page_crc(page: &mut [u8]) {
    let mut protected = page[..32].to_vec();
    protected.extend_from_slice(&page[36..256]);
    page[32..36].copy_from_slice(&crc32(&protected).to_le_bytes());
}

/// Rewrites `flash` slot `slot`/`copy` generation to `generation`, repairing header and linked commit CRCs.
/// Bypasses NOR semantics for corruption/admission tests; invalid indices panic.
fn set_generation(
    flash: &mut Flash,
    slot: usize,
    copy: usize,
    generation: u64,
) {
    let start = slot * SLOT_BYTES + copy * COPY_BYTES;
    flash.bytes[start + 16..start + 24].copy_from_slice(&generation.to_le_bytes());
    fix_page_crc(&mut flash.bytes[start..start + PAGE_BYTES]);
    let h = flash.bytes[start + 32..start + 36].to_vec();
    let end = start + COPY_BYTES - PAGE_BYTES;
    flash.bytes[end + 12..end + 20].copy_from_slice(&generation.to_le_bytes());
    flash.bytes[end + 28..end + 32].copy_from_slice(&h);
    fix_page_crc(&mut flash.bytes[end..end + PAGE_BYTES]);
}

/// Every byte mutation in the newest header/payload/commit rejects that copy and loads the previous committed bytes.
#[test]
fn corrupted_newest_header_payload_or_commit_falls_back_to_previous() {
    block_on(async {
        let mut baseline = Flash::new();
        save_slot(&mut baseline, 2, b"old").await.unwrap();
        save_slot(&mut baseline, 2, b"new").await.unwrap();
        let start = 2 * SLOT_BYTES + COPY_BYTES;
        let offsets = (0..PAGE_BYTES)
            .chain(PAGE_BYTES..PAGE_BYTES + 3)
            .chain(COPY_BYTES - PAGE_BYTES..COPY_BYTES);
        for offset in offsets {
            let mut flash = baseline.clone();
            flash.bytes[start + offset] ^= 1;
            let info = inspect_slot(&mut flash, 2).await.unwrap();
            assert_eq!(info.selected.unwrap().generation, 1, "offset {offset}");
            assert!(!matches!(info.copies[1], CopyState::Valid(_)));
            let mut output = [0; 8];
            let saved = load_slot(&mut flash, 2, &mut output).await.unwrap();
            assert_eq!(&output[..saved.length as usize], b"old");
        }
    });
}

/// A valid-CRC future journal schema remains reported and blocks replacement without any byte mutation.
#[test]
fn unknown_schema_is_reported_and_never_overwritten_even_with_valid_other_copy() {
    block_on(async {
        let mut flash = Flash::new();
        save_slot(&mut flash, 0, b"old").await.unwrap();
        save_slot(&mut flash, 0, b"future").await.unwrap();
        flash.bytes[COPY_BYTES + 8..COPY_BYTES + 10].copy_from_slice(&2u16.to_le_bytes());
        fix_page_crc(&mut flash.bytes[COPY_BYTES..COPY_BYTES + PAGE_BYTES]);
        assert_eq!(
            inspect_slot(&mut flash, 0).await.unwrap().copies[1],
            CopyState::UnsupportedSchema(2)
        );
        let before = flash.bytes.clone();
        assert_eq!(
            save_slot(&mut flash, 0, b"replacement").await,
            Err(Error::UnsupportedSchema(2))
        );
        assert_eq!(flash.bytes, before);
    });
}

/// Equal generations are ambiguous; maximum generation stays readable but refuses replacement without mutation.
#[test]
fn equal_generations_and_overflow_are_refused_without_mutation() {
    block_on(async {
        let mut flash = Flash::new();
        save_slot(&mut flash, 0, b"first").await.unwrap();
        save_slot(&mut flash, 0, b"second").await.unwrap();
        set_generation(&mut flash, 0, 0, 2);
        let before = flash.bytes.clone();
        assert_eq!(inspect_slot(&mut flash, 0).await, Err(Error::AmbiguousGeneration));
        assert_eq!(
            load_slot(&mut flash, 0, &mut [0; 20]).await,
            Err(Error::AmbiguousGeneration)
        );
        assert_eq!(save_slot(&mut flash, 0, b"next").await, Err(Error::AmbiguousGeneration));
        assert_eq!(flash.bytes, before);
        set_generation(&mut flash, 0, 1, u64::MAX);
        assert_eq!(
            inspect_slot(&mut flash, 0).await.unwrap().selected.unwrap().generation,
            u64::MAX
        );
        let before = flash.bytes.clone();
        assert_eq!(save_slot(&mut flash, 0, b"next").await, Err(Error::GenerationExhausted));
        assert_eq!(flash.bytes, before);
    });
}

/// Nonerased garbage is Corrupt, not Empty, and neither load nor save silently formats a slot lacking a valid copy.
#[test]
fn garbage_is_not_empty_and_no_valid_copy_never_autoformats() {
    block_on(async {
        let mut flash = Flash::new();
        assert_eq!(load_slot(&mut flash, 0, &mut [0; 10]).await, Err(Error::Empty));
        flash.bytes[PAGE_BYTES + 50] = 0;
        let info = inspect_slot(&mut flash, 0).await.unwrap();
        assert_eq!(info.copies, [CopyState::Corrupt, CopyState::Empty]);
        assert_eq!(load_slot(&mut flash, 0, &mut [0; 10]).await, Err(Error::Corrupt));
        assert_eq!(save_slot(&mut flash, 0, b"new").await, Err(Error::Corrupt));
        assert_eq!(flash.writes, 0);
    });
}

/// Readback verification catches silent erase/program failures and retains the previous committed generation.
#[test]
fn silent_erase_and_program_failures_never_commit_unverified_data() {
    block_on(async {
        let mut baseline = Flash::new();
        save_slot(&mut baseline, 0, b"first").await.unwrap();
        save_slot(&mut baseline, 0, b"second").await.unwrap();
        for bad_erase in [false, true] {
            let mut flash = baseline.clone();
            flash.silent_erase = bad_erase;
            flash.silent_program = !bad_erase;
            assert_eq!(save_slot(&mut flash, 0, b"third").await, Err(Error::VerifyFailed));
            let info = inspect_slot(&mut flash, 0).await.unwrap();
            assert_eq!(info.selected.unwrap().generation, 2);
        }
    });
}

/// CRC-repaired metadata mutations, old commits and relocated copies cannot escape slot/copy/generation/length/payload
/// binding.
#[test]
fn metadata_and_commit_are_bound_to_slot_copy_generation_length_and_payload() {
    block_on(async {
        let mut baseline = Flash::new();
        save_slot(&mut baseline, 0, b"first").await.unwrap();
        save_slot(&mut baseline, 0, b"second").await.unwrap();
        for (offset, bytes) in [
            (12, vec![1]),
            (13, vec![0]),
            (16, vec![0; 8]),
            (24, 8193u32.to_le_bytes().to_vec()),
        ] {
            let mut flash = baseline.clone();
            flash.bytes[COPY_BYTES + offset..COPY_BYTES + offset + bytes.len()].copy_from_slice(&bytes);
            fix_page_crc(&mut flash.bytes[COPY_BYTES..COPY_BYTES + PAGE_BYTES]);
            assert_eq!(
                inspect_slot(&mut flash, 0).await.unwrap().selected.unwrap().generation,
                1
            );
        }
        // A valid old commit is insufficient for the new header and payload.
        let old_commit = baseline.bytes[COPY_BYTES - PAGE_BYTES..COPY_BYTES].to_vec();
        baseline.bytes[SLOT_BYTES - PAGE_BYTES..SLOT_BYTES].copy_from_slice(&old_commit);
        assert_eq!(
            inspect_slot(&mut baseline, 0)
                .await
                .unwrap()
                .selected
                .unwrap()
                .generation,
            1
        );
        // Moving complete copies to another numbered slot must not admit them.
        let misplaced = baseline.bytes[..COPY_BYTES].to_vec();
        baseline.bytes[SLOT_BYTES..SLOT_BYTES + COPY_BYTES].copy_from_slice(&misplaced);
        assert_eq!(inspect_slot(&mut baseline, 1).await.unwrap().selected, None);
    });
}

/// Explicit reset repairs only its selected damaged slot and restarts generation 1; invalid slots issue no writes.
#[test]
fn explicit_reset_recovers_damaged_slot_and_preserves_other_slots() {
    block_on(async {
        let mut flash = Flash::new();
        for slot in 0..SLOT_COUNT {
            save_slot(&mut flash, slot, &[slot + 1; 400]).await.unwrap();
        }
        let before = flash.bytes.clone();
        flash.bytes[SLOT_BYTES + 8] = 2;
        assert_eq!(save_slot(&mut flash, 1, b"new").await, Err(Error::UnsupportedSchema(2)));
        reset_slot(&mut flash, 1).await.unwrap();
        assert_eq!(inspect_slot(&mut flash, 1).await.unwrap().copies, [CopyState::Empty; 2]);
        assert_eq!(&flash.bytes[..SLOT_BYTES], &before[..SLOT_BYTES]);
        assert_eq!(&flash.bytes[2 * SLOT_BYTES..], &before[2 * SLOT_BYTES..]);
        assert_eq!(
            save_slot(&mut flash, 1, b"after explicit reset")
                .await
                .unwrap()
                .generation,
            1
        );
        let writes = flash.writes;
        assert_eq!(reset_slot(&mut flash, 4).await, Err(Error::InvalidSlot));
        assert_eq!(writes, flash.writes);
    });
}

/// Every reset I/O cut/torn erase preserves other slots and needs a fresh explicit reset call to finish.
#[test]
fn interrupted_explicit_reset_is_local_and_repeat_requires_another_call() {
    block_on(async {
        let mut baseline = Flash::new();
        for slot in 0..SLOT_COUNT {
            save_slot(&mut baseline, slot, &[slot + 1; 400]).await.unwrap();
            save_slot(&mut baseline, slot, &[slot + 11; 400]).await.unwrap();
        }
        baseline.calls = 0;
        let mut completed = baseline.clone();
        reset_slot(&mut completed, 2).await.unwrap();
        for cut in 0..completed.calls {
            for tear in [0, 1, 255, 4095, 4096] {
                let mut flash = baseline.clone();
                flash.fail_at = Some(cut);
                flash.tear = tear;
                assert!(reset_slot(&mut flash, 2).await.is_err());
                assert_eq!(&flash.bytes[..2 * SLOT_BYTES], &baseline.bytes[..2 * SLOT_BYTES]);
                assert_eq!(&flash.bytes[3 * SLOT_BYTES..], &baseline.bytes[3 * SLOT_BYTES..]);
                flash.fail_at = None;
                reset_slot(&mut flash, 2).await.unwrap();
                assert_eq!(inspect_slot(&mut flash, 2).await.unwrap().copies, [CopyState::Empty; 2]);
            }
        }
    });
}

/// Header/commit bytes and erased padding match an independently packed Python/zlib fixture.
#[test]
fn canonical_envelopes_match_independent_python_zlib_fixture() {
    block_on(async {
        let mut flash = Flash::new();
        save_slot(&mut flash, 0, b"abc").await.unwrap();
        // Independently generated with Python's zlib.crc32, explicit LE fields.
        let expected_header = [
            0x42, 0x4C, 0x45, 0x43, 0x41, 0x50, 0x4A, 0x31, 0x01, 0x00, 0x00, 0x01, 0x00, 0x00, 0xFF, 0xFF, 0x01, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x03, 0x00, 0x00, 0x00, 0xC2, 0x41, 0x24, 0x35, 0x47, 0xBF, 0xE1, 0xF6,
        ];
        let expected_commit = [
            0x42, 0x4C, 0x45, 0x43, 0x4F, 0x4D, 0x54, 0x31, 0x01, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x03, 0x00, 0x00, 0x00, 0xC2, 0x41, 0x24, 0x35, 0x47, 0xBF, 0xE1, 0xF6, 0xF3, 0xD5, 0xAF, 0xBE,
        ];
        assert_eq!(&flash.bytes[..36], &expected_header);
        assert_eq!(
            &flash.bytes[COPY_BYTES - PAGE_BYTES..COPY_BYTES - PAGE_BYTES + 36],
            &expected_commit
        );
        assert!(flash.bytes[36..PAGE_BYTES].iter().all(|&b| b == 255));
        assert!(
            flash.bytes[COPY_BYTES - PAGE_BYTES + 36..COPY_BYTES]
                .iter()
                .all(|&b| b == 255)
        );
    });
}

/// CRC32 matches the standard ISO-HDLC check vector plus empty and independent sentence vectors.
#[test]
fn crc_matches_published_iso_hdlc_check_and_empty_vector() {
    assert_eq!(crc32(b"123456789"), 0xCBF43926);
    assert_eq!(crc32(b""), 0);
    assert_eq!(crc32(b"The quick brown fox jumps over the lazy dog"), 0x414FA339);
}

/// Four slots independently alternate copies and load the newest exact payload after replacement.
#[test]
fn four_slots_keep_independent_payloads_across_alternating_commits() {
    block_on(async {
        let mut flash = Flash::new();
        for slot in 0..SLOT_COUNT {
            assert_eq!(inspect_slot(&mut flash, slot).await.unwrap().selected, None);
            let initial = save_slot(&mut flash, slot, &[slot; 300]).await.unwrap();
            assert_eq!(
                (initial.slot, initial.copy, initial.generation, initial.length),
                (slot, 0, 1, 300)
            );
            let replacement = save_slot(&mut flash, slot, &[slot + 10; 257]).await.unwrap();
            assert_eq!((replacement.copy, replacement.generation), (1, 2));
        }
        for slot in 0..SLOT_COUNT {
            let mut data = [0; 300];
            let stored = load_slot(&mut flash, slot, &mut data).await.unwrap();
            assert_eq!(&data[..stored.length as usize], &[slot + 10; 257]);
        }
    });
}

/// Exhaustive operation cuts and representative torn prefixes always recover the previous or fully committed new
/// generation.
#[test]
fn every_storage_failure_or_torn_mutation_keeps_a_committed_generation() {
    block_on(async {
        let mut initial = Flash::new();
        save_slot(&mut initial, 1, b"original").await.unwrap();
        save_slot(&mut initial, 1, b"previous").await.unwrap();
        initial.calls = 0;
        let payload = vec![0x5A; 777];
        let mut complete = initial.clone();
        save_slot(&mut complete, 1, &payload).await.unwrap();
        for cut in 0..complete.calls {
            for tear in [0, 1, 127, 256, 4095, 4096] {
                let mut flash = initial.clone();
                flash.fail_at = Some(cut);
                flash.tear = tear;
                assert!(save_slot(&mut flash, 1, &payload).await.is_err(), "cut {cut}");
                flash.fail_at = None;
                let mut data = [0; 1024];
                let selected = load_slot(&mut flash, 1, &mut data).await.unwrap();
                if selected.generation == 2 {
                    assert_eq!(&data[..selected.length as usize], b"previous");
                } else {
                    assert_eq!(selected.generation, 3);
                    assert_eq!(&data[..selected.length as usize], &payload);
                }
            }
        }
    });
}

/// Invalid slots/lengths and short destinations issue no writes; the exact maximum payload round-trips.
#[test]
fn rejects_invalid_slots_lengths_and_short_destination_without_writes() {
    block_on(async {
        let mut flash = Flash::new();
        assert_eq!(save_slot(&mut flash, 4, b"x").await, Err(Error::InvalidSlot));
        assert_eq!(inspect_slot(&mut flash, 255).await, Err(Error::InvalidSlot));
        assert_eq!(save_slot(&mut flash, 0, b"").await, Err(Error::InvalidLength));
        assert_eq!(
            save_slot(&mut flash, 0, &vec![0; PAYLOAD_MAX + 1]).await,
            Err(Error::InvalidLength)
        );
        assert_eq!(flash.writes, 0);
        save_slot(&mut flash, 0, &[42; PAYLOAD_MAX]).await.unwrap();
        let writes = flash.writes;
        assert_eq!(load_slot(&mut flash, 0, &mut [0; 1]).await, Err(Error::BufferTooSmall));
        assert_eq!(flash.writes, writes);
        let mut output = vec![0; PAYLOAD_MAX];
        load_slot(&mut flash, 0, &mut output).await.unwrap();
        assert_eq!(output, vec![42; PAYLOAD_MAX]);
    });
}
#[path = "../../firmware/ble-probe/src/capture_admission.rs"]
mod admission;
use dashboard_core::discovery::{capture, capture_ui as ui, *};
/// Returns a valid pending-inspection capture at 12ms with synthetic stock build identity; fixture admission/capacity
/// failure panics.
fn evidence() -> Vec<u8> {
    let mut p = Probe::new();
    p.catalog.observe(Advertisement::new(
        Peer {
            address: [1; 6],
            random: false,
        },
        ReportKind::Connectable,
        -50,
        &[],
        10,
    ));
    p.input(Input::Click, 11).unwrap();
    let mut out = vec![0; 8192];
    let n = capture::encode(
        &p,
        capture::BuildIdentity {
            source_digest: [1; 32],
            cpu_hz: 150_000_000,
            spi_hz: 75_000_000,
            profile: capture::BuildProfile::Stock,
            source_identity_available: true,
        },
        12,
        &mut out,
    )
    .unwrap();
    out.truncate(n);
    out
}
/// Future payload schema in either copy blocks save, while admission loads the supported sibling independently of raw
/// newest selection.
#[test]
fn future_payload_in_either_copy_blocks_save_but_loads_supported_copy() {
    block_on(async {
        for future_first in [false, true] {
            let mut flash = Flash::new();
            let current = evidence();
            let mut future = current.clone();
            future[8..10].copy_from_slice(&u16::MAX.to_le_bytes());
            save_slot(&mut flash, 0, if future_first { &future } else { &current })
                .await
                .unwrap();
            save_slot(&mut flash, 0, if future_first { &current } else { &future })
                .await
                .unwrap();
            assert_eq!(
                admission::inspect(&mut flash, 0).await.unwrap().slot,
                ui::Slot::SavedBlocked {
                    generation: if future_first { 2 } else { 1 }
                }
            );
            let before = flash.bytes.clone();
            let writes = flash.writes;
            assert!(matches!(
                admission::save(&mut flash, &mut capture::Decoder::new(), 0, &current, true).await,
                Err(admission::Error::Unsupported)
            ));
            assert_eq!(flash.writes, writes);
            assert_eq!(flash.bytes, before);
            let mut out = vec![0; 8192];
            let (slot, identity) = admission::directory_entry(&mut flash, &mut capture::Decoder::new(), 0, &mut out)
                .await
                .unwrap();
            assert!(matches!(slot, ui::Slot::SavedBlocked { .. }));
            assert_eq!(identity.unwrap().peer.address, [1; 6]);
            let meta = admission::load(&mut flash, 0, &mut out).await.unwrap();
            assert_eq!(&out[..meta.length as usize], &current);
            let raw = load_slot(&mut flash, 0, &mut out).await.unwrap();
            assert_eq!(
                &out[..raw.length as usize],
                if future_first { &current } else { &future }
            );
        }
    });
}
/// Future/malformed payloads require explicit reset and cannot be replaced merely by setting overwrite true.
#[test]
fn future_or_malformed_payload_requires_reset_without_mutation() {
    block_on(async {
        let mut future = evidence();
        future[8..10].copy_from_slice(&u16::MAX.to_le_bytes());
        let mut wrong = evidence();
        wrong[0] = 0;
        for bad in [future, wrong, b"BLECAP01".to_vec(), vec![1], evidence()[..23].to_vec()] {
            let mut flash = Flash::new();
            save_slot(&mut flash, 1, &bad).await.unwrap();
            assert!(matches!(
                admission::inspect(&mut flash, 1).await.unwrap().slot,
                ui::Slot::Unsupported | ui::Slot::Corrupt
            ));
            let writes = flash.writes;
            let before = flash.bytes.clone();
            for overwrite in [true, false] {
                assert!(
                    admission::save(&mut flash, &mut capture::Decoder::new(), 1, &evidence(), overwrite)
                        .await
                        .is_err()
                );
            }
            assert_eq!(flash.writes, writes);
            assert_eq!(flash.bytes, before);
            reset_slot(&mut flash, 1).await.unwrap();
            admission::save(&mut flash, &mut capture::Decoder::new(), 1, &evidence(), false)
                .await
                .unwrap();
        }
    });
}
/// Every admission read failure prevents all storage mutation before overwrite can begin.
#[test]
fn payload_admission_read_failures_never_mutate() {
    block_on(async {
        let mut initial = Flash::new();
        save_slot(&mut initial, 0, &evidence()).await.unwrap();
        save_slot(&mut initial, 0, &evidence()).await.unwrap();
        initial.calls = 0;
        let mut counted = initial.clone();
        admission::inspect(&mut counted, 0).await.unwrap();
        for cut in 0..counted.calls {
            let mut flash = initial.clone();
            flash.fail_at = Some(cut);
            let writes = flash.writes;
            assert!(
                admission::save(&mut flash, &mut capture::Decoder::new(), 0, &evidence(), true)
                    .await
                    .is_err(),
                "cut {cut}"
            );
            assert_eq!(flash.writes, writes, "cut {cut}");
            assert_eq!(flash.bytes, initial.bytes);
        }
    });
}
/// Save rechecks emptiness immediately; a now-occupied slot requires explicit overwrite before generation 2 can commit.
#[test]
fn admitted_save_rechecks_empty_and_accepts_explicit_overwrite() {
    block_on(async {
        let mut flash = Flash::new();
        let data = evidence();
        assert_eq!(
            admission::save(&mut flash, &mut capture::Decoder::new(), 3, &data, false)
                .await
                .unwrap()
                .generation,
            1
        );
        let writes = flash.writes;
        assert!(matches!(
            admission::save(&mut flash, &mut capture::Decoder::new(), 3, &data, false).await,
            Err(admission::Error::Changed)
        ));
        assert_eq!(flash.writes, writes);
        assert_eq!(
            admission::save(&mut flash, &mut capture::Decoder::new(), 3, &data, true)
                .await
                .unwrap()
                .generation,
            2
        );
        assert_eq!(
            admission::inspect(&mut flash, 3).await.unwrap().slot,
            ui::Slot::Saved { generation: 2 }
        );
    });
}
/// Copy reads reject forged slot/copy/generation/length metadata and prefixes exceeding one page.
#[test]
fn checked_copy_reads_reject_forged_metadata_and_oversized_prefix() {
    block_on(async {
        let mut flash = Flash::new();
        let meta = save_slot(&mut flash, 0, &evidence()).await.unwrap();
        let mut prefix = [0; 24];
        read_prefix(&mut flash, meta, &mut prefix).await.unwrap();
        capture::check_header(&prefix, meta.length as usize).unwrap();
        for forged in [
            StoredCapture { slot: 4, ..meta },
            StoredCapture { copy: 2, ..meta },
            StoredCapture {
                length: u32::MAX,
                ..meta
            },
            StoredCapture { generation: 2, ..meta },
        ] {
            assert!(read_prefix(&mut flash, forged, &mut prefix).await.is_err());
            assert!(load_copy(&mut flash, forged, &mut [0; 8192]).await.is_err());
        }
        assert!(read_prefix(&mut flash, meta, &mut [0; 257]).await.is_err());
    });
}
/// Malformed inactive payload protects both copies from replacement while the supported current copy remains readable.
#[test]
fn malformed_inactive_payload_blocks_save_without_hiding_current_copy() {
    block_on(async {
        let current = evidence();
        let mut bad_crc = current.clone();
        bad_crc[20] ^= 1;
        let mut bad_length = current.clone();
        bad_length[10..12].copy_from_slice(&24u16.to_le_bytes());
        let crc = crc32(&bad_length[..20]);
        bad_length[20..24].copy_from_slice(&crc.to_le_bytes());
        for bad in [vec![0; 24], current[..23].to_vec(), bad_crc, bad_length] {
            let mut flash = Flash::new();
            save_slot(&mut flash, 0, &bad).await.unwrap();
            save_slot(&mut flash, 0, &current).await.unwrap();
            assert_eq!(
                admission::inspect(&mut flash, 0).await.unwrap().slot,
                ui::Slot::SavedBlocked { generation: 2 }
            );
            let before = flash.bytes.clone();
            let writes = flash.writes;
            assert!(matches!(
                admission::save(&mut flash, &mut capture::Decoder::new(), 0, &current, true).await,
                Err(admission::Error::Corrupt)
            ));
            assert_eq!(flash.writes, writes);
            assert_eq!(flash.bytes, before);
        }
    });
}
