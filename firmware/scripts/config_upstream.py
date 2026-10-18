"""Closed upstream source-state regions; no whole-parent permission.

Issuer retry recognition retains all weak-CAS alternatives and separately
matches its terminal typed panic suffix. Root ingress, caller phase, memory
ownership and exhaustive parent coverage remain explicit graph obligations.
"""

import re
from config_entry import entry_object
from config_finite import _literal_body
from config_issuer_contracts import ISSUER_GRAMMARS
from config_roots import root_window
from config_stack import canonical_symbol, typed_function, transfer_family
from validation import require


def _no_issuer_outline(*args):
    require(False, "issuer contains an unsupported outline call")


def issuer_region(objects, name, instructions):
    """A finite AtomicU32 region with a demanded root-suffix continuation.

    Structural anchors are discovered uniquely in the current typed parent;
    no saved absolute PC or caller offset is an acceptance key. The root
    suffix is independently checked before binding its incoming zero branch.
    This private region consumer reuses literal comparison, not the terminal
    window wrapper, and never certifies uncovered parent instructions.
    """
    role = canonical_symbol(name)
    variants = [(key, c) for key, c in ISSUER_GRAMMARS.items() if c["caller"] == role]
    require(name in objects.elf.functions and variants, "unreviewed issuer parent")
    parent = objects.elf.functions[name][0] & ~1
    extent, frame, _ = typed_function(objects, parent, role)
    rows = {a: r for a, r in instructions.items() if parent <= a < parent + extent}
    expect_targets = {
        e[0] & ~1
        for n, e in objects.elf.functions.items()
        if canonical_symbol(n) == "_RNvNtCs_4core6option13expect_failed"
    }
    roots = []
    for at, row in sorted(rows.items()):
        if row.mnemonic not in ("bl", "bl.w"):
            continue
        target = re.match(r"0x([0-9a-f]+)(?:\s|$)", row.operands)
        if target is None or int(target[1], 16) not in expect_targets:
            continue
        try:
            proof = root_window(objects, name, instructions, at)
        except AssertionError:
            continue
        if proof["location_role"] == "identity_first":
            roots.append(proof)
    require(len(roots) == 1, "issuer failure suffix is missing or ambiguous")
    failure = roots[0]
    results = []
    for variant, original in variants:
        if original["frame"] != frame:
            continue
        first = original["instructions"][0][2].split(" ", 1)[0]
        for start, row in sorted(rows.items()):
            if (
                row.mnemonic != first
                or start + original["extent"] > parent + extent
                or failure["window"] < start
            ):
                continue
            contract = dict(original)
            contract["instructions"] = [
                (
                    offset,
                    width,
                    operation.replace(
                        "@identity_failure", "@self+" + format(failure["window"] - start, "x")
                    ),
                )
                for offset, width, operation in original["instructions"]
            ]
            try:
                proof = _literal_body(
                    objects,
                    name,
                    rows,
                    role,
                    variant,
                    contract,
                    entry_object,
                    _no_issuer_outline,
                    window=start,
                )
            except AssertionError:
                continue
            end = start + original["extent"]
            edges = []
            for at, current in sorted(instructions.items()):
                family = transfer_family(current.mnemonic.removesuffix(".w"))
                target = re.search(r"(?<![#\w])0x([0-9a-f]+)(?:\s|$)", current.operands)
                if not family or not target:
                    continue
                destination = int(target[1], 16)
                require(
                    not (start < destination < end and not start <= at < end),
                    "foreign direct ingress into issuer interior",
                )
                if start <= at < end:
                    require(
                        destination in rows
                        and (start <= destination <= end or destination == failure["window"]),
                        "unbound issuer loop/continuation/root edge",
                    )
                    edges.append((at - start, current.mnemonic, destination))
            tag = "r0" if variant == "speed2" else "r1"
            previous = original["previous_register"]
            issuer = proof["objects"][0]
            proof.update(
                window=start,
                caller=parent,
                caller_extent=extent,
                attempts=original["attempts"],
                previous_register=original["previous_register"],
                failure_suffix=failure,
                branch_edges=tuple(edges),
                continuation=end,
                state_alternatives=(
                    dict(
                        phase="issued_previous",
                        tag_register=tag,
                        tag_value=0,
                        value_register=previous,
                    ),
                    dict(
                        phase="exhausted",
                        tag_register=tag,
                        tag_value=2,
                        value_register=previous,
                        value=0,
                    ),
                ),
                writes=(
                    dict(
                        owner_role=original["objects"][0]["role"],
                        address=issuer,
                        offset=0,
                        extent=4,
                        operation="exclusive-store checked increment",
                    ),
                ),
                local_write_ranges=(),
                helper_calls=(),
                failure_edge=dict(
                    kind="root_suffix",
                    parent=parent,
                    source="issuer_previous_zero",
                    destination=failure["window"],
                    site=failure["site"],
                    root_role="identity_first",
                ),
                requires_parent_phase_all_ingress_and_coverage=True,
            )
            results.append(proof)
    require(len(results) == 1, "missing, changed or ambiguous issuer retry region")
    return results[0]
