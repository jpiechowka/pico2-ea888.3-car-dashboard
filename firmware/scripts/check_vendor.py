"""Compatibility entry point for the typed, bounded Rust provenance verifier.

Optional --archive verifies the downloaded .crate and every original file.
No archive contents are extracted or executed.
"""

import argparse
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
ROOT = REPO / "firmware" / "vendor"
sys.path.insert(0, str(REPO / "host/scripts"))
from owned_process import run_bounded  # noqa: E402


def run_verifier(arguments: list[str], input_json: str | None = None) -> str:
    """Keep admission in Rust; launch from root so firmware ARM flags do not leak."""
    result = run_bounded(
        [
            "cargo",
            "run",
            "--release",
            "--quiet",
            "--locked",
            "--manifest-path",
            str(REPO / "Cargo.toml"),
            "-p",
            "dashboard-provenance",
            "--",
            *arguments,
        ],
        cwd=REPO,
        input_bytes=None if input_json is None else input_json.encode("utf-8"),
        on_stderr=lambda _: None,
        timeout=300,
    )
    if result.returncode:
        raise AssertionError(
            f"provenance verifier failed ({result.returncode}): {result.stderr[-4096:].decode('utf-8', errors='replace')}"
        )
    return result.stdout.decode("utf-8", errors="replace")


def check(archive: Path | None = None) -> None:
    arguments = ["hal", "--vendor-root", str(ROOT)]
    if archive is not None:
        arguments.extend(["--archive", str(archive.absolute())])
    print(run_verifier(arguments), end="")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--archive", type=Path)
    check(parser.parse_args().archive)
