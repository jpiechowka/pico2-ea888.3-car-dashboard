//! Brightness controls tied to completed feedback and clear-frame presentation.
use dashboard_host_validation::backlight::{Backlight, Brightness};
use dashboard_host_validation::display::pipeline::{BufferId, FrameToken};
use dashboard_host_validation::pages::Page;
use dashboard_host_validation::popup::Popup;
use dashboard_host_validation::render::FpsMode;
use dashboard_host_validation::settings::Settings;

/// Returns a buffer-A fixture token with caller-selected `sequence` identity.
fn frame(sequence: u64) -> FrameToken {
    FrameToken {
        buffer: BufferId::A,
        sequence,
    }
}

/// Recovery from every shutdown phase restores Full and invalidates late popup/clear completions.
#[test]
fn recovery_wakes_every_off_phase_and_invalidates_old_completion() {
    for phase in 0..5 {
        let mut bl = Backlight::new();
        let off = bl.encoder(Page::Dashboard, 0, 2, 100).popup.unwrap();
        if phase >= 1 {
            bl.frame_submitted(frame(1));
        }
        if phase >= 2 {
            bl.frame_completed(frame(1), 100);
            bl.expire_popup(off, 2100);
        }
        if phase >= 3 {
            bl.frame_submitted(frame(2));
        }
        if phase >= 4 {
            bl.frame_completed(frame(2), 2150);
        }
        bl.wake_full();
        bl.frame_completed(frame(1), 2200);
        bl.frame_completed(frame(2), 2200);
        bl.frame_unchanged(2200);
        assert_eq!(bl.brightness(), Brightness::Full);
        assert_eq!(bl.compare(), 10000);
        let off = bl.encoder(Page::Debug, 0, 1, 2300).popup.unwrap();
        bl.expire_popup(off, 4000);
        bl.encoder(Page::Debug, 0, 1, 4100);
        assert_eq!(
            bl.brightness(),
            Brightness::Full,
            "recovery also resets restore brightness"
        );
    }
}

/// The two-second Off dwell starts at first popup completion, unaffected by render delay or later frames.
#[test]
fn off_countdown_starts_on_first_popup_completion_despite_render_delay() {
    let mut bl = Backlight::new();
    let popup = bl.encoder(Page::Dashboard, 0, 2, 100).popup.unwrap();
    assert!(!bl.expire_popup(popup, 4000), "unpresented popup must not expire");
    assert_eq!(bl.compare(), 5000);
    bl.frame_submitted(frame(1));
    bl.frame_completed(frame(1), 4500);
    bl.frame_submitted(frame(2));
    bl.frame_completed(frame(2), 4600);
    assert!(!bl.expire_popup(popup, 6499));
    assert!(
        bl.expire_popup(popup, 6500),
        "later popup frames must not restart countdown"
    );
    bl.frame_completed(frame(2), 6501);
    assert_eq!(bl.compare(), 5000);
    bl.frame_submitted(frame(3));
    bl.frame_completed(frame(3), 6530);
    assert_eq!(bl.compare(), 0);
}

/// Repeated identical Off feedback gets a fresh dwell from unchanged-frame acknowledgment.
#[test]
fn identical_off_popup_starts_a_fresh_dwell_without_a_new_transfer() {
    let mut bl = Backlight::new();
    bl.encoder(Page::Dashboard, 0, 2, 100);
    bl.frame_submitted(frame(1));
    bl.frame_completed(frame(1), 100);
    // Three clicks while the same OFF popup is already on the panel. The
    // final framebuffer is identical, so the renderer can legitimately idle.
    let again = bl.encoder(Page::Dashboard, 0, 3, 1000).popup.unwrap();
    bl.frame_unchanged(1000);
    assert!(!bl.expire_popup(again, 2999));
    assert!(bl.expire_popup(again, 3000));
    bl.frame_submitted(frame(2));
    bl.frame_completed(frame(2), 3010);
    assert_eq!(bl.compare(), 0);
}

/// Every saved brightness boots Full while preserving units and FPS preferences.
#[test]
fn legacy_off_or_dim_settings_boot_full_and_keep_other_preferences() {
    for previous in 0..=100 {
        let saved = Settings::new(previous, 37, true, FpsMode::Combined).unwrap();
        let boot = saved.for_boot();
        assert_eq!(boot.brightness(), 100);
        assert_eq!(boot.restore_brightness(), 100);
        assert!(boot.boost_psi());
        assert_eq!(boot.fps(), FpsMode::Combined);
    }
}

/// Dashboard clicks cycle exact Full/Dim/Off duties; shutdown requires the matching clear-frame lease.
#[test]
fn dashboard_click_cycles_exact_pwm_levels_and_ignores_rotation() {
    let mut bl = Backlight::new();
    assert_eq!(bl.compare(), 10_000);
    assert!(bl.encoder(Page::Dashboard, -128, 0, 0).popup.is_none());
    assert_eq!(bl.compare(), 10_000);
    let dim = bl.encoder(Page::Dashboard, 0, 1, 100);
    assert!(dim.press_applied);
    assert!(matches!(dim.popup, Some(Popup::Brightness(100, Brightness::Dim))));
    assert_eq!(bl.compare(), 5_000);
    let off = bl.encoder(Page::Dashboard, 0, 1, 200).popup.unwrap();
    bl.frame_submitted(frame(1));
    bl.frame_completed(frame(1), 200);
    assert_eq!(bl.compare(), 5_000, "OFF feedback must remain lit");
    assert!(!off.is_expired_at(2199));
    assert!(off.is_expired_at(2200));
    bl.expire_popup(off, 2199);
    bl.frame_submitted(frame(1));
    bl.frame_completed(frame(1), 2300);
    assert_eq!(bl.compare(), 5_000, "early expiry must not arm shutdown");
    bl.expire_popup(off, 2200);
    bl.frame_completed(frame(1), 2300);
    assert_eq!(bl.compare(), 5_000, "old popup frame completion must not switch off");
    bl.frame_submitted(frame(2));
    bl.frame_completed(
        FrameToken {
            buffer: BufferId::B,
            sequence: 2,
        },
        2300,
    );
    assert_eq!(bl.compare(), 5_000, "wrong lease must not switch off");
    bl.frame_completed(frame(2), 2300);
    assert_eq!(bl.compare(), 0);
    bl.encoder(Page::Dashboard, 0, 1, 2300);
    assert_eq!(bl.compare(), 10_000);
    bl.frame_completed(frame(2), 2300);
    assert_eq!(bl.compare(), 10_000, "late completion cannot switch off again");
}

/// Coalesced clicks retain every modulo-three transition, keeping Off feedback lit at half duty.
#[test]
fn every_coalesced_dashboard_press_is_counted_modulo_three() {
    for (presses, wanted, duty) in [
        (1, Brightness::Dim, 5000),
        (2, Brightness::Off, 5000),
        (3, Brightness::Full, 10000),
        (4, Brightness::Dim, 5000),
        (5, Brightness::Off, 5000),
        (6, Brightness::Full, 10000),
        (7, Brightness::Dim, 5000),
        (8, Brightness::Off, 5000),
    ] {
        let mut bl = Backlight::new();
        let event = bl.encoder(Page::Dashboard, 0, presses, 50);
        assert!(event.press_applied);
        assert_eq!(bl.brightness(), wanted, "{presses} clicks");
        assert_eq!(bl.compare(), duty, "{presses} clicks");
    }
}

/// A subsequent click cancels shutdown before or after clear submission despite late acknowledgments.
#[test]
fn dashboard_shutdown_can_be_cancelled_before_or_after_clear_submission() {
    for submitted in [false, true] {
        let mut bl = Backlight::new();
        let off = bl.encoder(Page::Dashboard, 0, 2, 100).popup.unwrap();
        bl.frame_submitted(frame(1));
        bl.frame_completed(frame(1), 100);
        if submitted {
            bl.expire_popup(off, 2100);
            bl.frame_submitted(frame(1));
        }
        bl.encoder(Page::Dashboard, 0, 1, 2200);
        bl.expire_popup(off, 2300);
        bl.frame_completed(frame(1), 2300);
        bl.frame_unchanged(2300);
        assert_eq!(bl.compare(), 10_000);
    }
}

/// Replaced feedback or navigation resolves pending Dashboard Off to Dim without blackout.
#[test]
fn page_change_or_replacement_cancels_dashboard_off_without_blackout() {
    for clearing in [false, true] {
        let mut bl = Backlight::new();
        let off = bl.encoder(Page::Dashboard, 0, 2, 100).popup.unwrap();
        bl.frame_submitted(frame(1));
        bl.frame_completed(frame(1), 100);
        if clearing {
            bl.expire_popup(off, 2100);
            bl.frame_submitted(frame(5));
        }
        bl.interrupt_feedback();
        bl.expire_popup(off, 2200);
        bl.frame_completed(frame(5), 2300);
        assert_eq!(bl.compare(), 5000);
        assert_eq!(bl.brightness(), Brightness::Dim);
    }
}

/// An unchanged cleared presentation can finish shutdown only after popup expiry has armed it.
#[test]
fn unchanged_cleared_frame_can_finish_shutdown_without_dma() {
    let mut bl = Backlight::new();
    let off = bl.encoder(Page::Dashboard, 0, 2, 100).popup.unwrap();
    bl.frame_submitted(frame(1));
    bl.frame_completed(frame(1), 100);
    bl.frame_unchanged(2300);
    assert_eq!(bl.compare(), 5000);
    bl.expire_popup(off, 2100);
    bl.frame_unchanged(2300);
    assert_eq!(bl.compare(), 0);
}

/// Debug retains nonlinear percent PWM, 1.5-second toggle feedback and bounded extreme rotation.
#[test]
fn debug_keeps_legacy_rotation_toggle_and_short_popup() {
    let mut bl = Backlight::new();
    let event = bl.encoder(Page::Debug, -1, 0, 100);
    assert!(event.motion_applied);
    assert_eq!(bl.compare(), 9725);
    assert!(matches!(
        event.popup,
        Some(Popup::Brightness(_, Brightness::Percent(95)))
    ));
    let off = bl.encoder(Page::Debug, 0, 1, 200).popup.unwrap();
    assert!(!off.is_expired_at(1699));
    bl.expire_popup(off, 1700);
    assert_eq!(bl.compare(), 0);
    bl.encoder(Page::Debug, 0, 1, 1800);
    assert_eq!(bl.compare(), 9725);
    assert!(bl.encoder(Page::Debug, 0, 2, 1900).popup.is_none());
    assert_eq!(bl.compare(), 9725);
    bl.encoder(Page::Debug, i32::MIN, 0, 2000);
    assert_eq!(bl.compare(), 0);
    bl.encoder(Page::Debug, i32::MAX, 0, 2100);
    assert_eq!(bl.compare(), 10_000);
}

/// Logs ignore brightness controls; interrupted Debug toggles preserve the exact restore duty.
#[test]
fn logs_encoder_never_changes_backlight_and_debug_interruption_preserves_toggle() {
    let mut bl = Backlight::new();
    bl.encoder(Page::Dashboard, 0, 1, 0);
    let event = bl.encoder(Page::Logs, 128, 8, 100);
    assert!(event.popup.is_none());
    assert!(!event.press_applied && !event.motion_applied);
    assert_eq!(bl.compare(), 5000);
    bl.encoder(Page::Debug, 0, 1, 200);
    bl.interrupt_feedback();
    assert_eq!(bl.compare(), 0);
    bl.encoder(Page::Debug, 0, 1, 300);
    assert_eq!(bl.compare(), 5000, "Debug restores exact preset, without rounding duty");
}

/// Reversed popup time cancels pending Off safely; a dwell ending at u64::MAX still expires.
#[test]
fn regressed_popup_clock_keeps_light_on_and_deadline_handles_u64_end() {
    let mut bl = Backlight::new();
    let off = bl.encoder(Page::Dashboard, 0, 2, 100).popup.unwrap();
    bl.frame_submitted(frame(1));
    bl.frame_completed(frame(1), 100);
    bl.expire_popup(off, 99);
    bl.frame_unchanged(2300);
    assert_eq!(bl.compare(), 5000);
    assert_eq!(bl.brightness(), Brightness::Dim);
    let off = bl.encoder(Page::Dashboard, 0, 1, u64::MAX - 2000).popup.unwrap();
    bl.frame_submitted(frame(1));
    bl.frame_completed(frame(1), u64::MAX - 2000);
    assert!(!off.is_expired_at(u64::MAX - 1));
    bl.expire_popup(off, u64::MAX);
    bl.frame_unchanged(2300);
    assert_eq!(bl.compare(), 0);
}
