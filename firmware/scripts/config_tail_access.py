"""Three conditional Tail access requirements, never reached-use authority.

The three Screen outputs share one family with ordered call epochs. Matching
these records establishes no initialized value, ordinary continuation,
completion, mutable ownership or runtime memory permission.
"""

from dataclasses import dataclass, asdict
from config_regions import Storage, View, borrow, require_disjoint, require_writes
from config_owned_access import _storage, _view
from config_tail_contracts import ROLES
from validation import require


EXPECTED_TAIL_ROLES = (
    "Screen_output_words",
    "first_Screen_scratch_words",
    "first_Config_Screen_words",
)
SCREEN_EPOCHS = ((5712, 940, 3), (6198, 1000, 2), (6426, 1064, 1))
SITE_SHAPES = {
    "Screen_output_words": (
        (296, "c4f80600", 6, 4, "write", "same_Screen_fields_6_to_10_before_completion"),
        (306, "c4f80200", 2, 4, "write", "same_Screen_fields_2_to_6_before_completion"),
    ),
    "first_Screen_scratch_words": (
        (6496, "ddf8b203", 6, 4, "read", "first_completed_Screen_fields_6_to_10"),
        (6510, "ddf8ae03", 2, 4, "read", "first_completed_Screen_fields_2_to_6"),
    ),
    "first_Config_Screen_words": (
        (6074, "c6f8da20", 6, 4, "write", "first_Screen_staged_fields_6_to_10"),
        (6078, "c6f8d600", 2, 4, "write", "first_Screen_staged_fields_2_to_6"),
    ),
}
_PHASE = "decoded_controls_tail_group"


@dataclass(frozen=True)
class TailAccessContext:
    loader: Storage
    visitor: Storage
    frame: Storage
    config: View
    protected: tuple

    def __post_init__(self):
        require(
            all(
                isinstance(v, Storage)
                and v.space.startswith("stack:")
                and v.phase == _PHASE
                and v.writable
                and v.alignment >= 8
                for v in (self.loader, self.visitor, self.frame)
            ),
            "Tail access loses its typed activation",
        )
        require(
            self.loader.extent == 456
            and self.visitor.extent == 1608
            and self.visitor.start == self.loader.start - 1608
            and self.loader.space == self.visitor.space == self.frame.space
            and (
                self.frame == self.visitor
                or self.frame.extent == 64
                and self.frame.start == self.visitor.start - 64
            ),
            "Tail access changes loader/visitor/child geometry",
        )
        require(
            isinstance(self.config, View)
            and not self.config.writable
            and self.config.offset == 0
            and self.config.extent == 14576
            and self.config.storage.extent == 14604
            and self.config.storage.writable
            and self.config.storage.space == "address"
            and self.config.storage.phase == _PHASE
            and self.config.storage.identity == "private unpublished ConfigSlot"
            and self.config.storage.alignment >= 4
            and 0x20000000 <= self.config.start < self.config.start + 14604 <= 0x20080000,
            "Tail access loses the complete conditional Config allocation",
        )
        require_disjoint((self.loader, self.visitor, self.config.storage))
        # Absolute addresses preserve aliases across coordinate-space labels.
        # Screen64 + Button24 + Action16 + varint32 + pop16 is simultaneous.
        low, high = self.visitor.start - 152, self.loader.start + self.loader.extent
        require(0 <= low < high <= 0x100000000, "Tail physical activation wraps or is relative")
        physical = Storage(
            "address", "conditional Tail descendants and callers", low, high - low, 1, _PHASE, False
        )
        require_disjoint((physical, self.config.storage))
        require(
            type(self.protected) is tuple
            and 1 <= len(self.protected) <= 16
            and all(
                isinstance(v, Storage) and v.phase == _PHASE and not v.writable and v.start >= 0
                for v in self.protected
            ),
            "Tail access loses bounded typed protection",
        )
        require(
            len({(v.space, v.start, v.extent) for v in self.protected}) == len(self.protected),
            "Tail access protection duplicates an interval",
        )
        for at, size in ((self.loader.start + 352, 104), (self.visitor.start + 1512, 96)):
            require(
                sum(
                    v.space == self.loader.space and v.start == at and v.extent == size
                    for v in self.protected
                )
                == 1,
                "Tail access omits a parent saved interval",
            )
        require(
            any(
                v.space == "address"
                and v.start <= self.config.start + 14576
                and self.config.start + 14604 <= v.start + v.extent
                for v in self.protected
            ),
            "Tail access drops metadata/taken/padding protection",
        )


@dataclass(frozen=True)
class TailFamily:
    role: str
    mode: str
    parent: int
    caller: int
    sites: tuple
    context: TailAccessContext
    epochs: tuple

    def __post_init__(self):
        require(
            type(self.role) is str
            and self.role in SITE_SHAPES
            and type(self.mode) is str
            and self.mode == "sizez",
            "unreviewed Tail role/mode",
        )
        child = self.role == EXPECTED_TAIL_ROLES[0]
        extent = 312 if child else 7128
        require(
            type(self.parent) is int
            and self.parent % 2 == 0
            and 0x10000000 <= self.parent < self.parent + extent <= 0x101FE000
            and type(self.caller) is int
            and self.caller % 2 == 0
            and 0x10000000 <= self.caller < self.caller + 7128 <= 0x101FE000
            and (child or self.parent == self.caller),
            "Tail access parent/caller leaves its current extent",
        )
        require(
            type(self.sites) is tuple
            and len(self.sites) == 2
            and all(
                type(v) is tuple
                and len(v) == 6
                and type(v[0]) is int
                and type(v[1]) is str
                and type(v[2]) is int
                and type(v[3]) is int
                and type(v[4]) is str
                and type(v[5]) is str
                for v in self.sites
            )
            and tuple((at - self.parent, *rest) for at, *rest in self.sites)
            == SITE_SHAPES[self.role],
            "Tail access site/width/direction/source pairing differs",
        )
        require(type(self.context) is TailAccessContext, "Tail access has no typed context")
        require(
            type(self.epochs) is tuple
            and all(
                type(v) is tuple and len(v) == 3 and all(type(n) is int for n in v)
                for v in self.epochs
            ),
            "malformed Tail epoch tuple",
        )
        wanted = (
            tuple((self.caller + at, out, count) for at, out, count in SCREEN_EPOCHS)
            if child
            else ()
        )
        require(
            self.epochs == wanted, "Tail access loses its three ordered call/output/count pairs"
        )
        c = self.context
        if child:
            require(
                c.frame.extent == 64 and c.frame.start + 64 == c.visitor.start,
                "Tail access loses its actual Screen frame",
            )
            require_disjoint((c.frame, c.visitor, c.loader, c.config.storage))
            require(
                sum(
                    v.space == c.frame.space and v.start == c.frame.start + 28 and v.extent == 36
                    for v in c.protected
                )
                == 1,
                "Tail access omits Screen saved cells",
            )
        else:
            require(c.frame == c.visitor, "Tail parent access borrows a child activation")
        for index in range(3 if child else 1):
            owner = self.owner(index)
            for at, raw, field, width, direction, label in self.sites:
                require(field + width <= owner.extent, "Tail access escapes its exact owner")
                if direction == "write":
                    require_writes((owner,), ((owner, field, width),), c.protected)
                start = owner.start + field
                require(
                    all(
                        not (start < v.start + v.extent and v.start < start + width)
                        for v in c.protected
                    ),
                    "Tail access intersects protected live storage",
                )

    def owner(self, index):
        require(
            type(index) is int and 0 <= index < (3 if self.epochs else 1),
            "Tail access has a stale call epoch",
        )
        c = self.context
        if self.role == "Screen_output_words":
            return borrow(c.visitor, self.epochs[index][1], 10, _PHASE, True)
        if self.role == "first_Screen_scratch_words":
            return borrow(c.visitor, 940, 10, _PHASE)
        return borrow(c.config.storage, 212, 10, _PHASE, True)

    def footprints(self, index):
        owner = self.owner(index)
        return tuple(
            (owner.start + field, owner.start + field + width)
            for at, raw, field, width, direction, label in self.sites
        )

    def to_record(self):
        c = self.context
        return dict(
            kind="decoded_tail_access_family",
            role=self.role,
            mode=self.mode,
            parent=self.parent,
            caller=self.caller,
            sites=[
                dict(site=a, encoding=raw, offset=o, width=w, direction=d, source_phase=p)
                for a, raw, o, w, d, p in self.sites
            ],
            context=dict(
                loader=asdict(c.loader),
                visitor=asdict(c.visitor),
                frame=asdict(c.frame),
                config=asdict(c.config),
                protected=[asdict(v) for v in c.protected],
            ),
            epochs=[list(v) for v in self.epochs],
            source_phase="actual_Tail_aggregate_still_requires_complete_producer",
            physical_tag_is_completed_Tail=False,
        )


def read_tail_family(record):
    require(
        type(record) is dict
        and set(record)
        == set(TailFamily.__dataclass_fields__)
        | {"kind", "source_phase", "physical_tag_is_completed_Tail"}
        and record["kind"] == "decoded_tail_access_family"
        and type(record["sites"]) is list
        and len(record["sites"]) == 2
        and type(record["epochs"]) is list
        and len(record["epochs"]) in (0, 3),
        "malformed Tail access family record",
    )
    c = record["context"]
    require(
        type(c) is dict
        and set(c) == set(TailAccessContext.__dataclass_fields__)
        and type(c["protected"]) is list
        and 1 <= len(c["protected"]) <= 16,
        "malformed Tail access context",
    )
    context = TailAccessContext(
        _storage(c["loader"]),
        _storage(c["visitor"]),
        _storage(c["frame"]),
        _view(c["config"]),
        tuple(_storage(v) for v in c["protected"]),
    )
    sites = []
    for value in record["sites"]:
        require(
            type(value) is dict
            and set(value) == {"site", "encoding", "offset", "width", "direction", "source_phase"},
            "malformed Tail access site",
        )
        sites.append(
            tuple(
                value[k]
                for k in ("site", "encoding", "offset", "width", "direction", "source_phase")
            )
        )
    require(
        all(type(v) is list and len(v) == 3 for v in record["epochs"]),
        "malformed Tail access epoch",
    )
    result = TailFamily(
        record["role"],
        record["mode"],
        record["parent"],
        record["caller"],
        tuple(sites),
        context,
        tuple(tuple(v) for v in record["epochs"]),
    )
    require(
        record["physical_tag_is_completed_Tail"] is False and record == result.to_record(),
        "Tail requirement loses its exact conditional/partial-state qualification",
    )
    return result


def current_tail_family(objects, value):
    """Current role/extent/mapping/six-site bytes only, never a reached use."""
    from config_entry import CONFIG, entry_object
    from config_headers import VISITOR
    from config_stack import typed_function
    from llvm_stack import mapping_ranges

    require(type(value) is TailFamily, "missing typed Tail family")
    child = value.role == EXPECTED_TAIL_ROLES[0]
    role, extent, frame = (ROLES["ScreenBindings"], 312, 64) if child else (VISITOR, 7128, 1608)
    actual, stack, name = typed_function(objects, value.parent, role)
    require((actual, stack) == (extent, frame), "Tail current parent extent/frame differs")
    caller_extent, caller_frame, _ = typed_function(objects, value.caller, VISITOR)
    require(
        (caller_extent, caller_frame) == (7128, 1608), "Tail current caller extent/frame differs"
    )
    entry_object(objects, value.context.config.start, dict(kind="entry_static", role=CONFIG))
    mapping = mapping_ranges(objects.elf, name)
    for at, raw, offset, width, direction, label in value.sites:
        encoded = bytes.fromhex(raw)
        require(
            any(k == "t" and lo <= at < at + len(encoded) <= hi for lo, hi, k in mapping),
            "Tail operation is not wholly mapped Thumb code",
        )
        require(
            objects.read_only(at, len(encoded)) == encoded,
            "Tail operation bytes differ from current artifact",
        )
    return value
