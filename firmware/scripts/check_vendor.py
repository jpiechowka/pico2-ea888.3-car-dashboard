"""Verify the complete published HAL plus UART, RAM lockout and flash deltas.

Optional --archive verifies the downloaded .crate and every original file too.
No archive contents are extracted or executed.
"""
import argparse
import hashlib
import json
from pathlib import Path, PurePosixPath
import tarfile

ROOT = Path(__file__).resolve().parents[1] / "vendor"


def digest(data):
    return hashlib.sha256(data).hexdigest()


def check(archive=None):
    manifest = json.loads((ROOT / "embassy-rp-provenance.json").read_text(encoding="utf-8"))
    original = manifest["original_files_sha256"]
    patched = manifest["patched_files_sha256"]
    assert set(patched) == {"src/uart/buffered.rs", "src/uart/mod.rs", "src/uart/buffered_io.rs",
                            "src/multicore.rs", "src/flash.rs", "src/flash_probe.rs"}
    expected = original | patched
    for name in expected:
        path = PurePosixPath(name)
        assert not path.is_absolute() and ".." not in path.parts and "\\" not in name
    tree = ROOT / "embassy-rp"
    actual = {p.relative_to(tree).as_posix(): digest(p.read_bytes()) for p in tree.rglob("*") if p.is_file()}
    assert actual == expected, f"vendor mismatch: {sorted(k for k in actual.keys() | expected.keys() if actual.get(k) != expected.get(k))}"
    if archive is not None:
        assert digest(archive.read_bytes()) == manifest["archive_sha256"], "archive checksum mismatch"
        prefix = f'{manifest["crate"]}-{manifest["version"]}/'
        with tarfile.open(archive, "r:gz") as tar:
            restored = {}
            for member in tar.getmembers():
                assert member.name.startswith(prefix), "unexpected archive root"
                if member.isdir():
                    continue
                assert member.isfile(), "unexpected archive entry type"
                name = member.name[len(prefix):]
                assert name not in restored, "duplicate archive entry"
                restored[name] = digest(tar.extractfile(member).read())
        assert restored == original, "original file manifest does not match published archive"
    print(f"Verified {len(original)} original files, 4 modified files and 2 added helpers; archive={'checked' if archive else 'not supplied'}")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--archive", type=Path)
    check(parser.parse_args().archive)
