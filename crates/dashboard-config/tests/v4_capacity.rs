#![cfg(feature = "json")]
use dashboard_config::*;

/// One synthetic builder keeps all 64 instances independent, including hidden
/// settings. Wider fixtures reduce cells but use full 24-byte labels/legends.
fn bound_config(span: Span) -> Config {
    let mut c = default_config();
    c.orientation = Orientation::LandscapeInverted;
    c.views.clear();
    c.widgets.clear();
    c.boot_view = 248;
    c.history_seconds = [900; 8];
    c.reduced_motion = true;
    c.warnings.page_ms = 10000;
    c.controls.hold_ms = 1500;
    c.controls.screens[0].buttons[0].tap = ControlAction::AcknowledgeWarnings;
    for p in [&mut c.led.normal, &mut c.led.warning] {
        p.kind = LedPatternKind::Breathe;
        p.color = Color(u16::MAX);
        p.brightness_percent = 100;
        p.period_ms = 10000;
        p.on_percent = 90;
    }
    c.led.sync_warning_blink = false;
    c.led.during_feedback = true;
    c.led.on_debug = true;
    c.led.on_logs = true;
    c.led.backlight_off = true;
    c.fuel = Fuel::EthanolBlend(Blend {
        percent: 100.0,
        density_reference: DensityReference::CommonCelsius(60.0),
        gasoline_stoich: 25.0,
        ethanol_stoich: 24.0,
        gasoline_density: 1.2,
        ethanol_density: 1.2,
        ..Blend::illustrative()
    });
    for view in 248..=255 {
        c.views
            .push(View {
                id: view,
                name: "\\\\\\\\\\\\\\\\\\\\\\\\\\\\".try_into().unwrap(),
                orientation: Some(Orientation::LandscapeInverted),
            })
            .unwrap();
        let cells = if span == Span::TwoByTwo { 2 } else { 8 };
        for cell in 0..cells {
            let id = 65535 - c.widgets.len() as u16;
            let mut w = default_widget(
                Channel::Oil,
                id,
                view,
                Placement {
                    column: if cells == 2 { cell * 2 } else { cell % 4 },
                    row: if cells == 2 { 0 } else { cell / 4 },
                    span,
                },
            );
            // Every colour consumes the maximum three-byte u16 varint while
            // white foreground on dark red retains required contrast.
            for role in ColorRole::ALL {
                role.set(&mut w.colors, Color(65535));
            }
            w.colors.background = Color(0x8000);
            w.colors.track = Color(0x8000);
            w.colors.warning.background = Color(0x8000);
            w.label = if cells == 2 { "\\".repeat(24) } else { "\\".repeat(12) }
                .as_str()
                .try_into()
                .unwrap();
            w.display.decimals = 3;
            w.display.graph = true;
            w.display.average = true;
            w.display.extrema = true;
            w.display.trend = true;
            w.display.graph_range = GraphRange::Fixed {
                min: -99.12345,
                max: 1999.1234,
            };
            w.gauge.kind = if cells == 2 {
                GaugeKind::Horizontal
            } else {
                GaugeKind::Numeric
            };
            w.gauge.min = -99.12345;
            w.gauge.max = 1999.1234;
            w.gauge.expand = true;
            w.advanced.readout = Readout::Fahrenheit;
            w.advanced.reference = -98.12345;
            w.advanced.reference_mode = ReferenceMode::Manual;
            w.advanced.smoothing_ms = 5000;
            w.advanced.peak_hold_ms = 60000;
            w.advanced.peak_mode = PeakSelection::Maximum;
            w.advanced
                .set_bands(&[
                    Band::new(-99.12345, 0.000012345, Color(65535), "\\\\\\\\\\").unwrap(),
                    Band::new(0.000012345, 999.1234, Color(65534), "\"\"\"\"\"").unwrap(),
                    Band::new(999.1234, 1999.1234, Color(65533), "\\\"\\\"\\").unwrap(),
                ])
                .unwrap();
            w.warning.mode = LimitMode::OutsideWindow;
            w.warning.trigger = -99.12345;
            w.warning.clear = -1.234567;
            w.warning.advanced.trigger_high = 1999.1234;
            w.warning.advanced.clear_high = 1998.1234;
            w.warning.min_load_percent = 99.12345;
            w.warning.on_ms = 30000;
            w.warning.off_ms = 30000;
            w.warning.message = "\\".repeat(20).as_str().try_into().unwrap();
            w.warning.advanced.priority = Priority::Critical;
            w.warning.advanced.latch = true;
            w.warning.advanced.snooze_s = 900;
            w.warning.applicability = Applicability::VehicleAndDemo;
            c.widgets.push(w).unwrap();
        }
    }
    for ch in Channel::ALL {
        c.sources[ch.index()].route = Route::Physical11 {
            request: 2046,
            response: 2047,
        };
        c.sources[ch.index()].poll = Poll::EveryMs(10000);
        c.sources[ch.index()].freshness_ms = ch.freshness_cap();
    }
    c
}

fn saved(c: &Config) -> String {
    serde_json::to_string(
        &serde_json::json!({"editor_version":65535,"selected_view":255,"selected_widget":65535,"config":c}),
    )
    .unwrap()
}

#[test]
fn maximum_documents_fit_and_slot_matches_owned() {
    for span in [Span::OneByOne, Span::TwoByTwo] {
        let expected = validate(bound_config(span)).unwrap();
        let mut buffer = [0; MAX_FILE_BYTES];
        let bytes = encode(&expected, &mut buffer).unwrap();
        let json = to_json(&expected).unwrap();
        let envelope = saved(expected.config());
        println!(
            "{span:?}: eav={} json={} saved={}",
            bytes.len(),
            json.len(),
            envelope.len()
        );
        assert!(bytes.len() <= MAX_FILE_BYTES);
        assert!(json.len() <= MAX_JSON_BYTES);
        assert!(envelope.len() <= MAX_JSON_BYTES);
        assert_eq!(decode(bytes).unwrap(), expected);
        assert_eq!(ConfigSlot::new().load_at_boot(bytes).config, expected);
        assert_eq!(from_json_bytes(json.as_bytes()).unwrap(), expected);
        if let Some(directory) = std::env::var_os("V4_CAPACITY_OUTPUT") {
            let path = std::path::Path::new(&directory);
            std::fs::write(path.join(format!("v4-{span:?}.eav")), bytes).unwrap();
            std::fs::write(path.join(format!("v4-{span:?}.json")), &json).unwrap();
        }
    }
}

// Structural upper count independent of particular f32 renderings. Every
// finite f32 fits 24 bytes (including sign/exponent); printable free text can
// at most double through quote/backslash escaping. Keys are serialized exactly.
fn count_json(
    value: &serde_json::Value,
    key: &str,
) -> usize {
    use serde_json::Value::*;
    match value {
        Null => 4,
        Bool(_) => 5,
        Number(n) => {
            if n.is_f64() {
                24
            } else {
                n.to_string().len()
            }
        }
        String(s) => {
            if matches!(key, "name" | "label" | "message") {
                2 + 2 * s.len()
            } else if s.starts_with('#') {
                9
            } else {
                2 + s.len().max(20)
            }
        }
        Array(a) => 2 + a.len().saturating_sub(1) + a.iter().map(|v| count_json(v, key)).sum::<usize>(),
        Object(m) => {
            2 + m.len().saturating_sub(1)
                + m.iter()
                    .map(|(k, v)| serde_json::to_string(k).unwrap().len() + 1 + count_json(v, k))
                    .sum::<usize>()
        }
    }
}

#[test]
fn conservative_wire_and_json_count_covers_float_escape_and_varint_choices() {
    let config = bound_config(Span::OneByOne);
    let value = serde_json::to_value(&config).unwrap();
    // Non-widget alternatives: full explicit DID/route sources and the largest
    // fuel variant fit this extra 1024 bytes beyond this populated skeleton.
    let upper = count_json(&value, "") + 1024 + 100;
    println!("conservative compact saved JSON bound={upper}");
    assert!(upper <= 123_585);
    assert!(upper <= MAX_JSON_BYTES);
    struct CountWriter(usize);
    impl std::io::Write for CountWriter {
        fn write(
            &mut self,
            bytes: &[u8],
        ) -> std::io::Result<usize> {
            self.0 += bytes.len();
            Ok(bytes.len())
        }

        fn flush(&mut self) -> std::io::Result<()> { Ok(()) }
    }
    struct FloatBound;
    impl serde_json::ser::Formatter for FloatBound {
        fn write_f32<W: std::io::Write + ?Sized>(
            &mut self,
            writer: &mut W,
            _: f32,
        ) -> std::io::Result<()> {
            writer.write_all(&[b'0'; 24])
        }
    }
    let mut writer = CountWriter(0);
    serde::Serialize::serialize(
        &config,
        &mut serde_json::Serializer::with_formatter(&mut writer, FloatBound),
    )
    .unwrap();
    assert!(writer.0 + 100 <= upper);
    // Enumerate all catalogue unions to verify the conservative per-source
    // allowance covers every documentary decoder/access/definition spelling.
    let mut source_json_max = 0;
    let mut source_binary_max = 0;
    for id in DEFINITIONS {
        for channel in Channel::ALL {
            let mut source = definition(id).source(channel);
            source.route = Route::Physical11 {
                request: 2046,
                response: 2047,
            };
            source.poll = Poll::EveryMs(10000);
            source.freshness_ms = channel.freshness_cap();
            source_json_max = source_json_max.max(count_json(&serde_json::to_value(source).unwrap(), ""));
            let mut out = [0; 128];
            source_binary_max = source_binary_max.max(postcard::to_slice(&source, &mut out).unwrap().len());
        }
    }
    for source in config.sources {
        assert!(source_json_max <= count_json(&serde_json::to_value(source).unwrap(), "") + 128);
        let mut out = [0; 128];
        assert!(source_binary_max <= postcard::to_slice(&source, &mut out).unwrap().len() + 16);
    }
    // Count actual serializer output, then cover choices whose payload size
    // this skeleton does not maximize: source catalogue union (8*16 bytes),
    // boost Secondary and readout enums remain one-byte discriminants.
    // Largest labels across any layout consume <=64*12 bytes: wider
    // widgets occupy >=2 cells for 24 label bytes. All 64 IDs are >16383;
    // timing fields are at their highest admitted varint widths, and every
    // palette value is a 3-byte u16. f32 always consumes four binary bytes.
    struct Counter(usize);
    impl postcard::ser_flavors::Flavor for Counter {
        type Output = usize;

        fn try_push(
            &mut self,
            _: u8,
        ) -> postcard::Result<()> {
            self.0 += 1;
            Ok(())
        }

        fn finalize(self) -> postcard::Result<usize> { Ok(self.0) }
    }
    let eav = postcard::serialize_with_flavor(&config, Counter(HEADER_BYTES)).unwrap();
    let bound = eav + 8 * 16;
    println!("conservative EAV bound={bound}");
    assert!(bound <= 14_937);
    assert!(bound <= MAX_FILE_BYTES);
    assert_eq!(core::mem::size_of::<Band>(), 16);
    assert_eq!(core::mem::size_of::<AdvancedDisplay>(), 64);
    assert_eq!(core::mem::size_of::<AdvancedWarning>(), 12);
}
