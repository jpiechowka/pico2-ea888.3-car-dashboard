//! Exact firmware raster export: cargo run --example configured_preview -- out.ppm [config.eav] [view-id]
use std::io::Write;

use dashboard_host_validation::acquisition::AcquisitionModel;
use dashboard_host_validation::configured_scene::Layout;
use dashboard_host_validation::dashboard::DashboardScene;
use dashboard_host_validation::display::St7789Renderer;
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
    let mut model = AcquisitionModel::demo();
    for n in 0..60 {
        let now = n * 2000;
        model.ingest_batch(dashboard_host_validation::demo::values(now), now);
    }
    let snapshot = model.snapshot();
    let scene = DashboardScene {
        readings: None,
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
    scene.draw_full(&mut St7789Renderer::new(&mut frame.0));
    let mut file = std::fs::File::create(&args[0]).unwrap();
    file.write_all(b"P6\n320 240\n255\n").unwrap();
    for pixel in frame.0.as_chunks::<2>().0 {
        let rgb = u16::from_be_bytes([pixel[0], pixel[1]]);
        let (r, g, b) = ((rgb >> 11) as u8, ((rgb >> 5) & 63) as u8, (rgb & 31) as u8);
        file.write_all(&[(r << 3) | (r >> 2), (g << 2) | (g >> 4), (b << 3) | (b >> 2)])
            .unwrap();
    }
}
