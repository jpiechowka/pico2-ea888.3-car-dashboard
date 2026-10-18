"""Closed validator access-family facts; no source or producer permission.

These records preserve already established caller/owner/loop facts. Their
construction and JSON readers cannot prove source-valid Config, pointer/capture
transport, typed Result, guards, initialization, child effects or full coverage.
Those remain mandatory in the separate actual finite graph producer.
"""

from dataclasses import dataclass, asdict
import re
from config_regions import Storage, View, borrow, require_writes
from config_owned_access import _storage, _view
from validation import require

# Reviewed mode-specific physical layouts, not universal Rust enum/return ABI.
# frame, local envelope, first, capture, loader frame, default/decoded Result,
# current complete validator extent. Code/data addresses remain relocatable.
LAYOUTS = {
    "speed2": (296, 200, 84, 96, 1288, (156, 996), 6908),
    "sizes": (288, 184, 68, 80, 696, (100, 408), 4276),
    "sizez": (320, 216, 76, 88, 456, (236, 112), 5048),
}
PHASES = ("default_validation", "decoded_validation")
OPERATIONS = {
    "first_word": ("ordinary_single_STR", "write", 4),
    "first_reads": ("ordinary_single_LDR", "read", 4),
    "result_writes": ("ordinary_single_STR", "write", 4),
    "control_buttons": ("ordinary_single_LDR", "read", 4),
    "first_halfwords": ("ordinary_single_STRH", "write", 2),
    "y_action_pairs": ("ordinary_single_STRH", "write", 2),
    "widget_placement": ("ordinary_single_LDRH", "read", 2),
}


@dataclass(frozen=True)
class ValidatorContext:
    mode: str
    phase: str
    caller: Storage
    frame: Storage
    config: View
    result: View
    protected: tuple

    def __post_init__(self):
        require(
            isinstance(self.mode, str) and self.mode in LAYOUTS and self.phase in PHASES,
            "unreviewed validator context",
        )
        extent, local, first, capture, caller, outputs, code = LAYOUTS[self.mode]
        require(
            isinstance(self.caller, Storage)
            and isinstance(self.frame, Storage)
            and self.caller.space.startswith("stack:")
            and self.frame.space == self.caller.space
            and self.caller.phase == self.frame.phase == self.phase
            and self.caller.writable
            and self.frame.writable
            and self.caller.extent == caller
            and self.frame.extent == extent
            and self.caller.alignment >= 8
            and self.frame.alignment >= 8
            and self.frame.start == self.caller.start - extent,
            "validator loses its actual caller/prologue/SP epoch",
        )
        require(
            isinstance(self.config, View)
            and not self.config.writable
            and self.config.extent == 14576
            and self.config.offset == 0
            and self.config.storage.space == "address"
            and self.config.storage.identity == "private unpublished ConfigSlot"
            and self.config.storage.extent == 14604
            and self.config.storage.phase == self.phase
            and self.config.storage.alignment >= 4
            and self.config.storage.writable
            and 0x20000000 <= self.config.start < self.config.start + 14604 <= 0x20080000,
            "validator lacks its readonly borrowed private typed Config owner",
        )
        require(
            isinstance(self.result, View)
            and self.result.storage == self.caller
            and self.result.writable
            and self.result.extent == 12
            and self.result.offset == outputs[PHASES.index(self.phase)]
            and self.result.start % 4 == 0,
            "validator Result is not its separate actual caller output epoch",
        )
        require(
            isinstance(self.protected, tuple)
            and 1 <= len(self.protected) <= 16
            and all(isinstance(v, Storage) and v.phase == self.phase for v in self.protected),
            "validator loses bounded typed protection",
        )
        require(
            any(
                v.space == self.frame.space
                and v.start == self.frame.start + local
                and v.extent == extent - local
                and not v.writable
                for v in self.protected
            ),
            "validator omits its complete save area",
        )
        require_writes((self.result,), ((self.result, 0, 12),), self.protected)
        require(
            self.result.start >= self.frame.start + extent, "caller Result aliases validator frame"
        )


@dataclass(frozen=True)
class ValidatorFamily:
    role: str
    mode: str
    parent: int
    sites: tuple
    contexts: tuple

    def __post_init__(self):
        require(
            isinstance(self.role, str)
            and self.role in OPERATIONS
            and isinstance(self.mode, str)
            and (self.role, self.mode) in SITE_SHAPES,
            "unreviewed validator access family/mode",
        )
        require(
            type(self.parent) is int
            and 0x10000000 <= self.parent < self.parent + LAYOUTS[self.mode][6] <= 0x101FE000
            and self.parent % 2 == 0,
            "validator dependency has no current typed code parent",
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
                and type(v[3]) is bool
                for v in self.sites
            ),
            "malformed validator access sites",
        )
        require(
            tuple((raw, offset, outlined) for at, raw, offset, outlined in self.sites)
            == SITE_SHAPES[self.role, self.mode],
            "validator operation/width/paired field/outline sites differ",
        )
        require(
            len({at for at, raw, o, x in self.sites}) == len(self.sites)
            and all(
                at % 2 == 0
                and 0x10000000 <= at < at + len(bytes.fromhex(raw)) <= 0x101FE000
                and (
                    outlined
                    or self.parent
                    <= at
                    < at + len(bytes.fromhex(raw))
                    <= self.parent + LAYOUTS[self.mode][6]
                )
                for at, raw, o, outlined in self.sites
            ),
            "validator site escapes its typed parent or duplicates another",
        )
        require(
            isinstance(self.contexts, tuple)
            and len(self.contexts) == 2
            and all(isinstance(c, ValidatorContext) and c.mode == self.mode for c in self.contexts)
            and tuple(c.phase for c in self.contexts) == PHASES,
            "validator loses distinct default/decoded contexts",
        )

        def physical(c):
            return (
                c.caller.space,
                c.caller.identity,
                c.caller.start,
                c.caller.extent,
                c.frame.identity,
                c.frame.start,
                c.config.start,
                c.config.extent,
            )

        require(
            physical(self.contexts[0]) == physical(self.contexts[1]),
            "validator contexts mix physical Config or loader activations",
        )
        for context in self.contexts:
            # Protect the actual complete footprint, not only its first byte.
            for lo, hi in self.footprints(
                context.phase,
                0,
                3
                if self.role == "control_buttons"
                else 64
                if self.role == "widget_placement"
                else 1,
            ):
                if OPERATIONS[self.role][1] == "write":
                    owner = (
                        context.result
                        if self.role == "result_writes"
                        else borrow(
                            context.frame, 0, LAYOUTS[self.mode][1], context.phase, writable=True
                        )
                    )
                    require_writes(
                        (owner,), ((owner, lo - owner.start, hi - lo),), context.protected
                    )

    def footprints(self, phase, index=0, count=1):
        require(
            phase in PHASES and type(index) is int and type(count) is int,
            "untyped validator loop/epoch",
        )
        c = self.contexts[PHASES.index(phase)]
        width = OPERATIONS[self.role][2]
        require(0 <= index < count, "validator reads uninitialized or absent loop element")
        if self.role == "control_buttons":
            require(count == 3, "ControlPage loop is not exactly three paired pages")
            base = c.config.start + 213 + 10 * index
        elif self.role == "widget_placement":
            require(count <= 64, "Widget loop exceeds its initialized capacity")
            base = c.config.start + 444 + 220 * index
        else:
            require(index == 0 and count == 1, "non-loop access borrows a loop instance")
            base = (
                c.result.start
                if self.role == "result_writes"
                else c.frame.start
                + (168 if self.role == "y_action_pairs" else LAYOUTS[self.mode][2])
            )
        return tuple(
            (base + offset, base + offset + width) for at, raw, offset, outlined in self.sites
        )

    def to_record(self):
        record = asdict(self)
        record["sites"] = [
            dict(site=at, encoding=raw, offset=o, outlined=x) for at, raw, o, x in self.sites
        ]
        record["contexts"] = [
            dict(
                mode=c.mode,
                phase=c.phase,
                caller=asdict(c.caller),
                frame=asdict(c.frame),
                config=asdict(c.config),
                result=asdict(c.result),
                protected=[asdict(v) for v in c.protected],
            )
            for c in self.contexts
        ]
        operation, direction, width = OPERATIONS[self.role]
        record.update(
            kind="validator_access_family",
            operation=operation,
            direction=direction,
            width=width,
            loop="paired_ControlPage_ALL_3"
            if self.role == "control_buttons"
            else "initialized_Widget_prefix_0_to_64_stride220"
            if self.role == "widget_placement"
            else "single_source_constructor",
            transport=dict(
                first_offset=LAYOUTS[self.mode][2],
                capture_offset=LAYOUTS[self.mode][3],
                output="r0/r10"
                if self.mode == "speed2"
                else "r6/Q64"
                if self.mode == "sizes"
                else "r5/Q60",
            ),
            source_phase="typed_unpublished_Config_validation",
        )
        return record


def read_validator_family(record):
    keys = set(ValidatorFamily.__dataclass_fields__) | {
        "kind",
        "operation",
        "direction",
        "width",
        "loop",
        "transport",
        "source_phase",
    }
    require(
        isinstance(record, dict)
        and set(record) == keys
        and isinstance(record["sites"], list)
        and 1 <= len(record["sites"]) <= 6
        and isinstance(record["contexts"], list)
        and len(record["contexts"]) == 2,
        "malformed validator dependency family",
    )
    contexts = []
    for c in record["contexts"]:
        require(
            isinstance(c, dict)
            and set(c) == set(ValidatorContext.__dataclass_fields__)
            and isinstance(c["protected"], list)
            and 1 <= len(c["protected"]) <= 16,
            "malformed validator context fields",
        )
        contexts.append(
            ValidatorContext(
                c["mode"],
                c["phase"],
                _storage(c["caller"]),
                _storage(c["frame"]),
                _view(c["config"]),
                _view(c["result"]),
                tuple(_storage(v) for v in c["protected"]),
            )
        )
    sites = []
    for v in record["sites"]:
        require(
            isinstance(v, dict) and set(v) == {"site", "encoding", "offset", "outlined"},
            "malformed validator site fields",
        )
        sites.append((v["site"], v["encoding"], v["offset"], v["outlined"]))
    value = ValidatorFamily(
        record["role"], record["mode"], record["parent"], tuple(sites), tuple(contexts)
    )
    require(
        type(record["width"]) is int
        and isinstance(record["transport"], dict)
        and all(type(record["transport"].get(k)) is int for k in ("first_offset", "capture_offset"))
        and record == value.to_record(),
        "validator dependency operation/context qualifier differs",
    )
    return value


# Reviewed operation bytes/field roles only, never source/owner admission.
SITE_SHAPES = {
    ("first_word", "sizes"): (
        ("cdf84a10", 6, False),
        ("cdf84a10", 6, False),
        ("c0f80620", 6, False),
    ),
    ("first_word", "sizez"): (("c0f80620", 6, False), ("cdf85210", 6, True)),
    ("first_reads", "speed2"): (("ddf85630", 2, False), ("ddf85a20", 6, False)),
    ("result_writes", "speed2"): (("c0f80230", 2, False), ("c0f80620", 6, False)),
    ("control_buttons", "speed2"): (("d1f8d540", 0, False), ("d1f8d990", 4, False)),
    ("control_buttons", "sizes"): (("d0f8d910", 4, False), ("d0f8d500", 0, False)),
    ("control_buttons", "sizez"): (("d0f8d510", 0, False), ("d0f8d900", 4, False)),
    ("first_halfwords", "speed2"): (
        ("adf85d10", 9, False),
        ("adf85d20", 9, False),
        ("a0f80710", 7, False),
        ("a0f80710", 7, False),
        ("a0f80710", 7, False),
        ("a1f80720", 7, False),
    ),
    ("y_action_pairs", "speed2"): (("adf8a9c0", 1, False),),
    ("widget_placement", "speed2"): (("b0f8d700", 215, False),),
    ("widget_placement", "sizes"): (("b0f8d700", 215, False),),
    ("widget_placement", "sizez"): (("b0f8d7a0", 215, False),),
}


VALIDATOR_ROLE = "_RNvNtCs_16dashboard_config8validate12validate_ref"
EXPECTED_ROLES = {
    "speed2": (
        "first_reads",
        "result_writes",
        "control_buttons",
        "first_halfwords",
        "y_action_pairs",
        "widget_placement",
    ),
    "sizes": ("first_word", "control_buttons", "widget_placement"),
    "sizez": ("first_word", "control_buttons", "widget_placement"),
}


def current_validator_family(objects, value):
    """Bind a producer's family to bytes/metadata, never infer its ownership."""
    from config_entry import CONFIG, entry_object
    from config_stack import typed_function, canonical_symbol
    from llvm_stack import mapping_ranges

    require(isinstance(value, ValidatorFamily), "missing typed validator dependency family")
    extent, frame, name = typed_function(objects, value.parent, VALIDATOR_ROLE)
    require(
        (extent, frame) == (LAYOUTS[value.mode][6], LAYOUTS[value.mode][0]),
        "validator dependency belongs to another current mode/frame",
    )
    mapping = mapping_ranges(objects.elf, name)
    for context in value.contexts:
        entry_object(objects, context.config.start, dict(kind="entry_static", role=CONFIG))
    for site, raw, offset, outlined in value.sites:
        if outlined:
            start = site - 4
            names = [n for n, e in objects.elf.functions.items() if e[0] & ~1 == start]
            require(
                len(names) == 1 and re.fullmatch(r"OUTLINED_FUNCTION_[0-9]+", names[0]),
                "validator first-error store has no unique typed outline",
            )
            child_extent, child_frame, child = typed_function(
                objects, start, canonical_symbol(names[0])
            )
            require(
                (child_extent, child_frame) == (10, 0)
                and mapping_ranges(objects.elf, child) == [(start, start + 10, "t")]
                and objects.read_only(start, 10) == bytes.fromhex("8df85600cdf852107047"),
                "validator first-error outline loses its complete zero-growth caller-relative ABI",
            )
        else:
            require(
                any(
                    kind == "t" and lo <= site < site + len(bytes.fromhex(raw)) <= hi
                    for lo, hi, kind in mapping
                ),
                "validator dependency instruction is not wholly mapped Thumb code",
            )
        require(
            objects.read_only(site, len(bytes.fromhex(raw))) == bytes.fromhex(raw),
            "validator dependency operation bytes differ from the current artifact",
        )
    return value
