use std::hint::black_box;

use criterion::{BatchSize, Criterion};
use dashboard_config::*;

fn maximum() -> Config {
    let mut config = default_config();
    let widgets = config.widgets.clone();
    config.views[0].name = "12345678901234".try_into().unwrap();
    for view in 2..=8 {
        config
            .views
            .push(View {
                id: view,
                name: "12345678901234".try_into().unwrap(),
            })
            .unwrap();
        for (slot, template) in widgets.iter().enumerate() {
            let mut widget = template.clone();
            widget.view = view;
            widget.id = u16::from(view - 1) * 8 + slot as u16 + 1;
            config.widgets.push(widget).unwrap();
        }
    }
    for widget in &mut config.widgets {
        widget.id += 60000;
        widget.label = "123456789012".try_into().unwrap();
        widget.warning.message = "12345678901234567890".try_into().unwrap();
        widget.warning.on_ms = 30_000;
        widget.warning.off_ms = 30_000;
        let (min, max) = widget.channel.domain();
        widget.display.graph_range = GraphRange::Fixed { min, max };
    }
    for source in &mut config.sources {
        source.route = Route::Physical11 {
            request: 0x7E0,
            response: 0x7E8,
        };
        source.poll = Poll::EveryMs(10_000);
    }
    config.fuel = Fuel::EthanolBlend(Blend {
        percent: 5.0,
        ..Blend::illustrative()
    });
    config
}

pub fn benchmarks(c: &mut Criterion) {
    for (name, config) in [("default", default_config()), ("maximum", maximum())] {
        let valid = validate(config.clone()).unwrap();
        let mut scratch = [0; MAX_FILE_BYTES];
        let bytes = encode(&valid, &mut scratch).unwrap().to_vec();
        assert_eq!(decode(&bytes).unwrap(), valid);
        assert!(bytes.len() < MAX_FILE_BYTES);
        c.bench_function(&format!("config/{name}_validate"), |b| {
            b.iter_batched(
                || config.clone(),
                |input| black_box(validate(black_box(input)).unwrap()),
                BatchSize::SmallInput,
            )
        });
        c.bench_function(&format!("config/{name}_decode"), |b| {
            b.iter(|| black_box(decode(black_box(&bytes)).unwrap()));
        });
        c.bench_function(&format!("config/{name}_encode"), |b| {
            b.iter(|| {
                black_box(encode(black_box(&valid), &mut scratch).unwrap());
            });
        });
        let mut corrupt = bytes;
        *corrupt.last_mut().unwrap() ^= 1;
        assert_eq!(decode(&corrupt), Err(ConfigError::Checksum));
        c.bench_function(&format!("config/{name}_bad_crc"), |b| {
            b.iter(|| black_box(decode(black_box(&corrupt))));
        });
    }
    c.bench_function("config/oversize_reject", |b| {
        let bytes = vec![0; MAX_FILE_BYTES + 1];
        assert_eq!(decode(&bytes), Err(ConfigError::Length));
        b.iter(|| black_box(decode(black_box(&bytes))));
    });
}
