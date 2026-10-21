//! RGB565 storage, conservative contrast and dormant-role validation.
use dashboard_config::*;

#[test]
/// Hidden roles still require usable contrast, and popup foreground/background cannot become identical.
fn invisible_palette_is_rejected_even_when_role_is_currently_hidden() {
    let mut raw = default_config();
    raw.widgets[0].colors.value = raw.widgets[0].colors.background;
    assert!(validate(raw).is_err());
    let mut raw = default_config();
    raw.widgets[0].display.graph = false;
    raw.widgets[0].colors.graph = raw.widgets[0].colors.background;
    assert!(
        validate(raw).is_err(),
        "enabling a role must never trap a valid palette"
    );
    let mut raw = default_config();
    raw.warning_colors.foreground = raw.warning_colors.background;
    assert!(validate(raw).is_err());
}

#[test]
/// All RGB565 codes match an independent floating-point reference without false 3:1 admission.
fn quantized_contrast_matches_independent_srgb_reference_conservatively() {
    /// Return relative linear-sRGB luminance of represented RGB565 color `c`, using independent f64 math.
    fn luminance(c: Color) -> f64 {
        let channels = c.rgb888().map(|v| {
            let s = f64::from(v) / 255.;
            if s <= 0.04045 {
                s / 12.92
            } else {
                ((s + 0.055) / 1.055).powf(2.4)
            }
        });
        channels[0] * 0.2126 + channels[1] * 0.7152 + channels[2] * 0.0722
    }
    for code in 0..=u16::MAX {
        for other in [Color(0), Color(65535), Color(code.wrapping_mul(12345))] {
            let color = Color(code);
            let (a, b) = (luminance(color), luminance(other));
            let ratio = (a.max(b) + 0.05) / (a.min(b) + 0.05);
            assert_eq!(color.contrasts(other), other.contrasts(color));
            if color.contrasts(other) {
                assert!(ratio >= 3., "false admission {code}/{other:?}: {ratio}");
            }
            if ratio > 3.002 {
                assert!(color.contrasts(other), "excessive conservatism: {ratio}");
            }
        }
    }
}

#[test]
/// Every palette role roundtrips through accessors and enforces its documented contrast exemption or requirement.
fn every_stored_role_has_the_expected_visibility_contract() {
    for role in ColorRole::ALL {
        let mut raw = default_config();
        let c = &mut raw.widgets[0].colors;
        let (value, valid) = match role {
            ColorRole::Background => (Color(65535), false),
            ColorRole::Border | ColorRole::Track => (c.background, true),
            ColorRole::WarningBackground => (c.warning.foreground, false),
            ColorRole::WarningForeground => (c.warning.background, false),
            _ => (c.background, false),
        };
        role.set(c, value);
        assert_eq!(role.get(c), value);
        assert_eq!(validate(raw).is_ok(), valid, "{role:?}");
    }
    assert!(validate(default_config()).is_ok());
}

#[test]
/// Custom quantized widget and popup colors survive current binary and optional JSON roundtrips.
fn palette_changes_preserve_binary_and_json_roundtrips() {
    let mut raw = default_config();
    raw.widgets[0].colors.background = Color::rgb(0, 16, 32);
    raw.widgets[0].colors.value = Color::rgb(64, 255, 192);
    raw.widgets[0].gauge.kind = GaugeKind::Circular;
    raw.warning_colors = WarningColors {
        foreground: Color(0),
        background: Color(0xFFE0),
    };
    let config = validate(raw).unwrap();
    let mut bytes = [0; MAX_FILE_BYTES];
    assert_eq!(decode(encode(&config, &mut bytes).unwrap()).unwrap(), config);
    #[cfg(feature = "json")]
    assert_eq!(from_json_bytes(to_json(&config).unwrap().as_bytes()).unwrap(), config);
}

#[test]
/// Eight populated views with large IDs/text/palettes remain inside declared file ceilings.
fn maximum_palette_document_stays_inside_binary_and_json_file_limits() {
    let mut raw = default_config();
    raw.views.clear();
    raw.widgets.clear();
    raw.boot_view = 248;
    for view in 248..=255 {
        raw.views
            .push(View {
                id: view,
                name: "WWWWWWWWWWWWWW".try_into().unwrap(),
                orientation: None,
            })
            .unwrap();
        for (i, channel) in Channel::ALL.into_iter().enumerate() {
            let mut widget = default_widget(
                channel,
                65000 + (u16::from(view) - 248) * 8 + i as u16,
                view,
                Placement {
                    column: i as u8 % 4,
                    row: i as u8 / 4,
                    span: Span::OneByOne,
                },
            );
            widget.label = "WWWWWWWWWWWW".try_into().unwrap();
            widget.warning.message = "WWWWWWWWWWWWWWWWWWWW".try_into().unwrap();
            widget.warning.on_ms = 30000;
            widget.warning.off_ms = 30000;
            for role in ColorRole::ALL {
                role.set(&mut widget.colors, Color(65535));
            }
            widget.colors.background = Color(0x4000);
            widget.colors.track = Color(0x4000);
            widget.colors.warning.background = Color(0x4000);
            widget.gauge.kind = GaugeKind::ALL[i % 6];
            widget.gauge.expand = true;
            raw.widgets.push(widget).unwrap();
        }
    }
    let config = validate(raw).unwrap();
    let mut output = [0; MAX_FILE_BYTES];
    let bytes = encode(&config, &mut output).unwrap();
    assert!(bytes.len() < MAX_FILE_BYTES);
    assert_eq!(decode(bytes).unwrap(), config);
    #[cfg(feature = "json")]
    {
        let json = to_json(&config).unwrap();
        assert!(json.len() < MAX_JSON_BYTES);
        assert_eq!(from_json_bytes(json.as_bytes()).unwrap(), config);
    }
}

#[cfg(not(feature = "json"))]
#[test]
/// The device-only decoder rejects a v1 envelope instead of enabling host migration implicitly.
fn device_rejects_v1_without_json_migration_and_loads_default_with_fault() {
    let bytes = include_bytes!("fixtures/v1-default.eav");
    assert!(matches!(decode(bytes), Err(ConfigError::Version)));
}
