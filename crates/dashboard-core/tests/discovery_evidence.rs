use dashboard_core::discovery::*;

fn uuid(value: u16) -> Uuid { Uuid::from_wire(&value.to_le_bytes()).unwrap() }

#[test]
fn updating_one_packet_does_not_retimestamp_the_other() {
    let peer = Peer {
        address: [1, 2, 3, 4, 5, 6],
        random: false,
    };
    let mut catalog = Catalog::new();
    catalog.observe(Advertisement::new(peer, ReportKind::Connectable, -50, &[2, 1, 6], 10));
    catalog.observe(Advertisement::new(
        peer,
        ReportKind::ScanResponse,
        -60,
        &[2, 9, b'A'],
        20,
    ));
    catalog.observe(Advertisement::new(peer, ReportKind::Directed, 127, &[], 30));
    let device = &catalog.devices()[0];
    assert_eq!(device.advertisement_meta.unwrap().at_ms, 30);
    assert_eq!(device.advertisement_meta.unwrap().rssi, None);
    assert_eq!(device.advertisement_meta.unwrap().kind, ReportKind::Directed);
    assert_eq!(device.response_meta.unwrap().at_ms, 20);
    assert_eq!(device.response_meta.unwrap().rssi, Some(-60));
    assert_eq!(device.response.as_slice(), &[2, 9, b'A']);
}

#[test]
fn repeated_dis_instances_retain_separate_handle_values_and_exact_errors() {
    let mut inspection = Inspection::new();
    for start in [1, 10] {
        inspection.accept(InspectionItem::Service(Service {
            start,
            end: start + 5,
            uuid: uuid(0x180A),
        }));
        inspection.accept(InspectionItem::Characteristic(Characteristic {
            service_start: start,
            declaration: start + 1,
            handle: start + 2,
            properties: 2,
            uuid: uuid(0x2A29),
        }));
        inspection.accept(InspectionItem::Value(InfoValue {
            service_start: start,
            handle: start + 2,
            uuid: 0x2A29,
            value: Payload::empty(),
            result: ReadResult::Protected,
            failure: Some(FailureReason::Att(0x05)),
            elapsed_ms: 37,
        }));
    }
    assert_eq!(inspection.values.len(), 2);
    assert_eq!(inspection.values[0].handle, 3);
    assert_eq!(inspection.values[1].handle, 12);
    assert_eq!(inspection.values[1].failure, Some(FailureReason::Att(5)));
    assert_eq!(inspection.values[1].elapsed_ms, 37);
}

#[test]
fn descriptor_ranges_end_before_the_next_declaration_and_never_wrap() {
    let service = Service {
        start: 1,
        end: 0xFFFF,
        uuid: uuid(0x1801),
    };
    let rows = [
        Characteristic {
            service_start: 1,
            declaration: 2,
            handle: 3,
            properties: 32,
            uuid: uuid(0x2A05),
        },
        Characteristic {
            service_start: 1,
            declaration: 7,
            handle: 8,
            properties: 2,
            uuid: uuid(0x2B2A),
        },
    ];
    assert_eq!(descriptor_range(service, &rows, 0, true), Some((4, 6)));
    assert_eq!(descriptor_range(service, &rows, 1, false), None);
    assert_eq!(descriptor_range(service, &rows, 1, true), Some((9, 0xFFFF)));
    let final_row = Characteristic {
        handle: 0xFFFF,
        declaration: 0xFFFE,
        ..rows[1]
    };
    assert_eq!(descriptor_range(service, &[final_row], 0, true), None);
}

#[test]
fn standard_reads_are_service_scoped_and_binary_lengths_are_validated() {
    assert!(may_read(uuid(0x1800), uuid(0x2A00), 2));
    assert!(may_read(uuid(0x1800), uuid(0x2A01), 2));
    assert!(may_read(uuid(0x1800), uuid(0x2A04), 2));
    assert!(may_read(uuid(0x1801), uuid(0x2B2A), 2));
    for (service, characteristic, props) in [(0xFFF1, 0x2A29, 2), (0x180A, 0x2A00, 2), (0x1800, 0x2A01, 8)] {
        assert!(!may_read(uuid(service), uuid(characteristic), props));
    }
    for (id, length) in [(0x2A01, 2), (0x2A04, 8), (0x2A23, 8), (0x2A50, 7), (0x2B2A, 16)] {
        assert!(info_length_valid(id, length));
        assert!(!info_length_valid(id, length - 1));
        assert!(!info_length_valid(id, length + 1));
    }
}
