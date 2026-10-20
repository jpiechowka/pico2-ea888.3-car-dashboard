"""Ordered conditional Tail source packages, before parent publication.

All primitive and aggregate interfaces run freshly. These grouped relations
retain actual inputs, outcomes, child returns and protected outputs; the
selected current private parent/control join remains a separate obligation.
"""

from __future__ import annotations

from collections.abc import Mapping, Sequence
from typing import Generic, NotRequired, TypeAlias, TypeGuard, TypeVar, TypedDict
from dataclasses import replace
from config_body_types import BodyRecord, InstructionMap
from config_stack import DiagnosticObjects
from config_finite_returns import ReturnNode
from config_refill_children import OpaqueCopyResult
from config_abstract import PrimitiveResult, SequenceOutcome
from config_regions import View
from config_tail_children import is_parent_literals
from config_tail_effects import (
    ScreenResult,
    ScreenNormal,
    ScreenError,
    LedResult,
    LedNormal,
    LedError,
    OutputNone,
    Writes,
    GuardBindings,
)
from config_abstract import Span, SequenceInterface, sequence_interface
from config_regions import Storage, borrow, require_writes, require_disjoint
from config_decode_effects import _object
from config_headers import VISITOR
from config_owner_entry import _direct_target
from config_tail_children import tail_children, tail_coverage
from config_tail_contracts import ROLES
from config_tail_effects import (
    _guards,
    _guard_envelopes,
    _returns,
    screen_bindings_interface,
    led_pattern_interface,
)
from config_widget_children import _name
from validation import require


CallResult = TypeVar("CallResult")


class SourceCall(TypedDict, Generic[CallResult]):
    field: str
    site: int
    before: SequenceInterface
    result: CallResult
    predecessor: SourceInvocation | ControlsEffects | None
    protected_constituents: NotRequired[GuardBindings]
    nested_returns: NotRequired[tuple[ReturnNode, ...]]
    stable_Q_D: NotRequired[tuple[Storage, ...]]
    normal_after: NotRequired[SequenceInterface | None]
    output: NotRequired[View]


ScalarInvocation: TypeAlias = SourceCall[PrimitiveResult[SequenceOutcome]]
ScreenInvocation: TypeAlias = SourceCall[ScreenResult]
LedInvocation: TypeAlias = SourceCall[LedResult]
SourceInvocation: TypeAlias = ScalarInvocation | ScreenInvocation | LedInvocation


class MissingCause(TypedDict):
    after: SequenceInterface
    cursor: Span
    none: OutputNone


SourceCause: TypeAlias = SequenceOutcome | ScreenError | LedError | MissingCause


class SourceError(TypedDict):
    field: str
    origin: SourceInvocation
    cause: SourceCause
    error: int | None
    cursor: Span
    after: SequenceInterface
    Config_writes: NotRequired[Writes]
    completed_groups: NotRequired[tuple[str, ...]]


class ScreenPackage(TypedDict):
    invocation: ScreenInvocation
    source: ScreenNormal
    output: View
    original_return: tuple[ReturnNode, ...]


class ControlsNormal(TypedDict):
    hold: ScalarInvocation
    hold_domain: tuple[int, int]
    hold_outcomes: tuple[SequenceOutcome, ...]
    screens: tuple[ScreenPackage, ...]
    ControlQ: SequenceInterface
    ScreenQ: SequenceInterface
    after: SequenceInterface
    requires_all_parent_staging_and_Config_stores: bool


class ControlsSource(TypedDict):
    before: SequenceInterface
    invocations: tuple[ScalarInvocation | ScreenInvocation, ...]
    normal: ControlsNormal | None
    errors: tuple[SourceError, ...]
    Config_writes: Writes
    top_decrement: tuple[int, Span, Span]
    ControlQ_initialization: tuple[int, View, Span]
    requires_selected_parent_source_control: bool


class InlineTransport(TypedDict):
    site: int
    source: View
    destination: View
    possible_writes: Writes


class CopyTransport(TypedDict):
    site: int
    source: View
    destination: View
    effect: OpaqueCopyResult
    nested_returns: tuple[ReturnNode, ...]


class ControlStage(TypedDict):
    site: int
    possible_writes: Writes
    changed_bytes: tuple[int, ...]
    completed_screens: int
    complete_Controls: bool


class WordAccess(TypedDict):
    instruction: int
    view: View


class ControlsEffects(TypedDict):
    source: ControlsSource
    owner: Storage
    frame: Storage
    copies: tuple[CopyTransport, ...]
    inline_transports: tuple[InlineTransport, ...]
    Config_stages: tuple[ControlStage, ...]
    complete_after: int | None
    first_only_words: tuple[WordAccess, ...]
    completed_ordinary_group_issued: bool
    requires_selected_parent_source_control: bool
    requires_bound_first_Screen_word_access_policy: bool


class HistoryNormal(TypedDict):
    after: SequenceInterface
    domain: tuple[int, int]
    values: tuple[ScalarInvocation, ...]
    Q_retired_after: int


class HistorySource(TypedDict):
    invocations: tuple[ScalarInvocation, ...]
    normal: HistoryNormal | None


class ReducedNormal(TypedDict):
    after: SequenceInterface
    domain: tuple[int, int]
    outcomes: tuple[SequenceOutcome, ...]


class ReducedSource(TypedDict):
    invocation: ScalarInvocation
    normal: ReducedNormal | None


class PatternPackage(TypedDict):
    invocation: LedInvocation
    source: LedNormal
    output: View
    opaque_bytes: tuple[int, ...]
    original_return: tuple[ReturnNode, ...]


class LEDNormal(TypedDict):
    after: SequenceInterface
    patterns: tuple[PatternPackage, ...]
    bools: tuple[ScalarInvocation, ...]


class LEDSource(TypedDict):
    invocations: tuple[LedInvocation | ScalarInvocation, ...]
    patterns: tuple[PatternPackage, ...]
    normal: LEDNormal | None


class SuffixNormal(TypedDict):
    after: SequenceInterface
    LEDQ: SequenceInterface
    physical_top_count: int
    logical_remaining_fields: int
    old_history_Q_still_live: bool
    source: LEDNormal
    requires_all_late_LED_stores_and_original_Visitor_restore: bool


class SuffixSource(TypedDict):
    controls: ControlsEffects
    before: SequenceInterface
    invocations: tuple[SourceInvocation, ...]
    errors: tuple[SourceError, ...]
    history: HistorySource | None
    reduced: ReducedSource | None
    LED: LEDSource | None
    normal: SuffixNormal | None
    requires_selected_parent_source_control: bool
    completed_ordinary_group_issued: bool


class HistoryStage(TypedDict):
    site: int
    source: ScalarInvocation
    source_index: int
    possible_writes: Writes
    changed_bytes: tuple[int, ...]
    complete_history: bool


class ReducedStage(TypedDict):
    site: int
    source: ScalarInvocation
    possible_writes: Writes


class LEDStage(TypedDict):
    site: int
    source: PatternPackage | LedInvocation | ScalarInvocation
    possible_writes: Writes
    changed_bytes: tuple[int, ...]
    complete_LED: bool


class SuffixEffects(TypedDict):
    source: SuffixSource
    owner: Storage
    history: tuple[HistoryStage, ...]
    reduced: ReducedStage | None
    LED: tuple[LEDStage, ...]
    complete_after: int | None
    early_success_register: int | None
    preserved_LED_padding: tuple[int, ...]
    written_opaque_LED_bytes: tuple[int, ...]
    completed_ordinary_group_issued: bool
    requires_selected_parent_source_control: bool
    requires_bound_LED_word_access_policy: bool


def _is_scalar_call(call: SourceInvocation) -> TypeGuard[ScalarInvocation]:
    return "outcomes" in call["result"]


def _is_screen_call(call: SourceInvocation) -> TypeGuard[ScreenInvocation]:
    return "requires_bound_Screen_word_access_policy" in call["result"]


def _is_led_call(call: SourceInvocation) -> TypeGuard[LedInvocation]:
    return "requires_bound_LED_word_access_policy" in call["result"]


def _is_sequence_outcome(cause: SourceCause) -> TypeGuard[SequenceOutcome]:
    return "state" in cause


def _cause_after(cause: SourceCause) -> SequenceInterface:
    if _is_sequence_outcome(cause):
        return cause["state"]
    after: object = cause.get("after")
    if not isinstance(after, SequenceInterface):
        raise AssertionError("Tail error cause lacks its sequence state")
    return after


def _joined_sequence(options: Sequence[SequenceOutcome]) -> SequenceInterface:
    require(options, "Tail sequence join has no successful source")
    first = options[0]["state"]
    require(
        all(
            v["state"].header == first.header
            and v["state"].count == first.count
            and v["state"].decoder.header == first.decoder.header
            and v["state"].decoder.payload == first.decoder.payload
            for v in options
        ),
        "Tail sequence join mixes source/count/allocation epochs",
    )
    return replace(
        first,
        decoder=replace(
            first.decoder,
            cursor=Span(
                min(v["cursor"].lower for v in options), max(v["cursor"].upper for v in options)
            ),
        ),
    )


def _parent_inputs(
    objects: DiagnosticObjects,
    rows: InstructionMap,
    parents: tuple[BodyRecord, ...],
    state: SequenceInterface,
    frame: Storage,
) -> int:
    require(
        type(state) is SequenceInterface
        and type(frame) is Storage
        and frame.extent == 1608
        and frame.writable
        and frame.alignment >= 8
        and frame.space.startswith("stack:")
        and (state.header.storage, state.header.offset, state.header.extent) == (frame, 288, 8)
        and state.decoder.header.start == frame.start + 1608 + 236
        and state.decoder.header.storage.phase == state.decoder.payload.storage.phase == frame.phase
        and state.decoder.header.storage.space == frame.space,
        "Tail source has another current Visitor Q/D/lifetime",
    )
    V = objects.elf.functions[_name(objects, VISITOR)][0] & ~1
    current = tail_coverage(objects, _name(objects, VISITOR, V), rows)
    require(
        is_parent_literals(current)
        and current["kind"] == "tail_parent_literal_windows_only"
        and current["windows"] == parents,
        "Tail source parent literal receipt is stale or from another current package",
    )
    require(
        tuple(p["function"] - V for p in parents) == (170, 5630, 5922, 6018, 6412, 6450, 7000, 56)
        and all(p["role"] == VISITOR and p["frame"] == 1608 for p in parents),
        "Tail source loses its authored parent windows",
    )
    for p in parents:
        for off, op, target, role, size in p["ordinary_edges"]:
            at = p["function"] + off
            row = rows.get(at)
            require(
                row is not None
                and row.mnemonic == op
                and _direct_target(row, "bl" if op == "bl" else "b") == target
                and objects.read_only(at, len(row.raw)) == row.raw,
                "Tail source has another current parent call",
            )
    return V


def _call_binding(
    parents: tuple[BodyRecord, ...],
    children: Mapping[tuple[int, str], BodyRecord],
    call: SourceInvocation,
    frame: Storage,
) -> ReturnNode:
    """Retained result proof and original return belong to this typed BL."""
    edge = next(
        (e for p in parents for e in p["ordinary_edges"] if p["function"] + e[0] == call["site"]),
        None,
    )
    proof = call["result"]["proof"]
    require(
        edge is not None
        and edge[1] == "bl"
        and proof == children.get((edge[2], edge[3]))
        and (proof["function"], proof["role"], proof["frame"]) == (edge[2], edge[3], edge[4]),
        "Tail retained source result has another current typed call target",
    )
    if edge is None:
        raise AssertionError("Tail retained source result has another current typed call target")
    nodes = call.get("nested_returns", call["result"].get("nested_returns", ()))
    roots = tuple(
        v
        for v in nodes
        if v["function"] == edge[2]
        and v["role"] == edge[3]
        and v["entry_SP"] == frame.start
        and v["return_PC"] == call["site"] + 4
        and v["return_SP"] == frame.start
    )
    require(
        len(roots) == 1
        and roots[0]["preserved_high_gprs"] == tuple("r" + str(i) for i in range(4, 12)),
        "Tail retained source call has another original return/high-GPR transfer",
    )
    return roots[0]


def _controls_source(
    objects: DiagnosticObjects,
    rows: InstructionMap,
    parents: tuple[BodyRecord, ...],
    state: SequenceInterface,
    frame: Storage,
    ancestor_protected: Sequence[Storage],
) -> ControlsSource:
    """hold_ms then three complete Screens, with fresh same-source cursors.

    Top4 and literal initialization establish top3, ControlQ2 and ScreenQ3.
    The first two Screens remain protected through the third original return.
    No Config store or selected-parent execution authority is issued here.
    """
    V = _parent_inputs(objects, rows, parents, state, frame)
    require(state.count == Span(4, 4), "Tail Controls source lacks its actual top4 predecessor")
    guards = _guards(
        frame, tuple(ancestor_protected) + (_object(state.decoder.payload, "immutable-input"),)
    )
    top = _object(state.header, "Tail top Q")
    require_writes(
        (state.header,),
        ((state.header, 4, 4),),
        guards + (_object(state.decoder.header, "Tail D"),),
    )
    control = borrow(frame, 828, 8, frame.phase, True)
    require_writes(
        (control,), ((control, 0, 8),), guards + (top, _object(state.decoder.header, "Tail D"))
    )
    inner = SequenceInterface(control, state.decoder, Span(2, 2))
    children = tail_children(objects, rows, parents)
    edge = next(
        (e for p in parents for e in p["ordinary_edges"] if p["function"] + e[0] == V + 5656), None
    )
    if edge is None:
        raise AssertionError("Tail hold_ms loses its actual parent call")
    scalar = children[edge[2], edge[3]]
    require(
        scalar.get("semantic_role") == "seq_u16" and scalar["frame"] == 8,
        "Tail hold_ms has another scalar source",
    )
    activation = Storage(
        frame.space, "Tail hold_ms child", frame.start - 8, 8, 8, frame.phase, True
    )
    protected, bindings = _guard_envelopes(guards + (top,))
    returns = _returns(
        objects,
        rows,
        scalar,
        V + 5656,
        activation,
        guards + (top, _object(control, "Control Q"), _object(state.decoder.header, "Tail D")),
    )
    child = sequence_interface(objects, rows, scalar, inner, activation, protected)
    origin: ScalarInvocation = dict(
        field="hold_ms",
        site=V + 5656,
        before=inner,
        result=child,
        predecessor=None,
        protected_constituents=bindings,
        nested_returns=returns,
    )
    calls: list[ScalarInvocation | ScreenInvocation] = [origin]
    errors: list[SourceError] = []

    def record(call: SourceInvocation, cause: SourceCause, code: int | None) -> None:
        after = _cause_after(cause)
        errors.append(
            dict(
                field=call["field"],
                origin=call,
                cause=cause,
                error=code,
                cursor=cause["cursor"],
                after=replace(state, count=Span(3, 3), decoder=after.decoder),
                Config_writes=(),
            )
        )

    options = []
    for option in child["outcomes"]:
        if option["kind"] == "some":
            require(
                option["value_domain"] == (0, 65535),
                "Tail hold_ms was restricted by later semantic validation",
            )
            options.append(option)
        else:
            record(origin, option, 14 if option["kind"] == "none" else option["error"])
    result: ControlsSource = dict(
        before=state,
        invocations=(),
        normal=None,
        errors=(),
        Config_writes=(),
        top_decrement=(V + 5646, Span(4, 4), Span(3, 3)),
        ControlQ_initialization=(V + 5650, control, Span(2, 2)),
        requires_selected_parent_source_control=True,
    )
    if not options:
        result.update({"invocations": tuple(calls), "errors": tuple(errors)})
        return result
    control_after = _joined_sequence(options)
    require(
        control_after.count == Span(1, 1), "Tail ControlQ count does not follow the hold result"
    )
    screen_q = borrow(frame, 1128, 8, frame.phase, True)
    require_writes(
        (screen_q,),
        ((screen_q, 0, 8),),
        guards
        + (top, _object(control, "retained Control Q"), _object(state.decoder.header, "Tail D")),
    )
    current = SequenceInterface(screen_q, control_after.decoder, Span(3, 3))
    screens: list[ScreenPackage] = []
    for index, (site, offset) in enumerate(((5712, 940), (6198, 1000), (6426, 1064))):
        edge = next(
            (e for p in parents for e in p["ordinary_edges"] if p["function"] + e[0] == V + site),
            None,
        )
        if not (edge is not None and edge[3] == ROLES["ScreenBindings"]):
            raise AssertionError("Tail ordered Screen loses its typed current call")
        proof = tail_coverage(objects, _name(objects, edge[3], edge[2]), rows)
        output = borrow(frame, offset, 10, frame.phase, True)
        retained = (
            guards
            + (top, _object(control, "retained Control Q"))
            + tuple(
                _object(v["output"], "completed Screen " + str(j)) for j, v in enumerate(screens)
            )
        )
        activation = Storage(
            frame.space, "Tail Screen " + str(index), frame.start - 64, 64, 8, frame.phase, True
        )
        screen_child = screen_bindings_interface(
            objects, rows, proof, current, output, activation, retained, V + site
        )
        call: ScreenInvocation = dict(
            field="Screen" + str(index),
            site=V + site,
            before=current,
            result=screen_child,
            output=output,
            predecessor=calls[-1],
        )
        calls.append(call)
        for error in screen_child["errors"]:
            record(call, error, error["error"])
        if screen_child["none"] is not None:
            none = screen_child["none"]
            record(
                call, dict(after=none["after"], cursor=none["after"].decoder.cursor, none=none), 14
            )
        if screen_child["normal"] is None:
            break
        normal = screen_child["normal"]
        screens.append(
            dict(
                invocation=call,
                source=normal,
                output=output,
                original_return=tuple(
                    n
                    for n in screen_child["nested_returns"]
                    if n["role"] == ROLES["ScreenBindings"] and n["return_PC"] == V + site + 4
                ),
            )
        )
        require(
            len(screens[-1]["original_return"]) == 1,
            "Tail Screen loses its original same-invocation return",
        )
        current = normal["after"]
    else:
        require(current.count == Span(0, 0), "Tail Controls omits a Screen source")
        result["normal"] = dict(
            hold=origin,
            hold_domain=(0, 65535),
            hold_outcomes=tuple(options),
            screens=tuple(screens),
            ControlQ=control_after,
            ScreenQ=current,
            after=replace(state, count=Span(3, 3), decoder=current.decoder),
            requires_all_parent_staging_and_Config_stores=True,
        )
    result.update({"invocations": tuple(calls), "errors": tuple(errors)})
    return result


def _controls_effects(
    objects: DiagnosticObjects,
    rows: InstructionMap,
    parents: tuple[BodyRecord, ...],
    source: ControlsSource,
    owner: Storage,
    frame: Storage,
    ancestor_protected: Sequence[Storage],
) -> ControlsEffects:
    """Conditional complete parse -> actual staging -> partial Controls stores.

    The active private parent must still bind this relation to its selected
    execution, original source and reached first-only word-access policies.
    Copied bytes carry their same Screen origin; copies do not initialize
    arbitrary opaque bytes or establish an acquired owner.
    """
    from config_decoded_tail import _parents
    from config_refill_children import opaque_copy_interface

    state = source["before"]
    V = _parent_inputs(objects, rows, parents, state, frame)
    require(
        type(owner) is Storage
        and owner.space == "address"
        and owner.writable
        and owner.extent == 14604
        and owner.phase == frame.phase
        and parents == _parents(objects, rows, V, owner.start),
        "Tail Controls publication loses its same current Config",
    )
    require_disjoint(
        (
            owner,
            Storage(
                "address",
                "Tail Controls full physical activation",
                frame.start - 152,
                152 + 1608 + 456,
                8,
                frame.phase,
                False,
            ),
        )
    )
    result: ControlsEffects = dict(
        source=source,
        owner=owner,
        frame=frame,
        copies=(),
        inline_transports=(),
        Config_stages=(),
        complete_after=None,
        first_only_words=(),
        completed_ordinary_group_issued=False,
        requires_selected_parent_source_control=True,
        requires_bound_first_Screen_word_access_policy=True,
    )
    normal = source["normal"]
    if normal is None:
        return result
    calls = source["invocations"]
    screens = normal["screens"]
    require(
        len(calls) == 4
        and len(screens) == 3
        and normal["hold"] is calls[0]
        and tuple(v["site"] - V for v in calls) == (5656, 5712, 6198, 6426)
        and normal["ControlQ"].count == Span(1, 1)
        and normal["ScreenQ"].count == Span(0, 0)
        and normal["after"].count == Span(3, 3),
        "Tail Controls publication precedes its complete ordered source",
    )
    children = tail_children(objects, rows, parents)
    for call in calls:
        _call_binding(parents, children, call, frame)
    for index, screen in enumerate(screens):
        call = calls[index + 1]
        if not _is_screen_call(call):
            raise AssertionError(
                "Tail Controls publication mixes a Screen output/source/original-return epoch"
            )
        child = call["result"]
        parsed = child["normal"]
        previous = calls[index]
        require(
            screen["invocation"] is call
            and screen["source"] is parsed
            and parsed is not None
            and screen["output"] is call["output"]
            and call["before"] is child["before"]
            and call["predecessor"] is previous
            and (screen["output"].storage, screen["output"].offset, screen["output"].extent)
            == (frame, (940, 1000, 1064)[index], 10)
            and parsed["initialized_bytes"] == tuple(range(10))
            and parsed["physical_output_order"]
            == ((266, 1, 1), (268, 0, 1), (296, 6, 4), (306, 2, 4))
            and len(screen["original_return"]) == 1
            and screen["original_return"][0]["return_PC"] == call["site"] + 4
            and screen["original_return"][0]["return_SP"] == frame.start,
            "Tail Controls publication mixes a Screen output/source/original-return epoch",
        )
        if parsed is None:
            raise AssertionError(
                "Tail Controls publication mixes a Screen output/source/original-return epoch"
            )
        before_decoder = (
            normal["ControlQ"].decoder
            if index == 0
            else screens[index - 1]["source"]["after"].decoder
        )
        require(
            call["before"].decoder is before_decoder
            and parsed["after"].count == Span(2 - index, 2 - index),
            "Tail Controls publication changes its same-source Screen predecessor",
        )
    require(
        normal["after"].decoder is screens[-1]["source"]["after"].decoder,
        "Tail Controls publication has another final source cursor",
    )
    guards = _guards(
        frame,
        tuple(ancestor_protected)
        + (
            _object(state.decoder.header, "Tail retained D"),
            _object(state.decoder.payload, "immutable-input"),
            _object(state.header, "Tail top Q"),
            _object(normal["ControlQ"].header, "retired Control Q"),
            _object(normal["ScreenQ"].header, "retired Screen Q"),
        )
        + tuple(_object(v["output"], "returned Screen " + str(i)) for i, v in enumerate(screens)),
    )
    fixed = tuple(
        Storage(
            space,
            "Config outside Controls " + str(off),
            owner.start + off,
            size,
            1,
            frame.phase,
            False,
        )
        for off, size in ((0, 212), (244, 14360))
        for space in ("address", frame.space)
    )
    guards += fixed
    third_rotation = borrow(frame, 284, 4, frame.phase, True)
    require_writes((third_rotation,), ((third_rotation, 0, 4),), guards)
    guards += _guards(frame, (_object(third_rotation, "captured third Screen rotation"),))
    copies: list[CopyTransport] = []
    inline: list[InlineTransport] = []

    def inline_copy(site: int, src: int, dest: int, size: int) -> None:
        inp = borrow(frame, src, size, frame.phase)
        out = borrow(frame, dest, size, frame.phase, True)
        require_writes(
            (out,), ((out, 0, size),), guards + (_object(inp, "current Controls inline source"),)
        )
        inline.append(
            dict(site=V + site, source=inp, destination=out, possible_writes=((out, 0, size),))
        )

    def copied(
        site: int, src: int, dest: int, target_owner: Storage, protected: tuple[Storage, ...]
    ) -> None:
        edge = next(
            (e for p in parents for e in p["ordinary_edges"] if p["function"] + e[0] == V + site),
            None,
        )
        if not (edge is not None and edge[3] == "__aeabi_memcpy"):
            raise AssertionError("Tail Controls copy loses its actual typed current target")
        inp = borrow(frame, src, 9, frame.phase)
        out = borrow(target_owner, dest, 9, frame.phase, True)
        effect = opaque_copy_interface(objects, rows, edge[2], inp, out, 9, frame, protected)
        proof = children[edge[2], edge[3]]
        activation = Storage(
            frame.space,
            "Tail Controls copy original saves",
            frame.start - 8,
            8,
            8,
            frame.phase,
            True,
        )
        returns = _returns(
            objects,
            rows,
            proof,
            V + site,
            activation,
            protected + (_object(inp, "live Controls copied source"),),
        )
        copies.append(
            dict(site=V + site, source=inp, destination=out, effect=effect, nested_returns=returns)
        )

    # Actual third output copy returns before either first-output word load;
    # the inherited second pointer is read at6500 before the second copy.
    copied(6492, 1065, 516, frame, guards)
    inline_copy(6508, 946, 604, 4)
    inline_copy(6514, 942, 600, 4)
    copied(
        6518,
        1001,
        628,
        frame,
        guards + (_object(borrow(frame, 516, 9, frame.phase), "third transported Screen"),),
    )
    for spec in (
        (5936, 600, 712, 8),
        (5944, 628, 880, 8),
        (5950, 636, 888, 1),
        (5956, 516, 744, 8),
        (5960, 524, 752, 1),
        (6026, 712, 480, 8),
        (6036, 880, 468, 8),
        (6042, 888, 476, 1),
        (6048, 744, 456, 8),
        (6056, 752, 464, 1),
    ):
        inline_copy(*spec)
    stages: list[ControlStage] = []
    changed: set[int] = set()
    # (current instruction, Config start, width, number of complete Screens).
    for site, offset, size, completed in (
        (6062, 213, 1, 0),
        (6066, 212, 1, 0),
        (6070, 222, 1, 0),
        (6074, 218, 4, 0),
        (6078, 214, 4, 1),
        (6088, 223, 9, 2),
        (6098, 232, 1, 2),
        (6106, 233, 9, 3),
        (6114, 242, 2, 3),
    ):
        current = borrow(owner, offset, size, frame.phase, True)
        prior = tuple(
            Storage(
                "address",
                "already written Controls byte " + str(off),
                owner.start + off,
                1,
                1,
                frame.phase,
                False,
            )
            for off in sorted(changed)
        )
        suffix = tuple(
            Storage(
                "address",
                "unwritten Controls byte " + str(off),
                owner.start + off,
                1,
                1,
                frame.phase,
                False,
            )
            for off in range(212, 244)
            if off not in changed and not offset <= off < offset + size
        )
        if site in (6088, 6106):
            copied(site, 468 if site == 6088 else 456, offset, owner, guards + prior + suffix)
        else:
            require_writes((current,), ((current, 0, size),), guards + prior + suffix)
        changed.update(range(offset, offset + size))
        stages.append(
            dict(
                site=V + site,
                possible_writes=((current, 0, size),),
                changed_bytes=tuple(sorted(changed)),
                completed_screens=completed,
                complete_Controls=site == 6114,
            )
        )
    require(changed == set(range(212, 244)), "Tail Controls assignment misses a physical byte")
    result.update(
        {
            "copies": tuple(copies),
            "inline_transports": tuple(inline),
            "Config_stages": tuple(stages),
            "complete_after": V + 6118,
            "first_only_words": (
                dict(instruction=V + 6496, view=borrow(frame, 946, 4, frame.phase)),
                dict(instruction=V + 6510, view=borrow(frame, 942, 4, frame.phase)),
                dict(instruction=V + 6074, view=borrow(owner, 218, 4, frame.phase, True)),
                dict(instruction=V + 6078, view=borrow(owner, 214, 4, frame.phase, True)),
            ),
        }
    )
    return result


def _history_led_source(
    objects: DiagnosticObjects,
    rows: InstructionMap,
    parents: tuple[BodyRecord, ...],
    controls: ControlsEffects,
    frame: Storage,
    ancestor_protected: Sequence[Storage],
) -> SuffixSource:
    """The fixed eight history elements, reduced bool and complete LED fields.

    This relation is conditional on the same complete Controls assignment.
    It retains error origins and already completed physical group milestones.
    It never reinterprets the retired History Q after LED Result reuses it.
    """
    require(
        controls["frame"] is frame
        and controls["source"]["normal"] is not None
        and controls["complete_after"] is not None
        and controls["Config_stages"][-1]["complete_Controls"],
        "Tail history lacks its complete same-frame Controls predecessor",
    )
    control_source = controls["source"]["normal"]
    if control_source is None:
        raise AssertionError("Tail history lacks its complete same-frame Controls predecessor")
    state = control_source["after"]
    V = _parent_inputs(objects, rows, parents, state, frame)
    require(
        state.count == Span(3, 3) and controls["complete_after"] == V + 6118,
        "Tail history loses actual top3 after Controls",
    )
    children = tail_children(objects, rows, parents)
    owner = controls["owner"]
    phase = frame.phase
    base = _guards(
        frame,
        tuple(ancestor_protected)
        + (
            replace(owner, writable=False),
            _object(state.decoder.payload, "immutable-input"),
            _object(borrow(frame, 288, 4, phase), "stable Tail top D pointer"),
            _object(control_source["ControlQ"].header, "retired Control Q"),
            _object(control_source["ScreenQ"].header, "retired Screen Q"),
        ),
    )
    calls: list[SourceInvocation] = []
    errors: list[SourceError] = []
    result: SuffixSource = dict(
        controls=controls,
        before=state,
        invocations=(),
        errors=(),
        history=None,
        reduced=None,
        LED=None,
        normal=None,
        requires_selected_parent_source_control=True,
        completed_ordinary_group_issued=False,
    )

    def finish() -> SuffixSource:
        result.update({"invocations": tuple(calls), "errors": tuple(errors)})
        return result

    def edge_at(site: int, semantic: str) -> BodyRecord:
        edge = next(
            (e for p in parents for e in p["ordinary_edges"] if p["function"] + e[0] == V + site),
            None,
        )
        if edge is None:
            raise AssertionError("Tail fixed suffix loses its actual source call")
        proof = children[edge[2], edge[3]]
        require(
            proof.get("semantic_role") == semantic, "Tail fixed suffix changes its source semantic"
        )
        return proof

    def record(
        call: SourceInvocation,
        cause: SourceCause,
        code: int | None,
        count: int,
        completed: tuple[str, ...],
    ) -> None:
        after = _cause_after(cause)
        errors.append(
            dict(
                field=call["field"],
                origin=call,
                cause=cause,
                error=code,
                cursor=cause["cursor"],
                after=replace(state, count=Span(count, count), decoder=after.decoder),
                completed_groups=completed,
            )
        )

    def scalar(
        field: str,
        site: int,
        current: SequenceInterface,
        semantic: str,
        guards: tuple[Storage, ...],
        count: int,
        completed: tuple[str, ...],
    ) -> tuple[ScalarInvocation, SequenceInterface | None, tuple[SequenceOutcome, ...]]:
        proof = edge_at(site, semantic)
        # The primitive API reserves all eight Q bytes; its actual output
        # tuples write only the count. Keep the authored stable top-D cell
        # as an exact post-interface write obligation for this one top call.
        # No generic disjoint/write contract is weakened.
        stable = tuple(
            g
            for g in guards
            if site == 6736
            and current.header == state.header
            and g.start == frame.start + 288
            and g.extent == 4
            and g.phase == phase
            and not g.writable
            and g.space in ("address", frame.space)
        )
        if site == 6736:
            require(len(stable) == 2, "Tail reduced call lost both stable top-D coordinate guards")
        protected, bindings = _guard_envelopes(tuple(g for g in guards if g not in stable))
        activation = Storage(
            frame.space,
            "Tail " + field + " scalar",
            frame.start - proof["frame"],
            proof["frame"],
            8,
            phase,
            True,
        )
        returns = _returns(
            objects,
            rows,
            proof,
            V + site,
            activation,
            guards
            + (
                _object(current.header, "current suffix Q"),
                _object(current.decoder.header, "current suffix D"),
            ),
        )
        child = sequence_interface(objects, rows, proof, current, activation, protected)
        for option in child["outcomes"]:
            outputs = tuple(dict.fromkeys(v for v, off, size in option["possible_writes"]))
            require_writes(outputs, option["possible_writes"], stable)
        call: ScalarInvocation = dict(
            field=field,
            site=V + site,
            before=current,
            result=child,
            predecessor=calls[-1] if calls else controls,
            protected_constituents=bindings,
            stable_Q_D=stable,
            nested_returns=returns,
        )
        calls.append(call)
        good = []
        for option in child["outcomes"]:
            if option["kind"] == "some":
                good.append(option)
            else:
                record(
                    call,
                    option,
                    14 if option["kind"] == "none" else option["error"],
                    count,
                    completed,
                )
        after = None if not good else _joined_sequence(good)
        call["normal_after"] = after
        return call, after, tuple(good)

    top = state.header
    require_writes((top,), ((top, 4, 4),), base + (_object(state.decoder.header, "Tail D"),))
    history_q = borrow(frame, 1064, 8, phase, True)
    top_count = _object(borrow(frame, 292, 4, phase), "physical top count2")
    require_writes(
        (history_q,),
        ((history_q, 0, 8),),
        base + (top_count, _object(state.decoder.header, "Tail D")),
    )
    current = SequenceInterface(history_q, state.decoder, Span(8, 8))
    history: list[ScalarInvocation] = []
    for index, site in enumerate((6144, 6170, 6460, 6534, 6562, 6590, 6616, 6642)):
        captured = (
            ()
            if index < 7
            else _guards(frame, (_object(borrow(frame, 284, 4, phase), "captured history6"),))
        )
        call, after, good = scalar(
            "history" + str(index),
            site,
            current,
            "seq_u16",
            base + (top_count,) + captured,
            2,
            ("Controls",),
        )
        history.append(call)
        require(
            all(v["value_domain"] == (0, 65535) for v in good),
            "Tail history was restricted by later validation",
        )
        if after is None:
            result["history"] = dict(invocations=tuple(history), normal=None)
            return finish()
        if index == 6:
            captured_cell = borrow(frame, 284, 4, phase, True)
            require_writes(
                (captured_cell,),
                ((captured_cell, 0, 4),),
                base + (top_count, _object(history_q, "live history Q")),
            )
        current = after
    require(current.count == Span(0, 0), "Tail history omits a fixed element")
    history_normal: HistoryNormal = dict(
        after=current, domain=(0, 65535), values=tuple(history), Q_retired_after=V + 6734
    )
    result["history"] = dict(invocations=tuple(history), normal=history_normal)
    reduced_state = replace(state, count=Span(2, 2), decoder=current.decoder)
    call, after, good = scalar(
        "reduced_motion", 6736, reduced_state, "seq_bool", base, 1, ("Controls", "history")
    )
    reduced: ReducedSource = dict(invocation=call, normal=None)
    result["reduced"] = reduced
    require(
        all(v["value_domain"] == (0, 1) for v in good),
        "Tail reduced motion loses source bool domain",
    )
    if after is None:
        return finish()
    require(after.count == Span(1, 1), "Tail reduced motion has another counted successor")
    reduced["normal"] = dict(after=after, domain=(0, 1), outcomes=good)
    led_q = borrow(frame, 1000, 8, phase, True)
    require_writes(
        (led_q,), ((led_q, 0, 8),), base + (top_count, _object(state.decoder.header, "Tail D"))
    )
    current = SequenceInterface(led_q, after.decoder, Span(7, 7))
    led_calls: list[LedInvocation | ScalarInvocation] = []
    patterns: list[PatternPackage] = []
    for index, site in enumerate((6792, 6828)):
        edge = next(
            (e for p in parents for e in p["ordinary_edges"] if p["function"] + e[0] == V + site),
            None,
        )
        if not (edge is not None and edge[3] == ROLES["LedPattern"]):
            raise AssertionError("Tail LED loses its paired current Pattern source")
        proof = tail_coverage(objects, _name(objects, edge[3], edge[2]), rows)
        activation = Storage(
            frame.space, "Tail Pattern " + str(index), frame.start - 48, 48, 8, phase, True
        )
        output = borrow(frame, 1064, 10, phase, True)
        child = led_pattern_interface(
            objects, rows, proof, current, output, activation, base + (top_count,), V + site
        )
        pattern_call: LedInvocation = dict(
            field=("normal_pattern", "warning_pattern")[index],
            site=V + site,
            before=current,
            result=child,
            output=output,
            predecessor=calls[-1],
        )
        calls.append(pattern_call)
        led_calls.append(pattern_call)
        for error in child["errors"]:
            record(
                pattern_call, error, error["error"], 1, ("Controls", "history", "reduced_motion")
            )
        if child["none"] is not None:
            none = child["none"]
            record(
                pattern_call,
                dict(after=none["after"], cursor=none["after"].decoder.cursor, none=none),
                14,
                1,
                ("Controls", "history", "reduced_motion"),
            )
        if child["normal"] is None:
            result["LED"] = dict(
                invocations=tuple(led_calls), patterns=tuple(patterns), normal=None
            )
            return finish()
        patterns.append(
            dict(
                invocation=pattern_call,
                source=child["normal"],
                output=output,
                opaque_bytes=child["normal"]["opaque_bytes"],
                original_return=tuple(
                    n
                    for n in child["nested_returns"]
                    if n["role"] == ROLES["LedPattern"] and n["return_PC"] == V + site + 4
                ),
            )
        )
        require(
            len(patterns[-1]["original_return"]) == 1,
            "Tail Pattern loses its original same-call return",
        )
        current = child["normal"]["after"]
    # V280/284 now hold warning words. Earlier saved7/history scratch is
    # retired; these exact new stores are the only new captured word epoch.
    captures = tuple(
        _object(borrow(frame, off, 4, phase), "captured warning Pattern " + str(off))
        for off in (280, 284)
    )
    for off in (280, 284):
        target = borrow(frame, off, 4, phase, True)
        require_writes((target,), ((target, 0, 4),), base + (top_count, _object(led_q, "LED Q")))
    bools: list[ScalarInvocation] = []
    for index, site in enumerate((6876, 6900, 6926, 6952, 6978)):
        captured_bools = tuple(
            _object(borrow(frame, off, 4, phase), "captured LED bool " + str(off))
            for off in (276, 272, 268)[: max(0, index - 1)]
        )
        call, next_state, good = scalar(
            "LED_bool" + str(index),
            site,
            current,
            "seq_bool",
            base + (top_count,) + captures + captured_bools,
            1,
            ("Controls", "history", "reduced_motion"),
        )
        led_calls.append(call)
        bools.append(call)
        require(
            all(v["value_domain"] == (0, 1) for v in good),
            "Tail LED bool loses same-source success domain",
        )
        if next_state is None:
            result["LED"] = dict(
                invocations=tuple(led_calls), patterns=tuple(patterns), normal=None
            )
            return finish()
        if index in (1, 2, 3):
            target = borrow(frame, (276, 272, 268)[index - 1], 4, phase, True)
            require_writes(
                (target,),
                ((target, 0, 4),),
                base + (top_count,) + captures + captured_bools + (_object(led_q, "LED Q"),),
            )
        current = next_state
    require(current.count == Span(0, 0), "Tail LED omits a pattern or bool field")
    led_normal: LEDNormal = dict(after=current, patterns=tuple(patterns), bools=tuple(bools))
    result["LED"] = dict(invocations=tuple(led_calls), patterns=tuple(patterns), normal=led_normal)
    result["normal"] = dict(
        after=replace(state, count=Span(1, 1), decoder=current.decoder),
        LEDQ=current,
        physical_top_count=1,
        logical_remaining_fields=0,
        old_history_Q_still_live=False,
        source=led_normal,
        requires_all_late_LED_stores_and_original_Visitor_restore=True,
    )
    return finish()


def _suffix_effects(
    objects: DiagnosticObjects,
    rows: InstructionMap,
    parents: tuple[BodyRecord, ...],
    source: SuffixSource,
    frame: Storage,
    ancestor_protected: Sequence[Storage],
) -> SuffixEffects:
    """Keep completed history/reduced groups on later LED parse failure.

    The physical LED assignment starts only after every Pattern and bool has
    returned normally. Its early success register does not retire the final
    two word stores. Opaque payload/padding bytes remain opaque after copy.
    """
    from config_decoded_tail import _parents

    controls = source["controls"]
    owner = controls["owner"]
    V = _parent_inputs(objects, rows, parents, source["before"], frame)
    require(
        controls["frame"] is frame
        and controls["source"]["normal"] is not None
        and controls["source"]["normal"]["after"] is source["before"]
        and controls["complete_after"] == V + 6118
        and parents == _parents(objects, rows, V, owner.start),
        "Tail suffix writes have another Controls/source/current Config predecessor",
    )
    guards = _guards(
        frame,
        tuple(ancestor_protected)
        + (
            _object(source["before"].decoder.header, "Tail retained D"),
            _object(source["before"].decoder.payload, "immutable-input"),
        ),
    )
    guards += tuple(
        Storage(
            space,
            "unchanged Config outside suffix " + str(off),
            owner.start + off,
            size,
            1,
            frame.phase,
            False,
        )
        for off, size in ((0, 14528), (14569, 5), (14575, 29))
        for space in ("address", frame.space)
    )
    result: SuffixEffects = dict(
        source=source,
        owner=owner,
        history=(),
        reduced=None,
        LED=(),
        complete_after=None,
        early_success_register=None,
        preserved_LED_padding=(14569,),
        written_opaque_LED_bytes=(),
        completed_ordinary_group_issued=False,
        requires_selected_parent_source_control=True,
        requires_bound_LED_word_access_policy=True,
    )
    children = tail_children(objects, rows, parents)
    for call in source["invocations"]:
        _call_binding(parents, children, call, frame)
    history = source["history"]
    reduced = source["reduced"]
    led = source["LED"]
    if history is None or history["normal"] is None:
        return result
    calls = history["invocations"]
    normal = history["normal"]
    require(
        len(calls) == 8
        and normal["values"] == calls
        and normal["after"].count == Span(0, 0)
        and tuple(v["site"] - V for v in calls) == (6144, 6170, 6460, 6534, 6562, 6590, 6616, 6642),
        "Tail history publication omits its actual eight source returns",
    )
    for index, call in enumerate(calls):
        require(
            (
                call["before"].header.storage,
                call["before"].header.offset,
                call["before"].header.extent,
            )
            == (frame, 1064, 8)
            and call["before"].decoder.header == source["before"].decoder.header
            and call["before"].decoder.payload == source["before"].decoder.payload,
            "Tail history changes its current caller Q/D/source allocation",
        )
        if index == 0:
            require(
                call["before"].decoder is source["before"].decoder,
                "Tail history loses its actual initial cursor",
            )
        good = tuple(v for v in call["result"]["outcomes"] if v["kind"] == "some")
        require(
            good
            and all(v["value_domain"] == (0, 65535) for v in good)
            and call["before"].count == Span(8 - index, 8 - index)
            and call["result"]["semantic_role"] == "seq_u16"
            and call["nested_returns"],
            "Tail history publication loses its same-source scalar outcome/return",
        )
        if index:
            prior = calls[index - 1]
            require(
                call["predecessor"] is prior
                and call["before"]
                == _joined_sequence(
                    tuple(v for v in prior["result"]["outcomes"] if v["kind"] == "some")
                ),
                "Tail history publication skips its ordered source predecessor",
            )
    written: set[int] = set()
    stages: list[HistoryStage] = []
    for site, offset, size, index in (
        (6672, 14542, 2, 7),
        (6680, 14540, 2, 6),
        (6690, 14538, 2, 5),
        (6698, 14536, 2, 4),
        (6706, 14534, 2, 3),
        (6714, 14532, 2, 2),
        (6722, 14530, 2, 1),
        (6728, 14528, 1, 0),
        (6732, 14529, 1, 0),
    ):
        out = borrow(owner, offset, size, frame.phase, True)
        retained = tuple(
            Storage(
                "address",
                "history already written " + str(v),
                owner.start + v,
                1,
                1,
                frame.phase,
                False,
            )
            for v in sorted(written)
        )
        require_writes((out,), ((out, 0, size),), guards + retained)
        written.update(range(offset, offset + size))
        stages.append(
            dict(
                site=V + site,
                source=calls[index],
                source_index=index,
                possible_writes=((out, 0, size),),
                changed_bytes=tuple(sorted(written)),
                complete_history=site == 6732,
            )
        )
    require(written == set(range(14528, 14544)), "Tail history writes miss a physical byte")
    result["history"] = tuple(stages)
    guards += _guards(frame, (_object(borrow(owner, 14528, 16, frame.phase), "completed history"),))
    if reduced is None or reduced["normal"] is None:
        return result
    call = reduced["invocation"]
    rnormal = reduced["normal"]
    require(
        call["site"] == V + 6736
        and call["before"].count == Span(2, 2)
        and call["before"].decoder is normal["after"].decoder
        and rnormal["domain"] == (0, 1)
        and rnormal["after"].count == Span(1, 1)
        and call["result"]["semantic_role"] == "seq_bool"
        and all(
            any(option is v for v in call["result"]["outcomes"]) and option["kind"] == "some"
            for option in rnormal["outcomes"]
        ),
        "Tail reduced store loses its paired bool source",
    )
    out = borrow(owner, 14574, 1, frame.phase, True)
    require_writes((out,), ((out, 0, 1),), guards)
    result["reduced"] = dict(site=V + 6768, source=call, possible_writes=((out, 0, 1),))
    guards += _guards(frame, (_object(out, "completed reduced motion"),))
    if led is None or led["normal"] is None:
        return result
    patterns = led["patterns"]
    lnormal = led["normal"]
    led_calls = led["invocations"]
    require(
        len(patterns) == 2
        and len(led_calls) == 7
        and len(lnormal["bools"]) == 5
        and tuple(v["site"] - V for v in led_calls) == (6792, 6828, 6876, 6900, 6926, 6952, 6978)
        and source["normal"] is not None
        and source["normal"]["physical_top_count"] == 1
        and source["normal"]["LEDQ"] is lnormal["after"]
        and lnormal["after"].count == Span(0, 0),
        "Tail LED publication precedes its complete same-source Pattern/bool chain",
    )
    for index, pattern in enumerate(patterns):
        pattern_call = led_calls[index]
        if not _is_led_call(pattern_call):
            raise AssertionError(
                "Tail LED publication mixes Pattern epoch, output or original return"
            )
        require(
            pattern["invocation"] is pattern_call
            and pattern["source"] is pattern_call["result"]["normal"]
            and pattern_call["output"] is pattern["output"]
            and len(pattern["original_return"]) == 1
            and pattern["original_return"][0]["return_PC"] == pattern_call["site"] + 4
            and pattern["original_return"][0]["return_SP"] == frame.start
            and pattern["source"]["opaque_bytes"] == (1, 5, 9),
            "Tail LED publication mixes Pattern epoch, output or original return",
        )
        prior_decoder = (
            rnormal["after"].decoder
            if index == 0
            else patterns[index - 1]["source"]["after"].decoder
        )
        require(
            pattern_call["before"].decoder is prior_decoder,
            "Tail LED Pattern has another prior source cursor",
        )
    for index, bool_call in enumerate(led_calls[2:]):
        if not _is_scalar_call(bool_call):
            raise AssertionError("Tail LED publication skips its actual bool source predecessor")
        previous = led_calls[index + 1]
        if index == 0:
            before = patterns[1]["source"]["after"]
        else:
            if not _is_scalar_call(previous):
                raise AssertionError(
                    "Tail LED publication skips its actual bool source predecessor"
                )
            before = _joined_sequence(
                tuple(v for v in previous["result"]["outcomes"] if v["kind"] == "some")
            )
        require(
            bool_call["predecessor"] is previous
            and bool_call["before"] == before
            and bool_call["before"].count == Span(5 - index, 5 - index)
            and bool_call["result"]["semantic_role"] == "seq_bool",
            "Tail LED publication skips its actual bool source predecessor",
        )
    written = set()
    led_stages: list[LEDStage] = []
    for site, offset, size, origin in (
        (7026, 14556, 4, patterns[1]),
        (7034, 14546, 4, patterns[0]),
        (7038, 14568, 1, led_calls[6]),
        (7046, 14567, 1, led_calls[5]),
        (7054, 14566, 1, led_calls[4]),
        (7062, 14565, 1, led_calls[3]),
        (7068, 14564, 1, led_calls[2]),
        (7076, 14555, 1, patterns[1]),
        (7082, 14554, 1, patterns[1]),
        (7088, 14545, 1, patterns[0]),
        (7096, 14544, 1, patterns[0]),
        (7102, 14560, 4, patterns[1]),
        (7108, 14550, 4, patterns[0]),
    ):
        out = borrow(owner, offset, size, frame.phase, True)
        retained = tuple(
            Storage(
                "address",
                "LED already assigned " + str(off),
                owner.start + off,
                1,
                1,
                frame.phase,
                False,
            )
            for off in sorted(written)
        )
        require_writes((out,), ((out, 0, size),), guards + retained)
        written.update(range(offset, offset + size))
        led_stages.append(
            dict(
                site=V + site,
                source=origin,
                possible_writes=((out, 0, size),),
                changed_bytes=tuple(sorted(written)),
                complete_LED=site == 7108,
            )
        )
    require(
        written == set(range(14544, 14569)),
        "Tail LED assignment misses a physical field or writes settings padding",
    )
    result.update(
        {
            "LED": tuple(led_stages),
            "complete_after": V + 7112,
            "early_success_register": V + 7100,
            "written_opaque_LED_bytes": (14545, 14549, 14553, 14555, 14559, 14563),
        }
    )
    return result
