use dashboard_configurator::config::*;
use dashboard_configurator::preview::*;
use dashboard_core::alarms::{RuleIndex, Status};
use dashboard_core::colors::{BLACK, RED};
use dashboard_core::configured_scene::Layout;
use dashboard_core::dashboard::{DashboardScene, Overlay};
use dashboard_core::display::{FrameBuffer, St7789Renderer};
use dashboard_core::render::FpsMode;
use dashboard_core::samples::{DataMode, Quality, SensorId, Source};
use dashboard_core::vehicle_status::VehicleStatus;

fn fixture() -> ValidatedConfig { validate(default_config()).unwrap() }

#[test]
fn preview_observed_extrema_only_expand_until_rewind_or_sample_source_changes() {
    let config = fixture();
    let mut preview = Preview::new();
    let mut bounds = [(f32::INFINITY, f32::NEG_INFINITY); 8];
    for sample_ms in (0..240_000).step_by(3_001) {
        preview
            .render(
                &config,
                1,
                PreviewOptions {
                    sample_ms,
                    ..Default::default()
                },
            )
            .unwrap();
        for (i, id) in SensorId::ALL.into_iter().enumerate() {
            let sensor = preview.snapshot().unwrap().sensor(id);
            let current = (sensor.minimum().unwrap(), sensor.maximum().unwrap());
            assert!(
                current.0 <= bounds[i].0 && current.1 >= bounds[i].1,
                "preview shrank {id:?} at {sample_ms}"
            );
            bounds[i] = current;
        }
    }
    preview
        .render(
            &config,
            1,
            PreviewOptions {
                sample_ms: 0,
                values: Some([1.0; 8]),
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(preview.snapshot().unwrap().sensor(SensorId::Boost).maximum(), Some(1.0));
}

#[test]
fn browser_samples_match_the_firmware_demo_generator_at_the_actual_rendered_time() {
    let config = fixture();
    let mut preview = Preview::new();
    for sample_ms in [
        0,
        1,
        118_000,
        6_283_184,
        6_283_185,
        30 * 24 * 60 * 60 * 1000,
        u64::MAX - 118_001,
    ] {
        preview
            .render(
                &config,
                1,
                PreviewOptions {
                    sample_ms,
                    ..Default::default()
                },
            )
            .unwrap();
        let now = preview.rendered_at_ms().unwrap();
        let expected = dashboard_core::demo::values(now);
        for (sensor, value) in SensorId::ALL.into_iter().zip(expected) {
            assert_eq!(
                preview.snapshot().unwrap().sensor(sensor).value(),
                Some(value),
                "{sample_ms}, {sensor:?}"
            );
        }
    }
}

#[test]
fn stress_sets_have_bounded_values_and_recover_to_the_shared_demo() {
    let config = fixture();
    let mut preview = Preview::new();
    for set in [
        SampleSet::Minimum,
        SampleSet::Maximum,
        SampleSet::WidthStress,
        SampleSet::Zero,
        SampleSet::Demo,
    ] {
        for psi in [false, true] {
            let options = PreviewOptions {
                values: set.values(),
                boost_psi: psi,
                ..Default::default()
            };
            preview.render(&config, 1, options).unwrap();
            assert_exact_shared_scene(&preview, &config, 1, options);
        }
    }
}

#[test]
fn rgb565_is_big_endian_and_expands_every_channel_to_full_rgba_range() {
    let words = [0xF800_u16, 0x07E0, 0x001F, 0, 0xFFFF, 0x4208, 0x028A];
    let source: Vec<u8> = words.into_iter().flat_map(u16::to_be_bytes).collect();
    let mut output = vec![99; 28];
    rgb565_to_rgba(&source, &mut output).unwrap();
    assert_eq!(
        output,
        [
            255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 0, 0, 0, 255, 255, 255, 255, 255, 66, 65, 66, 255, 0, 81,
            82, 255,
        ]
    );
}

#[test]
fn bad_conversion_lengths_reject_without_partial_writes() {
    for (input, output_len) in [(vec![0; 1], 4), (vec![0; 4], 7), (vec![0; 4], 9), (vec![0; 2], 0)] {
        let mut output = vec![77; output_len];
        assert_eq!(rgb565_to_rgba(&input, &mut output), Err(PreviewError::BufferLength));
        assert_eq!(output, vec![77; output_len]);
    }
    assert_eq!(rgb565_to_rgba(&[], &mut []), Ok(()));
}

#[test]
fn preview_reuses_complete_buffers_and_populates_only_demo_histories() {
    let mut preview = Preview::new();
    let config = fixture();
    let rgba_ptr = preview.rgba().as_ptr();
    let frame_ptr = preview.rgb565().as_ptr();
    for sample_ms in [0, 3500] {
        let rgba = preview
            .render(
                &config,
                1,
                PreviewOptions {
                    sample_ms,
                    ..Default::default()
                },
            )
            .unwrap();
        assert_eq!(rgba.len(), 320 * 240 * 4);
        assert!(rgba.as_chunks::<4>().0.iter().all(|pixel| pixel[3] == 255));
        assert_eq!(rgba.as_ptr(), rgba_ptr);
        assert_eq!(preview.rgb565().len(), 320 * 240 * 2);
        assert_eq!(preview.rgb565().as_ptr(), frame_ptr);
        assert!(preview.rgb565().iter().any(|&byte| byte != 0));
        let snapshot = preview.snapshot().unwrap();
        let now = preview.rendered_at_ms().unwrap();
        assert_eq!(snapshot.mode, DataMode::Demo);
        assert_eq!(
            snapshot.sequence, 61,
            "work is a fixed 60 history points and one final dwell point"
        );
        for sensor in SensorId::ALL {
            assert_eq!(snapshot.metadata[sensor as usize].source, Source::Demo);
            assert_eq!(snapshot.quality(sensor, now), Quality::Valid);
            let state = &snapshot.sensors[sensor as usize];
            assert_eq!(state.get_graph_data().2, 60);
            assert!(state.get_average().is_some());
            assert!(state.minimum().is_some());
            assert!(state.maximum().is_some());
        }
        assert_eq!(preview.warning_page(), None);
    }
}

fn assert_exact_shared_scene(
    preview: &Preview,
    config: &ValidatedConfig,
    view: u8,
    options: PreviewOptions,
) {
    let snapshot = preview.snapshot().unwrap();
    let now = preview.rendered_at_ms().unwrap();
    let layout = Layout::new(config, view, false).unwrap();
    let blink = options.warnings && (options.sample_ms / 500).is_multiple_of(2);
    let scene = DashboardScene {
        layout: Some(layout),
        measurements: snapshot,
        status: VehicleStatus::Demo,
        now_ms: now,
        blink_on: blink,
        boost_psi: options.boost_psi,
        fps_mode: FpsMode::Off,
        fps_instant: 0.0,
        fps_average: 0.0,
        backgrounds: core::array::from_fn(|slot| {
            if blink && layout.alarm_active(slot, snapshot, now) {
                RED
            } else {
                BLACK
            }
        }),
        overlay: preview.warning_page().map(Overlay::Warnings),
    };
    let mut expected = Box::new(FrameBuffer::new());
    scene.draw_full(&mut St7789Renderer::new(&mut expected));
    assert_eq!(
        preview.rgb565(),
        &expected[..],
        "preview must draw the production scene, including header, fonts, graph and overlay"
    );
    let mut rgba = vec![0; 320 * 240 * 4];
    rgb565_to_rgba(&expected[..], &mut rgba).unwrap();
    assert_eq!(preview.rgba(), &rgba);
}

#[test]
fn pixels_match_the_shared_renderer_for_both_units_and_warning_phases() {
    let config = fixture();
    let mut preview = Preview::new();
    for options in [
        PreviewOptions::default(),
        PreviewOptions {
            boost_psi: true,
            ..Default::default()
        },
        PreviewOptions {
            warnings: true,
            ..Default::default()
        },
        PreviewOptions {
            warnings: true,
            sample_ms: 500,
            ..Default::default()
        },
    ] {
        preview.render(&config, 1, options).unwrap();
        assert_exact_shared_scene(&preview, &config, 1, options);
    }
}

#[test]
fn psi_only_changes_boost_presentation_and_custom_samples_remain_canonical() {
    let config = fixture();
    let mut preview = Preview::new();
    let options = PreviewOptions {
        values: Some([1.8, 97.0, 89.0, 90.0, 32.0, 700.0, 13.8, 0.91]),
        ..Default::default()
    };
    preview.render(&config, 1, options).unwrap();
    let bar = preview.rgba().to_vec();
    let snapshot = *preview.snapshot().unwrap();
    preview
        .render(
            &config,
            1,
            PreviewOptions {
                boost_psi: true,
                ..options
            },
        )
        .unwrap();
    assert_ne!(preview.rgba(), bar);
    assert_eq!(preview.snapshot().unwrap(), &snapshot);
    assert_eq!(
        snapshot.value_at(SensorId::Boost, preview.rendered_at_ms().unwrap()),
        Some(1.8)
    );
    for (index, (old, new)) in bar
        .as_chunks::<4>()
        .0
        .iter()
        .zip(preview.rgba().as_chunks::<4>().0)
        .enumerate()
    {
        if old != new {
            assert!(
                index % 320 < 80 && (26..133).contains(&(index / 320)),
                "unit change escaped the boost cell"
            );
        }
    }
}

#[test]
fn invalid_view_samples_and_time_preserve_the_last_complete_preview() {
    let config = fixture();
    let mut preview = Preview::new();
    preview.render(&config, 1, PreviewOptions::default()).unwrap();
    let bytes = preview.rgba().to_vec();
    let frame = preview.rgb565().to_vec();
    let snapshot = *preview.snapshot().unwrap();
    let time = preview.rendered_at_ms();
    assert_eq!(
        preview.render(&config, 200, PreviewOptions::default()).unwrap_err(),
        PreviewError::UnknownView(200)
    );
    for value in [f32::NAN, f32::INFINITY, 1_000_001.0] {
        let mut values = [1.0; 8];
        values[3] = value;
        assert_eq!(
            preview
                .render(
                    &config,
                    1,
                    PreviewOptions {
                        values: Some(values),
                        ..Default::default()
                    }
                )
                .unwrap_err(),
            PreviewError::InvalidSample(Channel::Dsg)
        );
    }
    assert_eq!(
        preview
            .render(
                &config,
                1,
                PreviewOptions {
                    sample_ms: u64::MAX,
                    ..Default::default()
                }
            )
            .unwrap_err(),
        PreviewError::TimeOverflow
    );
    assert_eq!(preview.rgba(), bytes);
    assert_eq!(preview.rgb565(), frame);
    assert_eq!(preview.snapshot().unwrap(), &snapshot);
    assert_eq!(preview.rendered_at_ms(), time);
}

fn warnings_config() -> Config {
    let mut config = default_config();
    config.widgets.clear();
    for index in 0..4 {
        let mut widget = default_widget(
            Channel::Oil,
            u16::from(index) + 1,
            1,
            Placement {
                column: index,
                row: 0,
                span: Span::OneByOne,
            },
        );
        widget.warning.message = format!("Warning {}", index + 1).as_str().try_into().unwrap();
        config.widgets.push(widget).unwrap();
    }
    config
}

#[test]
fn immediate_warning_demo_compresses_dwell_without_changing_the_export_document() {
    let mut config = warnings_config();
    for widget in &mut config.widgets {
        widget.warning.on_ms = 30_000;
    }
    for source in &mut config.sources {
        source.freshness_ms = 1;
    }
    let config = validate(config).unwrap();
    let before = config.clone();
    let mut preview = Preview::new();
    preview
        .render(
            &config,
            1,
            PreviewOptions {
                warnings: true,
                ..Default::default()
            },
        )
        .unwrap();
    let snapshot = preview.snapshot().unwrap();
    let now = preview.rendered_at_ms().unwrap();
    for index in 0..4 {
        assert_eq!(
            snapshot.rule_status_at(RuleIndex::new(index).unwrap(), now),
            Status::Active
        );
    }
    let page = preview.warning_page().unwrap();
    assert_eq!(page.lines()[0].as_str(), "Warning 1");
    assert_eq!((page.number, page.total), (1, 4));
    assert_eq!(snapshot.sequence, 61);
    assert_eq!(config, before);
}

#[test]
fn warning_pages_use_configured_messages_modes_intervals_and_scope() {
    let mut config = warnings_config();
    config.warnings.mode = WarningMode::ListThree;
    config.warnings.page_ms = 500;
    config
        .views
        .push(View {
            id: 2,
            name: "Other".try_into().unwrap(),
        })
        .unwrap();
    config.widgets[3].view = 2;
    let mut config = validate(config).unwrap();
    let mut preview = Preview::new();
    for (sample_ms, number, names) in [
        (0, 1, vec!["Warning 1", "Warning 2", "Warning 3"]),
        (500, 2, vec!["Warning 4"]),
    ] {
        preview
            .render(
                &config,
                1,
                PreviewOptions {
                    sample_ms,
                    warnings: true,
                    ..Default::default()
                },
            )
            .unwrap();
        let page = preview.warning_page().unwrap();
        assert_eq!((page.number, page.total), (number, 2));
        assert_eq!(page.lines().iter().map(|s| s.as_str()).collect::<Vec<_>>(), names);
    }
    let mut edit = config.config().clone();
    edit.warnings.scope = WarningScope::CurrentView;
    config = validate(edit).unwrap();
    preview
        .render(
            &config,
            1,
            PreviewOptions {
                sample_ms: 501,
                warnings: true,
                ..Default::default()
            },
        )
        .unwrap();
    let page = preview.warning_page().unwrap();
    assert_eq!((page.number, page.total, page.lines().len()), (1, 1, 3));
    preview
        .render(
            &config,
            2,
            PreviewOptions {
                sample_ms: 502,
                warnings: true,
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(preview.warning_page().unwrap().lines()[0].as_str(), "Warning 4");
}

#[test]
fn warning_demo_preserves_threshold_enablement_and_synthetic_context_gates() {
    let mut config = default_config();
    for widget in &mut config.widgets {
        widget.warning.enabled = widget.channel == Channel::Oil || widget.channel == Channel::Lambda;
        if widget.channel == Channel::Oil {
            widget.warning.trigger = 170.0;
        }
        if widget.channel == Channel::Lambda {
            widget.warning.min_load_percent = 90.0;
        }
    }
    let config = validate(config).unwrap();
    let mut preview = Preview::new();
    preview
        .render(
            &config,
            1,
            PreviewOptions {
                warnings: true,
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(
        preview.warning_page(),
        None,
        "Oil synthetic 150 is below trigger; Lambda synthetic context load is 80%"
    );
}

#[test]
fn warning_toggle_rewind_and_document_replacement_restart_paging() {
    let config = validate(warnings_config()).unwrap();
    let mut preview = Preview::new();
    for (sample_ms, warnings, expected) in [
        (0, true, Some(1)),
        (2000, true, Some(2)),
        (2001, false, None),
        (2002, true, Some(1)),
        (4002, true, Some(2)),
        (0, true, Some(1)),
    ] {
        preview
            .render(
                &config,
                1,
                PreviewOptions {
                    sample_ms,
                    warnings,
                    ..Default::default()
                },
            )
            .unwrap();
        assert_eq!(preview.warning_page().map(|page| page.number), expected);
    }
    let mut changed = config.config().clone();
    changed.widgets[0].warning.message = "New message".try_into().unwrap();
    let changed = validate(changed).unwrap();
    preview
        .render(
            &changed,
            1,
            PreviewOptions {
                sample_ms: 2000,
                warnings: true,
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(preview.warning_page().unwrap().lines()[0].as_str(), "New message");
}

#[test]
fn clearing_view_removes_old_pixels_and_wide_layout_uses_exact_shared_scene() {
    let mut preview = Preview::new();
    let config = fixture();
    preview.render(&config, 1, PreviewOptions::default()).unwrap();
    let mut empty = config.config().clone();
    empty.widgets.clear();
    let empty = validate(empty).unwrap();
    preview.render(&empty, 1, PreviewOptions::default()).unwrap();
    assert!(preview.rgb565()[26 * 320 * 2..].iter().all(|&byte| byte == 0));
    assert_exact_shared_scene(&preview, &empty, 1, PreviewOptions::default());
    let mut wide = empty.config().clone();
    wide.widgets
        .push(default_widget(
            Channel::Lambda,
            64,
            1,
            Placement {
                column: 0,
                row: 0,
                span: Span::TwoByTwo,
            },
        ))
        .unwrap();
    wide.widgets
        .push(default_widget(
            Channel::Oil,
            4,
            1,
            Placement {
                column: 2,
                row: 0,
                span: Span::TwoByOne,
            },
        ))
        .unwrap();
    let wide = validate(wide).unwrap();
    preview.render(&wide, 1, PreviewOptions::default()).unwrap();
    assert_exact_shared_scene(&preview, &wide, 1, PreviewOptions::default());
}
