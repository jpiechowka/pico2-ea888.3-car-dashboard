"""Shared closed child save/return equations for matched parser families.

Only fixed authored stack profiles supplied by the internal consumers are
used. No scalar opcode is executed, no unknown PC effect is admitted, and
every complete body must already have its current literal contract checked.
This does not prove caller source effects, observer compatibility or ownership.
High GPRs carry only incoming-value lineage or an unknown definition. They
are never evaluated as scalar values. Architectural LR is tracked separately
from the original saved return PC.
Code-address tokens are normalized even instruction identities, not scalar LR
bits. A real Thumb BL sets bit0 in LR; this model neither computes that bit
nor proves architectural register equality or exceptional return behavior.
"""

import re
from config_regions import _control
from config_owner_entry import _direct_target
from validation import require


_HIGH = tuple("r" + str(i) for i in range(4, 12))


def _high_definitions(row, control):
    """Finite integer destination footprints, without evaluating scalar bits."""
    if control is not None:
        return ()
    op = row.mnemonic.removesuffix(".w")
    args = row.operands
    if op in ("cmp", "cmn", "tst", "teq", "nop", "it", "itt", "ite", "ittt", "ittee"):
        return ()
    writes = set()
    if op.startswith(("str", "stm", "vstr", "vstm")):
        # Stores only define an integer base when the exact form writes back.
        if "!" in args or "], #" in args:
            match = re.search(
                r"\[?(r(?:[0-9]|1[01]))(?:,|!|\])", args.split(",", 1)[1] if "[" in args else args
            )
            if match:
                writes.add(match[1])
    elif op.startswith("ldm"):
        writes.update(re.findall(r"\br(?:[0-9]|1[01])\b", args.split("{", 1)[1]))
        if "!" in args:
            writes.add(args.split("!", 1)[0])
    else:
        fields = args.split(", ")
        count = 2 if op.startswith(("ldrd", "ldrexd", "umull", "smull", "umlal", "smlal")) else 1
        if (
            op == "vmov"
            and len(fields) > 2
            and fields[0].startswith("r")
            and fields[1].startswith("r")
        ):
            count = 2
        writes.update(fields[:count])
    if "[" in args and ("!" in args or "], #" in args):
        base = re.search(r"\[(r(?:[0-9]|1[01]))(?:,|\])", args)
        if base:
            writes.add(base[1])
    return tuple(reg for reg in _HIGH if reg in writes)


def _body_selector(rows, objects=None):
    """Fresh body dictionaries; only detached input permits address lookup.

    The invocation-local immutable address index validates actual keys once;
    direct _FrozenObjects construction alone does not grant that invariant.
    Unsupported/malformed providers retain the eager items comprehension.
    This selects records only; it caches no body or evaluated return state.
    """
    from config_structural import _FrozenObjects, _instruction_addresses, _instruction_items

    if type(objects) is _FrozenObjects:
        require(rows is objects._rows, "finite child return input package differs")
    indexed = _instruction_addresses(objects, rows) is not None

    def select(start, extent):
        if indexed:
            return dict(_instruction_items(objects, rows, start, extent))
        return {at: row for at, row in rows.items() if start <= at < start + extent}

    return select


def _closed_child_returns(rows, parents, children, sp, stacks, scratch_restores, *, objects=None):
    select_body = _body_selector(rows, objects)
    completed = {}
    nodes = []
    active = []
    activation = [0]

    def walk(proof, entry, token, expected_sp, cells=(), lr=None, shuttle=None, high=_HIGH):
        start = proof["function"]
        family = proof.get("semantic_role", proof["role"])
        require(family in stacks, "finite child has no authored child return family")
        key = (start, proof["role"], family, entry, token, expected_sp, cells, lr, shuttle, high)
        if key in completed:
            return completed[key]
        require(start not in active, "finite child child introduces an activation cycle")
        active.append(start)
        activation[0] += 1
        # A local incoming token is fresh even when its caller value is unknown.
        # Repeated definitions of an unknown value cannot prove preservation.
        incoming = tuple(("incoming", activation[0], reg) for reg in _HIGH)
        resolve = dict(zip(incoming, high))
        body = select_body(start, proof["extent"])
        edges = {start + e[0]: e for e in proof["ordinary_edges"]}
        require(not proof["indirect_sites"], "finite child child adds an executable callback")
        pending = [(start, entry, token if lr is None else lr, cells, shuttle, incoming)]
        seen = set()
        returns = []
        carrier_returns = []
        link_returns = []
        minimum = entry
        while pending:
            at, sp, link, saved, reg2, registers = pending.pop()
            state = (at, sp, link, saved, reg2, registers)
            if state in seen:
                continue
            seen.add(state)
            require(at in body, "finite child child successor enters data or outside its body")
            row = body[at]
            op = row.mnemonic.removesuffix(".w")
            following = at + len(row.raw)
            control, conditional = _control(row)
            memory = dict(saved)
            new_sp = sp
            new_link = link
            return_pc = None
            new_high = list(registers)
            stack_step = (
                op.startswith(("push", "pop"))
                or row.operands.startswith("sp,")
                or bool(re.fullmatch(r"r(?:8|11), \[sp(?:, #-4\]!|\], #4)", row.operands))
            )
            require(
                not stack_step or at - start in stacks[family],
                "finite child child changes an unreviewed SP/save instruction",
            )
            if at - start in stacks[family]:
                if op.startswith(("push", "pop")):
                    regs = row.operands[1:-1].split(", ")
                    if op == "push":
                        new_sp = sp - 4 * len(regs)
                        for i, reg in enumerate(regs):
                            cell = new_sp + 4 * i
                            require(
                                cell not in memory,
                                "finite child nested save overwrites a live caller slot",
                            )
                            memory[cell] = (
                                link
                                if reg == "lr"
                                else (reg, new_high[_HIGH.index(reg)])
                                if reg in _HIGH
                                else reg
                            )
                    else:
                        for i, reg in enumerate(regs):
                            cell = sp + 4 * i
                            require(
                                cell in memory, "finite child restore consumes an unowned save slot"
                            )
                            value = memory.pop(cell)
                            if reg == "pc":
                                return_pc = value
                            elif reg == "lr":
                                new_link = value
                            elif reg in ("r2", "r3"):
                                require(
                                    family in scratch_restores,
                                    "finite child restores scratch into another register family",
                                )
                            elif reg in ("r0", "r1"):
                                # The authored LedPattern H48 prologue uses
                                # its first sixteen bytes as Q/Result scratch.
                                # Its exact POP discards those four cells;
                                # r8/r9/r11 and the outer low saves still need
                                # their original lineage equations below.
                                from config_tail_contracts import ROLES as TAIL

                                require(
                                    family == TAIL["LedPattern"]
                                    and proof["shape"] == "LedPattern"
                                    and family in scratch_restores
                                    and at == start + 170
                                    and row.operands == "{r0, r1, r2, r3, r8, r9, r11}",
                                    "finite child restores scratch into another low-register family",
                                )
                            elif reg in _HIGH:
                                require(
                                    type(value) is tuple and len(value) == 2 and value[0] == reg,
                                    "finite child restore consumes another caller register",
                                )
                                new_high[_HIGH.index(reg)] = value[1]
                            else:
                                require(
                                    value == reg,
                                    "finite child restore consumes another caller register",
                                )
                        new_sp = sp + 4 * len(regs)
                elif op in ("add", "sub"):
                    count = int(row.operands.split("#")[1], 0)
                    new_sp = sp + (count if op == "add" else -count)
                elif op == "str":
                    new_sp = sp - 4
                    require(new_sp not in memory, "finite child high save overwrites its caller")
                    reg = row.operands.split(",")[0]
                    memory[new_sp] = (reg, new_high[_HIGH.index(reg)])
                else:
                    reg = row.operands.split(",")[0]
                    value = memory.get(sp)
                    require(
                        op == "ldr"
                        and type(value) is tuple
                        and len(value) == 2
                        and value[0] == reg,
                        "finite child postindexed restore loses its high register",
                    )
                    new_high[_HIGH.index(reg)] = value[1]
                    del memory[sp]
                    new_sp = sp + 4
            if not stack_step:
                for reg in _high_definitions(row, control):
                    source = row.operands.split(", ")[1] if op == "mov" else None
                    new_high[_HIGH.index(reg)] = (
                        registers[_HIGH.index(source)] if source in _HIGH else None
                    )
            if family == "slice_pop" and at - start == 30:
                reg2 = link
            elif family == "slice_pop" and at - start == 36:
                new_link = reg2
            elif row.operands.startswith("lr,") and not stack_step:
                new_link = ("scratch", at)
            minimum = min(minimum, new_sp)
            updated = tuple(sorted(memory.items()))
            if at in edges:
                off, operation, target, role, frame = edges[at]
                require(
                    control in ("bl", "b") and not conditional,
                    "finite child child edge changes call/tail family",
                )
                child = children[target, role]
                child_token = following if control == "bl" else token
                expected = new_sp if control == "bl" else expected_sp
                result = walk(
                    child,
                    new_sp,
                    child_token,
                    expected,
                    updated,
                    child_token if control == "bl" else new_link,
                    reg2,
                    tuple(new_high),
                )
                minimum = min(minimum, result["lowest_SP"])
                if control == "b":
                    returns.extend(result["returns"])
                    for values in result["high_gpr_return_values"]:
                        carrier_returns.append(values)
                    link_returns.extend(result["architectural_LR_return_values"])
                    continue
                require(
                    result["restored_cells"] == updated,
                    "finite child child changes live ancestor save cells",
                )
                new_link = result["architectural_LR_after"]
                new_high = list(result["returned_high_gprs"])
            elif return_pc is not None or control == "bx":
                require(
                    control == "return" if return_pc is not None else row.operands == "lr",
                    "finite child child has an unreviewed return consumer",
                )
                pc = return_pc if return_pc is not None else new_link
                require(
                    pc == token and new_sp == expected_sp,
                    "finite child child loses its saved return PC or caller SP",
                )
                returns.append((at, pc, new_sp))
                carrier_returns.append(tuple(new_high))
                link_returns.append(new_link)
                if conditional:
                    require(following in body, "conditional child restore falls outside its body")
                    pending.append((following, sp, link, saved, reg2, registers))
                continue
            elif control in ("b", "cbz", "cbnz"):
                target = _direct_target(row, control)
                require(target in body, "finite child child local branch escapes its matched body")
                pending.append((target, new_sp, new_link, updated, reg2, tuple(new_high)))
                if not conditional:
                    continue
            elif control == "tbb":
                # Closed authored aggregate families. This consumes their
                # already current complete literal/table proof, never an
                # arbitrary caller target list. Source argument/arm effects
                # remain separate from these saved-PC equations.
                from config_source_children import CHANNEL_SOURCE, ACCESS_TARGET_OFFSETS

                if family == CHANNEL_SOURCE:
                    targets = tuple(start + off for off in ACCESS_TARGET_OFFSETS)
                    require(
                        proof["shape"] == "ChannelSource"
                        and at == start + 74
                        and row.operands == "[pc, r6]"
                        and not conditional
                        and proof.get("access_targets") == targets
                        and proof.get("guarded_access_index_domain") == (0, 5)
                        and all(t in body for t in targets),
                        "finite child loses the guarded Source Access table relation",
                    )
                else:
                    from config_tail_children import ACTION_TARGET_OFFSETS
                    from config_tail_contracts import ROLES as TAIL

                    targets = tuple(start + off for off in ACTION_TARGET_OFFSETS)
                    require(
                        family == TAIL["ControlAction"]
                        and proof["shape"] == "ControlAction"
                        and at == start + 48
                        and row.operands == "[pc, r0]"
                        and not conditional
                        and proof.get("action_targets") == targets
                        and proof.get("guarded_action_index_domain") == (0, 18)
                        and all(t in body for t in targets),
                        "finite child loses the guarded ControlAction table relation",
                    )
                pending.extend(
                    (t, new_sp, new_link, updated, reg2, tuple(new_high)) for t in targets
                )
                continue
            else:
                require(control is None, "finite child child contains an unclassified PC effect")
            require(following in body, "finite child child falls outside its complete body")
            pending.append((following, new_sp, new_link, updated, reg2, tuple(new_high)))
        require(returns, "finite child child has no completed saved-PC return")
        # A tail restore can consume its parent's cells. Its final map is the
        # caller's enclosing map, not necessarily the cells present at entry.
        final_cells = tuple((a, v) for a, v in cells if a >= expected_sp)
        require(
            carrier_returns and link_returns, "finite child loses its register return alternatives"
        )
        preserved = tuple(
            reg for i, reg in enumerate(_HIGH) if all(v[i] == incoming[i] for v in carrier_returns)
        )
        values = tuple(tuple(resolve.get(v, v) for v in registers) for registers in carrier_returns)
        returned = tuple(
            values[0][i] if all(v[i] == values[0][i] for v in values) else None
            for i in range(len(_HIGH))
        )
        final_link = (
            link_returns[0]
            if all(v == link_returns[0] for v in link_returns)
            else ("unknown_LR", start, token)
        )
        result = dict(
            function=start,
            role=proof["role"],
            entry_SP=entry,
            return_PC=token,
            return_SP=expected_sp,
            lowest_SP=minimum,
            returns=tuple(sorted(set(returns))),
            restored_cells=final_cells,
            architectural_LR_preserved=False,
            architectural_LR_after=final_link,
            architectural_LR_return_values=tuple(link_returns),
            preserved_high_gprs=preserved,
            returned_high_gprs=returned,
            high_gpr_return_values=values,
        )
        nodes.append(result)
        completed[key] = result
        active.pop()
        return result

    for parent in parents:
        for off, op, target, role, frame in parent["ordinary_edges"]:
            require(op == "bl", "Views parent changes its call phase")
            walk(children[target, role], sp, parent["function"] + off + 4, sp)
    return tuple(nodes)
