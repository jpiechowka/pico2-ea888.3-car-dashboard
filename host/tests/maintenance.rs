//! Generation-bound owner parking/resume and finite flash-write permission across cancellation/fault paths.
use dashboard_host_validation::health::{Progress, ProgressMonitor};
use dashboard_host_validation::maintenance::{Command, Error, Gate, Owner};

/// Returns idle-display progress with acquisition/render/input generation `n`
/// and no submitted/completed transfers, suitable for maintenance admission.
fn progress(n: u32) -> Progress {
    Progress {
        acquisition: n,
        rendering: n,
        input: n,
        completed: 0,
        submitted: 0,
        display_pending: false,
    }
}

/// Optional-radio gates require radio park/resume acknowledgments exactly when that owner was configured.
#[test]
fn radio_is_a_required_owner_only_in_the_radio_gate() {
    for radio in [false, true] {
        let mut health = ProgressMonitor::new(0, progress(0));
        let ticket = health.begin_maintenance(0, progress(0)).unwrap();
        let mut gate = Gate::with_radio(radio);
        let request = gate.begin(&ticket).unwrap();
        let park = Command::Park(request);
        for owner in [Owner::Acquisition, Owner::Input] {
            assert!(gate.acknowledge(park, owner, true));
        }
        assert_eq!(gate.ready(1), Ok(!radio));
        assert_eq!(gate.acknowledge(park, Owner::Radio, true), radio);
        assert_eq!(gate.begin_write(2), Ok(()));
        assert_eq!(gate.writing(request, 3), Ok(()));
        gate.release(request).unwrap();
        assert!(!gate.acknowledge(park, Owner::Radio, true));
        for owner in [Owner::Input, Owner::Acquisition] {
            assert!(gate.acknowledge(Command::Resume(request), owner, true));
        }
        assert_eq!(gate.resumed(4), Ok(!radio));
        assert_eq!(gate.acknowledge(Command::Resume(request), Owner::Radio, true), radio);
        assert_eq!(gate.finish(request, 5), Ok(()));
    }
}

/// Other owners cannot hide failed/missing radio parking; explicit resume cleanup remains possible after rejected
/// preparation.
#[test]
fn radio_failure_or_missing_ack_cannot_be_masked_by_other_owners() {
    for success in [None, Some(false)] {
        let mut health = ProgressMonitor::new(0, progress(0));
        let ticket = health.begin_maintenance(0, progress(0)).unwrap();
        let mut gate = Gate::with_radio(true);
        let request = gate.begin(&ticket).unwrap();
        for owner in [Owner::Acquisition, Owner::Input] {
            gate.acknowledge(Command::Park(request), owner, true);
        }
        if let Some(ok) = success {
            gate.acknowledge(Command::Park(request), Owner::Radio, ok);
        }
        assert!(gate.begin_write(400).is_err());
        assert!(gate.begin_write(401).is_err());
        gate.release(request).unwrap();
        for owner in [Owner::Radio, Owner::Input, Owner::Acquisition] {
            assert!(gate.acknowledge(Command::Resume(request), owner, true));
        }
        assert_eq!(gate.finish(request, 402), Ok(()));
    }
}

/// Both owners must acknowledge each phase once for the current generation; health exits only after matching resume
/// completion.
#[test]
fn both_owners_must_park_and_resume_for_the_same_generation() {
    for owners in [[Owner::Acquisition, Owner::Input], [Owner::Input, Owner::Acquisition]] {
        let mut health = ProgressMonitor::new(0, progress(0));
        let ticket = health.begin_maintenance(0, progress(0)).unwrap();
        let mut gate = Gate::new();
        let request = gate.begin(&ticket).unwrap();
        let park = Command::Park(request);
        assert_eq!(gate.command(), Some(park));
        assert_eq!(gate.ready(0), Ok(false));
        assert!(gate.acknowledge(park, owners[0], true));
        assert!(!gate.acknowledge(park, owners[0], true));
        assert_eq!(gate.ready(100), Ok(false));
        assert!(gate.acknowledge(park, owners[1], true));
        assert_eq!(gate.ready(400), Ok(true));
        gate.release(request).unwrap();
        assert_eq!(gate.ready(401), Err(Error::State));
        assert!(gate.finish(request, 401).is_err());
        assert!(!gate.acknowledge(park, owners[0], true));
        assert!(gate.acknowledge(Command::Resume(request), owners[0], true));
        assert_eq!(gate.resumed(500), Ok(false));
        assert!(gate.acknowledge(Command::Resume(request), owners[1], true));
        assert_eq!(gate.resumed(1000), Ok(true));
        gate.finish(request, 1000).unwrap();
        health.end_maintenance(ticket, 1000, progress(0)).unwrap();
        assert_eq!(gate.command(), None);
        assert!(gate.finish(request, 1000).is_err());
    }
}

/// Failed, duplicate, old and timed-out acknowledgments never authorize writes, and expiry remains sticky across clock
/// rollback.
#[test]
fn late_duplicate_failed_and_old_acknowledgements_cannot_authorize_a_write() {
    let mut health = ProgressMonitor::new(0, progress(0));
    let ticket = health.begin_maintenance(0, progress(0)).unwrap();
    let mut gate = Gate::new();
    let first = gate.begin(&ticket).unwrap();
    assert!(gate.begin(&ticket).is_err());
    let park = Command::Park(first);
    assert!(gate.acknowledge(park, Owner::Input, false));
    assert!(!gate.acknowledge(park, Owner::Input, true));
    assert!(gate.acknowledge(park, Owner::Acquisition, true));
    assert_eq!(gate.ready(100), Err(Error::Owner));
    gate.release(first).unwrap();
    for owner in [Owner::Input, Owner::Acquisition] {
        gate.acknowledge(Command::Resume(first), owner, true);
    }
    gate.finish(first, 101).unwrap();
    assert!(gate.begin(&ticket).is_err());
    health.end_maintenance(ticket, 101, progress(0)).unwrap();
    let next = health.begin_maintenance(102, progress(1)).unwrap();
    let second = gate.begin(&next).unwrap();
    assert!(!gate.acknowledge(park, Owner::Input, true));
    assert!(gate.release(first).is_err());
    for owner in [Owner::Input, Owner::Acquisition] {
        gate.acknowledge(Command::Park(second), owner, true);
    }
    assert_eq!(gate.ready(503), Err(Error::Timeout));
    assert_eq!(gate.ready(502), Err(Error::Timeout)); // expiry stays rejected
    gate.release(second).unwrap(); // cleanup still possible after rejected preparation
    assert_eq!(gate.resumed(1103), Err(Error::Timeout));
    assert_eq!(gate.resumed(1102), Err(Error::Timeout));
}

/// Cancellation still requires resume acknowledgments; clock or resume-owner failure prevents a clean finish.
#[test]
fn cancelled_preparation_and_clock_failure_still_require_resume_acknowledgements() {
    let mut health = ProgressMonitor::new(100, progress(0));
    let ticket = health.begin_maintenance(100, progress(0)).unwrap();
    let mut gate = Gate::new();
    let request = gate.begin(&ticket).unwrap();
    assert_eq!(gate.ready(99), Err(Error::Clock));
    assert_eq!(gate.ready(100), Err(Error::Clock));
    gate.release(request).unwrap();
    assert_eq!(gate.resumed(100), Err(Error::Clock));
    for owner in [Owner::Input, Owner::Acquisition] {
        gate.acknowledge(Command::Resume(request), owner, true);
    }
    assert!(gate.finish(request, 101).is_err());

    let mut gate = Gate::new();
    let request = gate.begin(&ticket).unwrap();
    gate.release(request).unwrap(); // cancellation before either owner parks
    assert_eq!(gate.resumed(100), Ok(false));
    assert!(gate.acknowledge(Command::Resume(request), Owner::Input, true));
    assert!(gate.acknowledge(Command::Resume(request), Owner::Acquisition, false));
    assert_eq!(gate.resumed(101), Err(Error::Owner));
    assert!(gate.finish(request, 101).is_err());
}

/// The write phase starts only once after all park acknowledgments and cannot extend its absolute maintenance deadline.
#[test]
fn write_window_starts_once_after_both_acks_and_retains_absolute_limit() {
    let mut health = ProgressMonitor::new(0, progress(0));
    let ticket = health.begin_maintenance(0, progress(0)).unwrap();
    let mut gate = Gate::new();
    let request = gate.begin(&ticket).unwrap();
    assert_eq!(gate.begin_write(1), Err(Error::Pending));
    for owner in [Owner::Input, Owner::Acquisition] {
        gate.acknowledge(Command::Park(request), owner, true);
    }
    gate.begin_write(400).unwrap();
    assert_eq!(gate.ready(400), Err(Error::State));
    assert_eq!(gate.begin_write(401), Err(Error::State));
    assert_eq!(gate.writing(request, 900), Ok(()));
    assert_eq!(gate.writing(request, 901), Err(Error::Timeout));
    assert_eq!(gate.writing(request, 900), Err(Error::Timeout));
    gate.release(request).unwrap();
    assert_eq!(gate.writing(request, 902), Err(Error::Timeout));
    for owner in [Owner::Input, Owner::Acquisition] {
        gate.acknowledge(Command::Resume(request), owner, true);
    }
    assert_eq!(gate.finish(request, 999), Err(Error::Timeout));
}

/// Clock reversal in park/write/resume permanently poisons completion even after releasing owners.
#[test]
fn regression_inside_each_window_is_fatal_even_after_release() {
    for phase in 0..3 {
        let mut health = ProgressMonitor::new(100, progress(0));
        let ticket = health.begin_maintenance(100, progress(0)).unwrap();
        let mut gate = Gate::new();
        let request = gate.begin(&ticket).unwrap();
        for owner in [Owner::Input, Owner::Acquisition] {
            gate.acknowledge(Command::Park(request), owner, true);
        }
        match phase {
            0 => {
                assert_eq!(gate.ready(200), Ok(true));
                assert_eq!(gate.ready(199), Err(Error::Clock));
                gate.release(request).unwrap();
            }
            1 => {
                gate.begin_write(200).unwrap();
                assert_eq!(gate.writing(request, 300), Ok(()));
                assert_eq!(gate.writing(request, 299), Err(Error::Clock));
                gate.release(request).unwrap();
            }
            _ => {
                gate.ready(200).unwrap();
                gate.release(request).unwrap();
                assert_eq!(gate.resumed(199), Err(Error::Clock));
            }
        }
        for owner in [Owner::Input, Owner::Acquisition] {
            gate.acknowledge(Command::Resume(request), owner, true);
        }
        assert_eq!(gate.finish(request, 400), Err(Error::Clock));
    }
}
