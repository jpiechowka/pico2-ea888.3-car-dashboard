//! Synthetic documentation pixels, not vehicle readings or recommended limits.
//! From the repository root:
//! `cargo run --manifest-path host/Cargo.toml --example widget_gallery -- target/widget-gallery`
//! Convert the emitted PPMs losslessly to PNG; keep their native dimensions.
use std::fs::{self, File};
use std::io::{BufWriter, Write};
use std::path::Path;

use dashboard_config::{
    Channel,
    Color,
    GaugeKind,
    MAX_FILE_BYTES,
    PixelRect,
    Placement,
    Readout,
    Secondary,
    Span,
    default_config,
    default_widget,
    encode,
    validate,
};
use dashboard_host_validation::acquisition::AcquisitionModel;
use dashboard_host_validation::configured_scene::Layout;
use dashboard_host_validation::dashboard::DashboardScene;
use dashboard_host_validation::display::St7789Renderer;
use dashboard_host_validation::presentation::PresentationState;
use dashboard_host_validation::render::FpsMode;
use dashboard_host_validation::vehicle_status::VehicleStatus;
use dashboard_host_validation::{AlignedFrame, colors};

/// Final canonical sensor-order values: bar, five Celsius channels, volts and lambda.
const VALUES: [f32; 8] = [1.20, 95., 90., 80., 30., 700., 13.8, 1.00];
/// Synthetic sawtooth amplitudes in the same canonical units as VALUES.
const AMPLITUDES: [f32; 8] = [0.30, 6., 5., 5., 4., 50., 0.4, 0.04];
/// Final boot-relative observation time in milliseconds; its sawtooth phase is zero.
const NOW_MS: u64 = 118_000;

/// Emits eight channel and 18 gauge/span cases to the sole CLI directory, with full/cropped PPMs, configs and coverage
/// TSV. Returns argument/I/O errors; replaces existing named files and can leave partial output on failure.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args_os().skip(1);
    let directory = args.next().ok_or("expected output directory")?;
    if args.next().is_some() {
        return Err("expected only one output directory".into());
    }
    let directory = Path::new(&directory);
    fs::create_dir_all(directory.join("full"))?;
    fs::create_dir_all(directory.join("configs"))?;
    let mut manifest = BufWriter::new(File::create(directory.join("coverage.tsv"))?);
    writeln!(
        manifest,
        "stem\tchannel\tstyle\tspan\tx\ty\twidth\theight\tcanonical_value"
    )?;
    for (channel, name) in Channel::ALL.into_iter().zip([
        "boost",
        "oil",
        "coolant",
        "dsg",
        "iat",
        "egt",
        "ecu-voltage",
        "lambda-afr",
    ]) {
        render(
            directory,
            &mut manifest,
            &format!("widget-{name}"),
            channel,
            GaugeKind::Numeric,
            Span::TwoByOne,
        )?;
    }
    for (kind, name) in
        GaugeKind::ALL
            .into_iter()
            .zip(["numeric", "circular", "needle", "horizontal", "vertical", "segmented"])
    {
        for (span, size) in [
            (Span::OneByOne, "1x1"),
            (Span::TwoByOne, "2x1"),
            (Span::TwoByTwo, "2x2"),
        ] {
            render(
                directory,
                &mut manifest,
                &format!("gauge-{name}-{size}"),
                Channel::Boost,
                kind,
                span,
            )?;
        }
    }
    manifest.flush()?;
    Ok(())
}

/// Writes `stem` config/full/cropped images under `directory` and appends one row to `manifest` for
/// `channel`/`kind`/`span`. Returns I/O errors after any earlier writes; invalid fixtures panic. Uses admitted 50ms
/// synthetic history with warnings disabled.
fn render(
    directory: &Path,
    manifest: &mut impl Write,
    stem: &str,
    channel: Channel,
    kind: GaugeKind,
    span: Span,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut raw = default_config();
    raw.views[0].name = "SYNTHETIC".try_into().unwrap();
    raw.widgets.clear();
    let mut widget = default_widget(
        channel,
        1,
        1,
        Placement {
            column: 0,
            row: 0,
            span,
        },
    );
    widget.warning.enabled = false;
    widget.gauge.kind = kind;
    widget.display.graph = true;
    widget.display.average = true;
    widget.display.extrema = true;
    widget.display.trend = true;
    if channel == Channel::Boost {
        widget.advanced.readout = Readout::Bar;
        widget.advanced.secondary = Secondary::Psi;
    }
    let rect = widget.placement.rect().unwrap();
    raw.widgets.push(widget).unwrap();
    let config = validate(raw).expect("gallery fixture must remain valid");
    let mut encoded = vec![0; MAX_FILE_BYTES];
    fs::write(
        directory.join("configs").join(format!("{stem}.eav")),
        encode(&config, &mut encoded).unwrap(),
    )?;
    let mut model = AcquisitionModel::demo_configured(&config);
    // Fifty-millisecond synthetic observations preserve actual freshness/history semantics.
    // The final sawtooth phase is zero, so the visible sample is exactly VALUES.
    for step in 0..=NOW_MS / 50 {
        let phase = ((step % 80) as f32 - 40.) / 40.;
        let values = core::array::from_fn(|i| VALUES[i] + AMPLITUDES[i] * phase);
        assert!(model.ingest_batch(values, step * 50));
    }
    let snapshot = model.snapshot();
    let mut presentation = PresentationState::new();
    let readings = presentation.prepare(&config, &snapshot, 1, NOW_MS, false);
    let scene = DashboardScene {
        readings: Some(&readings),
        layout: Layout::new(&config, 1, false),
        measurements: &snapshot,
        now_ms: NOW_MS,
        status: VehicleStatus::Demo,
        blink_on: false,
        boost_psi: false,
        fps_mode: FpsMode::Off,
        fps_instant: 0.,
        fps_average: 0.,
        backgrounds: [colors::BLACK; 8],
        overlay: None,
    };
    let mut frame = AlignedFrame::boxed();
    scene.draw_full(&mut St7789Renderer::new(&mut frame.0));
    ppm(
        &directory.join("full").join(format!("{stem}.ppm")),
        &frame,
        PixelRect {
            x: 0,
            y: 0,
            width: 320,
            height: 240,
        },
    )?;
    ppm(&directory.join(format!("{stem}.ppm")), &frame, rect)?;
    writeln!(
        manifest,
        "{stem}\t{channel:?}\t{kind:?}\t{span:?}\t{}\t{}\t{}\t{}\t{}",
        rect.x,
        rect.y,
        rect.width,
        rect.height,
        VALUES[channel.index()]
    )?;
    Ok(())
}

/// Writes caller-bounded landscape `rect` from `frame` as RGB888 P6 to `path`; coordinates are not clipped.
/// Returns create/write/flush errors, possibly after partial replacement; invalid coordinates can panic.
fn ppm(
    path: &Path,
    frame: &AlignedFrame,
    rect: PixelRect,
) -> std::io::Result<()> {
    let mut output = BufWriter::new(File::create(path)?);
    write!(output, "P6\n{} {}\n255\n", rect.width, rect.height)?;
    for y in rect.y..rect.y + rect.height {
        for x in rect.x..rect.x + rect.width {
            output.write_all(&Color(frame.pixel(usize::from(x), usize::from(y))).rgb888())?;
        }
    }
    output.flush()
}
