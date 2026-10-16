use dashboard_host_validation::display::damage::{DamagePlan, PanelHistory, Region};
use dashboard_host_validation::display::pipeline::{BufferId, FrameToken};
use dashboard_host_validation::display::{FrameBuffer, HEIGHT, WIDTH};

fn apply(
    panel: &mut FrameBuffer,
    source: &FrameBuffer,
    plan: &DamagePlan,
) {
    // Emulate CASET/RASET/RAMWR, consuming the same stride iterator as the driver.
    for region in plan.regions() {
        let [x0, x1] = region.columns().map(u16::from_be_bytes);
        let [y0, y1] = region.rows().map(u16::from_be_bytes);
        let bytes: Vec<u8> = region
            .payload_ranges()
            .flat_map(|r| source[r].iter().copied())
            .collect();
        assert_eq!(bytes.len(), usize::from(x1 - x0 + 1) * usize::from(y1 - y0 + 1) * 2);
        let mut pixels = bytes.as_chunks::<2>().0.iter();
        for y in y0..=y1 {
            for x in x0..=x1 {
                let at = (usize::from(y) * usize::from(plan.geometry().width()) + usize::from(x)) * 2;
                panel[at..at + 2].copy_from_slice(pixels.next().unwrap());
            }
        }
        assert!(pixels.next().is_none());
    }
}

#[test]
fn valid_regions_use_inclusive_panel_addresses_and_never_flatten_strided_rows() {
    for invalid in [
        (0, 0, 0, 1),
        (0, 0, 1, 0),
        (319, 239, 2, 1),
        (320, 0, 1, 1),
        (u16::MAX, 1, u16::MAX, 1),
    ] {
        assert!(Region::new(invalid.0, invalid.1, invalid.2, invalid.3).is_none());
    }
    let region = Region::new(319, 237, 1, 3).unwrap();
    assert_eq!(region.columns(), [319u16.to_be_bytes(); 2]);
    assert_eq!(region.rows(), [237u16.to_be_bytes(), 239u16.to_be_bytes()]);
    assert_eq!(
        region.payload_ranges().collect::<Vec<_>>(),
        vec![152318..152320, 152958..152960, 153598..153600]
    );
    assert_eq!(Region::full().payload_ranges().collect::<Vec<_>>(), vec![0..153600]);
}

#[test]
fn stationary_panel_sends_nothing_and_dense_changes_fall_back_to_full() {
    let a = Box::new(FrameBuffer::new());
    let mut b = Box::new(FrameBuffer::new());
    let plan = DamagePlan::between(&a, &b);
    assert!(plan.is_empty());
    assert_eq!(plan.bytes(), 0);
    b.fill(0x55);
    let plan = DamagePlan::between(&a, &b);
    assert!(plan.is_full());
    assert_eq!(plan.bytes(), 153600);
    assert_eq!(plan.transfer_count(), 1);
}

#[test]
fn thin_vertical_stripes_cannot_create_thousands_of_dma_starts() {
    let previous = Box::<FrameBuffer>::default();
    let mut current = Box::<FrameBuffer>::default();
    for y in 0..HEIGHT {
        for x in (0..WIDTH).step_by(32) {
            current[(y * WIDTH + x) * 2] = 0xFF;
        }
    }
    let plan = DamagePlan::between(&previous, &current);
    assert!(plan.is_full());
    assert_eq!(plan.transfer_count(), 1);
}

#[test]
fn every_corner_and_rgb565_byte_produces_exact_panel_content() {
    let mut panel = Box::new(FrameBuffer::new());
    let mut next = Box::new(FrameBuffer::new());
    for (x, y) in [(0, 0), (319, 0), (0, 239), (319, 239), (17, 29), (254, 128)] {
        let at = (y * WIDTH + x) * 2;
        next[at] = 0xA5;
        next[at + 1] = 0x5A;
        let plan = DamagePlan::between(&panel, &next);
        assert!(plan.bytes() < 153600);
        apply(&mut panel, &next, &plan);
        assert!(panel == next);
    }
}

#[test]
fn randomized_damage_union_and_window_budget_never_lose_a_pixel() {
    let mut panel = Box::new(FrameBuffer::new());
    let mut next = Box::new(FrameBuffer::new());
    let mut seed = 0x1928_2350u32;
    for frame in 0..250 {
        for _ in 0..(frame % 71) {
            seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
            let at = seed as usize % (WIDTH * HEIGHT) * 2;
            next[at] = (seed >> 16) as u8;
            next[at + 1] = (seed >> 24) as u8;
        }
        let plan = DamagePlan::between(&panel, &next);
        assert!(plan.regions().len() <= 16);
        assert!(plan.bytes() <= 153600);
        apply(&mut panel, &next, &plan);
        assert!(panel == next, "frame {frame}");
    }
}

#[test]
fn first_and_last_bytes_of_every_tile_band_reconstruct_exact_pixels() {
    let previous = Box::<FrameBuffer>::default();
    let mut next = Box::<FrameBuffer>::default();
    let mut panel = Box::<FrameBuffer>::default();
    for band_y in (0..HEIGHT).step_by(16) {
        for (x, y) in [(0, band_y), (319, band_y), (0, band_y + 15), (319, band_y + 15)] {
            for byte in 0..2 {
                let at = (y * WIDTH + x) * 2 + byte;
                next[at] = 0x80;
                let plan = DamagePlan::between(&previous, &next);
                assert_eq!(
                    plan.regions(),
                    &[Region::new((x / 16 * 16) as u16, band_y as u16, 16, 16).unwrap()]
                );
                apply(&mut panel, &next, &plan);
                assert!(panel == next, "pixel ({x}, {y}), byte {byte}");
                next[at] = 0;
                panel[at] = 0;
            }
        }
    }
}

fn token(
    buffer: BufferId,
    sequence: u64,
) -> FrameToken {
    FrameToken { buffer, sequence }
}

#[test]
fn partial_base_must_be_the_exact_completed_and_unmodified_generation() {
    let (a1, b2, a3) = (token(BufferId::A, 1), token(BufferId::B, 2), token(BufferId::A, 3));
    let mut history = PanelHistory::new();
    assert_eq!(history.reference(), None);
    history.begin_render(a1).unwrap();
    assert!(history.queue(a1, Some(b2)).is_err());
    history.queue(a1, None).unwrap();
    assert!(history.complete(b2).is_err());
    assert!(history.begin_render(a3).is_err()); // A is still DMA-owned.
    history.complete(a1).unwrap();
    assert_eq!(history.reference(), Some(a1));
    assert!(history.complete(a1).is_err());
    history.begin_render(b2).unwrap();
    assert!(history.queue(b2, Some(a3)).is_err());
    history.queue(b2, Some(a1)).unwrap();
    history.begin_render(a3).unwrap(); // B is in flight, A may be rendered.
    assert_eq!(history.reference(), None);
    assert!(history.complete(a1).is_err());
    history.complete(b2).unwrap();
    assert_eq!(history.reference(), Some(b2));
    history.queue(a3, Some(b2)).unwrap();
    history.complete(a3).unwrap();
    assert_eq!(history.reference(), Some(a3));
    assert!(history.begin_render(a1).is_err());
}

#[test]
fn stationary_render_can_repeat_without_changing_the_panel_generation() {
    let mut history = PanelHistory::new();
    let a1 = token(BufferId::A, 1);
    history.begin_render(a1).unwrap();
    history.queue(a1, None).unwrap();
    history.complete(a1).unwrap();
    for sequence in 2..100 {
        history.begin_render(token(BufferId::B, sequence)).unwrap();
        assert_eq!(history.reference(), Some(a1));
    }
    assert!(history.begin_render(token(BufferId::B, 0)).is_err());
    assert_eq!(history.reference(), Some(a1));
}

#[test]
fn only_ready_frames_can_be_discarded_without_a_transfer() {
    use dashboard_host_validation::display::pipeline::{BufferState, FrameLifecycle};
    let mut lifecycle = FrameLifecycle::new();
    let a = lifecycle.begin(BufferId::A).unwrap();
    assert!(lifecycle.discard(a).is_err());
    lifecycle.ready(a).unwrap();
    lifecycle.discard(a).unwrap();
    assert_eq!(lifecycle.state(BufferId::A), BufferState::Free);
    assert!(lifecycle.discard(a).is_err());
    let next = lifecycle.begin(BufferId::A).unwrap();
    lifecycle.ready(next).unwrap();
    lifecycle.queue(next).unwrap();
    assert!(lifecycle.discard(next).is_err());
    lifecycle.started(next).unwrap();
    assert!(lifecycle.discard(next).is_err());
}

#[test]
fn portrait_damage_covers_bottom_row_and_uses_bounded_stride() {
    use dashboard_config::Geometry;
    let geometry = Geometry::Portrait;
    assert!(Region::new_in(geometry, 239, 319, 2, 1).is_none());
    assert!(Region::new_in(geometry, 0, 320, 1, 1).is_none());
    let region = Region::new_in(geometry, 239, 317, 1, 3).unwrap();
    assert_eq!(
        region.payload_ranges().collect::<Vec<_>>(),
        vec![152638..152640, 153118..153120, 153598..153600]
    );
    let previous = Box::<FrameBuffer>::default();
    let mut current = Box::<FrameBuffer>::default();
    current[153599] = 1;
    let plan = DamagePlan::between_in(geometry, &previous, &current);
    assert_eq!(plan.geometry(), geometry);
    assert_eq!(plan.regions(), &[Region::new_in(geometry, 224, 304, 16, 16).unwrap()]);
    assert!(
        plan.regions()
            .iter()
            .flat_map(|r| r.payload_ranges())
            .all(|r| r.end <= 153600)
    );
    let full = DamagePlan::full_in(geometry);
    assert!(full.is_full());
    assert_eq!(full.transfer_count(), 1);
    assert_eq!(full.bytes(), 153600);
}

#[test]
fn changed_orientation_requires_full_base_even_for_identical_black_pixels() {
    use dashboard_config::Orientation;
    use dashboard_host_validation::display::damage::GenerationError;
    let mut history = PanelHistory::new();
    let a = token(BufferId::A, 1);
    let b = token(BufferId::B, 2);
    history.begin_render_in(a, Orientation::Landscape).unwrap();
    history.queue(a, None).unwrap();
    history.complete(a).unwrap();
    history.begin_render_in(b, Orientation::Portrait).unwrap();
    assert_eq!(history.reference_for(Orientation::Portrait), None);
    assert_eq!(history.queue(b, Some(a)), Err(GenerationError::WrongBase));
    history.queue(b, None).unwrap();
    assert_eq!(history.panel_orientation(), Some(Orientation::Landscape));
    assert!(history.complete(a).is_err());
    history.complete(b).unwrap();
    assert_eq!(history.panel_orientation(), Some(Orientation::Portrait));
    assert_eq!(history.reference_for(Orientation::Portrait), Some(b));
    assert_eq!(history.reference_for(Orientation::PortraitInverted), None);
}

#[test]
fn both_geometries_reconstruct_every_tile_boundary_and_random_union() {
    use dashboard_config::Geometry;
    for geometry in Geometry::ALL {
        let width = usize::from(geometry.width());
        let height = usize::from(geometry.height());
        let previous = Box::<FrameBuffer>::default();
        let mut next = Box::<FrameBuffer>::default();
        for y in (0..height).step_by(16) {
            for x in (0..width).step_by(16) {
                for (px, py) in [(x, y), (x + 15, y + 15)] {
                    let at = (py * width + px) * 2;
                    next[at + 1] = 0x91;
                    let plan = DamagePlan::between_in(geometry, &previous, &next);
                    assert_eq!(
                        plan.regions(),
                        &[Region::new_in(geometry, x as u16, y as u16, 16, 16).unwrap()]
                    );
                    let mut panel = Box::<FrameBuffer>::default();
                    apply(&mut panel, &next, &plan);
                    assert!(panel == next);
                    next[at + 1] = 0;
                }
            }
        }
        let mut panel = Box::<FrameBuffer>::default();
        let mut seed = 0x2350u32;
        for count in 0..100 {
            for _ in 0..count {
                seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
                next[seed as usize % 153600] = (seed >> 24) as u8;
            }
            let plan = DamagePlan::between_in(geometry, &panel, &next);
            assert!(plan.regions().len() <= 16 && plan.transfer_count() <= 128);
            apply(&mut panel, &next, &plan);
            assert!(panel == next);
        }
        let band = Region::new_in(geometry, 0, geometry.height() - 16, geometry.width(), 16).unwrap();
        assert_eq!(
            band.payload_ranges().collect::<Vec<_>>(),
            vec![153600 - width * 16 * 2..153600]
        );
    }
}
