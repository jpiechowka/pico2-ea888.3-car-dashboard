//! Owned native command supervisor for probes, capture and explicit Flash.
//! Callers supply fixed executable/argument contracts, never browser command text.
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
/// Process-wide cancellation latch for the helper's single admitted native operation.
static CANCEL: AtomicBool = AtomicBool::new(false);
/// Requests cancellation; active transfer defers observation until its owned command boundary.
pub(crate) fn request_cancel() { CANCEL.store(true, Ordering::SeqCst); }
/// Clears cancellation before an exclusively admitted new operation; callers must not overlap operations.
pub(crate) fn reset_cancel() { CANCEL.store(false, Ordering::SeqCst); }
/// Returns global cancellation, or thread-local injected cancellation in tests.
pub(crate) fn cancelled() -> bool {
    #[cfg(test)]
    if TEST_CANCEL.get() {
        return true;
    }
    CANCEL.load(Ordering::SeqCst)
}
#[cfg(test)]
thread_local! { /// Test-thread cancellation injection, independent of parallel supervisor tests.
static TEST_CANCEL:std::cell::Cell<bool> = const {std::cell::Cell::new(false)}; }
/// Permanent process-wide quarantine after a supervised tree could not be confirmed closed.
static CLEANUP_FAILED: AtomicBool = AtomicBool::new(false);
/// Returns whether cleanup quarantine forbids any further native command launch.
pub(crate) fn cleanup_failed() -> bool { CLEANUP_FAILED.load(Ordering::SeqCst) }
use std::time::Duration;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// Supervisor failure independent of a child's ordinary nonzero exit status.
pub(crate) enum FailureKind {
    /// Cancellation observed before launch or outside protected transfer.
    Cancelled,
    /// Per-command or enclosing inspection deadline expired.
    Deadline,
    #[cfg(windows)]
    /// Raw stdout or stderr exceeded its independent byte allowance.
    OutputLimit,
    /// Executable/path/spawn/pipe/wait operation failed or native host is unsupported.
    Io,
    #[cfg(windows)]
    /// Owned build-attempt scratch accounting failed or exceeded its budget.
    Scratch,
    /// Owned tree termination/reaping failed; further native operations are quarantined.
    Cleanup,
}

#[derive(Debug)]
/// Supervisor error with bounded sanitized diagnostic tails and explicit incomplete-output accounting.
pub(crate) struct Failure {
    /// Typed cause; cleanup failure supersedes the original operation failure.
    pub kind: FailureKind,
    /// Supervisor diagnostic, without machine stdout when masked by ProcessContext.
    pub message: String,
    /// Pipe output may be incomplete after abnormal supervision or bounded recovery.
    pub output_incomplete: bool,
    /// Known raw bytes discarded after a stream's quota; not a count of unknowable unobserved output.
    pub discarded_bytes: u64,
    /// Last at-most-2048 UTF-8 bytes of sanitized stdout on failure.
    stdout_tail: String,
    /// Last at-most-2048 UTF-8 bytes of sanitized stderr on failure.
    stderr_tail: String,
}
impl Failure {
    /// Returns `kind`/`message` failure with no observed loss or captured tail; runner fills those facts after cleanup.
    fn new(
        kind: FailureKind,
        message: impl Into<String>,
    ) -> Self {
        Self {
            kind,
            message: message.into(),
            output_incomplete: false,
            discarded_bytes: 0,
            stdout_tail: String::new(),
            stderr_tail: String::new(),
        }
    }
}
impl std::fmt::Display for Failure {
    /// Writes message and nonempty sanitized tails to `f`; returns formatter failures.
    fn fmt(
        &self,
        f: &mut std::fmt::Formatter<'_>,
    ) -> std::fmt::Result {
        self.message.fmt(f)?;
        for (name, tail) in [("stdout", &self.stdout_tail), ("stderr", &self.stderr_tail)] {
            if !tail.is_empty() {
                write!(f, "; {name} tail: {tail}")?;
            }
        }
        Ok(())
    }
}
/// Explicit owner of all child output facts for one operation, including probes
/// whose failures become report rows instead of aborting the operation.
#[derive(Default)]
pub(crate) struct ProcessContext {
    /// Sticky incomplete-output evidence from any failed child in this operation.
    incomplete: bool,
    /// Saturating sum of known discarded raw bytes across failed children.
    discarded_bytes: u64,
    /// Most recent supervisor failure; ordinary child exit codes do not populate it.
    pub last_failure: Option<FailureKind>,
}
impl ProcessContext {
    /// Returns successful `result` unchanged; on failure accumulates loss, records cause and returns display text.
    /// Cleanup failures latch global quarantine even when the operation later converts them into report rows.
    fn record(
        &mut self,
        result: Result<Output, Failure>,
    ) -> Result<Output, String> {
        result.map_err(|failure| {
            self.incomplete |= failure.output_incomplete;
            self.discarded_bytes = self.discarded_bytes.saturating_add(failure.discarded_bytes);
            self.last_failure = Some(failure.kind);
            if failure.kind == FailureKind::Cleanup {
                CLEANUP_FAILED.store(true, Ordering::SeqCst);
            }
            failure.to_string()
        })
    }

    /// Returns synchronous-worker closure evidence including additional ingress-loss `bytes` and `chunks`.
    /// Tree emptiness is false after any global cleanup failure; loss totals saturate and never imply complete output.
    pub fn closure(
        &self,
        bytes: u64,
        chunks: u64,
    ) -> crate::jobs::OutputClosure {
        crate::jobs::OutputClosure {
            workers_joined: true,
            tree_empty: !cleanup_failed(),
            output_incomplete: self.incomplete || bytes > 0 || chunks > 0,
            discarded_bytes: self.discarded_bytes.saturating_add(bytes),
            discarded_chunks: chunks,
        }
    }

    /// Runs fixed absolute `path` with `args` in `cwd`; returns captured output even for nonzero exit.
    /// Short-probe deadline/output/cancellation/native failures accumulate in this operation and return sanitized text.
    pub fn run(
        &mut self,
        path: &Path,
        args: &[&str],
        cwd: &Path,
    ) -> Result<Output, String> {
        self.record(run(path, args, cwd))
    }

    /// Runs fixed absolute `path` with `args` in `cwd`, delivering sanitized `(stderr, text)` chunks to `sink`.
    /// `machine_stdout` removes stdout failure tails, but callers must also filter the sink when stdout is private.
    /// Returns complete captured output/nonzero exit or accumulated supervisor error; sink must not panic or block.
    pub fn capture(
        &mut self,
        path: &Path,
        args: &[&str],
        cwd: &Path,
        machine_stdout: bool,
        sink: &mut dyn FnMut(bool, &str),
    ) -> Result<Output, String> {
        self.record_capture(run_capture_stream(path, args, cwd, sink), machine_stdout)
    }

    /// Returns recorded `result`, stripping stdout failure tails when `machine_stdout` contains private machine data.
    fn record_capture(
        &mut self,
        result: Result<Output, Failure>,
        machine_stdout: bool,
    ) -> Result<Output, String> {
        self.record(result.map_err(|mut failure| {
            if machine_stdout {
                failure.stdout_tail.clear();
            }
            failure
        }))
    }

    /// Runs fixed absolute `path`/`args` in `cwd` under flash policy; `transfer` defers ordinary cancellation,
    /// `reboot` selects the shorter deadline, and `sink` receives sanitized stream text and must not panic/block.
    /// Returns captured exit/output or accumulated supervisor error; caller separately verifies exact tool semantics.
    pub fn flash(
        &mut self,
        path: &Path,
        args: &[&str],
        cwd: &Path,
        transfer: bool,
        reboot: bool,
        sink: &mut dyn FnMut(bool, &str),
    ) -> Result<Output, String> {
        self.record(run_flash(path, args, cwd, transfer, reboot, sink))
    }
}
#[derive(Debug)]
/// Complete supervised child result; success here means closed process tree/pipes, not successful command semantics.
pub(crate) struct Output {
    /// Raw interpreted child exit code, or `None` when the platform supplies none.
    pub code: Option<i32>,
    /// Sanitized bounded stdout records joined with newlines.
    pub stdout: String,
    /// Exact bounded stdout bytes for strict machine admission; never directly relayed as text.
    pub raw_stdout: Vec<u8>,
    /// Sanitized bounded stderr records joined with newlines.
    pub stderr: String,
    /// Exact bounded stderr bytes for strict command admission.
    pub raw_stderr: Vec<u8>,
}
#[cfg(windows)]
mod windows;
thread_local! { /// Optional enclosing deadline for sequential short probes on this worker thread.
static PROBE_DEADLINE:std::cell::Cell<Option<std::time::Instant>>=const{std::cell::Cell::new(None)};}
/// Runs synchronous `work` with a thread-local probe budget of `duration`, returning its result.
/// Nested scopes replace then restore the prior deadline, including unwinding; work itself is not preempted.
/// Panics if adding an unrepresentable duration to Instant; current callers use a fixed 30 seconds.
pub(crate) fn with_probe_deadline<T>(
    duration: Duration,
    work: impl FnOnce() -> T,
) -> T {
    /// Restores the enclosing deadline when this synchronous scope exits.
    struct Reset(
        /// Prior thread-local deadline, or no enclosing scope.
        Option<std::time::Instant>,
    );
    impl Drop for Reset {
        /// Restores the previous deadline, including during unwinding.
        fn drop(&mut self) { PROBE_DEADLINE.set(self.0); }
    }
    let previous = PROBE_DEADLINE.replace(Some(std::time::Instant::now() + duration));
    let _reset = Reset(previous);
    work()
}
/// Runs fixed absolute executable `path` with `args` in `cwd`, returning complete output for any child exit code.
/// Windows limits each stream to 65,536 raw bytes and time to min(15 seconds, remaining probe budget).
/// Cancellation, cleanup quarantine, relative paths, unsupported host or supervision failures return typed errors.
pub(crate) fn run(
    path: &Path,
    args: &[&str],
    cwd: &Path,
) -> Result<Output, Failure> {
    if cancelled() {
        return Err(Failure::new(FailureKind::Cancelled, "cancelled: probe check cancelled"));
    }
    if cleanup_failed() {
        return Err(Failure::new(
            FailureKind::Cleanup,
            "cleanup fatal: no further probes permitted",
        ));
    }
    if !path.is_absolute() {
        return Err(Failure::new(FailureKind::Io, "probe executable must be absolute"));
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
            return Err(Failure::new(
                FailureKind::Deadline,
                "deadline: inspection budget exhausted",
            ));
        }
        let result = windows::run(path, args, cwd, timeout, 65536);
        if result.as_ref().is_err_and(|e| e.kind == FailureKind::Cleanup) {
            CLEANUP_FAILED.store(true, Ordering::SeqCst);
        }
        result
    }
    #[cfg(not(windows))]
    {
        let _ = (path, args, cwd);
        Err(Failure::new(FailureKind::Io, "native checks require Windows 10/11 x64"))
    }
}

/// Fixed capture command only. Build output is bounded independently of short probes.
/// Runs absolute `python` (also used for the pinned converter) with fixed `args` in `cwd` and sanitized `sink`
/// delivery. Returns complete captured output for any exit, or typed cancellation/host/process error; Windows allows
/// 1800 seconds and 8 MiB per raw stream. `sink` must not panic or block; caller owns machine-stdout visibility
/// filtering.
pub(crate) fn run_capture_stream(
    python: &Path,
    args: &[&str],
    cwd: &Path,
    sink: &mut dyn FnMut(bool, &str),
) -> Result<Output, Failure> {
    if cancelled() || cleanup_failed() {
        return Err(Failure::new(
            if cleanup_failed() {
                FailureKind::Cleanup
            } else {
                FailureKind::Cancelled
            },
            "Capture unavailable after cancellation or cleanup failure",
        ));
    }
    if !python.is_absolute() {
        return Err(Failure::new(FailureKind::Io, "Python executable must be absolute"));
    }
    #[cfg(windows)]
    {
        let result = windows::run_stream(python, args, cwd, Duration::from_secs(1800), 8 * 1024 * 1024, sink);
        if result.as_ref().is_err_and(|e| e.kind == FailureKind::Cleanup) {
            CLEANUP_FAILED.store(true, Ordering::SeqCst);
        }
        result
    }
    #[cfg(not(windows))]
    {
        let _ = (python, args, cwd, sink);
        Err(Failure::new(FailureKind::Io, "Windows native host required"))
    }
}

thread_local! { /// True only while this worker owns a non-cancellable ordinary transfer command.
static TRANSFER_ACTIVE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) }; }
#[cfg(windows)]
/// Returns whether this thread's active fixed load command defers ordinary cancellation.
pub(crate) fn transfer_active() -> bool { TRANSFER_ACTIVE.get() }
/// Ordinary cancellation is deferred only while the fixed load command is owned.
/// Deadlines and fatal cleanup still apply; no retry is performed.
/// Runs absolute `path` with fixed `args` in `cwd`; `transfer` controls cancellation deferral and `reboot`
/// selects 15 seconds instead of 120. Each raw stream is capped at 16 MiB; `sink` must not panic/block.
/// Returns closed-child output for any exit or typed supervision failure. Prelaunch cancellation/quarantine/relative
/// path rejects dispatch; no subsequent command is launched automatically.
pub(crate) fn run_flash(
    path: &Path,
    args: &[&str],
    cwd: &Path,
    transfer: bool,
    reboot: bool,
    sink: &mut dyn FnMut(bool, &str),
) -> Result<Output, Failure> {
    if cancelled() || cleanup_failed() || !path.is_absolute() {
        return Err(Failure::new(
            if cleanup_failed() {
                FailureKind::Cleanup
            } else {
                FailureKind::Cancelled
            },
            "cancelled: flash command not launched",
        ));
    }
    /// Clears transfer deferral when the owned command scope exits.
    struct Reset;
    impl Drop for Reset {
        /// Restores ordinary cancellation on success, failure or unwind.
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
        if result.as_ref().is_err_and(|e| e.kind == FailureKind::Cleanup) {
            CLEANUP_FAILED.store(true, Ordering::SeqCst);
        }
        result
    }
    #[cfg(not(windows))]
    {
        let _ = (args, cwd, reboot, sink);
        Err(Failure::new(FailureKind::Io, "Windows native host required"))
    }
}

#[cfg(all(test, windows))]
mod flash_tests {
    //! Harmless child-process regression for transfer cancellation policy; no device command is invoked.
    use super::*;
    #[test]
    /// Cancellation raised during transfer waits for child closure and blocks the next command.
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

thread_local! { /// Optional owned attempt path and last scratch-accounting instant for this worker.
static SCRATCH:std::cell::RefCell<Option<(std::path::PathBuf,std::time::Instant)>>=const{std::cell::RefCell::new(None)};}
/// Synchronous non-nested scope enabling periodic accounting of one owned build-attempt directory.
pub(crate) struct ScratchBudget;
impl ScratchBudget {
    /// Returns a guard monitoring owned attempt `path`; replaces any previous scope, so callers must not nest guards.
    pub(crate) fn new(path: &Path) -> Self {
        SCRATCH.with_borrow_mut(|s| *s = Some((path.to_path_buf(), std::time::Instant::now())));
        Self
    }
}
impl Drop for ScratchBudget {
    /// Disables this thread's scratch monitoring without deleting files or changing retained attempt ownership.
    fn drop(&mut self) { SCRATCH.with_borrow_mut(|s| *s = None); }
}
#[cfg(windows)]
/// Returns success without an active scope or before one second has elapsed; otherwise rechecks owned scratch bounds.
/// Filesystem/accounting/budget errors stop the supervised command; the next-check timestamp advances even on failure.
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
