#!/usr/bin/env python3
"""Restore committed Criterion measurements to Cargo's ignored target tree."""

import argparse
import hashlib
from pathlib import Path
import shutil

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("name", help="baseline directory name, for example ae94c721")
args = parser.parse_args()

host = Path(__file__).resolve().parents[1]
source = host / "baselines" / args.name
target = host / "target" / "criterion"
if not source.is_dir() or args.name in (".", "..") or "/" in args.name or "\\" in args.name:
    parser.error("choose an existing simple baseline name")

restored = 0
for estimates in source.rglob("estimates.json"):
    measured = estimates.parent
    relative = measured.relative_to(source)
    destination = target / relative / args.name
    if destination.exists():
        for existing in measured.iterdir():
            copy = destination / existing.name
            if (
                not copy.is_file()
                or hashlib.sha256(existing.read_bytes()).digest()
                != hashlib.sha256(copy.read_bytes()).digest()
            ):
                parser.error(f"existing Criterion baseline differs: {copy}")
    else:
        shutil.copytree(measured, destination)
        restored += 1

print(f"Baseline {args.name}: {restored} measurement directories restored")
