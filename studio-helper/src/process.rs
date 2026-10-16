//! Owned native command supervisor for probes, capture and explicit Flash.
//! Callers supply fixed executable/argument contracts, never browser command text.
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
static CANCEL: AtomicBool = AtomicBool::new(false);
pub(crate) fn request_cancel() { CANCEL.store(true, Ordering::SeqCst); }
pub(crate) fn reset_cancel() { CANCEL.store(false, Ordering::SeqCst); }
pub(crate) fn cancelled() -> bool {
    #[cfg(test)]
    if TEST_CANCEL.get() {
        return true;
    }
    CANCEL.load(Ordering::SeqCst)
}
#[cfg(test)]
thread_local! { static TEST_CANCEL:std::cell::Cell<bool> = const {std::cell::Cell::new(false)}; }
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

thread_local! { static TRANSFER_ACTIVE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) }; }
pub(crate) fn transfer_active() -> bool { TRANSFER_ACTIVE.get() }
/// Ordinary cancellation is deferred only while the fixed load command is owned.
/// Deadlines and fatal cleanup still apply; no retry is performed.
pub(crate) fn run_flash(
    path: &Path,
    args: &[&str],
    cwd: &Path,
    transfer: bool,
    reboot: bool,
    sink: &mut dyn FnMut(bool, &str),
) -> Result<Output, String> {
    if cancelled() || cleanup_failed() || !path.is_absolute() {
        return Err("cancelled: flash command not launched".into());
    }
    struct Reset;
    impl Drop for Reset {
        fn drop(&mut self) { TRANSFER_ACTIVE.set(false); }
    }
    TRANSFER_ACTIVE.set(transfer);
    let _reset = Reset;
    #[cfg(windows)]
    {
        let result = windows::run_stream(
            path,
            args,
            cwd,
            Duration::from_secs(if reboot { 15 } else { 120 }),
            16 * 1024 * 1024,
            sink,
        );
        if result.as_ref().is_err_and(|e| e.starts_with("cleanup fatal:")) {
            CLEANUP_FAILED.store(true, Ordering::SeqCst);
        }
        result
    }
    #[cfg(not(windows))]
    {
        let _ = (args, cwd, reboot, sink);
        Err("Windows native host required".into())
    }
}

#[cfg(all(test, windows))]
mod flash_tests {
    use super::*;
    #[test]
    fn transfer_defers_cancel_to_owned_process_boundary() {
        TEST_CANCEL.set(false);
        let powershell = std::path::PathBuf::from(std::env::var_os("SystemRoot").unwrap())
            .join("System32/WindowsPowerShell/v1.0/powershell.exe");
        let mut seen = false;
        let output = run_flash(
            &powershell,
            &[
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                "Write-Output 'ready'; [Console]::Out.Flush(); Start-Sleep -Milliseconds 150; Write-Output 'finished'",
            ],
            &std::env::current_dir().unwrap(),
            true,
            false,
            &mut |_, text| {
                if text == "ready" {
                    seen = true;
                    TEST_CANCEL.set(true);
                }
            },
        )
        .unwrap();
        assert!(seen);
        assert!(cancelled());
        assert_eq!(output.code, Some(0));
        assert!(output.stdout.contains("finished"));
        assert!(!transfer_active());
        assert!(
            run_flash(
                &powershell,
                &[],
                &std::env::current_dir().unwrap(),
                true,
                false,
                &mut |_, _| {}
            )
            .is_err()
        );
        TEST_CANCEL.set(false);
    }
}

thread_local! {static SCRATCH:std::cell::RefCell<Option<(std::path::PathBuf,std::time::Instant)>>=const{std::cell::RefCell::new(None)};}
pub(crate) struct ScratchBudget;
impl ScratchBudget {
    pub(crate) fn new(path: &Path) -> Self {
        SCRATCH.with_borrow_mut(|s| *s = Some((path.to_path_buf(), std::time::Instant::now())));
        Self
    }
}
impl Drop for ScratchBudget {
    fn drop(&mut self) { SCRATCH.with_borrow_mut(|s| *s = None); }
}
pub(crate) fn check_scratch() -> Result<(), String> {
    SCRATCH.with_borrow_mut(|s| {
        if let Some((path, last)) = s
            && last.elapsed() >= Duration::from_secs(1)
        {
            *last = std::time::Instant::now();
            crate::build_storage::check_scratch(path)?;
        }
        Ok(())
    })
}
