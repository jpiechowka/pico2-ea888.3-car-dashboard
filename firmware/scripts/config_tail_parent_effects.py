"""Fixed Tail parent store envelopes, before active guarded-path admission."""

from config_regions import Storage, borrow, require_disjoint, require_writes
from config_decode_effects import _object
from config_tail_parent_contracts import GRAMMARS
from validation import require


# Site, coordinate, offset, physical width. Pointer/epoch/source lineage is
# separately bound by the ordered source, staging and publication relations.
PARENT_WRITES = (
    (5646, "V", 292, 4),
    (5650, "V", 828, 8),
    (5698, "V", 1132, 4),
    (5706, "V", 1128, 4),
    (5936, "V", 712, 8),
    (5944, "V", 880, 8),
    (5950, "V", 888, 1),
    (5956, "V", 744, 8),
    (5960, "V", 752, 1),
    (6026, "V", 480, 8),
    (6036, "V", 468, 8),
    (6042, "V", 476, 1),
    (6048, "V", 456, 8),
    (6056, "V", 464, 1),
    (6062, "B", 213, 1),
    (6066, "B", 212, 1),
    (6070, "B", 222, 1),
    (6074, "B", 218, 4),
    (6078, "B", 214, 4),
    (6098, "B", 232, 1),
    (6114, "B", 242, 2),
    (6128, "V", 292, 4),
    (6132, "V", 1068, 4),
    (6136, "V", 1064, 4),
    (6436, "V", 284, 4),
    (6508, "V", 604, 4),
    (6514, "V", 600, 4),
    (6622, "V", 284, 4),
    (6672, "B", 14542, 2),
    (6680, "B", 14540, 2),
    (6690, "B", 14538, 2),
    (6698, "B", 14536, 2),
    (6706, "B", 14534, 2),
    (6714, "B", 14532, 2),
    (6722, "B", 14530, 2),
    (6728, "B", 14528, 1),
    (6732, "B", 14529, 1),
    (6768, "B", 14574, 1),
    (6782, "V", 1000, 8),
    (6866, "V", 280, 4),
    (6872, "V", 284, 4),
    (6906, "V", 276, 4),
    (6932, "V", 272, 4),
    (6958, "V", 268, 4),
    (7026, "B", 14556, 4),
    (7034, "B", 14546, 4),
    (7038, "B", 14568, 1),
    (7046, "B", 14567, 1),
    (7054, "B", 14566, 1),
    (7062, "B", 14565, 1),
    (7068, "B", 14564, 1),
    (7076, "B", 14555, 1),
    (7082, "B", 14554, 1),
    (7088, "B", 14545, 1),
    (7096, "B", 14544, 1),
    (7102, "B", 14560, 4),
    (7108, "B", 14550, 4),
)


def _parent_envelopes(objects, rows, parents, entry, owner):
    from config_decoded_tail import _parents, _TailEntry

    require(type(entry) is _TailEntry, "Tail parent effects lack the actual private entry")
    frame = entry.sequence.header.storage
    V = entry.frontier - 5630
    phase = frame.phase
    require(
        parents == _parents(objects, rows, V, owner.start)
        and type(owner) is Storage
        and owner.space == "address"
        and owner.writable
        and owner.extent == 14604
        and owner.phase == phase,
        "Tail parent effects change current Config/parent",
    )
    require_disjoint(
        (
            owner,
            Storage(
                "address",
                "Tail complete descendants and retained callers",
                frame.start - 152,
                152 + 1608 + 456,
                8,
                phase,
                False,
            ),
        )
    )
    actual = {
        p["function"] - V + off
        for p in parents
        for off, width, text in GRAMMARS["visitor_" + str(p["function"] - V)]["instructions"]
        if text.split(" ", 1)[0].split(".", 1)[0] in ("str", "strh", "strb", "strd", "stm", "vstr")
    }
    require(
        len(PARENT_WRITES) == len({v[0] for v in PARENT_WRITES}) == 57
        and actual == {v[0] for v in PARENT_WRITES},
        "Tail parent store partition is incomplete or duplicated",
    )
    protected = entry.protected + (
        _object(entry.sequence.decoder.header, "stable parent Tail D"),
        _object(entry.sequence.decoder.payload, "immutable-input"),
        _object(borrow(frame, 288, 4, phase), "stable parent Tail Q.D"),
        _object(borrow(frame, 1512, 96, phase), "original Visitor complete saved cells"),
    )
    from config_tail_effects import _guards

    protected = _guards(frame, protected)
    writes = []
    for site, coordinate, offset, size in PARENT_WRITES:
        if coordinate == "V":
            require(0 <= offset < offset + size <= 1512, "Tail parent write escapes owned locals")
            target = frame
        else:
            require(
                212 <= offset < offset + size <= 244
                or 14528 <= offset < offset + size <= 14569
                or (offset, size) == (14574, 1),
                "Tail parent write changes another Config group or settings padding",
            )
            target = owner
        view = borrow(target, offset, size, phase, True)
        require_writes((view,), ((view, 0, size),), protected)
        # Ordinary potentially unaligned singles retain exact access-policy
        # obligations. Multiple-register/local transfers always need align4.
        op = rows[V + site].mnemonic.split(".", 1)[0]
        if coordinate == "V" and op in ("str", "strd", "stm", "vstr"):
            require(
                view.start % 4 == 0, "Tail local word/multiple transfer lacks actual alignment4"
            )
        writes.append((V + site, view, coordinate))
    require(
        sum(v[1] == "V" for v in PARENT_WRITES) == 27
        and sum(v[1] == "B" for v in PARENT_WRITES) == 30,
        "Tail parent coordinate partition changed",
    )
    return dict(
        stores=57,
        local_stores=27,
        Config_stores=30,
        writes=tuple(writes),
        requires_actual_source_control_pointer_epochs_and_child_effects=True,
        confers_active_state=False,
    )
