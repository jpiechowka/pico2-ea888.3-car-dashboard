use dashboard_configurator::helper_client::{Connection, DeviceSelection};
use dashboard_helper_protocol::*;
fn connection() -> Connection {
    Connection::decode(
        format!(
            r#"{{"protocol_version":1,"session_id":"{}","capability":"{}"}}"#,
            "a".repeat(32),
            "b".repeat(64)
        )
        .as_bytes(),
    )
    .unwrap()
}
fn device() -> DeviceSummary {
    DeviceSummary {
        device_id: DeviceId::new("c".repeat(32)).unwrap(),
        descriptor_sha256: Sha256::new("d".repeat(64)).unwrap(),
        vid: 0x2E8A,
        pid: 0x000F,
        serial: Some(Text::new("ABC123").unwrap()),
        product: None,
        status: DeviceStatus::DescriptorOnly,
        reason: Text::new("Detected").unwrap(),
        capacity: DeviceCapacity {
            state: DeviceCapacityState::Unverified,
            bytes: None,
            provenance: None,
        },
        flash_ready: false,
    }
}
fn inspection() -> DeviceInspection {
    let d = device();
    DeviceInspection {
        session_id: connection().session().clone(),
        device_id: d.device_id,
        descriptor_sha256: d.descriptor_sha256,
        complete: true,
        facts: Some(BootselFacts {
            revision: Text::new("A3").unwrap(),
            chip_id: Text::new("0x0123456789abcdef").unwrap(),
            boot_random: Text::new("00000001:00000002:00000003:00000004").unwrap(),
            available_cpus: Text::new("ARM, RISC-V").unwrap(),
            secure_boot: false,
            debug_enabled: true,
            secure_debug_enabled: true,
            unpartitioned_families: Text::new("rp2350-arm-s").unwrap(),
            flash_size_estimate: Some(4194304),
        }),
        information_sha256: Some(Sha256::new("e".repeat(64)).unwrap()),
        reason: Text::new("Inspected").unwrap(),
        capacity: d.capacity,
        flash_ready: false,
    }
}
#[test]
fn confirmation_requires_current_complete_inspection_and_explicit_choice() {
    let c = connection();
    let mut state = DeviceSelection::default();
    assert!(state.acknowledge(true).is_err());
    state.select(device()).unwrap();
    assert!(state.acknowledge(true).is_err());
    state.accept(&c, inspection()).unwrap();
    assert!(state.acknowledgement().is_none());
    state.acknowledge(true).unwrap();
    let ack = state.acknowledgement().unwrap();
    assert_eq!(ack.board, BoardContract::GenuinePico2);
    assert_eq!(ack.session_id, *c.session());
    assert_eq!(ack.information_sha256.as_str(), "e".repeat(64));
    state.acknowledge(false).unwrap();
    assert!(state.acknowledgement().is_none());
    state.acknowledge(true).unwrap();
    state.select(device()).unwrap();
    assert!(state.acknowledgement().is_none());
    assert!(state.inspection().is_none());
}
#[test]
fn foreign_and_changed_inspection_cannot_authorize_selected_device() {
    for variant in 0..5 {
        let mut state = DeviceSelection::default();
        state.select(device()).unwrap();
        let mut report = inspection();
        match variant {
            0 => report.session_id = SessionId::new("f".repeat(32)).unwrap(),
            1 => report.device_id = DeviceId::new("f".repeat(32)).unwrap(),
            2 => report.descriptor_sha256 = Sha256::new("f".repeat(64)).unwrap(),
            3 => report.facts = None,
            _ => report.flash_ready = true,
        };
        assert!(state.accept(&connection(), report).is_err());
        assert!(state.acknowledge(true).is_err());
    }
    let mut state = DeviceSelection::default();
    let mut d = device();
    d.status = DeviceStatus::Ambiguous;
    assert!(state.select(d).is_err());
    assert!(state.selected().is_none());
}
#[test]
fn failed_reinspection_removes_previous_confirmation() {
    let mut state = DeviceSelection::default();
    state.select(device()).unwrap();
    state.accept(&connection(), inspection()).unwrap();
    state.acknowledge(true).unwrap();
    let mut report = inspection();
    report.complete = false;
    report.facts = None;
    report.information_sha256 = None;
    state.accept(&connection(), report).unwrap();
    assert!(state.acknowledgement().is_none());
    assert!(state.acknowledge(true).is_err());
    state.clear();
    assert!(state.selected().is_none());
    assert!(state.inspection().is_none());
}
