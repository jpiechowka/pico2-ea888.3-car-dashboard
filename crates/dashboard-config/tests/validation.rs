use dashboard_config::*;

fn check(
    mut config: Config,
    field: Field,
    edit: impl FnOnce(&mut Config),
) {
    edit(&mut config);
    assert_eq!(validate(config).unwrap_err().field, field);
}

#[test]
fn exhaustive_geometry_uses_exact_nonoverlapping_pixel_cells() {
    for span in [Span::OneByOne, Span::TwoByOne, Span::TwoByTwo] {
        for column in 0..=255_u8 {
            for row in 0..=255_u8 {
                let placement = Placement { column, row, span };
                let inside = usize::from(column) + usize::from(span.columns()) <= 4
                    && usize::from(row) + usize::from(span.rows()) <= 2;
                assert_eq!(placement.rect().is_some(), inside);
                assert_eq!(placement.occupied().is_some(), inside);
                if let Some(rect) = placement.rect() {
                    assert_eq!(rect.x + rect.width, u16::from(column + span.columns()) * 80);
                    assert_eq!(rect.y + rect.height, 26 + u16::from(row + span.rows()) * 107);
                    assert_eq!(
                        placement.occupied().unwrap().count_ones(),
                        u32::from(span.columns() * span.rows())
                    );
                }
            }
        }
    }
    let default = default_config();
    for left in 0..8 {
        for right in 0..8 {
            let a = default.widgets[left].placement;
            let b = default.widgets[right].placement;
            assert_eq!(a.occupied().unwrap() & b.occupied().unwrap() != 0, left == right);
        }
    }
}

#[test]
fn ids_labels_messages_and_field_locations_are_explicit() {
    check(default_config(), Field::ViewId, |c| c.views[0].id = 0);
    check(default_config(), Field::ViewId, |c| {
        c.views.push(c.views[0].clone()).unwrap()
    });
    check(default_config(), Field::ViewName, |c| {
        c.views[0].name = "  ".try_into().unwrap()
    });
    let mut config = default_config();
    config.widgets[7].display.decimals = 255;
    let error = validate(config).unwrap_err();
    assert_eq!(
        (error.code, error.field, error.view, error.widget, error.channel),
        (ErrorCode::Bounds, Field::Decimals, Some(1), Some(8), Some(Channel::Egt))
    );
    for label in ["\0", "A\tB", "\r", "\u{7f}", "€", "é", "   "] {
        check(default_config(), Field::Label, |c| {
            c.widgets[0].label = label.try_into().unwrap()
        });
        check(default_config(), Field::WarningMessage, |c| {
            c.widgets[0].warning.message = label.try_into().unwrap()
        });
    }
    let mut config = default_config();
    config.views[0].id = 255;
    config.boot_view = 255;
    config.views[0].name = "12345678901234".try_into().unwrap();
    config.widgets.truncate(1);
    let widget = &mut config.widgets[0];
    widget.id = u16::MAX;
    widget.view = 255;
    widget.placement.span = Span::TwoByTwo;
    widget.label = "123456789012345678901234".try_into().unwrap();
    widget.warning.message = "12345678901234567890".try_into().unwrap();
    validate(config.clone()).unwrap();
    check(config, Field::Label, |c| c.widgets[0].placement.span = Span::OneByOne);
}

#[test]
fn thresholds_domains_and_timing_bounds_apply_even_to_disabled_rules() {
    for channel in Channel::ALL {
        let mut config = default_config();
        config.widgets.truncate(1);
        config.widgets[0] = default_widget(channel, 1, 1, config.widgets[0].placement);
        config.widgets[0].warning.enabled = false;
        for value in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            check(config.clone(), Field::WarningTrigger, |c| {
                c.widgets[0].warning.trigger = value
            });
            check(config.clone(), Field::WarningClear, |c| {
                c.widgets[0].warning.clear = value
            });
            check(config.clone(), Field::WarningLoad, |c| {
                c.widgets[0].warning.min_load_percent = value
            });
            check(config.clone(), Field::GraphRange, |c| {
                c.widgets[0].display.graph_range = GraphRange::Fixed {
                    min: value,
                    max: channel.domain().1,
                }
            });
        }
        check(config.clone(), Field::WarningClear, |c| {
            c.widgets[0].warning.clear = c.widgets[0].warning.trigger
        });
        let mode = if channel == Channel::Lambda {
            LimitMode::High
        } else {
            LimitMode::LambdaDeviation
        };
        check(config.clone(), Field::WarningMode, |c| c.widgets[0].warning.mode = mode);
        for invalid in [0, 30_001, u16::MAX] {
            check(config.clone(), Field::WarningOn, |c| {
                c.widgets[0].warning.on_ms = invalid
            });
            check(config.clone(), Field::WarningOff, |c| {
                c.widgets[0].warning.off_ms = invalid
            });
        }
        let (min, max) = channel.domain();
        check(config.clone(), Field::GraphRange, |c| {
            c.widgets[0].display.graph_range = GraphRange::Fixed { min: min - 1.0, max }
        });
        check(config.clone(), Field::GraphRange, |c| {
            c.widgets[0].display.graph_range = GraphRange::Fixed { min, max: max + 1.0 }
        });
        config.widgets[0].display.graph_range = GraphRange::Fixed { min, max };
        for value in [1, 30_000] {
            config.widgets[0].warning.on_ms = value;
            config.widgets[0].warning.off_ms = value;
            validate(config.clone()).unwrap();
        }
        if channel == Channel::Lambda {
            config.widgets[0].warning.trigger = 2.0;
            config.widgets[0].warning.clear = 0.0;
        } else {
            config.widgets[0].warning.mode = LimitMode::High;
            config.widgets[0].warning.trigger = max;
            config.widgets[0].warning.clear = min;
            validate(config.clone()).unwrap();
            config.widgets[0].warning.mode = LimitMode::Low;
            config.widgets[0].warning.trigger = min;
            config.widgets[0].warning.clear = max;
        }
        for load in [0.0, 100.0] {
            config.widgets[0].warning.min_load_percent = load;
            validate(config.clone()).unwrap();
        }
    }
    for interval in [0, 499, 10_001, u16::MAX] {
        check(default_config(), Field::WarningPage, |c| c.warnings.page_ms = interval);
    }
    for interval in [500, 10_000] {
        let mut config = default_config();
        config.warnings.page_ms = interval;
        validate(config).unwrap();
    }
}

#[test]
fn every_definition_is_channel_bound_and_metadata_cannot_be_forged() {
    for id in DEFINITIONS {
        let def = definition(id);
        assert_eq!(id, def.id);
        for channel in Channel::ALL {
            let mut config = default_config();
            config.sources[channel.index()] = def.source(channel);
            let result = validate(config.clone());
            assert_eq!(
                result.is_ok(),
                def.channel.is_none_or(|expected| expected == channel),
                "{id:?}/{channel:?}"
            );
            if let Ok(valid) = result {
                let source = valid.config().sources[channel.index()];
                check(config.clone(), Field::SourceEvidence, |c| {
                    c.sources[channel.index()].evidence = if source.evidence == Evidence::Unresolved {
                        Evidence::GenericObd
                    } else {
                        Evidence::Unresolved
                    }
                });
                check(config.clone(), Field::SourceAccess, |c| {
                    c.sources[channel.index()].access = if source.access == Access::None {
                        Access::Unresolved
                    } else {
                        Access::None
                    }
                });
                check(config, Field::SourceDecoder, |c| {
                    c.sources[channel.index()].decoder = if source.decoder == Decoder::None {
                        Decoder::Unresolved
                    } else {
                        Decoder::None
                    }
                });
            }
        }
    }
}

#[test]
fn source_resolver_never_turns_catalogue_candidates_into_live_queries() {
    for id in DEFINITIONS {
        let def = definition(id);
        let channel = def.channel.unwrap_or(Channel::Boost);
        let mut config = default_config();
        config.sources[channel.index()] = def.source(channel);
        assert_eq!(
            resolve_source(&validate(config.clone()).unwrap(), channel, 0x7E0, 0x7E8),
            if def.unavailable.is_none() && id != DefinitionId::Disabled {
                let Access::Obd01 { pid } = def.access else {
                    panic!("implemented source needs OBD request");
                };
                SourceResolution::Obd {
                    pid,
                    period_ms: minimum_period(channel),
                    freshness_ms: channel.freshness_cap(),
                }
            } else {
                SourceResolution::Disabled
            }
        );
        if id == DefinitionId::Disabled {
            continue;
        }
        config.sources[channel.index()].poll = Poll::EveryMs(10_000);
        config.sources[channel.index()].route = Route::DeploymentProfile;
        let result = resolve_source(&validate(config.clone()).unwrap(), channel, 0x7E0, 0x7E8);
        if let Some(reason) = def.unavailable {
            assert_eq!(result, SourceResolution::Unavailable(reason));
        }
        config.sources[channel.index()].route = Route::Unknown;
        assert_eq!(
            resolve_source(&validate(config.clone()).unwrap(), channel, 0x7E0, 0x7E8),
            SourceResolution::Unavailable(UnavailableReason::MissingRoute)
        );
        config.sources[channel.index()].route = Route::Physical11 {
            request: 0x700,
            response: 0x708,
        };
        assert_eq!(
            resolve_source(&validate(config.clone()).unwrap(), channel, 0x7E0, 0x7E8),
            SourceResolution::Unavailable(UnavailableReason::RouteNotAvailable)
        );
        config.sources[channel.index()].route = Route::Physical11 {
            request: 0x7E0,
            response: 0x7E8,
        };
        assert_eq!(
            resolve_source(&validate(config).unwrap(), channel, 0x7E0, 0x7E8),
            result
        );
    }
}

#[test]
fn polling_freshness_and_routes_cannot_relax_reviewed_limits() {
    for channel in Channel::ALL {
        for period in [0, minimum_period(channel) - 1, 10_001, u16::MAX] {
            check(default_config(), Field::SourcePoll, |c| {
                c.sources[channel.index()].poll = Poll::EveryMs(period)
            });
        }
        for freshness in [0, channel.freshness_cap() + 1, u16::MAX] {
            check(default_config(), Field::SourceFreshness, |c| {
                c.sources[channel.index()].freshness_ms = freshness
            });
        }
        for route in [
            Route::Physical11 {
                request: 0x800,
                response: 0x7E8,
            },
            Route::Physical11 {
                request: 0x7E0,
                response: 0x800,
            },
            Route::Physical11 {
                request: 0x7E8,
                response: 0x7E8,
            },
            Route::Physical11 {
                request: 0x7DF,
                response: 0x7E8,
            },
        ] {
            check(default_config(), Field::SourceRoute, |c| {
                c.sources[channel.index()].route = route
            });
        }
        for period in [minimum_period(channel), 10_000] {
            let mut config = default_config();
            config.sources[channel.index()].poll = Poll::EveryMs(period);
            config.sources[channel.index()].freshness_ms = 1;
            validate(config).unwrap();
        }
        let mut config = default_config();
        config.sources[channel.index()] = definition(DefinitionId::Disabled).source(channel);
        check(config, Field::SourcePoll, |c| {
            c.sources[channel.index()].poll = Poll::EveryMs(10_000)
        });
    }
}

#[test]
fn fuel_rejects_invalid_inputs_and_decreases_monotonically_by_either_basis() {
    for value in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, 4.99, 25.01] {
        check(default_config(), Field::FuelRatio, |c| {
            c.fuel = Fuel::Stoichiometric { ratio: value }
        });
    }
    for basis in [BlendBasis::Mass, BlendBasis::Volume] {
        let mut previous = 25.0;
        for tenths in 0..=1000 {
            let mut config = default_config();
            config.fuel = Fuel::EthanolBlend(Blend {
                percent: tenths as f32 / 10.0,
                basis,
                ..Blend::illustrative()
            });
            let ratio = validate(config).unwrap().stoich();
            assert!(ratio < previous);
            previous = ratio;
        }
    }
    type InvalidBlend = (Field, fn(&mut Blend));
    let invalid: &[InvalidBlend] = &[
        (Field::FuelPercent, |b| b.percent = -0.001),
        (Field::FuelPercent, |b| b.percent = 100.001),
        (Field::FuelPercent, |b| b.percent = f32::NAN),
        (Field::FuelGasolineRatio, |b| b.gasoline_stoich = 0.0),
        (Field::FuelEthanolRatio, |b| b.ethanol_stoich = f32::INFINITY),
        (Field::FuelGasolineRatio, |b| b.gasoline_stoich = b.ethanol_stoich),
        (Field::FuelGasolineDensity, |b| b.gasoline_density = 0.0),
        (Field::FuelEthanolDensity, |b| b.ethanol_density = -1.0),
        (Field::FuelGasolineDensity, |b| b.gasoline_density = 1.201),
        (Field::FuelEthanolDensity, |b| b.ethanol_density = f32::NAN),
        (Field::FuelReference, |b| {
            b.composition_origin = CompositionOrigin::Measured
        }),
        (Field::FuelReference, |b| {
            b.density_reference = DensityReference::CommonCelsius(f32::INFINITY)
        }),
        (Field::FuelReference, |b| {
            b.density_reference = DensityReference::CommonCelsius(-30.01)
        }),
        (Field::FuelReference, |b| {
            b.density_reference = DensityReference::CommonCelsius(60.01)
        }),
    ];
    for &(field, mutate) in invalid {
        check(default_config(), field, |c| {
            let mut blend = Blend::illustrative();
            mutate(&mut blend);
            c.fuel = Fuel::EthanolBlend(blend);
        });
    }
    for value in [-30.0, 60.0] {
        let mut config = default_config();
        config.fuel = Fuel::EthanolBlend(Blend {
            composition_origin: CompositionOrigin::Measured,
            density_reference: DensityReference::CommonCelsius(value),
            ..Blend::illustrative()
        });
        validate(config).unwrap();
    }
}
