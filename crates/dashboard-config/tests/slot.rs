use dashboard_config::*;

#[test]
fn current_file_is_published_only_after_validation() {
    let bytes = include_bytes!("../../../configs/default.eav");
    let expected = decode(bytes).unwrap();
    let mut slot = ConfigSlot::new();
    let loaded = slot.load_at_boot(bytes);
    assert_eq!(loaded.fault, None);
    assert_eq!(loaded.config, expected);
}

#[test]
fn failure_replaces_every_field_with_the_reviewed_default() {
    let expected = validate(default_config()).unwrap();
    let mut slot = ConfigSlot::new();
    let loaded = slot.load_at_boot(b"not a file");
    assert_eq!(loaded.fault, Some(ConfigError::Length));
    assert_eq!(loaded.config, expected);
}

// Independent envelope CRC matches the codec contract fixtures.
fn envelope(payload: &[u8]) -> Vec<u8> {
    let mut bytes = b"EAV1".to_vec();
    bytes.extend_from_slice(&VERSION.to_le_bytes());
    bytes.extend_from_slice(&u16::try_from(payload.len()).unwrap().to_le_bytes());
    let mut crc = u32::MAX;
    for &byte in bytes.iter().chain(payload) {
        crc ^= u32::from(byte.reverse_bits()) << 24;
        for _ in 0..8 {
            crc = (crc << 1) ^ if crc & 0x80000000 != 0 { 0x04C11DB7 } else { 0 };
        }
    }
    bytes.extend_from_slice(&(crc.reverse_bits() ^ u32::MAX).to_le_bytes());
    bytes.extend_from_slice(payload);
    bytes
}

fn raw(config: &Config) -> Vec<u8> {
    let mut out = [0; MAX_FILE_BYTES];
    envelope(postcard::to_slice(config, &mut out).unwrap())
}

fn customized() -> Config {
    let mut c = default_config();
    c.catalogue_version = CATALOGUE_VERSION;
    c.views[0].name = "Track".try_into().unwrap();
    c.controls.hold_ms = 1234;
    c.controls.screens[0].buttons[0].tap = ControlAction::PreviousView;
    c.widgets[0].label = "REL PRESS".try_into().unwrap();
    c.widgets[0].colors.label = Color::rgb(128, 255, 255);
    c.warnings.page_ms = 3210;
    c.warnings.scope = WarningScope::CurrentView;
    c.warning_colors.foreground = Color::rgb(255, 255, 128);
    c.fuel = Fuel::Stoichiometric { ratio: 12.5 };
    c.sources[0].route = Route::Physical11 {
        request: 0x700,
        response: 0x708,
    };
    c
}

fn maximum() -> Config {
    let mut c = customized();
    c.views.clear();
    c.widgets.clear();
    for view in 1..=MAX_VIEWS as u8 {
        c.views
            .push(View {
                id: view,
                name: "Max view".try_into().unwrap(),
            })
            .unwrap();
        for cell in 0..8 {
            c.widgets
                .push(default_widget(
                    Channel::ALL[cell],
                    u16::from(view) * 8 + cell as u16,
                    view,
                    Placement {
                        column: (cell % 4) as u8,
                        row: (cell / 4) as u8,
                        span: Span::OneByOne,
                    },
                ))
                .unwrap();
        }
    }
    c.boot_view = 8;
    c
}

fn assert_same(
    slot: &mut ConfigSlot,
    bytes: &[u8],
) {
    let expected = decode(bytes);
    let loaded = slot.load_at_boot(bytes);
    match expected {
        Ok(expected) => {
            assert_eq!(loaded.fault, None);
            assert_eq!(loaded.config, expected);
            assert_eq!(loaded.config.stoich(), expected.stoich());
        }
        Err(fault) => {
            assert_eq!(loaded.fault, Some(fault));
            assert_eq!(loaded.config, validate(default_config()).unwrap());
        }
    }
}

#[test]
fn current_corpus_matches_owned_decode_and_every_truncated_prefix() {
    let mut slot = ConfigSlot::new();
    let mut blend = customized();
    blend.fuel = Fuel::EthanolBlend(Blend {
        percent: 35.0,
        ..Blend::illustrative()
    });
    for c in [default_config(), customized(), blend, maximum()] {
        let bytes = raw(&c);
        validate(c).unwrap();
        assert_same(&mut slot, &bytes);
        for cut in 0..bytes.len() {
            assert_same(&mut slot, &bytes[..cut]);
            // Resealing reaches each partial Postcard field instead of the
            // envelope's length/checksum failure.
            if cut >= HEADER_BYTES {
                assert_same(&mut slot, &envelope(&bytes[HEADER_BYTES..cut]));
            }
        }
    }
}

#[test]
fn resealed_semantic_failures_reset_all_previously_loaded_fields() {
    let mut slot = ConfigSlot::new();
    let success = raw(&maximum());
    let mutations: &[fn(&mut Config)] = &[
        |c| c.version = VERSION + 1,
        |c| c.catalogue_version = CATALOGUE_VERSION + 1,
        |c| c.views[0].name = "\n".try_into().unwrap(),
        |c| {
            c.widgets[0].display.graph_range = GraphRange::Fixed {
                min: f32::NAN,
                max: 1.0,
            }
        },
        |c| c.widgets[63].warning.trigger = f32::INFINITY,
        |c| c.sources[7].freshness_ms = 0,
        |c| c.fuel = Fuel::Stoichiometric { ratio: f32::NAN },
        |c| c.fuel = Fuel::Stoichiometric { ratio: f32::INFINITY },
        |c| c.controls.screens[2].encoder_click = ControlAction::CycleFps,
    ];
    for mutate in mutations {
        assert_same(&mut slot, &success);
        assert_same(&mut slot, b"bad");
        let mut invalid = maximum();
        mutate(&mut invalid);
        let bytes = raw(&invalid);
        assert!(matches!(decode(&bytes), Err(ConfigError::Validation(_))));
        assert_same(&mut slot, &bytes);
        assert_same(&mut slot, &success);
    }
}

#[test]
fn resealed_payload_errors_and_noncanonical_encodings_match_owned_decoder() {
    let mut slot = ConfigSlot::new();
    let mut c = default_config();
    c.views[0].name = "A".try_into().unwrap();
    c.widgets.clear();
    let bytes = raw(&c);
    let payload = &bytes[HEADER_BYTES..];
    let mut failures = vec![vec![], vec![0x80; 10]];
    let mut trailing = payload.to_vec();
    trailing.push(0);
    failures.push(trailing);
    for (offset, encoding) in [
        (0, vec![0x83, 0]), // overlong version integer
        (3, vec![0x81, 0]), // overlong sequence length
        (3, vec![0x81, 0x80, 0x80, 0x80, 0x80, 0]),
        (3, vec![0xFF; 10]),      // overflowing sequence length
        (5, vec![2, 0xC0, 0xAF]), // invalid UTF-8 view name
    ] {
        let mut bad = payload[..offset].to_vec();
        bad.extend_from_slice(&encoding);
        let skip = if offset == 5 { 2 } else { 1 };
        bad.extend_from_slice(&payload[offset + skip..]);
        failures.push(bad);
    }
    // First source's definition enum follows the empty widgets sequence.
    let mut invalid_enum = payload.to_vec();
    invalid_enum[8] = 127;
    failures.push(invalid_enum);
    let mut views = vec![VERSION as u8, 1, 1, 9];
    for id in 1..=9 {
        views.extend_from_slice(&[id, 1, b'A']);
    }
    views.extend_from_slice(&payload[7..]);
    failures.push(views);
    let widget = default_config().widgets[0].clone();
    let widgets: Vec<Widget> = vec![widget; 65];
    let mut out = [0; MAX_FILE_BYTES];
    failures.push(
        postcard::to_slice(
            &(
                c.version,
                c.catalogue_version,
                c.boot_view,
                &c.views,
                widgets.as_slice(),
                c.sources,
                c.warnings,
                c.fuel,
                c.warning_colors,
                c.controls,
            ),
            &mut out,
        )
        .unwrap()
        .to_vec(),
    );
    for bad in failures {
        let bytes = envelope(&bad);
        assert_eq!(decode(&bytes), Err(ConfigError::Payload));
        assert_same(&mut slot, &bytes);
    }
}

#[test]
fn envelope_failure_classes_remain_identical() {
    let mut slot = ConfigSlot::new();
    let bytes = raw(&customized());
    for offset in [0, 4, 6, 8] {
        let mut bad = bytes.clone();
        bad[offset] ^= 0x40;
        assert_same(&mut slot, &bad);
    }
    assert_same(&mut slot, &vec![0; MAX_FILE_BYTES + 1]);
}

#[test]
fn slot_always_rejects_legacy_envelopes_and_current_envelope_legacy_payloads() {
    let mut slot = ConfigSlot::new();
    for bytes in [
        include_bytes!("fixtures/v1-default.eav").as_slice(),
        include_bytes!("fixtures/v2-default.eav").as_slice(),
    ] {
        let loaded = slot.load_at_boot(bytes);
        assert_eq!(loaded.fault, Some(ConfigError::Version));
        assert_eq!(loaded.config, validate(default_config()).unwrap());
        let current = envelope(&bytes[HEADER_BYTES..]);
        assert!(decode(&current).is_err());
        assert_same(&mut slot, &current);
    }
}
#[test]
fn publications_have_distinct_runtime_identity_but_equal_wire_content() {
    let mut slot = dashboard_config::ConfigSlot::new();
    let first = slot.load_at_boot(b"").config.clone();
    let second = &slot.load_at_boot(b"").config;
    assert_eq!(first, *second);
    assert_eq!(
        first.cache_identity_status(),
        dashboard_config::CacheIdentityStatus::Tracked
    );
    assert_ne!(first.cache_identity(), second.cache_identity());
    assert_eq!(first.cache_identity(), first.clone().cache_identity());
    let mut a = [0; 16384];
    let mut b = [0; 16384];
    assert_eq!(
        dashboard_config::encode(&first, &mut a).unwrap(),
        dashboard_config::encode(second, &mut b).unwrap()
    );
}
