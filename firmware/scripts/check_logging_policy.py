"""Fail closed on additive RTT features and operational flush imports/calls.

This is a source policy guard, not a proof about arbitrary future dependencies.
The locked defmt macros and panic-probe source were separately reviewed.
"""
import json
from validation import require
from pathlib import Path
import re
import subprocess

ROOT = Path(__file__).resolve().parents[1]


def check_source(source, label):
    require(not re.search(r"\bdefmt\s*::\s*flush\b", source), f"blocking RTT flush in {label}")
    # Conservatively reject the entire use item, including grouped imports,
    # leading ::, re-exports and nested aliases. False positives in comments
    # are intentional; this is a policy scanner, not a complete Rust parser.
    require(not re.search(r"\buse\b[^;]*\bdefmt\b[^;]*;", source), f"use explicit defmt macro paths in {label}")
    require(not re.search(r"\bextern\s+crate\s+defmt\b", source), f"defmt alias in {label}")


def check_app_sources():
    roots = [ROOT / "pico2" / "src"]
    roots.extend(p / "src" for p in (ROOT.parent / "crates").iterdir()
                 if p.is_dir() and (p / "Cargo.toml").is_file())
    for root in roots:
        for path in root.rglob("*.rs"):
            check_source(path.read_text(encoding="utf-8"), path)


def check_metadata(metadata):
    packages = {p["id"]: p for p in metadata["packages"]}
    rtt = [n for n in metadata["resolve"]["nodes"] if packages[n["id"]]["name"] == "defmt-rtt"]
    require(len(rtt) == 1, "expected one reviewed RTT implementation")
    require(packages[rtt[0]["id"]]["version"] == "1.3.1", "review the new RTT source before changing this guard")
    features = set(rtt[0]["features"])
    require("disable-blocking-mode" in features, "ordinary RTT writes must ignore probe blocking flags")
    require("drop-on-contention" not in features, "IPSR-only ownership is unsafe across cores")


def check():
    metadata = json.loads(subprocess.check_output(
        ["cargo", "metadata", "--locked", "--offline", "--format-version", "1",
         "--all-features", "--filter-platform", "thumbv8m.main-none-eabihf"],
        cwd=ROOT, text=True, encoding="utf-8",
    ))
    check_metadata(metadata)
    # Require explicit macro paths. Reject aliases/grouped imports/globs, which
    # could otherwise hide a flush call from this small source guard.
    check_app_sources()
    print("RTT1.3.1 all-feature graph: nonblocking writes, multicore encoder; no app flush calls/imports")


if __name__ == "__main__":
    check()
