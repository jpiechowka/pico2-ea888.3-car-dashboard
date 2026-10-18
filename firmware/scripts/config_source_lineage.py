"""Conditional Source publication lineage on the authored normal parent path.

This checks the package/argument/epoch relation behind the physical envelopes.
The actual private producer must additionally bind its entry, active source
guards and reached accesses. No supplied record creates an ordinary owner.
"""

from config_abstract import Span
from config_regions import Storage, borrow, require_disjoint, require_writes
from config_decode_effects import _object
from config_source_effects import SOURCE_EPOCHS
from config_source_parent_effects import PARENT_WRITES, POINTER_EPOCHS
from validation import require


# Fixed selected normal segments. The inherited entry and all child bodies
# have their own complete checked control relations. These segments retain
# the real noncontiguous parent order, rather than sorting instruction PCs.
_SEGMENTS = (
    (4376, 4394),
    (4892, 4922),
    (4930, 4960),
    (4968, 4998),
    (5140, 5170),
    (5240, 5270),
    (5278, 5308),
    (5316, 5348),
    (5356, 5518),
    (5762, 5856),
    (4410, 4534),
    (4534, 4796),
)
_SOURCE_BRANCHES = (
    (4390, 4892, 4896),
    (4920, 4930, 4934),
    (4958, 4968, 4972),
    (4996, 5140, 5144),
    (5168, 5240, 5244),
    (5268, 5278, 5282),
    (5306, 5316, 5320),
    (5346, 5356, 5360),
)
_CAPTURE_ORDER = (7, 0, 1, 2, 3, 4, 5, 6)
_TAG_STORES = (4670, 4682, 4700, 4714, 4734, 4748, 4768, 4782)


def _publication_lineage(
    objects, rows, parents, array, effects, frame, carriers, returns, envelopes
):
    """Check one supplied conditional array; actual entry authority is separate.

    No data from a support file is loaded here. The closed current literals,
    actual child returns, eight source packages and exact effects are the
    inputs. In the real producer these all belong to the same private phase.
    """
    from config_decoded_sources import (
        _parents,
        _CAPTURE,
        _TRANSPOSE,
        _CARRIER_COPY,
        _FINAL_LOCAL,
        _COMMIT,
    )
    from config_source_contracts import GRAMMARS

    V = parents[0]["function"] - 4376
    B = array["old_array"].start - 32
    require(
        parents == _parents(objects, rows, V, B)
        and frame.extent == 1608
        and array["sequence"] is not None
        and array["sequence"].count == Span(0, 0)
        and len(array["packages"]) == 8
        and effects is not None
        and envelopes["stores"] == 80,
        "Sources chronological publication lacks its current complete conditional array",
    )
    packages = array["packages"]
    before = packages[0]["sequence_before"]
    call_records = []
    high = tuple("r" + str(i) for i in range(4, 12))
    call_map = {site: (target, role, mask) for site, target, role, mask in returns["parent_calls"]}
    require(
        len(call_map) == 57 and all(mask == high for target, role, mask in call_map.values()),
        "Sources chronological carriers lose their actual child transfer",
    )
    # The existing full literals already match every selected row and its
    # encoding. Keep the complete ordered parent instruction membership.
    selected = {
        p["function"] - V + off
        for p in parents
        for off, width, text in GRAMMARS["visitor_" + str(p["function"] - V)]["instructions"]
    }
    path = tuple(off for lo, hi in _SEGMENTS for off in sorted(selected) if lo <= off < hi)
    require(
        len(path) == len(set(path)) and path[0] == 4376 and path[-1] == 4794,
        "Sources chronological normal path changes its exact membership",
    )
    positions = {off: i for i, off in enumerate(path)}
    guarded = {a: b for a, b, c in _SOURCE_BRANCHES}
    guarded.update({5516: 5762, 5854: 4410})
    no_error = {c for a, b, c in _SOURCE_BRANCHES} | {4530}
    for i, off in enumerate(path):
        row = rows[V + off]
        following = off + len(row.raw)
        next_pc = path[i + 1] if i + 1 < len(path) else 4796
        op = row.mnemonic.split(".", 1)[0]
        if op == "bl":
            require(
                V + off in call_map and next_pc == following,
                "Sources chronology bypasses an actual original child return",
            )
        elif off in guarded:
            require(
                op in ("b", "bne")
                and next_pc == guarded[off]
                and row.operands.split(" ", 1)[0] == hex(V + guarded[off]),
                "Sources chronological normal guard/target differs",
            )
        elif off in no_error:
            require(
                op == "beq"
                and row.operands.split(" ", 1)[0] == hex(V + 170)
                and next_pc == following,
                "Sources chronological None arm changes its actual stopped frontier",
            )
        else:
            require(
                op not in ("b", "beq", "bne", "bx", "tbb", "tbh", "cbz", "cbnz")
                and next_pc == following,
                "Sources chronological segment adds an unpaired parent successor",
            )
    for index, (package, (call, out, tag), (branch, target, none)) in enumerate(
        zip(packages, SOURCE_EPOCHS, _SOURCE_BRANCHES)
    ):
        result = array["invocations"][index]["result"]
        normal = result["normal"]
        require(
            package["source"] is normal
            and normal is not None
            and normal["tag_domain"] == (0, 1)
            and normal["before"] == before.decoder
            and package["sequence_before"] == before
            and package["sequence_after"].count == Span(7 - index, 7 - index)
            and package["sequence_after"].decoder == normal["after"]
            and normal["return_site"] == V + call + 4
            and (package["output"].storage, package["output"].offset, package["output"].extent)
            == (frame, out, 22)
            and package["captured_tag"].start == frame.start + tag
            and call_map[V + call][:2] == (result["proof"]["function"], result["proof"]["role"]),
            "Sources chronology mixes a returned package/tag/source epoch",
        )
        # The two parent predicates are justified by this actual child0/1
        # domain, not by assuming any value except ffff and2 must be valid.
        require(
            positions[call] < positions[branch] < positions[target] <= positions[none],
            "Sources parent tests precede their same-call tag capture",
        )
        call_records.append(
            dict(
                index=index,
                package=package,
                call=V + call,
                returned_tag_domain=normal["tag_domain"],
                nonerror_edge=(V + branch, V + target),
                absent_None_edge=(V + none, V + 170),
            )
        )
        before = package["sequence_after"]
    require(before == array["sequence"], "Sources chronology changes its final same-Q cursor")
    # Only these exact six writes can initialize/recycle the four scratch
    # pointer cells. Every other parent store, including later alternatives,
    # preserves them. The inherited cells retain their earlier checked epoch.
    expected_carriers = (
        (216, 1001),
        (224, 688),
        (228, 576),
        (232, 1420),
        (236, 1380),
        (240, 1400),
        (244, 1440),
        (248, 1360),
    )
    require(
        tuple((v.start - frame.start, target - frame.start) for v, target in carriers)
        == expected_carriers,
        "Sources chronology changed an inherited pointer producer/epoch",
    )
    scratch = tuple(
        _object(borrow(frame, off, 4, frame.phase), "Source chronological pointer " + str(off))
        for off in (200, 204, 208, 212)
    )
    stable = tuple(_object(v, "inherited Source pointer") for v, target in carriers)
    creators = {at: cell for at, cell, target, read in POINTER_EPOCHS}
    for site, coordinate, offset, size, category in PARENT_WRITES:
        if coordinate != "V":
            continue
        view = borrow(frame, offset, size, frame.phase, True)
        relevant = stable + tuple(
            v for v in scratch if site not in creators or v.start != frame.start + creators[site]
        )
        require_writes((view,), ((view, 0, size),), relevant)
    for at, cell, target, read in POINTER_EPOCHS:
        require(
            positions[at] < positions[read]
            and (cell != 204 or at != 5468 or positions[5458] < positions[at])
            and (cell != 208 or at != 5480 or positions[5470] < positions[at]),
            "Sources pointer cell is reused before its last read/copy return",
        )
    # Every actual copy after pointer creation has a disjoint destination and
    # restored-tail child frame. Recheck these against the pointer cells which
    # are initialized here, rather than relying on a previous phase's guards.
    child = Storage(
        frame.space,
        "Sources physical copy descendants",
        frame.start - 72,
        72,
        8,
        frame.phase,
        False,
    )
    for guard in scratch + stable:
        require_disjoint((child, guard))
    chains = effects["transports"]
    require(len(chains) == 4, "Sources chronology omits a staging phase")
    specs = (_CAPTURE, _TRANSPOSE, _CARRIER_COPY, _FINAL_LOCAL)
    lineages = []
    for index, package in enumerate(packages):
        previous = borrow(
            frame,
            package["output"].offset + (3 if index == 0 else 2),
            19 if index == 0 else 20,
            frame.phase,
        )
        stages = []
        for stage, (chain, definitions) in enumerate(zip(chains, specs)):
            event = chain[index]
            site, origin, destination, count = definitions[index]
            require(
                event["index"] == index
                and event["site"] == V + site
                and event["source"] == previous
                and event["source"] == borrow(frame, origin, count, frame.phase)
                and event["destination"] == borrow(frame, destination, count, frame.phase, True),
                "Sources chronological transport loses its same-package source/destination",
            )
            if stage == 0:
                require(
                    V + site in call_map
                    and event["effect"]["physical_activation_paths"] == ((8,), (72,))
                    and event["effect"]["initialized_semantic_bytes_not_inferred"] is True,
                    "Sources chronological copy loses its restored original-return closure",
                )
            require_writes(
                (event["destination"],), event["effect"]["possible_writes"], scratch + stable
            )
            stages.append((V + site, event["source"], event["destination"]))
            previous = borrow(frame, destination, count, frame.phase)
        slot = effects["slots"][index]
        site, origin, destination, count = _COMMIT[index]
        require(
            slot["source_package"] is package
            and slot["index"] == index
            and slot["tag_store"] == V + _TAG_STORES[index]
            and slot["payload"]["source"] == previous
            and slot["payload"]["site"] == V + site
            and slot["payload"]["destination"].start == B + destination
            and slot["payload"]["destination"].extent == count
            and slot["complete_after"] == V + site + len(rows[V + site].raw)
            and slot["prior_prefix_bytes"] == 22 * index,
            "Sources chronological Config last store loses its package/tag/copy association",
        )
        if index in (0, 2, 4, 6):
            require(V + site in call_map, "Sources Config copy omits its original child return")
        require(
            positions[_TAG_STORES[index]] < positions[site]
            and (index == 0 or positions[_COMMIT[index - 1][0]] < positions[_TAG_STORES[index]]),
            "Sources chronological Config prefix advances before its prior slot completed",
        )
        lineages.append(dict(index=index, package=package, stages=tuple(stages), slot=slot))
    require(
        tuple(i for i in sorted(range(8), key=lambda i: positions[_CAPTURE[i][0]]))
        == _CAPTURE_ORDER
        and positions[5410] < positions[5414] < positions[5426] < positions[5430]
        and positions[4664] < positions[4670] < positions[4676],
        "Sources split byte crosses its original package/copy/tag epoch",
    )
    return dict(
        normal_parent_path=tuple(V + off for off in path),
        source_calls=tuple(call_records),
        payload_lineage=tuple(lineages),
        capture_order=_CAPTURE_ORDER,
        pointer_epochs=tuple((V + a, cell, target, V + b) for a, cell, target, b in POINTER_EPOCHS),
        split_byte=(packages[0], V + 5414, V + 5426, V + 4664),
        source_nonpadding_bytes=168,
        copied_padding_is_initialized=False,
        complete_ordinary_Source_prefix_issued=False,
        requires_actual_private_entry_source_control_and_reached_accesses=True,
    )


def _suffix_normal_paths(rows, parents, suffix, calls):
    """Closed grouped parent successors for the same typed normal cases.

    Basis/origin and Warning selector alternatives are local relations, not a
    Cartesian set of complete configurations. Only literal current parent
    rows and the exact current original child-return map are accepted here.
    """
    from config_regions import _control
    from config_owner_entry import _direct_target
    from config_source_contracts import GRAMMARS

    V = parents[0]["function"] - 4376
    selected = tuple(
        sorted(
            p["function"] - V + off
            for p in parents
            for off, width, text in GRAMMARS["visitor_" + str(p["function"] - V)]["instructions"]
        )
    )

    def segment(lo, hi):
        return tuple(v for v in selected if lo <= v < hi)

    def path(offsets, end, choices):
        require(
            offsets and len(offsets) == len(set(offsets)),
            "Sources suffix parent path duplicates a current row",
        )
        branches = []
        for index, off in enumerate(offsets):
            row = rows[V + off]
            following = off + len(row.raw)
            after = offsets[index + 1] if index + 1 < len(offsets) else end
            control, conditional = _control(row)
            if control == "bl":
                edge = next(
                    e
                    for p in parents
                    for e in p["ordinary_edges"]
                    if p["function"] + e[0] == V + off
                )
                require(
                    calls.get(V + off, ())[:2] == (edge[2], edge[3]) and after == following,
                    "Sources suffix parent loses an actual original child return",
                )
            elif control in ("b", "cbz", "cbnz"):
                require(off in choices, "Sources suffix parent adds an unpaired source guard")
                taken, target = choices[off]
                require(
                    _direct_target(row, control) == V + target
                    and (conditional or taken)
                    and after == (target if taken else following),
                    "Sources suffix parent guard/successor differs",
                )
                branches.append((V + off, V + target, taken))
            else:
                require(
                    control is None and after == following,
                    "Sources suffix parent adds an implicit PC/fallthrough effect",
                )
        return dict(
            instructions=tuple(V + v for v in offsets), branches=tuple(branches), frontier=V + end
        )

    warning = []
    if suffix["warning"] is not None:
        for scope in (0, 1):
            head = (segment(4796, 4836) + segment(4844, 4862)) if scope else segment(4796, 4862)
            offsets = head + segment(5178, 5240) + segment(4870, 4886) + segment(5006, 5044)
            choices = {
                4800: (False, 170),
                4822: (False, 4862),
                4834: (scope == 1, 4844),
                4838: (False, 6446),
                4858: (True, 5178),
                5184: (False, 6446),
                5210: (False, 4866),
                5216: (False, 6446),
                5238: (True, 4870),
                4884: (True, 5006),
                5010: (False, 170),
                5032: (False, 56),
            }
            warning.append(
                dict(
                    scope=scope,
                    mode_domain=(0, 1),
                    path=path(offsets, 5044, choices),
                    inactive_predicated_instruction=V + 5022,
                )
            )
    common = {5048: (False, 170), 5072: (False, 140), 5082: (False, 170)}
    fuel = []
    for assignment in suffix["Fuel_assignments"]:
        wire, density = assignment["source_guard"]
        choices = dict(common)
        if wire == 0:
            offsets = segment(5044, 5090) + segment(5524, 5574)
            choices[5088] = (True, 5524)
            pieces = (path(offsets, 5574, choices),)
        elif wire == 1:
            offsets = segment(5044, 5140) + segment(5526, 5574)
            choices.update(
                {
                    5088: (False, 5524),
                    5094: (False, 5732),
                    5118: (False, 5754),
                    5132: (False, 6656),
                    5138: (True, 5526),
                }
            )
            pieces = (path(offsets, 5574, choices),)
        else:
            prefix = segment(5044, 5098) + segment(5732, 5754) + segment(5860, 5910)
            choices.update(
                {
                    5088: (False, 5524),
                    5094: (True, 5732),
                    5752: (True, 5860),
                    5862: (False, 170),
                    5870: (False, 170),
                    5898: (False, 140),
                    5912: (False, 170),
                    5920: (True, 5978),
                }
            )
            basis = tuple(
                path(
                    prefix + (segment(5974, 5978) if value else segment(5910, 5922)),
                    5978,
                    choices | {5908: (value == 1, 5974)},
                )
                for value in (0, 1)
            )
            origins = tuple(
                path(
                    segment(5978, 6008) + (segment(6218, 6220) if value else segment(6008, 6018)),
                    6220,
                    {
                        5980: (False, 170),
                        5996: (False, 140),
                        6006: (value == 1, 6218),
                        6010: (False, 170),
                        6016: (True, 6220),
                    },
                )
                for value in (0, 1)
            )
            tail = (
                segment(6220, 6254)
                + (segment(6254, 6282) if density else ())
                + segment(6282, 6412)
                + segment(5526, 5574)
            )
            tail_choices = {
                6222: (False, 170),
                6244: (False, 140),
                6252: (density == 0, 6282),
                6256: (False, 170),
                6274: (False, 140),
                6296: (False, 5754),
                6302: (False, 170),
                6328: (False, 5754),
                6334: (False, 170),
                6360: (False, 5754),
                6366: (False, 170),
                6392: (False, 5754),
                6398: (False, 170),
                6410: (True, 5526),
            }
            pieces = (basis, origins, path(tail, 5574, tail_choices))
        fuel.append(dict(assignment=assignment, source_guard=(wire, density), pieces=pieces))
    colors = ()
    if suffix["normal"]:
        colors = (path(segment(5574, 5630), 5630, {5592: (False, 56), 5598: (False, 170)}),)
    return dict(
        Warning=tuple(warning),
        Fuel=tuple(fuel),
        Colors=colors,
        requires_actual_case_argument_and_predication_relations=True,
    )


def _suffix_lineage(objects, rows, parents, suffix, frame, owner, returns, envelopes):
    """Conditional same-source child/result and late-store relation.

    This is a typed argument/effect interface. It does not independently
    supply the selected private predecessor or active parent successors.
    Those joins remain required before an ordinary Source prefix is issued.
    """
    from config_decoded_sources import _parents
    from config_decoded_widgets import _at_child, _edge_at
    from config_source_children import source_children

    V = parents[0]["function"] - 4376
    require(
        parents == _parents(objects, rows, V, owner.start)
        and frame.extent == 1608
        and suffix["before"].header == borrow(frame, 288, 8, frame.phase, True)
        and suffix["before"].count == Span(7, 7)
        and envelopes["stores"] == 80,
        "Sources suffix loses its current parent/owner/top7 relation",
    )
    children = source_children(objects, rows, parents)
    high = tuple("r" + str(i) for i in range(4, 12))
    calls = {site: (target, role, mask) for site, target, role, mask in returns["parent_calls"]}
    require(
        len(calls) == 57
        and all(mask == high for target, role, mask in calls.values())
        and returns["high_fp"],
        "Sources suffix lacks an actual child transfer",
    )
    steps = suffix["transitions"]
    checked = []
    direct = {
        "Warning scope": (4814, 1),
        "Warning mode": (4850, 1),
        "Fuel": (5064, 2),
        "Blend basis": (5890, 1),
        "Blend origin": (5988, 1),
        "Blend density": (6236, 1),
    }
    scalar = {
        "Warning page": (5190, "seq_u16", 1),
        "Stoich ratio": (5110, "seq_f32", 1),
        "Blend percent": (5744, "seq_f32", 8),
        "Blend gasoline_stoich": (6288, "seq_f32", 4),
        "Blend ethanol_stoich": (6320, "seq_f32", 3),
        "Blend gasoline_density": (6352, "seq_f32", 2),
        "Blend ethanol_density": (6384, "seq_f32", 1),
    }
    for step in steps:
        field = step["field"]
        before = step["before"]
        after = step["after"]
        if field in direct:
            off, maximum = direct[field]
            semantic = "take_varint_u32"
            require(step["domain"] == (0, maximum), "Sources suffix changes a paired child domain")
        elif field in scalar:
            off, semantic, count = scalar[field]
            require(
                before.header == borrow(frame, 1000, 8, frame.phase, True)
                and before.count == Span(count, count)
                and (after is None or after.count == Span(count - 1, count - 1)),
                "Sources suffix changes a counted Blend/Warning paired child",
            )
        else:
            require(
                field == "Blend density temperature", "Sources suffix injects another child field"
            )
            off = 6266
            semantic = "deserialize_seed_f32"
            require(
                step["source_guard"] == (2, 1)
                and step["sequence_count_before"] == step["sequence_count_after"] == Span(4, 4),
                "Sources suffix counts the direct density child",
            )
        proof = _at_child(parents, children, V + off, semantic)
        edge = _edge_at(parents, V + off)
        require(
            step["site"] == V + off
            and step["child"]["proof"] == proof
            and V + off in calls
            and calls[V + off][:2] == (edge[2], edge[3])
            and step["child"]["semantic_role"] == semantic,
            "Sources suffix changes its current paired child/call body or actual child transfer",
        )
        expected = "some" if field in scalar else "success"
        options = tuple(v["state"] for v in step["child"]["outcomes"] if v["kind"] == expected)
        require(
            len(options) <= 1
            and (after is None and not options or len(options) == 1 and after is options[0]),
            "Sources suffix after-state is not its paired child return",
        )
        decoder = before.decoder if field in scalar else before
        require(
            decoder.header.start == frame.start + 1608 + 236
            and decoder.header.extent == 8
            and decoder.payload == suffix["before"].decoder.payload,
            "Sources suffix child changes its same D/payload",
        )
        checked.append((step, V + off + 4))

    def find(field, decoder):
        matches = tuple(
            v
            for v in steps
            if v["field"] == field
            and (v["before"].decoder if field in scalar else v["before"]) is decoder
        )
        require(len(matches) == 1, "Sources suffix loses its paired child source epoch")
        return matches[0]

    warning = suffix["warning"]
    warning_relation = None
    if warning is not None:
        scope = find("Warning scope", suffix["before"].decoder)
        mode = find("Warning mode", scope["after"])
        page = find("Warning page", mode["after"])
        require(
            page["after"] is not None
            and warning["after"] is page["after"].decoder
            and warning["before"] is suffix["before"].decoder
            and warning["scratch_read"] == (frame.start + 1065, frame.start + 1067)
            and warning["complete_store"] == V + 5040
            and warning["scope_domain"] == warning["mode_domain"] == (0, 1),
            "Sources Warning pack loses its page/scope/mode child lineage",
        )
        warning_relation = dict(
            scope_alternatives=(0, 1),
            mode_alternatives=(0, 1),
            children=(scope, mode, page),
            scratch_read=warning["scratch_read"],
            complete_store=V + 5040,
        )
    physical = (
        (5532, 29, 1),
        (5534, 7, 1),
        (5542, 28, 1),
        (5546, 8, 4),
        (5550, 12, 4),
        (5554, 16, 4),
        (5558, 20, 4),
        (5562, 24, 4),
        (5566, 4, 1),
        (5568, 0, 4),
        (5570, 5, 2),
    )
    require(
        all(
            (at, "B", offset, size, "Fuel_Config") in PARENT_WRITES for at, offset, size in physical
        ),
        "Sources Fuel physical store partition differs",
    )
    fuels = []
    for assigned in suffix["Fuel_assignments"]:
        require(warning is not None, "Sources Fuel precedes its same Warning commit")
        selector = find("Fuel", warning["after"])
        wire, density = assigned["source_guard"]
        fuel = assigned["Fuel"]
        require(
            assigned["before"] is selector["after"]
            and ((wire, density) in ((0, None), (1, None), (2, 0), (2, 1)))
            and fuel["wire_tag"] == wire
            and fuel["emitted_tag"] == ({0: 2, 1: 3}.get(wire, density))
            and fuel["complete_store"] == V + 5570
            and fuel["early_tag_store"] == V + 5568
            and fuel["pending_after_tag"] == ((5, 7),),
            "Sources Fuel loses its paired selector/emitted tag/late store",
        )
        selected = []
        after = selector["after"]
        if wire == 1:
            ratio = find("Stoich ratio", after)
            selected.append(ratio)
            after = ratio["after"].decoder
        elif wire == 2:
            percent = find("Blend percent", after)
            selected.append(percent)
            after = percent["after"].decoder
            for name in ("Blend basis", "Blend origin", "Blend density"):
                step = find(name, after)
                selected.append(step)
                after = step["after"]
            if density == 1:
                extra = find("Blend density temperature", after)
                selected.append(extra)
                after = extra["after"]
            for name in (
                "Blend gasoline_stoich",
                "Blend ethanol_stoich",
                "Blend gasoline_density",
                "Blend ethanol_density",
            ):
                step = find(name, after)
                selected.append(step)
                after = step["after"].decoder
        require(
            assigned["after"] is after
            and all(
                v.storage == owner and v.offset == 0 and v.extent == 32
                for v, o, n in assigned["possible_writes"]
            )
            and set((o, n) for v, o, n in assigned["possible_writes"])
            == set((o, n) for at, o, n in physical),
            "Sources Fuel source/physical footprint is unpaired",
        )
        fuels.append(
            dict(
                assignment=assigned,
                source_guard=(wire, density),
                selected_children=tuple(selected),
                physical_store_order=tuple(V + at for at, o, n in physical),
                pending_after_tag=((5, 7),),
                direct_density_count_decrement=0,
                counted_tail=(4, 3, 2, 1, 0) if wire == 2 else (),
                inactive_payload_remains_opaque=True,
            )
        )
    invocations = suffix["Colors_invocations"]
    require(len(invocations) == len(fuels), "Sources Colors drops a completed Fuel arm")
    colors_errors = []
    from config_widget_contracts import ROLES as WIDGET

    color_edge = _edge_at(parents, V + 5578)
    require(
        color_edge[3] == WIDGET["warning_colors"]
        and calls.get(V + 5578, ())[:2] == (color_edge[2], color_edge[3]),
        "Sources Colors loses its actual child transfer",
    )
    color_proof = children[color_edge[2], color_edge[3]]
    for call, assigned in zip(invocations, suffix["Fuel_assignments"]):
        require(
            call["Fuel_assignment"] is assigned
            and call["site"] == V + 5578
            and V + 5578 in calls
            and call["child"]["proof"] == color_proof
            and color_proof["shape"] == "warning_colors"
            and call["before"].decoder is assigned["after"]
            and call["before"].count == Span(5, 5)
            and call["before"].header == suffix["before"].header,
            "Sources Colors does not consume the same completed Fuel",
        )
    for normal in suffix["normal"]:
        color = normal["colors"]
        call = color["invocation"]
        require(
            any(call is v for v in invocations)
            and normal["Fuel"] is call["Fuel_assignment"]["Fuel"]
            and color["source"] is call["child"]
            and any(
                v["kind"] == "some" and v["state"] is normal["after"]
                for v in call["child"]["outcomes"]
            )
            and normal["after"].count == Span(4, 4)
            and (color["partial_background_store"], color["complete_store"])
            == (V + 5618, V + 5628),
            "Sources Colors normal path loses its same completed Fuel/late stores",
        )
    for error in suffix["errors"]:
        if error["stage"] != "global WarningColors":
            field = error["stage"].removesuffix(".domain")
            option = error["child_outcome"]
            origins = tuple(
                step
                for step in steps
                if step["field"] == field and any(option is v for v in step["child"]["outcomes"])
            )
            require(
                len(origins) == 1
                and error["cursor"] == option["cursor"]
                and (
                    option["kind"] == "success"
                    if error["stage"].endswith(".domain")
                    else option["kind"] not in ("success", "some")
                ),
                "Sources suffix error loses its same child/source outcome",
            )
            continue
        call = error["source_invocation"]
        option = error["child_outcome"]
        require(
            any(call is v for v in invocations)
            and any(option is v for v in call["child"]["outcomes"])
            and option["kind"] != "some"
            and error["cursor"] == option["cursor"]
            and error["completed_aggregates"] == ("Sources", "WarningPresentation", "Fuel"),
            "Sources Colors error loses its reached Fuel/child outcome",
        )
        colors_errors.append(dict(invocation=call, outcome=option, completed_Fuel_store=V + 5570))
    chain = _suffix_source_chain(suffix)
    paths = _suffix_normal_paths(rows, parents, suffix, calls)
    arguments = _suffix_argument_relations(rows, V, suffix, frame, calls, paths)
    errors = _suffix_error_relations(objects, rows, V, parents, suffix, calls, chain, frame)
    return dict(
        warning=warning_relation,
        Fuel=tuple(fuels),
        checked_child_returns=tuple(checked),
        parent_normal_paths=paths,
        argument_transports=arguments,
        source_chain=chain,
        error_carriers=errors,
        Colors=dict(
            normal=bool(suffix["normal"]),
            errors=tuple(colors_errors),
            source_order=("foreground", "background"),
            store_order=(V + 5618, V + 5628),
        ),
        frontier=V + 5630,
        outgoing_top4=bool(suffix["normal"]),
        confers_active_state=False,
        requires_actual_private_entry_guarded_parent_successors_and_reached_uses=True,
    )


def _suffix_error_relations(objects, rows, V, parents, suffix, calls, chain, frame):
    """Exact stopped error carriers from the same entered child outcomes.

    The final frontier is before56's whole-register move. No low-byte
    truncation, later return, rollback or completed current aggregate follows.
    """
    from config_source_contracts import GRAMMARS
    from config_regions import _control
    from config_owner_entry import _direct_target

    selected = tuple(
        sorted(
            p["function"] - V + off
            for p in parents
            for off, width, text in GRAMMARS["visitor_" + str(p["function"] - V)]["instructions"]
        )
    )
    for off, raw, op, args in (
        (140, "9df82954", "ldrb.w", "r5, [sp, #0x429]"),
        (144, "d2e7", "b", hex(V + 56)),
        (170, "0e25", "movs", "r5, #0xe"),
        (172, "c4e7", "b", hex(V + 56)),
    ):
        row = rows.get(V + off)
        require(
            row is not None
            and row.raw == bytes.fromhex(raw)
            and row.mnemonic == op
            and (_direct_target(row, "b") == V + 56 if op == "b" else row.operands == args)
            and objects.read_only(V + off, len(row.raw)) == row.raw,
            "Sources error carrier inherited island differs",
        )

    def seg(lo, hi):
        return tuple(v for v in selected if lo <= v < hi)

    def checked_path(offsets, choices):
        for i, off in enumerate(offsets):
            row = rows[V + off]
            following = off + len(row.raw)
            after = offsets[i + 1] if i + 1 < len(offsets) else 56
            control, conditional = _control(row)
            if control == "bl":
                require(
                    V + off in calls
                    and _direct_target(row, "bl") == calls[V + off][0]
                    and after == following,
                    "Sources error carrier bypasses its original child return",
                )
            elif control in ("b", "cbz", "cbnz"):
                require(off in choices, "Sources error carrier adds an unpaired predicate")
                taken, target = choices[off]
                require(
                    _direct_target(row, control) == V + target
                    and (conditional or taken)
                    and after == (target if taken else following),
                    "Sources error carrier predicate/successor differs",
                )
            else:
                require(
                    control is None and after == following,
                    "Sources error carrier adds an implicit PC/fallthrough",
                )
        return tuple(V + off for off in offsets)

    direct = {
        "Warning scope": (4814, 4822, 4862),
        "Warning mode": (4850, 4858, 5178),
        "Fuel": (5064, 5072, 140),
        "Blend basis": (5890, 5898, 140),
        "Blend origin": (5988, 5996, 140),
        "Blend density": (6236, 6244, 140),
        "Blend density temperature": (6266, 6274, 140),
    }
    scalar = {
        "Stoich ratio": (5110, 5118),
        "Blend percent": (5744, 5752),
        "Blend gasoline_stoich": (6288, 6296),
        "Blend ethanol_stoich": (6320, 6328),
        "Blend gasoline_density": (6352, 6360),
        "Blend ethanol_density": (6384, 6392),
    }
    result = []
    for error in suffix["errors"]:
        stage = error["stage"]
        field = stage.removesuffix(".domain")
        domain = stage.endswith(".domain")
        option = error["child_outcome"]
        prior = ("Sources",) if field.startswith("Warning ") else ("Sources", "WarningPresentation")
        if field == "global WarningColors":
            prior += ("Fuel",)
        expected_frontier = V + (170 if domain and not field.startswith("Warning ") else 56)
        require(
            error["completed_aggregates"] == prior and error["frontier"] == expected_frontier,
            "Sources error carrier changes its stopped frontier/prior physical prefix",
        )
        choices = {
            144: (True, 56),
            172: (True, 56),
            5758: (True, 56),
            6658: (True, 56),
            6448: (True, 4866),
            4884: (False, 5006),
            4890: (True, 5024),
            5032: (True, 56),
        }
        physical_read = ()
        packed = field.startswith("Warning ")
        opaque = ()
        if field == "global WarningColors":
            origin = error["source_invocation"]
            require(
                any(origin is v for v in suffix["Colors_invocations"])
                and origin["Fuel_assignment"]["Fuel"]["complete_store"] == V + 5570,
                "Sources error carrier precedes its same Fuel last store",
            )
            offsets = seg(5578, 5596)
            choices[5592] = (option["kind"] == "error", 56)
            if option["kind"] == "none":
                offsets += seg(5596, 5602) + (170, 172)
                choices[5598] = (True, 170)
                opaque = ((1066, 1),)
            else:
                require(
                    option["kind"] == "error"
                    and option["tag"] == 2
                    and any(
                        v.offset == 1064 and off <= 2 < off + n
                        for v, off, n in option["possible_writes"]
                    ),
                    "Sources error carrier reads an uninitialized Colors error",
                )
            physical_read = ((1066, 1),)
        else:
            origins = tuple(
                v
                for v, before, after in chain
                if v["field"] == field and any(option is p for p in v["child"]["outcomes"])
            )
            require(len(origins) == 1, "Sources error carrier loses its same entered source call")
            origin = origins[0]
            if field in direct:
                site, branch, target = direct[field]
                if domain:
                    require(
                        option["kind"] == "success",
                        "Sources error carrier domain rejection is not a successful parse",
                    )
                    require(
                        (4, 4) in option["initialized_result_fields"],
                        "Sources error carrier domain selector word is not initialized",
                    )
                    physical_read = ((1068, 4),)
                    if field == "Warning scope":
                        offsets = seg(site, 4842) + (6446, 6448)
                        choices.update(
                            {4822: (False, 4862), 4834: (False, 4844), 4838: (True, 6446)}
                        )
                    elif field == "Warning mode":
                        offsets = seg(site, 4862) + seg(5178, 5188) + (6446, 6448)
                        choices.update({4858: (True, 5178), 5184: (True, 6446)})
                    else:
                        end, taken, extra = {
                            "Fuel": (5086, 5082, ()),
                            "Blend basis": (5916, 5912, ()),
                            "Blend origin": (6014, 6010, ()),
                            "Blend density": (6260, 6256, ()),
                        }[field]
                        offsets = seg(site, end) + (170, 172)
                        choices[branch] = (False, 140)
                        choices[taken] = (True, 170)
                        if field == "Blend basis":
                            choices[5908] = (False, 5974)
                        if field == "Blend origin":
                            choices[6006] = (False, 6218)
                        if field == "Blend density":
                            choices[6252] = (False, 6282)
                else:
                    require(
                        option["kind"].startswith("error")
                        and (1, 1) in option["initialized_result_fields"],
                        "Sources error carrier direct error byte is not initialized",
                    )
                    offsets = seg(site, branch + len(rows[V + branch].raw))
                    choices[branch] = (field != "Warning mode", target)
                    offsets += (4862,) if packed else (140, 144)
                    physical_read = ((1065, 1),)
            elif field == "Warning page":
                offsets = seg(5190, 5214)
                choices[5210] = (option["kind"] != "none", 4866)
                if option["kind"] == "none":
                    offsets += seg(5214, 5220) + (6446, 6448)
                    choices[5216] = (True, 6446)
            else:
                site, branch = scalar[field]
                offsets = seg(site, branch + len(rows[V + branch].raw))
                choices[branch] = (
                    field == "Blend percent"
                    if option["kind"] == "none"
                    else field != "Blend percent",
                    5860 if field == "Blend percent" else 5754,
                )
                if option["kind"] == "none":
                    if field == "Stoich ratio":
                        offsets += seg(5122, 5136) + (6656, 6658)
                        choices[5132] = (True, 6656)
                        opaque = ((1068, 4),)
                        physical_read = opaque
                    elif field == "Blend percent":
                        offsets += seg(5860, 5866) + (170, 172)
                        choices[5862] = (True, 170)
                    else:
                        stop = {
                            "Blend gasoline_stoich": 6306,
                            "Blend ethanol_stoich": 6338,
                            "Blend gasoline_density": 6370,
                            "Blend ethanol_density": 6402,
                        }[field]
                        offsets += seg(branch + 4, stop) + (170, 172)
                        choices[stop - 4] = (True, 170)
                else:
                    require(
                        option["kind"].startswith("error")
                        and (4, 1) in option["initialized_result_fields"],
                        "Sources error carrier sequence error byte is not initialized",
                    )
                    offsets += (5754, 5758)
                    physical_read = ((1068, 1),)
        if packed:
            offsets += seg(4866, 4892) + seg(5024, 5036)
            transport = error["error_transport"]
            require(
                transport["kind"] == "packed_Warning_lowbyte"
                and transport["mask"] == 0x020000FF
                and transport["whole_r5_is_scalar_error"] is False,
                "Sources error carrier loses its complete packed word",
            )
        result.append(
            dict(
                error=error,
                origin=origin,
                outcome=option,
                path=checked_path(offsets, choices),
                carrier_reads=tuple((frame.start + off, n) for off, n in physical_read),
                opaque_reads_discarded=tuple((frame.start + off, n) for off, n in opaque),
                carrier="packed_0x02000000_or_error_u8"
                if packed
                else "initialized_error_u8_or_constant14",
                stopped_before=V + 56,
                whole_register_narrowed=False,
                prior_physical_stores=(V + 4794,)
                + (() if field.startswith("Warning ") else (V + 5040,))
                + ((V + 5570,) if field == "global WarningColors" else ()),
            )
        )
    return tuple(result)


def _suffix_source_chain(suffix):
    """Every entered child, including error-only ones, has its real input.

    Independent enum choices remain local guards. Only the two density
    cursor alternatives fork this fixed child sequence; failed successors
    do not borrow another branch's normal input or publish an aggregate.
    """
    from config_abstract import SequenceInterface

    steps = suffix["transitions"]
    used = []
    edges = []

    def take(field, before):
        if before is None:
            return None
        matches = tuple(
            v
            for v in steps
            if v["field"] == field
            and (v["before"].decoder if type(v["before"]) is SequenceInterface else v["before"])
            is before
        )
        require(len(matches) == 1, "Sources suffix source chain loses its exact predecessor input")
        step = matches[0]
        require(not any(step is v for v in used), "Sources suffix source chain replays a child")
        used.append(step)
        after = step["after"]
        decoder = after.decoder if type(after) is SequenceInterface else after
        edges.append((step, before, decoder))
        return decoder

    current = take("Warning scope", suffix["before"].decoder)
    current = take("Warning mode", current)
    current = take("Warning page", current)
    require(
        (suffix["warning"] is None and current is None)
        or suffix["warning"] is not None
        and suffix["warning"]["after"] is current,
        "Sources suffix source chain changes its Warning completion",
    )
    fuel = take("Fuel", current)
    take("Stoich ratio", fuel)
    blend = take("Blend percent", fuel)
    for name in ("Blend basis", "Blend origin", "Blend density"):
        blend = take(name, blend)
    temperature = take("Blend density temperature", blend)
    for before in (
        () if blend is None else (blend,) + (() if temperature is None else (temperature,))
    ):
        after = before
        for field in (
            "Blend gasoline_stoich",
            "Blend ethanol_stoich",
            "Blend gasoline_density",
            "Blend ethanol_density",
        ):
            after = take(field, after)
    require(
        len(used) == len(steps) and all(any(step is v for v in used) for step in steps),
        "Sources suffix source chain contains an unpaired or unreachable child",
    )
    return tuple(edges)


def _suffix_argument_relations(rows, V, suffix, frame, calls, paths):
    """Authored argument/carrier equations for these grouped normal paths.

    The complete literal contracts and actual child transfer precede this
    check. No opcode scalar is executed. A selected child outcome supplies
    its typed result domain; only its corresponding finite parent case can
    consume that domain. This remains conditional on the private entry.
    """
    from config_finite_returns import _high_definitions
    from config_regions import _control
    from config_abstract import DecoderInterface, SequenceInterface
    import re

    def current(off, op, args):
        row = rows.get(V + off)
        require(
            row is not None and (row.mnemonic, row.operands) == (op, args),
            "Sources suffix argument/predication instruction differs at " + str(off),
        )

    def preserves(reg, instructions, start=None, stop=None, inactive=()):
        selected = instructions
        if start is not None:
            selected = selected[selected.index(V + start) + 1 :]
        if stop is not None:
            selected = selected[: selected.index(V + stop)]
        for at in selected:
            control, _ = _control(rows[at])
            require(
                reg in calls[at][2]
                if control == "bl"
                else at - V in inactive or reg not in _high_definitions(rows[at], control),
                "Sources suffix parent/child redefines its live argument carrier",
            )
        return tuple(selected)

    def fp_preserves(reg, instructions, start=None, stop=None, low=False):
        selected = instructions
        if start is not None:
            selected = selected[selected.index(V + start) + 1 :]
        if stop is not None:
            selected = selected[: selected.index(V + stop)]
        for at in selected:
            row = rows[at]
            control, _ = _control(row)
            require(not low or control != "bl", "Sources Fuel low FP source crosses a child call")
            destination = (
                row.operands
                if row.mnemonic.startswith(("vpop", "vldm"))
                else row.operands.split(",", 1)[0]
            )
            aliases = re.compile(r"\b(?:" + reg + "|d" + str(int(reg[1:]) // 2) + r")\b")
            require(
                not row.mnemonic.startswith("v")
                or row.mnemonic.startswith(("vstr", "vstm", "vpush", "vcmp"))
                or not aliases.search(destination),
                "Sources suffix parent redefines its live FP source carrier",
            )
        return tuple(selected)

    # The first constructor loads D from actual topQ.D. Later Q constructors
    # and stable-Q.D effects retain that same pointer while D.cursor evolves.
    fixed = {
        4804: ("ldr", "r4, [sp, #0x120]"),
        4806: ("add.w", "r0, sp, #0x428"),
        4810: ("mov", "r1, r4"),
        4812: ("str", "r4, [sp, #0x3e8]"),
        4846: ("add.w", "r0, sp, #0x428"),
        5054: ("mov", "r1, r4"),
        5060: ("add.w", "r0, sp, #0x428"),
        5098: ("movs", "r0, #0x1"),
        5100: ("add", "r1, sp, #0x3e8"),
        5102: ("strd", "r4, r0, [sp, #1000]"),
        5106: ("add.w", "r0, sp, #0x428"),
        5188: ("add", "r0, sp, #0x3e8"),
        5732: ("movs", "r0, #0x8"),
        5734: ("add", "r1, sp, #0x3e8"),
        5736: ("strd", "r4, r0, [sp, #1000]"),
        5740: ("add.w", "r0, sp, #0x428"),
        5874: ("ldr.w", "r8, [sp, #0x3e8]"),
        5886: ("add.w", "r0, sp, #0x428"),
        5984: ("add.w", "r0, sp, #0x428"),
        6228: ("mov", "r1, r8"),
        6230: ("str", "r0, [sp, #0x3ec]"),
        6232: ("add.w", "r0, sp, #0x428"),
        6260: ("add.w", "r0, sp, #0x428"),
        6264: ("mov", "r1, r8"),
        6282: ("add.w", "r0, sp, #0x428"),
        6286: ("add", "r1, sp, #0x3e8"),
        6310: ("add", "r1, sp, #0x3e8"),
        6316: ("add.w", "r0, sp, #0x428"),
        6342: ("add", "r1, sp, #0x3e8"),
        6348: ("add.w", "r0, sp, #0x428"),
        6374: ("add", "r1, sp, #0x3e8"),
        6380: ("add.w", "r0, sp, #0x428"),
        5538: ("add.w", "r4, sp, #0x428"),
        5574: ("add", "r1, sp, #0x120"),
        5576: ("mov", "r0, r4"),
        5198: ("it", "ne"),
        5200: ("andne", "r1, r0, #0xff000000"),
        5204: ("add", "r1, r2"),
        5206: ("lsr.w", "r1, r1, #0x10"),
        5014: ("lsrs", "r0, r0, #0x18"),
        5018: ("cmp", "r0, #0xfe"),
        5020: ("it", "hi"),
        5022: ("andhi", "r5, r1"),
        5126: ("cmp", "r0, #0x0"),
        5128: ("it", "eq"),
        5130: ("moveq", "r6, #0xe"),
        5916: ("mov.w", "r9, #0x0"),
        5974: ("mov.w", "r9, #0x1"),
        6014: ("movs", "r4, #0x0"),
        6218: ("movs", "r4, #0x1"),
        6406: ("vldr", "s0, [r0, #44]"),
        5562: ("vstr", "s0, [r1, #24]"),
    }
    for off, (op, args) in fixed.items():
        current(off, op, args)
    # These complete tail outlines supply the direct r1 receiver. Their
    # checked semantic child and original-return chain are already paired.
    for off, register in ((4850, "r4"), (5890, "r8"), (5988, "r8")):
        target, role, mask = calls[V + off]
        row = rows.get(target)
        require(
            role == "@outline:widget_varint_from_" + register
            and row is not None
            and (row.mnemonic, row.operands) == ("mov", "r1, " + register),
            "Sources suffix outline receiver equation differs",
        )
    transfers = []
    for step in suffix["transitions"]:
        before = step["before"]
        sequence = type(before) is SequenceInterface
        require(
            sequence or type(before) is DecoderInterface,
            "Sources suffix has an untyped caller argument",
        )
        decoder = before.decoder if sequence else before
        r0 = (
            before.header
            if sequence and step["field"] == "Warning page"
            else borrow(frame, 1064, 8, frame.phase, True)
        )
        r1 = before.header if sequence else decoder.header
        arguments = (("r0", r0),) if step["field"] == "Warning page" else (("r0", r0), ("r1", r1))
        require(
            decoder.header.start == frame.start + 1844 and decoder.header.extent == 8,
            "Sources suffix actual child argument loses the same D",
        )
        transfers.append(
            dict(
                site=step["site"],
                source=step,
                arguments=arguments,
                result_domain="Some1/None0/Err2" if sequence else "Ok0/Err1",
            )
        )
    for call in suffix["Colors_invocations"]:
        transfers.append(
            dict(
                site=call["site"],
                source=call,
                arguments=(
                    ("r0", borrow(frame, 1064, 6, frame.phase, True)),
                    ("r1", call["before"].header),
                ),
                result_domain="Some1/None0/Err2",
            )
        )
    carriers = []
    float_carriers = []
    source_values = []
    for alternative in paths["Warning"]:
        p = alternative["path"]["instructions"]
        scope = alternative["scope"]
        carriers.append(("r4", 4804, 5044, preserves("r4", p, 4804)))
        carriers.append(
            ("r5", 4830 if scope else 4842, 4872, preserves("r5", p, 4830 if scope else 4842, 4872))
        )
        carriers.append(("r6", 5178, 5234, preserves("r6", p, 5178, 5234)))
        carriers.append(("r5", 4872, 5040, preserves("r5", p, 4872, 5040, (5022,))))
    low = ()
    for alternative in paths["Fuel"]:
        wire, density = alternative["source_guard"]
        pieces = alternative["pieces"]
        if wire in (0, 1):
            p = pieces[0]["instructions"]
            stop = 5064 if wire == 0 else 5102
            carriers.append(("r4", 4804, stop, preserves("r4", p, stop=stop)))
            if wire == 1:
                carriers.append(("r6", 5122, 5534, preserves("r6", p, 5122, 5534, (5130,))))
            carriers.append(
                (
                    "r5",
                    5524 if wire == 0 else 5136,
                    5568,
                    preserves("r5", p, 5524 if wire == 0 else 5136, 5568),
                )
            )
        else:
            basis, origin, tail = pieces
            tail = tail["instructions"]
            for ordinal, p in enumerate(basis):
                p = p["instructions"]
                carriers.append(("r4", 4804, 5736, preserves("r4", p, stop=5736)))
                carriers.append(("r8", 5874, 5978, preserves("r8", p, 5874)))
                carriers.append(
                    (
                        "r9",
                        5974 if ordinal else 5916,
                        5978,
                        preserves("r9", p, 5974 if ordinal else 5916),
                    )
                )
                float_carriers.append(("s16", 5882, 5978, fp_preserves("s16", p, 5882)))
            for ordinal, p in enumerate(origin):
                p = p["instructions"]
                carriers.append(("r8", 5874, 6220, preserves("r8", p)))
                carriers.append(("r9", "same basis arm", 6220, preserves("r9", p)))
                carriers.append(
                    (
                        "r4",
                        6218 if ordinal else 6014,
                        6220,
                        preserves("r4", p, 6218 if ordinal else 6014),
                    )
                )
                float_carriers.append(("s16", 5882, 6220, fp_preserves("s16", p)))
            carriers.append(("r8", 5874, 6282, preserves("r8", tail, stop=6282)))
            carriers.append(("r9", "same basis arm", 5542, preserves("r9", tail, stop=5542)))
            carriers.append(("r4", "same origin arm", 5532, preserves("r4", tail, stop=5532)))
            carriers.append(("r5", 6248, 5568, preserves("r5", tail, 6248, 5568)))
            if density:
                carriers.append(("r6", 6278, 5566, preserves("r6", tail, 6278, 5566)))
            for reg, load, store in (
                ("s16", None, 5546),
                ("s18", 6312, 5550),
                ("s20", 6344, 5554),
                ("s22", 6376, 5558),
            ):
                float_carriers.append(
                    (
                        reg,
                        5882 if load is None else load,
                        store,
                        fp_preserves(reg, tail, load, store),
                    )
                )
            # The low FP source is loaded after the final child return.
            # The branch to common Fuel stores has no later call or s0 def.
            low = fp_preserves("s0", tail, 6406, 5562, True)
        p = pieces[0]["instructions"] if wire in (0, 1) else pieces[2]["instructions"]
        carriers.append(("r4", 5538, 5574, preserves("r4", p, 5538)))
        assignment = alternative["assignment"]
        _selected = ()
        # Bind each active stored FP/bit carrier to its exact checked child
        # outcome. Unit/ratio inactive FP words keep their opaque status.
        names = (
            (("Stoich ratio", "r6", 5122, 5566),)
            if wire == 1
            else (
                ("Blend percent", "s16", 5882, 5546),
                ("Blend gasoline_stoich", "s18", 6312, 5550),
                ("Blend ethanol_stoich", "s20", 6344, 5554),
                ("Blend gasoline_density", "s22", 6376, 5558),
                ("Blend ethanol_density", "s0", 6406, 5562),
            )
            if wire == 2
            else ()
        )
        if wire == 2 and density:
            names += (("Blend density temperature", "r6", 6278, 5566),)
        # A shared source can have both density alternatives; select its step
        # by the already checked ordered assignment, not its field label alone.
        ordered = next(v for v in suffix["Fuel_assignments"] if v is assignment)
        after = ordered["before"]
        paired = []
        wanted = (
            ("Stoich ratio",)
            if wire == 1
            else ("Blend percent", "Blend basis", "Blend origin", "Blend density")
            if wire == 2
            else ()
        )
        if wire == 2 and density:
            wanted += ("Blend density temperature",)
        if wire == 2:
            wanted += (
                "Blend gasoline_stoich",
                "Blend ethanol_stoich",
                "Blend gasoline_density",
                "Blend ethanol_density",
            )
        for name in wanted:
            matches = tuple(
                v
                for v in suffix["transitions"]
                if v["field"] == name
                and (v["before"].decoder if type(v["before"]) is SequenceInterface else v["before"])
                is after
            )
            require(len(matches) == 1, "Sources Fuel carrier loses its same ordered source child")
            step = matches[0]
            paired.append(step)
            after = (
                step["after"].decoder if type(step["after"]) is SequenceInterface else step["after"]
            )
        require(
            after is assignment["after"], "Sources Fuel carrier ends at another assignment cursor"
        )
        for name, register, load, store in names:
            step = next(v for v in paired if v["field"] == name)
            kind = "some" if type(step["before"]) is SequenceInterface else "success"
            options = tuple(v for v in step["child"]["outcomes"] if v["kind"] == kind)
            require(
                len(options) == 1
                and options[0]["state"] is step["after"]
                and options[0]["value_kind"] == "f32_bits"
                and options[0]["value_domain"] == (0, 0xFFFFFFFF)
                and (4, 4) in options[0]["initialized_result_fields"],
                "Sources Fuel carrier is not its initialized unknown f32 child result",
            )
            source_values.append(
                dict(
                    assignment=assignment,
                    source=step,
                    outcome=options[0],
                    register=register,
                    load=V + load,
                    store=V + store,
                )
            )
    for p in paths["Colors"]:
        carriers.append(("r4", 5538, 5630, preserves("r4", p["instructions"])))
    return dict(
        children=tuple(transfers),
        high_GPR_intervals=tuple(carriers),
        high_FP_intervals=tuple(float_carriers),
        active_Fuel_values=tuple(source_values),
        page_flag_path=tuple(V + off for off in (5194, 5198, 5200, 5204, 5206, 5210)),
        normal_inactive_predication=(V + 5022, V + 5130),
        low_FP_source=(V + 6406, V + 5562, "s0") if low else (),
        low_FP_no_call_path=tuple(low),
        requires_actual_private_entry_and_case_guards=True,
    )
