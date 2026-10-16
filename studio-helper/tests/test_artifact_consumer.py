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
 def test_native_gate_is_explicit_and_never_experimental(self):
  with tempfile.TemporaryDirectory() as temporary:
   root=Path(temporary)
   for gate in (None,'experimental-full','practical_image',True):
    manifest={'schema':2}
    if gate is not None:manifest['image_gate']=gate
    (root/'manifest.json').write_text(json.dumps(manifest))
    with self.subTest(gate=gate),patch.object(artifact,'ElfImage') as elf:
     with self.assertRaisesRegex(AssertionError,'explicit practical'):artifact.validate(root,root,root/'missing.uf2')
     elf.assert_not_called()
 def test_practical_result_is_revalidated_and_bound_to_capture(self):
  from contextlib import ExitStack
  from types import SimpleNamespace
  eav=b'eav';profile=b'profile';asset=b'asset';elf_data=b'elf';uf2=b'uf2'
  options={'mode':'demo'};build={'normalized':options};attempt=json.dumps(build).encode()
  memory={'schema':3,'gate':'practical_image'};memory_raw=json.dumps(memory).encode()
  program={'linked_configuration':{'sha':'config'},'linked_startup':{'sha':'startup'}}
  manifest={'schema':2,'image_gate':'practical','elf_sha256':artifact.sha(elf_data),
   'dashboard_config':{'embedded_sha256':artifact.sha(eav)},'build_options':build,
   'build_options_sha256':artifact.sha(attempt),'source_context':{'sha256':artifact.sha(b'context')},
   'memory_layout':memory,'memory_layout_json_sha256':artifact.sha(memory_raw),'program_image':program}
  blobs={'dashboard.eav':eav,'build-options.json':attempt,'source-context.json':b'context',
   'memory-layout.json':memory_raw,'startup.easp':profile,'startup.easa':asset}
  with ExitStack() as stack:
   def mock(name,**kwargs):return stack.enter_context(patch.object(artifact,name,**kwargs))
   mock('record',side_effect=lambda path,limit:manifest if path.name=='manifest.json' else options)
   mock('bounded',side_effect=lambda path,limit:blobs[path.name])
   elf=SimpleNamespace(data=elf_data);context=object()
   mock('ElfImage',return_value=elf);mock('load_source_context',return_value=context)
   reader=stack.enter_context(patch.object(artifact.check_memory_layout,'read_selected_image_result',create=True,return_value=memory))
   mock('inspect_program',return_value=({},{}));mock('linked_configuration',return_value=program['linked_configuration'])
   mock('linked_startup',return_value=program['linked_startup']);mock('read_uf2',return_value=uf2);mock('verify_uf2',return_value={})
   report=artifact.validate(Path('capture'),Path('input'),Path('program.uf2'))
   reader.assert_called_once_with(memory,elf,eav,context,gate='practical')
   self.assertEqual(report['schema_version'],2);self.assertEqual(report['image_gate'],'practical')
   self.assertFalse(report['complete_instruction_proof']);self.assertFalse(report['hardware_qualified'])
   reader.return_value={'schema':3,'tampered':True}
   with self.assertRaisesRegex(AssertionError,'practical image evidence mismatch'):
    artifact.validate(Path('capture'),Path('input'),Path('program.uf2'))
if __name__=='__main__':unittest.main()
