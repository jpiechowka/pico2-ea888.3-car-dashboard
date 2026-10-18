//! Host CPU costs only; no BLE scheduling, CAN, ECU or display timing.
use std::hint::black_box;
use std::time::Duration;

use criterion::{BatchSize, Criterion, criterion_group, criterion_main};
use dashboard_core::ble::link::LinkSnapshot;
use dashboard_core::ble::poll::{Cadence, Scheduler};
use dashboard_core::ble::session::{Command, Completion, INITIALIZATION, Mode, QualifiedRead, Session};

fn initialized() -> (Session, u64) {
    let mut session = Session::new(Mode::SyntheticPeer);
    session.connect(1, 0).unwrap();
    let mut sequence = 0;
    for init in INITIALIZATION {
        let tx = session.begin(Command::Initialize(init), 0, 500).unwrap();
        session.sent(tx.token, 0).unwrap();
        sequence += 1;
        session.notify(1, sequence, b"OK\r>", 0).unwrap().unwrap();
    }
    (session, sequence)
}

fn transactions(c: &mut Criterion) {
    println!(
        "Host layouts: Session={} Completion={} Scheduler<8>={} LinkSnapshot={}",
        size_of::<Session>(),
        size_of::<Completion>(),
        size_of::<Scheduler<8>>(),
        size_of::<LinkSnapshot>()
    );
    let mut group = c.benchmark_group("ble_protocol");
    group.sample_size(30);
    group.warm_up_time(Duration::from_secs(1));
    group.measurement_time(Duration::from_secs(2));
    let reply = b"7E8 8 03 41 05 82 00 00 00 00\r>";
    for chunk_size in [reply.len(), 1] {
        group.bench_function(format!("coolant_chunks_{chunk_size}"), |b| {
            b.iter_batched(
                initialized,
                |(mut session, mut sequence)| {
                    let read = QualifiedRead::synthetic_obd(black_box(5)).unwrap();
                    let tx = session.begin(Command::Read(read), 1, 500).unwrap();
                    session.sent(tx.token, 2).unwrap();
                    for chunk in black_box(reply).chunks(chunk_size) {
                        sequence += 1;
                        black_box(session.notify(1, sequence, chunk, 10).unwrap());
                    }
                    black_box(session);
                },
                BatchSize::SmallInput,
            );
        });
    }
    group.bench_function("four_channel_completion", |b| {
        b.iter(|| {
            let mut scheduler = Scheduler::new(
                1,
                [Cadence {
                    period_ms: 1000,
                    deadline_ms: 500,
                }; 4],
                5,
                0,
            )
            .unwrap();
            for now in [0, 50, 100, 150] {
                let ticket = scheduler.next(black_box(now)).unwrap().unwrap();
                scheduler.complete(ticket, now + 40, true).unwrap();
            }
            black_box(scheduler.latency());
            black_box(scheduler);
        });
    });
    group.finish();
}

criterion_group!(benches, transactions);
criterion_main!(benches);
