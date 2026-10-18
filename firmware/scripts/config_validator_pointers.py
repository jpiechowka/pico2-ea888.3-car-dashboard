"""Grouped current validator pointer regions and owned-effect interfaces.

Closed source-phase grammars preserve the reviewed physical aliases. They do
not establish the caller's private typed Config, source selectors/loop states,
child ingress outside these regions, or the final ordinary/diagnostic graph.
No JSON input can supply these grammars or turn their obligations into a pass.
"""

from dataclasses import replace
import re
import struct
from config_finite import _literal_body
from config_regions import Storage, borrow, require_disjoint, require_writes
from config_stack import canonical_symbol, typed_function, _typed_function_extent, transfer_family
from config_validator_pointer_contracts import REGIONS, CHILDREN, STORES, SOURCE_INGRESS
from config_validator_writes import validator_local_writes
from llvm_stack import mapping_ranges
from validation import require


_GEOMETRY = {
    "result": None,
    "page_array": (120, 123),
    "edges": (104, 107),
    "occupied": (104, 112),
    **{
        k: (76, 88)
        for k in ("widget_r0", "band_r3", "root_warning", "channel_r2", "widget_r5", "fuel_r0")
    },
}
_PHASES = {
    "result": "paired Err-copy or Ok-f32 then leading tag",
    "page_array": "three closure passes after binding staging",
    "edges": "three source pages and destination edges before retirement",
    "occupied": "post-retirement initialized widget/view geometry",
    "widget_r0": "current widget first error",
    "band_r3": "current widget band first error",
    "root_warning": "root warning first error with widget None",
    "channel_r2": "current initialized Channel first error",
    "widget_r5": "current widget warning first error, not caller Result",
    "fuel_r0": "captured first through fuel alternatives before late reset",
}


def _no_object(objects, value, descriptor):
    raise AssertionError("unexpected validator leaf object descriptor")


def _leaf(objects, name, rows, label):
    require(label in CHILDREN, "unreviewed validator leaf role")
    item = CHILDREN[label]
    role = item["role"]
    contract = item["contract"]
    start = objects.elf.functions[name][0] & ~1
    if role.startswith("@outline:"):
        require(
            re.fullmatch(r"OUTLINED_FUNCTION_[0-9]+", name),
            "validator outline borrows an ordinary identity",
        )
        _typed_function_extent(objects, start, canonical_symbol(name))
        require(objects.sizes.get(start, 0) == 0, "validator outline acquires a resident frame")
    else:
        typed_function(objects, start, role)

    def function(objects, target, raw_role):
        if target == start and role.startswith("@outline:"):
            extent, actual = _typed_function_extent(objects, target, raw_role)
            require(
                extent == contract["extent"] and objects.sizes.get(target, 0) == 0,
                "validator outline extent/frame differs",
            )
            return extent, 0, actual
        return typed_function(objects, target, raw_role)

    def unexpected(*args):
        raise AssertionError("unexpected nested validator leaf call")

    proof = _literal_body(
        objects,
        name,
        rows,
        role,
        label,
        contract,
        _no_object,
        unexpected,
        function_consumer=function,
    )
    require(
        not proof["ordinary_edges"] and not proof["indirect_sites"],
        "validator leaf gains another activation or callback",
    )
    return proof


def _source_contract(objects, name, rows, parent, config, item):
    """Bind a fixed source window's parent transfers and typed literal slots."""
    start = parent + item["offset"]
    operations = []
    caller_mapping = mapping_ranges(objects.elf, name)
    pools = []
    for offset, width, text in item["instructions"]:
        text = re.sub(r"@parent\+([0-9a-f]+)", lambda m: hex(parent + int(m[1], 16)), text)
        child = re.search(r"@child:([a-z_]+)", text)
        if child:
            label = child[1]
            role = CHILDREN[label]["role"]
            text = text.replace(child[0], role if role.startswith("@outline:") else "@role:" + role)
        pool = re.search(r"@pool([0-9]+)", text)
        if pool:
            at = start + offset
            row = rows.get(at)
            require(row is not None, "missing validator readonly producer")
            operation, _, operand = text.partition(" ")
            register = operand.partition(",")[0]
            actual = row.operands.split("@", 1)[0].split("<", 1)[0].strip()
            match = re.fullmatch(re.escape(register) + r", \[pc, #(0x[0-9a-f]+|[0-9]+)\]", actual)
            require(
                match is not None
                and row.mnemonic == operation
                and operation in ("ldr", "ldr.w", "vldr"),
                "validator typed scalar/Config pool producer differs",
            )
            slot = ((at + 4) & ~3) + int(match[1], 0)
            require(
                slot % 4 == 0
                and any(k == "d" and lo <= slot < slot + 4 <= hi for lo, hi, k in caller_mapping),
                "validator literal leaves mapped readonly parent data",
            )
            value = struct.unpack("<I", objects.read_only(slot, 4))[0]
            descriptor = item["pools"][int(pool[1])]
            expected = config.start if descriptor["kind"] == "config" else descriptor["value"]
            require(value == expected, "validator literal source role or scalar bits differ")
            text = operation + " " + actual
            pools.append((at, slot, value, descriptor["kind"]))
        operations.append((offset, width, text))
    contract = dict(extent=item["extent"], instructions=operations, objects=())
    for key in ("mapping", "data"):
        if key in item:
            contract[key] = item[key]
    return start, contract, tuple(pools)


def _region(objects, name, rows, parent, config, item, children):
    start, contract, pools = _source_contract(objects, name, rows, parent, config, item)

    def outline(objects, child, records, role, ancestry):
        require(
            role.startswith("@outline:") and role[9:] in CHILDREN,
            "unreviewed validator outline interface",
        )
        label = role[9:]
        proof = _leaf(objects, child, records, label)
        children[label] = proof
        return proof

    proof = _literal_body(
        objects,
        name,
        rows,
        canonical_symbol(name),
        item["label"],
        contract,
        _no_object,
        outline,
        window=start,
    )
    for offset, operation, target, role, frame in proof["ordinary_edges"]:
        if role.startswith("@outline:"):
            continue
        labels = [k for k, v in CHILDREN.items() if v["role"] == role]
        require(len(labels) == 1, "validator region gains an unsupported ordinary leaf")
        child = typed_function(objects, target, role)[2]
        children[labels[0]] = _leaf(objects, child, rows, labels[0])
    proof["pools"] = tuple(pools)
    return proof


def validator_pointer_regions(objects, name, instructions, context, call):
    """Current region/effect proof conditional on actual source-owner joins.

    Incoming source scalar states remain unknown unions. Exact phase sequences
    establish local alias production, source literal and child effects; all source
    validity, contextual ingress, loop and cross-region preservation requirements
    must still be consumed by the enclosing producer before stack admission.
    """
    local = validator_local_writes(objects, name, instructions, context, call)
    parent = objects.elf.functions[name][0] & ~1
    q = context.frame
    rows = {
        a: replace(r, operands=r.operands.split("@", 1)[0].split("<", 1)[0].strip())
        for a, r in instructions.items()
    }
    children = {}
    proofs = {
        x["label"]: _region(objects, name, rows, parent, context.config, x, children)
        for x in REGIONS
    }
    require(set(children) == set(CHILDREN), "validator regions omit a required complete leaf")
    intervals = tuple((parent + x["offset"], parent + x["offset"] + x["extent"]) for x in REGIONS)
    for at, row in rows.items():
        family = transfer_family(row.mnemonic.removesuffix(".w"))
        if family not in ("b", "bl", "cbz", "cbnz"):
            continue
        target = re.search(r"(?<![#\w])0x([0-9a-f]+)(?:\s|$)", row.operands)
        if target is None:
            continue
        destination = int(target[1], 16)
        if not any(lo <= destination < hi for lo, hi in intervals) or any(
            lo <= at < hi for lo, hi in intervals
        ):
            continue
        require(
            (at - parent, row.mnemonic, destination - parent) in SOURCE_INGRESS,
            "foreign direct ingress into validator pointer source phase",
        )
    guards = context.protected + tuple(
        Storage(q.space, q.identity + ":" + label, q.start + offset, extent, 1, q.phase, False)
        for label, offset, extent in (
            ("exclusive Result spill", 60, 4),
            ("exclusive first capture", 88, 4),
            ("valid byte", 95, 1),
        )
    )
    families = {}
    writes = []
    for family, offsets in _GEOMETRY.items():
        view = (
            context.result
            if offsets is None
            else borrow(q, offsets[0], offsets[1] - offsets[0], q.phase, writable=True)
        )
        families[family] = dict(view=view, owner_offsets=offsets, source_phase=_PHASES[family])
    for offset, family, width, text, footprint in STORES:
        at = parent + offset
        row = rows.get(at)
        operation, _, operand = text.partition(" ")
        require(
            row is not None
            and (len(row.raw), row.mnemonic, row.operands) == (width, operation, operand),
            "validator pointer store/source family differs",
        )
        view = families[family]["view"]
        lo, hi = footprint.get("owner_offsets", (0, view.extent))
        protected = guards
        if family == "result":
            protected += (
                Storage(
                    q.space, q.identity + ":complete live locals", q.start, 216, 8, q.phase, False
                ),
            )
        require_writes((view,), ((view, lo, hi - lo),), protected)
        writes.append((at, family, view, lo, hi))
    expected = {parent + offset for offset, *rest in STORES}
    actual = {row.address for row in local["unresolved_pointer_writes"] if row.mnemonic != "vstr"}
    require(expected == actual, "validator pointer effect inventory loses a current store")
    first = borrow(q, 76, 12, q.phase, writable=True)
    require_writes((first,), ((first, 10, 1),), guards)
    rect = borrow(q, 112, 10, q.phase, writable=True)
    require_writes((rect,), ((rect, 0, 10),), guards)
    for label, proof in children.items():
        if not proof["frame"]:
            continue  # Complete zero-growth outline grammar.
        child = Storage(
            q.space,
            q.identity + ":" + label,
            q.start - proof["frame"],
            proof["frame"],
            8,
            q.phase,
            True,
        )
        for live in (q, context.result.storage, context.config.storage) + context.protected:
            require_disjoint((child, live))
        proof.update(owned_frame=child, parent_SP_required=True, save_bytes=proof["frame"])
    children["first_capture"].update(
        clobbers=("r0", "r1", "lr", "flags"),
        first_alias="r0=[Q88]",
        writes=((first, 10, 1),),
        parent_SP_required=True,
    )
    children["band_capture"].update(
        clobbers=("r0", "r1", "r2", "r3", "r4", "lr", "flags"),
        first_alias="r3=[Q88]",
        writes=((first, 10, 1),),
        requires_actual_initialized_Widget_at_Q180=True,
        parent_SP_required=True,
    )
    children["placement_rect"].update(
        clobbers=("r1", "r2", "r3", "lr", "flags"),
        writes=((rect, 0, 10),),
        alternatives=(
            dict(tag=0, writes=((rect, 0, 2),), inactive=(2, 10)),
            dict(tag=1, writes=((rect, 0, 10),)),
        ),
        requires_actual_packed_Placement_argument_and_output_transport=True,
    )
    children["action_destination"].update(
        clobbers=("r0", "r1", "r2", "lr", "flags"),
        writes=(),
        requires_paired_ControlPage_domain_0_to_2=True,
        guarded_table_indices=(0, 1, 2, 3, 4),
        padding_index_excluded=5,
    )
    children["fuel_compare_bits"].update(
        clobbers=("r1", "lr", "flags"), writes=(), preserves_captured_r0=True
    )
    children["fuel_compare_flags"].update(
        clobbers=("lr", "flags"), writes=(), preserves_captured_r0=True
    )
    return dict(
        local=local,
        regions=proofs,
        children=children,
        families=families,
        pointer_writes=tuple(writes),
        requires_actual_source_typed_parent_entry_and_full_graph=True,
        requires_edges_to_occupied_retirement_and_R8_preservation_across_uncovered_children=True,
        requires_current_widget_Channel_band_and_fuel_source_loop_selector_states=True,
        requires_each_source_ingress_argument_and_first_typed_variant_pair=True,
        requires_all_remaining_helper_and_computed_ingress_closure=True,
        platform_requirements_unresolved=True,
    )
