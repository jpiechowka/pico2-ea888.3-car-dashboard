"""Compare source-frozen captures on an operator-identified controlled runner."""
import argparse
import hashlib
import json
import math
from pathlib import Path, PurePosixPath
from statistics import median as sample_median
import zipfile

BENCH = "host/benches/dashboard_perf.rs"
RUNNERS = ("host/scripts/capture_benchmark.py", "host/scripts/benchmark_environment.py")
BUILD_INPUTS = ("host/Cargo.toml", "host/Cargo.lock", "host/rust-toolchain.toml")


def positive(value):
    if isinstance(value, bool) or not isinstance(value, (int, float)) or not math.isfinite(value) or value <= 0:
        raise ValueError("expected finite positive measurement")
    return value


def load(path):
    path = Path(path)
    metadata = json.loads((path / "metadata.json").read_text(encoding="utf-8"))
    with zipfile.ZipFile(path / "source.zip") as archive:
        if archive.testzip() is not None or len(archive.namelist()) != len(set(archive.namelist())):
            raise ValueError("invalid source archive")
        hashes = {name: hashlib.sha256(archive.read(name)).hexdigest() for name in archive.namelist()}
        if hashes != metadata["sha256"] or BENCH not in hashes:
            raise ValueError("source provenance mismatch")
    records = {}
    if not metadata["median_ns"] or metadata["median_ns"].keys() != metadata["sample_counts"].keys():
        raise ValueError("incomplete measurement index")
    for name, median in metadata["median_ns"].items():
        relative = PurePosixPath(name)
        if relative.is_absolute() or ".." in relative.parts or "\\" in name or ":" in name:
            raise ValueError("invalid measurement path")
        try:
            raw = {filename: json.loads((path / name / filename).read_text(encoding="utf-8"))
                   for filename in ("benchmark.json", "estimates.json", "sample.json", "tukey.json")}
        except (OSError, json.JSONDecodeError) as error:
            raise ValueError("incomplete raw measurement files") from error
        sample = raw["sample.json"]
        count = metadata["sample_counts"][name]
        if type(count) is not int or len(sample["iters"]) != len(sample["times"]) or len(sample["times"]) != count or count < 30:
            raise ValueError("incomplete raw samples")
        for value in sample["iters"] + sample["times"]:
            positive(value)
        estimate = raw["estimates.json"]["median"]
        point = positive(estimate["point_estimate"])
        # Criterion computes its median from each sample's time per iteration.
        derived = sample_median([positive(t / n) for t, n in zip(sample["times"], sample["iters"])])
        if not math.isclose(point, derived, rel_tol=1e-12, abs_tol=0):
            raise ValueError("median disagrees with raw samples")
        interval = estimate["confidence_interval"]
        lo, hi = positive(interval["lower_bound"]), positive(interval["upper_bound"])
        if point != positive(median) or not lo <= point <= hi or interval["confidence_level"] != 0.95:
            raise ValueError("invalid median or confidence interval")
        records[name] = (point, lo, hi)
    return metadata, records


def compatible(before, after):
    for metadata in [before, after]:
        if metadata.get("capture_policy") != 2:
            raise ValueError("timing comparison requires capture policy 2")
        if not isinstance(metadata.get("runner_id"), str) or not metadata["runner_id"].strip():
            raise ValueError("explicit controlled runner identity is required")
        if any(name not in metadata["sha256"] for name in (*RUNNERS, *BUILD_INPUTS)):
            raise ValueError("capture runner or build sources are missing")
    for key in ["runner_id", "host", "processor", "rustc", "environment", "cargo_config"]:
        if key not in before or before[key] != after.get(key):
            raise ValueError("comparison identity mismatch: " + key)
    def workload(metadata):
        return {name: digest for name, digest in metadata["sha256"].items()
                if name.startswith("host/benches/") or name in (*RUNNERS, *BUILD_INPUTS)}
    if workload(before) != workload(after):
        raise ValueError("fixture or capture source changed; review workload semantics separately")
    # Ignore Criterion's filter/run name, but require identical Cargo features.
    def cargo_options(metadata):
        command = metadata["command"]
        return command[:command.index("--")]
    if cargo_options(before) != cargo_options(after):
        raise ValueError("Cargo build options differ")


def compare(before, after, budget):
    positive(budget)
    compatible(before[0], after[0])
    if before[1].keys() != after[1].keys():
        raise ValueError("measurement coverage differs")
    rows = []
    for name, (old, _, old_hi) in sorted(before[1].items()):
        new, new_lo, _ = after[1][name]
        change = (new / old - 1) * 100
        verdict = "pass" if change <= budget else "review"
        if new_lo > old_hi * (1 + budget / 100):
            verdict = "regression"
        rows.append((name, old, new, change, verdict))
    return rows


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("baseline", type=Path)
    parser.add_argument("candidate", type=Path)
    parser.add_argument("--budget-percent", type=float, required=True)
    parser.add_argument("--gate", action="store_true", help="fail on either review or regression; use only on a quiet controlled runner")
    args = parser.parse_args()
    rows = compare(load(args.baseline), load(args.candidate), args.budget_percent)
    print("| Fixture | Before ns | After ns | Change | Result |\n|---|---:|---:|---:|---|")
    for name, old, new, change, verdict in rows:
        print(f"| {name} | {old:.3f} | {new:.3f} | {change:+.2f}% | {verdict} |")
    if args.gate and any(row[-1] != "pass" for row in rows):
        raise SystemExit(1)
