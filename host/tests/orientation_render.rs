use dashboard_config::{GaugeKind, Geometry, Orientation, Span, default_config, validate};
use dashboard_core::acquisition::AcquisitionModel;
use dashboard_core::configured_scene::{Layout, portrait_content_layout};
use dashboard_core::dashboard::{DashboardCache, DashboardScene};
use dashboard_core::display::St7789Renderer;
use dashboard_core::pages::Page;
use dashboard_core::render::FpsMode;
use dashboard_core::startup::*;
use dashboard_core::vehicle_status::VehicleStatus;
use dashboard_host_validation::{AlignedFrame, colors};
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::Rectangle;

fn scene<'a>(
    config: &'a dashboard_config::ValidatedConfig,
    snapshot: &'a dashboard_core::acquisition::DashboardSnapshot,
) -> DashboardScene<'a> {
    DashboardScene {
        readings: None,
        layout: Layout::new(config, 1, true),
        measurements: snapshot,
        status: VehicleStatus::Demo,
        now_ms: 10,
        blink_on: false,
        boost_psi: false,
        fps_mode: FpsMode::Combined,
        fps_instant: 9999.,
        fps_average: 9999.,
        backgrounds: [colors::BLACK; 8],
        overlay: None,
    }
}

#[test]
fn orientation_policy_includes_phase_override_and_invalid_view_fallback() {
    let mut config = default_config();
    config.orientation = Orientation::Portrait;
    config.views[0].orientation = Some(Orientation::LandscapeInverted);
    let profile = StartupProfile {
        status_orientation: Some(Orientation::PortraitInverted),
        splash_orientation: Some(Orientation::Landscape),
        ..StartupProfile::DEFAULT
    };
    for page in [Page::Dashboard, Page::Debug, Page::Logs] {
        assert_eq!(
            screen_orientation(&config, profile, Phase::Status, page, 1),
            Orientation::PortraitInverted
        );
        assert_eq!(
            screen_orientation(&config, profile, Phase::Splash, page, 1),
            Orientation::Landscape
        );
    }
    assert_eq!(
        screen_orientation(&config, profile, Phase::Dashboard, Page::Dashboard, 1),
        Orientation::LandscapeInverted
    );
    for page in [Page::Debug, Page::Logs] {
        assert_eq!(
            screen_orientation(&config, profile, Phase::Dashboard, page, 1),
            Orientation::Portrait
        );
    }
    assert_eq!(
        screen_orientation(&config, profile, Phase::Dashboard, Page::Dashboard, 255),
        Orientation::Portrait
    );
}

#[test]
fn compact_numeric_lanes_keep_primary_and_history_with_maximum_warning_content() {
    let bounds = Rectangle::new(Point::new(2, 248), Size::new(116, 70));
    for strings in [
        [
            "-123.000",
            "-14.123 PSI",
            "AVG -123.000",
            "MIN -123.000",
            "MAX 123.000",
            "E85.00 EST",
        ],
        [
            "-1000000.000",
            "-1000000.000 PSI",
            "AVG -1000000.000",
            "MIN -1000000.000",
            "MAX 1000000.000",
            "E85.00 EST",
        ],
    ] {
        let lanes = portrait_content_layout(bounds, strings, 2, true, true, 0, true);
        assert!(lanes.text[0].size.height >= 8);
        assert_eq!(lanes.graph.size.height, 12);
        let all: Vec<_> = lanes
            .text
            .into_iter()
            .chain([lanes.graph])
            .filter(|r| r.size.height > 0)
            .collect();
        for (i, a) in all.iter().enumerate() {
            assert_eq!(a.intersection(&bounds), *a, "lane outside widget: {a:?}");
            for b in &all[i + 1..] {
                assert!(a.intersection(b).size == Size::zero(), "overlap {a:?}/{b:?}");
            }
        }
    }
}

#[test]
fn every_portrait_gauge_span_flags_draws_bottom_row_and_matches_retained() {
    for kind in GaugeKind::ALL {
        for span in [Span::OneByOne, Span::TwoByOne, Span::TwoByTwo] {
            for flags in 0u8..32 {
                let mut raw = default_config();
                raw.orientation = Orientation::Portrait;
                raw.views[0].name = "12345678901234".try_into().unwrap();
                raw.widgets.remove(0);
                raw.widgets.truncate(1);
                let widget = &mut raw.widgets[0];
                widget.gauge.kind = kind;
                widget.label = if span == Span::OneByOne {
                    "ABCDEFGHIJKL"
                } else {
                    "ABCDEFGHIJKLMNOPQRSTUVWX"
                }
                .try_into()
                .unwrap();
                widget.placement.column = 0;
                widget.placement.row = if span == Span::TwoByTwo { 2 } else { 3 };
                widget.placement.span = span;
                widget.display.graph = flags & 1 != 0;
                widget.display.average = flags & 2 != 0;
                widget.display.extrema = flags & 4 != 0;
                widget.display.trend = flags & 8 != 0;
                widget.display.afr = flags & 16 != 0;
                widget.advanced.readout = if widget.display.afr {
                    dashboard_config::Readout::LambdaAndAfr
                } else {
                    dashboard_config::Readout::Lambda
                };
                widget.display.decimals = 3;
                widget.colors.value = dashboard_config::Color(0x07E0);
                let config = validate(raw).unwrap();
                let mut model = AcquisitionModel::demo_configured(&config);
                model.ingest_batch([1.2, 90., 95., 80., 30., 700., 13.8, 1.], 0);
                let snapshot = model.snapshot();
                let scene = scene(&config, &snapshot);
                let mut full = AlignedFrame::boxed();
                scene.draw_full(&mut St7789Renderer::with_geometry(&mut full.0, Geometry::Portrait));
                let rect = config.config().widgets[0]
                    .placement
                    .rect_in(Geometry::Portrait)
                    .unwrap();
                assert!(
                    full.0[usize::from(rect.y) * 480..]
                        .chunks_exact(2)
                        .any(|p| p == [0x07, 0xE0]),
                    "missing primary {kind:?}/{span:?}/{flags}"
                );
                let mut cache = DashboardCache::new();
                for buffer in 0..2 {
                    let mut retained = AlignedFrame::boxed();
                    assert!(
                        cache
                            .draw(
                                &mut St7789Renderer::with_geometry(&mut retained.0, Geometry::Portrait),
                                buffer,
                                &scene
                            )
                            .clear
                    );
                    assert!(full.0 == retained.0);
                    assert!(!cache.prepare_in(buffer, &scene, Geometry::Portrait).clear);
                    assert!(cache.prepare_in(buffer, &scene, Geometry::Landscape).clear);
                }
            }
        }
    }
}

#[test]
fn portrait_debug_keeps_every_cooperative_row_visible() {
    use dashboard_core::debug_view::{DebugData, DebugLine, DebugPage};
    let data = DebugData::default();
    let map = dashboard_config::ControlMap::default();
    let page = DebugPage::new(&data, &map);
    for (index, line) in DebugLine::ALL.into_iter().enumerate() {
        let mut frame = AlignedFrame::boxed();
        page.draw_line(
            &mut St7789Renderer::with_geometry(&mut frame.0, Geometry::Portrait),
            line,
        );
        let y = 4 + index * 9;
        assert!(
            frame.0[y * 480..(y + 7) * 480].iter().any(|v| *v != 0),
            "missing {line:?}"
        );
        assert!(frame.0[..y * 480].iter().all(|v| *v == 0));
        assert!(frame.0[(y + 7) * 480..].iter().all(|v| *v == 0));
    }
}

#[test]
fn portrait_popups_are_centered_on_actual_frame() {
    use dashboard_core::widgets::*;
    for kind in 0..3 {
        let mut frame = AlignedFrame::boxed();
        let mut target = St7789Renderer::with_geometry(&mut frame.0, Geometry::Portrait);
        match kind {
            0 => draw_reset_popup(&mut target),
            1 => draw_view_test(&mut target),
            _ => draw_critical_test(&mut target),
        };
        let positions: Vec<_> = frame
            .0
            .chunks_exact(2)
            .enumerate()
            .filter(|(_, p)| *p != [0, 0])
            .map(|(i, _)| ((i % 240) as i32, (i / 240) as i32))
            .collect();
        let left = positions.iter().map(|p| p.0).min().unwrap();
        let right = positions.iter().map(|p| p.0).max().unwrap();
        let top = positions.iter().map(|p| p.1).min().unwrap();
        let bottom = positions.iter().map(|p| p.1).max().unwrap();
        assert!((left + right - 239).abs() <= 1);
        assert!((top + bottom - 319).abs() <= 1);
    }
    fn draw_view_test(d: &mut St7789Renderer<'_>) { draw_view_popup(d, "12345678901234"); }
    fn draw_critical_test(d: &mut St7789Renderer<'_>) {
        draw_critical_popup(d, dashboard_core::samples::SensorId::Oil, true);
    }
}

#[test]
fn portrait_startup_themes_preserve_time_and_quiet_frames() {
    let facts = StartupFacts {
        vehicle: VehicleStatus::Demo,
        config_ok: true,
        settings_loaded: true,
        configured_sources: 8,
    };
    for theme in StartupTheme::ALL {
        let p = StartupProfile {
            theme,
            ..StartupProfile::DEFAULT
        };
        let render = |time, reduced| {
            let mut frame = AlignedFrame::boxed();
            draw(
                &mut St7789Renderer::with_geometry(&mut frame.0, Geometry::Portrait),
                Phase::Splash,
                p,
                time,
                reduced,
                facts,
            );
            frame
        };
        assert!(render(0, false).0 != render(350, false).0, "{theme:?}");
        assert!(render(0, false).0 == render(2000, false).0);
        assert!(render(0, true).0 == render(u64::MAX, true).0);
    }
}

#[test]
fn portrait_fallback_missing_samples_keep_primary_marker_in_every_cell() {
    let config = validate(default_config()).unwrap();
    let model = AcquisitionModel::demo();
    let snapshot = model.snapshot();
    let mut scene = scene(&config, &snapshot);
    scene.layout = None;
    let mut frame = AlignedFrame::boxed();
    scene.draw_full(&mut St7789Renderer::with_geometry(&mut frame.0, Geometry::Portrait));
    for slot in 0..8 {
        let top = usize::from(Geometry::Portrait.row_edge(slot / 2).unwrap()) + 9;
        let bottom = top + 25;
        let x = usize::from(slot % 2) * 120;
        assert!(
            (top..bottom).any(|y| {
                frame.0[(y * 240 + x + 5) * 2..(y * 240 + x + 115) * 2]
                    .iter()
                    .any(|v| *v != 0)
            }),
            "missing primary slot{slot}"
        );
    }
}

#[test]
fn portrait_logs_preserve_twelve_complete_messages_and_fault() {
    use dashboard_core::logging::{Level, LogStore};
    use dashboard_core::logging_view::LogPage;
    use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
    use embedded_graphics::mono_font::MonoTextStyle;
    use embedded_graphics::mono_font::ascii::FONT_4X6;
    use embedded_graphics::text::Text;
    let store = LogStore::<CriticalSectionRawMutex>::new();
    let message = "0123456789012345678901234567890123456789";
    store.push(Level::Fault, 0, format_args!("{message}"));
    for n in 0..12 {
        store.push(Level::Info, n + 1, format_args!("{message}"));
    }
    let page = LogPage::capture(&store, None);
    let mut frame = AlignedFrame::boxed();
    page.draw_full(&mut St7789Renderer::with_geometry(&mut frame.0, Geometry::Portrait));
    for row in 0..=12 {
        let y = if row == 0 { 28 } else { 44 + (row - 1) * 20 };
        let mut expected = AlignedFrame::boxed();
        Text::new(message, Point::new(52, y), MonoTextStyle::new(&FONT_4X6, colors::WHITE))
            .draw(&mut St7789Renderer::with_geometry(&mut expected.0, Geometry::Portrait))
            .unwrap();
        for line in y - 5..y + 2 {
            let start = (line as usize * 240 + 52) * 2;
            let end = (line as usize * 240 + 212) * 2;
            assert_eq!(
                &frame.0[start..end],
                &expected.0[start..end],
                "message clipped row{row}"
            );
        }
    }
}

#[test]
fn custom_asset_geometry_mismatch_leaves_frame_untouched() {
    use dashboard_config::startup_asset::*;
    for geometry in Geometry::ALL {
        for rle in [false, true] {
            let payload = if rle {
                let mut payload = Vec::new();
                for _ in 0..geometry.height() {
                    payload.extend_from_slice(&(0x8000 | (geometry.width() - 1)).to_le_bytes());
                    payload.extend_from_slice(&[0xF8, 0]);
                }
                payload
            } else {
                [0xF8u8, 0].repeat(Geometry::PIXELS)
            };
            let mut bytes = vec![0; 52];
            bytes[..8].copy_from_slice(MAGIC);
            bytes[8..10].copy_from_slice(&2u16.to_le_bytes());
            bytes[10..12].copy_from_slice(&32u16.to_le_bytes());
            bytes[16..18].copy_from_slice(&geometry.width().to_le_bytes());
            bytes[18..20].copy_from_slice(&geometry.height().to_le_bytes());
            bytes[20..22].copy_from_slice(&1u16.to_le_bytes());
            bytes[24..28].copy_from_slice(&100u32.to_le_bytes());
            bytes[32..36].copy_from_slice(&100u32.to_le_bytes());
            bytes[36..40].copy_from_slice(&52u32.to_le_bytes());
            bytes[40..44].copy_from_slice(&(payload.len() as u32).to_le_bytes());
            bytes[44..48].copy_from_slice(&crc32(&payload).to_le_bytes());
            bytes[48] = u8::from(rle);
            bytes.extend(payload);
            let length = bytes.len() as u32;
            bytes[12..16].copy_from_slice(&length.to_le_bytes());
            let crc = asset_crc32(&bytes).unwrap();
            bytes[28..32].copy_from_slice(&crc.to_le_bytes());
            let asset = Asset::parse(&bytes).unwrap();
            let mut frame = AlignedFrame::boxed();
            let wrong = if geometry == Geometry::Portrait {
                Geometry::Landscape
            } else {
                Geometry::Portrait
            };
            assert_eq!(
                draw_custom_in(asset, 0, true, false, wrong, &mut frame.0),
                Err(AssetError::Pixels)
            );
            assert!(frame.0.iter().all(|v| *v == 0));
            draw_custom_in(asset, 0, true, false, geometry, &mut frame.0).unwrap();
            assert!(frame.0.chunks_exact(2).all(|pixel| pixel == [0xF8, 0]));
        }
    }
}

#[test]
fn portrait_turbo_ring_is_concentric_with_blades_and_hub() {
    let mut frame = AlignedFrame::boxed();
    draw(
        &mut St7789Renderer::with_geometry(&mut frame.0, Geometry::Portrait),
        Phase::Splash,
        StartupProfile {
            theme: StartupTheme::Turbo,
            ..StartupProfile::DEFAULT
        },
        0,
        false,
        StartupFacts {
            vehicle: VehicleStatus::Demo,
            config_ok: true,
            settings_loaded: true,
            configured_sources: 8,
        },
    );
    let pixel = |x: usize, y: usize| u16::from_be_bytes([frame.0[(y * 240 + x) * 2], frame.0[(y * 240 + x) * 2 + 1]]);
    // Original object center (160,125) maps to (120,153), with an82px
    // outer diameter. Ring cardinal points share the blade/hub center.
    let cyan = (3u16 << 11) | (53 << 5) | 28;
    assert_eq!(pixel(120, 112), cyan);
    assert_eq!(pixel(79, 153), cyan);
    assert_eq!(pixel(120, 193), cyan);
    let gold = (31u16 << 11) | (43 << 5) | 5;
    assert_eq!(pixel(120, 153), gold);
}

#[test]
fn portrait_peak_highlight_is_visible_and_expires() {
    let mut raw = default_config();
    raw.orientation = Orientation::Portrait;
    raw.widgets[0] = raw
        .widgets
        .iter()
        .find(|w| w.channel == dashboard_config::Channel::Oil)
        .unwrap()
        .clone();
    raw.widgets.truncate(1);
    raw.widgets[0].placement.column = 0;
    raw.widgets[0].placement.row = 0;
    raw.widgets[0].display.extrema = true;
    raw.widgets[0].colors.peak = dashboard_config::Color(0xF81F);
    let config = validate(raw).unwrap();
    let mut model = AcquisitionModel::demo_configured(&config);
    model.ingest_batch([1., 90., 95., 80., 30., 700., 13.8, 1.], 0);
    model.ingest_batch([1., 100., 95., 80., 30., 700., 13.8, 1.], 1);
    let snapshot = model.snapshot();
    let mut scene = scene(&config, &snapshot);
    let mut frame = AlignedFrame::boxed();
    scene.now_ms = 1;
    scene.draw_full(&mut St7789Renderer::with_geometry(&mut frame.0, Geometry::Portrait));
    assert!(frame.0.chunks_exact(2).any(|p| p == [0xF8, 0x1F]));
    scene.now_ms = 751;
    scene.draw_full(&mut St7789Renderer::with_geometry(&mut frame.0, Geometry::Portrait));
    assert!(!frame.0.chunks_exact(2).any(|p| p == [0xF8, 0x1F]));
}

#[test]
fn startup_portrait_visual_evidence() {
    let Some(directory) = std::env::var_os("ORIENTATION_ART_EVIDENCE") else {
        return;
    };
    for (name, phase, theme) in [
        ("status", Phase::Status, StartupTheme::Aezakmi),
        ("aezakmi", Phase::Splash, StartupTheme::Aezakmi),
        ("turbo", Phase::Splash, StartupTheme::Turbo),
        ("sanic", Phase::Splash, StartupTheme::Sanic),
        ("boost-bro", Phase::Splash, StartupTheme::BoostBro),
        ("where-lambo", Phase::Splash, StartupTheme::WhereLambo),
    ] {
        let mut frame = AlignedFrame::boxed();
        draw(
            &mut St7789Renderer::with_geometry(&mut frame.0, Geometry::Portrait),
            phase,
            StartupProfile {
                theme,
                status: StartupStatus::Detailed,
                ..StartupProfile::DEFAULT
            },
            350,
            false,
            StartupFacts {
                vehicle: VehicleStatus::Demo,
                config_ok: true,
                settings_loaded: true,
                configured_sources: 8,
            },
        );
        let mut ppm = b"P6\n240 320\n255\n".to_vec();
        for pixel in frame.0.chunks_exact(2) {
            let value = u16::from_be_bytes([pixel[0], pixel[1]]);
            ppm.extend_from_slice(&[
                (((value >> 11) & 31) * 255 / 31) as u8,
                (((value >> 5) & 63) * 255 / 63) as u8,
                ((value & 31) * 255 / 31) as u8,
            ]);
        }
        std::fs::write(std::path::PathBuf::from(&directory).join(format!("{name}.ppm")), ppm).unwrap();
    }
}
