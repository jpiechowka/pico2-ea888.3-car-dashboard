use dashboard_config::*;

// Construct the envelope independently. This non-reflected CRC uses bit-reversed
// bytes and the normal polynomial instead of the production reflected routine.
fn envelope(payload: &[u8]) -> Vec<u8> {
    let mut result = b"EAV1".to_vec();
    result.extend_from_slice(&VERSION.to_le_bytes());
    result.extend_from_slice(&u16::try_from(payload.len()).unwrap().to_le_bytes());
    let mut crc = u32::MAX;
    for &byte in result.iter().chain(payload) {
        crc ^= u32::from(byte.reverse_bits()) << 24;
        for _ in 0..8 {
            crc = (crc << 1) ^ if crc & 0x80000000 != 0 { 0x04C11DB7 } else { 0 };
        }
    }
    result.extend_from_slice(&(crc.reverse_bits() ^ u32::MAX).to_le_bytes());
    result.extend_from_slice(payload);
    result
}

fn raw_config(config: &Config) -> Vec<u8> {
    let mut out = [0; MAX_FILE_BYTES];
    envelope(postcard::to_slice(config, &mut out).unwrap())
}

fn minimal() -> Config {
    let mut config = default_config();
    config.views[0].name = "A".try_into().unwrap();
    config.widgets.clear();
    for channel in Channel::ALL {
        config.sources[channel.index()] = definition(DefinitionId::Disabled).source(channel);
    }
    config
}

#[test]
fn crc_known_answers_and_independently_built_golden_v4() {
    assert_eq!(crc32(b"123456789"), 0xCBF43926);
    assert_eq!(crc32(b""), 0);
    // Constructed from schema field order with a standalone Python uvarint
    // encoder, struct.pack('<HH') and zlib.crc32, not the production serializer.
    let hex = "4541563104009000f60a022f0401010101014100000000000200f403000000000200b817000000000200b817000000000200b817000000000200b817000000000200b817000000000200d00f000000000200f4030000d00f00ffff0380f003a00608070a06090b0100010b000e000d000c0100020c000f001000110100030078787878787878780001e00f649003320280f003649003320100000000";
    let golden: Vec<u8> = hex
        .as_bytes()
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| u8::from_str_radix(core::str::from_utf8(pair).unwrap(), 16).unwrap())
        .collect();
    assert_eq!(golden.len(), 156);
    let expected = validate(minimal()).unwrap();
    assert_eq!(decode(&golden).unwrap(), expected);
    let mut out = [0; MAX_FILE_BYTES];
    assert_eq!(encode(&expected, &mut out).unwrap(), golden);
    assert_eq!(envelope(&golden[12..]), golden);
}

#[test]
fn valid_checksum_does_not_bypass_payload_or_semantic_checks() {
    let mutations: &[fn(&mut Config)] = &[
        |c| c.version = VERSION + 1,
        |c| c.catalogue_version = 2,
        |c| c.widgets[0].warning.enabled = false, // followed by invalid NaN below
        |c| {
            c.widgets[0].display.graph_range = GraphRange::Fixed {
                min: f32::NAN,
                max: 1.0,
            }
        },
        |c| c.widgets[0].view = 200,
        |c| c.sources[1].access = Access::ReadDid22 { did: 0xFFFF },
        |c| {
            c.fuel = Fuel::Stoichiometric {
                ratio: f32::NEG_INFINITY,
            }
        },
    ];
    for (i, mutate) in mutations.iter().enumerate() {
        let mut config = default_config();
        mutate(&mut config);
        if i == 2 {
            config.widgets[0].warning.trigger = f32::NAN;
        }
        assert!(matches!(decode(&raw_config(&config)), Err(ConfigError::Validation(_))));
    }
    let bytes = raw_config(&minimal());
    let body = &bytes[12..];
    let mut trailing = body.to_vec();
    trailing.push(0);
    assert_eq!(decode(&envelope(&trailing)), Err(ConfigError::Payload));
    for offset in [8, 9, 10, 11, 12, 13] {
        let mut invalid_enum = body.to_vec();
        invalid_enum[offset] = 127;
        assert_eq!(
            decode(&envelope(&invalid_enum)),
            Err(ConfigError::Payload),
            "enum at {offset}"
        );
    }
    for payload in [
        vec![],
        vec![0x80; 10],
        vec![1, 1, 1, 1, 1, 2, 0xC0, 0xAF],
        vec![1, 1, 1, 1, 1, 15],
    ] {
        assert_eq!(decode(&envelope(&payload)), Err(ConfigError::Payload));
    }
    let mut too_many_views = vec![1, 1, 1, 9];
    for id in 1..=9 {
        too_many_views.extend_from_slice(&[id, 1, b'A']);
    }
    too_many_views.extend_from_slice(&body[7..]);
    assert_eq!(decode(&envelope(&too_many_views)), Err(ConfigError::Payload));

    let config = default_config();
    let mut widgets: heapless::Vec<Widget, 65> = heapless::Vec::new();
    for _ in 0..65 {
        widgets.push(config.widgets[0].clone()).unwrap();
    }
    let mut out = [0; MAX_FILE_BYTES];
    let payload = postcard::to_slice(
        &(
            1_u16,
            1_u16,
            1_u8,
            &config.views,
            widgets,
            config.sources,
            config.warnings,
            config.fuel,
        ),
        &mut out,
    )
    .unwrap();
    assert_eq!(decode(&envelope(payload)), Err(ConfigError::Payload));
}

#[test]
fn envelope_fields_have_distinct_failure_paths_and_size_bound() {
    let valid = raw_config(&minimal());
    let mut bad = valid.clone();
    bad[0] = b'X';
    assert_eq!(decode(&bad), Err(ConfigError::Header));
    let mut bad = valid.clone();
    bad[4] = (VERSION + 1) as u8;
    assert_eq!(decode(&bad), Err(ConfigError::Version));
    let mut bad = valid.clone();
    bad[6] += 1;
    assert_eq!(decode(&bad), Err(ConfigError::Length));
    let mut bad = valid;
    bad[8] ^= 1;
    assert_eq!(decode(&bad), Err(ConfigError::Checksum));
    assert_eq!(
        decode(&envelope(&vec![0; MAX_FILE_BYTES - HEADER_BYTES + 1])),
        Err(ConfigError::Length)
    );
    assert_eq!(
        decode(&envelope(&vec![0; MAX_FILE_BYTES - HEADER_BYTES])),
        Err(ConfigError::Payload)
    );
}

#[test]
fn noncanonical_varints_cannot_pass_host_validation_then_fail_on_pico() {
    let bytes = raw_config(&minimal());
    let payload = &bytes[12..];
    // A one-element view collection uses usize on the wire implementation.
    // Host usize accepts this 6-byte value, whereas Pico/Wasm usize is u32.
    // Reject even the shorter overlong form so every accepted payload has one
    // representation independent of the validating machine's pointer width.
    for length in [&[0x81, 0x00][..], &[0x81, 0x80, 0x80, 0x80, 0x80, 0x00][..]] {
        let mut malformed = payload[..3].to_vec();
        malformed.extend_from_slice(length);
        malformed.extend_from_slice(&payload[4..]);
        assert_eq!(decode(&envelope(&malformed)), Err(ConfigError::Payload));
    }
}

#[test]
fn deterministic_random_inputs_and_generated_layouts_never_escape_validation() {
    let mut seed = 0xBAA511E5_u32;
    let mut next = || {
        seed ^= seed << 13;
        seed ^= seed >> 17;
        seed ^= seed << 5;
        seed
    };
    for _ in 0..10_000 {
        let len = next() as usize % 512;
        let body: Vec<u8> = (0..len).map(|_| next() as u8).collect();
        for bytes in [&body[..], &envelope(&body)[..]] {
            if let Ok(parsed) = decode(bytes) {
                validate(parsed.config().clone()).unwrap();
            }
        }
    }
    let mut output = [0; MAX_FILE_BYTES];
    for _ in 0..2_000 {
        let mut config = default_config();
        config.widgets.clear();
        let mut occupied = 0;
        for id in 1..=8 {
            let placement = Placement {
                column: (next() % 4) as u8,
                row: (next() % 2) as u8,
                span: [Span::OneByOne, Span::TwoByOne, Span::TwoByTwo][next() as usize % 3],
            };
            if let Some(cells) = placement.occupied()
                && cells & occupied == 0
            {
                occupied |= cells;
                config
                    .widgets
                    .push(default_widget(Channel::ALL[next() as usize % 8], id, 1, placement))
                    .unwrap();
            }
        }
        let valid = validate(config).unwrap();
        assert_eq!(decode(encode(&valid, &mut output).unwrap()).unwrap(), valid);
    }
}

#[cfg(feature = "json")]
#[test]
fn json_nested_unknown_duplicate_enum_and_collection_overflow_are_rejected() {
    let json = to_json(&validate(default_config()).unwrap()).unwrap();
    for bad in [
        json.replacen("\"id\":1", "\"id\":1, \"id\":1", 1),
        json.replacen("\"row\":0", "\"row\":0, \"bogus\":true", 1),
        json.replacen("\"decimals\":2", "\"decimals\":2, \"decimals\":2", 1),
        json.replacen("\"High\"", "\"Critical\"", 1),
        json.replacen("\"OneByOne\"", "\"FourByTwo\"", 1),
        json.replacen("\"DemoOnly\"", "\"Qualified\"", 1),
        json.replacen("\"Overview\"", "\"123456789012345\"", 1),
        json.replacen("\"Overview\"", "\"\\u00e9\"", 1),
        json.replacen("\"trigger\":2.5", "\"trigger\":1e400", 1),
    ] {
        assert!(from_json_bytes(bad.as_bytes()).is_err());
    }
    let mut value: serde_json::Value = serde_json::from_str(&json).unwrap();
    let widget = value["widgets"][0].clone();
    value["widgets"] = serde_json::Value::Array(vec![widget; 65]);
    assert!(matches!(
        from_json_bytes(serde_json::to_string(&value).unwrap().as_bytes()),
        Err(ConfigError::Json { .. })
    ));
    let mut value: serde_json::Value = serde_json::from_str(&json).unwrap();
    let view = value["views"][0].clone();
    value["views"] = serde_json::Value::Array(vec![view; 9]);
    assert!(matches!(
        from_json_bytes(serde_json::to_string(&value).unwrap().as_bytes()),
        Err(ConfigError::Json { .. })
    ));
    for text in ["null", "[]", "{}", "true", "1", "{\"version\":1}"] {
        assert!(from_json_bytes(text.as_bytes()).is_err());
    }
}

#[test]
fn cache_publication_does_not_change_current_bytes() {
    let first = validate(default_config()).unwrap();
    let second = validate(default_config()).unwrap();
    assert_ne!(first.cache_identity(), second.cache_identity());
    let mut a = [0; MAX_FILE_BYTES];
    let mut b = [0; MAX_FILE_BYTES];
    assert_eq!(encode(&first, &mut a).unwrap(), encode(&second, &mut b).unwrap());
    #[cfg(feature = "json")]
    assert_eq!(to_json(&first).unwrap(), to_json(&second).unwrap());
}
