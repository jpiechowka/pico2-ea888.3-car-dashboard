"""Run Criterion with immutable source evidence and archive raw results."""
import argparse
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import shutil
import subprocess
import zipfile
from benchmark_environment import compiler, configuration, validate_filter
from source_inventory import shared_inputs

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("baseline")
parser.add_argument("--runner-id", required=True, help="stable operator-assigned ID for this physical controlled runner; do not reuse across machines")
parser.add_argument("--filter", default="")
parser.add_argument("--features")
parser.add_argument("--no-default-features", action="store_true")
args = parser.parse_args()
validate_filter(args.filter)
if not re.fullmatch(r"[A-Za-z0-9][A-Za-z0-9_-]*", args.runner_id):
    parser.error("runner ID must contain only letters, digits, _ or - and start with a letter/digit")
host = Path(__file__).resolve().parents[1]
repo = host.parent
if not re.fullmatch(r"[A-Za-z0-9][A-Za-z0-9_-]*", args.baseline):
    parser.error("baseline must start with a letter/digit and contain only letters, digits, _ or -")
destination = host / "baselines" / args.baseline
criterion = host / "target" / "criterion"
if destination.exists() or any(criterion.rglob(f"{args.baseline}/sample.json")):
    parser.error("refusing to reuse an existing run name or raw results")
if os.environ.get("CARGO_TARGET_DIR"):
    parser.error("unset CARGO_TARGET_DIR so artifact paths are unambiguous")
rustc_path, rustc_version = compiler(host)
cargo_config = configuration(host)
run_env = os.environ.copy()
# Make Cargo use precisely the compiler whose identity is recorded, after
# rejecting all environment/config wrapper and optimization overrides.
run_env["RUSTC"] = rustc_path

def inputs():
    paths = shared_inputs(repo)
    for tree in [host / "src", host / "benches", repo / "firmware" / "pico2" / "src",
                 repo / "firmware" / "vendor" / "embassy-rp" / "src"]:
        paths.update(tree.rglob("*.rs"))
    for root in [host, repo / "firmware", repo / "firmware" / "pico2", repo / "firmware" / "vendor" / "embassy-rp"]:
        for name in ["Cargo.toml", "Cargo.lock", "rust-toolchain.toml", "build.rs", ".cargo/config.toml", "memory.x"]:
            path = root / name
            if path.is_file():
                paths.add(path)
    paths.add(Path(__file__).resolve())
    paths.add(Path(__file__).with_name("benchmark_environment.py"))
    paths.update(p for p in (host / "benches").rglob("*") if p.is_file())
    return {p.relative_to(repo).as_posix(): p.read_bytes() for p in sorted(paths)}

before = inputs()
hashes = {key: hashlib.sha256(data).hexdigest() for key, data in before.items()}
command = ["cargo", "bench", "--locked", "--bench", "dashboard_perf"]
if args.features:
    command += ["--features", args.features]
if args.no_default_features:
    command += ["--no-default-features"]
command += ["--", args.filter, "--save-baseline", args.baseline]
metadata = {
    "head": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=repo, text=True).strip(),
    "host": platform.platform(),
    "processor": os.environ.get("PROCESSOR_IDENTIFIER", platform.processor()),
    "runner_id": args.runner_id,
    "rustc": rustc_version,
    "rustc_path": rustc_path,
    "cargo_config": cargo_config,
    "capture_policy": 2,
    "command": command,
    "environment": {key: value for key, value in os.environ.items() if key.startswith(("RUSTFLAGS", "RUSTUP_TOOLCHAIN", "CARGO_PROFILE_BENCH_", "CARGO_ENCODED_RUSTFLAGS"))},
    "hardware_validation": False,
    "provenance": "Pre-run source archive, hashes checked unchanged after execution",
    "started_utc": datetime.now(timezone.utc).isoformat(),
    "sha256": hashes,
}
destination.mkdir(parents=True)
with zipfile.ZipFile(destination / "source.zip", "w", zipfile.ZIP_DEFLATED) as archive:
    for key, data in before.items():
        archive.writestr(key, data)
(destination / "run-inputs.json").write_text(json.dumps(metadata, indent=2) + "\n")
subprocess.run(command, cwd=host, check=True, env=run_env)
after = inputs()
if before != after or configuration(host) != cargo_config or compiler(host) != (rustc_path, rustc_version):
    raise RuntimeError("source changed during execution; run rejected; inputs and raw output retained")
records = sorted(criterion.rglob(f"{args.baseline}/sample.json"))
if not records:
    raise RuntimeError("no raw samples produced")
medians = {}
counts = {}
for sample in records:
    source = sample.parent
    key = source.parent.relative_to(criterion)
    data = json.loads(sample.read_text())
    if len(data["iters"]) != len(data["times"]) or not data["iters"]:
        raise ValueError(f"invalid samples: {sample}")
    target = destination / key
    target.mkdir(parents=True)
    for name in ("benchmark.json", "estimates.json", "sample.json", "tukey.json"):
        shutil.copyfile(source / name, target / name)
    medians[key.as_posix()] = json.loads((source / "estimates.json").read_text())["median"]["point_estimate"]
    counts[key.as_posix()] = len(data["iters"])
metadata.update(median_ns=medians, sample_counts=counts, finished_utc=datetime.now(timezone.utc).isoformat())
(destination / "metadata.json").write_text(json.dumps(metadata, indent=2) + "\n")
print(json.dumps(medians, indent=2))
