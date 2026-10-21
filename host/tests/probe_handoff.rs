//! Derived export hints must retain uncertainty and the original record identity.
use dashboard_core::discovery::capture::{BuildIdentity, BuildProfile, encode};
use dashboard_core::discovery::*;
use dashboard_host_validation::probe_captures::decode_json;
use serde_json::{Value, json};

/// Returns short `short` UUID from its little-endian wire encoding; fixed-width decode failure panics.
fn uuid(short: u16) -> Uuid { Uuid::from_wire(&short.to_le_bytes()).unwrap() }
/// Returns a probe with `peer` advertised and explicit Inspect requested at 2ms; no radio traffic occurs.
fn probe(peer: Peer) -> Probe {
    let mut p = Probe::new();
    p.catalog
        .observe(Advertisement::new(peer, ReportKind::Connectable, -44, &[], 1));
    p.input(Input::Click, 2);
    p
}
/// Returns the common explicitly selected public-address peer fixture.
fn base() -> Probe {
    probe(Peer {
        address: [1, 2, 3, 4, 5, 6],
        random: false,
    })
}
/// Returns the exact encoded capture of `p` at 100ms with unavailable source identity and stock clock metadata.
/// Panics if the fixture exceeds the 8192-byte buffer or violates capture admission.
fn bytes(p: &Probe) -> Vec<u8> {
    let mut bytes = vec![0; 8192];
    let n = encode(
        p,
        BuildIdentity {
            source_digest: [0; 32],
            source_identity_available: false,
            cpu_hz: 150_000_000,
            spi_hz: 75_000_000,
            profile: BuildProfile::Stock,
        },
        100,
        &mut bytes,
    )
    .unwrap();
    bytes.truncate(n);
    bytes
}
/// Returns the JSON evidence projection of encoded `p`; fixture encoding/decoding failure panics.
fn export(p: &Probe) -> Value { decode_json(&bytes(p)).unwrap() }
/// Offers inclusive service range `start`..=`end` with UUID `id` to `p`'s inspection; admission result is intentionally
/// ignored.
fn service(
    p: &mut Probe,
    start: u16,
    end: u16,
    id: Uuid,
) {
    p.inspection
        .accept(InspectionItem::Service(Service { start, end, uuid: id }));
}
/// Offers `p` a characteristic for service `start`, declaration `declaration`, raw `props` and UUID `id`.
/// Value handle is declaration+1; checked overflow panics and admission result is ignored.
fn characteristic(
    p: &mut Probe,
    start: u16,
    declaration: u16,
    props: u8,
    id: Uuid,
) {
    p.inspection.accept(InspectionItem::Characteristic(Characteristic {
        service_start: start,
        declaration,
        handle: declaration + 1,
        properties: props,
        uuid: id,
    }));
}
/// Offers `p` a CCCD at `handle`, attached to `characteristic_handle` in service `start`; admission result is ignored.
fn descriptor(
    p: &mut Probe,
    start: u16,
    characteristic_handle: u16,
    handle: u16,
) {
    p.inspection.accept(InspectionItem::Descriptor(Descriptor {
        service_start: start,
        characteristic_handle,
        handle,
        uuid: uuid(0x2902),
    }));
}

/// UUID canonicalization equates Bluetooth-base aliases while retaining original wire width and bytes.
#[test]
fn uuid_normalization_preserves_original_width_bytes_and_base_aliases() {
    let mut p = base();
    for (i, wire) in [
        vec![0xF1, 0xFF],
        vec![0xF1, 0xFF, 0, 0],
        vec![
            0xFB, 0x34, 0x9B, 0x5F, 0x80, 0, 0, 0x80, 0, 0x10, 0, 0, 0xF1, 0xFF, 0, 0,
        ],
        vec![0x78, 0x56, 0x34, 0x12],
        (0u8..16).collect(),
    ]
    .iter()
    .enumerate()
    {
        service(
            &mut p,
            1 + (i as u16) * 10,
            9 + (i as u16) * 10,
            Uuid::from_wire(wire).unwrap(),
        );
    }
    let v = export(&p);
    for i in 0..3 {
        assert_eq!(
            v["records"][i]["uuid"]["canonical"],
            "0000fff1-0000-1000-8000-00805f9b34fb"
        );
    }
    assert_eq!(v["records"][0]["uuid"]["wire_hex"], "f1ff");
    assert_eq!(v["records"][1]["uuid"]["width_bytes"], 4);
    assert_eq!(v["records"][2]["uuid"]["width_bytes"], 16);
    assert_eq!(
        v["records"][3]["uuid"]["canonical"],
        "12345678-0000-1000-8000-00805f9b34fb"
    );
    assert_eq!(
        v["records"][4]["uuid"]["canonical"],
        "0f0e0d0c-0b0a-0908-0706-050403020100"
    );
}

/// All characteristic property bits remain distinct and never enable a connection profile automatically.
#[test]
fn properties_keep_write_request_command_notify_and_indicate_distinct() {
    let mut p = base();
    service(&mut p, 1, 40, uuid(0xFFF0));
    for (i, props) in [0, 1, 2, 4, 8, 16, 32, 64, 128, 255].into_iter().enumerate() {
        characteristic(&mut p, 1, 2 + (i as u16) * 3, props, uuid(0xFFF1));
    }
    let v = export(&p);
    let names = [
        "broadcast",
        "read",
        "write_without_response",
        "write_request",
        "notify",
        "indicate",
        "authenticated_signed_writes",
        "extended_properties",
    ];
    for (i, bits) in [0, 1, 2, 4, 8, 16, 32, 64, 128, 255].into_iter().enumerate() {
        for (bit, name) in names.iter().enumerate() {
            assert_eq!(v["records"][i + 1]["property_flags"][name], bits & (1 << bit) != 0);
        }
    }
    assert_eq!(v["connection_handoff"]["enabled"], false);
    assert!(v["connection_handoff"]["profile"].is_null());
}

/// Duplicate service/characteristic aliases retain separate record indices and cannot yield a silently selected
/// endpoint.
#[test]
fn duplicate_services_and_characteristics_are_never_silently_selected() {
    let mut p = base();
    service(&mut p, 1, 9, uuid(0xFFF0));
    service(&mut p, 20, 29, Uuid::from_wire(&[0xF0, 0xFF, 0, 0]).unwrap());
    characteristic(&mut p, 1, 2, 0x10, uuid(0xFFF1));
    characteristic(&mut p, 1, 5, 0x10, uuid(0xFFF1));
    characteristic(&mut p, 20, 21, 0x10, uuid(0xFFF2));
    descriptor(&mut p, 1, 3, 4);
    descriptor(&mut p, 1, 6, 7);
    let v = export(&p);
    let endpoints = v["connection_handoff"]["endpoints"].as_array().unwrap();
    assert_eq!(endpoints.len(), 3);
    assert!(endpoints.iter().all(|e| e["uuid_pair_unique_in_capture"] == false));
    assert_eq!(endpoints[0]["characteristic_record_index"], 2);
    assert_eq!(endpoints[1]["characteristic_record_index"], 3);
    assert_eq!(endpoints[2]["service_record_index"], 1);
    assert_eq!(endpoints[0]["cccd_record_indices"], json!([5]));
    assert_eq!(endpoints[1]["cccd_record_indices"], json!([6]));
    assert_eq!(endpoints[2]["cccd_record_indices"], json!([]));
}

/// Observed/missing/multiple CCCDs describe evidence only, with saved-handle reuse disabled.
#[test]
fn cccd_evidence_does_not_assume_a_writable_or_subscribed_endpoint() {
    let mut p = base();
    service(&mut p, 1, 19, uuid(0xFFF0));
    characteristic(&mut p, 1, 2, 0x10, uuid(0xFFF1));
    characteristic(&mut p, 1, 8, 0x20, uuid(0xFFF2));
    characteristic(&mut p, 1, 12, 2, uuid(0xFFF3));
    descriptor(&mut p, 1, 3, 4);
    descriptor(&mut p, 1, 3, 5);
    let v = export(&p);
    let e = &v["connection_handoff"]["endpoints"];
    assert_eq!(e[0]["cccd_record_indices"], json!([4, 5]));
    assert_eq!(e[0]["subscription_evidence"], "multiple_cccd_observed");
    assert_eq!(e[1]["subscription_evidence"], "cccd_not_observed");
    assert_eq!(e[2]["subscription_evidence"], "not_advertised");
    assert_eq!(v["connection_handoff"]["saved_handles_reusable"], false);
}

/// Shared address admission allows valid public/static candidates while private/reserved addresses remain blocked and
/// disabled.
#[test]
fn address_candidates_use_shared_admission_and_do_not_make_private_peers_stable() {
    for (address, random, kind) in [
        ([1, 2, 3, 4, 5, 6], false, Some("Public")),
        ([0xC1, 2, 3, 4, 5, 6], true, Some("RandomStatic")),
        ([0x41, 2, 3, 4, 5, 6], true, None),
        ([1, 2, 3, 4, 5, 6], true, None),
        ([0x81, 2, 3, 4, 5, 6], true, None),
        ([0; 6], false, None),
        ([0xFF; 6], false, None),
        ([0xFF; 6], true, None),
        ([0xC0, 0, 0, 0, 0, 0], true, None),
    ] {
        let v = export(&probe(Peer { address, random }));
        let h = &v["connection_handoff"];
        if let Some(kind) = kind {
            assert_eq!(h["address_candidate"]["kind"], kind);
            assert_eq!(h["address_candidate"]["address"], json!(address));
            assert!(h["address_blocker"].is_null());
        } else {
            assert!(h["address_candidate"].is_null());
            assert!(h["address_blocker"].is_string());
        }
        assert_eq!(h["enabled"], false);
    }
}

/// Useful endpoint rows cannot hide incomplete, omitted or malformed inventory; even complete evidence has no inferred
/// profile.
#[test]
fn partial_omitted_or_malformed_inventory_is_explicit_even_with_useful_rows() {
    let mut p = base();
    service(&mut p, 1, 5, uuid(0xFFF0));
    characteristic(&mut p, 1, 2, 0x10, uuid(0xFFF1));
    descriptor(&mut p, 1, 3, 4);
    assert_eq!(export(&p)["connection_handoff"]["inventory_complete"], false);
    p.inspection.stages = [true; 4];
    p.inspection.complete = true;
    p.inspection.ended_at_ms = Some(50);
    p.inspection.cleanup_complete = true;
    let v = export(&p);
    assert_eq!(v["connection_handoff"]["inventory_complete"], true);
    assert_eq!(
        v["connection_handoff"]["endpoints"][0]["uuid_pair_unique_in_capture"],
        true
    );
    assert_eq!(
        v["connection_handoff"]["endpoints"][0]["subscription_evidence"],
        "cccd_observed"
    );
    assert!(v["connection_handoff"]["profile"].is_null());
    p.inspection.complete = false;
    p.inspection.omitted = 1;
    assert_eq!(export(&p)["connection_handoff"]["inventory_complete"], false);
    p.inspection.omitted = 0;
    p.inspection.malformed = 1;
    assert_eq!(export(&p)["connection_handoff"]["inventory_complete"], false);
}

/// Hash candidates require successful untruncated 16-byte evidence and still do not authorize saved-handle reuse.
#[test]
fn database_hash_candidates_exclude_failed_truncated_or_invalid_length_reads() {
    for (result, failure, len, truncated, expected) in [
        (ReadResult::Ok, None, 16, false, json!([2])),
        (ReadResult::Ok, None, 15, false, json!([])),
        (ReadResult::Ok, None, 16, true, json!([])),
        (ReadResult::Protected, Some(FailureReason::Att(5)), 0, false, json!([])),
    ] {
        let mut p = base();
        service(&mut p, 1, 5, uuid(0x1801));
        characteristic(&mut p, 1, 2, 2, uuid(0x2B2A));
        let mut value = Payload::new(&[0x77; 16][..len]);
        value.truncated = truncated;
        p.inspection.accept(InspectionItem::Value(InfoValue {
            service_start: 1,
            handle: 3,
            uuid: 0x2B2A,
            value,
            result,
            failure,
            elapsed_ms: 5,
        }));
        let v = export(&p);
        assert_eq!(v["connection_handoff"]["database_hash_read_record_indices"], expected);
        assert_eq!(v["records"][2]["truncated"], truncated);
        assert_eq!(v["connection_handoff"]["saved_handles_reusable"], false);
    }
}

/// Frozen schema 2 bytes retain their independent SHA256 and deterministic projection with unavailable newer hardware
/// metadata.
#[test]
fn legacy_payload_gets_deterministic_projection_and_unknown_hardware_fields() {
    let fixture = include_str!("fixtures/ble-capture-v2-failure.hex");
    let hex: String = fixture.split_whitespace().collect();
    let b: Vec<u8> = hex
        .as_bytes()
        .as_chunks::<2>()
        .0
        .iter()
        .map(|v| u8::from_str_radix(core::str::from_utf8(v).unwrap(), 16).unwrap())
        .collect();
    let v = decode_json(&b).unwrap();
    assert_eq!(v["schema"], 2);
    assert_eq!(v["export_schema"], 1);
    assert_eq!(v["binary"]["byte_length"], b.len());
    // Independently computed from the frozen fixture with Python hashlib.
    assert_eq!(
        v["binary"]["sha256"],
        "2ca1b68c602c8683b38107bfd7e58a026e1eb04b12244ff04b705d608b8d96ba"
    );
    assert!(v["led_latch"].is_null());
    assert!(v["build"]["radio_gspi_hz"].is_null());
    assert_eq!(v["connection_handoff"]["version"], 1);
    assert_eq!(v["connection_handoff"]["status"], "evidence_only");
    assert_eq!(v, decode_json(&b).unwrap());
    assert!(decode_json(&b[..b.len() - 1]).is_err());
}

/// Same-service UUID aliases remain ambiguous even when characteristic write properties differ.
#[test]
fn repeated_characteristic_aliases_in_a_single_service_remain_ambiguous() {
    let mut p = base();
    service(&mut p, 1, 10, uuid(0xFFF0));
    characteristic(&mut p, 1, 2, 8, uuid(0xFFF1));
    characteristic(&mut p, 1, 5, 4, Uuid::from_wire(&[0xF1, 0xFF, 0, 0]).unwrap());
    let v = export(&p);
    let e = v["connection_handoff"]["endpoints"].as_array().unwrap();
    assert_eq!(e.len(), 2);
    assert!(e.iter().all(|v| v["uuid_pair_unique_in_capture"] == false));
}

/// Advertisement-only evidence creates neither endpoints, completed inventory, termination proof nor an enabled recipe.
#[test]
fn advertisement_only_handoff_cannot_become_a_completed_connection_recipe() {
    let mut p = Probe::new();
    p.catalog.observe(Advertisement::new(
        Peer {
            address: [1; 6],
            random: false,
        },
        ReportKind::Connectable,
        -44,
        &[],
        1,
    ));
    let v = export(&p);
    assert_eq!(v["gatt_requested"], false);
    assert_eq!(v["connection_handoff"]["inventory_complete"], false);
    assert_eq!(v["connection_handoff"]["endpoints"], json!([]));
    assert_eq!(v["connection_handoff"]["enabled"], false);
    assert!(v["connection_handoff"]["profile"].is_null());
    assert_eq!(v["inspection"]["termination_assessment"]["status"], "not_observed");
}

/// An old complete flag remains raw evidence while derived abnormal termination is reported without modifying encoded
/// bytes.
#[test]
fn old_complete_flag_does_not_suppress_derived_abnormal_termination_assessment() {
    let mut p = base();
    service(&mut p, 1, 5, uuid(0x1800));
    characteristic(&mut p, 1, 2, 2, uuid(0x2A00));
    p.inspection.link = Some(LinkInfo {
        connected_at_ms: 3,
        connect_elapsed_ms: 1,
        interval_us: 30_000,
        latency: 0,
        supervision_timeout_us: 4_000_000,
        att_mtu: Some(23),
    });
    p.inspection.stages = [true; 4];
    p.inspection.complete = true;
    p.inspection.ended_at_ms = Some(50);
    p.inspection.cleanup_complete = true;
    p.inspection.disconnect_reason = Some(0x22);
    let original = bytes(&p);
    let v = decode_json(&original).unwrap();
    assert_eq!(v["inspection"]["complete"], true, "retain the old firmware's raw claim");
    assert_eq!(v["inspection"]["disconnect_reason"], 34);
    assert!(v["inspection"]["failure"].is_null());
    assert_eq!(v["led_latch"]["mask"], 0, "do not invent historical latch events");
    assert_eq!(v["connection_handoff"]["inventory_complete"], true);
    assert_eq!(v["inspection"]["termination_assessment"]["status"], "abnormal_reason");
    assert_eq!(v["inspection"]["termination_assessment"]["reason_hex"], "0x22");
    assert_eq!(
        v["inspection"]["termination_assessment"]["reason_label"],
        "LL response timeout"
    );
    assert_eq!(
        v["inspection"]["termination_assessment"]["acceptable_for_clean_probe"],
        false
    );
    assert_eq!(v["inspection"]["termination_assessment"]["version"], 1);
    assert_eq!(original, bytes(&p));
}

/// Derived teardown assessment distinguishes reason absence/conflict, local ingress faults and unconfirmed cleanup from
/// normal end.
#[test]
fn termination_assessment_distinguishes_normal_missing_ingress_unconfirmed_and_conflicting_evidence() {
    for (reason, cleanup, local_error, context_reason, expected, acceptable) in [
        (Some(0x16), true, None, Some(0x16), "normal_reason", true),
        (Some(0x13), true, None, None, "normal_reason", true),
        (None, true, None, None, "reason_unavailable", false),
        (Some(0), true, None, None, "abnormal_reason", false),
        (Some(0xFF), true, None, None, "abnormal_reason", false),
        (
            Some(0x16),
            true,
            Some(FailureReason::Codec),
            Some(0x16),
            "local_ingress_error",
            false,
        ),
        (Some(0x16), false, None, Some(0x16), "cleanup_unconfirmed", false),
        (Some(0x16), true, None, Some(0x22), "conflicting_reason", false),
    ] {
        let mut p = base();
        p.inspection.link = Some(LinkInfo {
            connected_at_ms: 3,
            connect_elapsed_ms: 1,
            interval_us: 30_000,
            latency: 0,
            supervision_timeout_us: 4_000_000,
            att_mtu: Some(23),
        });
        p.inspection.ended_at_ms = Some(50);
        p.inspection.cleanup_complete = cleanup;
        p.inspection.disconnect_reason = reason;
        p.failure_context = ConnectionFailureContext {
            local_error,
            disconnect_reason: context_reason,
        };
        let v = export(&p);
        let assessment = &v["inspection"]["termination_assessment"];
        assert_eq!(assessment["status"], expected);
        assert_eq!(assessment["acceptable_for_clean_probe"], acceptable);
        assert_eq!(
            v["connection_handoff"]["inventory_complete"], false,
            "even normal teardown is not completed inventory"
        );
        assert_eq!(v["connection_handoff"]["enabled"], false);
    }
    let mut p = base();
    p.inspection.ended_at_ms = Some(50);
    p.inspection.cleanup_complete = true;
    p.failure = Some(InspectionFailure::new(FailureStage::Connect, FailureReason::Timeout));
    p.error = ProbeError::Timeout;
    assert_eq!(
        export(&p)["inspection"]["termination_assessment"]["status"],
        "not_observed"
    );
}
