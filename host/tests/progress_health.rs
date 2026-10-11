use dashboard_host_validation::health::{Fault, Progress, ProgressMonitor};

fn progress(sequence: u32) -> Progress {
    Progress {
        acquisition: sequence,
        rendering: sequence,
        input: sequence,
        completed: sequence,
        submitted: 0,
        display_pending: true,
    }
}

#[test]
fn healthy_progress_and_counter_wrap_keep_watchdog_eligible() {
    let mut monitor = ProgressMonitor::new(0, progress(u32::MAX));
    for step in 0..1000 {
        assert_eq!(monitor.observe(step as u64 * 50, progress(step)), Ok(()));
    }
}

#[test]
fn stopped_acquisition_display_and_rendering_are_independent_faults() {
    for expected in [Fault::Acquisition, Fault::Display, Fault::Rendering, Fault::Input] {
        let mut monitor = ProgressMonitor::new(0, progress(0));
        let mut result = Ok(());
        for step in 1..=10 {
            let mut current = progress(step);
            match expected {
                Fault::Acquisition => current.acquisition = 0,
                Fault::Display => current.completed = 0,
                Fault::Rendering => current.rendering = 0,
                Fault::Input => current.input = 0,
                _ => unreachable!(),
            }
            result = monitor.observe(step as u64 * 50, current);
            if result.is_err() {
                break;
            }
        }
        assert_eq!(result, Err(expected));
        assert_eq!(
            monitor.observe(1000, progress(100)),
            Err(expected),
            "fault must latch until reset"
        );
    }
}

#[test]
fn idle_display_still_requires_render_and_acquisition_progress() {
    let mut initial = progress(0);
    initial.display_pending = false;
    let mut monitor = ProgressMonitor::new(0, initial);
    for step in 1..=100 {
        let mut current = progress(step);
        current.completed = 0;
        current.display_pending = false;
        assert_eq!(monitor.observe(step as u64 * 50, current), Ok(()));
    }
    let mut resumed = progress(101);
    resumed.completed = 0;
    assert_eq!(
        monitor.observe(5050, resumed),
        Ok(()),
        "new work gets a completion deadline"
    );
    resumed.acquisition += 1;
    resumed.rendering += 1;
    assert_eq!(monitor.observe(5300, resumed), Err(Fault::Display));
}

#[test]
fn clock_regression_and_deadline_edges_are_checked() {
    let mut monitor = ProgressMonitor::new(100, progress(0));
    assert_eq!(monitor.observe(99, progress(1)), Err(Fault::Clock));
    let mut monitor = ProgressMonitor::new(u64::MAX - 250, progress(0));
    let mut current = progress(1);
    current.completed = 0;
    assert_eq!(monitor.observe(u64::MAX - 1, current), Ok(()));
    assert_eq!(monitor.observe(u64::MAX, current), Err(Fault::Display));
    let mut monitor = ProgressMonitor::new(0, progress(0));
    assert_eq!(monitor.observe(251, progress(1)), Err(Fault::Supervisor));
}

#[test]
fn a_new_queue_generation_starts_its_deadline_even_if_idle_was_not_sampled() {
    let mut initial = progress(0);
    initial.submitted = u32::MAX;
    let mut monitor = ProgressMonitor::new(0, initial);
    let mut current = progress(1);
    current.submitted = u32::MAX;
    assert_eq!(monitor.observe(50, current), Ok(())); // previous job completed
    current.acquisition += 1;
    current.rendering += 1;
    current.input += 1;
    assert_eq!(monitor.observe(210, current), Ok(()));
    // Idle -> queued happened between observations, with no new completion yet.
    current.submitted = 0;
    current.acquisition += 1;
    current.rendering += 1;
    current.input += 1;
    assert_eq!(monitor.observe(300, current), Ok(()));
    current.acquisition += 1;
    current.rendering += 1;
    current.input += 1;
    assert_eq!(monitor.observe(549, current), Ok(()));
    assert_eq!(monitor.observe(550, current), Err(Fault::Display));
}

fn idle(sequence: u32) -> Progress {
    Progress {
        display_pending: false,
        completed: 0,
        ..progress(sequence)
    }
}

#[test]
fn maintenance_is_one_finite_window_without_watchdog_feeds() {
    let mut monitor = ProgressMonitor::new(0, idle(0));
    let ticket = monitor.begin_maintenance(50, idle(1)).unwrap();
    assert!(!monitor.feeding_allowed());
    // Intentional flash stalls may exceed the normal supervisor/task limits.
    assert_eq!(monitor.observe(900, idle(1)), Ok(()));
    assert_eq!(monitor.end_maintenance(ticket, 1050, idle(1)), Ok(()));
    assert!(monitor.feeding_allowed());
    assert_eq!(monitor.observe(1250, idle(1)), Ok(()));
    assert_eq!(monitor.observe(1450, idle(1)), Ok(()));
    assert_eq!(monitor.observe(1550, idle(1)), Err(Fault::Acquisition));
}

#[test]
fn expired_or_nested_maintenance_latches_and_cannot_be_renewed() {
    let mut monitor = ProgressMonitor::new(0, idle(0));
    let ticket = monitor.begin_maintenance(0, idle(0)).unwrap();
    assert_eq!(monitor.observe(1001, idle(100)), Err(Fault::Maintenance));
    assert_eq!(
        monitor.end_maintenance(ticket, 1001, idle(100)),
        Err(Fault::Maintenance)
    );
    assert!(!monitor.feeding_allowed());
    assert!(monitor.begin_maintenance(1001, idle(100)).is_err());

    let mut monitor = ProgressMonitor::new(0, idle(0));
    let ticket = monitor.begin_maintenance(0, idle(0)).unwrap();
    assert!(monitor.begin_maintenance(1, idle(1)).is_err());
    assert_eq!(monitor.end_maintenance(ticket, 2, idle(2)), Err(Fault::Maintenance));
}

#[test]
fn maintenance_requires_healthy_idle_owners_and_fresh_progress_between_windows() {
    let mut monitor = ProgressMonitor::new(0, idle(0));
    assert!(monitor.begin_maintenance(251, idle(1)).is_err());
    assert_eq!(monitor.observe(251, idle(1)), Err(Fault::Supervisor));
    let mut monitor = ProgressMonitor::new(0, progress(0));
    assert!(monitor.begin_maintenance(1, progress(1)).is_err());
    assert_eq!(monitor.observe(2, idle(2)), Err(Fault::Display));
    for stuck in 0..3 {
        let mut monitor = ProgressMonitor::new(0, idle(0));
        let ticket = monitor.begin_maintenance(0, idle(0)).unwrap();
        monitor.end_maintenance(ticket, 100, idle(1)).unwrap();
        let mut next = idle(2);
        match stuck {
            0 => next.acquisition = 1,
            1 => next.rendering = 1,
            _ => next.input = 1,
        }
        assert!(monitor.begin_maintenance(150, next).is_err());
        assert!(!monitor.feeding_allowed());
    }
    let mut monitor = ProgressMonitor::new(0, idle(0));
    let first = monitor.begin_maintenance(0, idle(0)).unwrap();
    let first_id = first.generation();
    monitor.end_maintenance(first, 1000, idle(1)).unwrap();
    let second = monitor.begin_maintenance(1050, idle(2)).unwrap();
    assert!(second.generation() > first_id);
    monitor.end_maintenance(second, 1051, idle(2)).unwrap();
}

#[test]
fn maintenance_detects_clock_and_unexpected_display_activity() {
    for mutation in 0..3 {
        let mut monitor = ProgressMonitor::new(100, idle(0));
        let ticket = monitor.begin_maintenance(100, idle(0)).unwrap();
        let mut current = idle(0);
        match mutation {
            0 => current.display_pending = true,
            1 => current.submitted = 1,
            _ => current.completed = 1,
        }
        assert_eq!(monitor.end_maintenance(ticket, 101, current), Err(Fault::Display));
    }
    let mut monitor = ProgressMonitor::new(100, idle(0));
    let ticket = monitor.begin_maintenance(100, idle(0)).unwrap();
    assert_eq!(monitor.end_maintenance(ticket, 99, idle(0)), Err(Fault::Clock));
    let mut monitor = ProgressMonitor::new(u64::MAX - 1000, idle(0));
    let ticket = monitor.begin_maintenance(u64::MAX - 1000, idle(0)).unwrap();
    assert_eq!(monitor.end_maintenance(ticket, u64::MAX, idle(0)), Ok(()));
}
