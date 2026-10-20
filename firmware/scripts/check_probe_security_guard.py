"""Require the specific no-security feature-unification rejection."""

import argparse
from pathlib import Path
import sys

from validation import require
from cargo_diagnostics import compiler_errors

REPO = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(REPO / "host/scripts"))
from owned_process import run_checked  # noqa: E402

GUARD = "discovery-no-security forbids the security feature; build the BLE probe separately from pairing-enabled firmware"


def verify_rejection(code: int, output: str) -> None:
    errors = [error.message for error in compiler_errors(output)]
    require(code != 0 and errors == [GUARD], "expected only discovery-no-security feature guard")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--target", required=True)
    args = parser.parse_args()
    result = run_checked(
        [
            "cargo",
            "check",
            "--locked",
            "--manifest-path",
            str(REPO / "firmware/vendor/trouble-host/Cargo.toml"),
            "--target",
            args.target,
            "--no-default-features",
            "--message-format=json",
            "--features",
            "central,gatt,scan,discovery-no-security,security,default-packet-pool",
        ],
        cwd=REPO,
        timeout=300,
        capture_output=True,
    )
    try:
        verify_rejection(result.returncode, result.stdout)
    except (AssertionError, ValueError):
        print(result.stdout[-8192:], file=sys.stderr)
        print(result.stderr[-8192:], file=sys.stderr)
        raise
    print("Exact discovery-no-security feature guard confirmed")


if __name__ == "__main__":
    main()
