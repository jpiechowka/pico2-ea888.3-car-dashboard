use dashboard_config::*;

#[test]
fn enabled_ble_requires_explicit_peer_and_preserves_bytes() {
    let mut c = default_config();
    assert!(!c.ble.enabled);
    c.ble.enabled = true;
    assert_eq!(validate(c.clone()).unwrap_err().field, Field::BlePeer);
    c.ble.peer.address = [0xC1, 2, 3, 4, 5, 6];
    c.ble.peer.kind = BleAddressKind::RandomStatic;
    c.ble.profile = BleProfile::TestPeer;
    let expected = validate(c).unwrap();
    let mut bytes = [0; MAX_FILE_BYTES];
    let bytes = encode(&expected, &mut bytes).unwrap();
    assert_eq!(decode(bytes).unwrap(), expected);
    let mut slot = ConfigSlot::new();
    assert_eq!(slot.load_at_boot(bytes).config, expected);
    assert_eq!(slot.load_at_boot(b"bad").config.config().ble, BleSettings::DEFAULT);
}

#[test]
fn random_static_and_inactive_timings_are_validated() {
    for address in [
        [0xFF; 6],
        [0xC0, 0, 0, 0, 0, 0],
        [0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF],
        [0x41, 2, 3, 4, 5, 6],
    ] {
        let mut c = default_config();
        c.ble.peer = BlePeer {
            address,
            kind: BleAddressKind::RandomStatic,
        };
        assert_eq!(validate(c).unwrap_err().field, Field::BlePeer);
    }
    for (connect, response, gap, valid) in [
        (5000, 250, 10, true),
        (30000, 5000, 200, true),
        (4999, 1500, 20, false),
        (30001, 1500, 20, false),
        (10000, 249, 20, false),
        (10000, 5001, 20, false),
        (10000, 1500, 9, false),
        (10000, 1500, 201, false),
    ] {
        let mut c = default_config();
        c.ble.connect_timeout_ms = connect;
        c.ble.response_timeout_ms = response;
        c.ble.poll_gap_ms = gap;
        assert_eq!(validate(c).is_ok(), valid);
    }
}

#[cfg(feature = "json")]
#[test]
fn frozen_v5_migrates_without_enabling_radio() {
    let a = decode(include_bytes!("fixtures/v5-default.eav")).unwrap();
    let b = from_json_bytes(include_bytes!("fixtures/v5-default.json")).unwrap();
    assert_eq!(a, b);
    assert_eq!(a.config().ble, BleSettings::DEFAULT);
    assert_eq!(a.config().version, VERSION);
    assert_eq!(a, validate(default_config()).unwrap());
    let mut json = serde_json::to_value(a.config()).unwrap();
    json.as_object_mut().unwrap().remove("ble");
    assert!(from_json_bytes(&serde_json::to_vec(&json).unwrap()).is_err());
}
