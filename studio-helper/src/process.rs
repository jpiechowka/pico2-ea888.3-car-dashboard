//! Read-only probe supervisor. No browser-controlled executable or arguments.
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
static CLEANUP_FAILED: AtomicBool = AtomicBool::new(false);
pub(crate) fn cleanup_failed() -> bool { CLEANUP_FAILED.load(Ordering::SeqCst) }
use std::time::Duration;
#[derive(Debug)]
pub(crate) struct Output {
    pub code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}
#[cfg(windows)]
mod windows;
pub(crate) fn run(
    path: &Path,
    args: &[&str],
    cwd: &Path,
) -> Result<Output, String> {
    if cleanup_failed() {
        return Err("cleanup fatal: no further probes permitted".into());
    }
    if !path.is_absolute() {
        return Err("probe executable must be absolute".into());
    }
    #[cfg(windows)]
    {
        let result = windows::run(path, args, cwd, Duration::from_secs(15), 65536);
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
