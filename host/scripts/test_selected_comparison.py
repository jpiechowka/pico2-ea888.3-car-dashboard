"""The historical selected-fixture review must retain its guards under -O."""

import hashlib
import json
from pathlib import Path
import sys
import tempfile
import unittest
import zipfile

from owned_process import run_checked
from test_benchmark_comparison import SOURCES, metadata


HELPER = (
    Path(__file__).resolve().parents[2]
    / "docs/performance/orientation-landscape-2026-10-17/compare-selected.py"
)


def capture(path: Path, *, changed: bool, runner: str, supervisor: bytes = b"fixture") -> None:
    path.mkdir()
    sources = dict(SOURCES)
    sources["host/scripts/source_inventory.py"] = b"fixture"
    sources["host/scripts/owned_process.py"] = supervisor
    sources["host/benches/view_perf.rs"] = (
        b"fn view() {}\n\nfn orientation(c: &mut Criterion) {\n}\n"
        if changed
        else b"fn view() {}\n"
    )
    for name in ("config_perf.rs", "rule_perf.rs"):
        sources["host/benches/" + name] = (
            b"struct C {}\n                orientation: None,\n" if changed else b"struct C {}\n"
        )
    record = metadata()
    record["runner_id"] = runner
    serialized: dict[str, object] = dict(record)
    serialized["rustc_path"] = "fixture-rustc"
    serialized["sha256"] = {
        name: hashlib.sha256(data).hexdigest() for name, data in sources.items()
    }
    (path / "metadata.json").write_text(json.dumps(serialized), encoding="utf-8")
    with zipfile.ZipFile(path / "source.zip", "w") as archive:
        for name, data in sources.items():
            archive.writestr(name, data)
    fixture = path / "fixture"
    fixture.mkdir()
    for name, value in {
        "benchmark.json": {},
        "tukey.json": {},
        "sample.json": {"iters": [1] * 30, "times": [100] * 30},
        "estimates.json": {
            "median": {
                "point_estimate": 100,
                "confidence_interval": {
                    "confidence_level": 0.95,
                    "lower_bound": 95,
                    "upper_bound": 105,
                },
            }
        },
    }.items():
        (fixture / name).write_text(json.dumps(value), encoding="utf-8")


class SelectedComparison(unittest.TestCase):
    def test_runner_identity_guard_survives_optimized_python(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            before, after = Path(directory) / "before", Path(directory) / "after"
            capture(before, changed=False, runner="runner-a")
            capture(after, changed=True, runner="runner-b")
            for flags in ([], ["-O"]):
                with self.subTest(flags=flags):
                    result = run_checked(
                        [sys.executable, *flags, str(HELPER), str(before), str(after)],
                        capture_output=True,
                        timeout=15,
                    )
                    self.assertNotEqual(result.returncode, 0)
                    self.assertNotIn('"rows":', result.stdout)

    def test_supervisor_changes_are_outside_selected_fixture_exception(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            before, after = Path(directory) / "before", Path(directory) / "after"
            capture(before, changed=False, runner="same")
            capture(after, changed=True, runner="same", supervisor=b"changed")
            for flags in ([], ["-O"]):
                with self.subTest(flags=flags):
                    result = run_checked(
                        [sys.executable, *flags, str(HELPER), str(before), str(after)],
                        capture_output=True,
                        timeout=15,
                    )
                    self.assertNotEqual(result.returncode, 0)
                    self.assertNotIn('"rows":', result.stdout)

    def test_reviewed_fixture_migration_still_compares(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            before, after = Path(directory) / "before", Path(directory) / "after"
            capture(before, changed=False, runner="same")
            capture(after, changed=True, runner="same")
            for flags in ([], ["-O"]):
                result = run_checked(
                    [sys.executable, *flags, str(HELPER), str(before), str(after)],
                    capture_output=True,
                    timeout=15,
                    check=True,
                )
                self.assertIn('"rows":', result.stdout)


if __name__ == "__main__":
    unittest.main()
