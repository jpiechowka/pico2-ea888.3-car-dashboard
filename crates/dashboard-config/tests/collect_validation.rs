//! Multi-error traversal, causal suppression and stable JSON-location contracts.
use core::ops::ControlFlow;

use dashboard_config::*;

/// Visit every issue in `config`, returning deterministic ordered errors; panics if traversal unexpectedly stops.
fn issues(config: &Config) -> Vec<ValidationIssue> {
    let mut issues = Vec::new();
    assert_eq!(
        visit_validation(config, |issue| {
            issues.push(issue);
            ControlFlow::Continue(())
        }),
        ControlFlow::Continue(None)
    );
    issues
}

#[test]
/// Bad references/IDs do not suppress independent errors or invent overlap coordinates for invalid placements.
fn independent_errors_survive_bad_geometry_references_and_duplicate_ids() {
    let mut config = default_config();
    config
        .views
        .push(View {
            id: 2,
            name: "SECOND".try_into().unwrap(),
            orientation: None,
        })
        .unwrap();
    config.views.push(config.views[1].clone()).unwrap();
    config.views[2].name = " ".try_into().unwrap();
    config.widgets[0].view = 255;
    config.widgets[0].placement.column = 255;
    config.widgets[0].label = " ".try_into().unwrap();
    config.widgets[1].id = config.widgets[0].id;
    config.widgets[2].placement = config.widgets[1].placement;
    config.sources[Channel::Oil.index()].freshness_ms = 0;
    let found = issues(&config);
    assert_eq!(found[0].error, validate(config.clone()).unwrap_err());
    assert_eq!(found[0].error.field, Field::ViewId);
    assert_eq!(found[0].location.view_index(), Some(2));
    assert!(
        found
            .iter()
            .any(|i| i.error.field == Field::ViewName && i.location.view_index() == Some(2))
    );
    for field in [Field::WidgetView, Field::Placement, Field::Label] {
        assert!(
            found
                .iter()
                .any(|i| i.error.field == field && i.location.widget_index() == Some(0))
        );
    }
    assert!(
        found
            .iter()
            .any(|i| i.error.code == ErrorCode::Duplicate && i.location.widget_index() == Some(1))
    );
    assert!(
        found
            .iter()
            .any(|i| i.error.code == ErrorCode::Overlap && i.location.widget_index() == Some(2))
    );
    assert!(
        found
            .iter()
            .any(|i| i.error.field == Field::SourceFreshness && i.location.channel_index() == Some(1))
    );
    assert!(
        !found
            .iter()
            .any(|i| i.error.code == ErrorCode::Overlap && i.location.widget_index() == Some(0))
    );
    assert_eq!(
        found
            .iter()
            .find(|i| i.error.field == Field::Label)
            .unwrap()
            .location
            .to_string(),
        "/widgets/0/label"
    );
}

#[test]
/// A visitor break emits only the same first issue returned by single-error validation.
fn break_stops_at_the_existing_first_error() {
    let mut config = default_config();
    config.version = 999;
    config.catalogue_version = 999;
    let mut calls = 0;
    assert_eq!(
        visit_validation(&config, |issue| {
            calls += 1;
            assert_eq!(issue.error.field, Field::Version);
            ControlFlow::Break(())
        }),
        ControlFlow::Break(())
    );
    assert_eq!(calls, 1);
}

#[test]
/// Control/color/fuel errors accumulate, while invalid fuel inputs prevent misleading derived-math errors.
fn nested_helpers_collect_independent_fields_and_skip_derived_fuel_math() {
    let mut config = default_config();
    config.controls.hold_ms = 0;
    config.controls.screens[0].buttons[3].hold = ControlAction::NextScreen;
    config.widgets[0].colors.label = config.widgets[0].colors.background;
    config.widgets[0].colors.value = config.widgets[0].colors.background;
    config.fuel = Fuel::EthanolBlend(Blend {
        percent: f32::NAN,
        gasoline_stoich: 0.0,
        ethanol_stoich: 0.0,
        gasoline_density: 0.0,
        ethanol_density: 0.0,
        ..Blend::illustrative()
    });
    let found = issues(&config);
    for field in [
        Field::Controls(ControlError::HoldDuration),
        Field::Controls(ControlError::ReservedRecovery {
            page: ControlPage::Dashboard,
        }),
        Field::Color(ColorRole::Label),
        Field::Color(ColorRole::Value),
        Field::FuelPercent,
        Field::FuelGasolineRatio,
        Field::FuelEthanolRatio,
        Field::FuelGasolineDensity,
        Field::FuelEthanolDensity,
    ] {
        assert!(found.iter().any(|i| i.error.field == field), "{field:?}");
    }
    assert!(
        !found
            .iter()
            .any(|i| i.error.code == ErrorCode::FuelBasis || i.error.field == Field::FuelRatio)
    );
}

#[test]
/// Default and 64-widget candidates emit no issues and return the same derived stoichiometry as admission.
fn accepted_default_and_maximum_fixture_produce_no_issues() {
    let mut maximum = default_config();
    maximum.views.clear();
    maximum.widgets.clear();
    for view in 1..=8 {
        maximum
            .views
            .push(View {
                id: view,
                name: "MAX".try_into().unwrap(),
                orientation: None,
            })
            .unwrap();
        for cell in 0..8 {
            maximum
                .widgets
                .push(default_widget(
                    Channel::Oil,
                    maximum.widgets.len() as u16 + 1,
                    view,
                    Placement {
                        column: cell % 4,
                        row: cell / 4,
                        span: Span::OneByOne,
                    },
                ))
                .unwrap();
        }
    }
    for config in [default_config(), maximum] {
        let mut calls = 0;
        let result = visit_validation(&config, |_| {
            calls += 1;
            ControlFlow::Continue(())
        });
        assert_eq!(result, ControlFlow::Continue(Some(validate(config).unwrap().stoich())));
        assert_eq!(calls, 0);
    }
}

#[test]
/// Envelope, payload, JSON and semantic errors remain distinguishable through all import entry points.
fn codecs_preserve_distinct_envelope_payload_and_semantic_categories() {
    let mut config = default_config();
    config.widgets[0].display.decimals = 255;
    let first = validate(config.clone()).unwrap_err();
    let mut payload = [0; MAX_FILE_BYTES];
    let payload = postcard::to_slice(&config, &mut payload).unwrap();
    let envelope = |payload: &[u8], version: u16| {
        let mut bytes = b"EAV1".to_vec();
        bytes.extend_from_slice(&version.to_le_bytes());
        bytes.extend_from_slice(&(payload.len() as u16).to_le_bytes());
        let mut checked = bytes.clone();
        checked.extend_from_slice(payload);
        bytes.extend_from_slice(&crc32(&checked).to_le_bytes());
        bytes.extend_from_slice(payload);
        bytes
    };
    assert_eq!(
        decode(&envelope(payload, VERSION)).unwrap_err(),
        ConfigError::Validation(first)
    );
    assert_eq!(decode(&envelope(payload, 999)).unwrap_err(), ConfigError::Version);
    assert_eq!(decode(&envelope(&[255], VERSION)).unwrap_err(), ConfigError::Payload);
    assert_eq!(decode(&[]).unwrap_err(), ConfigError::Length);
    #[cfg(feature = "json")]
    {
        assert!(matches!(from_json_bytes(b"{"), Err(ConfigError::Json { .. })));
        assert_eq!(
            from_json_bytes(br#"{"version":999}"#).unwrap_err(),
            ConfigError::Version
        );
        assert_eq!(
            from_json_bytes(&vec![b' '; MAX_JSON_BYTES + 1]).unwrap_err(),
            ConfigError::Length
        );
    }
}

#[test]
/// A wrong-channel definition suppresses dependent metadata blame but preserves route/freshness findings.
fn incompatible_source_skips_dependent_metadata_but_collects_route_and_freshness() {
    let mut config = default_config();
    config.sources[0] = definition(DefinitionId::Disabled).source(Channel::Boost);
    // A real channel-specific definition belongs to a different channel.
    config.sources[0].definition = default_config().sources[Channel::Oil.index()].definition;
    config.sources[0].route = Route::Physical11 {
        request: 0xFFFF,
        response: 0xFFFF,
    };
    config.sources[0].freshness_ms = 0;
    let found = issues(&config);
    assert_eq!(found[0].error.field, Field::SourceDefinition);
    assert!(found.iter().any(|i| i.error.field == Field::SourceRoute));
    assert!(found.iter().any(|i| i.error.field == Field::SourceFreshness));
    assert!(!found.iter().any(|i| {
        matches!(
            i.error.field,
            Field::SourceAccess | Field::SourceDecoder | Field::SourceEvidence
        )
    }));
}

#[test]
/// Nested errors retain exact JSON pointers, including button gesture and band coordinates.
fn nested_locations_keep_led_control_and_band_coordinates() {
    let mut config = default_config();
    config.led.normal.period_ms = 0;
    config.led.warning.period_ms = 0;
    config.controls.screens[1].buttons[2].tap = ControlAction::NextView;
    config.widgets[0].warning.advanced.snooze_s = 1;
    config.widgets[0]
        .advanced
        .set_bands(&[
            Band::new(-1.0, 1.0, Color(0xFFFF), "A").unwrap(),
            Band::new(0.5, 2.0, Color(0xFFFF), "B").unwrap(),
        ])
        .unwrap();
    let found = issues(&config);
    for path in [
        "/controls/screens/1/buttons/2/tap",
        "/led/normal/period_ms",
        "/led/warning/period_ms",
        "/widgets/0/advanced/bands/1/min",
        "/widgets/0/warning/snooze_s",
    ] {
        assert!(found.iter().any(|i| i.location.to_string() == path), "{path}");
    }
    assert_eq!(
        found[0].location.field(),
        Field::Controls(ControlError::IncompatibleButton {
            page: ControlPage::Debug,
            button: ButtonId::X,
            hold: false
        })
    );
    #[cfg(feature = "json")]
    {
        let value = serde_json::to_value(&config).unwrap();
        for issue in found {
            assert!(
                value.pointer(&issue.location.to_string()).is_some(),
                "{}",
                issue.location
            );
        }
    }
}

#[test]
/// Duplicate view identity cannot manufacture a chosen view index or overlap claim for a widget.
fn ambiguous_view_ids_never_supply_an_arbitrary_view_coordinate_or_overlap() {
    let mut config = default_config();
    config.views.push(config.views[0].clone()).unwrap();
    config.widgets[1].placement = config.widgets[0].placement;
    config.widgets[1].display.decimals = 255;
    let found = issues(&config);
    let decimal = found.iter().find(|i| i.error.field == Field::Decimals).unwrap();
    assert_eq!(decimal.location.widget_index(), Some(1));
    assert_eq!(decimal.location.view_index(), None);
    assert!(!found.iter().any(|i| i.error.code == ErrorCode::Overlap));
}

#[test]
/// A bad minimum identifies only that endpoint, while later independent warning errors remain visible.
fn invalid_band_minimum_never_blames_a_valid_maximum_and_keeps_later_errors() {
    // Boost's domain is [-1, 10]; 2 is an independently valid maximum.
    for min in [11.0, -2.0, f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        let mut config = default_config();
        config.widgets[0]
            .advanced
            .set_bands(&[Band::new(min, 2.0, Color(0xFFFF), "A").unwrap()])
            .unwrap();
        config.widgets[0].warning.on_ms = 0;
        let found = issues(&config);
        assert_eq!(found[0].error, validate(config.clone()).unwrap_err());
        assert_eq!(found[0].error.field, Field::BandMinimum(0));
        assert_eq!(found[0].location.to_string(), "/widgets/0/advanced/bands/0/min");
        assert_eq!(
            found.iter().map(|i| i.error.field).collect::<Vec<_>>(),
            [Field::BandMinimum(0), Field::WarningOn],
            "minimum {min:?}"
        );
    }
}

#[test]
/// Both independently invalid endpoints report; ordering is checked only after an admitted minimum.
fn band_maximum_keeps_independent_bounds_and_valid_minimum_ordering_checks() {
    for (min, max, expected) in [
        (11.0, 12.0, vec![Field::BandMinimum(0), Field::BandMaximum(0)]),
        (2.0, 1.0, vec![Field::BandMaximum(0)]),
    ] {
        let mut config = default_config();
        config.widgets[0]
            .advanced
            .set_bands(&[Band::new(min, max, Color(0xFFFF), "A").unwrap()])
            .unwrap();
        let found = issues(&config);
        assert_eq!(found[0].error, validate(config.clone()).unwrap_err());
        assert_eq!(found.iter().map(|i| i.error.field).collect::<Vec<_>>(), expected);
    }
}
