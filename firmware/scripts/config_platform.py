"""Finite conditional platform scope for named RP2350 stack accounting.

Version8 assumes a usable stack, normal fresh Secure Core0 IMAGE_DEF context,
Secure UNALIGN_TRP=0 at relevant single accesses, readable Normal embedded
input and effective Normal read/write permission for the closed owned accesses.
These are external conditions, never host-established hardware facts. They do
not waive source/body/ordinary/control/range/ownership/callback proof. Unknown
effects cannot become assumptions. No established policy is supported here.
The reviewed scope declaration is copied into these bounded constants so a
fresh checkout does not depend on an ignored report or runtime approve switch.
"""

from dataclasses import dataclass
from collections.abc import Mapping, Sequence
from typing import TYPE_CHECKING, TypeVar, TypedDict, NoReturn, TypeGuard
from elf_image import MappedImage
from script_values import object_fields, array_items, integer_value, string_value

if TYPE_CHECKING:
    from config_stack import DiagnosticObjects
import hashlib
from config_structural import _image_hash, _readonly_hash
import json
import re
import struct
import zlib
from config_regions import View, Storage
from config_owned_access import OwnedAccess, read_owned_access, current_owned_access
from config_validator_access import (
    ValidatorFamily,
    ValidatorContext,
    read_validator_family,
    current_validator_family,
    EXPECTED_ROLES,
)
from config_led_access import LedFamily, read_led_family, current_led_family, EXPECTED_LED_ROLES
from config_widget_access import (
    WidgetFamily,
    read_widget_family,
    current_widget_family,
    EXPECTED_WIDGET_ROLES,
)
from config_source_access import (
    SourceFamily,
    read_source_family,
    current_source_family,
    EXPECTED_SOURCE_ROLES,
)
from config_tail_access import (
    TailFamily,
    read_tail_family,
    current_tail_family,
    EXPECTED_TAIL_ROLES,
)
from validation import require

TARGET = "thumbv8m.main-none-eabihf"
BOOT_CONTEXT = "fresh_normal_core0_image_def"
SCOPE = "h2-rp2350-secure-owned-access-v8"
SCOPE_SHA256 = "0decd7e28e1eaa62d95730efe4bb0eb02ced8f457c92939190c0f7eb722bcf03"
POLICY_SCHEMA = 7
ASSUMPTIONS = (
    "A valid usable stack is available.",
    "Executing Core0 is Secure in the normal fresh ROM IMAGE_DEF context; other entry modes are outside scope.",
    "Secure CCR.UNALIGN_TRP remains zero at every dependent ordinary single access.",
    "The complete bound embedded EAV payload remains readable Normal memory under effective MPU, security and XIP mapping.",
    "Every internally bound closed LDR/STR/LDRH/STRH footprint remains compatible Normal memory with effective permission matching its read/write direction under MPU, security and interconnect mappings.",
)
BOUND_KEYS = ("boot", "constructor", "runtime", "dashboard")


Dependency = OwnedAccess | ValidatorFamily | LedFamily | WidgetFamily | SourceFamily | TailFamily


class PolicyRecord(TypedDict):
    input_start: int
    input_end: int
    input_sha256: str
    elf_sha256: str
    owned_dependencies: list[dict[str, object]]
    owned_family: str
    schema: int
    scope: str
    scope_sha256: str
    status: str
    target: str
    core: int
    security: str
    boot_context: str
    input_role: str
    assumptions: list[str]
    hardware_qualified: bool


class ExecutionRecord(TypedDict):
    schema: int
    kind: str
    scope: str
    scope_sha256: str
    domain: str
    elf_sha256: str
    input_sha256: str
    source_context_sha256: str
    all_observers_compatible: bool
    exception_absence_established: bool
    exception_safe_completion: bool
    unconditional_owner: bool
    hardware_qualified: bool
    exception_graph_limit: list[str]
    pending_exception_frontier: list[str]
    external_qualifications: list[str]
    requires_H8_consumed_input_closure: bool
    requires_M2_pre_init_source_reconciliation: bool


class StackBounds(TypedDict):
    boot: int
    constructor: int
    runtime: int
    dashboard: int


class StackRecord(TypedDict):
    schema: int
    status: str
    platform_policy: str
    hardware_qualified: bool
    stack_bounds: StackBounds
    headroom: int
    policies: list[PolicyRecord]
    execution_scope: ExecutionRecord


_Record = TypeVar("_Record", bound=Mapping[str, object])


def _bounded_record(record: _Record) -> _Record:
    # Exactly the memory CLI's sorted JSON plus terminating LF. This
    # checks the enclosing record too; a fitting child policy is insufficient.
    require(
        len((json.dumps(record, sort_keys=True) + "\n").encode("utf-8")) <= 65536,
        "oversized conditional result/dependency record",
    )
    return record


def _read_dependency(value: object) -> Dependency:
    record = object_fields(value, "untyped conditional access dependency")
    require(isinstance(record, dict), "untyped conditional access dependency")
    if "kind" in record:
        if record["kind"] == "validator_access_family":
            return read_validator_family(record)
        if record["kind"] == "decoded_widget_access_family":
            return read_widget_family(record)
        if record["kind"] == "decoded_source_access_family":
            return read_source_family(record)
        if record["kind"] == "decoded_tail_access_family":
            return read_tail_family(record)
        require(
            record["kind"] == "decoded_led_access_family",
            "unknown conditional access dependency tag",
        )
        return read_led_family(record)
    return read_owned_access(record)


def _typed_dependencies(value: object) -> TypeGuard[tuple[Dependency, ...]]:
    return (
        isinstance(value, tuple)
        and len(value) <= 32
        and all(
            isinstance(
                v, (OwnedAccess, ValidatorFamily, LedFamily, WidgetFamily, SourceFamily, TailFamily)
            )
            for v in value
        )
    )


def _family_name(dependencies: tuple[Dependency, ...]) -> str:
    validators = tuple(
        v
        for v in dependencies
        if isinstance(v, (ValidatorFamily, LedFamily, WidgetFamily, SourceFamily, TailFamily))
    )
    if validators:
        require(len({v.mode for v in validators}) == 1, "conditional set mixes validator modes")
        return validators[0].mode + "_configuration"
    return "sizez_default_refill" if dependencies else "readonly_eav"


_Member = TypeVar("_Member")


def _members(values: tuple[Dependency, ...], kind: type[_Member]) -> tuple[_Member, ...]:
    result: list[_Member] = []
    for value in values:
        if not isinstance(value, kind):
            raise AssertionError("conditional set has another access family")
        result.append(value)
    return tuple(result)


def _legacy_default(dependencies: tuple[Dependency, ...]) -> None:
    require(
        len(dependencies) == 10 and all(isinstance(v, OwnedAccess) for v in dependencies),
        "missing inherited default dependency family",
    )
    owned = _members(dependencies, OwnedAccess)
    advanced = tuple(v for v in owned if v.role == "advanced_default_fields")
    configs = tuple(v for v in owned if v.role == "configuration_versions")
    widgets = tuple(v for v in owned if v.role == "widget_colors")
    require(
        len(advanced) == len(configs) == 1
        and len(widgets) == 8
        and tuple(v.loop_index for v in widgets) == tuple(range(8)),
        "owned default family loses a paired store/loop instance",
    )
    require(
        tuple(v.role for v in owned)
        == ("configuration_versions", "advanced_default_fields") + ("widget_colors",) * 8,
        "owned source-stage dependency order changed",
    )
    require(
        all(v.frame == configs[0].frame for v in owned)
        and all(v.owner.storage == configs[0].owner.storage for v in widgets),
        "owned family mixes parent activations/private slots",
    )
    keys = tuple(
        (v.role, v.activation, v.owner.storage.space, v.owner.storage.identity, v.owner.start)
        for v in owned
    )
    require(len(set(keys)) == len(keys), "duplicate owned platform dependency/context")


def _dependency_set(family: str, dependencies: tuple[Dependency, ...]) -> None:
    if family == "readonly_eav":
        require(not dependencies, "readonly EAV family cannot authorize owned accesses")
        return
    if family == "sizez_default_refill":
        _legacy_default(dependencies)
        return
    require(
        family in tuple(mode + "_configuration" for mode in EXPECTED_ROLES),
        "unknown conditional dependency family",
    )
    mode = family.removesuffix("_configuration")
    legacy = dependencies[:10] if mode == "sizez" else ()
    rest = dependencies[10:] if mode == "sizez" else dependencies
    count = len(EXPECTED_ROLES[mode])
    led_count = len(EXPECTED_LED_ROLES[mode])
    widget_count = len(EXPECTED_WIDGET_ROLES) if mode == "sizez" else 0
    source_count = len(EXPECTED_SOURCE_ROLES) if mode == "sizez" else 0
    validators = _members(rest[:count], ValidatorFamily)
    leds = _members(rest[count : count + led_count], LedFamily)
    widgets = _members(rest[count + led_count : count + led_count + widget_count], WidgetFamily)
    source_start = count + led_count + widget_count
    sources = _members(rest[source_start : source_start + source_count], SourceFamily)
    tails = _members(rest[source_start + source_count :], TailFamily)
    if legacy:
        _legacy_default(legacy)
    require(
        all(isinstance(v, LedFamily) and v.mode == mode for v in leds)
        and tuple(v.role for v in leds) == EXPECTED_LED_ROLES[mode],
        "conditional set loses/injects/duplicates a decoded LED family",
    )
    require(
        all(isinstance(v, ValidatorFamily) and v.mode == mode for v in validators)
        and tuple(v.role for v in validators) == EXPECTED_ROLES[mode],
        "conditional validator set loses/injects/duplicates a source family",
    )
    require(
        all(isinstance(v, WidgetFamily) and v.mode == mode for v in widgets)
        and tuple(v.role for v in widgets) == (EXPECTED_WIDGET_ROLES if mode == "sizez" else ()),
        "conditional set loses/injects/duplicates a decoded Widget family",
    )
    require(
        all(isinstance(v, SourceFamily) and v.mode == mode for v in sources)
        and tuple(v.role for v in sources) == (EXPECTED_SOURCE_ROLES if mode == "sizez" else ()),
        "conditional set loses/injects/duplicates a decoded Sources family",
    )
    require(
        all(isinstance(v, TailFamily) and v.mode == mode for v in tails)
        and tuple(v.role for v in tails) == (EXPECTED_TAIL_ROLES if mode == "sizez" else ()),
        "conditional set loses/injects/duplicates a decoded Tail family",
    )
    first = validators[0]

    def shared(
        contexts: tuple[ValidatorContext, ...],
    ) -> tuple[tuple[str, str, Storage, Storage, View, View], ...]:
        # The source phases can carry different extra protected locals. Retain
        # each exact list while requiring the same caller, Config and Result.
        return tuple((c.mode, c.phase, c.caller, c.frame, c.config, c.result) for c in contexts)

    require(
        all(
            v.parent == first.parent and shared(v.contexts) == shared(first.contexts)
            for v in validators
        ),
        "conditional validator families mix activation/Config/output contexts",
    )
    decoded = first.contexts[1]

    def allocation(s: Storage) -> tuple[str, str, int, int, int, bool]:
        return (s.space, s.identity, s.start, s.extent, s.alignment, s.writable)

    for led in leds:
        for c in led.contexts:
            require(
                allocation(c.loader) == allocation(decoded.caller)
                and allocation(c.config.storage) == allocation(decoded.config.storage),
                "LED and validator dependencies mix loader/Config activations",
            )
    if mode == "sizez":
        pattern, reads, commit = leds
        require(reads.parent == commit.parent, "LED read/commit families name different visitors")
        for a, b in zip(pattern.contexts, reads.contexts):
            require(
                allocation(a.owner.storage) == allocation(b.frame)
                and allocation(b.frame) == allocation(commit.contexts[0].frame),
                "LED child/read/commit families lose the same visitor lifetime lineage",
            )
        for widget in widgets:
            widget_context = widget.context
            require(
                allocation(widget_context.loader) == allocation(decoded.caller)
                and allocation(widget_context.config.storage) == allocation(decoded.config.storage)
                and allocation(widget_context.visitor) == allocation(commit.contexts[0].frame),
                "Widget dependencies lose same loader/visitor/Config lineage",
            )
        require(
            len({v.parent for v in widgets[:3]}) == 1 and widgets[0].parent == reads.parent,
            "Widget and LED dependencies name different visitors",
        )
        for source in sources:
            source_context = source.context
            require(
                source.caller == reads.parent
                and allocation(source_context.loader) == allocation(decoded.caller)
                and allocation(source_context.config.storage) == allocation(decoded.config.storage)
                and allocation(source_context.visitor) == allocation(commit.contexts[0].frame),
                "Sources dependencies lose same loader/visitor/Config lineage",
            )
        require(
            sources[0].parent == sources[1].parent
            and sources[0].context == sources[1].context
            and sources[2].context == sources[3].context,
            "Sources paired accesses lose the same child/scratch activation",
        )
        for tail in tails:
            tail_context = tail.context
            require(
                tail.caller == reads.parent
                and allocation(tail_context.loader) == allocation(decoded.caller)
                and allocation(tail_context.config.storage) == allocation(decoded.config.storage)
                and allocation(tail_context.visitor) == allocation(commit.contexts[0].frame),
                "Tail dependencies lose same loader/visitor/Config lineage",
            )
    if legacy:
        legacy_first = _members(legacy, OwnedAccess)[0]
        validation_first = first.contexts[0]
        require(
            (
                legacy_first.frame.space,
                legacy_first.frame.identity,
                legacy_first.frame.start,
                legacy_first.frame.extent,
                legacy_first.owner.storage.start,
            )
            == (
                validation_first.caller.space,
                validation_first.caller.identity,
                validation_first.caller.start,
                validation_first.caller.extent,
                validation_first.config.start,
            ),
            "default and validator dependencies name different actual parent/Config",
        )


@dataclass(frozen=True)
class PlatformPolicy:
    input_start: int
    input_end: int
    input_sha256: str
    elf_sha256: str
    owned_dependencies: tuple[Dependency, ...] = ()
    owned_family: str = "readonly_eav"
    schema: int = POLICY_SCHEMA
    scope: str = SCOPE
    scope_sha256: str = SCOPE_SHA256
    status: str = "assumed_external"
    target: str = TARGET
    core: int = 0
    security: str = "secure"
    boot_context: str = BOOT_CONTEXT
    input_role: str = "embedded_eav_payload"
    assumptions: tuple[str, ...] = ASSUMPTIONS
    hardware_qualified: bool = False

    def __post_init__(self) -> None:
        require(
            type(self.schema) is int
            and self.schema == POLICY_SCHEMA
            and (self.scope, self.scope_sha256) == (SCOPE, SCOPE_SHA256),
            "missing/stale platform scope declaration",
        )
        require(
            self.status == "assumed_external" and self.hardware_qualified is False,
            "unresolved or unproved platform status",
        )
        require(
            self.target == TARGET
            and type(self.core) is int
            and self.core == 0
            and self.security == "secure"
            and self.boot_context == BOOT_CONTEXT
            and self.input_role == "embedded_eav_payload",
            "platform target/bank/input context differs",
        )
        require(
            type(self.input_start) is int
            and type(self.input_end) is int
            and 0x10000000 <= self.input_start < self.input_end <= 0x101FE000,
            "platform input exceeds program XIP range",
        )
        require(
            all(
                isinstance(v, str) and re.fullmatch("[0-9a-f]{64}", v)
                for v in (self.input_sha256, self.elf_sha256)
            ),
            "platform input/artifact identity is missing",
        )
        require(
            self.assumptions == ASSUMPTIONS and isinstance(self.assumptions, tuple),
            "platform assumptions changed or lost",
        )
        require(
            _typed_dependencies(self.owned_dependencies),
            "untyped/unbounded owned platform dependencies",
        )
        _dependency_set(self.owned_family, self.owned_dependencies)
        _bounded_record(self.to_record())

    def for_f32_load(self, cursor: int) -> "PlatformPolicy":
        require(
            type(cursor) is int and self.input_start <= cursor < cursor + 4 <= self.input_end,
            "f32 read exceeds bound platform input",
        )
        return self

    def to_record(self) -> PolicyRecord:
        return PolicyRecord(
            input_start=self.input_start,
            input_end=self.input_end,
            input_sha256=self.input_sha256,
            elf_sha256=self.elf_sha256,
            owned_dependencies=[v.to_record() for v in self.owned_dependencies],
            owned_family=self.owned_family,
            schema=self.schema,
            scope=self.scope,
            scope_sha256=self.scope_sha256,
            status=self.status,
            target=self.target,
            core=self.core,
            security=self.security,
            boot_context=self.boot_context,
            input_role=self.input_role,
            assumptions=list(self.assumptions),
            hardware_qualified=self.hardware_qualified,
        )


def reviewed_eav_policy(
    objects: "DiagnosticObjects",
    payload: View,
    context: object,
    *,
    contradictory_effects: object,
    owned_dependencies: object = (),
) -> PlatformPolicy:
    """Called only with recognized source input and closed source-effect facts.

    The returned assumption does not establish their constructor/transport or
    preservation. This factory has no caller-selected scope/status/approval.
    """
    require(
        isinstance(context, dict)
        and context == dict(target=TARGET, core=0, security="secure", boot_context=BOOT_CONTEXT)
        and type(context.get("core")) is int,
        "unreviewed platform entry context",
    )
    require(
        isinstance(contradictory_effects, tuple) and not contradictory_effects,
        "known/unknown contradictory platform effect",
    )
    require(
        isinstance(payload, View)
        and payload.storage.space == "address"
        and payload.storage.identity == "embedded_eav"
        and not payload.storage.writable
        and not payload.writable
        and payload.offset == 12
        and payload.extent == payload.storage.extent - 12,
        "platform input lacks its recognized complete readonly EAV payload",
    )
    _raw = objects.read_only(payload.start, payload.extent)
    if not _typed_dependencies(owned_dependencies):
        raise AssertionError("untyped/unbounded owned platform dependencies")
    return PlatformPolicy(
        payload.start,
        payload.start + payload.extent,
        _readonly_hash(objects, payload.start, payload.extent),
        _image_hash(objects),
        owned_dependencies=owned_dependencies,
        owned_family=_family_name(owned_dependencies),
    )


def read_policy(value: object, expected: PlatformPolicy) -> PlatformPolicy:
    record = object_fields(value, "absent/unresolved platform prerequisite")
    require(
        isinstance(expected, PlatformPolicy) and isinstance(record, dict),
        "absent/unresolved platform prerequisite",
    )
    wanted = expected.to_record()
    require(
        set(record) == set(wanted)
        and isinstance(record.get("owned_dependencies"), list)
        and len(array_items(record["owned_dependencies"], "malformed owned dependencies"))
        == len(expected.owned_dependencies),
        "platform reader lost its exact dependency tuple",
    )
    dependencies = tuple(
        _read_dependency(v)
        for v in array_items(record["owned_dependencies"], "malformed owned dependencies")
    )
    require(dependencies == expected.owned_dependencies, "platform dependency context differs")
    require(
        set(record) == set(wanted)
        and type(record.get("schema")) is int
        and type(record.get("core")) is int
        and type(record.get("input_start")) is int
        and type(record.get("input_end")) is int
        and record.get("hardware_qualified") is False
        and record == wanted,
        "platform qualifier or current target/input identity differs",
    )
    return expected


def compose_policy(expected: PlatformPolicy, children: Sequence[object]) -> PlatformPolicy:
    require(
        isinstance(expected, PlatformPolicy)
        and isinstance(children, (tuple, list))
        and bool(children),
        "lost dependent platform prerequisite",
    )
    require(
        all(isinstance(child, PlatformPolicy) and child == expected for child in children),
        "dependent platform prerequisite differs or was dropped",
    )
    return expected


def _stack_numbers(value: object, headroom: int) -> StackBounds:
    bounds = object_fields(value, "invalid stack result bounds/headroom")
    require(set(bounds) == set(BOUND_KEYS), "invalid stack result bounds/headroom")
    checked = StackBounds(
        boot=integer_value(bounds["boot"], "invalid stack bound"),
        constructor=integer_value(bounds["constructor"], "invalid stack bound"),
        runtime=integer_value(bounds["runtime"], "invalid stack bound"),
        dashboard=integer_value(bounds["dashboard"], "invalid stack bound"),
    )
    require(
        all(
            0 <= v < 0x100000000
            for v in (
                checked["boot"],
                checked["constructor"],
                checked["runtime"],
                checked["dashboard"],
            )
        )
        and type(headroom) is int
        and 0 < headroom <= 0x80000,
        "invalid stack result bounds/headroom",
    )
    from check_stack_budget import budgets

    budgets(
        checked["boot"], checked["constructor"], checked["runtime"], checked["dashboard"], headroom
    )
    return checked


def qualified_stack_result(
    bounds: object, headroom: int, policy: PlatformPolicy, *, ordinary_result: object = None
) -> NoReturn:
    """Fail closed until the actual complete named graph producer exists.

    No successful prefix, public receipt, constants or supplied stage dictionary
    is the complete internal ordinary/resource analysis required by this gate.
    The current check_stack_budget.check still returns no complete result.
    """
    require(type(policy) is PlatformPolicy, "stack result has no resolved conditional scope")
    _stack_numbers(bounds, headroom)
    raise AssertionError(
        "complete internal named ordinary graph/resource producer remains unavailable"
    )


def read_stack_result(
    value: object,
    expected_policy: PlatformPolicy,
    expected_headroom: object,
    *,
    source_context: object = None,
    elf: MappedImage | None = None,
) -> StackRecord:
    """Check a producer record without constructing private completion state."""
    record = object_fields(value, "stack consumer lost execution scope or schema2")
    require(
        type(record) is dict
        and set(record)
        == {
            "schema",
            "status",
            "platform_policy",
            "hardware_qualified",
            "stack_bounds",
            "headroom",
            "policies",
            "execution_scope",
        }
        and type(record["schema"]) is int
        and record["schema"] == 2,
        "stack consumer lost execution scope or schema2",
    )
    require(
        record["status"] == "CONDITIONAL_STACK_PASS"
        and record["platform_policy"] == "assumed_external"
        and record["hardware_qualified"] is False
        and type(record["headroom"]) is int
        and type(expected_headroom) is int
        and record["headroom"] == expected_headroom
        and type(record["policies"]) is list
        and len(array_items(record["policies"], "stack consumer lost policies")) == 1,
        "stack consumer lost conditional policy/qualification",
    )
    policy = read_policy(
        array_items(record["policies"], "stack consumer lost policies")[0], expected_policy
    )
    if elf is None:
        raise AssertionError("stack consumer lacks current ELF")
    receipt = read_execution_scope(record["execution_scope"], policy, source_context, elf=elf)
    checked_headroom = integer_value(record["headroom"], "invalid stack headroom")
    checked_bounds = _stack_numbers(record["stack_bounds"], checked_headroom)
    return _bounded_record(
        StackRecord(
            schema=2,
            status="CONDITIONAL_STACK_PASS",
            platform_policy="assumed_external",
            hardware_qualified=False,
            stack_bounds=checked_bounds,
            headroom=checked_headroom,
            policies=[policy.to_record()],
            execution_scope=receipt.to_record(),
        )
    )


def current_image_policy(
    value: object, elf: MappedImage, expected_eav: bytes | None = None
) -> PlatformPolicy:
    """Check an existing producer policy against current artifact/input bytes.

    This consumer does not discover the used source or establish graph proof.
    The source-reviewed stack producer must supply the actual policy; no scan
    or caller approval can create one here. Capture additionally binds the
    exact generated complete EAV, independently of its declared payload hash.
    """
    record = object_fields(value, "missing current image platform policy")
    require(
        isinstance(record, dict)
        and all(k in record for k in ("input_start", "input_end", "input_sha256", "elf_sha256")),
        "missing current image platform policy",
    )
    require(
        isinstance(record.get("owned_dependencies"), list)
        and len(array_items(record["owned_dependencies"], "malformed owned dependencies")) <= 32,
        "image policy lost its bounded owned dependencies",
    )
    require(
        type(record.get("schema")) is int
        and record["schema"] == POLICY_SCHEMA
        and (record.get("scope"), record.get("scope_sha256")) == (SCOPE, SCOPE_SHA256),
        "missing/stale platform scope declaration",
    )
    dependencies = tuple(
        _read_dependency(v)
        for v in array_items(record["owned_dependencies"], "malformed owned dependencies")
    )
    policy = PlatformPolicy(
        integer_value(record["input_start"], "malformed input_start"),
        integer_value(record["input_end"], "malformed input_end"),
        string_value(record["input_sha256"], "malformed input_sha256"),
        string_value(record["elf_sha256"], "malformed elf_sha256"),
        owned_dependencies=dependencies,
        owned_family=string_value(record.get("owned_family"), "malformed owned family"),
    )
    read_policy(record, policy)
    require(
        hashlib.sha256(elf.data).hexdigest() == policy.elf_sha256,
        "platform policy belongs to another ELF",
    )
    from config_stack import DiagnosticObjects

    sizes = {}
    if any(
        isinstance(v, (ValidatorFamily, LedFamily, WidgetFamily, SourceFamily, TailFamily))
        for v in dependencies
    ):
        from check_stack_budget import frames

        sizes = frames(elf.section_bytes(".stack_sizes"))
    objects = DiagnosticObjects(elf, sizes)
    for dependency in dependencies:
        if isinstance(dependency, ValidatorFamily):
            current_validator_family(objects, dependency)
        elif isinstance(dependency, LedFamily):
            current_led_family(objects, dependency)
        elif isinstance(dependency, WidgetFamily):
            current_widget_family(objects, dependency)
        elif isinstance(dependency, SourceFamily):
            current_source_family(objects, dependency)
        elif isinstance(dependency, TailFamily):
            current_tail_family(objects, dependency)
        else:
            current_owned_access(objects, dependency)
    extent = policy.input_end - policy.input_start
    require(
        12 < extent + 12 <= 16384 and policy.input_start >= 0x1000000C,
        "platform payload has no bounded EAV envelope",
    )
    start = policy.input_start - 12
    backing = []
    for section in elf.sections:
        if (
            not section[2] & 2
            or not section[5]
            or not (section[3] < policy.input_end and start < section[3] + section[5])
        ):
            continue
        require(
            0 <= section[3] < section[3] + section[5] <= 0x100000000,
            "platform input allocation wraps",
        )
        backing.append(section)
    require(
        len(backing) == 1
        and backing[0][1] == 1
        and not backing[0][2] & 1
        and backing[0][3] <= start < policy.input_end <= backing[0][3] + backing[0][5],
        "platform input has aliased or writable backing",
    )
    raw = objects.read_only(policy.input_start - 12, extent + 12)
    require(
        raw[:4] == b"EAV1"
        and struct.unpack_from("<HH", raw, 4) == (4, extent)
        and zlib.crc32(raw[:8] + raw[12:]) == struct.unpack_from("<I", raw, 8)[0],
        "platform payload does not match its current complete EAV envelope",
    )
    require(
        hashlib.sha256(raw[12:]).hexdigest() == policy.input_sha256,
        "platform payload content changed",
    )
    if expected_eav is not None:
        require(
            isinstance(expected_eav, bytes) and raw == expected_eav,
            "generated capture input differs from stack policy",
        )
    return policy


# Separate execution namespace: this does not consume the reserved access
# versions or change any PlatformPolicy permission. A receipt is identity and
# qualification metadata, never an ordinary continuation/completion proof.
EXECUTION_SCOPE = "h2-named-ordinary-continuation-v1"
EXECUTION_SCOPE_SHA256 = "b55723348ef4ffab36786a763f1c0d8035a516a416bb79aa30e2cae6143fd9eb"
EXECUTION_DOMAIN = "named_ordinary_and_callback_paths"
EXCEPTION_FRONTIER = (
    "routing_and_stack_selection_unproved",
    "architectural_entry_memory_and_nested_transitions_unproved",
    "reset_reentry_requires_fresh_qualified_epoch",
)


@dataclass(frozen=True)
class ExecutionReceipt:
    """Checked public descriptor; it cannot authorize or resume an analysis."""

    elf_sha256: str
    input_sha256: str
    source_context_sha256: str

    def __post_init__(self) -> None:
        require(
            all(
                type(v) is str and re.fullmatch("[0-9a-f]{64}", v)
                for v in (self.elf_sha256, self.input_sha256, self.source_context_sha256)
            ),
            "execution scope identity is not a bounded digest",
        )

    def to_record(self) -> ExecutionRecord:
        from config_boot_history import _EXTERNAL

        return ExecutionRecord(
            schema=1,
            kind="named_ordinary_execution",
            scope=EXECUTION_SCOPE,
            scope_sha256=EXECUTION_SCOPE_SHA256,
            domain=EXECUTION_DOMAIN,
            elf_sha256=self.elf_sha256,
            input_sha256=self.input_sha256,
            source_context_sha256=self.source_context_sha256,
            all_observers_compatible=False,
            exception_absence_established=False,
            exception_safe_completion=False,
            unconditional_owner=False,
            hardware_qualified=False,
            exception_graph_limit=["NMI", "HardFault", "reset_reentry"],
            pending_exception_frontier=list(EXCEPTION_FRONTIER),
            external_qualifications=list(_EXTERNAL),
            requires_H8_consumed_input_closure=True,
            requires_M2_pre_init_source_reconciliation=True,
        )


def _execution_receipt(
    policy: PlatformPolicy, source_context: object, elf: MappedImage
) -> ExecutionReceipt:
    """Format current metadata only, separately from the private producer.

    Validate the exact typed source context/current ELF before its canonical
    digest. A bound source pair or policy still proves no source semantics,
    completed Widget, compiler-consumed inputs, graph or final stack pass.
    """
    from config_build_source import validate_source_context

    require(type(policy) is PlatformPolicy, "execution scope lacks a typed current platform policy")
    fresh = current_image_policy(policy.to_record(), elf)
    require(fresh == policy, "execution scope platform identity differs")
    checked_source = validate_source_context(source_context, elf)
    return ExecutionReceipt(
        policy.elf_sha256,
        policy.input_sha256,
        hashlib.sha256(checked_source.to_bytes()).hexdigest(),
    )


def read_execution_scope(
    value: object, policy: PlatformPolicy, source_context: object, *, elf: MappedImage
) -> ExecutionReceipt:
    """Return a receipt only; do not manufacture private completion from JSON."""
    record = object_fields(value, "execution scope record has missing or extra fields")
    receipt = _execution_receipt(policy, source_context, elf)
    wanted = receipt.to_record()
    require(
        type(record) is dict and set(record) == set(wanted),
        "execution scope record has missing or extra fields",
    )
    require(
        type(record["schema"]) is int
        and record["schema"] == 1
        and all(type(record[k]) is str for k, v in wanted.items() if type(v) is str)
        and all(record[k] is v for k, v in wanted.items() if type(v) is bool)
        and all(
            type(record[k]) is list
            and all(type(v) is str for v in array_items(record[k], "execution scope list differs"))
            for k, v in wanted.items()
            if type(v) is list
        ),
        "execution scope types or qualifications differ",
    )
    require(record == wanted, "execution scope declaration/frontier or current identity differs")
    _bounded_record(record)
    return receipt


def require_exception_safe_execution(receipt: ExecutionReceipt) -> None:
    """Fixed stronger-use boundary; no caller-selected approval or upgrade."""
    require(
        type(receipt) is ExecutionReceipt,
        "exception-safe consumer lacks checked execution metadata",
    )
    require(
        receipt.to_record()["exception_safe_completion"] is True,
        "named ordinary receipt cannot authorize exception-safe completion",
    )


def require_global_execution_owner(receipt: ExecutionReceipt) -> None:
    """Reject global/asynchronous use before any bounds or state is consumed."""
    require(
        type(receipt) is ExecutionReceipt,
        "global ownership consumer lacks checked execution metadata",
    )
    require(
        receipt.to_record()["unconditional_owner"] is True,
        "named ordinary receipt cannot authorize global ownership",
    )
