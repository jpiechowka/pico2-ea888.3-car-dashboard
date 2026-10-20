"""Finite controls source arguments, selectors and first-error effects.

The current loop/child sequences establish machine transport and bounded
effects. Source-valid Config/enum input, source loop entry and predecessor
states remain explicit requirements of the enclosing abstract producer.
This component cannot issue a stack, policy or image result.
"""

from typing import Never, TypedDict
from config_body_types import BodyRecord, Callback, InstructionMap
from config_stack import DiagnosticObjects
from config_validator_access import ValidatorContext
from config_validator_pointers import PointerRegions, PoolEvidence
from config_regions import Storage
from config_default import Writes
from dataclasses import replace
import re
from config_finite import _literal_body
from config_refill_children import _readonly
from config_regions import borrow, require_disjoint, require_writes
from config_stack import canonical_symbol, typed_function, _typed_function_extent, transfer_family
from config_validator_control_contracts import REGIONS, CHILDREN, SOURCE_INGRESS
from config_validator_pointers import validator_pointer_regions, _source_contract
from validation import require


class ControlBody(BodyRecord, total=False):
    pools: tuple[PoolEvidence, ...]
    source_action_indices: tuple[int, ...]
    source_page_indices: tuple[int, ...]
    padding_index_excluded: int
    returns_source_bool: bool
    clobbers: tuple[str, ...]
    writes: Writes
    requires_actual_typed_action_and_page_pair: bool
    owned_frame: Storage
    save_bytes: int
    parent_SP_required: bool
    source_error_bytes: tuple[int, ...]
    code_map: tuple[int, ...]
    source_variant_domain: str
    requires_paired_variant_page_button_hold_payload: bool
    preserves: tuple[str, ...]
    inactive_first_bytes: tuple[tuple[int, int], ...]
    returns: int


class TypedTable(TypedDict):
    site: int
    input_domain: tuple[int, ...]
    targets: tuple[int, ...]
    requires_actual_source_typed_selector: bool


class ControlLoops(TypedDict):
    pages: tuple[int, ...]
    buttons: tuple[int, ...]
    tap_hold: tuple[tuple[int, bool], ...]
    rotation: tuple[int, ...]


class ValidatorControls(TypedDict):
    pointer: PointerRegions
    regions: dict[str, ControlBody]
    children: dict[str, ControlBody]
    capture_cells: dict[str, int]
    typed_tables: tuple[TypedTable, ...]
    loops: ControlLoops
    child_stack_alternatives: tuple[tuple[int, ...], ...]
    capture_calls: tuple[int, ...]
    shared_capture_arguments: dict[str, str]
    requires_actual_private_Config_source_and_loop_entry: bool
    requires_paired_page_button_tap_hold_and_action_domains: bool
    requires_edges_occupied_and_control_staging_retirement: bool
    requires_all_other_capture_callers_and_child_saved_carrier_preservation: bool
    requires_whole_parent_control_coverage_and_platform_policy: bool


def _control_body(proof: BodyRecord) -> ControlBody:
    result: ControlBody = {
        "shape": proof["shape"],
        "role": proof["role"],
        "function": proof["function"],
        "extent": proof["extent"],
        "frame": proof["frame"],
        "objects": proof["objects"],
        "callbacks": proof["callbacks"],
        "ordinary_edges": proof["ordinary_edges"],
        "indirect_sites": proof["indirect_sites"],
        "requires_parent_outline_abi": proof["requires_parent_outline_abi"],
    }
    if "caller_extent" in proof:
        result["caller_extent"] = proof["caller_extent"]
    return result


def _object(objects: DiagnosticObjects, address: int, descriptor: object) -> tuple[Callback, ...]:
    require(
        descriptor == {"kind": "control_error_code_map", "hex": "05070707070303"},
        "unreviewed controls readonly role",
    )
    if not isinstance(descriptor, dict) or not isinstance(descriptor.get("hex"), str):
        raise AssertionError("unreviewed controls readonly role")
    require(
        _readonly(objects, address, 7, 1) == bytes.fromhex(descriptor["hex"]),
        "controls typed ErrorCode map differs",
    )
    return ()


def _child(
    objects: DiagnosticObjects,
    name: str,
    rows: InstructionMap,
    label: str,
    children: dict[str, ControlBody],
) -> ControlBody:
    require(label in CHILDREN, "unreviewed controls child")
    contract = CHILDREN[label]
    role = contract["role"]
    start = objects.elf.functions[name][0] & ~1

    def function(objects: DiagnosticObjects, target: int, raw_role: str) -> tuple[int, int, str]:
        if label == "zero_to_bool" and target == start:
            require(
                re.fullmatch(r"OUTLINED_FUNCTION_[0-9]+", name),
                "controls outline borrows ordinary identity",
            )
            extent, actual = _typed_function_extent(objects, target, raw_role)
            require(
                (extent, objects.sizes.get(target, 0)) == (8, 0),
                "controls zero-growth outline frame differs",
            )
            return extent, 0, actual
        return typed_function(objects, target, raw_role)

    def outline(
        objects: DiagnosticObjects,
        child: str,
        records: InstructionMap,
        raw_role: str,
        ancestry: tuple[int, ...],
    ) -> ControlBody:
        require(
            label == "action_allowed" and raw_role == "@outline:zero_to_bool" and not ancestry[1:],
            "unsupported controls nested outline",
        )
        return _child(objects, child, records, "zero_to_bool", children)

    proof = _literal_body(
        objects, name, rows, role, label, contract, _object, outline, function_consumer=function
    )
    require(not proof["indirect_sites"], "controls child gains an indirect callback")
    expected = ((50, "bl", "@outline:zero_to_bool", 0),) if label == "action_allowed" else ()
    require(
        tuple((o, op, role, frame) for o, op, target, role, frame in proof["ordinary_edges"])
        == expected,
        "controls child gains another activation or return interface",
    )
    result = _control_body(proof)
    children[label] = result
    return result


def validator_controls(
    objects: DiagnosticObjects,
    name: str,
    instructions: InstructionMap,
    context: ValidatorContext,
    call: int,
) -> ValidatorControls:
    pointer = validator_pointer_regions(objects, name, instructions, context, call)
    parent = objects.elf.functions[name][0] & ~1
    q = context.frame
    rows = {
        a: replace(r, operands=r.operands.split("@", 1)[0].split("<", 1)[0].strip())
        for a, r in instructions.items()
    }
    children: dict[str, ControlBody] = {}
    proofs: dict[str, ControlBody] = {}
    for item in REGIONS:
        start, contract, pools = _source_contract(objects, name, rows, parent, context.config, item)

        def unexpected(*args: object) -> Never:
            raise AssertionError("unsupported source controls outline")

        proof = _literal_body(
            objects,
            name,
            rows,
            canonical_symbol(name),
            item["label"],
            contract,
            _object,
            unexpected,
            window=start,
        )
        for offset, operation, target, role, frame in proof["ordinary_edges"]:
            labels = [k for k, v in CHILDREN.items() if v["role"] == role]
            require(len(labels) == 1, "unreviewed ordinary controls child")
            child = typed_function(objects, target, role)[2]
            _child(objects, child, rows, labels[0], children)
        result = _control_body(proof)
        result["pools"] = pools
        proofs[item["label"]] = result
    require(set(children) == set(CHILDREN), "controls source omits a complete child")
    intervals = tuple((parent + x["offset"], parent + x["offset"] + x["extent"]) for x in REGIONS)
    for at, row in rows.items():
        if transfer_family(row.mnemonic.removesuffix(".w")) not in ("b", "bl", "cbz", "cbnz"):
            continue
        branch_target = re.search(r"(?<![#\w])0x([0-9a-f]+)(?:\s|$)", row.operands)
        if branch_target is None:
            continue
        destination = int(branch_target[1], 16)
        if not any(lo <= destination < hi for lo, hi in intervals) or any(
            lo <= at < hi for lo, hi in intervals
        ):
            continue
        require(
            (at - parent, row.mnemonic, destination - parent) in SOURCE_INGRESS,
            "foreign direct ingress into controls source arguments",
        )
    # Both emitted tables have fixed padding beyond the source-valid domain.
    # UXTB/TBB alone does not establish these enum domains; their actual source
    # origin and all predecessor states are owed by the enclosing producer.
    action = children["action_allowed"]
    capture = children["control_error_capture"]
    require(
        objects.read_only(action["function"] + 12, 20).hex()
        == "1515151515150a0a0a0a0a0a0c0c0c0f0f0f0a00",
        "controls action table differs",
    )
    action.update(
        {
            "source_action_indices": tuple(range(19)),
            "source_page_indices": (0, 1, 2),
            "padding_index_excluded": 19,
            "returns_source_bool": True,
            "clobbers": ("r0", "r1", "r2", "lr", "flags"),
            "writes": (),
            "requires_actual_typed_action_and_page_pair": True,
        }
    )
    tables: tuple[TypedTable, ...] = (
        dict(
            site=action["function"] + 8,
            input_domain=tuple(range(19)),
            targets=tuple(
                action["function"] + 12 + 2 * v
                for v in objects.read_only(action["function"] + 12, 19)
            ),
            requires_actual_source_typed_selector=True,
        ),
        dict(
            site=parent + 272,
            input_domain=(0, 1, 2, 3),
            targets=tuple(parent + 276 + 2 * v for v in objects.read_only(parent + 276, 4)),
            requires_actual_source_typed_selector=True,
        ),
    )
    first = borrow(q, 76, 12, q.phase, writable=True)
    valid = borrow(q, 95, 1, q.phase, writable=True)
    guarded = context.protected + tuple(
        Storage(q.space, q.identity + ":" + label, q.start + lo, n, 1, q.phase, False)
        for label, lo, n in (("Result spill", 60, 4), ("first capture", 88, 4))
    )
    writes = tuple(
        (first, lo, hi - lo) for lo, hi in ((0, 2), (4, 5), (6, 7), (7, 8), (8, 10), (10, 11))
    ) + ((valid, 0, 1),)
    require_writes((first, valid), writes, guarded)
    for label, child_proof in children.items():
        if not child_proof["frame"]:
            continue
        owned = Storage(
            q.space,
            q.identity + ":controls " + label,
            q.start - child_proof["frame"],
            child_proof["frame"],
            8,
            q.phase,
            True,
        )
        for live in (q, context.result.storage, context.config.storage) + context.protected:
            require_disjoint((owned, live))
        child_proof.update(
            {"owned_frame": owned, "save_bytes": child_proof["frame"], "parent_SP_required": True}
        )
    capture.update(
        {
            "source_error_bytes": (0, 1, 2, 3, 5, 6, 7, 8),
            "code_map": (5, 7, 7, 7, 7, 3, 3),
            "source_variant_domain": "typed ControlError: physical byte0 includes IncompatibleButton.hold bool",
            "requires_paired_variant_page_button_hold_payload": True,
            "clobbers": ("r0", "r1", "r3", "lr", "flags"),
            "preserves": ("r2", "r4", "r5", "r6", "r7", "r8", "r9", "r10", "r11", "sp"),
            "writes": writes,
            "inactive_first_bytes": ((2, 4), (5, 6), (11, 12)),
            "returns": 255,
        }
    )
    children["zero_to_bool"].update(
        {"clobbers": ("r0", "lr", "flags"), "writes": (), "parent_SP_required": True}
    )
    return dict(
        pointer=pointer,
        regions=proofs,
        children=children,
        capture_cells=dict(valid=95, first=76, capture=88),
        typed_tables=tables,
        loops=dict(
            pages=(0, 1, 2),
            buttons=(0, 1, 2, 3),
            tap_hold=((0, False), (2, True)),
            rotation=(0, 1, 2, 3),
        ),
        child_stack_alternatives=((8, 0), (16,)),
        capture_calls=(parent + 366, parent + 392),
        shared_capture_arguments=dict(
            r0="Q95", r1="Q88 -> Q76", r2="source-valid packed ControlError"
        ),
        requires_actual_private_Config_source_and_loop_entry=True,
        requires_paired_page_button_tap_hold_and_action_domains=True,
        requires_edges_occupied_and_control_staging_retirement=True,
        requires_all_other_capture_callers_and_child_saved_carrier_preservation=True,
        requires_whole_parent_control_coverage_and_platform_policy=True,
    )
