import copy
import unittest
from pathlib import Path
from unittest.mock import patch
from check_logging_policy import check_source, check_app_sources, check_metadata


class LoggingPolicy(unittest.TestCase):
    def test_reviewed_graph_and_feature_rejections(self):
        graph = {
            "packages": [{"id": "rtt", "name": "defmt-rtt", "version": "1.3.1"}],
            "resolve": {"nodes": [{"id": "rtt", "features": ["disable-blocking-mode"]}]},
        }
        check_metadata(graph)
        for version in ["1.3.0", "1.3.2"]:
            bad = copy.deepcopy(graph)
            bad["packages"][0]["version"] = version
            with self.assertRaises(AssertionError):
                check_metadata(bad)
        for features in [[], ["disable-blocking-mode", "drop-on-contention"]]:
            bad = copy.deepcopy(graph)
            bad["resolve"]["nodes"][0]["features"] = features
            with self.assertRaises(AssertionError):
                check_metadata(bad)
        for nodes in [[], graph["resolve"]["nodes"] * 2]:
            bad = copy.deepcopy(graph)
            bad["resolve"]["nodes"] = nodes
            with self.assertRaises(AssertionError):
                check_metadata(bad)

    def test_portable_sources_cannot_escape_policy(self):
        original = Path.read_text

        def malicious(path, *args, **kwargs):
            source = original(path, *args, **kwargs)
            if path.as_posix().endswith("dashboard-core/src/logging.rs"):
                source += "\nuse defmt::flush as concealed;"
            return source

        with patch.object(Path, "read_text", malicious), self.assertRaises(AssertionError):
            check_app_sources()

    def test_legal_rust_imports_cannot_hide_flush(self):
        for source in [
            "use {defmt as logger}; fn bad() { logger::flush(); }",
            "use ::defmt::{flush as drain}; fn bad() { drain(); }",
            "pub use {core::fmt, defmt as output};",
            "use defmt::*;",
            "use defmt::flush as drain;",
            "extern crate defmt as logger;",
            "::defmt :: flush();",
        ]:
            with self.subTest(source=source), self.assertRaises(AssertionError):
                check_source(source, "fixture")

    def test_standalone_probe_cannot_escape_policy(self):
        original = Path.read_text

        def malicious(path, *args, **kwargs):
            source = original(path, *args, **kwargs)
            if path.as_posix().endswith("ble-probe/src/tasks.rs"):
                source += "\nfn bad() { defmt::flush(); }"
            return source

        with patch.object(Path, "read_text", malicious), self.assertRaises(AssertionError):
            check_app_sources()

    def test_macro_paths_and_other_flushes_remain_allowed(self):
        check_source('defmt::warn!("fault"); display.flush(); uart.flush();', "fixture")


if __name__ == "__main__":
    unittest.main()
