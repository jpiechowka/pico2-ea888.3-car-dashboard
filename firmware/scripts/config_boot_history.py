"""Finite first ordinary Reset invocation, stopping at the loader boundary.

These are authored successor/return/state families over the complete reviewed
entry grammars, not an ARM executor or a graph learned from an input ELF.
The public entry derives all addresses and recomputes captured source facts.
No acquired owner, exception-handler theorem or whole-image pass is emitted.
"""

import re
from config_build_source import inspect_source_context, validate_source_context
from config_entry_contracts import ENTRY_GRAMMARS
from config_preclaim import preclaim_configuration_preservation, _activation
from config_owner_entry import _direct_target
from config_regions import _control
from config_startup_contracts import STARTUP
from validation import require


# (offset, target phase/diagnostic, temporal scope). All current ordinary
# edges must occur exactly once. Targets are rebound to this SAME composition.
_EDGES = {
    "speed2_5": ((4, "cortex_main", "before"),),
    "speed2_3": ((18, "run", "before"),),
    "sizes_3": ((18, "run", "before"),),
    "sizez_3": ((16, "run", "before"),),
    "speed2_2": ((48, "boot_initializer", "before"), (280, "Busy", "diagnostic")),
    "sizes_2": ((74, "boot_initializer", "before"), (190, "Busy", "diagnostic")),
    "sizez_2": (
        (52, "callback_install", "before"),
        (56, "boot_initializer", "before"),
        (64, "raw_spawn", "before"),
        (70, "raw_poll", "before"),
        (90, "Busy", "diagnostic"),
    ),
    "speed2_4": (
        (22, "configuration_initialize", "before"),
        (66, "dashboard_initializer", "after"),
        (258, "resumed", "diagnostic"),
        (298, "Dashboard_Busy", "after"),
    ),
    "sizes_4": (
        (22, "configuration_initialize", "before"),
        (96, "dashboard_initializer", "after"),
        (178, "resumed", "diagnostic"),
        (214, "Dashboard_Busy", "after"),
    ),
    "sizez_4": (
        (22, "configuration_initialize", "before"),
        (88, "dashboard_initializer", "after"),
        (96, "raw_spawn", "after"),
        (124, "resumed", "diagnostic"),
        (142, "Dashboard_Busy", "after"),
    ),
    "speed2_6": ((38, "loader", "boundary"), (64, "already_taken", "diagnostic")),
    "sizez_10": ((32, "loader", "boundary"), (42, "already_taken", "diagnostic")),
    "sizez_8": ((6, "queue_push", "before"),),
    "speed2_0": (),
    "sizes_0": (),
    "sizez_0": (),
    "speed2_1": (),
    "sizes_1": (),
    "sizez_1": (),
    "sizez_6": (),
    "sizez_7": (),
    "sizez_9": (),
    "Reset": ((0, "pre_init", "before"), (56, "main", "before")),
}
_RETURNS = {
    "speed2_0": 20,
    "sizes_0": 10,
    "sizez_0": 10,
    "sizez_9": 6,
    "sizez_6": 46,
    "sizez_8": 14,
    "sizez_7": 38,
    "speed2_4": 240,
    "sizes_4": 168,
    "sizez_4": 120,
    "speed2_1": 40,
    "sizes_1": 28,
    "sizez_1": 22,
}
_CALLERS = {
    "boot_initializer": ("run", {"speed2": 48, "sizes": 74, "sizez": 56}),
    "callback_install": ("run", {"sizez": 52}),
    "raw_spawn": ("run", {"sizez": 64}),
    "queue_push": ("raw_spawn", {"sizez": 6}),
}
_EXTERNAL = (
    "exact expected physical entry and successful ordinary accesses",
    "interrupt/DMA/core quiescence and exclusive-memory behavior conditional, not measured",
    "deprecated Rust pre_init source issue remains Maintenance M2; emitted byte proof does not define Rust pre-init",
)


def _edges(preclaim):
    startup = preclaim["startup"]
    phases = startup["entry"]["entry"]["phases"]
    bodies = dict(phases, Reset=startup["reset"], pre_init=startup["pre_init"])
    classified = {}
    diagnostics = {v["kind"]: v["edges"][0] for v in preclaim["pending_diagnostics"]}
    for phase, proof in bodies.items():
        expected = () if phase == "pre_init" else _EDGES.get(proof["shape"])
        require(
            expected is not None and len(expected) == len(proof["ordinary_edges"]),
            "history has an unclassified ordinary edge",
        )
        for actual, (offset, target, scope) in zip(proof["ordinary_edges"], expected):
            off, op, address, role, frame = actual
            require(
                off == offset and op == ("b.w" if target == "loader" else "bl"),
                "history call/tail offset or operation differs",
            )
            if target in bodies:
                require(
                    address == bodies[target]["function"],
                    "history child targets another phase invocation",
                )
            elif target == "loader":
                require(
                    address == startup["entry"]["loader"]["function"],
                    "history tail selects another loader",
                )
            elif target == "Dashboard_Busy":
                require(
                    role == "_RNvNtCs_4core6result13unwrap_failed",
                    "post-boot diagnostic target differs",
                )
            else:
                require(
                    target in diagnostics
                    and diagnostics[target] == (phase, offset, address, role, frame),
                    "history diagnostic loses its exact actual edge",
                )
            classified[(phase, offset)] = dict(
                site=proof["function"] + offset,
                operation=op,
                target=address,
                phase=target,
                scope=scope,
                frame=frame,
            )
    return bodies, classified


def _successors(preclaim, rows, bodies, edges):
    """Exhaustive literal PC-effect partition, including unused later code.

    Local loops keep the existing source-family invariants (zero/copy, claim,
    publication and detach). There is no execution of arbitrary operands.
    Diagnostic and post-initialize successors are retained but not followed
    as predecessors of the selected successful claim.
    """
    result = []
    mode = preclaim["mode"]
    callback = preclaim["callback"]
    entry = preclaim["startup"]["entry"]["entry"]
    for phase, proof in bodies.items():
        contract = (
            STARTUP[proof["shape"]]
            if phase == "pre_init"
            else STARTUP["Reset"]
            if phase == "Reset"
            else ENTRY_GRAMMARS[proof["shape"]]
        )
        start = proof["function"]
        points = {o for o, w, t in contract["instructions"]}
        used = set()
        ret = (
            contract["instructions"][-1][0] if phase == "pre_init" else _RETURNS.get(proof["shape"])
        )
        for offset, width, text in contract["instructions"]:
            row = rows[start + offset]
            family, conditional = _control(row)
            following = offset + width
            record = dict(phase=phase, site=start + offset, operation=row.mnemonic, successors=())
            if (phase, offset) in edges:
                edge = edges[(phase, offset)]
                used.add(offset)
                require(
                    family in ("bl", "b") and not conditional,
                    "history ordinary edge has a new PC effect",
                )
                record.update(kind=edge["scope"], target=edge["target"], target_phase=edge["phase"])
                if edge["scope"] == "before":
                    nonreturn = edge["phase"] in (
                        "main",
                        "cortex_main",
                        "run",
                        "raw_poll",
                        "configuration_initialize",
                    )
                    record["continuation"] = (
                        "no return before selected claim" if nonreturn else start + following
                    )
                    if not nonreturn:
                        require(following in points, "returning child falls out of its caller")
                elif edge["scope"] == "diagnostic":
                    record["continuation"] = (
                        "infeasible in selected ordinary history; no diagnostic nonreturn inferred"
                    )
                elif edge["scope"] == "after":
                    record["continuation"] = "requires initialize return; outside this prefix"
                else:
                    record["continuation"] = "pending complete same-B specialized loader effects"
            elif family in ("b", "cbz", "cbnz"):
                target = _direct_target(row, family)
                require(
                    target - start in points,
                    "history local branch enters data/padding/outside code",
                )
                successors = (target,)
                if conditional:
                    require(following in points, "history conditional falls out of mapped code")
                    successors += (start + following,)
                record.update(
                    kind="local_source_family_loop" if target <= row.address else "local_guard",
                    successors=successors,
                )
            elif family == "blx":
                require(
                    (row.address, row.mnemonic, row.operands.split("@", 1)[0].strip())
                    == (callback[0], "blx", "r1")
                    and phase == ("raw_poll" if mode == "sizez" else "run")
                    and not conditional,
                    "history has an unbound executable callback consumer",
                )
                require(
                    entry["callback_edge"]["target"]
                    == callback[1]
                    == bodies["boot_poll"]["function"],
                    "history callback target is not the initialized Boot poll",
                )
                record.update(
                    kind="same_node_callback",
                    target=callback[1],
                    receiver=callback[2],
                    continuation="no callback return before selected claim",
                )
            elif family == "return" or family == "bx":
                require(
                    offset == ret
                    and not conditional
                    and (family == "return" or row.operands.strip() == "lr"),
                    "history has an unclassified saved-PC/LR consumer",
                )
                record.update(
                    kind="selected_child_return"
                    if phase
                    in (
                        "pre_init",
                        "boot_initializer",
                        "callback_install",
                        "raw_spawn",
                        "queue_push",
                    )
                    else "post_initialize_or_empty_batch_return",
                    continuation="bound selected LR/save slot, never arbitrary PC",
                )
            elif family == "udf":
                require(
                    phase == "Reset" and offset == 60 and not conditional,
                    "unclassified history exceptional instruction",
                )
                record.update(
                    kind="Reset_UDF_after_nonreturning_main",
                    continuation="not reached in this selected ordinary prefix",
                )
            else:
                require(
                    family is None, "unknown implicit/computed/security/exception history transfer"
                )
                if following in points:
                    record.update(kind="sequential", successors=(start + following,))
                else:
                    # Only the exact two-byte NOP after an unreachable diagnostic
                    # call precedes mapped data in the reviewed Z shapes.
                    prior = [v for (p, o), v in edges.items() if p == phase and o + 4 == offset]
                    require(
                        row.mnemonic == "nop"
                        and not row.operands.strip()
                        and width == 2
                        and len(prior) == 1
                        and prior[0]["scope"] in ("diagnostic", "after"),
                        "unbound function/data fallthrough in history",
                    )
                    record.update(
                        kind="padding_after_unselected_diagnostic",
                        continuation="no selected predecessor",
                    )
            result.append(record)
        require(
            used == {o for p, o in edges if p == phase},
            "history loses an authored ordinary transfer",
        )
    return tuple(result)


def _save_protection(preclaim, bodies):
    """Actual saved return/carrier cells, protected by the full write partition."""
    sp = preclaim["startup"]["copy"]["initial_sp"]
    mode = preclaim["mode"]
    saves = {}
    creation = {}
    for phase, proof in bodies.items():
        if phase in ("Reset", "pre_init", "dashboard_initializer"):
            continue
        entry, frame = _activation(mode, phase)
        entry += sp
        count = (
            24
            if phase == "boot_poll"
            else 16
            if phase in ("run", "raw_poll") and mode == "sizez"
            else 0
            if phase == "callback_install"
            else 8
        )
        saves[phase] = (entry - count, entry)
        # Z cortex pushes r5/r6 as eight scratch bytes later occupied by P;
        # only its upper r7/LR cells remain saved carriers. The complete PUSH
        # creates those cells but must not turn the lower P into readonly saves.
        pushed = (
            20
            if phase == "boot_poll"
            else 16
            if mode == "sizez" and phase in ("cortex_main", "run", "raw_poll")
            else count
        )
        creation[phase] = {(proof["function"], entry - pushed, entry)} if count else set()
        if phase == "boot_poll":
            creation[phase].add((proof["function"] + 4, entry - 24, entry - 20))
    base = ("main", "cortex_main", "run")
    ancestors = {
        "main": (),
        "cortex_main": ("main",),
        "run": base[:2],
        "boot_initializer": base,
        "callback_install": base,
        "raw_spawn": base,
        "queue_push": base + ("raw_spawn",),
        "raw_poll": base,
        "boot_poll": base + (("raw_poll",) if mode == "sizez" else ()),
        "configuration_initialize": base
        + (("raw_poll",) if mode == "sizez" else ())
        + ("boot_poll",),
    }
    for write in preclaim["writes"]:
        if write["scope"] in ("after", "after_diagnostic", "diagnostic"):
            continue
        phase = write["phase"]
        a, b = write["interval"]
        current = saves[phase]
        require(
            all(not (a < saves[p][1] and saves[p][0] < b) for p in ancestors[phase]),
            "history child write corrupts a retained caller return/carrier cell",
        )
        if current[0] < b and a < current[1]:
            require(
                (write["instruction"], a, b) in creation[phase],
                "history write corrupts its saved return/carrier cells",
            )
    return saves


def _returns(preclaim, bodies, edges, saves):
    mode = preclaim["mode"]
    sp = preclaim["startup"]["copy"]["initial_sp"]
    selected = []
    # The exact current hook preserves LR without stack writes; historical
    # Rust bodies retain their independently checked PUSH/POP contract.
    hook = bodies["pre_init"]
    contract = STARTUP[hook["shape"]]
    ret = contract["instructions"][-1][0]
    assembly = hook["shape"] == "assembly_rp235x"
    require(
        (assembly and hook["frame"] == 0 and contract["instructions"][-1][2] == "bx lr")
        or (
            not assembly
            and contract["instructions"][0][2] == "push {r7, lr}"
            and contract["instructions"][-1][2] == "pop {r7, pc}"
        ),
        "hook return ABI differs",
    )
    selected.append(
        dict(
            phase="pre_init",
            call_site=bodies["Reset"]["function"],
            continuation=bodies["Reset"]["function"] + 4,
            LR_value=(bodies["Reset"]["function"] + 4) | 1,
            entry_SP=sp,
            return_SP=sp,
            saved_LR=None if assembly else (sp - 4, sp),
            return_site=hook["function"] + ret,
        )
    )
    for phase, (caller, offsets) in _CALLERS.items():
        if mode not in offsets:
            continue
        body = bodies[phase]
        site = bodies[caller]["function"] + offsets[mode]
        edge = edges[(caller, offsets[mode])]
        require(
            edge["phase"] == phase and edge["scope"] == "before" and edge["operation"] == "bl",
            "history return is not paired with its selected call",
        )
        contract = ENTRY_GRAMMARS[body["shape"]]
        return_offset = _RETURNS[body["shape"]]
        expected = "bx lr" if phase == "callback_install" else "pop {r7, pc}"
        require(
            contract["instructions"][-1][2] == expected
            and contract["instructions"][-1][0] == return_offset,
            "history child restore/return ABI differs",
        )
        entry = sp + _activation(mode, phase)[0]
        if phase != "callback_install":
            require(
                contract["instructions"][0][2] == "push {r7, lr}"
                and saves[phase] == (entry - 8, entry),
                "history saved LR lacks the actual push/restore pair",
            )
        selected.append(
            dict(
                phase=phase,
                call_site=site,
                continuation=site + 4,
                LR_value=(site + 4) | 1,
                entry_SP=entry,
                return_SP=entry,
                saved_LR=None if phase == "callback_install" else (entry - 4, entry),
                return_site=body["function"] + return_offset,
                preserves=("r4", "r5", "r6", "r7", "r8", "r9", "r10", "r11", "sp"),
            )
        )
    return tuple(selected)


def _family(rows, body, inventory, label, intervals, begin, end, rules, returns, require_success):
    """One authored guard family, not instruction/value interpretation.

    Only its named zero/literal/exclusive-status guards refine the already
    closed successor relation. The single boolean is this family's exclusive
    result, correlated with its actual failure/success branch. Other guards
    retain both successors. No arbitrary register/scalar expressions execute.
    """
    start = body["function"]
    nodes = {v["site"] - start: v for v in inventory if v["phase"] == label[0]}
    allowed = {o for o in nodes if any(a <= o < b for a, b in intervals)}
    require(
        begin in allowed and end in nodes and end not in allowed,
        "history family interval/exit differs",
    )
    pending = [(begin, False)]
    seen = set()
    exits = set()
    decisions = {}
    calls = {v["call_site"]: v for v in returns}
    while pending:
        offset, success = pending.pop()
        if offset == end:
            exits.add(success)
            continue
        require(offset in allowed, "history family escapes its closed source boundary")
        if (offset, success) in seen:
            continue
        seen.add((offset, success))
        record = nodes[offset]
        row = rows[start + offset]
        family, conditional = _control(row)
        destinations = record["successors"]
        if record["kind"] == "before":
            require(
                record["site"] in calls
                and calls[record["site"]]["continuation"] == row.address + len(row.raw),
                "history family calls an unclosed return/effect interface",
            )
            destinations = (calls[record["site"]]["continuation"],)
        if offset in rules:
            rule = rules[offset]
            require(
                conditional and len(destinations) == 2,
                "history guard is not its actual conditional branch",
            )
            kind = rule[0]
            target, fallthrough = destinations
            if kind == "exclusive":
                producer = rows[start + rule[1]]
                operand = producer.operands.split("@", 1)[0].strip()
                status = operand.split(",", 1)[0]
                require(
                    producer.mnemonic in ("strex", "stlex", "strexb")
                    and re.fullmatch(r"r(?:[0-9]|1[0-2])", status)
                    and not success,
                    "history retries/republishes after a successful family exit",
                )
                if family == "cbnz":
                    require(
                        row.operands.split(",", 1)[0] == status,
                        "exclusive branch consumes another status",
                    )
                else:
                    compare = rows[start + rule[2]]
                    require(
                        row.mnemonic == "bne"
                        and compare.mnemonic == "cmp"
                        and compare.operands == status + ", #0x0",
                        "exclusive result/compare/failure branch differs",
                    )
                pending.extend(((target - start, False), (fallthrough - start, True)))
                decisions[offset] = (
                    kind,
                    producer.address,
                    status,
                    "failure=no write/retry; fallthrough=successful exclusive store",
                )
                continue
            if kind == "source_false":
                # The producer and source fact belong to this exact reviewed
                # family. Facts are established by the prior causal stage,
                # never supplied as caller booleans or general assumptions.
                require(
                    rule[2] in nodes and rows[start + rule[2]].address == start + rule[2],
                    "history source guard loses its actual producer",
                )
                take = False
                decisions[offset] = (kind, rule[1], start + rule[2])
            elif kind == "literal":
                producer = rows[start + rule[2]]
                require(
                    producer.mnemonic == "movs" and producer.operands.endswith(("#0x0", "#0x1")),
                    "history constant branch loses its literal producer",
                )
                take = rule[1]
                decisions[offset] = (kind, take, producer.address)
            elif kind == "retry_if_failed":
                require(row.mnemonic == "beq", "history retry flag has another conditional meaning")
                take = not success
                decisions[offset] = (kind, "success returns; failed/no-write retries")
            else:
                require(
                    kind == "nonempty_detach" and success,
                    "history callback precedes a successful same-head detach",
                )
                take = False
                decisions[offset] = (
                    kind,
                    "only published node is initialized Boot; saved next null",
                )
            destinations = (target if take else fallthrough,)
        else:
            require(
                record["kind"]
                in ("sequential", "local_guard", "local_source_family_loop", "before"),
                "unclassified PC/diagnostic/callback effect inside a history source family",
            )
        require(destinations, "history family has no classified successor")
        pending.extend((target - start, success) for target in destinations)
    require(
        exits and (not require_success or exits == {True}) and set(decisions) == set(rules),
        "history family exit lacks exclusive success or a guard obligation was not consumed",
    )
    return dict(
        phase=label[0],
        family=label[1],
        start=start + begin,
        exit=start + end,
        reached_offsets=tuple(sorted({o for o, status in seen})),
        guard_relations=decisions,
        exit_exclusive_success=tuple(sorted(exits)),
        nonterminating_retry_has_no_successor=True,
    )


def _families(preclaim, rows, bodies, inventory, returns):
    """Causal claim -> initialize -> publish -> detach -> guard -> once.

    All offsets/rules are authored for the already admitted complete shapes.
    Source parameters are same typed objects, not concrete Config field values.
    """
    mode = preclaim["mode"]
    result = []

    def add(phase, label, intervals, begin, end, rules=(), exclusive=False):
        result.append(
            _family(
                rows,
                bodies[phase],
                inventory,
                (phase, label),
                intervals,
                begin,
                end,
                dict(rules),
                returns,
                exclusive,
            )
        )

    if mode == "speed2":
        add(
            "run",
            "sole_zero_slot_claim",
            ((0, 36),),
            0,
            36,
            (
                (26, ("source_false", "same complete Boot pool zero; no earlier claimant", 20)),
                (34, ("exclusive", 28, 32)),
            ),
            True,
        )
        add("run", "initialized_same_callback_and_future", ((36, 54),), 36, 54)
        add(
            "run",
            "sole_publication_all_four_attempts",
            ((54, 196), (238, 246)),
            54,
            196,
            (
                (70, ("exclusive", 66, None)),
                (90, ("exclusive", 86, None)),
                (110, ("exclusive", 106, None)),
                (132, ("exclusive", 128, None)),
                (74, ("literal", True, 72)),
                (94, ("literal", True, 92)),
                (116, ("literal", True, 112)),
                (138, ("literal", False, 134)),
                (150, ("literal", True, 146)),
                (162, ("literal", True, 158)),
                (174, ("literal", True, 170)),
                (186, ("literal", True, 182)),
            ),
            True,
        )
        add(
            "run",
            "first_detached_same_node_before_BLX",
            ((196, 228),),
            196,
            228,
            ((214, ("exclusive", 208, 212)), (218, ("nonempty_detach",))),
            True,
        )
    elif mode == "sizes":
        add(
            "run",
            "sole_zero_slot_claim",
            ((0, 58),),
            0,
            58,
            (
                (24, ("source_false", "initial scan index0, one slot/stride/limit48", 22)),
                (40, ("source_false", "same complete Boot pool zero; no earlier claimant", 36)),
                (48, ("exclusive", 42, 46)),
            ),
            True,
        )
        add("run", "initialized_same_callback_and_future", ((58, 80),), 58, 80)
        add(
            "run",
            "sole_publication_retry",
            ((80, 122),),
            80,
            122,
            ((100, ("exclusive", 96, None)), (106, ("retry_if_failed",))),
            True,
        )
        add(
            "run",
            "first_detached_same_node_before_BLX",
            ((122, 148),),
            122,
            148,
            ((134, ("exclusive", 128, 132)), (136, ("nonempty_detach",))),
            True,
        )
    else:
        add(
            "run",
            "sole_zero_slot_claim",
            ((0, 48),),
            0,
            48,
            (
                (16, ("source_false", "initial scan index0, one slot/stride/limit48", 14)),
                (30, ("source_false", "same complete Boot pool zero; no earlier claimant", 26)),
                (38, ("exclusive", 32, 36)),
            ),
            True,
        )
        add("run", "initialized_same_callback_and_future", ((48, 64),), 48, 64)
        add(
            "queue_push",
            "sole_publication_retry",
            ((0, 46),),
            0,
            46,
            ((22, ("exclusive", 18, None)), (28, ("retry_if_failed",))),
            True,
        )
        add("raw_spawn", "publish_then_SEV_only_normal_return", ((0, 14),), 0, 14)
        add("run", "spawn_return_then_first_poll", ((64, 70),), 64, 70)
        add(
            "raw_poll",
            "first_detached_same_node_before_BLX",
            ((0, 32),),
            0,
            32,
            ((18, ("exclusive", 12, 16)), (22, ("nonempty_detach",))),
            True,
        )
    add(
        "boot_poll",
        "first_initialized_completion_zero",
        ((0, 22),),
        0,
        22,
        (
            (
                18,
                (
                    "source_false",
                    "same initialized completion0, no earlier poll/completion writer",
                    12,
                ),
            ),
        ),
    )
    z = mode == "sizez"
    end = 32 if z else 38
    add(
        "configuration_initialize",
        "same_B_first_taken_zero_then_successful_claim",
        ((0, end),),
        0,
        end,
        (
            (
                18 if z else 24,
                (
                    "source_false",
                    "same copied taken0, closed preceding writes/alias/control consumers",
                    14 if z else 20,
                ),
            ),
            (26 if z else 32, ("exclusive", 20 if z else 26, 24 if z else 30)),
        ),
        True,
    )
    return tuple(result)


def _joins(preclaim, bodies, families, edges):
    by_name = {v["family"]: v for v in families}
    mode = preclaim["mode"]
    joins = []

    def adjacent(left, right):
        a, b = by_name[left], by_name[right]
        require(
            a["phase"] == b["phase"] and a["exit"] == b["start"],
            "history causal families do not meet at the same continuation",
        )
        joins.append((left, a["exit"], right))

    adjacent("sole_zero_slot_claim", "initialized_same_callback_and_future")
    if mode != "sizez":
        pub = "sole_publication_all_four_attempts" if mode == "speed2" else "sole_publication_retry"
        adjacent("initialized_same_callback_and_future", pub)
        adjacent(pub, "first_detached_same_node_before_BLX")
    else:
        adjacent("initialized_same_callback_and_future", "spawn_return_then_first_poll")
        spawn = edges[("run", 64)]
        push = edges[("raw_spawn", 6)]
        poll = edges[("run", 70)]
        require(
            spawn["target"] == by_name["publish_then_SEV_only_normal_return"]["start"]
            and push["target"] == by_name["sole_publication_retry"]["start"]
            and poll["site"] == by_name["spawn_return_then_first_poll"]["exit"]
            and poll["target"] == by_name["first_detached_same_node_before_BLX"]["start"],
            "Z initializer/publication/poll children lose their same-invocation source order",
        )
        joins.extend(
            (
                ("run spawn", spawn["site"], "raw_spawn -> queue_push -> same selected return"),
                ("first raw_poll", poll["site"], "same sole published Boot node"),
            )
        )
    detach = by_name["first_detached_same_node_before_BLX"]
    prefix = by_name["first_initialized_completion_zero"]
    once = by_name["same_B_first_taken_zero_then_successful_claim"]
    initialize = edges[("boot_poll", 22)]
    require(
        detach["exit"] == preclaim["callback"][0]
        and prefix["start"] == preclaim["callback"][1]
        and prefix["exit"] == initialize["site"]
        and initialize["target"] == once["start"]
        and once["exit"] == preclaim["startup"]["entry"]["once"]["success"]["tail"],
        "history callback/receiver/guard/once/loader continuations do not compose",
    )
    joins.extend(
        (
            ("same-node BLX", detach["exit"], "initialized Boot prefix"),
            ("synchronous initialize", initialize["site"], "same B once claim"),
            ("restored once tail", once["exit"], "specialized loader effects pending"),
        )
    )
    return tuple(joins)


def _state_writers(preclaim, bodies):
    """All active Boot/P field writers, with the constructor scratch epoch."""
    boot = preclaim["task"][0]
    P = preclaim["executor"][0]
    records = []
    fields = (
        (24, 4, "spawned/queued state", ("run",)),
        (25, 1, "clear queued", ("run", "raw_poll")),
        (28, 4, "next", ("run", "queue_push", "raw_poll")),
        (32, 4, "executor", ("run", "raw_spawn")),
        (36, 4, "callback", ("run", "callback_install")),
        (40, 4, "future executor", ("boot_initializer",)),
        (44, 1, "completion zero", ("boot_initializer",)),
    )
    completion = []
    for value in preclaim["writes"]:
        if value["scope"] in ("after", "after_diagnostic", "diagnostic"):
            continue
        a, b = value["interval"]
        phase = value["phase"]
        site = value["instruction"]
        if a < boot + 48 and boot < b:
            matches = [
                name
                for off, width, name, phases in fields
                if (a, b) == (boot + off, boot + off + width) and phase in phases
            ]
            require(len(matches) == 1, "history has another Boot field writer or phase")
            records.append((site, phase, matches[0], a, b))
            if matches[0] == "completion zero":
                completion.append(site)
        if a < P + 8 and P < b:
            if phase == "cortex_main" and site == bodies[phase]["function"]:
                require(
                    preclaim["mode"] == "sizez" and (a, b) == (P, P + 16),
                    "history has an unreviewed preconstruction scratch write",
                )
                records.append((site, phase, "scratch PUSH before P construction", a, b))
                continue
            require(
                (a, b) in ((P, P + 4), (P + 4, P + 8))
                and (
                    phase == "cortex_main"
                    or (a, b) == (P, P + 4)
                    and phase in ("run", "queue_push", "raw_poll")
                ),
                "history changes inherited P outside its initialized queue-head family",
            )
            records.append(
                (site, phase, "initial P word" if phase == "cortex_main" else "queue head", a, b)
            )
    require(len(completion) == 1, "history has a missing/duplicate initialized completion writer")
    return tuple(records)


def first_boot_history(objects, instructions, source_context):
    """Close only the selected ordinary history; never issue a Config owner.

    Conditional exceptional/noninterference limits remain explicit. Unknown
    internal calls/PC effects are rejected, never hidden by those limits.
    """
    context = validate_source_context(source_context, objects.elf)
    source = inspect_source_context(context, objects.elf)
    p = preclaim_configuration_preservation(objects, instructions)
    bodies, edges = _edges(p)
    successors = _successors(p, instructions, bodies, edges)
    saves = _save_protection(p, bodies)
    returns = _returns(p, bodies, edges, saves)
    families = _families(p, instructions, bodies, successors, returns)
    joins = _joins(p, bodies, families, edges)
    writers = _state_writers(p, bodies)
    mode = p["mode"]
    startup = p["startup"]
    entry = startup["entry"]
    boot = bodies["boot_poll"]
    # These finite source-state equations consume full actual bodies/effects:
    # Reset zeros one private48-byte pool; P starts null; exclusive failure
    # writes nothing; the only published node is initialized before detach.
    B = entry["initial"]["address"]
    task = p["task"][0]
    P = p["executor"][0]
    require(
        p["task"] == (task, 48)
        and p["future"] == (task + 40, P, task + 44, 0)
        and p["Config_write_intersections"] == ((B + 14600, 1, "claim_transition"),)
        and p["claim"]["taken"] == B + 14600
        and p["claim"]["loader_r0"] == B + 14600
        and p["claim"]["required_before_taken"] == 0
        and p["claim"]["after_success_taken"] == 1
        and p["claim"]["failed_exclusive_write_bytes"] == 0,
        "history loses same-object initialization/claim equations",
    )
    constructor = tuple(v for v in writers if v[1:3] == ("cortex_main", "initial P word"))
    require(
        {v[3:5] for v in constructor} == {(P, P + 4), (P + 4, P + 8)}
        and len(constructor) == 2
        and all(
            v[0] < edges[("cortex_main", 16 if mode == "sizez" else 18)]["site"]
            for v in constructor
        ),
        "history P construction is incomplete or follows its run invocation",
    )
    require(
        len(p["publication_attempts"]) == (4 if mode == "speed2" else 1)
        and p["callback"] == (p["callback"][0], boot["function"], task)
        and p["callback_expiry"] == "before next shared dequeue",
        "history loses sole publication or callback expiry",
    )
    # Complete phase bodies and source main close the forward invocation.
    # This is not a global absence-of-address-taken or initialize-name claim.
    contradictions = {
        "Busy": (
            "complete same Boot pool zeroed at Reset",
            "one candidate, no prior selected claimant",
            "failed exclusive stores write no bytes; only successful zero claim advances",
        ),
        "resumed": (
            "same callback/future initialized before sole publication",
            "completion0 preserved through first detach",
            "same-node callback receiver; completion1 and retirement only after initialize returns",
        ),
        "already_taken": (
            "same copied B has taken0",
            "closed ordinary prefix has no earlier take/alias/writer",
            "only successful STREXB changes taken; failed exclusive retries without a write",
        ),
    }
    diagnostics = tuple(
        dict(
            kind=v["kind"],
            edges=v["edges"],
            status="source_infeasible_in_selected_ordinary_history",
            contradiction=contradictions[v["kind"]],
            not_globally_unreachable=True,
            exceptional_or_other_invocation_effects_remain_unresolved=True,
        )
        for v in p["pending_diagnostics"]
    )
    outside = tuple(
        dict(
            site=at,
            operation=op,
            target_phase=phase,
            target=target,
            status="outside_closed_selected_prefix",
            caller_context=tuple(
                (name, value & ~1, extent)
                for name, (value, extent, index) in objects.elf.functions.items()
                if value & ~1 <= at < (value & ~1) + extent
            ),
            caller_effects_remain_unreviewed=True,
            reason="no classified selected successor/callback/return reaches this site; exceptions separately unresolved",
            grants_target_or_caller_permission=False,
        )
        for at, op, phase, target in p["pending_other_shared_invocations"]
    )
    require(
        all(
            not any(
                body["function"] <= v["site"] < body["function"] + body["extent"]
                for body in bodies.values()
            )
            for v in outside
        ),
        "outside shared invocation is actually inside the selected complete bodies",
    )
    return dict(
        phase="first_boot_history_pending_specialized_loader_effects",
        mode=mode,
        scope="selected ordinary Reset invocation through first successful once claim",
        source_context=context.binding(),
        source_semantics=source,
        preclaim=p,
        successors=successors,
        families=families,
        temporal_joins=joins,
        actual_state_writers=writers,
        executor_constructor=constructor,
        selected_returns=returns,
        protected_save_intervals=saves,
        publication_attempts=p["publication_attempts"],
        state_induction=(
            "actual zero/copy exit",
            "private sole-slot zero claim",
            "same callback/future completion0",
            "sole publication, retry/no-write alternatives",
            "first detach with saved-null next",
            "same-node completion guard then synchronous initialize",
            "taken0 exclusive success then restored loader boundary",
        ),
        diagnostics=diagnostics,
        outside_shared_invocations=outside,
        boot_initialize_call=entry["boot_call"],
        callback_binding=dict(
            site=p["callback"][0],
            target=p["callback"][1],
            receiver=task,
            scope="same first detached Boot node only",
            global_whitelist=False,
        ),
        once_return_transport=dict(
            continuation=entry["boot_call"] + 4,
            saved_LR=(
                saves["configuration_initialize"][1] - 4,
                saves["configuration_initialize"][1],
            ),
            LR_value=(entry["boot_call"] + 4) | 1,
            restored_SP=saves["configuration_initialize"][1],
            restore=entry["once"]["success"]["restore"],
            restored_before_specialized_tail=True,
        ),
        nonreturn_prefix=dict(
            main_cortex_and_run_have_no_return_before_claim=True,
            Reset_UDF_not_reached_by_selected_prefix=True,
        ),
        callback_expiry=p["callback_expiry"],
        later_invocations_require_separate_context=True,
        initial_state_expires_at_decode_refill_or_publication=True,
        requires_complete_same_B_loader_effects=True,
        requires_H8_consumed_input_closure=True,
        external_qualifications=(
            _EXTERNAL
            if p["requires_M2_pre_init_source_reconciliation"]
            else _EXTERNAL[:2]
            + ("exact reviewed assembly pre_init; no Rust executes before RAM initialization",)
        ),
        exception_graph_limit=("NMI", "HardFault", "reset_reentry"),
        exception_absence_established=False,
        source_qualification=p["source_qualification"],
        requires_M2_pre_init_source_reconciliation=p["requires_M2_pre_init_source_reconciliation"],
        hardware_qualified=False,
        existing_conservative_frame_arithmetic_unchanged=True,
    )
