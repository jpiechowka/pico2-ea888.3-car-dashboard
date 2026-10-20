//! Exact firmware raster export: cargo run --example configured_preview -- out.ppm [config.eav] [view-id]
use std::io::Write;

use dashboard_host_validation::acquisition::AcquisitionModel;
use dashboard_host_validation::configured_scene::Layout;
use dashboard_host_validation::dashboard::DashboardScene;
use dashboard_host_validation::display::St7789Renderer;
use dashboard_host_validation::presentation::PresentationState;
use dashboard_host_validation::render::FpsMode;
use dashboard_host_validation::view_config::{decode, default_config, validate};
use dashboard_host_validation::{AlignedFrame, colors};

fn main() {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    assert!(!args.is_empty(), "expected output.ppm [config.eav] [view-id]");
    let config = if let Some(path) = args.get(1) {
        decode(&std::fs::read(path).unwrap()).unwrap()
    } else {
        validate(default_config()).unwrap()
    };
    let view = args
        .get(2)
        .map_or(config.config().boot_view, |id| id.to_str().unwrap().parse().unwrap());
    let layout = Layout::new(&config, view, false).expect("unknown configured view ID");
    let geometry = config
        .config()
        .view_orientation(view)
        .expect("validated view")
        .geometry();
    let mut model = AcquisitionModel::demo_configured(&config);
    let mut presentation = PresentationState::new();
    for n in 0..60 {
        let now = n * 2000;
        model.ingest_batch(dashboard_host_validation::demo::values(now), now);
        presentation.prepare(&config, &model.snapshot(), view, now, false);
    }
    let snapshot = model.snapshot();
    let readings = presentation.prepare(&config, &snapshot, view, 118000, false);
    let scene = DashboardScene {
        readings: Some(&readings),
        layout: Some(layout),
        measurements: &snapshot,
        now_ms: 118000,
        status: dashboard_host_validation::vehicle_status::VehicleStatus::Demo,
        blink_on: false,
        boost_psi: false,
        fps_mode: FpsMode::Combined,
        fps_instant: 70.0,
        fps_average: 69.0,
        backgrounds: [colors::BLACK; 8],
        overlay: None,
    };
    let mut frame = AlignedFrame::boxed();
    scene.draw_full(&mut St7789Renderer::with_geometry(&mut frame.0, geometry));
    let mut file = std::fs::File::create(&args[0]).unwrap();
    write!(file, "P6\n{} {}\n255\n", geometry.width(), geometry.height()).unwrap();
    for pixel in frame.0.as_chunks::<2>().0 {
        let rgb = u16::from_be_bytes([pixel[0], pixel[1]]);
        let (r, g, b) = ((rgb >> 11) as u8, ((rgb >> 5) & 63) as u8, (rgb & 31) as u8);
        file.write_all(&[(r << 3) | (r >> 2), (g << 2) | (g >> 4), (b << 3) | (b >> 2)])
            .unwrap();
    }
}
