//! Exercise the published executable, including its fail-before-output contract.
use std::path::PathBuf;
use std::process::Command;
use std::sync::OnceLock;

use dashboard_host_validation::acquisition::AcquisitionModel;
use dashboard_host_validation::configured_scene::Layout;
use dashboard_host_validation::dashboard::DashboardScene;
use dashboard_host_validation::display::St7789Renderer;
use dashboard_host_validation::presentation::PresentationState;
use dashboard_host_validation::render::FpsMode;
use dashboard_host_validation::vehicle_status::VehicleStatus;
use dashboard_host_validation::view_config::*;
use dashboard_host_validation::{AlignedFrame, colors};

/// Builds the configured_preview example once with matching outline features and returns its cached executable path.
/// Honors CARGO_TARGET_DIR relative to the host manifest; spawn or unsuccessful build panics.
fn executable() -> &'static PathBuf {
    /// Process-wide successful example path shared by concurrently running tests.
    static EXE: OnceLock<PathBuf> = OnceLock::new();
    EXE.get_or_init(|| {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let mut build = Command::new(env!("CARGO"));
        build
            .current_dir(&root)
            .args(["build", "--locked", "--example", "configured_preview"]);
        if !cfg!(feature = "simple-outline") {
            build.arg("--no-default-features");
        }
        let status = build.status().unwrap();
        assert!(status.success());
        let target = std::env::var_os("CARGO_TARGET_DIR").map_or_else(|| root.join("target"), |path| root.join(path));
        target
            .join("debug/examples")
            .join(format!("configured_preview{}", std::env::consts::EXE_SUFFIX))
    })
}

/// Returns binary P6 bytes for RGB565 `frame` with header dimensions from `geometry`.
/// Expands 5/6-bit channels by bit replication; caller must supply the frame's actual geometry.
fn ppm(
    frame: &AlignedFrame,
    geometry: Geometry,
) -> Vec<u8> {
    let mut out = format!("P6\n{} {}\n255\n", geometry.width(), geometry.height()).into_bytes();
    for pixel in frame.0.as_chunks::<2>().0 {
        let rgb = u16::from_be_bytes([pixel[0], pixel[1]]);
        let (r, g, b) = ((rgb >> 11) as u8, ((rgb >> 5) & 63) as u8, (rgb & 31) as u8);
        out.extend_from_slice(&[(r << 3) | (r >> 2), (g << 2) | (g >> 4), (b << 3) | (b >> 2)]);
    }
    out
}

/// Executable output matches configured smoothing/peaks; invalid view selection preserves existing output bytes.
/// Creates and removes process-specific temporary fixtures; I/O/build/child failures panic.
#[test]
fn published_snapshot_uses_bound_advanced_acquisition_and_prepared_readings() {
    let dir = std::env::temp_dir().join(format!("dashboard-preview-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let selected = dir.join("advanced.eav");
    let output = dir.join("snapshot.ppm");
    let mut raw = default_config();
    raw.history_seconds = [30; 8];
    raw.widgets[0].advanced.smoothing_ms = 5000;
    raw.widgets[0].advanced.peak_mode = PeakSelection::Maximum;
    raw.widgets[0].advanced.peak_hold_ms = 60000;
    raw.widgets[0].warning.trigger = 0.2;
    raw.widgets[0].warning.clear = 0.1;
    raw.widgets[0].warning.on_ms = 100;
    let config = validate(raw).unwrap();
    let mut bytes = [0; MAX_FILE_BYTES];
    std::fs::write(&selected, encode(&config, &mut bytes).unwrap()).unwrap();
    let result = Command::new(executable()).args([&output, &selected]).output().unwrap();
    assert!(result.status.success(), "{}", String::from_utf8_lossy(&result.stderr));
    let mut model = AcquisitionModel::demo_configured(&config);
    let mut presentation = PresentationState::new();
    for n in 0..60 {
        let now = n * 2000;
        model.ingest_batch(dashboard_host_validation::demo::values(now), now);
        presentation.prepare(&config, &model.snapshot(), config.config().boot_view, now, false);
    }
    let snapshot = model.snapshot();
    let readings = presentation.prepare(&config, &snapshot, config.config().boot_view, 118000, false);
    let scene = DashboardScene {
        readings: Some(&readings),
        layout: Layout::new(&config, config.config().boot_view, false),
        measurements: &snapshot,
        now_ms: 118000,
        status: VehicleStatus::Demo,
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
    assert!(
        std::fs::read(&output).unwrap() == ppm(&frame, Geometry::Landscape),
        "published snapshot diverges from configured prepared shared renderer"
    );
    // Reject an unknown view before opening even an already-existing destination.
    std::fs::write(&output, b"preserve existing output").unwrap();
    let result = Command::new(executable())
        .arg(&output)
        .arg(&selected)
        .arg("255")
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert_eq!(std::fs::read(&output).unwrap(), b"preserve existing output");
    std::fs::remove_file(&selected).unwrap();
    std::fs::remove_file(&output).unwrap();
    std::fs::remove_dir(&dir).unwrap();
}

/// Device defaults and explicit view overrides produce exact shared-renderer geometry and PPM pixels.
/// Creates and removes process-specific temporary fixtures; I/O/build/child failures panic.
#[test]
fn published_snapshot_resolves_selected_view_geometry() {
    let dir = std::env::temp_dir().join(format!("dashboard-preview-geometry-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let selected = dir.join("geometry.eav");
    let output = dir.join("snapshot.ppm");
    for (device, override_orientation, geometry) in [
        (Orientation::Portrait, None, Geometry::Portrait),
        (Orientation::Landscape, Some(Orientation::Portrait), Geometry::Portrait),
        (Orientation::Portrait, Some(Orientation::Landscape), Geometry::Landscape),
    ] {
        let mut raw = default_config();
        raw.orientation = device;
        raw.views[0].id = 2;
        raw.views[0].orientation = override_orientation;
        raw.views
            .push(View {
                id: 1,
                name: "Boot".try_into().unwrap(),
                orientation: None,
            })
            .unwrap();
        for (index, widget) in raw.widgets.iter_mut().enumerate() {
            widget.view = 2;
            widget.placement.column = index as u8 % geometry.columns();
            widget.placement.row = index as u8 / geometry.columns();
        }
        let config = validate(raw).unwrap();
        let mut bytes = [0; MAX_FILE_BYTES];
        std::fs::write(&selected, encode(&config, &mut bytes).unwrap()).unwrap();
        let result = Command::new(executable())
            .args([&output, &selected])
            .arg("2")
            .output()
            .unwrap();
        assert!(result.status.success(), "{}", String::from_utf8_lossy(&result.stderr));
        let actual = std::fs::read(&output).unwrap();
        let header = format!("P6\n{} {}\n255\n", geometry.width(), geometry.height());
        assert!(
            actual.starts_with(header.as_bytes()),
            "wrong geometry: {device:?}/{override_orientation:?}"
        );
        assert_eq!(actual.len(), header.len() + 320 * 240 * 3);
        let mut model = AcquisitionModel::demo_configured(&config);
        let mut presentation = PresentationState::new();
        for n in 0..60 {
            let now = n * 2000;
            model.ingest_batch(dashboard_host_validation::demo::values(now), now);
            presentation.prepare(&config, &model.snapshot(), 2, now, false);
        }
        let snapshot = model.snapshot();
        let readings = presentation.prepare(&config, &snapshot, 2, 118000, false);
        let scene = DashboardScene {
            readings: Some(&readings),
            layout: Layout::new(&config, 2, false),
            measurements: &snapshot,
            now_ms: 118000,
            status: VehicleStatus::Demo,
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
        assert!(
            actual == ppm(&frame, geometry),
            "native geometry pixels differ: {device:?}/{override_orientation:?}"
        );
    }
    std::fs::remove_file(selected).unwrap();
    std::fs::remove_file(output).unwrap();
    std::fs::remove_dir(dir).unwrap();
}
