//! Raw engine collection capture/export regressions using synthetic peers and no hardware.
use dashboard_core::ble::canyonero::CleanupOutcome;
use dashboard_core::ble::canyonero::collection::{Command, Error, EvidenceKind, ReadStatus, SendState, Submission};
use dashboard_core::ble::canyonero::reads::ReadId;
use dashboard_core::discovery::capture::{self, BuildIdentity, BuildProfile};
use dashboard_core::discovery::collection::Update;
use dashboard_core::discovery::*;

/// Returns synthetic selected-peer inspection with an explicit collection and interrupted setup.
fn fixture() -> Probe {
    let mut p = Probe::new();
    p.catalog.observe(Advertisement::new(
        Peer {
            address: [0xC0, 1, 2, 3, 4, 5],
            random: true,
        },
        ReportKind::Connectable,
        -42,
        &[],
        1,
    ));
    p.input(Input::Click, 2);
    p.request.action = Action::Collect(p.selected_device().unwrap().peer);
    p.identity_mode = true;
    p.collection_enabled = true;
    p.phase = RadioPhase::Discovering;
    p.inspection
        .accept(InspectionItem::Collection(Update::Start { at_ms: 3 }));
    p.inspection.accept(InspectionItem::Collection(Update::Voltage {
        millivolts: 0,
        at_ms: 4,
    }));
    p.inspection.accept(InspectionItem::Collection(Update::End {
        at_ms: 5,
        completed: false,
        fault: Some(Error::Cancelled),
        cleanup: CleanupOutcome::Clean,
        last_submission: None,
    }));
    p
}
/// Returns deterministic synthetic source/clock identity, not physical build qualification.
fn build() -> BuildIdentity {
    BuildIdentity {
        source_digest: [1; 32],
        cpu_hz: 150_000_000,
        spi_hz: 37_500_000,
        profile: BuildProfile::Stock,
        source_identity_available: true,
    }
}

/// Clearing resident storage must erase prior rows/terminal state without changing its address;
/// duplicate starts and pre-start updates must retain the previous ordering rejection policy.
#[test]
fn resident_collection_clear_restart_and_duplicate_ordering() {
    let mut p = ready();
    row(
        &mut p,
        ReadId::ActiveSession,
        7,
        &[0x62, 0xF1, 0x86, 1],
        ReadStatus::Positive,
    );
    update(
        &mut p,
        Update::TerminalHeader {
            kind: EvidenceKind::Envelope,
            length: 3,
        },
    );
    update(
        &mut p,
        Update::TerminalChunk {
            offset: 0,
            bytes: Payload::new(&[0x1F, 0xB2, 0]),
        },
    );
    let resident = p.inspection.collection.as_ref().unwrap() as *const _;
    assert!(!p.inspection.collection.start(90));
    assert_eq!(p.inspection.collection.as_ref().unwrap().summary.started_at_ms, 3);
    p.inspection
        .accept(InspectionItem::Collection(Update::Start { at_ms: 90 }));
    assert_eq!(p.inspection.malformed, 1);
    assert_eq!(p.inspection.collection.as_ref().unwrap().terminal(), &[0x1F, 0xB2, 0]);

    p.inspection.ended_at_ms = Some(99);
    p.inspection.disconnect_reason = Some(0x16);
    p.inspection.omitted = 2;
    p.inspection.stages = [true; 4];
    p.inspection.complete = true;
    p.inspection.cleanup_complete = true;
    p.inspection.clear();
    p.inspection.clear();
    assert!(p.inspection.collection.is_none());
    assert!(p.inspection.collection.as_mut().is_none());
    assert_eq!(p.inspection.started_at_ms, None);
    assert_eq!(p.inspection.ended_at_ms, None);
    assert_eq!(p.inspection.disconnect_reason, None);
    assert_eq!(p.inspection.malformed, 0);
    assert_eq!(p.inspection.omitted, 0);
    assert_eq!(p.inspection.stages, [false; 4]);
    assert!(!p.inspection.complete && !p.inspection.cleanup_complete);
    assert!(p.inspection.identity.is_none() && p.inspection.link.is_none());
    p.inspection.accept(InspectionItem::Collection(Update::Voltage {
        millivolts: 12300,
        at_ms: 100,
    }));
    p.inspection
        .accept(InspectionItem::Collection(Update::Start { at_ms: 100 }));
    assert_eq!(p.inspection.malformed, 2);
    assert!(p.inspection.collection.is_none());
    p.inspection.started_at_ms = Some(100);
    p.inspection
        .accept(InspectionItem::Collection(Update::Start { at_ms: 99 }));
    assert_eq!(p.inspection.malformed, 3);
    assert!(p.inspection.collection.is_none());
    p.inspection
        .accept(InspectionItem::Collection(Update::Start { at_ms: 100 }));
    let fresh = p.inspection.collection.as_ref().unwrap();
    assert_eq!(fresh as *const _, resident);
    assert_eq!(fresh.summary.started_at_ms, 100);
    assert_eq!(fresh.summary.updated_at_ms, 100);
    assert!(fresh.summary.adapter_mv.is_none() && fresh.summary.handle.is_none());
    assert!(fresh.summary.terminal_kind.is_none() && fresh.terminal().is_empty());
    assert!(!fresh.summary.configured && !fresh.summary.completed && !fresh.successful());
    for id in ReadId::ALL {
        let empty = fresh.row(id);
        assert!(empty.raw.is_empty() && empty.status.is_none());
        assert!(empty.attempted_at_ms().is_none() && empty.submitted_at_ms().is_none());
        assert!(empty.completed_at_ms().is_none());
        assert_eq!((empty.pending, empty.declared_len), (0, 0));
    }
}
#[test]
/// Catches silently encoding new collection evidence as legacy INFO-only metadata.
fn interrupted_collection_is_v6_and_retains_its_adapter_voltage() {
    let p = fixture();
    let mut bytes = [0; 8192];
    let len = capture::encode(&p, build(), 10, &mut bytes).unwrap();
    let decoded = capture::decode(&bytes[..len]).unwrap();
    assert_eq!(decoded.metadata().schema_version, 6);
    assert_eq!(decoded.counts(), [0; 4]);
    let summary = decoded.metadata().collection.unwrap();
    assert_eq!(summary.adapter_mv, Some(0));
    assert_eq!(summary.fault, Some(Error::Cancelled));
    assert_eq!(decoded.metadata().collection_row_count, 28);
    assert_eq!(decoded.records().count(), 28);
    assert_eq!(capture::encoded_size(&p, build(), 10).unwrap(), len);
    let json = dashboard_host_validation::probe_captures::decode_json(&bytes[..len]).unwrap();
    assert_eq!(json["collection"]["adapter_supply"]["millivolts"], 0);
    assert_eq!(json["tool_mode"], "ngm-collector");
    assert_eq!(json["policy"], "explicit-fixed-engine-single-DID-raw-collection-v1");
    assert_eq!(json["collection"]["rows"].as_array().unwrap().len(), 28);
    assert!(json["records"].as_array().unwrap().is_empty());
    assert_eq!(json["collection"]["rows"][0]["response_complete"], false);
}

/// Applies `update` to synthetic `p`, asserting lossless ordered publication.
fn update(
    p: &mut Probe,
    update: Update,
) {
    assert!(p.inspection.collection.as_mut().unwrap().apply(update));
}

/// Returns an authorized synthetic collection after adapter setup, without actual I/O.
fn ready() -> Probe {
    let mut p = fixture();
    p.inspection.collection.clear();
    assert!(p.inspection.collection.start(3));
    update(
        &mut p,
        Update::Voltage {
            millivolts: 12000,
            at_ms: 4,
        },
    );
    update(&mut p, Update::Opened { handle: 2, at_ms: 5 });
    update(&mut p, Update::Configured { at_ms: 6 });
    p
}

/// Publishes exact `raw` for `id` at three consecutive timestamps starting at `at`.
/// `p` receives the ordered observation; `status` supplies its asserted protocol outcome.
fn row(
    p: &mut Probe,
    id: ReadId,
    at: u64,
    raw: &[u8],
    status: ReadStatus,
) {
    update(p, Update::Attempt { id, at_ms: at });
    update(p, Update::Submitted { id, at_ms: at + 1 });
    update(
        p,
        Update::RowHeader {
            id,
            at_ms: at + 2,
            status,
            length: raw.len() as u8,
            pending: 3,
        },
    );
    for (index, chunk) in raw.chunks(64).enumerate() {
        update(
            p,
            Update::Chunk {
                id,
                offset: (index * 64) as u8,
                bytes: Payload::new(chunk),
            },
        );
    }
}

/// Returns an admitted synthetic capture at200ms, panicking on any unexpected format rejection.
/// `p` supplies frozen evidence; serializer sizing must equal the returned owned byte length.
fn encoded(p: &Probe) -> Vec<u8> {
    let mut bytes = vec![0; 8192];
    let len = capture::encode(p, build(), 200, &mut bytes).unwrap();
    assert_eq!(capture::encoded_size(p, build(), 200).unwrap(), len);
    bytes.truncate(len);
    bytes
}

#[test]
/// All28 rows and the largest terminal envelope fit; unknown/mismatched widths stay raw.
fn full_manifest_preserves_maximum_raw_and_never_qualifies_unknown_widths() {
    let mut p = ready();
    for id in ReadId::ALL {
        let mut raw = vec![0xFF; 67];
        raw[..3].copy_from_slice(&[0x62, (id.did() >> 8) as u8, id.did() as u8]);
        if id == ReadId::ActiveSession {
            raw.truncate(4);
            raw[3] = 1;
        }
        row(&mut p, id, 7 + id.index() as u64 * 3, &raw, ReadStatus::Positive);
    }
    update(
        &mut p,
        Update::TerminalHeader {
            kind: EvidenceKind::Envelope,
            length: 245,
        },
    );
    for (index, bytes) in [0xAA; 245].chunks(64).enumerate() {
        update(
            &mut p,
            Update::TerminalChunk {
                offset: (index * 64) as u16,
                bytes: Payload::new(bytes),
            },
        );
    }
    update(
        &mut p,
        Update::End {
            at_ms: 100,
            completed: true,
            fault: None,
            cleanup: CleanupOutcome::Clean,
            last_submission: None,
        },
    );
    let bytes = encoded(&p);
    assert!(bytes.len() < 8192);
    let view = capture::decode(&bytes).unwrap();
    assert_eq!(view.records().count(), 29);
    assert!(view.metadata().collection.unwrap().completed);
    assert!(p.inspection.collection.as_ref().unwrap().successful());
    let json = dashboard_host_validation::probe_captures::decode_json(&bytes).unwrap();
    for value in json["collection"]["rows"].as_array().unwrap() {
        assert_eq!(value["response_complete"], true);
        assert!(value["candidate"].is_null());
        assert_eq!(value["live_measurement"], false);
    }
    assert_eq!(json["collection"]["terminal_prefix"]["retained_prefix_bytes"], 245);
    let mut short = vec![0; bytes.len() - 1];
    assert!(capture::encode(&p, build(), 200, &mut short).is_err());
    let evidence = p.inspection.collection.as_mut().unwrap();
    evidence.summary.cleanup = CleanupOutcome::Failed;
    assert!(!evidence.successful());
    evidence.summary.cleanup = CleanupOutcome::Clean;
    evidence.summary.fault = Some(Error::Timeout);
    assert!(!evidence.successful());
}

#[test]
/// A fully retained negative reply is useful evidence but cannot grant clean live green status.
fn negative_response_does_not_make_a_completed_collection_successful() {
    let mut p = ready();
    for id in ReadId::ALL {
        if id == ReadId::ActiveSession {
            row(&mut p, id, 7, &[0x62, 0xF1, 0x86, 1], ReadStatus::Positive);
        } else {
            row(
                &mut p,
                id,
                7 + id.index() as u64 * 3,
                &[0x7F, 0x22, 0x31],
                ReadStatus::Negative(0x31),
            );
        }
    }
    update(
        &mut p,
        Update::End {
            at_ms: 100,
            completed: true,
            fault: None,
            cleanup: CleanupOutcome::Clean,
            last_submission: None,
        },
    );
    assert!(!p.inspection.collection.as_ref().unwrap().successful());
    let bytes = encoded(&p);
    let json = dashboard_host_validation::probe_captures::decode_json(&bytes).unwrap();
    assert_eq!(json["collection"]["rows"][1]["response_complete"], true);
    assert_eq!(json["collection"]["rows"][1]["raw_hex"], "7f2231");
    assert!(json["collection"]["rows"][1]["candidate"].is_null());
}

#[test]
/// Nondefault session stops later reads; close acknowledgment remains distinct from complete raw publication.
fn nondefault_session_and_terminal_bounds_reject_advancement() {
    let mut p = ready();
    row(
        &mut p,
        ReadId::ActiveSession,
        7,
        &[0x62, 0xF1, 0x86, 3],
        ReadStatus::SessionNotDefault,
    );
    let e = p.inspection.collection.as_mut().unwrap();
    assert!(!e.apply(Update::Attempt {
        id: ReadId::OdxIdentifier,
        at_ms: 10
    }));
    assert!(!e.apply(Update::TerminalHeader {
        kind: EvidenceKind::DiagnosticPayload,
        length: 68
    }));
    assert!(!e.apply(Update::TerminalHeader {
        kind: EvidenceKind::Envelope,
        length: 246
    }));
    assert!(e.apply(Update::TerminalHeader {
        kind: EvidenceKind::Envelope,
        length: 2
    }));
    assert!(!e.apply(Update::TerminalChunk {
        offset: 1,
        bytes: Payload::new(&[1])
    }));
    assert!(e.apply(Update::TerminalChunk {
        offset: 0,
        bytes: Payload::new(&[1])
    }));
    assert!(!e.apply(Update::TerminalChunk {
        offset: 1,
        bytes: Payload::new(&[2, 3])
    }));
    assert!(e.apply(Update::End {
        at_ms: 10,
        completed: true,
        fault: Some(Error::Diagnostic),
        cleanup: CleanupOutcome::Clean,
        last_submission: None
    }));
    assert!(!e.successful());
    assert_eq!(e.terminal(), &[1]);
    let bytes = encoded(&p);
    assert!(
        capture::decode(&bytes)
            .unwrap()
            .metadata()
            .collection
            .unwrap()
            .completed
    );
    assert_eq!(
        capture::decode(&bytes).unwrap().metadata().collection.unwrap().fault,
        Some(Error::Diagnostic)
    );
}

#[test]
/// Interrupted row publication and a partial terminal envelope remain explicitly incomplete.
fn partial_row_and_terminal_prefix_survive_cancellation_without_false_completion() {
    let mut p = ready();
    let id = ReadId::ActiveSession;
    update(&mut p, Update::Attempt { id, at_ms: 7 });
    update(&mut p, Update::Submitted { id, at_ms: 8 });
    update(
        &mut p,
        Update::RowHeader {
            id,
            at_ms: 9,
            status: ReadStatus::Positive,
            length: 4,
            pending: 0,
        },
    );
    update(
        &mut p,
        Update::Chunk {
            id,
            offset: 0,
            bytes: Payload::new(&[0x62, 0xF1]),
        },
    );
    update(
        &mut p,
        Update::TerminalHeader {
            kind: EvidenceKind::Envelope,
            length: 100,
        },
    );
    update(
        &mut p,
        Update::TerminalChunk {
            offset: 0,
            bytes: Payload::new(&[1, 2, 3]),
        },
    );
    update(
        &mut p,
        Update::End {
            at_ms: 10,
            completed: false,
            fault: Some(Error::Cancelled),
            cleanup: CleanupOutcome::TimedOut,
            last_submission: None,
        },
    );
    let bytes = encoded(&p);
    let json = dashboard_host_validation::probe_captures::decode_json(&bytes).unwrap();
    assert_eq!(json["collection"]["rows"][0]["raw_hex"], "62f1");
    assert_eq!(json["collection"]["rows"][0]["response_complete"], false);
    assert!(json["collection"]["rows"][0]["candidate"].is_null());
    assert_eq!(json["collection"]["terminal_prefix"]["prefix_complete"], false);
    assert_eq!(json["collection"]["terminal_prefix"]["raw_hex"], "010203");
}

#[test]
/// Cancellation after submission starts preserves unknown effects rather than inventing no-send or success.
fn attempted_write_and_latest_submission_remain_unknown_after_cleanup() {
    let mut p = ready();
    let id = ReadId::ActiveSession;
    update(&mut p, Update::Attempt { id, at_ms: 7 });
    update(
        &mut p,
        Update::End {
            at_ms: 10,
            completed: false,
            fault: Some(Error::Cancelled),
            cleanup: CleanupOutcome::Clean,
            last_submission: Some(Submission {
                command: Command::Read(id),
                state: SendState::InFlightUnknown,
                attempted_at_ms: Some(7),
                completed_at_ms: None,
            }),
        },
    );
    let bytes = encoded(&p);
    for end in 0..bytes.len() {
        assert!(capture::decode(&bytes[..end]).is_err());
    }
    let json = dashboard_host_validation::probe_captures::decode_json(&bytes).unwrap();
    assert_eq!(json["collection"]["rows"][0]["submission_unknown"], true);
    assert!(json["collection"]["rows"][0]["locally_submitted_at_boot_ms"].is_null());
    assert_eq!(json["collection"]["last_submission"]["state"], "InFlightUnknown");
    assert!(json["collection"]["last_submission"]["completed_at_boot_ms"].is_null());
}

/// Recomputes both checksums after intentional mutation of synthetic capture `bytes`.
fn checksum(bytes: &mut [u8]) {
    let body = capture_store::crc32(&bytes[24..]);
    bytes[16..20].copy_from_slice(&body.to_le_bytes());
    let header = capture_store::crc32(&bytes[..20]);
    bytes[20..24].copy_from_slice(&header.to_le_bytes());
}

/// Reusing one decoder after a partial/invalid summary must clear every collection field on legacy input.
#[test]
fn reused_decoder_drops_partial_collection_before_legacy_capture() {
    let mut p = ready();
    for id in ReadId::ALL {
        row(
            &mut p,
            id,
            7 + id.index() as u64 * 3,
            &[0x62, (id.did() >> 8) as u8, id.did() as u8, 1],
            ReadStatus::Positive,
        );
    }
    update(
        &mut p,
        Update::End {
            at_ms: 100,
            completed: true,
            fault: None,
            cleanup: CleanupOutcome::Clean,
            last_submission: None,
        },
    );
    let original = encoded(&p);
    p.inspection.collection.clear();
    let legacy = encoded(&p);
    let summary = 24 + usize::from(u16::from_le_bytes(legacy[14..16].try_into().unwrap()));
    assert_eq!(&original[summary..summary + 6], &[1, 0xE0, 7, 0xE8, 7, 28]);

    let mut invalid_count = original.clone();
    invalid_count[summary + 5] = 27;
    checksum(&mut invalid_count);
    let mut invalid_field = original.clone();
    // Prefix6 + two times16 + end9 + voltage3 + handle2 precede configured.
    assert_eq!(invalid_field[summary + 36], 1);
    invalid_field[summary + 36] = 2;
    checksum(&mut invalid_field);
    let mut partial = original[..summary + 14].to_vec();
    let total = partial.len() as u16;
    partial[10..12].copy_from_slice(&total.to_le_bytes());
    partial[14..16].copy_from_slice(&(total - 24).to_le_bytes());
    checksum(&mut partial);

    let mut decoder = capture::Decoder::new();
    for (bad, expected) in [
        (&invalid_count, capture::CaptureError::InvalidData),
        (&invalid_field, capture::CaptureError::InvalidData),
        (&partial, capture::CaptureError::Truncated),
    ] {
        let valid = decoder.decode(&original).unwrap();
        assert!(valid.metadata().collection.unwrap().completed);
        assert_eq!(valid.metadata().collection_row_count, 28);
        assert_eq!(
            valid
                .records()
                .filter(|r| matches!(r, capture::Record::Collection(_)))
                .count(),
            28
        );
        assert_eq!(decoder.decode(bad).err(), Some(expected));
        let valid = decoder.decode(&legacy).unwrap();
        assert_eq!(valid.metadata().schema_version, 5);
        assert!(valid.metadata().collection.is_none());
        assert_eq!(valid.metadata().collection_row_count, 0);
        assert!(!valid.records().any(|r| {
            matches!(
                r,
                capture::Record::Collection(_) | capture::Record::CollectionEnvelope(_)
            )
        }));
    }
}

#[test]
/// A valid checksum cannot admit reordered or duplicated canonical manifest identities.
fn malformed_manifest_order_and_unknown_versions_are_rejected() {
    let original = encoded(&fixture());
    let offset = 24 + usize::from(u16::from_le_bytes(original[14..16].try_into().unwrap()));
    assert_eq!(original[offset], 5);
    let mut reordered = original.clone();
    reordered[offset + 3] = 1;
    reordered[offset + 4..offset + 6].copy_from_slice(&ReadId::OdxIdentifier.did().to_le_bytes());
    checksum(&mut reordered);
    assert!(capture::decode(&reordered).is_err());
    let mut unknown = original;
    unknown[8..10].copy_from_slice(&7u16.to_le_bytes());
    checksum(&mut unknown);
    assert!(matches!(capture::decode(&unknown), Err(capture::CaptureError::Version)));
    assert!(matches!(
        capture::decode(&vec![0; 8193]),
        Err(capture::CaptureError::TooLarge)
    ));
}

#[test]
/// Legacy fixtures retain their original formats and never acquire collection evidence.
fn frozen_legacy_formats_have_no_collection_extension() {
    /// Frozen synthetic v5 fixture: legacy v4 record content plus absent identity fields.
    const V5: &str = "424c454341503031050045010400f500226ee3e37dd64340a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a50076b010801d2c0401013c0000000000000002000000010204010203040506000a0000000000000014000000000000000200000001d00302010600010a0000000000000001ce000504094e474d0001140000000000000001d002011c000000000000000300000030750000000000093d0001f7000119000000000000000132000000000000000001010101010000000000000000000103130001010014000102000300000114000500021500012a00000000000000000000000000000000000000000000000000000000800008a02526000000000001070001001400020a18020a000100020003000202292a03090001000300040002012904120001000300292a0000000c000000000300ff58";
    for text in [
        include_str!("fixtures/ble-capture-v1-failure.hex"),
        include_str!("fixtures/ble-capture-v2-failure.hex"),
        include_str!("fixtures/ble-capture-v3-watchdog.hex"),
        include_str!("fixtures/ble-capture-v4-legacy.hex"),
        V5,
    ] {
        let compact: String = text.chars().filter(|c| !c.is_whitespace()).collect();
        let bytes: Vec<u8> = (0..compact.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&compact[i..i + 2], 16).unwrap())
            .collect();
        let view = capture::decode(&bytes).unwrap();
        assert!(view.metadata().collection.is_none());
        assert_eq!(view.metadata().collection_row_count, 0);
    }
    let mut p = fixture();
    p.inspection.collection.clear();
    let bytes = encoded(&p);
    assert_eq!(capture::decode(&bytes).unwrap().metadata().schema_version, 5);
    assert!(
        dashboard_host_validation::probe_captures::decode_json(&bytes)
            .unwrap()
            .get("collection")
            .is_none()
    );
}

/// Adds valid completed INFO bytes and frozen inventory prerequisites to synthetic `p`.
fn info_ready(p: &mut Probe) {
    use dashboard_core::discovery::identity::{ChannelParameters, Evidence as Identity, Update as I};
    let mut evidence = Identity::new(2);
    assert!(evidence.apply(I::Channel(ChannelParameters {
        offered_mtu: 245,
        offered_mps: 247,
        offered_credits: 8,
        local_mtu: Some(245),
        local_mps: Some(247),
        peer_mtu: Some(512),
        peer_mps: Some(247),
        local_cid: None,
        peer_cid: None,
        local_credits: None,
        peer_credits: None,
    })));
    assert!(evidence.apply(I::RequestSubmitted { at_ms: 3 }));
    let raw = b"\x1f\x91\x00\x09v\nm\nh\ns\nf";
    assert!(evidence.apply(I::Sdu {
        length: raw.len() as u16
    }));
    assert!(evidence.apply(I::Chunk {
        offset: 0,
        bytes: Payload::new(raw)
    }));
    p.inspection.identity = Some(evidence);
    p.inspection.stages = [true; 4];
}

#[test]
/// Reservation requires valid completed INFO and refuses a frozen inventory too large for worst-case reads.
fn capacity_preflight_checks_info_and_reserves_all_rows_before_first_command() {
    let mut p = ready();
    assert!(!capture::collection_fits(&p, build(), 200));
    info_ready(&mut p);
    assert!(capture::collection_fits(&p, build(), 200));
    p.inspection.omitted = 1;
    assert!(!capture::collection_fits(&p, build(), 200));
    p.inspection.omitted = 0;
    p.inspection.malformed = 1;
    assert!(!capture::collection_fits(&p, build(), 200));
    p.inspection.malformed = 0;
    p.phase = RadioPhase::Paused;
    assert!(!capture::collection_fits(&p, build(), 200));
    p.phase = RadioPhase::Discovering;
    p.request.action = Action::Collect(Peer {
        address: [9; 6],
        random: false,
    });
    assert!(!capture::collection_fits(&p, build(), 200));
    p.request.action = Action::Collect(p.selected_device().unwrap().peer);
    p.inspection.identity.as_mut().unwrap().request_submitted_at_ms = None;
    assert!(!capture::collection_fits(&p, build(), 200));
    p.inspection.identity.as_mut().unwrap().request_submitted_at_ms = Some(3);
    p.inspection.stages[3] = false;
    assert!(!capture::collection_fits(&p, build(), 200));
    p.inspection.stages[3] = true;
    // Maximum UUID widths remain ordinary bounded inventory; dry-run sizing is shared with encode.
    let uuid = Uuid::from_wire(&[0xAB; 16]).unwrap();
    for i in 0..24 {
        p.inspection
            .services
            .push(Service {
                start: i * 100 + 1,
                end: i * 100 + 99,
                uuid,
            })
            .unwrap();
    }
    for i in 0..64 {
        p.inspection
            .characteristics
            .push(Characteristic {
                service_start: 1,
                declaration: i * 3 + 2,
                handle: i * 3 + 3,
                properties: 2,
                uuid,
            })
            .unwrap();
    }
    for i in 0..128 {
        p.inspection
            .descriptors
            .push(Descriptor {
                service_start: 1,
                characteristic_handle: 3,
                handle: i + 4,
                uuid,
            })
            .unwrap();
    }
    for i in 0..16 {
        p.inspection
            .values
            .push(InfoValue {
                service_start: 1,
                handle: i + 1,
                uuid: 0x2A29,
                value: Payload::new(&[0x41; 64]),
                result: ReadResult::Ok,
                failure: None,
                elapsed_ms: 0,
            })
            .unwrap();
    }
    assert!(!capture::collection_fits(&p, build(), 200));
}
