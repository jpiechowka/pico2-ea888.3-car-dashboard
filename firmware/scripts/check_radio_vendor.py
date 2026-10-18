"""Verify reviewed radio source overlays and pinned firmware bytes, without execution."""

import hashlib
import json
import re
from pathlib import Path, PurePosixPath

from validation import require

ROOT = Path(__file__).resolve().parents[1] / "vendor"
CRATES = {
    "cyw43": ("0.7.0", "033e856bf6f38c96fee3e3f398cceff0ca2c2fae5db255bcd23495216c7047da"),
    "trouble-host": ("0.8.0", "19a566cc37ceaff5cc2a6ff8cc02debd1ee6b95d59ced8830d005da8b080cbe9"),
}
RADIO_COMMIT = "84444a19eb57d978e3c09fcdc8e60cdd7278eb03"


def verify_tree(root, original, patched):
    require(
        isinstance(original, dict) and original and isinstance(patched, dict),
        "invalid provenance maps",
    )
    for name, digest in (original | patched).items():
        require(isinstance(name, str) and name, "empty source path")
        path = PurePosixPath(name)
        require(
            not path.is_absolute()
            and ".." not in path.parts
            and "." != name
            and "\\" not in name
            and ":" not in name
            and path.as_posix() == name,
            "noncanonical source path",
        )
        require(path.parts[0] != "target", "source cannot be generated Cargo output")
        require(
            isinstance(digest, str) and re.fullmatch("[0-9a-f]{64}", digest), "invalid source hash"
        )
    expected = original | patched
    # Standalone vendor regressions create Cargo output here. It is not source;
    # every other entry, including nested src/target, remains hash checked.
    files = [
        p
        for child in root.iterdir()
        if child.name != "target"
        for p in ([child, *child.rglob("*")] if child.is_dir() else [child])
    ]
    require(not any(p.is_symlink() for p in files), "vendor symlink")
    actual = {
        p.relative_to(root).as_posix(): hashlib.sha256(p.read_bytes()).hexdigest()
        for p in files
        if p.is_file()
    }
    require(
        actual == expected,
        "radio vendor mismatch: "
        + str(
            sorted(
                name
                for name in actual.keys() | expected.keys()
                if actual.get(name) != expected.get(name)
            )
        ),
    )


def check():
    for crate, (version, archive) in CRATES.items():
        data = json.loads((ROOT / (crate + "-provenance.json")).read_text(encoding="utf-8"))
        require(
            data["crate"] == crate
            and data["version"] == version
            and data["archive_sha256"] == archive,
            "unexpected radio source release",
        )
        require(
            data["source"] == f"https://static.crates.io/crates/{crate}/{crate}-{version}.crate",
            "unexpected radio source URL",
        )
        verify_tree(ROOT / crate, data["original"], data["modified"])
        print(f"{crate} {version}: complete upstream tree and reviewed overlay verified")
    data = json.loads((ROOT / "cyw43-firmware-provenance.json").read_text(encoding="utf-8"))
    require(
        data["revision"] == RADIO_COMMIT
        and data["repository"] == "https://github.com/embassy-rs/embassy",
        "unexpected radio firmware revision",
    )
    require(
        set(data["files"])
        == {
            "43439A0.bin",
            "43439A0_btfw.bin",
            "nvram_rp2040.bin",
            "README.md",
            "LICENSE-permissive-binary-license-1.0.txt",
        },
        "radio firmware inventory",
    )
    verify_tree(ROOT / "cyw43-firmware", data["files"], {})
    print("CYW firmware, NVRAM and license bytes verified")


if __name__ == "__main__":
    check()
