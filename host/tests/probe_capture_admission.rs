//! Saved-slot labels must pass the same journal and full payload admission as a load.
use dashboard_core::discovery::capture_ui::Slot;
use dashboard_core::discovery::{capture, capture_store as journal, *};
use embassy_futures::block_on;

/// Production capture policy layered over opaque journal storage.
#[path = "../../firmware/ble-probe/src/capture_admission.rs"]
mod admission;

/// In-memory four-slot NOR model with deterministic read failures and mutation accounting.
#[derive(Clone)]
struct Flash {
    /// Reserved capture range, initialized erased to FF.
    bytes: Vec<u8>,
    /// Attempted bounded reads, including the injected failure.
    reads: usize,
    /// Zero-based read attempt to fail; None disables injection.
    fail_at: Option<usize>,
    /// Completed page programs and sector erases.
    writes: usize,
}
impl Flash {
    /// Returns erased four-slot storage with zero counters and no injected failure.
    fn new() -> Self {
        Self {
            bytes: vec![255; journal::SLOT_BYTES * 4],
            reads: 0,
            fail_at: None,
            writes: 0,
        }
    }

    /// Returns the backing index for absolute flash `offset` and byte `length`.
    /// Panics on underflow, unrepresentable offset or range outside the reserved four-slot span.
    fn index(
        offset: u32,
        length: usize,
    ) -> usize {
        let at = usize::try_from(offset.checked_sub(journal::FLASH_OFFSET).unwrap()).unwrap();
        assert!(at + length <= journal::SLOT_BYTES * 4);
        at
    }
}
impl journal::Storage for Flash {
    /// Injected storage read failure without hardware diagnostics.
    type Error = ();

    /// Reads at most one page from absolute `offset` into `bytes`, counting the attempt.
    /// Returns Err at the configured attempt without changing output; otherwise Ok after copying.
    /// Panics for oversized output or an address outside the modeled region.
    async fn read(
        &mut self,
        offset: u32,
        bytes: &mut [u8],
    ) -> Result<(), ()> {
        assert!(bytes.len() <= journal::PAGE_BYTES);
        let at = Self::index(offset, bytes.len());
        let fail = self.fail_at == Some(self.reads);
        self.reads += 1;
        if fail {
            return Err(());
        }
        bytes.copy_from_slice(&self.bytes[at..at + bytes.len()]);
        Ok(())
    }

    /// Erases one sector at absolute aligned `offset` to FF and increments writes; returns Ok.
    /// Panics on misalignment or a range outside the modeled region.
    async fn erase_sector(
        &mut self,
        offset: u32,
    ) -> Result<(), ()> {
        assert_eq!(offset as usize % journal::SECTOR_BYTES, 0);
        let at = Self::index(offset, journal::SECTOR_BYTES);
        self.bytes[at..at + journal::SECTOR_BYTES].fill(255);
        self.writes += 1;
        Ok(())
    }

    /// Programs `bytes` at absolute aligned `offset` by clearing NOR bits and counts success; returns Ok.
    /// Panics on address/alignment failure or a forbidden zero-to-one bit transition; earlier bytes may change.
    async fn program_page(
        &mut self,
        offset: u32,
        bytes: &[u8; journal::PAGE_BYTES],
    ) -> Result<(), ()> {
        assert_eq!(offset as usize % journal::PAGE_BYTES, 0);
        let at = Self::index(offset, bytes.len());
        for (target, source) in self.bytes[at..at + bytes.len()].iter_mut().zip(bytes) {
            assert_eq!(*target & source, *source);
            *target &= source;
        }
        self.writes += 1;
        Ok(())
    }
}

/// Returns encoded selected-peer evidence for display-order `address`, address-type `random` and AD `name` bytes.
/// Uses a fixed stock build and capture time 12 ms; panics on overlong AD length, selection or encoding failure.
fn evidence(
    address: [u8; 6],
    random: bool,
    name: &[u8],
) -> Vec<u8> {
    let mut probe = Probe::new();
    let mut packet = vec![u8::try_from(name.len() + 1).unwrap(), 9];
    packet.extend_from_slice(name);
    probe.catalog.observe(Advertisement::new(
        Peer { address, random },
        ReportKind::Connectable,
        -50,
        &packet,
        10,
    ));
    probe.input(Input::Click, 11).unwrap();
    let mut bytes = vec![0; journal::PAYLOAD_MAX];
    let length = capture::encode(
        &probe,
        capture::BuildIdentity {
            source_digest: [1; 32],
            source_identity_available: true,
            cpu_hz: 150_000_000,
            spi_hz: 37_500_000,
            profile: capture::BuildProfile::Stock,
        },
        12,
        &mut bytes,
    )
    .unwrap();
    bytes.truncate(length);
    bytes
}

/// Returns owned capture `bytes` with version set to u16::MAX, leaving checksums unchanged.
/// Panics for fewer than ten bytes; version rejection intentionally precedes header CRC validation.
fn future(mut bytes: Vec<u8>) -> Vec<u8> {
    bytes[8..10].copy_from_slice(&u16::MAX.to_le_bytes());
    bytes
}

/// Returns capture `bytes` with zero CPU Hz and recomputed body/header CRCs.
/// Panics on short input or if header admission/full semantic rejection differs from the intended fixture.
fn malformed_body(mut bytes: Vec<u8>) -> Vec<u8> {
    // CPU Hz follows the frozen 24-byte header and 32-byte source digest.
    // A zero clock passes checksums/header admission but fails semantic decoding.
    bytes[56..60].fill(0);
    let crc = journal::crc32(&bytes[24..]);
    bytes[16..20].copy_from_slice(&crc.to_le_bytes());
    let crc = journal::crc32(&bytes[..20]);
    bytes[20..24].copy_from_slice(&crc.to_le_bytes());
    capture::check_header(&bytes[..24], bytes.len()).unwrap();
    assert!(matches!(
        capture::Decoder::new().decode(&bytes),
        Err(capture::CaptureError::InvalidData)
    ));
    bytes
}

/// Directory reads reuse scratch/decoder without leaking another slot's name and perform no writes.
#[test]
fn directory_reuses_one_buffer_without_borrowing_identity_from_another_slot() {
    block_on(async {
        let mut flash = Flash::new();
        let mut decoder = capture::Decoder::new();
        let mut bytes = [0; journal::PAYLOAD_MAX];
        let first = evidence([0xC0, 0x12, 0x34, 0x56, 0x78, 0x9A], true, b"NEEWER-TL21C");
        admission::save(&mut flash, &mut decoder, 0, &first, false)
            .await
            .unwrap();
        journal::save_slot(&mut flash, 2, &malformed_body(evidence([2; 6], false, b"Bad")))
            .await
            .unwrap();
        admission::save(&mut flash, &mut decoder, 3, &evidence([4; 6], false, b""), false)
            .await
            .unwrap();
        let before = flash.bytes.clone();
        let writes = flash.writes;

        let (slot, label) = admission::directory_entry(&mut flash, &mut decoder, 0, &mut bytes)
            .await
            .unwrap();
        assert_eq!(slot, Slot::Saved { generation: 1 });
        let label = label.unwrap();
        assert_eq!(label.name.as_slice(), b"NEEWER-TL21C");
        assert_eq!(
            label.peer,
            Peer {
                address: [0xC0, 0x12, 0x34, 0x56, 0x78, 0x9A],
                random: true
            }
        );
        assert_eq!(
            admission::directory_entry(&mut flash, &mut decoder, 1, &mut bytes)
                .await
                .unwrap(),
            (Slot::Empty, None)
        );
        assert!(matches!(
            admission::directory_entry(&mut flash, &mut decoder, 2, &mut bytes).await,
            Err(admission::Error::Corrupt)
        ));
        let (slot, label) = admission::directory_entry(&mut flash, &mut decoder, 3, &mut bytes)
            .await
            .unwrap();
        assert_eq!(slot, Slot::Saved { generation: 1 });
        let label = label.unwrap();
        assert_eq!(label.name.as_slice(), b"");
        assert_eq!(
            label.peer,
            Peer {
                address: [4; 6],
                random: false
            }
        );
        assert_eq!(flash.writes, writes);
        assert_eq!(flash.bytes, before);
    });
}

/// A supported copy supplies its own label while an unknown sibling blocks overwrite in either generation order.
#[test]
fn label_uses_selected_supported_generation_when_a_future_sibling_blocks_replacement() {
    block_on(async {
        for future_first in [false, true] {
            let mut flash = Flash::new();
            let mut decoder = capture::Decoder::new();
            let mut bytes = [0; journal::PAYLOAD_MAX];
            let supported = evidence([7; 6], true, b"Supported");
            let unknown = future(evidence([9; 6], false, b"Must not appear"));
            for payload in if future_first {
                [&unknown, &supported]
            } else {
                [&supported, &unknown]
            } {
                journal::save_slot(&mut flash, 1, payload).await.unwrap();
            }
            let before = flash.bytes.clone();
            let writes = flash.writes;
            let (slot, label) = admission::directory_entry(&mut flash, &mut decoder, 1, &mut bytes)
                .await
                .unwrap();
            assert_eq!(
                slot,
                Slot::SavedBlocked {
                    generation: if future_first { 2 } else { 1 }
                }
            );
            let label = label.unwrap();
            assert_eq!(label.name.as_slice(), b"Supported");
            assert_eq!(
                label.peer,
                Peer {
                    address: [7; 6],
                    random: true
                }
            );
            assert!(matches!(
                admission::save(&mut flash, &mut decoder, 1, &supported, true).await,
                Err(admission::Error::Unsupported)
            ));
            assert_eq!(flash.writes, writes);
            assert_eq!(flash.bytes, before);
            let loaded = admission::load(&mut flash, 1, &mut bytes).await.unwrap();
            assert_eq!(&bytes[..loaded.length as usize], &supported);
        }
    });
}

/// Future, semantically corrupt, CRC-damaged and out-of-range slots cannot publish a stale decoder identity.
#[test]
fn no_label_is_published_for_future_corrupt_or_unreadable_copies() {
    block_on(async {
        let mut flash = Flash::new();
        let good = evidence([1; 6], true, b"Earlier label");
        let mut decoder = capture::Decoder::new();
        decoder.decode(&good).unwrap();
        let mut bytes = [0; journal::PAYLOAD_MAX];
        journal::save_slot(&mut flash, 0, &future(good.clone())).await.unwrap();
        journal::save_slot(&mut flash, 1, &malformed_body(good.clone()))
            .await
            .unwrap();
        journal::save_slot(&mut flash, 2, &good).await.unwrap();
        flash.bytes[journal::SLOT_BYTES * 2 + journal::PAGE_BYTES + 30] ^= 1;
        assert_eq!(
            admission::directory_entry(&mut flash, &mut decoder, 0, &mut bytes)
                .await
                .unwrap(),
            (Slot::Unsupported, None)
        );
        assert!(matches!(
            admission::directory_entry(&mut flash, &mut decoder, 1, &mut bytes).await,
            Err(admission::Error::Corrupt)
        ));
        assert_eq!(
            admission::directory_entry(&mut flash, &mut decoder, 2, &mut bytes)
                .await
                .unwrap(),
            (Slot::Corrupt, None)
        );
        assert!(
            admission::directory_entry(&mut flash, &mut decoder, 4, &mut bytes)
                .await
                .is_err()
        );
    });
}

/// Failure at every directory read boundary returns no identity and leaves all flash bytes/writes unchanged.
#[test]
fn each_directory_read_failure_returns_no_identity_and_performs_no_writes() {
    block_on(async {
        let mut initial = Flash::new();
        journal::save_slot(&mut initial, 0, &evidence([1; 6], true, b"Older"))
            .await
            .unwrap();
        journal::save_slot(&mut initial, 0, &evidence([2; 6], false, b"Latest"))
            .await
            .unwrap();
        initial.reads = 0;
        let mut counted = initial.clone();
        let mut decoder = capture::Decoder::new();
        let mut bytes = [0; journal::PAYLOAD_MAX];
        let (_, label) = admission::directory_entry(&mut counted, &mut decoder, 0, &mut bytes)
            .await
            .unwrap();
        assert_eq!(label.unwrap().name.as_slice(), b"Latest");
        for cut in 0..counted.reads {
            let mut flash = initial.clone();
            flash.fail_at = Some(cut);
            assert!(
                admission::directory_entry(&mut flash, &mut decoder, 0, &mut bytes)
                    .await
                    .is_err(),
                "read {cut}"
            );
            assert_eq!(flash.writes, initial.writes, "read {cut}");
            assert_eq!(flash.bytes, initial.bytes, "read {cut}");
        }
    });
}

/// Identity changes only after admitted overwrite commits a newer payload; reset returns an empty unlabeled slot.
#[test]
fn reset_and_save_replace_identity_only_from_new_committed_payload() {
    block_on(async {
        let mut flash = Flash::new();
        let mut decoder = capture::Decoder::new();
        let mut bytes = [0; journal::PAYLOAD_MAX];
        let old = evidence([1; 6], true, b"Old");
        let new = evidence([2; 6], false, b"New");
        admission::save(&mut flash, &mut decoder, 0, &old, false).await.unwrap();
        assert!(matches!(
            admission::save(&mut flash, &mut decoder, 0, &new, false).await,
            Err(admission::Error::Changed)
        ));
        assert_eq!(
            admission::directory_entry(&mut flash, &mut decoder, 0, &mut bytes)
                .await
                .unwrap()
                .1
                .unwrap()
                .name
                .as_slice(),
            b"Old"
        );
        admission::save(&mut flash, &mut decoder, 0, &new, true).await.unwrap();
        let (slot, label) = admission::directory_entry(&mut flash, &mut decoder, 0, &mut bytes)
            .await
            .unwrap();
        assert_eq!(slot, Slot::Saved { generation: 2 });
        assert_eq!(label.unwrap().name.as_slice(), b"New");
        journal::reset_slot(&mut flash, 0).await.unwrap();
        assert_eq!(
            admission::directory_entry(&mut flash, &mut decoder, 0, &mut bytes)
                .await
                .unwrap(),
            (Slot::Empty, None)
        );
    });
}
