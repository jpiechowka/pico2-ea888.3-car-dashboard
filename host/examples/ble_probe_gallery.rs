//! Synthetic BLE probe evidence: `cargo run --locked --example ble_probe_gallery -- <output-dir>`.
//! Converts RGB565 framebuffer bytes to PPM; no radio or hardware is accessed.
use std::io::Write;
use std::path::Path;

use dashboard_core::discovery::*;
use dashboard_core::display::St7789Renderer;
use dashboard_core::display::telemetry::FrameRate;
use dashboard_host_validation::AlignedFrame;
/// Renders live `p` at synthetic 12400ms and writes stem `name` with .ppm suffix under `dir`; rendering/I/O failures
/// panic.
fn save(
    dir: &Path,
    name: &str,
    p: &Probe,
) {
    let mut frame = AlignedFrame::boxed();
    render(&mut St7789Renderer::new(&mut frame.0), p, 12_400).unwrap();
    save_frame(dir, name, &frame);
}
/// Converts all landscape RGB565 `frame` pixels to RGB888 P6 under `dir` with stem `name` and .ppm suffix.
/// Existing files are replaced; create/write failures panic and may leave partial output.
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
/// Renders slot `ui` with supplied synthetic `diagnostics`, writing stem `name` plus .ppm beneath `dir`; failures
/// panic.
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
/// Writes synthetic expiry/pending/released timeout scenes under `dir` with current-style `diagnostics`.
/// Exercises real UI transitions without I/O; panics on unexpected actions or rendering/file errors.
fn save_timeout_scenes(
    dir: &Path,
    diagnostics: ProbeDiagnostics,
) {
    use capture_ui::{Action, CaptureUi, Completion, Context, Input, SaveAvailability, Slot};
    let context = Context {
        save: SaveAvailability::Ready,
        revision: 1,
    };
    let identity = names::SavedIdentity {
        peer: Peer {
            address: [0xC1; 6],
            random: true,
        },
        name: Payload::new(b"SYNTHETIC SAVED DEVICE"),
    };
    for (slot, name) in [
        (Slot::Saved { generation: 1 }, "confirmation-expired-synthetic"),
        (Slot::SavedBlocked { generation: 1 }, "reset-expired-synthetic"),
    ] {
        let mut ui = CaptureUi::new();
        let Some(Action::Refresh { id }) = ui.input(Input::XHold, 0, context) else {
            panic!()
        };
        ui.complete(
            id,
            Completion::Listed {
                slots: [slot; 4],
                identities: [Some(identity); 4],
            },
        );
        ui.input(Input::XShort, 10, context);
        ui.tick(10_010);
        save_slots(dir, name, &ui, diagnostics);
        if matches!(slot, Slot::Saved { .. }) {
            ui.input(Input::Back, 10_011, context);
            ui.input(Input::XShort, 10_012, context);
            let Some(Action::Save { id, .. }) = ui.input(Input::Click, 10_013, context) else {
                panic!()
            };
            ui.tick(40_013);
            save_slots(dir, "slot-timeout-pending-synthetic", &ui, diagnostics);
            ui.complete(
                id,
                Completion::Saved {
                    generation: 2,
                    identity,
                },
            );
            save_slots(dir, "slot-timeout-finished-synthetic", &ui, diagnostics);
        }
    }
}

/// Applies synthetic identity `update` through `p`'s real revision-qualified event path.
/// Panics if the fixture violates admission; no transport is invoked.
fn identity_update(
    p: &mut Probe,
    update: identity::Update,
) {
    let malformed = p.inspection.malformed;
    p.event(Event::Item {
        revision: p.request.revision,
        item: InspectionItem::Identity(update),
    });
    assert_eq!(p.inspection.malformed, malformed);
}

/// Returns a synthetic explicitly inspected peer with fresh readable vendor declarations.
/// `diagnostics` is fabricated display telemetry; all four inventory stages are complete.
fn identity_probe(diagnostics: ProbeDiagnostics) -> Probe {
    let mut p = Probe::new();
    p.identity_mode = true;
    p.diagnostics = diagnostics;
    p.encoder_connected = true;
    p.radio_bus_hz = Some(2_500_000);
    p.catalog.observe(Advertisement::new(
        Peer {
            address: [0xC0, 0, 0, 0, 0, 0x99],
            random: true,
        },
        ReportKind::Connectable,
        -55,
        b"\x02\x01\x06\x0e\x09SYNTHETIC NGM",
        10_000,
    ));
    p.input(Input::Click, 12_000).unwrap();
    p.phase = RadioPhase::Paused;
    p.inspection.accept(InspectionItem::Service(Service {
        start: 1,
        end: 30,
        uuid: Uuid::from_wire(&[0xF1, 0xFF]).unwrap(),
    }));
    for (index, uuid) in identity::VENDOR_UUIDS.into_iter().enumerate() {
        p.inspection.accept(InspectionItem::Characteristic(Characteristic {
            service_start: 1,
            declaration: 2 + index as u16 * 3,
            handle: 3 + index as u16 * 3,
            properties: 2,
            uuid: Uuid::from_wire(&uuid.to_le_bytes()).unwrap(),
        }));
    }
    p.inspection.stages = [true; 4];
    p.inspection.link = Some(LinkInfo {
        connected_at_ms: 12_005,
        connect_elapsed_ms: 5,
        interval_us: 30_000,
        latency: 0,
        supervision_timeout_us: 4_000_000,
        att_mtu: Some(247),
    });
    identity_update(&mut p, identity::Update::Start { at_ms: 12_000 });
    for (index, uuid) in identity::VENDOR_UUIDS.into_iter().enumerate() {
        identity_update(
            &mut p,
            identity::Update::Vendor(InfoValue {
                service_start: 1,
                handle: 3 + index as u16 * 3,
                uuid,
                result: ReadResult::Ok,
                failure: None,
                elapsed_ms: 5,
                value: Payload::new(b"Synthetic raw value"),
            }),
        );
    }
    p
}

/// Adds offered channel values to `p`; `established` supplies actual directional limits.
fn identity_channel(
    p: &mut Probe,
    established: bool,
) {
    identity_update(
        p,
        identity::Update::Channel(identity::ChannelParameters {
            offered_mtu: 245,
            offered_mps: 247,
            offered_credits: 1,
            local_mtu: established.then_some(245),
            local_mps: established.then_some(247),
            peer_mtu: established.then_some(512),
            peer_mps: established.then_some(247),
            local_cid: None,
            peer_cid: None,
            local_credits: None,
            peer_credits: None,
        }),
    );
}

/// Publishes complete request submission and raw `reply` in admitted chunks to synthetic `p`.
fn identity_reply(
    p: &mut Probe,
    reply: &[u8],
) {
    identity_channel(p, true);
    identity_update(p, identity::Update::RequestSubmitted { at_ms: 12_040 });
    identity_update(
        p,
        identity::Update::Sdu {
            length: reply.len() as u16,
        },
    );
    for (index, chunk) in reply.chunks(64).enumerate() {
        identity_update(
            p,
            identity::Update::Chunk {
                offset: (index * 64) as u16,
                bytes: Payload::new(chunk),
            },
        );
    }
}

/// Completes synthetic `p` cleanup, keeping the actual identity outcome separate from inventory.
fn identity_end(p: &mut Probe) {
    identity_update(
        p,
        identity::Update::End {
            at_ms: 12_100,
            cleanup: dashboard_core::ble::canyonero::CleanupOutcome::Clean,
        },
    );
    let success = p.inspection.identity.as_ref().unwrap().successful();
    p.inspection.ended_at_ms = Some(12_100);
    p.inspection.cleanup_complete = true;
    p.inspection.disconnect_reason = Some(0x16);
    p.inspection.complete = success;
    p.failure_context.disconnect_reason = Some(0x16);
    if !success {
        p.error = ProbeError::Transport;
        p.fault_latch = FaultLatch::single(Fault::Probe);
    }
}

/// Writes every live and validated saved page for synthetic `p` under scene `name` in `dir`.
/// `at_ms` is the boot-relative capture time. Panics on invalid fixtures or rendering/I/O errors.
fn identity_pages(
    dir: &Path,
    name: &str,
    p: &mut Probe,
    at_ms: u64,
) {
    for page in 0..page_count(p) {
        p.page = page;
        save(dir, &format!("ngm-{name}-live-{page:02}-synthetic"), p);
    }
    let mut bytes = [0; 8192];
    let size = capture::encode(
        p,
        capture::BuildIdentity {
            source_digest: [0x55; 32],
            cpu_hz: 150_000_000,
            spi_hz: 37_500_000,
            profile: capture::BuildProfile::Stock,
            source_identity_available: true,
        },
        at_ms,
        &mut bytes,
    )
    .unwrap();
    let decoded = capture::decode(&bytes[..size]).unwrap();
    let view = decoded.view();
    for page in 0..saved_page_count(&view) {
        let mut frame = AlignedFrame::boxed();
        render_saved(&mut St7789Renderer::new(&mut frame.0), &view, 0, page, p.diagnostics).unwrap();
        save_frame(dir, &format!("ngm-{name}-saved-{page:02}-synthetic"), &frame);
    }
}

/// Adds complete, refused, omitted-prefix, maximal-field/counter and frozen legacy NGM QA scenes.
/// Writes pages to `dir` using the supplied `diagnostics` for their diagnostic displays.
/// All values are synthetic; only production model/capture/render APIs create the pages.
fn save_identity_scenes(
    dir: &Path,
    diagnostics: ProbeDiagnostics,
) {
    use dashboard_core::ble::canyonero::{Fault as IdentityFault, Reason, Stage, TransportDomain, TransportFault};
    let mut p = identity_probe(diagnostics);
    let body = b"SYNTHETIC\nMODEL-TEST\nHW-TEST\nSERIAL-TEST\nFW";
    let mut raw = vec![0x1F, 0x91];
    raw.extend_from_slice(&(body.len() as u16).to_be_bytes());
    raw.extend_from_slice(body);
    identity_reply(&mut p, &raw);
    identity_end(&mut p);
    assert!(p.inspection.identity.as_ref().unwrap().successful());
    identity_pages(dir, "success", &mut p, 12_200);

    let mut p = identity_probe(diagnostics);
    identity_channel(&mut p, false);
    identity_update(
        &mut p,
        identity::Update::Stage {
            stage: Stage::Closing,
            at_ms: 12_050,
            fault: Some(IdentityFault {
                stage: Stage::OpeningCoc,
                at_ms: 12_050,
                reason: Reason::Transport(TransportFault {
                    domain: TransportDomain::L2cap,
                    code: Some(0x0002),
                    partial_send: false,
                }),
            }),
        },
    );
    identity_end(&mut p);
    identity_pages(dir, "refused", &mut p, 12_200);

    let mut p = identity_probe(diagnostics);
    identity_reply(&mut p, b"\x1f\x91\x00\x09v\nm\nh\ns\nf");
    identity_update(&mut p, identity::Update::Sdu { length: 1 });
    identity_update(
        &mut p,
        identity::Update::Stage {
            stage: Stage::Closing,
            at_ms: 12_060,
            fault: Some(IdentityFault {
                stage: Stage::AwaitingInfo,
                at_ms: 12_060,
                reason: Reason::Protocol(dashboard_core::ble::canyonero::DecodeError::TrailingData),
            }),
        },
    );
    identity_end(&mut p);
    let v = p.inspection.identity.as_mut().unwrap().vendor_values[1]
        .as_mut()
        .unwrap();
    v.value = Payload::new(&[0xFF; 64]);
    v.value.truncated = true;
    identity_pages(dir, "truncated", &mut p, 12_200);

    let mut p = identity_probe(diagnostics);
    let mut raw = vec![0x1F, 0x91, 0, 241];
    for (i, (byte, len)) in [(b'V', 64), (b'M', 64), (b'H', 64), (b'S', 40), (b'F', 5)]
        .into_iter()
        .enumerate()
    {
        if i > 0 {
            raw.push(b'\n');
        }
        raw.extend(std::iter::repeat_n(byte, len));
    }
    assert_eq!(raw.len(), 245);
    identity_reply(&mut p, &raw);
    identity_end(&mut p);
    assert!(p.inspection.identity.as_ref().unwrap().successful());
    identity_pages(dir, "max-fields", &mut p, 12_200);

    // Saturated counters/times exercise display widths, never imply a possible-duration device run.
    let e = p.inspection.identity.as_mut().unwrap();
    e.observed_bytes = u64::MAX;
    e.sdu_count = u32::MAX;
    e.max_sdu_bytes = u16::MAX;
    e.updated_at_ms = u64::MAX;
    e.ended_at_ms = Some(u64::MAX);
    e.fault = Some(IdentityFault {
        stage: Stage::AwaitingInfo,
        at_ms: u64::MAX,
        reason: Reason::Transport(TransportFault {
            domain: TransportDomain::Local,
            code: Some(u16::MAX),
            partial_send: true,
        }),
    });
    for v in e.vendor_values.iter_mut().flatten() {
        v.elapsed_ms = u32::MAX;
    }
    p.inspection.ended_at_ms = Some(u64::MAX);
    p.inspection.complete = false;
    p.error = ProbeError::Transport;
    p.fault_latch = FaultLatch::single(Fault::Capacity);
    identity_pages(dir, "max-counters", &mut p, u64::MAX);

    for (version, text) in [
        (1, include_str!("../tests/fixtures/ble-capture-v1-failure.hex")),
        (4, include_str!("../tests/fixtures/ble-capture-v4-legacy.hex")),
    ] {
        let hex: String = text.split_whitespace().collect();
        let bytes: Vec<u8> = (0..hex.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
            .collect();
        let decoded = capture::decode(&bytes).unwrap();
        let view = decoded.view();
        assert!(view.metadata().identity_mode.is_none());
        assert!(view.metadata().identity.is_none());
        for page in 0..saved_page_count(&view) {
            let mut frame = AlignedFrame::boxed();
            render_saved(&mut St7789Renderer::new(&mut frame.0), &view, 0, page, diagnostics).unwrap();
            save_frame(dir, &format!("ngm-legacy-v{version}-saved-{page:02}-synthetic"), &frame);
        }
    }
}
/// Emits synthetic scan/inspection/saved-slot/error/reset-evidence PPMs into the first CLI argument's directory.
/// Missing arguments, fixture invariant failures and I/O errors panic; trailing arguments are ignored and existing
/// images replaced. All radio, timing, memory and peer observations are fabricated display fixtures; no device is
/// contacted.
fn main() {
    let directory = std::env::args_os().nth(1).expect("output directory");
    let directory = Path::new(&directory);
    std::fs::create_dir_all(directory).unwrap();
    let mut p = Probe::new();
    p.radio_bus_hz = Some(2_500_000);
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
    p.fault_latch = FaultLatch::single(Fault::Scan);
    p.malformed_reports = 5;
    save(directory, "scan-latched-synthetic", &p);
    p.fault_latch = FaultLatch::single(Fault::Probe);
    save(directory, "error-latched-synthetic", &p);
    p.fault_latch = FaultLatch::from_parts(Fault::Encoder.bit() | Fault::Scan.bit(), Fault::Encoder as u8).unwrap();
    p.screen = Screen::Details;
    p.phase = RadioPhase::Paused;
    p.page = page_count(&p) - 1;
    save(directory, "fault-ledger-synthetic", &p);
    p.screen = Screen::List;
    p.phase = RadioPhase::Scanning;
    p.page = 0;
    p.fault_latch = FaultLatch::EMPTY;
    p.malformed_reports = 0;
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
    // Historical firmware could mark inventory complete despite an abnormal
    // terminal reason. Preserve that evidence and show its separate warning.
    p.inspection.disconnect_reason = Some(0x22);
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
    let view = decoder.decode(&bytes[..size]).unwrap();
    let mut frame = AlignedFrame::boxed();
    render_saved(&mut St7789Renderer::new(&mut frame.0), &view, 0, 6, p.diagnostics).unwrap();
    save_frame(directory, "saved-abnormal-termination-synthetic", &frame);
    p.event(Event::FailureContext {
        revision: p.request.revision,
        context: ConnectionFailureContext {
            local_error: None,
            disconnect_reason: Some(0x22),
        },
    });
    p.fault_latch = FaultLatch::single(Fault::Probe);
    save(directory, "termination-failure-synthetic", &p);
    p.inspection.disconnect_reason = Some(0x16);
    p.inspection.complete = true;
    p.failure = None;
    p.failure_context = ConnectionFailureContext::default();
    p.error = ProbeError::None;
    p.fault_latch = FaultLatch::EMPTY;
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
        let saved_identity = names::SavedIdentity {
            peer: Peer {
                address: [0xC0, 0x12, 0x34, 0x56, 0x78, 0x9A],
                random: true,
            },
            name: Payload::new(b"SYNTHETIC NEEWER-TL21C LONG NAME"),
        };
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
            Completion::Listed {
                slots: [
                    Slot::Saved { generation: 1 },
                    Slot::Empty,
                    Slot::Corrupt,
                    Slot::Unsupported,
                ],
                identities: [Some(saved_identity), None, None, None],
            },
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
        ui.complete(
            id,
            Completion::Saved {
                generation: 1,
                identity: names::SavedIdentity {
                    peer: Peer {
                        address: [0xDE, 0xAD, 0xBE, 0xEF, 0x12, 0x34],
                        random: false,
                    },
                    name: Payload::new(&[]),
                },
            },
        );
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
            Completion::Listed {
                slots: [
                    Slot::SavedBlocked { generation: 1 },
                    Slot::Empty,
                    Slot::Empty,
                    Slot::Empty,
                ],
                identities: [Some(saved_identity), None, None, None],
            },
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
    save_timeout_scenes(directory, p.diagnostics);
    save_identity_scenes(directory, p.diagnostics);
    // Deliberately fabricated failure, not a diagnosis of any physical peer.
    p.event(Event::Failure {
        revision: p.request.revision,
        failure: InspectionFailure {
            stage: FailureStage::Characteristics,
            reason: FailureReason::Att(0x0E),
            range: Some((1, 9)),
            last_accepted: Some((2, 3)),
            rejected: None,
            elapsed_ms: Some(321),
        },
    });
    save(directory, "failure-synthetic", &p);
    p.failure = Some(InspectionFailure {
        stage: FailureStage::Characteristics,
        reason: FailureReason::DeclarationOrder,
        range: Some((1, 6)),
        last_accepted: Some((4, 5)),
        rejected: Some(RejectedDeclaration::from_wire(6, &[2, 7, 0, 0x29, 0x2A])),
        elapsed_ms: Some(119),
    });
    p.fault_latch = FaultLatch::single(Fault::Probe);
    p.page = 0;
    save(directory, "rejected-declaration-synthetic", &p);
    p.page = 1;
    save(directory, "rejected-context-synthetic", &p);
    p.input(Input::Click, 12_000).unwrap();
    p.phase = RadioPhase::Paused;
    p.event(Event::Failure {
        revision: p.request.revision,
        failure: InspectionFailure {
            stage: FailureStage::Mtu,
            reason: FailureReason::Disconnected,
            range: None,
            last_accepted: None,
            rejected: None,
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
    p.page = page_count(&p) - 1;
    save(directory, "scan-errors-lengths-synthetic", &p);
    p.boot = Some(dashboard_core::health::reset::BootEvidence {
        raw_reason: 1,
        boot_type: Some(0),
        previous: Some(dashboard_core::health::reset::ResetRecord {
            stage: dashboard_core::health::reset::ResetStage::Failure(
                dashboard_core::health::reset::RecoveryCause::RadioStartup,
            ),
            elapsed_ms: 15_000,
        }),
    });
    let pages = page_count(&p);
    for (index, page) in (0..pages).enumerate() {
        p.page = page;
        save(directory, &format!("reset-evidence-{index}-synthetic"), &p);
    }
}
