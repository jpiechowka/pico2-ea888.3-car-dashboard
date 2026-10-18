"""Internal ownership and interface facts for authored finite regions.

These records are not user input or evidence learned from the current ELF.
A closed machine/source family must establish every allocation, phase, view
and scalar domain before using them. Fact construction does not establish
layout, initialization, register transport or a source domain.
They prevent composition from widening facts supplied by those recognizers.
The coverage auditor separately reconciles their exact current byte/edge
inventory; it does not establish the named family's semantic/effect claims.
"""

from dataclasses import dataclass
import re
from config_stack import canonical_symbol, typed_function, transfer_family, CONDITIONS
from llvm_stack import mapping_ranges
from validation import require


@dataclass(frozen=True)
class Storage:
    space: str
    identity: str
    start: int
    extent: int
    alignment: int
    phase: str
    writable: bool

    def __post_init__(self):
        require(
            isinstance(self.space, str)
            and (self.space == "address" or self.space.startswith("stack:")),
            "unreviewed storage coordinate space",
        )
        require(
            isinstance(self.identity, str)
            and bool(self.identity)
            and isinstance(self.phase, str)
            and bool(self.phase),
            "missing object identity or lifetime phase",
        )
        require(
            type(self.start) is int
            and type(self.extent) is int
            and self.extent > 0
            and (0 if self.space == "address" else -0x100000000)
            <= self.start
            < self.start + self.extent
            <= 0x100000000,
            "object interval exceeds its checked coordinate space",
        )
        require(
            type(self.alignment) is int
            and 0 < self.alignment <= 0x100000000
            and self.alignment & (self.alignment - 1) == 0
            and self.start % self.alignment == 0,
            "object alignment differs",
        )
        require(type(self.writable) is bool, "object mutability is not explicit")


@dataclass(frozen=True)
class View:
    storage: Storage
    offset: int
    extent: int
    writable: bool = False

    def __post_init__(self):
        require(
            isinstance(self.storage, Storage)
            and type(self.offset) is int
            and type(self.extent) is int
            and 0 <= self.offset < self.offset + self.extent <= self.storage.extent,
            "borrow exceeds its owned object",
        )
        require(
            type(self.writable) is bool and (not self.writable or self.storage.writable),
            "mutable borrow lacks a mutable owner",
        )

    @property
    def start(self):
        return self.storage.start + self.offset


def borrow(storage, offset, extent, phase, writable=False):
    require(storage.phase == phase, "borrow belongs to another lifetime phase")
    return View(storage, offset, extent, writable)


def _intersects(space, start, end, other):
    return space == other.space and start < other.start + other.extent and other.start < end


def require_disjoint(storages):
    # Keep one-shot facts for both validation and every pairwise comparison.
    storages = tuple(storages)
    require(all(isinstance(v, Storage) for v in storages), "untyped allocation")
    identities = set()
    for i, storage in enumerate(storages):
        key = (storage.space, storage.identity)
        require(key not in identities, "two active allocations share an object identity")
        identities.add(key)
        require(
            all(
                not _intersects(storage.space, storage.start, storage.start + storage.extent, other)
                for other in storages[:i]
            ),
            "distinct live owned allocations overlap",
        )


def require_writes(available, writes, protected):
    """Check declared child writes against exact borrowed output intervals."""
    available = tuple(available)
    protected = tuple(protected)
    require(
        all(isinstance(v, View) for v in available)
        and all(isinstance(v, Storage) for v in protected),
        "untyped output or protected storage",
    )
    for view, offset, extent in writes:
        require(
            view in available and view.writable, "write lacks the current mutable output borrow"
        )
        require(
            type(offset) is int
            and type(extent) is int
            and 0 <= offset < offset + extent <= view.extent,
            "helper write exceeds its exact output borrow",
        )
        start = view.start + offset
        require(
            all(not _intersects(view.storage.space, start, start + extent, v) for v in protected),
            "helper output intersects protected live storage",
        )


@dataclass(frozen=True)
class Argument:
    kind: str
    view: View | None
    levels: int
    bits: int = 0
    domain: frozenset | None = None

    def __post_init__(self):
        require(
            isinstance(self.kind, str)
            and bool(self.kind)
            and type(self.levels) is int
            and 0 <= self.levels <= 2,
            "untyped argument/reference level",
        )
        if self.view is not None:
            require(
                isinstance(self.view, View) and self.bits == 0 and self.domain is None,
                "mixed scalar and borrowed argument",
            )
        else:
            require(
                self.levels == 0
                and type(self.bits) is int
                and 1 <= self.bits <= 32
                and isinstance(self.domain, frozenset)
                and bool(self.domain)
                and all(type(v) is int and 0 <= v < 1 << self.bits for v in self.domain),
                "unbound scalar domain",
            )


def require_arguments(actual, expected):
    """No receiver-level, pointer/length cross-product or domain widening."""
    slots = re.compile(r"(?:r(?:[0-9]|1[0-2])|lr|S\+(?:0|[1-9][0-9]*))")
    require(isinstance(actual, dict) and isinstance(expected, dict), "missing finite call state")
    require(
        all(
            isinstance(k, str) and slots.fullmatch(k) and isinstance(v, Argument)
            for state in (actual, expected)
            for k, v in state.items()
        ),
        "untyped call argument location",
    )
    for slot, needed in expected.items():
        require(slot in actual, "required call argument is missing")
        provided = actual[slot]
        require(
            (provided.kind, provided.levels, provided.bits)
            == (needed.kind, needed.levels, needed.bits),
            "callee receiver/value/reference ABI differs",
        )
        if needed.view is None:
            require(
                provided.view is None and provided.domain <= needed.domain,
                "callee scalar domain is not established",
            )
        else:
            require(
                provided.view is not None
                and provided.view.storage == needed.view.storage
                and provided.view.offset == needed.view.offset
                and provided.view.extent >= needed.view.extent,
                "callee argument names another object, phase or insufficient extent",
            )
            require(
                not needed.view.writable or provided.view.writable,
                "callee mutable input has only a readonly borrow",
            )


def require_join(alternatives, expected):
    require(
        isinstance(alternatives, (tuple, list)) and bool(alternatives),
        "join has no incoming source state",
    )
    for predecessor in alternatives:
        require_arguments(predecessor, expected)


@dataclass(frozen=True)
class Transfer:
    site: int
    operation: str
    kind: str
    target: object
    interface: str
    role: str | None = None


@dataclass(frozen=True)
class Region:
    context: tuple
    family: str
    start: int
    end: int
    instructions: tuple
    data: tuple
    transfers: tuple


def _control(record):
    operation = record.mnemonic.removesuffix(".w")
    operands = record.operands.split("@", 1)[0].strip()
    family = transfer_family(operation)
    if family is not None:
        return family, operation != family or family in ("cbz", "cbnz")
    if re.fullmatch(r"pop" + CONDITIONS, operation) and re.search(r"\bpc\b", operands):
        return "return", operation != "pop"
    if re.fullmatch(r"ldm(?:ia|fd)?" + CONDITIONS, operation) and re.search(r"\bpc\b", operands):
        require(re.match(r"sp!,", operands), "ordinary LDM PC is not the reviewed stack return ABI")
        return "return", operation not in ("ldm", "ldmia", "ldmfd")
    require(
        not (operation.startswith("ldm") and re.search(r"\bpc\b", operands)),
        "unsupported multiple-load PC transfer",
    )
    require(not re.match(r"pc(?:,|$)", operands), "unsupported implicit PC transfer")
    return None, False


def audit_coverage(
    objects, name, instructions, context, regions, _function_consumer=typed_function
):
    """Inventory coverage of facts emitted by closed authored recognizers.

    This is not an ISA effect interpreter or a grammar learner. Region records
    are internal facts; no input manifest can supply them to the final gate.
    All semantic/source/layout/argument/effect contracts remain prerequisites.
    Intraprocedural backedges require a named invariant from their recognizer.
    """
    require(
        name in objects.elf.functions and isinstance(context, tuple) and bool(context),
        "missing function context",
    )
    start = objects.elf.functions[name][0] & ~1
    extent, frame, _ = typed_function(objects, start, canonical_symbol(name))
    end = start + extent
    require(context[0] == start, "coverage context belongs to another function")
    mapping = mapping_ranges(objects.elf, name)
    raw = objects.read_only(start, extent)
    code = {a for lo, hi, k in mapping if k == "t" for a in range(lo, hi)}
    data = {a for lo, hi, k in mapping if k == "d" for a in range(lo, hi)}
    rows = {a: r for a, r in instructions.items() if start <= a < end}
    code_seen = set()
    for at, row in rows.items():
        points = set(range(at, at + len(row.raw)))
        require(
            at == row.address
            and len(row.raw) in (2, 4)
            and points <= code
            and not points & code_seen
            and row.raw == raw[at - start : at - start + len(row.raw)],
            "coverage decoder bytes/Thumb interval differ",
        )
        code_seen |= points
    require(code_seen == code, "coverage decoder omits mapped code")
    # This closes direct ingress in the supplied whole-image decoder map.
    # Computed destinations still require their authored table/caller closure.
    for at, row in instructions.items():
        if start <= at < end:
            continue
        family = transfer_family(row.mnemonic.removesuffix(".w"))
        if family in ("b", "bl", "cbz", "cbnz", "blx"):
            target = re.search(
                r"(?<![#\w])0x([0-9a-f]+)(?:\s|$)", row.operands.split("@", 1)[0].strip()
            )
            require(
                target is None or not start < int(target[1], 16) < end,
                "foreign direct ingress into a function region",
            )
    owners = {}
    data_seen = set()
    machine = {}
    fallthrough = {}
    padding = set()
    padding_start = None
    families = {
        "entry",
        "frame",
        "helper",
        "selector",
        "result",
        "visitor",
        "loop",
        "issuer",
        "diagnostic",
        "data",
        "padding",
    }
    for ordinal, region in enumerate(regions):
        require(
            isinstance(region, Region)
            and region.context == context
            and region.family in families
            and start <= region.start < region.end <= end,
            "unbound region context/family/extent",
        )
        points = set()
        for at in region.instructions:
            require(
                at in rows and region.start <= at < at + len(rows[at].raw) <= region.end,
                "region instruction exceeds its interval",
            )
            covered = set(range(at, at + len(rows[at].raw)))
            require(
                all(byte not in owners for byte in covered),
                "overlapping instruction region ownership",
            )
            points |= covered
            for byte in covered:
                owners[byte] = ordinal
        for at, value in region.data:
            require(
                isinstance(value, bytes)
                and bool(value)
                and region.start <= at < at + len(value) <= region.end,
                "invalid region data extent",
            )
            covered = set(range(at, at + len(value)))
            require(
                covered <= data
                and not covered & data_seen
                and raw[at - start : at - start + len(value)] == value,
                "region data bytes/ownership differ",
            )
            data_seen |= covered
            points |= covered
        require(points == set(range(region.start, region.end)), "region has an unowned byte or gap")
        if region.family == "padding":
            require(
                padding_start is None
                and start < region.start
                and region.end == max(code) + 1
                and not region.data
                and not region.transfers
                and all(
                    rows[a].raw == b"\x00\xbf"
                    and rows[a].mnemonic == "nop"
                    and not rows[a].operands.strip()
                    for a in region.instructions
                ),
                "unreviewed post-terminal padding",
            )
            padding_start = region.start
            padding.update(region.instructions)
        for edge in region.transfers:
            require(
                isinstance(edge, Transfer)
                and edge.site in region.instructions
                and isinstance(edge.interface, str)
                and bool(edge.interface)
                and edge.operation == rows[edge.site].mnemonic,
                "unbound region transfer interface",
            )
            table = fallthrough if edge.kind == "fallthrough" else machine
            require(edge.site not in table, "duplicate transfer interface")
            table[edge.site] = edge
    require(
        set(owners) == code and data_seen == data, "uncovered function instruction or mapped data"
    )
    consumed = set()
    falls = set()
    terminal_ends = set()
    for at, row in sorted(rows.items()):
        if at in padding:
            continue
        family, conditional = _control(row)
        edge = machine.get(at)
        stops = False
        if family is None:
            require(edge is None, "spurious machine transfer interface")
        else:
            require(edge is not None, "unbound current control transfer")
            consumed.add(at)
            operands = row.operands.split("@", 1)[0].strip()
            target = re.search(r"(?<![#\w])0x([0-9a-f]+)(?:\s|$)", operands)
            returning = family == "return" or family == "bx" and operands == "lr"
            if returning:
                require(
                    edge.kind == ("conditional_return" if conditional else "return")
                    and edge.target is None
                    and edge.role is None,
                    "return interface/condition differs",
                )
                stops = not conditional
            elif family in ("b", "cbz", "cbnz", "bl", "blx", "bx"):
                require(
                    type(edge.target) is int
                    and 0 <= edge.target < 0x100000000
                    and edge.target % 2 == 0,
                    "unbound transfer destination",
                )
                if target is not None:
                    require(
                        edge.target == int(target[1], 16), "direct transfer destination differs"
                    )
                require(target is not None or family in ("blx", "bx"), "unresolved direct transfer")
                call = family in ("bl", "blx")
                if call or not start <= edge.target < end:
                    require(
                        edge.kind in (("call", "nonreturn") if call else ("tail",))
                        and isinstance(edge.role, str),
                        "external/call interface has no typed role",
                    )
                    _function_consumer(objects, edge.target, edge.role)
                    require(
                        edge.kind != "nonreturn" or not conditional,
                        "conditional call cannot erase its fallthrough",
                    )
                    stops = edge.kind == "nonreturn" or edge.kind == "tail" and not conditional
                else:
                    require(
                        edge.target in rows
                        and edge.target not in padding
                        and edge.kind in ("branch", "loop", "root_suffix", "region_exit")
                        and edge.role is None,
                        "internal continuation is not a current instruction",
                    )
                    require(
                        edge.kind == "loop" if edge.target <= at else edge.kind != "loop",
                        "backedge lacks its named invariant",
                    )
                    stops = family in ("b", "bx") and not conditional
            elif family in ("tbb", "tbh"):
                require(
                    edge.kind == "table"
                    and edge.role is None
                    and isinstance(edge.target, tuple)
                    and bool(edge.target)
                    and all(type(v) is int and v in rows and v not in padding for v in edge.target),
                    "unbound typed table targets",
                )
                stops = True
            elif family in ("udf", "trap"):
                require(
                    edge.kind == "terminal"
                    and edge.target is None
                    and edge.role is None
                    and not conditional,
                    "unbound terminal control",
                )
                stops = True
            else:
                require(False, "unsupported ordinary control transfer")
        following = at + len(row.raw)
        continuation = fallthrough.get(at)
        if stops:
            require(continuation is None, "terminal transfer has a spurious fallthrough")
            terminal_ends.add(following)
        else:
            require(
                following in rows and following not in padding,
                "unbound data/function-end/padding fallthrough",
            )
            crossing = owners[at] != owners[following]
            if crossing:
                require(
                    continuation is not None
                    and continuation.target == following
                    and continuation.role is None,
                    "region boundary lacks its exact fallthrough state",
                )
                falls.add(at)
            else:
                require(continuation is None, "spurious intra-region fallthrough interface")
    require(
        consumed == set(machine) and falls == set(fallthrough),
        "unconsumed region transfer interface",
    )
    require(
        padding_start is None or padding_start in terminal_ends,
        "padding has no preceding closed terminal boundary",
    )
    return dict(
        function=start,
        frame=frame,
        context=context,
        code_bytes=len(code),
        data_bytes=len(data),
        instructions=len(rows),
        transfers=len(consumed),
        cross_region_fallthrough=len(falls),
        requires_authored_semantics_source_and_child_effects=True,
    )
