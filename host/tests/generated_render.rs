//! Capacity and retained-render interleavings, using the public shared scene path.
/// Shared panic-only seed/step reporting.
#[path = "../../test-support/generated.rs"]
mod generated;
use dashboard_config::*;
use dashboard_core::acquisition::AcquisitionModel;
use dashboard_core::configured_scene::Layout;
use dashboard_core::dashboard::{DashboardCache, DashboardScene, Overlay};
use dashboard_core::display::St7789Renderer;
use dashboard_core::presentation::PresentationState;
use dashboard_core::render::FpsMode;
use dashboard_core::vehicle_status::VehicleStatus;
use dashboard_core::view_state::WarningPager;
use dashboard_host_validation::{AlignedFrame, colors};

/// Returns 64 widgets across eight views spanning all orientations/styles, with ten-second histories and warnings.
/// Enables advanced display features and 250 ms smoothing; fixture construction/validation failure panics.
fn capacity_config() -> ValidatedConfig {
    let mut raw = default_config();
    raw.views.clear();
    raw.widgets.clear();
    raw.history_seconds = [10; 8];
    raw.warnings.mode = WarningMode::ListThree;
    for view in 1..=8 {
        let orientation = Orientation::ALL[(view - 1) as usize % 4];
        raw.views
            .push(View {
                id: view,
                name: "FULL CAPACITY".try_into().unwrap(),
                orientation: Some(orientation),
            })
            .unwrap();
        for cell in 0..8 {
            let channel = Channel::ALL[cell as usize];
            let columns = orientation.geometry().columns();
            let mut widget = default_widget(
                channel,
                u16::from(view) * 8 + u16::from(cell),
                view,
                Placement {
                    column: cell % columns,
                    row: cell / columns,
                    span: Span::OneByOne,
                },
            );
            widget.gauge.kind = GaugeKind::ALL[(view - 1) as usize % 6];
            widget.display.decimals = 3;
            widget.display.graph = true;
            widget.display.average = true;
            widget.display.extrema = true;
            widget.display.trend = true;
            widget.gauge.expand = true;
            widget.advanced.smoothing_ms = 250;
            widget.warning.enabled = true;
            widget.warning.on_ms = 1;
            if channel != Channel::Lambda {
                widget.warning.mode = LimitMode::High;
                let (low, high) = channel.domain();
                widget.warning.trigger = low + (high - low) * 0.25;
                widget.warning.clear = low + (high - low) * 0.125;
            }
            widget.warning.message = "MAXIMUM WARNING TEXT".try_into().unwrap();
            raw.widgets.push(widget).unwrap();
        }
    }
    validate(raw).unwrap()
}

/// Full-capacity randomized view/unit/overlay/staleness changes keep both retained buffers equal to poisoned full
/// repaint.
#[test]
fn maximum_configuration_stays_coherent_after_view_warning_and_geometry_changes() {
    // Mutation targets: cache keys missing view/geometry/unit/overlay/data age,
    // stale graph backgrounds and one retained buffer missing a repaint.
    let config = capacity_config();
    let mut model = AcquisitionModel::demo_configured(&config);
    // Wrap all channel rings before beginning the view-switch sequence.
    for sample in 0..400 {
        let values = Channel::ALL.map(|channel| {
            let (min, max) = channel.domain();
            min + (max - min) * (sample % 31) as f32 / 30.
        });
        assert!(model.ingest_batch(values, sample * 50));
    }
    let mut presentation = PresentationState::new();
    let mut pager = WarningPager::new();
    let mut cache = DashboardCache::new();
    let mut buffers = [AlignedFrame::boxed(), AlignedFrame::boxed()];
    let mut oracle = AlignedFrame::boxed();
    let mut rng = 0x1723_5001u32;
    let mut warning_frames = 0;
    for step in 0..96 {
        let _case = generated::Case::new("render", 0x1723_5001, step as usize);
        rng ^= rng << 13;
        rng ^= rng >> 17;
        rng ^= rng << 5;
        let view = 1 + (rng % 8) as u8;
        let geometry = config.config().view_orientation(view).unwrap().geometry();
        let now = 20_000 + step * 100;
        // Include missing/stale periods without inventing replacement samples.
        if step % 17 < 11 {
            let values = Channel::ALL.map(|channel| channel.domain().1);
            model.ingest_batch(values, now);
        }
        let snapshot = model.snapshot();
        let psi = rng & 16 != 0;
        let readings = presentation.prepare(&config, &snapshot, view, now, psi);
        let warnings = pager.select(&config, &snapshot, view, now, rng & 32 != 0);
        warning_frames += usize::from(warnings.is_some());
        let overlay = if step % 5 == 0 {
            Some(Overlay::Fps(FpsMode::Combined))
        } else if step % 3 == 0 {
            warnings.map(Overlay::Warnings)
        } else {
            None
        };
        let scene = DashboardScene {
            readings: Some(&readings),
            layout: Layout::new(&config, view, false),
            measurements: &snapshot,
            status: VehicleStatus::Demo,
            now_ms: now,
            blink_on: rng & 32 != 0,
            boost_psi: psi,
            fps_mode: FpsMode::Combined,
            fps_instant: 70.,
            fps_average: 69.5,
            backgrounds: [colors::BLACK; 8],
            overlay,
        };
        // Full repaint is the independent cache oracle; poison it first so a
        // shared failure to clear does not compare two identical stale frames.
        oracle.0.fill(0xA5);
        scene.draw_full(&mut St7789Renderer::with_geometry(&mut oracle.0, geometry));
        for (index, buffer) in buffers.iter_mut().enumerate() {
            cache.draw(
                &mut St7789Renderer::with_geometry(&mut buffer.0, geometry),
                index,
                &scene,
            );
            assert!(
                buffer.0 == oracle.0,
                "seed=17235001 step={step} view={view} buffer={index}"
            );
        }
    }
    assert!(warning_frames > 48, "the generated run must actually exercise warnings");
}
