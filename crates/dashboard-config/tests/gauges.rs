//! Gauge range, palette and frozen v1 migration contracts.
#![cfg(feature = "json")]
use dashboard_config::*;

#[test]
/// Frozen v1 forms migrate identically, export current version and reject every truncated legacy prefix.
fn v1_json_and_binary_upgrade_to_the_same_current_document() {
    let json = from_json_bytes(include_bytes!("fixtures/v1-default.json")).unwrap();
    let binary = decode(include_bytes!("fixtures/v1-default.eav")).unwrap();
    assert_eq!(json, binary);
    assert_eq!(json.config(), &default_config());
    let mut buffer = [0; MAX_FILE_BYTES];
    let bytes = encode(&binary, &mut buffer).unwrap();
    assert_eq!(bytes[4], VERSION as u8);
    assert_eq!(decode(bytes).unwrap(), binary);
    for cut in 0..include_bytes!("fixtures/v1-default.eav").len() {
        assert!(decode(&include_bytes!("fixtures/v1-default.eav")[..cut]).is_err());
    }
}

#[test]
/// Gauge expansion respects fixed mode, ignores nonfinite extrema and rejects invalid declared ranges.
fn ranges_are_bounded_and_expand_only_from_finite_accepted_extrema() {
    for channel in Channel::ALL {
        let mut gauge = Gauge::for_channel(channel);
        assert!(gauge.valid(channel));
        assert_eq!(
            gauge.limits(Some(-1_000_000.0), Some(1_000_000.0)),
            (gauge.min, gauge.max)
        );
        gauge.expand = true;
        assert_eq!(
            gauge.limits(Some(gauge.min + 0.1), Some(gauge.max - 0.1)),
            (gauge.min, gauge.max)
        );
        assert_eq!(
            gauge.limits(Some(gauge.min - 1.0), Some(gauge.max + 1.0)),
            (gauge.min - 1.0, gauge.max + 1.0)
        );
        assert_eq!(
            gauge.limits(Some(f32::NAN), Some(f32::INFINITY)),
            (gauge.min, gauge.max)
        );
        for (min, max) in [
            (0.0, 0.0),
            (1.0, 0.0),
            (f32::NAN, 1.0),
            (0.0, f32::INFINITY),
            (-1_000_001.0, 1.0),
            (1.0, 1.00001),
        ] {
            assert!(!Gauge { min, max, ..gauge }.valid(channel));
        }
    }
}

#[test]
/// All 65536 two-byte RGB565 values roundtrip hex; malformed lengths, syntax and Unicode reject.
fn colours_roundtrip_all_panel_values_without_widening_ram() {
    assert_eq!(core::mem::size_of::<Color>(), 2);
    for code in 0..=u16::MAX {
        let color = Color(code);
        assert_eq!(Color::parse(&color.hex()), Some(color));
    }
    for bad in [
        "red",
        "#123",
        "#FFFFFFFF",
        "#GG0000",
        "#１２３４５６",
        " #FFFFFF",
        "#FFFFFF\n",
    ] {
        assert_eq!(Color::parse(bad), None);
    }
}

#[test]
/// JSON preserves separately editable gauge shape/range/expansion and palette values.
fn new_documents_store_independent_gauge_ranges_and_colours() {
    let valid = validate(default_config()).unwrap();
    let mut json: serde_json::Value = serde_json::from_str(&to_json(&valid).unwrap()).unwrap();
    json["widgets"][0]["gauge"] = serde_json::json!({
        "kind": "Circular", "min": -1.0, "max": 3.0, "expand": true
    });
    let imported = from_json_bytes(&serde_json::to_vec(&json).unwrap());
    assert!(imported.is_ok(), "gauge settings must be parseable: {imported:?}");
    let restored: serde_json::Value = serde_json::from_str(&to_json(&imported.unwrap()).unwrap()).unwrap();
    assert_eq!(restored["widgets"][0]["gauge"]["max"], 3.0);
    assert_eq!(restored["widgets"][0]["gauge"]["expand"], true);
    assert!(restored["widgets"][0]["colors"]["value"].is_string());
}
