use dashboard_host_validation::health::{Fault, Progress, ProgressMonitor};

fn progress(sequence: u32) -> Progress {
    Progress {
        acquisition: sequence,
        rendering: sequence,
        completed: sequence,
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
    for expected in [Fault::Acquisition, Fault::Display, Fault::Rendering] {
        let mut monitor = ProgressMonitor::new(0, progress(0));
        let mut result = Ok(());
        for step in 1..=10 {
            let mut current = progress(step);
            match expected {
                Fault::Acquisition => current.acquisition = 0,
                Fault::Display => current.completed = 0,
                Fault::Rendering => current.rendering = 0,
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
