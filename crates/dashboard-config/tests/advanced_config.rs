#![cfg(feature = "json")]
use dashboard_config::*;

#[test]
fn v4_complete_defaults_and_compact_output() {
    let config = validate(default_config()).unwrap();
    assert_eq!(VERSION, 4, "the complete advanced contract is version four");
    let json = to_json(&config).unwrap();
    assert!(!json.contains('\n'));
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(value["history_seconds"], serde_json::json!(vec![120; 8]));
    assert_eq!(value["reduced_motion"], false);
    assert_eq!(value["widgets"][0]["advanced"]["smoothing_ms"], 0);
    assert_eq!(value["widgets"][0]["warning"]["priority"], "Normal");
    assert_eq!(value["led"]["warning"]["period_ms"], 400);
}

#[test]
fn legacy_v3_migrates_to_current_defaults() {
    assert_eq!(
        decode(include_bytes!("fixtures/v3-default.eav")).unwrap(),
        validate(default_config()).unwrap()
    );
    assert_eq!(
        from_json_bytes(include_bytes!("fixtures/v3-default.json")).unwrap(),
        validate(default_config()).unwrap()
    );
    assert_eq!(
        decode(include_bytes!("fixtures/v3-maximum.eav"))
            .unwrap()
            .config()
            .widgets
            .len(),
        64
    );
}

fn advanced_json() -> serde_json::Value {
    let mut c: serde_json::Value = serde_json::from_slice(include_bytes!("fixtures/v3-default.json")).unwrap();
    c["version"] = serde_json::json!(4);
    c["history_seconds"] = serde_json::json!(vec![120; 8]);
    c["reduced_motion"] = false.into();
    c["led"] = serde_json::json!({
        "normal":{"kind":"Solid","color":"#00FF00","brightness_percent":100,"period_ms":400,"on_percent":50},
        "warning":{"kind":"Blink","color":"#FF0000","brightness_percent":100,"period_ms":400,"on_percent":50},
        "sync_warning_blink":true,"during_feedback":false,"on_debug":false,"on_logs":false,"backlight_off":false
    });
    for w in c["widgets"].as_array_mut().unwrap() {
        let readout = match w["channel"].as_str().unwrap() {
            "Boost" => "FollowBoost",
            "Battery" => "Volts",
            "Lambda" => "LambdaAndAfr",
            _ => "Celsius",
        };
        w["advanced"] = serde_json::json!({"bands":[],"reference":0.0,"smoothing_ms":0,"peak_hold_ms":750,"readout":readout,"secondary":"None","peak_mode":"Off","reference_mode":"Off"});
        for (k, v) in [
            ("trigger_high", serde_json::json!(0.0)),
            ("clear_high", serde_json::json!(0.0)),
            ("snooze_s", serde_json::json!(0)),
            ("priority", serde_json::json!("Normal")),
            ("latch", serde_json::json!(false)),
        ] {
            w["warning"][k] = v;
        }
    }
    c
}

#[test]
fn advanced_bounds_reject_precise_fields() {
    for (path, value, field, code) in [
        (
            "/widgets/0/advanced/smoothing_ms",
            serde_json::json!(49),
            "Smoothing",
            ErrorCode::Bounds,
        ),
        (
            "/widgets/0/advanced/smoothing_ms",
            serde_json::json!(5001),
            "Smoothing",
            ErrorCode::Bounds,
        ),
        (
            "/widgets/0/advanced/peak_hold_ms",
            serde_json::json!(99),
            "PeakHold",
            ErrorCode::Bounds,
        ),
        (
            "/widgets/0/advanced/peak_hold_ms",
            serde_json::json!(60001),
            "PeakHold",
            ErrorCode::Bounds,
        ),
        (
            "/widgets/0/advanced/readout",
            serde_json::json!("Celsius"),
            "Readout",
            ErrorCode::Incompatible,
        ),
        (
            "/widgets/1/display/afr",
            serde_json::json!(false),
            "Afr",
            ErrorCode::Incompatible,
        ),
        (
            "/widgets/2/advanced/secondary",
            serde_json::json!("Bar"),
            "Secondary",
            ErrorCode::Incompatible,
        ),
        (
            "/history_seconds/0",
            serde_json::json!(9),
            "HistorySeconds(Boost)",
            ErrorCode::Bounds,
        ),
        (
            "/history_seconds/7",
            serde_json::json!(901),
            "HistorySeconds(Lambda)",
            ErrorCode::Bounds,
        ),
        (
            "/widgets/0/warning/snooze_s",
            serde_json::json!(4),
            "WarningSnooze",
            ErrorCode::Bounds,
        ),
        (
            "/widgets/0/warning/snooze_s",
            serde_json::json!(901),
            "WarningSnooze",
            ErrorCode::Bounds,
        ),
        (
            "/widgets/0/warning/trigger_high",
            serde_json::json!(1.0),
            "WarningTriggerHigh",
            ErrorCode::Bounds,
        ),
        (
            "/led/normal/brightness_percent",
            serde_json::json!(101),
            "LedBrightness(false)",
            ErrorCode::Bounds,
        ),
        (
            "/led/warning/period_ms",
            serde_json::json!(199),
            "LedPeriod(true)",
            ErrorCode::Bounds,
        ),
        (
            "/led/warning/on_percent",
            serde_json::json!(91),
            "LedOnPercent(true)",
            ErrorCode::Bounds,
        ),
    ] {
        let mut c = advanced_json();
        *c.pointer_mut(path).unwrap() = value;
        let err = from_json_bytes(&serde_json::to_vec(&c).unwrap()).unwrap_err();
        let ConfigError::Validation(err) = err else {
            panic!("expected {field}, got {err:?}")
        };
        assert_eq!(format!("{:?}", err.field), field);
        assert_eq!(err.code, code);
    }
}

#[test]
fn band_wire_is_bounded_strict_and_preserves_escaped_printable_text() {
    let mut c = advanced_json();
    let band = serde_json::json!({"min":-1.0,"max":0.0,"color":"#FFFFFF","label":"\"\\abc"});
    c["widgets"][0]["advanced"]["bands"] = serde_json::json!([band]);
    let accepted = from_json_bytes(&serde_json::to_vec(&c).unwrap()).unwrap();
    assert_eq!(accepted.config().widgets[0].advanced.bands()[0].label(), "\"\\abc");
    let json = to_json(&accepted).unwrap();
    assert!(!json.contains("label_len") && !json.contains("label_bytes") && !json.contains("band_count"));
    for label in ["123456", "     ", "\n", "é"] {
        let mut bad = c.clone();
        bad["widgets"][0]["advanced"]["bands"][0]["label"] = label.into();
        assert!(matches!(
            from_json_bytes(&serde_json::to_vec(&bad).unwrap()),
            Err(ConfigError::Json { .. })
        ));
    }
    let mut bad = c.clone();
    bad["widgets"][0]["advanced"]["bands"] = serde_json::json!([band, band, band, band]);
    assert!(matches!(
        from_json_bytes(&serde_json::to_vec(&bad).unwrap()),
        Err(ConfigError::Json { .. })
    ));
    for key in ["band_count", "padding", "unknown"] {
        let mut bad = c.clone();
        bad["widgets"][0]["advanced"][key] = 0.into();
        assert!(matches!(
            from_json_bytes(&serde_json::to_vec(&bad).unwrap()),
            Err(ConfigError::Json { .. })
        ));
    }
    let mut a = accepted.config().widgets[0].advanced;
    let err = a.set_bands(&[Band::EMPTY; 4]).unwrap_err();
    assert_eq!(err.field, Field::BandCount);
    assert_eq!(err.code, ErrorCode::Bounds);
    a.set_bands(&[]).unwrap();
    assert_eq!(a, AdvancedDisplay::for_channel(Channel::Boost, false));
}

fn error(
    c: Config,
    field: Field,
    code: ErrorCode,
) {
    let e = validate(c).unwrap_err();
    assert_eq!(e.field, field);
    assert_eq!(e.code, code);
}
#[test]
fn hidden_active_nonfinite_and_band_domains_never_escape_validation() {
    for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        for mode in [ReferenceMode::Off, ReferenceMode::Zero, ReferenceMode::Manual] {
            let mut c = default_config();
            c.widgets[0].advanced.reference = bad;
            c.widgets[0].advanced.reference_mode = mode;
            error(c, Field::Reference, ErrorCode::Bounds);
        }
        for (field, high) in [(Field::WarningTriggerHigh, true), (Field::WarningClearHigh, false)] {
            let mut c = default_config();
            if high {
                c.widgets[0].warning.advanced.trigger_high = bad;
            } else {
                c.widgets[0].warning.advanced.clear_high = bad;
            }
            error(c, field, ErrorCode::Bounds);
        }
        for index in 0..3 {
            let mut c = default_config();
            let mut bands = [
                Band::new(-1.0, 0.0, Color(0xFFFF), "").unwrap(),
                Band::new(0.0, 1.0, Color(0xFFFF), "").unwrap(),
                Band::new(1.0, 2.0, Color(0xFFFF), "").unwrap(),
            ];
            bands[index].min = bad;
            c.widgets[0].advanced.set_bands(&bands).unwrap();
            error(c, Field::BandMinimum(index as u8), ErrorCode::Bounds);
        }
    }
    let mut c = default_config();
    c.widgets[0]
        .advanced
        .set_bands(&[
            Band::new(0.0, 1.0, Color(0xFFFF), "").unwrap(),
            Band::new(0.5, 2.0, Color(0xFFFF), "").unwrap(),
        ])
        .unwrap();
    error(c, Field::BandOrder(1), ErrorCode::Overlap);
    let mut c = default_config();
    let track = c.widgets[0].colors.track;
    c.widgets[0]
        .advanced
        .set_bands(&[Band::new(0.0, 1.0, track, "").unwrap()])
        .unwrap();
    error(c, Field::BandColor(0), ErrorCode::Contrast);
}

#[test]
fn window_hysteresis_and_acknowledgment_are_independent_of_enabled() {
    for (mode, t, clear, th, ch) in [
        (LimitMode::OutsideWindow, 0.0, 1.0, 3.0, 2.0),
        (LimitMode::InsideWindow, 1.0, 0.0, 2.0, 3.0),
    ] {
        let mut c = default_config();
        let w = &mut c.widgets[0].warning;
        w.mode = mode;
        w.trigger = t;
        w.clear = clear;
        w.advanced.trigger_high = th;
        w.advanced.clear_high = ch;
        validate(c.clone()).unwrap();
        for enabled in [false, true] {
            let mut bad = c.clone();
            bad.widgets[0].warning.enabled = enabled;
            bad.widgets[0].warning.advanced.clear_high = th;
            error(bad, Field::WarningClear, ErrorCode::Hysteresis);
        }
    }
    let mut c = default_config();
    c.widgets[1].warning.mode = LimitMode::OutsideWindow;
    c.widgets[1].warning.advanced.trigger_high = 2.0;
    c.widgets[1].warning.advanced.clear_high = 1.0;
    error(c, Field::WarningMode, ErrorCode::Incompatible);
    for snooze in [0, 5, 900] {
        let mut c = default_config();
        c.widgets[3].warning.advanced.latch = true;
        c.widgets[3].warning.advanced.snooze_s = snooze;
        error(c.clone(), Field::WarningAcknowledge, ErrorCode::Reference);
        c.controls.screens[0].encoder_click = ControlAction::AcknowledgeWarnings;
        error(c.clone(), Field::WarningAcknowledge, ErrorCode::Reference);
        c.controls.screens[0].buttons[0].tap = ControlAction::AcknowledgeWarnings;
        validate(c).unwrap();
    }
}

#[test]
fn all_inclusive_boundaries_capabilities_and_slot_equivalence() {
    for smoothing in [0, 50, 250, 750, 5000] {
        for hold in [100, 750, 60000] {
            let mut c = default_config();
            c.widgets[0].advanced.smoothing_ms = smoothing;
            c.widgets[0].advanced.peak_hold_ms = hold;
            c.history_seconds = [10, 30, 120, 300, 900, 10, 900, 120];
            let v = validate(c).unwrap();
            let mut buf = [0; MAX_FILE_BYTES];
            let bytes = encode(&v, &mut buf).unwrap();
            assert_eq!(decode(bytes).unwrap(), v);
            assert_eq!(ConfigSlot::new().load_at_boot(bytes).config, v);
        }
    }
    for kind in GaugeKind::ALL {
        for span in [Span::OneByOne, Span::TwoByOne, Span::TwoByTwo] {
            let mut w = default_config().widgets[0].clone();
            w.gauge.kind = kind;
            w.placement.span = span;
            let c = w.capabilities();
            assert_eq!(c.history, kind == GaugeKind::Numeric);
            assert_eq!(c.markers, kind != GaugeKind::Numeric);
            assert_eq!(c.secondary, kind == GaugeKind::Numeric || span != Span::OneByOne);
            assert_eq!(c.band_labels, kind != GaugeKind::Numeric && span == Span::TwoByTwo);
            assert_eq!(c.statistics, kind == GaugeKind::Numeric || span == Span::TwoByTwo);
        }
    }
}

#[test]
fn frozen_v3_rejects_v4_only_nested_vocabulary() {
    let original = core::str::from_utf8(include_bytes!("fixtures/v3-default.json")).unwrap();
    for bad in [
        original.replacen("ToggleBoostUnits", "AcknowledgeWarnings", 1),
        original.replacen("\"High\"", "\"OutsideWindow\"", 1),
        original.replacen("\"High\"", "\"InsideWindow\"", 1),
        original.replacen("\"decimals\": 2", "\"readout\":\"FollowBoost\",\"decimals\": 2", 1),
    ] {
        assert_ne!(bad, original);
        assert!(matches!(from_json_bytes(bad.as_bytes()), Err(ConfigError::Json { .. })));
    }
}

#[test]
fn inactive_reference_is_retained_but_manual_activation_is_validated() {
    for mode in [ReferenceMode::Off, ReferenceMode::Zero] {
        let mut c = default_config();
        c.widgets[1].advanced.reference = -5.0;
        c.widgets[1].advanced.reference_mode = mode;
        validate(c.clone()).unwrap();
        c.widgets[1].advanced.reference_mode = ReferenceMode::Manual;
        error(c, Field::Reference, ErrorCode::Bounds);
    }
}

#[test]
fn inactive_high_endpoints_require_canonical_positive_zero() {
    let mut c = default_config();
    c.widgets[0].warning.advanced.trigger_high = -0.0;
    error(c, Field::WarningTriggerHigh, ErrorCode::Bounds);
    let mut c = default_config();
    c.widgets[0].warning.advanced.clear_high = -0.0;
    error(c, Field::WarningClearHigh, ErrorCode::Bounds);
}

#[test]
fn band_edit_errors_target_fields_without_mutating_stored_state() {
    let mut display = AdvancedDisplay::for_channel(Channel::Boost, false);
    let before = display;
    let error = display.set_bands(&[Band::EMPTY; 4]).unwrap_err();
    assert_eq!(
        format!("{error:?}"),
        "ValidationError { code: Bounds, field: BandCount, view: None, widget: None, channel: None }"
    );
    assert_eq!(display, before);
    display
        .set_bands(&[
            Band::new(-1.0, 0.0, Color(65535), "LOW").unwrap(),
            Band::new(0.0, 1.0, Color(65535), "MID").unwrap(),
        ])
        .unwrap();
    let before = display;
    for label in ["123456", "     ", "é", "\n"] {
        let e = display.set_band_label(1, label).unwrap_err();
        assert_eq!(e.field, Field::BandLabel(1));
        assert_eq!(e.code, ErrorCode::Text);
        assert_eq!(display, before);
    }
    for index in [2, 3, usize::MAX] {
        let e = display.set_band_label(index, "OK").unwrap_err();
        assert_eq!(e.field, Field::BandCount);
        assert_eq!(e.code, ErrorCode::Bounds);
        assert_eq!(display, before);
    }
    display.set_band_label(1, "NEW").unwrap();
    assert_eq!(display.bands()[1].label(), "NEW");
}

#[test]
fn new_enum_spellings_are_bounded_and_unknown_values_report_json_location() {
    let values = [
        serde_json::to_value([
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
        ])
        .unwrap(),
        serde_json::to_value([Secondary::None, Secondary::Bar, Secondary::Psi, Secondary::Kpa]).unwrap(),
        serde_json::to_value([PeakSelection::Off, PeakSelection::Maximum, PeakSelection::Minimum]).unwrap(),
        serde_json::to_value([ReferenceMode::Off, ReferenceMode::Zero, ReferenceMode::Manual]).unwrap(),
        serde_json::to_value([Priority::Low, Priority::Normal, Priority::High, Priority::Critical]).unwrap(),
        serde_json::to_value([
            LedPatternKind::Off,
            LedPatternKind::Solid,
            LedPatternKind::Blink,
            LedPatternKind::Breathe,
        ])
        .unwrap(),
        serde_json::to_value([LimitMode::OutsideWindow, LimitMode::InsideWindow]).unwrap(),
        serde_json::to_value([ControlAction::AcknowledgeWarnings]).unwrap(),
    ];
    for values in values {
        for value in values.as_array().unwrap() {
            let text = value.as_str().unwrap();
            assert!(text.is_ascii() && text.len() <= 20);
        }
    }
    for path in [
        "/widgets/0/advanced/readout",
        "/widgets/0/advanced/secondary",
        "/widgets/0/advanced/peak_mode",
        "/widgets/0/advanced/reference_mode",
        "/widgets/0/warning/priority",
        "/led/normal/kind",
    ] {
        let mut c = advanced_json();
        *c.pointer_mut(path).unwrap() = "NotAChoice".into();
        let text = serde_json::to_string(&c).unwrap();
        assert_eq!(
            from_json_bytes(text.as_bytes()),
            Err(ConfigError::Json {
                line: 1,
                column: text.find("\"NotAChoice\"").unwrap() + 12
            })
        );
    }
}

#[test]
fn pretty_reordered_advanced_json_normalizes_to_exact_compact_wire() {
    let original = validate(default_config()).unwrap();
    let compact = to_json(&original).unwrap();
    // serde_json's sorted Value maps reorder Config and all nested custom fields.
    let reordered: serde_json::Value = serde_json::from_str(&compact).unwrap();
    let pretty = serde_json::to_string_pretty(&reordered).unwrap();
    assert!(pretty.contains('\n'));
    assert!(pretty.find("\"bands\"").unwrap() < pretty.find("\"smoothing_ms\"").unwrap());
    let imported = from_json_bytes(pretty.as_bytes()).unwrap();
    assert_eq!(to_json(&imported).unwrap(), compact);
    let mut a = [0; MAX_FILE_BYTES];
    let mut b = [0; MAX_FILE_BYTES];
    let bytes = encode(&original, &mut a).unwrap();
    assert_eq!(bytes, encode(&imported, &mut b).unwrap());
    assert_eq!(ConfigSlot::new().load_at_boot(bytes).config, original);
}
