"""Compare source-frozen captures on an operator-identified controlled runner."""

import argparse
import hashlib
import json
import math
from pathlib import Path, PurePosixPath
from statistics import median as sample_median
import zipfile
from collections.abc import Mapping
from benchmark_records import cargo_options, fields, positive, sample_values, strings

Measurement = tuple[float, float, float]
Capture = tuple[Mapping[str, object], Mapping[str, Measurement]]
ComparisonRow = tuple[str, float, float, float, str]

BENCH = "host/benches/dashboard_perf.rs"
RUNNERS = ("host/scripts/capture_benchmark.py", "host/scripts/benchmark_environment.py")
# Older captures predate the shared inventory/process helpers. Their presence/bytes still
# participate in identity, without making historical captures unreadable.
OPTIONAL_RUNNERS = (
    "host/scripts/source_inventory.py",
    "host/scripts/owned_process.py",
    "host/scripts/benchmark_records.py",
)
BUILD_INPUTS = ("host/Cargo.toml", "host/Cargo.lock", "host/rust-toolchain.toml")


def load(path: str | Path) -> Capture:
    path = Path(path)
    metadata = fields(json.loads((path / "metadata.json").read_text(encoding="utf-8")))
    with zipfile.ZipFile(path / "source.zip") as archive:
        if archive.testzip() is not None or len(archive.namelist()) != len(set(archive.namelist())):
            raise ValueError("invalid source archive")
        hashes = {
            name: hashlib.sha256(archive.read(name)).hexdigest() for name in archive.namelist()
        }
        if hashes != strings(metadata.get("sha256")) or BENCH not in hashes:
            raise ValueError("source provenance mismatch")
    records: dict[str, Measurement] = {}
    medians = fields(metadata.get("median_ns"))
    counts = fields(metadata.get("sample_counts"))
    if not medians or medians.keys() != counts.keys():
        raise ValueError("incomplete measurement index")
    for name, median in medians.items():
        relative = PurePosixPath(name)
        if relative.is_absolute() or ".." in relative.parts or "\\" in name or ":" in name:
            raise ValueError("invalid measurement path")
        try:
            raw: dict[str, object] = {
                filename: json.loads((path / name / filename).read_text(encoding="utf-8"))
                for filename in ("benchmark.json", "estimates.json", "sample.json", "tukey.json")
            }
        except (OSError, json.JSONDecodeError) as error:
            raise ValueError("incomplete raw measurement files") from error
        iters, times = sample_values(raw["sample.json"])
        count = counts[name]
        if type(count) is not int or len(times) != count or count < 30:
            raise ValueError("incomplete raw samples")
        estimate = fields(fields(raw["estimates.json"]).get("median"))
        point = positive(estimate.get("point_estimate"))
        # Criterion computes its median from each sample's time per iteration.
        derived = sample_median([positive(t / n) for t, n in zip(times, iters)])
        if not math.isclose(point, derived, rel_tol=1e-12, abs_tol=0):
            raise ValueError("median disagrees with raw samples")
        interval = fields(estimate.get("confidence_interval"))
        lo, hi = positive(interval.get("lower_bound")), positive(interval.get("upper_bound"))
        if (
            point != positive(median)
            or not lo <= point <= hi
            or interval.get("confidence_level") != 0.95
        ):
            raise ValueError("invalid median or confidence interval")
        records[name] = (point, lo, hi)
    return metadata, records


def compatible(before: Mapping[str, object], after: Mapping[str, object]) -> None:
    for metadata in [before, after]:
        if metadata.get("capture_policy") != 2:
            raise ValueError("timing comparison requires capture policy 2")
        runner_id = metadata.get("runner_id")
        if not isinstance(runner_id, str) or not runner_id.strip():
            raise ValueError("explicit controlled runner identity is required")
        if any(name not in strings(metadata.get("sha256")) for name in (*RUNNERS, *BUILD_INPUTS)):
            raise ValueError("capture runner or build sources are missing")
    for key in ["runner_id", "host", "processor", "rustc", "environment", "cargo_config"]:
        if key not in before or before[key] != after.get(key):
            raise ValueError("comparison identity mismatch: " + key)

    def workload(metadata: Mapping[str, object]) -> dict[str, str]:
        return {
            name: digest
            for name, digest in strings(metadata.get("sha256")).items()
            if name.startswith("host/benches/")
            or name in (*RUNNERS, *OPTIONAL_RUNNERS, *BUILD_INPUTS)
        }

    if workload(before) != workload(after):
        raise ValueError("fixture or capture source changed; review workload semantics separately")

    # Ignore Criterion's filter/run name, but require identical Cargo features.
    if cargo_options(before) != cargo_options(after):
        raise ValueError("Cargo build options differ")


def compare(before: Capture, after: Capture, budget: float) -> list[ComparisonRow]:
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
    parser.add_argument(
        "--gate",
        action="store_true",
        help="fail on either review or regression; use only on a quiet controlled runner",
    )
    args = parser.parse_args()
    rows = compare(load(args.baseline), load(args.candidate), args.budget_percent)
    print("| Fixture | Before ns | After ns | Change | Result |\n|---|---:|---:|---:|---|")
    for name, old, new, change, verdict in rows:
        print(f"| {name} | {old:.3f} | {new:.3f} | {change:+.2f}% | {verdict} |")
    if args.gate and any(row[-1] != "pass" for row in rows):
        raise SystemExit(1)
