"""Actual SizeZ checksum continuation, stopped at diagnostics or Boot return.

Private source/validator state is consumed once. Closed continuation grammars
remain in config_loader_continuation; this module joins their typed phases,
owned effects and finite issuer lanes. No public description creates ownership.
"""

from dataclasses import replace
from config_validator_return import _checksum_refill_validator_state
from config_loader_continuation import _checked_continuation, _destination
from config_validator_activation import computed_context_dispositions
from config_regions import Storage, require_disjoint, require_writes, _control
from config_headers import ROOT
from config_stack import canonical_symbol
from validation import require


def _phase_guards(source, context):
    """Retire only the metadata fields at this actual continuation boundary."""
    prepared = source["widget"]["prepared"]
    owner = context.config.storage
    B = owner.start
    old = prepared["protected"][:-1]
    retired = [v for v in old if (v.space, v.start, v.extent) == ("address", B + 14576, 28)]
    require(len(retired) == 1, "continuation loses exact prior metadata protection")
    retained = tuple(v for v in old if v is not retired[0])
    guards = retained + tuple(
        Storage("address", label, B + offset, n, alignment, context.phase, False)
        for label, offset, n, alignment in (
            ("unchanged Config payload", 0, 14576, 4),
            ("cache padding", 14585, 3, 1),
            ("taken and enclosing padding", 14600, 4, 4),
        )
    )
    external = tuple(
        v
        for v in retained
        if not (
            v.space == context.caller.space
            and context.caller.start
            <= v.start
            < v.start + v.extent
            <= context.caller.start + context.caller.extent
        )
    )
    # Parent saved cells are contained in the caller allocation, not additional
    # disjoint allocations. They remain in guards for every exact write check.
    require_disjoint((context.caller,) + external)
    return external, guards


def _join_inputs(description, source, context, boundary, state, continuation):
    P = context.caller.start
    B = context.config.start
    R = context.result.start
    result = state["result"]
    returned = result["normal_return"]
    root = boundary["loader_geometry"]
    start = continuation["proofs"]["fallback"]["function"]
    fallback = continuation["validator_contexts"][0]
    require(
        (
            fallback["call"] + 4,
            fallback["result"].start,
            fallback["r4"],
            fallback["r8"],
            fallback["r10"],
        )
        == (start, R, R, P + 112, B)
        and R == P + 236
        and returned["return_to"] == start
        and returned["final_sp"] == P
        and returned["restored_bytes"] == 320
        and state["closure"]["return_to"] == start
        and state["closure"]["invocations"],
        "loader continuation lacks its same privately checked validator return",
    )
    require(
        {"r4", "r8", "r10", "r7", "sp"} <= set(returned["preserved"]),
        "validator return loses actual continuation carriers",
    )
    # These facts are returned by the current source/loop/first-error proofs,
    # not accepted from the public readonly normal-alternative description.
    first = state["relations"]["first"]
    fuel = state["relations"]["checked_Fuel_to_Ok"]
    require(
        first["Some_tags"] == (0, 1)
        and first["initial_None"] == 2
        and first["Err_payload_before_tag"]
        and first["Ok_s0_before_tag2"]
        and fuel["same_B"] == B
        and fuel["constructor_word"] == 2
        and fuel["selected_f32"][2] == 0x416B3333,
        "continuation loses its typed Err/Ok producer and actual stoichiometric source",
    )
    require(
        result["error_copy"]["possible_writes"]
        and state["relations"]["checked_owned_write_intervals"],
        "continuation loses complete Result effects",
    )
    fault = continuation["retained_fault"]["view"]
    prepared = source["widget"]["prepared"]
    require(
        fault.start == P + 8
        and fault.extent == 16
        and prepared["fault"].start == P + 8
        and continuation["retained_fault"]["first_word"] == "(P16 & 0xffff0000)+P20",
        "continuation changes retained checksum fault reconstruction",
    )
    # CRC selection and r5=0/r4=5 -> P16/P20 were consumed in the private
    # refill producer; its complete source/child chain protected P8..24. No
    # initial-data tag or declaration ordinal is substituted here.
    # The private state already contains the checked source constructor. Its
    # identity is the same source_context and source, without a second parse.
    require(
        description["source_context"] == boundary["history"]["source_context"]
        and description["checksum_alternative_observed_taken"] is False
        and prepared["config"].start == B
        and source["outgoing"]["r10"] == B,
        "continuation changes actual selected checksum source/Config",
    )
    fault_source = state["fault_source"]
    require(
        fault_source["source_variant"] == "ConfigError::Checksum"
        and fault_source["registers_until_String_return"] == (("r5", 0), ("r4", 5))
        and fault_source["stored_words"][2:] == ((P + 16, 0), (P + 20, 5))
        and fault_source["retained"] == (P + 8, 16),
        "continuation lacks its checked CRC constructor and retained split fault",
    )
    require(
        root["frame"] == (P, P + 456)
        and root["LR_value"] == boundary["history"]["once_return_transport"]["LR_value"]
        and root["LR_value"] == (boundary["history"]["boot_initialize_call"] + 4) | 1,
        "loader saved return value is not its original once/Boot transport",
    )
    return start


def _issuer_physical(continuation, description, context, boundary):
    address = continuation["issuer"]["objects"][0]
    S = boundary["history"]["preclaim"]["startup"]["copy"]["initial_sp"]
    require(
        type(address) is int and address % 4 == 0 and 0 <= address < address + 4 <= 0x100000000,
        "issuer exclusive owner loses checked alignment/extent",
    )
    forbidden = [
        (context.config.start, 14604),
        (description["normal_child_low"], S - description["normal_child_low"]),
    ]
    forbidden.extend(
        (v["interval"][0], v["interval"][1] - v["interval"][0])
        for v in boundary["physical_protected"]
    )
    for lo, n in forbidden:
        require(
            not (lo < address + 4 and address < lo + n),
            "issuer physical write aliases live Config/task/stack/return state",
        )
    return address


def _return_relation(objects, rows, source, context, boundary, state, continuation, guards):
    """Finite current loader save map, distinct from architectural LR."""
    P = context.caller.start
    entry = P + 456
    original = boundary["loader_geometry"]["LR_value"]
    require(
        boundary["prologue"]["extent"] == 14
        and boundary["loader_geometry"]["root_frame_bytes"] == 456,
        "loader original save map lacks its complete actual prologue",
    )
    saves = next(
        (
            v
            for v in source["widget"]["prepared"]["protected"]
            if (v.space, v.start, v.extent) == (context.caller.space, P + 352, 104)
        ),
        None,
    )
    require(saves is not None, "loader continuation has no protected original save map")
    writes = tuple(continuation[key] for key in ("fallback_writes", "publication_writes"))
    for pairs in writes:
        require_writes(
            tuple(v for v, o, n in pairs), pairs, guards + (source["widget"]["prepared"]["fault"],)
        )
    for lo, hi in state["relations"]["checked_owned_write_intervals"]:
        require(
            hi <= P + 352 or entry <= lo,
            "normal validator write aliases original loader saves/return",
        )
    restore = continuation["return_interface"]
    proof = continuation["proofs"]["restore"]
    require(
        (
            restore["parent_frame"],
            restore["incoming_SP"],
            restore["restored_bytes"],
            restore["final_SP"],
        )
        == (456, P + 416, 456, entry)
        and proof["extent"] == 8
        and proof["frame"] == 0,
        "loader return outline belongs to another physical frame",
    )
    # Complete matched prefix: discard locals352, restore VFP64; exact outline:
    # discard duplicate r7 slot4, restore high16, then low16 and original PC4.
    cells = (
        tuple((P + 352 + 8 * i, "d" + str(8 + i), 8) for i in range(8))
        + ((P + 416, "duplicate frame r7", 4),)
        + tuple((P + 420 + 4 * i, "r" + str(8 + i), 4) for i in range(4))
        + tuple((P + 436 + 4 * i, "r" + str(4 + i), 4) for i in range(4))
        + ((P + 452, original, 4),)
    )
    require(
        sum(n for a, v, n in cells) == 104 and cells[-1] == (entry - 4, original, 4),
        "loader saved-PC map loses its original return cell",
    )
    return dict(
        SP=entry,
        PC=original & ~1,
        saved_PC_slot=P + 452,
        saved_value=original,
        restore_entry_SP=P + 416,
        outline=proof["function"],
        saved_intervals=tuple((a, n) for a, v, n in cells),
        r0="issued previous identity or zero; not Config pointer",
        architectural_LR_preservation_claimed=False,
        Boot_continuation_executed=False,
    )


def _closure(rows, context, state, continuation, returned):
    """Closed finite continuation successors and source-labelled issuer lanes.

    No scalar ISA evaluator: only the already matched fallback/issuer/restore
    family receives these guard/phase transitions. Any extra PC/SP operation
    cannot acquire a transition by name or by physical adjacency.
    """
    proofs = continuation["proofs"]
    issuer = continuation["issuer"]
    issuer_window = issuer["window"]
    F = proofs["fallback"]["function"]
    U = continuation["default_error"]
    Z = issuer["failure_suffix"]
    publication = proofs["publication"]["function"]
    restore = proofs["restore"]["function"]
    require(
        issuer["shape"] == "sizez"
        and issuer["attempts"] == 1
        and issuer["frame"] == 456
        and issuer["helper_calls"] == ()
        and issuer["local_write_ranges"] == ()
        and publication == issuer_window + 56,
        "actual continuation borrows another issuer ABI/effect family",
    )
    bodies = (
        (F, F + 62),
        (issuer_window, issuer_window + 56),
        (publication, publication + 26),
        (restore, restore + 8),
        (U["window"], U["window"] + U["extent"]),
        (Z["window"], Z["window"] + Z["extent"]),
    )
    sites = {a for lo, hi in bodies for a in rows if lo <= a < hi}
    require(
        len({byte for lo, hi in bodies for byte in range(lo, hi)})
        == sum(hi - lo for lo, hi in bodies),
        "continuation selected intervals overlap",
    )
    # Diagnose only the matched two frontiers. Neither call is entered.
    cut_sites = {U["site"]: ("reviewed_defaults", U), Z["site"]: ("identity_first", Z)}
    pending = [(F, context.caller.start, "validator_return")]
    seen = set()
    projection = set()
    cuts = []
    returns = []
    lanes = set()
    while pending:
        at, sp, lane = pending.pop()
        key = (at, sp, lane)
        if key in seen:
            continue
        seen.add(key)
        require(at in sites, "selected loader successor leaves matched continuation")
        row = rows[at]
        family, conditional = _control(row)
        following = at + len(row.raw)
        projection.add(at)
        if at in cut_sites:
            label, proof = cut_sites[at]
            require(
                family == "bl" and not conditional,
                "pending loader diagnostic changes its call effect",
            )
            cuts.append(
                dict(
                    kind="pending_diagnostic_cut",
                    site=at,
                    target=_destination(row),
                    source_phase=label,
                    SP=sp,
                    return_token=None,
                    machine_nonreturn_established=False,
                    retained_loader_saved_PC=returned["saved_PC_slot"],
                    Result_and_error=((context.result.start, 12), (context.caller.start + 112, 12))
                    if label == "reviewed_defaults"
                    else (),
                    concrete_reference_levels=0 if label == "reviewed_defaults" else None,
                    source_guard="Err tag0/1; concrete copied ValidationError"
                    if label == "reviewed_defaults"
                    else "successful previous0",
                )
            )
            continue
        if at == F + 6:
            require(
                lane == "validator_return" and family == "b" and conditional,
                "fallback guard has no same-invocation Result alternative",
            )
            pending.extend(((U["window"], sp, "Err0/1"), (following, sp, "Ok2")))
            continue
        if at == issuer_window + 8:
            require(
                lane in ("Ok2", "retry") and family == "b" and conditional,
                "issuer increment loses current old-value lane",
            )
            pending.extend(
                ((_destination(row), sp, "exhausted"), (following, sp, "increment_nonoverflow"))
            )
            continue
        if at == issuer_window + 18:
            require(
                lane == "increment_nonoverflow",
                "issuer reload comparison loses expected-old transport",
            )
            pending.extend(((_destination(row), sp, "retry"), (following, sp, "exclusive_equal")))
            continue
        if at == issuer_window + 24:
            require(
                lane == "exclusive_equal", "issuer store lacks the matching exclusive comparison"
            )
            pending.extend(((_destination(row), sp, "retry"), (following, sp, "issued")))
            continue
        if at == issuer_window + 30:
            require(
                lane in ("retry", "issued"),
                "issuer retry guard lacks source store-status provenance",
            )
            pending.append((_destination(row) if lane == "retry" else following, sp, lane))
            continue
        if at == issuer_window + 50:
            require(
                lane == "issued", "issuer zero guard was not reached by successful exclusive store"
            )
            pending.extend(
                ((_destination(row), sp, "issued_zero"), (following, sp, "issued_nonzero"))
            )
            continue
        if at == publication:
            require(
                lane in ("issued_nonzero", "exhausted"),
                "publication mixes issuer alternative or decoded context",
            )
            lanes.add(lane)
        if at == publication + 16:
            require(sp == context.caller.start, "loader locals retire from another SP")
            sp += 352
        elif at == publication + 18:
            require(sp == context.caller.start + 352, "loader VFP restores from another save map")
            sp += 64
        elif at == restore:
            require(sp == context.caller.start + 416, "loader tail has another parent-owned SP")
            sp += 4
        elif at == restore + 2:
            sp += 16
        elif at == restore + 6:
            require(
                family == "return"
                and sp + 20 == returned["SP"]
                and lane in ("issued_nonzero", "exhausted"),
                "loader normal return loses its paired exit/save/SP",
            )
            returns.append(
                dict(
                    site=at,
                    PC=returned["PC"],
                    SP=sp + 20,
                    source_lane=lane,
                    saved_PC_slot=returned["saved_PC_slot"],
                    token="original Boot call continuation",
                )
            )
            continue
        elif row.mnemonic.startswith(("push", "pop", "vpush", "vpop")) or row.operands.startswith(
            "sp,"
        ):
            raise AssertionError("continuation gains an unreviewed SP/save operation")
        if family == "b":
            require(not conditional, "continuation gains an unbound conditional phase edge")
            pending.append((_destination(row), sp, lane))
        else:
            require(
                family is None, "continuation gains an unbound call/computed/implicit PC effect"
            )
            pending.append((following, sp, lane))
    require(
        lanes == {"issued_nonzero", "exhausted"}
        and len(returns) == 2
        and {v["source_phase"] for v in cuts} == {"reviewed_defaults", "identity_first"}
        and projection == sites,
        "actual continuation omits a source exit, pending cut or current instruction",
    )
    return dict(
        scope="selected checksum ordinary prefix through loader return before pending diagnostic/exception",
        return_to=returned["PC"],
        return_sites=tuple(returns),
        pending_diagnostic_cuts=tuple(cuts),
        invocations=state["closure"]["invocations"],
        selected_continuation_sites=tuple(sorted(projection)),
        instruction_states=len(seen),
        diagnostics_entered_or_nonreturn_proved=False,
    ), projection


def checksum_refill_loader_return(objects, instructions, source_context):
    description, source, context, boundary, state = _checksum_refill_validator_state(
        objects, instructions, source_context
    )
    names = [
        n
        for n, e in objects.elf.functions.items()
        if canonical_symbol(n) == ROOT and e[0] & ~1 == boundary["loader_geometry"]["function"]
    ]
    require(len(names) == 1, "actual continuation has ambiguous same-B loader")
    external, guards = _phase_guards(source, context)
    frame = replace(context.caller, phase=source["widget"]["prepared"]["config"].storage.phase)
    continuation = _checked_continuation(
        objects, names[0], instructions, frame, external, source, guards
    )
    _join_inputs(description, source, context, boundary, state, continuation)
    issuer = _issuer_physical(continuation, description, context, boundary)
    returned = _return_relation(
        objects, state["rows"], source, context, boundary, state, continuation, guards
    )
    closure, projection = _closure(state["rows"], context, state, continuation, returned)
    # Query every current architectural envelope against a conservative hull
    # containing all newly selected windows, including the separate restore.
    # Actual active tables keep their prior current tuples and invocation IDs.
    intervals = [
        (p["function"], p["function"] + p["extent"]) for p in continuation["proofs"].values()
    ]
    intervals.extend(
        (v["window"], v["window"] + v["extent"])
        for v in (continuation["default_error"], continuation["issuer"]["failure_suffix"])
    )
    low = min(a for a, b in intervals)
    high = max(b for a, b in intervals)
    computed = computed_context_dispositions(
        objects,
        state["rows"],
        dict(function=low, extent=high - low),
        state["projection"] | projection,
        state["computed"],
        closure,
    )
    B = context.config.start
    pairs = (
        dict(
            status=0,
            identity="nonzero successfully issued previous",
            guard="successful STREX and previous!=0",
        ),
        dict(status=2, identity=0, guard="checked increment overflow; no exclusive store"),
    )
    static = tuple(
        (v.start, v.extent)
        for v in guards
        if v.space == "address" and B <= v.start < v.start + v.extent <= B + 14604
    )
    metadata = tuple(
        (v.start + offset, n)
        for key in ("fallback_writes", "publication_writes")
        for v, offset, n in continuation[key]
    )
    return dict(
        phase="checksum_refill_loader_return_pending_diagnostics_and_other_loader_routes",
        mode="sizez",
        claim_scope=closure["scope"],
        source_context=description["source_context"],
        validator=description,
        result_consumption=dict(
            tags=(0, 1, 2),
            Result=(context.result.start, 12),
            Err_copy=(context.result.start, context.caller.start + 112, 12),
            reference_levels=0,
            opaque_padding=(11, 12),
        ),
        fault_installation=dict(
            constructor="ConfigError::Checksum",
            retained=(context.caller.start + 8, 16),
            first_word="(P16 & 0xffff0000)+P20",
            writes=tuple((a, n) for a, n in metadata if B + 14588 <= a),
            initial_data_reused=False,
        ),
        publication_pairs=pairs,
        issuer_retry=dict(
            owner=(issuer, 4),
            mismatch_and_store_failure=True,
            current_checked_region=(
                continuation["issuer"]["window"],
                continuation["issuer"]["extent"],
            ),
            termination_established=False,
            initial_value_not_live_invariant=True,
        ),
        protected_static_ranges=static,
        metadata_writes=metadata,
        continuation_closure=closure,
        computed_exclusions=computed,
        **{"return": returned},
        pending_diagnostics=description["pending_diagnostics"]
        + tuple((v["site"], v["source_phase"]) for v in closure["pending_diagnostic_cuts"]),
        decoded_caller_and_other_routes_pending=True,
        other_optimization_variants_pending=True,
        source_qualification=description["source_qualification"],
        external_qualifications=description["external_qualifications"],
        requires_M2_pre_init_source_reconciliation=description[
            "requires_M2_pre_init_source_reconciliation"
        ],
        requires_H8_consumed_input_closure=True,
        exception_graph_limit=description["exception_graph_limit"],
        exception_absence_established=False,
        target_access_requirements_unresolved=True,
        requires_complete_same_B_loader_before_owner=True,
        requires_full_diagnostic_and_activation_graph_and_resource_gates=True,
        hardware_qualified=False,
    )
