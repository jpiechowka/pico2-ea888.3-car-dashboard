"""Build, gate and freeze a target-bench ELF with exact source/config identity.
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
from benchmark_environment import CONFIG_KEYS, compiler


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
    paths = set()
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


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('name')
    parser.add_argument('--features',default='target-bench,longan-rx12,frame-trace,te-trace')
    parser.add_argument('--no-default-features',action='store_true')
    args=parser.parse_args()
    if not re.fullmatch(r'[A-Za-z0-9][A-Za-z0-9_-]*',args.name): parser.error('invalid capture name')
    if not re.fullmatch(r'[a-z0-9-]+(?:,[a-z0-9-]+)*',args.features) or 'target-bench' not in args.features.split(','):
        parser.error('features must include target-bench and contain only feature names')
    destination=FIRMWARE/'target/bench-captures'/args.name
    if destination.exists(): parser.error('capture name already exists')
    if any(k.startswith('DEFMT_') for k in os.environ): parser.error('unset external DEFMT overrides')
    rustc,version=compiler(FIRMWARE)
    config=configuration()
    before=inputs()
    # Reserve the capture atomically and give this build its own output tree.
    # Failed runs stay incomplete under their original name for inspection.
    destination.mkdir(parents=True)
    environment=os.environ.copy()
    environment.update(RUSTC=rustc,CARGO_PROFILE_RELEASE_STRIP='none')
    command=['cargo','build','--locked','-p','dashboard-pico2','--release',
             '--target','thumbv8m.main-none-eabihf','--target-dir',str(destination/'build'),
             '--message-format=json','--features',args.features]
    if args.no_default_features: command.append('--no-default-features')
    build=subprocess.run(command,cwd=FIRMWARE,env=environment,check=True,stdout=subprocess.PIPE,text=True,encoding='utf-8')
    elf=emitted_elf(build.stdout)
    if not elf.is_relative_to((destination/'build').resolve()):
        raise ValueError('Cargo emitted an executable outside this capture build')
    frozen=destination/'pico2.elf'
    elf_bytes=elf.read_bytes()
    frozen.write_bytes(elf_bytes)
    subprocess.run([sys.executable,'scripts/check_memory_layout.py',str(frozen)],cwd=FIRMWARE,check=True)
    if before!=inputs() or config!=configuration() or (rustc,version)!=compiler(FIRMWARE):
        raise ValueError('build inputs changed during capture')
    if frozen.read_bytes()!=elf_bytes or elf.read_bytes()!=elf_bytes:
        raise ValueError('ELF changed during capture')
    with zipfile.ZipFile(destination/'source.zip','w',compression=zipfile.ZIP_DEFLATED) as archive:
        for name,content in before.items(): archive.writestr(name,content)
    hashes={name:hashlib.sha256(data).hexdigest() for name,data in before.items()}
    record=dict(schema=1,scope='Build artifact only; hardware not executed',rustc=version,rustc_path=rustc,
                command=command,emitted_executable=str(elf),cargo_config=config,
                explicit_environment=dict(CARGO_PROFILE_RELEASE_STRIP='none',RUSTC=rustc),
                sha256=hashes,elf_sha256=hashlib.sha256(elf_bytes).hexdigest())
    (destination/'manifest.json').write_text(json.dumps(record,indent=2)+'\n',encoding='utf-8')
    print(destination)
    print('ELF SHA256 '+record['elf_sha256'])


if __name__=='__main__': main()
