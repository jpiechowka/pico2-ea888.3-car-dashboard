"""Test discovery must never start the opt-in live helper harness."""
import importlib.util
from pathlib import Path
import unittest
from unittest.mock import patch

class HarnessImport(unittest.TestCase):
    def test_import_has_no_process_or_network_side_effect(self):
        spec = importlib.util.spec_from_file_location('http_harness', Path(__file__).with_name('test_server_http.py'))
        module = importlib.util.module_from_spec(spec)
        with patch('subprocess.Popen') as process, patch('urllib.request.urlopen') as network:
            spec.loader.exec_module(module)
            process.assert_not_called()
            network.assert_not_called()
        self.assertTrue(callable(module.main))

if __name__ == '__main__':
    unittest.main()
