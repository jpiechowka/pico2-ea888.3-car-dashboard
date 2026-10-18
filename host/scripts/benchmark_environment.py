"""Conservative host capture policy; never persist arbitrary config/secrets."""

from collections.abc import Mapping
import os
from pathlib import Path
import subprocess

CONFIG_KEYS = (
    "build",
    "target",
    "profile",
    "env",
    "paths",
    "patch",
    "source",
    "unstable",
    "include",
)
ENV_PREFIXES = (
    "RUSTC",
    "RUSTFLAGS",
    "RUSTDOC",
    "CARGO_ENCODED_",
    "CARGO_BUILD_",
    "CARGO_PROFILE_",
    "CARGO_TARGET_",
    "CARGO_INCREMENTAL",
)


def validate_filter(value: str) -> None:
    if value.startswith("-"):
        raise ValueError("benchmark filter cannot begin with a CLI option prefix")


def check_environment(env: Mapping[str, str]) -> None:
    bad = sorted(name for name, value in env.items() if value and name.startswith(ENV_PREFIXES))
    if bad:
        raise ValueError("unset benchmark compiler/config overrides: " + ", ".join(bad))


def configuration(host: str | Path) -> dict[str, str]:
    # Query effective configuration, including Cargo home, ancestors and includes.
    # Reject overrides rather than recording potentially private values. Manifests
    # and the exact normal compiler remain in the run's source/compiler evidence.
    for key in CONFIG_KEYS:
        result = subprocess.run(
            ["cargo", "-Z", "unstable-options", "config", "get", key, "--format", "json"],
            cwd=host,
            capture_output=True,
            text=True,
            encoding="utf-8",
        )
        absent = f"config value `{key}` is not set" in result.stderr
        if result.returncode == 0 or not absent:
            raise ValueError(f"benchmark capture requires default Cargo config for {key}")
    return {key: "not set" for key in CONFIG_KEYS}


def compiler(host: str | Path) -> tuple[str, str]:
    check_environment(os.environ)
    rustc = subprocess.check_output(
        ["rustup", "which", "rustc"], cwd=host, text=True, encoding="utf-8"
    ).strip()
    if not Path(rustc).is_file():
        raise ValueError("rustup compiler is unavailable")
    version = subprocess.check_output([rustc, "-Vv"], cwd=host, text=True, encoding="utf-8")
    return rustc, version
