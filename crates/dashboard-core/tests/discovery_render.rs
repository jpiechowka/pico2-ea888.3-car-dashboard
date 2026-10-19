use dashboard_core::discovery::*;
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::*;
#[derive(Default)]
struct ScreenTarget {
    pixels: std::collections::BTreeMap<(i32, i32), Rgb565>,
}
impl OriginDimensions for ScreenTarget {
    fn size(&self) -> Size { Size::new(320, 240) }
}
impl DrawTarget for ScreenTarget {
    type Color = Rgb565;
    type Error = core::convert::Infallible;

    fn draw_iter<I: IntoIterator<Item = Pixel<Rgb565>>>(
        &mut self,
        pixels: I,
    ) -> Result<(), Self::Error> {
        for Pixel(p, c) in pixels {
            assert!(
                (0..320).contains(&p.x) && (0..240).contains(&p.y),
                "out of bounds {p:?}"
            );
            self.pixels.insert((p.x, p.y), c);
        }
        Ok(())
    }
}
fn populated() -> Probe {
    let mut p = Probe::new();
    for i in 0..48 {
        p.catalog.observe(Advertisement::new(
            Peer {
                address: [i; 6],
                random: true,
            },
            ReportKind::Connectable,
            -99,
            &[
                30, 9, b'A', b'A', b'A', b'A', b'A', b'A', b'A', b'A', b'A', b'A', b'A', b'A', b'A', b'A', b'A', b'A',
                b'A', b'A', b'A', b'A', b'A', b'A', b'A', b'A', b'A', b'A', b'A', b'A', b'A',
            ],
            0,
        ));
    }
    p
}
#[test]
fn every_phase_and_extreme_selection_stays_on_display() {
    let mut p = populated();
    for phase in [
        RadioPhase::Starting,
        RadioPhase::Scanning,
        RadioPhase::Paused,
        RadioPhase::Stopping,
        RadioPhase::Connecting,
        RadioPhase::Discovering,
        RadioPhase::Disconnecting,
        RadioPhase::Fault,
    ] {
        p.phase = phase;
        for selected in [0, 47, usize::MAX] {
            p.selected = selected;
            render(&mut ScreenTarget::default(), &p, u64::MAX).unwrap();
        }
    }
}
#[test]
fn detail_pages_cover_large_inspection_and_clamp() {
    let mut p = populated();
    p.input(Input::Click, 0).unwrap();
    for i in 0..64 {
        p.inspection
            .characteristics
            .push(Characteristic {
                service_start: 1,
                declaration: i * 2 + 1,
                handle: i * 2 + 2,
                properties: 0xFF,
                uuid: Uuid::from_wire(&[0xFF; 16]).unwrap(),
            })
            .unwrap();
    }
    for _ in 0..8 {
        p.inspection
            .values
            .push(InfoValue {
                service_start: 1,
                handle: 3,
                failure: None,
                elapsed_ms: 1,
                uuid: 0x2A29,
                value: Payload::new(&[0xFF; 64]),
                result: ReadResult::Ok,
            })
            .unwrap();
    }
    assert!(page_count(&p) > 20);
    for page in 0..page_count(&p) {
        p.page = page;
        render(&mut ScreenTarget::default(), &p, 0).unwrap();
    }
    p.page = usize::MAX;
    let mut last = ScreenTarget::default();
    render(&mut last, &p, 0).unwrap();
    p.page = page_count(&p) - 1;
    let mut expected = ScreenTarget::default();
    render(&mut expected, &p, 0).unwrap();
    assert_eq!(last.pixels, expected.pixels);
}
#[test]
fn spinner_is_elapsed_time_based() {
    let p = Probe::new();
    let mut a = ScreenTarget::default();
    let mut b = ScreenTarget::default();
    render(&mut a, &p, 0).unwrap();
    render(&mut b, &p, 200).unwrap();
    assert_ne!(a.pixels, b.pixels);
}

#[test]
fn telemetry_is_always_visible_and_does_not_overpaint_other_regions() {
    let mut p = populated();
    for screen in [Screen::List, Screen::Details] {
        p.screen = screen;
        for phase in [
            RadioPhase::Scanning,
            RadioPhase::Paused,
            RadioPhase::Connecting,
            RadioPhase::Fault,
        ] {
            p.phase = phase;
            p.diagnostics = ProbeDiagnostics::default();
            let mut before = ScreenTarget::default();
            render(&mut before, &p, 500).unwrap();
            p.diagnostics = ProbeDiagnostics {
                fps: Some(dashboard_core::display::telemetry::FrameRate {
                    current: 20.0,
                    average: 19.5,
                }),
                work: [Some(200), Some(1000)],
                ram: Some(MemoryUse {
                    used: 372_720,
                    total: 524_288,
                }),
                stacks: [
                    Some(MemoryUse {
                        used: 4097,
                        total: 151_568,
                    }),
                    Some(MemoryUse {
                        used: 2048,
                        total: 32768,
                    }),
                ],
            };
            let mut after = ScreenTarget::default();
            render(&mut after, &p, 500).unwrap();
            let changes: Vec<_> = before
                .pixels
                .iter()
                .filter(|(point, color)| after.pixels.get(point) != Some(color))
                .collect();
            assert!(!changes.is_empty(), "telemetry missing for {screen:?}/{phase:?}");
            assert!(
                changes
                    .iter()
                    .all(|((x, y), _)| (10..310).contains(x) && (25..47).contains(y))
            );
            assert!(changes.iter().any(|((_, y), _)| (25..35).contains(y)));
            assert!(changes.iter().any(|((_, y), _)| (37..47).contains(y)));
        }
    }
}

#[test]
fn failure_and_scan_pages_preserve_telemetry_and_footer_pixels() {
    let mut p = populated();
    p.input(Input::Click, 0).unwrap();
    p.phase = RadioPhase::Paused;
    let mut before = ScreenTarget::default();
    render(&mut before, &p, 0).unwrap();
    p.event(Event::Failure {
        revision: p.request.revision,
        failure: InspectionFailure {
            stage: FailureStage::Characteristics,
            reason: FailureReason::Att(255),
            range: Some((1, 65535)),
            last_accepted: Some((65534, 65535)),
            rejected: Some(RejectedDeclaration::from_wire(65535, &[0xFF; 253])),
            elapsed_ms: Some(u32::MAX),
        },
    });
    p.event(Event::FailureContext {
        revision: p.request.revision,
        context: ConnectionFailureContext {
            local_error: Some(FailureReason::UnexpectedResponse),
            disconnect_reason: Some(255),
        },
    });
    p.scan_diagnostics.record(ScanFailure {
        kind: ScanErrorKind::InvalidSize,
        revision: u32::MAX,
        index: 254,
        total_reports: 255,
        report_bytes: u16::MAX,
        remaining_bytes: u16::MAX,
        declared_data_len: Some(u8::MAX),
        at_ms: u64::MAX,
    });
    p.scan_diagnostics.total = u32::MAX;
    p.scan_diagnostics.invalid_size = u32::MAX;
    p.malformed_reports = u32::MAX;
    for page in 0..page_count(&p) {
        p.page = page;
        let mut after = ScreenTarget::default();
        render(&mut after, &p, 0).unwrap();
        let changes: Vec<_> = before
            .pixels
            .iter()
            .filter(|(point, color)| after.pixels.get(point) != Some(color))
            .collect();
        assert!(!changes.is_empty());
        assert!(
            changes
                .iter()
                .all(|((x, y), _)| (10..310).contains(x) && (61..198).contains(y)),
            "failure content escaped status/body or changed header/footer on page {page}"
        );
    }
}

#[test]
fn latched_scan_hint_changes_only_the_status_band_even_with_saturated_counts() {
    let mut p = populated();
    p.malformed_reports = u32::MAX;
    let mut before = ScreenTarget::default();
    render(&mut before, &p, 0).unwrap();
    for latch in [ErrorLatch::ScanOnly, ErrorLatch::Other] {
        p.error_latch = latch;
        let mut after = ScreenTarget::default();
        render(&mut after, &p, 0).unwrap();
        let changes: Vec<_> = before
            .pixels
            .iter()
            .filter(|(point, color)| after.pixels.get(point) != Some(color))
            .collect();
        assert!(!changes.is_empty());
        assert!(
            changes
                .iter()
                .all(|((x, y), _)| (10..310).contains(x) && (61..71).contains(y))
        );
    }
}

#[test]
fn saved_pages_and_slot_prompts_fit_and_preserve_the_same_diagnostics_band() {
    use capture_ui::{
        Action as CaptureAction,
        CaptureUi,
        Completion,
        Context,
        Input as CaptureInput,
        SaveAvailability,
        Slot,
    };
    let mut probe = populated();
    probe.input(Input::Click, 0).unwrap();
    probe
        .inspection
        .services
        .push(Service {
            start: 1,
            end: 9,
            uuid: Uuid::from_wire(&[0, 0x18]).unwrap(),
        })
        .unwrap();
    probe
        .inspection
        .characteristics
        .push(Characteristic {
            service_start: 1,
            declaration: 2,
            handle: 3,
            properties: 2,
            uuid: Uuid::from_wire(&[1, 0x2A]).unwrap(),
        })
        .unwrap();
    probe
        .inspection
        .descriptors
        .push(Descriptor {
            service_start: 1,
            characteristic_handle: 3,
            handle: 4,
            uuid: Uuid::from_wire(&[1, 0x29]).unwrap(),
        })
        .unwrap();
    probe
        .inspection
        .values
        .push(InfoValue {
            service_start: 1,
            handle: 3,
            uuid: 0x2A01,
            value: Payload::new(&[0xFF]),
            result: ReadResult::Ok,
            failure: None,
            elapsed_ms: u32::MAX,
        })
        .unwrap();
    let mut bytes = [0; 8192];
    let n = capture::encode(
        &probe,
        capture::BuildIdentity {
            source_digest: [0xAA; 32],
            cpu_hz: 280_000_000,
            spi_hz: 70_000_000,
            profile: capture::BuildProfile::Expert280,
            source_identity_available: true,
        },
        1,
        &mut bytes,
    )
    .unwrap();
    let mut decoder = capture::Decoder::new();
    let view = decoder.decode(&bytes[..n]).unwrap();
    let diagnostic = ProbeDiagnostics::default();
    let mut first = ScreenTarget::default();
    render_saved(&mut first, &view, 0, 0, diagnostic).unwrap();
    for page in 0..saved_page_count(&view) {
        let mut target = ScreenTarget::default();
        render_saved(&mut target, &view, 0, page, diagnostic).unwrap();
        for (&point, &pixel) in &first.pixels {
            if (25..47).contains(&point.1) || point.1 >= 204 {
                assert_eq!(target.pixels[&point], pixel);
            }
        }
    }
    let mut clamped = ScreenTarget::default();
    let mut last = ScreenTarget::default();
    render_saved(&mut clamped, &view, 255, usize::MAX, diagnostic).unwrap();
    render_saved(&mut last, &view, 255, saved_page_count(&view) - 1, diagnostic).unwrap();
    assert_eq!(clamped.pixels, last.pixels);
    let mut ui = CaptureUi::new();
    let context = Context {
        save: SaveAvailability::Ready,
        revision: 1,
    };
    ui.input(CaptureInput::XDown, 0, context);
    let CaptureAction::Refresh { id } = ui.tick(2000).unwrap() else {
        panic!()
    };
    render_slots(&mut ScreenTarget::default(), &ui, diagnostic).unwrap();
    ui.complete(
        id,
        Completion::Listed([
            Slot::SavedBlocked { generation: u64::MAX },
            Slot::Empty,
            Slot::Unsupported,
            Slot::Corrupt,
        ]),
    );
    ui.input(CaptureInput::XUp, 2001, context);
    for selected in 0..4 {
        render_slots(&mut ScreenTarget::default(), &ui, diagnostic).unwrap();
        ui.input(CaptureInput::XDown, 2100 + selected * 10, context);
        ui.input(CaptureInput::XUp, 2101 + selected * 10, context);
        render_slots(&mut ScreenTarget::default(), &ui, diagnostic).unwrap();
        if let Some(id) = ui.pending() {
            ui.complete(id, Completion::Failed(capture_ui::UiError::Storage));
        }
        ui.input(CaptureInput::Back, 2102 + selected * 10, context);
        ui.input(CaptureInput::Turn(1), 2103 + selected * 10, context);
    }
}
