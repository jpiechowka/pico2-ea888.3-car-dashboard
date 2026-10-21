//! Native shared-renderer frames and repeatable host timing samples.
//! Run with an output directory; convert PPM files losslessly to PNG.
//! Optional `--animation` adds forty 50ms samples per two-second cycle beneath
//! `animation/`, named theme-Geometry-0000.ppm through theme-Geometry-1950.ppm.
//! Example: cargo run --release --example startup_gallery -- output --animation
use std::fs;
use std::hint::black_box;
use std::path::PathBuf;
use std::time::Instant;

use dashboard_config::Geometry;
use dashboard_core::display::St7789Renderer;
use dashboard_core::startup::*;
use dashboard_core::vehicle_status::VehicleStatus;
use dashboard_host_validation::AlignedFrame;

/// Writes procedural-theme/geometry PPM samples and 30 host timing samples per case to the CLI directory.
/// Optional --animation adds 40 frames per cycle. Returns argument/I/O errors; existing files are replaced and errors
/// may leave partial output. Timing includes only shared host rendering, not target cycles, transfer or scanout.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args_os().skip(1);
    let directory = PathBuf::from(args.next().ok_or("expected output directory")?);
    let animation = match args.next() {
        None => false,
        Some(flag) if flag == "--animation" => true,
        Some(_) => return Err("expected --animation after directory".into()),
    };
    if args.next().is_some() {
        return Err("unexpected trailing argument".into());
    }
    if animation {
        fs::create_dir_all(directory.join("animation"))?;
    }
    fs::create_dir_all(&directory)?;
    let facts = StartupFacts {
        vehicle: VehicleStatus::Demo,
        config_ok: true,
        settings_loaded: true,
        configured_sources: 8,
    };
    let mut samples = String::from("theme,geometry,sample,ns_per_frame\n");
    for geometry in [Geometry::Landscape, Geometry::Portrait] {
        for theme in StartupTheme::ALL {
            let name = match theme {
                StartupTheme::Aezakmi => "aezakmi",
                StartupTheme::Turbo => "turbo",
                StartupTheme::Sanic => "sanic",
                StartupTheme::BoostBro => "boost-bro",
                StartupTheme::WhereLambo => "where-lambo",
                StartupTheme::MachineSpirit => "machine-spirit",
                StartupTheme::RitualDiagnostics => "ritual-diagnostics",
                StartupTheme::RedGoesFasta => "red-goes-fasta",
                StartupTheme::BoostPadawan => "boost-padawan",
                StartupTheme::TurboBrainlet => "turbo-brainlet",
                StartupTheme::Custom => unreachable!("procedural list excludes artwork"),
            };
            let profile = StartupProfile {
                theme,
                ..StartupProfile::DEFAULT
            };
            let mut frame = AlignedFrame::boxed();
            for time in [0, 350, 1200, 1999] {
                draw(
                    &mut St7789Renderer::with_geometry(&mut frame.0, geometry),
                    Phase::Splash,
                    profile,
                    time,
                    false,
                    facts,
                );
                write_frame(
                    directory.join(format!("{name}-{geometry:?}-{time}.ppm")),
                    geometry,
                    &frame,
                )?;
            }
            if animation {
                for time in (0..2000).step_by(50) {
                    draw(
                        &mut St7789Renderer::with_geometry(&mut frame.0, geometry),
                        Phase::Splash,
                        profile,
                        time,
                        false,
                        facts,
                    );
                    write_frame(
                        directory
                            .join("animation")
                            .join(format!("{name}-{geometry:?}-{time:04}.ppm")),
                        geometry,
                        &frame,
                    )?;
                }
            }
            for sample in 0..30 {
                let start = Instant::now();
                for time in 0..100 {
                    draw(
                        &mut St7789Renderer::with_geometry(black_box(&mut frame.0), geometry),
                        Phase::Splash,
                        profile,
                        time * 20,
                        false,
                        facts,
                    );
                    black_box(&frame.0);
                }
                samples.push_str(&format!(
                    "{name},{geometry:?},{sample},{}\n",
                    start.elapsed().as_nanos() / 100
                ));
            }
        }
    }
    fs::write(directory.join("timings.csv"), samples)?;
    Ok(())
}

/// Writes RGB888 P6 pixels from big-endian RGB565 `frame` to `path`, declaring caller-matched `geometry` dimensions.
/// Returns filesystem errors; replaces an existing file and may leave partial bytes on failure.
fn write_frame(
    path: PathBuf,
    geometry: Geometry,
    frame: &AlignedFrame,
) -> std::io::Result<()> {
    let mut ppm = format!("P6\n{} {}\n255\n", geometry.width(), geometry.height()).into_bytes();
    for pixel in frame.0.as_chunks::<2>().0 {
        ppm.extend_from_slice(&dashboard_config::Color(u16::from_be_bytes(*pixel)).rgb888());
    }
    fs::write(path, ppm)
}
