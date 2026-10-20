"""Source-typed Tail aggregate relations, before parent publication.

Primitive values stay abstract. Ordered same-source calls retain their exact
input/result/error association; neither a body receipt nor this conditional
adapter establishes the selected parent execution or an acquired owner.
"""

from __future__ import annotations

from collections.abc import Sequence
from dataclasses import replace
from typing import Generic, NotRequired, TypeAlias, TypeVar, TypedDict
from config_abstract import (
    Span,
    DecoderInterface,
    DecoderOutcome,
    SequenceInterface,
    SequenceOutcome,
    PrimitiveResult,
    decoder_interface,
    sequence_interface,
)
from config_body_types import BodyRecord, InstructionMap
from config_stack import DiagnosticObjects
from config_regions import Storage, View, borrow, require_disjoint, require_writes
from config_decode_effects import _object
from config_tail_children import (
    tail_coverage,
    tail_children,
    tail_return_profiles,
    ACTION_TARGET_OFFSETS,
    TailCoverageRecord,
    TailParentRecord,
    is_child_coverage,
)
from config_tail_contracts import ROLES
from config_widget_children import _name, ColorResult, ColorOutcome
from config_finite_returns import ReturnNode, ReturnParent, _closed_child_returns
from config_owner_entry import _direct_target
from validation import require

Writes: TypeAlias = tuple[tuple[View, int, int], ...]
GuardBindings: TypeAlias = tuple[tuple[Storage, Storage], ...]


class ActionPrimitive(TypedDict):
    site: int
    semantic_role: str
    before: DecoderInterface
    result: PrimitiveResult[DecoderOutcome]
    decrement_before_call: bool
    caller: int
    protected_constituents: GuardBindings


class ActionNone(TypedDict):
    status: int
    action_low: int
    after: SequenceInterface
    possible_writes: Writes


class ActionNormal(TypedDict):
    status: int
    domain: tuple[int, int]
    mapping: tuple[tuple[int, int], ...]
    guard: str
    after: SequenceInterface
    origins: tuple[ActionPrimitive, ...]
    successful_outcomes: tuple[DecoderOutcome, ...]
    possible_writes: Writes


class ActionError(TypedDict):
    stage: str
    status: int
    error: int | None
    guard: NotRequired[str]
    origin: ActionPrimitive
    child_outcome: DecoderOutcome
    after: SequenceInterface
    cursor: Span


Normal = TypeVar("Normal")
Missing = TypeVar("Missing")
Error = TypeVar("Error")


class ResultBase(TypedDict, Generic[Normal, Missing, Error]):
    proof: TailCoverageRecord
    before: SequenceInterface
    normal: Normal | None
    none: Missing | None
    errors: tuple[Error, ...]
    nested_returns: tuple[ReturnNode, ...]
    call_site: int
    protected: tuple[Storage, ...]
    requires_selected_parent_source_control: bool


class ActionResult(ResultBase[ActionNormal, ActionNone, ActionError]):
    primitive_invocations: tuple[ActionPrimitive, ...]
    parent: TailCoverageRecord


class ActionCall(TypedDict):
    field: str
    site: int
    before: SequenceInterface
    result: ActionResult
    predecessor: TailCall | None


class ButtonNone(TypedDict):
    low: int
    upper_bits: str
    after: SequenceInterface
    possible_writes: Writes


class ButtonNormal(TypedDict):
    fields: tuple[tuple[str, tuple[int, int]], ...]
    upper_bits: str
    after: SequenceInterface
    invocations: tuple[ActionCall, ...]
    source_order: tuple[str, ...]


class ButtonError(TypedDict):
    stage: str
    low: int
    error: int | None
    origin: ActionCall
    cause: ActionError | ActionNone
    after: SequenceInterface
    cursor: Span


class ButtonResult(ResultBase[ButtonNormal, ButtonNone, ButtonError]):
    invocations: tuple[ActionCall, ...]
    parent: TailCoverageRecord


class ButtonCall(TypedDict):
    field: str
    site: int
    before: SequenceInterface
    result: ButtonResult
    predecessor: TailCall | None


class DirectCall(TypedDict):
    field: str
    site: int
    before: DecoderInterface
    result: PrimitiveResult[DecoderOutcome]
    protected_constituents: GuardBindings
    predecessor: NotRequired[TailCall]


class ScalarCall(TypedDict):
    field: str
    site: int
    before: SequenceInterface
    result: PrimitiveResult[SequenceOutcome] | ColorResult
    protected_constituents: GuardBindings
    predecessor: TailCall


TailCall: TypeAlias = ActionCall | ButtonCall | DirectCall | ScalarCall


class OrdinaryReturn(TypedDict):
    return_PC: int
    return_SP: int
    preserved_high_gprs: tuple[str, ...]
    architectural_LR_preserved: bool


class OutputNone(TypedDict):
    tag: int
    after: SequenceInterface
    possible_writes: Writes
    physical_child_bytes: int
    source_guard: tuple[int, str]
    selected_local_path: tuple[int, ...]
    ordinary_return: OrdinaryReturn


class ScreenError(TypedDict):
    stage: str
    tag: int
    error: int | None
    origin: TailCall
    cause: NotRequired[ActionError | ButtonError | ActionNone | ButtonNone]
    child_outcome: NotRequired[DecoderOutcome]
    guard: NotRequired[str]
    cursor: Span
    after: SequenceInterface


class ScreenNormal(TypedDict):
    source_order: tuple[str, ...]
    buttons: tuple[ButtonNormal, ...]
    rotation_domain: tuple[int, int]
    click_domain: tuple[int, int]
    invocations: tuple[TailCall, ...]
    after: SequenceInterface
    physical_output_order: tuple[tuple[int, int, int], ...]
    outstanding_after_tag: tuple[tuple[int, int], ...]
    initialized_bytes: tuple[int, ...]
    completion_requires_all_stores_and_original_return: bool


class ScreenResult(ResultBase[ScreenNormal, OutputNone, ScreenError]):
    output: View
    invocations: tuple[TailCall, ...]
    requires_bound_Screen_word_access_policy: bool


class LedError(TypedDict):
    stage: str
    tag: int
    error: int | None
    origin: DirectCall | ScalarCall
    child_outcome: DecoderOutcome | SequenceOutcome | ColorOutcome
    guard: NotRequired[str]
    cursor: Span
    after: SequenceInterface


class LedNormal(TypedDict):
    fields: tuple[tuple[str, tuple[int, int]], ...]
    after: SequenceInterface
    invocations: tuple[TailCall, ...]
    initialized_bytes: tuple[int, ...]
    opaque_bytes: tuple[int, ...]
    physical_output_order: tuple[tuple[int, int, int], ...]
    completion_requires_all_stores_and_original_return: bool


class LedResult(ResultBase[LedNormal, OutputNone, LedError]):
    output: View
    invocations: tuple[TailCall, ...]
    requires_bound_LED_word_access_policy: bool


def _guards(frame: Storage, values: Sequence[Storage]) -> tuple[Storage, ...]:
    """Retain all guards and their physical meaning across coordinate spaces."""
    result = list(dict.fromkeys(values))
    require(
        all(type(v) is Storage and v.phase == frame.phase for v in result),
        "Tail protected lifetime differs",
    )
    for value in tuple(result):
        other = "address" if value.space != "address" else frame.space
        if not any(
            v.space == other
            and v.start <= value.start
            and value.start + value.extent <= v.start + v.extent
            for v in result
        ):
            result.append(
                Storage(
                    other,
                    "physical " + value.identity,
                    value.start,
                    value.extent,
                    1,
                    frame.phase,
                    False,
                )
            )
    return tuple(result)


def _guard_envelopes(values: Sequence[Storage]) -> tuple[tuple[Storage, ...], GuardBindings]:
    """Use containing guards for pairwise helper APIs, retaining every part.

    Only exact containment in the same space/lifetime can coalesce. No union
    span, retagging or partial-overlap waiver is introduced. The returned
    association is retained with each actual primitive invocation.
    """
    values = tuple(dict.fromkeys(values))
    envelopes = []
    bindings = []
    for value in values:
        require(type(value) is Storage, "untyped Tail protected constituent")
        candidates = tuple(
            v
            for v in values
            if v.space == value.space
            and v.phase == value.phase
            and v.start <= value.start
            and value.start + value.extent <= v.start + v.extent
        )
        envelope = max(candidates, key=lambda v: v.extent)
        if envelope not in envelopes:
            envelopes.append(envelope)
        bindings.append((value, envelope))
    return tuple(envelopes), tuple(bindings)


def _current(
    objects: DiagnosticObjects,
    rows: InstructionMap,
    proof: BodyRecord | TailParentRecord,
    shape: str,
) -> TailCoverageRecord:
    require(
        type(proof) is dict and proof.get("shape") == shape and proof.get("role") == ROLES[shape],
        "Tail effect lacks its complete current aggregate body",
    )
    current = tail_coverage(objects, _name(objects, ROLES[shape], proof["function"]), rows)
    require(current == proof, "Tail effect uses stale complete body/dispatch facts")
    if not is_child_coverage(current):
        raise AssertionError("Tail effect lacks its complete current aggregate body")
    return current


def _layout(
    state: SequenceInterface,
    frame: Storage,
    size: int,
    protected: Sequence[Storage],
) -> tuple[Storage, Storage, tuple[Storage, ...]]:
    require(
        type(state) is SequenceInterface
        and type(frame) is Storage
        and frame.writable
        and frame.space.startswith("stack:")
        and frame.extent == size
        and frame.alignment >= 8
        and state.header.storage.phase
        == frame.phase
        == state.decoder.header.storage.phase
        == state.decoder.payload.storage.phase
        and state.header.storage.space == frame.space == state.decoder.header.storage.space,
        "Tail aggregate loses its current Q/D/frame/source lifetime",
    )
    q = _object(state.header, "Tail caller Q")
    d = _object(state.decoder.header, "Tail D")
    guards = _guards(frame, tuple(protected) + (_object(state.decoder.payload, "immutable-input"),))
    require_disjoint((q, d))
    if state.count.upper:
        for guard in guards:
            require_disjoint((frame, guard))
        require_disjoint((frame, q, d))
    return q, d, guards


def _caller(
    objects: DiagnosticObjects,
    rows: InstructionMap,
    proof: TailCoverageRecord,
    call_site: int,
    state: SequenceInterface,
    frame: Storage,
    shape: str,
) -> TailCoverageRecord:
    require(
        type(call_site) is int and call_site in rows, "Tail child has no current caller instruction"
    )
    row = rows[call_site]
    require(
        row.mnemonic == "bl"
        and len(row.raw) == 4
        and _direct_target(row, "bl") == proof["function"]
        and objects.read_only(call_site, 4) == row.raw,
        "Tail child has another current ordinary target",
    )
    allowed: tuple[tuple[str, tuple[int, ...], int, int], ...]
    if shape == "ControlAction":
        allowed = (("ButtonBinding", (22, 48), 24, 0), ("ScreenBindings", (234,), 64, 12))
    elif shape == "ButtonBinding":
        allowed = (("ScreenBindings", (30, 62, 94, 122), 64, 20),)
    else:
        require(False, "unreviewed Tail scalar caller family")
    matches = []
    for parent, offsets, extent, q_offset in allowed:
        name = _name(objects, ROLES[parent])
        body = tail_coverage(objects, name, rows)
        if not is_child_coverage(body):
            raise AssertionError("Tail child call is not a selected current typed parent site")
        if call_site - body["function"] in offsets:
            require(
                (state.header.storage.extent, state.header.offset, state.header.extent)
                == (extent, q_offset, 8)
                and frame.start + frame.extent == state.header.storage.start,
                "Tail child caller receiver/frame differs",
            )
            matches.append(body)
    require(len(matches) == 1, "Tail child call is not a selected current typed parent site")
    return matches[0]


def _returns(
    objects: DiagnosticObjects,
    rows: InstructionMap,
    proof: BodyRecord,
    call_site: int,
    frame: Storage,
    guards: Sequence[Storage],
) -> tuple[ReturnNode, ...]:
    children = {(proof["function"], proof["role"]): proof, **tail_children(objects, rows, (proof,))}
    parent: ReturnParent = dict(
        function=call_site,
        ordinary_edges=((0, "bl", proof["function"], proof["role"], proof["frame"]),),
    )
    entry = frame.start + frame.extent
    nodes = _closed_child_returns(
        rows,
        (parent,),
        children,
        entry,
        tail_return_profiles(),
        (ROLES["ControlAction"], ROLES["ButtonBinding"], ROLES["LedPattern"]),
        objects=objects,
    )
    root = next(
        (
            n
            for n in nodes
            if n["role"] == proof["role"]
            and n["entry_SP"] == entry
            and n["return_PC"] == call_site + 4
        ),
        None,
    )
    if not (
        root is not None
        and root["return_SP"] == entry
        and root["preserved_high_gprs"] == tuple("r" + str(i) for i in range(4, 12))
    ):
        raise AssertionError("Tail aggregate loses original return/high-register lineage")
    active = Storage(
        frame.space,
        frame.identity + ":complete descendants",
        root["lowest_SP"],
        entry - root["lowest_SP"],
        8,
        frame.phase,
        True,
    )
    for guard in guards:
        require_disjoint((active, guard))
    return nodes


def _child_frame(frame: Storage, size: int, label: str) -> Storage:
    return Storage(
        frame.space, frame.identity + ":" + label, frame.start - size, size, 8, frame.phase, True
    )


def _join_decoders(options: Sequence[DecoderOutcome]) -> DecoderInterface:
    require(options, "Tail join has no source-valid successful predecessor")
    before = options[0]["state"]
    require(
        all(
            v["state"].header == before.header and v["state"].payload == before.payload
            for v in options
        ),
        "Tail join mixes independent decoder allocations",
    )
    return replace(
        before,
        cursor=Span(
            min(v["cursor"].lower for v in options), max(v["cursor"].upper for v in options)
        ),
    )


def control_action_interface(
    objects: DiagnosticObjects,
    rows: InstructionMap,
    proof: BodyRecord | TailParentRecord,
    state: SequenceInterface,
    frame: Storage,
    ancestor_protected: Sequence[Storage],
    call_site: int,
) -> ActionResult:
    """One Q element: actual varint domain guard, nineteen mapped ordinals."""
    proof = _current(objects, rows, proof, "ControlAction")
    q, d, guards = _layout(state, frame, 16, ancestor_protected)
    parent = _caller(objects, rows, proof, call_site, state, frame, "ControlAction")
    result: ActionResult = dict(
        proof=proof,
        before=state,
        normal=None,
        none=None,
        errors=(),
        primitive_invocations=(),
        nested_returns=(),
        parent=parent,
        call_site=call_site,
        protected=guards,
        requires_selected_parent_source_control=True,
    )
    if state.count.lower == 0:
        result["none"] = dict(
            status=0, action_low=255, after=replace(state, count=Span(0, 0)), possible_writes=()
        )
    if not state.count.upper:
        return result
    returns = _returns(objects, rows, proof, call_site, frame, guards + (q, d))
    saved = Storage(frame.space, "Action original r7/LR", frame.start + 8, 8, 4, frame.phase, False)
    scratch = borrow(frame, 0, 8, frame.phase, True)
    require_writes((state.header,), ((state.header, 4, 4),), guards + (d, saved))
    require_writes((scratch,), ((scratch, 0, 8),), guards + (q, d, saved))
    children = tail_children(objects, rows, (proof,))
    edge = proof["ordinary_edges"]
    require(len(edge) == 1 and edge[0][0] == 16, "Action changes its same-source varint call")
    scalar = children[edge[0][2], edge[0][3]]
    require(
        scalar.get("semantic_role") == "take_varint_u32", "Action loses its u32 selector source"
    )
    protected, bindings = _guard_envelopes(guards + (q, saved))
    child = decoder_interface(
        objects,
        rows,
        scalar,
        state.decoder,
        _child_frame(frame, 32, "varint"),
        protected,
        output=scratch,
    )
    origin: ActionPrimitive = dict(
        site=proof["function"] + 16,
        semantic_role="take_varint_u32",
        before=state.decoder,
        result=child,
        decrement_before_call=True,
        caller=call_site,
        protected_constituents=bindings,
    )
    count = Span(max(1, state.count.lower) - 1, state.count.upper - 1)
    errors: list[ActionError] = []
    success = []
    for option in child["outcomes"]:
        after = replace(state, count=count, decoder=option["state"])
        if option["kind"] == "success":
            require(
                option["value_domain"] == (0, 0xFFFFFFFF), "Action scalar domain is not source u32"
            )
            success.append(option)
            errors.append(
                dict(
                    stage="Action domain",
                    status=1,
                    error=14,
                    guard="u32 >18",
                    origin=origin,
                    child_outcome=option,
                    after=after,
                    cursor=option["cursor"],
                )
            )
        else:
            errors.append(
                dict(
                    stage="Action parse",
                    status=1,
                    error=option["error"],
                    origin=origin,
                    child_outcome=option,
                    after=after,
                    cursor=option["cursor"],
                )
            )
    if success:
        decoder = _join_decoders(success)
        result["normal"] = dict(
            status=0,
            domain=(0, 18),
            mapping=tuple(enumerate(ACTION_TARGET_OFFSETS)),
            guard="same successful u32 <=18",
            after=replace(state, count=count, decoder=decoder),
            origins=(origin,),
            successful_outcomes=tuple(success),
            possible_writes=((state.header, 4, 4),),
        )
    result.update(
        {"errors": tuple(errors), "primitive_invocations": (origin,), "nested_returns": returns}
    )
    return result


def button_binding_interface(
    objects: DiagnosticObjects,
    rows: InstructionMap,
    proof: BodyRecord | TailParentRecord,
    state: SequenceInterface,
    frame: Storage,
    ancestor_protected: Sequence[Storage],
    call_site: int,
) -> ButtonResult:
    """Ordered tap then hold; no independent-field Cartesian histories."""
    proof = _current(objects, rows, proof, "ButtonBinding")
    q, d, guards = _layout(state, frame, 24, ancestor_protected)
    parent = _caller(objects, rows, proof, call_site, state, frame, "ButtonBinding")
    result: ButtonResult = dict(
        proof=proof,
        before=state,
        normal=None,
        none=None,
        errors=(),
        invocations=(),
        nested_returns=(),
        parent=parent,
        call_site=call_site,
        protected=guards,
        requires_selected_parent_source_control=True,
    )
    if state.count.lower == 0:
        result["none"] = dict(
            low=255, upper_bits="opaque", after=replace(state, count=Span(0, 0)), possible_writes=()
        )
    if not state.count.upper:
        return result
    returns = _returns(objects, rows, proof, call_site, frame, guards + (q, d))
    saved = Storage(
        frame.space, "Button original low saves", frame.start + 8, 16, 4, frame.phase, False
    )
    local = borrow(frame, 0, 8, frame.phase, True)
    require_writes((local,), ((local, 0, 8),), guards + (saved, q, d))
    require_writes((state.header,), ((state.header, 4, 4),), guards + (saved, d))
    count = Span(max(1, state.count.lower) - 1, state.count.upper - 1)
    inner = SequenceInterface(local, state.decoder, Span(2, 2))
    invocations: list[ActionCall] = []
    errors: list[ButtonError] = []
    previous: ActionCall | None = None
    targets = {e[0]: e for e in proof["ordinary_edges"]}
    for field, off in (("tap", 22), ("hold", 48)):
        edge = targets.get(off)
        if edge is None or edge[3] != ROLES["ControlAction"]:
            raise AssertionError("Button changes its paired Action child")
        action = tail_coverage(objects, _name(objects, ROLES["ControlAction"], edge[2]), rows)
        child = control_action_interface(
            objects,
            rows,
            action,
            inner,
            _child_frame(frame, 16, "Action"),
            guards + (saved, q),
            proof["function"] + off,
        )
        origin: ActionCall = dict(
            field=field,
            site=proof["function"] + off,
            before=inner,
            result=child,
            predecessor=previous,
        )
        invocations.append(origin)
        for error in child["errors"]:
            errors.append(
                dict(
                    stage=field,
                    low=254,
                    error=error["error"],
                    origin=origin,
                    cause=error,
                    after=replace(state, count=count, decoder=error["after"].decoder),
                    cursor=error["cursor"],
                )
            )
        if child["none"] is not None:
            none = child["none"]
            errors.append(
                dict(
                    stage=field,
                    low=254,
                    error=14,
                    origin=origin,
                    cause=none,
                    after=replace(state, count=count, decoder=none["after"].decoder),
                    cursor=none["after"].decoder.cursor,
                )
            )
        if child["normal"] is None:
            break
        require(
            child["normal"]["domain"] == (0, 18),
            "Button Action was semantically restricted or widened",
        )
        inner = child["normal"]["after"]
        previous = origin
    else:
        require(inner.count == Span(0, 0), "Button omitted a source Action")
        result["normal"] = dict(
            fields=(("tap", (0, 18)), ("hold", (0, 18))),
            upper_bits="opaque; only low16 carry tap/hold",
            after=replace(state, count=count, decoder=inner.decoder),
            invocations=tuple(invocations),
            source_order=("tap", "hold"),
        )
    result.update(
        {"errors": tuple(errors), "invocations": tuple(invocations), "nested_returns": returns}
    )
    return result


def _visitor_caller(
    objects: DiagnosticObjects,
    rows: InstructionMap,
    proof: TailCoverageRecord,
    state: SequenceInterface,
    output: View,
    frame: Storage,
    call_site: int,
    shape: str,
) -> int:
    from config_headers import VISITOR

    name = _name(objects, VISITOR)
    V = objects.elf.functions[name][0] & ~1
    epochs = (
        {5712: 940, 6198: 1000, 6426: 1064}
        if shape == "ScreenBindings"
        else {6792: 1064, 6828: 1064}
    )
    q_offset = 1128 if shape == "ScreenBindings" else 1000
    require(
        type(call_site) is int
        and call_site - V in epochs
        and type(output) is View
        and output.writable
        and output.storage == state.header.storage
        and output.storage.extent == 1608
        and output.extent == 10
        and output.offset == epochs[call_site - V]
        and output.start % 4 == 0
        and (state.header.offset, state.header.extent) == (q_offset, 8)
        and frame.start + frame.extent == output.storage.start
        and state.decoder.header.start == output.storage.start + 1608 + 236,
        "Tail aggregate output/Q/D/current Visitor call epoch differs",
    )
    row = rows.get(call_site)
    require(
        row is not None
        and row.mnemonic == "bl"
        and len(row.raw) == 4
        and _direct_target(row, "bl") == proof["function"]
        and objects.read_only(call_site, 4) == row.raw,
        "Tail output aggregate has another current ordinary call",
    )
    return V


def _output_layout(
    output: View,
    frame: Storage,
    q: Storage,
    d: Storage,
    guards: tuple[Storage, ...],
    positive: int,
    writes: Sequence[tuple[int, int]],
) -> Storage:
    out = _object(output, "Tail aggregate output")
    require_disjoint((frame, q, d, out))
    require_writes(
        (output,),
        tuple((output, off, n) for off, n in (writes if positive else ((0, 1),))),
        guards + (q, d),
    )
    return out


def _direct_u32(
    objects: DiagnosticObjects,
    rows: InstructionMap,
    proof: TailCoverageRecord,
    off: int,
    before: DecoderInterface,
    output: View,
    frame: Storage,
    guards: Sequence[Storage],
    field: str,
) -> tuple[DirectCall, tuple[DecoderOutcome, ...]]:
    children = tail_children(objects, rows, (proof,))
    edge = next((e for e in proof["ordinary_edges"] if e[0] == off), None)
    if edge is None:
        raise AssertionError("Tail aggregate loses its actual direct scalar site")
    scalar = children[edge[2], edge[3]]
    require(
        scalar.get("semantic_role") == "take_varint_u32",
        "Tail aggregate direct selector changes decoder type",
    )
    protected, bindings = _guard_envelopes(guards)
    child = decoder_interface(
        objects, rows, scalar, before, _child_frame(frame, 32, "varint"), protected, output=output
    )
    origin: DirectCall = dict(
        field=field,
        site=proof["function"] + off,
        before=before,
        result=child,
        protected_constituents=bindings,
    )
    return origin, tuple(v for v in child["outcomes"] if v["kind"] == "success")


def _pushed_output_none(
    objects: DiagnosticObjects,
    rows: InstructionMap,
    proof: TailCoverageRecord,
    state: SequenceInterface,
    output: View,
    frame: Storage,
    guards: Sequence[Storage],
    call_site: int,
) -> OutputNone:
    """The two authored output families push BEFORE their outer count test.

    Universal fresh return equations include their selected missing path.
    The matched count-zero branch bypasses every descendant call; only this
    activation is physically live. No evaluated return proof is cached.
    """
    require(
        proof["shape"] in ("ScreenBindings", "LedPattern") and state.count.lower == 0,
        "Tail missing output has no authored count-zero branch",
    )
    for guard in guards:
        require_disjoint((frame, guard))
    nodes = _returns(objects, rows, proof, call_site, frame, ())
    root = next(n for n in nodes if n["role"] == proof["role"] and n["return_PC"] == call_site + 4)
    path = (
        (0, 2, 4, 8, 12, 52, 54, 246, 248, 250)
        if proof["shape"] == "ScreenBindings"
        else (0, 2, 4, 8, 12, 46, 48, 168, 170, 174)
    )
    return dict(
        tag=255,
        after=replace(state, count=Span(0, 0)),
        possible_writes=((output, 0, 1),),
        physical_child_bytes=frame.extent,
        source_guard=(proof["function"] + 12, "count==0"),
        selected_local_path=tuple(proof["function"] + off for off in path),
        ordinary_return=dict(
            return_PC=root["return_PC"],
            return_SP=root["return_SP"],
            preserved_high_gprs=root["preserved_high_gprs"],
            architectural_LR_preserved=False,
        ),
    )


def screen_bindings_interface(
    objects: DiagnosticObjects,
    rows: InstructionMap,
    proof: BodyRecord | TailParentRecord,
    state: SequenceInterface,
    output: View,
    frame: Storage,
    ancestor_protected: Sequence[Storage],
    call_site: int,
) -> ScreenResult:
    """Four ordered Button packages, rotation and click before ten-byte output."""
    proof = _current(objects, rows, proof, "ScreenBindings")
    q, d, guards = _layout(state, frame, 64, ancestor_protected)
    _visitor_caller(objects, rows, proof, state, output, frame, call_site, "ScreenBindings")
    out = _output_layout(output, frame, q, d, guards, state.count.upper, ((0, 10),))
    result: ScreenResult = dict(
        proof=proof,
        before=state,
        output=output,
        normal=None,
        none=None,
        errors=(),
        invocations=(),
        nested_returns=(),
        call_site=call_site,
        protected=guards,
        requires_selected_parent_source_control=True,
        requires_bound_Screen_word_access_policy=True,
    )
    if state.count.lower == 0:
        result["none"] = _pushed_output_none(
            objects, rows, proof, state, output, frame, guards, call_site
        )
    if not state.count.upper:
        return result
    returns = _returns(objects, rows, proof, call_site, frame, guards + (q, d, out))
    saved = Storage(
        frame.space, "Screen original high/low saves", frame.start + 28, 36, 4, frame.phase, False
    )
    pointer = Storage(
        frame.space, "Screen same D pointer", frame.start + 12, 4, 4, frame.phase, False
    )
    captured = Storage(
        frame.space, "Screen captured Buttons X/Y", frame.start + 4, 8, 4, frame.phase, False
    )
    local = borrow(frame, 0, 28, frame.phase, True)
    require_writes(
        (local,),
        tuple((local, a, n) for a, n in ((20, 8), (12, 4), (4, 8), (16, 4))),
        guards + (saved, q, d, out),
    )
    require_writes((state.header,), ((state.header, 4, 4),), guards + (saved, d, out))
    count = Span(max(1, state.count.lower) - 1, state.count.upper - 1)
    inner = SequenceInterface(borrow(frame, 20, 8, frame.phase, True), state.decoder, Span(4, 4))
    calls: list[TailCall] = []
    errors: list[ScreenError] = []
    buttons: list[ButtonNormal] = []

    def record_errors(origin: TailCall, options: Sequence[ActionError | ButtonError]) -> None:
        for error in options:
            errors.append(
                dict(
                    stage=origin["field"],
                    tag=254,
                    error=error["error"],
                    origin=origin,
                    cause=error,
                    cursor=error["after"].decoder.cursor,
                    after=replace(state, count=count, decoder=error["after"].decoder),
                )
            )

    for index, off in enumerate((30, 62, 94, 122)):
        edge = next((e for e in proof["ordinary_edges"] if e[0] == off), None)
        if edge is None or edge[3] != ROLES["ButtonBinding"]:
            raise AssertionError("Screen loses its paired Button call")
        child_proof = tail_coverage(objects, _name(objects, ROLES["ButtonBinding"], edge[2]), rows)
        protected = guards + (saved, q, out, pointer) + ((captured,) if index == 3 else ())
        child = button_binding_interface(
            objects,
            rows,
            child_proof,
            inner,
            _child_frame(frame, 24, "Button"),
            protected,
            proof["function"] + off,
        )
        origin: ButtonCall = dict(
            field=("buttonA", "buttonB", "buttonX", "buttonY")[index],
            site=proof["function"] + off,
            before=inner,
            result=child,
            predecessor=calls[-1] if calls else None,
        )
        calls.append(origin)
        record_errors(origin, child["errors"])
        if child["none"] is not None:
            none = child["none"]
            errors.append(
                dict(
                    stage=origin["field"],
                    tag=254,
                    error=14,
                    origin=origin,
                    cause=none,
                    cursor=none["after"].decoder.cursor,
                    after=replace(state, count=count, decoder=none["after"].decoder),
                )
            )
        if child["normal"] is None:
            result.update(
                {"errors": tuple(errors), "invocations": tuple(calls), "nested_returns": returns}
            )
            return result
        buttons.append(child["normal"])
        inner = child["normal"]["after"]
    require(inner.count == Span(0, 0), "Screen omits a fixed Button")
    # H20..28 changes from the retired Button Q to rotation Result8. H12
    # plus freshly stored H16 becomes a distinct counted click Q1.
    click_q = borrow(frame, 12, 8, frame.phase, True)
    click_header = Storage(
        frame.space, "Screen initialized click Q", frame.start + 12, 8, 4, frame.phase, False
    )
    rotation, normal = _direct_u32(
        objects,
        rows,
        proof,
        204,
        inner.decoder,
        borrow(frame, 20, 8, frame.phase, True),
        frame,
        guards + (saved, q, out, captured, click_header),
        "rotation",
    )
    rotation["predecessor"] = calls[-1]
    calls.append(rotation)
    for option in rotation["result"]["outcomes"]:
        if option["kind"] == "success":
            require(
                option["value_domain"] == (0, 0xFFFFFFFF), "Screen rotation loses source u32 domain"
            )
            errors.append(
                dict(
                    stage="rotation domain",
                    tag=254,
                    error=14,
                    guard="u32 >3",
                    origin=rotation,
                    child_outcome=option,
                    cursor=option["cursor"],
                    after=replace(state, count=count, decoder=option["state"]),
                )
            )
        else:
            errors.append(
                dict(
                    stage="rotation parse",
                    tag=254,
                    error=option["error"],
                    origin=rotation,
                    child_outcome=option,
                    cursor=option["cursor"],
                    after=replace(state, count=count, decoder=option["state"]),
                )
            )
    if normal:
        click = SequenceInterface(click_q, _join_decoders(normal), Span(1, 1))
        edge = next(e for e in proof["ordinary_edges"] if e[0] == 234)
        child_proof = tail_coverage(objects, _name(objects, ROLES["ControlAction"], edge[2]), rows)
        click_child = control_action_interface(
            objects,
            rows,
            child_proof,
            click,
            _child_frame(frame, 16, "click Action"),
            guards + (saved, q, out, captured),
            proof["function"] + 234,
        )
        click_origin: ActionCall = dict(
            field="click",
            site=proof["function"] + 234,
            before=click,
            result=click_child,
            predecessor=rotation,
        )
        calls.append(click_origin)
        record_errors(click_origin, click_child["errors"])
        if click_child["none"] is not None:
            click_none = click_child["none"]
            errors.append(
                dict(
                    stage="click",
                    tag=254,
                    error=14,
                    origin=click_origin,
                    cause=click_none,
                    cursor=click_none["after"].decoder.cursor,
                    after=replace(state, count=count, decoder=click_none["after"].decoder),
                )
            )
        if click_child["normal"] is not None:
            require(
                click_child["normal"]["after"].count == Span(0, 0), "Screen omits its click source"
            )
            result["normal"] = dict(
                source_order=("buttonA", "buttonB", "buttonX", "buttonY", "rotation", "click"),
                buttons=tuple(buttons),
                rotation_domain=(0, 3),
                click_domain=(0, 18),
                invocations=tuple(calls),
                after=replace(state, count=count, decoder=click_child["normal"]["after"].decoder),
                physical_output_order=((266, 1, 1), (268, 0, 1), (296, 6, 4), (306, 2, 4)),
                outstanding_after_tag=((2, 8),),
                initialized_bytes=tuple(range(10)),
                completion_requires_all_stores_and_original_return=True,
            )
    result.update({"errors": tuple(errors), "invocations": tuple(calls), "nested_returns": returns})
    return result


def led_pattern_interface(
    objects: DiagnosticObjects,
    rows: InstructionMap,
    proof: BodyRecord | TailParentRecord,
    state: SequenceInterface,
    output: View,
    frame: Storage,
    ancestor_protected: Sequence[Storage],
    call_site: int,
) -> LedResult:
    """All fields decode for every kind, including Off; padding stays opaque."""
    from config_widget_children import color_interface
    from config_widget_contracts import ROLES as WIDGET

    proof = _current(objects, rows, proof, "LedPattern")
    q, d, guards = _layout(state, frame, 48, ancestor_protected)
    _visitor_caller(objects, rows, proof, state, output, frame, call_site, "LedPattern")
    out = _output_layout(output, frame, q, d, guards, state.count.upper, ((0, 1), (2, 4), (6, 4)))
    result: LedResult = dict(
        proof=proof,
        before=state,
        output=output,
        normal=None,
        none=None,
        errors=(),
        invocations=(),
        nested_returns=(),
        call_site=call_site,
        protected=guards,
        requires_selected_parent_source_control=True,
        requires_bound_LED_word_access_policy=True,
    )
    if state.count.lower == 0:
        result["none"] = _pushed_output_none(
            objects, rows, proof, state, output, frame, guards, call_site
        )
    if not state.count.upper:
        return result
    returns = _returns(objects, rows, proof, call_site, frame, guards + (q, d, out))
    saved = Storage(
        frame.space, "LED original high/low saves", frame.start + 16, 32, 4, frame.phase, False
    )
    local = borrow(frame, 0, 16, frame.phase, True)
    require_writes((local,), ((local, 0, 16),), guards + (saved, q, d, out))
    require_writes((state.header,), ((state.header, 4, 4),), guards + (saved, d, out))
    count = Span(max(1, state.count.lower) - 1, state.count.upper - 1)
    local_q = borrow(frame, 0, 8, frame.phase, True)
    local_owner = _object(local_q, "LED fields Q")
    kind, success = _direct_u32(
        objects,
        rows,
        proof,
        28,
        state.decoder,
        borrow(frame, 8, 8, frame.phase, True),
        frame,
        guards + (saved, q, out, local_owner),
        "kind",
    )
    calls: list[TailCall] = [kind]
    errors: list[LedError] = []
    for option in kind["result"]["outcomes"]:
        if option["kind"] == "success":
            require(option["value_domain"] == (0, 0xFFFFFFFF), "LED kind loses source u32 domain")
            errors.append(
                dict(
                    stage="kind domain",
                    tag=254,
                    error=14,
                    guard="u32 >3",
                    origin=kind,
                    child_outcome=option,
                    cursor=option["cursor"],
                    after=replace(state, count=count, decoder=option["state"]),
                )
            )
        else:
            errors.append(
                dict(
                    stage="kind parse",
                    tag=254,
                    error=option["error"],
                    origin=kind,
                    child_outcome=option,
                    cursor=option["cursor"],
                    after=replace(state, count=count, decoder=option["state"]),
                )
            )
    if not success:
        result.update(
            {"errors": tuple(errors), "invocations": tuple(calls), "nested_returns": returns}
        )
        return result
    inner = SequenceInterface(local_q, _join_decoders(success), Span(4, 4))
    children = tail_children(objects, rows, (proof,))
    fields = (
        ("color", 62, "color", (0, 65535)),
        ("brightness", 90, "seq_u8", (0, 255)),
        ("period_ms", 116, "seq_u16", (0, 65535)),
        ("on_percent", 148, "seq_u8", (0, 255)),
    )
    for field, off, semantic, domain in fields:
        edge = next((e for e in proof["ordinary_edges"] if e[0] == off), None)
        if edge is None:
            raise AssertionError("LED loses a source field call")
        scalar = children[edge[2], edge[3]]
        protected, bindings = _guard_envelopes(guards + (saved, q, out))
        child: ColorResult | PrimitiveResult[SequenceOutcome]
        if semantic == "color":
            require(scalar["role"] == WIDGET["color"], "LED color changes actual typed child")
            child = color_interface(
                objects, rows, scalar, inner, _child_frame(frame, 8, "Color"), protected
            )
        else:
            require(
                scalar.get("semantic_role") == semantic,
                "LED scalar field changes actual typed child",
            )
            child = sequence_interface(
                objects, rows, scalar, inner, _child_frame(frame, 8, semantic), protected
            )
        origin: ScalarCall = dict(
            field=field,
            site=proof["function"] + off,
            before=inner,
            result=child,
            predecessor=calls[-1],
            protected_constituents=bindings,
        )
        calls.append(origin)
        normal = []
        for field_option in child["outcomes"]:
            if field_option["kind"] == "some":
                require(
                    field_option["value_domain"] == domain,
                    "LED field was restricted by later semantic validation",
                )
                normal.append(field_option)
            else:
                errors.append(
                    dict(
                        stage=field,
                        tag=254,
                        error=14 if field_option["kind"] == "none" else field_option["error"],
                        origin=origin,
                        child_outcome=field_option,
                        cursor=field_option["cursor"],
                        after=replace(state, count=count, decoder=field_option["state"].decoder),
                    )
                )
        if not normal:
            break
        reference = normal[0]["state"]
        require(
            all(
                v["state"].header == reference.header
                and v["state"].count == reference.count
                and v["state"].decoder.header == reference.decoder.header
                and v["state"].decoder.payload == reference.decoder.payload
                for v in normal
            ),
            "LED source join mixes field epochs/allocations",
        )
        inner = replace(
            reference,
            decoder=replace(
                reference.decoder,
                cursor=Span(
                    min(v["cursor"].lower for v in normal), max(v["cursor"].upper for v in normal)
                ),
            ),
        )
    else:
        require(inner.count == Span(0, 0), "LED omits one of its four payload fields")
        result["normal"] = dict(
            fields=(("kind", (0, 3)),)
            + tuple((field, domain) for field, off, semantic, domain in fields),
            after=replace(state, count=count, decoder=inner.decoder),
            invocations=tuple(calls),
            initialized_bytes=(0, 2, 3, 4, 6, 7, 8),
            opaque_bytes=(1, 5, 9),
            physical_output_order=((184, 0, 1), (186, 6, 4), (194, 2, 4)),
            completion_requires_all_stores_and_original_return=True,
        )
    result.update({"errors": tuple(errors), "invocations": tuple(calls), "nested_returns": returns})
    return result
