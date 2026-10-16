use dashboard_host_validation::acquisition::AcquisitionModel;
use dashboard_host_validation::configured_scene::{Layout, widget_text};
use dashboard_host_validation::dashboard::{DashboardCache, DashboardScene, Overlay};
use dashboard_host_validation::display::St7789Renderer;
use dashboard_host_validation::render::FpsMode;
use dashboard_host_validation::view_config::*;
use dashboard_host_validation::{AlignedFrame, colors};

#[test]
fn configuration_fault_keeps_mode_visible_and_repairs_both_headers() {
    use dashboard_host_validation::samples::DataMode;
    use dashboard_host_validation::vehicle_status::VehicleStatus;
    use embedded_graphics::mono_font::MonoTextStyle;
    use embedded_graphics::mono_font::ascii::FONT_6X10;
    use embedded_graphics::prelude::*;
    use embedded_graphics::text::Text;

    let mut raw = default_config();
    raw.views[0].name = "12345678901234".try_into().unwrap();
    let config = validate(raw).unwrap();
    for (mode, status, label) in [
        (DataMode::Demo, VehicleStatus::Demo, "DEMO"),
        (DataMode::Vehicle, VehicleStatus::Raw, "RAW ONLY"),
        (DataMode::Vehicle, VehicleStatus::ResetRequired, "RESET REQ"),
    ] {
        let mut snapshot = AcquisitionModel::demo().snapshot();
        snapshot.mode = mode;
        let mut cache = DashboardCache::new();
        let mut buffers = [AlignedFrame::boxed(), AlignedFrame::boxed()];
        let mut full = AlignedFrame::boxed();
        let mut normal = AlignedFrame::boxed();
        let mut expected = AlignedFrame::boxed();
        for fault in [false, true, false] {
            let scene = DashboardScene {
                readings: None,
                layout: Layout::new(&config, 1, fault),
                measurements: &snapshot,
                now_ms: 0,
                status,
                blink_on: false,
                boost_psi: false,
                fps_mode: FpsMode::Combined,
                fps_instant: 9999.0,
                fps_average: 9999.0,
                backgrounds: [colors::BLACK; 8],
                overlay: None,
            };
            scene.draw_full(&mut St7789Renderer::new(&mut full.0));
            let mut oracle = St7789Renderer::new(&mut expected.0);
            oracle.clear(colors::RED).unwrap();
            let style = MonoTextStyle::new(&FONT_6X10, colors::WHITE);
            // Independent glyph/position oracle, not a second call to the header.
            Text::new(label, Point::new(4, if fault { 10 } else { 17 }), style)
                .draw(&mut oracle)
                .unwrap();
            if fault {
                Text::new("CONFIG!", Point::new(4, 22), style)
                    .draw(&mut oracle)
                    .unwrap();
            } else {
                normal.0.copy_from_slice(&full.0[..]);
            }
            for y in 0..26 {
                let start = y * 320 * 2;
                assert_eq!(
                    &full.0[start..start + 64 * 2],
                    &expected.0[start..start + 64 * 2],
                    "status lane {mode:?}, fault={fault}, row={y}"
                );
                assert_eq!(
                    &full.0[start + 64 * 2..start + 320 * 2],
                    &normal.0[start + 64 * 2..start + 320 * 2],
                    "title/FPS changed"
                );
            }
            for (buffer, frame) in buffers.iter_mut().enumerate() {
                let redraw = cache.draw(&mut St7789Renderer::new(&mut frame.0), buffer, &scene);
                assert!(redraw.header, "fault transition did not dirty buffer {buffer}");
                assert!(frame.0 == full.0, "fault header differs on buffer {buffer}");
                assert!(!cache.prepare(buffer, &scene).header);
            }
        }
    }
}

#[test]
fn default_small_widgets_prioritize_the_primary_measurement() {
    let config = default_config();
    let lambda = &config.widgets[1].display;
    assert!(lambda.afr && lambda.graph);
    assert!(!lambda.average && !lambda.extrema && !lambda.trend);
    assert!(!config.widgets[0].display.graph && !config.widgets[0].display.average);
}

#[test]
fn wider_and_taller_widgets_use_visibly_larger_primary_digits() {
    // Count the vertical extent of actual white pixels in the primary lane,
    // independently of the renderer's font selection or reported metrics.
    let mut heights = Vec::new();
    for span in [Span::OneByOne, Span::TwoByOne, Span::TwoByTwo] {
        let mut raw = default_config();
        raw.widgets.truncate(1);
        let widget = &mut raw.widgets[0];
        widget.placement.span = span;
        widget.display.decimals = 1;
        widget.display.graph = false;
        widget.display.average = false;
        widget.display.extrema = false;
        widget.display.trend = false;
        let config = validate(raw).unwrap();
        let mut model = AcquisitionModel::demo();
        model.ingest_batch([1.7, 90.0, 90.0, 80.0, 30.0, 700.0, 13.8, 1.0], 0);
        let snapshot = model.snapshot();
        let scene = DashboardScene {
            readings: None,
            layout: Layout::new(&config, 1, false),
            measurements: &snapshot,
            now_ms: 0,
            status: dashboard_host_validation::vehicle_status::VehicleStatus::Demo,
            blink_on: false,
            boost_psi: false,
            fps_mode: FpsMode::Off,
            fps_instant: 0.0,
            fps_average: 0.0,
            backgrounds: [colors::BLACK; 8],
            overlay: None,
        };
        let mut frame = AlignedFrame::boxed();
        scene.draw_full(&mut St7789Renderer::new(&mut frame.0));
        // The first connected run of nonempty rows after the label is the number.
        let mut started = false;
        let mut height = 0;
        for y in 41..230 {
            let white = (3..(usize::from(span.columns()) * 80 - 3)).any(|x| {
                let p = (y * 320 + x) * 2;
                frame.0[p..p + 2] == [255, 255]
            });
            if white {
                started = true;
                height += 1;
            } else if started {
                break;
            }
        }
        heights.push(height);
    }
    assert!(heights[0] >= 15, "compact digits missing: {heights:?}");
    assert!(
        heights[1] >= heights[0] * 2,
        "wide digits were not enlarged: {heights:?}"
    );
    assert!(
        heights[2] >= heights[0] * 3,
        "tall digits were not enlarged: {heights:?}"
    );
}

#[test]
fn growing_and_shrinking_digits_repair_both_buffers_across_fit_thresholds() {
    use dashboard_host_validation::widgets::value_text::fit_value;
    use embedded_graphics::geometry::Size;
    for (span, values) in [
        (Span::TwoByOne, [99.9, 100.0, 99.9, -100.0, 0.0]),
        (Span::TwoByTwo, [9.9, 10.0, 9.9, -10.0, 0.0]),
    ] {
        let mut raw = default_config();
        raw.widgets.truncate(1);
        let w = &mut raw.widgets[0];
        w.placement.span = span;
        w.display.decimals = 1;
        w.display.graph = false;
        w.display.extrema = false;
        w.display.average = false;
        w.display.trend = false;
        let config = validate(raw).unwrap();
        let mut buffers = [AlignedFrame::boxed(), AlignedFrame::boxed()];
        let mut reference = AlignedFrame::boxed();
        let mut cache = DashboardCache::new();
        let mut sizes = Vec::new();
        for (step, value) in values.into_iter().enumerate() {
            let mut model = AcquisitionModel::demo();
            model.ingest_batch([value, 90.0, 90.0, 80.0, 30.0, 700.0, 13.8, 1.0], step as u64);
            let snapshot = model.snapshot();
            let text = widget_text(
                &config.config().widgets[0],
                &snapshot,
                step as u64,
                false,
                config.stoich(),
                &config.config().fuel,
            );
            sizes.push(
                fit_value(
                    &text[0],
                    Size::new(156, if span == Span::TwoByOne { 79 } else { 186 }),
                    if span == Span::TwoByOne { 2 } else { 3 },
                )
                .unwrap()
                .size
                .height,
            );
            for psi in [false, true, false] {
                let scene = DashboardScene {
                    readings: None,
                    layout: Layout::new(&config, 1, false),
                    measurements: &snapshot,
                    now_ms: step as u64,
                    status: dashboard_host_validation::vehicle_status::VehicleStatus::Demo,
                    blink_on: false,
                    boost_psi: psi,
                    fps_mode: FpsMode::Off,
                    fps_instant: 0.0,
                    fps_average: 0.0,
                    backgrounds: [colors::BLACK; 8],
                    overlay: None,
                };
                scene.draw_full(&mut St7789Renderer::new(&mut reference.0));
                for (slot, frame) in buffers.iter_mut().enumerate() {
                    cache.draw(&mut St7789Renderer::new(&mut frame.0), slot, &scene);
                    assert!(frame.0 == reference.0, "{span:?}, {value}, PSI={psi}, buffer={slot}");
                }
            }
        }
        assert!(sizes[0] > sizes[1], "fit threshold not exercised: {sizes:?}");
        assert_eq!(sizes[0], sizes[2]);
    }
}

#[test]
fn extreme_text_stays_measurable_and_invalid_lambda_has_no_numeric_afr() {
    use embedded_graphics::mono_font::MonoTextStyle;
    use embedded_graphics::prelude::*;
    use embedded_graphics::text::Text;
    for text in ["-1000000.000C", "14503800.000", "--", "0.800"] {
        for width in [76, 156] {
            for height in [12, 19, 32] {
                let font = dashboard_host_validation::configured_scene::fitted_font(text, width, height);
                let bounds = Text::new(text, Point::zero(), MonoTextStyle::new(font, colors::WHITE)).bounding_box();
                assert!(
                    bounds.size.width + 2 <= width && bounds.size.height + 2 <= height,
                    "{text}: {width}x{height}"
                );
            }
        }
    }
    let config = validate(default_config()).unwrap();
    for lambda in [-1.0, 0.0, 0.0001, 1000.0] {
        let mut model = AcquisitionModel::demo();
        model.ingest_batch([1.0, 90.0, 90.0, 80.0, 30.0, 700.0, 13.8, lambda], 0);
        let snapshot = model.snapshot();
        let text = widget_text(
            &config.config().widgets[1],
            &snapshot,
            0,
            false,
            config.stoich(),
            &config.config().fuel,
        );
        assert_eq!(text[1], "AFR --");
    }
}

#[test]
fn configured_text_respects_precision_units_flags_and_fuel_scale() {
    let mut config = default_config();
    config.fuel = Fuel::Stoichiometric { ratio: 14.0 };
    let config = validate(config).unwrap();
    let mut model = AcquisitionModel::demo();
    model.ingest_batch([1.234, 90.0, 95.0, 80.0, -3.5, 750.0, 13.8, 0.8], 0);
    let snapshot = model.snapshot();
    let lambda = &config.config().widgets[1];
    let text = widget_text(lambda, &snapshot, 0, false, config.stoich(), &config.config().fuel);
    assert_eq!(text[0], "0.800");
    assert_eq!(text[1], "AFR 11.20");
    assert_eq!(text[5], "S 14.00");
    let mut boost = config.config().widgets[0].clone();
    boost.display.decimals = 3;
    boost.advanced.secondary = Secondary::Bar;
    let bar = widget_text(&boost, &snapshot, 0, false, config.stoich(), &config.config().fuel);
    let psi = widget_text(&boost, &snapshot, 0, true, config.stoich(), &config.config().fuel);
    assert_eq!(bar[0], "1.234");
    assert_eq!(psi[0], "17.898");
    assert!(psi[1].ends_with("BAR"));
    boost.display.average = false;
    boost.display.extrema = false;
    let text = widget_text(&boost, &snapshot, 0, false, config.stoich(), &config.config().fuel);
    assert!(text[2..].iter().all(|s| s.is_empty()));
}

#[test]
fn peak_highlight_expires_on_elapsed_time_without_a_new_sample() {
    let config = validate(default_config()).unwrap();
    let mut model = AcquisitionModel::demo();
    model.ingest_batch([1.0, 90.0, 95.0, 80.0, 30.0, 750.0, 13.8, 0.8], 0);
    model.ingest_batch([1.0, 100.0, 95.0, 80.0, 30.0, 750.0, 13.8, 0.8], 1);
    let snapshot = model.snapshot();
    let mut scene = DashboardScene {
        readings: None,
        layout: Layout::new(&config, 1, false),
        measurements: &snapshot,
        now_ms: 1,
        status: dashboard_host_validation::vehicle_status::VehicleStatus::Demo,
        blink_on: false,
        boost_psi: false,
        fps_mode: FpsMode::Off,
        fps_instant: 0.0,
        fps_average: 0.0,
        backgrounds: [colors::BLACK; 8],
        overlay: None,
    };
    let mut cache = DashboardCache::new();
    cache.prepare(0, &scene);
    scene.now_ms = 751;
    assert_ne!(cache.prepare(0, &scene).cells & (1 << 4), 0);
}

#[test]
fn blend_percentage_is_explicit_and_afr_only_changes_repaint_both_buffers() {
    let mut model = AcquisitionModel::demo();
    model.ingest_batch([1.0, 90.0, 95.0, 80.0, 30.0, 750.0, 13.8, 0.8002], 0);
    for percent in [0.0, 5.5, 100.0] {
        let mut config = default_config();
        config.fuel = Fuel::EthanolBlend(Blend {
            percent,
            ..Blend::illustrative()
        });
        let config = validate(config).unwrap();
        let text = widget_text(
            &config.config().widgets[1],
            &model.snapshot(),
            0,
            false,
            config.stoich(),
            &config.config().fuel,
        );
        assert_eq!(text[5].as_str(), format!("E{percent:.2} EST"));
    }
    let config = validate(default_config()).unwrap();
    let first = model.snapshot();
    model.ingest_batch([1.0, 90.0, 95.0, 80.0, 30.0, 750.0, 13.8, 0.8004], 1);
    let second = model.snapshot();
    let mut cache = DashboardCache::new();
    let mut frames = [AlignedFrame::boxed(), AlignedFrame::boxed()];
    let mut reference = AlignedFrame::boxed();
    for (snapshot, now) in [(&first, 0), (&second, 1), (&second, 502)] {
        let scene = DashboardScene {
            readings: None,
            layout: Layout::new(&config, 1, false),
            measurements: snapshot,
            now_ms: now,
            status: dashboard_host_validation::vehicle_status::VehicleStatus::Demo,
            blink_on: false,
            boost_psi: false,
            fps_mode: FpsMode::Off,
            fps_instant: 0.0,
            fps_average: 0.0,
            backgrounds: [colors::BLACK; 8],
            overlay: None,
        };
        scene.draw_full(&mut St7789Renderer::new(&mut reference.0));
        for (buffer, frame) in frames.iter_mut().enumerate() {
            let redraw = cache.draw(&mut St7789Renderer::new(&mut frame.0), buffer, &scene);
            assert_ne!(redraw.cells & (1 << 1), 0, "lambda failed to repaint at {now}");
            assert!(frame.0 == reference.0);
        }
    }
    for quality in [
        dashboard_host_validation::samples::Quality::InvalidPayload,
        dashboard_host_validation::samples::Quality::Unsupported,
        dashboard_host_validation::samples::Quality::TransportError,
    ] {
        model.mark_fault(dashboard_host_validation::samples::SensorId::Lambda, quality, 600);
        let snapshot = model.snapshot();
        let scene = DashboardScene {
            readings: None,
            layout: Layout::new(&config, 1, false),
            measurements: &snapshot,
            now_ms: 600,
            status: dashboard_host_validation::vehicle_status::VehicleStatus::Demo,
            blink_on: false,
            boost_psi: false,
            fps_mode: FpsMode::Off,
            fps_instant: 0.0,
            fps_average: 0.0,
            backgrounds: [colors::BLACK; 8],
            overlay: None,
        };
        scene.draw_full(&mut St7789Renderer::new(&mut reference.0));
        for (buffer, frame) in frames.iter_mut().enumerate() {
            cache.draw(&mut St7789Renderer::new(&mut frame.0), buffer, &scene);
            assert!(frame.0 == reference.0);
        }
    }
}

#[test]
fn both_buffers_match_full_redraw_for_spans_holes_views_overlays_and_options() {
    let mut config = default_config();
    config
        .views
        .push(View {
            id: 2,
            name: "12345678901234".try_into().unwrap(),
            orientation: None,
        })
        .unwrap();
    for (id, channel, column, span) in [
        (20, Channel::Lambda, 0, Span::TwoByTwo),
        (21, Channel::Egt, 2, Span::TwoByOne),
    ] {
        let mut widget = default_widget(channel, id, 2, Placement { column, row: 0, span });
        widget.label = "123456789012345678901234".try_into().unwrap();
        widget.display.average = true;
        widget.display.extrema = true;
        config.widgets.push(widget).unwrap();
    }
    let config = validate(config).unwrap();
    let mut model = AcquisitionModel::demo();
    let mut cache = DashboardCache::new();
    let mut buffers = [AlignedFrame::boxed(), AlignedFrame::boxed()];
    let mut reference = AlignedFrame::boxed();
    for step in 0..16 {
        let now = step * 2000;
        model.ingest_batch(
            [1.5, 90.0, 95.0, 80.0, -3.5, 1500.0, 13.8, 0.8 + step as f32 * 0.001],
            now,
        );
        let snapshot = model.snapshot();
        let scene = DashboardScene {
            readings: None,
            layout: Layout::new(&config, if step % 4 < 2 { 1 } else { 2 }, step == 11),
            measurements: &snapshot,
            now_ms: now,
            status: dashboard_host_validation::vehicle_status::VehicleStatus::Demo,
            blink_on: step % 2 == 0,
            boost_psi: step % 3 == 0,
            fps_mode: FpsMode::Combined,
            fps_instant: 72.3,
            fps_average: 69.9,
            backgrounds: [colors::BLACK; 8],
            overlay: (step % 5 == 0).then_some(Overlay::Reset),
        };
        scene.draw_full(&mut St7789Renderer::new(&mut reference.0));
        let buffer = step as usize % 2;
        cache.draw(&mut St7789Renderer::new(&mut buffers[buffer].0), buffer, &scene);
        assert!(buffers[buffer].0 == reference.0, "retained differs on step {step}");
        let clean = cache.prepare(buffer, &scene);
        assert!(!clean.clear && !clean.header && clean.cells == 0 && !clean.overlay);
    }
    assert!(Layout::new(&config, 255, false).is_none());
}

#[test]
fn every_span_and_display_option_stays_inside_its_declared_rectangle() {
    use embedded_graphics::pixelcolor::Rgb565;
    use embedded_graphics::prelude::*;
    let mut model = AcquisitionModel::demo();
    model.ingest_batch([9.999, -99.9, 1999.0, 2000.0, -99.0, 1999.9, 65.535, 9.999], 0);
    let snapshot = model.snapshot();
    let mut frame = AlignedFrame::boxed();
    for span in [Span::OneByOne, Span::TwoByOne, Span::TwoByTwo] {
        for channel in Channel::ALL {
            for flags in 0..32 {
                let mut config = default_config();
                config.widgets.clear();
                let column = 4 - span.columns();
                let row = 2 - span.rows();
                let mut widget = default_widget(channel, 99, 1, Placement { column, row, span });
                widget.label = "X"
                    .repeat(usize::from(span.columns()) * 12)
                    .as_str()
                    .try_into()
                    .unwrap();
                widget.display.decimals = 3;
                widget.display.graph = flags & 1 != 0;
                widget.display.average = flags & 2 != 0;
                widget.display.extrema = flags & 4 != 0;
                widget.display.trend = flags & 8 != 0;
                widget.display.afr = channel == Channel::Lambda && flags & 16 != 0;
                if channel == Channel::Lambda {
                    widget.advanced.readout = if widget.display.afr {
                        dashboard_host_validation::view_config::Readout::LambdaAndAfr
                    } else {
                        dashboard_host_validation::view_config::Readout::Lambda
                    };
                }
                config.widgets.push(widget).unwrap();
                let config = validate(config).unwrap();
                let scene = DashboardScene {
                    readings: None,
                    layout: Layout::new(&config, 1, false),
                    measurements: &snapshot,
                    now_ms: 0,
                    status: dashboard_host_validation::vehicle_status::VehicleStatus::Demo,
                    blink_on: true,
                    boost_psi: flags & 1 != 0,
                    fps_mode: FpsMode::Off,
                    fps_instant: 0.0,
                    fps_average: 0.0,
                    backgrounds: [colors::BLACK; 8],
                    overlay: None,
                };
                let mut display = St7789Renderer::new(&mut frame.0);
                display.clear(Rgb565::BLUE).unwrap();
                scene.draw_cell(&mut display, 0);
                // Independent 4x2 mapping, not Placement::rect as the oracle.
                let (left, top) = (usize::from(column) * 80, 26 + usize::from(row) * 107);
                for y in 0..240 {
                    for x in 0..320 {
                        if x < left || y < top {
                            assert_eq!(
                                &frame.0[(y * 320 + x) * 2..][..2],
                                &[0, 31],
                                "neighbor touched: {channel:?} {span:?} flags {flags}, {x},{y}"
                            );
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn cache_repairs_empty_views_and_reused_ids_with_changed_options() {
    let mut model = AcquisitionModel::demo();
    model.ingest_batch([1.2, 90.0, 95.0, 80.0, 30.0, 750.0, 13.8, 0.8002], 0);
    let snapshot = model.snapshot();
    let mut configs = vec![validate(default_config()).unwrap()];
    let mut changed = default_config();
    changed.widgets[1].display.afr = false;
    changed.widgets[1].advanced.readout = dashboard_host_validation::view_config::Readout::Lambda;
    changed.widgets[1].display.graph = false;
    changed.widgets[1].label = "RENAMED".try_into().unwrap();
    configs.push(validate(changed).unwrap());
    let mut empty = default_config();
    empty.widgets.clear();
    configs.push(validate(empty).unwrap());
    let mut cache = DashboardCache::new();
    let mut buffers = [AlignedFrame::boxed(), AlignedFrame::boxed()];
    let mut reference = AlignedFrame::boxed();
    for selection in [None, Some(2), Some(0), Some(1), None, Some(2), Some(0)] {
        let scene = DashboardScene {
            readings: None,
            layout: selection.map(|i| Layout::new(&configs[i], 1, false).unwrap()),
            measurements: &snapshot,
            now_ms: 0,
            status: dashboard_host_validation::vehicle_status::VehicleStatus::Demo,
            blink_on: false,
            boost_psi: false,
            fps_mode: FpsMode::Off,
            fps_instant: 0.0,
            fps_average: 0.0,
            backgrounds: [colors::BLACK; 8],
            overlay: None,
        };
        scene.draw_full(&mut St7789Renderer::new(&mut reference.0));
        for (buffer, frame) in buffers.iter_mut().enumerate() {
            cache.draw(&mut St7789Renderer::new(&mut frame.0), buffer, &scene);
            assert!(frame.0 == reference.0, "repair failed for selection {selection:?}");
        }
    }
}
