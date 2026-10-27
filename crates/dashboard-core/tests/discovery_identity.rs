//! Identity evidence admission, persistent format compatibility and bounded publication regressions.
use dashboard_core::ble::canyonero::{CleanupOutcome, Fault, Reason, Stage};
use dashboard_core::discovery::capture::{self, BuildIdentity, BuildProfile};
use dashboard_core::discovery::identity::{ChannelParameters, Evidence, Update, VENDOR_UUIDS};
use dashboard_core::discovery::*;

/// Returns synthetic local/peer channel observations, with unavailable CID/credit snapshots.
fn channel() -> ChannelParameters {
    ChannelParameters {
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
    }
}
/// Returns synthetic selected-peer evidence, four unique vendor endpoints and completed inventory stages.
fn probe() -> Probe {
    let mut p = Probe::new();
    p.identity_mode = true;
    p.catalog.observe(Advertisement::new(
        Peer {
            address: [1, 2, 3, 4, 5, 6],
            random: false,
        },
        ReportKind::Connectable,
        -50,
        &[2, 1, 6],
        10,
    ));
    p.input(Input::Click, 20).unwrap();
    p.inspection.accept(InspectionItem::Service(Service {
        start: 1,
        end: 100,
        uuid: uuid(0xFFF1),
    }));
    for (index, uuid_value) in VENDOR_UUIDS.into_iter().enumerate() {
        p.inspection.accept(InspectionItem::Characteristic(Characteristic {
            service_start: 1,
            declaration: 2 + index as u16 * 3,
            handle: 3 + index as u16 * 3,
            properties: 2,
            uuid: uuid(uuid_value),
        }));
    }
    p.inspection.stages = [true; 4];
    p.phase = RadioPhase::Paused;
    update(&mut p, Update::Start { at_ms: 20 });
    p
}
/// Returns a valid two-byte UUID for synthetic `value`.
fn uuid(value: u16) -> Uuid { Uuid::from_wire(&value.to_le_bytes()).unwrap() }
/// Applies `value` to `p` through the actual generation-fenced model event path.
fn update(
    p: &mut Probe,
    value: Update,
) {
    p.event(Event::Item {
        revision: p.request.revision,
        item: InspectionItem::Identity(value),
    });
}
/// Returns a bounded synthetic read of vendor endpoint `index` in0..4; larger indexes panic.
fn vendor(index: usize) -> InfoValue {
    InfoValue {
        service_start: 1,
        handle: 3 + index as u16 * 3,
        uuid: VENDOR_UUIDS[index],
        value: Payload::new(&[index as u8; 64]),
        result: ReadResult::Ok,
        failure: None,
        elapsed_ms: 3,
    }
}
/// Returns encoded synthetic `p` at100ms with nonzero test source identity; panics on unexpected rejection.
fn encode(p: &Probe) -> Vec<u8> {
    let mut bytes = vec![0; 8192];
    let size = capture::encode(
        p,
        BuildIdentity {
            source_digest: [0x55; 32],
            cpu_hz: 150_000_000,
            spi_hz: 37_500_000,
            profile: BuildProfile::Stock,
            source_identity_available: true,
        },
        100,
        &mut bytes,
    )
    .unwrap();
    bytes.truncate(size);
    bytes
}
/// Returns a structurally valid synthetic INFO with five one-byte fields.
fn info() -> &'static [u8] { b"\x1f\x91\x00\x09v\nm\nh\ns\nf" }
/// Adds owner-ordered request, SDU, split raw chunks and cleanup updates to synthetic `p`.
fn finish(p: &mut Probe) {
    for index in 0..4 {
        if p.inspection.identity.as_ref().unwrap().vendor_values[index].is_none() {
            update(p, Update::Vendor(vendor(index)));
        }
    }
    update(p, Update::Channel(channel()));
    update(p, Update::RequestSubmitted { at_ms: 30 });
    update(
        p,
        Update::Sdu {
            length: info().len() as u16,
        },
    );
    update(
        p,
        Update::Chunk {
            offset: 0,
            bytes: Payload::new(&info()[..3]),
        },
    );
    update(
        p,
        Update::Chunk {
            offset: 3,
            bytes: Payload::new(&info()[3..]),
        },
    );
    update(
        p,
        Update::Stage {
            stage: Stage::Closing,
            at_ms: 40,
            fault: None,
        },
    );
    update(
        p,
        Update::End {
            at_ms: 50,
            cleanup: CleanupOutcome::Clean,
        },
    );
}

/// Exact raw INFO/vendor observations roundtrip; standard values remain separately empty.
#[test]
fn identity_roundtrip_preserves_raw_parameters_and_missing_fields() {
    let mut p = probe();
    for i in 0..4 {
        update(&mut p, Update::Vendor(vendor(i)));
    }
    finish(&mut p);
    assert_eq!(p.inspection.malformed, 0);
    assert!(p.inspection.values.is_empty());
    let bytes = encode(&p);
    let decoded = capture::decode(&bytes).unwrap();
    assert_eq!(decoded.metadata().schema_version, 5);
    let e = decoded.metadata().identity.as_ref().unwrap();
    assert!(e.successful());
    assert_eq!(e.raw(), info());
    assert_eq!(e.channel, Some(channel()));
    assert_eq!(e.sdu_count, 1);
    assert_eq!(e.observed_bytes, info().len() as u64);
    for (i, value) in e.vendor_values.iter().enumerate() {
        let value = value.as_ref().unwrap();
        assert_eq!(value.value.as_slice(), &[i as u8; 64]);
        assert_eq!(value.uuid, VENDOR_UUIDS[i]);
    }
    for end in 0..bytes.len() {
        assert!(capture::decode(&bytes[..end]).is_err());
    }
}

/// Connect-stage failure persists before any GATT/CoC result and cannot turn into success.
#[test]
fn connection_failure_and_first_cause_survive_clean_teardown() {
    let mut p = probe();
    p.inspection.stages = [false; 4];
    p.inspection.services.clear();
    p.inspection.characteristics.clear();
    let first = Fault {
        stage: Stage::Connecting,
        reason: Reason::TimedOut,
        at_ms: 25,
    };
    update(
        &mut p,
        Update::Stage {
            stage: Stage::Closing,
            at_ms: 25,
            fault: Some(first),
        },
    );
    update(
        &mut p,
        Update::Stage {
            stage: Stage::Closing,
            at_ms: 26,
            fault: Some(Fault {
                stage: Stage::Closing,
                reason: Reason::CleanupFailed,
                at_ms: 26,
            }),
        },
    );
    update(
        &mut p,
        Update::End {
            at_ms: 30,
            cleanup: CleanupOutcome::Clean,
        },
    );
    let bytes = encode(&p);
    let decoded = capture::decode(&bytes).unwrap();
    let e = decoded.metadata().identity.as_ref().unwrap();
    assert_eq!(e.fault, Some(first));
    assert!(!e.successful());
    assert!(e.channel.is_none());
    assert!(e.raw().is_empty());
}

/// Cancellation during publication retains known observed length and contiguous prefix.
#[test]
fn partial_publication_gaps_duplicates_and_stale_revisions_never_fabricate_bytes() {
    let mut p = probe();
    update(&mut p, Update::Channel(channel()));
    update(&mut p, Update::RequestSubmitted { at_ms: 30 });
    update(&mut p, Update::Sdu { length: 245 });
    update(
        &mut p,
        Update::Chunk {
            offset: 0,
            bytes: Payload::new(&[7; 64]),
        },
    );
    update(
        &mut p,
        Update::Chunk {
            offset: 65,
            bytes: Payload::new(&[8; 3]),
        },
    );
    update(
        &mut p,
        Update::Chunk {
            offset: 0,
            bytes: Payload::new(&[9; 3]),
        },
    );
    p.event(Event::Item {
        revision: p.request.revision - 1,
        item: InspectionItem::Identity(Update::Chunk {
            offset: 64,
            bytes: Payload::new(&[10; 3]),
        }),
    });
    assert_eq!(p.inspection.malformed, 2);
    let e = p.inspection.identity.as_ref().unwrap();
    assert_eq!(e.raw(), &[7; 64]);
    assert!(e.raw_incomplete());
    let bytes = encode(&p);
    let decoded = capture::decode(&bytes).unwrap();
    assert_eq!(decoded.metadata().identity.as_ref().unwrap().observed_bytes, 245);
    assert!(!decoded.metadata().identity.as_ref().unwrap().successful());
}

/// Vendor evidence needs fresh unique FFF1 declarations; it never enters standard values.
#[test]
fn vendor_reads_reject_wrong_service_duplicates_properties_and_unrequested_updates() {
    let mut p = probe();
    p.inspection.identity = None;
    update(&mut p, Update::Vendor(vendor(0)));
    assert_eq!(p.inspection.malformed, 1);
    update(&mut p, Update::Start { at_ms: 20 });
    p.inspection.characteristics[0].properties = 0;
    update(&mut p, Update::Vendor(vendor(0)));
    p.inspection.characteristics[0].properties = 2;
    p.inspection.services[0].uuid = uuid(0x180A);
    update(&mut p, Update::Vendor(vendor(0)));
    p.inspection.services[0].uuid = uuid(0xFFF1);
    p.inspection.characteristics[1].uuid = uuid(VENDOR_UUIDS[0]);
    update(&mut p, Update::Vendor(vendor(0)));
    assert_eq!(p.inspection.malformed, 4);
    assert!(
        p.inspection
            .identity
            .as_ref()
            .unwrap()
            .vendor_values
            .iter()
            .all(Option::is_none)
    );
    p.inspection.accept(InspectionItem::Value(vendor(0)));
    assert_eq!(p.inspection.malformed, 5);
}

/// Complete but abnormal cleanup and extra observed bytes independently prevent success.
#[test]
fn cleanup_and_raw_omission_independently_revoke_success() {
    let mut p = probe();
    finish(&mut p);
    let e = p.inspection.identity.as_mut().unwrap();
    assert!(e.successful());
    e.cleanup = CleanupOutcome::Failed;
    assert!(!e.successful());
    e.cleanup = CleanupOutcome::Clean;
    e.observed_bytes += 1;
    assert!(!e.successful());
    assert!(!e.apply(Update::Chunk {
        offset: info().len() as u16,
        bytes: Payload::new(&[1])
    }));
}

/// Successful submission/cleanup cannot promote malformed, partial or suffixed raw bytes.
#[test]
fn clean_metadata_still_requires_exact_valid_complete_frame() {
    for (raw, expected) in [
        (info(), true),
        (&info()[..info().len() - 1], false),
        (b"\x1f\x80\0\0".as_slice(), false),
        (b"\x1f\x91\0\x05\n\n\n\n\xff".as_slice(), false),
        (b"\x1f\x91\0\x05\n\n\n\n\0".as_slice(), false),
        (b"\x1f\x91\0\x04bad!".as_slice(), false),
        (b"\x1f\x91\0\x09v\nm\nh\ns\nf\x1f".as_slice(), false),
    ] {
        let mut e = Evidence::new(20);
        for index in 0..4 {
            assert!(e.apply(Update::Vendor(vendor(index))));
        }
        assert!(e.apply(Update::Channel(channel())));
        assert!(e.apply(Update::RequestSubmitted { at_ms: 30 }));
        assert!(e.apply(Update::Sdu {
            length: raw.len() as u16
        }));
        assert!(e.apply(Update::Chunk {
            offset: 0,
            bytes: Payload::new(raw)
        }));
        assert!(e.apply(Update::End {
            at_ms: 50,
            cleanup: CleanupOutcome::Clean
        }));
        assert!(e.valid(50));
        assert_eq!(e.successful(), expected);
        assert_eq!(e.raw(), raw);
    }
}

/// Offered parameters, missing/protected/truncated vendor values and reversed stages cannot claim success.
#[test]
fn success_requires_complete_channel_and_all_four_valid_vendor_reads() {
    let mut p = probe();
    finish(&mut p);
    let original = p.inspection.identity.unwrap();
    assert!(original.successful());
    for index in 0..4 {
        let mut e = original;
        e.vendor_values[index] = None;
        assert!(!e.successful());
        let mut e = original;
        e.vendor_values[index].as_mut().unwrap().value.truncated = true;
        assert!(!e.successful());
        let mut e = original;
        e.vendor_values[index].as_mut().unwrap().result = ReadResult::Protected;
        e.vendor_values[index].as_mut().unwrap().failure = Some(FailureReason::Att(5));
        assert!(!e.successful());
        let mut e = original;
        let c = e.channel.as_mut().unwrap();
        match index {
            0 => c.local_mtu = None,
            1 => c.local_mps = None,
            2 => c.peer_mtu = None,
            _ => c.peer_mps = None,
        };
        assert!(!e.successful());
    }
    let mut e = Evidence::new(20);
    assert!(e.apply(Update::Stage {
        stage: Stage::AwaitingInfo,
        at_ms: 30,
        fault: None
    }));
    assert!(!e.apply(Update::Stage {
        stage: Stage::Connecting,
        at_ms: 31,
        fault: None
    }));
    let fault = Fault {
        stage: Stage::AwaitingInfo,
        reason: Reason::TimeReversed,
        at_ms: 10,
    };
    assert!(e.apply(Update::Stage {
        stage: Stage::Closing,
        at_ms: 10,
        fault: Some(fault)
    }));
    assert_eq!(e.fault, Some(fault));
    assert_eq!(e.updated_at_ms, 30);
    assert!(e.valid(30));
}

/// Returns bytes loaded from frozen legacy hex fixture `text`; panics on malformed test data.
fn hex(text: &str) -> Vec<u8> {
    let text = text.trim();
    (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&text[i..i + 2], 16).unwrap())
        .collect()
}
/// Reusing a current decoder on each frozen legacy format clears identity rather than inventing it.
#[test]
fn legacy_versions_clear_identity_and_unknown_versions_stay_protected() {
    let mut p = probe();
    finish(&mut p);
    let bytes = encode(&p);
    let mut decoder = capture::Decoder::new();
    for (version, text) in [
        (
            1,
            include_str!("../../../host/tests/fixtures/ble-capture-v1-failure.hex"),
        ),
        (
            2,
            include_str!("../../../host/tests/fixtures/ble-capture-v2-failure.hex"),
        ),
        (
            3,
            include_str!("../../../host/tests/fixtures/ble-capture-v3-watchdog.hex"),
        ),
        (
            4,
            include_str!("../../../host/tests/fixtures/ble-capture-v4-legacy.hex"),
        ),
    ] {
        assert!(decoder.decode(&bytes).unwrap().metadata().identity.is_some());
        let old = hex(text);
        let view = decoder.decode(&old).unwrap();
        assert_eq!(view.metadata().schema_version, version);
        assert!(view.metadata().identity.is_none());
    }
    let mut unknown = bytes.clone();
    unknown[8..10].copy_from_slice(&6u16.to_le_bytes());
    let crc = capture_store::crc32(&unknown[..20]);
    unknown[20..24].copy_from_slice(&crc.to_le_bytes());
    assert_eq!(
        capture::check_header(&unknown, unknown.len()),
        Err(capture::CaptureError::Version)
    );
}

/// Identity maximum prefix and vendor reads coexist with all inventory capacities inside8192.
#[test]
fn maximum_inventory_and_identity_evidence_fit_existing_envelope() {
    let mut p = probe();
    // Learn a maximum-width name through the admitted GAP read/end path first.
    p.inspection.accept(InspectionItem::Service(Service {
        start: 101,
        end: 110,
        uuid: uuid(0x1800),
    }));
    p.inspection.accept(InspectionItem::Characteristic(Characteristic {
        service_start: 101,
        declaration: 102,
        handle: 103,
        properties: 2,
        uuid: uuid(0x2A00),
    }));
    p.inspection.accept(InspectionItem::Value(InfoValue {
        service_start: 101,
        handle: 103,
        uuid: 0x2A00,
        value: Payload::new(&[b'N'; 64]),
        result: ReadResult::Ok,
        failure: None,
        elapsed_ms: 1,
    }));
    p.event(Event::InspectionEnd {
        revision: p.request.revision,
        at_ms: 25,
        cleanup_complete: true,
        disconnect_reason: Some(0x16),
    });
    assert_eq!(p.selected_device().unwrap().cached_name.unwrap().data.len, 64);
    p.inspection.values.clear();
    p.inspection.services.clear();
    p.inspection.characteristics.clear();
    let wide = |short: u16| {
        let mut bytes = [0xFB, 0x34, 0x9B, 0x5F, 0x80, 0, 0, 0x80, 0, 0x10, 0, 0, 0, 0, 0, 0];
        bytes[12..14].copy_from_slice(&short.to_le_bytes());
        Uuid::from_wire(&bytes).unwrap()
    };
    for index in 0..24 {
        p.inspection
            .services
            .push(Service {
                start: 1 + index * 100,
                end: 100 + index * 100,
                uuid: wide(if index == 0 { 0xFFF1 } else { 0x180A }),
            })
            .unwrap();
    }
    for index in 0..64u16 {
        let (service, index_in_service, short) = if index < 4 {
            (1, index, VENDOR_UUIDS[usize::from(index)])
        } else {
            (101 + ((index - 4) / 3) * 100, (index - 4) % 3, 0x2A29)
        };
        let declaration = service + 1 + index_in_service * 10;
        let handle = declaration + 1;
        p.inspection
            .characteristics
            .push(Characteristic {
                service_start: service,
                declaration,
                handle,
                properties: 2,
                uuid: wide(short),
            })
            .unwrap();
        for offset in [1, 2] {
            p.inspection
                .descriptors
                .push(Descriptor {
                    service_start: service,
                    characteristic_handle: handle,
                    handle: handle + offset,
                    uuid: wide(0x2901),
                })
                .unwrap();
        }
        if index < 4 {
            let mut value = vendor(usize::from(index));
            value.handle = handle;
            update(&mut p, Update::Vendor(value));
        } else if index < 20 {
            p.inspection
                .values
                .push(InfoValue {
                    service_start: service,
                    handle,
                    uuid: short,
                    value: Payload::new(&[255; 64]),
                    result: ReadResult::Ok,
                    failure: None,
                    elapsed_ms: u32::MAX,
                })
                .unwrap();
        }
    }
    update(&mut p, Update::Channel(channel()));
    update(&mut p, Update::RequestSubmitted { at_ms: 30 });
    update(&mut p, Update::Sdu { length: 245 });
    for offset in (0..245).step_by(64) {
        update(
            &mut p,
            Update::Chunk {
                offset: offset as u16,
                bytes: Payload::new(&[0xFF; 64][..(245 - offset).min(64)]),
            },
        );
    }
    update(
        &mut p,
        Update::Stage {
            stage: Stage::Closing,
            at_ms: 40,
            fault: Some(Fault {
                stage: Stage::AwaitingInfo,
                reason: Reason::TimedOut,
                at_ms: 40,
            }),
        },
    );
    update(
        &mut p,
        Update::End {
            at_ms: 50,
            cleanup: CleanupOutcome::Failed,
        },
    );
    let peer = p.selected_device().unwrap().peer;
    p.catalog
        .observe(Advertisement::new(peer, ReportKind::Connectable, -50, &[0; 31], 10));
    p.catalog
        .observe(Advertisement::new(peer, ReportKind::ScanResponse, -49, &[0; 31], 12));
    p.inspection.ended_at_ms = Some(50);
    p.inspection.link = Some(LinkInfo {
        connected_at_ms: 21,
        connect_elapsed_ms: u32::MAX,
        interval_us: u32::MAX,
        latency: u16::MAX,
        supervision_timeout_us: u32::MAX,
        att_mtu: Some(247),
    });
    p.boot = Some(dashboard_core::health::reset::BootEvidence {
        raw_reason: 1,
        boot_type: Some(0),
        previous: Some(dashboard_core::health::reset::ResetRecord {
            stage: dashboard_core::health::reset::ResetStage::Supervised,
            elapsed_ms: u32::MAX,
        }),
    });
    p.failure = Some(InspectionFailure {
        stage: FailureStage::Characteristics,
        reason: FailureReason::DeclarationLength,
        range: Some((1, u16::MAX)),
        last_accepted: Some((65533, 65534)),
        elapsed_ms: Some(u32::MAX),
        rejected: Some(RejectedDeclaration::from_wire(u16::MAX, &[0xFF; 253])),
    });
    p.scan_diagnostics.record(ScanFailure {
        kind: ScanErrorKind::InvalidSize,
        revision: 1,
        index: 0,
        total_reports: 1,
        report_bytes: 255,
        remaining_bytes: 255,
        declared_data_len: Some(255),
        at_ms: 10,
    });
    let e = p.inspection.identity.as_mut().unwrap();
    let c = e.channel.as_mut().unwrap();
    c.local_cid = Some(0x40);
    c.peer_cid = Some(0x7F);
    c.local_credits = Some(u16::MAX);
    c.peer_credits = Some(u16::MAX);
    e.fault = Some(Fault {
        stage: Stage::AwaitingInfo,
        at_ms: 40,
        reason: Reason::Transport(dashboard_core::ble::canyonero::TransportFault {
            domain: dashboard_core::ble::canyonero::TransportDomain::L2cap,
            code: Some(u16::MAX),
            partial_send: true,
        }),
    });
    let bytes = encode(&p);
    assert!(bytes.len() <= 8192);
    let decoded = capture::decode(&bytes).unwrap();
    assert_eq!(decoded.counts(), [24, 64, 128, 16]);
    assert_eq!(decoded.metadata().identity.as_ref().unwrap().raw().len(), 245);
    println!(
        "maximum identity capture={} Event={} Update={} InfoValue={} Evidence={} OptionalEvidence={} Inspection={} \
         Decoder={}",
        bytes.len(),
        size_of::<Event>(),
        size_of::<Update>(),
        size_of::<InfoValue>(),
        size_of::<Evidence>(),
        size_of::<Option<Evidence>>(),
        size_of::<Inspection>(),
        size_of::<capture::Decoder>()
    );
    assert!(size_of::<Update>() <= size_of::<InfoValue>() + 8);
    assert!(size_of::<Event>() <= 128);
}
