use dashboard_host_validation::acquisition::{AcquisitionModel, DashboardSnapshot};
use dashboard_host_validation::dashboard::{DashboardCache, DashboardScene};
use dashboard_host_validation::display::St7789Renderer;
use dashboard_host_validation::render::FpsMode;
use dashboard_host_validation::samples::{Quality, SensorId, Source};
use dashboard_host_validation::vehicle_status::VehicleStatus;
use dashboard_host_validation::widgets::cell_text;
use dashboard_host_validation::{AlignedFrame, colors};
use embedded_graphics::prelude::*;
use embedded_graphics::text::Text;

fn scene(
    snapshot: &DashboardSnapshot,
    now_ms: u64,
) -> DashboardScene<'_> {
    DashboardScene {
        layout: None,
        measurements: snapshot,
        status: VehicleStatus::Demo,
        now_ms,
        blink_on: false,
        boost_psi: false,
        fps_mode: FpsMode::Off,
        fps_instant: 0.0,
        fps_average: 0.0,
        backgrounds: [colors::BLACK; 8],
        overlay: None,
    }
}

fn values(lambda: f32) -> [f32; 8] { [1.0, 90.0, 95.0, 80.0, 30.0, 750.0, 13.8, lambda] }

fn row(
    frame: &AlignedFrame,
    top: usize,
    bottom: usize,
) -> Vec<u16> {
    (top..bottom)
        .flat_map(|y| (81..159).map(move |x| frame.pixel(x, y)))
        .collect()
}

#[test]
fn afr_is_derived_from_unrounded_lambda_on_the_labelled_gasoline_scale() {
    for (lambda, expected_lambda, expected_afr) in [
        (0.8, "0.800", "AFR 11.76"),
        (1.0, "1.000", "AFR 14.70"),
        (1.2, "1.200", "AFR 17.64"),
        (0.8002, "0.800", "AFR 11.76"),
        (0.8004, "0.800", "AFR 11.77"),
    ] {
        let text = cell_text::lambda(lambda);
        assert_eq!(text[0], expected_lambda);
        assert_eq!(text[1], expected_afr);
        assert_eq!(text[2], "EQ 14.7");
    }
}

#[test]
fn invalid_or_unrepresentable_lambda_never_produces_a_numeric_afr() {
    for lambda in [
        0.0,
        -0.0,
        -1.0,
        f32::NAN,
        f32::INFINITY,
        f32::NEG_INFINITY,
        f32::MAX,
        f32::MIN_POSITIVE,
        100.0,
    ] {
        let text = cell_text::lambda(lambda);
        assert_eq!(text[1], "AFR --", "lambda={lambda}");
    }
}

#[test]
fn afr_and_scale_are_rasterized_inside_the_lambda_cell() {
    let mut model = AcquisitionModel::demo();
    model.ingest_batch(values(0.8), 0);
    let snapshot = model.snapshot();
    let mut actual = AlignedFrame::boxed();
    actual.0.fill(0x5A);
    scene(&snapshot, 0).draw_cell(&mut St7789Renderer::new(&mut actual.0), 1);
    let mut expected = AlignedFrame::boxed();
    let mut renderer = St7789Renderer::new(&mut expected.0);
    for (text, baseline) in [("AFR 11.76", 84), ("EQ 14.7", 125)] {
        Text::with_text_style(
            text,
            Point::new(120, baseline),
            dashboard_host_validation::styles::LABEL_STYLE_WHITE,
            dashboard_host_validation::styles::CENTERED,
        )
        .draw(&mut renderer)
        .unwrap();
    }
    assert_eq!(row(&actual, 73, 91), row(&expected, 73, 91));
    assert_eq!(row(&actual, 115, 132), row(&expected, 115, 132));
    assert!(row(&actual, 73, 91).iter().any(|pixel| *pixel != 0));
    for y in 0..240 {
        for x in 0..320 {
            if !(81..159).contains(&x) || !(27..132).contains(&y) {
                assert_eq!(actual.pixel(x, y), 0x5A5A, "neighbor {x},{y}");
            }
        }
    }
}

#[test]
fn afr_only_rounding_change_redraws_both_retained_buffers() {
    let mut model = AcquisitionModel::demo();
    model.ingest_batch(values(0.8002), 0);
    let first = model.snapshot();
    let mut cache = DashboardCache::new();
    let mut frames = [AlignedFrame::boxed(), AlignedFrame::boxed()];
    for (buffer, frame) in frames.iter_mut().enumerate() {
        cache.draw(&mut St7789Renderer::new(&mut frame.0), buffer, &scene(&first, 0));
    }
    let old_afr = row(&frames[0], 73, 91);
    model.ingest_batch(values(0.8004), 1);
    let changed = model.snapshot();
    let mut reference = AlignedFrame::boxed();
    scene(&changed, 1).draw_full(&mut St7789Renderer::new(&mut reference.0));
    for (buffer, frame) in frames.iter_mut().enumerate() {
        let redraw = cache.draw(&mut St7789Renderer::new(&mut frame.0), buffer, &scene(&changed, 1));
        assert_eq!(
            redraw.cells,
            1 << 1,
            "AFR must dirty its cell despite unchanged lambda digits"
        );
        assert!(frame.0 == reference.0);
        assert_ne!(row(frame, 73, 91), old_afr);
        assert_eq!(cache.prepare(buffer, &scene(&changed, 1)).cells, 0);
    }
}

#[test]
fn afr_disappears_with_stale_invalid_faulted_or_unqualified_lambda() {
    let mut model = AcquisitionModel::demo();
    model.ingest_batch(values(0.8), 0);
    let fresh = model.snapshot();
    let mut cases = Vec::new();
    for quality in [
        Quality::Unknown,
        Quality::Unsupported,
        Quality::TransportError,
        Quality::InvalidPayload,
    ] {
        let mut snapshot = fresh;
        snapshot.metadata[SensorId::Lambda as usize].quality = quality;
        cases.push((snapshot, 0));
    }
    cases.push((fresh, 501));
    let mut wrong_source = fresh;
    wrong_source.metadata[SensorId::Lambda as usize].source = Source::Unknown;
    cases.push((wrong_source, 0));
    let mut future = fresh;
    future.metadata[SensorId::Lambda as usize].received_at_ms = Some(1);
    cases.push((future, 0));
    for (snapshot, now_ms) in cases {
        let mut cache = DashboardCache::new();
        let mut frames = [AlignedFrame::boxed(), AlignedFrame::boxed()];
        let mut reference = AlignedFrame::boxed();
        scene(&snapshot, now_ms).draw_full(&mut St7789Renderer::new(&mut reference.0));
        for (buffer, frame) in frames.iter_mut().enumerate() {
            cache.draw(&mut St7789Renderer::new(&mut frame.0), buffer, &scene(&fresh, 500));
            let live_afr = row(frame, 73, 91);
            let redraw = cache.draw(
                &mut St7789Renderer::new(&mut frame.0),
                buffer,
                &scene(&snapshot, now_ms),
            );
            assert_ne!(redraw.cells & (1 << 1), 0);
            assert!(frame.0 == reference.0);
            assert_ne!(row(frame, 73, 91), live_afr);
        }
    }
}
