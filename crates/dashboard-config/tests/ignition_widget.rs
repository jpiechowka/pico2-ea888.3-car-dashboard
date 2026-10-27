//! Group-widget wire admission, precise repair and preserved scalar contracts.
#![cfg(feature = "json")]
use dashboard_config::*;

/// Return a one-widget schema-seven JSON candidate with readout `mode`, without
/// depending on the new factory. Panics if built-in defaults cannot serialize.
fn group_json(mode: &str) -> serde_json::Value {
    let mut value: serde_json::Value =
        serde_json::from_str(&to_json(&validate(default_config()).unwrap()).unwrap()).unwrap();
    value["version"] = 7.into();
    value["widgets"].as_array_mut().unwrap().truncate(1);
    let widget = &mut value["widgets"][0];
    widget["channel"] = "IgnitionRetard".into();
    widget["label"] = "IGN RETARD".into();
    widget["display"]["decimals"] = 1.into();
    for flag in ["graph", "average", "extrema", "trend", "afr"] {
        widget["display"][flag] = false.into();
    }
    widget["advanced"]["readout"] = mode.into();
    widget["warning"]["enabled"] = false.into();
    widget["warning"]["mode"] = "Low".into();
    widget["warning"]["trigger"] = 0.0.into();
    widget["warning"]["clear"] = 0.0.into();
    widget["warning"]["min_load_percent"] = 0.0.into();
    value
}

#[test]
/// Each explicit group mode survives JSON, binary and current-only boot decoding.
fn three_group_modes_roundtrip() {
    for mode in ["IgnitionNumeric", "IgnitionStatus", "IgnitionStatusAndNumber"] {
        let value = group_json(mode);
        let admitted = from_json_bytes(&serde_json::to_vec(&value).unwrap()).unwrap();
        let mut storage = [0; MAX_FILE_BYTES];
        let bytes = encode(&admitted, &mut storage).unwrap();
        assert_eq!(decode(bytes).unwrap(), admitted);
        assert_eq!(ConfigSlot::new().load_at_boot(bytes).config, admitted);
        let exported: serde_json::Value = serde_json::from_str(&to_json(&admitted).unwrap()).unwrap();
        assert_eq!(exported["widgets"][0]["advanced"]["readout"], mode);
    }
}

#[test]
/// Firmware storage rejects legacy schema six even when host migration admits it.
fn schema_six_is_host_only_and_slot_recovers() {
    let bytes = include_bytes!("fixtures/v6-default.eav");
    assert!(decode(bytes).is_ok());
    let mut slot = ConfigSlot::new();
    assert_eq!(slot.load_at_boot(bytes).fault, Some(ConfigError::Version));
    let current = validate(default_config()).unwrap();
    let mut storage = [0; MAX_FILE_BYTES];
    let encoded = encode(&current, &mut storage).unwrap();
    assert_eq!(slot.load_at_boot(encoded).fault, None);
}

#[test]
/// Each unavailable scalar feature has an exact repair path, without assigning a scalar channel.
fn unsupported_group_features_remain_repairable() {
    for (path, bad) in [
        ("/display/graph", serde_json::json!(true)),
        ("/display/average", serde_json::json!(true)),
        ("/display/extrema", serde_json::json!(true)),
        ("/display/trend", serde_json::json!(true)),
        ("/display/afr", serde_json::json!(true)),
        ("/gauge/kind", serde_json::json!("Circular")),
        ("/advanced/smoothing_ms", serde_json::json!(50)),
        ("/advanced/secondary", serde_json::json!("Psi")),
        ("/advanced/peak_mode", serde_json::json!("Maximum")),
        ("/advanced/reference_mode", serde_json::json!("Zero")),
        (
            "/advanced/bands",
            serde_json::json!([{"min":0.0,"max":1.0,"color":"#FFFFFF","label":""}]),
        ),
    ] {
        let mut value = group_json("IgnitionNumeric");
        *value["widgets"][0].pointer_mut(path).unwrap() = bad;
        let bytes = serde_json::to_vec(&value).unwrap();
        let candidate = parse_current_json_candidate(&bytes).unwrap();
        let mut issues = Vec::new();
        let _ = visit_validation(&candidate, |issue| {
            issues.push(issue);
            core::ops::ControlFlow::Continue(())
        });
        assert!(
            issues.iter().any(|issue| {
                issue.location.to_string() == format!("/widgets/0{path}") && issue.error.channel.is_none()
            }),
            "{path}: {issues:?}"
        );
        assert!(from_json_bytes(&bytes).is_err(), "{path}");
    }
}

/// Return a single group candidate with footprint `span` and mounting `orientation`.
fn group_config(
    span: Span,
    orientation: Orientation,
) -> Config {
    let mut config = default_config();
    config.widgets.clear();
    config.orientation = orientation;
    config
        .widgets
        .push(default_ignition_widget(
            1,
            1,
            Placement {
                column: 0,
                row: 0,
                span,
            },
        ))
        .unwrap();
    config
}

#[test]
/// Every supported group presentation fits every span/mounting and leaves scalar features inactive.
fn factory_geometry_modes_and_scalar_wire_identity() {
    println!(
        "Widget={} AdvancedDisplay={} Config={} BootConfig={}",
        core::mem::size_of::<Widget>(),
        core::mem::size_of::<AdvancedDisplay>(),
        core::mem::size_of::<Config>(),
        core::mem::size_of::<BootConfig>()
    );
    assert_eq!(Channel::ALL.len(), 8);
    assert_eq!(WidgetChannel::ALL.len(), 9);
    assert_eq!(core::mem::size_of::<WidgetChannel>(), core::mem::size_of::<Channel>());
    assert_eq!(core::mem::size_of::<AdvancedDisplay>(), 64);
    for (index, channel) in Channel::ALL.into_iter().enumerate() {
        let selector = WidgetChannel::from(channel);
        assert_eq!(selector.scalar(), Some(channel));
        assert_eq!(selector.label(), channel.label());
        assert_eq!(selector.domain(), channel.domain());
        let mut scalar = [0; 4];
        let mut selected = [0; 4];
        assert_eq!(
            postcard::to_slice(&channel, &mut scalar).unwrap(),
            postcard::to_slice(&selector, &mut selected).unwrap()
        );
        assert_eq!(selected[0], index as u8);
        assert_eq!(
            serde_json::to_string(&selector).unwrap(),
            serde_json::to_string(&channel).unwrap()
        );
    }
    for orientation in Orientation::ALL {
        for span in [Span::OneByOne, Span::TwoByOne, Span::TwoByTwo] {
            let config = group_config(span, orientation);
            let widget = &config.widgets[0];
            assert_eq!(widget.channel.scalar(), None);
            assert_eq!(widget.display.decimals, 1);
            assert_eq!(widget.advanced.readout, Readout::IgnitionStatusAndNumber);
            let capabilities = widget.capabilities();
            assert_eq!(
                capabilities,
                WidgetCapabilities {
                    secondary: false,
                    history: false,
                    markers: false,
                    bands: false,
                    band_labels: false,
                    statistics: false
                }
            );
            assert!(
                !widget.shows_graph() && !widget.shows_trend() && !widget.shows_average() && !widget.shows_extrema()
            );
            assert_eq!(
                default_widget_for_channel(widget.channel, widget.id, widget.view, widget.placement),
                *widget
            );
            for mode in [
                Readout::IgnitionNumeric,
                Readout::IgnitionStatus,
                Readout::IgnitionStatusAndNumber,
            ] {
                for decimals in 0..=3 {
                    let mut candidate = config.clone();
                    candidate.widgets[0].advanced.readout = mode;
                    candidate.widgets[0].display.decimals = decimals;
                    let admitted = validate(candidate).unwrap();
                    let mut storage = [0; MAX_FILE_BYTES];
                    assert_eq!(decode(encode(&admitted, &mut storage).unwrap()).unwrap(), admitted);
                }
                for channel in Channel::ALL {
                    assert!(!mode.compatible(channel));
                }
            }
        }
    }
}

#[test]
/// Dormant degree fields retain legal nondefaults while rejecting nonfinite/out-of-range/order violations.
fn dormant_group_fields_are_preserved_and_bounded() {
    let mut config = group_config(Span::OneByOne, Orientation::Landscape);
    let widget = &mut config.widgets[0];
    widget.gauge.expand = true;
    widget.gauge.min = -300.0;
    widget.gauge.max = 250.0;
    widget.display.graph_range = GraphRange::Fixed {
        min: -327.68,
        max: 327.67,
    };
    widget.advanced.reference = -123.45;
    widget.advanced.peak_hold_ms = 60000;
    widget.warning.mode = LimitMode::Low;
    widget.warning.trigger = -327.68;
    widget.warning.clear = 0.0;
    let admitted = validate(config.clone()).unwrap();
    assert_eq!(
        from_json_bytes(to_json(&admitted).unwrap().as_bytes()).unwrap(),
        admitted
    );
    /// Candidate mutation paired with its expected repair field.
    type Mutation = (fn(&mut Widget), Field);
    let mutations: &[Mutation] = &[
        (|w| w.display.decimals = 4, Field::Decimals),
        (|w| w.gauge.min = -327.69, Field::GaugeRange),
        (|w| w.gauge.max = 327.68, Field::GaugeRange),
        (|w| w.gauge.max = w.gauge.min, Field::GaugeRange),
        (|w| w.advanced.reference = f32::NAN, Field::Reference),
        (|w| w.advanced.reference = 327.68, Field::Reference),
        (|w| w.advanced.reference = -327.69, Field::Reference),
        (|w| w.advanced.peak_hold_ms = 99, Field::PeakHold),
        (
            |w| w.display.graph_range = GraphRange::Fixed { min: 1.0, max: 1.0 },
            Field::GraphRange,
        ),
        (
            |w| {
                w.display.graph_range = GraphRange::Fixed {
                    min: f32::NEG_INFINITY,
                    max: 1.0,
                }
            },
            Field::GraphRange,
        ),
        (|w| w.warning.trigger = f32::INFINITY, Field::WarningTrigger),
        (
            |w| {
                w.warning.trigger = -10.0;
                w.warning.clear = -20.0;
            },
            Field::WarningClear,
        ),
        (|w| w.warning.on_ms = 0, Field::WarningOn),
        (|w| w.warning.off_ms = 0, Field::WarningOff),
        (|w| w.warning.min_load_percent = 101.0, Field::WarningLoad),
    ];
    for (mutate, field) in mutations {
        let mut candidate = config.clone();
        mutate(&mut candidate.widgets[0]);
        let error = validate(candidate).unwrap_err();
        assert_eq!(error.field, *field);
        assert_eq!(error.channel, None);
    }
}

#[test]
/// All current group tags and new properties reject in each historical advanced-widget JSON shape.
fn frozen_schemas_reject_new_widget_vocabulary() {
    for fixture in [
        include_bytes!("fixtures/v4-default.json").as_slice(),
        include_bytes!("fixtures/v5-default.json"),
        include_bytes!("fixtures/v6-default.json"),
    ] {
        let original: serde_json::Value = serde_json::from_slice(fixture).unwrap();
        for (path, value) in [
            ("/channel", "IgnitionRetard"),
            ("/advanced/readout", "IgnitionNumeric"),
            ("/advanced/readout", "IgnitionStatus"),
            ("/advanced/readout", "IgnitionStatusAndNumber"),
        ] {
            let mut candidate = original.clone();
            *candidate["widgets"][0].pointer_mut(path).unwrap() = value.into();
            assert!(matches!(
                from_json_bytes(&serde_json::to_vec(&candidate).unwrap()),
                Err(ConfigError::Json { .. })
            ));
        }
        let mut candidate = original;
        candidate["widgets"][0]["advanced"]["unknown"] = true.into();
        assert!(matches!(
            from_json_bytes(&serde_json::to_vec(&candidate).unwrap()),
            Err(ConfigError::Json { .. })
        ));
    }
}

#[test]
/// Full mixed scalar/group documents preserve all sixty-four instances within unchanged byte limits.
fn maximum_mixed_document_stays_bounded() {
    for orientation in Orientation::ALL {
        let mut config = default_config();
        config.orientation = orientation;
        config.views.clear();
        config.widgets.clear();
        for view in 1..=8 {
            config
                .views
                .push(View {
                    id: view,
                    name: "Mixed".try_into().unwrap(),
                    orientation: None,
                })
                .unwrap();
            for cell in 0..8 {
                let selector = if cell % 2 == 0 {
                    WidgetChannel::IgnitionRetard
                } else {
                    Channel::ALL[usize::from(cell)].into()
                };
                let columns = orientation.geometry().columns();
                config
                    .widgets
                    .push(default_widget_for_channel(
                        selector,
                        u16::from(view) * 8 + u16::from(cell),
                        view,
                        Placement {
                            column: cell % columns,
                            row: cell / columns,
                            span: Span::OneByOne,
                        },
                    ))
                    .unwrap();
            }
        }
        assert_eq!(config.widgets.len(), 64);
        let admitted = validate(config).unwrap();
        let mut storage = [0; MAX_FILE_BYTES];
        let binary = encode(&admitted, &mut storage).unwrap();
        assert!(binary.len() <= 16384);
        assert_eq!(ConfigSlot::new().load_at_boot(binary).config, admitted);
        let json = to_json(&admitted).unwrap();
        assert!(json.len() <= 131072);
        assert_eq!(from_json_bytes(json.as_bytes()).unwrap(), admitted);
    }
}

/// Return a checksummed envelope for arbitrary `payload` and `version`, allowing negative wire tests.
fn envelope(
    version: u16,
    payload: &[u8],
) -> Vec<u8> {
    let mut bytes = b"EAV1".to_vec();
    bytes.extend_from_slice(&version.to_le_bytes());
    bytes.extend_from_slice(&(payload.len() as u16).to_le_bytes());
    let mut checked = bytes.clone();
    checked.extend_from_slice(payload);
    bytes.extend_from_slice(&crc32(&checked).to_le_bytes());
    bytes.extend_from_slice(payload);
    bytes
}

#[test]
/// Resealed legacy/new tags, unknown fields and trailing bytes cannot publish a partial group configuration.
fn group_shape_and_binary_consumption_fail_closed() {
    let config = validate(group_config(Span::OneByOne, Orientation::Landscape)).unwrap();
    let mut storage = [0; MAX_FILE_BYTES];
    let original = encode(&config, &mut storage).unwrap().to_vec();
    let mut slot = ConfigSlot::new();
    assert_eq!(slot.load_at_boot(&original).fault, None);
    let mut payload = original[HEADER_BYTES..].to_vec();
    payload.push(0);
    let trailing = envelope(VERSION, &payload);
    assert_eq!(decode(&trailing), Err(ConfigError::Payload));
    assert_eq!(slot.load_at_boot(&trailing).fault, Some(ConfigError::Payload));
    assert_eq!(slot.load_at_boot(&trailing).config, validate(default_config()).unwrap());
    payload.pop();
    payload[0] = 6;
    assert_eq!(decode(&envelope(6, &payload)), Err(ConfigError::Payload));
    assert_eq!(slot.load_at_boot(&original).config, config);
    for path in ["/channel", "/advanced/readout"] {
        let mut candidate = group_json("IgnitionNumeric");
        *candidate["widgets"][0].pointer_mut(path).unwrap() = "Unknown".into();
        assert!(matches!(
            from_json_bytes(&serde_json::to_vec(&candidate).unwrap()),
            Err(ConfigError::Json { .. })
        ));
    }
    let mut candidate = group_json("IgnitionNumeric");
    candidate["widgets"][0]["unknown"] = true.into();
    assert!(matches!(
        from_json_bytes(&serde_json::to_vec(&candidate).unwrap()),
        Err(ConfigError::Json { .. })
    ));
    let mut json = serde_json::to_vec(&group_json("IgnitionNumeric")).unwrap();
    json.extend_from_slice(b" {}");
    assert!(matches!(from_json_bytes(&json), Err(ConfigError::Json { .. })));
}

#[test]
/// Shared group warnings admit both popup presentations without allocating separate cylinder settings.
fn shared_group_warnings_accept_zero_hysteresis_and_both_presentations() {
    for presentation in ["Global", "CylinderLines"] {
        let mut value = group_json("IgnitionNumeric");
        let widget = &mut value["widgets"][0];
        widget["advanced"]["ignition_warning"] = presentation.into();
        widget["warning"]["enabled"] = true.into();
        widget["warning"]["mode"] = "Low".into();
        widget["warning"]["trigger"] = 0.0.into();
        widget["warning"]["clear"] = 0.0.into();
        widget["warning"]["min_load_percent"] = 0.0.into();
        widget["warning"]["message"] = "Ignition retarded".into();
        let admitted = from_json_bytes(&serde_json::to_vec(&value).unwrap()).unwrap();
        let exported: serde_json::Value = serde_json::from_str(&to_json(&admitted).unwrap()).unwrap();
        assert_eq!(exported["widgets"][0]["advanced"]["ignition_warning"], presentation);
    }
    let config = group_config(Span::OneByOne, Orientation::Landscape);
    let warning = &config.widgets[0].warning;
    assert!(warning.enabled);
    assert_eq!(warning.message.as_str(), "Ignition retarded");
    assert_eq!(warning.mode, LimitMode::Low);
    assert_eq!(warning.trigger, 0.0);
    assert_eq!(warning.clear, 0.0);
    assert_eq!(warning.min_load_percent, 0.0);
    assert_eq!(warning.applicability, Applicability::DemoOnly);
}

#[test]
/// Group-only threshold/applicability invariants reject even disabled candidates, without weakening scalar hysteresis.
fn shared_warning_bounds_and_repair_locations_are_explicit() {
    let baseline = group_config(Span::OneByOne, Orientation::Landscape);
    /// One stored edit, its expected field and exact candidate JSON repair suffix.
    type WarningMutation = (fn(&mut Widget), Field, &'static str);
    let mutations: &[WarningMutation] = &[
        (
            |w| w.warning.mode = LimitMode::High,
            Field::WarningMode,
            "/warning/mode",
        ),
        (
            |w| w.warning.applicability = Applicability::VehicleAndDemo,
            Field::WarningApplicability,
            "/warning/applicability",
        ),
        (
            |w| w.warning.min_load_percent = 1.0,
            Field::WarningLoad,
            "/warning/min_load_percent",
        ),
        (|w| w.warning.trigger = 0.01, Field::WarningTrigger, "/warning/trigger"),
        (
            |w| w.warning.trigger = -327.69,
            Field::WarningTrigger,
            "/warning/trigger",
        ),
        (|w| w.warning.clear = 0.01, Field::WarningClear, "/warning/clear"),
        (|w| w.warning.clear = -1.0, Field::WarningClear, "/warning/clear"),
    ];
    for enabled in [false, true] {
        for (mutate, field, path) in mutations {
            let mut config = baseline.clone();
            config.widgets[0].warning.enabled = enabled;
            mutate(&mut config.widgets[0]);
            assert_eq!(validate(config.clone()).unwrap_err().field, *field);
            let mut found = false;
            let _ = visit_validation(&config, |issue| {
                if issue.error.field == *field {
                    assert_eq!(issue.location.to_string(), format!("/widgets/0{path}"));
                    found = true;
                }
                core::ops::ControlFlow::Continue(())
            });
            assert!(found);
        }
    }
    for (trigger, clear) in [(-327.68, -327.68), (-327.68, 0.0), (-2.5, -1.0), (0.0, 0.0)] {
        let mut config = baseline.clone();
        config.widgets[0].warning.trigger = trigger;
        config.widgets[0].warning.clear = clear;
        validate(config).unwrap();
    }
    let mut scalar = default_config();
    scalar.widgets.truncate(1);
    scalar.widgets[0].warning.mode = LimitMode::Low;
    scalar.widgets[0].warning.trigger = 0.0;
    scalar.widgets[0].warning.clear = 0.0;
    assert_eq!(validate(scalar).unwrap_err().field, Field::WarningClear);
    let mut scalar = default_config();
    scalar.widgets[0].advanced.ignition_warning = IgnitionWarningPresentation::CylinderLines;
    assert_eq!(validate(scalar).unwrap_err().field, Field::IgnitionWarningPresentation);
}

#[test]
/// The new required bounded selector rejects unknown/missing tags and never enters historical widget schemas.
fn ignition_warning_selector_has_a_closed_current_wire_contract() {
    for presentation in [
        IgnitionWarningPresentation::Global,
        IgnitionWarningPresentation::CylinderLines,
    ] {
        let mut config = group_config(Span::OneByOne, Orientation::Landscape);
        config.widgets[0].advanced.ignition_warning = presentation;
        let admitted = validate(config).unwrap();
        let mut storage = [0; MAX_FILE_BYTES];
        let bytes = encode(&admitted, &mut storage).unwrap();
        assert_eq!(decode(bytes).unwrap(), admitted);
        assert_eq!(ConfigSlot::new().load_at_boot(bytes).config, admitted);
    }
    for fixture in [
        include_bytes!("fixtures/v4-default.json").as_slice(),
        include_bytes!("fixtures/v5-default.json"),
        include_bytes!("fixtures/v6-default.json"),
    ] {
        let mut value: serde_json::Value = serde_json::from_slice(fixture).unwrap();
        value["widgets"][0]["advanced"]["ignition_warning"] = "Global".into();
        assert!(matches!(
            from_json_bytes(&serde_json::to_vec(&value).unwrap()),
            Err(ConfigError::Json { .. })
        ));
    }
    let original = group_json("IgnitionNumeric");
    let mut unknown = original.clone();
    unknown["widgets"][0]["advanced"]["ignition_warning"] = "Unknown".into();
    assert!(matches!(
        from_json_bytes(&serde_json::to_vec(&unknown).unwrap()),
        Err(ConfigError::Json { .. })
    ));
    let mut missing = original;
    missing["widgets"][0]["advanced"]
        .as_object_mut()
        .unwrap()
        .remove("ignition_warning");
    assert!(matches!(
        from_json_bytes(&serde_json::to_vec(&missing).unwrap()),
        Err(ConfigError::Json { .. })
    ));
}

#[test]
/// Malformed popup tags cannot bypass canonical admission or leave a partially decoded group in a reused slot.
fn binary_popup_selector_failures_restore_defaults_and_allow_reload() {
    let mut config = group_config(Span::OneByOne, Orientation::PortraitInverted);
    config.views[0].name = "Retard test".try_into().unwrap();
    config.controls.hold_ms = 1234;
    config.history_seconds = [900; 8];
    config.fuel = Fuel::Stoichiometric { ratio: 12.5 };
    let global = validate(config.clone()).unwrap();
    config.widgets[0].advanced.ignition_warning = IgnitionWarningPresentation::CylinderLines;
    let lines = validate(config).unwrap();
    let mut storage = [0; MAX_FILE_BYTES];
    let global_bytes = encode(&global, &mut storage).unwrap().to_vec();
    let lines_bytes = encode(&lines, &mut storage).unwrap().to_vec();
    let payload = &lines_bytes[HEADER_BYTES..];
    // Locate the one changed logical field without hard-coding offsets of earlier variable-length fields.
    let differences: Vec<_> = global_bytes[HEADER_BYTES..]
        .iter()
        .zip(payload)
        .enumerate()
        .filter_map(|(index, (global, lines))| (global != lines).then_some(index))
        .collect();
    assert_eq!(global_bytes.len(), lines_bytes.len());
    assert_eq!(differences.len(), 1);
    let selector = differences[0];
    assert_eq!(global_bytes[HEADER_BYTES + selector], 0);
    assert_eq!(payload[selector], 1);

    let mut unknown = payload.to_vec();
    unknown[selector] = 2;
    let mut overlong = payload[..selector].to_vec();
    overlong.extend_from_slice(&[0x81, 0]);
    overlong.extend_from_slice(&payload[selector + 1..]);
    let missing = payload[..selector].to_vec();
    let mut truncated = missing.clone();
    truncated.push(0x81);
    let defaults = validate(default_config()).unwrap();
    let mut slot = ConfigSlot::new();
    for (name, bad_payload) in [
        ("unknown", unknown),
        ("overlong", overlong),
        ("missing", missing),
        ("truncated varint", truncated),
    ] {
        let loaded = slot.load_at_boot(&lines_bytes);
        assert_eq!(loaded.fault, None, "{name}: preload");
        assert_eq!(loaded.config, lines, "{name}: preload");
        let bad = envelope(VERSION, &bad_payload);
        assert_eq!(decode(&bad), Err(ConfigError::Payload), "{name}: owned decoder");
        let loaded = slot.load_at_boot(&bad);
        assert_eq!(loaded.fault, Some(ConfigError::Payload), "{name}: boot fault");
        assert_eq!(loaded.config, defaults, "{name}: complete fallback");
        assert_eq!(
            loaded.config.stoich(),
            defaults.stoich(),
            "{name}: derived fuel fallback"
        );
        let loaded = slot.load_at_boot(&lines_bytes);
        assert_eq!(loaded.fault, None, "{name}: reload");
        assert_eq!(loaded.config, lines, "{name}: reload");
        assert_eq!(loaded.config.stoich(), 12.5, "{name}: derived fuel reload");
    }
}

#[test]
/// Enabled group latch/snooze needs an onboard Dashboard acknowledgment; an encoder alone is insufficient.
fn group_warning_acknowledgment_requires_reachable_onboard_binding() {
    for presentation in [
        IgnitionWarningPresentation::Global,
        IgnitionWarningPresentation::CylinderLines,
    ] {
        for (enabled, latch, snooze_s, needs_ack) in [
            (true, false, 0, false),
            (true, true, 0, true),
            (true, false, 5, true),
            (true, false, 900, true),
            (true, true, 900, true),
            (false, true, 0, false),
            (false, false, 5, false),
            (false, true, 900, false),
        ] {
            let mut config = group_config(Span::OneByOne, Orientation::Landscape);
            let widget = &mut config.widgets[0];
            widget.advanced.ignition_warning = presentation;
            widget.warning.enabled = enabled;
            widget.warning.advanced.latch = latch;
            widget.warning.advanced.snooze_s = snooze_s;
            for encoder_only in [false, true] {
                let mut candidate = config.clone();
                if encoder_only {
                    candidate.controls.screens[ControlPage::Dashboard.index()].encoder_click =
                        ControlAction::AcknowledgeWarnings;
                }
                if needs_ack {
                    let error = validate(candidate).unwrap_err();
                    assert_eq!(error.field, Field::WarningAcknowledge);
                    assert_eq!(error.code, ErrorCode::Reference);
                    assert_eq!(error.widget, Some(1));
                    assert_eq!(error.channel, None);
                } else {
                    validate(candidate).unwrap();
                }
            }
            for hold in [false, true] {
                let mut candidate = config.clone();
                let button = &mut candidate.controls.screens[ControlPage::Dashboard.index()].buttons[2];
                if hold {
                    button.hold = ControlAction::AcknowledgeWarnings;
                } else {
                    button.tap = ControlAction::AcknowledgeWarnings;
                }
                validate(candidate).unwrap();
            }
        }
    }
}
