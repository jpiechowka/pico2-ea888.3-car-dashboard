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
    p.screen = Screen::Details;
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
