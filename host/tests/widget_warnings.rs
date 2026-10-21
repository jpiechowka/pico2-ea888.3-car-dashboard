//! Independent warning-badge pixel oracle across gauge styles, spans, orientations and retained buffers.
use dashboard_host_validation::acquisition::{AcquisitionModel, DashboardSnapshot};
use dashboard_host_validation::alarms::{RuleIndex, Status};
use dashboard_host_validation::configured_scene::Layout;
use dashboard_host_validation::control_session::ControlSession;
use dashboard_host_validation::dashboard::{DashboardCache, DashboardScene, Overlay};
use dashboard_host_validation::display::St7789Renderer;
use dashboard_host_validation::display::pipeline::{BufferId, FrameToken};
use dashboard_host_validation::pages::Page;
use dashboard_host_validation::presentation::PresentationState;
use dashboard_host_validation::render::FpsMode;
use dashboard_host_validation::samples::{Quality, SensorId};
use dashboard_host_validation::status_led::{PresentedLed, PresentedScene};
use dashboard_host_validation::vehicle_status::VehicleStatus;
use dashboard_host_validation::view_config::*;
use dashboard_host_validation::view_state::WarningPager;
use dashboard_host_validation::warning_state::{WarningFeedback, WarningRequests};
use dashboard_host_validation::{AlignedFrame, colors};
use embedded_graphics::mono_font::MonoTextStyle;
use embedded_graphics::mono_font::ascii::FONT_4X6;
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::Rectangle;
use embedded_graphics::text::Text;

/// Ingests canonical boost `value` in bar with fixed healthy companion channels into `model` at `now` ms.
/// Advances quality/alarms to the same time; panics if the batch is rejected.
fn sample(
    model: &mut AcquisitionModel<'_>,
    value: f32,
    now: u64,
) {
    assert!(model.ingest_batch([value, 90., 95., 80., 30., 700., 13.8, 1.], now));
    model.tick(now);
}

/// Returns a validated boost widget with gauge `kind`, tile `span` and every display feature enabled.
/// `advanced` enables latching, five-second snooze and reachable acknowledgment; invalid fixtures panic.
fn fixture(
    kind: GaugeKind,
    span: Span,
    advanced: bool,
) -> ValidatedConfig {
    let mut c = default_config();
    c.widgets.truncate(1);
    let w = &mut c.widgets[0];
    w.placement.span = span;
    w.gauge.kind = kind;
    w.display.average = true;
    w.display.extrema = true;
    w.display.trend = true;
    w.display.graph = true;
    w.advanced.reference_mode = ReferenceMode::Manual;
    w.advanced.reference = 1.;
    w.advanced
        .set_bands(&[
            Band::new(-1., 0., Color(0x07E0), "LOW").unwrap(),
            Band::new(0., 1., Color(0x07FF), "MID").unwrap(),
            Band::new(1., 3., Color(0xF81F), "HIGH").unwrap(),
        ])
        .unwrap();
    w.warning.enabled = true;
    w.warning.trigger = 2.;
    w.warning.clear = 1.;
    w.warning.on_ms = 100;
    w.warning.off_ms = 100;
    w.warning.advanced.latch = advanced;
    w.warning.advanced.snooze_s = if advanced { 5 } else { 0 };
    if advanced {
        c.controls.screens[0].buttons[0].tap = ControlAction::AcknowledgeWarnings;
    }
    validate(c).unwrap()
}

/// Presents and acknowledges the current warning in `model` using configuration `c` at `now` ms.
/// `requests` supplies acknowledgment identity and observes acceptance. Returns controls showing accepted feedback.
/// Panics without eligible attention or if one-rule acceptance changes raw sensors/rule status.
fn acknowledge(
    c: &ValidatedConfig,
    model: &mut AcquisitionModel<'_>,
    now: u64,
    requests: &mut WarningRequests,
) -> ControlSession {
    let s = model.snapshot();
    let mut pager = WarningPager::new();
    let page = pager.select(c, &s, 1, now, true).expect("attention page");
    let scene = PresentedScene::capture(c, &s, Page::Dashboard, 1, Some(Overlay::Warnings(page)), now);
    let mut presented = PresentedLed::new();
    let token = FrameToken {
        buffer: BufferId::A,
        sequence: 1,
    };
    assert!(presented.submit(token, scene));
    assert!(presented.complete(token));
    let command = requests.request_presented(presented.warning()).unwrap();
    let mut controls = ControlSession::new(c, FpsMode::Combined, false);
    controls.warning_feedback(WarningFeedback::Pending, now);
    let before = model.snapshot();
    assert_eq!(model.apply_warning_command(command, now).accepted, 1);
    let after = model.snapshot();
    assert_eq!(before.sensors, after.sensors, "ack changed raw statistics");
    assert_eq!(
        before.rule_status_at(RuleIndex::new(0).unwrap(), now),
        after.rule_status_at(RuleIndex::new(0).unwrap(), now)
    );
    assert_eq!(requests.observe(&after), Some(WarningFeedback::Accepted));
    controls.warning_feedback(WarningFeedback::Accepted, now);
    assert!(controls.overlay(c).is_some());
    controls
}

/// Independent full-render oracle compared against both retained framebuffers.
struct RasterCheck {
    /// Retained scene identity for each physical buffer.
    cache: DashboardCache,
    /// Two independently dirty RGB565 framebuffers reused through the scenario.
    buffers: [Box<AlignedFrame>; 2],
    /// Prepared display values, separate from immutable acquisition evidence.
    presentation: PresentationState,
}
impl RasterCheck {
    /// Returns empty cache/presentation state and two black aligned buffers.
    fn new() -> Self {
        Self {
            cache: DashboardCache::new(),
            buffers: [AlignedFrame::boxed(), AlignedFrame::boxed()],
            presentation: PresentationState::new(),
        }
    }

    /// Checks `snapshot` under `c` at `now` ms against complete and retained render paths.
    /// `marker` is the expected badge text; empty skips its glyph oracle. `stale` adds the quality line;
    /// `steady` requires identical pixels in both blink phases. Mutates cache/presentation/buffers only.
    /// Panics on raster/quality/raw-data drift or optional WIDGET_WARNING_EVIDENCE PPM output failure.
    fn check(
        &mut self,
        c: &ValidatedConfig,
        snapshot: &DashboardSnapshot,
        now: u64,
        marker: &str,
        stale: bool,
        steady: bool,
    ) {
        let geometry = c.config().orientation.geometry();
        let portrait = geometry == dashboard_config::Geometry::Portrait;
        let line_height = if portrait { 7 } else { 8 };
        let baseline = if portrait { 5 } else { 6 };
        let before = *snapshot;
        let readings = self.presentation.prepare(c, snapshot, 1, now, false);
        let mut previous = None;
        for blink_on in [false, true] {
            let scene = DashboardScene {
                readings: Some(&readings),
                layout: Layout::new(c, 1, false),
                measurements: snapshot,
                status: VehicleStatus::Demo,
                now_ms: now,
                blink_on,
                boost_psi: false,
                fps_mode: FpsMode::Combined,
                fps_instant: 0.,
                fps_average: 0.,
                backgrounds: [colors::BLACK; 8],
                overlay: None,
            };
            let mut full = AlignedFrame::boxed();
            scene.draw_full(&mut St7789Renderer::with_geometry(&mut full.0, geometry));
            if let Some(previous) = &previous {
                if steady {
                    assert_eq!(&full.0[..], previous, "snoozed tile blinked at {now}");
                } else if snapshot.rule_status_at(RuleIndex::new(0).unwrap(), now) == Status::Active {
                    assert_ne!(&full.0[..], previous, "ordinary attention stopped blinking at {now}");
                }
            }
            if !marker.is_empty() {
                // Independent complete badge glyph oracle in its dedicated lane.
                // FONT4X6 uses4px advance; every complete label fits the interior.
                let rect = c.config().widgets[0].placement.rect_in(geometry).unwrap();
                let width = rect.width as usize;
                assert!(marker.len() * 4 <= width - 4);
                let mut expected = AlignedFrame::boxed();
                let mut target = St7789Renderer::with_geometry(&mut expected.0, geometry);
                let palette = c.config().widgets[0].colors.warning;
                let background: embedded_graphics::pixelcolor::Rgb565 =
                    embedded_graphics::pixelcolor::raw::RawU16::new(palette.background.0).into();
                let foreground = embedded_graphics::pixelcolor::raw::RawU16::new(palette.foreground.0).into();
                let y = rect.y as i32 + if portrait { 9 } else { 15 };
                let center = rect.x as i32 + rect.width as i32 / 2 - i32::from(portrait);
                let x = center - (marker.len() as i32 * 4 - 1) / 2;
                target
                    .fill_solid(
                        &Rectangle::new(
                            Point::new(rect.x as i32 + 2, y),
                            Size::new(rect.width as u32 - 4, if stale { line_height * 2 } else { line_height }),
                        ),
                        background,
                    )
                    .unwrap();
                Text::new(
                    marker,
                    Point::new(x, y + baseline),
                    MonoTextStyle::new(&FONT_4X6, foreground),
                )
                .draw(&mut target)
                .unwrap();
                if stale {
                    let unavailable = if snapshot.quality(SensorId::Boost, now) == Quality::Stale {
                        "DATA STALE"
                    } else {
                        "UNAVAILABLE"
                    };
                    Text::new(
                        unavailable,
                        Point::new(
                            center - (unavailable.len() as i32 * 4 - 1) / 2,
                            y + baseline + line_height as i32,
                        ),
                        MonoTextStyle::new(&FONT_4X6, foreground),
                    )
                    .draw(&mut target)
                    .unwrap();
                }
                for row in y as usize..y as usize + (if stale { line_height * 2 } else { line_height }) as usize {
                    let start = (row * usize::from(geometry.width()) + rect.x as usize + 2) * 2;
                    let end = start + (width - 4) * 2;
                    assert_eq!(
                        &full.0[start..end],
                        &expected.0[start..end],
                        "incomplete badge {marker}, {stale}, {:?}/{:?}, row{row}",
                        c.config().widgets[0].gauge.kind,
                        c.config().widgets[0].placement.span
                    );
                }
            }
            for (buffer, frame) in self.buffers.iter_mut().enumerate() {
                let changed = frame.0 != full.0;
                let redraw = self.cache.draw(
                    &mut St7789Renderer::with_geometry(&mut frame.0, geometry),
                    buffer,
                    &scene,
                );
                if changed {
                    assert_ne!(redraw.cells & 1, 0, "visible state missed buffer{buffer} at{now}");
                }
                assert!(frame.0 == full.0, "retained mismatch buffer{buffer} at{now}");
                assert_eq!(self.cache.prepare_in(buffer, &scene, geometry).cells, 0);
            }
            // Diagnostic/raw fallback must use the same query and palette as
            // the prepared path when smoothing is off, including unavailable.
            let raw_scene = DashboardScene {
                readings: None,
                ..scene
            };
            let mut raw = AlignedFrame::boxed();
            raw_scene.draw_full(&mut St7789Renderer::with_geometry(&mut raw.0, geometry));
            assert!(raw.0 == full.0);
            if !portrait
                && !blink_on
                && !marker.is_empty()
                && matches!(now, 3400 | 3901 | 4000 | 5100 | 5500 | 10700)
                && let Some(directory) = std::env::var_os("WIDGET_WARNING_EVIDENCE")
            {
                let mut ppm = b"P6\n320 240\n255\n".to_vec();
                for pixel in full.0.as_chunks::<2>().0 {
                    let rgb = u16::from_be_bytes([pixel[0], pixel[1]]);
                    ppm.extend_from_slice(&[
                        (((rgb >> 11) & 31) * 255 / 31) as u8,
                        (((rgb >> 5) & 63) * 255 / 63) as u8,
                        ((rgb & 31) * 255 / 31) as u8,
                    ]);
                }
                let widget = &c.config().widgets[0];
                std::fs::write(
                    std::path::PathBuf::from(directory)
                        .join(format!("{:?}-{:?}-{now}.ppm", widget.gauge.kind, widget.placement.span)),
                    ppm,
                )
                .unwrap();
            }
            previous = Some(full.0.to_vec());
        }
        assert_eq!(before, *snapshot, "drawing mutated raw snapshot");
    }
}

/// Every style/span/orientation preserves full acknowledgment, snooze and stale-latch badges in both buffers.
#[test]
fn acknowledged_snoozed_and_stale_latch_badges_fit_every_style_span_and_both_buffers() {
    for orientation in [
        dashboard_config::Orientation::Landscape,
        dashboard_config::Orientation::Portrait,
    ] {
        for kind in GaugeKind::ALL {
            for span in [Span::OneByOne, Span::TwoByOne, Span::TwoByTwo] {
                let mut raw = fixture(kind, span, true).config().clone();
                raw.orientation = orientation;
                if orientation == dashboard_config::Orientation::Portrait {
                    raw.widgets[0].placement.row = if span == Span::TwoByTwo { 2 } else { 3 };
                }
                let c = validate(raw).unwrap();
                let mut m = AcquisitionModel::demo_configured(&c);
                let mut raster = RasterCheck::new();
                let mut requests = WarningRequests::new();
                sample(&mut m, 3., 0);
                raster.check(&c, &m.snapshot(), 0, "", false, false);
                sample(&mut m, 3., 100);
                raster.check(&c, &m.snapshot(), 100, "LATCHED", false, false);
                let mut controls = acknowledge(&c, &mut m, 100, &mut requests);
                // Feedback has expired; continuously admitted fresh samples preserve the episode.
                for now in (200..=3400).step_by(100) {
                    sample(&mut m, 3., now);
                }
                let mut pager = WarningPager::new();
                controls.expire_feedback(3400);
                assert!(controls.overlay(&c).is_none());
                assert!(pager.select(&c, &m.snapshot(), 1, 3400, true).is_none());
                raster.check(&c, &m.snapshot(), 3400, "ACTIVE ACK SNOOZE", false, true);
                m.tick(3901);
                assert_eq!(m.snapshot().quality(SensorId::Boost, 3901), Quality::Stale);
                raster.check(&c, &m.snapshot(), 3901, "LATCH ACK SNOOZE", true, true);
                m.mark_fault(SensorId::Boost, Quality::TransportError, 4000);
                assert_eq!(m.snapshot().value_at(SensorId::Boost, 4000), None);
                assert_eq!(
                    m.snapshot().rule_status_at(RuleIndex::new(0).unwrap(), 4000),
                    Status::Unavailable
                );
                raster.check(&c, &m.snapshot(), 4000, "LATCH ACK SNOOZE", true, true);
                m.tick(5100);
                assert!(pager.select(&c, &m.snapshot(), 1, 5100, true).is_some());
                raster.check(&c, &m.snapshot(), 5100, "LATCHED / ACK", true, true);
                // A fresh activation retires prior acknowledgment; stale is never fresh Active.
                sample(&mut m, 3., 5200);
                raster.check(&c, &m.snapshot(), 5200, "LATCHED / ACK", false, true);
                sample(&mut m, 3., 5300);
                raster.check(&c, &m.snapshot(), 5300, "LATCHED", false, false);
                sample(&mut m, 0., 5400);
                sample(&mut m, 0., 5500);
                raster.check(&c, &m.snapshot(), 5500, "CLEARED ACK DUE", false, true);
                acknowledge(&c, &mut m, 5500, &mut requests);
                raster.check(&c, &m.snapshot(), 5500, "", false, true);
                // The other ordering: acknowledgment while active, expiry, qualified clear.
                sample(&mut m, 3., 5600);
                sample(&mut m, 3., 5700);
                acknowledge(&c, &mut m, 5700, &mut requests);
                for now in (5800..=10700).step_by(100) {
                    sample(&mut m, 3., now);
                }
                raster.check(&c, &m.snapshot(), 10700, "ACTIVE / ACK", false, false);
                sample(&mut m, 0., 10800);
                sample(&mut m, 0., 10900);
                raster.check(&c, &m.snapshot(), 10900, "", false, true);
            }
        }
    }
}

/// Ordinary unacknowledged warnings blink while fresh and lose visible eligibility when stale.
#[test]
fn ordinary_unacknowledged_warning_keeps_blink_and_unavailability() {
    let c = fixture(GaugeKind::ALL[0], Span::OneByOne, false);
    let mut m = AcquisitionModel::demo_configured(&c);
    sample(&mut m, 3., 0);
    sample(&mut m, 3., 100);
    let mut raster = RasterCheck::new();
    raster.check(&c, &m.snapshot(), 100, "", false, false);
    m.tick(1000);
    assert!(
        !m.snapshot()
            .warning_eligibility(RuleIndex::new(0).unwrap(), 1000)
            .visible()
    );
    raster.check(&c, &m.snapshot(), 1000, "", false, true);
}
