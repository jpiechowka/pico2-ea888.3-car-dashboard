//! Export shared RGB565 scene samples for visual inspection, without image crates.
use dashboard_core::display::{FrameBuffer, St7789Renderer};
use dashboard_core::startup::*;
use dashboard_core::vehicle_status::VehicleStatus;
/// Writes four landscape RGB565 samples for each procedural theme into the first
/// CLI argument's directory, creating it if necessary and replacing matching files.
/// Panics for a missing argument or filesystem failure; performs no device I/O.
fn main() {
    let directory = std::env::args().nth(1).expect("output directory");
    std::fs::create_dir_all(&directory).unwrap();
    let mut frame = Box::new(FrameBuffer::new());
    for (index, theme) in StartupTheme::ALL.into_iter().enumerate() {
        for (sample, time) in [0, 350, 1200, 1999].into_iter().enumerate() {
            draw(
                &mut St7789Renderer::new(&mut frame),
                Phase::Splash,
                StartupProfile {
                    theme,
                    ..StartupProfile::DEFAULT
                },
                time,
                false,
                StartupFacts {
                    vehicle: VehicleStatus::Demo,
                    config_ok: true,
                    settings_loaded: false,
                    configured_sources: 0,
                },
            );
            std::fs::write(format!("{directory}/{index}-{sample}.rgb565"), &frame[..]).unwrap();
        }
    }
}
