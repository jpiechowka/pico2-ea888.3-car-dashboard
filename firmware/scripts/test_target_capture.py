"""Source completeness and emitted-artifact contracts; no target execution."""
import json
from pathlib import Path
import unittest
import tempfile
from unittest.mock import patch
import capture_target_bench as capture


class CaptureTests(unittest.TestCase):
    def test_only_matching_build_snapshot_is_archived(self):
        with tempfile.TemporaryDirectory() as folder:
            build = Path(folder) / 'build'
            out = build / 'out'
            out.mkdir(parents=True)
            data = b'validated configuration snapshot'
            (out / 'dashboard.eav').write_bytes(data)
            record = dict(reason='build-script-executed', out_dir=str(out),
                          env=[['DASHBOARD_CONFIG_SHA256', capture.hashlib.sha256(data).hexdigest()]])
            line = json.dumps(record)
            self.assertEqual(capture.embedded_config(line, build), data)
            for output in ['', line + '\n' + line,
                           json.dumps(dict(record, env=[['DASHBOARD_CONFIG_SHA256', 'bad']]))]:
                with self.subTest(output=output), self.assertRaises(ValueError):
                    capture.embedded_config(output, build)
            with self.assertRaises(ValueError):
                capture.embedded_config(line, build / 'different')
            (out / 'dashboard.eav').write_bytes(b'replaced after build')
            with self.assertRaises(ValueError):
                capture.embedded_config(line, build)

    def test_external_configuration_is_bounded_frozen_and_captured_by_content(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            selected = root / 'external.eav'
            selected.write_bytes(b'first snapshot')
            environment = {'DASHBOARD_CONFIG': str(selected)}
            before = capture.selected_config(environment)
            self.assertEqual(before[1], b'first snapshot')
            out = root / 'capture'
            out.mkdir()
            frozen, metadata = capture.freeze_config(before, out, environment)
            self.assertEqual(Path(frozen['DASHBOARD_CONFIG']).read_bytes(), b'first snapshot')
            self.assertEqual(metadata['sha256'], capture.hashlib.sha256(b'first snapshot').hexdigest())
            selected.write_bytes(b'changed snapshot')
            self.assertNotEqual(before, capture.selected_config(environment))
            self.assertEqual(Path(frozen['DASHBOARD_CONFIG']).read_bytes(), b'first snapshot')
            self.assertEqual(capture.selected_config({}), (None, None))
            default_env, default = capture.freeze_config((None, None), out, environment)
            self.assertNotIn('DASHBOARD_CONFIG', default_env)
            self.assertEqual(default['selection'], 'default')
            for value in ['', str(root), str(root / 'missing'), 'bad\npath']:
                with self.subTest(value=value), self.assertRaises((ValueError, OSError)):
                    capture.selected_config({'DASHBOARD_CONFIG': value})
            selected.write_bytes(bytes(16385))
            with self.assertRaises(ValueError):
                capture.selected_config(environment)

    def test_archive_contains_portable_crates_and_workspace_inputs(self):
        before = capture.inputs()
        keys = ['Cargo.toml', 'Cargo.lock', 'rust-toolchain.toml',
                'firmware/scripts/Build-Dashboard.ps1',
                'crates/dashboard-core/Cargo.toml', 'crates/dashboard-core/src/lib.rs',
                'crates/dashboard-core/src/display/pipeline.rs']
        for key in keys:
            self.assertTrue(key in before, f'missing shared input: {key}')
            self.assertEqual(before[key], (capture.REPO / key).read_bytes())
        original = Path.read_bytes
        with patch.object(Path, 'read_bytes', lambda p: original(p) + (b'changed' if p.name == 'pipeline.rs' else b'')):
            self.assertNotEqual(before, capture.inputs(), 'portable source escaped archive')

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
