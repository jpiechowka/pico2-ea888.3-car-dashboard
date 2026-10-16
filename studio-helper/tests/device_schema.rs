use dashboard_helper_protocol::*;
fn report() -> DeviceScanReport {
    DeviceScanReport {
        schema_version: 1,
        session_id: SessionId::new("0".repeat(32)).unwrap(),
        complete: true,
        reason: Text::new("descriptor scan").unwrap(),
        devices: BoundedVec::new(vec![DeviceSummary {
            descriptor_sha256: Sha256::new("1".repeat(64)).unwrap(),
            device_id: DeviceId::new("1".repeat(32)).unwrap(),
            vid: 0x2E8A,
            pid: 0x000F,
            serial: None,
            product: None,
            status: DeviceStatus::DescriptorOnly,
            reason: Text::new("capacity unverified").unwrap(),
            capacity: DeviceCapacity {
                state: DeviceCapacityState::Unverified,
                bytes: None,
                provenance: None,
            },
            flash_ready: false,
        }])
        .unwrap(),
    }
}
#[test]
fn descriptors_cannot_claim_flash_readiness() {
    let mut r = report();
    assert!(r.validate().is_ok());
    let mut devices = r.devices.into_vec();
    devices[0].flash_ready = true;
    r.devices = BoundedVec::new(devices).unwrap();
    assert!(r.validate().is_err());
}
#[test]
fn capacity_requires_explicit_provenance() {
    let mut r = report();
    let mut devices = r.devices.into_vec();
    devices[0].capacity.bytes = Some(4194304);
    r.devices = BoundedVec::new(devices).unwrap();
    assert!(r.validate().is_err());
}

#[test]
fn incomplete_inspection_cannot_retain_trusted_information_digest(){
 let report=DeviceInspection{session_id:SessionId::new("1".repeat(32)).unwrap(),device_id:DeviceId::new("2".repeat(32)).unwrap(),descriptor_sha256:Sha256::new("3".repeat(64)).unwrap(),complete:false,facts:None,information_sha256:Some(Sha256::new("4".repeat(64)).unwrap()),reason:Text::new("incomplete").unwrap(),capacity:DeviceCapacity{state:DeviceCapacityState::Unverified,bytes:None,provenance:None},flash_ready:false};
 assert!(encode(&report).is_err());
}
