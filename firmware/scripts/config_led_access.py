"""Closed decoded LED dependency records, not LED producer admission.

These source-bound mode/owner/epoch families preserve facts supplied by the
actual finite producer. Neither construction nor current-byte rebinding proves
successful pattern/bool decoding, retirement, saved carriers or ownership.
The enclosing graph must discharge those obligations before any stack result.

Three roles are closed: successful pattern/aggregate output word writes,
SizeZ same-epoch pattern reads, and SizeZ normal Config-field writes. Payload
padding5/9 stays opaque. Complete body/guard/child or source changes require
fresh finite-contract review; byte rebinding never extends these roles.
"""

from dataclasses import dataclass, asdict
from typing import TYPE_CHECKING
from script_values import object_fields, array_items, string_value, integer_value

if TYPE_CHECKING:
    from config_stack import DiagnosticObjects
from config_regions import Storage, View, require_disjoint
from config_owned_access import _storage, _view
from validation import require

EXPECTED_LED_ROLES = {
    "speed2": ("result_words",),
    "sizes": ("result_words",),
    "sizez": ("result_words", "pattern_words", "normal_config_words"),
}
SITE_SHAPES = {
    ("result_words", "speed2"): (
        (0x142, "c4f80200", 2, "settings"),
        (0x14C, "c4f80600", 6, "settings"),
    ),
    ("result_words", "sizes"): (
        (0xFC, "c4f80200", 2, "settings"),
        (0x10A, "c4f806a0", 6, "settings"),
    ),
    ("result_words", "sizez"): ((0xBA, "c4f80600", 6, "pattern"), (0xC2, "c4f80200", 2, "pattern")),
    ("pattern_words", "sizez"): (
        (0x1A9E, "ddf82aa4", 2, "normal"),
        (0x1AA2, "ddf82e94", 6, "normal"),
        (0x1ACE, "ddf82a04", 2, "warning"),
        (0x1AD4, "ddf82e04", 6, "warning"),
    ),
    ("normal_config_words", "sizez"): (
        (0x1B7A, "42f80ca0", 2, "settings"),
        (0x1BC4, "c0f80490", 6, "settings"),
    ),
}
_PARENTS = {
    ("result_words", "speed2"): (
        "_RINvNtCs_16dashboard_config15decode_in_place4nextNtNtB4_3led11LedSettingsINtNtNtCs_8postcard2de12deserializer9SeqAccessNtNtB1p_7flavors5SliceEEB4_",
        368,
        80,
    ),
    ("result_words", "sizes"): (
        "_RINvXNvNtCs_16dashboard_config3leds3_1__NtB5_11LedSettingsNtNtCs_10serde_core2de11Deserialize11deserializeQINtNtNtCs_8postcard2de12deserializer12DeserializerNtNtB28_7flavors5SliceEEB7_",
        300,
        80,
    ),
    ("result_words", "sizez"): (
        "_RINvYINtNtNtCs_8postcard2de12deserializer9SeqAccessNtNtB8_7flavors5SliceENtNtCs_10serde_core2de9SeqAccess12next_elementNtNtCs_16dashboard_config3led10LedPatternEB2j_",
        200,
        48,
    ),
    ("pattern_words", "sizez"): (
        "_RINvXs_NtCs_16dashboard_config15decode_in_placeNtB5_10ConfigSeedNtNtCs_10serde_core2de7Visitor9visit_seqINtNtNtCs_8postcard2de12deserializer9SeqAccessNtNtB25_7flavors5SliceEEB7_",
        7128,
        1608,
    ),
}
_PARENTS["normal_config_words", "sizez"] = _PARENTS["pattern_words", "sizez"]
_LOADERS = {"speed2": 1288, "sizes": 696, "sizez": 456}
_SOURCE = "decoded_LED_source_success_still_requires_actual_producer"


@dataclass(frozen=True)
class LedContext:
    mode: str
    epoch: str
    loader: Storage
    frame: Storage
    config: View
    owner: View
    protected: tuple[Storage, ...]

    def __post_init__(self) -> None:
        require(
            isinstance(self.mode, str)
            and self.mode in _LOADERS
            and isinstance(self.epoch, str)
            and self.epoch in ("normal", "warning", "settings"),
            "unknown LED mode or successful source-field epoch",
        )
        phase = "decoded_led_" + self.epoch
        require(
            isinstance(self.loader, Storage)
            and self.loader.space.startswith("stack:")
            and self.loader.extent == _LOADERS[self.mode]
            and self.loader.alignment >= 8
            and self.loader.writable
            and self.loader.phase == phase,
            "LED loses its actual loader activation/epoch",
        )
        require(
            isinstance(self.frame, Storage)
            and self.frame.space == self.loader.space
            and self.frame.phase == phase
            and self.frame.alignment >= 8
            and self.frame.writable,
            "LED lacks its actual child/visitor frame",
        )
        require(
            isinstance(self.config, View)
            and not self.config.writable
            and self.config.offset == 0
            and self.config.extent == 14576
            and self.config.storage.space == "address"
            and self.config.storage.identity == "private unpublished ConfigSlot"
            and self.config.storage.extent == 14604
            and self.config.storage.phase == phase
            and self.config.storage.alignment >= 4
            and self.config.storage.writable
            and 0x20000000 <= self.config.start < self.config.start + 14604 <= 0x20080000,
            "LED lacks its exact unpublished initialized Config allocation",
        )
        require(
            isinstance(self.owner, View)
            and self.owner.storage.phase == phase
            and isinstance(self.protected, tuple)
            and 1 <= len(self.protected) <= 16
            and all(
                isinstance(v, Storage) and v.phase == phase and not v.writable
                for v in self.protected
            ),
            "LED loses output lifetime or bounded readonly protection",
        )
        require_disjoint((self.loader, self.frame, self.config.storage))
        require(
            len({(v.space, v.start, v.extent) for v in self.protected}) == len(self.protected),
            "LED duplicates a protected interval",
        )
        owned = Storage(
            self.owner.storage.space,
            "LED exact owner interval",
            self.owner.start,
            self.owner.extent,
            1,
            phase,
            False,
        )
        for v in self.protected:
            require_disjoint((owned, v))


@dataclass(frozen=True)
class LedFamily:
    role: str
    mode: str
    parent: int
    sites: tuple[tuple[int, str, int, str], ...]
    contexts: tuple[LedContext, ...]

    def __post_init__(self) -> None:
        require(
            isinstance(self.role, str)
            and isinstance(self.mode, str)
            and (self.role, self.mode) in SITE_SHAPES,
            "unreviewed decoded LED family/mode",
        )
        _, extent, frame = _PARENTS[self.role, self.mode]
        require(
            type(self.parent) is int
            and self.parent % 2 == 0
            and 0x10000000 <= self.parent < self.parent + extent <= 0x101FE000,
            "LED typed code parent wraps or leaves XIP",
        )
        require(
            isinstance(self.sites, tuple)
            and 1 <= len(self.sites) <= 6
            and all(
                isinstance(v, tuple)
                and len(v) == 4
                and type(v[0]) is int
                and isinstance(v[1], str)
                and type(v[2]) is int
                and isinstance(v[3], str)
                for v in self.sites
            ),
            "malformed decoded LED sites",
        )
        require(
            tuple((at - self.parent, raw, offset, epoch) for at, raw, offset, epoch in self.sites)
            == SITE_SHAPES[self.role, self.mode],
            "LED code position/operation/field/epoch pairing differs",
        )
        epochs = (
            ("normal", "warning")
            if self.mode == "sizez" and self.role != "normal_config_words"
            else ("settings",)
        )
        require(
            isinstance(self.contexts, tuple)
            and len(self.contexts) == len(epochs)
            and all(isinstance(v, LedContext) and v.mode == self.mode for v in self.contexts)
            and tuple(v.epoch for v in self.contexts) == epochs,
            "LED loses separate successful source epochs",
        )
        physical = []
        guards: tuple[tuple[int, int], ...]
        for c in self.contexts:
            p = c.loader.start
            require(
                c.frame.extent == frame
                and c.frame.start == p - (1656 if frame == 48 else 1608 if frame == 1608 else 80),
                "LED has another access-parent SP/frame",
            )
            if self.mode != "sizez":
                require(
                    c.owner.storage == c.loader
                    and c.owner.offset == (996 if self.mode == "speed2" else 408)
                    and c.owner.extent == 26
                    and c.owner.writable,
                    "LED aggregate is not its separate emitted E26 output",
                )
                guards = ((c.frame.start + 44, 36), (p + _LOADERS[self.mode] - 104, 104))
            elif self.role == "normal_config_words":
                require(
                    c.owner.storage == c.config.storage
                    and c.owner.offset == 14544
                    and c.owner.extent == 10
                    and c.owner.writable,
                    "LED normal Config owner substituted warning or another source field",
                )
                guards = ((p - 96, 96), (p + 352, 104))
            else:
                v = c.owner.storage
                require(
                    v.space == c.loader.space
                    and v.start == p - 1608
                    and v.extent == 1608
                    and v.alignment >= 8
                    and v.writable
                    and c.owner.offset == 1064
                    and c.owner.extent == 10
                    and c.owner.start % 4 == 0
                    and c.owner.writable == (self.role == "result_words"),
                    "LED pattern output is not its exact T10 visitor epoch",
                )
                if self.role == "pattern_words":
                    require(v == c.frame, "LED reads name another visitor activation")
                else:
                    require_disjoint((v, c.frame, c.loader))
                guards = ((p - 96, 96), (p + 352, 104)) + (
                    ((p - 1640, 32),) if self.role == "result_words" else ()
                )
            for at, size in guards:
                require(
                    sum(
                        v.space == c.loader.space and v.start == at and v.extent == size
                        for v in c.protected
                    )
                    == 1,
                    "LED omits a complete parent/child/ancestor save interval",
                )
            physical.append(
                (
                    c.loader.space,
                    c.loader.identity,
                    p,
                    c.loader.extent,
                    c.frame.identity,
                    c.frame.start,
                    c.frame.extent,
                    c.config.start,
                    c.owner.storage.space,
                    c.owner.storage.identity,
                    c.owner.start,
                    c.owner.extent,
                )
            )
        require(
            all(v == physical[0] for v in physical),
            "LED sequential epochs mix actual owners or activations",
        )

    def footprints(self, epoch: str) -> tuple[tuple[int, int], ...]:
        require(
            isinstance(epoch, str) and epoch in tuple(v.epoch for v in self.contexts),
            "LED read before success or after retirement",
        )
        c = next(v for v in self.contexts if v.epoch == epoch)
        return tuple(
            (c.owner.start + offset, c.owner.start + offset + 4)
            for at, raw, offset, which in self.sites
            if which in (epoch, "pattern")
        )

    def to_record(self) -> dict[str, object]:
        record: dict[str, object] = asdict(self)
        record["sites"] = [
            dict(site=at, encoding=raw, offset=o, epoch=e) for at, raw, o, e in self.sites
        ]
        record["contexts"] = [
            dict(
                mode=c.mode,
                epoch=c.epoch,
                loader=asdict(c.loader),
                frame=asdict(c.frame),
                config=asdict(c.config),
                owner=asdict(c.owner),
                protected=[asdict(v) for v in c.protected],
            )
            for c in self.contexts
        ]
        record.update(
            kind="decoded_led_access_family",
            operation="ordinary_single_LDR"
            if self.role == "pattern_words"
            else "ordinary_single_STR",
            direction="read" if self.role == "pattern_words" else "write",
            width=4,
            source_phase=_SOURCE,
            source_outcome="same_epoch_complete_kind_0_to_3_pattern"
            if self.mode == "sizez" and self.role != "normal_config_words"
            else "complete_two_patterns_and_five_bools",
            opaque_payload_padding=[5, 9],
        )
        return record


def read_led_family(value: object) -> LedFamily:
    record = object_fields(value, "malformed LedFamily record")
    keys = set(LedFamily.__dataclass_fields__) | {
        "kind",
        "operation",
        "direction",
        "width",
        "source_phase",
        "source_outcome",
        "opaque_payload_padding",
    }
    require(
        isinstance(record, dict)
        and set(record) == keys
        and record["kind"] == "decoded_led_access_family"
        and isinstance(record["sites"], list)
        and 1 <= len(array_items(record["sites"], "malformed sites")) <= 6
        and isinstance(record["contexts"], list)
        and 1 <= len(array_items(record["contexts"], "malformed contexts")) <= 2,
        "malformed LED dependency record",
    )
    contexts = []
    for raw_context in array_items(record["contexts"], "malformed contexts"):
        c = object_fields(raw_context, "malformed context")
        require(
            isinstance(c, dict)
            and set(c) == set(LedContext.__dataclass_fields__)
            and isinstance(c["protected"], list)
            and 1 <= len(array_items(c["protected"], "malformed protected")) <= 16,
            "malformed LED context",
        )
        contexts.append(
            LedContext(
                string_value(c["mode"], "malformed mode"),
                string_value(c["epoch"], "malformed epoch"),
                _storage(c["loader"]),
                _storage(c["frame"]),
                _view(c["config"]),
                _view(c["owner"]),
                tuple(_storage(v) for v in array_items(c["protected"], "malformed protected")),
            )
        )
    sites = []
    for raw_site in array_items(record["sites"], "malformed sites"):
        v = object_fields(raw_site, "malformed site")
        require(
            isinstance(v, dict) and set(v) == {"site", "encoding", "offset", "epoch"},
            "malformed LED site",
        )
        sites.append(
            (
                integer_value(v["site"], "malformed site"),
                string_value(v["encoding"], "malformed encoding"),
                integer_value(v["offset"], "malformed offset"),
                string_value(v["epoch"], "malformed epoch"),
            )
        )
    value = LedFamily(
        string_value(record["role"], "malformed role"),
        string_value(record["mode"], "malformed mode"),
        integer_value(record["parent"], "malformed parent"),
        tuple(sites),
        tuple(contexts),
    )
    require(
        type(record["width"]) is int
        and isinstance(record["opaque_payload_padding"], list)
        and all(
            type(v) is int
            for v in array_items(record["opaque_payload_padding"], "malformed padding")
        )
        and record == value.to_record(),
        "LED dependency loses exact operation/source/padding requirements",
    )
    return value


def current_led_family(objects: "DiagnosticObjects", value: LedFamily) -> LedFamily:
    """Current byte/role binding only; does not reconstruct source success."""
    from config_entry import CONFIG, entry_object
    from config_stack import typed_function
    from llvm_stack import mapping_ranges

    require(isinstance(value, LedFamily), "missing typed decoded LED family")
    role, extent, frame = _PARENTS[value.role, value.mode]
    actual_extent, actual_frame, name = typed_function(objects, value.parent, role)
    require(
        (actual_extent, actual_frame) == (extent, frame),
        "LED current function extent/frame differs",
    )
    mapping = mapping_ranges(objects.elf, name)
    for c in value.contexts:
        entry_object(objects, c.config.start, dict(kind="entry_static", role=CONFIG))
    for at, raw, offset, epoch in value.sites:
        require(
            any(kind == "t" and lo <= at < at + 4 <= hi for lo, hi, kind in mapping),
            "LED operation is not wholly mapped Thumb code",
        )
        require(
            objects.read_only(at, 4) == bytes.fromhex(raw),
            "LED operation bytes differ from current artifact",
        )
    return value
