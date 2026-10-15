"""Build, gate and freeze a firmware ELF with exact source/config identity.
This script never flashes hardware or claims a device timing measurement.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import tomllib
import zipfile

FIRMWARE = Path(__file__).resolve().parents[1]
REPO = FIRMWARE.parent
sys.path.insert(0, str(REPO / 'host' / 'scripts'))
from benchmark_environment import CONFIG_KEYS, ENV_PREFIXES, compiler
from source_inventory import shared_inputs
from elf_image import ElfImage
from check_memory_layout import read_image_result
from config_build_source import make_source_context,write_source_context,load_source_context,inspect_source_context

CUSTOM_KEYS = ('DASHBOARD_CPU_HZ', 'DASHBOARD_SPI_HZ', 'DASHBOARD_VSEL_MV')


def read_memory_gate(path,frozen,config_bytes,source_context=None):
    """Retain a full successful image result, bound to this immutable input."""
    with path.open('rb') as stream:raw=stream.read(65537)
    if len(raw)>65536:raise ValueError('oversized image gate result')
    def unique(pairs):
        result={}
        for key,value in pairs:
            if key in result:raise ValueError('duplicate image result key')
            result[key]=value
        return result
    record=json.loads(raw,object_pairs_hook=unique)
    checked=read_image_result(record,ElfImage(frozen),config_bytes,source_context=source_context)
    return dict(record=checked,sha256=hashlib.sha256(raw).hexdigest())


def run_memory_gate(frozen,destination,config_bytes,source_context=None,source_context_path=None):
    path=destination/'memory-layout.json'
    if path.exists():raise ValueError('image result path already exists')
    if (source_context is None)!=(source_context_path is None):
        raise ValueError('explicit source context and its owned path must be supplied together')
    command=[sys.executable,'scripts/check_memory_layout.py',str(frozen)]
    if source_context is not None:
        current=load_source_context(source_context_path,ElfImage(frozen),destination.name,source_context.build_options)
        if current!=source_context:raise ValueError('source sidecar differs from the owned attempt')
        inspect_source_context(current,ElfImage(frozen))
        command.extend(['--source-context',str(source_context_path.resolve())])
    command.extend(['--result-json',str(path.resolve())])
    subprocess.run(command,cwd=FIRMWARE,check=True)
    return read_memory_gate(path,frozen,config_bytes,source_context)


def check_clock_environment(environment):
    if any(key in CUSTOM_KEYS or key.startswith(ENV_PREFIXES) for key in environment):
        raise ValueError('unset ambient compiler/config/custom-clock overrides; select typed inputs explicitly')


def run_build(command, environment, destination):
    build = subprocess.run(command,cwd=FIRMWARE,env=environment,stdout=subprocess.PIPE,text=True,encoding='utf-8')
    (destination/'cargo-messages.jsonl').write_text(build.stdout,encoding='utf-8')
    build.check_returncode()
    return build


def fixed_json_tool(arguments, environment):
    """Drain stdout without retaining unbounded output; stderr remains live."""
    command = ['cargo', 'run', '--quiet', '--locked', '--offline', '-p',
               'dashboard-config-tools', '--', 'capture-options', *arguments]
    with subprocess.Popen(command, cwd=REPO, env=environment, stdout=subprocess.PIPE) as child:
        retained = bytearray()
        oversized = False
        while chunk := child.stdout.read(4096):
            if len(retained) + len(chunk) > 8193:
                oversized = True
            elif not oversized:
                retained.extend(chunk)
        status = child.wait()
    if status or oversized:
        raise ValueError('capture options producer failed or exceeded output bound')
    value = json.loads(retained.decode('utf-8'))
    expected = {'schema_version','normalized','features','no_default_features','optimization',
                'custom_clock','benchmark','legacy_stock','legacy_unavailable_backend','voltage_mv'}
    if (not isinstance(value, dict) or set(value) != expected or value['schema_version'] != 1
            or value['no_default_features'] is not True
            or not isinstance(value['features'], list) or len(value['features']) > 16
            or any(not isinstance(f, str) or not re.fullmatch('[a-z0-9-]+', f) for f in value['features'])
            or value['features'] != sorted(set(value['features']))
            or value['optimization'] not in ('3','2','s','z')):
        raise ValueError('invalid fixed capture options envelope')
    return command, value


def firmware_package(output):
    records = [json.loads(line) for line in output.splitlines()]
    matches = [r['package_id'] for r in records
               if r.get('reason') == 'compiler-artifact'
               and Path(r.get('manifest_path', '')).resolve()
               == (FIRMWARE/'pico2/Cargo.toml').resolve()
               and r.get('target', {}).get('kind') == ['bin']
               and r.get('profile', {}).get('test') is False]
    if len(matches) != 1:
        raise ValueError('expected exactly one firmware package identity')
    return matches[0]


def verify_clock_literals(source, options):
    """Check fixed generated syntax/values, without resolving any clock policy."""
    text = source.decode('utf-8')
    expected_header = '// Generated from dashboard-build-profiles; do not edit.\n'
    if not text.startswith(expected_header):
        raise ValueError('unknown generated clock source schema')
    text = text[len(expected_header):]
    scalar_pattern = r'pub const (\w+): (u32|u8|bool) = ([0-9]+|true|false);\n'
    matches = re.findall(scalar_pattern, text)
    scalars = {name:(kind,value) for name,kind,value in matches}
    if len(scalars) != len(matches):
        raise ValueError('duplicate generated clock literal')
    clock = options['normalized']['clock']
    expected = {key:('u32',str(clock[field])) for key,field in [
        ('REQUESTED_CPU_HZ','requested_cpu_hz'),('ACTUAL_CPU_HZ','actual_cpu_hz'),
        ('REQUESTED_SPI_HZ','requested_spi_hz'),('ACTUAL_SPI_HZ','actual_spi_hz')]}
    expected.update(VOLTAGE_MV=('u32',str(options['voltage_mv'])),
                    CPSDVSR=('u8',str(clock['cpsdvsr'])),SCR=('u8',str(clock['scr'])),
                    LEGACY_ROUNDING=('bool',str(clock['legacy_rounding']).lower()))
    if scalars != expected:
        raise ValueError('generated scalar constants differ from normalized clock')
    remaining = re.sub(scalar_pattern,'',text)
    pll = clock['pll']
    expected_remaining = (f"pub const CORE_VOLTAGE: CoreVoltage = CoreVoltage::{clock['voltage'].upper()};\n"
                          f"pub const PLL: PllConfig = PllConfig {{ refdiv: {pll['refdiv']}, fbdiv: {pll['fbdiv']}, "
                          f"post_div1: {pll['postdiv1']}, post_div2: {pll['postdiv2']} }};\n")
    if remaining != expected_remaining:
        raise ValueError('generated PLL/voltage constants differ from normalized clock')


def generated_clock(output, build_dir, package_id, options):
    records = [json.loads(line) for line in output.splitlines()]
    matches = [r for r in records if r.get('reason') == 'build-script-executed'
               and r.get('package_id') == package_id]
    if len(matches) != 1:
        raise ValueError('expected one firmware clock generation record')
    record = matches[0]
    out = Path(record['out_dir']).resolve()
    if not out.is_relative_to(build_dir.resolve()):
        raise ValueError('clock generation is outside this capture build')
    environment = dict(record.get('env', []))
    evidence = {'out_dir': str(out)}
    for label, filename, key in [('source','clock_profile.rs','DASHBOARD_CLOCK_SOURCE_SHA256'),
                                 ('metadata','clock_profile.json','DASHBOARD_CLOCK_METADATA_SHA256')]:
        path = (out/filename).resolve()
        if not path.is_relative_to(build_dir.resolve()) or not path.is_file():
            raise ValueError('missing or outside-tree generated clock evidence')
        with path.open('rb') as stream:
            data = stream.read(4097)
        digest = hashlib.sha256(data).hexdigest()
        if len(data) > 4096 or digest != environment.get(key):
            raise ValueError('generated clock evidence hash/bound mismatch')
        evidence[label] = data
        evidence[label+'_sha256'] = digest
    metadata = json.loads(evidence['metadata'].decode('utf-8'))
    expected = dict(schema_version=1,selection=options['normalized']['requested']['clock'],
                    clock=options['normalized']['clock'],optimization=options['optimization'],
                    features=options['features'])
    if metadata != expected:
        raise ValueError('actual generated clock/options differ from immutable attempt')
    verify_clock_literals(evidence['source'], options)
    return evidence


def configuration():
    expected = tomllib.loads((FIRMWARE / '.cargo/config.toml').read_text(encoding='utf-8'))
    result = {}
    for key in CONFIG_KEYS:
        call = subprocess.run(['cargo', '-Z', 'unstable-options', 'config', 'get', key, '--format', 'json'],
                              cwd=FIRMWARE, text=True, capture_output=True, encoding='utf-8')
        if key not in expected:
            if call.returncode == 0 or f'config value `{key}` is not set' not in call.stderr:
                raise ValueError(f'unrecorded Cargo configuration for {key}')
        elif call.returncode or json.loads(call.stdout) != {key: expected[key]}:
            raise ValueError(f'effective {key} differs from checked-in Cargo configuration')
        result[key] = expected.get(key)
    return result


def inputs():
    paths = shared_inputs(REPO)
    for crate in [FIRMWARE/'pico2', FIRMWARE/'vendor/embassy-rp']:
        paths.update(p for p in (crate/'src').rglob('*') if p.is_file())
        # Root files are also compile inputs (include_str!, linker templates,
        # manifests and build scripts). Keep the complete crate root.
        paths.update(p for p in crate.iterdir() if p.is_file())
    for directory in [FIRMWARE, FIRMWARE/'pico2', FIRMWARE/'vendor/embassy-rp']:
        for name in ['Cargo.toml','Cargo.lock','build.rs','rust-toolchain.toml','.cargo/config.toml']:
            path=directory/name
            if path.is_file(): paths.add(path)
    paths.update([Path(__file__).resolve(), REPO/'host/scripts/benchmark_environment.py'])
    paths.update((FIRMWARE/'scripts').glob('*.py'))
    paths.update((FIRMWARE/'scripts').glob('*.ps1'))
    return {p.relative_to(REPO).as_posix():p.read_bytes() for p in sorted(paths)}


def emitted_elf(output):
    candidates, finished = [], []
    manifest = (FIRMWARE / 'pico2/Cargo.toml').resolve()
    for line in output.splitlines():
        record = json.loads(line)
        if not isinstance(record, dict):
            raise ValueError('invalid Cargo message')
        if record.get('reason') == 'build-finished':
            finished.append(record.get('success'))
        if (record.get('reason') == 'compiler-artifact'
                and Path(record.get('manifest_path', '')).resolve() == manifest
                and record.get('target', {}).get('name') == 'pico2'
                and record.get('target', {}).get('kind') == ['bin']
                and record.get('profile', {}).get('test') is False):
            executable = record.get('executable')
            if not isinstance(executable, str) or not Path(executable).is_absolute():
                raise ValueError('Cargo emitted no absolute firmware executable')
            candidates.append(Path(executable).resolve())
    if finished != [True] or len(candidates) != 1:
        raise ValueError('expected one successful build and one firmware executable')
    return candidates[0]


def bounded_bytes(path):
    if not path.is_file():
        raise ValueError('configuration must be a regular binary .eav file')
    with path.open('rb') as stream:
        data = stream.read(16385)
    if len(data) > 16384:
        raise ValueError('configuration exceeds 16 KiB')
    return data


def selected_config(environment):
    selected = environment.get('DASHBOARD_CONFIG')
    if selected is None:
        return None, None
    if not selected or any(ord(ch) < 32 or ord(ch) == 127 for ch in selected):
        raise ValueError('DASHBOARD_CONFIG must be a nonempty path without controls')
    # Match build.rs: environment-relative paths are relative to the crate.
    path = (FIRMWARE / 'pico2' / selected).absolute()
    return str(path), bounded_bytes(path)


def freeze_config(selection, destination, environment):
    selected, data = selection
    frozen = environment.copy()
    if selected is None:
        frozen.pop('DASHBOARD_CONFIG', None)
        return frozen, dict(selection='default')
    path = destination / 'selected.eav'
    path.write_bytes(data)
    frozen['DASHBOARD_CONFIG'] = str(path.resolve())
    return frozen, dict(selection='explicit', source=selected,
                        sha256=hashlib.sha256(data).hexdigest())


def embedded_config(output, build_dir):
    matches = []
    for line in output.splitlines():
        record = json.loads(line)
        if record.get('reason') == 'build-script-executed':
            environment = dict(record.get('env', []))
            if 'DASHBOARD_CONFIG_SHA256' in environment:
                path = (Path(record['out_dir']) / 'dashboard.eav').resolve()
                if not path.is_relative_to(build_dir.resolve()):
                    raise ValueError('embedded config is outside this build')
                data = bounded_bytes(path)
                if hashlib.sha256(data).hexdigest() != environment['DASHBOARD_CONFIG_SHA256']:
                    raise ValueError('embedded config differs from validated build snapshot')
                matches.append(data)
    if len(matches) != 1:
        raise ValueError('expected one embedded configuration snapshot')
    return matches[0]


def verify_selected_config(destination, environment, embedded, record):
    """Compare to the canonical upgrade, retaining the original input identity."""
    if record['selection'] != 'explicit':
        return
    original = bounded_bytes(destination / 'selected.eav')
    if hashlib.sha256(original).hexdigest() != record['sha256']:
        raise ValueError('frozen selected configuration changed')
    normalized = destination / 'normalized.eav'
    command = ['cargo', 'run', '--locked', '--release', '-p', 'dashboard-config-tools',
               '--target-dir', str(destination / 'config-compiler'), '--', 'compile',
               str(destination / 'selected.eav'), str(normalized)]
    subprocess.run(command, cwd=REPO, env=environment, check=True)
    expected = bounded_bytes(normalized)
    if bounded_bytes(destination / 'selected.eav') != original:
        raise ValueError('frozen selected configuration changed during normalization')
    if expected != embedded:
        raise ValueError('embedded configuration differs from canonical selected file')
    record['normalization_command'] = command
    record['normalized_sha256'] = hashlib.sha256(expected).hexdigest()


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('name')
    parser.add_argument('--features')
    parser.add_argument('--no-default-features',action='store_true')
    parser.add_argument('--firmware',action='store_true', help='allow an ordinary firmware image without target-bench')
    parser.add_argument('--dashboard-config', help='binary .eav file; overrides DASHBOARD_CONFIG')
    parser.add_argument('--options-json', help='bounded shared NormalizedBuildOptions JSON; excludes raw feature switches')
    parser.add_argument('--expert', action='store_true', help='explicit local authorization for experimental/custom clock selection')
    parser.add_argument('--optimization', choices=['3','2','s','z'])
    parser.add_argument('--cpu-hz')
    parser.add_argument('--spi-hz')
    parser.add_argument('--vsel-mv')
    args=parser.parse_args()
    if not re.fullmatch(r'[A-Za-z0-9][A-Za-z0-9_-]*',args.name): parser.error('invalid capture name')
    if args.options_json is not None and (args.features is not None or args.no_default_features or args.expert or args.optimization or args.cpu_hz is not None or args.spi_hz is not None or args.vsel_mv is not None):
        parser.error('typed options cannot be combined with raw capture overrides')
    destination=FIRMWARE/'target/bench-captures'/args.name
    if destination.exists(): parser.error('capture name already exists')
    if any(k.startswith('DEFMT_') for k in os.environ): parser.error('unset external DEFMT overrides')
    check_clock_environment(os.environ)
    rustc,version=compiler(FIRMWARE)
    config=configuration()
    before=inputs()
    host_environment = os.environ.copy()
    host_environment['RUSTC'] = rustc
    if args.options_json is not None:
        options_arguments = ['--normalized-json', args.options_json]
    else:
        options_arguments = ['--features', args.features if args.features is not None else 'target-bench,longan-rx12,frame-trace,te-trace',
                             '--no-default-features', str(args.no_default_features).lower(), '--expert', str(args.expert).lower()]
        for key, value in [('--optimization',args.optimization),('--cpu-hz',args.cpu_hz),('--spi-hz',args.spi_hz),('--vsel-mv',args.vsel_mv)]:
            if value is not None: options_arguments.extend([key, value])
    options_command, options = fixed_json_tool(options_arguments, host_environment)
    if not args.firmware and not options['benchmark']:
        parser.error('non-firmware capture requires explicit target-bench')
    selected_environment = os.environ.copy()
    if args.dashboard_config is not None:
        selected_environment['DASHBOARD_CONFIG'] = args.dashboard_config
    selection = selected_config(selected_environment)
    # Reserve the capture atomically and give this build its own output tree.
    # Failed runs stay incomplete under their original name for inspection.
    destination.mkdir(parents=True)
    attempt_bytes = json.dumps(options, sort_keys=True, separators=(',',':')).encode('utf-8')
    (destination/'build-options.json').write_bytes(attempt_bytes)
    environment, config_record = freeze_config(selection, destination, selected_environment)
    environment.update(RUSTC=rustc,CARGO_PROFILE_RELEASE_STRIP='none',CARGO_PROFILE_RELEASE_OPT_LEVEL=options['optimization'])
    if options['custom_clock'] is not None:
        custom = options['custom_clock']
        if set(custom) != {'cpu_hz','spi_hz','vsel_mv'} or any(not isinstance(v,str) or not re.fullmatch('[0-9]+',v) for v in custom.values()):
            raise ValueError('invalid fixed custom environment framing')
        environment.update(zip(CUSTOM_KEYS,[custom['cpu_hz'],custom['spi_hz'],custom['vsel_mv']]))
    command=['cargo','build','--locked','-p','dashboard-pico2','--release',
             '--target','thumbv8m.main-none-eabihf','--target-dir',str(destination/'build'),
             '--message-format=json','--features',','.join(options['features']),'--no-default-features']
    build=run_build(command, environment, destination)
    elf=emitted_elf(build.stdout)
    clock_evidence = generated_clock(build.stdout, destination/'build', firmware_package(build.stdout), options)
    config_bytes = embedded_config(build.stdout, destination/'build')
    verify_selected_config(destination, environment, config_bytes, config_record)
    (destination/'dashboard.eav').write_bytes(config_bytes)
    config_record['embedded_sha256'] = hashlib.sha256(config_bytes).hexdigest()
    if not elf.is_relative_to((destination/'build').resolve()):
        raise ValueError('Cargo emitted an executable outside this capture build')
    frozen=destination/'pico2.elf'
    elf_bytes=elf.read_bytes()
    frozen.write_bytes(elf_bytes)
    source_context=make_source_context(elf_bytes,destination.name,attempt_bytes,before)
    source_context_path=destination/'source-context.json'
    write_source_context(source_context_path,source_context)
    source_context_bytes=source_context_path.read_bytes()
    source_context_record=dict(path='source-context.json',sha256=hashlib.sha256(source_context_bytes).hexdigest(),
                               binding=source_context.binding())
    memory_evidence=run_memory_gate(frozen,destination,config_bytes,source_context,source_context_path)
    for flags in [[], ['-O']]:
        subprocess.run([sys.executable,*flags,'scripts/test_flash_lockout.py',str(frozen)],cwd=FIRMWARE,check=True)
    if before!=inputs() or config!=configuration() or (rustc,version)!=compiler(FIRMWARE):
        raise ValueError('build inputs changed during capture')
    if frozen.read_bytes()!=elf_bytes or elf.read_bytes()!=elf_bytes:
        raise ValueError('ELF changed during capture')
    if selection != selected_config(selected_environment):
        raise ValueError('selected configuration changed during capture')
    check_clock_environment(os.environ)
    if (destination/'build-options.json').read_bytes() != attempt_bytes:
        raise ValueError('immutable build attempt changed during capture')
    if clock_evidence != generated_clock(build.stdout,destination/'build',firmware_package(build.stdout),options):
        raise ValueError('clock generation changed during capture')
    if (destination/'build-options.json').read_bytes()!=attempt_bytes or source_context_path.read_bytes()!=source_context_bytes or load_source_context(
            source_context_path,ElfImage(frozen),destination.name,attempt_bytes)!=source_context:
        raise ValueError('captured source sidecar changed during the gate')
    if memory_evidence!=read_memory_gate(destination/'memory-layout.json',frozen,config_bytes,source_context):
        raise ValueError('conditional image result changed during capture')
    (destination/'clock_profile.rs').write_bytes(clock_evidence['source'])
    (destination/'clock_profile.json').write_bytes(clock_evidence['metadata'])
    with zipfile.ZipFile(destination/'source.zip','w',compression=zipfile.ZIP_DEFLATED) as archive:
        for name,content in before.items(): archive.writestr(name,content)
        archive.writestr('build-config/source-context.json',source_context_bytes)
        archive.writestr('build-config/dashboard.eav', config_bytes)
        archive.writestr('build-config/build-options.json', attempt_bytes)
        archive.writestr('build-config/clock_profile.rs',clock_evidence['source'])
        archive.writestr('build-config/clock_profile.json',clock_evidence['metadata'])
        if selection[1] is not None:
            archive.writestr('build-config/selected.eav', selection[1])
    hashes={name:hashlib.sha256(data).hexdigest() for name,data in before.items()}
    record=dict(schema=1,scope='Build artifact only; hardware not executed',rustc=version,rustc_path=rustc,
                command=command,emitted_executable=str(elf),cargo_config=config,
                explicit_environment={key: environment[key] for key in
                                      ['CARGO_PROFILE_RELEASE_STRIP','CARGO_PROFILE_RELEASE_OPT_LEVEL','RUSTC','DASHBOARD_CONFIG',*CUSTOM_KEYS] if key in environment},
                options_command=options_command,build_options=options,
                build_options_sha256=hashlib.sha256(attempt_bytes).hexdigest(),
                generated_clock={k:v for k,v in clock_evidence.items() if k not in ('source','metadata')},
                dashboard_config=config_record,source_context=source_context_record,
                memory_layout=memory_evidence['record'],
                memory_layout_json_sha256=memory_evidence['sha256'],
                sha256=hashes,elf_sha256=hashlib.sha256(elf_bytes).hexdigest())
    (destination/'manifest.json').write_text(json.dumps(record,indent=2)+'\n',encoding='utf-8')
    print(destination)
    print('ELF SHA256 '+record['elf_sha256'])


if __name__=='__main__': main()
