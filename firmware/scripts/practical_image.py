"""Practical static image admission, distinct from the experimental full proof.

Readers recompute the checks against the exact ELF and explicit source pair.
No execution receipt, mutable owner or whole-program proof is issued here.
"""

from contextlib import redirect_stdout
import hashlib
import io
import json
import struct
import zlib
from typing import TypedDict
from check_stack_budget import check as stack_check, FrameAdmission, FrameRecord
from check_program_image import inspect_program, linked_configuration
from config_build_source import validate_source_context
from elf_image import InspectableImage
from script_values import object_fields
from validation import require

# Current-only device envelope, matched against the shipped EAV by regression test.
# Historical schemas 1..6 migrate through host tooling before image admission.
CURRENT_EAV_VERSION = 7

ASSUMPTIONS = (
    "supported_rp2350_pico2_fresh_boot_and_selected_profile",
    "source_reviewed_named_paths_and_fixed_child_stack_allowances",
    "source_reviewed_buffer_config_and_flash_ownership_protocols",
)
LIMITATIONS = (
    "no_complete_instruction_effect_or_activation_graph_proof",
    "no_exception_or_reset_reentry_safety_proof",
    "captured_source_pair_is_not_complete_compiler_consumed_input_closure",
    "no_physical_stack_high_water_clock_voltage_or_device_qualification",
)


class PracticalRecord(TypedDict):
    schema: int
    gate: str
    status: str
    hardware_qualified: bool
    complete_instruction_proof: bool
    elf_sha256: str
    config_sha256: str
    source_context_sha256: str
    static_ram_bytes: int
    headroom: int
    stack: FrameRecord
    assumptions: list[str]
    limitations: list[str]


def _configuration(elf: InspectableImage, expected: bytes | None) -> bytes:
    sections = [section for _, name, section in elf.section_entries if name == ".dashboard_config"]
    require(len(sections) == 1, "unique linked EAV section required")
    section = sections[0]
    require(12 < section[5] <= 16384, "linked EAV byte bound")
    raw = elf.data[section[4] : section[4] + section[5]]
    require(
        len(raw) == section[5]
        and raw[:4] == b"EAV1"
        and struct.unpack_from("<HH", raw, 4) == (CURRENT_EAV_VERSION, len(raw) - 12)
        and zlib.crc32(raw[:8] + raw[12:]) == struct.unpack_from("<I", raw, 8)[0],
        "linked EAV envelope/checksum mismatch",
    )
    if expected is not None:
        require(type(expected) is bytes and raw == expected, "accepted EAV differs from image")
    linked_configuration(elf, raw)
    return raw


def check(
    elf: InspectableImage, expected_eav: bytes | None = None, source_context: object = None
) -> PracticalRecord:
    from check_memory_layout import geometry, check_boundaries

    context = validate_source_context(source_context, elf)
    static, headroom = geometry(elf)
    inspect_program(elf)
    eav = _configuration(elf, expected_eav)
    stack = stack_check(elf, headroom, source_context=context)
    require(type(stack) is FrameAdmission, "missing conservative frame admission")
    check_boundaries(elf)

    def sha(raw: bytes) -> str:
        return hashlib.sha256(raw).hexdigest()

    result: PracticalRecord = dict(
        schema=3,
        gate="practical_image",
        status="STATIC_IMAGE_CHECKS_PASS",
        hardware_qualified=False,
        complete_instruction_proof=False,
        elf_sha256=sha(elf.data),
        config_sha256=sha(eav),
        source_context_sha256=sha(context.to_bytes()),
        static_ram_bytes=static,
        headroom=headroom,
        stack=stack.to_record(),
        assumptions=list(ASSUMPTIONS),
        limitations=list(LIMITATIONS),
    )
    require(
        len(json.dumps(result, sort_keys=True).encode()) <= 65536, "practical image result bound"
    )
    print(
        "STATIC_IMAGE_CHECKS_PASS; conservative named stack paths; complete instruction proof=false; hardware_qualified=false"
    )
    return result


def read_result(
    record: object,
    elf: InspectableImage,
    expected_eav: bytes | None = None,
    source_context: object = None,
) -> PracticalRecord:
    record = object_fields(record, "practical image requires its distinct schema3 receipt")
    require(
        type(record) is dict
        and record.get("gate") == "practical_image"
        and type(record.get("schema")) is int
        and record["schema"] == 3,
        "practical image requires its distinct schema3 receipt",
    )
    # JSON consumers must not mix diagnostic lines into their output.
    with redirect_stdout(io.StringIO()):
        wanted = check(elf, expected_eav, source_context)
    # Canonical JSON preserves bool/int distinctions at every nesting level.
    require(
        json.dumps(record, sort_keys=True, separators=(",", ":"), allow_nan=False)
        == json.dumps(wanted, sort_keys=True, separators=(",", ":"), allow_nan=False),
        "practical image result fields, assumptions or current identity differ",
    )
    return wanted
