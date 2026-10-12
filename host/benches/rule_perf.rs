use std::hint::black_box;

use criterion::{BatchSize, Criterion};
use dashboard_host_validation::acquisition::AcquisitionModel;
use dashboard_host_validation::alarms::RuleIndex;
use dashboard_host_validation::view_config::*;
use dashboard_host_validation::view_state::WarningPager;

fn maximum() -> ValidatedConfig {
    let mut config = default_config();
    config.views.clear();
    config.widgets.clear();
    for view in 1..=8 {
        config
            .views
            .push(View {
                id: view,
                name: "View".try_into().unwrap(),
            })
            .unwrap();
        for cell in 0..8 {
            let mut widget = default_widget(
                Channel::Boost,
                (view as u16 - 1) * 8 + cell as u16 + 1,
                view,
                Placement {
                    column: cell % 4,
                    row: cell / 4,
                    span: Span::OneByOne,
                },
            );
            widget.warning.enabled = true;
            config.widgets.push(widget).unwrap();
        }
    }
    validate(config).unwrap()
}

pub fn benchmarks(c: &mut Criterion) {
    let config = maximum();
    let values = [10., 90., 95., 80., 30., 700., 13.8, 1.];
    let ready = || {
        let mut model = Box::new(AcquisitionModel::demo_configured(&config));
        for at in [0, 250, 500, 750, 1000] {
            assert!(model.ingest_batch(values, at));
        }
        model
    };
    let model = ready();
    let snapshot = model.snapshot();
    for i in 0..64 {
        assert!(snapshot.rule_active_at(RuleIndex::new(i).unwrap(), 1000));
    }
    c.bench_function("rules/64_ingest_eight_channels", |b| {
        b.iter_batched_ref(
            ready,
            |m| black_box(m.ingest_batch(black_box(values), black_box(1250))),
            BatchSize::PerIteration,
        )
    });
    c.bench_function("rules/64_snapshot_copy", |b| {
        b.iter(|| black_box(black_box(&model).snapshot()))
    });
    let mut pager = WarningPager::new();
    assert!(pager.select(&config, &snapshot, 1, 1000, true).is_some());
    c.bench_function("rules/64_warning_page", |b| {
        b.iter(|| black_box(pager.select(black_box(&config), black_box(&snapshot), 1, black_box(1000), true)))
    });
}
