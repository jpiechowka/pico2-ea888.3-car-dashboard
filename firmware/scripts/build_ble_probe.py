"""Build and admit a standalone BLE probe artifact; never accesses a device."""

import argparse
import json
import os
from pathlib import Path
import re
import shutil
import sys
import tomllib

from ble_probe_image import GATE, check, check_sources, sha, source_hashes
from validation import require

REPO = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(REPO / "host/scripts"))
from owned_process import run_checked  # noqa: E402

TARGET = "thumbv8m.main-none-eabihf"


def build_environment(original, toolchain, attempt):
    # Child-only configuration: never mutate the caller or another build.
    from benchmark_environment import ENV_PREFIXES

    env = {
        key: value
        for key, value in original.items()
        if not key.startswith((*ENV_PREFIXES, "DASHBOARD_"))
    }
    flags = [
        "-Z",
        "emit-stack-sizes",
        "-C",
        "link-arg=--nmagic",
        "-C",
        "link-arg=-Tlink.x",
        "-C",
        "link-arg=-Tdefmt.x",
        "-C",
        "target-cpu=cortex-m33",
    ]
    env.update(
        RUSTUP_TOOLCHAIN=toolchain,
        CARGO_ENCODED_RUSTFLAGS="\x1f".join(flags),
        CARGO_PROFILE_RELEASE_STRIP="none",
        CARGO_TARGET_DIR=str(attempt / "build"),
        CARGO_BUILD_JOBS="2",
        CARGO_INCREMENTAL="0",
        DEFMT_LOG="info,trouble_host=warn,cyw43=warn,cyw43_pio=warn",
    )
    return env


def find_clock_metadata(build_root):
    # Cargo nightlies use both package-hash/out and package/hash/out layouts.
    # The attempt has a fresh target tree; never pick one from a shared cache.
    found = []
    for path in build_root.rglob("clock_profile.json"):
        require(len(found) < 8, "generated clock candidate bound")
        found.append(path)
    require(len(found) == 1, "missing or ambiguous generated clock metadata")
    return found[0]


def build(name, profile, expert):
    require(
        re.fullmatch(r"[A-Za-z0-9][A-Za-z0-9_-]{0,63}", name) is not None, "invalid attempt name"
    )
    require(profile in ("stock", "280-70"), "unknown profile")
    require(profile == "stock" or expert, "280-70 requires explicit Expert")
    attempt = REPO / "firmware/target/ble-probe-captures" / name
    attempt.mkdir(parents=True, exist_ok=False)
    firmware = REPO / "firmware"
    toolchain = tomllib.loads((firmware / "rust-toolchain.toml").read_text())["toolchain"][
        "channel"
    ]
    env = build_environment(os.environ, toolchain, attempt)
    flags = env["CARGO_ENCODED_RUSTFLAGS"].split("\x1f")

    def run(args, timeout=120, host=False):
        selected_env = env.copy()
        if host:
            selected_env.pop("CARGO_ENCODED_RUSTFLAGS", None)
        result = run_checked(
            args, cwd=firmware, env=selected_env, timeout=timeout, capture_output=True, check=False
        )
        with (attempt / "commands.log").open("a", encoding="utf-8") as stream:
            stream.write(json.dumps(args) + "\n" + result.stdout + result.stderr + "\n")
        print(result.stdout, end="")
        if result.stderr:
            print(result.stderr, end="", file=sys.stderr)
        result.check_returncode()
        return result.stdout.strip()

    sources = source_hashes(REPO)
    (attempt / "sources-before.json").write_text(
        json.dumps(sources, sort_keys=True), encoding="utf-8"
    )
    rustc = run(["rustc", "--version", "--verbose"])
    cargo = run(["cargo", "--version"])
    converter = REPO / "target/configurator-tools/bin/picotool.exe"
    require(converter.is_file(), "missing pinned picotool; run Studio.ps1 -Setup")
    converter_hash = sha(converter.read_bytes())
    require(run([str(converter), "version", "-s"]) == "2.3.1", "picotool 2.3.1 required")
    help_text = run([str(converter), "help", "uf2", "convert"])
    require("--family" in help_text and "--platform" in help_text, "converter capabilities")
    run([sys.executable, "scripts/check_vendor.py"], 300, host=True)
    run([sys.executable, "scripts/check_radio_vendor.py"], 300, host=True)
    args = [
        "cargo",
        "build",
        "--locked",
        "--release",
        "-p",
        "dashboard-ble-probe",
        "--bin",
        "ble-probe",
        "--target",
        TARGET,
    ]
    if profile == "280-70":
        args += ["--features", "cpu280-spi70-1v30"]
    print("Compilation: " + " ".join(args) + "; target-cpu=cortex-m33")
    run(args, 1200)
    check_sources(REPO, sources)
    generated = find_clock_metadata(attempt / "build" / TARGET / "release/build")
    clock_raw = generated.read_bytes()
    require(len(clock_raw) <= 4096, "clock metadata bound")
    clock = json.loads(clock_raw)
    expected_cpu, expected_spi = (
        (280000000, 70000000) if profile == "280-70" else (150000000, 37500000)
    )
    require(
        clock["clock"]["actual_cpu_hz"] == expected_cpu
        and clock["clock"]["actual_spi_hz"] == expected_spi,
        "resolved clock differs",
    )
    (attempt / "clock_profile.json").write_bytes(clock_raw)
    clock_source = generated.with_name("clock_profile.rs").read_bytes()
    (attempt / "clock_profile.rs").write_bytes(clock_source)
    elf = attempt / "ble-probe.elf"
    shutil.copyfile(attempt / "build" / TARGET / "release/ble-probe", elf)
    uf2 = attempt / "ble-probe.uf2"
    require(sha(converter.read_bytes()) == converter_hash, "converter changed")
    run(
        [
            str(converter),
            "uf2",
            "convert",
            str(elf),
            str(uf2),
            "--family",
            "rp2350-arm-s",
            "--platform",
            "rp2350",
        ]
    )
    require(sha(converter.read_bytes()) == converter_hash, "converter changed during conversion")
    manifest = {
        "schema": 1,
        "gate": GATE,
        "sources": sources,
        "rustc": rustc,
        "cargo": cargo,
        "target": TARGET,
        "target_cpu": "cortex-m33",
        "flags": flags,
        "profile": profile,
        "command": args,
        "build_jobs": 2,
        "defmt_log": env["DEFMT_LOG"],
        "incremental": False,
        "strip": "none",
        "toolchain": toolchain,
        "converter_sha256": converter_hash,
        "clock_metadata": clock,
        "clock_source_sha256": sha(clock_source),
        "elf_sha256": sha(elf.read_bytes()),
        "uf2_sha256": sha(uf2.read_bytes()),
    }
    manifest_path = attempt / "manifest.json"
    manifest_path.write_text(json.dumps(manifest, sort_keys=True, indent=2), encoding="utf-8")
    result = check(elf, uf2, manifest_path, REPO)
    (attempt / "image-result.json").write_text(
        json.dumps(result, sort_keys=True, indent=2), encoding="utf-8"
    )
    print(f"Ready for explicit device selection: {elf}\nNo device was accessed.")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("name")
    parser.add_argument("--profile", choices=("stock", "280-70"), default="stock")
    parser.add_argument("--expert", action="store_true")
    args = parser.parse_args()
    build(args.name, args.profile, args.expert)


if __name__ == "__main__":
    main()
