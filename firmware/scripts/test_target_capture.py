"""Source completeness and emitted-artifact contracts; no target execution."""
import json
from pathlib import Path
import unittest
import tempfile
import os
import subprocess
import sys
import io
from contextlib import ExitStack,redirect_stdout
from unittest.mock import patch
import capture_target_bench as capture


class CaptureTests(unittest.TestCase):
    def test_custom_environment_presence_is_rejected_even_when_empty(self):
        for key in ['DASHBOARD_CPU_HZ','DASHBOARD_SPI_HZ','DASHBOARD_VSEL_MV']:
            with self.assertRaises(ValueError):
                capture.check_clock_environment({key:''})

    def test_every_ambient_compiler_override_is_rejected_by_presence(self):
        from benchmark_environment import ENV_PREFIXES
        for key in [*ENV_PREFIXES, 'RUSTFLAGS','CARGO_ENCODED_RUSTFLAGS','CARGO_PROFILE_RELEASE_OPT_LEVEL']:
            for value in ['', 'override']:
                with self.subTest(key=key,value=value), self.assertRaises(ValueError):
                    capture.check_clock_environment({key:value})
        capture.check_clock_environment({'PATH':'ordinary PATH','DASHBOARD_CONFIG':'selected.eav'})

    def test_failed_build_keeps_raw_stdout_before_raising(self):
        with tempfile.TemporaryDirectory() as folder:
            destination=Path(folder)
            command=[sys.executable,'-c','import sys; print("{\\"reason\\":\\"build-finished\\",\\"success\\":false}"); sys.exit(7)']
            with self.assertRaises(subprocess.CalledProcessError) as error:
                capture.run_build(command,os.environ.copy(),destination)
            self.assertEqual(error.exception.returncode,7)
            self.assertEqual(json.loads((destination/'cargo-messages.jsonl').read_text()),{'reason':'build-finished','success':False})

    def test_generated_clock_is_bound_to_firmware_package_and_exact_attempt(self):
        with tempfile.TemporaryDirectory() as folder:
            build=Path(folder)/'build';out=build/'out';out.mkdir(parents=True)
            source=b'''// Generated from dashboard-build-profiles; do not edit.
pub const REQUESTED_CPU_HZ: u32 = 150000000;
pub const ACTUAL_CPU_HZ: u32 = 150000000;
pub const REQUESTED_SPI_HZ: u32 = 62500000;
pub const ACTUAL_SPI_HZ: u32 = 37500000;
pub const VOLTAGE_MV: u32 = 1100;
pub const CORE_VOLTAGE: CoreVoltage = CoreVoltage::V1_10;
pub const PLL: PllConfig = PllConfig { refdiv: 1, fbdiv: 125, post_div1: 5, post_div2: 2 };
pub const CPSDVSR: u8 = 2;
pub const SCR: u8 = 1;
pub const LEGACY_ROUNDING: bool = false;
'''
            clock=dict(requested_cpu_hz=150000000,actual_cpu_hz=150000000,requested_spi_hz=62500000,actual_spi_hz=37500000,
                       voltage='v1_10',pll=dict(refdiv=1,fbdiv=125,postdiv1=5,postdiv2=2),cpsdvsr=2,scr=1,legacy_rounding=False,
                       spi_neighbors=dict(below_or_equal=dict(request_hz=37500000,actual_hz=37500000,cpsdvsr=2,scr=1),above=dict(request_hz=75000000,actual_hz=75000000,cpsdvsr=2,scr=0)))
            options=dict(normalized=dict(requested=dict(clock={'named':'stock'}),clock=clock),features=['demo'],optimization='3',voltage_mv=1100)
            metadata=dict(schema_version=1,selection={'named':'stock'},clock=clock,optimization='3',features=['demo'])
            (out/'clock_profile.rs').write_bytes(source)
            encoded=json.dumps(metadata).encode();(out/'clock_profile.json').write_bytes(encoded)
            record=dict(reason='build-script-executed',package_id='firmware-id',out_dir=str(out),env=[['DASHBOARD_CLOCK_SOURCE_SHA256',capture.hashlib.sha256(source).hexdigest()],['DASHBOARD_CLOCK_METADATA_SHA256',capture.hashlib.sha256(encoded).hexdigest()]])
            line=json.dumps(record)
            evidence=capture.generated_clock(line,build,'firmware-id',options)
            self.assertEqual(evidence['source'],source)
            for stream, package, attempt in [('', 'firmware-id', options),(line+'\n'+line,'firmware-id',options),(line,'other',options),(line,'firmware-id',dict(options,optimization='s'))]:
                with self.assertRaises(ValueError): capture.generated_clock(stream,build,package,attempt)
            with self.assertRaises(ValueError): capture.generated_clock(line,build/'outside','firmware-id',options)
            (out/'clock_profile.rs').write_bytes(b'mismatched')
            with self.assertRaises(ValueError): capture.generated_clock(line,build,'firmware-id',options)
            wrong=source.replace(b'fbdiv: 125',b'fbdiv: 124')
            (out/'clock_profile.rs').write_bytes(wrong)
            record['env'][0][1]=capture.hashlib.sha256(wrong).hexdigest()
            with self.assertRaises(ValueError): capture.generated_clock(json.dumps(record),build,'firmware-id',options)

    def test_new_clock_and_protocol_producers_are_capture_inputs(self):
        data=capture.inputs()
        for path in ['Cargo.lock','firmware/Cargo.lock','firmware/pico2/build.rs','firmware/pico2/src/clock_profile.rs','crates/dashboard-build-profiles/src/resolve.rs','crates/dashboard-helper-protocol/src/options.rs','crates/dashboard-config-tools/src/clocks.rs']:
            self.assertEqual(data[path],(capture.REPO/path).read_bytes())
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

    def test_migration_compares_canonical_bytes_and_keeps_original_identity(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            original, upgraded = b'original v1', b'canonical v2'
            (root / 'selected.eav').write_bytes(original)
            record = dict(selection='explicit', sha256=capture.hashlib.sha256(original).hexdigest())
            def compile_stub(*args, **kwargs):
                (root / 'normalized.eav').write_bytes(upgraded)
            with patch.object(capture.subprocess, 'run', side_effect=compile_stub):
                capture.verify_selected_config(root, {}, upgraded, record)
                self.assertEqual(record['sha256'],capture.hashlib.sha256(original).hexdigest())
                self.assertEqual(record['normalized_sha256'],capture.hashlib.sha256(upgraded).hexdigest())
                with self.assertRaises(ValueError):
                    capture.verify_selected_config(root, {}, original, record)
                (root / 'selected.eav').write_bytes(b'changed')
                with self.assertRaises(ValueError):
                    capture.verify_selected_config(root, {}, upgraded, record)
            capture.verify_selected_config(root, {}, upgraded, dict(selection='default'))

    def test_archive_contains_portable_crates_and_workspace_inputs(self):
        before = capture.inputs()
        keys = ['Cargo.toml', 'Cargo.lock', 'rust-toolchain.toml',
                'firmware/scripts/Build-Dashboard.ps1',
                'crates/dashboard-core/Cargo.toml', 'crates/dashboard-core/src/lib.rs',
                'crates/dashboard-core/src/display/pipeline.rs',
                'crates/dashboard-config/src/cache_identity.rs',
                'crates/dashboard-core/src/cache_diagnostics.rs',
                'firmware/pico2/src/cache_reporting.rs']
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

    def test_complete_advanced_runtime_and_seeded_decoder_are_capture_inputs(self):
        data = capture.inputs()
        for crate, names in [
            ('dashboard-core', ('acquisition.rs', 'presentation.rs', 'peak_hold.rs',
                                'warning_state.rs', 'status_led.rs', 'configured_scene.rs')),
            ('dashboard-config', ('advanced.rs', 'decode_in_place.rs', 'slot.rs', 'legacy_v3.rs')),
        ]:
            for name in names:
                key = f'crates/{crate}/src/{name}'
                self.assertEqual(data[key], (capture.REPO / key).read_bytes())

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


class CaptureMemoryQualification(unittest.TestCase):
    def setUp(self):
        self.stdout=io.StringIO();self.enterContext(redirect_stdout(self.stdout))

    def fixture(self):
        from test_validation_policy import MemoryQualification
        import check_memory_layout
        elf,eav,stack=MemoryQualification.fixture(self)
        return elf,eav,check_memory_layout._qualified_image(stack,elf,512,0x7fe00)

    def test_actual_child_result_is_retained_and_rechecked_against_current_input(self):
        elf,eav,record=self.fixture()
        with tempfile.TemporaryDirectory() as folder:
            destination=Path(folder);frozen=destination/'pico2.elf';frozen.write_bytes(elf.data)
            def child(argv,**kwargs):
                self.assertEqual(argv[-2],'--result-json');self.assertTrue(kwargs['check'])
                Path(argv[-1]).write_text(json.dumps(record));return subprocess.CompletedProcess(argv,0)
            with patch.object(capture,'ElfImage',return_value=elf),patch.object(capture.subprocess,'run',side_effect=child) as run:
                evidence=capture.run_memory_gate(frozen,destination,eav)
                self.assertEqual(evidence['record'],record)
                self.assertEqual(capture.read_memory_gate(destination/'memory-layout.json',frozen,eav),evidence)
                self.assertEqual(run.call_count,1)
                with self.assertRaises(AssertionError):capture.read_memory_gate(destination/'memory-layout.json',frozen,eav+b'changed')
                with self.assertRaises(ValueError):capture.run_memory_gate(frozen,destination,eav)

    def test_nonzero_missing_and_intermediate_child_output_never_admit(self):
        elf,eav,record=self.fixture()
        for kind in ('nonzero','missing','intermediate','lost','stale'):
            with self.subTest(kind=kind),tempfile.TemporaryDirectory() as folder:
                destination=Path(folder);frozen=destination/'pico2.elf';frozen.write_bytes(elf.data)
                def child(argv,**kwargs):
                    value=record['stack'] if kind=='intermediate' else dict(record)
                    if kind=='lost':value['status']='IMAGE_PASS'
                    if kind=='stale':value['elf_sha256']='0'*64
                    if kind!='missing':Path(argv[-1]).write_text(json.dumps(value))
                    if kind=='nonzero':raise subprocess.CalledProcessError(7,argv)
                    return subprocess.CompletedProcess(argv,0)
                with patch.object(capture,'ElfImage',return_value=elf),patch.object(capture.subprocess,'run',side_effect=child),\
                     self.assertRaises((AssertionError,ValueError,FileNotFoundError,subprocess.CalledProcessError)):
                    capture.run_memory_gate(frozen,destination,eav)

    def test_real_nonzero_child_preserves_file_without_admitting_it(self):
        elf,eav,record=self.fixture();real_run=subprocess.run
        with tempfile.TemporaryDirectory() as folder:
            destination=Path(folder);frozen=destination/'pico2.elf';frozen.write_bytes(elf.data)
            def child(argv,**kwargs):
                return real_run([sys.executable,'-c',
                    'import pathlib,sys;pathlib.Path(sys.argv[1]).write_text(sys.argv[2]);sys.exit(7)',
                    argv[-1],json.dumps(record)],check=kwargs['check'],timeout=10)
            with patch.object(capture.subprocess,'run',side_effect=child),self.assertRaises(subprocess.CalledProcessError) as error:
                capture.run_memory_gate(frozen,destination,eav)
            self.assertEqual(error.exception.returncode,7)
            self.assertEqual(json.loads((destination/'memory-layout.json').read_text()),record)

    def test_actual_manifest_path_retains_qualifier_and_rejects_final_drift(self):
        elf,eav,record=self.fixture()
        for drift in (False,True):
            with self.subTest(drift=drift),tempfile.TemporaryDirectory() as folder:
                firmware=Path(folder);destination=firmware/'target/bench-captures/fixture'
                emitted=destination/'build/firmware.elf';calls=[]
                def build(*args):
                    emitted.parent.mkdir();emitted.write_bytes(elf.data)
                    return subprocess.CompletedProcess([],0,'messages')
                def child(argv,**kwargs):
                    if 'scripts/check_memory_layout.py' in argv:Path(argv[-1]).write_text(json.dumps(record))
                    return subprocess.CompletedProcess(argv,0)
                def clock(*args):
                    calls.append('clock')
                    if drift and len(calls)==2:
                        changed=dict(record,hardware_qualified=True)
                        (destination/'memory-layout.json').write_text(json.dumps(changed))
                    return dict(source=b'clock source',metadata=b'clock metadata')
                options=dict(benchmark=True,features=['demo'],optimization='3',custom_clock=None)
                with ExitStack() as patches:
                    for context in (patch.object(capture,'FIRMWARE',firmware),patch.object(capture,'ElfImage',return_value=elf),
                        patch.object(capture,'check_clock_environment'),patch.object(capture,'compiler',return_value=('rustc','pinned')),
                        patch.object(capture,'configuration',return_value={}),patch.object(capture,'inputs',return_value={'source':b'bytes'}),
                        patch.object(capture,'fixed_json_tool',return_value=(['tool'],options)),
                        patch.object(capture,'selected_config',return_value=('default',None)),
                        patch.object(capture,'freeze_config',return_value=({},{})),patch.object(capture,'verify_selected_config'),
                        patch.object(capture,'run_build',side_effect=build),patch.object(capture,'emitted_elf',return_value=emitted),
                        patch.object(capture,'generated_clock',side_effect=clock),patch.object(capture,'firmware_package',return_value='id'),
                        patch.object(capture,'embedded_config',return_value=eav),patch.object(capture.subprocess,'run',side_effect=child),
                        patch.object(sys,'argv',['capture','fixture']),patch.dict(os.environ,{},clear=True)):
                        patches.enter_context(context)
                    if drift:
                        with self.assertRaises(AssertionError):capture.main()
                        self.assertFalse((destination/'manifest.json').exists())
                    else:
                        capture.main();manifest=json.loads((destination/'manifest.json').read_text())
                        self.assertEqual(manifest['memory_layout'],record)
                        self.assertEqual(manifest['memory_layout_json_sha256'],capture.hashlib.sha256((destination/'memory-layout.json').read_bytes()).hexdigest())


if __name__ == '__main__':
    unittest.main()
