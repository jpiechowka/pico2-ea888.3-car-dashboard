"""Compile a positive control, then require the intended profile diagnostics."""

from validation import require
from itertools import combinations
from pathlib import Path
import json
import os
import subprocess

ROOT = Path(__file__).resolve().parents[1]
CUSTOM_KEYS = ("DASHBOARD_CPU_HZ", "DASHBOARD_SPI_HZ", "DASHBOARD_VSEL_MV")


def compile_features(features, custom=None):
    environment = os.environ.copy()
    for key in CUSTOM_KEYS:
        environment.pop(key, None)
    if custom is not None:
        environment.update(zip(CUSTOM_KEYS, custom))
    return subprocess.run(
        ["cargo", "check", "--locked", "-p", "dashboard-pico2", "--features", features],
        cwd=ROOT,
        env=environment,
        text=True,
        encoding="utf-8",
        errors="replace",
        capture_output=True,
    )


if __name__ == "__main__":
    data = subprocess.check_output(
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
    )
    require(len(data) <= 65537, "catalogue export exceeds bound")
    CLOCKS = [row["profile"]["feature"] for row in json.loads(data)["profiles"]]
    require(len(CLOCKS) == 8 and len(set(CLOCKS)) == 8, "invalid catalogue inventory")
    control = compile_features("ble-obd,frame-trace,te-trace")
    if control.returncode:
        raise SystemExit(control.stdout + control.stderr)
    for feature in CLOCKS:
        result = compile_features(feature + ",ble-obd,frame-trace,te-trace")
        require(result.returncode == 0, result.stdout + result.stderr)
        print("Positive profile:", feature)
    result = compile_features("demo", ["50000000", "25000000", "1100"])
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
        result = compile_features(feature, ["50000000", "25000000", "1100"])
        require(
            result.returncode
            and "named profile cannot be combined with custom clock" in result.stderr,
            result.stdout + result.stderr,
        )
        print("Rejected named/custom:", feature)
    print(
        "BLE, eight profile and custom controls; all28 clock pairs, four backend conflicts and eight named/custom rejections verified"
    )
