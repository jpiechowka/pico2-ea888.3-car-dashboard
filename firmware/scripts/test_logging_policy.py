import unittest
from check_logging_policy import check_source


class LoggingPolicy(unittest.TestCase):
    def test_legal_rust_imports_cannot_hide_flush(self):
        for source in [
            "use {defmt as logger}; fn bad() { logger::flush(); }",
            "use ::defmt::{flush as drain}; fn bad() { drain(); }",
            "pub use {core::fmt, defmt as output};",
            "use defmt::*;", "use defmt::flush as drain;",
            "extern crate defmt as logger;", "::defmt :: flush();",
        ]:
            with self.subTest(source=source), self.assertRaises(AssertionError):
                check_source(source, "fixture")

    def test_macro_paths_and_other_flushes_remain_allowed(self):
        check_source('defmt::warn!("fault"); display.flush(); uart.flush();', "fixture")


if __name__ == "__main__":
    unittest.main()
