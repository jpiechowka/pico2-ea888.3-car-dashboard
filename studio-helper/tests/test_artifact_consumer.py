"""Focused artifact consumer failures; no compiler/converter/hardware calls."""
import importlib.util,json,tempfile,unittest
from pathlib import Path
from unittest.mock import patch
ROOT=Path(__file__).resolve().parents[2]
spec=importlib.util.spec_from_file_location('artifact',ROOT/'firmware/scripts/validate_capture_artifact.py');artifact=importlib.util.module_from_spec(spec);spec.loader.exec_module(artifact)
class Consumer(unittest.TestCase):
 def test_duplicate_identity_and_bounded_read(self):
  with tempfile.TemporaryDirectory() as temporary:
   path=Path(temporary)/'manifest';path.write_text('{"schema":2,"schema":1}')
   with self.assertRaises(AssertionError):artifact.record(path,1024)
   path.write_bytes(bytes(33))
   with self.assertRaises(AssertionError):artifact.bounded(path,32)
 def test_incomplete_capture_never_reaches_elf(self):
  with tempfile.TemporaryDirectory() as temporary:
   root=Path(temporary);(root/'manifest.json').write_text('{"schema":1}')
   with patch.object(artifact,'ElfImage') as elf:
    with self.assertRaises(AssertionError):artifact.validate(root,root,root/'missing.uf2')
    elf.assert_not_called()
if __name__=='__main__':unittest.main()
