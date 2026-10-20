"""Closed owned-access facts carried by the conditional platform policy.

These internal records preserve the result of finite source/parent analysis;
constructing or reading one does not prove pointer transport, exclusive origin,
coverage or runtime memory attributes. The complete graph producer must bind
its internally expected tuple after those interfaces close. Partial helper
requirements remain unresolved and cannot be converted by a JSON consumer.
"""

from dataclasses import dataclass, asdict
from typing import TYPE_CHECKING
from script_values import object_fields, array_items, string_value, integer_value

if TYPE_CHECKING:
    from config_stack import DiagnosticObjects
from config_regions import Storage, View, require_writes
from validation import require


# Only the actually reviewed ordinary STR families. No generic mnemonic or
# user-selected width/addressing-mode extension is accepted here.
# Tuples are window/body offset, source output extent, field offset, retained
# receiver register and coordinate kind. Source meanings: advanced.rs's four
# adjacent default fields; defaults.rs's Config version pair; appearance.rs's
# warning/background Color pair in each default Widget. A compiler/source/ABI
# change needs fresh complete finite-family and parent interface review.
ROLES = {
    "advanced_default_fields": (84, 64, 57, "r4", "stack"),
    "configuration_versions": (28, 14576, 14570, "r8", "address"),
    "widget_colors": (670, 220, 182, "r4", "widget"),
}


@dataclass(frozen=True)
class OwnedAccess:
    role: str
    anchor: int
    site: int
    activation: str
    owner: View
    frame: Storage
    receiver_register: str
    protected: tuple[Storage, ...]
    loop_index: int | None = None

    def __post_init__(self) -> None:
        require(
            isinstance(self.role, str) and self.role in ROLES, "unreviewed owned single-access role"
        )
        offset, extent, access, receiver, space = ROLES[self.role]
        require(
            type(self.anchor) is int
            and type(self.site) is int
            and 0x10000000 <= self.anchor < self.site == self.anchor + offset
            and self.site + 4 <= 0x101FE000
            and self.anchor % 2 == 0,
            "owned access has another current instruction/window",
        )
        require(
            isinstance(self.activation, str) and 0 < len(self.activation) <= 128,
            "owned access loses its activation",
        )
        require(
            isinstance(self.frame, Storage)
            and self.frame.space.startswith("stack:")
            and self.frame.writable
            and self.frame.extent == 456
            and self.frame.alignment >= 8
            and self.frame.phase == "default_refill",
            "owned access loses its actual loader frame/phase",
        )
        require(
            isinstance(self.owner, View)
            and self.owner.writable
            and self.owner.extent == extent
            and self.owner.storage.phase == self.frame.phase
            and self.owner.storage.alignment >= 4
            and self.owner.start % 4 == 0,
            "owned access lacks its complete mutable current owner",
        )
        require(
            self.receiver_register == receiver, "owned access loses its matched receiver register"
        )
        if space == "stack":
            require(
                self.owner.storage.space == self.frame.space
                and self.owner.storage == self.frame
                and self.frame.start
                <= self.owner.start
                < self.owner.start + extent
                <= self.frame.start + 352,
                "AdvancedDisplay output is not in its current parent local epoch",
            )
        else:
            require(
                self.owner.storage.space == "address"
                and (self.owner.storage.identity, self.owner.storage.extent)
                == ("private unpublished ConfigSlot", 14604)
                and 0x20000000
                <= self.owner.storage.start
                < self.owner.storage.start + 14604
                <= 0x20080000,
                "Config output loses the private slot identity/view",
            )
            if space == "address":
                require(self.owner.offset == 0, "Config view starts inside another object")
            else:
                require(
                    type(self.loop_index) is int
                    and 0 <= self.loop_index < 8
                    and self.owner.offset == 444 + 220 * self.loop_index,
                    "Widget owner loses paired loop index/offset",
                )
        if space != "widget":
            require(self.loop_index is None, "non-loop store borrows a Widget index")
        require(
            isinstance(self.protected, tuple)
            and 1 <= len(self.protected) <= 16
            and all(isinstance(v, Storage) for v in self.protected),
            "owned access loses bounded typed protection",
        )
        require(
            any(
                v.space == self.frame.space
                and v.start == self.frame.start + 352
                and v.extent == 104
                and v.phase == self.frame.phase
                and not v.writable
                for v in self.protected
            ),
            "owned access omits loader saves",
        )
        require(
            all(v.phase == self.frame.phase for v in self.protected),
            "owned access mixes protected epochs",
        )
        require_writes((self.owner,), ((self.owner, 0, extent),), self.protected)
        require(access + 4 <= extent, "owned store exceeds its source object")

    def to_record(self) -> dict[str, object]:
        record: dict[str, object] = asdict(self)
        record["protected"] = [asdict(v) for v in self.protected]
        record.update(
            operation="ordinary_single_STR",
            direction="write",
            width=4,
            source_phase=self.frame.phase,
            access_offset=ROLES[self.role][2],
            receiver_start=self.owner.start - (2204 if self.role == "widget_colors" else 0),
        )
        return record


def _boolean(value: object, message: str) -> bool:
    if type(value) is not bool:
        raise AssertionError(message)
    return value


def _optional_int(value: object) -> int | None:
    return None if value is None else integer_value(value, "malformed optional integer")


def _integer_triple(value: object) -> tuple[int, int, int]:
    items = array_items(value, "malformed integer triple")
    require(len(items) == 3, "malformed integer triple")
    return _tuple_member(items, 0), _tuple_member(items, 1), _tuple_member(items, 2)


def _integer_quadruple(value: object) -> tuple[int, int, int, int]:
    items = array_items(value, "malformed integer quadruple")
    require(len(items) == 4, "malformed integer quadruple")
    return (
        _tuple_member(items, 0),
        _tuple_member(items, 1),
        _tuple_member(items, 2),
        _tuple_member(items, 3),
    )


def _tuple_member(items: list[object], index: int) -> int:
    return integer_value(items[index], "malformed integer tuple member")


def _storage(value: object) -> Storage:
    record = object_fields(value, "malformed owned storage")
    require(set(record) == set(Storage.__dataclass_fields__), "malformed owned storage")
    return Storage(
        string_value(record["space"], "malformed storage space"),
        string_value(record["identity"], "malformed storage identity"),
        integer_value(record["start"], "malformed storage start"),
        integer_value(record["extent"], "malformed storage extent"),
        integer_value(record["alignment"], "malformed storage alignment"),
        string_value(record["phase"], "malformed storage phase"),
        _boolean(record["writable"], "malformed storage mutability"),
    )


def _view(value: object) -> View:
    record = object_fields(value, "malformed owned view")
    require(set(record) == set(View.__dataclass_fields__), "malformed owned view")
    return View(
        _storage(record["storage"]),
        integer_value(record["offset"], "malformed view offset"),
        integer_value(record["extent"], "malformed view extent"),
        _boolean(record["writable"], "malformed view mutability"),
    )


def read_owned_access(value: object) -> OwnedAccess:
    record = object_fields(value, "malformed OwnedAccess record")
    keys = set(OwnedAccess.__dataclass_fields__) | {
        "operation",
        "direction",
        "width",
        "source_phase",
        "access_offset",
        "receiver_start",
    }
    require(
        isinstance(record, dict)
        and set(record) == keys
        and isinstance(record["protected"], list)
        and 1 <= len(array_items(record["protected"], "malformed protected")) <= 16,
        "missing/extra owned dependency fields",
    )
    value = OwnedAccess(
        string_value(record["role"], "malformed role"),
        integer_value(record["anchor"], "malformed anchor"),
        integer_value(record["site"], "malformed site"),
        string_value(record["activation"], "malformed activation"),
        _view(record["owner"]),
        _storage(record["frame"]),
        string_value(record["receiver_register"], "malformed receiver_register"),
        tuple(_storage(v) for v in array_items(record["protected"], "malformed protected")),
        _optional_int(record["loop_index"]),
    )
    require(
        type(record["width"]) is int
        and type(record["access_offset"]) is int
        and type(record["receiver_start"]) is int
        and record == value.to_record(),
        "owned access operation/footprint/context differs",
    )
    return value


def current_owned_access(objects: "DiagnosticObjects", value: OwnedAccess) -> OwnedAccess:
    """Rebind a producer fact to its artifact; never recreate parent proof."""
    from config_stack import canonical_symbol, _typed_function_extent
    from config_headers import ROOT

    require(isinstance(value, OwnedAccess), "missing typed owned access")
    if value.role == "advanced_default_fields":
        role = "_RNvMs2_NtCs_16dashboard_config8advancedNtB5_15AdvancedDisplay11for_channel"
        extent, _ = _typed_function_extent(objects, value.anchor, role)
        require(extent == 104, "owned AdvancedDisplay family extent changed")
        expected = bytes.fromhex("c4f83900")
    else:
        window_extent = 182 if value.role == "configuration_versions" else 728
        candidates = [
            (n, e)
            for n, e in objects.elf.functions.items()
            if canonical_symbol(n) == ROOT
            and e[0] & ~1 <= value.anchor < value.anchor + window_extent <= (e[0] & ~1) + e[1]
        ]
        require(len(candidates) == 1, "owned prepared window has no unique current loader")
        _typed_function_extent(objects, candidates[0][1][0] & ~1, ROOT)
        from config_entry import CONFIG, entry_object

        entry_object(objects, value.owner.storage.start, dict(kind="entry_static", role=CONFIG))
        expected = bytes.fromhex(
            "48f80010" if value.role == "configuration_versions" else "c4f85209"
        )
    # Exact reviewed instruction bytes, not a complete-body fingerprint or a
    # claim to infer the dynamic receiver/permission from the instruction alone.
    require(
        objects.read_only(value.site, 4) == expected,
        "owned store instruction belongs to another artifact/family",
    )
    return value
