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
    render(&mut St7789Renderer::new(&mut frame.0), p, 12_400).unwrap();
    save_frame(dir, name, &frame);
}
fn save_frame(
    dir: &Path,
    name: &str,
    frame: &AlignedFrame,
) {
    let mut f = std::fs::File::create(dir.join(format!("{name}.ppm"))).unwrap();
    f.write_all(b"P6\n320 240\n255\n").unwrap();
    for px in frame.0.as_chunks::<2>().0 {
        let rgb = u16::from_be_bytes(*px);
        let (r, g, b) = ((rgb >> 11) as u8, ((rgb >> 5) & 63) as u8, (rgb & 31) as u8);
        f.write_all(&[(r << 3) | (r >> 2), (g << 2) | (g >> 4), (b << 3) | (b >> 2)])
            .unwrap();
    }
}
fn save_slots(
    dir: &Path,
    name: &str,
    ui: &capture_ui::CaptureUi,
    diagnostics: ProbeDiagnostics,
) {
    let mut frame = AlignedFrame::boxed();
    render_slots(&mut St7789Renderer::new(&mut frame.0), ui, diagnostics).unwrap();
    save_frame(dir, name, &frame);
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
    p.catalog.observe(Advertisement::new(
        p.selected_device().unwrap().peer,
        ReportKind::ScanResponse,
        -47,
        &[5, 0xFF, 0x34, 0x12, 0xDE, 0xAD, 5, 0x16, 0xF0, 0xFF, 1, 2],
        11_000,
    ));
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
            service_start: 1,
            handle: 3,
            failure: None,
            elapsed_ms: 1,
            uuid: 0x2A29,
            value: Payload::new(b"Synthetic example only"),
            result: ReadResult::Ok,
        })
        .unwrap();
    p.inspection.complete = true;
    p.inspection.stages = [true; 4];
    p.inspection.cleanup_complete = true;
    p.inspection.ended_at_ms = Some(12_030);
    p.inspection.disconnect_reason = Some(0x16);
    p.inspection.link = Some(LinkInfo {
        connected_at_ms: 12_005,
        connect_elapsed_ms: 5,
        interval_us: 30_000,
        latency: 0,
        supervision_timeout_us: 4_000_000,
        att_mtu: Some(247),
    });
    p.inspection
        .descriptors
        .push(Descriptor {
            service_start: 1,
            characteristic_handle: 3,
            handle: 4,
            uuid: Uuid::from_wire(&[1, 0x29]).unwrap(),
        })
        .unwrap();
    // Include a binary, protected standard field and a second service's hash.
    p.inspection.accept(InspectionItem::Characteristic(Characteristic {
        service_start: 1,
        declaration: 5,
        handle: 6,
        properties: 2,
        uuid: Uuid::from_wire(&[0x23, 0x2A]).unwrap(),
    }));
    p.inspection.accept(InspectionItem::Value(InfoValue {
        service_start: 1,
        handle: 6,
        uuid: 0x2A23,
        value: Payload::empty(),
        result: ReadResult::Protected,
        failure: Some(FailureReason::Att(5)),
        elapsed_ms: 14,
    }));
    p.inspection.accept(InspectionItem::Service(Service {
        start: 12,
        end: 18,
        uuid: Uuid::from_wire(&[1, 0x18]).unwrap(),
    }));
    p.inspection.accept(InspectionItem::Characteristic(Characteristic {
        service_start: 12,
        declaration: 13,
        handle: 14,
        properties: 2,
        uuid: Uuid::from_wire(&[0x2A, 0x2B]).unwrap(),
    }));
    p.inspection.accept(InspectionItem::Value(InfoValue {
        service_start: 12,
        handle: 14,
        uuid: 0x2B2A,
        value: Payload::new(&[0x10, 0x32, 0x54, 0x76, 0x98, 0xBA, 0xDC, 0xFE, 1, 2, 3, 4, 5, 6, 7, 8]),
        result: ReadResult::Ok,
        failure: None,
        elapsed_ms: 2,
    }));
    for page in 0..page_count(&p) {
        p.page = page;
        save(directory, &format!("details-{page}-synthetic"), &p);
    }
    let mut bytes = [0; 8192];
    let size = capture::encode(
        &p,
        capture::BuildIdentity {
            source_digest: [0xA5; 32],
            cpu_hz: 280_000_000,
            spi_hz: 70_000_000,
            profile: capture::BuildProfile::Expert280,
            source_identity_available: true,
        },
        12_040,
        &mut bytes,
    )
    .unwrap();
    let mut decoder = capture::Decoder::new();
    let view = decoder.decode(&bytes[..size]).unwrap();
    for page in 0..saved_page_count(&view) {
        let mut frame = AlignedFrame::boxed();
        render_saved(&mut St7789Renderer::new(&mut frame.0), &view, 0, page, p.diagnostics).unwrap();
        save_frame(directory, &format!("saved-{page}-synthetic"), &frame);
    }
    {
        use capture_ui::{
            Action as CaptureAction,
            CaptureUi,
            Completion,
            Context,
            Input as CaptureInput,
            SaveAvailability,
            Slot,
            UiError,
        };
        let mut ui = CaptureUi::new();
        let context = Context {
            save: SaveAvailability::Ready,
            revision: p.request.revision,
        };
        let CaptureAction::Refresh { id } = ui.input(CaptureInput::XHold, 2000, context).unwrap() else {
            panic!()
        };
        save_slots(directory, "slots-loading-synthetic", &ui, p.diagnostics);
        ui.complete(
            id,
            Completion::Listed([
                Slot::Saved { generation: 1 },
                Slot::Empty,
                Slot::Corrupt,
                Slot::Unsupported,
            ]),
        );
        for (name, input, at) in [
            ("slots-synthetic", CaptureInput::Turn(0), 2010),
            ("overwrite-synthetic", CaptureInput::XShort, 2030),
            ("slots-back-synthetic", CaptureInput::Back, 2040),
            ("slots-damaged-synthetic", CaptureInput::Turn(2), 2050),
            ("erase-confirm-synthetic", CaptureInput::XShort, 2070),
        ] {
            ui.input(input, at, context);
            save_slots(directory, name, &ui, p.diagnostics);
        }
        let CaptureAction::Reset { id, .. } = ui.input(CaptureInput::Click, 2080, context).unwrap() else {
            panic!()
        };
        save_slots(directory, "resetting-synthetic", &ui, p.diagnostics);
        ui.complete(id, Completion::Reset);
        ui.input(CaptureInput::Turn(-1), 2090, context);
        let CaptureAction::Save {
            id, overwrite: false, ..
        } = ui.input(CaptureInput::XShort, 2100, context).unwrap()
        else {
            panic!()
        };
        save_slots(directory, "saving-synthetic", &ui, p.diagnostics);
        ui.complete(id, Completion::Saved { generation: 1 });
        save_slots(directory, "saved-slot-synthetic", &ui, p.diagnostics);
        let CaptureAction::Load { id, .. } = ui.input(CaptureInput::Click, 2110, context).unwrap() else {
            panic!()
        };
        save_slots(directory, "capture-loading-synthetic", &ui, p.diagnostics);
        ui.complete(id, Completion::Failed(UiError::Storage));
        save_slots(directory, "storage-error-synthetic", &ui, p.diagnostics);
        ui.input(CaptureInput::Back, 2120, context);
        let CaptureAction::Refresh { id } = ui.input(CaptureInput::XHold, 3000, context).unwrap() else {
            panic!()
        };
        ui.complete(
            id,
            Completion::Listed([
                Slot::SavedBlocked { generation: 1 },
                Slot::Empty,
                Slot::Empty,
                Slot::Empty,
            ]),
        );
        ui.input(CaptureInput::Turn(-1), 3001, context);
        for (name, input, at) in [
            ("blocked-save-synthetic", CaptureInput::Turn(0), 3010),
            ("blocked-erase-confirm-synthetic", CaptureInput::XShort, 3020),
        ] {
            ui.input(input, at, context);
            save_slots(directory, name, &ui, p.diagnostics);
        }
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
