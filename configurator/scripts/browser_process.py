"""Own only the browser tree created here, including after its root exits.

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


if os.name == 'nt':
    import ctypes as c
    from ctypes import wintypes as w
    import msvcrt

    class Startup(c.Structure):
        _fields_ = [('cb', w.DWORD), ('reserved', w.LPWSTR), ('desktop', w.LPWSTR),
                    ('title', w.LPWSTR), ('x', w.DWORD), ('y', w.DWORD),
                    ('xsize', w.DWORD), ('ysize', w.DWORD), ('xchars', w.DWORD),
                    ('ychars', w.DWORD), ('fill', w.DWORD), ('flags', w.DWORD),
                    ('show', w.WORD), ('reserved_size', w.WORD), ('reserved2', c.c_void_p),
                    ('stdin', w.HANDLE), ('stdout', w.HANDLE), ('stderr', w.HANDLE)]

    class StartupEx(c.Structure):
        _fields_ = [('startup', Startup), ('attributes', c.c_void_p)]

    class ProcessInfo(c.Structure):
        _fields_ = [('process', w.HANDLE), ('thread', w.HANDLE), ('pid', w.DWORD), ('tid', w.DWORD)]

    class Limits(c.Structure):
        _fields_ = [('process_time', c.c_int64), ('job_time', c.c_int64), ('flags', w.DWORD),
                    ('min_working', c.c_size_t), ('max_working', c.c_size_t),
                    ('active', w.DWORD), ('affinity', c.c_size_t), ('priority', w.DWORD),
                    ('scheduling', w.DWORD)]

    class ExtendedLimits(c.Structure):
        _fields_ = [('basic', Limits), ('io', c.c_uint64 * 6),
                    ('process_memory', c.c_size_t), ('job_memory', c.c_size_t),
                    ('peak_process', c.c_size_t), ('peak_job', c.c_size_t)]

    class Accounting(c.Structure):
        _fields_ = [('times', c.c_int64 * 4), ('faults', w.DWORD), ('total', w.DWORD),
                    ('active', w.DWORD), ('terminated', w.DWORD)]

    kernel = c.WinDLL('kernel32', use_last_error=True)

    def api(name, args, result=w.BOOL):
        function = getattr(kernel, name)
        function.argtypes, function.restype = args, result
        return function

    close = api('CloseHandle', [w.HANDLE])
    create_job = api('CreateJobObjectW', [c.c_void_p, w.LPCWSTR], w.HANDLE)
    set_job = api('SetInformationJobObject', [w.HANDLE, c.c_int, c.c_void_p, w.DWORD])
    query_job = api('QueryInformationJobObject', [w.HANDLE, c.c_int, c.c_void_p, w.DWORD, c.c_void_p])
    terminate_job = api('TerminateJobObject', [w.HANDLE, w.UINT])
    current = api('GetCurrentProcess', [], w.HANDLE)
    duplicate = api('DuplicateHandle', [w.HANDLE, w.HANDLE, w.HANDLE, c.POINTER(w.HANDLE), w.DWORD, w.BOOL, w.DWORD])
    init_attributes = api('InitializeProcThreadAttributeList', [c.c_void_p, w.DWORD, w.DWORD, c.POINTER(c.c_size_t)])
    update_attribute = api('UpdateProcThreadAttribute', [c.c_void_p, w.DWORD, c.c_size_t, c.c_void_p, c.c_size_t, c.c_void_p, c.c_void_p])
    delete_attributes = api('DeleteProcThreadAttributeList', [c.c_void_p], None)
    create_process = api('CreateProcessW', [w.LPCWSTR, w.LPWSTR, c.c_void_p, c.c_void_p,
                         w.BOOL, w.DWORD, c.c_void_p, w.LPCWSTR, c.POINTER(StartupEx), c.POINTER(ProcessInfo)])
    wait_handle = api('WaitForSingleObject', [w.HANDLE, w.DWORD], w.DWORD)
    exit_code = api('GetExitCodeProcess', [w.HANDLE, c.POINTER(w.DWORD)])

    def checked(result):
        if not result:
            raise c.WinError(c.get_last_error())
        return result


class OwnedProcess:
    def __init__(self, command, *, stdout, stderr):
        self.command = command
        self.returncode = None
        self._closed = False
        if os.name != 'nt':
            self._process = subprocess.Popen(command, stdout=stdout, stderr=stderr, start_new_session=True)
            self.pid = self._process.pid
            return
        self._job = checked(create_job(None, None))
        self._handle = None
        inherited = []
        attributes = None
        try:
            limits = ExtendedLimits()
            limits.basic.flags = 0x2000  # JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE
            checked(set_job(self._job, 9, c.byref(limits), c.sizeof(limits)))
            with open(os.devnull, 'rb') as null:
                for stream in (null, stdout, stderr):
                    handle = w.HANDLE()
                    checked(duplicate(current(), msvcrt.get_osfhandle(stream.fileno()), current(),
                                      c.byref(handle), 0, True, 2))  # DUPLICATE_SAME_ACCESS
                    inherited.append(handle.value)
            size = c.c_size_t()
            init_attributes(None, 2, 0, c.byref(size))
            storage = c.create_string_buffer(size.value)
            checked(init_attributes(storage, 2, 0, c.byref(size)))
            attributes = storage
            jobs = (w.HANDLE * 1)(self._job)
            handles = (w.HANDLE * 3)(*inherited)
            checked(update_attribute(storage, 0, 0x2000D, jobs, c.sizeof(jobs), None, None))
            checked(update_attribute(storage, 0, 0x20002, handles, c.sizeof(handles), None, None))
            startup = StartupEx()
            startup.startup.cb = c.sizeof(startup)
            startup.startup.flags = 0x100  # STARTF_USESTDHANDLES
            startup.startup.stdin, startup.startup.stdout, startup.startup.stderr = inherited
            startup.attributes = c.cast(storage, c.c_void_p)
            info = ProcessInfo()
            line = c.create_unicode_buffer(subprocess.list2cmdline(command))
            # Hidden, extended startup; ownership established atomically by JOB_LIST.
            checked(create_process(None, line, None, None, True, 0x08080000,
                                   None, None, c.byref(startup), c.byref(info)))
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
            for handle in inherited:
                close(handle)

    def poll(self):
        if self._closed or self.returncode is not None:
            return self.returncode
        if os.name != 'nt':
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

    def wait(self, timeout=None):
        deadline = None if timeout is None else time.monotonic() + timeout
        while self.poll() is None:
            if deadline is not None and time.monotonic() >= deadline:
                raise subprocess.TimeoutExpired(self.command, timeout)
            time.sleep(0.01)
        return self.returncode

    def cleanup(self):
        if self._closed:
            return
        try:
            if os.name == 'nt':
                checked(terminate_job(self._job, 1))
                deadline = time.monotonic() + 5
                while True:
                    accounting = Accounting()
                    checked(query_job(self._job, 1, c.byref(accounting), c.sizeof(accounting), None))
                    if accounting.active == 0:
                        break
                    if time.monotonic() >= deadline:
                        raise TimeoutError('owned browser job did not terminate')
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
            if os.name == 'nt':
                close(self._job)  # kill-on-close remains the failure-path backstop
                close(self._handle)

    def __enter__(self):
        return self

    def __exit__(self, *_):
        self.cleanup()
