"""Actual private Tail/LED source-reached uses, before final loader routes.

Canonical conditional descriptors never initialize these uses. The only
initial collection entry derives its actual private Tail control internally
and consumes every member in the same non-yielding scope. Later consumption
revalidates the complete upstream collection once, not once per member.
"""

from __future__ import annotations

from dataclasses import dataclass, replace, fields
from collections.abc import Sequence
from typing import TYPE_CHECKING, TypedDict, TypeGuard
from config_abstract import DecoderInterface
from config_widget_uses import AllocationLineage
from config_tail_source import (
    ScreenInvocation,
    LedInvocation,
    ScreenPackage,
    ControlsNormal,
    WordAccess,
    CopyTransport,
    ControlStage,
    InlineTransport,
    LEDNormal,
    LEDStage,
    _is_screen_call,
    _is_led_call,
)
from config_tail_effects import ScreenNormal, LedNormal

if TYPE_CHECKING:
    from config_tail_control import _TailActiveControl
    from config_decoded_tail import _TailPhasesState, _TailEntry
    from config_decoded_sources import PhaseGraph

from config_regions import Storage, View, borrow, require_disjoint, require_writes
from config_decode_effects import _object
from config_tail_access import (
    TailAccessContext,
    TailFamily,
    EXPECTED_TAIL_ROLES,
    SCREEN_EPOCHS,
    SITE_SHAPES,
    _PHASE,
    current_tail_family,
)
from config_led_access import (
    LedContext,
    LedFamily,
    EXPECTED_LED_ROLES,
    SITE_SHAPES as LED_SITES,
    current_led_family,
)
from config_tail_contracts import ROLES
from validation import require


class ScreenOutputSource(TypedDict):
    invocation: ScreenInvocation
    normal: ScreenNormal
    partial_before_words: tuple[int, ...]
    after_first_word: tuple[int, ...]
    complete_only_after_second_word_and_original_return: bool


class ScreenScratchSource(TypedDict):
    Screen: ScreenPackage
    Controls: ControlsNormal
    words: tuple[WordAccess, ...]
    copies: tuple[CopyTransport, ...]


class ScreenConfigSource(TypedDict):
    Screen: ScreenPackage
    Controls: ControlsNormal
    stages: tuple[ControlStage, ...]
    words: tuple[WordAccess, ...]
    inline_transports: tuple[InlineTransport, ...]


class LedOutputSource(TypedDict):
    invocation: LedInvocation
    normal: LedNormal
    opaque_bytes: tuple[int, ...]
    partial_after_kind: tuple[int, ...]


class LedReadSource(TypedDict):
    invocation: LedInvocation
    normal: LedNormal
    read_epoch: str
    opaque_fields_remain_opaque: tuple[int, ...]


class LedConfigSource(TypedDict):
    invocation: LedInvocation
    normal: LedNormal
    LED: LEDNormal | None
    stages: tuple[LEDStage, ...]


UseSource = (
    ScreenOutputSource
    | ScreenScratchSource
    | ScreenConfigSource
    | LedOutputSource
    | LedReadSource
    | LedConfigSource
)


def is_screen_output_source(value: UseSource) -> TypeGuard[ScreenOutputSource]:
    return "partial_before_words" in value


def is_screen_scratch_source(value: UseSource) -> TypeGuard[ScreenScratchSource]:
    return "copies" in value


def is_screen_config_source(value: UseSource) -> TypeGuard[ScreenConfigSource]:
    return "inline_transports" in value


def is_led_output_source(value: UseSource) -> TypeGuard[LedOutputSource]:
    return "partial_after_kind" in value


def is_led_read_source(value: UseSource) -> TypeGuard[LedReadSource]:
    return "read_epoch" in value


def is_led_config_source(value: UseSource) -> TypeGuard[LedConfigSource]:
    return "LED" in value


def _family_data(
    control: _TailActiveControl,
) -> tuple[tuple[TailFamily, ...], tuple[LedFamily, ...], AllocationLineage]:
    from config_tail_control import _TailActiveControl, _structural_data
    from config_source_uses import _SourceAccessBridge

    require(type(control) is _TailActiveControl, "Tail access lacks the actual private control")
    state = control.state
    _structural_data(control.structural, state)
    previous = state.predecessor.access_bridge
    c = state.predecessor.phases.widgets.views.context
    require(
        type(previous) is _SourceAccessBridge
        and previous.control is state.predecessor.control
        and previous.control.state is state.predecessor.phases,
        "Tail access loses its actual checked Source allocation lineage",
    )
    if previous is None:
        raise AssertionError("Tail access loses its actual checked Source allocation lineage")
    prior = previous.families[2].context
    require(
        previous.allocation_lineage[:3]
        == (
            (c.frame, prior.loader),
            (c.headers["visitor_frame"], prior.visitor),
            (c.owner, prior.config.storage),
        ),
        "Tail access changes its inherited loader/Visitor/Config identity",
    )
    loader = replace(prior.loader, phase=_PHASE)
    visitor = replace(prior.visitor, phase=_PHASE)
    config = replace(prior.config.storage, phase=_PHASE)
    protected = tuple(replace(v, phase=_PHASE) for v in prior.protected)
    V = c.headers["visitor"]["proof"]["function"]
    screen = tuple(p for p in state.children.values() if p["role"] == ROLES["ScreenBindings"])
    led = tuple(p for p in state.children.values() if p["role"] == ROLES["LedPattern"])
    require(
        len(screen) == len(led) == 1 and screen[0]["frame"] == 64 and led[0]["frame"] == 48,
        "Tail access has another current aggregate child geometry",
    )
    child = Storage(
        visitor.space, "actual ScreenBindings activation", visitor.start - 64, 64, 8, _PHASE, True
    )
    child_saved = Storage(
        visitor.space, "Screen original saved cells", child.start + 28, 36, 4, _PHASE, False
    )
    families = []
    for i, role in enumerate(EXPECTED_TAIL_ROLES):
        local = i == 0
        parent = screen[0]["function"] if local else V
        context = TailAccessContext(
            loader,
            visitor,
            child if local else visitor,
            borrow(config, 0, 14576, _PHASE),
            protected + ((child_saved,) if local else ()),
        )
        sites = tuple(
            (parent + off, raw, cell, width, direction, phase)
            for off, raw, cell, width, direction, phase in SITE_SHAPES[role]
        )
        epochs = tuple((V + off, out, n) for off, out, n in SCREEN_EPOCHS) if local else ()
        families.append(
            current_tail_family(
                state.objects, TailFamily(role, "sizez", parent, V, sites, context, epochs)
            )
        )
    LEDs = []
    for role in EXPECTED_LED_ROLES["sizez"]:
        contexts = []
        for epoch in ("settings",) if role == "normal_config_words" else ("normal", "warning"):
            phase = "decoded_led_" + epoch
            led_loader = replace(prior.loader, phase=phase)
            v = replace(prior.visitor, phase=phase)
            cfg = replace(prior.config.storage, phase=phase)
            guards = tuple(replace(g, phase=phase) for g in prior.protected)
            f = (
                Storage(v.space, "actual LedPattern activation", v.start - 48, 48, 8, phase, True)
                if role == "result_words"
                else v
            )
            if role == "result_words":
                guards += (
                    Storage(v.space, "LED original saved cells", v.start - 32, 32, 4, phase, False),
                )
            owner = (
                borrow(cfg, 14544, 10, phase, True)
                if role == "normal_config_words"
                else borrow(v, 1064, 10, phase, role == "result_words")
            )
            contexts.append(
                LedContext(
                    "sizez", epoch, led_loader, f, borrow(cfg, 0, 14576, phase), owner, guards
                )
            )
        parent = led[0]["function"] if role == "result_words" else V
        led_sites = tuple(
            (parent + off, raw, cell, epoch) for off, raw, cell, epoch in LED_SITES[role, "sizez"]
        )
        LEDs.append(
            current_led_family(
                state.objects, LedFamily(role, "sizez", parent, led_sites, tuple(contexts))
            )
        )
    lineage: AllocationLineage = (
        (
            previous.allocation_lineage[0][0],
            replace(previous.allocation_lineage[0][1], phase=_PHASE),
        ),
        (
            previous.allocation_lineage[1][0],
            replace(previous.allocation_lineage[1][1], phase=_PHASE),
        ),
        (
            previous.allocation_lineage[2][0],
            replace(previous.allocation_lineage[2][1], phase=_PHASE),
        ),
        previous.allocation_lineage[3],
    )
    return tuple(families), tuple(LEDs), lineage


@dataclass(frozen=True)
class _TailUse:
    role: str
    phase_index: int
    epoch: int | str
    entry: _TailEntry
    before: DecoderInterface
    after: DecoderInterface
    output: View
    footprint: tuple[tuple[int, int], ...]
    protected: tuple[Storage, ...]
    source: UseSource
    continuation: tuple[int, ...]
    completed_prefix_issued: bool = False


def _make_use(
    state: _TailPhasesState,
    family: TailFamily | LedFamily,
    phase_index: int,
    epoch: int | str,
    before: object,
    after: object,
    output: View,
    protected: Sequence[Storage],
    source: UseSource,
    continuation: tuple[int, ...],
) -> _TailUse:
    from config_abstract import DecoderInterface
    from config_tail_effects import _guards

    require(
        type(before) is DecoderInterface
        and type(after) is DecoderInterface
        and before.header == after.header
        and before.payload == after.payload
        and before.cursor.lower
        <= after.cursor.lower
        <= after.cursor.upper
        <= before.payload.extent,
        "Tail reached use changes its exact source/header/cursor epoch",
    )
    if not isinstance(before, DecoderInterface) or not isinstance(after, DecoderInterface):
        raise AssertionError("Tail reached use changes its exact source/header/cursor epoch")
    if type(family) is TailFamily:
        if type(epoch) is not int:
            raise AssertionError("Tail Screen use has another source epoch")
        owner = family.owner(epoch)
        footprints = family.footprints(epoch)
        direction = family.sites[0][4]
    else:
        if type(family) is not LedFamily or not isinstance(epoch, str):
            raise AssertionError("unreviewed Tail reached family")
        context = next((v for v in family.contexts if v.epoch == epoch), None)
        if context is None:
            raise AssertionError("Tail LED use loses its same source epoch")
        owner = context.owner
        footprints = family.footprints(epoch)
        direction = "read" if family.role == "pattern_words" else "write"
    require(
        output.start == owner.start
        and output.extent == owner.extent
        and output.storage.space == owner.storage.space
        and output.writable == (direction == "write"),
        "Tail reached use borrows another current family output",
    )
    frame = state.entries[phase_index].sequence.header.storage
    protected = _guards(frame, tuple(protected))
    if direction == "write":
        require_writes(
            (output,),
            tuple((output, lo - output.start, hi - lo) for lo, hi in footprints),
            protected,
        )
    else:
        for lo, hi in footprints:
            read = Storage(
                output.storage.space,
                "actual Tail read footprint",
                lo,
                hi - lo,
                1,
                frame.phase,
                False,
            )
            for guard in protected:
                require_disjoint((read, guard))
    return _TailUse(
        family.role,
        phase_index,
        epoch,
        state.entries[phase_index],
        before,
        after,
        output,
        footprints,
        protected,
        source,
        continuation,
    )


def _reached_data(
    control: _TailActiveControl, families: Sequence[TailFamily], LEDs: Sequence[LedFamily]
) -> tuple[_TailUse, ...]:
    from config_abstract import Span
    from config_tail_source import _call_binding

    state = control.state
    c = state.predecessor.phases.widgets.views.context
    frame = c.headers["visitor_frame"]
    V = c.headers["visitor"]["proof"]["function"]
    tail = {v.role: v for v in families}
    led = {v.role: v for v in LEDs}
    uses = []
    require(
        len(control.guards) == len(state.phases) == len(state.entries),
        "Tail reached uses omit a current entry",
    )
    for index, (phase, guard) in enumerate(zip(state.phases, control.guards)):
        entry = phase.entry
        require(
            guard["entry_index"] == index
            and guard["entry"] is entry
            and entry is state.entries[index]
            and phase.controls["before"] is entry.sequence,
            "Tail reached use leaves its actual guarded entry",
        )
        base = entry.protected + (
            _object(entry.sequence.decoder.header, "live Tail D"),
            _object(entry.sequence.decoder.payload, "immutable same Tail source"),
            _object(entry.sequence.header, "live top Q"),
        )
        for call in phase.controls["invocations"][1:]:
            if not _is_screen_call(call):
                raise AssertionError("Tail Screen use substitutes another aggregate role")
            proof = call["result"]["proof"]
            normal = call["result"]["normal"]
            require(
                proof["role"] == ROLES["ScreenBindings"],
                "Tail Screen use substitutes another aggregate role",
            )
            if normal is None:
                continue
            epoch = next(
                (i for i, (off, out, n) in enumerate(SCREEN_EPOCHS) if call["site"] == V + off),
                None,
            )
            require(
                epoch is not None
                and call["before"].count == Span(SCREEN_EPOCHS[epoch][2], SCREEN_EPOCHS[epoch][2])
                and call["output"].storage is frame
                and call["output"].offset == SCREEN_EPOCHS[epoch][1]
                and call["output"] is call["result"]["output"]
                and normal["initialized_bytes"] == tuple(range(10))
                and normal["physical_output_order"]
                == ((266, 1, 1), (268, 0, 1), (296, 6, 4), (306, 2, 4)),
                "Tail output word precedes its same Screen package/late-store relation",
            )
            if epoch is None:
                raise AssertionError("Tail output word loses its Screen epoch")
            _call_binding(state.parents, state.children, call, frame)
            protected = call["result"]["protected"] + (
                _object(call["before"].header, "current Screen Q"),
                _object(call["before"].decoder.header, "current Screen D"),
                Storage(
                    frame.space,
                    "Screen own original saves",
                    frame.start - 36,
                    36,
                    4,
                    frame.phase,
                    False,
                ),
            )
            screen_output_source: ScreenOutputSource = dict(
                invocation=call,
                normal=normal,
                partial_before_words=(0, 1),
                after_first_word=(0, 1, 6, 7, 8, 9),
                complete_only_after_second_word_and_original_return=True,
            )
            uses.append(
                _make_use(
                    state,
                    tail["Screen_output_words"],
                    index,
                    epoch,
                    call["before"].decoder,
                    normal["after"].decoder,
                    call["output"],
                    protected,
                    screen_output_source,
                    (
                        proof["function"] + 266,
                        proof["function"] + 268,
                        proof["function"] + 296,
                        proof["function"] + 306,
                        call["site"] + 4,
                    ),
                )
            )
        parsed = phase.controls["normal"]
        effects = phase.control_effects
        if parsed is not None:
            require(
                effects["complete_after"] == V + 6118
                and len(parsed["screens"]) == 3
                and guard["lineage"]["errors"] is not None,
                "Tail first-only use precedes all three successful Screen returns",
            )
            first = parsed["screens"][0]
            before = parsed["after"].decoder
            require(
                first["invocation"] is phase.controls["invocations"][1]
                and first["invocation"]["site"] == V + 5712
                and tuple(v["instruction"] - V for v in effects["first_only_words"])
                == (6496, 6510, 6074, 6078),
                "Tail first-only access borrows a later Screen or another operation epoch",
            )
            read_guards = (
                base
                + (
                    _object(parsed["ControlQ"].header, "retired Controls Q"),
                    _object(parsed["ScreenQ"].header, "retired Screens Q"),
                )
                + tuple(
                    _object(v["output"], "other returned Screen") for v in parsed["screens"][1:]
                )
            )
            screen_scratch_source: ScreenScratchSource = dict(
                Screen=first,
                Controls=parsed,
                words=effects["first_only_words"][:2],
                copies=effects["copies"],
            )
            uses.append(
                _make_use(
                    state,
                    tail["first_Screen_scratch_words"],
                    index,
                    0,
                    before,
                    before,
                    replace(first["output"], writable=False),
                    read_guards,
                    screen_scratch_source,
                    tuple(V + o for o in (6496, 6508, 6510, 6514, 5936, 6026, 6058)),
                )
            )
            stages = effects["Config_stages"]
            require(
                tuple(v["site"] - V for v in stages[:5]) == (6062, 6066, 6070, 6074, 6078)
                and stages[4]["completed_screens"] == 1
                and stages[4]["complete_Controls"] is False,
                "Tail first Config Screen words precede their actual partial assignment",
            )
            partial = tuple(
                Storage(
                    "address",
                    "earlier partial Controls byte",
                    c.owner.start + off,
                    n,
                    1,
                    frame.phase,
                    False,
                )
                for off, n in ((212, 2), (222, 1))
            )
            screen_config_source: ScreenConfigSource = dict(
                Screen=first,
                Controls=parsed,
                stages=stages[:5],
                words=effects["first_only_words"][2:],
                inline_transports=effects["inline_transports"],
            )
            uses.append(
                _make_use(
                    state,
                    tail["first_Config_Screen_words"],
                    index,
                    0,
                    before,
                    before,
                    borrow(c.owner, 212, 10, frame.phase, True),
                    base + partial,
                    screen_config_source,
                    tuple(V + o for o in (6058, 6062, 6066, 6070, 6074, 6078, 6114)),
                )
            )
        suffix = phase.suffix
        if suffix is None or suffix["LED"] is None:
            continue
        patterns = suffix["LED"]["invocations"][:2]
        for led_epoch, led_call in zip(("normal", "warning"), patterns):
            if not _is_led_call(led_call):
                raise AssertionError("Tail LED reached word changes its current child")
            proof = led_call["result"]["proof"]
            led_normal = led_call["result"]["normal"]
            require(
                proof["role"] == ROLES["LedPattern"]
                and led_call["site"] == V + (6792 if led_epoch == "normal" else 6828),
                "Tail LED reached word changes its current child or source epoch",
            )
            if led_normal is None:
                continue
            _call_binding(state.parents, state.children, led_call, frame)
            require(
                led_normal["initialized_bytes"] == (0, 2, 3, 4, 6, 7, 8)
                and led_normal["opaque_bytes"] == (1, 5, 9)
                and led_normal["physical_output_order"] == ((184, 0, 1), (186, 6, 4), (194, 2, 4))
                and led_call["output"].offset == 1064,
                "Tail LED word loses its exact initialized versus opaque source fields",
            )
            protected = led_call["result"]["protected"] + (
                _object(led_call["before"].header, "current LED Q"),
                _object(led_call["before"].decoder.header, "current LED D"),
                Storage(
                    frame.space,
                    "LED own original saves",
                    frame.start - 32,
                    32,
                    4,
                    frame.phase,
                    False,
                ),
            )
            led_output_source: LedOutputSource = dict(
                invocation=led_call,
                normal=led_normal,
                opaque_bytes=(1, 5, 9),
                partial_after_kind=(0,),
            )
            uses.append(
                _make_use(
                    state,
                    led["result_words"],
                    index,
                    led_epoch,
                    led_call["before"].decoder,
                    led_normal["after"].decoder,
                    led_call["output"],
                    protected,
                    led_output_source,
                    (
                        proof["function"] + 184,
                        proof["function"] + 186,
                        proof["function"] + 194,
                        led_call["site"] + 4,
                    ),
                )
            )
            # These reads precede reuse of O by the next pattern or later
            # bool failures. No full LED Config success is inferred here.
            sites = (6814, 6818) if led_epoch == "normal" else (6862, 6868)
            guards = base + (_object(led_call["before"].header, "LED Q"),)
            led_read_source: LedReadSource = dict(
                invocation=led_call,
                normal=led_normal,
                read_epoch=led_epoch,
                opaque_fields_remain_opaque=(5, 9),
            )
            uses.append(
                _make_use(
                    state,
                    led["pattern_words"],
                    index,
                    led_epoch,
                    led_normal["after"].decoder,
                    led_normal["after"].decoder,
                    replace(led_call["output"], writable=False),
                    guards,
                    led_read_source,
                    tuple(V + o for o in (*sites, 6828 if led_epoch == "normal" else 6876)),
                )
            )
        if suffix["normal"] is None:
            continue
        if phase.suffix_effects is None:
            raise AssertionError("Tail normal Config LED access lacks its actual late stores")
        require(
            phase.suffix_effects["complete_after"] == V + 7112
            and guard["lineage"]["normal"] is not None
            and suffix["LED"]["normal"] is suffix["normal"]["source"],
            "Tail normal Config LED access precedes the actual full source and late stores",
        )
        first_pattern = suffix["LED"]["patterns"][0]
        before = suffix["normal"]["after"].decoder
        led_stages = tuple(
            v for v in phase.suffix_effects["LED"] if v["site"] in (V + 7034, V + 7108)
        )
        require(
            len(led_stages) == 2 and all(v["source"] is first_pattern for v in led_stages),
            "Tail normal Config word borrows warning or another Pattern origin",
        )
        led_config_source: LedConfigSource = dict(
            invocation=first_pattern["invocation"],
            normal=first_pattern["source"],
            LED=suffix["LED"]["normal"],
            stages=led_stages,
        )
        uses.append(
            _make_use(
                state,
                led["normal_config_words"],
                index,
                "settings",
                before,
                before,
                borrow(c.owner, 14544, 10, frame.phase, True),
                base,
                led_config_source,
                tuple(V + o for o in (7034, 7100, 7108, 7112)),
            )
        )
    return tuple(uses)


def _snapshot(
    control: _TailActiveControl,
    families: Sequence[TailFamily],
    LEDs: Sequence[LedFamily],
    lineage: AllocationLineage,
    uses: Sequence[_TailUse],
) -> PhaseGraph:
    from config_decoded_sources import _phase_graph
    from config_decoded_tail import _entry_data
    from config_tail_control import _guard_data, _structural_data

    state = control.state
    data = []
    for use in uses:
        require(
            type(use) is _TailUse
            and type(use.phase_index) is int
            and 0 <= use.phase_index < len(state.entries)
            and use.entry is state.entries[use.phase_index]
            and use.completed_prefix_issued is False,
            "Tail reached member changes its exact entry/type/phase association",
        )
        data.append({f.name: getattr(use, f.name) for f in fields(use) if f.name != "entry"})
    phases = tuple(
        dict(
            entry=_entry_data(p.entry, state.predecessor),
            controls=p.controls,
            effects=p.control_effects,
            suffix=p.suffix,
            suffix_effects=p.suffix_effects,
            envelopes=p.parent_envelopes,
        )
        for p in state.phases
    )
    return _phase_graph(
        (
            families,
            LEDs,
            lineage,
            tuple(data),
            phases,
            _guard_data(state, control.guards),
            control.record,
            _structural_data(control.structural, state),
        )
    )


@dataclass(frozen=True)
class _TailAccessBridge:
    control: _TailActiveControl
    tail_families: tuple[TailFamily, ...]
    LED_families: tuple[LedFamily, ...]
    allocation_lineage: AllocationLineage
    uses: tuple[_TailUse, ...]
    checkpoint: PhaseGraph

    def current(self) -> _TailAccessBridge:
        _checked_graph(self)
        self.control.current()
        families, LEDs, lineage = _family_data(self.control)
        uses = _reached_data(self.control, families, LEDs)
        require(
            _snapshot(self.control, families, LEDs, lineage, uses) == self.checkpoint,
            "Tail current allocation/source/epoch/use relation changed",
        )
        return self


def _checked_graph(bridge: _TailAccessBridge) -> None:
    require(
        type(bridge) is _TailAccessBridge
        and _snapshot(
            bridge.control,
            bridge.tail_families,
            bridge.LED_families,
            bridge.allocation_lineage,
            bridge.uses,
        )
        == bridge.checkpoint,
        "Tail reached access collection changed",
    )


def _check_member(bridge: _TailAccessBridge, use: _TailUse) -> _TailUse:
    family = next(
        (v for v in bridge.tail_families + bridge.LED_families if v.role == use.role), None
    )
    require(
        family is not None and any(use is v for v in bridge.uses),
        "Tail access is not an exact checked collection member",
    )
    if family is None:
        raise AssertionError("Tail access is not an exact checked collection member")
    if type(family) is TailFamily:
        if type(use.epoch) is not int:
            raise AssertionError("Tail Screen use has another source epoch")
        output = family.owner(use.epoch)
        footprint = family.footprints(use.epoch)
    else:
        if not isinstance(family, LedFamily) or not isinstance(use.epoch, str):
            raise AssertionError("Tail LED use has another source epoch")
        output = next(v.owner for v in family.contexts if v.epoch == use.epoch)
        footprint = family.footprints(use.epoch)
    require(
        use.footprint == footprint
        and (use.output.start, use.output.extent, use.output.writable)
        == (output.start, output.extent, output.writable),
        "Tail reached access has another footprint/source owner",
    )
    return use


def _tail_access_group(state: _TailPhasesState) -> tuple[_TailAccessBridge, tuple[_TailUse, ...]]:
    """One internally derived current collection; no supplied control ticket."""
    from config_decoded_tail import _TailPhasesState
    from config_tail_control import _tail_control

    require(type(state) is _TailPhasesState, "Tail access group lacks its actual private producer")
    control = _tail_control(state)
    families, LEDs, lineage = _family_data(control)
    uses = _reached_data(control, families, LEDs)
    bridge = _TailAccessBridge(
        control, families, LEDs, lineage, uses, _snapshot(control, families, LEDs, lineage, uses)
    )
    for use in uses:
        _check_member(bridge, use)
    return bridge, uses


def _check_tail_uses(bridge: _TailAccessBridge) -> tuple[_TailUse, ...]:
    """Fresh authoritative batch; one full graph/upstream check, no ticket."""
    require(type(bridge) is _TailAccessBridge, "Tail batch lacks its actual private bridge")
    bridge.current()
    for use in bridge.uses:
        _check_member(bridge, use)
    return bridge.uses
