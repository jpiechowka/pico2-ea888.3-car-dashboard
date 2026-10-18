"""Compatibility entry point for typed radio source and firmware provenance."""

import json
from pathlib import Path

from check_vendor import run_verifier

ROOT = Path(__file__).resolve().parents[1] / "vendor"


def verify_tree(root: Path, original: dict[str, str], patched: dict[str, str]) -> None:
    """Retain the test/consumer API without a second verifier implementation."""
    run_verifier(
        ["tree", "--root", str(root.absolute())],
        json.dumps({"original": original, "modified": patched}),
    )


def check() -> None:
    print(run_verifier(["radio", "--vendor-root", str(ROOT)]), end="")


if __name__ == "__main__":
    check()
