use std::hint::black_box;
use std::time::Duration;

use criterion::{Criterion, Throughput, criterion_group, criterion_main};
use dashboard_host_validation::acquisition::{AcquisitionModel, DashboardSnapshot};
use dashboard_host_validation::dashboard::{DashboardCache, DashboardScene};
use dashboard_host_validation::diagnostics::client::{Client, Event};
use dashboard_host_validation::diagnostics::isotp::{IsoTpRx, RxEvent};
use dashboard_host_validation::diagnostics::longan::Rx12;
use dashboard_host_validation::diagnostics::request::{Reply, Request};
use dashboard_host_validation::diagnostics::session::Session;
use dashboard_host_validation::diagnostics::{CanFrame, obd};
use dashboard_host_validation::display::damage::DamagePlan;
use dashboard_host_validation::render::FpsMode;
use dashboard_host_validation::renderer::St7789Renderer;
use dashboard_host_validation::samples::{Quantity, Sample, Source};
use dashboard_host_validation::sensor_state::SensorState;
use dashboard_host_validation::{AlignedFrame, FRAME_BYTES, primitives, widgets};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::watch::Watch;
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::Rectangle;

fn framebuffer(c: &mut Criterion) {
    let mut frame = AlignedFrame::boxed();
    let mut group = c.benchmark_group("framebuffer");
    group.throughput(Throughput::Bytes(FRAME_BYTES as u64));
    group.bench_function("clear_full", |b| {
        b.iter(|| {
            St7789Renderer::new(&mut frame.0).clear_buffer(black_box(Rgb565::BLUE));
            black_box(&frame.0);
        });
    });
    group.finish();

    let rect = Rectangle::new(Point::new(-8, 40), Size::new(128, 80));
    let mut group = c.benchmark_group("clipped_contiguous");
    group.throughput(Throughput::Elements(128 * 80));
    group.bench_function("left_8_px", |b| {
        b.iter(|| {
            let colors = (0..128 * 80).map(|i| {
                if black_box(i) & 1 == 0 {
                    Rgb565::RED
                } else {
                    Rgb565::BLUE
                }
            });
            St7789Renderer::new(&mut frame.0)
                .fill_contiguous(black_box(&rect), colors)
                .unwrap();
            black_box(&frame.0);
        });
    });
    group.finish();
}

fn widgets_and_history(c: &mut Criterion) {
    let mut frame = AlignedFrame::boxed();
    c.bench_function("boost_cell/psi_peak", |b| {
        b.iter(|| {
            widgets::draw_boost_cell(
                &mut St7789Renderer::new(&mut frame.0),
                0,
                26,
                80,
                107,
                black_box(1.0),
                black_box(2.0),
                true,
                false,
                true,
                0,
            );
            black_box(&frame.0);
        });
    });

    let mut samples = [0.0f32; 60];
    for (index, sample) in samples.iter_mut().enumerate() {
        *sample = (index as f32 * 0.37) % 2.5;
    }
    samples[59] = 3.0;
    c.bench_function("mini_graph/60_samples_newest_impulse", |b| {
        b.iter(|| {
            primitives::draw_mini_graph(
                &mut St7789Renderer::new(&mut frame.0),
                0,
                0,
                64,
                20,
                black_box(&samples),
                60,
                0,
                60,
                0.0,
                3.0,
                |_| Rgb565::WHITE,
            );
            black_box(&frame.0);
        });
    });

    let mut state = SensorState::new();
    let mut sample_index = 0usize;
    let mut sample_clock_ms = 0u64;
    let trace = [0.2f32, 0.8, 1.1, 2.0, 0.4, -0.3, 1.5, 0.7];
    c.bench_function("sensor_state/next_measurement_update", |b| {
        b.iter(|| {
            state.update_at(black_box(trace[sample_index % trace.len()]), false, sample_clock_ms);
            sample_index = sample_index.wrapping_add(1);
            sample_clock_ms = sample_clock_ms.wrapping_add(16);
            black_box(state.get_average());
        });
    });
}

fn acquisition(c: &mut Criterion) {
    let mut vehicle = AcquisitionModel::new();
    let mut sequence = 0;
    c.bench_function("acquisition/typed_sample_and_snapshot", |b| {
        b.iter(|| {
            sequence += 1;
            let sample = Sample {
                value: Quantity::CoolantC(black_box(95.0)),
                received_at_ms: sequence * 10,
                sequence,
                source: Source::Obd { ecu: 0x7E8, pid: 0x05 },
            };
            vehicle.ingest_sample(black_box(sample), sequence * 10).unwrap();
            black_box(vehicle.snapshot());
        });
    });
    let mut model = AcquisitionModel::demo();
    let mut at = 0u64;
    c.bench_function("acquisition/batch_and_snapshot", |b| {
        b.iter(|| {
            model.ingest_batch(black_box([1.5, 90.0, 95.0, 80.0, 30.0, 750.0, 13.8, 1.0]), at);
            at += 10;
            black_box(model.snapshot());
        });
    });
    let watch = Watch::<CriticalSectionRawMutex, DashboardSnapshot, 1>::new();
    let mut receiver = watch.receiver().unwrap();
    let snapshot = model.snapshot();
    c.bench_function("acquisition/watch_copy_roundtrip", |b| {
        b.iter(|| {
            watch.sender().send(black_box(snapshot));
            black_box(receiver.try_changed().unwrap());
        });
    });
}

fn damage_planning(c: &mut Criterion) {
    let previous = AlignedFrame::boxed();
    let mut current = AlignedFrame::boxed();
    for (label, mode) in [("stationary", 0), ("one_cell", 1), ("dense", 2)] {
        current.0.fill(0);
        if mode == 1 {
            for y in 32..128 {
                for x in 160..240 {
                    current.0[(y * 320 + x) * 2] = 0xF8;
                }
            }
        } else if mode == 2 {
            current.0.fill(0xFF);
        }
        c.bench_function(&format!("damage/{label}"), |b| {
            b.iter(|| black_box(DamagePlan::between(black_box(&previous.0), black_box(&current.0))));
        });
    }
}

fn dashboard_scene(c: &mut Criterion) {
    let mut model = AcquisitionModel::demo();
    for sample in 0..200 {
        model.ingest_batch([1.2, 90.0, 95.0, 80.0, 30.0, 750.0, 13.8, 1.0], sample * 10);
    }
    let snapshot = model.snapshot();
    let scene = DashboardScene {
        measurements: &snapshot,
        now_ms: 2000,
        blink_on: true,
        boost_psi: false,
        fps_mode: FpsMode::Off,
        fps_instant: 40.0,
        fps_average: 39.0,
        backgrounds: [Rgb565::BLACK; 8],
        overlay: None,
    };
    let mut frame = AlignedFrame::boxed();
    c.bench_function("scene/full_dashboard", |b| {
        b.iter(|| {
            black_box(&scene).draw_full(&mut St7789Renderer::new(&mut frame.0));
            black_box(&frame.0);
        });
    });
    let mut cache = DashboardCache::new();
    cache.draw(&mut St7789Renderer::new(&mut frame.0), 0, &scene);
    c.bench_function("scene/retained_stationary", |b| {
        b.iter(|| {
            cache.draw(&mut St7789Renderer::new(&mut frame.0), 0, black_box(&scene));
            black_box(&frame.0);
        });
    });
    // The paired changing fixtures toggle only boost. Acquisition is outside
    // the measured region, and both paths render exactly the same scenes.
    model.ingest_batch(
        [
            1.7,
            f32::NAN,
            f32::NAN,
            f32::NAN,
            f32::NAN,
            f32::NAN,
            f32::NAN,
            f32::NAN,
        ],
        2000,
    );
    let alternate = model.snapshot();
    for retained in [false, true] {
        let label = if retained {
            "scene/retained_one_cell"
        } else {
            "scene/full_one_cell"
        };
        let mut toggle = false;
        c.bench_function(label, |b| {
            b.iter(|| {
                toggle = !toggle;
                let scene = DashboardScene {
                    measurements: if toggle { &snapshot } else { &alternate },
                    ..scene
                };
                if retained {
                    cache.draw(&mut St7789Renderer::new(&mut frame.0), 0, black_box(&scene));
                } else {
                    black_box(&scene).draw_full(&mut St7789Renderer::new(&mut frame.0));
                }
                black_box(&frame.0);
            });
        });
    }
}

fn protocol(c: &mut Criterion) {
    let mut rx12 = Rx12::new();
    rx12.confirm_boundary().unwrap();
    c.bench_function("protocol/rx12_record", |b| {
        b.iter(|| {
            for byte in black_box([0, 0, 7, 0xE8, 4, 0x41, 0x42, 0x36, 0xB0, 0, 0, 0]) {
                black_box(rx12.push(byte).unwrap());
            }
        })
    });
    let sf = CanFrame::new(0x7E8, &[4, 0x41, 0x42, 0x36, 0xB0, 0, 0, 0]).unwrap();
    c.bench_function("protocol/single_frame_obd", |b| {
        b.iter(|| {
            let mut rx = IsoTpRx::new();
            rx.begin(0x7E8, 1, 0, 100).unwrap();
            let RxEvent::Complete(payload) = rx.on_frame(black_box(&sf), 1, 1).unwrap() else {
                panic!()
            };
            let Reply::Positive(bytes) = Request::Obd(0x42).reply(payload.as_slice()).unwrap() else {
                panic!()
            };
            black_box(obd::decode(0x42, bytes).unwrap());
        })
    });
    let first = CanFrame::new(0x7E8, &[0x10, 128, 0x62, 0xF1, 0x90, 1, 2, 3]).unwrap();
    let subsequent: Vec<_> = (0..18)
        .map(|i| CanFrame::new(0x7E8, &[0x20 | ((i + 1) & 15), 4, 5, 6, 7, 8, 9, 10]).unwrap())
        .collect();
    c.bench_function("protocol/isotp_128_bytes", |b| {
        b.iter(|| {
            let mut rx = IsoTpRx::new();
            rx.begin(0x7E8, 1, 0, 1000).unwrap();
            black_box(rx.on_frame(black_box(&first), 1, 0).unwrap());
            rx.flow_control_sent(0).unwrap();
            for (i, frame) in subsequent.iter().enumerate() {
                let now = (i + 1) as u64 * 5;
                let event = rx.on_frame(black_box(frame), 1, now).unwrap();
                if matches!(event, RxEvent::FlowControl(_)) {
                    rx.flow_control_sent(now).unwrap();
                }
                black_box(event);
            }
        })
    });
}

fn diagnostic_client(c: &mut Criterion) {
    let mut client = Client::new(0x7E0, 0x7E8, 1).unwrap();
    let mut now = 0;
    let sf = CanFrame::new(0x7E8, &[4, 0x41, 0x42, 0x36, 0xB0, 0, 0, 0]).unwrap();
    c.bench_function("client/single_frame_obd", |b| {
        b.iter(|| {
            let tx = client.start(black_box(Request::Obd(0x42)), now).unwrap();
            black_box(client.tx_completed(tx.token, now + 1).unwrap());
            let Event::Positive(reply) = client.receive(black_box(&sf), 1, now + 2).unwrap() else {
                panic!()
            };
            black_box(obd::decode(0x42, reply.bytes.as_slice()).unwrap());
            now += 100;
        });
    });
    let first = CanFrame::new(0x7E8, &[0x10, 128, 0x62, 0xF1, 0x90, 1, 2, 3]).unwrap();
    let subsequent: Vec<_> = (0..18)
        .map(|i| CanFrame::new(0x7E8, &[0x20 | ((i + 1) & 15), 4, 5, 6, 7, 8, 9, 10]).unwrap())
        .collect();
    c.bench_function("client/isotp_128_bytes", |b| {
        b.iter(|| {
            let tx = client.start(black_box(Request::ReadDid(0xF190)), now).unwrap();
            black_box(client.tx_completed(tx.token, now + 1).unwrap());
            let Event::Transmit(fc) = client.receive(black_box(&first), 1, now + 2).unwrap() else {
                panic!()
            };
            black_box(client.tx_completed(fc.token, now + 3).unwrap());
            for (index, frame) in subsequent.iter().enumerate() {
                let at = now + (index + 1) as u64 * 5;
                let event = client.receive(black_box(frame), 1, at).unwrap();
                if let Event::Transmit(fc) = &event {
                    black_box(client.tx_completed(fc.token, at + 1).unwrap());
                }
                black_box(event);
            }
            now += 100;
        });
    });
}

fn vehicle_session(c: &mut Criterion) {
    let mut session = Session::new(0x7E8, 1).unwrap();
    let tx = session.poll(0).unwrap().unwrap();
    session.tx_completed(tx.token, 1).unwrap();
    session
        .receive(&CanFrame::new(0x7E8, &[6, 0x41, 0, 8, 0, 0, 0, 0]).unwrap(), 1, 2)
        .unwrap();
    let coolant = CanFrame::new(0x7E8, &[3, 0x41, 5, 135, 0, 0, 0, 0]).unwrap();
    let mut now = 2_000;
    c.bench_function("session/coolant_transaction_and_snapshot", |b| {
        b.iter(|| {
            let tx = session.poll(black_box(now)).unwrap().unwrap();
            black_box(session.tx_completed(tx.token, now + 1).unwrap());
            black_box(session.receive(black_box(&coolant), 1, now + 2).unwrap());
            black_box(session.snapshot());
            now += 2_000;
        });
    });
}

criterion_group! {
    name = dashboard_perf;
    config = Criterion::default()
        .sample_size(30)
        .warm_up_time(Duration::from_secs(1))
        .measurement_time(Duration::from_secs(2));
    targets = framebuffer, widgets_and_history, acquisition, damage_planning, dashboard_scene, protocol, diagnostic_client, vehicle_session
}
criterion_main!(dashboard_perf);
