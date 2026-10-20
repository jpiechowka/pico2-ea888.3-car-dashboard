"""SizeZ validator Result/carrier boundary within an unresolved enclosing graph.

Complete finite entry/output/restore windows establish the physical ABI, not
typed Config ownership or the omitted visitor's scalar/control effects. The
enclosing producer must discharge every returned prerequisite before it can
produce a qualified stack result. No policy is minted by this component.
"""

from collections.abc import Mapping
from typing import Literal, TypedDict
from config_body_types import BodyRecord, InstructionMap, Callback
from config_stack import DiagnosticObjects
from config_regions import View
from config_default import Writes
from config_refill_children import OpaqueCopyResult
from config_roots import WindowRecord
from config_validator_contract import ValidatorObject
from llvm_stack import Instruction
from dataclasses import replace
import re
from config_default import _ingress
from config_entry import CONFIG, entry_object
from config_finite import _literal_body
from config_headers import ROOT
from config_helpers import _helper_function
from config_refill_children import opaque_copy_interface
from config_regions import Storage, borrow, require_disjoint, require_writes
from config_roots import root_window
from config_stack import typed_function, canonical_symbol, transfer_family
from config_validator_access import ValidatorContext, VALIDATOR_ROLE
from config_validator_contract import CONTRACTS
from validation import require


class EarlyCarrier(TypedDict):
    result: str
    first: str
    spill_initialized: bool


class LateCarrier(TypedDict):
    result: str
    first: str
    resets: tuple[int, int]


class ErrorAlternative(TypedDict):
    kind: Literal["Err"]
    requires_completed_first_Some_tags: tuple[int, int]
    opaque_payload: tuple[int, int]
    writes: Writes
    tag_after_payload: bool


class SuccessAlternative(TypedDict):
    kind: Literal["Ok"]
    tag: int
    requires_valid_visitor_stoichiometric_s0: bool
    writes: Writes
    inactive_or_padding: tuple[tuple[int, int], ...]
    tag_after_payload: bool


class NormalReturn(TypedDict):
    restored_bytes: int
    saved_return: int
    return_to: int
    final_sp: int
    preserved: tuple[str, ...]
    r0: str


class SaveLayout(TypedDict):
    vfp: tuple[int, int]
    discarded_duplicate_r7: tuple[int, int]
    registers: dict[str, int]


class ValidatorResult(TypedDict):
    proofs: dict[str, BodyRecord]
    caller: BodyRecord
    context: ValidatorContext
    first: View
    capture: View
    spill: View
    local_writes: Writes
    error_copy: OpaqueCopyResult
    none_failure: WindowRecord
    early_carrier: EarlyCarrier
    late_carrier: LateCarrier
    alternatives: tuple[ErrorAlternative, SuccessAlternative]
    normal_return: NormalReturn
    save_layout: SaveLayout
    requires_intact_complete_save_area_before_normal_restore: bool
    requires_complete_typed_Config_and_exclusive_Result_owner: bool
    requires_all_visitor_first_capture_spill_valid_save_and_ancestor_preservation: bool
    requires_source_valid_errors_and_fuel_success_not_arbitrary_non2: bool
    requires_early_child_and_all_late_predecessor_effects: bool
    requires_whole_parent_computed_ingress_and_coverage: bool
    requires_diagnostic_default_Formatter_sink_PANICKED_nonreturn_closure: bool
    platform_requirements_unresolved: bool


def _object(
    objects: DiagnosticObjects, address: int, descriptor: ValidatorObject
) -> tuple[Callback, ...]:
    kind = descriptor["kind"]
    if kind == "config":
        return entry_object(objects, address, dict(kind="entry_static", role=CONFIG))
    if kind == "hold_range":
        require(
            address % 2 == 0 and objects.read_only(address, 6) == bytes.fromhex("9001dc050000"),
            "validator hold range loses its aligned u16 reference/fields",
        )
    else:
        require(
            descriptor["kind"] == "first_literal" and address == descriptor["value"],
            "validator first-error constructor literal differs",
        )
    return ()


def _match(
    objects: DiagnosticObjects,
    name: str,
    rows: InstructionMap,
    label: str,
    start: int,
    bindings: Mapping[str, int],
) -> BodyRecord:
    contract = CONTRACTS[label].copy()
    operations = []
    for offset, width, text in contract["instructions"]:
        for key, value in bindings.items():
            text = text.replace("@" + key, hex(value))
        operations.append((offset, width, text))
    contract["instructions"] = operations
    clean = {
        a: replace(r, operands=r.operands.split("@", 1)[0].split("<", 1)[0].strip())
        for a, r in rows.items()
    }

    def outline(
        objects: DiagnosticObjects,
        child: str,
        rows: InstructionMap,
        role: str,
        ancestry: tuple[int, ...],
    ) -> BodyRecord:
        require(
            role in ("@outline:first_tail", "@outline:validator_restore")
            and re.fullmatch(r"OUTLINED_FUNCTION_[0-9]+", child),
            "unreviewed validator outline role",
        )
        label = role.partition(":")[2]
        return _literal_body(
            objects, child, clean, role, label, CONTRACTS[label], _object, outline, ancestry
        )

    return _literal_body(
        objects,
        name,
        clean,
        canonical_symbol(name),
        label,
        contract,
        _object,
        outline,
        window=start,
        function_consumer=_helper_function,
    )


def _target(row: Instruction) -> int:
    match = re.search(r"(?<![#\w])0x([0-9a-f]+)(?:\s|$)", row.operands.split("@", 1)[0])
    if match is None:
        raise AssertionError("unresolved validator direct transfer")
    return int(match[1], 16)


def validator_result_boundary(
    objects: DiagnosticObjects,
    name: str,
    rows: InstructionMap,
    context: ValidatorContext,
    call: int,
) -> ValidatorResult:
    """Preserve supplied source context through actual before/after-call ABI.

    The first Some and success s0 are typed incoming visitor alternatives,
    explicitly unresolved here. Early R is r5; Q60 does not exist on those
    branches. The common tag is paired with Err copy or Ok f32 separately.
    """
    require(
        isinstance(context, ValidatorContext) and context.mode == "sizez" and type(call) is int,
        "Result boundary lacks its exact SizeZ caller/output context",
    )
    parent = objects.elf.functions[name][0] & ~1
    extent, frame, _ = typed_function(objects, parent, VALIDATOR_ROLE)
    require((extent, frame) == (5048, 320), "validator Result boundary has another current ABI")
    owners = [
        n
        for n, e in objects.elf.functions.items()
        if e[0] & ~1 <= call < (e[0] & ~1) + e[1] and canonical_symbol(n) == ROOT
    ]
    require(len(owners) == 1, "validator caller is not the current typed loader")
    caller = owners[0]
    caller_start = objects.elf.functions[caller][0] & ~1
    caller_extent, caller_frame, _ = typed_function(objects, caller_start, ROOT)
    require(
        caller_frame == 456
        and call in rows
        and rows[call].mnemonic == "bl"
        and _target(rows[call]) == parent,
        "validator loses actual caller frame/BL entry",
    )
    label, back = (
        ("default_call", 10) if context.phase == "default_validation" else ("decoded_call", 2)
    )
    caller_proof = _match(objects, caller, rows, label, call - back, {})
    _ingress(rows, caller_start, caller_extent, call - back, call + 4)
    q = context.frame
    result = context.result
    config = context.config
    # The supplied context still owes actual private typed source provenance.
    require_disjoint((q, context.caller, config.storage))
    save_guards = tuple(
        v
        for v in context.protected
        if v.space == q.space and v.start == q.start + 216 and v.extent == 104 and not v.writable
    )
    require(len(save_guards) == 1, "validator has no unique complete current save-area guard")
    ancestors = tuple(v for v in context.protected if v is not save_guards[0])
    for protected in ancestors:
        require_disjoint((q, protected))
    bindings = dict(
        version=parent + 304,
        catalogue=parent + 310,
        hold=parent + 358,
        error=parent + 326,
        none=parent + 928,
        tag=parent + 346,
    )
    offsets = {
        "entry": 0,
        "early": 304,
        "error": 326,
        "hold": 358,
        "late_reset": 396,
        "success": 4478,
    }
    proofs = {
        label: _match(objects, name, rows, label, parent + offset, bindings)
        for label, offset in offsets.items()
    }
    require(
        proofs["entry"]["objects"][0] == config.start,
        "specialized validator receiver changes private Config",
    )
    # Each allowed direct predecessor retains a distinct source-state
    # obligation. This is not an all-ingress/indirect visitor proof.
    allowed = {
        304: {44},
        310: {56},
        314: {308},
        326: {370, 400, 4488},
        346: {4498},
        358: {74},
        396: {
            392,
            0x1EE9C - 0x1EA20,
            0x1EEAE - 0x1EA20,
            0x1F79E - 0x1EA20,
            0x1F7AA - 0x1EA20,
            0x1F9E2 - 0x1EA20,
            0x1FA40 - 0x1EA20,
            0x1FB22 - 0x1EA20,
            0x1FB56 - 0x1EA20,
            0x1FD30 - 0x1EA20,
        },
        928: {332},
    }
    intervals = tuple(
        (parent + o, parent + o + CONTRACTS[label]["extent"]) for label, o in offsets.items()
    ) + ((parent + 928, parent + 942),)
    for at, row in rows.items():
        family = transfer_family(row.mnemonic.removesuffix(".w"))
        if family not in ("b", "bl", "cbz", "cbnz"):
            continue
        target = _target(row)
        if not any(lo <= target < hi for lo, hi in intervals):
            continue
        if target == parent:
            require(
                family == "bl" and caller_start <= at < caller_start + caller_extent,
                "foreign validator entry context",
            )
            continue
        relative = target - parent
        require(
            at - parent in allowed.get(relative, set()),
            "foreign validator Result/carrier phase ingress",
        )
    none = root_window(objects, name, rows, parent + 938)
    require(
        (none["location_role"], none["window"], none["extent"])
        == ("first_invalid", parent + 928, 14),
        "validator None invariant loses its exact nonreturn diagnostic root",
    )
    locals_ = borrow(q, 0, 216, q.phase, writable=True)
    local_ranges = ((76, 2), (88, 4), (95, 1), (60, 4), (82, 4), (86, 1), (80, 1))
    local_writes = tuple((locals_, o, n) for o, n in local_ranges)
    require_writes((locals_,), local_writes, context.protected)
    first = borrow(q, 76, 12, q.phase)
    capture = borrow(q, 88, 4, q.phase)
    spill = borrow(q, 60, 4, q.phase)
    complete_locals = Storage(
        q.space, q.identity + ":live Result-boundary locals", q.start, 216, 8, q.phase, False
    )
    require_writes((result,), ((result, 0, 12),), context.protected + (complete_locals,))
    copy_edge = proofs["error"]["ordinary_edges"][0]
    require(
        copy_edge[0] == 16 and copy_edge[3] == "__aeabi_memcpy",
        "validator error copy changes actual child/count boundary",
    )
    copied = opaque_copy_interface(
        objects,
        rows,
        copy_edge[2],
        borrow(q, 78, 10, q.phase),
        borrow(result.storage, result.offset + 2, 10, q.phase, writable=True),
        10,
        q,
        context.protected + (complete_locals,),
    )
    restore_edge = proofs["error"]["ordinary_edges"][1]
    require(
        restore_edge[0] == 28
        and restore_edge[3] == "@outline:validator_restore"
        and restore_edge[4] == 0,
        "validator normal restore acquires another stack/return ABI",
    )
    return dict(
        proofs=proofs,
        caller=caller_proof,
        context=context,
        first=first,
        capture=capture,
        spill=spill,
        local_writes=local_writes,
        error_copy=copied,
        none_failure=none,
        early_carrier=dict(result="r5", first="r6=Q76", spill_initialized=False),
        late_carrier=dict(
            result="reload Q60", first="reset Q76", resets=(parent + 396, parent + 4482)
        ),
        alternatives=(
            dict(
                kind="Err",
                requires_completed_first_Some_tags=(0, 1),
                opaque_payload=(2, 12),
                writes=((result, 2, 10), (result, 0, 2)),
                tag_after_payload=True,
            ),
            dict(
                kind="Ok",
                tag=2,
                requires_valid_visitor_stoichiometric_s0=True,
                writes=((result, 4, 4), (result, 0, 2)),
                inactive_or_padding=((2, 4), (8, 12)),
                tag_after_payload=True,
            ),
        ),
        normal_return=dict(
            restored_bytes=320,
            saved_return=q.start + 316,
            return_to=call + 4,
            final_sp=context.caller.start,
            preserved=("r4", "r5", "r6", "r7", "r8", "r9", "r10", "r11", "d8-d15", "sp"),
            r0="not a Result pointer ABI",
        ),
        save_layout=dict(
            vfp=(216, 280),
            discarded_duplicate_r7=(280, 284),
            registers={
                "r8": 284,
                "r9": 288,
                "r10": 292,
                "r11": 296,
                "r4": 300,
                "r5": 304,
                "r6": 308,
                "r7": 312,
                "return": 316,
            },
        ),
        requires_intact_complete_save_area_before_normal_restore=True,
        requires_complete_typed_Config_and_exclusive_Result_owner=True,
        requires_all_visitor_first_capture_spill_valid_save_and_ancestor_preservation=True,
        requires_source_valid_errors_and_fuel_success_not_arbitrary_non2=True,
        requires_early_child_and_all_late_predecessor_effects=True,
        requires_whole_parent_computed_ingress_and_coverage=True,
        requires_diagnostic_default_Formatter_sink_PANICKED_nonreturn_closure=True,
        platform_requirements_unresolved=True,
    )
