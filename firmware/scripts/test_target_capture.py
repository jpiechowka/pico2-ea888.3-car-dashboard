"""Source completeness and emitted-artifact contracts; no target execution."""
import json
from pathlib import Path
import unittest
from unittest.mock import patch
import capture_target_bench as capture


class CaptureTests(unittest.TestCase):
    def test_archive_contains_crate_root_compile_inputs(self):
        data = capture.inputs()
        for name in ('README.md', 'link-rp.x.in', 'build.rs', 'Cargo.toml'):
            key = 'firmware/vendor/embassy-rp/' + name
            self.assertEqual(data[key], (capture.REPO / key).read_bytes())
        for name in ('memory.x', 'rp235x_riscv.x', 'build.rs'):
            self.assertIn('firmware/pico2/' + name, data)

    def test_source_inventory_notices_root_compile_input_changes(self):
        original = Path.read_bytes
        before = capture.inputs()
        with patch.object(Path, 'read_bytes', lambda p: original(p) + (b'changed' if p.name == 'README.md' else b'')):
            self.assertTrue(before != capture.inputs(), 'changed README escaped source inventory')

    def test_only_cargo_emitted_firmware_executable_is_selected(self):
        executable = (capture.FIRMWARE / 'target/alternative/pico2').resolve()
        artifact = dict(reason='compiler-artifact', manifest_path=str(capture.FIRMWARE / 'pico2/Cargo.toml'),
                        target=dict(name='pico2', kind=['bin']), profile=dict(test=False), executable=str(executable))
        unrelated = dict(artifact, manifest_path=str(capture.FIRMWARE / 'other/Cargo.toml'))
        def stream(*records):
            return '\n'.join(json.dumps(record) for record in records)
        finish = dict(reason='build-finished', success=True)
        self.assertEqual(capture.emitted_elf(stream(unrelated, artifact, finish)), executable)
        for records in ((finish,), (artifact,), (artifact, dict(finish, success=False)),
                        (artifact, artifact, finish), (artifact, finish, finish),
                        (dict(artifact, executable=None), finish),
                        (dict(artifact, executable='relative/pico2'), finish),
                        (dict(artifact, profile=dict(test=True)), finish)):
            with self.subTest(records=records), self.assertRaises(ValueError):
                capture.emitted_elf(stream(*records))
        with self.assertRaises(ValueError):
            capture.emitted_elf('invalid JSON')


if __name__ == '__main__':
    unittest.main()
