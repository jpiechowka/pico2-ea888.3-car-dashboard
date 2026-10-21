//! Versioned capture codec integrity, bounded evidence, semantic relationships and reusable decoder isolation.

use dashboard_core::discovery::capture::*;
use dashboard_core::discovery::*;
use dashboard_core::health::reset::{BootEvidence, RecoveryCause, ResetRecord, ResetStage};

/// Returns synthetic Expert280 build metadata with a known test digest and 280/70MHz CPU/display clocks.
fn build() -> BuildIdentity {
    BuildIdentity {
        source_digest: [0xA5; 32],
        cpu_hz: 280_000_000,
        spi_hz: 70_000_000,
        profile: BuildProfile::Expert280,
        source_identity_available: true,
    }
}
/// Returns selected-peer evidence with separate advertisement/response times and one row of each GATT kind.
/// Inspection is marked complete/cleaned at 50ms with synthetic link/radio metadata; fixture admission failures panic.
fn probe() -> Probe {
    let mut p = Probe::new();
    p.radio_bus_hz = Some(2_500_000);
    let peer = Peer {
        address: [1, 2, 3, 4, 5, 6],
        random: false,
    };
    p.catalog
        .observe(Advertisement::new(peer, ReportKind::Connectable, -50, &[2, 1, 6], 10));
    p.catalog.observe(Advertisement::new(
        peer,
        ReportKind::ScanResponse,
        -48,
        &[4, 9, b'N', b'G', b'M'],
        20,
    ));
    p.input(Input::Click, 25);
    p.inspection.accept(InspectionItem::Service(Service {
        start: 1,
        end: 20,
        uuid: Uuid::from_wire(&[0x0A, 0x18]).unwrap(),
    }));
    p.inspection.accept(InspectionItem::Characteristic(Characteristic {
        service_start: 1,
        declaration: 2,
        handle: 3,
        properties: 2,
        uuid: Uuid::from_wire(&[0x29, 0x2A]).unwrap(),
    }));
    p.inspection.accept(InspectionItem::Descriptor(Descriptor {
        service_start: 1,
        characteristic_handle: 3,
        handle: 4,
        uuid: Uuid::from_wire(&[1, 0x29]).unwrap(),
    }));
    p.inspection.accept(InspectionItem::Value(InfoValue {
        service_start: 1,
        handle: 3,
        uuid: 0x2A29,
        value: Payload::new(&[0, 255, b'X']),
        result: ReadResult::Ok,
        failure: None,
        elapsed_ms: 12,
    }));
    p.inspection.link = Some(LinkInfo {
        connected_at_ms: 28,
        connect_elapsed_ms: 3,
        interval_us: 30_000,
        latency: 0,
        supervision_timeout_us: 4_000_000,
        att_mtu: Some(247),
    });
    p.inspection.ended_at_ms = Some(50);
    p.inspection.cleanup_complete = true;
    p.inspection.stages = [true; 4];
    p.inspection.complete = true;
    p.phase = RadioPhase::Paused;
    p
}

/// Rejected declaration prefixes preserve original lengths through 65535 bytes; every truncation fails and legacy reuse
/// clears new evidence.
#[test]
fn rejected_evidence_round_trips_and_legacy_decode_clears_it() {
    let legacy = hex(include_str!("fixtures/ble-capture-v1-failure.hex").trim());
    let mut decoder = Decoder::new();
    let mut p = probe();
    p.inspection.complete = false;
    p.error = ProbeError::Discovery;
    let mut failure = InspectionFailure::new(FailureStage::Characteristics, FailureReason::DeclarationLength);
    failure.range = Some((1, 20));
    failure.last_accepted = Some((2, 3));
    failure.elapsed_ms = Some(119);
    for length in [0, 1, 2, 3, 4, 5, 18, 19, 20, 255, 512, 65535] {
        let bytes = vec![0xAB; length];
        let rejected = RejectedDeclaration::from_wire(7, &bytes);
        failure.rejected = Some(rejected);
        p.failure = Some(failure);
        let mut output = [0; 8192];
        let size = encode(&p, build(), 60, &mut output).unwrap();
        let view = decoder.decode(&output[..size]).unwrap();
        assert_eq!(view.metadata().schema_version, 4);
        assert_eq!(view.metadata().failure, Some(failure));
        assert_eq!(rejected.length(), length as u16);
        assert_eq!(rejected.bytes(), &bytes[..length.min(19)]);
        assert_eq!(rejected.value_handle(), (length >= 3).then_some(0xABAB));
        for end in 0..size {
            assert!(decoder.decode(&output[..end]).is_err());
        }
        let view = decoder.decode(&legacy).unwrap();
        assert_eq!(view.metadata().schema_version, 1);
        let old = view.metadata().failure.unwrap();
        assert_eq!(old.reason, FailureReason::DeclarationOrder);
        assert_eq!(old.range, Some((1, 20)));
        assert_eq!(old.last_accepted, Some((2, 3)));
        assert_eq!(old.elapsed_ms, Some(119));
        assert_eq!(old.rejected, None);
        assert_eq!(view.counts(), [1; 4]);
    }
}

/// Export retains actual schema/raw rejected bytes; encoding rejects rejected evidence without
/// characteristic-stage/range context.
#[test]
fn rejected_evidence_export_preserves_raw_fields_and_actual_schema() {
    use dashboard_host_validation::probe_captures::{decode_json, text_report};
    let legacy = hex(include_str!("fixtures/ble-capture-v1-failure.hex").trim());
    let old = decode_json(&legacy).unwrap();
    assert_eq!(old["schema"], 1);
    assert!(old["inspection"]["failure"]["rejected"].is_null());
    let mut p = probe();
    p.inspection.complete = false;
    p.error = ProbeError::Discovery;
    let mut f = InspectionFailure::new(FailureStage::Characteristics, FailureReason::DeclarationOrder);
    f.range = Some((1, 20));
    f.rejected = Some(RejectedDeclaration::from_wire(20, &[2, 21, 0, 0x25, 0x2A]));
    p.failure = Some(f);
    let mut bytes = [0; 8192];
    let len = encode(&p, build(), 60, &mut bytes).unwrap();
    let json = decode_json(&bytes[..len]).unwrap();
    assert_eq!(json["schema"], 4);
    let rejected = &json["inspection"]["failure"]["rejected"];
    assert_eq!(rejected["declaration_handle"], 20);
    assert_eq!(rejected["value_handle"], 21);
    assert_eq!(rejected["raw_prefix_hex"], "021500252a");
    assert_eq!(rejected["prefix_truncated"], false);
    assert!(text_report(&json).contains("inspection.failure.rejected.raw_prefix_hex: \"021500252a\""));
    // CRC-valid evidence still needs an appropriate failure context.
    p.failure.as_mut().unwrap().stage = FailureStage::Mtu;
    assert_eq!(encode(&p, build(), 60, &mut bytes), Err(CaptureError::InvalidState));
    p.failure.as_mut().unwrap().stage = FailureStage::Characteristics;
    p.failure.as_mut().unwrap().range = None;
    assert_eq!(encode(&p, build(), 60, &mut bytes), Err(CaptureError::InvalidState));
}

/// Selected-peer capture retains exact packet timestamps, raw read bytes, build identity, link MTU and operation
/// duration.
#[test]
fn selected_peer_capture_retains_raw_records_build_and_separate_packet_times() {
    let p = probe();
    let mut output = [0; 8192];
    let size = encode(&p, build(), 60, &mut output).unwrap();
    let view = decode(&output[..size]).unwrap();
    assert_eq!(view.metadata().build, build());
    assert_eq!(view.metadata().advertisement.observation.unwrap().at_ms, 10);
    assert_eq!(view.metadata().response.observation.unwrap().at_ms, 20);
    assert_eq!(view.metadata().link.unwrap().att_mtu, Some(247));
    assert_eq!(view.counts(), [1, 1, 1, 1]);
    let records: Vec<_> = view.records().collect();
    assert_eq!(records.len(), 4);
    let Record::Value(v) = records[3] else {
        panic!("missing value");
    };
    assert_eq!(v.bytes, &[0, 255, b'X']);
    assert_eq!(v.elapsed_ms, 12);
}

/// Every truncated prefix/one-byte corruption and trailing data fails; live out-of-service value handles fail
/// relationship admission.
#[test]
fn rejects_every_truncated_prefix_integrity_damage_trailing_and_invalid_live_relations() {
    let mut p = probe();
    let mut output = [0; 8192];
    let size = encode(&p, build(), 60, &mut output).unwrap();
    for end in 0..size {
        assert!(decode(&output[..end]).is_err(), "prefix {end}");
    }
    for at in 0..size {
        let mut damaged = output;
        damaged[at] ^= 1;
        assert!(decode(&damaged[..size]).is_err(), "byte {at}");
    }
    assert!(decode(&output[..size + 1]).is_err());
    p.inspection.characteristics[0].handle = 25;
    assert_eq!(encode(&p, build(), 60, &mut output), Err(CaptureError::Relationships));
}

/// Returns bytes from adjacent hexadecimal pairs in `text`; invalid pairs panic and a trailing odd byte is ignored.
fn hex(text: &str) -> Vec<u8> {
    text.as_bytes()
        .as_chunks::<2>()
        .0
        .iter()
        .map(|c| u8::from_str_radix(core::str::from_utf8(c).unwrap(), 16).unwrap())
        .collect()
}

/// Repairs capture body then header CRC in `bytes` for semantic-corruption tests; fewer than 24 bytes panic.
fn fix_crc(bytes: &mut [u8]) {
    let crc = capture_store::crc32(&bytes[24..]);
    bytes[16..20].copy_from_slice(&crc.to_le_bytes());
    let crc = capture_store::crc32(&bytes[..20]);
    bytes[20..24].copy_from_slice(&crc.to_le_bytes());
}

/// An independently packed Python/zlib passive schema 1 fixture decodes without invented inspection or source identity.
#[test]
fn passive_fixture_from_independent_python_struct_and_zlib_is_readable() {
    let bytes=hex("424c4543415030310100ac00000094006d1fdee578aae4cd000000000000000000000000000000000000000000000000000000000000000080d1f008c06878040000630000000000000001000000000200010203040506000a000000000000000a0000000000000001000000000000010a0000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000");
    let view = decode(&bytes).unwrap();
    let m = view.metadata();
    assert_eq!(m.build.cpu_hz, 150_000_000);
    assert_eq!(m.build.spi_hz, 75_000_000);
    assert!(!m.build.source_identity_available);
    assert_eq!(m.captured_at_ms, 99);
    assert!(!m.gatt_requested);
    assert_eq!(m.peer.address, [1, 2, 3, 4, 5, 6]);
    assert_eq!(view.counts(), [0; 4]);
}

/// Recomputed CRCs cannot admit invalid schema/enums/flags/record tags or invalid service relationships.
#[test]
fn crc_valid_malformed_enums_counts_and_relations_are_rejected() {
    let mut output = [0; 8192];
    let size = encode(&probe(), build(), 60, &mut output).unwrap();
    for (at, value) in [(8, 5), (12, 255), (64, 255), (65, 2), (78, 2), (79, 255), (80, 255)] {
        let mut bytes = output[..size].to_vec();
        bytes[at] = value;
        fix_crc(&mut bytes);
        assert!(decode(&bytes).is_err(), "at {at}");
    }
    let meta = u16::from_le_bytes([output[14], output[15]]) as usize;
    let start = 24 + meta;
    for offset in [3, 4] {
        let mut bytes = output[..size].to_vec();
        bytes[start + offset] = 0;
        fix_crc(&mut bytes);
        if offset == 3 {
            assert!(decode(&bytes).is_err());
        }
    }
    let mut bytes = output[..size].to_vec();
    bytes[start] = 5;
    fix_crc(&mut bytes);
    assert!(decode(&bytes).is_err());
}

/// Wrong-width standard values retain raw evidence with invalid-length status; protected reads require their exact
/// failure code.
#[test]
fn standard_failed_reads_and_invalid_raw_length_remain_explicit() {
    let mut p = probe();
    p.inspection.characteristics[0].uuid = Uuid::from_wire(&[0x23, 0x2A]).unwrap();
    p.inspection.values[0].uuid = 0x2A23;
    let mut output = [0; 8192];
    let size = encode(&p, build(), 60, &mut output).unwrap();
    let view = decode(&output[..size]).unwrap();
    let Record::Value(v) = view.records().last().unwrap() else {
        panic!()
    };
    assert!(!v.length_valid());
    assert_eq!(v.bytes, &[0, 255, b'X']);
    p.inspection.values[0].result = ReadResult::Protected;
    p.inspection.values[0].failure = Some(FailureReason::Att(0x0F));
    p.inspection.values[0].value = Payload::empty();
    let size = encode(&p, build(), 60, &mut output).unwrap();
    let view = decode(&output[..size]).unwrap();
    let Record::Value(v) = view.records().last().unwrap() else {
        panic!()
    };
    assert_eq!(v.result, ReadResult::Protected);
    assert_eq!(v.failure, Some(FailureReason::Att(0x0F)));
    assert_eq!(v.elapsed_ms, 12);
    p.inspection.values[0].failure = None;
    assert!(encode(&p, build(), 60, &mut output).is_err());
}

/// Missing peers, short output buffers, orphan/duplicate GATT rows and contradictory passive state fail without
/// silently dropping evidence.
#[test]
fn invalid_live_relationships_and_small_buffers_never_silently_drop_evidence() {
    let mut output = [0; 8192];
    assert_eq!(
        encode(&Probe::new(), build(), 0, &mut output),
        Err(CaptureError::NoPeer)
    );
    assert_eq!(
        encode(&probe(), build(), 60, &mut [0; 30]),
        Err(CaptureError::BufferTooSmall)
    );
    let mut p = probe();
    p.inspection.descriptors[0].characteristic_handle = 19;
    assert_eq!(encode(&p, build(), 60, &mut output), Err(CaptureError::Relationships));
    let mut p = probe();
    p.inspection.values[0].handle = 4;
    assert_eq!(encode(&p, build(), 60, &mut output), Err(CaptureError::Relationships));
    let mut p = probe();
    let s = p.inspection.services[0];
    p.inspection.services.push(s).unwrap();
    assert_eq!(encode(&p, build(), 60, &mut output), Err(CaptureError::Relationships));
    let mut p = probe();
    p.request.action = Action::Scan(false);
    assert_eq!(encode(&p, build(), 60, &mut output), Err(CaptureError::InvalidState));
}

/// Maximum service/characteristic/descriptor/read/name/failure/reset evidence fits 8192 bytes; a 129th descriptor is
/// rejected after CRC repair.
#[test]
fn all_model_capacities_fit_with_full_width_uuids_and_values() {
    let mut p = probe();
    // Learn the longest retained GAP name through the real admission/end path
    // before replacing the inventory with maximum-width synthetic records.
    p.inspection.services[0].uuid = Uuid::from_wire(&[0, 0x18]).unwrap();
    p.inspection.characteristics[0].uuid = Uuid::from_wire(&[0, 0x2A]).unwrap();
    p.inspection.values[0].uuid = 0x2A00;
    p.inspection.values[0].value = Payload::new(&[b'N'; 64]);
    p.event(Event::InspectionEnd {
        revision: p.request.revision,
        at_ms: 50,
        cleanup_complete: true,
        disconnect_reason: Some(0x16),
    });
    assert_eq!(p.selected_device().unwrap().cached_name.unwrap().data.len, 64);
    p.boot = Some(BootEvidence {
        raw_reason: 1,
        boot_type: Some(0),
        previous: Some(ResetRecord {
            stage: ResetStage::Failure(RecoveryCause::RadioStartup),
            elapsed_ms: u32::MAX,
        }),
    });
    p.inspection.services.clear();
    p.inspection.characteristics.clear();
    p.inspection.descriptors.clear();
    p.inspection.values.clear();
    let wide = |short: u16| {
        let mut b = [0xFB, 0x34, 0x9B, 0x5F, 0x80, 0, 0, 0x80, 0, 0x10, 0, 0, 0, 0, 0, 0];
        b[12..14].copy_from_slice(&short.to_le_bytes());
        Uuid::from_wire(&b).unwrap()
    };
    for index in 0..24 {
        let start = 1 + index * 100;
        p.inspection
            .services
            .push(Service {
                start,
                end: start + 99,
                uuid: wide(0x180A),
            })
            .unwrap();
    }
    for index in 0..64 {
        let start = 1 + (index / 3) * 100;
        let declaration = start + 1 + (index % 3) * 10;
        let handle = declaration + 1;
        p.inspection
            .characteristics
            .push(Characteristic {
                service_start: start,
                declaration,
                handle,
                properties: 2,
                uuid: wide(0x2A29),
            })
            .unwrap();
        for offset in [1, 2] {
            p.inspection
                .descriptors
                .push(Descriptor {
                    service_start: start,
                    characteristic_handle: handle,
                    handle: handle + offset,
                    uuid: wide(0x2901),
                })
                .unwrap();
        }
        if index < 16 {
            p.inspection
                .values
                .push(InfoValue {
                    service_start: start,
                    handle,
                    uuid: 0x2A29,
                    value: Payload::new(&[255; 64]),
                    result: ReadResult::Ok,
                    failure: None,
                    elapsed_ms: u32::MAX,
                })
                .unwrap();
        }
    }
    let mut output = [0; 8192];
    let size = encode(&p, build(), u64::MAX, &mut output).unwrap();
    assert!(size < 8192);
    assert_eq!(decode(&output[..size]).unwrap().counts(), [24, 64, 128, 16]);
    // Insert a 129th descriptor, keeping the envelope CRC and all row tags valid.
    let meta = u16::from_le_bytes([output[14], output[15]]) as usize;
    let descriptor = 24 + meta + 24 * 24 + 64 * 27;
    let end = descriptor + 128 * 26;
    let mut bytes = output[..end].to_vec();
    bytes.extend_from_slice(&output[descriptor..descriptor + 26]);
    bytes.extend_from_slice(&output[end..size]);
    let len = bytes.len() as u16;
    bytes[10..12].copy_from_slice(&len.to_le_bytes());
    bytes[12..14].copy_from_slice(&233u16.to_le_bytes());
    fix_crc(&mut bytes);
    assert!(decode(&bytes).is_err());
    // Maximum retained records must still leave room for the longest failure
    // prefix. This synthetic combination measures the bounded storage envelope.
    p.inspection.complete = false;
    p.error = ProbeError::Discovery;
    let rejected = RejectedDeclaration::from_wire(0xFFFF, &[0xFF; 253]);
    p.failure = Some(InspectionFailure {
        stage: FailureStage::Characteristics,
        reason: FailureReason::DeclarationLength,
        range: Some((1, 0xFFFF)),
        last_accepted: Some((0xFFFC, 0xFFFD)),
        rejected: Some(rejected),
        elapsed_ms: Some(u32::MAX),
    });
    let size = encode(&p, build(), u64::MAX, &mut output).unwrap();
    assert!(size <= 8192);
    let view = decode(&output[..size]).unwrap();
    assert_eq!(view.counts(), [24, 64, 128, 16]);
    assert_eq!(view.metadata().failure.unwrap().rejected, Some(rejected));
    assert_eq!(view.metadata().cached_name.unwrap().data.as_slice(), &[b'N'; 64]);
    assert_eq!(view.metadata().boot, p.boot);
}
/// Ending the decoded view releases its input borrow; failed/reused decoding cannot retain the previous peer, response
/// or GATT rows.
#[test]
fn reusable_decoder_releases_buffer_and_replaces_all_prior_evidence() {
    let mut decoder = Decoder::new();
    let mut bytes = vec![0; 8192];
    let size = decoder.encode(&probe(), build(), 60, &mut bytes).unwrap();
    {
        let view = decoder.decode(&bytes[..size]).unwrap();
        assert_eq!(view.metadata().response.bytes(), &[4, 9, b'N', b'G', b'M']);
        assert_eq!(view.counts(), [1, 1, 1, 1]);
    }
    // Reusing the same mutable buffer must be possible after the view ends.
    bytes[0] ^= 1;
    assert!(decoder.decode(&bytes[..size]).is_err());
    let mut passive = Probe::new();
    passive.catalog.observe(Advertisement::new(
        Peer {
            address: [9; 6],
            random: true,
        },
        ReportKind::Connectable,
        -80,
        &[2, 1, 4],
        100,
    ));
    passive.input(Input::Details, 101);
    let size = decoder.encode(&passive, build(), 110, &mut bytes).unwrap();
    let view = decoder.decode(&bytes[..size]).unwrap();
    assert_eq!(view.metadata().peer.address, [9; 6]);
    assert_eq!(view.metadata().advertisement.bytes(), &[2, 1, 4]);
    assert!(view.metadata().response.bytes().is_empty());
    assert!(view.metadata().response.observation.is_none());
    assert!(view.metadata().link.is_none());
    assert!(view.metadata().failure.is_none());
    assert_eq!(view.counts(), [0; 4]);
    assert_eq!(view.records().count(), 0);
}

/// Alternating current and frozen v1/v2 loads keeps newer latch/radio evidence absent in legacy captures and JSON.
#[test]
fn frozen_v2_and_v1_do_not_inherit_new_fault_evidence() {
    let old2 = hex(include_str!("fixtures/ble-capture-v2-failure.hex").trim());
    let old1 = hex(include_str!("fixtures/ble-capture-v1-failure.hex").trim());
    let mut decoder = Decoder::new();
    let mut p = probe();
    p.fault_latch = FaultLatch::from_parts(Fault::Encoder.bit() | Fault::Scan.bit(), Fault::Encoder as u8).unwrap();
    let mut output = [0; 8192];
    let n = encode(&p, build(), 60, &mut output).unwrap();
    for _ in 0..3 {
        let view = decoder.decode(&output[..n]).unwrap();
        assert_eq!(view.metadata().schema_version, 4);
        assert_eq!(view.metadata().fault_latch, Some(p.fault_latch));
        assert_eq!(view.metadata().radio_bus_hz, Some(2_500_000));
        for (bytes, version) in [(&old2, 2), (&old1, 1)] {
            let view = decoder.decode(bytes).unwrap();
            assert_eq!(view.metadata().schema_version, version);
            assert_eq!(view.metadata().fault_latch, None);
            assert_eq!(view.metadata().radio_bus_hz, None);
            assert_eq!(view.counts(), [1; 4]);
        }
    }
    let json = dashboard_host_validation::probe_captures::decode_json(&output[..n]).unwrap();
    assert_eq!(json["led_latch"]["first"], "encoder I2C");
    assert_eq!(json["led_latch"]["sources"].as_array().unwrap().len(), 2);
    assert_eq!(json["build"]["radio_gspi_hz"], 2_500_000);
    let legacy = dashboard_host_validation::probe_captures::decode_json(&old2).unwrap();
    assert!(legacy["led_latch"].is_null());
    assert!(legacy["build"]["radio_gspi_hz"].is_null());
}

/// CRC-valid unknown latch bits, inconsistent first-fault markers and out-of-range radio clocks are rejected.
#[test]
fn crc_valid_fault_metadata_rejects_unknown_bits_bad_first_and_invalid_bus() {
    let bytes = hex(include_str!("fixtures/ble-capture-v3-watchdog.hex").trim());
    let n = bytes.len();
    let end = 24 + usize::from(u16::from_le_bytes([bytes[14], bytes[15]]));
    // Frozen schema 3's seven-byte append: mask LE u16, first u8, configured bus LE u32.
    for (bits, first, hz) in [
        (0u16, 1u8, 2_500_000u32),
        (1, 0, 2_500_000),
        (1, 2, 2_500_000),
        (0x200, 10, 2_500_000),
        (0, 0, 300_000_000),
        (0, 0, 50_000_001),
    ] {
        let mut damaged = bytes[..n].to_vec();
        damaged[end - 7..end - 5].copy_from_slice(&bits.to_le_bytes());
        damaged[end - 5] = first;
        damaged[end - 4..end].copy_from_slice(&hz.to_le_bytes());
        fix_crc(&mut damaged);
        assert!(decode(&damaged).is_err(), "{bits:x}/{first}/{hz}");
    }
}

/// Missing configured radio clock round-trips as unavailable; an explicit zero clock is invalid data.
#[test]
fn radio_configuration_unknown_is_distinct_from_invalid_zero() {
    let mut p = probe();
    let mut bytes = [0; 8192];
    p.radio_bus_hz = None;
    let n = encode(&p, build(), 60, &mut bytes).unwrap();
    assert_eq!(decode(&bytes[..n]).unwrap().metadata().radio_bus_hz, None);
    p.radio_bus_hz = Some(0);
    assert_eq!(encode(&p, build(), 60, &mut bytes), Err(CaptureError::InvalidData));
}

/// Schema4 names/previous-reset evidence round-trip while schema 3 reuse clears them and preserves its own latch/radio
/// metadata.
#[test]
fn v4_cached_name_and_reset_record_roundtrip_and_legacy_reuse() {
    let mut p = probe();
    p.boot = Some(BootEvidence {
        raw_reason: 1,
        boot_type: Some(0),
        previous: Some(ResetRecord {
            stage: ResetStage::Failure(RecoveryCause::RadioStartup),
            elapsed_ms: 12001,
        }),
    });
    let mut output = [0; 8192];
    let n = encode(&p, build(), 60, &mut output).unwrap();
    let mut decoder = Decoder::new();
    let view = decoder.decode(&output[..n]).unwrap();
    assert_eq!(view.metadata().schema_version, 4);
    assert_eq!(view.metadata().boot, p.boot);
    assert_eq!(view.metadata().cached_name.unwrap().data.as_slice(), b"NGM");
    let old = hex(include_str!("fixtures/ble-capture-v3-watchdog.hex").trim());
    let view = decoder.decode(&old).unwrap();
    assert_eq!(view.metadata().schema_version, 3);
    assert!(view.metadata().cached_name.is_none());
    assert!(view.metadata().boot.is_none());
    assert_eq!(view.metadata().fault_latch.unwrap().first(), Some(Fault::Watchdog));
    assert_eq!(view.metadata().radio_bus_hz, Some(2_500_000));
    assert_eq!(view.counts(), [1; 4]);
    let json = dashboard_host_validation::probe_captures::decode_json(&output[..n]).unwrap();
    assert_eq!(json["boot"]["raw_watchdog_reason"], 1);
    assert_eq!(json["boot"]["previous"]["stage"], "radio startup");
    assert_eq!(json["boot"]["previous"]["observed_at_previous_boot_ms"], 12001);
    assert_eq!(json["cached_name"]["utf8"], "NGM");
    let old = dashboard_host_validation::probe_captures::decode_json(&old).unwrap();
    assert!(old["cached_name"].is_null() && old["boot"].is_null());
}

/// CRC-repaired bad name/boot flags, time bounds and reset-stage relationships fail without leaking partially decoded
/// metadata on reuse.
#[test]
fn v4_metadata_rejects_corrupt_names_timestamps_flags_and_reset_stages_with_valid_crc() {
    let mut p = probe();
    p.boot = Some(BootEvidence {
        raw_reason: 1,
        boot_type: None,
        previous: Some(ResetRecord {
            stage: ResetStage::Supervised,
            elapsed_ms: 1,
        }),
    });
    let mut output = [0; 8192];
    let n = encode(&p, build(), 60, &mut output).unwrap();
    let end = 24 + usize::from(u16::from_le_bytes([output[14], output[15]]));
    // name present/source/complete/time/length/NGM/trunc =16 bytes;
    // boot present/reason/type absent/previous present/stage/time =12.
    let name = end - 28;
    let boot = end - 12;
    let legacy = hex(include_str!("fixtures/ble-capture-v3-watchdog.hex").trim());
    let mut decoder = Decoder::new();
    for (at, byte) in [
        (name, 2),
        (name + 1, 0),
        (name + 1, 4),
        (name + 2, 2),
        (name + 11, 0),
        (name + 11, 65),
        (name + 15, 2),
        (boot, 2),
        (boot + 5, 2),
        (boot + 6, 2),
        (boot + 7, 0),
        (boot + 7, 255),
    ] {
        let mut broken = output[..n].to_vec();
        broken[at] = byte;
        fix_crc(&mut broken);
        assert!(decode(&broken).is_err(), "offset {at}, byte {byte}");
        decoder.decode(&output[..n]).unwrap();
        // Several cuts occur after cached-name insertion. Failed partial
        // metadata must remain unpublished and cannot leak into the next load.
        assert!(decoder.decode(&broken).is_err(), "reused offset {at}");
        let view = decoder.decode(&legacy).unwrap();
        assert!(view.metadata().cached_name.is_none());
        assert!(view.metadata().boot.is_none());
        let view = decoder.decode(&output[..n]).unwrap();
        assert_eq!(view.metadata().cached_name.unwrap().data.as_slice(), b"NGM");
        assert_eq!(view.metadata().boot, p.boot);
    }
    for at in [0u64, 61, u64::MAX] {
        let mut broken = output[..n].to_vec();
        broken[name + 3..name + 11].copy_from_slice(&at.to_le_bytes());
        fix_crc(&mut broken);
        assert!(decode(&broken).is_err());
    }
    // A retained prior stage cannot be attached to a non-watchdog boot.
    let mut broken = output[..n].to_vec();
    broken[boot + 1..boot + 5].fill(0);
    fix_crc(&mut broken);
    assert!(decode(&broken).is_err());
    // A GAP label must come from a successful non-truncated full read.
    let mut broken = output[..n].to_vec();
    broken[name + 1] = 3;
    broken[name + 15] = 1;
    fix_crc(&mut broken);
    assert!(decode(&broken).is_err());
}
