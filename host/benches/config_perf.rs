use std::hint::black_box;

use criterion::{BatchSize, Criterion};
use dashboard_config::*;

pub fn maximum() -> Config {
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
    config.history_seconds = [900, 840, 780, 720, 660, 600, 540, 480];
    config.controls.screens[0].buttons[0].tap = ControlAction::AcknowledgeWarnings;
    config.led.normal.kind = LedPatternKind::Breathe;
    config.led.normal.period_ms = 9876;
    config.led.normal.on_percent = 87;
    config.led.warning.kind = LedPatternKind::Blink;
    config.led.warning.period_ms = 7654;
    config.led.warning.on_percent = 23;
    config.led.sync_warning_blink = false;
    config.led.during_feedback = true;
    config.led.on_debug = true;
    config.led.on_logs = true;
    config.led.backlight_off = true;
    for (index, widget) in config.widgets.iter_mut().enumerate() {
        widget.id += 60000;
        widget.label = "123456789012".try_into().unwrap();
        widget.warning.message = "12345678901234567890".try_into().unwrap();
        widget.warning.on_ms = 30_000;
        widget.warning.off_ms = 30_000;
        let (min, max) = widget.channel.domain();
        widget.display.graph_range = GraphRange::Fixed { min, max };
        widget.gauge.kind = GaugeKind::ALL[index % GaugeKind::ALL.len()];
        widget.gauge.expand = true;
        let step = (max - min) / 8.;
        let offset = step * index as f32 / 128.;
        widget.advanced.reference_mode = ReferenceMode::Manual;
        widget.advanced.reference = min + step + offset;
        widget.advanced.smoothing_ms = 5000 - index as u16;
        widget.advanced.peak_hold_ms = 60000 - index as u16;
        widget.advanced.peak_mode = if index % 2 == 0 {
            PeakSelection::Maximum
        } else {
            PeakSelection::Minimum
        };
        widget
            .advanced
            .set_bands(&[
                Band::new(min, min + step + offset, Color(0x07E0), "LOW").unwrap(),
                Band::new(min + step + offset, max - step - offset, Color(0x07FF), "MID").unwrap(),
                Band::new(max - step - offset, max, Color(0xF81F), "HIGH").unwrap(),
            ])
            .unwrap();
        widget.warning.enabled = true;
        widget.warning.applicability = Applicability::VehicleAndDemo;
        widget.warning.on_ms -= index as u16;
        widget.warning.off_ms -= index as u16;
        widget.warning.advanced.priority =
            [Priority::Low, Priority::Normal, Priority::High, Priority::Critical][index % 4];
        widget.warning.advanced.latch = index % 2 == 0;
        widget.warning.advanced.snooze_s = 900 - index as u16;
        if widget.channel != Channel::Lambda {
            let a = min + step;
            let b = min + 2. * step;
            let c = max - 2. * step;
            let d = max - step;
            if index % 2 == 0 {
                widget.warning.mode = LimitMode::OutsideWindow;
                widget.warning.trigger = a;
                widget.warning.clear = b;
                widget.warning.advanced.clear_high = c;
                widget.warning.advanced.trigger_high = d;
            } else {
                widget.warning.mode = LimitMode::InsideWindow;
                widget.warning.clear = a;
                widget.warning.trigger = b;
                widget.warning.advanced.trigger_high = c;
                widget.warning.advanced.clear_high = d;
            }
        }
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
        let mut slot = Box::new(ConfigSlot::new());
        assert_eq!(slot.load_at_boot(&bytes).config, valid);
        c.bench_function(&format!("config/{name}_slot_decode"), |b| {
            b.iter(|| {
                black_box(slot.load_at_boot(black_box(&bytes)));
            });
        });
        // Last field is the backlight_off bool. Reseal its invalid encoding so
        // fallback follows a complete bounded decode, rather than CRC rejection.
        let mut late = bytes.clone();
        *late.last_mut().unwrap() = 2;
        let crc = crc32(late[..8].iter().chain(late[HEADER_BYTES..].iter()).copied());
        late[8..12].copy_from_slice(&crc.to_le_bytes());
        assert_eq!(slot.load_at_boot(&late).config, validate(default_config()).unwrap());
        assert_eq!(slot.load_at_boot(&late).fault, Some(ConfigError::Payload));
        c.bench_function(&format!("config/{name}_slot_late_fallback"), |b| {
            b.iter(|| {
                black_box(slot.load_at_boot(black_box(&late)));
            });
        });
        if name == "maximum" {
            if let Some(path) = std::env::var_os("TASK11_MAX_EAV") {
                std::fs::write(path, &bytes).unwrap();
            }
        }
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

fn crc32(bytes: impl Iterator<Item = u8>) -> u32 {
    let mut crc = !0u32;
    for byte in bytes {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xEDB8_8320 & 0u32.wrapping_sub(crc & 1));
        }
    }
    !crc
}
