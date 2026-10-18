"""Focused startup capture identity tests; no firmware compile or USB access."""

import importlib.util
import json
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location(
    "capture", ROOT / "firmware/scripts/capture_target_bench.py"
)
capture = importlib.util.module_from_spec(spec)
spec.loader.exec_module(capture)


class StartupCapture(unittest.TestCase):
    def test_exact_snapshot_and_generated_identity(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            source = root / "profile"
            source.write_bytes(bytes(16))
            destination = root / "capture"
            destination.mkdir()
            environment = {}
            record = capture.freeze_startup(source, None, destination, environment)
            build = root / "build"
            build.mkdir()
            out = build / "out"
            out.mkdir()
            for name in record:
                (out / name).write_bytes((destination / name).read_bytes())
            row = {
                "reason": "build-script-executed",
                "out_dir": str(out),
                "env": [
                    ["DASHBOARD_STARTUP_SHA256", record["startup_profile.bin"]["sha256"]],
                    ["DASHBOARD_STARTUP_ASSET_SHA256", record["startup_asset.bin"]["sha256"]],
                ],
            }
            capture.verify_startup(json.dumps(row), build, destination, record)
            (out / "startup_profile.bin").write_bytes(b"changed")
            with self.assertRaises(ValueError):
                capture.verify_startup(json.dumps(row), build, destination, record)

    def test_size_bound_and_ambient_rejected(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            profile = root / "profile"
            profile.write_bytes(bytes(17))
            with self.assertRaises(ValueError):
                capture.freeze_startup(profile, None, root, {})
            with self.assertRaises(ValueError):
                capture.freeze_startup(None, None, root, {"DASHBOARD_STARTUP_PROFILE": "ambient"})


if __name__ == "__main__":
    unittest.main()
