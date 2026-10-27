//! Offline identity export and USB-selection regressions; no ports or devices are opened.
use dashboard_core::ble::canyonero::{CleanupOutcome, Fault, Reason, Stage, TransportDomain, TransportFault};
use dashboard_core::discovery::capture::{self, BuildIdentity, BuildProfile};
use dashboard_core::discovery::identity::{ChannelParameters, Update, VENDOR_UUIDS};
use dashboard_core::discovery::*;
use dashboard_host_validation::probe_captures::*;

/// Returns an inspected synthetic peer whose advertised label differs from its INFO model.
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
        b"\x06\x09Label",
        10,
    ));
    p.input(Input::Click, 20).unwrap();
    p.phase = RadioPhase::Paused;
    update(&mut p, Update::Start { at_ms: 20 });
    p
}
/// Applies small `u` to current inspection `p`, asserting successful evidence admission.
fn update(
    p: &mut Probe,
    u: Update,
) {
    let before = p.inspection.malformed;
    p.inspection.accept(InspectionItem::Identity(u));
    assert_eq!(p.inspection.malformed, before);
}
/// Returns a validated capture of synthetic `p` at boot100ms, never initiating discovery.
fn encode(p: &Probe) -> Vec<u8> {
    let mut bytes = vec![0; 8192];
    let n = capture::encode(
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
    bytes.truncate(n);
    bytes
}
/// Adds unique fresh service/readable declarations and four retained reads to `p`.
fn vendor_reads(p: &mut Probe) {
    p.inspection.accept(InspectionItem::Service(Service {
        start: 1,
        end: 30,
        uuid: Uuid::from_wire(&0xFFF1u16.to_le_bytes()).unwrap(),
    }));
    for (i, uuid) in VENDOR_UUIDS.into_iter().enumerate() {
        p.inspection.accept(InspectionItem::Characteristic(Characteristic {
            service_start: 1,
            declaration: 2 + i as u16 * 3,
            handle: 3 + i as u16 * 3,
            properties: 2,
            uuid: Uuid::from_wire(&uuid.to_le_bytes()).unwrap(),
        }));
    }
    p.inspection.stages = [true; 4];
    for (i, uuid) in VENDOR_UUIDS.into_iter().enumerate() {
        update(
            p,
            Update::Vendor(InfoValue {
                service_start: 1,
                handle: 3 + i as u16 * 3,
                uuid,
                value: Payload::new(&[i as u8, 0xFF]),
                result: ReadResult::Ok,
                failure: None,
                elapsed_ms: 4,
            }),
        );
    }
}
/// Adds offered/established channel values, one submitted request and raw `reply` to `p`.
fn reply(
    p: &mut Probe,
    reply: &[u8],
) {
    update(
        p,
        Update::Channel(ChannelParameters {
            offered_mtu: 245,
            offered_mps: 247,
            offered_credits: 1,
            local_mtu: Some(245),
            local_mps: Some(247),
            peer_mtu: Some(512),
            peer_mps: Some(247),
            local_cid: Some(0x40),
            peer_cid: Some(0x41),
            local_credits: Some(0),
            peer_credits: Some(8),
        }),
    );
    update(p, Update::RequestSubmitted { at_ms: 30 });
    update(
        p,
        Update::Sdu {
            length: reply.len() as u16,
        },
    );
    for (i, chunk) in reply.chunks(64).enumerate() {
        update(
            p,
            Update::Chunk {
                offset: (i * 64) as u16,
                bytes: Payload::new(chunk),
            },
        );
    }
}
/// Ends `p` with observed clean transport cleanup at50ms; earlier faults remain sticky.
fn end(p: &mut Probe) {
    update(
        p,
        Update::End {
            at_ms: 50,
            cleanup: CleanupOutcome::Clean,
        },
    );
}
/// Returns frozen `text` hex fixture bytes without current-schema re-encoding; malformed hex panics.
fn fixture(text: &str) -> Vec<u8> {
    let text: String = text.split_whitespace().collect();
    (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&text[i..i + 2], 16).unwrap())
        .collect()
}

/// Legacy formats retain all prior fields and explicitly lack producer mode and identity evidence.
#[test]
fn legacy_exports_do_not_infer_identity_mode() {
    for (i, text) in [
        include_str!("fixtures/ble-capture-v1-failure.hex"),
        include_str!("fixtures/ble-capture-v2-failure.hex"),
        include_str!("fixtures/ble-capture-v3-watchdog.hex"),
        include_str!("fixtures/ble-capture-v4-legacy.hex"),
    ]
    .into_iter()
    .enumerate()
    {
        let bytes = fixture(text);
        let value = decode_json(&bytes).unwrap();
        assert_eq!(value["schema"], i + 1);
        assert!(value["identity_tool_mode"].is_null());
        assert!(value["ngm_identity"].is_null());
        assert_eq!(value["connection_handoff"]["enabled"], false);
        assert_eq!(
            value["binary"]["sha256"],
            hex(&sha2_const_stable::Sha256::new().update(&bytes).finalize())
        );
    }
    let mut p = probe();
    p.identity_mode = false;
    p.inspection.identity = None;
    let value = decode_json(&encode(&p)).unwrap();
    assert_eq!(value["identity_tool_mode"], false);
    assert!(value["ngm_identity"].is_null());
}

/// Connect failures retain original u16 transport status, unavailable observations and clean later cleanup.
#[test]
fn connection_error_exports_raw_domain_code_without_inventing_channel_or_reply() {
    let mut p = probe();
    update(
        &mut p,
        Update::Stage {
            stage: Stage::Closing,
            at_ms: 25,
            fault: Some(Fault {
                stage: Stage::Connecting,
                at_ms: 25,
                reason: Reason::Transport(TransportFault {
                    domain: TransportDomain::L2cap,
                    code: Some(0x1234),
                    partial_send: false,
                }),
            }),
        },
    );
    end(&mut p);
    let value = decode_json(&encode(&p)).unwrap();
    let e = &value["ngm_identity"];
    assert_eq!(value["identity_tool_mode"], true);
    assert_eq!(e["fault"]["transport"]["raw_code_u16"], 0x1234);
    assert_eq!(e["fault"]["transport"]["domain"], "L2cap");
    assert_eq!(e["fault"]["stage"], "Connecting");
    assert_eq!(e["cleanup"], "Clean");
    assert_eq!(e["successful"], false);
    assert!(e["channel"].is_null());
    assert_eq!(e["request"]["submitted"], false);
    assert!(e["reply"]["derived_fields"].is_null());
    assert!(e["reply"]["max_observed_sdu_bytes"].is_null());
    assert_eq!(e["candidate"]["configured_spsm"], 129);
    assert!(e["candidate"]["discovered_spsm"].is_null());
    assert!(
        e["vendor_reads"]
            .as_array()
            .unwrap()
            .iter()
            .all(|v| v["read"].is_null())
    );
    assert!(text_report(&value).contains("ngm_identity.fault.transport.raw_code_u16: 4660"));
}

/// Raw partial/malformed replies and protected vendor reads stay inspectable without parsed success.
#[test]
fn partial_and_negative_replies_preserve_raw_failures() {
    for raw in [&b"\x1f\x91\x00\x09v"[..], &b"\x1f\xe1\x00\x02\xaa\xbb"[..]] {
        let mut p = probe();
        vendor_reads(&mut p);
        reply(&mut p, raw);
        let v = p.inspection.identity.as_mut().unwrap().vendor_values[0]
            .as_mut()
            .unwrap();
        v.result = ReadResult::Protected;
        v.failure = Some(FailureReason::Att(5));
        v.value = Payload::empty();
        end(&mut p);
        let value = decode_json(&encode(&p)).unwrap();
        let e = &value["ngm_identity"];
        assert_eq!(e["successful"], false);
        assert_eq!(e["reply"]["raw_prefix_hex"], hex(raw));
        assert!(e["reply"]["derived_fields"].is_null());
        assert_eq!(e["vendor_reads"][0]["read"]["failure"]["raw_code"], 5);
        assert_eq!(e["vendor_reads"][0]["read"]["result"], "Protected");
        if raw[1] == 0xE1 {
            assert_eq!(e["reply"]["retained_prefix_parse_error"]["raw_message_type"], 225);
        } else {
            assert_eq!(e["reply"]["retained_prefix_parse_status"], "NeedMore");
        }
    }
    let mut p = probe();
    reply(&mut p, b"\x1f\x91\x00\x09v\nm\nh\ns\nf");
    update(&mut p, Update::Sdu { length: 1 });
    end(&mut p);
    let e = decode_json(&encode(&p)).unwrap();
    assert_eq!(e["ngm_identity"]["reply"]["omitted_observed_bytes"], 1);
    assert!(e["ngm_identity"]["reply"]["derived_fields"].is_null());
}

/// Success derives five fields while retaining direction, raw bytes, original filename labels and disabled handoff.
#[test]
fn success_projection_and_publication_keep_content_identity_and_observed_name() {
    let mut p = probe();
    vendor_reads(&mut p);
    let raw = b"\x1f\x91\x00\x09v\nm\nh\ns\nf";
    reply(&mut p, raw);
    end(&mut p);
    let bytes = encode(&p);
    let value = decode_json(&bytes).unwrap();
    let e = &value["ngm_identity"];
    assert_eq!(e["successful"], true);
    for (field, expected) in [
        ("vendor", "v"),
        ("model", "m"),
        ("hardware", "h"),
        ("serial", "s"),
        ("firmware", "f"),
    ] {
        assert_eq!(e["reply"]["derived_fields"][field], expected);
    }
    assert_eq!(e["reply"]["raw_prefix_hex"], hex(raw));
    assert_eq!(e["reply"]["observed_bytes"], raw.len());
    assert_eq!(e["channel"]["observed_local_receive"]["remaining_credits"], 0);
    assert_eq!(e["channel"]["observed_peer_receive"]["mtu_bytes"], 512);
    assert_eq!(e["channel"]["observed_local_receive"]["cid"], 64);
    assert_eq!(e["source_commit"], dashboard_core::discovery::identity::SOURCE_COMMIT);
    assert_eq!(value["connection_handoff"]["enabled"], false);
    assert!(value["connection_handoff"]["profile"].is_null());
    assert_eq!(value["connection_handoff"]["saved_handles_reusable"], false);
    assert_eq!(
        value["connection_handoff"]["observation_scope"],
        "standard_advertisement_and_gatt_hints_only"
    );
    assert_eq!(
        value["connection_handoff"]["independent_identity_evidence_path"],
        "ngm_identity"
    );
    assert!(
        value["connection_handoff"]["unobserved"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item == "l2cap_spsm_and_channel_limits")
    );
    assert_eq!(value["identity"]["device_name"]["utf8"], "Label");
    let output = std::env::temp_dir().join(format!(
        "ngm-export-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let published = publish(&output, &bytes, None, None).unwrap();
    assert!(published.file_stem.starts_with("blecap-label-p-010203040506-"));
    assert_eq!(value["identity"]["capture_id"], published.capture_id);
    for suffix in ["blecap", "json", "txt"] {
        let file = output.join(format!("{}.{suffix}", published.file_stem));
        let data = std::fs::read(&file).unwrap();
        if suffix == "blecap" {
            assert_eq!(data, bytes);
        } else if suffix == "txt" {
            assert!(
                std::str::from_utf8(&data)
                    .unwrap()
                    .contains("ngm_identity.reply.derived_fields.model: \"m\"")
            );
        }
        std::fs::remove_file(file).unwrap();
    }
    std::fs::remove_file(output.join("COMPLETE")).unwrap();
    std::fs::remove_dir(output).unwrap();
}

/// Returns enumerated fake `name` with capture VID/PID and supplied `product`; does not open hardware.
fn port(
    name: &str,
    product: &str,
) -> serialport::SerialPortInfo {
    serialport::SerialPortInfo {
        port_name: name.into(),
        port_type: serialport::SerialPortType::UsbPort(serialport::UsbPortInfo {
            vid: USB_VID,
            pid: USB_PID,
            serial_number: None,
            manufacturer: None,
            product: Some(product.into()),
        }),
    }
}
/// Exact products and their same-port Windows friendly names auto-select; mixed tools remain ambiguous.
#[test]
fn usb_selection_accepts_exact_tool_products_and_rejects_ambiguity_without_io() {
    for product in [USB_PRODUCT, NGM_USB_PRODUCT] {
        assert_eq!(select_port(&[port("COM1", product)], None).unwrap(), "COM1");
        assert_eq!(
            select_port(&[port("COM3", &format!("{product} (COM3)"))], None).unwrap(),
            "COM3"
        );
    }
    let ports = [port("COM1", USB_PRODUCT), port("COM2", NGM_USB_PRODUCT)];
    assert!(select_port(&ports, None).is_err());
    assert_eq!(select_port(&ports, Some("COM2")).unwrap(), "COM2");
    let ports = [
        port("COM1", &format!("{USB_PRODUCT} (COM1)")),
        port("COM2", &format!("{NGM_USB_PRODUCT} (COM2)")),
    ];
    assert!(select_port(&ports, None).is_err());
    for product in [
        "Pico NGM Identity Captures other",
        "Pico NGM Identity Captures (COM4)",
        "Pico NGM Identity Captures (COM3) other",
        "Pico NGM Identity Captures (COM3",
        "Pico BLE Probe Captures (COM4)",
        "USB Serial Device",
    ] {
        assert!(select_port(&[port("COM3", product)], None).is_err());
        assert_eq!(select_port(&[port("COM3", product)], Some("COM3")).unwrap(), "COM3");
    }
}
