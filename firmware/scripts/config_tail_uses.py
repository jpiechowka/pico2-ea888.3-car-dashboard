"""Actual private Tail/LED source-reached uses, before final loader routes.

Canonical conditional descriptors never initialize these uses. The only
initial collection entry derives its actual private Tail control internally
and consumes every member in the same non-yielding scope. Later consumption
revalidates the complete upstream collection once, not once per member.
"""

from dataclasses import dataclass, replace, fields
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


def _family_data(control):
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
        sites = tuple((parent + off, *rest) for off, *rest in SITE_SHAPES[role])
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
        sites = tuple((parent + off, *rest) for off, *rest in LED_SITES[role, "sizez"])
        LEDs.append(
            current_led_family(
                state.objects, LedFamily(role, "sizez", parent, sites, tuple(contexts))
            )
        )
    lineage = tuple((a, replace(b, phase=_PHASE)) for a, b in previous.allocation_lineage[:3]) + (
        previous.allocation_lineage[3],
    )
    return tuple(families), tuple(LEDs), lineage


@dataclass(frozen=True)
class _TailUse:
    role: str
    phase_index: int
    epoch: object
    entry: object
    before: object
    after: object
    output: View
    footprint: tuple
    protected: tuple
    source: dict
    continuation: tuple
    completed_prefix_issued: bool = False


def _make_use(
    state, family, phase_index, epoch, before, after, output, protected, source, continuation
):
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
    if type(family) is TailFamily:
        owner = family.owner(epoch)
        footprints = family.footprints(epoch)
        direction = family.sites[0][4]
    else:
        require(type(family) is LedFamily, "unreviewed Tail reached family")
        context = next((v for v in family.contexts if v.epoch == epoch), None)
        require(context is not None, "Tail LED use loses its same source epoch")
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


def _reached_data(control, families, LEDs):
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
            source = dict(
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
                    source,
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
            source = dict(
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
                    source,
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
            source = dict(
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
                    source,
                    tuple(V + o for o in (6058, 6062, 6066, 6070, 6074, 6078, 6114)),
                )
            )
        suffix = phase.suffix
        if suffix is None or suffix["LED"] is None:
            continue
        patterns = suffix["LED"]["invocations"][:2]
        for epoch, call in zip(("normal", "warning"), patterns):
            proof = call["result"]["proof"]
            normal = call["result"]["normal"]
            require(
                proof["role"] == ROLES["LedPattern"]
                and call["site"] == V + (6792 if epoch == "normal" else 6828),
                "Tail LED reached word changes its current child or source epoch",
            )
            if normal is None:
                continue
            _call_binding(state.parents, state.children, call, frame)
            require(
                normal["initialized_bytes"] == (0, 2, 3, 4, 6, 7, 8)
                and normal["opaque_bytes"] == (1, 5, 9)
                and normal["physical_output_order"] == ((184, 0, 1), (186, 6, 4), (194, 2, 4))
                and call["output"].offset == 1064,
                "Tail LED word loses its exact initialized versus opaque source fields",
            )
            protected = call["result"]["protected"] + (
                _object(call["before"].header, "current LED Q"),
                _object(call["before"].decoder.header, "current LED D"),
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
            source = dict(
                invocation=call, normal=normal, opaque_bytes=(1, 5, 9), partial_after_kind=(0,)
            )
            uses.append(
                _make_use(
                    state,
                    led["result_words"],
                    index,
                    epoch,
                    call["before"].decoder,
                    normal["after"].decoder,
                    call["output"],
                    protected,
                    source,
                    (
                        proof["function"] + 184,
                        proof["function"] + 186,
                        proof["function"] + 194,
                        call["site"] + 4,
                    ),
                )
            )
            # These reads precede reuse of O by the next pattern or later
            # bool failures. No full LED Config success is inferred here.
            sites = (6814, 6818) if epoch == "normal" else (6862, 6868)
            guards = base + (_object(call["before"].header, "LED Q"),)
            source = dict(
                invocation=call, normal=normal, read_epoch=epoch, opaque_fields_remain_opaque=(5, 9)
            )
            uses.append(
                _make_use(
                    state,
                    led["pattern_words"],
                    index,
                    epoch,
                    normal["after"].decoder,
                    normal["after"].decoder,
                    replace(call["output"], writable=False),
                    guards,
                    source,
                    tuple(V + o for o in (*sites, 6828 if epoch == "normal" else 6876)),
                )
            )
        if suffix["normal"] is None:
            continue
        require(
            phase.suffix_effects["complete_after"] == V + 7112
            and guard["lineage"]["normal"] is not None
            and suffix["LED"]["normal"] is suffix["normal"]["source"],
            "Tail normal Config LED access precedes the actual full source and late stores",
        )
        first = suffix["LED"]["patterns"][0]
        before = suffix["normal"]["after"].decoder
        stages = tuple(v for v in phase.suffix_effects["LED"] if v["site"] in (V + 7034, V + 7108))
        require(
            len(stages) == 2 and all(v["source"] is first for v in stages),
            "Tail normal Config word borrows warning or another Pattern origin",
        )
        source = dict(
            invocation=first["invocation"],
            normal=first["source"],
            LED=suffix["LED"]["normal"],
            stages=stages,
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
                source,
                tuple(V + o for o in (7034, 7100, 7108, 7112)),
            )
        )
    return tuple(uses)


def _snapshot(control, families, LEDs, lineage, uses):
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
    control: object
    tail_families: tuple
    LED_families: tuple
    allocation_lineage: tuple
    uses: tuple
    checkpoint: object

    def current(self):
        _checked_graph(self)
        self.control.current()
        families, LEDs, lineage = _family_data(self.control)
        uses = _reached_data(self.control, families, LEDs)
        require(
            _snapshot(self.control, families, LEDs, lineage, uses) == self.checkpoint,
            "Tail current allocation/source/epoch/use relation changed",
        )
        return self


def _checked_graph(bridge):
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


def _check_member(bridge, use):
    family = next(
        (v for v in bridge.tail_families + bridge.LED_families if v.role == use.role), None
    )
    require(
        family is not None and any(use is v for v in bridge.uses),
        "Tail access is not an exact checked collection member",
    )
    output = (
        family.owner(use.epoch)
        if type(family) is TailFamily
        else next(v.owner for v in family.contexts if v.epoch == use.epoch)
    )
    require(
        use.footprint == family.footprints(use.epoch)
        and (use.output.start, use.output.extent, use.output.writable)
        == (output.start, output.extent, output.writable),
        "Tail reached access has another footprint/source owner",
    )
    return use


def _tail_access_group(state):
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


def _check_tail_uses(bridge):
    """Fresh authoritative batch; one full graph/upstream check, no ticket."""
    require(type(bridge) is _TailAccessBridge, "Tail batch lacks its actual private bridge")
    bridge.current()
    for use in bridge.uses:
        _check_member(bridge, use)
    return bridge.uses
