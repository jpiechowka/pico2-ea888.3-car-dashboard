"""Verify the complete published HAL plus UART, RAM lockout and flash deltas.

Optional --archive verifies the downloaded .crate and every original file too.
No archive contents are extracted or executed.
"""
import argparse
import hashlib
import json
from validation import require
from pathlib import Path, PurePosixPath
import tarfile

ROOT = Path(__file__).resolve().parents[1] / "vendor"


def digest(data):
    return hashlib.sha256(data).hexdigest()


def check(archive=None):
    manifest = json.loads((ROOT / "embassy-rp-provenance.json").read_text(encoding="utf-8"))
    original = manifest["original_files_sha256"]
    patched = manifest["patched_files_sha256"]
    require(set(patched) == {"src/uart/buffered.rs", "src/uart/mod.rs", "src/uart/buffered_io.rs",
                            "src/multicore.rs", "src/flash.rs", "src/flash_probe.rs", "src/pwm.rs",
                            "Cargo.toml", "Cargo.toml.orig", "src/lib.rs"})
    expected = original | patched
    for name in expected:
        path = PurePosixPath(name)
        require(not path.is_absolute() and ".." not in path.parts and "\\" not in name)
    tree = ROOT / "embassy-rp"
    actual = {p.relative_to(tree).as_posix(): digest(p.read_bytes()) for p in tree.rglob("*") if p.is_file()}
    require(actual == expected, f"vendor mismatch: {sorted(k for k in actual.keys() | expected.keys() if actual.get(k) != expected.get(k))}")
    if archive is not None:
        require(digest(archive.read_bytes()) == manifest["archive_sha256"], "archive checksum mismatch")
        prefix = f'{manifest["crate"]}-{manifest["version"]}/'
        with tarfile.open(archive, "r:gz") as tar:
            restored = {}
            for member in tar.getmembers():
                require(member.name.startswith(prefix), "unexpected archive root")
                if member.isdir():
                    continue
                require(member.isfile(), "unexpected archive entry type")
                name = member.name[len(prefix):]
                require(name not in restored, "duplicate archive entry")
                restored[name] = digest(tar.extractfile(member).read())
        require(restored == original, "original file manifest does not match published archive")
    print(f"Verified {len(original)} original files, {len(set(patched) & set(original))} modified files and {len(set(patched) - set(original))} added helpers; archive={'checked' if archive else 'not supplied'}")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--archive", type=Path)
    check(parser.parse_args().archive)
