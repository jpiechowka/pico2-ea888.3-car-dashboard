//! Fixed worst-change fixture shared with pixel/metadata acceptance tests.
use std::hint::black_box;

use dashboard_host_validation::acquisition::{AcquisitionModel, DashboardSnapshot};
use dashboard_host_validation::colors;
use dashboard_host_validation::dashboard::{DashboardCache, DashboardScene, Overlay};
use dashboard_host_validation::display::{FrameBuffer, St7789Renderer};
use dashboard_host_validation::render::FpsMode;
use dashboard_host_validation::samples::{Quality, SensorId};
use dashboard_host_validation::vehicle_status::VehicleStatus;

pub const STEPS: usize = 8;
const SEED: u64 = 60;
const PERIOD_MS: u64 = 5000;

fn values(index: u64) -> [f32; 8] {
    let wave = (index % 13) as f32;
    [
        0.8 + wave / 10.0,
        85.0 + wave,
        80.0 + wave,
        70.0 + wave,
        20.0 + wave,
        700.0 + wave * 10.0,
        12.0 + wave / 10.0,
        0.8 + wave / 50.0,
    ]
}

pub fn model() -> AcquisitionModel {
    let mut model = AcquisitionModel::demo();
    for index in 0..SEED {
        assert!(model.ingest_batch(values(index), index * PERIOD_MS));
    }
    model
}

fn update(
    model: &mut AcquisitionModel,
    step: usize,
) -> DashboardSnapshot {
    let index = SEED + step as u64;
    assert!(model.ingest_batch(black_box(values(index)), black_box(index * PERIOD_MS)));
    model.snapshot()
}

pub fn snapshots() -> [DashboardSnapshot; STEPS] {
    let mut model = model();
    core::array::from_fn(|step| update(&mut model, step))
}

pub struct Raster {
    frames: [Box<FrameBuffer>; 2],
    cache: DashboardCache,
}
impl Raster {
    pub fn new(initial: &DashboardSnapshot) -> Self {
        let mut raster = Self {
            frames: [Box::new(FrameBuffer::new()), Box::new(FrameBuffer::new())],
            cache: DashboardCache::new(),
        };
        // Warm both buffer caches outside measurement. The measured first step
        // therefore changes real retained content rather than testing startup.
        raster.draw(initial, 0, true);
        raster.draw(initial, 1, true);
        raster
    }

    fn draw(
        &mut self,
        snapshot: &DashboardSnapshot,
        step: usize,
        retained: bool,
    ) -> &FrameBuffer {
        let scene = DashboardScene {
            measurements: black_box(snapshot),
            now_ms: snapshot.sensors[0].sampled_at_ms().unwrap(),
            status: VehicleStatus::Demo,
            blink_on: step.is_multiple_of(2),
            boost_psi: step.is_multiple_of(2),
            fps_mode: FpsMode::Combined,
            fps_instant: 40.0 + step as f32,
            fps_average: 39.0,
            backgrounds: if step.is_multiple_of(2) {
                [colors::ORANGE; 8]
            } else {
                [colors::BLACK; 8]
            },
            overlay: match step {
                1 => Some(Overlay::Reset),
                3 => Some(Overlay::Brightness(30)),
                4 => Some(Overlay::Critical(SensorId::Egt, true)),
                5 => Some(Overlay::Critical(SensorId::Egt, false)),
                6 => Some(Overlay::Fps(FpsMode::Combined)),
                _ => None,
            },
        };
        let buffer = step % 2;
        let mut display = St7789Renderer::new(&mut self.frames[buffer]);
        if retained {
            self.cache.draw(&mut display, buffer, black_box(&scene));
        } else {
            black_box(&scene).draw_full(&mut display);
        }
        black_box(&self.frames[buffer])
    }

    pub fn run(
        &mut self,
        snapshots: &[DashboardSnapshot; STEPS],
        retained: bool,
    ) {
        for (step, snapshot) in snapshots.iter().enumerate() {
            self.draw(snapshot, step, retained);
        }
    }
}

pub fn run_model(
    model: &mut AcquisitionModel,
    raster: &mut Raster,
    retained: bool,
) {
    for step in 0..STEPS {
        let snapshot = update(black_box(&mut *model), step);
        raster.draw(&snapshot, step, retained);
    }
}

pub fn verify() {
    let prepared = snapshots();
    let mut model = model();
    let mut previous = model.snapshot();
    let mut retained = Raster::new(&previous);
    let mut reference = Raster::new(&previous);
    for (step, expected) in prepared.iter().enumerate() {
        let current = update(&mut model, step);
        assert_eq!(current, *expected);
        for id in SensorId::ALL {
            let sensor = current.sensor(id);
            let at = (SEED + step as u64) * PERIOD_MS;
            assert_eq!(sensor.sampled_at_ms(), Some(at));
            assert_eq!(current.quality(id, at), Quality::Valid);
            assert_ne!(sensor.value(), previous.sensor(id).value());
            let (_, start, count, ..) = sensor.get_graph_data();
            let (_, old_start, ..) = previous.sensor(id).get_graph_data();
            assert_eq!(count, 60);
            assert_eq!(start, (old_start + 1) % 60);
        }
        assert!(
            retained.draw(&current, step, true) == reference.draw(expected, step, false),
            "replay pixels at step {step}"
        );
        previous = current;
    }
    // Exercise the actual measured loops as well as individual checkpoints.
    let mut owner = self::model();
    let initial = owner.snapshot();
    let mut live = Raster::new(&initial);
    let mut frozen = Raster::new(&initial);
    run_model(&mut owner, &mut live, true);
    frozen.run(&prepared, false);
    assert!(live.frames == frozen.frames);
}
