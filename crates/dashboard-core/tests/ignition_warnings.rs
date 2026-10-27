//! Synthetic group warning thresholds, membership, acknowledgment and popup paging.
use dashboard_config::{
    ControlAction,
    IgnitionWarningPresentation,
    Placement,
    Span,
    ValidatedConfig,
    default_config,
    default_ignition_widget,
    validate,
};
use dashboard_core::acquisition::AcquisitionModel;
use dashboard_core::alarms::{RuleIndex, Status};
use dashboard_core::ignition::Cylinder;
use dashboard_core::samples::Quality;
use dashboard_core::warning_state::WarningCommand;

/// Returns a single synthetic group with 100 ms on/off dwell; `latch` enables
/// remembered episodes and five-second snooze. Invalid fixture configuration panics.
fn config(latch: bool) -> ValidatedConfig {
    let mut config = default_config();
    config.widgets.clear();
    let mut widget = default_ignition_widget(
        1,
        1,
        Placement {
            column: 0,
            row: 0,
            span: Span::OneByOne,
        },
    );
    widget.warning.enabled = true;
    widget.warning.on_ms = 100;
    widget.warning.off_ms = 100;
    widget.warning.advanced.latch = latch;
    widget.warning.advanced.snooze_s = if latch { 5 } else { 0 };
    config.controls.screens[0].buttons[0].tap = ControlAction::AcknowledgeWarnings;
    config.widgets.push(widget).unwrap();
    validate(config).unwrap()
}

/// Catches zero activating the strict negative threshold and ticks advancing dwell.
#[test]
fn strict_negative_default_and_capture_driven_shared_dwell() {
    let config = config(false);
    let mut owner = AcquisitionModel::demo_configured(&config);
    let index = RuleIndex::new(0).unwrap();
    owner.ingest_ignition_demo([0.0, -0.0, 1.0, 0.0], 0);
    assert_eq!(owner.snapshot().rule_status_at(index, 0), Status::Normal);
    owner.ingest_ignition_demo([0.0, -2.0, 0.0, 0.0], 100);
    assert_eq!(owner.snapshot().rule_status_at(index, 100), Status::Pending);
    owner.tick(200);
    assert_eq!(owner.snapshot().rule_status_at(index, 200), Status::Pending);
    owner.ingest_batch([1.0, 90.0, 95.0, 80.0, 30.0, 700.0, 13.8, 1.0], 200);
    assert_eq!(owner.snapshot().rule_status_at(index, 200), Status::Pending);
    owner.ingest_ignition_demo([0.0, -2.0, 0.0, 0.0], 200);
    let snapshot = owner.snapshot();
    assert_eq!(snapshot.rule_status_at(index, 200), Status::Active);
    assert_eq!(snapshot.ignition_warning_mask_at(index, 200), 0b0010);
    assert_eq!(snapshot.ignition_warning_mask_at(index, 701), 0);
    assert_eq!(snapshot.rule_status_at(index, 701), Status::Unavailable);
}

/// Catches shared-value aggregation losing cylinder hysteresis or clearing with missing data.
#[test]
fn independent_hysteresis_and_all_cylinders_required_for_clear() {
    let mut candidate = config(true).config().clone();
    candidate.widgets[0].warning.trigger = -2.0;
    candidate.widgets[0].warning.clear = -1.0;
    let config = validate(candidate).unwrap();
    let mut owner = AcquisitionModel::demo_configured(&config);
    let index = RuleIndex::new(0).unwrap();
    for at in [0, 100] {
        owner.ingest_ignition_demo([-2.0, -3.0, -1.5, 0.0], at);
    }
    assert_eq!(owner.snapshot().ignition_warning_mask_at(index, 100), 0b0010);
    owner.ingest_ignition_demo([-3.0, -1.5, -1.5, 0.0], 200);
    assert_eq!(owner.snapshot().ignition_warning_mask_at(index, 200), 0b0011);
    owner.mark_ignition_fault(Cylinder::One, Quality::TransportError, 201);
    assert_eq!(owner.snapshot().ignition_warning_mask_at(index, 201), 0b0010);
    owner.ingest_ignition_demo([f32::NAN, 0.0, 0.0, 0.0], 300);
    owner.ingest_ignition_demo([f32::NAN, 0.0, 0.0, 0.0], 400);
    let missing = owner.snapshot();
    assert_eq!(missing.rule_status_at(index, 400), Status::Unavailable);
    assert!(missing.warning_eligibility(index, 400).latched);
    assert!(!missing.warning_eligibility(index, 400).cleared);
    assert_eq!(missing.ignition_warning_latched_mask(index), 0b0011);
    for at in [500, 600] {
        owner.ingest_ignition_demo([0.0; 4], at);
    }
    assert!(owner.snapshot().warning_eligibility(index, 600).cleared);
}

/// Catches one stale member suppressing another fresh warning or an old ACK covering a new member.
#[test]
fn new_member_changes_episode_identity_and_preserves_fresh_sibling() {
    let config = config(true);
    let mut owner = AcquisitionModel::demo_configured(&config);
    let index = RuleIndex::new(0).unwrap();
    for at in [0, 100] {
        owner.ingest_ignition_demo([-1.0, 0.0, 0.0, 0.0], at);
    }
    let previous = owner.snapshot();
    let old_command = WarningCommand {
        request_id: 1,
        run_id: previous.warning_run_id().unwrap(),
        observed_event_serial: previous.warning_event_serial(),
        rules: 1,
    };
    assert_eq!(owner.apply_warning_command(old_command, 100).accepted, 1);
    owner.ingest_ignition_demo([-1.0, -2.0, 0.0, 0.0], 200);
    let joined = owner.snapshot();
    assert!(joined.warning_event_serial() > previous.warning_event_serial());
    assert!(!joined.warning_eligibility(index, 200).acknowledged);
    assert!(!joined.warning_eligibility(index, 200).snoozed);
    assert_eq!(
        owner
            .apply_warning_command(
                WarningCommand {
                    request_id: 2,
                    ..old_command
                },
                200
            )
            .rejected,
        1
    );
    owner.ingest_ignition_demo([f32::NAN, -2.0, 0.0, 0.0], 700);
    let mixed = owner.snapshot();
    assert_eq!(mixed.rule_status_at(index, 701), Status::Active);
    assert_eq!(mixed.ignition_warning_mask_at(index, 701), 0b0010);
}

/// Catches synthetic rules leaking into a replacement Vehicle owner.
#[test]
fn group_warning_is_disabled_in_vehicle_mode() {
    let config = config(false);
    let mut vehicle = AcquisitionModel::configured(&config, 0x7E8);
    assert!(!vehicle.ingest_ignition_demo([-3.0; 4], 0));
    assert_eq!(
        vehicle.snapshot().rule_status_at(RuleIndex::new(0).unwrap(), 0),
        Status::Disabled
    );
}

/// Catches old raw Active state reviving when clear data returns after a silent gap.
#[test]
fn silent_gap_requires_new_activation_and_does_not_revive_an_old_warning() {
    for latch in [false, true] {
        let config = config(latch);
        let mut owner = AcquisitionModel::demo_configured(&config);
        let index = RuleIndex::new(0).unwrap();
        for at in [0, 100] {
            owner.ingest_ignition_demo([-1.0, 0.0, 0.0, 0.0], at);
        }
        owner.ingest_ignition_demo([0.0; 4], 1_000);
        assert_ne!(owner.snapshot().rule_status_at(index, 1_000), Status::Active);
        owner.tick(1_001);
        assert_ne!(owner.snapshot().rule_status_at(index, 1_001), Status::Active);
        owner.ingest_ignition_demo([0.0; 4], 1_100);
        if latch {
            assert!(owner.snapshot().warning_eligibility(index, 1_100).cleared);
        } else {
            assert_eq!(owner.snapshot().rule_status_at(index, 1_100), Status::Normal);
        }
    }
}

/// Catches newer clear siblings filling a gap in the negative evidence supporting dwell.
#[test]
fn clear_siblings_cannot_bridge_a_gap_in_negative_support() {
    let mut candidate = config(false).config().clone();
    candidate.widgets[0].warning.on_ms = 1000;
    let config = validate(candidate).unwrap();
    let mut owner = AcquisitionModel::demo_configured(&config);
    let index = RuleIndex::new(0).unwrap();
    owner.ingest_ignition_demo([-1.0, 0.0, 0.0, 0.0], 0);
    owner.ingest_ignition_demo([f32::NAN, 0.0, 0.0, 0.0], 500);
    owner.ingest_ignition_demo([f32::NAN, -1.0, 0.0, 0.0], 1000);
    assert_eq!(owner.snapshot().rule_status_at(index, 1000), Status::Pending);
    assert_eq!(owner.snapshot().ignition_warning_mask_at(index, 1000), 0);
}

/// Catches a recovered band value inheriting a stale cylinder's old hysteresis bit.
#[test]
fn stale_member_must_retrigger_even_with_a_fresh_negative_sibling() {
    for sibling in [0.0, -3.0] {
        let mut candidate = config(false).config().clone();
        candidate.widgets[0].warning.trigger = -2.0;
        candidate.widgets[0].warning.clear = -1.0;
        let config = validate(candidate).unwrap();
        let mut owner = AcquisitionModel::demo_configured(&config);
        let index = RuleIndex::new(0).unwrap();
        for at in [0, 100] {
            owner.ingest_ignition_demo([-3.0, sibling, 0.0, 0.0], at);
        }
        owner.ingest_ignition_demo([f32::NAN, sibling, 0.0, 0.0], 600);
        owner.ingest_ignition_demo([-1.5, sibling, 0.0, 0.0], 700);
        assert_eq!(owner.snapshot().ignition_warning_mask_at(index, 700) & 1, 0);
        if sibling < 0.0 {
            assert_eq!(owner.snapshot().ignition_warning_mask_at(index, 700), 2);
        } else {
            assert_eq!(owner.snapshot().rule_status_at(index, 700), Status::Normal);
        }
    }
}

/// Catches a stale clear member completing a latch clear dwell after recovery.
#[test]
fn clear_dwell_restarts_after_any_member_gap_in_batch_and_typed_recovery() {
    use dashboard_core::ignition::IgnitionSample;
    use dashboard_core::samples::Source;
    for typed in [false, true] {
        let mut candidate = config(true).config().clone();
        candidate.widgets[0].warning.off_ms = 1000;
        let config = validate(candidate).unwrap();
        let mut owner = AcquisitionModel::demo_configured(&config);
        let index = RuleIndex::new(0).unwrap();
        for at in [0, 100] {
            owner.ingest_ignition_demo([-1.0, 0.0, 0.0, 0.0], at);
        }
        owner.ingest_ignition_demo([0.0; 4], 200);
        owner.ingest_ignition_demo([f32::NAN, 0.0, 0.0, 0.0], 700);
        if typed {
            owner
                .ingest_ignition(
                    IgnitionSample {
                        cylinder: Cylinder::One,
                        degrees: 0.0,
                        received_at_ms: 1200,
                        sequence: 4,
                        source: Source::Demo,
                    },
                    1200,
                )
                .unwrap();
            owner.ingest_ignition_demo([f32::NAN, 0.0, 0.0, 0.0], 1200);
        } else {
            owner.ingest_ignition_demo([0.0; 4], 1200);
        }
        assert!(!owner.snapshot().warning_eligibility(index, 1200).cleared);
        for at in (1300..=2200).step_by(100) {
            owner.ingest_ignition_demo([0.0; 4], at);
        }
        assert!(owner.snapshot().warning_eligibility(index, 2200).cleared);
    }
}

/// Catches processing latency breaking capture continuity or reviving untouched stale members.
#[test]
fn delayed_typed_capture_preserves_contiguous_support_without_reviving_siblings() {
    use dashboard_core::ignition::IgnitionSample;
    use dashboard_core::samples::Source;
    let mut candidate = config(false).config().clone();
    candidate.widgets[0].warning.on_ms = 500;
    candidate.widgets[0].warning.trigger = -2.0;
    candidate.widgets[0].warning.clear = -1.0;
    let config = validate(candidate).unwrap();
    let index = RuleIndex::new(0).unwrap();
    for degrees in [-3.0, -1.5] {
        let mut owner = AcquisitionModel::demo_configured(&config);
        owner.ingest_ignition_demo([-3.0, -3.0, 0.0, 0.0], 0);
        let sample = IgnitionSample {
            cylinder: Cylinder::One,
            degrees,
            received_at_ms: 500,
            sequence: 2,
            source: Source::Demo,
        };
        owner.ingest_ignition(sample, 1000).unwrap();
        assert_eq!(owner.snapshot().rule_status_at(index, 1000), Status::Active);
        assert_eq!(owner.snapshot().ignition_warning_mask_at(index, 1000), 1);
        owner
            .ingest_ignition(
                IgnitionSample {
                    cylinder: Cylinder::Two,
                    degrees: -1.5,
                    received_at_ms: 1000,
                    ..sample
                },
                1000,
            )
            .unwrap();
        assert_eq!(owner.snapshot().ignition_warning_mask_at(index, 1000), 1);

        let mut observed_gap = AcquisitionModel::demo_configured(&config);
        observed_gap.ingest_ignition_demo([-3.0; 4], 0);
        observed_gap.tick(501);
        observed_gap.ingest_ignition(sample, 1000).unwrap();
        assert_ne!(observed_gap.snapshot().rule_status_at(index, 1000), Status::Active);
    }
}

/// Catches line storage multiplication and truncation of the configured maximum message.
#[test]
fn cylinder_popup_keeps_one_message_and_four_explicit_members() {
    let mut candidate = config(false).config().clone();
    candidate.widgets[0].advanced.ignition_warning = IgnitionWarningPresentation::CylinderLines;
    candidate.widgets[0].warning.message = "ABCDEFGHIJKLMNOPQRST".try_into().unwrap();
    let config = validate(candidate).unwrap();
    let mut owner = AcquisitionModel::demo_configured(&config);
    for at in [0, 100] {
        owner.ingest_ignition_demo([-1.0; 4], at);
    }
    let page = dashboard_core::view_state::WarningPager::new()
        .select(&config, &owner.snapshot(), 1, 100, true)
        .unwrap();
    assert_eq!(page.cylinder_mask(), 0b1111);
    assert_eq!(page.lines().len(), 1);
    assert_eq!(page.lines()[0].as_str(), "ABCDEFGHIJKLMNOPQRST");
    assert_eq!(page.rules, 1);
    assert_eq!(core::mem::size_of::<dashboard_core::view_state::WarningPage>(), 88);
}

/// Catches mixed ListThree pages swallowing a group or reducing ordinary capacity.
#[test]
fn list_three_gives_cylinder_group_its_own_page_and_retains_ordinary_rows() {
    use dashboard_config::{Channel, WarningMode, default_widget};
    let mut candidate = config(false).config().clone();
    candidate.widgets.clear();
    candidate.warnings.mode = WarningMode::ListThree;
    for index in 0..7u8 {
        let placement = Placement {
            column: index % 4,
            row: index / 4,
            span: Span::OneByOne,
        };
        let mut widget = if index == 3 {
            default_ignition_widget(u16::from(index) + 1, 1, placement)
        } else {
            default_widget(Channel::Boost, u16::from(index) + 1, 1, placement)
        };
        widget.warning.enabled = true;
        widget.warning.on_ms = 100;
        widget.warning.off_ms = 100;
        if index == 3 {
            widget.advanced.ignition_warning = IgnitionWarningPresentation::CylinderLines;
        } else {
            widget.warning.trigger = 1.5;
            widget.warning.clear = 1.0;
        }
        candidate.widgets.push(widget).unwrap();
    }
    let config = validate(candidate).unwrap();
    let period = u64::from(config.config().warnings.page_ms);
    let mut owner = AcquisitionModel::demo_configured(&config);
    let mut pager = dashboard_core::view_state::WarningPager::new();
    let mut pages = Vec::new();
    for at in (0..=100 + 2 * period).step_by(100) {
        owner.ingest_batch([2.0, 90.0, 95.0, 80.0, 30.0, 700.0, 13.8, 1.0], at);
        owner.ingest_ignition_demo([-1.0; 4], at);
        if at >= 100 && (at - 100) % period == 0 {
            pages.push(pager.select(&config, &owner.snapshot(), 1, at, true).unwrap());
        }
    }
    assert_eq!(pages.len(), 3);
    assert_eq!(
        (pages[0].number, pages[0].total, pages[0].rules, pages[0].lines().len()),
        (1, 3, 0b111, 3)
    );
    assert_eq!(
        (
            pages[1].number,
            pages[1].total,
            pages[1].rules,
            pages[1].cylinder_mask()
        ),
        (2, 3, 0b1000, 15)
    );
    assert_eq!(
        (pages[2].number, pages[2].total, pages[2].rules, pages[2].lines().len()),
        (3, 3, 0b1110000, 3)
    );
}

/// Catches four longest cylinder lines escaping the popup in either native geometry.
#[test]
fn maximum_cylinder_popup_is_clipped_in_both_native_shapes() {
    use dashboard_core::display::{FrameBuffer, St7789Renderer};
    let mut candidate = config(false).config().clone();
    candidate.widgets[0].advanced.ignition_warning = IgnitionWarningPresentation::CylinderLines;
    candidate.widgets[0].warning.message = "ABCDEFGHIJKLMNOPQRST".try_into().unwrap();
    let config = validate(candidate).unwrap();
    let mut owner = AcquisitionModel::demo_configured(&config);
    for at in [0, 100] {
        owner.ingest_ignition_demo([-1.0; 4], at);
    }
    let page = dashboard_core::view_state::WarningPager::new()
        .select(&config, &owner.snapshot(), 1, 100, true)
        .unwrap();
    for geometry in [
        dashboard_config::Geometry::Landscape,
        dashboard_config::Geometry::Portrait,
    ] {
        let mut frame = Box::new(FrameBuffer::new());
        frame.fill(0xA5);
        dashboard_core::widgets::draw_warning_page(&mut St7789Renderer::with_geometry(&mut frame, geometry), page);
        let width = usize::from(geometry.width());
        let height = usize::from(geometry.height());
        let mut painted = 0;
        for y in 0..height {
            for x in 0..width {
                let pixel = &frame[(y * width + x) * 2..(y * width + x + 1) * 2];
                if x < width / 2 - 108 || x >= width / 2 + 108 || y < height / 2 - 68 || y >= height / 2 + 68 {
                    assert_eq!(pixel, [0xA5, 0xA5]);
                } else if pixel != [0xA5, 0xA5] {
                    painted += 1;
                }
            }
        }
        assert_eq!(painted, 216 * 136);
    }
}
