#[cfg(feature = "json")]
#[test]
fn current_json_accepts_a_complete_portrait_grid() {
    let mut value: serde_json::Value = serde_json::from_str(include_str!("fixtures/v4-default.json")).unwrap();
    value["version"] = 5.into();
    value["orientation"] = "Portrait".into();
    value["views"][0]["orientation"] = serde_json::Value::Null;
    for (cell, widget) in value["widgets"].as_array_mut().unwrap().iter_mut().enumerate() {
        widget["placement"]["column"] = (cell % 2).into();
        widget["placement"]["row"] = (cell / 2).into();
    }
    let json = serde_json::to_vec(&value).unwrap();
    let parsed = dashboard_config::from_json_bytes(&json).expect("valid 2x4 portrait configuration");
    assert_eq!(parsed.config().widgets[7].placement.row, 3);
}
use dashboard_config::*;

#[test]
fn every_cell_owns_exactly_its_pixels_and_no_header_pixels() {
    for geometry in Geometry::ALL {
        let mut area = 0;
        let mut mask = 0;
        for row in 0..geometry.rows() {
            for column in 0..geometry.columns() {
                let placement = Placement {
                    column,
                    row,
                    span: Span::OneByOne,
                };
                let rect = placement.rect_in(geometry).unwrap();
                let occupied = placement.occupied_in(geometry).unwrap();
                assert_eq!(occupied.count_ones(), 1);
                assert_eq!(mask & occupied, 0);
                mask |= occupied;
                area += usize::from(rect.width) * usize::from(rect.height);
                for y in rect.y..rect.y + rect.height {
                    for x in rect.x..rect.x + rect.width {
                        assert_eq!(geometry.cell_at(i32::from(x), i32::from(y)), Some((column, row)));
                    }
                }
            }
        }
        assert_eq!(mask, 255);
        assert_eq!(
            area,
            Geometry::PIXELS - usize::from(Geometry::HEADER_HEIGHT * geometry.width())
        );
        for (x, y) in [
            (0, 0),
            (0, 25),
            (-1, 26),
            (i32::from(geometry.width()), 26),
            (0, i32::from(geometry.height())),
            (i32::MAX, i32::MAX),
            (i32::MIN, i32::MIN),
        ] {
            assert_eq!(geometry.cell_at(x, y), None);
        }
    }
    assert_eq!(
        (0..=4)
            .map(|row| Geometry::Portrait.row_edge(row).unwrap())
            .collect::<Vec<_>>(),
        [26, 99, 173, 246, 320]
    );
    assert_eq!(
        Placement {
            column: 1,
            row: 3,
            span: Span::OneByOne
        }
        .rect_in(Geometry::Portrait),
        Some(PixelRect {
            x: 120,
            y: 246,
            width: 120,
            height: 74
        })
    );
}

#[test]
fn every_span_and_byte_coordinate_is_checked_without_overflow() {
    for geometry in Geometry::ALL {
        for span in [Span::OneByOne, Span::TwoByOne, Span::TwoByTwo] {
            for row in 0..=u8::MAX {
                for column in 0..=u8::MAX {
                    let placement = Placement { column, row, span };
                    let fits = u16::from(column) + u16::from(span.columns()) <= u16::from(geometry.columns())
                        && u16::from(row) + u16::from(span.rows()) <= u16::from(geometry.rows());
                    let rect = placement.rect_in(geometry);
                    assert_eq!(rect.is_some(), fits);
                    assert_eq!(placement.occupied_in(geometry).is_some(), fits);
                    if let Some(rect) = rect {
                        assert!(rect.width > 0 && rect.height > 0);
                        assert!(rect.x + rect.width <= geometry.width());
                        assert!(rect.y + rect.height <= geometry.height());
                        assert_eq!(
                            placement.occupied_in(geometry).unwrap().count_ones(),
                            u32::from(span.columns() * span.rows())
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn inherited_and_explicit_geometry_controls_validation_and_roundtrip() {
    for orientation in Orientation::ALL {
        let mut config = default_config();
        config.orientation = orientation;
        let geometry = orientation.geometry();
        for (cell, widget) in config.widgets.iter_mut().enumerate() {
            widget.placement.column = cell as u8 % geometry.columns();
            widget.placement.row = cell as u8 / geometry.columns();
        }
        assert_eq!(config.view_orientation(1), Some(orientation));
        assert_eq!(config.view_orientation(200), None);
        let valid = validate(config.clone()).unwrap();
        let mut output = [0; MAX_FILE_BYTES];
        let bytes = encode(&valid, &mut output).unwrap();
        assert_eq!(decode(bytes).unwrap(), valid);
        let mut slot = ConfigSlot::new();
        let boot = slot.load_at_boot(bytes);
        assert_eq!(boot.fault, None);
        assert_eq!(boot.config, valid);
        assert_eq!(
            slot.load_at_boot(b"").config.config().orientation,
            Orientation::Landscape
        );
        config.views[0].orientation = Some(if geometry == Geometry::Portrait {
            Orientation::Landscape
        } else {
            Orientation::Portrait
        });
        assert_eq!(validate(config).unwrap_err().field, Field::Placement);
    }
}

#[test]
fn portrait_overlap_and_outside_cells_remain_rejected() {
    let mut config = default_config();
    config.orientation = Orientation::Portrait;
    config.widgets.truncate(2);
    config.widgets[0].placement = Placement {
        column: 0,
        row: 2,
        span: Span::TwoByTwo,
    };
    config.widgets[1].placement = Placement {
        column: 1,
        row: 3,
        span: Span::OneByOne,
    };
    assert_eq!(validate(config.clone()).unwrap_err().code, ErrorCode::Overlap);
    config.widgets[1].placement.column = 2;
    assert_eq!(validate(config).unwrap_err().code, ErrorCode::Bounds);
}

#[cfg(feature = "json")]
#[test]
fn frozen_v4_migrates_identically_from_binary_and_json() {
    let binary = decode(include_bytes!("fixtures/v4-default.eav")).unwrap();
    let json = from_json_bytes(include_bytes!("fixtures/v4-default.json")).unwrap();
    assert_eq!(binary, json);
    assert_eq!(binary, validate(default_config()).unwrap());
    assert_eq!(binary.config().orientation, Orientation::Landscape);
    assert!(binary.config().views.iter().all(|view| view.orientation.is_none()));
    assert_eq!(
        ConfigSlot::new()
            .load_at_boot(include_bytes!("fixtures/v4-default.eav"))
            .fault,
        Some(ConfigError::Version)
    );
}

#[cfg(feature = "json")]
#[test]
fn orientation_unknowns_duplicates_and_legacy_field_injection_fail() {
    let current = to_json(&validate(default_config()).unwrap()).unwrap();
    for bad in [
        current.replace("\"orientation\":\"Landscape\"", "\"orientation\":\"Sideways\""),
        current.replace("\"orientation\":\"Landscape\"", "\"orientation\":99"),
        current.replace("\"orientation\":null", "\"orientation\":\"Sideways\""),
        current.replace("\"orientation\":null", "\"orientation\":null,\"orientation\":null"),
        current.replace(
            "\"orientation\":\"Landscape\"",
            "\"orientation\":\"Landscape\",\"orientation\":\"Portrait\"",
        ),
        include_str!("fixtures/v4-default.json").replacen(
            "\"name\":\"Overview\"",
            "\"name\":\"Overview\",\"orientation\":null",
            1,
        ),
    ] {
        assert!(from_json_bytes(bad.as_bytes()).is_err());
    }
}
