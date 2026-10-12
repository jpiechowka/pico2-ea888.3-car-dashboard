use std::hint::black_box;

use criterion::{BatchSize, Criterion};
use dashboard_host_validation::acquisition::{AcquisitionModel, DashboardSnapshot};
use dashboard_host_validation::dashboard::{DashboardCache, DashboardScene, Overlay, Redraw};
use dashboard_host_validation::display::damage::DamagePlan;
use dashboard_host_validation::display::{FrameBuffer, St7789Renderer};
use dashboard_host_validation::render::FpsMode;
use dashboard_host_validation::samples::{Quality, Quantity, Sample, SensorId, Source};
use dashboard_host_validation::vehicle_status::VehicleStatus;
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::RgbColor;

#[derive(Clone, Copy)]
struct Input {
    snapshot: DashboardSnapshot,
    now: u64,
    psi: bool,
    overlay: Option<Overlay>,
    fps: Option<f32>,
}
impl Input {
    fn scene(&self) -> DashboardScene<'_> {
        DashboardScene {
            layout: None,
            measurements: &self.snapshot,
            status: VehicleStatus::derive(false, None, &self.snapshot, self.now),
            now_ms: self.now,
            blink_on: false,
            boost_psi: self.psi,
            fps_mode: if self.fps.is_some() {
                FpsMode::Instant
            } else {
                FpsMode::Off
            },
            fps_instant: self.fps.unwrap_or(0.0),
            fps_average: 0.0,
            backgrounds: [Rgb565::BLACK; 8],
            overlay: self.overlay,
        }
    }

    fn full(&self) -> Box<FrameBuffer> {
        let mut frame = Box::new(FrameBuffer::new());
        self.scene().draw_full(&mut St7789Renderer::new(&mut frame));
        frame
    }
}

struct Case {
    label: &'static str,
    initial: Input,
    steps: Vec<Input>,
    cold: bool,
}
struct Raster {
    frame: Box<FrameBuffer>,
    cache: DashboardCache,
}
impl Raster {
    fn new(case: &Case) -> Self {
        let mut raster = Self {
            frame: Box::new(FrameBuffer::new()),
            cache: DashboardCache::new(),
        };
        if !case.cold {
            raster.draw(&case.initial);
        }
        raster
    }

    fn draw(
        &mut self,
        input: &Input,
    ) -> Redraw {
        self.cache
            .draw(&mut St7789Renderer::new(&mut self.frame), 0, black_box(&input.scene()))
    }

    fn run(
        &mut self,
        case: &Case,
    ) {
        for input in &case.steps {
            black_box(self.draw(black_box(input)));
        }
        black_box(&self.frame);
    }
}

fn cases() -> Vec<Case> {
    let mut model = AcquisitionModel::demo();
    let values = [1.2001, 90.0, 99.0, 80.0, 30.0, 750.0, 13.8, 1.0];
    assert!(model.ingest_batch(values, 1000));
    let initial = Input {
        snapshot: model.snapshot(),
        now: 1000,
        psi: false,
        overlay: None,
        fps: None,
    };
    let mut result = vec![
        Case {
            label: "cold_cache_one_frame",
            initial,
            steps: vec![initial],
            cold: true,
        },
        Case {
            label: "unit_toggle_one_frame",
            initial,
            steps: vec![Input { psi: true, ..initial }],
            cold: false,
        },
    ];
    let mut same = values;
    same[0] = 1.2002;
    assert!(model.ingest_batch(same, 1001));
    result.push(Case {
        label: "same_rounded_text_one_frame",
        initial,
        steps: vec![Input {
            snapshot: model.snapshot(),
            now: 1001,
            ..initial
        }],
        cold: false,
    });
    let mut wider = same;
    wider[2] = 100.0;
    assert!(model.ingest_batch(wider, 1002));
    result.push(Case {
        label: "digit_width_one_frame",
        initial,
        steps: vec![Input {
            snapshot: model.snapshot(),
            now: 1002,
            ..initial
        }],
        cold: false,
    });

    let mut vehicle = AcquisitionModel::new();
    let sample = |at, sequence| {
        Sample {
            value: Quantity::CoolantC(95.0),
            received_at_ms: at,
            sequence,
            source: Source::Obd { ecu: 0x7E8, pid: 5 },
        }
    };
    vehicle.ingest_sample(sample(1000, 1), 1000).unwrap();
    let valid = Input {
        snapshot: vehicle.snapshot(),
        ..initial
    };
    let stale_at = 1001 + SensorId::Water.stale_after_ms();
    let stale = Input { now: stale_at, ..valid };
    assert!(vehicle.mark_fault(SensorId::Water, Quality::InvalidPayload, stale_at + 1));
    let invalid = Input {
        snapshot: vehicle.snapshot(),
        now: stale_at + 1,
        ..valid
    };
    vehicle.ingest_sample(sample(stale_at + 2, 2), stale_at + 2).unwrap();
    let recovered = Input {
        snapshot: vehicle.snapshot(),
        now: stale_at + 2,
        ..valid
    };
    for (input, quality) in [
        (valid, Quality::Valid),
        (stale, Quality::Stale),
        (invalid, Quality::InvalidPayload),
        (recovered, Quality::Valid),
    ] {
        assert_eq!(input.snapshot.quality(SensorId::Water, input.now), quality);
    }
    result.push(Case {
        label: "quality_transitions_three_frames",
        initial: valid,
        steps: vec![stale, invalid, recovered],
        cold: false,
    });
    result
}

fn damage_cases() -> Vec<(&'static str, Box<FrameBuffer>, Box<FrameBuffer>)> {
    let transitions = cases();
    let digit = transitions.iter().find(|c| c.label == "digit_width_one_frame").unwrap();
    let initial = digit.initial;
    let mut output = vec![
        (
            "one_digit",
            Input {
                fps: Some(40.0),
                ..initial
            }
            .full(),
            Input {
                fps: Some(41.0),
                ..initial
            }
            .full(),
        ),
        ("digit_width", initial.full(), digit.steps[0].full()),
    ];
    let mut model = AcquisitionModel::demo();
    assert!(model.ingest_batch([1.8, 90.0, 99.0, 80.0, 40.0, 750.0, 13.8, 1.0], 1000));
    output.push((
        "disjoint_full_fallback",
        initial.full(),
        Input {
            snapshot: model.snapshot(),
            ..initial
        }
        .full(),
    ));
    let mut sparse = initial.full();
    // Two glyph-sized patches wholly inside separate cells/tiles. Keep this
    // planner-only pattern separate from the real scene that exceeds the
    // payload-start cap above.
    for (x0, y0) in [(16, 48), (272, 176)] {
        for y in y0..y0 + 12 {
            for x in x0..x0 + 8 {
                let at = (y * 320 + x) * 2;
                sparse[at] ^= 0xFF;
                sparse[at + 1] ^= 0xFF;
            }
        }
    }
    output.push(("disjoint_sparse_cells", initial.full(), sparse));
    output.push((
        "popup_removal",
        Input {
            overlay: Some(Overlay::Reset),
            ..initial
        }
        .full(),
        initial.full(),
    ));
    output
}

fn verify_damage(
    previous: &FrameBuffer,
    current: &FrameBuffer,
) -> DamagePlan {
    let plan = DamagePlan::between(previous, current);
    let mut panel = Box::new(FrameBuffer::new());
    panel.copy_from_slice(&previous[..]);
    let mut bytes = 0;
    let mut transfers = 0;
    for region in plan.regions() {
        for range in region.payload_ranges() {
            bytes += range.len();
            transfers += 1;
            panel[range.clone()].copy_from_slice(&current[range]);
        }
    }
    assert!(*panel == *current, "region replay must restore every panel pixel");
    assert_eq!(bytes, plan.bytes());
    assert_eq!(transfers, plan.transfer_count());
    assert!(!plan.is_empty());
    plan
}

pub fn verify() {
    for case in cases() {
        let mut raster = Raster::new(&case);
        for input in &case.steps {
            let redraw = raster.draw(input);
            assert!(raster.frame == input.full(), "{} pixels", case.label);
            match case.label {
                "same_rounded_text_one_frame" => assert_eq!(redraw.cells, 0),
                "unit_toggle_one_frame" => assert_eq!(redraw.cells, 1),
                "cold_cache_one_frame" => assert!(redraw.clear && redraw.cells == 255),
                _ => assert_ne!(redraw.cells, 0),
            }
        }
        let mut measured = Raster::new(&case);
        measured.run(&case);
        assert!(measured.frame == raster.frame);
    }
    for (label, previous, current) in damage_cases() {
        let plan = verify_damage(&previous, &current);
        if label == "disjoint_sparse_cells" {
            assert_eq!(
                (plan.bytes(), plan.regions().len(), plan.transfer_count()),
                (1024, 2, 32)
            );
        }
        if label == "disjoint_full_fallback" {
            assert!(plan.is_full());
        }
    }
}

pub fn benchmarks(c: &mut Criterion) {
    verify();
    let initial = cases().remove(0).initial;
    for (label, cell) in [("boost", 0), ("lambda", 1), ("battery", 2), ("temperature", 3)] {
        let mut frame = initial.full();
        let expected = initial.full();
        use dashboard_host_validation::config::{COL_WIDTH, HEADER_HEIGHT, ROW_HEIGHT};
        let x0 = (cell % 4) * COL_WIDTH as usize;
        let y0 = HEADER_HEIGHT as usize + (cell / 4) * ROW_HEIGHT as usize;
        for y in y0 + 1..y0 + ROW_HEIGHT as usize - 1 {
            for x in x0 + 1..x0 + COL_WIDTH as usize - 1 {
                let at = (y * 320 + x) * 2;
                frame[at] ^= 0xFF;
                frame[at + 1] ^= 0xFF;
            }
        }
        assert!(
            frame != expected,
            "cell preflight must begin with wrong interior pixels"
        );
        initial.scene().draw_cell(&mut St7789Renderer::new(&mut frame), cell);
        assert!(frame == expected);
        c.bench_function(&format!("cell_render/{label}"), |b| {
            b.iter(|| {
                black_box(initial.scene()).draw_cell(&mut St7789Renderer::new(&mut frame), black_box(cell));
                black_box(&frame);
            })
        });
    }
    for case in cases() {
        let mut check = Raster::new(&case);
        for (i, input) in case.steps.iter().enumerate() {
            let changed = check.draw(input);
            eprintln!(
                "WORK scene {} step={i}: dirty_cells={} clear={} header={}",
                case.label,
                changed.cells.count_ones(),
                changed.clear,
                changed.header
            );
        }
        c.bench_function(&format!("scene_transition/{}", case.label), |b| {
            b.iter_batched_ref(
                || Raster::new(&case),
                |raster| raster.run(black_box(&case)),
                BatchSize::PerIteration,
            )
        });
    }
    for (label, previous, current) in damage_cases() {
        let plan = verify_damage(&previous, &current);
        eprintln!(
            "WORK damage {label}: bytes={} regions={} transfers={}",
            plan.bytes(),
            plan.regions().len(),
            plan.transfer_count()
        );
        c.bench_function(&format!("damage/{label}_with_ranges"), |b| {
            b.iter(|| {
                let plan = DamagePlan::between(black_box(&previous), black_box(&current));
                for region in plan.regions() {
                    for range in region.payload_ranges() {
                        black_box(range);
                    }
                }
                black_box(plan);
            })
        });
    }
}
