"""Own only the child tree created here, including after its root exits.

Windows uses JOB_LIST at CreateProcess time, matching studio-helper's ownership
contract. No child runs before job assignment; only explicit log handles inherit.
https://devblogs.microsoft.com/oldnewthing/20230209-00/?p=107812
https://learn.microsoft.com/windows/win32/procthread/job-objects
POSIX owns a new session/process group; children must not deliberately escape it.
"""

import os
import signal
import subprocess
import time
import math
import codecs
import sys
import unicodedata
import argparse
import tempfile
from collections.abc import Callable, Mapping, Sequence
from typing import Any, BinaryIO, Protocol, TypeVar, cast
from types import TracebackType

Result = TypeVar("Result")


if sys.platform == "win32":
    import ctypes as c
    from ctypes import wintypes as w
    import msvcrt

    class Startup(c.Structure):
        _fields_ = [
            ("cb", w.DWORD),
            ("reserved", w.LPWSTR),
            ("desktop", w.LPWSTR),
            ("title", w.LPWSTR),
            ("x", w.DWORD),
            ("y", w.DWORD),
            ("xsize", w.DWORD),
            ("ysize", w.DWORD),
            ("xchars", w.DWORD),
            ("ychars", w.DWORD),
            ("fill", w.DWORD),
            ("flags", w.DWORD),
            ("show", w.WORD),
            ("reserved_size", w.WORD),
            ("reserved2", c.c_void_p),
            ("stdin", w.HANDLE),
            ("stdout", w.HANDLE),
            ("stderr", w.HANDLE),
        ]

    class StartupEx(c.Structure):
        _fields_ = [("startup", Startup), ("attributes", c.c_void_p)]

    class ProcessInfo(c.Structure):
        _fields_ = [("process", w.HANDLE), ("thread", w.HANDLE), ("pid", w.DWORD), ("tid", w.DWORD)]

    class Limits(c.Structure):
        _fields_ = [
            ("process_time", c.c_int64),
            ("job_time", c.c_int64),
            ("flags", w.DWORD),
            ("min_working", c.c_size_t),
            ("max_working", c.c_size_t),
            ("active", w.DWORD),
            ("affinity", c.c_size_t),
            ("priority", w.DWORD),
            ("scheduling", w.DWORD),
        ]

    class ExtendedLimits(c.Structure):
        _fields_ = [
            ("basic", Limits),
            ("io", c.c_uint64 * 6),
            ("process_memory", c.c_size_t),
            ("job_memory", c.c_size_t),
            ("peak_process", c.c_size_t),
            ("peak_job", c.c_size_t),
        ]

    class Accounting(c.Structure):
        _fields_ = [
            ("times", c.c_int64 * 4),
            ("faults", w.DWORD),
            ("total", w.DWORD),
            ("active", w.DWORD),
            ("terminated", w.DWORD),
        ]

    kernel = c.WinDLL("kernel32", use_last_error=True)

    def api(name: str, args: list[Any], result: Any = w.BOOL) -> Any:
        # ctypes describes native ABI types dynamically; keep Any at this boundary.
        function = getattr(kernel, name)
        function.argtypes, function.restype = args, result
        return function

    close = api("CloseHandle", [w.HANDLE])
    create_job = api("CreateJobObjectW", [c.c_void_p, w.LPCWSTR], w.HANDLE)
    set_job = api("SetInformationJobObject", [w.HANDLE, c.c_int, c.c_void_p, w.DWORD])
    query_job = api(
        "QueryInformationJobObject", [w.HANDLE, c.c_int, c.c_void_p, w.DWORD, c.c_void_p]
    )
    terminate_job = api("TerminateJobObject", [w.HANDLE, w.UINT])
    current = api("GetCurrentProcess", [], w.HANDLE)
    duplicate = api(
        "DuplicateHandle",
        [w.HANDLE, w.HANDLE, w.HANDLE, c.POINTER(w.HANDLE), w.DWORD, w.BOOL, w.DWORD],
    )
    init_attributes = api(
        "InitializeProcThreadAttributeList", [c.c_void_p, w.DWORD, w.DWORD, c.POINTER(c.c_size_t)]
    )
    update_attribute = api(
        "UpdateProcThreadAttribute",
        [c.c_void_p, w.DWORD, c.c_size_t, c.c_void_p, c.c_size_t, c.c_void_p, c.c_void_p],
    )
    delete_attributes = api("DeleteProcThreadAttributeList", [c.c_void_p], None)
    create_process = api(
        "CreateProcessW",
        [
            w.LPCWSTR,
            w.LPWSTR,
            c.c_void_p,
            c.c_void_p,
            w.BOOL,
            w.DWORD,
            c.c_void_p,
            w.LPCWSTR,
            c.POINTER(StartupEx),
            c.POINTER(ProcessInfo),
        ],
    )
    wait_handle = api("WaitForSingleObject", [w.HANDLE, w.DWORD], w.DWORD)
    exit_code = api("GetExitCodeProcess", [w.HANDLE, c.POINTER(w.DWORD)])
    peek_pipe = api(
        "PeekNamedPipe", [w.HANDLE, c.c_void_p, w.DWORD, c.c_void_p, c.POINTER(w.DWORD), c.c_void_p]
    )

    def checked(result: Result) -> Result:
        if not result:
            raise c.WinError(c.get_last_error())
        return result


class FileDescriptor(Protocol):
    def fileno(self) -> int: ...


class OwnedProcess:
    def __init__(
        self,
        command: Sequence[str],
        *,
        stdout: FileDescriptor,
        stderr: FileDescriptor,
        stdin: FileDescriptor | None = None,
        cwd: str | os.PathLike[str] | None = None,
        env: Mapping[str, str] | None = None,
    ):
        self.command = list(command)
        self.returncode: int | None = None
        self._closed = False
        if sys.platform != "win32":
            self._process = subprocess.Popen(
                command,
                stdin=subprocess.DEVNULL if stdin is None else stdin.fileno(),
                stdout=stdout.fileno(),
                stderr=stderr.fileno(),
                start_new_session=True,
                cwd=cwd,
                env=env,
            )
            self.pid = self._process.pid
            return
        else:
            self._job = checked(create_job(None, None))
            self._handle = None
            inherited = []
            attributes = None
            try:
                limits = ExtendedLimits()
                limits.basic.flags = 0x2000  # JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE
                checked(set_job(self._job, 9, c.byref(limits), c.sizeof(limits)))
                with open(os.devnull, "rb") as null:
                    for stream in (null if stdin is None else stdin, stdout, stderr):
                        handle = w.HANDLE()
                        checked(
                            duplicate(
                                current(),
                                msvcrt.get_osfhandle(stream.fileno()),
                                current(),
                                c.byref(handle),
                                0,
                                True,
                                2,
                            )
                        )  # DUPLICATE_SAME_ACCESS
                        inherited.append(handle.value)
                size = c.c_size_t()
                init_attributes(None, 2, 0, c.byref(size))
                storage = c.create_string_buffer(size.value)
                checked(init_attributes(storage, 2, 0, c.byref(size)))
                attributes = storage
                jobs = (w.HANDLE * 1)(self._job)
                handles = (w.HANDLE * 3)(*inherited)
                checked(update_attribute(storage, 0, 0x2000D, jobs, c.sizeof(jobs), None, None))
                checked(
                    update_attribute(storage, 0, 0x20002, handles, c.sizeof(handles), None, None)
                )
                startup = StartupEx()
                startup.startup.cb = c.sizeof(startup)
                startup.startup.flags = 0x100  # STARTF_USESTDHANDLES
                startup.startup.stdin, startup.startup.stdout, startup.startup.stderr = inherited
                startup.attributes = c.cast(storage, c.c_void_p)
                info = ProcessInfo()
                line = c.create_unicode_buffer(subprocess.list2cmdline(command))
                environment = None
                if env is not None:
                    if any(
                        not key or "=" in key or "\0" in key or "\0" in value
                        for key, value in env.items()
                    ):
                        raise ValueError("invalid child environment")
                    environment = c.create_unicode_buffer(
                        "\0".join(
                            f"{key}={value}"
                            for key, value in sorted(env.items(), key=lambda item: item[0].upper())
                        )
                        + "\0\0"
                    )
                # Hidden, extended startup; ownership established atomically by JOB_LIST.
                checked(
                    create_process(
                        None,
                        line,
                        None,
                        None,
                        True,
                        0x08080400,
                        environment,
                        None if cwd is None else os.fspath(cwd),
                        c.byref(startup),
                        c.byref(info),
                    )
                )
                self._handle, self.pid = info.process, info.pid
                close(info.thread)
            except BaseException:
                close(self._job)
                if self._handle:
                    close(self._handle)
                raise
            finally:
                if attributes is not None:
                    delete_attributes(attributes)
                for inherited_handle in inherited:
                    close(inherited_handle)

    def poll(self) -> int | None:
        if self._closed and self.returncode is None:
            raise OSError("owned process closed without confirmed terminal status")
        if self._closed or self.returncode is not None:
            return self.returncode
        if sys.platform != "win32":
            self.returncode = self._process.poll()
        else:
            status = wait_handle(self._handle, 0)
            if status == 0:
                code = w.DWORD()
                checked(exit_code(self._handle, c.byref(code)))
                self.returncode = code.value
            elif status != 258:  # WAIT_TIMEOUT
                raise c.WinError(c.get_last_error())
        return self.returncode

    def wait(self, timeout: float | None = None) -> int:
        deadline = None if timeout is None else time.monotonic() + timeout
        while (status := self.poll()) is None:
            if timeout is not None and deadline is not None and time.monotonic() >= deadline:
                raise subprocess.TimeoutExpired(self.command, timeout)
            time.sleep(0.01)
        return status

    def cleanup(self) -> None:
        if self._closed:
            return
        try:
            if sys.platform == "win32":
                checked(terminate_job(self._job, 1))
                deadline = time.monotonic() + 5
                while True:
                    accounting = Accounting()
                    checked(
                        query_job(self._job, 1, c.byref(accounting), c.sizeof(accounting), None)
                    )
                    if accounting.active == 0:
                        break
                    if time.monotonic() >= deadline:
                        raise TimeoutError("owned process job did not terminate")
                    time.sleep(0.01)
            else:
                # Root exit does not release ownership of the session's children.
                for sig in (signal.SIGTERM, signal.SIGKILL):
                    try:
                        os.killpg(self.pid, sig)
                    except ProcessLookupError:
                        break
                    if sig == signal.SIGTERM:
                        time.sleep(0.2)
            self.wait(timeout=5)
        finally:
            self._closed = True
            if sys.platform == "win32":
                close(self._job)  # kill-on-close remains the failure-path backstop
                close(self._handle)

    def __enter__(self) -> "OwnedProcess":
        return self

    def __exit__(
        self,
        exc_type: type[BaseException] | None,
        exc_value: BaseException | None,
        traceback: TracebackType | None,
    ) -> None:
        self.cleanup()


def _read_available(stream: BinaryIO) -> bytes | None:
    """None means no progress; empty bytes means the writer has closed."""
    if sys.platform == "win32":
        available = w.DWORD()
        if not peek_pipe(
            msvcrt.get_osfhandle(stream.fileno()), None, 0, None, c.byref(available), None
        ):
            error = c.get_last_error()
            if error == 109:  # ERROR_BROKEN_PIPE
                return b""
            raise c.WinError(error)
        if not available.value:
            return None
        return os.read(stream.fileno(), min(4096, available.value))
    import select

    if not select.select([stream], [], [], 0)[0]:
        return None
    return os.read(stream.fileno(), 4096)


class OutputLimitError(ValueError):
    """Observed bytes are a lower bound; unread output remains explicitly incomplete."""

    def __init__(self, stream: str, observed: int, retained: int) -> None:
        self.stream, self.observed, self.retained = stream, observed, retained
        self.known_discarded = observed - retained
        super().__init__(
            f"{stream} exceeds process output bound ({observed} observed, "
            f"{retained} retained, {self.known_discarded} discarded); output incomplete"
        )


def run_bounded(
    command: Sequence[str],
    *,
    cwd: str | os.PathLike[str] | None = None,
    env: Mapping[str, str] | None = None,
    timeout: float = 120,
    stdout_limit: int = 8 * 1024 * 1024,
    stderr_limit: int = 8 * 1024 * 1024,
    on_stdout: Callable[[bytes], None] | None = None,
    on_stderr: Callable[[bytes], None] | None = None,
    input_bytes: bytes | None = None,
) -> subprocess.CompletedProcess[bytes]:
    """Bound both pipes and elapsed time; cleanup the complete owned tree on every exit.

    Callbacks are synchronous, bounded application work and must not block. Stderr
    is forwarded as sanitized text by default. Stdout is retained for typed parsing.
    A failure does not claim complete output; accepted stdout chunks reach the
    callback before a quota error so callers can preserve partial evidence.
    """
    if not math.isfinite(timeout) or timeout <= 0 or min(stdout_limit, stderr_limit) < 0:
        raise ValueError("invalid process deadline or byte bound")
    if input_bytes is not None and len(input_bytes) > 8 * 1024 * 1024:
        raise ValueError("process stdin exceeds 8 MiB bound")
    from contextlib import ExitStack

    with ExitStack() as stack:
        # A bounded anonymous file avoids a blocking stdin pipe writer and gives
        # the child an EOF without exposing a payload in argv or a named artifact.
        stdin = None
        if input_bytes is not None:
            stdin = cast(BinaryIO, stack.enter_context(tempfile.TemporaryFile()))
            stdin.write(input_bytes)
            stdin.seek(0)
        pipes: list[BinaryIO] = []
        writers: list[BinaryIO] = []
        for _ in range(2):
            read, write = os.pipe()
            pipes.append(stack.enter_context(os.fdopen(read, "rb", buffering=0)))
            writers.append(stack.enter_context(os.fdopen(write, "wb", buffering=0)))
        child = stack.enter_context(
            OwnedProcess(
                command, stdout=writers[0], stderr=writers[1], stdin=stdin, cwd=cwd, env=env
            )
        )
        for writer in writers:
            writer.close()
        retained = [bytearray(), bytearray()]
        observed = [0, 0]
        closed = [False, False]
        decoder = codecs.getincrementaldecoder("utf-8")(errors="replace")

        def stderr_text(chunk: bytes, final: bool = False) -> None:
            text = decoder.decode(chunk, final=final)
            text = "".join(
                character
                for character in text
                if character in "\n\t" or not unicodedata.category(character).startswith("C")
            )
            encoding = getattr(sys.stderr, "encoding", None) or "utf-8"
            sys.stderr.write(text.encode(encoding, errors="backslashreplace").decode(encoding))
            sys.stderr.flush()

        callbacks = [on_stdout, on_stderr or stderr_text]
        deadline = time.monotonic() + timeout
        while True:
            progressed = False
            for index, (stream, limit, callback) in enumerate(
                zip(pipes, (stdout_limit, stderr_limit), callbacks, strict=True)
            ):
                if closed[index]:
                    continue
                chunk = _read_available(stream)
                if chunk is None:
                    continue
                if not chunk:
                    closed[index] = True
                    continue
                progressed = True
                observed[index] += len(chunk)
                accepted = chunk[: max(0, limit - len(retained[index]))]
                retained[index].extend(accepted)
                if accepted and callback is not None:
                    callback(accepted)
                if len(accepted) != len(chunk):
                    raise OutputLimitError(
                        "stderr" if index else "stdout", observed[index], len(retained[index])
                    )
            status = child.poll()
            if status is not None and all(closed):
                if on_stderr is None:
                    stderr_text(b"", final=True)
                return subprocess.CompletedProcess(
                    list(command), status, bytes(retained[0]), bytes(retained[1])
                )
            if time.monotonic() >= deadline:
                raise subprocess.TimeoutExpired(
                    list(command), timeout, output=bytes(retained[0]), stderr=bytes(retained[1])
                )
            if not progressed:
                time.sleep(0.005)


def run_checked(
    command: Sequence[str],
    *,
    cwd: str | os.PathLike[str] | None = None,
    env: Mapping[str, str] | None = None,
    timeout: float = 120,
    capture_output: bool = False,
    text: bool = True,
    encoding: str = "utf-8",
    check: bool = False,
) -> subprocess.CompletedProcess[str]:
    """Text-only adapter for fixed validation tools, with live bounded diagnostics."""
    if not text or encoding != "utf-8":
        raise ValueError("owned text process requires UTF-8")
    decoder = codecs.getincrementaldecoder("utf-8")(errors="replace")

    def stdout(chunk: bytes) -> None:
        sys.stdout.write(decoder.decode(chunk))
        sys.stdout.flush()

    result = run_bounded(
        command,
        cwd=cwd,
        env=env,
        timeout=timeout,
        on_stdout=None if capture_output else stdout,
        on_stderr=(lambda _: None) if capture_output else None,
    )
    if not capture_output:
        sys.stdout.write(decoder.decode(b"", final=True))
        sys.stdout.flush()
    completed = subprocess.CompletedProcess(
        list(command),
        result.returncode,
        result.stdout.decode("utf-8"),
        result.stderr.decode("utf-8"),
    )
    if check:
        completed.check_returncode()
    return completed


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--cwd")
    parser.add_argument("--timeout", type=float, default=120)
    parser.add_argument("--stdout-limit", type=int, default=8 * 1024 * 1024)
    parser.add_argument("--json", action="store_true", help="publish stdout only after success")
    parser.add_argument("command", nargs=argparse.REMAINDER)
    options = parser.parse_args()
    command = options.command
    if command[:1] == ["--"]:
        command = command[1:]
    if not command:
        parser.error("a child command is required")

    def stdout(chunk: bytes) -> None:
        sys.stdout.buffer.write(chunk)
        sys.stdout.buffer.flush()

    try:
        result = run_bounded(
            command,
            cwd=options.cwd,
            timeout=options.timeout,
            stdout_limit=options.stdout_limit,
            on_stdout=None if options.json else stdout,
        )
        if options.json and result.returncode == 0:
            # A failed producer's machine payload is never used as diagnostics.
            result.stdout.decode("utf-8", errors="strict")
            stdout(result.stdout)
        return result.returncode
    except (OSError, ValueError, subprocess.TimeoutExpired) as error:
        # Do not print command arguments or retained machine payloads.
        detail = str(error) if isinstance(error, OutputLimitError) else type(error).__name__
        if error.__context__ is not None:
            detail += f"; prior failure: {type(error.__context__).__name__}"
        print(f"Owned process failed ({detail}); output may be incomplete", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
