//! Synthetic BLE probe evidence: cargo run --locked --example ble_probe_gallery -- <output-dir>.
//! Converts RGB565 framebuffer bytes to PPM; no radio or hardware is accessed.
use std::io::Write;
use std::path::Path;

use dashboard_core::discovery::*;
use dashboard_core::display::St7789Renderer;
use dashboard_core::display::telemetry::FrameRate;
use dashboard_host_validation::AlignedFrame;
fn save(
    dir: &Path,
    name: &str,
    p: &Probe,
) {
    let mut frame = AlignedFrame::boxed();
    render(&mut St7789Renderer::new(&mut frame.0), p, 12_000).unwrap();
    let mut f = std::fs::File::create(dir.join(format!("{name}.ppm"))).unwrap();
    f.write_all(b"P6\n320 240\n255\n").unwrap();
    for px in frame.0.as_chunks::<2>().0 {
        let rgb = u16::from_be_bytes(*px);
        let (r, g, b) = ((rgb >> 11) as u8, ((rgb >> 5) & 63) as u8, (rgb & 31) as u8);
        f.write_all(&[(r << 3) | (r >> 2), (g << 2) | (g >> 4), (b << 3) | (b >> 2)])
            .unwrap();
    }
}
fn main() {
    let directory = std::env::args_os().nth(1).expect("output directory");
    let directory = Path::new(&directory);
    std::fs::create_dir_all(directory).unwrap();
    let mut p = Probe::new();
    // Deliberately synthetic measurements, not a device performance claim.
    p.diagnostics = ProbeDiagnostics {
        fps: Some(FrameRate {
            current: 19.8,
            average: 19.7,
        }),
        work: [Some(125), Some(68)],
        ram: Some(MemoryUse {
            used: 373_000,
            total: 524_288,
        }),
        stacks: [
            Some(MemoryUse {
                used: 7_000,
                total: 151_288,
            }),
            Some(MemoryUse {
                used: 4_000,
                total: 32_768,
            }),
        ],
    };
    p.phase = RadioPhase::Scanning;
    p.encoder_connected = true;
    for (n, name) in [
        "SYNTHETIC peer A",
        "SYNTHETIC peer B",
        "SYNTHETIC peer C",
        "SYNTHETIC peer D",
        "SYNTHETIC peer E",
    ]
    .iter()
    .enumerate()
    {
        let mut data = vec![2, 1, 6, 3, 3, 0x0A, 0x18];
        data.push((name.len() + 1) as u8);
        data.push(9);
        data.extend_from_slice(name.as_bytes());
        p.catalog.observe(Advertisement::new(
            Peer {
                address: [0xC0, 0, 0, 0, 0, n as u8],
                random: true,
            },
            ReportKind::Connectable,
            -40 - n as i8 * 9,
            &data,
            10_000 - n as u64 * 1000,
        ));
    }
    p.selected = 1;
    save(directory, "scan-synthetic", &p);
    let pause = p.input(Input::Back, 12_000).unwrap();
    p.event(Event::State {
        revision: pause.revision,
        phase: RadioPhase::Paused,
        error: ProbeError::None,
    });
    p.encoder_connected = false;
    save(directory, "paused-synthetic", &p);
    p.input(Input::Details, 12_000).unwrap();
    p.phase = RadioPhase::Paused;
    save(directory, "advertisement-synthetic", &p);
    p.page = page_count(&p) - 1;
    save(directory, "not-inspected-synthetic", &p);
    p.input(Input::Click, 12_000).unwrap();
    p.phase = RadioPhase::Discovering;
    save(directory, "inspecting-synthetic", &p);
    p.phase = RadioPhase::Paused;
    p.inspection
        .services
        .push(Service {
            start: 1,
            end: 9,
            uuid: Uuid::from_wire(&[0x0A, 0x18]).unwrap(),
        })
        .unwrap();
    p.inspection
        .characteristics
        .push(Characteristic {
            service_start: 1,
            declaration: 2,
            handle: 3,
            properties: 2,
            uuid: Uuid::from_wire(&[0x29, 0x2A]).unwrap(),
        })
        .unwrap();
    p.inspection
        .values
        .push(InfoValue {
            uuid: 0x2A29,
            value: Payload::new(b"Synthetic example only"),
            result: ReadResult::Ok,
        })
        .unwrap();
    p.inspection.complete = true;
    for page in 0..page_count(&p) {
        p.page = page;
        save(directory, &format!("details-{page}-synthetic"), &p);
    }
    // Deliberately fabricated failure, not a diagnosis of any physical peer.
    p.event(Event::Failure {
        revision: p.request.revision,
        failure: InspectionFailure {
            stage: FailureStage::Characteristics,
            reason: FailureReason::Att(0x0E),
            range: Some((1, 9)),
            last_accepted: Some((2, 3)),
            elapsed_ms: Some(321),
        },
    });
    save(directory, "failure-synthetic", &p);
    p.input(Input::Click, 12_000).unwrap();
    p.phase = RadioPhase::Paused;
    p.event(Event::Failure {
        revision: p.request.revision,
        failure: InspectionFailure {
            stage: FailureStage::Mtu,
            reason: FailureReason::Disconnected,
            range: None,
            last_accepted: None,
            elapsed_ms: Some(215),
        },
    });
    p.event(Event::FailureContext {
        revision: p.request.revision,
        context: ConnectionFailureContext {
            local_error: Some(FailureReason::InvalidValue),
            disconnect_reason: Some(0x16),
        },
    });
    save(directory, "mtu-failure-synthetic", &p);
    let back = p.input(Input::Back, 12_000).unwrap();
    p.event(Event::State {
        revision: back.revision,
        phase: RadioPhase::Fault,
        error: ProbeError::Cleanup,
    });
    p.dropped_reports = 31;
    p.catalog.overflow_reports = 8;
    save(directory, "fault-synthetic", &p);
    p.catalog = Catalog::new();
    save(directory, "empty-fault-synthetic", &p);
    let pause = p.input(Input::Back, 12_000).unwrap();
    p.event(Event::State {
        revision: pause.revision,
        phase: RadioPhase::Paused,
        error: ProbeError::None,
    });
    save(directory, "empty-paused-synthetic", &p);
    p.scan_diagnostics.record(ScanFailure {
        kind: ScanErrorKind::InvalidSize,
        revision: 7,
        index: 1,
        total_reports: 3,
        report_bytes: 51,
        remaining_bytes: 10,
        declared_data_len: Some(31),
        at_ms: 11_000,
    });
    p.malformed_reports = p.scan_diagnostics.total;
    p.input(Input::Details, 12_000).unwrap();
    p.phase = RadioPhase::Paused;
    save(directory, "scan-errors-synthetic", &p);
}
