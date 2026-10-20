"""Closed Tail ordinary/control and original Visitor return relations.

Literal restore equations are conditional on separately checked protected
save slots and the actual selected source path. They confer no owner,
exceptional resume or permission to execute the loader successor.
"""

from __future__ import annotations

from dataclasses import dataclass
from collections.abc import Sequence
from typing import TYPE_CHECKING, Never, TypedDict
from config_body_types import BodyRecord, InstructionMap
from config_stack import DiagnosticObjects
from config_regions import Storage
from config_abstract import SequenceInterface
from config_finite_returns import ReturnNode, ReturnParent
from config_views_contracts import FallthroughWitness, PendingWitness
from config_decoded_views import (
    Children,
    Successors,
    PendingContexts,
    ControlExclusions,
    _ControlSelection,
)
from llvm_stack import Instruction
from config_tail_source import (
    ControlsSource,
    ControlsEffects,
    SuffixSource,
    SuffixEffects,
    SourceInvocation,
    SourceError,
    SourceCause,
    _is_scalar_call,
    _is_screen_call,
    _is_led_call,
    _cause_after,
)

if TYPE_CHECKING:
    from config_decoded_prefix import _PrefixState
    from config_decoded_tail import _TailPhasesState, _TailEntry
    from config_decoded_sources import _SourcesState, PhaseGraph, RemainingNormal

import re
from config_headers import VISITOR, VISITOR_ROWS
from config_finite import _literal_body
from config_tail_parent_contracts import GRAMMARS
from config_tail_children import _function, _outline, tail_body
from config_tail_contracts import ROLES
from config_widget_children import _name
from config_owner_entry import _direct_target
from validation import require
from config_source_control import _SourceSelection
from config_decoded_views import _control_selection, _selected_successors, _foreign_ingress


class RestoreEquations(TypedDict):
    prologue: BodyRecord
    epilogue: BodyRecord
    restore: BodyRecord
    original_call_site: int
    original_return_PC: int
    return_SP: int
    release_bytes: tuple[int, ...]
    saved_interval: tuple[int, int]
    FP_cells: tuple[tuple[str, int, int], ...]
    high_cells: tuple[tuple[str, int, int], ...]
    low_cells: tuple[tuple[str, int, int], ...]
    original_saved_PC: tuple[int, int, int]
    duplicate_frame_r7_retired: tuple[int, int]
    restored_GPRs: tuple[str, ...]
    restored_FP: tuple[str, ...]
    original_input_r8_restored_from_incoming_save: bool
    architectural_LR_preserved: bool
    return_tokens_are_normalized_even_code_identities: bool
    proves_guarded_parent_execution_or_saved_cell_noninterference: bool
    successor_executed: bool
    visitor_local_borrows_retire_at_original_return: bool


class FunctionIdentity(TypedDict):
    function: int


class RestoreIngress(TypedDict):
    restore: FunctionIdentity
    epilogue: FunctionIdentity


class CarrierReturns(TypedDict):
    parent_calls: tuple[tuple[int, int, str, tuple[str, ...]], ...]
    parent_call_count: int
    high_FP: tuple[tuple[int, int], ...]
    architectural_LR_preserved: bool
    requires_protected_saved_cells_and_actual_parent_lifetimes: bool


class TerminalPartition(TypedDict):
    padding: tuple[int, int]
    pool: tuple[int, int]
    predecessor: tuple[int, int]
    dormant_constructor: tuple[int, int]
    confers_active_state: bool


class RestoreContext(TypedDict):
    caller_role: str
    caller: int
    entry_site: int
    current_context: tuple[tuple[int, bytes, str, str], ...]
    inherited_source_stack_and_return_unresolved: bool
    selected_Visitor_permission: bool


class NormalLineage(TypedDict):
    frontier: int
    path: tuple[int, ...]
    calls: tuple[tuple[int, ReturnNode], ...]
    carriers: tuple[tuple[str, int | None, int], ...]
    captured_epochs: tuple[tuple[int, int, int], ...]
    return_carrier: int
    physical_top_count: int
    logical_remaining_fields: int
    requires_first_only_access_and_full_original_restore: bool


class ErrorLineage(TypedDict):
    source_error: SourceError
    call: SourceInvocation
    origin: SourceCause
    path: tuple[int, ...]
    return_carrier: int | None
    completed_groups: tuple[str, ...]
    LED_Config_writes: tuple[()]
    requires_full_original_restore: bool


class ParentLineage(TypedDict):
    normal: NormalLineage | None
    errors: tuple[ErrorLineage, ...]
    confers_active_state: bool
    original_PC_SP_and_saved_noninterference_pending: bool


class GuardFields(TypedDict):
    entry_index: int
    Source_phase: int
    lineage: ParentLineage
    saved_interval: tuple[int, int]
    protected_calls: tuple[tuple[int, int, tuple[int, int]], ...]
    original_return: tuple[int, ...]


class TailGuard(GuardFields):
    entry: _TailEntry
    Source_normal: RemainingNormal


TailControlRecord = TypedDict(
    "TailControlRecord",
    {
        "selected_instructions": int,
        "successors": Successors,
        "child_returns": tuple[int, ...],
        "return_equations": tuple[tuple[int, tuple[int, ...]], ...],
        "return": RestoreEquations,
        "outside_shared_contexts": PendingContexts,
        "separate_restore_contexts": tuple[RestoreContext, ...],
        "pending_contexts_confer_state": bool,
        "original_saved_bytes_protected_on_each_actual_Tail_phase": tuple[tuple[int, int], ...],
        "stopped_frontiers": tuple[int, ...],
        "successor_executed": bool,
        "exceptional_resume_established": bool,
        "exception_safe_completion": bool,
        "actual_reached_accesses_pending": bool,
        "complete_policy_and_resource_producer_pending": bool,
    },
)


class StructuralData(TypedDict):
    selection: tuple[
        tuple[BodyRecord, ...],
        dict[int, Instruction],
        dict[int, tuple[str, int, str]],
        tuple[int, ...],
        bool,
        Children,
        tuple[tuple[tuple[int, str], BodyRecord, BodyRecord], ...],
    ]
    raw_successors: Successors
    raw_returns: tuple[int, ...]
    nested_returns: tuple[ReturnNode, ...]
    carriers: CarrierReturns
    restore: RestoreEquations
    terminal_partition: TerminalPartition
    outside: PendingContexts
    foreign_restore: tuple[RestoreContext, ...]
    dormant: tuple[tuple[int, int], ...]
    confers_active_state: bool


def _restore_equations(
    objects: DiagnosticObjects,
    rows: InstructionMap,
    context: _PrefixState,
    parents: Sequence[BodyRecord],
    children: Children,
) -> RestoreEquations:
    """Exact authored 1512+56+4+36 equation, never a fresh restore call.

    Tokens name normalized even code identities, not scalar Thumb LR bits.
    All original high/low GPRs and seven FP double registers are restored
    from the actual original96-byte save envelope. The duplicate frame r7
    is skipped; it is not substituted for original r7 in the low save block.
    """
    from config_decoded_prefix import _PrefixState

    require(type(context) is _PrefixState, "Tail restore lacks its actual private prefix context")
    frame = context.headers["visitor_frame"]
    root = context.headers["root"]
    entry = context.headers["visitor"]
    V = entry["proof"]["function"]
    R = root["proof"]["function"]
    P = frame.start + 1608
    require(
        frame.extent == 1608
        and frame.alignment >= 8
        and context.frame.start == P
        and context.frame.extent == 456
        and root["call_site"] == R + 126
        and entry["sequence_state"].header.storage == frame,
        "Tail restore changes the original loader/Visitor activation",
    )
    row = rows.get(R + 126)
    require(
        row is not None
        and row.mnemonic == "bl"
        and len(row.raw) == 4
        and _direct_target(row, "bl") == V
        and objects.read_only(row.address, 4) == row.raw,
        "Tail original return is not paired to the actual loader BL",
    )

    def no_object(*args: object) -> Never:
        raise AssertionError("unexpected Tail original-save object")

    name = _name(objects, VISITOR, V)
    prologue = _literal_body(
        objects,
        name,
        rows,
        VISITOR,
        "tail_original_Visitor_saves",
        dict(extent=16, instructions=VISITOR_ROWS[:5], objects=[], data=[]),
        no_object,
        _outline,
        window=V,
        function_consumer=_function,
    )
    epilogue = _literal_body(
        objects,
        name,
        rows,
        VISITOR,
        "decoded_tail_56",
        GRAMMARS["visitor_56"],
        no_object,
        _outline,
        window=V + 56,
        function_consumer=_function,
    )
    matched = tuple(p for p in parents if p["function"] == V + 56)
    require(
        len(matched) == 1 and matched[0] == epilogue and len(epilogue["ordinary_edges"]) == 1,
        "Tail original Visitor epilogue leaves its current parent literal",
    )
    off, operation, target, role, size = epilogue["ordinary_edges"][0]
    require(
        (off, operation, role, size) == (12, "b.w", ROLES["OUTLINED_FUNCTION_155"], 0),
        "Tail original restore is not the inherited36-byte tail",
    )
    names = tuple(n for n, e in objects.elf.functions.items() if e[0] & ~1 == target)
    require(len(names) == 1, "Tail original restore identity is ambiguous")
    restored = tail_body(objects, names[0], rows, role)
    require(
        restored == children.get((target, role))
        and restored["extent"] == 6
        and restored["frame"] == 0
        and not restored["ordinary_edges"]
        and not restored["indirect_sites"],
        "Tail original restore has another current complete inherited body",
    )
    require(
        any(
            v.space == frame.space
            and v.start == frame.start + 1512
            and v.extent == 96
            and not v.writable
            for v in entry["protected"]
        ),
        "Tail original96-byte saved envelope was not retained",
    )
    fp = tuple(("d" + str(i), frame.start + 1512 + 8 * (i - 8), 8) for i in range(8, 15))
    high = tuple(("r" + str(i), frame.start + 1572 + 4 * (i - 8), 4) for i in range(8, 12))
    low = tuple(("r" + str(i), frame.start + 1588 + 4 * (i - 4), 4) for i in range(4, 8))
    original_PC = (frame.start + 1604, 4, R + 130)
    # Each equality follows a fixed matched instruction. No caller-supplied
    # register map, scalar executor or body hash can set these save offsets.
    require(
        frame.start + 1512 + 56 + 4 == high[0][1]
        and high[-1][1] + 4 == low[0][1]
        and low[-1][1] + 4 == original_PC[0]
        and original_PC[0] + 4 == P,
        "Tail original save/restore partition does not release1608 bytes",
    )
    return dict(
        prologue=prologue,
        epilogue=epilogue,
        restore=restored,
        original_call_site=R + 126,
        original_return_PC=R + 130,
        return_SP=P,
        release_bytes=(1512, 56, 4, 36),
        saved_interval=(frame.start + 1512, P),
        FP_cells=fp,
        high_cells=high,
        low_cells=low,
        original_saved_PC=original_PC,
        duplicate_frame_r7_retired=(frame.start + 1568, 4),
        restored_GPRs=tuple("r" + str(i) for i in range(4, 12)),
        restored_FP=tuple("d" + str(i) for i in range(8, 15)),
        original_input_r8_restored_from_incoming_save=True,
        architectural_LR_preserved=False,
        return_tokens_are_normalized_even_code_identities=True,
        proves_guarded_parent_execution_or_saved_cell_noninterference=False,
        successor_executed=False,
        visitor_local_borrows_retire_at_original_return=True,
    )


@dataclass(frozen=True)
class _TailSelection(_SourceSelection):
    source_selection: _SourceSelection
    source_state: _SourcesState
    Tail_parents: tuple[BodyRecord, ...]
    Tail_children: Children


@dataclass(frozen=True)
class _TailStructuralControl:
    selection: _TailSelection
    raw_successors: Successors
    raw_returns: tuple[int, ...]
    nested_returns: tuple[ReturnNode, ...]
    carrier_returns: CarrierReturns
    restore: RestoreEquations
    terminal_partition: TerminalPartition
    outside_shared_contexts: PendingContexts
    foreign_restore_contexts: tuple[RestoreContext, ...]
    unentered_outgoing: tuple[tuple[int, int], ...]
    confers_active_state: bool = False


def _tail_pending_profile(
    selection: _ControlSelection,
) -> tuple[dict[int, PendingWitness], dict[int, FallthroughWitness]]:
    from config_source_control import SOURCE_FALLTHROUGH, SOURCE_PENDING

    require(
        type(selection) is _TailSelection
        and type(selection.source_selection) is _SourceSelection
        and selection.source_selection is selection.source_state.phases.structural.selection
        and selection.context is selection.source_selection.context
        and selection.Source_parents is selection.source_selection.Source_parents
        and selection.Source_children is selection.source_selection.Source_children,
        "Tail selection lost its exact private Source structure",
    )
    if type(selection) is not _TailSelection:
        raise AssertionError("Tail selection lost its exact private Source structure")
    V = selection.context.headers["visitor"]["proof"]["function"]
    require(
        all(V + site in selection.selected for site in SOURCE_PENDING)
        and V + 6654 in selection.selected
        and V + 4408 not in selection.selected,
        "Tail did not consume its exact Source pending partition",
    )
    return {}, {4408: SOURCE_FALLTHROUGH[4408]}


def _tail_table_targets(selection: _ControlSelection, at: int, row: Instruction) -> tuple[int, ...]:
    from config_source_control import _source_table_targets
    from config_source_children import CHANNEL_SOURCE
    from config_tail_children import ACTION_TARGET_OFFSETS

    if type(selection) is not _TailSelection:
        raise AssertionError("unreviewed Tail computed selection")
    source = tuple(p for p in selection.Source_children.values() if p["role"] == CHANNEL_SOURCE)
    if len(source) == 1 and at == source[0]["function"] + 74:
        targets = _source_table_targets(selection.source_selection, at, row)
        require(
            all(t in selection.selected for t in targets),
            "Tail loses an inherited Source table target",
        )
        return targets
    proofs = tuple(
        p for p in selection.Tail_children.values() if p["role"] == ROLES["ControlAction"]
    )
    require(len(proofs) == 1, "Tail Action table lacks its unique complete child")
    proof = proofs[0]
    start = proof["function"]
    targets = tuple(start + off for off in ACTION_TARGET_OFFSETS)
    require(
        at == start + 48
        and row.mnemonic == "tbb"
        and row.operands == "[pc, r0]"
        and proof["shape"] == "ControlAction"
        and proof.get("action_targets") == targets
        and proof.get("guarded_action_index_domain") == (0, 18)
        and all(t in selection.selected for t in targets),
        "Tail Action computed transfer loses its actual finite19-target structure",
    )
    return targets


def _terminal_partition(
    objects: DiagnosticObjects, rows: InstructionMap, selection: _TailSelection
) -> TerminalPartition:
    from llvm_stack import mapping_ranges
    from config_regions import _control

    V = selection.context.headers["visitor"]["proof"]["function"]
    pad = rows.get(V + 7122)
    previous = rows.get(V + 7120)
    require(
        pad is not None
        and pad.raw == b"\x00\xbf"
        and pad.mnemonic == "nop"
        and not pad.operands
        and objects.read_only(V + 7122, 2) == pad.raw
        and previous is not None
        and previous.mnemonic == "b"
        and len(previous.raw) == 2
        and _direct_target(previous, "b") == V + 6988
        and objects.read_only(previous.address, 2) == previous.raw
        and V + 7118 not in rows,
        "Tail terminal padding loses its exact current unconditional predecessor",
    )
    require(
        V + 7122 not in selection.selected
        and all(V + off not in selection.selected for off in range(7124, 7128)),
        "Tail terminal padding/data became active instructions",
    )
    mapping = mapping_ranges(objects.elf, _name(objects, VISITOR, V))
    require(
        any(k == "t" and lo <= V + 7122 < V + 7124 <= hi for lo, hi, k in mapping)
        and any(k == "d" and lo <= V + 7124 < V + 7128 <= hi for lo, hi, k in mapping),
        "Tail terminal padding and Config pool mapping differ",
    )
    unselected = {V + off for off in (3506, 4402, 4404, 4406, 4408, 7122)}
    current = {at for at in rows if V <= at < V + 7128}
    require(
        current == {at for at in selection.selected if V <= at < V + 7128} | unselected,
        "Tail full Visitor partition leaves another instruction or interior unclassified",
    )
    for at, row in rows.items():
        family, conditional = _control(row)
        if family in ("b", "bl", "cbz", "cbnz"):
            target = _direct_target(row, family)
            require(
                not V + 7122 <= target < V + 7128,
                "Tail foreign or selected ingress enters terminal padding/data",
            )
    return dict(
        padding=(V + 7122, V + 7124),
        pool=(V + 7124, V + 7128),
        predecessor=(V + 7120, V + 6988),
        dormant_constructor=(V + 4402, V + 4410),
        confers_active_state=False,
    )


def _restore_ingress(
    objects: DiagnosticObjects, rows: InstructionMap, restore: RestoreEquations | RestoreIngress
) -> tuple[RestoreContext, ...]:
    """Retain exact separate outside contexts; grant none inherited saves."""
    from config_tail_restore_contracts import RESTORE_CONTEXTS
    from config_stack import canonical_symbol, typed_function
    from config_regions import _control
    from llvm_stack import mapping_ranges

    target = restore["restore"]["function"]
    actual = restore["epilogue"]["function"] + 12
    obligations: list[RestoreContext] = []
    expected = {actual}
    for role, contract in RESTORE_CONTEXTS.items():
        names = tuple(n for n in objects.elf.functions if canonical_symbol(n) == role)
        if not names:
            continue
        require(len(names) == 1, "Tail foreign restore caller identity is ambiguous")
        name = names[0]
        parent = objects.elf.functions[name][0] & ~1
        extent, frame, _ = typed_function(objects, parent, role)
        require(
            (extent, frame) == (contract["extent"], contract["frame"]),
            "Tail separate restore invocation changed caller extent/frame",
        )
        mapping = mapping_ranges(objects.elf, name)
        anchors = []
        for off, width, mnemonic, operands in contract["anchors"]:
            at = parent + off
            row = rows.get(at)
            require(
                row is not None
                and len(row.raw) == width
                and row.mnemonic == mnemonic
                and objects.read_only(at, width) == row.raw
                and any(k == "t" and lo <= at < at + width <= hi for lo, hi, k in mapping),
                "Tail separate restore context has missing/stale current instructions",
            )
            if row is None:
                raise AssertionError(
                    "Tail separate restore context has missing current instructions"
                )
            text = row.operands.split("@", 1)[0].split("<", 1)[0].strip()
            marker = re.search(r"@(restore|parent\+[0-9a-f]+|typed:.*)$", operands)
            if marker:
                match = re.search(r"0x([0-9a-f]+)(?:\s|$)", text)
                if match is None:
                    raise AssertionError("Tail separate restore target differs")
                destination = int(match[1], 16)
                label = marker[0]
                if label == "@restore":
                    require(destination == target, "Tail separate inherited tail target differs")
                elif label.startswith("@parent+"):
                    require(
                        destination == parent + int(label[8:], 16) and destination in rows,
                        "Tail separate context local branch moved into data/interior",
                    )
                else:
                    typed_function(objects, destination, label[7:])
                text = text[: match.start()] + label
            require(text == operands, "Tail separate restore caller context differs")
            anchors.append((at, row.raw, row.mnemonic, row.operands))
        site = parent + contract["site"]
        expected.add(site)
        obligations.append(
            dict(
                caller_role=role,
                caller=parent,
                entry_site=site,
                current_context=tuple(anchors),
                inherited_source_stack_and_return_unresolved=True,
                selected_Visitor_permission=False,
            )
        )
    seen = set()
    for at, row in rows.items():
        family, conditional = _control(row)
        if family not in ("b", "bl", "cbz", "cbnz"):
            continue
        destination = _direct_target(row, family)
        require(
            not target < destination < target + 6,
            "Tail foreign direct ingress enters an inherited restore interior",
        )
        if destination != target:
            continue
        require(
            at in expected and family == "b" and not conditional and row.mnemonic == "b.w",
            "Tail foreign inherited restore entry is not a checked separate tail context",
        )
        seen.add(at)
    require(seen == expected, "Tail inherited restore entry contexts were omitted or retargeted")
    return tuple(obligations)


def _tail_structural(state: _TailPhasesState) -> _TailStructuralControl:
    """Full current selected structure, still no active source/owner state."""
    from config_decoded_tail import _TailPhasesState, _parents
    from config_tail_children import tail_children, tail_return_profiles
    from config_finite_returns import _closed_child_returns
    from config_decoded_widgets import _source_fp_preservation

    require(
        type(state) is _TailPhasesState, "Tail structure lacks the actual private phase producer"
    )
    predecessor = state.predecessor
    source = predecessor.phases.structural.selection
    c = source.context
    objects = state.objects
    rows = state.rows
    V = c.headers["visitor"]["proof"]["function"]
    R = c.headers["root"]["proof"]["function"]
    require(
        type(source) is _SourceSelection
        and state.parents == _parents(objects, rows, V, c.owner.start)
        and state.children == tail_children(objects, rows, state.parents),
        "Tail structure changes current private parents/children",
    )
    combined = dict(source.children)
    lineage = list(source.child_contexts)
    for key, p in state.children.items():
        if key in combined:
            prior = combined[key]
            require(
                {k: v for k, v in prior.items() if k != "coverage"}
                == {k: v for k, v in p.items() if k != "coverage"},
                "Tail shared child has inconsistent complete current body",
            )
            a = prior.get("coverage")
            b = p.get("coverage")
            require(
                a == b
                or isinstance(a, dict)
                and isinstance(b, dict)
                and {k: v for k, v in a.items() if k != "context"}
                == {k: v for k, v in b.items() if k != "context"},
                "Tail shared child has inconsistent current coverage",
            )
            lineage.append((key, prior, p))
        combined[key] = p
    widgets = predecessor.phases.widgets
    parents = widgets.views.parents + widgets.parents + predecessor.phases.parents + state.parents
    base = _control_selection(objects, rows, c, parents, combined)

    def no_object(*args: object) -> Never:
        raise AssertionError("unexpected prefix error-carrier object")

    prefix = _literal_body(
        objects,
        _name(objects, VISITOR, V),
        rows,
        VISITOR,
        "tail_prefix_error_carrier",
        dict(extent=2, instructions=((0, 2, "lsrs r5, r0, #0x8"),), objects=[], data=[]),
        no_object,
        _outline,
        window=V + 54,
        function_consumer=_function,
    )
    proofs = base.proofs + (prefix,)
    selected = {**base.selected, V + 54: rows[V + 54]}
    cuts = frozenset((base.cuts - {V + 54, V + 56, V + 4376}) | {R + 130})
    selection = _TailSelection(
        base.context,
        base.children,
        proofs,
        selected,
        base.edges,
        cuts,
        base.widget,
        source.widgets,
        source.Source_parents,
        source.Source_children,
        tuple(lineage),
        source,
        predecessor,
        state.parents,
        state.children,
    )
    terminal = _terminal_partition(objects, rows, selection)
    successors, returns, dormant = _selected_successors(selection)
    callable_parents: tuple[ReturnParent, ...] = tuple(
        dict(
            function=p["function"],
            ordinary_edges=tuple(e for e in p["ordinary_edges"] if e[1] == "bl"),
        )
        for p in state.parents
        if any(e[1] == "bl" for e in p["ordinary_edges"])
    )
    nested = _closed_child_returns(
        rows,
        callable_parents,
        state.children,
        c.headers["visitor_frame"].start,
        tail_return_profiles(),
        (ROLES["ControlAction"], ROLES["ButtonBinding"], ROLES["LedPattern"]),
        objects=objects,
    )
    sp = c.headers["visitor_frame"].start
    checked = []
    high = tuple("r" + str(i) for i in range(4, 12))
    for callable_parent in callable_parents:
        for off, op, target, role, frame in callable_parent["ordinary_edges"]:
            pc = callable_parent["function"] + off + 4
            matches = tuple(
                v
                for v in nested
                if v["function"] == target
                and v["role"] == role
                and (v["entry_SP"], v["return_PC"], v["return_SP"]) == (sp, pc, sp)
            )
            require(
                len(matches) == 1 and matches[0]["preserved_high_gprs"] == high,
                "Tail current parent call changes a live high-register carrier",
            )
            checked.append((pc - 4, target, role, high))
    require(
        len(checked) == 25
        and len({v[0] for v in checked}) == 25
        and min(v["lowest_SP"] for v in nested) == sp - 152,
        "Tail current parent call/deepest child graph differs",
    )
    carriers: CarrierReturns = dict(
        parent_calls=tuple(checked),
        parent_call_count=25,
        high_FP=_source_fp_preservation(rows, state.children),
        architectural_LR_preserved=False,
        requires_protected_saved_cells_and_actual_parent_lifetimes=True,
    )
    restore = _restore_equations(objects, rows, c, state.parents, state.children)
    outside = _foreign_ingress(objects, rows, selection)
    foreign_restore = _restore_ingress(objects, rows, restore)
    return _TailStructuralControl(
        selection,
        successors,
        returns,
        predecessor.phases.structural.nested_returns + nested,
        carriers,
        restore,
        terminal,
        outside,
        foreign_restore,
        dormant,
    )


# Closed chronological parent paths. Intervening child executions are bound
# to their same current typed original-return nodes, never skipped by a PC
# list. A path is conditional on the paired source outcomes below.
_CONTROLS_PATH = (
    (5630, 5726),
    (6188, 6212),
    (6412, 6440),
    (6478, 6524),
    (5928, 5964),
    (6018, 6118),
)
_HISTORY_PATH = (
    (6118, 6184),
    (6450, 6474),
    (6524, 6548),
    (6552, 6576),
    (6580, 6604),
    (6608, 6630),
    (6634, 6654),
    (6662, 6734),
)
_LED_PATH = (
    (6734, 6844),
    (6858, 6886),
    (6890, 6910),
    (6916, 6936),
    (6942, 6962),
    (6968, 6986),
    (7000, 7116),
)
_HISTORY_SITES = (6144, 6170, 6460, 6534, 6562, 6590, 6616, 6642)
_BOOL_SITES = (6876, 6900, 6926, 6952, 6978)


def _parent_lineage(
    objects: DiagnosticObjects,
    rows: InstructionMap,
    parents: tuple[BodyRecord, ...],
    children: Children,
    returns: Sequence[ReturnNode],
    controls: ControlsSource,
    effects: ControlsEffects,
    suffix: SuffixSource | None,
    suffix_effects: SuffixEffects | None,
    frame: Storage,
) -> ParentLineage:
    """Finite conditional source -> actual parent paths and carrier lifetimes.

    This does not acquire an entry/owner or independently grant active state.
    The actual private producer checks currentness and full physical guards
    before consuming this relation, including every cross-phase association.
    """
    from config_abstract import Span
    from config_tail_source import _parent_inputs, _call_binding, _joined_sequence
    from config_tail_parent_effects import PARENT_WRITES
    from config_finite_returns import _high_definitions
    from config_regions import _control

    V = _parent_inputs(objects, rows, parents, controls["before"], frame)
    require(
        effects["source"] is controls
        and effects["frame"] is frame
        and controls["before"].count == Span(4, 4),
        "Tail parent lineage changes its actual Controls source",
    )
    high = tuple("r" + str(i) for i in range(4, 12))
    call_map = {}
    for parent in parents:
        for off, operation, target, role, size in parent["ordinary_edges"]:
            if operation != "bl":
                continue
            site = parent["function"] + off
            nodes = tuple(
                v
                for v in returns
                if v["function"] == target
                and v["role"] == role
                and (v["entry_SP"], v["return_PC"], v["return_SP"])
                == (frame.start, site + 4, frame.start)
            )
            require(
                len(nodes) == 1 and nodes[0]["preserved_high_gprs"] == high,
                "Tail parent carrier loses an original call/high-GPR transfer",
            )
            call_map[site] = nodes[0]
    require(len(call_map) == 25, "Tail parent original call graph is incomplete")
    selected = {
        p["function"] - V + off
        for p in parents
        for off, width, text in GRAMMARS["visitor_" + str(p["function"] - V)]["instructions"]
    }

    def segment(parts: Sequence[tuple[int, int]]) -> tuple[int, ...]:
        return tuple(off for lo, hi in parts for off in sorted(selected) if lo <= off < hi)

    controls_path = segment(_CONTROLS_PATH)
    history_path = segment(_HISTORY_PATH)
    led_path = segment(_LED_PATH)
    whole = controls_path + history_path + led_path
    require(
        len(whole) == len(set(whole)) and whole[0] == 5630 and whole[-1] == 7112,
        "Tail parent chronological normal membership differs",
    )

    def path_check(path: Sequence[int], exit_pc: int) -> tuple[tuple[int, ReturnNode], ...]:
        checked = []
        for i, off in enumerate(path):
            row = rows[V + off]
            family, conditional = _control(row)
            following = off + len(row.raw)
            after = path[i + 1] if i + 1 < len(path) else exit_pc
            require(
                objects.read_only(row.address, len(row.raw)) == row.raw,
                "Tail parent chronology has stale current bytes",
            )
            if family == "bl":
                require(
                    V + off in call_map and after == following,
                    "Tail parent path bypasses its original child return",
                )
                checked.append((V + off, call_map[V + off]))
            elif family == "b":
                target = _direct_target(row, "b") - V
                require(
                    after == target or conditional and after == following,
                    "Tail parent source guard changes its checked branch successor",
                )
            else:
                require(
                    family is None and after == following,
                    "Tail parent chronology adds an unpaired PC effect",
                )
        return tuple(checked)

    # Successful primitive outcomes have the actual same Q/D/allocation and
    # decremented count, not just a copied semantic label or final cursor.
    all_calls: list[SourceInvocation] = list(controls["invocations"])
    if suffix is not None:
        all_calls.extend(suffix["invocations"])
    for call in all_calls:
        site = call["site"] - V
        expected_controls = {5656: (828, 2), 5712: (1128, 3), 6198: (1128, 2), 6426: (1128, 1)}
        if site in expected_controls:
            offset, count = expected_controls[site]
            require(
                (
                    call["before"].header.storage,
                    call["before"].header.offset,
                    call["before"].header.extent,
                    call["before"].count,
                )
                == (frame, offset, 8, Span(count, count)),
                "Tail Controls source call changes its actual Q/count",
            )
        _call_binding(parents, children, call, frame)
        before = call["before"]
        if _is_scalar_call(call):
            for option in call["result"]["outcomes"]:
                scalar_after = option["state"]
                require(
                    scalar_after.header == before.header
                    and scalar_after.decoder.header == before.decoder.header
                    and scalar_after.decoder.payload == before.decoder.payload
                    and scalar_after.count
                    == Span(max(0, before.count.lower - 1), max(0, before.count.upper - 1))
                    and option["cursor"] == scalar_after.decoder.cursor,
                    "Tail parent scalar has another same-source/count outcome",
                )
        else:
            if not (_is_screen_call(call) or _is_led_call(call)):
                raise AssertionError("Tail parent aggregate lacks its exact source/caller pairing")
            require(
                call["result"]["before"] is before and call["result"]["call_site"] == call["site"],
                "Tail parent aggregate changes its exact source/caller pairing",
            )
    normal_calls = controls["invocations"]
    normal = controls["normal"]
    if normal is None:
        require(
            effects["complete_after"] is None
            and not effects["Config_stages"]
            and suffix is None
            and suffix_effects is None,
            "Tail failed Controls source reaches a later group",
        )
    else:
        require(
            len(normal_calls) == 4
            and normal["hold"] is normal_calls[0]
            and len(normal["screens"]) == 3
            and normal["after"].count == Span(3, 3)
            and normal["ControlQ"].count == Span(1, 1)
            and normal["ScreenQ"].count == Span(0, 0)
            and effects["complete_after"] == V + 6118
            and tuple(v["site"] - V for v in effects["Config_stages"])
            == (6062, 6066, 6070, 6074, 6078, 6088, 6098, 6106, 6114)
            and effects["Config_stages"][-1]["complete_Controls"],
            "Tail Controls complete flag precedes its actual last assignment",
        )
        hold_call = normal_calls[0]
        if not _is_scalar_call(hold_call):
            raise AssertionError("Tail hold normal guard lacks the same source outcome")
        require(
            hold_call["before"].count == Span(2, 2)
            and normal["hold_outcomes"]
            and all(
                any(v is o for o in hold_call["result"]["outcomes"])
                and v["kind"] == "some"
                and v["value_domain"] == (0, 65535)
                for v in normal["hold_outcomes"]
            ),
            "Tail hold normal guard lacks the same source outcome",
        )
        for i, screen in enumerate(normal["screens"]):
            screen_call = normal_calls[i + 1]
            if not _is_screen_call(screen_call):
                raise AssertionError("Tail Screen normal guard loses its source package")
            require(
                screen["invocation"] is screen_call
                and screen["source"] is screen_call["result"]["normal"]
                and screen_call["before"].count == Span(3 - i, 3 - i)
                and screen["source"]["after"].count == Span(2 - i, 2 - i)
                and screen_call["predecessor"] is normal_calls[i]
                and screen["output"] is screen_call["output"],
                "Tail Screen normal guard loses its source/rotation/package epoch",
            )
        path_check(controls_path, 6118)
    if suffix is not None:
        if normal is None or suffix_effects is None:
            raise AssertionError("Tail suffix lineage loses its exact Controls predecessor")
        require(
            normal is not None
            and suffix["controls"] is effects
            and suffix["before"] is normal["after"]
            and suffix_effects["source"] is suffix,
            "Tail suffix lineage loses its exact Controls predecessor",
        )
        current_decoder = normal["after"].decoder
        for i, call in enumerate(suffix["invocations"]):
            site = call["site"] - V
            if site in _HISTORY_SITES:
                index = _HISTORY_SITES.index(site)
                count = 8 - index
                offset = 1064
            elif site == 6736:
                count = 2
                offset = 288
            elif site in (6792, 6828):
                count = 7 - (site == 6828)
                offset = 1000
            else:
                require(site in _BOOL_SITES, "Tail suffix adds another source invocation")
                count = 5 - _BOOL_SITES.index(site)
                offset = 1000
            before = call["before"]
            require(
                before.count == Span(count, count)
                and before.decoder is current_decoder
                and (before.header.storage, before.header.offset, before.header.extent)
                == (frame, offset, 8)
                and call["predecessor"] is (effects if i == 0 else suffix["invocations"][i - 1]),
                "Tail suffix ordered source/Q/count/predecessor guard differs",
            )
            if _is_scalar_call(call):
                good = tuple(v for v in call["result"]["outcomes"] if v["kind"] == "some")
                after: SequenceInterface | None = call["normal_after"]
                require(
                    after == (None if not good else _joined_sequence(good)),
                    "Tail scalar retained successor differs from its exact same-source outcome join",
                )
            else:
                if not (_is_screen_call(call) or _is_led_call(call)):
                    raise AssertionError("Tail aggregate call lacks its current result")
                aggregate_normal = call["result"]["normal"]
                after = None if aggregate_normal is None else aggregate_normal["after"]
            if after is None:
                require(
                    i == len(suffix["invocations"]) - 1,
                    "Tail failed source reaches another invocation",
                )
            else:
                current_decoder = after.decoder
        if suffix["history"] is None:
            raise AssertionError("Tail suffix lineage lacks its history source")
        if suffix["history"]["normal"] is not None:
            require(
                len(suffix_effects["history"]) == 9
                and suffix_effects["history"][-1]["site"] == V + 6732,
                "Tail history complete guard precedes its final split byte",
            )
            path_check(history_path, 6734)
        if suffix["normal"] is not None:
            if suffix["LED"] is None:
                raise AssertionError("Tail successful carrier lacks its LED source")
            require(
                suffix["normal"]["after"].count == Span(1, 1)
                and suffix["normal"]["LEDQ"].count == Span(0, 0)
                and suffix["normal"]["physical_top_count"] == 1
                and suffix["normal"]["logical_remaining_fields"] == 0
                and suffix["normal"]["source"] is suffix["LED"]["normal"]
                and not suffix["normal"]["old_history_Q_still_live"]
                and suffix_effects["complete_after"] == V + 7112
                and tuple(v["site"] - V for v in suffix_effects["LED"])
                == (7026, 7034, 7038, 7046, 7054, 7062, 7068, 7076, 7082, 7088, 7096, 7102, 7108)
                and suffix_effects["LED"][-1]["complete_LED"],
                "Tail successful carrier precedes its actual late LED stores/retired headers",
            )
            path_check(led_path, 56)
    complete = suffix is not None and suffix["normal"] is not None
    normal_record: NormalLineage | None = None
    if complete:
        calls = path_check(whole, 56)
        positions = {at: i for i, at in enumerate(whole)}
        lifetimes = (
            ("r4", None, 6426),
            ("r8", 5638, 6110),
            ("r10", 5672, 6114),
            ("r5", 5716, 6066),
            ("r9", 6202, 6070),
            ("r11", 6502, 6062),
            ("r4", 6148, 6730),
            ("r6", 6150, 6728),
            ("r5", 6174, 6722),
            ("r8", 6466, 6714),
            ("r9", 6540, 6706),
            ("r10", 6568, 6698),
            ("r11", 6596, 6690),
            ("r5", 6796, 7096),
            ("r8", 6800, 7088),
            ("r10", 6814, 7034),
            ("r9", 6818, 7108),
            ("r4", 6832, 7082),
            ("r6", 6836, 7076),
            ("r11", 6880, 7068),
        )
        for reg, producer, consumer in lifetimes:
            lo = -1 if producer is None else positions[producer]
            hi = positions[consumer]
            require(lo < hi, "Tail parent register consumer precedes its producer")
            for off in whole[lo + 1 : hi + 1]:
                row = rows[V + off]
                family, conditional = _control(row)
                require(
                    reg not in _high_definitions(row, family),
                    "Tail parent register carrier is overwritten before consumption",
                )
                if family == "bl":
                    require(
                        reg in call_map[V + off]["preserved_high_gprs"],
                        "Tail child does not restore the same incoming carrier value",
                    )
        # Scratch cells have separate exact epochs. This is checked against
        # every parent store in chronology, with child physical writes already
        # protected by each fresh source/copy interface.
        scratch = (
            (284, 6436, 6092),
            (284, 6622, 6674),
            (280, 6866, 7016),
            (284, 6872, 7098),
            (276, 6906, 7056),
            (272, 6932, 7048),
            (268, 6958, 7040),
        )
        for cell, producer, consumer in scratch:
            lo, hi = positions[producer], positions[consumer]
            require(lo < hi, "Tail scratch consumer precedes its current captured epoch")
            for site, coordinate, off, n in PARENT_WRITES:
                if site in positions and lo < positions[site] <= hi and coordinate == "V":
                    require(
                        off + n <= cell or cell + 4 <= off,
                        "Tail captured scratch epoch is overwritten before consumption",
                    )
        normal_record = dict(
            frontier=V + 7112,
            path=tuple(V + off for off in whole),
            calls=calls,
            carriers=tuple((reg, None if a is None else V + a, V + b) for reg, a, b in lifetimes),
            captured_epochs=tuple((frame.start + cell, V + a, V + b) for cell, a, b in scratch),
            return_carrier=255,
            physical_top_count=1,
            logical_remaining_fields=0,
            requires_first_only_access_and_full_original_restore=True,
        )
    # An error branches from its same call's actual return after all previous
    # successful guards. None is impossible here: all fixed caller counts are
    # strictly positive, while field-level None is already an aggregate error.
    post = {
        5656: ((5660, 5680), (5968, 5974)),
        5712: ((5716, 5732), (5968, 5974)),
        6198: ((6202, 6218), (5968, 5974)),
        6426: ((6430, 6446), (5968, 5974)),
        6144: ((6148, 6160), (6656, 6662)),
        6170: ((6174, 6188), (6656, 6662)),
        6460: ((6464, 6478), (6656, 6662)),
        6534: ((6538, 6552), (6656, 6662)),
        6562: ((6566, 6580), (6656, 6662)),
        6590: ((6594, 6608), (6656, 6662)),
        6616: ((6620, 6634), (6656, 6662)),
        6642: ((6646, 6662),),
        6736: ((6740, 6758), (6848, 6854)),
        6792: ((6796, 6808), (6988, 6994)),
        6828: ((6832, 6848), (6988, 6994)),
        6876: ((6880, 6890), (6988, 6994)),
        6900: ((6904, 6916), (6988, 6994)),
        6926: ((6930, 6942), (6988, 6994)),
        6952: ((6956, 6968), (6988, 6994)),
        6978: ((6982, 6994),),
    }
    errors: list[ErrorLineage] = []
    for group, records in (
        ((), controls["errors"]),
        (() if suffix is None else ("Controls",), () if suffix is None else suffix["errors"]),
    ):
        for error in records:
            call = error["origin"]
            cause = error["cause"]
            site = call["site"] - V
            if _is_scalar_call(call):
                outcomes: Sequence[object] = call["result"]["outcomes"]
            elif _is_screen_call(call) or _is_led_call(call):
                outcomes = call["result"]["errors"]
            else:
                raise AssertionError("Tail error loses its exact source call")
            require(
                any(call is v for v in all_calls)
                and error["field"] == call["field"]
                and any(cause is v for v in outcomes)
                and error["cursor"] == cause["cursor"]
                and "error" in cause
                and error["error"] == cause.get("error"),
                "Tail error loses the exact same-call error origin/cursor/carrier",
            )
            require(
                cause.get("kind") != "none" and (cause.get("kind", "error") != "some"),
                "Tail positive caller count cannot route a normal/None as parser error",
            )
            require(
                site in post and site in whole, "Tail error has another actual source call site"
            )
            prefix = whole[: whole.index(site) + 1]
            path = prefix + segment(post[site])
            path_check(path, 56)
            milestones = error.get("completed_groups", group)
            expected_groups = (
                ()
                if site in (5656, 5712, 6198, 6426)
                else (
                    ("Controls",)
                    if site in _HISTORY_SITES
                    else (
                        ("Controls", "history")
                        if site == 6736
                        else ("Controls", "history", "reduced_motion")
                    )
                )
            )
            after = _cause_after(cause)
            count = 3 if not expected_groups else (2 if len(expected_groups) == 1 else 1)
            require(
                milestones == expected_groups
                and error["after"].header is controls["before"].header
                and error["after"].decoder is after.decoder
                and error["after"].count == Span(count, count),
                "Tail error grants a later incomplete Config group or another source cursor",
            )
            errors.append(
                dict(
                    source_error=error,
                    call=call,
                    origin=cause,
                    path=tuple(V + off for off in path) + (V + 56,),
                    return_carrier=error["error"],
                    completed_groups=milestones,
                    LED_Config_writes=(),
                    requires_full_original_restore=True,
                )
            )
    return dict(
        normal=normal_record,
        errors=tuple(errors),
        confers_active_state=False,
        original_PC_SP_and_saved_noninterference_pending=True,
    )


@dataclass(frozen=True)
class _TailActiveControl:
    state: _TailPhasesState
    structural: _TailStructuralControl
    guards: tuple[TailGuard, ...]
    record: TailControlRecord
    checkpoint: PhaseGraph
    actual_reached_accesses_pending: bool = True

    def current(self) -> _TailActiveControl:

        require(
            self.actual_reached_accesses_pending is True,
            "Tail active retained reached-access qualification changed",
        )
        require(
            _control_checkpoint(self.state, self.guards, self.record, self.structural)
            == self.checkpoint,
            "Tail active source/control record changed",
        )
        self.state.current()
        fresh = _tail_structural(self.state)
        guards, record = _tail_control_data(self.state, fresh)
        require(
            _control_checkpoint(self.state, guards, record, fresh) == self.checkpoint,
            "Tail actual guarded source/return/phase association changed",
        )
        return self


def _structural_data(raw: _TailStructuralControl, state: _TailPhasesState) -> StructuralData:
    require(
        type(raw) is _TailStructuralControl
        and type(raw.selection) is _TailSelection
        and raw.selection.source_state is state.predecessor
        and raw.selection.Tail_parents is state.parents
        and raw.selection.Tail_children is state.children
        and not raw.confers_active_state,
        "Tail raw selection changes its exact private predecessor/parent/child association",
    )
    s = raw.selection
    source = state.predecessor.phases.structural.selection
    require(
        s.source_selection is source
        and s.context is source.context is state.predecessor.phases.widgets.views.context
        and s.widgets is source.widgets is state.predecessor.phases.widgets
        and s.Source_parents is source.Source_parents is state.predecessor.phases.parents
        and s.Source_children is source.Source_children is state.predecessor.phases.children,
        "Tail retained selection changes its exact predecessor association",
    )
    return dict(
        selection=(
            s.proofs,
            s.selected,
            s.edges,
            tuple(sorted(s.cuts)),
            s.widget,
            s.children,
            s.child_contexts,
        ),
        raw_successors=raw.raw_successors,
        raw_returns=raw.raw_returns,
        nested_returns=raw.nested_returns,
        carriers=raw.carrier_returns,
        restore=raw.restore,
        terminal_partition=raw.terminal_partition,
        outside=raw.outside_shared_contexts,
        foreign_restore=raw.foreign_restore_contexts,
        dormant=raw.unentered_outgoing,
        confers_active_state=raw.confers_active_state,
    )


def _tail_control_data(
    state: _TailPhasesState, raw: _TailStructuralControl
) -> tuple[tuple[TailGuard, ...], TailControlRecord]:
    from config_source_control import _SourceActiveControl
    from config_decoded_widgets import _WidgetActiveControl
    from config_decoded_tail import _TailPhasesState, _entry_data
    from config_tail_parent_effects import _parent_envelopes
    from config_regions import Storage, require_disjoint
    from config_abstract import Span

    require(
        type(state) is _TailPhasesState,
        "Tail active control lacks its private source phase producer",
    )
    _structural_data(raw, state)
    predecessor = state.predecessor
    previous = predecessor.control
    if state.entries:
        require(
            type(previous) is _SourceActiveControl
            and previous.state is predecessor.phases
            and len(previous.source_guards) == len(predecessor.phases.phases),
            "Tail active control lacks its actual checked Source ordinary predecessor",
        )
    widget = predecessor.phases.widgets.control
    if type(widget) is not _WidgetActiveControl:
        raise AssertionError(
            "Tail active control lacks its actual checked Widget ordinary predecessor"
        )
    require(
        type(widget) is _WidgetActiveControl and widget.state is predecessor.phases.widgets.views,
        "Tail active control lacks its actual checked Widget ordinary predecessor",
    )
    c = raw.selection.context
    frame = c.headers["visitor_frame"]
    V = c.headers["visitor"]["proof"]["function"]
    R = c.headers["root"]["proof"]["function"]
    guards: list[TailGuard] = []
    for index, (phase, entry) in enumerate(zip(state.phases, state.entries)):
        _entry_data(entry, predecessor)
        require(
            phase.entry is entry
            and phase.controls["before"] is entry.sequence
            and entry.sequence.count == Span(4, 4)
            and phase.parent_envelopes
            == _parent_envelopes(state.objects, state.rows, state.parents, entry, c.owner),
            "Tail guarded phase changes its actual source entry or protected parent effects",
        )
        saved = (frame.start + 1512, frame.start + 1608)
        require(
            raw.restore["saved_interval"] == saved
            and any(
                v.space == frame.space and (v.start, v.start + v.extent) == saved and not v.writable
                for v in entry.protected
            ),
            "Tail guarded return loses the exact original Visitor saved interval",
        )
        # Every authored parent footprint is checked with its real address;
        # all descendants are below V and must preserve the same actual
        # original96-byte saved envelope in both coordinate spaces.
        for site, view, coordinate in phase.parent_envelopes["writes"]:
            require_disjoint(
                (
                    Storage(
                        "address",
                        "actual original Visitor saves",
                        saved[0],
                        96,
                        4,
                        frame.phase,
                        False,
                    ),
                    Storage(
                        "address",
                        "actual Tail parent footprint",
                        view.start,
                        view.extent,
                        1,
                        frame.phase,
                        True,
                    ),
                )
            )
        calls = phase.controls["invocations"] + (
            () if phase.suffix is None else phase.suffix["invocations"]
        )
        protected_calls = []
        for call in calls:
            result = call["result"]
            bindings = call.get("protected_constituents")
            values = (
                tuple(v for v, envelope in bindings)
                if bindings is not None
                else result["protected"]
            )
            require(
                any(
                    v.space == frame.space
                    and v.start <= saved[0]
                    and saved[1] <= v.start + v.extent
                    and not v.writable
                    and v.phase == frame.phase
                    for v in values
                ),
                "Tail actual source call dropped its original saved-cell guard",
            )
            roots = call.get("nested_returns", result.get("nested_returns", ()))
            root = _call_root(roots, call["site"], result["proof"], frame.start)
            active = Storage(
                "address",
                "actual Tail source descendants",
                root["lowest_SP"],
                frame.start - root["lowest_SP"],
                1,
                frame.phase,
                True,
            )
            require_disjoint(
                (
                    active,
                    Storage(
                        "address",
                        "original Visitor saved bytes",
                        saved[0],
                        96,
                        4,
                        frame.phase,
                        False,
                    ),
                )
            )
            protected_calls.append((call["site"], root["lowest_SP"], saved))
        lineage = _parent_lineage(
            state.objects,
            state.rows,
            state.parents,
            state.children,
            raw.nested_returns,
            phase.controls,
            phase.control_effects,
            phase.suffix,
            phase.suffix_effects,
            frame,
        )
        if lineage["normal"] is not None:
            if phase.suffix_effects is None:
                raise AssertionError("Tail original return lacks the actual final LED assignment")
            require(
                phase.suffix_effects["complete_after"] == V + 7112
                and lineage["normal"]["frontier"] == V + 7112
                and lineage["normal"]["path"][-1] == V + 7112,
                "Tail original return precedes the actual final LED assignment",
            )
        guards.append(
            dict(
                entry_index=index,
                entry=entry,
                Source_phase=entry.exit.phase_index,
                Source_normal=entry.exit.source_normal,
                lineage=lineage,
                saved_interval=saved,
                protected_calls=tuple(protected_calls),
                original_return=(
                    V + 56,
                    V + 58,
                    V + 62,
                    V + 66,
                    V + 68,
                    raw.restore["restore"]["function"],
                    raw.restore["restore"]["function"] + 4,
                    R + 130,
                ),
            )
        )
    extension: ControlExclusions = dict(
        excluded_taken=dict(widget.profile["excluded_taken"]),
        excluded_fallthrough=dict(widget.profile["excluded_fallthrough"]),
    )
    successors, returns, dormant = _selected_successors(raw.selection, extension)
    require(
        not dormant
        and raw.selection.cuts
        == frozenset((c.headers["root"]["checksum_failure"]["function"], R + 130)),
        "Tail active original return retains another unproved outgoing cut",
    )
    require(
        raw.restore["original_return_PC"] == R + 130
        and raw.restore["return_SP"] == frame.start + 1608
        and raw.restore["release_bytes"] == (1512, 56, 4, 36),
        "Tail active original return changes the saved-PC/SP equation",
    )
    record: TailControlRecord = {
        "selected_instructions": len(raw.selection.selected),
        "successors": successors,
        "child_returns": returns,
        "return_equations": tuple((g["entry_index"], g["original_return"]) for g in guards),
        "return": raw.restore,
        "outside_shared_contexts": raw.outside_shared_contexts,
        "separate_restore_contexts": raw.foreign_restore_contexts,
        "pending_contexts_confer_state": False,
        "original_saved_bytes_protected_on_each_actual_Tail_phase": tuple(
            g["saved_interval"] for g in guards
        ),
        "stopped_frontiers": tuple(sorted(raw.selection.cuts)),
        "successor_executed": False,
        "exceptional_resume_established": False,
        "exception_safe_completion": False,
        "actual_reached_accesses_pending": True,
        "complete_policy_and_resource_producer_pending": True,
    }
    return tuple(guards), record


def _call_root(nodes: Sequence[ReturnNode], site: int, proof: BodyRecord, sp: int) -> ReturnNode:
    roots = tuple(
        v
        for v in nodes
        if v["function"] == proof["function"]
        and v["role"] == proof["role"]
        and (v["entry_SP"], v["return_PC"], v["return_SP"]) == (sp, site + 4, sp)
    )
    require(
        len(roots) == 1
        and roots[0]["preserved_high_gprs"] == tuple("r" + str(i) for i in range(4, 12)),
        "Tail guarded source loses its original returned high-register lineage",
    )
    return roots[0]


def _tail_control(state: _TailPhasesState) -> _TailActiveControl:
    """Actual named ordinary Tail segment; reached access and finalizer pending.

    A structural selection, source-only relation or public description cannot
    enter. Unproved exceptional entry/return cannot resume this state.
    """
    from config_decoded_tail import _TailPhasesState

    require(type(state) is _TailPhasesState, "Tail control lacks its actual private producer")
    state.current()
    raw = _tail_structural(state)
    guards, record = _tail_control_data(state, raw)
    return _TailActiveControl(
        state, raw, guards, record, _control_checkpoint(state, guards, record, raw)
    )


def _control_checkpoint(
    state: _TailPhasesState,
    guards: Sequence[TailGuard],
    record: TailControlRecord,
    raw: _TailStructuralControl,
) -> PhaseGraph:
    """Snapshot this join without recursively serializing prior checkpoints.

    The actual predecessor/exit/normal identities are checked explicitly and
    the predecessor's own currentness checks their complete value/alias graph.
    Every local retained field and its aliases still enters this snapshot.
    """
    from config_decoded_sources import _phase_graph

    return _phase_graph((_guard_data(state, guards), record, _structural_data(raw, state)))


def _guard_data(
    state: _TailPhasesState, guards: Sequence[TailGuard]
) -> tuple[dict[str, object], ...]:
    """Local guard values with their actual upstream identities checked."""
    from config_decoded_tail import _entry_data

    require(len(guards) == len(state.entries), "Tail active guard collection changed")
    # Snapshot all extra retained fields too, so mutation cannot hide behind the static schema.
    data: list[dict[str, object]] = []
    for i, (guard, entry) in enumerate(zip(guards, state.entries)):
        require(
            guard["entry_index"] == i
            and guard["entry"] is entry
            and guard["Source_phase"] == entry.exit.phase_index
            and guard["Source_normal"] is entry.exit.source_normal,
            "Tail active guard lost its exact entry/normal association",
        )
        data.append(
            dict(
                entry=_entry_data(entry, state.predecessor),
                **{k: v for k, v in guard.items() if k not in ("entry", "Source_normal")},
            )
        )
    return tuple(data)
