"""Independently consume completed capture identity and strict converted UF2 bytes.
Practical static checks admit software artifacts, not hardware qualification.
"""

import hashlib
import json
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from validation import require
from script_values import object_fields, integer_value
from elf_image import ElfImage
from check_program_image import (
    inspect_program,
    linked_configuration,
    linked_startup,
    read_uf2,
    verify_uf2,
)
import check_memory_layout
from config_build_source import load_source_context


def bounded(path: str | Path, maximum: int) -> bytes:
    with Path(path).open("rb") as stream:
        data = stream.read(maximum + 1)
    require(len(data) <= maximum, "artifact input byte bound")
    return data


def unique(pairs: list[tuple[str, object]]) -> dict[str, object]:
    result: dict[str, object] = {}
    for key, value in pairs:
        require(key not in result, "duplicate artifact key")
        result[key] = value
    return result


def record(path: str | Path, maximum: int) -> dict[str, object]:
    return object_fields(
        json.loads(bounded(path, maximum), object_pairs_hook=unique), "artifact must be an object"
    )


def sha(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def canonical(value: object) -> str:
    # Match JSON array representation without Python's bool/int equality coercion.
    return json.dumps(value, sort_keys=True, separators=(",", ":"), allow_nan=False)


def validate(capture: str | Path, inputs: str | Path, uf2: str | Path) -> dict[str, object]:
    capture = Path(capture)
    inputs = Path(inputs)
    manifest = record(capture / "manifest.json", 4 * 1024 * 1024)
    require(
        integer_value(manifest.get("schema"), "completed schema2 capture required") == 2,
        "completed schema2 capture required",
    )
    require(
        manifest.get("image_gate") == "practical",
        "native artifacts require explicit practical image checks",
    )
    elf = ElfImage(capture / "pico2.elf")
    require(sha(elf.data) == manifest.get("elf_sha256"), "capture ELF identity mismatch")
    eav = bounded(inputs / "dashboard.eav", 65536)
    require(eav == bounded(capture / "dashboard.eav", 65536), "accepted EAV differs from capture")
    require(
        object_fields(manifest.get("dashboard_config"), "missing configuration identity").get(
            "embedded_sha256"
        )
        == sha(eav),
        "capture configuration hash mismatch",
    )
    options = record(inputs / "options.json", 65536)
    attempt = bounded(capture / "build-options.json", 65536)
    require(
        canonical(manifest.get("build_options"))
        == canonical(json.loads(attempt, object_pairs_hook=unique))
        and canonical(
            object_fields(manifest.get("build_options"), "missing build options").get("normalized")
        )
        == canonical(options)
        and manifest.get("build_options_sha256") == sha(attempt),
        "capture options mismatch",
    )
    context_path = capture / "source-context.json"
    context = load_source_context(context_path, elf, capture.name, attempt)
    require(
        object_fields(manifest.get("source_context"), "missing source context").get("sha256")
        == sha(bounded(context_path, 262144)),
        "source context identity mismatch",
    )
    memory_raw = bounded(capture / "memory-layout.json", 65536)
    memory = check_memory_layout.read_selected_image_result(
        json.loads(memory_raw, object_pairs_hook=unique), elf, eav, context, gate="practical"
    )
    require(
        canonical(manifest.get("memory_layout")) == canonical(memory)
        and manifest.get("memory_layout_json_sha256") == sha(memory_raw),
        "practical image evidence mismatch",
    )
    mapping, base_program = inspect_program(elf)
    program: dict[str, object] = dict(base_program)
    program["linked_configuration"] = linked_configuration(elf, eav)
    profile = bounded(inputs / "startup.easp", 16)
    asset = bounded(inputs / "startup.easa", 524288)
    program["linked_startup"] = linked_startup(elf, profile, asset)
    require(
        canonical(program) == canonical(manifest.get("program_image")),
        "program image identity mismatch",
    )
    converted = read_uf2(uf2)
    comparison = verify_uf2(converted, mapping)
    return {
        "schema_version": 2,
        "scope": "practical_software_artifact",
        "image_gate": "practical",
        "complete_instruction_proof": False,
        "hardware_qualified": False,
        "flash_ready": False,
        "elf_sha256": sha(elf.data),
        "uf2_sha256": sha(converted),
        "uf2_bytes": len(converted),
        "config_sha256": sha(eav),
        "startup_profile_sha256": sha(profile),
        "startup_asset_sha256": sha(asset),
        "options": options,
        "program_image": program,
        "uf2": comparison,
        "memory_layout": memory,
    }


def main() -> None:
    import argparse

    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("capture")
    parser.add_argument("inputs")
    parser.add_argument("uf2")
    args = parser.parse_args()
    result = validate(args.capture, args.inputs, args.uf2)
    print(json.dumps(result, sort_keys=True, separators=(",", ":")))


if __name__ == "__main__":
    main()
