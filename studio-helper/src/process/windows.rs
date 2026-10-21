//! Hidden Windows children assigned to a kill-on-close Job at process creation, with bounded pipe capture.
//! Ownership follows [Job Objects](https://learn.microsoft.com/en-us/windows/win32/procthread/job-objects)
//! and the [Rust attribute-list API](https://doc.rust-lang.org/std/os/windows/process/struct.ProcThreadAttributeList.html).
use std::io::{self, Read};
use std::mem::{size_of, zeroed};
use std::os::windows::io::AsRawHandle;
use std::os::windows::process::{CommandExt, ProcThreadAttributeList};
use std::path::Path;
use std::process::{Command, Stdio};
use std::ptr::{null, null_mut};
use std::time::{Duration, Instant};

use windows_sys::Win32::Foundation::{CloseHandle, ERROR_BROKEN_PIPE, HANDLE};
use windows_sys::Win32::System::JobObjects::*;
use windows_sys::Win32::System::Pipes::PeekNamedPipe;
use windows_sys::Win32::System::Threading::{CREATE_NO_WINDOW, PROC_THREAD_ATTRIBUTE_JOB_LIST};

use super::{Failure, FailureKind, Output};
use crate::jobs::logs::StreamFramer;
/// Owned non-inheritable kill-on-close Job handle; default child inheritance keeps ordinary descendants inside it.
struct Job(
    /// Valid handle returned by CreateJobObjectW, closed exactly once.
    HANDLE,
);
impl Drop for Job {
    /// Closes the Job handle, requesting termination of remaining associated processes; explicit join is separate.
    fn drop(&mut self) {
        // SAFETY: Job exclusively owns a non-null handle created by CreateJobObjectW.
        unsafe {
            CloseHandle(self.0);
        }
    }
}
impl Job {
    /// Returns a new kill-on-close Job, or creation/configuration error after releasing partial ownership.
    fn new() -> io::Result<Self> {
        // SAFETY: null security/name pointers request the documented defaults and an unnamed Job.
        let handle = unsafe { CreateJobObjectW(null(), null()) };
        if handle.is_null() {
            return Err(io::Error::last_os_error());
        }
        let job = Self(handle);
        // SAFETY: the Win32 numeric/pointer structure accepts a zero initialization before flags are set.
        let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { zeroed() };
        info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        // SAFETY: live owned handle; info and its matching size remain valid for this synchronous call.
        if unsafe {
            SetInformationJobObject(
                handle,
                JobObjectExtendedLimitInformation,
                (&info as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
                size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        Ok(job)
    }

    /// Returns the current associated active-process count, or the Win32 accounting-query error.
    fn active(&self) -> io::Result<u32> {
        // SAFETY: this output structure has integer fields and permits zero initialization.
        let mut info: JOBOBJECT_BASIC_ACCOUNTING_INFORMATION = unsafe { zeroed() };
        // SAFETY: owned handle and writable output structure/size match the requested accounting class.
        if unsafe {
            QueryInformationJobObject(
                self.0,
                JobObjectBasicAccountingInformation,
                (&mut info as *mut JOBOBJECT_BASIC_ACCOUNTING_INFORMATION).cast(),
                size_of::<JOBOBJECT_BASIC_ACCOUNTING_INFORMATION>() as u32,
                null_mut(),
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        Ok(info.ActiveProcesses)
    }

    /// Terminates all associated processes and polls accounting for at most five seconds.
    /// Returns success only after zero active processes; termination/query/deadline errors become fatal cleanup
    /// failures.
    fn terminate_and_join(&self) -> Result<(), Failure> {
        // SAFETY: the live Job handle is exclusively owned; exit code 1 is an ordinary termination status.
        if unsafe { TerminateJobObject(self.0, 1) } == 0 {
            return Err(Failure::new(
                FailureKind::Cleanup,
                format!("cleanup fatal: {}", io::Error::last_os_error()),
            ));
        }
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if self
                .active()
                .map_err(|e| Failure::new(FailureKind::Cleanup, format!("cleanup fatal: {e}")))?
                == 0
            {
                return Ok(());
            }
            if Instant::now() >= deadline {
                return Err(Failure::new(
                    FailureKind::Cleanup,
                    "cleanup fatal: job processes did not terminate",
                ));
            }
            std::thread::sleep(Duration::from_millis(5));
        }
    }
}
/// One pipe's raw quota, sanitized framing, closure and known-loss accounting.
struct Pipe {
    /// Incremental UTF-8/control-sequence sanitizer with no secret patterns in this supervisor layer.
    frame: StreamFramer,
    /// Complete sanitized chunks, each followed by a newline.
    text: String,
    /// Exact accepted raw bytes up to the caller's per-stream quota.
    raw: Vec<u8>,
    /// Raw bytes read, including any discarded quota excess.
    bytes: usize,
    /// Broken-pipe EOF or explicit finish already flushed decoder state.
    closed: bool,
    /// Raw bytes read but omitted after quota exhaustion.
    discarded: u64,
}
impl Pipe {
    /// Returns an open empty stream; the fixed empty-secret framer construction is infallible by its contract.
    fn new() -> Self {
        Self {
            frame: StreamFramer::new(&[]).unwrap(),
            text: String::new(),
            raw: Vec::new(),
            bytes: 0,
            closed: false,
            discarded: 0,
        }
    }

    /// Reads at most one available 4096-byte chunk from owned `pipe` without waiting for absent data.
    /// Retains at most `limit` raw bytes, counting discarded excess; returns pipe/quota failure after retaining
    /// the permitted prefix. Broken-pipe EOF flushes framing once. Callers use small limits and must not share readers.
    /// Internal framer unwraps rely on chunk size <=4096 and no pushes after closure.
    fn drain(
        &mut self,
        pipe: &mut (impl Read + AsRawHandle),
        limit: usize,
    ) -> Result<(), Failure> {
        if self.closed {
            return Ok(());
        }
        let mut available = 0;
        // SAFETY: pipe supplies a live readable pipe handle; available is writable and other optional outputs are null.
        if unsafe {
            PeekNamedPipe(
                pipe.as_raw_handle(),
                null_mut(),
                0,
                null_mut(),
                &mut available,
                null_mut(),
            )
        } == 0
        {
            let e = io::Error::last_os_error();
            if e.raw_os_error() == Some(ERROR_BROKEN_PIPE as i32) {
                self.finish();
                return Ok(());
            }
            return Err(Failure::new(FailureKind::Io, e.to_string()));
        }
        if available > 0 {
            let mut buffer = [0; 4096];
            let n = pipe
                .read(&mut buffer[..(available as usize).min(4096)])
                .map_err(|e| Failure::new(FailureKind::Io, e.to_string()))?;
            let accepted = n.min(limit.saturating_sub(self.bytes));
            self.bytes += n;
            self.discarded += (n - accepted) as u64;
            self.raw.extend_from_slice(&buffer[..accepted]);
            self.frame
                .push(&buffer[..accepted], |s, _| {
                    self.text.push_str(&s);
                    self.text.push('\n');
                })
                .unwrap();
            if self.bytes > limit {
                return Err(Failure::new(
                    FailureKind::OutputLimit,
                    format!("output_limit: process exceeded {limit} bytes per stream"),
                ));
            }
        }
        Ok(())
    }

    /// Flushes incomplete UTF-8/line state once and marks closed; repeated calls do nothing.
    /// Incomplete escape strings are discarded by StreamFramer, not exposed as console controls.
    fn finish(&mut self) {
        if !self.closed {
            self.frame
                .finish(|s, _| {
                    self.text.push_str(&s);
                    self.text.push('\n');
                })
                .unwrap();
            self.closed = true;
        }
    }

    /// Drains owned `pipe` under raw byte `limit`, emitting newly sanitized lines to `sink` tagged by `stderr`.
    /// Returns drain failure after delivering its retained prefix; sink must not panic or block cleanup/deadline
    /// checks.
    fn drain_to(
        &mut self,
        pipe: &mut (impl Read + AsRawHandle),
        limit: usize,
        stderr: bool,
        sink: &mut dyn FnMut(bool, &str),
    ) -> Result<(), Failure> {
        let start = self.text.len();
        let result = self.drain(pipe, limit);
        for line in self.text[start..].lines() {
            sink(stderr, line);
        }
        result
    }
}
/// Returns supervised output for fixed executable `path`, `args`, `cwd`, `timeout` and per-stream raw-byte `limit`.
/// Uses run_stream with no live sink; process/pipe/deadline/quota/cancellation/cleanup failures retain bounded tails.
pub(super) fn run(
    path: &Path,
    args: &[&str],
    cwd: &Path,
    timeout: Duration,
    limit: usize,
) -> Result<Output, Failure> {
    run_stream(path, args, cwd, timeout, limit, &mut |_, _| {})
}
/// Launches fixed `path`/`args` in `cwd` hidden, with null stdin and owned piped output under `timeout` and raw `limit`
/// per stream. `sink` receives sanitized `(stderr, line)` chunks and must not panic/block; callers authorize the
/// command. Returns output (including nonzero exit) only after direct-child exit, closed pipes and zero associated
/// processes. Cancellation outside transfer, scratch, timeout, quota or I/O failures terminate/join the Job; cleanup
/// failure supersedes the cause. Error recovery drains at most 64 rounds, flags incomplete output and retains 2048-byte
/// tails. Panicking callbacks unwind through kill-on-close but bypass explicit join/accounting; normal callers never
/// panic.
pub(super) fn run_stream(
    path: &Path,
    args: &[&str],
    cwd: &Path,
    timeout: Duration,
    limit: usize,
    sink: &mut dyn FnMut(bool, &str),
) -> Result<Output, Failure> {
    let io_failure = |e: io::Error| Failure::new(FailureKind::Io, e.to_string());
    let job = Job::new().map_err(io_failure)?;
    // The non-inheritable Job handle and array outlive attribute list and spawn.
    // JOB_LIST assigns ownership during CreateProcess, before any child code executes.
    let jobs = [job.0];
    let attributes = ProcThreadAttributeList::build()
        .attribute(PROC_THREAD_ATTRIBUTE_JOB_LIST as usize, &jobs)
        .finish()
        .map_err(io_failure)?;
    let mut command = Command::new(path);
    command
        .args(args)
        .current_dir(cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .creation_flags(CREATE_NO_WINDOW);
    command
        .env("RUSTUP_AUTO_INSTALL", "0")
        .env("NO_COLOR", "true")
        .env("PYTHONDONTWRITEBYTECODE", "1")
        .env("CARGO_NET_OFFLINE", "true");
    let mut child = command.spawn_with_attributes(&attributes).map_err(io_failure)?;
    let mut stdout = child.stdout.take().unwrap();
    let mut stderr = child.stderr.take().unwrap();
    let mut out = Pipe::new();
    let mut err = Pipe::new();
    let start = Instant::now();
    let result = (|| {
        loop {
            super::check_scratch().map_err(|e| Failure::new(FailureKind::Scratch, e))?;
            out.drain_to(&mut stdout, limit, false, sink)?;
            err.drain_to(&mut stderr, limit, true, sink)?;
            let status = child.try_wait().map_err(io_failure)?;
            if let Some(status) = status
                && out.closed
                && err.closed
                && job.active().map_err(io_failure)? == 0
            {
                return Ok(Output {
                    code: status.code(),
                    stdout: std::mem::take(&mut out.text),
                    raw_stdout: std::mem::take(&mut out.raw),
                    stderr: std::mem::take(&mut err.text),
                    raw_stderr: std::mem::take(&mut err.raw),
                });
            }
            if super::cancelled() && !super::transfer_active() {
                return Err(Failure::new(FailureKind::Cancelled, "cancelled: probe check cancelled"));
            }
            if start.elapsed() >= timeout {
                return Err(Failure::new(
                    FailureKind::Deadline,
                    "deadline: probe exceeded its deadline",
                ));
            }
            if out.bytes == 0 && err.bytes == 0 {
                std::thread::sleep(Duration::from_millis(5));
            } else {
                std::thread::sleep(Duration::from_millis(1));
            }
        }
    })();
    // Always close the entire tree and reap the direct child, including pipe errors.
    let cleanup = job.terminate_and_join().and_then(|()| {
        child
            .wait()
            .map(|_| ())
            .map_err(|e| Failure::new(FailureKind::Cleanup, format!("cleanup fatal: {e}")))
    });
    let result = match cleanup {
        Ok(()) => result,
        Err(failure) => Err(failure),
    };
    result.map_err(|mut failure| {
        // The child tree has stopped when cleanup succeeded. Bound recovery work
        // even if a pipe is broken or the retained-output quota is exhausted.
        for _ in 0..64 {
            let before = out.bytes + err.bytes;
            let a = out.drain_to(&mut stdout, limit, false, sink);
            let b = err.drain_to(&mut stderr, limit, true, sink);
            if a.is_err() || b.is_err() || before == out.bytes + err.bytes {
                break;
            }
        }
        for (stderr, pipe) in [(false, &mut out), (true, &mut err)] {
            let start = pipe.text.len();
            pipe.finish();
            for line in pipe.text[start..].lines() {
                sink(stderr, line);
            }
        }
        failure.output_incomplete = true;
        failure.discarded_bytes = out.discarded + err.discarded;
        // Non-streaming probes still retain bounded diagnostic tails in their
        // failure report. These sanitized strings are never machine admission.
        for (tail, text) in [
            (&mut failure.stdout_tail, &out.text),
            (&mut failure.stderr_tail, &err.text),
        ] {
            if !text.is_empty() {
                let start = text.ceil_char_boundary(text.len().saturating_sub(2048));
                *tail = text[start..].to_owned();
            }
        }
        failure
    })
}
#[cfg(test)]
mod tests {
    //! Harmless Python subprocess fixtures for stream delivery, limits, cancellation and descendant ownership.
    use super::*;
    /// Returns the first existing python.exe on PATH; panics if the test prerequisite is unavailable.
    fn cmd() -> std::path::PathBuf {
        std::env::split_paths(&std::env::var_os("PATH").unwrap())
            .map(|p| p.join("python.exe"))
            .find(|p| p.is_file())
            .unwrap()
    }
    #[test]
    /// Live stdout reaches the sink before the child can proceed past its acknowledgment barrier.
    fn sanitized_output_is_delivered_before_child_exit() {
        let flag = std::env::temp_dir().join(format!(
            "studio-stream-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let mut observed = false;
        let output = run_stream(
            &cmd(),
            &[
                "-I",
                "-u",
                "-c",
                "import sys,time,pathlib; print('early',flush=True)\nwhile not pathlib.Path(sys.argv[1]).exists(): \
                 time.sleep(0.005)\nprint('acknowledged')",
                flag.to_str().unwrap(),
            ],
            &std::env::current_dir().unwrap(),
            Duration::from_secs(5),
            65536,
            &mut |stderr, text| {
                if text == "early" {
                    assert!(!stderr);
                    assert!(!flag.exists());
                    std::fs::write(&flag, b"observed").unwrap();
                    observed = true;
                }
            },
        )
        .unwrap();
        assert!(observed);
        assert_eq!(output.code, Some(0));
        assert!(flag.exists());
        assert!(output.stdout.contains("acknowledged"));
        std::fs::remove_file(flag).unwrap();
    }
    #[test]
    /// Nonzero child exit is retained as Output rather than being confused with supervisor failure.
    fn successful_and_failed_exit() {
        let cwd = std::env::current_dir().unwrap();
        let o = run(
            &cmd(),
            &[
                "-I",
                "-c",
                "import sys; print('hello'); print('error', file=sys.stderr); sys.exit(7)",
            ],
            &cwd,
            Duration::from_secs(2),
            65536,
        )
        .unwrap();
        assert_eq!(o.code, Some(7));
        assert!(o.stdout.contains("hello"));
        assert!(o.stderr.contains("error"));
    }
    #[test]
    /// A nonexistent executable is rejected without producing a successful child result.
    fn missing_executable() {
        assert!(
            run(
                Path::new("C:/does-not-exist.exe"),
                &[],
                &std::env::current_dir().unwrap(),
                Duration::from_secs(1),
                65536
            )
            .is_err()
        );
    }
    #[test]
    /// Deadline cleanup flushes a sanitized incomplete UTF-8 line and keeps loss evidence scoped to its operation.
    fn deadline_retains_sanitized_partial_line() {
        let mut lines = Vec::new();
        let result = run_stream(
            &cmd(),
            &[
                "-I",
                "-u",
                "-c",
                "import sys,time; sys.stdout.buffer.write(b'important diagnostic\\x1b[31m\\xe2'); sys.stdout.flush(); \
                 time.sleep(30)",
            ],
            &std::env::current_dir().unwrap(),
            Duration::from_secs(1),
            65536,
            &mut |stderr, text| lines.push((stderr, text.to_owned())),
        );
        let failure = result.unwrap_err();
        assert_eq!(failure.kind, FailureKind::Deadline);
        assert!(failure.output_incomplete);
        assert_eq!(failure.discarded_bytes, 0);
        assert!(failure.to_string().contains("important diagnostic�"));
        assert_eq!(lines, [(false, "important diagnostic�".to_owned())]);
        let mut operation = super::super::ProcessContext::default();
        assert!(operation.record(Err(failure)).is_err());
        assert!(operation.closure(0, 0).output_incomplete);
        assert!(!super::super::ProcessContext::default().closure(0, 0).output_incomplete);
    }
    #[test]
    /// Cancellation flushes both stream tails and distinguishes incomplete output from known discarded bytes.
    fn cancellation_finishes_both_streams_and_accounts_loss() {
        /// Restores thread-local injected cancellation even when an assertion panics.
        struct Reset;
        impl Drop for Reset {
            /// Clears only this test thread's cancellation injection.
            fn drop(&mut self) { super::super::TEST_CANCEL.set(false); }
        }
        let _reset = Reset;
        let mut lines = Vec::new();
        let failure = run_stream(
            &cmd(),
            &[
                "-I",
                "-u",
                "-c",
                "import sys,time; sys.stdout.write('partial'); sys.stdout.flush(); sys.stderr.write('ready\\n'); \
                 sys.stderr.flush(); time.sleep(30)",
            ],
            &std::env::current_dir().unwrap(),
            Duration::from_secs(5),
            65536,
            &mut |stderr, text| {
                lines.push((stderr, text.to_owned()));
                if stderr && text == "ready" {
                    super::super::TEST_CANCEL.set(true);
                }
            },
        )
        .unwrap_err();
        assert_eq!(failure.kind, FailureKind::Cancelled);
        assert!(failure.output_incomplete);
        assert_eq!(failure.discarded_bytes, 0);
        assert_eq!(lines, [(true, "ready".into()), (false, "partial".into())]);
    }
    #[test]
    /// Machine-stdout failure tails stay private while stderr remains available in the operation diagnostic.
    fn failed_machine_report_retains_only_public_stderr_tail() {
        let result = run_stream(
            &cmd(),
            &[
                "-I",
                "-u",
                "-c",
                "import sys,time; print('private-machine-report'); print('public-diagnostic',file=sys.stderr); \
                 time.sleep(30)",
            ],
            &std::env::current_dir().unwrap(),
            Duration::from_secs(1),
            65536,
            &mut |_, _| {},
        );
        let mut operation = super::super::ProcessContext::default();
        let message = operation.record_capture(result, true).unwrap_err();
        assert!(message.contains("public-diagnostic"));
        assert!(!message.contains("private-machine-report"));
        assert!(operation.closure(0, 0).output_incomplete);
    }
    #[test]
    /// Deadline termination covers a spawned descendant as well as its direct parent.
    fn deadline_closes_descendant_tree() {
        let e = run(
            &cmd(),
            &[
                "-I",
                "-c",
                "import subprocess,sys,time; subprocess.Popen([sys.executable, '-c', 'import time; time.sleep(30)']); \
                 time.sleep(30)",
            ],
            &std::env::current_dir().unwrap(),
            Duration::from_millis(100),
            65536,
        )
        .unwrap_err();
        assert_eq!(e.kind, FailureKind::Deadline);
    }
    #[test]
    /// A descendant observed alive before output overflow is no longer active when the runner returns.
    fn descendant_is_observed_and_joined() {
        use windows_sys::Win32::System::Threading::{
            GetExitCodeProcess,
            OpenProcess,
            PROCESS_QUERY_LIMITED_INFORMATION,
        };
        /// Optional owned process-query handle retained across forced tree termination.
        struct Handle(
            /// Null before observation, otherwise the process handle to close.
            HANDLE,
        );
        impl Drop for Handle {
            /// Closes an observed non-null process handle once.
            fn drop(&mut self) {
                if !self.0.is_null() {
                    unsafe {
                        CloseHandle(self.0);
                    }
                }
            }
        }
        let marker = std::env::temp_dir().join(format!(
            "studio-probe-ready-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        /// Test-owned acknowledgment marker removed on every scope exit.
        struct Marker(
            /// Exact temporary marker pathname; never a recursive cleanup target.
            std::path::PathBuf,
        );
        impl Drop for Marker {
            /// Best-effort removes only this test's marker file.
            fn drop(&mut self) { let _ = std::fs::remove_file(&self.0); }
        }
        let _marker = Marker(marker.clone());
        // The parent waits for our live-process observation before triggering the
        // pipe failure. Python startup time is not the cleanup assertion's clock.
        let script = "import subprocess,sys,time,pathlib\np=subprocess.Popen([sys.executable,'-I','-c','import time; \
                      time.sleep(30)'])\nprint(p.pid,flush=True)\nwhile not pathlib.Path(sys.argv[1]).exists(): \
                      time.sleep(0.005)\nprint('x'*100000,flush=True)\ntime.sleep(30)";
        let mut handle = Handle(null_mut());
        let mut observed_live = false;
        let mut acknowledged = false;
        let error = run_stream(
            &cmd(),
            &["-I", "-u", "-c", script, marker.to_str().unwrap()],
            &std::env::current_dir().unwrap(),
            Duration::from_secs(10),
            1024,
            &mut |stderr, text| {
                if !stderr
                    && handle.0.is_null()
                    && let Ok(pid) = text.parse::<u32>()
                {
                    handle.0 = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
                    if !handle.0.is_null() {
                        let mut code = 0;
                        observed_live = unsafe { GetExitCodeProcess(handle.0, &mut code) } != 0 && code == 259;
                    }
                    acknowledged = std::fs::write(&marker, b"observed").is_ok();
                }
            },
        )
        .unwrap_err();
        assert_eq!(error.kind, FailureKind::OutputLimit);
        assert!(error.output_incomplete);
        assert!(error.discarded_bytes > 0);
        assert!(
            acknowledged && observed_live,
            "descendant must be observed alive before inducing pipe failure"
        );
        let mut code = 259;
        let success = unsafe { GetExitCodeProcess(handle.0, &mut code) };
        assert_ne!(success, 0);
        assert_ne!(code, 259, "descendant remains active after runner returned");
    }
    #[test]
    /// Output exceeding the per-stream quota terminates the owned job and marks capture incomplete.
    fn excessive_output_is_rejected() {
        let e = run(
            &cmd(),
            &["-I", "-c", "print('x'*100000)"],
            &std::env::current_dir().unwrap(),
            Duration::from_secs(3),
            1024,
        )
        .unwrap_err();
        assert_eq!(e.kind, FailureKind::OutputLimit);
        assert!(e.output_incomplete);
    }
}
