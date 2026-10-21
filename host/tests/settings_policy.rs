//! Save debounce, affine in-flight snapshots, retry limits and uncertain persistence state.
use dashboard_host_validation::render::FpsMode;
use dashboard_host_validation::settings::Settings;
use dashboard_host_validation::settings::policy::{Policy, PolicyError, Status};

/// Returns valid preferences with brightness percent `n`, 100% restore brightness,
/// BAR and hidden FPS; panics if `n` violates settings validation.
fn choice(n: u8) -> Settings { Settings::new(n, 100, false, FpsMode::Off).unwrap() }

/// Equal settings do not extend debounce; only one save ticket exists and clean state never writes periodically.
#[test]
fn changes_debounce_without_periodic_rewrites_or_overlapping_tickets() {
    let mut policy = Policy::new(choice(100), 0);
    assert_eq!(policy.status(), Status::Clean);
    assert!(policy.begin(10000).unwrap().is_none());
    policy.set(choice(50), 10000).unwrap();
    policy.set(choice(50), 11000).unwrap(); // same value must not extend debounce
    assert!(policy.begin(11999).unwrap().is_none());
    let ticket = policy.begin(12000).unwrap().unwrap();
    assert_eq!(ticket.settings(), choice(50));
    assert_eq!(policy.status(), Status::Saving);
    assert!(policy.begin(13000).unwrap().is_none());
    policy.finish(ticket, true, 13000).unwrap();
    assert_eq!(policy.status(), Status::Clean);
    assert!(policy.begin(u64::MAX).unwrap().is_none());
}

/// Completing an old save cannot acknowledge newer/reverted preferences; the next write still obeys rate limiting.
#[test]
fn in_flight_snapshot_cannot_mark_newer_settings_saved_even_when_reverting() {
    let mut policy = Policy::new(choice(100), 0);
    policy.set(choice(50), 0).unwrap();
    let ticket = policy.begin(2000).unwrap().unwrap();
    policy.set(choice(100), 2001).unwrap();
    policy.finish(ticket, true, 2100).unwrap();
    assert_eq!(policy.status(), Status::Dirty);
    assert!(policy.begin(31999).unwrap().is_none());
    let ticket = policy.begin(32000).unwrap().unwrap();
    assert_eq!(ticket.settings(), choice(100));
    policy.finish(ticket, true, 32001).unwrap();
    assert_eq!(policy.status(), Status::Clean);
}

/// Uncertain failed writes need persistence even after reversion; three retries exhaust until a new preference edit.
#[test]
fn failures_retry_three_times_at_most_and_do_not_assume_flash_unchanged() {
    let mut policy = Policy::new(choice(100), 0);
    policy.set(choice(50), 0).unwrap();
    let ticket = policy.begin(2000).unwrap().unwrap();
    // Hardware might have committed before a readback error. Reverting to the
    // previously acknowledged value must still perform a save.
    policy.set(choice(100), 2001).unwrap();
    policy.finish(ticket, false, 2100).unwrap();
    assert_eq!(policy.status(), Status::Failed);
    for now in [32000, 62000, 92000] {
        let ticket = policy.begin(now).unwrap().unwrap();
        policy.finish(ticket, false, now + 1).unwrap();
    }
    assert!(policy.begin(1_000_000).unwrap().is_none());
    assert_eq!(policy.status(), Status::Failed);
    policy.set(choice(99), 1_000_001).unwrap();
    assert!(policy.begin(1_002_001).unwrap().is_some());
}

/// Clock reversal disables further persistence, while explicitly disabled policy still permits live preference changes.
#[test]
fn corrupt_clock_and_disabled_persistence_never_start_or_renew_writes() {
    let mut policy = Policy::new(choice(100), 100);
    policy.set(choice(50), 100).unwrap();
    assert_eq!(policy.begin(99).err(), Some(PolicyError::Clock));
    assert_eq!(policy.status(), Status::Disabled);
    assert!(policy.begin(10000).is_err());
    let mut policy = Policy::disabled(choice(100), 0);
    policy.set(choice(50), 100).unwrap(); // allow the live UI preference
    assert_eq!(policy.status(), Status::Disabled);
    assert!(policy.begin(10000).unwrap().is_none());
}
