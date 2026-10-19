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
