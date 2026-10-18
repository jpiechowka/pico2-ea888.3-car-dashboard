"""Four closed Widget access requirements carried by conditional policies.

Records preserve internally derived owner/loop/epoch facts. Constructing or
reading them proves no decoding, source exclusivity, completion or observation
permission. The complete producer must close those relations separately.
The bounded affine loops remain one family each, never 64 context copies.
"""

from dataclasses import dataclass, asdict
from config_regions import Storage, View, require_disjoint, require_writes
from config_owned_access import _storage, _view
from validation import require

EXPECTED_WIDGET_ROLES = (
    "graph_payload",
    "band_assembly",
    "widget_assembly",
    "advanced_initializer",
)
_VISITOR = "_RINvXs_NtCs_16dashboard_config15decode_in_placeNtB5_10ConfigSeedNtNtCs_10serde_core2de7Visitor9visit_seqINtNtNtCs_8postcard2de12deserializer9SeqAccessNtNtB25_7flavors5SliceEEB7_"
_ADVANCED = "_RNvMs2_NtCs_16dashboard_config8advancedNtB5_15AdvancedDisplay11for_channel"
# (code offset, exact encoding, field offset, width, direction, source phase).
SITE_SHAPES = {
    "graph_payload": ((1230, "0568", 1, 4, "read", "Auto_opaque_or_Fixed_min_max"),),
    "band_assembly": (
        (3072, "a0f80110", 1, 2, "write", "decoded_min_before_count"),
        (3094, "c0f80b10", 11, 4, "write", "normalized_label_after_count"),
    ),
    "widget_assembly": (
        (3782, "a5f8c521", 9, 2, "write", "paired_graph_max"),
        (3786, "c5f8d101", 21, 4, "write", "decoded_gauge_min_max"),
        (3792, "c5f8c101", 5, 4, "write", "paired_graph_min_max"),
        (3880, "c5f8d501", 25, 4, "write", "decoded_gauge_max_kind"),
    ),
    "advanced_initializer": (
        (84, "c4f83900", 57, 4, "write", "Boost_false_before_decoded_overrides"),
    ),
}
_LAYOUT = {
    "graph_payload": (940, 8, 0, 0, 0),
    "band_assembly": (1068, 48, 16, 0, 2),
    "widget_assembly": (444, 14080, 220, 0, 63),
    "advanced_initializer": (1064, 64, 0, 0, 0),
}
_PHASE = "decoded_widget_group"


@dataclass(frozen=True)
class WidgetContext:
    loader: Storage
    visitor: Storage
    frame: Storage
    config: View
    owner: View
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
            "Widget access loses actual parent/child activation",
        )
        require(
            self.loader.extent == 456
            and self.visitor.extent == 1608
            and self.visitor.space == self.frame.space == self.loader.space
            and self.visitor.start == self.loader.start - 1608,
            "Widget access has another loader/visitor geometry",
        )
        require(
            isinstance(self.config, View)
            and not self.config.writable
            and self.config.offset == 0
            and self.config.extent == 14576
            and self.config.storage.space == "address"
            and self.config.storage.identity == "private unpublished ConfigSlot"
            and self.config.storage.phase == _PHASE
            and self.config.storage.extent == 14604
            and self.config.storage.alignment >= 4
            and self.config.storage.writable
            and 0x20000000 <= self.config.start < self.config.start + 14604 <= 0x20080000,
            "Widget access lacks the same complete private Config allocation",
        )
        require(
            isinstance(self.owner, View)
            and self.owner.storage.phase == _PHASE
            and isinstance(self.protected, tuple)
            and 1 <= len(self.protected) <= 16
            and all(
                isinstance(v, Storage) and v.phase == _PHASE and not v.writable
                for v in self.protected
            ),
            "Widget access loses typed owner or bounded protection",
        )
        require_disjoint((self.loader, self.visitor, self.config.storage))
        require(
            len({(v.space, v.start, v.extent) for v in self.protected}) == len(self.protected),
            "Widget protection duplicates an interval",
        )
        for at, size in ((self.loader.start + 352, 104), (self.visitor.start + 1512, 96)):
            require(
                sum(
                    v.space == self.loader.space and v.start == at and v.extent == size
                    for v in self.protected
                )
                == 1,
                "Widget access omits parent/visitor saves",
            )
        require(
            any(
                v.space == "address"
                and v.start <= self.config.start + 14576
                and self.config.start + 14604 <= v.start + v.extent
                for v in self.protected
            ),
            "Widget access drops metadata/taken/padding protection",
        )
        interval = Storage(
            self.owner.storage.space,
            "Widget exact access owner",
            self.owner.start,
            self.owner.extent,
            1,
            _PHASE,
            False,
        )
        for guard in self.protected:
            require_disjoint((interval, guard))


@dataclass(frozen=True)
class WidgetFamily:
    role: str
    mode: str
    parent: int
    sites: tuple
    context: WidgetContext
    loop: tuple

    def __post_init__(self):
        require(
            isinstance(self.role, str) and self.role in SITE_SHAPES and self.mode == "sizez",
            "unreviewed Widget access role/mode",
        )
        size = 104 if self.role == "advanced_initializer" else 7128
        require(
            type(self.parent) is int
            and self.parent % 2 == 0
            and 0x10000000 <= self.parent < self.parent + size <= 0x101FE000,
            "Widget code parent wraps or leaves XIP",
        )
        require(
            isinstance(self.sites, tuple)
            and len(self.sites) == len(SITE_SHAPES[self.role])
            and all(
                isinstance(v, tuple)
                and len(v) == 6
                and type(v[0]) is int
                and isinstance(v[1], str)
                and type(v[2]) is int
                and type(v[3]) is int
                and isinstance(v[4], str)
                and isinstance(v[5], str)
                for v in self.sites
            ),
            "malformed bounded Widget access sites",
        )
        require(
            tuple((a - self.parent, *v) for a, *v in self.sites) == SITE_SHAPES[self.role],
            "Widget code/width/direction/field/source pairing differs",
        )
        require(
            isinstance(self.context, WidgetContext), "Widget access has no typed invocation context"
        )
        c = self.context
        offset, extent, stride, lo, hi = _LAYOUT[self.role]
        require(
            isinstance(self.loop, tuple)
            and all(type(v) is int for v in self.loop)
            and self.loop == (lo, hi, stride),
            "Widget access loses its exact source-bounded affine loop",
        )
        base = c.config.storage if self.role == "widget_assembly" else c.visitor
        require(
            c.owner.storage == base
            and c.owner.offset == offset
            and c.owner.extent == extent
            and c.owner.writable == (self.role != "graph_payload"),
            "Widget access uses another field/owner/epoch",
        )
        if self.role == "advanced_initializer":
            require(
                c.frame.start == c.visitor.start - 72 and c.frame.extent == 72,
                "Widget initializer has another child frame",
            )
            require_disjoint((c.frame, c.visitor, c.loader))
            require(
                sum(
                    v.space == c.frame.space and v.start == c.frame.start + 48 and v.extent == 24
                    for v in c.protected
                )
                == 1,
                "Widget initializer omits its saved return/register cells",
            )
        else:
            require(c.frame == c.visitor, "Widget access is not in its actual visitor activation")
        # Check complete family footprints, including the last loop instance.
        # The source/epoch proof is still owed by the actual producer.
        for index in (lo, hi):
            for at, raw, field, width, direction, phase in self.sites:
                require(
                    field + stride * index + width <= extent,
                    "Widget access escapes its bounded owner",
                )
                if direction == "write":
                    require_writes(
                        (c.owner,), ((c.owner, field + stride * index, width),), c.protected
                    )

    def footprints(self, index):
        lo, hi, stride = self.loop
        require(
            type(index) is int and lo <= index <= hi,
            "Widget access uses a stale/out-of-capacity iteration",
        )
        return tuple(
            (
                self.context.owner.start + index * stride + offset,
                self.context.owner.start + index * stride + offset + width,
            )
            for at, raw, offset, width, direction, phase in self.sites
        )

    def to_record(self):
        c = self.context
        return dict(
            kind="decoded_widget_access_family",
            role=self.role,
            mode=self.mode,
            parent=self.parent,
            sites=[
                dict(site=a, encoding=raw, offset=o, width=w, direction=d, source_phase=p)
                for a, raw, o, w, d, p in self.sites
            ],
            context=dict(
                loader=asdict(c.loader),
                visitor=asdict(c.visitor),
                frame=asdict(c.frame),
                config=asdict(c.config),
                owner=asdict(c.owner),
                protected=[asdict(v) for v in c.protected],
            ),
            loop=list(self.loop),
            source_phase="actual_Widget_aggregate_still_requires_complete_producer",
            physical_count_is_completed_prefix=False,
        )


def read_widget_family(record):
    require(
        isinstance(record, dict)
        and set(record)
        == set(WidgetFamily.__dataclass_fields__)
        | {"kind", "source_phase", "physical_count_is_completed_prefix"}
        and record["kind"] == "decoded_widget_access_family"
        and isinstance(record["sites"], list)
        and 1 <= len(record["sites"]) <= 6
        and isinstance(record["loop"], list)
        and len(record["loop"]) == 3,
        "malformed Widget dependency record",
    )
    c = record["context"]
    require(
        isinstance(c, dict)
        and set(c) == set(WidgetContext.__dataclass_fields__)
        and isinstance(c["protected"], list)
        and 1 <= len(c["protected"]) <= 16,
        "malformed Widget dependency context",
    )
    context = WidgetContext(
        _storage(c["loader"]),
        _storage(c["visitor"]),
        _storage(c["frame"]),
        _view(c["config"]),
        _view(c["owner"]),
        tuple(_storage(v) for v in c["protected"]),
    )
    sites = []
    for v in record["sites"]:
        require(
            isinstance(v, dict)
            and set(v) == {"site", "encoding", "offset", "width", "direction", "source_phase"},
            "malformed Widget dependency site",
        )
        sites.append(
            tuple(
                v[k] for k in ("site", "encoding", "offset", "width", "direction", "source_phase")
            )
        )
    result = WidgetFamily(
        record["role"],
        record["mode"],
        record["parent"],
        tuple(sites),
        context,
        tuple(record["loop"]),
    )
    require(
        record["physical_count_is_completed_prefix"] is False and record == result.to_record(),
        "Widget requirement loses exact source/partial-state qualification",
    )
    return result


def current_widget_family(objects, value):
    """Bind current bytes/roles only; no source or mutable-owner permission."""
    from config_entry import CONFIG, entry_object
    from config_stack import typed_function
    from llvm_stack import mapping_ranges

    require(isinstance(value, WidgetFamily), "missing typed decoded Widget family")
    role, size, frame = (
        (_ADVANCED, 104, 72) if value.role == "advanced_initializer" else (_VISITOR, 7128, 1608)
    )
    extent, actual, name = typed_function(objects, value.parent, role)
    require((extent, actual) == (size, frame), "Widget current code extent/frame differs")
    entry_object(objects, value.context.config.start, dict(kind="entry_static", role=CONFIG))
    mapping = mapping_ranges(objects.elf, name)
    for at, raw, offset, width, direction, phase in value.sites:
        encoded = bytes.fromhex(raw)
        require(
            any(k == "t" and lo <= at < at + len(encoded) <= hi for lo, hi, k in mapping),
            "Widget operation is not wholly mapped Thumb code",
        )
        require(
            objects.read_only(at, len(encoded)) == encoded,
            "Widget operation bytes differ from current artifact",
        )
    return value
