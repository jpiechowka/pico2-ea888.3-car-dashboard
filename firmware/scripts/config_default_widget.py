"""Grouped default Widget phase with paired source and protected copy effects.

Incoming exclusive slot, fault provenance and actual SP/predecessor transport
remain enclosing obligations. This phase never publishes Config, resolves a
platform assumption or emits a graph/stack/image pass.
"""

from collections.abc import Iterable, Sequence
from typing import NotRequired, TypedDict
from config_body_types import BodyRecord, Callback, InstructionMap
from config_stack import DiagnosticObjects
from config_regions import View
from config_roots import WindowRecord
from config_helpers import HelperCoverageRecord
from config_headers import RootHeader
from config_default import PreparedDefault, DefaultTargetStore, Writes
from config_construction import ExtendResult
from config_refill_children import AdvancedResult, OpaqueCopyResult, TargetStore
from config_widget_contract import WidgetObject, ChannelCase
from llvm_stack import Instruction
import copy
import re
import struct
from config_abstract import Span
from config_abstract_compare import InitializedPrefix
from config_construction import VecInterface, extend_interface
from config_default import default_preparation, _PREPARE, _ingress
from config_entry import entry_object
from config_finite import _literal_body, _complete_body
from config_headers import ROOT, ROOT_ROWS, root_decoder_header
from config_helpers import _helper_function, helper_coverage, HELPER_SEMANTIC_ROLES
from config_refill_children import (
    _readonly,
    child_body,
    child_coverage,
    advanced_interface,
    opaque_copy_interface,
)
from config_regions import Storage, Argument, Region, Transfer, borrow, require_writes, _control
from config_roots import root_window
from config_stack import canonical_symbol, transfer_family
from config_widget_contract import WIDGET_CONTRACT, CHANNEL_CASES, WIDGET_WRITES, COPY28_CONTRACT
from validation import require


class WidgetChildHistory(TypedDict):
    label_append: ExtendResult
    warning_append: ExtendResult
    advanced_constructor: AdvancedResult


class WidgetIteration(TypedDict):
    index: int
    channel: int
    offset: int
    widget: View
    source_case: ChannelCase
    child_history: WidgetChildHistory
    opaque_copies: tuple[OpaqueCopyResult, ...]
    initialized_label: InitializedPrefix
    initialized_warning: InitializedPrefix
    advanced_semantic_range: tuple[int, int]
    advanced_padding: tuple[int, int]
    initialized_capacity_elements: int
    uninitialized_graph_payload: tuple[int, int]
    trailing_padding: tuple[int, int]


class WidgetDiagnostic(TypedDict):
    root: WindowRecord
    local_writes: Writes
    requires_nonreturn_default_formatter_sink_closure: bool
    child_status: NotRequired[int]
    error_payload_bytes: NotRequired[int]
    argument_reference_levels: NotRequired[int]
    argument_address: NotRequired[int]
    tagged_literal_bytes: NotRequired[int]


class WidgetTargetStore(TypedDict):
    role: str
    site: int
    owner: View
    loop_index: int
    operation: str
    width: int
    access_offset: int
    receiver_start: int
    phase: str
    status: str


TargetRequirement = DefaultTargetStore | TargetStore | WidgetTargetStore


class WidgetLoop(TypedDict):
    proof: BodyRecord
    regions: tuple[Region, ...]
    prepared: PreparedDefault
    iterations: tuple[WidgetIteration, ...]
    diagnostics: dict[str, WidgetDiagnostic]
    possible_writes: Writes
    local_writes: Writes
    normal_exit_count: int
    continuation: int
    target_access_requirements: tuple[TargetRequirement, ...]
    source_array_epoch_not_established: bool
    requires_actual_incoming_exclusive_slot_fault_SP_and_whole_parent_graph: bool
    requires_complete_parent_O8_and_all_indirect_ingress: bool
    requires_nonreturn_diagnostic_default_sink_closure: bool
    physical_peak_child_paths: tuple[tuple[int, ...], ...]
    parent_frame: int


def _object(
    objects: DiagnosticObjects, address: int, descriptor: WidgetObject
) -> tuple[Callback, ...]:
    kind = descriptor["kind"]
    if descriptor["kind"] == "entry_static":
        return entry_object(objects, address, descriptor)
    if kind == "widget_payload_scalar":
        require(
            type(address) is int and 0 <= address <= 0xFFFFFFFF, "invalid widget scalar pool value"
        )
        return ()
    if kind == "widget_label_pointers":
        pointers = struct.unpack("<8I", _readonly(objects, address, 32, 4))
        for pointer, case in zip(pointers, CHANNEL_CASES):
            require(
                _readonly(objects, pointer, case["length"], 1) == case["label"].encode(),
                "Widget label source differs",
            )
    elif kind == "widget_label_lengths":
        require(
            _readonly(objects, address, 8, 1) == bytes(c["length"] for c in CHANNEL_CASES),
            "Widget label length/index differs",
        )
    elif kind == "widget_limit_modes":
        require(
            _readonly(objects, address, 8, 1) == bytes(c["limit_mode"] for c in CHANNEL_CASES),
            "Widget LimitMode/Channel differs",
        )
    elif kind == "widget_f32_payload_table":
        _readonly(objects, address, 32, 4)
    else:
        require(
            kind == "widget_warning_default"
            and _readonly(objects, address, 12, 4)[10:] == b"\x01\0",
            "Widget warning Priority/latch source differs",
        )
    return ()


def _outline(
    objects: DiagnosticObjects,
    name: str,
    instructions: InstructionMap,
    role: str,
    ancestry: tuple[int, ...] = (),
) -> BodyRecord:
    if role != "@outline:default_widget_copy28":
        return child_body(objects, name, instructions, role, ancestry)
    require(re.fullmatch(r"OUTLINED_FUNCTION_[0-9]+", name), "copy28 has another origin")
    return _complete_body(
        objects,
        name,
        instructions,
        role,
        "Widget copy28",
        COPY28_CONTRACT,
        _object,
        _outline,
        ancestry,
        function_consumer=_helper_function,
    )


def _suffix(
    objects: DiagnosticObjects, name: str, instructions: InstructionMap, target: int, role: str
) -> WindowRecord:
    candidates = []
    for at, row in instructions.items():
        if target <= at < target + 64 and row.mnemonic == "bl":
            try:
                root = root_window(objects, name, instructions, at)
            except AssertionError:
                continue
            if root["window"] == target and root["location_role"] == role:
                candidates.append(root)
    require(len(candidates) == 1, "Widget failure loses its typed diagnostic suffix")
    return candidates[0]


def _pool(row: Instruction) -> int:
    match = re.fullmatch(
        r"(?:r[0-9]+|lr), \[pc, #(0x[0-9a-f]+|[0-9]+)\]", row.operands.split("@", 1)[0].strip()
    )
    if match is None:
        raise AssertionError("prior source pool consumer changed")
    return ((row.address + 4) & ~3) + int(match[1], 0)


def _prior_pools(
    objects: DiagnosticObjects,
    instructions: InstructionMap,
    header: RootHeader,
    prepared: PreparedDefault,
    start: int,
) -> bytes:
    # This interval is not Widget dataflow. Its fifteen words are owned by
    # earlier matched CRC/constructor consumers, including unknown f32 payload.
    # Reuse those typed sources, rather than learning an acceptance fingerprint.
    owned = set(prepared["readonly_float_pools"])
    for rows, proof, ordinals in (
        (ROOT_ROWS, header["proof"], (1, 2)),
        (_PREPARE, prepared["proof"], (1, 3, 4, 5)),
    ):
        for offset, width, operation in rows:
            match = re.search(r"@object([0-9]+)\.pc_slot", operation)
            if match and int(match[1]) in ordinals:
                owned.add(_pool(instructions[proof["function"] + offset]))
    require(
        owned == set(range(start + 256, start + 316, 4)),
        "Widget interleaved prior data loses its typed owners",
    )
    return objects.read_only(start + 256, 60)


def _window(
    objects: DiagnosticObjects,
    name: str,
    instructions: InstructionMap,
    frame: Storage,
    ancestors: Sequence[Storage],
) -> tuple[PreparedDefault, BodyRecord, dict[str, WindowRecord], list[Region], tuple[int, ...]]:
    prepared = default_preparation(objects, name, instructions, frame, ancestors)
    header = root_decoder_header(objects, name, instructions, frame, ancestors)
    start = prepared["continuation"]
    end = start + 728
    contract = copy.deepcopy(WIDGET_CONTRACT)
    roots = {}
    for offset, role in (
        (0x30, "label_capacity"),
        (0x168, "warning_capacity"),
        (0x198, "widget_count"),
    ):
        row = instructions.get(start + offset)
        if row is None:
            raise AssertionError("missing Widget diagnostic guard")
        matched_target = re.match(r"0x([0-9a-f]+)(?:\s|$)", row.operands)
        if matched_target is None:
            raise AssertionError("unresolved Widget failure target")
        destination = int(matched_target[1], 16)
        roots[role] = _suffix(objects, name, instructions, destination, role)
        token = "@" + role
        contract["instructions"] = [
            (o, w, t.replace(token, "@self+" + format(destination - start, "x")))
            for o, w, t in contract["instructions"]
        ]
    data = list(contract["data"])
    data[1] = (
        256,
        60,
        _prior_pools(objects, instructions, header, prepared, start).hex(),
    )
    contract["data"] = data
    proof = _literal_body(
        objects,
        name,
        instructions,
        ROOT,
        "default Widget loop",
        contract,
        _object,
        _outline,
        window=start,
        function_consumer=_helper_function,
    )
    require(
        len(proof["ordinary_edges"]) == 13 and not proof["indirect_sites"],
        "Widget ordinary/callback inventory differs",
    )
    # The one source-guarded TBB is checked below. Other parent computed ingress
    # still follows the existing fail-closed classifier/envelope exclusion.
    _ingress(
        {a: r for a, r in instructions.items() if a != start + 130},
        objects.elf.functions[name][0] & ~1,
        objects.elf.functions[name][1],
        start,
        end,
    )
    for at, row in instructions.items():
        if start <= at < end:
            continue
        if transfer_family(row.mnemonic.removesuffix(".w")) in ("b", "bl", "cbz", "cbnz"):
            target = re.search(r"(?<![#\w])0x([0-9a-f]+)(?:\s|$)", row.operands.split("@", 1)[0])
            require(
                target is None or int(target[1], 16) != start,
                "foreign Widget entry bypasses prepared handoff",
            )
    pointers = struct.unpack("<8I", _readonly(objects, proof["objects"][0], 32, 4))
    lengths = _readonly(objects, proof["objects"][1], 8, 1)
    for pointer, length, case in zip(pointers, lengths, CHANNEL_CASES):
        require(
            length == case["length"]
            and _readonly(objects, pointer, length, 1) == case["label"].encode(),
            "Widget same-Channel pointer/length pair differs",
        )
    require(
        proof["objects"][5] == proof["objects"][10] == prepared["config"].storage.start,
        "Widget reload changes the private Config owner",
    )
    rows = {a: r for a, r in instructions.items() if start <= a < end}
    table = objects.read_only(start + 134, 8)
    targets = tuple(start + 134 + 2 * v for v in table)
    require(
        targets == tuple(start + v for v in (142, 236, 172, 182, 162, 228, 316, 188))
        and all(v in rows for v in targets),
        "Widget TBB loses one of its eight paired source arms",
    )
    allowed = {end} | {v["window"] for v in roots.values()}
    edges = {start + e[0]: e for e in proof["ordinary_edges"]}
    regions = []
    context = (objects.elf.functions[name][0] & ~1, "default Widget phase")
    for lo, hi, kind in contract["mapping"]:
        if kind == "d":
            regions.append(
                Region(
                    context,
                    "data",
                    start + lo,
                    start + hi,
                    (),
                    ((start + lo, objects.read_only(start + lo, hi - lo)),),
                    (),
                )
            )
            continue
        members = tuple(a for a in sorted(rows) if start + lo <= a < start + hi)
        transfers = []
        for at in members:
            row = rows[at]
            family, conditional = _control(row)
            stops = False
            if family == "tbb":
                require(at == start + 130, "unreviewed Widget table")
                transfers.append(
                    Transfer(at, row.mnemonic, "table", targets, "paired source Channel/index")
                )
                stops = True
            elif family in ("b", "bl"):
                branch_target = re.match(r"0x([0-9a-f]+)", row.operands)
                if branch_target is None:
                    raise AssertionError("unresolved Widget current branch")
                destination = int(branch_target[1], 16)
                if family == "bl":
                    require(at in edges, "unbound Widget child")
                    edge = edges[at]
                    transfers.append(
                        Transfer(
                            at,
                            row.mnemonic,
                            "call",
                            destination,
                            "matched source/call-phase interface",
                            edge[3],
                        )
                    )
                else:
                    require(
                        destination in rows or destination in allowed,
                        "Widget branch enters data/unbound continuation",
                    )
                    label = (
                        "loop"
                        if destination == start
                        else "region_exit"
                        if destination == end
                        else "root_suffix"
                        if destination in allowed
                        else "branch"
                    )
                    transfers.append(
                        Transfer(
                            at,
                            row.mnemonic,
                            label,
                            destination,
                            "paired source path/phase continuation",
                        )
                    )
                    stops = not conditional
            else:
                require(family is None, "unreviewed Widget control/return")
            if not stops:
                require(
                    at + len(row.raw) in rows,
                    "Widget executable fallthrough enters mapped data/end",
                )
        regions.append(
            Region(context, "loop", start + lo, start + hi, members, (), tuple(transfers))
        )
    return prepared, proof, roots, regions, pointers


def _store(
    view: View, offset: int, width: int, protected: Iterable[Storage]
) -> tuple[View, int, int]:
    write = (view, offset, width)
    require_writes((view,), (write,), protected)
    return write


def _child(
    objects: DiagnosticObjects,
    instructions: InstructionMap,
    proof: BodyRecord,
    offset: int,
    semantic: str,
) -> HelperCoverageRecord:
    edges = [e for e in proof["ordinary_edges"] if e[0] == offset]
    require(len(edges) == 1, "missing Widget child call")
    target = edges[0][2]
    role = HELPER_SEMANTIC_ROLES[semantic]
    names = [
        n
        for n, e in objects.elf.functions.items()
        if e[0] & ~1 == target and canonical_symbol(n) == role
    ]
    require(len(names) == 1, "Widget construction target differs")
    return helper_coverage(
        objects, names[0], instructions, (target, "Widget call", proof["function"] + offset)
    )


def widget_loop(
    objects: DiagnosticObjects,
    name: str,
    instructions: InstructionMap,
    frame: Storage,
    ancestor_protected: Iterable[Storage],
) -> WidgetLoop:
    require(
        isinstance(frame, Storage) and frame.phase == "default_refill",
        "Widget loop belongs to another source epoch",
    )
    ancestor_protected = tuple(ancestor_protected)
    prepared, proof, roots, regions, pointers = _window(
        objects, name, instructions, frame, ancestor_protected
    )
    start = proof["function"]
    phase = frame.phase
    base = prepared["config"]
    guard = prepared["protected"]
    persistent = guard + tuple(
        Storage(frame.space, label, frame.start + o, n, 4, phase, False)
        for label, o, n in (
            ("Widget pointer cells", 24, 12),
            ("Widget carriers", 36, 28),
            ("Channel array", 72, 8),
        )
    )
    iterations: list[WidgetIteration] = []
    all_writes = []
    target_requirements: list[TargetRequirement] = list(prepared["target_access_requirements"])
    copy_target = next(e[2] for e in proof["ordinary_edges"] if e[3] == "__aeabi_memcpy")
    advanced_target = next(
        e[2] for e in proof["ordinary_edges"] if e[3].endswith("15AdvancedDisplay11for_channel")
    )
    advanced_name = next(
        n for n, e in objects.elf.functions.items() if e[0] & ~1 == advanced_target
    )
    advanced = child_coverage(
        objects, advanced_name, instructions, (advanced_target, "Widget constructor", start)
    )
    for edge in proof["ordinary_edges"]:
        if edge[3] == "__aeabi_memcpy":
            require(edge[2] == copy_target, "Widget copy context changes its complete child")
        elif edge[3].startswith("@outline:"):
            target_name = next(n for n, e in objects.elf.functions.items() if e[0] & ~1 == edge[2])
            bound = _outline(objects, target_name, instructions, edge[3])
            child_edges = bound["ordinary_edges"]
            require(len(child_edges) == 1, "Widget tail outline has an unbound child")
            require(
                child_edges[0][2]
                == (
                    copy_target if edge[3] == "@outline:default_widget_copy28" else advanced_target
                ),
                "Widget outline does not reach the fully bound ordinary child",
            )
    label_child = _child(objects, instructions, proof, 0x2A, "vec_extend24")
    warning_child = _child(objects, instructions, proof, 0x162, "vec_extend20")
    base_guard = guard + tuple(
        Storage(frame.space, label, frame.start + o, n, 4, phase, False)
        for label, o, n in (("Widget pointer cells", 24, 12), ("Channel array", 72, 8))
    )
    # These direct parent writes occur in their authored source phases. They
    # initialize/replace owned carriers, unlike opaque copies, which preserve
    # all carriers. Old label/Advanced/snapshot owners retire at their last use.
    local_writes = tuple(
        _store(borrow(frame, o, n, phase, writable=True), 0, n, base_guard)
        for o, n in (
            (60, 4),
            (236, 4),
            (48, 4),
            (52, 4),
            (56, 4),
            (44, 4),
            (40, 4),
            (36, 4),
            (324, 4),
            (300, 24),
        )
    )
    for i, channel in enumerate(prepared["widget_handoff"]["channels"]):
        case = CHANNEL_CASES[channel]
        length = case["length"]
        W = borrow(base.storage, 444 + 220 * i, 220, phase, writable=True)
        source = InitializedPrefix(
            borrow(
                Storage(
                    "address",
                    "Channel label " + str(channel),
                    pointers[channel],
                    length,
                    1,
                    phase,
                    False,
                ),
                0,
                length,
                phase,
            ),
            Span(length, length),
        )
        appends = []
        for child, offset, capacity in ((label_child, 236, 24), (warning_child, 324, 20)):
            output = borrow(frame, offset, ((4 + capacity + 3) // 4) * 4, phase, writable=True)
            appended = extend_interface(
                objects,
                instructions,
                child,
                VecInterface(output, capacity, Span(0, 0)),
                source,
                Storage(frame.space, "Widget append", frame.start - 16, 16, 8, phase, True),
                persistent,
            )
            require(
                set(("r4", "r5", "r6", "r8", "r9", "r10", "r11", "sp"))
                <= set(appended["normal_preserves"]),
                "Widget label carriers lose child preservation",
            )
            appends.append(appended)
        displayed = advanced_interface(
            objects,
            instructions,
            advanced,
            Argument("Channel", None, 0, 8, frozenset((channel,))),
            Argument("bool", None, 0, 8, frozenset((case["afr_bool"],))),
            borrow(frame, 236, 64, phase, writable=True),
            Storage(frame.space, "Widget AdvancedDisplay", frame.start - 72, 72, 8, phase, True),
            persistent
            + (
                Storage(frame.space, "live label snapshot", frame.start + 208, 28, 4, phase, False),
                Storage(frame.space, "warning receiver", frame.start + 324, 24, 4, phase, False),
            ),
        )
        all_writes += [_store(W, o, n, persistent) for site, o, n, field in WIDGET_WRITES]
        all_writes.append(_store(base, 440, 4, persistent))
        # Copy sources/destinations are phase-specific opaque epochs. Their
        # order below is the actual source order; old owners retire at last read.
        contexts: tuple[
            tuple[int, int | tuple[str, int], int, tuple[tuple[int, int], ...]], ...
        ] = (
            (236, 208, 28, ()),
            (300, 182, 24, ((208, 28),)),
            (208, 84, 28, ((182, 24),)),
            (236, 113, 64, ((84, 28), (182, 24))),
            (180, ("W", 30), 26, ((84, 28), (113, 64))),
            (112, ("W", 87), 65, ((84, 28),)),
            (84, ("W", 152), 28, ()),
        )
        per_copy = []
        for source_offset, destination, count, live in contexts:
            src = borrow(frame, source_offset, count, phase)
            dest = (
                borrow(base.storage, W.offset + destination[1], count, phase, writable=True)
                if isinstance(destination, tuple)
                else borrow(frame, destination, count, phase, writable=True)
            )
            live_epochs = tuple(
                Storage(
                    frame.space,
                    "live Widget scratch " + str(o),
                    frame.start + o,
                    n,
                    1,
                    phase,
                    False,
                )
                for o, n in live
            )
            copied = opaque_copy_interface(
                objects,
                instructions,
                copy_target,
                src,
                dest,
                count,
                frame,
                persistent + live_epochs,
            )
            copied["retained_scratch_epochs"] = live_epochs
            per_copy.append(copied)
        # Direct 24-byte snapshot is aligned LDM/STM with exact owned envelope.
        all_writes.append(_store(borrow(frame, 300, 24, phase, writable=True), 0, 24, persistent))
        iterations.append(
            dict(
                index=i,
                channel=channel,
                offset=W.offset,
                widget=W,
                source_case=case,
                child_history=dict(
                    label_append=appends[0],
                    warning_append=appends[1],
                    advanced_constructor=displayed,
                ),
                opaque_copies=tuple(per_copy),
                initialized_label=InitializedPrefix(
                    borrow(base.storage, W.offset + 156, 24, phase), Span(length, length)
                ),
                initialized_warning=InitializedPrefix(
                    borrow(base.storage, W.offset + 36, 20, phase), Span(length, length)
                ),
                advanced_semantic_range=(W.offset + 88, W.offset + 149),
                advanced_padding=(W.offset + 149, W.offset + 152),
                initialized_capacity_elements=i + 1,
                uninitialized_graph_payload=(4, 12),
                trailing_padding=(219, 220),
            )
        )
        target_requirements.append(
            dict(
                role="widget_colors",
                site=start + 670,
                owner=W,
                loop_index=i,
                operation="single_word_store",
                width=4,
                access_offset=182,
                receiver_start=W.start - 2204,
                phase=phase,
                status="unresolved_object_store_policy",
            )
        )
    location = borrow(frame, 0, 4, phase, writable=True)
    diagnostic_write = _store(location, 0, 4, persistent)
    for root in roots.values():
        require(root["frame"] == 456, "Widget diagnostic loses parent frame")
    diagnostics: dict[str, WidgetDiagnostic] = {}
    for role, root in roots.items():
        diagnostic: WidgetDiagnostic = dict(
            root=root,
            local_writes=(diagnostic_write,),
            requires_nonreturn_default_formatter_sink_closure=True,
        )
        if role != "widget_count":
            diagnostic["child_status"] = 1
            diagnostic["error_payload_bytes"] = 0
            diagnostic["argument_reference_levels"] = 1
            diagnostic["argument_address"] = frame.start + 351
        else:
            diagnostic["tagged_literal_bytes"] = 21
        diagnostics[role] = diagnostic
    # The output's current semantic string views are distinct from historical
    # child states at reused stack offsets; no stale staging owner is exported
    # as the live Widget field. All capacity slack/padding remains opaque.
    requirements = (
        target_requirements[0],
        iterations[0]["child_history"]["advanced_constructor"]["target_access_requirements"][0],
    ) + tuple(target_requirements[1:])
    return dict(
        proof=proof,
        regions=tuple(regions),
        prepared=prepared,
        iterations=tuple(iterations),
        diagnostics=diagnostics,
        possible_writes=tuple(all_writes),
        local_writes=local_writes,
        normal_exit_count=8,
        continuation=start + 728,
        target_access_requirements=requirements,
        source_array_epoch_not_established=True,
        requires_actual_incoming_exclusive_slot_fault_SP_and_whole_parent_graph=True,
        requires_complete_parent_O8_and_all_indirect_ingress=True,
        requires_nonreturn_diagnostic_default_sink_closure=True,
        physical_peak_child_paths=((16, 24), (72, 8), (72, 72), (8,), (72,)),
        parent_frame=456,
    )
