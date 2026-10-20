"""Complete finite typed payloads; caller/source/default/sink closure is separate."""

import re
import struct
from config_payload_contracts import PAYLOAD_GRAMMARS, PAYLOAD_ROLES, PayloadObject
from config_stack import DiagnosticObjects, canonical_symbol, typed_function
from config_body_types import BodyRecord, Callback, InstructionMap
from config_finite import _complete_body
from validation import require


class PayloadRecord(BodyRecord):
    requires_value_source_and_live_sink: bool
    requires_options: tuple[int, int, int]
    requires_pretty_inventory_exclusion: bool


def payload_object(
    objects: DiagnosticObjects, address: int, descriptor: PayloadObject
) -> tuple[Callback, ...]:
    if descriptor["kind"] == "literal":
        value = bytes.fromhex(descriptor["bytes"])
        require(
            len(value) == descriptor["length"],
            "inconsistent payload literal contract",
        )
        require(
            objects.read_only(address, len(value)) == value,
            "payload label/ordered enum lengths differ",
        )
        return ()
    if descriptor["kind"] == "enum_lengths":
        value = bytes(descriptor["lengths"])
        require(
            objects.read_only(address, len(value)) == value,
            "payload label/ordered enum lengths differ",
        )
        return ()
    if descriptor["kind"] == "enum_pointers":
        require(address % 4 == 0, "unaligned payload enum pointer table")
        labels = [v.encode() for v in descriptor["labels"]]
        words = struct.unpack("<" + "I" * len(labels), objects.read_only(address, 4 * len(labels)))
        for pointer, label in zip(words, labels):
            require(
                objects.read_only(pointer, len(label)) == label,
                "payload ordered enum label differs",
            )
        return ()
    require(
        descriptor["kind"] in ("debug_vtable", "raw_debug_callback")
        and len(descriptor["roles"]) == 1,
        "unreviewed payload object",
    )
    if descriptor["kind"] == "debug_vtable":
        require(address % 4 == 0, "unaligned payload Debug table")
        words = struct.unpack("<4I", objects.read_only(address, 16))
        require(words[:3] == tuple(descriptor["layout"]), "payload Debug table layout differs")
        pointer = words[3]
    else:
        pointer = address
    require(pointer & 1, "payload callback is not Thumb")
    role = descriptor["roles"][0]
    extent, frame, _ = typed_function(objects, pointer & ~1, role)
    return ((pointer & ~1, role, extent, frame),)


def _payload_shape(
    objects: DiagnosticObjects,
    name: str,
    instructions: InstructionMap,
    role: str,
    shape: str,
    ancestry: tuple[int, ...],
) -> PayloadRecord:
    proof = _complete_body(
        objects,
        name,
        instructions,
        role,
        shape,
        PAYLOAD_GRAMMARS[shape],
        payload_object,
        payload_body,
        ancestry,
    )
    return {
        **proof,
        "requires_value_source_and_live_sink": True,
        "requires_options": (0x60000020, 0, 0),
        "requires_pretty_inventory_exclusion": True,
    }


def payload_body(
    objects: DiagnosticObjects,
    name: str,
    instructions: InstructionMap,
    outline_role: str | None = None,
    _ancestry: tuple[int, ...] = (),
) -> PayloadRecord:
    """Select exactly one full typed body; never widen clones by demangled name."""
    require(name in objects.elf.functions, "missing payload body")
    start = objects.elf.functions[name][0] & ~1
    require(start not in _ancestry, "payload outline activation cycle")
    role = outline_role or canonical_symbol(name)
    require(
        not outline_role or re.fullmatch(r"OUTLINED_FUNCTION_[0-9]+", name),
        "payload outline origin differs",
    )
    require(role in PAYLOAD_ROLES, "unreviewed exact payload receiver/context")
    results = []
    for shape in PAYLOAD_ROLES[role]:
        try:
            results.append(_payload_shape(objects, name, instructions, role, shape, _ancestry))
        except AssertionError:
            continue
    require(len(results) == 1, "missing, changed or ambiguous complete typed payload shape")
    return results[0]
