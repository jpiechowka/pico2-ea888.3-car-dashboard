"""Fail closed on additive RTT features and operational flush imports/calls.

This is a source policy guard, not a proof about arbitrary future dependencies.
The locked defmt macros and panic-probe source were separately reviewed.
"""

import json
from validation import require
from pathlib import Path
import re
import sys

from script_values import array_items, object_fields, string_value

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT.parent / "host/scripts"))
from owned_process import run_checked  # noqa: E402


def check_source(source: str, label: str | Path) -> None:
    require(not re.search(r"\bdefmt\s*::\s*flush\b", source), f"blocking RTT flush in {label}")
    # Conservatively reject the entire use item, including grouped imports,
    # leading ::, re-exports and nested aliases. False positives in comments
    # are intentional; this is a policy scanner, not a complete Rust parser.
    require(
        not re.search(r"\buse\b[^;]*\bdefmt\b[^;]*;", source),
        f"use explicit defmt macro paths in {label}",
    )
    require(not re.search(r"\bextern\s+crate\s+defmt\b", source), f"defmt alias in {label}")


def check_app_sources() -> None:
    roots = [ROOT / "pico2" / "src", ROOT / "ble-probe" / "src"]
    roots.extend(
        p / "src"
        for p in (ROOT.parent / "crates").iterdir()
        if p.is_dir() and (p / "Cargo.toml").is_file()
    )
    for root in roots:
        for path in root.rglob("*.rs"):
            check_source(path.read_text(encoding="utf-8"), path)


def check_metadata(metadata: object) -> None:
    graph = object_fields(metadata, "invalid Cargo package graph")
    packages: dict[str, tuple[str, str]] = {}
    for raw in array_items(graph.get("packages"), "Cargo packages must be an array"):
        package = object_fields(raw, "invalid Cargo package")
        identity = string_value(package.get("id"), "Cargo package lacks identity")
        require(identity not in packages, "duplicate Cargo package identity")
        packages[identity] = (
            string_value(package.get("name"), "Cargo package lacks name"),
            string_value(package.get("version"), "Cargo package lacks version"),
        )
    resolve = object_fields(graph.get("resolve"), "Cargo graph is unresolved")
    rtt: list[tuple[str, list[str]]] = []
    for raw in array_items(resolve.get("nodes"), "Cargo nodes must be an array"):
        node = object_fields(raw, "invalid Cargo node")
        identity = string_value(node.get("id"), "Cargo node lacks identity")
        require(identity in packages, "Cargo node refers to an unknown package")
        features = [
            string_value(feature, "Cargo feature must be a string")
            for feature in array_items(node.get("features"), "Cargo features must be an array")
        ]
        name, version = packages[identity]
        if name == "defmt-rtt":
            rtt.append((version, features))
    require(len(rtt) == 1, "expected one reviewed RTT implementation")
    require(
        rtt[0][0] == "1.3.1",
        "review the new RTT source before changing this guard",
    )
    enabled = set(rtt[0][1])
    require(
        "disable-blocking-mode" in enabled, "ordinary RTT writes must ignore probe blocking flags"
    )
    require("drop-on-contention" not in enabled, "IPSR-only ownership is unsafe across cores")


def check() -> None:
    metadata: object = json.loads(
        run_checked(
            [
                "cargo",
                "metadata",
                "--locked",
                "--offline",
                "--format-version",
                "1",
                "--all-features",
                "--filter-platform",
                "thumbv8m.main-none-eabihf",
            ],
            cwd=ROOT,
            timeout=120,
            capture_output=True,
            check=True,
        ).stdout
    )
    check_metadata(metadata)
    # Require explicit macro paths. Reject aliases/grouped imports/globs, which
    # could otherwise hide a flush call from this small source guard.
    check_app_sources()
    print(
        "RTT1.3.1 all-feature graph: nonblocking writes, multicore encoder; no app flush calls/imports"
    )


if __name__ == "__main__":
    check()
