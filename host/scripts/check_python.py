"""Check every tracked or non-ignored Python source, including tests and vendor code."""

import argparse
from pathlib import Path, PurePosixPath
import sys
import subprocess
from collections.abc import Sequence

from owned_process import run_bounded

REPO = Path(__file__).resolve().parents[2]


def source_paths(repo: Path, listing: bytes) -> list[Path]:
    """Admit Git's NUL-separated paths without passing filenames as tool options."""
    if not listing or not listing.endswith(b"\0") or len(listing) > 4 * 1024 * 1024:
        raise ValueError("invalid or oversized Python inventory")
    names = listing[:-1].decode("utf-8").split("\0")
    if len(names) > 4096:
        raise ValueError("Python inventory exceeds 4096 entries")
    paths: set[Path] = set()
    root = repo.resolve()
    for name in names:
        relative = PurePosixPath(name)
        if (
            not name
            or relative.is_absolute()
            or relative.as_posix() != name
            or ".." in relative.parts
            or "\\" in name
            or ":" in name
            or any(ord(char) < 32 for char in name)
            or relative.suffix != ".py"
        ):
            raise ValueError(f"invalid Python source path: {name!r}")
        path = root.joinpath(*relative.parts)
        if path.is_symlink() or not path.is_file() or not path.resolve().is_relative_to(root):
            raise ValueError(f"Python source is missing or outside the repository: {name!r}")
        paths.add(path)
    return sorted(paths)


def check(tool: str, paths: Sequence[Path], platform: str | None) -> int:
    arguments = {
        "mypy": ["mypy", "--strict", "--follow-imports=normal"],
        "lint": ["ruff", "check"],
        "format": ["ruff", "format", "--check"],
    }[tool]
    if tool == "mypy" and platform is not None:
        arguments.extend(["--platform", platform])
    result = run_bounded(
        [sys.executable, "-m", *arguments, *(str(path) for path in paths)],
        cwd=REPO,
        timeout=600,
        stdout_limit=8 * 1024 * 1024,
        stderr_limit=1024 * 1024,
    )
    sys.stdout.buffer.write(result.stdout)
    return result.returncode


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("tool", choices=("mypy", "lint", "format", "list"))
    parser.add_argument("--platform", choices=("win32", "linux"))
    args = parser.parse_args()
    result = run_bounded(
        ["git", "ls-files", "-z", "--cached", "--others", "--exclude-standard", "--", "*.py"],
        cwd=REPO,
        timeout=15,
        stdout_limit=4 * 1024 * 1024,
        stderr_limit=65536,
    )
    if result.returncode:
        raise ValueError("cannot inventory Python sources")
    paths = source_paths(REPO, result.stdout)
    if args.tool == "list":
        for path in paths:
            print(path.relative_to(REPO).as_posix())
        return 0
    print(f"Checking all {len(paths)} Python sources ({args.tool})", flush=True)
    return check(args.tool, paths, args.platform)


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (ValueError, OSError, RuntimeError, subprocess.SubprocessError) as error:
        print(f"Python quality failed: {error}", file=sys.stderr)
        raise SystemExit(1) from error
