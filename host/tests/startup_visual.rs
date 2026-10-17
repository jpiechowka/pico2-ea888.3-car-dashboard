use dashboard_config::Geometry;
use dashboard_core::display::St7789Renderer;
use dashboard_core::startup::*;
use dashboard_core::vehicle_status::VehicleStatus;
use dashboard_host_validation::AlignedFrame;

fn render(
    theme: StartupTheme,
    geometry: Geometry,
    time: u64,
    looping: bool,
    reduced: bool,
) -> Box<AlignedFrame> {
    let mut frame = AlignedFrame::boxed();
    draw(
        &mut St7789Renderer::with_geometry(&mut frame.0, geometry),
        Phase::Splash,
        StartupProfile {
            theme,
            loop_animation: looping,
            ..StartupProfile::DEFAULT
        },
        time,
        reduced,
        StartupFacts {
            vehicle: VehicleStatus::Demo,
            config_ok: true,
            settings_loaded: true,
            configured_sources: 8,
        },
    );
    frame
}

#[test]
fn light_splashes_preserve_quiet_edges_and_footer_at_sampled_animation_phases() {
    for geometry in [Geometry::Landscape, Geometry::Portrait] {
        let width = usize::from(geometry.width());
        let height = usize::from(geometry.height());
        for theme in StartupTheme::ALL {
            let reference = render(theme, geometry, 0, true, false);
            let footer = if geometry == Geometry::Portrait { 270 } else { 217 };
            for time in (0..2000).step_by(37) {
                let frame = render(theme, geometry, time, true, false);
                let pixels = frame.0.as_chunks::<2>().0;
                assert!(
                    pixels.iter().filter(|p| **p == [255, 255]).count() > width * height / 2,
                    "{theme:?} {geometry:?} {time}: paper background"
                );
                for y in 0..height {
                    assert_eq!(pixels[y * width], [255, 255], "left edge {theme:?} at {time}");
                    assert_eq!(
                        pixels[y * width + width - 1],
                        [255, 255],
                        "right edge {theme:?} at {time}"
                    );
                }
                assert!(
                    frame.0[footer * width * 2..] == reference.0[footer * width * 2..],
                    "animated content invaded footer {theme:?} {time}"
                );
            }
        }
    }
}

#[test]
fn arbitrary_seek_loop_final_and_reduced_frames_are_time_based() {
    for geometry in [Geometry::Landscape, Geometry::Portrait] {
        for theme in StartupTheme::ALL {
            assert!(
                render(theme, geometry, 350, true, false).0 != render(theme, geometry, 1200, true, false).0,
                "{theme:?}"
            );
            assert!(render(theme, geometry, 350, true, false).0 == render(theme, geometry, 2350, true, false).0);
            assert!(render(theme, geometry, 1999, false, false).0 == render(theme, geometry, u64::MAX, false, false).0);
            assert!(render(theme, geometry, 0, true, true).0 == render(theme, geometry, u64::MAX, false, true).0);
        }
    }
}
