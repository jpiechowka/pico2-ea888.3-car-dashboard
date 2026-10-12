use crate::*;

/// Reviewed synthetic warning examples, not recommended limits for an engine.
pub fn default_widget(
    channel: Channel,
    id: u16,
    view: u8,
    placement: Placement,
) -> Widget {
    let (mode, trigger, clear) = match channel {
        Channel::Water => (LimitMode::High, 110.0, 105.0),
        Channel::Oil => (LimitMode::High, 130.0, 125.0),
        Channel::Dsg => (LimitMode::High, 120.0, 115.0),
        Channel::Iat => (LimitMode::High, 60.0, 55.0),
        Channel::Egt => (LimitMode::High, 1050.0, 1000.0),
        Channel::Battery => (LimitMode::Low, 11.5, 12.0),
        Channel::Lambda => (LimitMode::LambdaDeviation, 0.15, 0.05),
        Channel::Boost => (LimitMode::High, 2.5, 2.3),
    };
    Widget {
        id,
        view,
        channel,
        placement,
        label: channel.label().try_into().expect("bounded built-in label"),
        gauge: Gauge::for_channel(channel),
        colors: WidgetColors::default(),
        display: DisplayOptions {
            decimals: match channel {
                Channel::Boost => 2,
                Channel::Battery => 1,
                Channel::Lambda => 3,
                _ => 0,
            },
            graph: channel != Channel::Boost,
            average: !matches!(channel, Channel::Boost | Channel::Lambda | Channel::Battery),
            extrema: channel != Channel::Lambda,
            trend: !matches!(channel, Channel::Boost | Channel::Lambda),
            afr: channel == Channel::Lambda,
            graph_range: GraphRange::Auto,
        },
        warning: WarningSettings {
            enabled: channel != Channel::Boost,
            applicability: Applicability::DemoOnly,
            mode,
            trigger,
            clear,
            min_load_percent: 60.0,
            on_ms: 1000,
            off_ms: 500,
            message: channel.label().try_into().expect("bounded built-in warning"),
        },
    }
}

pub fn default_config() -> Config {
    let mut config = empty_config();
    fill_default_config(&mut config);
    config
}

// Legally initialized storage only. It is private and is not a validated
// document or a public default. No placeholder may be published by a slot.
pub(crate) const fn empty_config() -> Config {
    Config {
        version: 0,
        catalogue_version: 0,
        boot_view: 0,
        views: heapless::Vec::new(),
        widgets: heapless::Vec::new(),
        sources: [ChannelSource {
            definition: DefinitionId::Disabled,
            access: Access::None,
            decoder: Decoder::None,
            evidence: Evidence::Disabled,
            route: Route::Unknown,
            poll: Poll::Off,
            freshness_ms: 0,
        }; 8],
        warnings: WarningPresentation {
            scope: WarningScope::AllViews,
            mode: WarningMode::CycleOne,
            page_ms: 0,
        },
        fuel: Fuel::EquivalentGasoline,
        warning_colors: WarningColors {
            foreground: Color(0),
            background: Color(0),
        },
        controls: ControlMap {
            hold_ms: 0,
            screens: [ScreenBindings {
                buttons: [ButtonBinding {
                    tap: ControlAction::None,
                    hold: ControlAction::None,
                }; 4],
                encoder_rotation: RotationAction::None,
                encoder_click: ControlAction::None,
            }; 3],
        },
    }
}

pub(crate) fn fill_default_config(config: &mut Config) {
    config.version = VERSION;
    config.catalogue_version = CATALOGUE_VERSION;
    config.boot_view = 1;
    config.views.clear();
    config
        .views
        .push(View {
            id: 1,
            name: "Overview".try_into().expect("bounded built-in view"),
        })
        .expect("built-in view count");
    config.widgets.clear();
    for (slot, channel) in [
        Channel::Boost,
        Channel::Lambda,
        Channel::Battery,
        Channel::Water,
        Channel::Oil,
        Channel::Dsg,
        Channel::Iat,
        Channel::Egt,
    ]
    .into_iter()
    .enumerate()
    {
        config
            .widgets
            .push(default_widget(
                channel,
                slot as u16 + 1,
                1,
                Placement {
                    column: (slot % 4) as u8,
                    row: (slot / 4) as u8,
                    span: Span::OneByOne,
                },
            ))
            .expect("built-in widget count");
    }
    use DefinitionId::*;
    let ids = [
        ObdMapMinusAmbient,
        ObdOil5c,
        ObdCoolant05,
        DsgOil2104,
        ObdIntake0f,
        UnresolvedEgt,
        ObdVoltage42,
        SimosLambda10c0,
    ];
    for (i, id) in ids.into_iter().enumerate() {
        config.sources[i] = definition(id).source(Channel::ALL[i]);
    }
    config.warnings = WarningPresentation {
        scope: WarningScope::AllViews,
        mode: WarningMode::CycleOne,
        page_ms: 2000,
    };
    config.fuel = Fuel::EquivalentGasoline;
    config.warning_colors = WarningColors::default();
    config.controls = ControlMap::default();
}
