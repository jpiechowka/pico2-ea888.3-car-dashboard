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

use super::Output;
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

    fn terminate_and_join(&self) -> Result<(), String> {
        if unsafe { TerminateJobObject(self.0, 1) } == 0 {
            return Err(format!("cleanup fatal: {}", io::Error::last_os_error()));
        }
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if self.active().map_err(|e| format!("cleanup fatal: {e}"))? == 0 {
                return Ok(());
            }
            if Instant::now() >= deadline {
                return Err("cleanup fatal: job processes did not terminate".into());
            }
            std::thread::sleep(Duration::from_millis(5));
        }
    }
}
struct Pipe {
    frame: StreamFramer,
    text: String,
    bytes: usize,
    closed: bool,
}
impl Pipe {
    fn new() -> Self {
        Self {
            frame: StreamFramer::new(&[]).unwrap(),
            text: String::new(),
            bytes: 0,
            closed: false,
        }
    }

    fn drain(
        &mut self,
        pipe: &mut (impl Read + AsRawHandle),
        limit: usize,
    ) -> Result<(), String> {
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
                self.closed = true;
                self.frame
                    .finish(|s, _| {
                        self.text.push_str(&s);
                        self.text.push('\n');
                    })
                    .unwrap();
                return Ok(());
            }
            return Err(e.to_string());
        }
        if available > 0 {
            let mut buffer = [0; 4096];
            let n = pipe
                .read(&mut buffer[..(available as usize).min(4096)])
                .map_err(|e| e.to_string())?;
            self.bytes += n;
            if self.bytes > limit {
                return Err("output_limit: probe exceeded 65536 bytes per stream".into());
            }
            self.frame
                .push(&buffer[..n], |s, _| {
                    self.text.push_str(&s);
                    self.text.push('\n');
                })
                .unwrap();
        }
        Ok(())
    }
}
pub(super) fn run(
    path: &Path,
    args: &[&str],
    cwd: &Path,
    timeout: Duration,
    limit: usize,
) -> Result<Output, String> {
    let job = Job::new().map_err(|e| e.to_string())?;
    // The non-inheritable Job handle and array outlive attribute list and spawn.
    // JOB_LIST assigns ownership during CreateProcess, before any child code executes.
    let jobs = [job.0];
    let attributes = ProcThreadAttributeList::build()
        .attribute(PROC_THREAD_ATTRIBUTE_JOB_LIST as usize, &jobs)
        .finish()
        .map_err(|e| e.to_string())?;
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
        .env("PYTHONDONTWRITEBYTECODE", "1");
    let mut child = command.spawn_with_attributes(&attributes).map_err(|e| e.to_string())?;
    let mut stdout = child.stdout.take().unwrap();
    let mut stderr = child.stderr.take().unwrap();
    let mut out = Pipe::new();
    let mut err = Pipe::new();
    let start = Instant::now();
    let result = (|| {
        loop {
            out.drain(&mut stdout, limit)?;
            err.drain(&mut stderr, limit)?;
            let status = child.try_wait().map_err(|e| e.to_string())?;
            if let Some(status) = status {
                if out.closed && err.closed && job.active().map_err(|e| e.to_string())? == 0 {
                    return Ok(Output {
                        code: status.code(),
                        stdout: std::mem::take(&mut out.text),
                        stderr: std::mem::take(&mut err.text),
                    });
                }
            }
            if super::cancelled() {
                return Err("cancelled: probe check cancelled".into());
            }
            if start.elapsed() >= timeout {
                return Err("deadline: probe exceeded its deadline".into());
            }
            if out.bytes == 0 && err.bytes == 0 {
                std::thread::sleep(Duration::from_millis(5));
            } else {
                std::thread::sleep(Duration::from_millis(1));
            }
        }
    })();
    // Always close the entire tree and reap the direct child, including pipe errors.
    job.terminate_and_join()?;
    child.wait().map_err(|e| format!("cleanup fatal: {e}"))?;
    result
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
        assert!(e.starts_with("deadline"), "{e}");
    }
    #[test]
    fn descendant_is_observed_and_joined() {
        use windows_sys::Win32::System::Threading::{
            GetExitCodeProcess,
            OpenProcess,
            PROCESS_QUERY_LIMITED_INFORMATION,
        };
        let marker = std::env::temp_dir().join(format!("studio-probe-child-{}.txt", std::process::id()));
        let _ = std::fs::remove_file(&marker);
        let script = "import subprocess,sys,time; p=subprocess.Popen([sys.executable,'-I','-c','import time; \
                      time.sleep(30)']); open(sys.argv[1],'w').write(str(p.pid)); time.sleep(30)";
        let e = run(
            &cmd(),
            &["-I", "-c", script, marker.to_str().unwrap()],
            &std::env::current_dir().unwrap(),
            Duration::from_millis(600),
            65536,
        )
        .unwrap_err();
        assert!(e.starts_with("deadline"), "{e}");
        let pid: u32 = std::fs::read_to_string(&marker)
            .expect("descendant was spawned before deadline")
            .parse()
            .unwrap();
        std::fs::remove_file(marker).unwrap();
        let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
        if !handle.is_null() {
            let mut code = 259;
            let success = unsafe { GetExitCodeProcess(handle, &mut code) };
            unsafe { CloseHandle(handle) };
            assert_ne!(success, 0);
            assert_ne!(code, 259, "descendant remains active after runner returned");
        } else {
            assert_eq!(
                std::io::Error::last_os_error().raw_os_error(),
                Some(87),
                "unexpected OpenProcess failure"
            );
        }
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
        assert!(e.starts_with("output_limit"), "{e}");
    }
}
