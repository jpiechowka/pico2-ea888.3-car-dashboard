"""Build, gate and freeze a firmware ELF with exact source/config identity.
This script never flashes hardware or claims a device timing measurement.
"""

import argparse
from collections.abc import Mapping, Sequence
from typing import NotRequired, TypedDict
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import tomllib
import unicodedata
import zipfile


def external_tool_path(path: str | Path) -> Path:
    """Keep filesystem identity but avoid MSVC's long verbatim-path argument failure."""
    resolved = Path(path).resolve(strict=True)
    value = str(resolved)
    if os.name != "nt" or not value.startswith("\\\\?\\"):
        return resolved
    # Only ordinary drive/UNC spellings have equivalent external-tool paths.
    tail = value[4:]
    if tail.startswith("UNC\\"):
        candidate = Path("\\\\" + tail[4:])
    elif re.match(r"^[A-Za-z]:\\", tail):
        candidate = Path(tail)
    else:
        raise ValueError("unsupported Windows tool path namespace")
    if not candidate.samefile(resolved):
        raise ValueError("Windows tool path identity changed")
    return candidate


FIRMWARE = external_tool_path(Path(__file__).parent.parent)
REPO = FIRMWARE.parent
# These shared script imports follow the two explicit repository search paths.
sys.path.insert(0, str(REPO / "host" / "scripts"))
sys.path.insert(0, str(FIRMWARE / "scripts"))
from benchmark_environment import CONFIG_KEYS, ENV_PREFIXES, compiler  # noqa: E402
from source_inventory import shared_inputs  # noqa: E402
from owned_process import run_bounded, run_checked  # noqa: E402
from elf_image import ElfImage  # noqa: E402
from check_program_image import inspect_program, linked_configuration, linked_startup  # noqa: E402
from check_memory_layout import FullImageRecord, read_selected_image_result  # noqa: E402
from practical_image import PracticalRecord  # noqa: E402
from config_build_source import (  # noqa: E402
    SourceContext,
    make_source_context,
    write_source_context,
    load_source_context,
    inspect_source_context,
)

from script_values import object_fields, array_items, string_value  # noqa: E402


class MemoryEvidence(TypedDict):
    record: PracticalRecord | FullImageRecord
    sha256: str


class ClockEvidence(TypedDict):
    out_dir: str
    source: bytes
    metadata: bytes
    source_sha256: str
    metadata_sha256: str


class ConfigRecord(TypedDict):
    selection: str
    source: NotRequired[str]
    sha256: NotRequired[str]
    normalization_command: NotRequired[list[str]]
    normalized_sha256: NotRequired[str]
    embedded_sha256: NotRequired[str]


class StartupMember(TypedDict):
    sha256: str
    bytes: int


def fields(value: object) -> dict[str, object]:
    try:
        return object_fields(value, "capture value must be an object")
    except AssertionError as error:
        raise ValueError(str(error)) from error


def text_value(value: object) -> str:
    try:
        return string_value(value, "capture value must be a string")
    except AssertionError as error:
        raise ValueError(str(error)) from error


def items(value: object) -> list[object]:
    try:
        return array_items(value, "capture value must be an array")
    except AssertionError as error:
        raise ValueError(str(error)) from error


def unique(pairs: list[tuple[str, object]]) -> dict[str, object]:
    result: dict[str, object] = {}
    for key, value in pairs:
        if key in result:
            raise ValueError("duplicate capture JSON key")
        result[key] = value
    return result


def json_object(raw: str | bytes | bytearray) -> dict[str, object]:
    return fields(json.loads(raw, object_pairs_hook=unique))


def cargo_records(output: str) -> list[dict[str, object]]:
    return [json_object(line) for line in output.splitlines()]


def cargo_environment(record: Mapping[str, object]) -> dict[str, str]:
    result: dict[str, str] = {}
    for raw in items(record.get("env", [])):
        pair = items(raw)
        if len(pair) != 2:
            raise ValueError("invalid Cargo environment pair")
        key, value = map(text_value, pair)
        if key in result:
            raise ValueError("duplicate Cargo environment key")
        result[key] = value
    return result


CUSTOM_KEYS = ("DASHBOARD_CPU_HZ", "DASHBOARD_SPI_HZ", "DASHBOARD_VSEL_MV")


def read_memory_gate(
    path: Path,
    frozen: Path,
    config_bytes: bytes,
    source_context: SourceContext | None = None,
    gate: str = "practical",
) -> MemoryEvidence:
    """Retain the explicitly selected successful image result, bound to this immutable input."""
    with path.open("rb") as stream:
        raw = stream.read(65537)
    if len(raw) > 65536:
        raise ValueError("oversized image gate result")

    record = json_object(raw)
    checked = read_selected_image_result(
        record, ElfImage(frozen), config_bytes, source_context=source_context, gate=gate
    )
    return dict(record=checked, sha256=hashlib.sha256(raw).hexdigest())


def run_memory_gate(
    frozen: Path,
    destination: Path,
    config_bytes: bytes,
    source_context: SourceContext | None = None,
    source_context_path: Path | None = None,
    gate: str = "practical",
) -> MemoryEvidence:
    if source_context is None or source_context_path is None:
        raise ValueError(
            "image gate requires an explicit source context and owned sidecar before launching child"
        )
    path = destination / "memory-layout.json"
    if path.exists():
        raise ValueError("image result path already exists")
    command = [sys.executable, "scripts/check_memory_layout.py", str(frozen), "--gate", gate]
    current = load_source_context(
        source_context_path, ElfImage(frozen), destination.name, source_context.build_options
    )
    if current != source_context:
        raise ValueError("source sidecar differs from the owned attempt")
    inspect_source_context(current, ElfImage(frozen))
    command.extend(["--source-context", str(source_context_path.resolve())])
    command.extend(["--result-json", str(path.resolve())])
    run_checked(command, cwd=FIRMWARE, check=True)
    return read_memory_gate(path, frozen, config_bytes, source_context, gate)


def check_clock_environment(environment: Mapping[str, str]) -> None:
    if any(key in CUSTOM_KEYS or key.startswith(ENV_PREFIXES) for key in environment):
        raise ValueError(
            "unset ambient compiler/config/custom-clock overrides; select typed inputs explicitly"
        )


MAX_CARGO_DIAGNOSTICS = 256 * 1024


class CargoDiagnostics:
    """Bound rendered messages independently of the raw Cargo identity stream."""

    def __init__(self, destination: Path) -> None:
        self.file = (destination / "cargo-diagnostics.log").open("xb")
        self.remaining = MAX_CARGO_DIAGNOSTICS
        self.truncated = False

    def record(self, raw: str | bytes | bytearray) -> None:
        if self.truncated:
            return
        try:
            record = json.loads(raw)
        except (ValueError, UnicodeError):
            return
        if not isinstance(record, dict) or record.get("reason") != "compiler-message":
            return
        message = record.get("message")
        rendered = message.get("rendered") if isinstance(message, dict) else None
        if not isinstance(rendered, str):
            return
        # No terminal escapes, carriage-return rewriting or Unicode format controls.
        rendered = "".join(
            c for c in rendered if c in "\n\t" or not unicodedata.category(c).startswith("C")
        )
        marker = b"[Cargo diagnostics truncated]\n"
        for line in rendered.splitlines():
            for offset in range(0, max(1, len(line)), 2048):
                data = (line[offset : offset + 2048] + "\n").encode("utf-8")
                if len(data) + len(marker) > self.remaining:
                    self.emit(marker)
                    self.truncated = True
                    return
                self.emit(data)

    def emit(self, data: bytes) -> None:
        self.file.write(data)
        self.file.flush()
        self.remaining -= len(data)
        text = data.decode("utf-8")
        encoding = getattr(sys.stderr, "encoding", None) or "utf-8"
        sys.stderr.write(text.encode(encoding, errors="backslashreplace").decode(encoding))
        sys.stderr.flush()


def run_build(
    command: Sequence[str],
    environment: Mapping[str, str],
    destination: Path,
    *,
    timeout: float = 1800,
) -> subprocess.CompletedProcess[str]:
    # stderr (including rendered JSON diagnostics below) uses the native redactor.
    diagnostics = CargoDiagnostics(destination)
    output = bytearray()
    pending = bytearray()

    def record(chunk: bytes) -> None:
        output.extend(chunk)
        pending.extend(chunk)
        while (end := pending.find(b"\n")) >= 0:
            diagnostics.record(pending[:end])
            del pending[: end + 1]

    try:
        result = run_bounded(
            command, cwd=FIRMWARE, env=environment, timeout=timeout, on_stdout=record
        )
        if pending:
            diagnostics.record(pending)
    finally:
        diagnostics.file.close()
        (destination / "cargo-messages.jsonl").write_bytes(output)
    text = output.decode("utf-8")
    build = subprocess.CompletedProcess(command, result.returncode, text)
    build.check_returncode()
    return build


def fixed_json_tool(
    arguments: Sequence[str], environment: Mapping[str, str], target_dir: Path | None = None
) -> tuple[list[str], dict[str, object]]:
    """Bound both streams and time; stop the owned tree immediately on overflow."""
    command = [
        "cargo",
        "run",
        "--quiet",
        "--locked",
        "--offline",
        "-p",
        "dashboard-config-tools",
        "--",
        "capture-options",
        *arguments,
    ]
    if target_dir is not None:
        command[2:2] = ["--target-dir", str(target_dir)]
    result = run_bounded(command, cwd=REPO, env=environment, timeout=120, stdout_limit=8193)
    if result.returncode:
        raise ValueError("capture options producer failed or exceeded output bound")
    value = json_object(result.stdout)
    expected = {
        "schema_version",
        "normalized",
        "features",
        "no_default_features",
        "optimization",
        "custom_clock",
        "benchmark",
        "legacy_stock",
        "legacy_unavailable_backend",
        "legacy_longan_backend",
        "voltage_mv",
    }
    if set(value) != expected:
        raise ValueError("invalid fixed capture options envelope")
    features = items(value["features"])
    if (
        type(value["schema_version"]) is not int
        or value["schema_version"] != 2
        or value["no_default_features"] is not True
        or len(features) > 16
        or any(not isinstance(f, str) or not re.fullmatch("[a-z0-9-]+", f) for f in features)
        or features != sorted({text_value(feature) for feature in features})
        or value["optimization"] not in ("3", "2", "s", "z")
    ):
        raise ValueError("invalid fixed capture options envelope")
    fields(value["normalized"])
    for key in ("benchmark", "legacy_stock", "legacy_unavailable_backend", "legacy_longan_backend"):
        if type(value[key]) is not bool:
            raise ValueError("invalid fixed capture boolean")
    if type(value["voltage_mv"]) is not int:
        raise ValueError("invalid fixed capture voltage")
    if value["custom_clock"] is not None:
        custom = fields(value["custom_clock"])
        if set(custom) != {"cpu_hz", "spi_hz", "vsel_mv"} or any(
            not isinstance(v, str) or re.fullmatch("[0-9]+", v) is None for v in custom.values()
        ):
            raise ValueError("invalid fixed custom environment framing")
    return command, value


def firmware_package(output: str) -> str:
    records = cargo_records(output)
    matches = [
        r.get("package_id")
        for r in records
        if r.get("reason") == "compiler-artifact"
        and Path(text_value(r.get("manifest_path", ""))).resolve()
        == (FIRMWARE / "pico2/Cargo.toml").resolve()
        and fields(r.get("target", {})).get("kind") == ["bin"]
        and fields(r.get("profile", {})).get("test") is False
    ]
    if len(matches) != 1:
        raise ValueError("expected exactly one firmware package identity")
    return text_value(matches[0])


def verify_clock_literals(source: bytes, options: Mapping[str, object]) -> None:
    """Check fixed generated syntax/values, without resolving any clock policy."""
    text = source.decode("utf-8")
    expected_header = "// Generated from dashboard-build-profiles; do not edit.\n"
    if not text.startswith(expected_header):
        raise ValueError("unknown generated clock source schema")
    text = text[len(expected_header) :]
    scalar_pattern = r"pub const (\w+): (u32|u8|bool) = ([0-9]+|true|false);\n"
    matches = re.findall(scalar_pattern, text)
    scalars = {name: (kind, value) for name, kind, value in matches}
    if len(scalars) != len(matches):
        raise ValueError("duplicate generated clock literal")
    clock = fields(fields(options["normalized"])["clock"])
    expected = {
        key: ("u32", str(clock[field]))
        for key, field in [
            ("REQUESTED_CPU_HZ", "requested_cpu_hz"),
            ("ACTUAL_CPU_HZ", "actual_cpu_hz"),
            ("REQUESTED_SPI_HZ", "requested_spi_hz"),
            ("ACTUAL_SPI_HZ", "actual_spi_hz"),
        ]
    }
    expected.update(
        VOLTAGE_MV=("u32", str(options["voltage_mv"])),
        CPSDVSR=("u8", str(clock["cpsdvsr"])),
        SCR=("u8", str(clock["scr"])),
        LEGACY_ROUNDING=("bool", str(clock["legacy_rounding"]).lower()),
    )
    if scalars != expected:
        raise ValueError("generated scalar constants differ from normalized clock")
    remaining = re.sub(scalar_pattern, "", text)
    pll = fields(clock["pll"])
    expected_remaining = (
        f"pub const CORE_VOLTAGE: CoreVoltage = CoreVoltage::{text_value(clock['voltage']).upper()};\n"
        f"pub const PLL: PllConfig = PllConfig {{ refdiv: {pll['refdiv']}, fbdiv: {pll['fbdiv']}, "
        f"post_div1: {pll['postdiv1']}, post_div2: {pll['postdiv2']} }};\n"
    )
    if remaining != expected_remaining:
        raise ValueError("generated PLL/voltage constants differ from normalized clock")


def generated_clock(
    output: str, build_dir: Path, package_id: str, options: Mapping[str, object]
) -> ClockEvidence:
    records = cargo_records(output)
    matches = [
        r
        for r in records
        if r.get("reason") == "build-script-executed" and r.get("package_id") == package_id
    ]
    if len(matches) != 1:
        raise ValueError("expected one firmware clock generation record")
    record = matches[0]
    out = Path(text_value(record.get("out_dir"))).resolve()
    if not out.is_relative_to(build_dir.resolve()):
        raise ValueError("clock generation is outside this capture build")
    environment = cargo_environment(record)
    evidence: dict[str, bytes] = {}
    hashes: dict[str, str] = {}
    for label, filename, key in [
        ("source", "clock_profile.rs", "DASHBOARD_CLOCK_SOURCE_SHA256"),
        ("metadata", "clock_profile.json", "DASHBOARD_CLOCK_METADATA_SHA256"),
    ]:
        path = (out / filename).resolve()
        if not path.is_relative_to(build_dir.resolve()) or not path.is_file():
            raise ValueError("missing or outside-tree generated clock evidence")
        with path.open("rb") as stream:
            data = stream.read(4097)
        digest = hashlib.sha256(data).hexdigest()
        if len(data) > 4096 or digest != environment.get(key):
            raise ValueError("generated clock evidence hash/bound mismatch")
        evidence[label] = data
        hashes[label] = digest
    metadata = json_object(evidence["metadata"])
    expected = dict(
        schema_version=1,
        selection=fields(fields(options["normalized"])["requested"])["clock"],
        clock=fields(options["normalized"])["clock"],
        optimization=options["optimization"],
        features=options["features"],
    )
    if json.dumps(metadata, sort_keys=True, allow_nan=False) != json.dumps(
        expected, sort_keys=True, allow_nan=False
    ):
        raise ValueError("actual generated clock/options differ from immutable attempt")
    verify_clock_literals(evidence["source"], options)
    return ClockEvidence(
        out_dir=str(out),
        source=evidence["source"],
        metadata=evidence["metadata"],
        source_sha256=hashes["source"],
        metadata_sha256=hashes["metadata"],
    )


def configuration() -> dict[str, object]:
    expected = tomllib.loads((FIRMWARE / ".cargo/config.toml").read_text(encoding="utf-8"))
    result = {}
    for key in CONFIG_KEYS:
        call = run_checked(
            ["cargo", "-Z", "unstable-options", "config", "get", key, "--format", "json"],
            cwd=FIRMWARE,
            text=True,
            capture_output=True,
            encoding="utf-8",
        )
        if key not in expected:
            if call.returncode == 0 or f"config value `{key}` is not set" not in call.stderr:
                raise ValueError(f"unrecorded Cargo configuration for {key}")
        elif call.returncode or json.loads(call.stdout) != {key: expected[key]}:
            raise ValueError(f"effective {key} differs from checked-in Cargo configuration")
        result[key] = expected.get(key)
    return result


def inputs() -> dict[str, bytes]:
    paths = shared_inputs(REPO)
    # Cargo resolves every workspace member even when only dashboard-pico2 is built.
    # Bind the sibling manifest, build script and complete source/include tree too.
    for crate in (FIRMWARE / "pico2", FIRMWARE / "ble-probe"):
        paths.update(
            path
            for path in crate.rglob("*")
            if path.is_file()
            and not any(
                part in ("target", ".git", "__pycache__") for part in path.relative_to(crate).parts
            )
        )
    # Radio firmware blobs and include_bytes! inputs may live outside src/.
    # Hash the complete reviewed vendor trees and their provenance manifests.
    vendor = FIRMWARE / "vendor"
    paths.update(
        p
        for p in vendor.rglob("*")
        if p.is_file()
        and not any(
            part in ("target", ".git", "__pycache__") for part in p.relative_to(vendor).parts
        )
    )
    for directory in (FIRMWARE, FIRMWARE / "pico2", FIRMWARE / "ble-probe"):
        for name in [
            "Cargo.toml",
            "Cargo.lock",
            "build.rs",
            "rust-toolchain.toml",
            ".cargo/config.toml",
        ]:
            path = directory / name
            if path.is_file():
                paths.add(path)
    paths.update(
        [
            Path(__file__).resolve(),
            REPO / "host/scripts/benchmark_environment.py",
            REPO / "host/scripts/owned_process.py",
        ]
    )
    paths.update((FIRMWARE / "scripts").glob("*.py"))
    paths.update((FIRMWARE / "scripts").glob("*.ps1"))
    # __file__ can retain the native launcher's verbatim spelling, including in
    # imported inventory helpers. Normalize every member before relative keys.
    paths = {external_tool_path(p) for p in paths}
    return {p.relative_to(REPO).as_posix(): p.read_bytes() for p in sorted(paths)}


def emitted_elf(output: str) -> Path:
    candidates: list[Path] = []
    finished: list[bool] = []
    manifest = (FIRMWARE / "pico2/Cargo.toml").resolve()
    for record in cargo_records(output):
        if record.get("reason") == "build-finished":
            success = record.get("success")
            if type(success) is not bool:
                raise ValueError("Cargo success must be boolean")
            finished.append(success)
        if (
            record.get("reason") == "compiler-artifact"
            and Path(text_value(record.get("manifest_path", ""))).resolve() == manifest
            and fields(record.get("target", {})).get("name") == "pico2"
            and fields(record.get("target", {})).get("kind") == ["bin"]
            and fields(record.get("profile", {})).get("test") is False
        ):
            executable = record.get("executable")
            if not isinstance(executable, str) or not Path(executable).is_absolute():
                raise ValueError("Cargo emitted no absolute firmware executable")
            candidates.append(Path(executable).resolve())
    if finished != [True] or len(candidates) != 1:
        raise ValueError("expected one successful build and one firmware executable")
    return candidates[0]


def bounded_bytes(path: Path) -> bytes:
    if not path.is_file():
        raise ValueError("configuration must be a regular binary .eav file")
    with path.open("rb") as stream:
        data = stream.read(16385)
    if len(data) > 16384:
        raise ValueError("configuration exceeds 16 KiB")
    return data


def selected_config(environment: Mapping[str, str]) -> tuple[str, bytes] | tuple[None, None]:
    selected = environment.get("DASHBOARD_CONFIG")
    if selected is None:
        return None, None
    if not selected or any(ord(ch) < 32 or ord(ch) == 127 for ch in selected):
        raise ValueError("DASHBOARD_CONFIG must be a nonempty path without controls")
    # Match build.rs: environment-relative paths are relative to the crate.
    path = (FIRMWARE / "pico2" / selected).absolute()
    return str(path), bounded_bytes(path)


def freeze_config(
    selection: tuple[str, bytes] | tuple[None, None],
    destination: Path,
    environment: Mapping[str, str],
) -> tuple[dict[str, str], ConfigRecord]:
    selected, data = selection
    frozen = dict(environment)
    if selected is None:
        frozen.pop("DASHBOARD_CONFIG", None)
        return frozen, dict(selection="default")
    if data is None:
        raise ValueError("explicit configuration requires bytes")
    path = destination / "selected.eav"
    path.write_bytes(data)
    frozen["DASHBOARD_CONFIG"] = str(path.resolve())
    return frozen, dict(
        selection="explicit", source=selected, sha256=hashlib.sha256(data).hexdigest()
    )


def embedded_config(output: str, build_dir: Path) -> bytes:
    matches = []
    for record in cargo_records(output):
        if record.get("reason") == "build-script-executed":
            environment = cargo_environment(record)
            if "DASHBOARD_CONFIG_SHA256" in environment:
                path = (Path(text_value(record.get("out_dir"))) / "dashboard.eav").resolve()
                if not path.is_relative_to(build_dir.resolve()):
                    raise ValueError("embedded config is outside this build")
                data = bounded_bytes(path)
                if hashlib.sha256(data).hexdigest() != environment["DASHBOARD_CONFIG_SHA256"]:
                    raise ValueError("embedded config differs from validated build snapshot")
                matches.append(data)
    if len(matches) != 1:
        raise ValueError("expected one embedded configuration snapshot")
    return matches[0]


def verify_selected_config(
    destination: Path,
    environment: Mapping[str, str],
    embedded: bytes,
    record: ConfigRecord,
    target_dir: Path | None = None,
    *,
    reuse_host: bool = False,
) -> None:
    """Compare to the canonical upgrade, retaining the original input identity."""
    if record["selection"] != "explicit":
        return
    original = bounded_bytes(destination / "selected.eav")
    if hashlib.sha256(original).hexdigest() != record["sha256"]:
        raise ValueError("frozen selected configuration changed")
    normalized = destination / "normalized.eav"
    # Native attempts already built this exact host CLI in their private target.
    profile_flags = ["--offline"] if reuse_host else ["--release"]
    command = [
        "cargo",
        "run",
        "--locked",
        *profile_flags,
        "-p",
        "dashboard-config-tools",
        "--target-dir",
        str(target_dir if target_dir is not None else destination / "config-compiler"),
        "--",
        "compile",
        str(destination / "selected.eav"),
        str(normalized),
    ]
    run_checked(command, cwd=REPO, env=environment, check=True, timeout=1800)
    expected = bounded_bytes(normalized)
    if bounded_bytes(destination / "selected.eav") != original:
        raise ValueError("frozen selected configuration changed during normalization")
    if expected != embedded:
        raise ValueError("embedded configuration differs from canonical selected file")
    record["normalization_command"] = command
    record["normalized_sha256"] = hashlib.sha256(expected).hexdigest()


def freeze_startup(
    profile_path: str | Path | None,
    asset_path: str | Path | None,
    destination: Path,
    environment: dict[str, str],
) -> dict[str, StartupMember] | None:
    if profile_path is None:
        if asset_path is not None:
            raise ValueError("startup asset requires explicit profile")
        if any(k in environment for k in ("DASHBOARD_STARTUP_PROFILE", "DASHBOARD_STARTUP_ASSET")):
            raise ValueError("use explicit startup arguments instead of ambient inputs")
        return None
    result: dict[str, StartupMember] = {}
    for key, path, name, maximum in [
        ("DASHBOARD_STARTUP_PROFILE", profile_path, "startup_profile.bin", 16),
        ("DASHBOARD_STARTUP_ASSET", asset_path, "startup_asset.bin", 524288),
    ]:
        if path is None:
            data = b""
        else:
            with Path(path).open("rb") as stream:
                data = stream.read(maximum + 1)
        if len(data) > maximum or (key.endswith("PROFILE") and len(data) != 16):
            raise ValueError("startup input byte bound")
        target = destination / name
        target.write_bytes(data)
        if data:
            environment[key] = str(target.resolve())
        else:
            environment.pop(key, None)
        result[name] = {"sha256": hashlib.sha256(data).hexdigest(), "bytes": len(data)}
    return result


def verify_startup(
    output: str, build_dir: Path, destination: Path, record: Mapping[str, StartupMember] | None
) -> None:
    if record is None:
        return
    matches = []
    for row in cargo_records(output):
        if row.get("reason") != "build-script-executed":
            continue
        env = cargo_environment(row)
        if "DASHBOARD_STARTUP_SHA256" not in env:
            continue
        for name, key in [
            ("startup_profile.bin", "DASHBOARD_STARTUP_SHA256"),
            ("startup_asset.bin", "DASHBOARD_STARTUP_ASSET_SHA256"),
        ]:
            path = (Path(text_value(row.get("out_dir"))) / name).resolve()
            if not path.is_relative_to(build_dir.resolve()):
                raise ValueError("startup output outside build")
            with path.open("rb") as stream:
                data = stream.read(524289)
            expected = record[name]
            if (
                len(data) != expected["bytes"]
                or hashlib.sha256(data).hexdigest() != expected["sha256"]
                or env.get(key) != expected["sha256"]
                or (destination / name).read_bytes() != data
            ):
                raise ValueError("startup snapshot identity mismatch")
        matches.append(row)
    if len(matches) != 1:
        raise ValueError("expected one startup snapshot producer")


def build_directories(destination: Path, native: bool) -> tuple[Path, Path | None, Path | None]:
    """Native scratch is fixed inside the locked attempt, never a user path/cache."""
    if not native:
        return destination / "build", None, None
    if REPO.name != "source" or destination != FIRMWARE / "target/bench-captures/native":
        raise ValueError("native scratch requires the fixed frozen source layout")
    root = external_tool_path(REPO.parent / "b")
    if root != REPO.parent / "b" or not root.is_dir() or any(root.iterdir()):
        raise ValueError("native scratch must be an empty owned attempt directory")
    paths = (root / "f", root / "h", root / "n")
    # Allow 145 UTF-16 units for this locked graph's Cargo/MSVC output suffixes.
    # This is a host path limit, not a flash/RAM budget or arbitrary-package guarantee.
    if os.name == "nt" and any(len(str(p).encode("utf-16-le")) // 2 + 145 >= 260 for p in paths):
        raise ValueError("native build path too long for MSVC; move repository to a shorter path")
    return paths


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("name")
    parser.add_argument("--features")
    parser.add_argument("--no-default-features", action="store_true")
    parser.add_argument("--native-scratch", action="store_true", help=argparse.SUPPRESS)
    parser.add_argument(
        "--firmware",
        action="store_true",
        help="allow an ordinary firmware image without target-bench",
    )
    parser.add_argument("--dashboard-config", help="binary .eav file; overrides DASHBOARD_CONFIG")
    parser.add_argument("--startup-profile", help="explicit EASP16 input")
    parser.add_argument("--startup-asset", help="explicit converted EASA input")
    parser.add_argument(
        "--options-json",
        help="bounded shared NormalizedBuildOptions JSON; excludes raw feature switches",
    )
    parser.add_argument(
        "--expert",
        action="store_true",
        help="explicit local authorization for experimental/custom clock selection",
    )
    parser.add_argument("--optimization", choices=["3", "2", "s", "z"])
    parser.add_argument("--cpu-hz")
    parser.add_argument("--spi-hz")
    parser.add_argument("--vsel-mv")
    parser.add_argument(
        "--image-gate",
        choices=("practical", "experimental-full"),
        default="practical",
        help="static software checks, or unfinished complete instruction proof",
    )
    args = parser.parse_args()
    if not re.fullmatch(r"[A-Za-z0-9][A-Za-z0-9_-]*", args.name):
        parser.error("invalid capture name")
    if args.options_json is not None and (
        args.features is not None
        or args.no_default_features
        or args.expert
        or args.optimization
        or args.cpu_hz is not None
        or args.spi_hz is not None
        or args.vsel_mv is not None
    ):
        parser.error("typed options cannot be combined with raw capture overrides")
    destination = FIRMWARE / "target/bench-captures" / args.name
    if destination.exists():
        parser.error("capture name already exists")
    build_dir, host_dir, normalize_dir = build_directories(destination, args.native_scratch)
    if any(k.startswith("DEFMT_") for k in os.environ):
        parser.error("unset external DEFMT overrides")
    check_clock_environment(os.environ)
    rustc, version = compiler(FIRMWARE)
    config = configuration()
    before = inputs()
    host_environment = os.environ.copy()
    host_environment["RUSTC"] = rustc
    if args.options_json is not None:
        options_arguments = ["--normalized-json", args.options_json]
    else:
        options_arguments = [
            "--features",
            args.features
            if args.features is not None
            else "target-bench,ble-obd,frame-trace,te-trace",
            "--no-default-features",
            str(args.no_default_features).lower(),
            "--expert",
            str(args.expert).lower(),
        ]
        for key, value in [
            ("--optimization", args.optimization),
            ("--cpu-hz", args.cpu_hz),
            ("--spi-hz", args.spi_hz),
            ("--vsel-mv", args.vsel_mv),
        ]:
            if value is not None:
                options_arguments.extend([key, value])
    options_command, options = fixed_json_tool(options_arguments, host_environment, host_dir)
    if not args.firmware and not options["benchmark"]:
        parser.error("non-firmware capture requires explicit target-bench")
    selected_environment = os.environ.copy()
    if args.dashboard_config is not None:
        selected_environment["DASHBOARD_CONFIG"] = args.dashboard_config
    selection = selected_config(selected_environment)
    # Reserve the capture atomically and give this build its own output tree.
    # Failed runs stay incomplete under their original name for inspection.
    destination.mkdir(parents=True)
    attempt_bytes = json.dumps(options, sort_keys=True, separators=(",", ":")).encode("utf-8")
    (destination / "build-options.json").write_bytes(attempt_bytes)
    environment, config_record = freeze_config(selection, destination, selected_environment)
    startup_record = freeze_startup(
        args.startup_profile, args.startup_asset, destination, environment
    )
    environment.update(
        RUSTC=rustc,
        CARGO_PROFILE_RELEASE_STRIP="none",
        CARGO_PROFILE_RELEASE_OPT_LEVEL=text_value(options["optimization"]),
    )
    if options["custom_clock"] is not None:
        custom = fields(options["custom_clock"])
        if set(custom) != {"cpu_hz", "spi_hz", "vsel_mv"} or any(
            not isinstance(v, str) or not re.fullmatch("[0-9]+", v) for v in custom.values()
        ):
            raise ValueError("invalid fixed custom environment framing")
        environment.update(
            zip(CUSTOM_KEYS, [text_value(custom[key]) for key in ("cpu_hz", "spi_hz", "vsel_mv")])
        )
    command = [
        "cargo",
        "build",
        "--locked",
        "-p",
        "dashboard-pico2",
        "--release",
        "--target",
        "thumbv8m.main-none-eabihf",
        "--target-dir",
        str(build_dir),
        "--message-format=json",
        "--features",
        ",".join(text_value(feature) for feature in items(options["features"])),
        "--no-default-features",
    ]
    print("Capture: compiling accepted frozen project", flush=True)
    build = run_build(command, environment, destination)
    verify_startup(build.stdout, build_dir, destination, startup_record)
    elf = emitted_elf(build.stdout)
    clock_evidence = generated_clock(
        build.stdout, build_dir, firmware_package(build.stdout), options
    )
    config_bytes = embedded_config(build.stdout, build_dir)
    verify_selected_config(
        destination,
        host_environment if args.native_scratch else environment,
        config_bytes,
        config_record,
        host_dir if args.native_scratch else normalize_dir,
        reuse_host=args.native_scratch,
    )
    (destination / "dashboard.eav").write_bytes(config_bytes)
    config_record["embedded_sha256"] = hashlib.sha256(config_bytes).hexdigest()
    if not elf.is_relative_to(build_dir.resolve()):
        raise ValueError("Cargo emitted an executable outside this capture build")
    frozen = destination / "pico2.elf"
    elf_bytes = elf.read_bytes()
    frozen.write_bytes(elf_bytes)
    _, base_program = inspect_program(ElfImage(elf_bytes))
    program_image: dict[str, object] = dict(base_program)
    program_image["linked_configuration"] = linked_configuration(ElfImage(elf_bytes), config_bytes)
    if startup_record is not None:
        program_image["linked_startup"] = linked_startup(
            ElfImage(elf_bytes),
            (destination / "startup_profile.bin").read_bytes(),
            (destination / "startup_asset.bin").read_bytes(),
        )
    source_context = make_source_context(elf_bytes, destination.name, attempt_bytes, before)
    source_context_path = destination / "source-context.json"
    write_source_context(source_context_path, source_context)
    source_context_bytes = source_context_path.read_bytes()
    source_context_record = dict(
        path="source-context.json",
        sha256=hashlib.sha256(source_context_bytes).hexdigest(),
        binding=source_context.binding(),
    )
    print(
        "Capture: validating linked configuration, program image, memory and stack gates",
        flush=True,
    )
    memory_evidence = run_memory_gate(
        frozen, destination, config_bytes, source_context, source_context_path, args.image_gate
    )
    for flags in [[], ["-O"]]:
        run_checked(
            [sys.executable, *flags, "scripts/test_flash_lockout.py", str(frozen)],
            cwd=FIRMWARE,
            check=True,
        )
    if before != inputs() or config != configuration() or (rustc, version) != compiler(FIRMWARE):
        raise ValueError("build inputs changed during capture")
    if frozen.read_bytes() != elf_bytes or elf.read_bytes() != elf_bytes:
        raise ValueError("ELF changed during capture")
    if selection != selected_config(selected_environment):
        raise ValueError("selected configuration changed during capture")
    check_clock_environment(os.environ)
    if (destination / "build-options.json").read_bytes() != attempt_bytes:
        raise ValueError("immutable build attempt changed during capture")
    if clock_evidence != generated_clock(
        build.stdout, build_dir, firmware_package(build.stdout), options
    ):
        raise ValueError("clock generation changed during capture")
    if (
        (destination / "build-options.json").read_bytes() != attempt_bytes
        or source_context_path.read_bytes() != source_context_bytes
        or load_source_context(
            source_context_path, ElfImage(frozen), destination.name, attempt_bytes
        )
        != source_context
    ):
        raise ValueError("captured source sidecar changed during the gate")
    if memory_evidence != read_memory_gate(
        destination / "memory-layout.json", frozen, config_bytes, source_context, args.image_gate
    ):
        raise ValueError("selected image result changed during capture")
    (destination / "clock_profile.rs").write_bytes(clock_evidence["source"])
    (destination / "clock_profile.json").write_bytes(clock_evidence["metadata"])
    with zipfile.ZipFile(
        destination / "source.zip", "w", compression=zipfile.ZIP_DEFLATED
    ) as archive:
        for name, content in before.items():
            archive.writestr(name, content)
        archive.writestr("build-config/source-context.json", source_context_bytes)
        archive.writestr("build-config/dashboard.eav", config_bytes)
        if startup_record is not None:
            for name in startup_record:
                archive.writestr("build-config/" + name, (destination / name).read_bytes())
        archive.writestr("build-config/build-options.json", attempt_bytes)
        archive.writestr("build-config/clock_profile.rs", clock_evidence["source"])
        archive.writestr("build-config/clock_profile.json", clock_evidence["metadata"])
        if selection[1] is not None:
            archive.writestr("build-config/selected.eav", selection[1])
    verify_startup(build.stdout, build_dir, destination, startup_record)
    hashes = {name: hashlib.sha256(data).hexdigest() for name, data in before.items()}
    record = dict(
        schema=2,
        image_gate=args.image_gate,
        scope="Build artifact only; hardware not executed",
        rustc=version,
        rustc_path=rustc,
        command=command,
        emitted_executable=str(elf),
        cargo_config=config,
        explicit_environment={
            key: environment[key]
            for key in [
                "CARGO_PROFILE_RELEASE_STRIP",
                "CARGO_PROFILE_RELEASE_OPT_LEVEL",
                "RUSTC",
                "DASHBOARD_CONFIG",
                *CUSTOM_KEYS,
            ]
            if key in environment
        },
        options_command=options_command,
        build_options=options,
        build_options_sha256=hashlib.sha256(attempt_bytes).hexdigest(),
        generated_clock={
            k: v for k, v in clock_evidence.items() if k not in ("source", "metadata")
        },
        program_image=program_image,
        startup=startup_record,
        dashboard_config=config_record,
        source_context=source_context_record,
        memory_layout=memory_evidence["record"],
        memory_layout_json_sha256=memory_evidence["sha256"],
        sha256=hashes,
        elf_sha256=hashlib.sha256(elf_bytes).hexdigest(),
    )
    (destination / "manifest.json").write_text(
        json.dumps(record, indent=2) + "\n", encoding="utf-8"
    )
    print("Capture: frozen software evidence complete", flush=True)
    print(destination)
    print("ELF SHA256 " + hashlib.sha256(elf_bytes).hexdigest())


if __name__ == "__main__":
    main()
