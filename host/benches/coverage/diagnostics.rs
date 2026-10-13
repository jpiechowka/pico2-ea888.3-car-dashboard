use std::hint::black_box;

use criterion::{BatchSize, Criterion};
use dashboard_host_validation::acquisition::DashboardSnapshot;
use dashboard_host_validation::dashboard::{DashboardCache, DashboardScene};
use dashboard_host_validation::diagnostics::CanFrame;
use dashboard_host_validation::diagnostics::client::{Client, Event, Failure};
use dashboard_host_validation::diagnostics::isotp::IsoError;
use dashboard_host_validation::diagnostics::longan::Rx12;
use dashboard_host_validation::diagnostics::request::Request;
use dashboard_host_validation::diagnostics::session::Session;
use dashboard_host_validation::display::{FrameBuffer, St7789Renderer};
use dashboard_host_validation::render::FpsMode;
use dashboard_host_validation::samples::{Quality, SensorId};
use dashboard_host_validation::vehicle_status::VehicleStatus;
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::RgbColor;

fn started() -> Client {
    let mut client = Client::new(0x7E0, 0x7E8, 1).unwrap();
    let tx = client.start(Request::Obd(5), 0).unwrap();
    assert_eq!(client.tx_completed(tx.token, 1).unwrap(), Event::Ignored);
    client
}

#[derive(Clone, Copy, Debug)]
enum Terminal {
    Negative,
    Malformed,
    Timeout,
    PendingThenNegative,
}
const TERMINALS: [Terminal; 4] = [
    Terminal::Negative,
    Terminal::Malformed,
    Terminal::Timeout,
    Terminal::PendingThenNegative,
];
fn terminal(
    client: &mut Client,
    case: Terminal,
) -> (Event, Event) {
    let frame = |bytes: &[u8]| CanFrame::new(0x7E8, bytes).unwrap();
    let initial = if matches!(case, Terminal::PendingThenNegative) {
        client.receive(&black_box(frame(&[3, 0x7F, 1, 0x78])), 1, 400).unwrap()
    } else {
        Event::Ignored
    };
    let result = match case {
        Terminal::Negative => client.receive(&black_box(frame(&[3, 0x7F, 1, 0x12])), 1, 100),
        Terminal::Malformed => client.receive(&black_box(frame(&[7, 0x41, 5])), 1, 100),
        Terminal::Timeout => client.tick(black_box(502)),
        Terminal::PendingThenNegative => client.receive(&black_box(frame(&[3, 0x7F, 1, 0x12])), 1, 800),
    }
    .unwrap();
    (initial, result)
}

pub fn verify_terminals() {
    for case in TERMINALS {
        let mut client = started();
        let (initial, event) = terminal(&mut client, case);
        assert_eq!(
            initial,
            if matches!(case, Terminal::PendingThenNegative) {
                Event::Pending
            } else {
                Event::Ignored
            }
        );
        let expected = match case {
            Terminal::Negative | Terminal::PendingThenNegative => {
                Event::Negative {
                    request: Request::Obd(5),
                    nrc: 0x12,
                }
            }
            Terminal::Malformed => {
                Event::Failed {
                    request: Request::Obd(5),
                    reason: Failure::Transport(IsoError::Length),
                }
            }
            Terminal::Timeout => {
                Event::Failed {
                    request: Request::Obd(5),
                    reason: Failure::ResponseTimeout,
                }
            }
        };
        assert_eq!(event, expected, "{case:?}");
        assert!(!client.active());
        assert_eq!(
            client
                .receive(&CanFrame::new(0x7E8, &[3, 0x41, 5, 135]).unwrap(), 1, 900)
                .unwrap(),
            Event::Ignored
        );
    }
}

// Synthetic qualified RX12 records. Qualification is a fixture assumption;
// their contents do not establish DLC/flags or any real bridge/ECU support.
const CAP: [u8; 12] = [0, 0, 7, 0xE8, 6, 0x41, 0, 8, 0, 0, 0, 0];
const COOLANT: [u8; 12] = [0, 0, 7, 0xE8, 3, 0x41, 5, 135, 0, 0, 0, 0];
const BAD: [u8; 12] = [0, 0, 7, 0xE8, 4, 0x41, 5, 140, 0, 0, 0, 0];
const LOW: [u8; 12] = [0, 0, 7, 0xE8, 3, 0x41, 5, 130, 0, 0, 0, 0];
const BURST: [u8; 36] = [
    0, 0, 7, 0xE8, 3, 0x41, 5, 145, 0, 0, 0, 0, 0, 0, 7, 0xE8, 3, 0x41, 5, 255, 0, 0, 0, 0, 0, 0, 7, 0xE8, 3, 0x41, 5,
    130, 0, 0, 0, 0,
];
enum Action {
    Poll,
    Bytes(&'static [u8]),
    Observe,
}
struct Step {
    at: u64,
    action: Action,
    request: Option<u8>,
    quality: Quality,
    value: Option<f32>,
    sample_at: Option<u64>,
}
fn steps() -> [Step; 17] {
    use Action::*;
    use Quality::*;
    let step = |at, action, request, quality, value, sample_at| {
        Step {
            at,
            action,
            request,
            quality,
            value,
            sample_at,
        }
    };
    [
        step(1, Poll, Some(0), Unknown, None, None),
        step(200, Bytes(&CAP[..3]), None, Unknown, None, None),
        step(210, Bytes(&CAP[3..]), None, Unknown, None, None),
        step(231, Poll, Some(5), Unknown, None, None),
        step(600, Bytes(&COOLANT[..5]), None, Unknown, None, None),
        step(620, Bytes(&COOLANT[5..]), None, Valid, Some(95.0), Some(620)),
        step(1621, Poll, Some(5), Valid, Some(95.0), Some(620)),
        step(2123, Poll, None, TransportError, None, Some(620)),
        step(3123, Poll, Some(5), TransportError, None, Some(620)),
        step(3320, Bytes(&BAD), None, InvalidPayload, None, Some(620)),
        step(4321, Poll, Some(5), InvalidPayload, None, Some(620)),
        step(4600, Bytes(&BURST[..4]), None, InvalidPayload, None, Some(620)),
        step(4610, Bytes(&BURST[4..]), None, Valid, Some(105.0), Some(4610)),
        step(5611, Poll, Some(5), Valid, Some(105.0), Some(4610)),
        step(5710, Bytes(&LOW), None, Valid, Some(90.0), Some(5710)),
        step(6000, Observe, None, Valid, Some(90.0), Some(5710)),
        step(9001, Observe, None, Stale, None, Some(5710)),
    ]
}

struct Replay {
    session: Session<'static>,
    parser: Rx12,
    frames: [Box<FrameBuffer>; 2],
    cache: DashboardCache,
}
impl Replay {
    fn new() -> Self {
        let mut parser = Rx12::new();
        let epoch = parser.confirm_boundary().unwrap();
        Self {
            session: Session::new(0x7E8, epoch).unwrap(),
            parser,
            frames: [Box::new(FrameBuffer::new()), Box::new(FrameBuffer::new())],
            cache: DashboardCache::new(),
        }
    }

    fn advance(
        &mut self,
        step: &Step,
    ) -> Option<u8> {
        match step.action {
            Action::Poll => {
                self.session.poll(black_box(step.at - 1)).unwrap().map(|tx| {
                    self.session.tx_completed(tx.token, black_box(step.at)).unwrap();
                    tx.frame.data()[2]
                })
            }
            Action::Bytes(bytes) => {
                for &byte in black_box(bytes) {
                    if let Some(record) = self.parser.push(byte).unwrap() {
                        let frame = CanFrame::new(record.id(), record.data()).unwrap();
                        black_box(
                            self.session
                                .receive(&frame, record.epoch(), black_box(step.at))
                                .unwrap(),
                        );
                    }
                }
                None
            }
            Action::Observe => None,
        }
    }

    fn draw(
        &mut self,
        snapshot: &DashboardSnapshot,
        index: usize,
        now: u64,
        retained: bool,
    ) {
        let scene = DashboardScene {
            readings: None,
            layout: None,
            measurements: black_box(snapshot),
            status: VehicleStatus::derive(false, None, snapshot, now),
            now_ms: now,
            blink_on: false,
            boost_psi: false,
            fps_mode: FpsMode::Off,
            fps_instant: 0.0,
            fps_average: 0.0,
            backgrounds: [Rgb565::BLACK; 8],
            overlay: None,
        };
        let mut display = St7789Renderer::new(&mut self.frames[index % 2]);
        if retained {
            black_box(self.cache.draw(&mut display, index % 2, black_box(&scene)));
        } else {
            black_box(&scene).draw_full(&mut display);
        }
        black_box(&self.frames[index % 2]);
    }

    fn run(
        &mut self,
        steps: &[Step],
        retained: bool,
    ) {
        for (i, step) in steps.iter().enumerate() {
            black_box(self.advance(black_box(step)));
            self.draw(&self.session.snapshot(), i, step.at, retained);
        }
    }
}

pub fn verify_replay() {
    let steps = steps();
    let mut retained = Replay::new();
    let mut full = Replay::new();
    for (i, step) in steps.iter().enumerate() {
        assert_eq!(retained.advance(step), step.request, "request at {}", step.at);
        assert_eq!(full.advance(step), step.request);
        let snapshot = retained.session.snapshot();
        assert_eq!(snapshot, full.session.snapshot());
        assert_eq!(
            snapshot.quality(SensorId::Water, step.at),
            step.quality,
            "quality at {}",
            step.at
        );
        assert_eq!(snapshot.value_at(SensorId::Water, step.at), step.value);
        assert_eq!(snapshot.sensor(SensorId::Water).sampled_at_ms(), step.sample_at);
        assert_eq!(
            snapshot.metadata[SensorId::Water as usize]
                .received_at_ms
                .map(|t| step.at - t),
            step.sample_at.map(|t| step.at - t)
        );
        retained.draw(&snapshot, i, step.at, true);
        full.draw(&snapshot, i, step.at, false);
        assert!(retained.frames == full.frames, "image at {}", step.at);
    }
    let snapshot = retained.session.snapshot();
    assert_eq!(snapshot.sensor(SensorId::Water).minimum(), Some(90.0));
    assert_eq!(
        snapshot.sensor(SensorId::Water).maximum(),
        Some(105.0),
        "unsolicited burst replies cannot alter peaks"
    );
    let counters = retained.session.counters();
    assert_eq!(counters.completed, 4);
    assert_eq!(counters.timeouts, 1);
    assert_eq!(counters.malformed, 1);
    assert!(retained.parser.at_boundary());
    for mode in [false, true] {
        let mut actual = Replay::new();
        actual.run(&steps, mode);
        assert!(actual.frames == full.frames);
        assert_eq!(actual.session.snapshot(), snapshot);
    }
}

pub fn benchmarks(c: &mut Criterion) {
    verify_terminals();
    verify_replay();
    for case in TERMINALS {
        c.bench_function(&format!("client_terminal/{case:?}"), |b| {
            b.iter_batched_ref(
                started,
                |client| black_box(terminal(black_box(client), black_box(case))),
                BatchSize::PerIteration,
            )
        });
    }
    let steps = steps();
    for retained in [false, true] {
        let mode = if retained { "retained" } else { "full" };
        c.bench_function(&format!("replay/slow_bursty_session_to_{mode}_seventeen_frames"), |b| {
            b.iter_batched_ref(
                Replay::new,
                |replay| replay.run(black_box(&steps), black_box(retained)),
                BatchSize::PerIteration,
            )
        });
    }
}
