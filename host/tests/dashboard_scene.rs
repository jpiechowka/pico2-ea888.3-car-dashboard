use dashboard_host_validation::acquisition::AcquisitionModel;
use dashboard_host_validation::backlight::Brightness;
use dashboard_host_validation::dashboard::{DashboardScene, Overlay};
use dashboard_host_validation::display::St7789Renderer;
use dashboard_host_validation::render::FpsMode;
use dashboard_host_validation::{AlignedFrame, colors};

#[test]
fn scene_cell_order_and_overlay_removal_match_a_fresh_frame() {
    let mut model = AcquisitionModel::demo();
    model.ingest_batch([1.2, 90.0, 95.0, 80.0, 30.0, 750.0, 13.8, 1.0], 0);
    let snapshot = model.snapshot();
    let mut scene = DashboardScene {
        measurements: &snapshot,
        now_ms: 0,
        status: dashboard_host_validation::vehicle_status::VehicleStatus::Demo,
        blink_on: true,
        boost_psi: false,
        fps_mode: FpsMode::Off,
        fps_instant: 40.0,
        fps_average: 39.0,
        backgrounds: [colors::BLACK; 8],
        overlay: None,
    };
    let mut frame = AlignedFrame::boxed();
    let mut reference = AlignedFrame::boxed();
    scene.draw_full(&mut St7789Renderer::new(&mut reference.0));
    for overlay in [
        Overlay::Reset,
        Overlay::Fps(FpsMode::Combined),
        Overlay::BoostUnit(true),
        Overlay::Brightness(Brightness::Percent(0)),
        Overlay::Critical(dashboard_host_validation::samples::SensorId::Egt, true),
    ] {
        scene.overlay = Some(overlay);
        scene.draw_full(&mut St7789Renderer::new(&mut frame.0));
        assert!(frame.0 != reference.0);
        scene.overlay = None;
        scene.draw_full(&mut St7789Renderer::new(&mut frame.0));
        assert!(frame.0 == reference.0);
    }
    assert_eq!(scene.cell_sensor(0), Some(0));
    assert_eq!(scene.cell_sensor(1), Some(7));
    assert_eq!(scene.cell_sensor(2), Some(6));
    assert_eq!(scene.cell_sensor(3), Some(2));
    assert_eq!(scene.cell_sensor(4), Some(1));
    assert_eq!(scene.cell_sensor(5), Some(3));
    assert_eq!(scene.cell_sensor(6), Some(4));
    assert_eq!(scene.cell_sensor(7), Some(5));
    assert_eq!(scene.cell_sensor(8), None);
}
