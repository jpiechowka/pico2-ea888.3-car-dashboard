"""Deterministic regression tests for provenance and timing gate decisions."""

import copy
import hashlib
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
import zipfile

import benchmark_environment as environment
from compare_benchmarks import BENCH, RUNNERS, BUILD_INPUTS, compare, load

SOURCES = {name: b"fixture" for name in (BENCH, *RUNNERS, *BUILD_INPUTS)}


def metadata():
    return {
        "capture_policy": 2,
        "runner_id": "controlled-runner",
        "host": "test",
        "processor": "test",
        "rustc": "test",
        "environment": {},
        "cargo_config": {},
        "sha256": {name: hashlib.sha256(data).hexdigest() for name, data in SOURCES.items()},
        "command": ["cargo", "bench", "--locked", "--", "test"],
        "median_ns": {"fixture": 100},
        "sample_counts": {"fixture": 30},
    }


class Comparison(unittest.TestCase):
    def test_filter_cannot_inject_criterion_options(self):
        for value in ["--measurement-time=0.1", "--sample-size=10", "--", "-h"]:
            with self.assertRaises(ValueError):
                environment.validate_filter(value)
        for value in ["", "scene/|acquisition/", r"\-literal"]:
            environment.validate_filter(value)

    def test_all_workload_and_capture_helpers_are_part_of_identity(self):
        for name in [
            "host/benches/vehicle_port/mod.rs",
            "host/benches/data/fixture.bin",
            "host/scripts/source_inventory.py",
            "host/scripts/capture_benchmark.py",
            "host/scripts/benchmark_environment.py",
            *BUILD_INPUTS,
        ]:
            before = (metadata(), {"fixture": (100, 95, 105)})
            before[0]["sha256"][name] = "before"
            after = copy.deepcopy(before)
            after[0]["sha256"][name] = "after"
            with self.assertRaises(ValueError):
                compare(before, after, 20)
            del after[0]["sha256"][name]
            with self.assertRaises(ValueError):
                compare(before, after, 20)

    def test_gate_distinguishes_normal_noise_review_and_disjoint_regression(self):
        before = (metadata(), {"fixture": (100, 95, 105)})
        for value, low, want in [
            (90, 85, "pass"),
            (120, 115, "pass"),
            (121, 115, "review"),
            (140, 139, "regression"),
        ]:
            self.assertEqual(
                compare(before, (metadata(), {"fixture": (value, low, value + 5)}), 20)[0][-1], want
            )

    def test_changed_runner_compiler_config_features_or_fixture_cannot_be_gated(self):
        before = (metadata(), {"fixture": (100, 95, 105)})
        for key in [
            "runner_id",
            "host",
            "processor",
            "rustc",
            "environment",
            "cargo_config",
            "capture_policy",
            "sha256",
            "command",
        ]:
            after = copy.deepcopy(before)
            after[0][key] = {BENCH: "changed"} if key == "sha256" else "changed"
            with self.assertRaises(ValueError):
                compare(before, after, 20)
        for value in [None, "", "   "]:
            before[0]["runner_id"] = value
            with self.assertRaises(ValueError):
                compare(before, before, 20)
        before = (metadata(), {"fixture": (100, 95, 105)})
        with self.assertRaisesRegex(ValueError, "measurement coverage differs"):
            compare(before, (metadata(), {}), 20)
        for budget in [0, -1, float("nan"), float("inf")]:
            with self.assertRaises(ValueError):
                compare(before, before, budget)

    def test_raw_capture_integrity_and_counts_are_required(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory)
            record = path / "fixture"
            record.mkdir()
            m = metadata()
            (path / "metadata.json").write_text(json.dumps(m), encoding="utf-8")
            with zipfile.ZipFile(path / "source.zip", "w") as archive:
                for name, data in SOURCES.items():
                    archive.writestr(name, data)
            sample = {"iters": [1] * 30, "times": [100] * 30}
            (record / "sample.json").write_text(json.dumps(sample), encoding="utf-8")
            estimate = {
                "median": {
                    "point_estimate": 100,
                    "confidence_interval": {
                        "confidence_level": 0.95,
                        "lower_bound": 95,
                        "upper_bound": 105,
                    },
                }
            }
            (record / "estimates.json").write_text(json.dumps(estimate), encoding="utf-8")
            for filename in ["benchmark.json", "tukey.json"]:
                (record / filename).write_text("{}", encoding="utf-8")
            self.assertEqual(load(path)[1]["fixture"], (100, 95, 105))
            for filename in ["benchmark.json", "tukey.json"]:
                (record / filename).unlink()
                with self.assertRaises(ValueError):
                    load(path)
                (record / filename).write_text("{}", encoding="utf-8")
            forged = {"iters": [1] * 30, "times": [10000] * 30}
            (record / "sample.json").write_text(json.dumps(forged), encoding="utf-8")
            with self.assertRaises(ValueError):
                load(path)
            for value in [0, -1, float("nan"), float("inf")]:
                bad = copy.deepcopy(sample)
                bad["times"][0] = value
                (record / "sample.json").write_text(json.dumps(bad), encoding="utf-8")
                with self.assertRaises(ValueError):
                    load(path)
            (record / "sample.json").write_text(json.dumps(sample), encoding="utf-8")
            for key, value in [
                ("sample_counts", {"fixture": 31}),
                ("sha256", {BENCH: "bad"}),
                ("median_ns", {"fixture": 99}),
            ]:
                bad = copy.deepcopy(m)
                bad[key] = value
                (path / "metadata.json").write_text(json.dumps(bad), encoding="utf-8")
                with self.assertRaises(ValueError):
                    load(path)

    def test_capture_rejects_hidden_compiler_overrides_without_logging_values(self):
        for key in [
            "RUSTC",
            "RUSTC_WRAPPER",
            "RUSTC_WORKSPACE_WRAPPER",
            "CARGO_BUILD_RUSTFLAGS",
            "CARGO_PROFILE_BENCH_OPT_LEVEL",
            "CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_RUNNER",
            "CARGO_ENCODED_RUSTFLAGS",
            "CARGO_INCREMENTAL",
        ]:
            with self.assertRaises(ValueError) as error:
                environment.check_environment({key: "private"})
            self.assertNotIn("private", str(error.exception))
        environment.check_environment({"RUSTUP_TOOLCHAIN": "nightly", "RUSTC_WRAPPER": ""})
        for returncode, stderr in [(0, ""), (1, "unexpected error")]:
            with patch.object(environment, "run_checked") as run:
                run.return_value.returncode = returncode
                run.return_value.stderr = stderr
                with self.assertRaises(ValueError):
                    environment.configuration(Path("."))


if __name__ == "__main__":
    unittest.main()
