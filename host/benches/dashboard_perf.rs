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

#[allow(dead_code)]
#[path = "../../firmware/vendor/embassy-rp/src/uart/buffered_io.rs"]
mod uart_io;

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
    for (label, mode) in [
        ("stationary", 0),
        ("one_cell", 1),
        ("dense", 2),
        ("first_byte", 3),
        ("last_byte", 4),
        ("last_byte_each_band", 5),
    ] {
        current.0.fill(0);
        if mode == 1 {
            for y in 32..128 {
                for x in 160..240 {
                    current.0[(y * 320 + x) * 2] = 0xF8;
                }
            }
        } else if mode == 2 {
            current.0.fill(0xFF);
        } else if mode == 3 {
            current.0[0] = 0x80;
        } else if mode == 4 {
            current.0[FRAME_BYTES - 1] = 0x80;
        } else if mode == 5 {
            for end in (16 * 320 * 2..=FRAME_BYTES).step_by(16 * 320 * 2) {
                current.0[end - 1] = 0x80;
            }
        }
        let plan = DamagePlan::between(&previous.0, &current.0);
        let expected_bytes = match mode {
            0 => 0,
            1 => 80 * 96 * 2,
            3 | 4 => 16 * 16 * 2,
            // The tall last-column region exceeds the bounded row-start budget.
            _ => FRAME_BYTES,
        };
        assert_eq!(plan.bytes(), expected_bytes, "damage preflight: {label}");
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
        readings: None,
        layout: None,
        measurements: &snapshot,
        now_ms: 2000,
        status: dashboard_host_validation::vehicle_status::VehicleStatus::Demo,
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
        // Independent initial pixels/cache: filtering or the preceding full
        // renderer must not leave the retained case with a mismatched cache.
        let mut frame = AlignedFrame::boxed();
        let mut cache = DashboardCache::new();
        cache.draw(&mut St7789Renderer::new(&mut frame.0), 0, &scene);
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
                    readings: None,
                    layout: None,
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
    // Independent fixed seed: changing Criterion's filter or preceding run
    // length must not change this export workload.
    let mut export_session = Session::new(0x7E8, 1).unwrap();
    for step in 0..56u64 {
        let now = step * 2_000;
        let tx = export_session.poll(now).unwrap().unwrap();
        export_session.tx_completed(tx.token, now + 1).unwrap();
        let pid = tx.frame.data()[2];
        let value: &[u8] = match pid {
            0 => &[8, 2, 0, 1],
            0x20 => &[0, 0, 0, 1],
            0x40 => &[0x40, 0, 0, 0x10],
            0x42 => &[0x36, 0xB0],
            _ => &[130],
        };
        let mut bytes = [0u8; 8];
        bytes[..3].copy_from_slice(&[value.len() as u8 + 2, 0x41, pid]);
        bytes[3..3 + value.len()].copy_from_slice(value);
        export_session
            .receive(&CanFrame::new(0x7E8, &bytes).unwrap(), 1, now + 2 + step % 7 * 60)
            .unwrap();
    }
    c.bench_function("session/diagnostic_distribution_export", |b| {
        b.iter(|| black_box(black_box(&export_session).telemetry()));
    });
}

mod vehicle_port;

fn vehicle_service(c: &mut Criterion) {
    use std::task::{Context, Waker};

    use dashboard_host_validation::diagnostics::service::{Profile, ReceiveContract, Vehicle};
    let mut vehicle = Vehicle::new(
        vehicle_port::ScriptedPort::new(),
        Profile {
            responder: 0x7E8,
            receive: ReceiveContract::PaddedStandardDataDlc8,
        },
        0,
    )
    .unwrap();
    let mut cx = Context::from_waker(Waker::noop());
    for now in [0, 50, 51, 52, 53, 54, 55, 56] {
        vehicle.step(&mut cx, now);
    }
    assert_eq!(vehicle.link().diagnostics.completed, 1);
    let mut now = 2_000;
    c.bench_function("vehicle/coolant_io_session_snapshot", |b| {
        b.iter(|| {
            vehicle.step(&mut cx, black_box(now));
            vehicle.step(&mut cx, black_box(now + 1));
            black_box(vehicle.snapshot());
            now += 2_000;
        });
    });
}

fn uart_ring(c: &mut Criterion) {
    use core::sync::atomic::AtomicU8;
    use core::task::Poll;

    use embassy_hal_internal::atomic_ring_buffer::RingBuffer;

    let mut storage = [0; 1024];
    let ring = RingBuffer::new();
    // This fixture retains storage and the single reader/writer until completion.
    unsafe { ring.init(storage.as_mut_ptr(), storage.len()) };
    let mut reader = unsafe { ring.reader() };
    let mut writer = unsafe { ring.writer() };
    let errors = AtomicU8::new(0);
    c.bench_function("uart/rx12_ring_copy", |b| {
        b.iter(|| {
            let input = black_box([0, 0, 7, 0xE8, 3, 0x41, 5, 135, 0, 0, 0, 0]);
            let mut at = 0;
            while at < input.len() {
                uart_io::fill_rx(&mut writer, &errors, 32, || {
                    let byte = input.get(at).copied()?;
                    at += 1;
                    Some(Ok(byte))
                });
            }
            let mut out = [0; 12];
            let mut copied = 0;
            while copied < out.len() {
                let Poll::Ready(Ok(n)) = uart_io::try_read(&mut reader, &errors, &mut out[copied..], || {
                    black_box(());
                }) else {
                    panic!()
                };
                copied += n;
            }
            black_box(out);
        });
    });
}

fn alarm_policy(c: &mut Criterion) {
    use dashboard_host_validation::alarms::{AlarmEngine, demo_contexts, demo_rules};
    let mut model = AcquisitionModel::demo();
    let snapshots = [0, 500, 1000].map(|at| {
        model.ingest_batch([1.0, 140.0, 120.0, 130.0, 70.0, 1100.0, 11.0, 1.3], at);
        model.snapshot()
    });
    let contexts = snapshots.map(|snapshot| demo_contexts(&snapshot));
    let rules = demo_rules();
    c.bench_function("alarms/eight_channel_three_samples", |b| {
        b.iter(|| {
            let mut engine = AlarmEngine::new(black_box(rules)).unwrap();
            for (i, at) in [0, 500, 1000].into_iter().enumerate() {
                black_box(engine.update(black_box(&snapshots[i]), black_box(&contexts[i]), at));
            }
            black_box(engine.snapshot())
        })
    });
}

fn logging(c: &mut Criterion) {
    use dashboard_host_validation::logging::{Level, LogStore};
    use dashboard_host_validation::logging_view::LogPage;
    let store = LogStore::<CriticalSectionRawMutex>::new();
    for n in 0..128 {
        store.push(Level::Info, n, format_args!("Transport observation {n}"));
    }
    c.bench_function("logging/insert_and_snapshot", |b| {
        b.iter(|| {
            let timestamp = black_box(1234);
            store.push(
                Level::Info,
                timestamp,
                format_args!("Transport observation {timestamp}"),
            );
            black_box(LogPage::capture(&store, None));
        });
    });
    // Selection/calibration of the insertion fixture cannot change pixel work.
    let render_store = LogStore::<CriticalSectionRawMutex>::new();
    for n in 0..128 {
        render_store.push(Level::Info, n, format_args!("Transport observation {n}"));
    }
    render_store.push(Level::Fault, 1234, format_args!("Transport fault"));
    let page = LogPage::capture(&render_store, None);
    let mut frame = AlignedFrame::boxed();
    c.bench_function("logging/full_page", |b| {
        b.iter(|| {
            page.draw_full(&mut St7789Renderer::new(&mut frame.0));
            black_box(&frame.0);
        });
    });
}

fn settings_journal(c: &mut Criterion) {
    use dashboard_host_validation::settings::{self, PAGE_SIZE, SECTOR_SIZE, STORAGE_SIZE, Settings, Storage};
    #[derive(Clone)]
    struct Memory([u8; STORAGE_SIZE]);
    impl Storage for Memory {
        type Error = ();

        fn read(
            &mut self,
            offset: u32,
            out: &mut [u8],
        ) -> Result<(), ()> {
            out.copy_from_slice(&self.0[offset as usize..offset as usize + out.len()]);
            Ok(())
        }

        fn program(
            &mut self,
            offset: u32,
            page: &[u8; PAGE_SIZE],
        ) -> Result<(), ()> {
            let slice = &mut self.0[offset as usize..offset as usize + PAGE_SIZE];
            for (byte, source) in slice.iter_mut().zip(page) {
                *byte &= source;
            }
            Ok(())
        }

        fn erase(
            &mut self,
            offset: u32,
        ) -> Result<(), ()> {
            self.0[offset as usize..offset as usize + SECTOR_SIZE].fill(255);
            Ok(())
        }
    }
    let mut seeded = Memory([255; STORAGE_SIZE]);
    for n in 1..=16 {
        settings::save(&mut seeded, Settings::new(n, 100, false, FpsMode::Off).unwrap()).unwrap();
    }
    let mut reader = seeded.clone();
    c.bench_function("settings/load_full_journal", |b| {
        b.iter(|| black_box(settings::load(black_box(&mut reader)).unwrap()));
    });
    let desired = Settings::new(17, 100, true, FpsMode::Combined).unwrap();
    c.bench_function("settings/rollover_transaction_model", |b| {
        b.iter_batched(
            || seeded.clone(),
            |mut flash| {
                black_box(settings::save(black_box(&mut flash), black_box(desired)).unwrap());
            },
            criterion::BatchSize::PerIteration,
        );
    });
}

fn maintenance(c: &mut Criterion) {
    use dashboard_host_validation::health::{Progress, ProgressMonitor};
    use dashboard_host_validation::maintenance::{Command, Gate, Owner};
    c.bench_function("settings/maintenance_transaction_model", |b| {
        b.iter(|| {
            let progress = black_box(Progress {
                acquisition: 1,
                rendering: 1,
                input: 1,
                completed: 0,
                submitted: 0,
                display_pending: false,
            });
            let mut health = ProgressMonitor::new(black_box(0), progress);
            let ticket = health.begin_maintenance(black_box(1), progress).unwrap();
            let mut gate = Gate::new();
            let request = gate.begin(black_box(&ticket)).unwrap();
            for owner in [Owner::Input, Owner::Acquisition] {
                black_box(gate.acknowledge(Command::Park(request), black_box(owner), true));
            }
            gate.begin_write(black_box(100)).unwrap();
            gate.writing(request, black_box(300)).unwrap();
            gate.release(request).unwrap();
            for owner in [Owner::Acquisition, Owner::Input] {
                black_box(gate.acknowledge(Command::Resume(request), black_box(owner), true));
            }
            gate.finish(request, black_box(400)).unwrap();
            health.end_maintenance(ticket, black_box(401), progress).unwrap();
            black_box((health, gate));
        });
    });
}

criterion_group! {
    name = dashboard_perf;
    config = Criterion::default()
        .sample_size(30)
        .warm_up_time(Duration::from_secs(1))
        .measurement_time(Duration::from_secs(2));
    targets = framebuffer, widgets_and_history, acquisition, damage_planning, dashboard_scene, protocol, diagnostic_client, vehicle_session, vehicle_service, uart_ring, alarm_policy, logging, settings_journal, maintenance, input_mailbox, frame_window, target_kernels, mixed_scene_replay, coverage::benchmarks, config_perf::benchmarks, view_perf::benchmarks, rule_perf::benchmarks
}
criterion_main!(dashboard_perf);

mod config_perf;
mod coverage;
mod rule_perf;
mod scene_replay;
mod view_perf;

fn mixed_scene_replay(c: &mut Criterion) {
    scene_replay::verify();
    let snapshots = scene_replay::snapshots();
    let initial = scene_replay::model().snapshot();
    for retained in [false, true] {
        let mode = if retained { "retained" } else { "full" };
        c.bench_function(&format!("scene/{mode}_all_changed_eight_frames"), |b| {
            b.iter_batched_ref(
                || scene_replay::Raster::new(&initial),
                |raster| raster.run(black_box(&snapshots), black_box(retained)),
                criterion::BatchSize::PerIteration,
            )
        });
        c.bench_function(&format!("replay/demo_model_to_{mode}_eight_frames"), |b| {
            b.iter_batched_ref(
                || (scene_replay::model(), scene_replay::Raster::new(&initial)),
                |(model, raster)| scene_replay::run_model(black_box(model), black_box(raster), black_box(retained)),
                criterion::BatchSize::PerIteration,
            )
        });
    }
}

fn target_kernels(c: &mut Criterion) {
    use dashboard_host_validation::display::FrameBuffer;
    use dashboard_host_validation::microbench::ITERATIONS;
    use dashboard_host_validation::microbench::workload::{Fixture, Kernel};
    for kind in Kernel::ALL {
        let name = format!("target_kernel/{kind:?}_eight_iterations");
        // Verify the same finite output before timing. Setup and full checksum
        // are excluded, matching the target batch's computation boundary.
        let mut check_frame = Box::new(FrameBuffer::new());
        let mut check_fixture = Fixture::new();
        for _ in 0..ITERATIONS {
            check_fixture.run(kind, &mut check_frame);
        }
        assert_eq!(
            check_fixture.checksum(kind, &check_frame).unwrap(),
            kind.expected_checksum()
        );
        c.bench_function(&name, |b| {
            b.iter_batched_ref(
                || (Box::new(FrameBuffer::new()), Fixture::new()),
                |(frame, fixture)| {
                    for _ in 0..ITERATIONS {
                        black_box(&mut *fixture).run(black_box(kind), black_box(frame));
                    }
                    black_box((&*frame, &*fixture));
                },
                criterion::BatchSize::PerIteration,
            )
        });
    }
}

// Exact input_model.rs from 56af170, retained solely for the paired cost fixture.
#[allow(dead_code)]
mod input_legacy;

fn input_mailbox(c: &mut Criterion) {
    let legacy = input_legacy::EncoderEvents::new();
    let current = dashboard_host_validation::input_model::EncoderEvents::new();
    legacy.publish_delta(5);
    legacy.publish_press();
    current.publish_delta(5);
    current.publish_press();
    assert_eq!(legacy.take(), (5, 1));
    assert_eq!(current.take(), (5, 1));
    c.bench_function("input/legacy_atomic_roundtrip", |b| {
        b.iter(|| {
            legacy.publish_delta(black_box(5));
            legacy.publish_press();
            black_box(legacy.take());
        })
    });
    c.bench_function("input/coherent_mailbox_roundtrip", |b| {
        b.iter(|| {
            current.publish_delta(black_box(5));
            current.publish_press();
            black_box(current.take());
        })
    });
}

fn frame_window(c: &mut Criterion) {
    use dashboard_host_validation::display::pipeline::{BufferId, FrameToken};
    use dashboard_host_validation::display::telemetry::{FrameTrace, RenderTrace};
    use dashboard_host_validation::display::window::{FrameWindow, WINDOW_US};
    use dashboard_host_validation::input_trace::{InputCounters, InputTrace};
    let trace = |id, at| {
        FrameTrace::finish(
            FrameToken {
                buffer: BufferId::A,
                sequence: id,
            },
            RenderTrace {
                start_us: at,
                end_us: at + 3,
                queued_us: at + 5,
                blocked_us: 2,
                active_cycles: 50,
                sample_us: Some(0),
                input: Some(InputTrace {
                    first_id: id,
                    last_id: id,
                    observed_us: 0,
                    accepted_us: 1,
                    consumed_us: 2,
                    events: 1,
                }),
            },
            at + 10,
            at + 20,
            153600,
        )
        .unwrap()
    };
    let first = trace(1, 100);
    let second = trace(2, 115);
    c.bench_function("frame_window/record_all_metrics", |b| {
        b.iter_batched_ref(
            || {
                let mut w = Box::new(FrameWindow::new(0));
                w.record(first).unwrap();
                w
            },
            |w| black_box(w).record(black_box(second)).unwrap(),
            criterion::BatchSize::PerIteration,
        )
    });
    c.bench_function("frame_window/summarize_1500_frames", |b| {
        b.iter_batched_ref(
            || {
                let mut w = Box::new(FrameWindow::new(0));
                for id in 1..=1500 {
                    w.record(trace(id, id * 10000)).unwrap();
                }
                w
            },
            |w| {
                black_box(w.report(black_box(WINDOW_US), InputCounters::ZERO).unwrap().unwrap());
            },
            criterion::BatchSize::PerIteration,
        )
    });
}
