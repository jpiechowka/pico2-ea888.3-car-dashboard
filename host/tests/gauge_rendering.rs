use dashboard_host_validation::acquisition::AcquisitionModel;
use dashboard_host_validation::configured_scene::Layout;
use dashboard_host_validation::dashboard::{DashboardCache, DashboardScene};
use dashboard_host_validation::display::St7789Renderer;
use dashboard_host_validation::render::FpsMode;
use dashboard_host_validation::view_config::*;
use dashboard_host_validation::{AlignedFrame, colors};

#[test]
fn prepared_subpixel_gauge_motion_keeps_both_buffers_clean() {
    use dashboard_host_validation::presentation::PresentationState;
    for kind in GaugeKind::ALL.into_iter().filter(|k| !k.is_numeric()) {
        let mut raw = default_config();
        raw.widgets.truncate(1);
        let w = &mut raw.widgets[0];
        w.gauge.kind = kind;
        w.gauge.expand = false;
        w.display.decimals = 0;
        w.display.extrema = false;
        w.display.average = false;
        w.display.graph = false;
        w.display.trend = false;
        w.advanced.smoothing_ms = 5000;
        w.advanced.peak_mode = PeakSelection::Off;
        w.advanced.reference_mode = ReferenceMode::Zero;
        w.advanced
            .set_bands(&[
                Band::new(-1., 0., Color(0x07E0), "LOW").unwrap(),
                Band::new(0., 1., Color(0x07FF), "MID").unwrap(),
                Band::new(1., 3., Color(0xF81F), "HIGH").unwrap(),
            ])
            .unwrap();
        let config = validate(raw).unwrap();
        let mut model = AcquisitionModel::demo_configured(&config);
        let mut presentation = PresentationState::new();
        let mut cache = DashboardCache::new();
        let mut frames = [AlignedFrame::boxed(), AlignedFrame::boxed()];
        let mut previous = None;
        for (now, value) in [(0, 1.), (100, 1.001)] {
            model.ingest_batch([value; 8], now);
            let snapshot = model.snapshot();
            let readings = presentation.prepare(&config, &snapshot, 1, now, false);
            let mut scene = scene(&config, &snapshot, now);
            scene.readings = Some(&readings);
            let mut full = AlignedFrame::boxed();
            scene.draw_full(&mut St7789Renderer::new(&mut full.0));
            if let Some(previous) = &previous {
                assert_eq!(&full.0[..], previous);
            }
            for (slot, frame) in frames.iter_mut().enumerate() {
                let redraw = cache.draw(&mut St7789Renderer::new(&mut frame.0), slot, &scene);
                assert!(frame.0 == full.0);
                if now > 0 {
                    assert_eq!(redraw.cells, 0, "redundant draw {kind:?}/{slot}");
                }
            }
            previous = Some(full.0.to_vec());
        }
    }
}

// These pixels catch missing production marker integration, smoothed peaks,
// unsupported numeric decoration and writes beyond the owning cell.
#[test]
fn advanced_gauge_markers_use_prepared_raw_peaks_and_static_bands_in_every_span() {
    use dashboard_host_validation::presentation::PresentationState;
    let mut golden = Vec::new();
    for kind in GaugeKind::ALL {
        for span in [Span::OneByOne, Span::TwoByOne, Span::TwoByTwo] {
            let mut raw = default_config();
            raw.widgets.clear();
            let mut w = default_widget(
                Channel::Boost,
                1,
                1,
                Placement {
                    column: 2,
                    row: 0,
                    span,
                },
            );
            w.gauge.kind = kind;
            w.display.average = false;
            w.display.extrema = false;
            w.display.graph = false;
            w.advanced.smoothing_ms = 5000;
            w.advanced.peak_mode = PeakSelection::Maximum;
            w.advanced.reference_mode = ReferenceMode::Manual;
            w.advanced.reference = 1.;
            w.advanced
                .set_bands(&[
                    Band::new(-1., 0., Color(0x07E0), "LOW").unwrap(),
                    Band::new(0., 1., Color(0x07FF), "MID").unwrap(),
                    Band::new(1., 3., Color(0xF81F), "HIGH").unwrap(),
                ])
                .unwrap();
            let rect = w.placement.rect().unwrap();
            raw.widgets.push(w).unwrap();
            let config = validate(raw).unwrap();
            let mut model = AcquisitionModel::demo_configured(&config);
            let mut presentation = PresentationState::new();
            model.ingest_batch([0.; 8], 0);
            presentation.prepare(&config, &model.snapshot(), 1, 0, false);
            model.ingest_batch([2.; 8], 100);
            let snapshot = model.snapshot();
            let mut cache = DashboardCache::new();
            let mut frames = [AlignedFrame::boxed(), AlignedFrame::boxed()];
            for now in [100, 4000] {
                let prepared = presentation.prepare(&config, &snapshot, 1, now, false);
                let mut scene = scene(&config, &snapshot, now);
                scene.readings = Some(&prepared);
                if now == 100 {
                    assert!(prepared.widget(0).unwrap().value.unwrap() < 0.1);
                }
                let mut full = AlignedFrame::boxed();
                scene.draw_full(&mut St7789Renderer::new(&mut full.0));
                for (i, frame) in frames.iter_mut().enumerate() {
                    cache.draw(&mut St7789Renderer::new(&mut frame.0), i, &scene);
                    assert!(frame.0 == full.0, "{kind:?}/{span:?}/{now}");
                }
                let mut cell = AlignedFrame::boxed();
                cell.0.fill(0x5A);
                scene.draw_cell(&mut St7789Renderer::new(&mut cell.0), 0);
                for y in 0..240 {
                    for x in 0..320 {
                        if x < rect.x as usize
                            || x >= (rect.x + rect.width) as usize
                            || y < rect.y as usize
                            || y >= (rect.y + rect.height) as usize
                        {
                            assert_eq!(cell.pixel(x, y), 0x5A5A);
                        }
                    }
                }
                for color in [0x07E0_u16, 0x07FF, 0xF81F] {
                    let seen = cell.0.chunks_exact(2).any(|p| p == color.to_be_bytes());
                    assert_eq!(seen, !kind.is_numeric(), "band {color:x} {kind:?}/{span:?}/{now}");
                }
                let peak_seen = cell.0.chunks_exact(2).any(|p| p == 0xFFE0_u16.to_be_bytes());
                assert_eq!(
                    peak_seen,
                    !kind.is_numeric() && now == 100,
                    "raw peak {kind:?}/{span:?}/{now}"
                );
                let mut pixels = Vec::new();
                for y in rect.y..rect.y + rect.height {
                    for x in rect.x..rect.x + rect.width {
                        pixels.push(cell.pixel(x as usize, y as usize));
                    }
                }
                golden.extend_from_slice(&rle_pixels(&pixels));
                if let Some(directory) = std::env::var_os("GAUGE_EVIDENCE_DIR") {
                    use std::io::Write;
                    let directory = std::path::PathBuf::from(directory);
                    std::fs::create_dir_all(&directory).unwrap();
                    let mut file =
                        std::fs::File::create(directory.join(format!("{kind:?}-{span:?}-{now}.ppm"))).unwrap();
                    write!(file, "P6\n{} {}\n255\n", rect.width, rect.height).unwrap();
                    for pixel in pixels {
                        file.write_all(&Color(pixel).rgb888()).unwrap();
                    }
                }
            }
        }
    }
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/gauge-markers.rle");
    if std::env::var_os("BLESS_GAUGE_GOLDENS").is_some() {
        std::fs::write(&path, &golden).unwrap();
    }
    assert_eq!(
        golden,
        std::fs::read(path).unwrap(),
        "reviewed lossless RGB565 gauge matrix"
    );
}

fn rle_pixels(pixels: &[u16]) -> Vec<u8> {
    let mut output = Vec::new();
    let mut start = 0;
    while start < pixels.len() {
        let mut end = start + 1;
        while end < pixels.len() && pixels[end] == pixels[start] && end - start < u16::MAX as usize {
            end += 1;
        }
        output.extend_from_slice(&((end - start) as u16).to_le_bytes());
        output.extend_from_slice(&pixels[start].to_be_bytes());
        start = end;
    }
    output
}

#[test]
fn wide_compact_gauges_give_short_values_larger_complete_digits() {
    use dashboard_host_validation::widgets::gauge::{Reading, draw};
    use embedded_graphics::prelude::*;
    use embedded_graphics::primitives::Rectangle;
    for kind in GaugeKind::ALL.into_iter().filter(|kind| !kind.is_numeric()) {
        let count = |width, text| {
            let mut frame = AlignedFrame::boxed();
            draw(
                &mut St7789Renderer::new(&mut frame.0),
                kind,
                Rectangle::new(Point::new(2, 40), Size::new(width, 72)),
                Reading {
                    min: -1.,
                    max: 3.,
                    value: Some(1.),
                    peak: None,
                },
                text,
                false,
                (colors::YELLOW, colors::RED, colors::GRAY, colors::WHITE),
            );
            frame
                .0
                .chunks_exact(2)
                .filter(|p| *p == 0xFFE0_u16.to_be_bytes())
                .count()
        };
        assert!(count(156, "0.04") > count(76, "0.04") * 3 / 2, "wide digits {kind:?}");
        // Compare every value-colour pixel in the independent readout lane.
        // A Dial hub cannot satisfy this complete-string oracle.
        use embedded_graphics::mono_font::MonoTextStyle;
        use embedded_graphics::mono_font::ascii::FONT_4X6;
        use embedded_graphics::text::{Baseline, Text, TextStyleBuilder};
        let text = "-1000000.000C";
        let mut actual = AlignedFrame::boxed();
        draw(
            &mut St7789Renderer::new(&mut actual.0),
            kind,
            Rectangle::new(Point::new(2, 40), Size::new(156, 72)),
            Reading {
                min: -1.,
                max: 3.,
                value: Some(1.),
                peak: None,
            },
            text,
            false,
            (colors::YELLOW, colors::RED, colors::GRAY, colors::WHITE),
        );
        let mut expected = AlignedFrame::boxed();
        // 13 four-pixel glyphs + two outline pixels =54, centered in78.
        Text::with_text_style(
            text,
            Point::new(93, 54),
            MonoTextStyle::new(&FONT_4X6, colors::YELLOW),
            TextStyleBuilder::new().baseline(Baseline::Top).build(),
        )
        .draw(&mut St7789Renderer::new(&mut expected.0))
        .unwrap();
        let mut glyphs = 0;
        for y in 53..89 {
            for x in 80..158 {
                let wanted = expected.pixel(x, y) == 0xFFE0;
                glyphs += usize::from(wanted);
                assert_eq!(actual.pixel(x, y) == 0xFFE0, wanted, "full readout {kind:?}/{x}/{y}");
            }
        }
        assert!(glyphs > 0);
    }
}

#[test]
fn reference_peak_and_adjacent_bands_share_affine_geometry_and_omit_outside_values() {
    use dashboard_host_validation::presentation::Unit;
    use dashboard_host_validation::widgets::gauge::{Decorations, Reading, draw_decorated};
    use embedded_graphics::prelude::*;
    use embedded_graphics::primitives::Rectangle;
    for kind in GaugeKind::ALL.into_iter().filter(|k| !k.is_numeric()) {
        let mut reference_pixels = None;
        for (unit, min, max, peak) in [
            (Unit::Celsius, -1., 1., 0.5),
            (Unit::Fahrenheit, 30.2, 33.8, 32.9),
            (Unit::Bar, -1., 1., 0.5),
            (Unit::Kpa, -100., 100., 50.),
            (Unit::Psi, -14.503774, 14.503774, 7.251887),
            (Unit::Volts, -1., 1., 0.5),
            (Unit::Lambda, -1., 1., 0.5),
            (Unit::Afr, -14.7, 14.7, 7.35),
        ] {
            let mut advanced = AdvancedDisplay::for_channel(Channel::Boost, false);
            advanced.reference_mode = ReferenceMode::Zero;
            advanced
                .set_bands(&[
                    Band::new(-2., 0., Color(0x07E0), "LEFT").unwrap(),
                    Band::new(0., 0.5, Color(0x07FF), "MID").unwrap(),
                    Band::new(0.5, 2., Color(0xF81F), "RIGHT").unwrap(),
                ])
                .unwrap();
            let draw = |advanced: &AdvancedDisplay, peak| {
                let mut frame = AlignedFrame::boxed();
                draw_decorated(
                    &mut St7789Renderer::new(&mut frame.0),
                    kind,
                    Rectangle::new(Point::new(10, 30), Size::new(100, 80)),
                    Reading {
                        min,
                        max,
                        value: None,
                        peak,
                    },
                    "--",
                    false,
                    (colors::WHITE, colors::RED, colors::GRAY, colors::WHITE),
                    Some(Decorations {
                        settings: advanced,
                        unit,
                        stoich: 14.7,
                        peak_color: colors::YELLOW,
                    }),
                );
                frame
            };
            let frame = draw(&advanced, Some(peak));
            // Compare only the graphic lane: numeric endpoint units differ.
            let graphic = frame.0[30 * 640..78 * 640].to_vec();
            if let Some(expected) = &reference_pixels {
                assert_eq!(&graphic, expected, "{kind:?}/{unit:?}");
            }
            reference_pixels = Some(graphic);
            advanced.reference_mode = ReferenceMode::Off;
            let absent = draw(&advanced, None);
            for value in [-2., 2.] {
                advanced.reference_mode = ReferenceMode::Manual;
                advanced.reference = value;
                let clipped = draw(&advanced, Some(unit.value(value, 14.7)));
                assert!(absent.0 == clipped.0, "out-of-range glyph {kind:?}/{unit:?}/{value}");
            }
        }
    }
}

#[test]
fn peak_only_expiry_and_reset_repair_both_buffers_and_raw_lookup_uses_global_rule() {
    use dashboard_host_validation::presentation::PresentationState;
    let mut raw = default_config();
    raw.widgets.truncate(1);
    raw.widgets[0].advanced.peak_mode = PeakSelection::Minimum;
    raw.views
        .push(View {
            id: 2,
            name: "PEAK".try_into().unwrap(),
        })
        .unwrap();
    let mut w = raw.widgets[0].clone();
    w.id = 100;
    w.view = 2;
    w.gauge.kind = GaugeKind::Horizontal;
    w.gauge.expand = true;
    w.display.average = false;
    w.display.extrema = false;
    w.advanced.peak_mode = PeakSelection::Maximum;
    w.advanced.peak_hold_ms = 200;
    w.advanced.reference_mode = ReferenceMode::Manual;
    w.advanced.reference = 9.;
    raw.widgets.push(w).unwrap();
    let cfg = validate(raw).unwrap();
    let mut model = AcquisitionModel::demo_configured(&cfg);
    model.ingest_batch([5.; 8], 0);
    model.ingest_batch([1.; 8], 100);
    let mut state = PresentationState::new();
    let mut cache = DashboardCache::new();
    let mut frames = [AlignedFrame::boxed(), AlignedFrame::boxed()];
    let mut previous = None;
    for (now, reset, max, peak) in [
        (100, false, 5., Some(5.)),
        (200, false, 5., None),
        (201, true, 3., None),
    ] {
        model.tick(now);
        if reset {
            model.reset_statistics(1);
        }
        let snapshot = model.snapshot();
        let prepared = state.prepare(&cfg, &snapshot, 2, now, false);
        let mut scene = scene(&cfg, &snapshot, now);
        scene.layout = Layout::new(&cfg, 2, false);
        // View-local slot zero refers to global rule one, whose mode is Maximum.
        assert_eq!(scene.gauge_reading(0).unwrap().peak, peak);
        scene.readings = Some(&prepared);
        let reading = scene.gauge_reading(0).unwrap();
        assert_eq!((reading.min, reading.max, reading.peak), (-1., max, peak));
        let mut full = AlignedFrame::boxed();
        scene.draw_full(&mut St7789Renderer::new(&mut full.0));
        if let Some(old) = previous {
            assert_ne!(full.0.to_vec(), old, "visible peak/reset transition");
        }
        previous = Some(full.0.to_vec());
        for (index, frame) in frames.iter_mut().enumerate() {
            assert_ne!(
                cache.draw(&mut St7789Renderer::new(&mut frame.0), index, &scene).cells & 1,
                0
            );
            assert!(frame.0 == full.0);
        }
    }
}

#[test]
fn widget_clears_each_background_once_and_preserves_warning_inset() {
    use embedded_graphics::pixelcolor::Rgb565;
    use embedded_graphics::prelude::*;
    use embedded_graphics::primitives::Rectangle;

    struct Tracked<'a> {
        renderer: St7789Renderer<'a>,
        fills: Vec<(Rectangle, Rgb565)>,
    }
    impl OriginDimensions for Tracked<'_> {
        fn size(&self) -> Size { self.renderer.size() }
    }
    impl DrawTarget for Tracked<'_> {
        type Color = Rgb565;
        type Error = core::convert::Infallible;

        fn draw_iter<I: IntoIterator<Item = Pixel<Rgb565>>>(
            &mut self,
            pixels: I,
        ) -> Result<(), Self::Error> {
            self.renderer.draw_iter(pixels)
        }

        fn fill_solid(
            &mut self,
            area: &Rectangle,
            color: Rgb565,
        ) -> Result<(), Self::Error> {
            self.fills.push((*area, color));
            self.renderer.fill_solid(area, color)
        }
    }

    for span in [Span::OneByOne, Span::TwoByOne, Span::TwoByTwo] {
        let mut raw = default_config();
        raw.widgets.clear();
        let mut widget = default_widget(
            Channel::Oil,
            1,
            1,
            Placement {
                column: 0,
                row: 0,
                span,
            },
        );
        widget.colors.background = Color::rgb(0, 16, 32);
        widget.warning.on_ms = 1;
        raw.widgets.push(widget).unwrap();
        let config = validate(raw).unwrap();
        let widget = &config.config().widgets[0];
        let rect = widget.placement.rect().unwrap();
        let outer = Rectangle::new(Point::new(0, 26), Size::new(rect.width.into(), rect.height.into()));
        let interior = Rectangle::new(Point::new(2, 28), outer.size - Size::new(4, 4));
        let mut model = AcquisitionModel::demo_configured(&config);
        for now in [0, 1] {
            model.ingest_batch([150.; 8], now);
        }
        let snapshot = model.snapshot();
        for (now, blink, warning) in [
            (1, false, false),
            (2, true, true),
            (3, false, false),
            (4000, true, false),
        ] {
            let mut scene = scene(&config, &snapshot, now);
            scene.blink_on = blink;
            assert_eq!(
                scene.layout.as_ref().unwrap().alarm_active(0, &snapshot, now) && blink,
                warning
            );
            let mut frame = AlignedFrame::boxed();
            frame.0.fill(0x5A);
            let mut target = Tracked {
                renderer: St7789Renderer::new(&mut frame.0),
                fills: Vec::new(),
            };
            scene.draw_cell(&mut target, 0);
            let large: Vec<_> = target
                .fills
                .iter()
                .filter(|(area, _)| area.size.width > 60 && area.size.height > 60)
                .copied()
                .collect();
            let color = |color: Color| Rgb565::from(embedded_graphics::pixelcolor::raw::RawU16::new(color.0));
            let mut expected = vec![(outer, color(widget.colors.background))];
            if warning {
                expected.push((interior, color(widget.colors.warning.background)));
            }
            assert_eq!(large, expected, "{span:?}, now={now}");
            // Independent blank-pixel oracle: warning changes only the inset,
            // the normal background gutter and adjacent widget stay intact.
            assert_eq!(frame.pixel(1, 130), widget.colors.background.0);
            assert_eq!(
                frame.pixel(3, 130),
                if warning {
                    widget.colors.warning.background.0
                } else {
                    widget.colors.background.0
                }
            );
            assert_eq!(frame.pixel(usize::from(rect.width), 130), 0x5A5A);
        }
    }
}

#[test]
fn widget_background_palette_is_rendered_and_invalidated_on_both_buffers() {
    let config = validate(default_config()).unwrap();
    let model = AcquisitionModel::demo();
    let snapshot = model.snapshot();
    let mut frames = [AlignedFrame::boxed(), AlignedFrame::boxed()];
    let mut cache = DashboardCache::new();
    let mut raw = config.config().clone();
    raw.widgets[0].colors.background = Color::rgb(0, 16, 32);
    let custom = validate(raw).unwrap();
    for config in [&config, &custom] {
        let scene = scene(config, &snapshot, 0);
        let mut full = AlignedFrame::boxed();
        scene.draw_full(&mut St7789Renderer::new(&mut full.0));
        // Blank interior pixel is independent of text and of unavailable status.
        let pixel = (130 * 320 + 3) * 2;
        assert_eq!(
            &full.0[pixel..pixel + 2],
            &config.config().widgets[0].colors.background.0.to_be_bytes()
        );
        for (i, frame) in frames.iter_mut().enumerate() {
            cache.draw(&mut St7789Renderer::new(&mut frame.0), i, &scene);
            assert!(full.0 == frame.0);
        }
    }
}

#[test]
fn indicator_level_has_independent_endpoint_and_failure_oracles() {
    use dashboard_host_validation::widgets::gauge::Reading;
    for (value, expected) in [
        (-100.0, 0),
        (-1.0, 0),
        (0.0, 256),
        (1.0, 512),
        (3.0, 1024),
        (1_000_000.0, 1024),
    ] {
        assert_eq!(
            Reading {
                peak: None,
                min: -1.0,
                max: 3.0,
                value: Some(value)
            }
            .level(),
            Some(expected)
        );
    }
    for value in [None, Some(f32::NAN), Some(f32::INFINITY)] {
        assert_eq!(
            Reading {
                peak: None,
                min: 0.0,
                max: 1.0,
                value
            }
            .level(),
            None
        );
    }
    for (min, max) in [(0.0, 0.0), (1.0, 0.0), (f32::NAN, 1.0), (0.0, f32::INFINITY)] {
        assert_eq!(
            Reading {
                peak: None,
                min,
                max,
                value: Some(0.0)
            }
            .level(),
            None
        );
    }
}

#[test]
fn gauge_matrix_fits_every_channel_span_and_value_and_matches_retained_frames() {
    for channel in Channel::ALL {
        for span in [Span::OneByOne, Span::TwoByOne, Span::TwoByTwo] {
            for kind in GaugeKind::ALL.into_iter().filter(|kind| !kind.is_numeric()) {
                let mut raw = default_config();
                raw.widgets.clear();
                let mut widget = default_widget(
                    channel,
                    1,
                    1,
                    Placement {
                        column: 2,
                        row: 0,
                        span,
                    },
                );
                widget.gauge.kind = kind;
                widget.gauge.expand = true;
                widget.display.average = true;
                widget.display.extrema = true;
                widget.display.trend = true;
                widget.display.decimals = 3;
                let min = widget.gauge.min;
                let max = widget.gauge.max;
                let rect = widget.placement.rect().unwrap();
                raw.widgets.push(widget).unwrap();
                let config = validate(raw).unwrap();
                let mut model = AcquisitionModel::demo();
                let mut cache = DashboardCache::new();
                let mut frames = [AlignedFrame::boxed(), AlignedFrame::boxed()];
                for (at, value) in [-1_000_000.0, min, (min + max) / 2.0, max, 1_000_000.0]
                    .into_iter()
                    .enumerate()
                {
                    model.ingest_batch([value; 8], at as u64);
                    let snapshot = model.snapshot();
                    for stale in [false, true] {
                        let mut scene = scene(&config, &snapshot, if stale { 4000 + at as u64 } else { at as u64 });
                        scene.boost_psi = at % 2 == 0;
                        let mut full = AlignedFrame::boxed();
                        scene.draw_full(&mut St7789Renderer::new(&mut full.0));
                        for (i, frame) in frames.iter_mut().enumerate() {
                            cache.draw(&mut St7789Renderer::new(&mut frame.0), i, &scene);
                            assert!(full.0 == frame.0, "{channel:?}/{span:?}/{kind:?}/{value}/{stale}");
                        }
                        let mut owned = AlignedFrame::boxed();
                        owned.0.fill(0x5A);
                        scene.draw_cell(&mut St7789Renderer::new(&mut owned.0), 0);
                        for row in 0..240usize {
                            let start = row * 640;
                            let bytes = &owned.0[start..start + 640];
                            if (usize::from(rect.y)..usize::from(rect.y + rect.height)).contains(&row) {
                                assert!(bytes[..usize::from(rect.x) * 2].iter().all(|b| *b == 0x5A));
                                assert!(bytes[usize::from(rect.x + rect.width) * 2..].iter().all(|b| *b == 0x5A));
                            } else {
                                assert!(bytes.iter().all(|b| *b == 0x5A));
                            }
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn hidden_statistics_and_trend_do_not_dirty_a_compact_gauge() {
    let mut raw = default_config();
    raw.widgets.clear();
    let mut widget = default_widget(
        Channel::Oil,
        1,
        1,
        Placement {
            column: 0,
            row: 0,
            span: Span::OneByOne,
        },
    );
    widget.gauge.kind = GaugeKind::Vertical;
    raw.widgets.push(widget).unwrap();
    let config = validate(raw).unwrap();
    let mut model = AcquisitionModel::demo();
    model.ingest_batch([80.0; 8], 0);
    model.ingest_batch([90.0; 8], 1);
    let snapshot = model.snapshot();
    let mut cache = DashboardCache::new();
    let first = scene(&config, &snapshot, 1);
    cache.prepare(0, &first);
    cache.prepare(1, &first);
    for now in [751, 2002] {
        let later = scene(&config, &snapshot, now);
        for buffer in 0..2 {
            assert_eq!(cache.prepare(buffer, &later).cells, 0);
        }
    }
}

#[test]
fn unavailable_gauge_unit_toggle_and_reset_repair_both_buffers() {
    let mut raw = default_config();
    raw.widgets.truncate(1);
    raw.widgets[0].gauge.kind = GaugeKind::Circular;
    raw.widgets[0].gauge.expand = true;
    let config = validate(raw).unwrap();
    let mut model = AcquisitionModel::demo();
    model.ingest_batch([5.0; 8], 0);
    model.ingest_batch([1.0; 8], 1);
    let mut cache = DashboardCache::new();
    let mut frames = [AlignedFrame::boxed(), AlignedFrame::boxed()];
    for (psi, reset) in [(false, false), (true, false), (true, true), (false, false)] {
        if reset {
            model.reset_statistics(1);
        }
        let snapshot = model.snapshot();
        let mut scene = scene(&config, &snapshot, 4000);
        scene.boost_psi = psi;
        let mut full = AlignedFrame::boxed();
        scene.draw_full(&mut St7789Renderer::new(&mut full.0));
        for (i, frame) in frames.iter_mut().enumerate() {
            cache.draw(&mut St7789Renderer::new(&mut frame.0), i, &scene);
            assert!(full.0 == frame.0, "stale psi={psi} reset={reset}");
        }
    }
}

fn scene<'a>(
    config: &'a ValidatedConfig,
    snapshot: &'a dashboard_host_validation::acquisition::DashboardSnapshot,
    now: u64,
) -> DashboardScene<'a> {
    DashboardScene {
        readings: None,
        layout: Layout::new(config, 1, false),
        measurements: snapshot,
        status: dashboard_host_validation::vehicle_status::VehicleStatus::Demo,
        now_ms: now,
        blink_on: false,
        boost_psi: false,
        fps_mode: FpsMode::Off,
        fps_instant: 0.0,
        fps_average: 0.0,
        backgrounds: [colors::BLACK; 8],
        overlay: None,
    }
}

#[test]
fn every_gauge_draws_a_distinct_indicator_and_repairs_both_buffers() {
    let mut model = AcquisitionModel::demo();
    model.ingest_batch([1.1, 80.0, 90.0, 75.0, 30.0, 700.0, 13.8, 0.9], 0);
    let snapshot = model.snapshot();
    let mut pictures = Vec::new();
    let mut cache = DashboardCache::new();
    let mut buffers = [AlignedFrame::boxed(), AlignedFrame::boxed()];
    for kind in GaugeKind::ALL {
        let mut raw = default_config();
        raw.widgets.truncate(1);
        raw.widgets[0].gauge.kind = kind;
        let config = validate(raw).unwrap();
        let scene = scene(&config, &snapshot, 0);
        let mut full = AlignedFrame::boxed();
        scene.draw_full(&mut St7789Renderer::new(&mut full.0));
        for (i, frame) in buffers.iter_mut().enumerate() {
            cache.draw(&mut St7789Renderer::new(&mut frame.0), i, &scene);
            assert!(frame.0 == full.0);
        }
        assert!(
            pictures.iter().all(|p: &Vec<u8>| *p != full.0.to_vec()),
            "style {kind:?} is indistinguishable"
        );
        pictures.push(full.0.to_vec());
    }
}

#[test]
fn needle_changes_below_numeric_rounding_and_when_only_scale_changes() {
    let mut raw = default_config();
    raw.widgets.truncate(1);
    raw.widgets[0].display.decimals = 0;
    raw.widgets[0].display.extrema = false;
    raw.widgets[0].gauge = Gauge {
        kind: GaugeKind::Dial,
        min: 0.0,
        max: 1.0,
        expand: true,
    };
    let config = validate(raw).unwrap();
    let mut model = AcquisitionModel::demo();
    let mut cache = DashboardCache::new();
    let mut buffers = [AlignedFrame::boxed(), AlignedFrame::boxed()];
    for (now, value) in [(0, 0.11), (1, 0.12), (2, 3.0), (3, 0.12)] {
        model.ingest_batch([value, 80.0, 90.0, 75.0, 30.0, 700.0, 13.8, 0.9], now);
        let snapshot = model.snapshot();
        let scene = scene(&config, &snapshot, now);
        let mut full = AlignedFrame::boxed();
        scene.draw_full(&mut St7789Renderer::new(&mut full.0));
        for (i, frame) in buffers.iter_mut().enumerate() {
            cache.draw(&mut St7789Renderer::new(&mut frame.0), i, &scene);
            assert!(frame.0 == full.0);
        }
    }
}

#[test]
fn vehicle_gauges_expand_only_on_accepted_samples_and_reset_with_statistics() {
    use dashboard_host_validation::samples::{DataMode, Quality, Quantity, Sample, SensorId, Source};
    use dashboard_host_validation::widgets::gauge::Reading;
    let mut raw = default_config();
    raw.widgets.clear();
    let mut w = default_widget(
        Channel::Water,
        1,
        1,
        Placement {
            column: 0,
            row: 0,
            span: Span::TwoByTwo,
        },
    );
    w.gauge = Gauge {
        kind: GaugeKind::Circular,
        min: 60.,
        max: 110.,
        expand: true,
    };
    w.colors.background = Color::rgb(0, 16, 32);
    raw.widgets.push(w).unwrap();
    let config = validate(raw).unwrap();
    let w = &config.config().widgets[0];
    let mut model = AcquisitionModel::configured(&config, 0x7E8);
    let mut cache = DashboardCache::new();
    let mut buffers = [AlignedFrame::boxed(), AlignedFrame::boxed()];
    for (sequence, value) in [90., 120., 95., f32::NAN, 100.].into_iter().enumerate() {
        let now = sequence as u64 * 100;
        let sample = Sample {
            value: Quantity::CoolantC(value),
            sequence: sequence as u64 + 1,
            received_at_ms: now,
            source: Source::Obd { ecu: 0x7E8, pid: 5 },
        };
        assert_eq!(model.ingest_sample(sample, now).is_ok(), value.is_finite());
        let snapshot = model.snapshot();
        assert_eq!(snapshot.mode, DataMode::Vehicle);
        let reading = Reading::new(w, &snapshot, now);
        assert_eq!(
            (reading.min, reading.max),
            (60., if sequence == 0 { 110. } else { 120. })
        );
        if !value.is_finite() {
            assert!(reading.value.is_none());
        }
        let mut scene = scene(&config, &snapshot, now);
        scene.status = dashboard_host_validation::vehicle_status::VehicleStatus::Data;
        let mut full = AlignedFrame::boxed();
        scene.draw_full(&mut St7789Renderer::new(&mut full.0));
        for (i, frame) in buffers.iter_mut().enumerate() {
            cache.draw(&mut St7789Renderer::new(&mut frame.0), i, &scene);
            assert!(full.0 == frame.0);
        }
    }
    assert!(model.mark_fault(SensorId::Water, Quality::TransportError, 501));
    assert_eq!(Reading::new(w, &model.snapshot(), 501).value, None);
    assert!(model.reset_statistics(1));
    assert!(!model.reset_statistics(1));
    let reading = Reading::new(w, &model.snapshot(), 501);
    assert_eq!((reading.min, reading.max), (60., 110.));
    assert_eq!(reading.value, None);
}

#[test]
fn public_gauge_draw_caps_geometry_before_iteration_and_handles_tiny_regions() {
    use dashboard_host_validation::widgets::gauge::{Reading, draw};
    use embedded_graphics::prelude::*;
    use embedded_graphics::primitives::Rectangle;
    for kind in GaugeKind::ALL {
        for (point, size) in [
            (Point::new(i32::MAX, i32::MAX), Size::new(u32::MAX, u32::MAX)),
            (Point::zero(), Size::zero()),
            (Point::zero(), Size::new(19, 27)),
            (Point::new(10, 30), Size::new(u32::MAX, u32::MAX)),
        ] {
            let mut frame = AlignedFrame::boxed();
            draw(
                &mut St7789Renderer::new(&mut frame.0),
                kind,
                Rectangle::new(point, size),
                Reading {
                    peak: None,
                    min: -1.,
                    max: 3.,
                    value: Some(1.),
                },
                "-1000000.000",
                false,
                (colors::WHITE, colors::RED, colors::GRAY, colors::WHITE),
            );
            if size.width < 20 || point.x == i32::MAX {
                assert!(frame.0.iter().all(|b| *b == 0));
            } else {
                for y in 0..240 {
                    for x in 0..320 {
                        if !(10..166).contains(&x) || !(30..228).contains(&y) {
                            assert_eq!(&frame.0[(y * 320 + x) * 2..(y * 320 + x) * 2 + 2], &[0, 0]);
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn each_active_palette_role_has_its_own_pixels_without_leaking_from_the_cell() {
    for role in ColorRole::ALL
        .into_iter()
        .filter(|r| !matches!(r, ColorRole::WarningForeground | ColorRole::WarningBackground))
    {
        let mut raw = default_config();
        raw.widgets.clear();
        let mut w = default_widget(
            if role == ColorRole::Peak {
                Channel::Oil
            } else {
                Channel::Boost
            },
            1,
            1,
            Placement {
                column: 0,
                row: 0,
                span: Span::TwoByTwo,
            },
        );
        if w.channel == Channel::Boost {
            w.advanced.secondary = Secondary::Psi;
        }
        w.display.graph = true;
        w.display.trend = true;
        w.display.average = true;
        w.display.extrema = true;
        if matches!(role, ColorRole::Gauge | ColorRole::Track | ColorRole::Ticks) {
            w.gauge.kind = GaugeKind::Circular;
        }
        let colour = if matches!(role, ColorRole::Background | ColorRole::Track) {
            Color::rgb(0, 0, 96)
        } else {
            Color(0x07E0)
        };
        role.set(&mut w.colors, colour);
        raw.widgets.push(w).unwrap();
        let config = validate(raw).unwrap();
        let mut model = AcquisitionModel::demo();
        model.ingest_batch([0.5; 8], 0);
        model.ingest_batch([2.; 8], 2000);
        let now = if role == ColorRole::Peak {
            2001
        } else {
            model.ingest_batch([1.5; 8], 4000);
            4100
        };
        let snapshot = model.snapshot();
        let mut frame = AlignedFrame::boxed();
        scene(&config, &snapshot, now).draw_cell(&mut St7789Renderer::new(&mut frame.0), 0);
        assert!(
            frame.0.as_chunks::<2>().0.iter().any(|p| *p == colour.0.to_be_bytes()),
            "missing {role:?}"
        );
        assert!(
            frame.0[26 * 640..]
                .as_chunks::<640>()
                .0
                .iter()
                .all(|row| row[320..].iter().all(|b| *b == 0))
        );
    }
}

#[test]
fn warning_foreground_only_phase_and_overlay_palette_change_repaint_both_buffers() {
    use dashboard_host_validation::dashboard::Overlay;
    use dashboard_host_validation::view_state::WarningPager;
    let mut raw = default_config();
    raw.widgets.clear();
    raw.warnings.mode = WarningMode::ListThree;
    let mut w = default_widget(
        Channel::Oil,
        1,
        1,
        Placement {
            column: 0,
            row: 0,
            span: Span::TwoByTwo,
        },
    );
    for role in ColorRole::ALL {
        role.set(&mut w.colors, Color(65535));
    }
    w.colors.background = Color(0xF800);
    w.colors.track = Color(0);
    w.colors.warning = WarningColors {
        foreground: Color(0),
        background: Color(0xF800),
    };
    w.warning.on_ms = 1;
    raw.widgets.push(w).unwrap();
    let config = validate(raw).unwrap();
    let mut changed = config.config().clone();
    changed.warning_colors.background = Color::rgb(0, 0, 96);
    let changed = validate(changed).unwrap();
    let mut model = AcquisitionModel::demo_configured(&config);
    for at in [0, 1] {
        model.ingest_batch([150.; 8], at);
    }
    let snapshot = model.snapshot();
    let mut cache = DashboardCache::new();
    let mut frames = [AlignedFrame::boxed(), AlignedFrame::boxed()];
    let mut previous = None;
    for (config, now, blink, popup) in [
        (&config, 1, false, false),
        (&config, 2, true, false),
        (&config, 3, true, true),
        (&changed, 4, true, true),
        (&changed, 5, false, false),
        (&changed, 4000, true, false),
    ] {
        let mut scene = scene(config, &snapshot, now);
        scene.blink_on = blink;
        if popup {
            scene.overlay = WarningPager::new()
                .select(config, &snapshot, 1, now, blink)
                .map(Overlay::Warnings);
        }
        let mut full = AlignedFrame::boxed();
        scene.draw_full(&mut St7789Renderer::new(&mut full.0));
        if let Some(old) = previous {
            assert!(
                old != full.0.to_vec(),
                "visible transition did not change pixels at {now}"
            );
        }
        previous = Some(full.0.to_vec());
        for (i, frame) in frames.iter_mut().enumerate() {
            cache.draw(&mut St7789Renderer::new(&mut frame.0), i, &scene);
            assert!(frame.0 == full.0);
        }
    }
}

#[test]
fn zero_reference_omits_positive_and_negative_seed_and_expanded_ranges() {
    use dashboard_host_validation::presentation::Unit;
    use dashboard_host_validation::widgets::gauge::{Decorations, Reading, draw_decorated};
    use embedded_graphics::prelude::*;
    use embedded_graphics::primitives::Rectangle;
    for kind in GaugeKind::ALL.into_iter().filter(|k| !k.is_numeric()) {
        for unit in [Unit::Celsius, Unit::Fahrenheit] {
            // Seed and expanded ranges, with Zero still outside in canonical units.
            for (lo, hi) in [(10., 20.), (5., 30.), (-20., -10.), (-30., -5.)] {
                let render = |mode| {
                    let mut settings = AdvancedDisplay::for_channel(Channel::Water, false);
                    settings.reference_mode = mode;
                    let mut frame = AlignedFrame::boxed();
                    draw_decorated(
                        &mut St7789Renderer::new(&mut frame.0),
                        kind,
                        Rectangle::new(Point::new(10, 30), Size::new(100, 80)),
                        Reading {
                            min: unit.value(lo, 14.7),
                            max: unit.value(hi, 14.7),
                            value: None,
                            peak: None,
                        },
                        "--",
                        false,
                        (colors::WHITE, colors::RED, colors::GRAY, colors::WHITE),
                        Some(Decorations {
                            settings: &settings,
                            unit,
                            stoich: 14.7,
                            peak_color: colors::YELLOW,
                        }),
                    );
                    frame
                };
                assert!(
                    render(ReferenceMode::Zero).0 == render(ReferenceMode::Off).0,
                    "Zero must be omitted {kind:?}/{unit:?}/{lo}/{hi}"
                );
            }
        }
    }
}

#[test]
fn same_raster_level_with_changed_text_still_repaints_both_buffers() {
    let mut raw = default_config();
    raw.widgets.truncate(1);
    let w = &mut raw.widgets[0];
    w.gauge.kind = GaugeKind::Horizontal;
    w.gauge.min = -1.;
    w.gauge.max = 10.;
    w.gauge.expand = false;
    w.display.decimals = 3;
    w.display.extrema = false;
    w.advanced.peak_mode = PeakSelection::Off;
    let config = validate(raw).unwrap();
    let mut model = AcquisitionModel::demo_configured(&config);
    let mut cache = DashboardCache::new();
    let mut buffers = [AlignedFrame::boxed(), AlignedFrame::boxed()];
    let mut level = None;
    let mut previous = None;
    for (now, value) in [(0, 1.000), (1, 1.001)] {
        model.ingest_batch([value; 8], now);
        let snapshot = model.snapshot();
        let scene = scene(&config, &snapshot, now);
        let current = scene.gauge_reading(0).unwrap().level();
        if now != 0 {
            assert_eq!(level, current);
        }
        level = current;
        let mut full = AlignedFrame::boxed();
        scene.draw_full(&mut St7789Renderer::new(&mut full.0));
        if let Some(previous) = &previous {
            assert_ne!(&full.0[..], previous);
        }
        for (slot, frame) in buffers.iter_mut().enumerate() {
            let redraw = cache.draw(&mut St7789Renderer::new(&mut frame.0), slot, &scene);
            assert_eq!(redraw.cells, 1);
            assert!(full.0 == frame.0);
        }
        previous = Some(full.0.to_vec());
    }
}
