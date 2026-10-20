"""Closed decoded ChannelSource bodies and contextual transports.

Literal/coverage results bind current structure only. The aggregate adapter
must additionally supply actual source, typed frames, partial payloads and
normal/error/None effects before any complete Source is consumed.
"""

import re
from collections.abc import Sequence
from typing import Never, NotRequired, TypedDict
from config_body_types import BodyRecord, InstructionMap
from config_source_contracts import GRAMMARS, ROLES
from config_widget_children import _outline as _widget_outline, _function as _widget_function
from config_regions import CoverageRecord, Region, Transfer, audit_coverage, _control
from config_owner_entry import _direct_target
from config_finite import _complete_body
from config_stack import DiagnosticObjects, canonical_symbol
from validation import require
from config_structural import _structural


CHANNEL_SOURCE = ROLES["ChannelSource"]
_OUTLINES = {v: k for k, v in ROLES.items() if v.startswith("@outline:")}
ACCESS_TARGET_OFFSETS = (314, 84, 182, 226, 126, 252)


class SourceFields(TypedDict):
    access_targets: NotRequired[tuple[int, ...]]
    guarded_access_index_domain: NotRequired[tuple[int, int]]
    requires_same_source_successful_varint_argument: NotRequired[bool]
    original_return_outline_remaining_saves: NotRequired[int]


class SourceBodyRecord(BodyRecord, SourceFields):
    pass


class SourceCoverageRecord(SourceBodyRecord):
    coverage: CoverageRecord


def _function(objects: DiagnosticObjects, address: int, role: str) -> tuple[int, int, str]:
    extent, frame, name = _widget_function(objects, address, role)
    if role == CHANNEL_SOURCE:
        require((extent, frame) == (754, 72), "Source current aggregate extent/frame differs")
    return extent, frame, name


def _outline(
    objects: DiagnosticObjects,
    name: str,
    rows: InstructionMap,
    role: str,
    ancestry: tuple[int, ...] = (),
) -> BodyRecord:
    if role in _OUTLINES:
        return source_body(objects, name, rows, role, ancestry)
    return _widget_outline(objects, name, rows, role, ancestry)


def source_body(
    objects: DiagnosticObjects,
    name: str,
    rows: InstructionMap,
    outline_role: str | None = None,
    ancestry: tuple[int, ...] = (),
) -> SourceBodyRecord:
    """Exact finite family; an override can only identify a named outline."""
    role = outline_role or canonical_symbol(name)
    if outline_role is not None:
        require(
            role in _OUTLINES and re.fullmatch(r"OUTLINED_FUNCTION_[0-9]+", name),
            "Source outline borrows an ordinary or unreviewed identity",
        )
        shape = _OUTLINES[role]
    else:
        require(role == CHANNEL_SOURCE, "unreviewed Source ordinary body")
        shape = "ChannelSource"

    def no_object(*args: object) -> Never:
        raise AssertionError("unexpected Source child readonly object")

    base = _complete_body(
        objects,
        name,
        rows,
        role,
        shape,
        GRAMMARS[shape],
        no_object,
        _outline,
        ancestry,
        function_consumer=_function,
    )
    extra: SourceFields = {}
    proof: SourceBodyRecord = {**base, **extra}
    require(not proof["indirect_sites"], "Source contains an unreviewed executable callback")
    if shape == "ChannelSource":
        start = proof["function"]
        table = objects.read_only(start + 78, 6)
        targets = tuple(start + 78 + 2 * v for v in table)
        require(
            targets == tuple(start + off for off in ACCESS_TARGET_OFFSETS)
            and all(at in rows for at in targets),
            "Source Access table/typed arm targets differ",
        )
        proof["access_targets"] = targets
        proof["guarded_access_index_domain"] = (0, 5)
        proof["requires_same_source_successful_varint_argument"] = True
        proof["original_return_outline_remaining_saves"] = 36
    return proof


@_structural(2)
def source_coverage(
    objects: DiagnosticObjects, name: str, rows: InstructionMap, outline_role: str | None = None
) -> SourceCoverageRecord:
    proof = source_body(objects, name, rows, outline_role)
    start = proof["function"]
    contract = GRAMMARS[proof["shape"]]
    calls = {
        start + off: (operation, target, role, frame)
        for off, operation, target, role, frame in proof["ordinary_edges"]
    }
    transfers = []
    for off, width, text in contract["instructions"]:
        at = start + off
        row = rows[at]
        family, conditional = _control(row)
        if family is None:
            continue
        if at in calls:
            op, target, role, frame = calls[at]
            transfers.append(
                Transfer(
                    at,
                    row.mnemonic,
                    "call" if family == "bl" else "tail",
                    target,
                    "complete Source child; caller argument/effects still required",
                    role,
                )
            )
        elif family == "tbb":
            require(
                proof["shape"] == "ChannelSource" and off == 74,
                "unclassified Source computed transfer",
            )
            transfers.append(
                Transfer(
                    at,
                    row.mnemonic,
                    "table",
                    proof["access_targets"],
                    "six exact Access targets; successful selector provenance remains required",
                )
            )
        elif family == "return" or family == "bx" and row.operands == "lr":
            transfers.append(
                Transfer(
                    at,
                    row.mnemonic,
                    "conditional_return" if conditional else "return",
                    None,
                    "actual original save/return relation still required",
                )
            )
        else:
            require(family in ("b", "cbz", "cbnz"), "unclassified Source implicit PC effect")
            target = _direct_target(row, family)
            transfers.append(
                Transfer(
                    at,
                    row.mnemonic,
                    "loop" if target <= at else "branch",
                    target,
                    "Source arm/error convergence; complete saved-return relation remains required",
                )
            )
    context = (start, "decoded Source literal coverage")
    regions = []
    for low, high, kind in contract["mapping"]:
        if kind == "d":
            regions.append(
                Region(
                    context,
                    "data",
                    start + low,
                    start + high,
                    (),
                    ((start + low, objects.read_only(start + low, high - low)),),
                    (),
                )
            )
        else:
            regions.append(
                Region(
                    context,
                    "helper",
                    start + low,
                    start + high,
                    tuple(
                        start + off
                        for off, width, text in contract["instructions"]
                        if low <= off < high
                    ),
                    (),
                    tuple(t for t in transfers if start + low <= t.site < start + high),
                )
            )

    def child(objects: DiagnosticObjects, address: int, role: str) -> tuple[int, int, str]:
        if not role.startswith("@outline:"):
            return _function(objects, address, role)
        names = [n for n, e in objects.elf.functions.items() if e[0] & ~1 == address]
        require(len(names) == 1, "ambiguous Source outline")
        p = _outline(objects, names[0], rows, role)
        return p["extent"], p["frame"], names[0]

    coverage = audit_coverage(objects, name, rows, context, regions, _function_consumer=child)
    return {**proof, "coverage": coverage}


def source_children(
    objects: DiagnosticObjects, rows: InstructionMap, parents: Sequence[BodyRecord]
) -> dict[tuple[int, str], BodyRecord]:
    """Current complete transitive bodies, with no unknown child waiver."""
    from config_widget_children import _name, widget_body
    from config_widget_contracts import ROLES as WIDGET
    from config_helper_contracts import HELPER_SEMANTIC_ROLES
    from config_helpers import helper_coverage
    from config_logger import logger_body

    proofs: dict[tuple[int, str], BodyRecord] = {}
    pending = [edge for p in parents for edge in p["ordinary_edges"]]
    while pending:
        off, operation, target, role, frame = pending.pop()
        key = (target, role)
        if key in proofs:
            continue
        if role.startswith("@outline:"):
            names = [n for n, e in objects.elf.functions.items() if e[0] & ~1 == target]
            require(len(names) == 1, "ambiguous Sources child outline")
            proof = _outline(objects, names[0], rows, role)
        elif role == CHANNEL_SOURCE:
            proof = source_coverage(objects, _name(objects, role, target), rows)
        elif role in HELPER_SEMANTIC_ROLES.values():
            proof = helper_coverage(
                objects, _name(objects, role, target), rows, (target, "decoded Source primitive")
            )
        elif role in (WIDGET["color"], WIDGET["warning_colors"]):
            proof = widget_body(objects, _name(objects, role, target), rows)
        elif role in ("__aeabi_memcpy", "_RNvNtCs_17compiler_builtins3mem6memcpy"):
            proof = logger_body(objects, _name(objects, role, target), rows)
        else:
            raise AssertionError("unclassified Sources child")
        require(proof["frame"] == frame, "Sources child changes its current activation frame")
        proofs[key] = proof
        pending.extend(proof["ordinary_edges"])
    return proofs


def source_return_profiles() -> dict[str, set[int]]:
    from config_decoded_views import _views_return_profiles
    from config_widget_contracts import ROLES as WIDGET

    profiles = _views_return_profiles()
    profiles.update(
        {
            CHANNEL_SOURCE: {0, 4, 8, 362},
            "seq_u16": {4, 14},
            "deserialize_u16": {0, 4},
            "restore_r11_low20": {0, 4},
            "seq_decrement_tail_u16": set(),
            "seq_f32": {0, 40, 46, 56},
            "seq_f32_setup_flags": set(),
            "deserialize_seed_f32": {16, 26},
            WIDGET["color"]: {4, 14},
            WIDGET["warning_colors"]: {0, 98},
        }
    )
    profiles.update({role: set() for role in WIDGET.values() if role.startswith("@outline:")})
    profiles.update({role: set() for role in _OUTLINES})
    profiles[ROLES["OUTLINED_FUNCTION_75"]] = {0, 4}
    return profiles
