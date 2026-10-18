"""Closed Sources structural extension of the existing private control walk.

Pending later invocations retain their exact contexts without becoming active
successors, source packages, return facts or ownership. This raw structure
still needs the actual per-entry Source/aggregate guards and reached uses.
"""

from dataclasses import dataclass
from config_decoded_views import (
    _ControlSelection,
    _control_selection,
    _selected_successors,
    _foreign_ingress,
)
from config_source_children import (
    CHANNEL_SOURCE,
    ACCESS_TARGET_OFFSETS,
    source_children,
    source_return_profiles,
)
from validation import require


# Reviewed current predecessor contexts, normalized to the same typed parent.
# The BL identity is rebound through the current complete seq_u16 proof.
SOURCE_PENDING = {
    6156: (
        (6144, 4, "bl @pending:seq_u16"),
        (6148, 2, "mov r4, r0"),
        (6150, 2, "lsrs r6, r0, #0x10"),
        (6152, 2, "uxth r0, r0"),
        (6154, 2, "cmp r0, #0x2"),
        (6156, 4, "beq.w @parent+1a00"),
    ),
    6186: (
        (6170, 4, "bl @pending:seq_u16"),
        (6174, 2, "lsrs r5, r0, #0x10"),
        (6176, 2, "uxth r1, r0"),
        (6178, 2, "cmp r1, #0x2"),
        (6180, 4, "bne.w @parent+1932"),
        (6184, 2, "mov r6, r5"),
        (6186, 2, "b @parent+1a00"),
    ),
    6476: (
        (6460, 4, "bl @pending:seq_u16"),
        (6464, 2, "uxth r1, r0"),
        (6466, 4, "lsr.w r8, r0, #0x10"),
        (6470, 2, "cmp r1, #0x2"),
        (6472, 2, "bne @parent+197c"),
        (6474, 2, "mov r6, r8"),
        (6476, 2, "b @parent+1a00"),
    ),
    6550: (
        (6534, 4, "bl @pending:seq_u16"),
        (6538, 2, "uxth r1, r0"),
        (6540, 4, "lsr.w r9, r0, #0x10"),
        (6544, 2, "cmp r1, #0x2"),
        (6546, 2, "bne @parent+1998"),
        (6548, 2, "mov r6, r9"),
        (6550, 2, "b @parent+1a00"),
    ),
    6578: (
        (6562, 4, "bl @pending:seq_u16"),
        (6566, 2, "uxth r1, r0"),
        (6568, 4, "lsr.w r10, r0, #0x10"),
        (6572, 2, "cmp r1, #0x2"),
        (6574, 2, "bne @parent+19b4"),
        (6576, 2, "mov r6, r10"),
        (6578, 2, "b @parent+1a00"),
    ),
    6606: (
        (6590, 4, "bl @pending:seq_u16"),
        (6594, 2, "uxth r1, r0"),
        (6596, 4, "lsr.w r11, r0, #0x10"),
        (6600, 2, "cmp r1, #0x2"),
        (6602, 2, "bne @parent+19d0"),
        (6604, 2, "mov r6, r11"),
        (6606, 2, "b @parent+1a00"),
    ),
    6632: (
        (6616, 4, "bl @pending:seq_u16"),
        (6620, 2, "lsrs r1, r0, #0x10"),
        (6622, 2, "str r1, [sp, #0x11c]"),
        (6624, 2, "uxth r1, r0"),
        (6626, 2, "cmp r1, #0x2"),
        (6628, 2, "bne @parent+19ea"),
        (6630, 2, "ldr r6, [sp, #0x11c]"),
        (6632, 2, "b @parent+1a00"),
    ),
    6856: ((6854, 2, "movs r6, #0xe"), (6856, 2, "b @parent+1a00")),
}
SOURCE_FALLTHROUGH = {
    4408: dict(
        target=4410,
        context=(
            (4402, 2, "movs r0, #0x2"),
            (4404, 2, "str r0, [sp, #0x11c]"),
            (4406, 2, "movs r0, #0x0"),
            (4408, 2, "str r0, [sp, #0x118]"),
        ),
    ),
    6654: dict(
        target=6656,
        context=(
            (6642, 4, "bl @pending:seq_u16"),
            (6646, 2, "lsrs r1, r0, #0x10"),
            (6648, 2, "uxth r2, r0"),
            (6650, 2, "cmp r2, #0x2"),
            (6652, 2, "bne @parent+1a06"),
            (6654, 2, "mov r6, r1"),
        ),
    ),
}


@dataclass(frozen=True)
class _SourceSelection(_ControlSelection):
    widgets: object
    Source_parents: tuple
    Source_children: dict
    child_contexts: tuple


@dataclass(frozen=True)
class _SourceStructuralControl:
    selection: _SourceSelection
    raw_successors: tuple
    raw_returns: tuple
    Source_returns: tuple
    carrier_returns: dict
    nested_returns: tuple
    outside_shared_contexts: tuple
    unentered_outgoing: tuple
    confers_active_state: bool = False

    def __post_init__(self):
        require(
            type(self.selection) is _SourceSelection and self.confers_active_state is False,
            "raw Sources structure cannot grant active execution",
        )


def _pending_profile(selection):
    require(type(selection) is _SourceSelection, "unreviewed Sources control selection")
    require(
        selection.context is selection.widgets.views.context and selection.widget,
        "Sources selected parent leaves its actual Widget predecessor",
    )
    return (
        {
            site: dict(target=6656, operation=rows[-1][2].split(" ", 1)[0], context=rows)
            for site, rows in SOURCE_PENDING.items()
        },
        SOURCE_FALLTHROUGH,
    )


def _source_table_targets(selection, at, row):
    require(
        type(selection) is _SourceSelection, "Views selected invocation has an unresolved PC effect"
    )
    proofs = tuple(p for p in selection.Source_children.values() if p["role"] == CHANNEL_SOURCE)
    require(len(proofs) == 1, "Sources table lacks its current complete child")
    proof = proofs[0]
    start = proof["function"]
    targets = tuple(start + off for off in ACCESS_TARGET_OFFSETS)
    require(
        at == start + 74
        and row.mnemonic == "tbb"
        and row.operands == "[pc, r6]"
        and proof["shape"] == "ChannelSource"
        and proof["access_targets"] == targets
        and proof["guarded_access_index_domain"] == (0, 5)
        and all(v in selection.selected for v in targets),
        "Sources structural table loses its exact guarded current targets",
    )
    # This is only fixed structure. Actual source-successful selector and its
    # dependent arm/return/effect pairing remain required by active control.
    return targets


def _source_structural_control(objects, rows, widgets, parents, children):
    from config_decoded_widgets import _WidgetsState
    from config_decoded_sources import _parents
    from config_finite_returns import _closed_child_returns
    from config_widget_contracts import ROLES as WIDGET

    require(
        type(widgets) is _WidgetsState, "Sources structure lacks the actual private predecessor"
    )
    bridge = widgets.family_bridge.current()
    c = widgets.views.context
    V = c.headers["visitor"]["proof"]["function"]
    require(
        objects is bridge.objects
        and rows is bridge.rows
        and parents == _parents(objects, rows, V, c.owner.start)
        and children == source_children(objects, rows, parents),
        "Sources structure changes its current parent/child input pair",
    )
    combined = dict(widgets.views.children)
    lineage = []
    for family in (widgets.children, children):
        for key, p in family.items():
            if key in combined:
                prior = combined[key]
                require(
                    {k: v for k, v in prior.items() if k != "coverage"}
                    == {k: v for k, v in p.items() if k != "coverage"},
                    "Sources shared child has inconsistent current bodies",
                )
                a = prior.get("coverage")
                b = p.get("coverage")
                require(
                    a == b
                    or isinstance(a, dict)
                    and isinstance(b, dict)
                    and {k: v for k, v in a.items() if k != "context"}
                    == {k: v for k, v in b.items() if k != "context"},
                    "Sources shared child has inconsistent current coverage",
                )
                lineage.append((key, prior, p))
            combined[key] = p
    all_parents = widgets.views.parents + widgets.parents + parents
    base = _control_selection(objects, rows, c, all_parents, combined)
    selection = _SourceSelection(
        base.context,
        base.children,
        base.proofs,
        base.selected,
        base.edges,
        frozenset((base.cuts - {V + 4376}) | {V + 5630}),
        base.widget,
        widgets,
        parents,
        children,
        tuple(lineage),
    )
    successors, returns, dormant = _selected_successors(selection)
    nested = _closed_child_returns(
        rows,
        parents,
        children,
        c.headers["visitor_frame"].start,
        source_return_profiles(),
        ("seq_f32", WIDGET["warning_colors"]),
        objects=objects,
    )
    require(
        nested and min(v["lowest_SP"] for v in nested) == c.headers["visitor_frame"].start - 120,
        "Sources structure loses its current child save/return geometry",
    )
    carriers = _source_carrier_returns(
        rows, parents, children, nested, c.headers["visitor_frame"].start
    )
    pending = _foreign_ingress(objects, rows, selection)
    return _SourceStructuralControl(
        selection,
        successors,
        returns,
        nested,
        carriers,
        widgets.structural.nested_returns + nested,
        pending,
        dormant,
    )


def _source_carrier_returns(rows, parents, children, returns, sp):
    """Actual per-call transfer, conditional on separate protected save slots.

    Complete current body matching and the existing finite saved-PC walk are
    prerequisites. This does not initialize a carrier, choose a source arm,
    or establish a pointer's parent lifetime from the callee ABI alone.
    """
    from config_decoded_widgets import _source_fp_preservation

    high = tuple("r" + str(i) for i in range(4, 12))
    checked = []
    for parent in parents:
        for off, operation, target, role, frame in parent["ordinary_edges"]:
            pc = parent["function"] + off + 4
            matches = tuple(
                v
                for v in returns
                if v["function"] == target
                and v["role"] == role
                and v["entry_SP"] == sp
                and v["return_PC"] == pc
                and v["return_SP"] == sp
            )
            require(
                operation == "bl"
                and len(matches) == 1
                and matches[0]["preserved_high_gprs"] == high,
                "Sources child changes an actual live high GPR carrier",
            )
            checked.append((pc - 4, target, role, high))
    require(
        len(checked) == 57 and len({v[0] for v in checked}) == 57,
        "Sources carrier graph omits a current parent call",
    )
    return dict(
        parent_calls=tuple(checked),
        high_fp=_source_fp_preservation(rows, children),
        architectural_LR_preserved=False,
        requires_actual_protected_save_slots_and_parent_lifetimes=True,
    )


@dataclass(frozen=True)
class _SourceActiveControl:
    state: object
    source_guards: tuple
    record: dict
    checkpoint: object
    complete_ordinary_group_issued: bool = False

    def current(self):
        from config_decoded_sources import _phase_graph

        require(
            _phase_graph((self.source_guards, self.record)) == self.checkpoint,
            "Sources active source/control record changed",
        )
        self.state.current()
        guards, record = _source_control_data(self.state)
        require(
            _phase_graph((guards, record)) == self.checkpoint,
            "Sources active guarded successor/phase association changed",
        )
        return self


def _phase_guards(state, phase):
    """Paired source outcomes, not a body-list or completed-prefix approval."""
    from config_abstract import Span
    from config_source_effects import SOURCE_EPOCHS, _source_error_origins

    require(
        any(phase is p and phase.entry is e for p, e in zip(state.phases, state.entries)),
        "Sources guarded phase leaves its actual entry association",
    )
    frame = state.widgets.views.context.headers["visitor_frame"]
    V = state.widgets.views.context.headers["visitor"]["proof"]["function"]
    array = phase.array
    current = phase.entry.sequence
    calls = []
    parsed = 0
    saved, value, producer = phase.entry.saved_top_count
    require(
        (saved.storage, saved.offset, saved.extent, value, producer)
        == (frame, 280, 4, Span(7, 7), V + 4360)
        and phase.entry.exit.sequence.count == Span(7, 7)
        and any(
            v.space == frame.space and v.start == saved.start and v.extent == 4 and not v.writable
            for v in phase.entry.protected
        ),
        "Sources saved top count loses its actual initialized/protected epoch",
    )
    # _source_entries matched the same selected top8 predecessor and the
    # complete 4348..4374 constructor. Its post-init guard is passed into
    # every fresh child/copy/suffix and every authored parent write check.
    # The following exact reaching loads use that preserved cell, rather
    # than inferring its value from the independently changing V292 count.
    expected = {
        4796: ("ldr", "r0, [sp, #0x118]"),
        4798: ("cmp", "r0, #0x0"),
        5044: ("ldr", "r0, [sp, #0x118]"),
        5046: ("cmp", "r0, #0x1"),
        5052: ("ldr", "r0, [sp, #0x118]"),
        5054: ("mov", "r1, r4"),
        5056: ("subs", "r0, #0x2"),
        5058: ("str", "r0, [sp, #0x124]"),
    }
    for off, pair in expected.items():
        row = state.rows.get(V + off)
        require(
            row is not None
            and (row.mnemonic, row.operands) == pair
            and state.objects.read_only(V + off, len(row.raw)) == row.raw,
            "Sources saved top count changes its reaching load/update",
        )
    require(
        current.count == Span(8, 8)
        and phase.entry.exit.sequence.count == Span(7, 7)
        and phase.parent_envelopes["stores"] == 80
        and len(state.structural.carrier_returns["parent_calls"]) == 57,
        "Sources guarded phase lacks actual count/protected-write/callee relations",
    )
    for index, invocation in enumerate(array["invocations"]):
        off, out, tag = SOURCE_EPOCHS[index]
        result = invocation["result"]
        error_origins = _source_error_origins(result)
        require(
            invocation["index"] == index
            and invocation["site"] == V + off
            and invocation["before"] == current
            and current.count == Span(8 - index, 8 - index),
            "Sources guarded invocation changes its paired call/source/count",
        )
        proof = result["proof"]
        require(
            state.children[proof["function"], CHANNEL_SOURCE] == proof
            and proof["guarded_access_index_domain"] == (0, 5)
            and result["returns"]
            and result["last_output_store"] == proof["function"] + 734,
            "Sources guarded invocation loses its current table/output/return body",
        )
        access = tuple(v for v in result["transitions"] if v["field"] == "access")
        require(
            len(access) <= 1
            and all(
                v["domain"] == (0, 5) and v["guard"] == "same parsed enum in domain" for v in access
            ),
            "Sources table argument is not the same successfully bounded Access selector",
        )
        entered = bool(access and access[0]["after"] is not None)
        normal = result["normal"]
        calls.append(
            dict(
                index=index,
                site=V + off,
                before=current,
                Access_table_entered=entered,
                guarded_Access_domain=(0, 5),
                Access_targets=proof["access_targets"] if entered else (),
                error_alternatives=result["errors"],
                guarded_routes=result["guarded_routes"],
                primitive_error_origins=error_origins,
                output_store_reached=normal is not None,
                after=None if normal is None else normal["after"],
            )
        )
        if normal is None:
            require(
                index == len(array["invocations"]) - 1
                and len(array["packages"]) == index
                and array["sequence"] is None,
                "Sources failed invocation advances to another package",
            )
            current = None
            break
        package = array["packages"][index]
        require(
            package["source"] is normal
            and normal["before"] == current.decoder
            and normal["count_after"] == Span(7 - index, 7 - index)
            and normal["return_site"] == V + off + 4
            and (
                normal["definition_domain"],
                normal["decoder_domain"],
                normal["evidence_domain"],
                normal["freshness_domain"],
            )
            == ((0, 15), (0, 11), (0, 3), (0, 65535))
            and normal["tag_domain"] == (0, 1)
            and package["sequence_before"] == current
            and package["sequence_after"].decoder == normal["after"]
            and (package["output"].storage, package["output"].offset, package["output"].extent)
            == (frame, out, 22)
            and package["captured_tag"].start == frame.start + tag,
            "Sources normal arm changes its actual package/selector/output/return pairing",
        )
        current = package["sequence_after"]
        parsed += 1
    require(
        current == array["sequence"]
        and (current is None or parsed == 8 and current.count == Span(0, 0)),
        "Sources guarded array bypasses a current package",
    )
    complete = current is not None
    publication = None
    suffix_relation = None
    if not complete:
        require(
            phase.effects is None and phase.remaining is None,
            "Sources parse failure reaches Config publication or remaining fields",
        )
    else:
        effects = phase.effects
        suffix = phase.remaining
        require(
            effects is not None
            and suffix is not None
            and len(effects["slots"]) == 8
            and all(
                slot["source_package"] is package and slot["index"] == i
                for i, (slot, package) in enumerate(zip(effects["slots"], array["packages"]))
            )
            and suffix["before"].decoder == current.decoder
            and suffix["before"].count == Span(7, 7),
            "Sources fixed-array last stores or remaining-field cursor are unpaired",
        )
        for normal in suffix["normal"]:
            require(
                normal["after"].count == Span(4, 4)
                and normal["after"].decoder.header == current.decoder.header
                and normal["after"].decoder.payload == current.decoder.payload
                and normal["Fuel"]["complete_store"] == V + 5570
                and normal["colors"]["complete_store"] == V + 5628,
                "Sources outgoing top4 precedes the paired Fuel/Colors last stores",
            )
        require(
            all(
                v["Fuel"]["complete_store"] == V + 5570
                and v["source_guard"]
                == (
                    v["Fuel"]["wire_tag"],
                    v["Fuel"]["emitted_tag"] if v["Fuel"]["wire_tag"] == 2 else None,
                )
                for v in suffix["Fuel_assignments"]
            ),
            "Sources retained Fuel assignment mixes its source/density arm",
        )
        from config_source_lineage import _publication_lineage, _suffix_lineage

        publication = _publication_lineage(
            state.objects,
            state.rows,
            state.parents,
            array,
            effects,
            frame,
            phase.entry.carriers,
            state.structural.carrier_returns,
            phase.parent_envelopes,
        )
        suffix_relation = _suffix_lineage(
            state.objects,
            state.rows,
            state.parents,
            suffix,
            frame,
            state.widgets.views.context.owner,
            state.structural.carrier_returns,
            phase.parent_envelopes,
        )
    return dict(
        entry_index=next(i for i, p in enumerate(state.phases) if p is phase),
        invocations=tuple(calls),
        parsed_sources=parsed,
        complete_Source_array=complete,
        Controls_ready=bool(complete and phase.remaining["normal"]),
        source_array_last_stores=()
        if not complete
        else tuple(v["complete_after"] for v in phase.effects["slots"]),
        publication_lineage=publication,
        suffix_lineage=suffix_relation,
        saved_top_count=dict(
            cell=saved,
            value=value,
            initialized_at=producer,
            checked_read_sites=(V + 4796, V + 5044, V + 5052),
            reached_reads=(
                ()
                if not complete
                else (V + 4796,)
                + ((V + 5044, V + 5052) if phase.remaining["warning"] is not None else ())
            ),
            later_top_count_store=()
            if not complete or phase.remaining["warning"] is None
            else (V + 5058, 5),
            guard_rejections=((V + 4800, 0), (V + 5048, 1)),
        ),
        retained_Fuel_assignments=() if not complete else phase.remaining["Fuel_assignments"],
    )


def _source_control(state):
    """Internally derived named ordinary relation, before reached-use closure.

    Raw structure, descriptions and receipts cannot enter this interface.
    Source/variant outcomes remain paired and errors stay stopped; stronger
    ownership and exceptional resume are not consequences of this relation.
    """
    from config_decoded_sources import _SourcePhasesState
    from config_decoded_sources import _phase_graph

    require(
        type(state) is _SourcePhasesState, "Sources control lacks its actual private phase state"
    )
    state.current()
    guards, record = _source_control_data(state)
    return _SourceActiveControl(state, guards, record, _phase_graph((guards, record)))


def _source_control_data(state):
    from config_decoded_widgets import _WidgetActiveControl

    widgets = state.widgets
    previous = widgets.control
    require(
        state.entries
        and type(previous) is _WidgetActiveControl
        and previous.state is widgets.views
        and previous.parents is widgets.parents
        and previous.children is widgets.children,
        "Sources active control lacks its actual checked ordinary predecessor",
    )
    guards = tuple(_phase_guards(state, p) for p in state.phases)
    # current() recomposes the predecessor profile through _source_entries.
    # No arbitrary external exclusions or raw-context cuts are accepted.
    extension = dict(
        excluded_taken=dict(previous.profile["excluded_taken"]),
        excluded_fallthrough=dict(previous.profile["excluded_fallthrough"]),
    )
    successors, returns, dormant = _selected_successors(state.structural.selection, extension)
    require(not dormant, "Sources active control retains an unproved outgoing route")
    pending = _foreign_ingress(state.objects, state.rows, state.structural.selection)
    record = dict(
        selected_instructions=len(state.structural.selection.selected),
        successors=successors,
        nested_returns=state.structural.nested_returns,
        returns=returns,
        stopped_frontiers=tuple(sorted(state.structural.selection.cuts)),
        outside_shared_contexts=pending,
        pending_contexts_confer_state=False,
        full_visitor_return_pending=True,
    )
    return guards, record


def _structural_data(value, widgets, parents, children):
    """Complete immutable-checkpoint input, excluding only checked identities."""
    require(
        type(value) is _SourceStructuralControl
        and value.selection.widgets is widgets
        and value.selection.context is widgets.views.context
        and value.selection.Source_parents is parents
        and value.selection.Source_children is children
        and value.confers_active_state is False,
        "Sources raw structure lost its same private context/parent/child association",
    )
    s = value.selection
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
        raw_successors=value.raw_successors,
        raw_returns=value.raw_returns,
        Source_returns=value.Source_returns,
        carrier_returns=value.carrier_returns,
        nested_returns=value.nested_returns,
        outside_shared_contexts=value.outside_shared_contexts,
        unentered_outgoing=value.unentered_outgoing,
        confers_active_state=value.confers_active_state,
    )
