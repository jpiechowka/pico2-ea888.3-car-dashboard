//! Fixed worst-change fixture shared with pixel/metadata acceptance tests.
use std::hint::black_box;

use dashboard_host_validation::acquisition::{AcquisitionModel, DashboardSnapshot};
use dashboard_host_validation::backlight::Brightness;
use dashboard_host_validation::colors;
use dashboard_host_validation::dashboard::{DashboardCache, DashboardScene, Overlay};
use dashboard_host_validation::display::{FrameBuffer, St7789Renderer};
use dashboard_host_validation::render::FpsMode;
use dashboard_host_validation::samples::{Quality, SensorId};
use dashboard_host_validation::vehicle_status::VehicleStatus;

/// Number of measured scene transitions in one fixed replay iteration.
pub const STEPS: usize = 8;
/// Number of synthetic measurements used to prefill statistics before timing.
const SEED: u64 = 60;
/// Fixed measurement spacing in milliseconds, retaining explicit graph gaps.
const PERIOD_MS: u64 = 5000;

/// Returns eight canonical channel values from periodic sample `index`, changing all channels together.
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

/// Returns a demo model prefilled with sixty five-second samples; fixture admission failure panics.
pub fn model() -> AcquisitionModel<'static> {
    let mut model = AcquisitionModel::demo();
    for index in 0..SEED {
        assert!(model.ingest_batch(values(index), index * PERIOD_MS));
    }
    model
}

/// Advances seeded `model` with zero-based replay `step` and returns its coherent snapshot.
/// Panics on rejected/repeated ordering; callers replay steps 0..STEPS once from the seeded model.
fn update(
    model: &mut AcquisitionModel,
    step: usize,
) -> DashboardSnapshot {
    let index = SEED + step as u64;
    assert!(model.ingest_batch(black_box(values(index)), black_box(index * PERIOD_MS)));
    model.snapshot()
}

/// Returns all eight snapshots from the fixed seeded acquisition trace.
pub fn snapshots() -> [DashboardSnapshot; STEPS] {
    let mut model = model();
    core::array::from_fn(|step| update(&mut model, step))
}

/// Two prewarmed framebuffers with retained identities for timed alternating presentation.
pub struct Raster {
    /// Independent landscape buffers selected by step parity.
    frames: [Box<FrameBuffer>; 2],
    /// Shared retained-scene metadata for both buffers.
    cache: DashboardCache,
}
impl Raster {
    /// Returns both buffers prewarmed from `initial` before measurement; missing boost sample time panics.
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

    /// Draws `snapshot` with fixed visual changes selected by `step`; `retained` chooses cached versus full rendering.
    /// Returns the parity-selected framebuffer after mutation. Panics if the boost sample has no timestamp.
    fn draw(
        &mut self,
        snapshot: &DashboardSnapshot,
        step: usize,
        retained: bool,
    ) -> &FrameBuffer {
        let scene = DashboardScene {
            readings: None,
            layout: None,
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
                3 => Some(Overlay::Brightness(Brightness::Percent(30))),
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

    /// Replays prepared `snapshots` through both buffers; `retained` selects cache or full paint at every step.
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

/// Advances seeded `model` and paints all eight steps into `raster`, with `retained` selecting the draw path.
/// Mutates both owners; repeated use without resetting the model rejects timestamps and panics.
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

/// Asserts all channels change with exact sample time, quality and sparse history, and both draw paths match every
/// pixel.
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
            let history = sensor.history();
            assert_eq!(history.count(), 24); // Qualified points remain separated by timed gaps.
            assert!(history.newest_ordinal() > previous.sensor(id).history().newest_ordinal());
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
