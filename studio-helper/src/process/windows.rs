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
struct Job(HANDLE);
impl Drop for Job {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.0);
        }
    }
}
impl Job {
    fn new() -> io::Result<Self> {
        let handle = unsafe { CreateJobObjectW(null(), null()) };
        if handle.is_null() {
            return Err(io::Error::last_os_error());
        }
        let job = Self(handle);
        let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { zeroed() };
        info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
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

    fn active(&self) -> io::Result<u32> {
        let mut info: JOBOBJECT_BASIC_ACCOUNTING_INFORMATION = unsafe { zeroed() };
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

    fn terminate_and_join(&self) -> Result<(), Failure> {
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
struct Pipe {
    frame: StreamFramer,
    text: String,
    raw: Vec<u8>,
    bytes: usize,
    closed: bool,
    discarded: u64,
}
impl Pipe {
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

    fn drain(
        &mut self,
        pipe: &mut (impl Read + AsRawHandle),
        limit: usize,
    ) -> Result<(), Failure> {
        if self.closed {
            return Ok(());
        }
        let mut available = 0;
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
pub(super) fn run(
    path: &Path,
    args: &[&str],
    cwd: &Path,
    timeout: Duration,
    limit: usize,
) -> Result<Output, Failure> {
    run_stream(path, args, cwd, timeout, limit, &mut |_, _| {})
}
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
    use super::*;
    fn cmd() -> std::path::PathBuf {
        std::env::split_paths(&std::env::var_os("PATH").unwrap())
            .map(|p| p.join("python.exe"))
            .find(|p| p.is_file())
            .unwrap()
    }
    #[test]
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
    fn cancellation_finishes_both_streams_and_accounts_loss() {
        struct Reset;
        impl Drop for Reset {
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
    fn descendant_is_observed_and_joined() {
        use windows_sys::Win32::System::Threading::{
            GetExitCodeProcess,
            OpenProcess,
            PROCESS_QUERY_LIMITED_INFORMATION,
        };
        struct Handle(HANDLE);
        impl Drop for Handle {
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
        struct Marker(std::path::PathBuf);
        impl Drop for Marker {
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
