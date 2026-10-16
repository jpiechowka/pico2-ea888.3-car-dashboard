use dashboard_config::*;
#[test]
fn current_configuration_requires_recovery_safe_controls() {
    let mut config = default_config();
    config.controls.screens[0].buttons[0].hold = ControlAction::None;
    config.controls.screens[0].buttons[1].hold = ControlAction::None;
    assert_eq!(
        validate(config).unwrap_err().field,
        Field::Controls(ControlError::MissingViewNavigation)
    );
}
#[cfg(feature = "json")]
#[test]
fn both_historical_versions_upgrade() {
    for (json, binary) in [
        (
            include_bytes!("fixtures/v1-default.json").as_slice(),
            include_bytes!("fixtures/v1-default.eav").as_slice(),
        ),
        (
            include_bytes!("fixtures/v2-default.json").as_slice(),
            include_bytes!("fixtures/v2-default.eav").as_slice(),
        ),
    ] {
        assert_eq!(from_json_bytes(json).unwrap().config(), &default_config());
        assert_eq!(decode(binary).unwrap().config(), &default_config());
    }
}
#[cfg(not(feature = "json"))]
#[test]
fn firmware_rejects_both_historical_versions() {
    for bytes in [
        include_bytes!("fixtures/v1-default.eav").as_slice(),
        include_bytes!("fixtures/v2-default.eav").as_slice(),
    ] {
        assert_eq!(decode(bytes), Err(ConfigError::Version));
        let fallback = load_at_boot(bytes);
        assert_eq!(fallback.fault, Some(ConfigError::Version));
        assert_eq!(fallback.config.config(), &default_config());
    }
}

#[test]
fn shared_validation_preserves_control_cause_and_category() {
    type Case = (fn(&mut ControlMap), ControlError, ErrorCode);
    let cases: &[Case] = &[
        (|m| m.hold_ms = 399, ControlError::HoldDuration, ErrorCode::Bounds),
        (
            |m| m.screens[1].buttons[3].hold = ControlAction::Dashboard,
            ControlError::ReservedRecovery {
                page: ControlPage::Debug,
            },
            ErrorCode::Incompatible,
        ),
        (
            |m| m.screens[2].buttons[0].tap = ControlAction::CycleFps,
            ControlError::IncompatibleButton {
                page: ControlPage::Logs,
                button: ButtonId::A,
                hold: false,
            },
            ErrorCode::Incompatible,
        ),
        (
            |m| m.screens[0].buttons[3].tap = ControlAction::None,
            ControlError::UnreachableScreen {
                page: ControlPage::Debug,
            },
            ErrorCode::Reference,
        ),
    ];
    for (mutate, cause, code) in cases {
        let mut c = default_config();
        mutate(&mut c.controls);
        let e = validate(c).unwrap_err();
        assert_eq!(e.field, Field::Controls(*cause));
        assert_eq!(e.code, *code);
    }
}

#[cfg(feature = "json")]
#[test]
fn custom_current_controls_roundtrip_and_strict_import() {
    let mut c = default_config();
    c.controls.hold_ms = 1234;
    c.controls.screens[0].buttons[0].tap = ControlAction::PreviousView;
    let c = validate(c).unwrap();
    let json = to_json(&c).unwrap();
    assert_eq!(from_json_bytes(json.as_bytes()).unwrap(), c);
    let mut out = [0; MAX_FILE_BYTES];
    assert_eq!(decode(encode(&c, &mut out).unwrap()).unwrap(), c);
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();
    for mutate in [
        |v: &mut serde_json::Value| {
            v.as_object_mut().unwrap().remove("controls");
        },
        |v: &mut serde_json::Value| {
            v["controls"]["extra"] = true.into();
        },
        |v: &mut serde_json::Value| {
            v["controls"]["screens"][0]["buttons"][0]["tap"] = "Unknown".into();
        },
        |v: &mut serde_json::Value| {
            v["controls"]["screens"][1]["buttons"][0]["tap"] = "CycleFps".into();
        },
        |v: &mut serde_json::Value| {
            v["controls"]["screens"][0]["buttons"][3]["tap"] = "None".into();
        },
        |v: &mut serde_json::Value| {
            v["controls"]["screens"][0]["buttons"][3]["hold"] = "Dashboard".into();
        },
    ] {
        let mut v = value.clone();
        mutate(&mut v);
        assert!(from_json_bytes(&serde_json::to_vec(&v).unwrap()).is_err());
    }
}

#[cfg(feature = "json")]
#[test]
fn legacy_payload_version_canonicality_and_nested_shapes_are_checked() {
    fn reseal(bytes: &mut [u8]) {
        let mut checked = bytes[..8].to_vec();
        checked.extend_from_slice(&bytes[12..]);
        bytes[8..12].copy_from_slice(&crc32(&checked).to_le_bytes());
    }
    for fixture in [
        include_bytes!("fixtures/v1-default.eav").as_slice(),
        include_bytes!("fixtures/v2-default.eav").as_slice(),
    ] {
        let mut bad = fixture.to_vec();
        bad[12] = 3;
        reseal(&mut bad);
        assert_eq!(decode(&bad), Err(ConfigError::Version));
        let mut bad = fixture.to_vec();
        bad[14] = 0; // valid wire shape, nonexistent boot view
        reseal(&mut bad);
        assert!(matches!(
            decode(&bad),
            Err(ConfigError::Validation(ValidationError {
                field: Field::BootView,
                ..
            }))
        ));
        let mut bad = fixture.to_vec();
        bad[12] |= 128;
        bad.insert(13, 0);
        let len = (bad.len() - 12) as u16;
        bad[6..8].copy_from_slice(&len.to_le_bytes());
        reseal(&mut bad);
        assert_eq!(decode(&bad), Err(ConfigError::Payload));
        let mut bad = fixture.to_vec();
        bad.push(0);
        let len = (bad.len() - 12) as u16;
        bad[6..8].copy_from_slice(&len.to_le_bytes());
        reseal(&mut bad);
        assert_eq!(decode(&bad), Err(ConfigError::Payload));
    }
    for fixture in [
        include_bytes!("fixtures/v1-default.json").as_slice(),
        include_bytes!("fixtures/v2-default.json").as_slice(),
    ] {
        let value: serde_json::Value = serde_json::from_slice(fixture).unwrap();
        for path in ["display", "warning", "warnings", "gauge"] {
            if path == "gauge" && value["version"] == 1 {
                continue;
            }
            let mut v = value.clone();
            if path == "warnings" {
                v[path]["future"] = true.into();
            } else {
                v["widgets"][0][path]["future"] = true.into();
            }
            assert!(from_json_bytes(&serde_json::to_vec(&v).unwrap()).is_err());
        }
    }
}

#[cfg(feature = "json")]
#[test]
fn v2_import_preserves_custom_widget_values_and_colours() {
    let mut v: serde_json::Value = serde_json::from_slice(include_bytes!("fixtures/v2-default.json")).unwrap();
    v["views"][0]["name"] = "Historical".into();
    v["widgets"][0]["label"] = "CUSTOM".into();
    v["widgets"][0]["gauge"]["max"] = 4.0.into();
    v["widgets"][0]["colors"]["value"] = "#FFFF00".into();
    let c = from_json_bytes(&serde_json::to_vec(&v).unwrap()).unwrap();
    assert_eq!(c.config().controls, ControlMap::default());
    let mut upgraded: serde_json::Value = serde_json::from_str(&to_json(&c).unwrap()).unwrap();
    for key in ["controls", "history_seconds", "reduced_motion", "led", "orientation"] {
        upgraded.as_object_mut().unwrap().remove(key);
    }
    for view in upgraded["views"].as_array_mut().unwrap() {
        assert!(view["orientation"].is_null());
        view.as_object_mut().unwrap().remove("orientation");
    }
    for widget in upgraded["widgets"].as_array_mut().unwrap() {
        widget.as_object_mut().unwrap().remove("advanced");
        for key in ["trigger_high", "clear_high", "snooze_s", "priority", "latch"] {
            widget["warning"].as_object_mut().unwrap().remove(key);
        }
    }
    upgraded["version"] = 2.into();
    assert_eq!(upgraded, v);
    // Historical binary is checked against frozen schemas in legacy_v3 tests;
    // never manufacture it by stripping fields from an evolving current Widget.
}
