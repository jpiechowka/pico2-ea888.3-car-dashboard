//! Read-only probe supervisor. No browser-controlled executable or arguments.
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
static CANCEL: AtomicBool = AtomicBool::new(false);
pub(crate) fn request_cancel() { CANCEL.store(true, Ordering::SeqCst); }
pub(crate) fn reset_cancel() { CANCEL.store(false, Ordering::SeqCst); }
pub(crate) fn cancelled() -> bool { CANCEL.load(Ordering::SeqCst) }
static CLEANUP_FAILED: AtomicBool = AtomicBool::new(false);
pub(crate) fn cleanup_failed() -> bool { CLEANUP_FAILED.load(Ordering::SeqCst) }
use std::time::Duration;
#[derive(Debug)]
pub(crate) struct Output {
    pub code: Option<i32>,
    pub stdout: String,
    pub raw_stdout: Vec<u8>,
    pub stderr: String,
    pub raw_stderr: Vec<u8>,
}
#[cfg(windows)]
mod windows;
thread_local! {static PROBE_DEADLINE:std::cell::Cell<Option<std::time::Instant>>=const{std::cell::Cell::new(None)};}
pub(crate) fn with_probe_deadline<T>(
    duration: Duration,
    work: impl FnOnce() -> T,
) -> T {
    struct Reset(Option<std::time::Instant>);
    impl Drop for Reset {
        fn drop(&mut self) { PROBE_DEADLINE.set(self.0); }
    }
    let previous = PROBE_DEADLINE.replace(Some(std::time::Instant::now() + duration));
    let _reset = Reset(previous);
    work()
}
pub(crate) fn run(
    path: &Path,
    args: &[&str],
    cwd: &Path,
) -> Result<Output, String> {
    if cancelled() {
        return Err("cancelled: probe check cancelled".into());
    }
    if cleanup_failed() {
        return Err("cleanup fatal: no further probes permitted".into());
    }
    if !path.is_absolute() {
        return Err("probe executable must be absolute".into());
    }
    #[cfg(windows)]
    {
        let timeout = PROBE_DEADLINE
            .get()
            .map(|deadline| {
                deadline
                    .saturating_duration_since(std::time::Instant::now())
                    .min(Duration::from_secs(15))
            })
            .unwrap_or(Duration::from_secs(15));
        if timeout.is_zero() {
            return Err("deadline: inspection budget exhausted".into());
        }
        let result = windows::run(path, args, cwd, timeout, 65536);
        if result.as_ref().is_err_and(|e| e.starts_with("cleanup fatal:")) {
            CLEANUP_FAILED.store(true, Ordering::SeqCst);
        }
        result
    }
    #[cfg(not(windows))]
    {
        let _ = (path, args, cwd);
        Err("native checks require Windows 10/11 x64".into())
    }
}

/// Fixed capture command only. Build output is bounded independently of short probes.
pub(crate) fn run_capture(
    python: &Path,
    args: &[&str],
    cwd: &Path,
) -> Result<Output, String> {
    run_capture_stream(python, args, cwd, &mut |_, _| {})
}
pub(crate) fn run_capture_stream(
    python: &Path,
    args: &[&str],
    cwd: &Path,
    sink: &mut dyn FnMut(bool, &str),
) -> Result<Output, String> {
    if cancelled() || cleanup_failed() {
        return Err("Capture unavailable after cancellation or cleanup failure".into());
    }
    if !python.is_absolute() {
        return Err("Python executable must be absolute".into());
    }
    #[cfg(windows)]
    {
        let result = windows::run_stream(python, args, cwd, Duration::from_secs(1800), 8 * 1024 * 1024, sink);
        if result.as_ref().is_err_and(|e| e.starts_with("cleanup fatal:")) {
            CLEANUP_FAILED.store(true, Ordering::SeqCst);
        }
        result
    }
    #[cfg(not(windows))]
    {
        let _ = (python, args, cwd);
        Err("Windows native host required".into())
    }
}
