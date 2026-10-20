use std::io::{self, Cursor, Read, Write};
use std::time::Duration;

use dashboard_core::discovery::capture::*;
use dashboard_core::discovery::capture_wire::{Request, *};
use dashboard_core::discovery::*;
use dashboard_host_validation::probe_captures::*;
struct Fake {
    incoming: Cursor<Vec<u8>>,
    sent: Vec<u8>,
    chunk: usize,
}
impl Read for Fake {
    fn read(
        &mut self,
        b: &mut [u8],
    ) -> io::Result<usize> {
        let n = b.len().min(self.chunk);
        self.incoming.read(&mut b[..n])
    }
}
impl Write for Fake {
    fn write(
        &mut self,
        b: &[u8],
    ) -> io::Result<usize> {
        let n = b.len().min(self.chunk);
        self.sent.extend_from_slice(&b[..n]);
        Ok(n)
    }

    fn flush(&mut self) -> io::Result<()> { panic!("must not use potentially unbounded serial flush") }
}
fn frame(
    kind: FrameKind,
    status: FrameStatus,
    slot: Option<u8>,
    generation: u64,
    body: &[u8],
) -> Vec<u8> {
    let mut v = FrameHeader::new(kind, status, slot, generation, body)
        .unwrap()
        .encode()
        .unwrap()
        .to_vec();
    v.extend_from_slice(body);
    v
}
fn fake(bytes: Vec<u8>) -> Fake {
    Fake {
        incoming: Cursor::new(bytes),
        sent: Vec::new(),
        chunk: 1,
    }
}
fn end() -> Vec<u8> { frame(FrameKind::End, FrameStatus::Ok, None, 0, &[]) }
fn payload() -> Vec<u8> {
    let mut p = Probe::new();
    p.catalog.observe(Advertisement::new(
        Peer {
            address: [1, 2, 3, 4, 5, 6],
            random: true,
        },
        ReportKind::Connectable,
        -44,
        &[2, 1, 6],
        1,
    ));
    let mut b = vec![0; 8192];
    let n = encode(
        &p,
        BuildIdentity {
            source_digest: [1; 32],
            cpu_hz: 150_000_000,
            spi_hz: 75_000_000,
            profile: BuildProfile::Stock,
            source_identity_available: true,
        },
        2,
        &mut b,
    )
    .unwrap();
    b.truncate(n);
    b
}
#[test]
fn list_requires_four_unique_slots_and_end_with_fragmented_io() {
    let mut incoming = Vec::new();
    for slot in 0..4 {
        incoming.extend(frame(FrameKind::ListEntry, FrameStatus::Empty, Some(slot), 0, &[]));
    }
    incoming.extend(end());
    let mut io = fake(incoming);
    let list = exchange(&mut io, Request::List, Duration::from_secs(1)).unwrap();
    assert_eq!(list.len(), 4);
    assert_eq!(io.sent, b"BLECAP LIST\n");
}
#[test]
fn get_binds_slot_validates_capture_and_requires_end() {
    let body = payload();
    let mut incoming = frame(FrameKind::Capture, FrameStatus::Ok, Some(2), 9, &body);
    incoming.extend(end());
    let mut io = fake(incoming.clone());
    let replies = exchange(&mut io, Request::Get(2), Duration::from_secs(1)).unwrap();
    assert_eq!(replies[0].payload, body);
    assert_eq!(io.sent, b"BLECAP GET 3\n");
    let mut io = fake(incoming);
    assert!(exchange(&mut io, Request::Get(1), Duration::from_secs(1)).is_err());
    let mut io = fake(frame(FrameKind::Capture, FrameStatus::Ok, Some(2), 9, &body));
    assert!(exchange(&mut io, Request::Get(2), Duration::from_secs(1)).is_err());
}
#[test]
fn json_contains_raw_selected_peer_packet_and_truthful_boot_relative_identity() {
    let value = decode_json(&payload()).unwrap();
    assert_eq!(value["peer"]["address_display_hex"], "010203040506");
    assert_eq!(value["peer"]["address_display"], "01:02:03:04:05:06");
    assert!(value["peer"].get("address_wire_hex").is_none());
    assert!(text_report(&value).contains("peer addresses use display order"));
    assert_eq!(value["advertisement"]["raw_hex"], "020106");
    assert_eq!(value["captured_at_boot_ms"], 2);
    assert_eq!(value["build"]["cpu_hz"], 150000000);
    assert!(value.get("utc").is_none());
}

#[test]
fn malformed_response_sequences_and_crc_valid_invalid_capture_are_refused() {
    let valid = payload();
    let mut duplicate = Vec::new();
    for _ in 0..4 {
        duplicate.extend(frame(FrameKind::ListEntry, FrameStatus::Empty, Some(0), 0, &[]));
    }
    duplicate.extend(end());
    assert!(exchange(&mut fake(duplicate), Request::List, Duration::from_secs(1)).is_err());
    assert!(exchange(&mut fake(end()), Request::List, Duration::from_secs(1)).is_err());
    let mut unknown = valid.clone();
    unknown[8] = 4;
    let mut bad = frame(FrameKind::Capture, FrameStatus::Ok, Some(0), 1, &unknown);
    bad.extend(end());
    assert!(exchange(&mut fake(bad), Request::Get(0), Duration::from_secs(1)).is_err());
    let mut bad = frame(FrameKind::Capture, FrameStatus::Ok, Some(0), 1, &valid);
    bad[32] ^= 1;
    bad.extend(end());
    assert!(exchange(&mut fake(bad), Request::Get(0), Duration::from_secs(1)).is_err());
    for status in [
        FrameStatus::Busy,
        FrameStatus::Corrupt,
        FrameStatus::Empty,
        FrameStatus::Unsupported,
        FrameStatus::StorageError,
    ] {
        let mut incoming = frame(FrameKind::Error, status, Some(0), 0, &[]);
        incoming.extend(end());
        let error = exchange(&mut fake(incoming), Request::Get(0), Duration::from_secs(1))
            .err()
            .unwrap();
        assert!(error.contains(&format!("{status:?}")));
    }
    let mut incoming = frame(FrameKind::Capture, FrameStatus::Ok, Some(0), 1, &valid);
    incoming.extend(frame(FrameKind::Capture, FrameStatus::Ok, Some(0), 1, &valid));
    assert!(exchange(&mut fake(incoming), Request::Get(0), Duration::from_secs(1)).is_err());
}

struct Timeout;
impl Read for Timeout {
    fn read(
        &mut self,
        _: &mut [u8],
    ) -> io::Result<usize> {
        Err(io::ErrorKind::TimedOut.into())
    }
}
impl Write for Timeout {
    fn write(
        &mut self,
        b: &[u8],
    ) -> io::Result<usize> {
        Ok(b.len())
    }

    fn flush(&mut self) -> io::Result<()> { panic!() }
}
#[test]
fn deadlines_invalid_requests_and_closed_streams_are_bounded() {
    let start = std::time::Instant::now();
    assert!(exchange(&mut Timeout, Request::List, Duration::from_millis(2)).is_err());
    assert!(start.elapsed() < Duration::from_secs(1));
    let mut io = fake(vec![]);
    assert!(exchange(&mut io, Request::Get(4), Duration::from_secs(1)).is_err());
    assert!(io.sent.is_empty());
    assert!(exchange(&mut io, Request::List, Duration::ZERO).is_err());
    assert!(io.sent.is_empty());
    let mut io = fake(vec![]);
    io.chunk = 0;
    assert!(exchange(&mut io, Request::List, Duration::from_secs(1)).is_err());
}

fn port(
    name: &str,
    product: Option<&str>,
) -> serialport::SerialPortInfo {
    serialport::SerialPortInfo {
        port_name: name.into(),
        port_type: serialport::SerialPortType::UsbPort(serialport::UsbPortInfo {
            vid: USB_VID,
            pid: USB_PID,
            serial_number: None,
            manufacturer: None,
            product: product.map(str::to_owned),
        }),
    }
}
#[test]
fn port_selection_requires_known_product_or_explicit_matching_usb_candidate() {
    let ports = vec![port("COM1", Some(USB_PRODUCT))];
    assert_eq!(select_port(&ports, None).unwrap(), "COM1");
    let ports = vec![port("COM1", Some(USB_PRODUCT)), port("COM2", Some(USB_PRODUCT))];
    assert!(select_port(&ports, None).is_err());
    assert_eq!(select_port(&ports, Some("COM2")).unwrap(), "COM2");
    let ports = vec![port("COM3", Some("USB Serial Device (COM3)"))];
    assert!(select_port(&ports, None).is_err());
    assert_eq!(select_port(&ports, Some("COM3")).unwrap(), "COM3");
    assert!(select_port(&ports, Some("COM9")).is_err());
    let mut wrong = port("COM4", Some(USB_PRODUCT));
    let serialport::SerialPortType::UsbPort(usb) = &mut wrong.port_type else {
        panic!()
    };
    usb.pid = 1;
    assert!(select_port(&[wrong], Some("COM4")).is_err());
}

#[test]
fn publication_is_validated_unique_complete_and_never_overwrites() {
    use std::fs;
    let root = std::env::temp_dir().join(format!(
        "ble-capture-test-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir(&root).unwrap();
    let output = root.join("export");
    let invalid = root.join("invalid");
    assert!(publish(&invalid, b"invalid", None, None).is_err());
    assert!(!invalid.exists());
    let bytes = payload();
    let published = publish(&output, &bytes, Some(2), Some(9)).unwrap();
    let binary_name = format!("{}.blecap", published.file_stem);
    let json_name = format!("{}.json", published.file_stem);
    let text_name = format!("{}.txt", published.file_stem);
    assert_eq!(fs::read(output.join(&binary_name)).unwrap(), bytes);
    assert!(output.join("COMPLETE").exists());
    let value: serde_json::Value = serde_json::from_slice(&fs::read(output.join(&json_name)).unwrap()).unwrap();
    assert_eq!(value["export"]["slot"], 3);
    assert_eq!(value["export"]["generation"], 9);
    assert_eq!(value["export_schema"], 1);
    assert_eq!(value["connection_handoff"]["enabled"], false);
    assert_eq!(value["binary"]["byte_length"], bytes.len());
    assert!(
        fs::read_to_string(output.join(&text_name))
            .unwrap()
            .contains("advertisement.raw_hex: \"020106\"")
    );
    assert!(publish(&output, &bytes, None, None).is_err());
    assert_eq!(fs::read(output.join(&binary_name)).unwrap(), bytes);
    for name in [&binary_name, &json_name, &text_name, "COMPLETE"] {
        fs::remove_file(output.join(name)).unwrap();
    }
    fs::remove_dir(output).unwrap();
    fs::remove_dir(root).unwrap();
}

#[test]
fn publication_rejects_invalid_or_incomplete_slot_envelopes_before_writing() {
    let output = std::env::temp_dir().join(format!("ble-invalid-envelope-{}", std::process::id()));
    assert!(!output.exists());
    for (slot, generation) in [
        (Some(4), Some(1)),
        (Some(255), Some(1)),
        (Some(0), None),
        (None, Some(1)),
        (Some(0), Some(0)),
    ] {
        assert!(publish(&output, &payload(), slot, generation).is_err());
        assert!(!output.exists());
    }
}

#[test]
fn structured_export_includes_every_gatt_row_raw_status_link_and_cleanup() {
    let mut p = Probe::new();
    p.catalog.observe(Advertisement::new(
        Peer {
            address: [2; 6],
            random: false,
        },
        ReportKind::Connectable,
        -55,
        &[],
        10,
    ));
    p.input(Input::Click, 11);
    p.inspection.accept(InspectionItem::Service(Service {
        start: 1,
        end: 10,
        uuid: Uuid::from_wire(&[10, 24]).unwrap(),
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
        value: Payload::empty(),
        result: ReadResult::Protected,
        failure: Some(FailureReason::Att(5)),
        elapsed_ms: 14,
    }));
    p.inspection.link = Some(LinkInfo {
        connected_at_ms: 12,
        connect_elapsed_ms: 1,
        interval_us: 30000,
        latency: 2,
        supervision_timeout_us: 5000000,
        att_mtu: Some(247),
    });
    p.inspection.ended_at_ms = Some(30);
    p.inspection.disconnect_reason = Some(0x13);
    p.inspection.cleanup_complete = true;
    let mut bytes = vec![0; 8192];
    let length = encode(
        &p,
        BuildIdentity {
            source_digest: [0; 32],
            source_identity_available: false,
            profile: BuildProfile::Stock,
            cpu_hz: 150000000,
            spi_hz: 75000000,
        },
        31,
        &mut bytes,
    )
    .unwrap();
    let json = decode_json(&bytes[..length]).unwrap();
    assert_eq!(json["records"][0]["uuid"]["wire_hex"], "0a18");
    assert_eq!(json["records"][1]["properties"], 2);
    assert_eq!(json["records"][2]["characteristic_value_handle"], 3);
    assert_eq!(json["records"][3]["failure"]["kind"], "Att");
    assert_eq!(json["records"][3]["failure"]["raw_code"], 5);
    assert_eq!(json["records"][3]["elapsed_ms"], 14);
    assert_eq!(json["records"][3]["raw_hex"], "");
    assert_eq!(json["link"]["att_mtu"], 247);
    assert_eq!(json["inspection"]["disconnect_reason"], 19);
    assert_eq!(json["inspection"]["cleanup_complete"], true);
    assert!(text_report(&json).contains("records[3].failure.raw_code: 5"));
}
