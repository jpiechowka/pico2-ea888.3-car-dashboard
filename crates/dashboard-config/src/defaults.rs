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
        display: DisplayOptions {
            decimals: match channel {
                Channel::Boost => 2,
                Channel::Battery => 1,
                Channel::Lambda => 3,
                _ => 0,
            },
            graph: true,
            average: true,
            extrema: true,
            trend: true,
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
    let mut views = heapless::Vec::new();
    views
        .push(View {
            id: 1,
            name: "Overview".try_into().expect("bounded built-in view"),
        })
        .expect("built-in view count");
    let mut widgets = heapless::Vec::new();
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
        widgets
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
    Config {
        version: VERSION,
        catalogue_version: CATALOGUE_VERSION,
        boot_view: 1,
        views,
        widgets,
        sources: core::array::from_fn(|i| definition(ids[i]).source(Channel::ALL[i])),
        warnings: WarningPresentation {
            scope: WarningScope::AllViews,
            mode: WarningMode::CycleOne,
            page_ms: 2000,
        },
        fuel: Fuel::EquivalentGasoline,
    }
}
