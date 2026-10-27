//! Immutable schema-six inputs exercise complete field preservation across host migration.
#![cfg(feature = "json")]

use dashboard_config::*;

/// Frozen binary/JSON pairs from the pre-schema-seven production writers; never regenerate in tests.
const FIXTURES: [(&[u8], &[u8]); 4] = [
    (
        include_bytes!("fixtures/v6-default.eav"),
        include_bytes!("fixtures/v6-default.json"),
    ),
    (
        include_bytes!("fixtures/v6-capacity-1x1.eav"),
        include_bytes!("fixtures/v6-capacity-1x1.json"),
    ),
    (
        include_bytes!("fixtures/v6-capacity-2x2.eav"),
        include_bytes!("fixtures/v6-capacity-2x2.json"),
    ),
    (
        include_bytes!("fixtures/v6-mixed.eav"),
        include_bytes!("fixtures/v6-mixed.json"),
    ),
];

#[test]
/// Imports preserve every historical field, add Global ignition popup layout and converge to current boot storage.
fn frozen_schema_six_preserves_every_field_and_canonical_output() {
    for (binary, json) in FIXTURES {
        assert_eq!(u16::from_le_bytes(binary[4..6].try_into().unwrap()), 6);
        let mut expected: serde_json::Value = serde_json::from_slice(json).unwrap();
        assert_eq!(expected["version"], 6);
        expected["version"] = serde_json::json!(VERSION);
        for widget in expected["widgets"].as_array_mut().unwrap() {
            widget["advanced"]["ignition_warning"] = "Global".into();
        }
        let from_binary = decode(binary).unwrap();
        let from_json = from_json_bytes(json).unwrap();
        assert_eq!(from_binary, from_json);
        // Compare JSON writer output so f32 values use the same shortest-decimal representation.
        let actual: serde_json::Value = serde_json::from_str(&to_json(&from_binary).unwrap()).unwrap();
        assert_eq!(actual, expected);
        let mut storage = [0; MAX_FILE_BYTES];
        let canonical = encode(&from_binary, &mut storage).unwrap();
        if VERSION == 6 {
            assert_eq!(canonical, binary);
        }
        assert_eq!(decode(canonical).unwrap(), from_binary);
        let mut slot = ConfigSlot::new();
        let boot = slot.load_at_boot(canonical);
        assert_eq!(boot.fault, None);
        assert_eq!(boot.config, from_binary);
        assert_eq!(
            from_json_bytes(to_json(&from_binary).unwrap().as_bytes()).unwrap(),
            from_binary
        );
    }
}

#[test]
/// Frozen mixed content covers eight channels, ten historical readouts, full capacity and dormant BLE settings.
fn mixed_schema_six_covers_all_scalar_channels_and_readouts() {
    let mixed = decode(include_bytes!("fixtures/v6-mixed.eav")).unwrap();
    let config = mixed.config();
    assert_eq!(config.views.len(), 8);
    assert_eq!(config.widgets.len(), 64);
    assert_eq!(config.boot_view, 8);
    assert_eq!(config.orientation, Orientation::LandscapeInverted);
    assert_eq!(config.views[0].orientation, Some(Orientation::Landscape));
    assert_eq!(config.views[1].orientation, Some(Orientation::LandscapeInverted));
    assert!(config.views[2..].iter().all(|view| view.orientation.is_none()));
    assert_eq!(config.history_seconds, [10, 20, 30, 40, 50, 60, 70, 900]);
    assert!(config.reduced_motion);
    assert_eq!(config.controls.hold_ms, 1500);
    assert!(!config.ble.enabled);
    assert_eq!(config.ble.profile, BleProfile::ObdLinkCx);
    assert_eq!(config.ble.peer.address, [0xC1, 2, 3, 4, 5, 6]);
    assert_eq!(config.ble.peer.kind, BleAddressKind::RandomStatic);
    assert_eq!(config.ble.connect_timeout_ms, 30000);
    assert_eq!(config.ble.response_timeout_ms, 5000);
    assert_eq!(config.ble.poll_gap_ms, 200);
    for channel in ["Boost", "Oil", "Water", "Dsg", "Iat", "Egt", "Battery", "Lambda"] {
        assert_eq!(
            config
                .widgets
                .iter()
                .filter(|w| serde_json::to_value(w.channel).unwrap() == channel)
                .count(),
            8
        );
    }
    for readout in [
        Readout::Celsius,
        Readout::Fahrenheit,
        Readout::FollowBoost,
        Readout::Bar,
        Readout::Psi,
        Readout::Kpa,
        Readout::Volts,
        Readout::Lambda,
        Readout::Afr,
        Readout::LambdaAndAfr,
    ] {
        assert!(config.widgets.iter().any(|widget| widget.advanced.readout == readout));
    }
    for (index, widget) in config.widgets.iter().enumerate() {
        assert_eq!(usize::from(widget.id), index + 1);
        assert_eq!(usize::from(widget.view), index / 8 + 1);
        assert_eq!(widget.display.afr, widget.advanced.readout.includes_afr());
    }
}

#[test]
/// Capacity fixtures retain rich warnings, bands, controls, fuel and wide labels without relying on current defaults.
fn capacity_schema_six_retains_rich_small_and_large_widgets() {
    for (binary, count, span, label_bytes) in [
        (
            include_bytes!("fixtures/v6-capacity-1x1.eav").as_slice(),
            64,
            Span::OneByOne,
            12,
        ),
        (
            include_bytes!("fixtures/v6-capacity-2x2.eav").as_slice(),
            16,
            Span::TwoByTwo,
            24,
        ),
    ] {
        let frozen = decode(binary).unwrap();
        let config = frozen.config();
        assert_eq!(config.views.len(), 8);
        assert_eq!(config.widgets.len(), count);
        assert_eq!(config.boot_view, 248);
        assert_eq!(config.history_seconds, [900; 8]);
        assert_eq!(
            config.controls.screens[0].buttons[0].tap,
            ControlAction::AcknowledgeWarnings
        );
        assert!(matches!(config.fuel, Fuel::EthanolBlend(_)));
        for widget in &config.widgets {
            assert_eq!(widget.placement.span, span);
            assert_eq!(widget.label.len(), label_bytes);
            assert_eq!(widget.advanced.bands().len(), 3);
            assert_eq!(widget.advanced.readout, Readout::Fahrenheit);
            assert_eq!(widget.advanced.smoothing_ms, 5000);
            assert_eq!(widget.warning.mode, LimitMode::OutsideWindow);
            assert!(widget.warning.advanced.latch);
            assert_eq!(widget.warning.advanced.snooze_s, 900);
            assert_eq!(widget.warning.advanced.priority, Priority::Critical);
        }
    }
}
