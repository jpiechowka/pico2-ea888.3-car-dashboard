"""Compile a positive control, then require the intended profile diagnostics."""

from validation import require
from itertools import combinations
from pathlib import Path
from dataclasses import dataclass
import json
import os
import subprocess
import sys

from script_values import array_items, object_fields, string_value

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT.parent / "host/scripts"))
from owned_process import run_bounded, run_checked  # noqa: E402

CUSTOM_KEYS = ("DASHBOARD_CPU_HZ", "DASHBOARD_SPI_HZ", "DASHBOARD_VSEL_MV")


@dataclass(frozen=True)
class ClockProfile:
    feature: str
    aliases: tuple[str, ...]


def clock_profiles() -> list[ClockProfile]:
    result = run_bounded(
        [
            "cargo",
            "run",
            "--quiet",
            "--locked",
            "--offline",
            "-p",
            "dashboard-config-tools",
            "--",
            "profiles",
        ],
        cwd=ROOT.parent,
        timeout=300,
        stdout_limit=65536,
    )
    result.check_returncode()
    return parse_profiles(result.stdout)


def parse_profiles(data: bytes) -> list[ClockProfile]:
    require(len(data) <= 65536, "catalogue export exceeds bound")
    raw: object = json.loads(data)
    root = object_fields(raw, "catalogue must be an object")
    profiles: list[ClockProfile] = []
    for raw_row in array_items(root.get("profiles"), "catalogue profiles must be an array"):
        row = object_fields(raw_row, "invalid catalogue row")
        profile = object_fields(row.get("profile"), "invalid catalogue profile")
        feature = string_value(profile.get("feature"), "profile feature must be a string")
        require(bool(feature) and "," not in feature, "invalid individual profile feature")
        aliases = tuple(
            string_value(alias, "profile alias must be a string")
            for alias in array_items(profile.get("aliases"), "profile aliases must be an array")
        )
        profiles.append(ClockProfile(feature, aliases))
    require(
        len(profiles) == 8 and len({profile.feature for profile in profiles}) == 8,
        "invalid catalogue inventory",
    )
    return profiles


def compile_features(
    features: str, custom: tuple[str, str, str] | None = None
) -> subprocess.CompletedProcess[str]:
    environment = os.environ.copy()
    for key in CUSTOM_KEYS:
        environment.pop(key, None)
    if custom is not None:
        environment.update(zip(CUSTOM_KEYS, custom, strict=True))
    return run_checked(
        ["cargo", "check", "--locked", "-p", "dashboard-pico2", "--features", features],
        cwd=ROOT,
        env=environment,
        timeout=600,
        capture_output=True,
    )


if __name__ == "__main__":
    CLOCKS = [row.feature for row in clock_profiles()]
    control = compile_features("ble-obd,frame-trace,te-trace")
    if control.returncode:
        raise SystemExit(control.stdout + control.stderr)
    for feature in CLOCKS:
        result = compile_features(feature + ",ble-obd,frame-trace,te-trace")
        require(result.returncode == 0, result.stdout + result.stderr)
        print("Positive profile:", feature)
    result = compile_features("demo", ("50000000", "25000000", "1100"))
    require(result.returncode == 0, result.stdout + result.stderr)
    cases = [
        (a + "," + b, "Only one clock profile feature can be enabled")
        for a, b in combinations(CLOCKS, 2)
    ]
    for backends in (
        "demo,ble-obd",
        "demo,longan-rx12",
        "ble-obd,longan-rx12",
        "demo,ble-obd,longan-rx12",
    ):
        cases.append(
            (backends, "Select exactly one acquisition backend: demo, ble-obd or longan-rx12")
        )
    for features, diagnostic in cases:
        result = compile_features(features)
        require(result.returncode and diagnostic in result.stderr, result.stdout + result.stderr)
        print("Rejected incompatible profile:", features)
    for feature in CLOCKS:
        result = compile_features(feature, ("50000000", "25000000", "1100"))
        require(
            result.returncode
            and "named profile cannot be combined with custom clock" in result.stderr,
            result.stdout + result.stderr,
        )
        print("Rejected named/custom:", feature)
    print(
        "BLE, eight profile and custom controls; all28 clock pairs, four backend conflicts and eight named/custom rejections verified"
    )
