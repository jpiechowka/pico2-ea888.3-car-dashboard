"""Compile a positive control, then require the intended profile diagnostics."""
from validation import require
from itertools import combinations
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[1]
CLOCKS = ["cpu250-spi62-1v10", "cpu280-spi70-1v30", "cpu290-spi72-1v30", "cpu300-spi75-1v30"]


def compile_features(features):
    return subprocess.run(
        ["cargo", "check", "--locked", "-p", "dashboard-pico2", "--features", features],
        cwd=ROOT, text=True, encoding="utf-8", errors="replace", capture_output=True,
    )


if __name__ == "__main__":
    control = compile_features("longan-rx12,frame-trace,te-trace")
    if control.returncode:
        raise SystemExit(control.stdout + control.stderr)
    cases = [(a + "," + b, "Only one overclock feature can be enabled") for a, b in combinations(CLOCKS, 2)]
    cases.append(("demo,longan-rx12", "Select either demo or longan-rx12"))
    for features, diagnostic in cases:
        result = compile_features(features)
        require(result.returncode and diagnostic in result.stderr, result.stdout + result.stderr)
        print("Rejected incompatible profile:", features)
    print("Positive control and all seven intended rejection diagnostics verified")
