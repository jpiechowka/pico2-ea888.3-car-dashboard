"""Opt-in HTTP integration against one owned helper; importing has no side effects."""

from collections.abc import Callable, Mapping
from contextlib import ExitStack
import json
import os
from pathlib import Path
import queue
import sys
import threading
import time
from typing import BinaryIO, Protocol
import urllib.error
import urllib.request

REPO = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(REPO / "host/scripts"))
sys.path.insert(0, str(REPO / "firmware/scripts"))
from owned_process import OwnedProcess  # noqa: E402
from script_values import array_items, object_fields, string_value  # noqa: E402
from validation import require  # noqa: E402

RESPONSE_LIMIT = 8 * 1024 * 1024
BASE = "http://127.0.0.1:8765"
ORIGIN = "http://127.0.0.1:8080"


class ByteReader(Protocol):
    def read(self, size: int = -1) -> bytes: ...


def response_body(stream: ByteReader) -> object:
    data = stream.read(RESPONSE_LIMIT + 1)
    require(len(data) <= RESPONSE_LIMIT, "helper HTTP response exceeds byte bound")
    try:
        result: object = json.loads(data or b"null")
    except (ValueError, UnicodeDecodeError) as error:
        raise AssertionError("helper HTTP response is not valid JSON") from error
    return result


def request(
    path: str, method: str = "GET", headers: Mapping[str, str] | None = None
) -> tuple[int, object]:
    h = {"Origin": ORIGIN}
    h.update(headers or {})
    request = urllib.request.Request(
        BASE + path, method=method, headers=h, data=b"" if method == "POST" else None
    )
    try:
        with urllib.request.urlopen(request, timeout=10) as response:
            return response.status, response_body(response)
    except urllib.error.HTTPError as error:
        with error:
            return error.code, response_body(error)


def job_identity(raw_job: object) -> str:
    job = object_fields(raw_job, "invalid job acceptance")
    identity = string_value(job.get("job_id"), "missing job identity")
    require(
        bool(identity)
        and len(identity) <= 64
        and all(character in "0123456789abcdefghijklmnopqrstuvwxyz-" for character in identity),
        "invalid job identity",
    )
    return identity


def exercise(*, devices: bool = False, check_process: Callable[[], None] = lambda: None) -> None:
    deadline = time.monotonic() + 180

    def req(
        path: str, method: str = "GET", headers: Mapping[str, str] | None = None
    ) -> tuple[int, object]:
        check_process()
        require(time.monotonic() < deadline, "HTTP harness exceeded its bounded check window")
        return request(path, method, headers)

    status, raw_session = req("/api/v1/connect", "POST", {"X-Studio-Connect": "1"})
    require(status == 200, "helper connection failed")
    session = object_fields(raw_session, "invalid helper session")
    capability = string_value(session.get("capability"), "invalid session capability")
    require(len(capability) == 64, "invalid session capability length")
    headers = {"X-Studio-Capability": capability}
    require(req("/api/v1/jobs")[0] == 403, "unauthenticated jobs were not rejected")
    require(
        req(
            "/api/v1/connect",
            "POST",
            {"Origin": "https://evil.example", "X-Studio-Connect": "1"},
        )[0]
        == 403,
        "foreign origin was not rejected",
    )
    status, raw_session = req("/api/v1/session", headers=headers)
    require(
        status == 200
        and object_fields(raw_session, "invalid helper session").get("ready") is False,
        "fresh helper unexpectedly reports ready",
    )
    require(req("/api/v1/jobs", headers=headers) == (200, []), "fresh helper has jobs")
    status, job = req("/api/v1/prerequisites/check", "POST", headers)
    require(status == 202, "prerequisite check not accepted")
    require(
        req("/api/v1/prerequisites/check", "POST", headers)[0] == 409,
        "overlapping prerequisite check not rejected",
    )

    def poll(raw_job: object) -> dict[str, object]:
        identity = job_identity(raw_job)
        poll_deadline = time.monotonic() + 25
        while time.monotonic() < poll_deadline:
            status, raw_batch = req(
                "/api/v1/jobs/" + identity + "/events?after=0&limit=128", headers=headers
            )
            require(status == 200, "job events request failed")
            batch = object_fields(raw_batch, "invalid job events")
            summary = object_fields(batch.get("summary"), "invalid job summary")
            if summary.get("events_closed") is True:
                return batch
            require(summary.get("events_closed") is False, "invalid job closed state")
            time.sleep(0.05)
        raise RuntimeError("Job did not close within bounded check window")

    batch = poll(job)
    require(
        object_fields(batch.get("summary"), "invalid job summary").get("terminal_cause")
        in ("failed", "succeeded"),
        "unexpected prerequisite terminal cause",
    )
    records = array_items(batch.get("records"), "invalid job records")
    require(0 < len(records) <= 128, "job record count outside bounds")
    status, report = req("/api/v1/prerequisites", headers=headers)
    require(
        status == 200
        and object_fields(report, "invalid prerequisite report").get("readiness_scope")
        == "installed_tool_versions_only",
        "prerequisite qualification differs",
    )
    status, second = req("/api/v1/prerequisites/check", "POST", headers)
    require(status == 202, "second prerequisite check not accepted")
    require(
        req("/api/v1/jobs/" + job_identity(second) + "/cancel", "POST", headers)[0] == 200,
        "job cancellation failed",
    )
    cancelled = object_fields(poll(second).get("summary"), "invalid cancellation summary")
    require(cancelled.get("terminal_cause") == "cancelled", "job did not cancel")
    require(req("/api/v1/builds", "POST", headers)[0] == 415, "invalid build body accepted")
    if devices:
        status, device_job = req("/api/v1/devices/refresh", "POST", headers)
        require(status == 202, "device refresh not accepted")
        poll(device_job)
        status, raw_devices = req("/api/v1/devices", headers=headers)
        devices_record = object_fields(raw_devices, "invalid device snapshot")
        device_list = array_items(devices_record.get("devices"), "invalid device list")
        schema = devices_record.get("schema_version")
        require(
            status == 200 and type(schema) is int and schema == 1 and len(device_list) <= 16,
            "invalid device snapshot contract",
        )
        for raw_device in device_list:
            device = object_fields(raw_device, "invalid device")
            capacity = object_fields(device.get("capacity"), "invalid device capacity")
            require(
                device.get("flash_ready") is False
                and capacity.get("state") == "unverified"
                and capacity.get("bytes") is None,
                "descriptor endpoint claimed flash readiness",
            )
        print(
            f"Read-only descriptor endpoint: {len(device_list)} candidates; no flash-ready claims."
        )
    print(
        "Live HTTP: connect/auth, single active job, cached report, event polling, "
        "cancel/join and unsupported build content-type rejection passed."
    )


def main() -> None:
    with ExitStack() as stack:
        readers: list[BinaryIO] = []
        writers: list[BinaryIO] = []
        for _ in range(2):
            read, write = os.pipe()
            readers.append(stack.enter_context(os.fdopen(read, "rb", buffering=0)))
            writers.append(stack.enter_context(os.fdopen(write, "wb", buffering=0)))
        child = stack.enter_context(
            OwnedProcess(
                [
                    str(REPO / "studio-helper/target/debug/studio-helper.exe"),
                    "serve",
                    "--repo",
                    str(REPO),
                ],
                stdout=writers[0],
                stderr=writers[1],
                cwd=REPO,
            )
        )
        for writer in writers:
            writer.close()
        startup: queue.Queue[bytes] = queue.Queue(maxsize=1)
        failed = threading.Event()

        def drain(stream: BinaryIO, first_line: bool) -> None:
            try:
                seen = 0
                if first_line:
                    line = stream.readline(4097)
                    startup.put_nowait(line)
                    seen += len(line)
                while data := stream.read(4096):
                    seen += len(data)
                    if seen > RESPONSE_LIMIT:
                        failed.set()
                        return
            except (OSError, ValueError, queue.Full):
                failed.set()

        threads = [
            threading.Thread(target=drain, args=(stream, index == 1), daemon=True)
            for index, stream in enumerate(readers)
        ]
        for thread in threads:
            thread.start()
        try:
            line = startup.get(timeout=5)
            require(
                len(line) <= 4096 and line.startswith(b"Studio helper listening at "),
                "Owned helper could not bind; no existing listener was contacted or terminated",
            )

            def check_process() -> None:
                require(not failed.is_set(), "owned helper output exceeded bound or failed")
                require(child.poll() is None, "owned helper exited during HTTP checks")

            exercise(devices="--devices" in sys.argv, check_process=check_process)
        finally:
            try:
                child.cleanup()
            finally:
                for thread in threads:
                    thread.join(timeout=5)
            require(not any(thread.is_alive() for thread in threads), "helper output did not close")
        print("Owned helper tree stopped and joined; no capability logged.")


if __name__ == "__main__":
    main()
