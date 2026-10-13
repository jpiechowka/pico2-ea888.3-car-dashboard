use dashboard_config::*;

#[test]
fn default_roundtrip_and_all_truncations_are_checked() {
    let valid = validate(default_config()).unwrap();
    let mut output = [0; MAX_FILE_BYTES];
    let bytes = encode(&valid, &mut output).unwrap();
    assert_eq!(decode(bytes).unwrap().config(), valid.config());
    assert_eq!(&bytes[..4], b"EAV1");
    assert_eq!(usize::from(u16::from_le_bytes([bytes[6], bytes[7]])) + 12, bytes.len());
    for cut in 0..bytes.len() {
        assert!(decode(&bytes[..cut]).is_err(), "accepted truncation at {cut}");
    }
    let mut trailing = bytes.to_vec();
    trailing.push(0);
    assert!(decode(&trailing).is_err());
    for index in 0..bytes.len() {
        let mut corrupt = bytes.to_vec();
        corrupt[index] ^= 0x80;
        assert!(decode(&corrupt).is_err(), "accepted corrupt byte {index}");
    }
    for capacity in [0, 1, 11, bytes.len() - 1] {
        let mut small = vec![0; capacity];
        assert!(encode(&valid, &mut small).is_err());
    }
}

#[test]
fn geometry_identity_text_and_numeric_errors_reject_the_whole_document() {
    let mutations: &[fn(&mut Config)] = &[
        |c| c.version = VERSION + 1,
        |c| c.catalogue_version = 2,
        |c| c.boot_view = 0,
        |c| c.views.clear(),
        |c| c.views[0].name.clear(),
        |c| c.views[0].name = "\n".try_into().unwrap(),
        |c| c.widgets[1].id = c.widgets[0].id,
        |c| c.widgets[0].id = 0,
        |c| c.widgets[0].view = 255,
        |c| c.widgets[1].placement = c.widgets[0].placement,
        |c| c.widgets[0].placement.column = 255,
        |c| c.widgets[0].placement.row = 2,
        |c| c.widgets[0].label = "THIRTEENCHARS".try_into().unwrap(),
        |c| c.widgets[0].label = "é".try_into().unwrap(),
        |c| c.widgets[0].display.decimals = 4,
        |c| c.widgets[0].display.afr = true,
        |c| c.widgets[0].display.graph_range = GraphRange::Fixed { min: 2.0, max: 1.0 },
        |c| c.widgets[0].warning.trigger = f32::NAN,
        |c| c.widgets[0].warning.on_ms = 0,
        |c| c.widgets[0].warning.off_ms = 30001,
        |c| c.widgets[0].warning.min_load_percent = 101.0,
        |c| c.widgets[0].warning.message = "   ".try_into().unwrap(),
        |c| c.warnings.page_ms = 0,
        |c| c.fuel = Fuel::Stoichiometric { ratio: f32::INFINITY },
        |c| c.sources[0].freshness_ms = 501,
    ];
    for (index, mutate) in mutations.iter().enumerate() {
        let mut config = default_config();
        mutate(&mut config);
        assert!(validate(config).is_err(), "accepted invalid mutation {index}");
    }
}

#[test]
fn all_spans_empty_view_and_maximum_document_are_valid() {
    for (span, width, height) in [
        (Span::OneByOne, 80, 107),
        (Span::TwoByOne, 160, 107),
        (Span::TwoByTwo, 160, 214),
    ] {
        let mut config = default_config();
        config.widgets.truncate(1);
        config.widgets[0].placement.span = span;
        assert_eq!(
            config.widgets[0].placement.rect().unwrap(),
            PixelRect {
                x: 0,
                y: 26,
                width,
                height
            }
        );
        validate(config).unwrap();
    }
    let mut empty = default_config();
    empty.widgets.clear();
    validate(empty).unwrap();
    let mut maximum = default_config();
    let template = maximum.widgets.clone();
    for view in 2..=8 {
        maximum
            .views
            .push(View {
                id: view,
                name: "View".try_into().unwrap(),
            })
            .unwrap();
        for (slot, widget) in template.iter().enumerate() {
            let mut widget = widget.clone();
            widget.view = view;
            widget.id = (u16::from(view) - 1) * 8 + slot as u16 + 1;
            maximum.widgets.push(widget).unwrap();
        }
    }
    assert_eq!(maximum.widgets.len(), MAX_WIDGETS);
    let valid = validate(maximum).unwrap();
    let mut output = [0; MAX_FILE_BYTES];
    let bytes = encode(&valid, &mut output).unwrap();
    assert_eq!(decode(bytes).unwrap().config(), valid.config());
}

#[test]
fn fuel_mass_volume_and_endpoints_match_independent_values() {
    for (percent, expected) in [(0.0, 14.7), (5.0, 14.395964), (10.0, 14.094058), (100.0, 9.0)] {
        let mut config = default_config();
        config.fuel = Fuel::EthanolBlend(Blend {
            percent,
            ..Blend::illustrative()
        });
        assert!((validate(config).unwrap().stoich() - expected).abs() < 0.00001);
    }
    let mut config = default_config();
    config.fuel = Fuel::EthanolBlend(Blend {
        percent: 5.0,
        basis: BlendBasis::Mass,
        ..Blend::illustrative()
    });
    assert!((validate(config).unwrap().stoich() - 14.415).abs() < 0.00001);
}

#[cfg(feature = "json")]
#[test]
fn strict_json_is_shared_and_never_discards_fields() {
    let valid = validate(default_config()).unwrap();
    let json = to_json(&valid).unwrap();
    assert_eq!(from_json_bytes(json.as_bytes()).unwrap().config(), valid.config());
    for bad in [
        json.replacen("{", "{\"unknown\":1,", 1),
        json.replacen("{", "{\"version\":1,", 1),
        format!("{json}{{}}"),
        json.replace("\"Overview\"", "\"\\u0000\""),
        json.replace("\"decimals\":2", "\"decimals\":1.5"),
    ] {
        assert!(from_json_bytes(bad.as_bytes()).is_err(), "accepted invalid JSON");
    }
    assert!(from_json_bytes(&[0xFF]).is_err());
    assert!(from_json_bytes(&vec![b' '; MAX_JSON_BYTES + 1]).is_err());
}

#[cfg(feature = "json")]
#[test]
fn json_decimal_fields_round_directly_to_the_stored_f32() {
    let mut config = default_config();
    config.fuel = Fuel::EthanolBlend(Blend {
        percent: 0.0,
        ..Blend::illustrative()
    });
    let template = to_json(&validate(config).unwrap()).unwrap();
    assert!(template.contains("\"percent\":0.0"));
    // The first three decimals straddle the exact midpoint between 1.0 and
    // its next f32. The fourth lies below the next midpoint, which ties up.
    for (text, expected_bits) in [
        ("1.0000000596046448", 0x3F80_0001),
        ("1.0000000596046447", 0x3F80_0000),
        ("1.000000059604644775390625", 0x3F80_0000),
        ("1.0000001788139343", 0x3F80_0001),
        ("0.0", 0x0000_0000),
        ("-0.0", 0x8000_0000),
        ("12.75", 0x414C_0000),
    ] {
        let json = template.replacen("\"percent\":0.0", &format!("\"percent\":{text}"), 1);
        let accepted = from_json_bytes(json.as_bytes()).unwrap();
        let Fuel::EthanolBlend(blend) = accepted.config().fuel else {
            unreachable!()
        };
        assert_eq!(blend.percent.to_bits(), expected_bits, "JSON decimal {text}");
        let mut output = [0; MAX_FILE_BYTES];
        let decoded = decode(encode(&accepted, &mut output).unwrap()).unwrap();
        let Fuel::EthanolBlend(blend) = decoded.config().fuel else {
            unreachable!()
        };
        assert_eq!(blend.percent.to_bits(), expected_bits, "device payload for {text}");
    }
}

#[cfg(feature = "json")]
#[test]
fn json_decimal_rounding_cannot_turn_an_equal_graph_range_into_a_valid_one() {
    let mut config = default_config();
    config.widgets[0].display.graph_range = GraphRange::Fixed {
        min: 0.0,
        max: f32::from_bits(0x3F80_0001),
    };
    let template = to_json(&validate(config).unwrap()).unwrap();
    assert!(template.contains("\"min\":0.0"));
    let json = template.replacen("\"min\":0.0", "\"min\":1.0000000596046448", 1);
    assert!(matches!(
        from_json_bytes(json.as_bytes()),
        Err(ConfigError::Validation(ValidationError {
            code: ErrorCode::Bounds,
            field: Field::GraphRange,
            ..
        }))
    ));
}
