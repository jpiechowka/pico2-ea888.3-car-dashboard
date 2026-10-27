"""Reject reviewed ARM stack-frame regressions using LLVM ELF metadata.

This is a conservative admission gate for named, source-reviewed call paths,
not a whole-program stack proof or a device high-water measurement. Changes to
those paths/dependencies require disassembly review of the child allowances.
See docs/configurator/configuration-storage.md. Never increase budgets to pass.
"""

from collections.abc import Callable, Mapping
from dataclasses import dataclass
from typing import NotRequired, TypeAlias, TypedDict
from elf_image import InspectableImage
import re
import struct
from validation import require

CORE1_BYTES = 32768
CORE1_RESERVE = 4096
CORE0_RESERVE = 8192


FunctionTable: TypeAlias = Mapping[str, tuple[int, int, int]]
FrameSizes: TypeAlias = Mapping[int, int]


class FrameBounds(TypedDict):
    boot: int
    constructor: int
    runtime: int
    dashboard: int
    radio: NotRequired[int]


class FrameRecord(TypedDict):
    schema: int
    method: str
    bounds: FrameBounds
    core0_reserve: int
    core1_bytes: int
    core1_reserve: int


@dataclass(frozen=True)
class FrameAdmission:
    """Conservative named paths only; never a complete execution proof."""

    boot: int
    constructor: int
    runtime: int
    dashboard: int
    radio: int | None = None

    def to_record(self) -> FrameRecord:
        values: FrameBounds = {
            "boot": self.boot,
            "constructor": self.constructor,
            "runtime": self.runtime,
            "dashboard": self.dashboard,
        }
        if self.radio is not None:
            values["radio"] = self.radio
        require(
            all(type(value) is int and 0 <= value <= 0xFFFFFFFF for value in values.values()),
            "invalid conservative stack bound",
        )
        return dict(
            schema=1,
            method="conservative_named_frames_v1",
            bounds=values,
            core0_reserve=CORE0_RESERVE,
            core1_bytes=CORE1_BYTES,
            core1_reserve=CORE1_RESERVE,
        )


def frames(data: bytes | bytearray) -> dict[int, int]:
    require(data, "missing stack frame metadata")
    result: dict[int, int] = {}
    cursor = 0
    while cursor < len(data):
        require(cursor + 4 <= len(data), "truncated stack function address")
        address = struct.unpack_from("<I", data, cursor)[0] & ~1
        cursor += 4
        size = 0
        for index in range(5):
            require(cursor < len(data), "truncated stack size")
            byte = data[cursor]
            cursor += 1
            size |= (byte & 127) << (index * 7)
            require(size <= 0xFFFFFFFF, "stack size overflow")
            if byte < 128:
                require(index == 0 or byte != 0, "noncanonical stack size")
                break
        else:
            raise AssertionError("unterminated stack size")
        require(address not in result or result[address] == size, "conflicting stack sizes")
        result[address] = size
    return result


def budgets(
    boot: int,
    constructor: int,
    runtime: int,
    dashboard: int,
    headroom: int,
    radio: int | None = None,
) -> None:
    require(boot + CORE0_RESERVE <= headroom, "boot stack path lacks 8 KiB reserve")
    require(dashboard + CORE0_RESERVE <= headroom, "dashboard stack path lacks 8 KiB reserve")
    require(
        constructor + CORE1_RESERVE <= CORE1_BYTES, "core1 constructor stack lacks 4 KiB reserve"
    )
    require(runtime + CORE1_RESERVE <= CORE1_BYTES, "core1 runtime stack lacks 4 KiB reserve")
    if radio is not None:
        require(radio + CORE0_RESERVE <= headroom, "radio stack path lacks 8 KiB reserve")


def task_frames(functions: FunctionTable, sizes: FrameSizes, name: str) -> int:
    matches = {
        address & ~1
        for symbol, (address, _, _) in functions.items()
        if name in symbol and "write_in_place" not in symbol
    }
    polls = [
        symbol
        for symbol in functions
        if name in symbol and "TaskStorage" in symbol and "4poll" in symbol
    ]
    require(len(polls) == 1, "missing or ambiguous task poll: " + name)
    require(all(address in sizes for address in matches), "missing task frame metadata: " + name)
    # LLVM may outline the future body behind a tiny TaskStorage::poll shim.
    # Count all task-associated bodies/closures conservatively, not only the shim.
    return sum(sizes[address] for address in matches)


# LLVM 23 / nightly 2026-10-16 ARM release: exact identities, only crate
# disambiguators normalized. No module-wide or generic unwrap exemption.
REVIEWED_STARTUP_FRAMES = {
    # Exact observed helpers: header parsing is inlined in new; u32_at is a
    # bounded leaf. Geometry assert adapter calls existing panic formatting.
    "_RNvMs1_NtCs_16dashboard_config13startup_assetNtB5_9Validator3new": 16,
    "_RNvNtCs_16dashboard_config13startup_asset6u32_at": 16,
    "_RINvNtCs_4core9panicking13assert_failedNtNtCs_16dashboard_config11orientation8GeometryBM_ECs_14dashboard_core": 32,
    # Oct17 portrait ELF: geometry-carrying frame return grows 24 -> 32 B.
    "_RNvMNtCs_16dashboard_config13startup_assetNtB2_5Asset5frame": 32,
    "_RNvNtCs_16dashboard_config13startup_asset10crc_update": 32,
    "_RINvNtCs_4core6result13unwrap_failedNtNtNtB4_3str5error9Utf8ErrorECs_16dashboard_config": 16,
    "_RINvNtCs_4core6result13unwrap_failedNtNtCs_16dashboard_config8validate15ValidationErrorEBN_": 16,
}


def startup_frames(functions: FunctionTable, sizes: FrameSizes) -> tuple[set[str], set[int]]:
    names: set[str] = set()
    addresses: set[int] = set()
    for identity, ceiling in REVIEWED_STARTUP_FRAMES.items():
        matches = {n for n in functions if re.sub(r"Cs[A-Za-z0-9]+_", "Cs_", n) == identity}
        entries = {functions[n][0] & ~1 for n in matches}
        require(len(entries) <= 1, "ambiguous reviewed startup helper: " + identity)
        for address in entries:
            size = sizes.get(address)
            if type(size) is not int or not 0 <= size <= ceiling:
                raise AssertionError("missing or enlarged startup helper frame: " + identity)
        names.update(matches)
        addresses.update(entries)
    return names, addresses


def configuration_frames(functions: FunctionTable, sizes: FrameSizes) -> int:
    """Reviewed slot call tree, with sibling branches taking their maximum.

    Required emitted boundaries come from the traced LLVM 23 ARM captures.
    Header/ConfigSeed/VecSeed/default filling are inlined in the slot loader;
    if emitted, their frames are added at their source call-tree positions.
    Unknown config helpers, including validation descendants, require review.
    This is intentionally a named-path gate, not an instruction decoder.
    """
    covered: set[str] = set()

    def select(*tokens: str, optional: bool = False) -> set[int]:
        names = [n for n in functions if all(t in n for t in tokens)]
        addresses = {functions[n][0] & ~1 for n in names}
        require(
            optional or len(addresses) == 1,
            "missing or ambiguous config stack path: " + str(tokens),
        )
        require(optional or bool(names), "missing config stack path")
        require(all(a in sizes for a in addresses), "missing config frame metadata: " + str(tokens))
        covered.update(names)
        return addresses

    def weight(path: set[int]) -> int:
        return sum(sizes[a] for a in path)

    def branch(parent: set[int], *children: set[int]) -> set[int]:
        # Deduplicate aliases across the complete simultaneously resident path.
        return max((parent | c for c in children), key=weight) if children else parent

    def dec(token: str) -> set[int]:
        return select("dashboard_config", "12next_element", token)

    def ser(token: str) -> set[int]:
        return select("dashboard_config", "15serialize_field", token)

    def outlined(*tokens: str) -> set[int]:
        return select("dashboard_config", *tokens, optional=True)

    # Oct17 v5 demo280 source/ELF review: Widget decoding is inlined into
    # load_at_boot (1280 B observed); Secondary serialization is folded into
    # the emitted Orientation serializer called by Widget (72 B). Only these
    # wrappers may disappear; their required
    # children and actual parent frames still contribute. A later outline is
    # counted, with multiple distinct boundaries rejected. This is not a
    # general exemption for missing helpers or an instruction-effects proof.
    orientation = any("11orientation11Orientation" in n for n in functions)
    ble = any("3ble11BleSettings" in n for n in functions)
    ignition = any("27IgnitionWarningPresentation" in n for n in functions)

    def reviewed_inline(*tokens: str) -> set[int]:
        addresses = select("dashboard_config", *tokens, optional=True)
        require(len(addresses) <= 1, "ambiguous reviewed inline stack path: " + str(tokens))
        return addresses

    def reviewed_method(identity: str) -> set[int]:
        """Count an optional exact mangled method, normalizing only crate disambiguators."""
        names = {n for n in functions if re.sub(r"Cs[A-Za-z0-9]+_", "Cs_", n) == identity}
        addresses = {functions[n][0] & ~1 for n in names}
        require(len(addresses) <= 1, "ambiguous reviewed inline stack path: " + identity)
        require(all(a in sizes for a in addresses), "missing config frame metadata: " + identity)
        covered.update(names)
        return addresses

    loader = select("dashboard_config", "ConfigSlot", "12load_at_boot")
    # Shared generic helpers embed validate_ref in their callback type names.
    # Match the actual free-function entry rather than those child symbols.
    validate = select("dashboard_config", "8validate12validate_ref")
    canonical = select("dashboard_config", "17compare_canonical")
    utf8 = select("4core", "9from_utf8")
    # The only boot children without .stack_sizes are compiler-builtins memory
    # routines: memcpy 72 B, memcpy4 <=16 B then tail-calls memcpy, memcmp 32 B.
    # They and panic formatting/entry/executor children fit the separate 1024 B
    # allowance in check(). UTF-8 has metadata and is counted explicitly here.
    view = branch(dec("4View"), utf8)
    text = branch(dec("11StringInner"), utf8)
    color = dec("5ColorE")
    warning_colors = dec("13WarningColors")
    v4 = any("15AdvancedDisplay" in n for n in functions)
    warning_children = [text, color]
    if v4:
        warning_children.extend([dec("8Priority"), dec("12next_elementb")])
    warning = branch(dec("15WarningSettings"), *warning_children)
    widget_colors = branch(dec("12WidgetColors"), color, warning_colors)
    widget_children = [utf8, warning, widget_colors, dec("10GraphRange"), dec("5Gauge")]
    if v4:
        advanced_children = [utf8, dec("13PeakSelection")]
        if ignition:
            # Oct27 schema7 LLVM23: AdvancedDisplay384 -> popup decoder16.
            # The selector is a leaf; it is not a sibling of AdvancedDisplay.
            advanced_children.append(dec("27IgnitionWarningPresentation"))
        widget_children.append(branch(dec("15AdvancedDisplay"), *advanced_children))
    widget_parent = (
        reviewed_inline("12next_element", "6WidgetE") if orientation else dec("6WidgetE")
    )
    widget = branch(widget_parent, *widget_children)
    source = branch(select("dashboard_config", "12next_elementNt", "13ChannelSource"), color)
    # LLVM outlines the v4 array in ConfigSeed::next instead of SeqAccess.
    # Both reviewed entry points are accepted; a missing/ambiguous one fails.
    array_token = "15decode_in_place4nextA" if v4 else "12next_elementA"
    sources = branch(select("dashboard_config", array_token, "13ChannelSource"), source)
    controls = branch(
        select("dashboard_config", "15decode_in_place4next", "10ControlMap"),
        branch(dec("14ScreenBindings"), dec("13ControlAction")),
    )
    fields = [sources, controls]
    if ble:
        # LLVM folds BleSettings u16 decoding into the emitted Color decoder.
        # Compact ELF 0x10087a66 calls Color at0x10084d8c; keep that child.
        fields.append(
            branch(
                select("dashboard_config", "15decode_in_place4next", "3ble11BleSettings"),
                dec("5ColorE"),
            )
        )
    if orientation:
        fields.append(
            select("dashboard_config", "15decode_in_place4next", "11orientation11Orientation")
        )
    if v4:
        fields.extend(
            [
                select("dashboard_config", "15decode_in_place4nextAtj8_"),
                branch(
                    select("dashboard_config", "15decode_in_place4nextb"), dec("12next_elementb")
                ),
                branch(
                    select("dashboard_config", "15decode_in_place4next", "11LedSettings"),
                    dec("10LedPattern"),
                    dec("12next_elementb"),
                ),
            ]
        )
    for token in ("19WarningPresentation", "4Fuel", "13WarningColors"):
        fields.append(select("dashboard_config", "15decode_in_place4next", token))
    # ConfigSeed and VecSeed may each emit deserialize and visit_seq frames.
    # The two vector instantiations are siblings, never simultaneous.
    fields.extend(
        [branch(outlined("VecSeed", "4View"), view), branch(outlined("VecSeed", "6Widget"), widget)]
    )
    seed = branch(outlined("ConfigSeed"), *fields)
    header = outlined("16inspect_envelope") | outlined("9crc_parts")
    serial_widget_children = [ser("5ColorE"), ser("13WarningColors")]
    orientation_serializer = ser("11orientation11Orientation") if orientation else set()
    if orientation:
        # Frozen ELF: Widget at0x1004888a calls Orientation at0x10049bd4
        # for AdvancedDisplay::secondary. Preserve this folded child under
        # Widget even if Secondary has no symbol. Address sets deduplicate
        # any retained alias; distinct emitted Secondary remains counted.
        serial_widget_children.append(orientation_serializer)
    if v4:
        secondary = (
            reviewed_inline("15serialize_field", "9Secondary") if orientation else ser("9Secondary")
        )
        serial_widget_children.extend([utf8, ser("7Readout"), secondary, ser("13PeakSelection")])
    if ignition:
        # Oct27: compare_canonical56 -> Widget72 -> popup serializer8.
        serial_widget_children.append(ser("27IgnitionWarningPresentation"))
    serial_widget = branch(ser("6Widget"), *serial_widget_children)
    serial_fuel = branch(ser("4Fuel"), ser("serialize_fieldf"))
    serial_controls = branch(ser("10ControlMap"), ser("13ControlAction"))
    serial_children = [
        serial_widget,
        serial_fuel,
        serial_controls,
        ser("13ChannelSource"),
        ser("19WarningPresentation"),
        ser("13WarningColors"),
    ]
    if v4:
        serial_children.extend(
            [
                ser("serialize_fieldAtj8_"),
                branch(ser("11LedSettings"), ser("10LedPattern"), ser("serialize_fieldb")),
            ]
        )
    if orientation:
        serial_children.append(orientation_serializer)
    if ble:
        # Same folding for u16 serialization: BleSettings calls Color twice.
        serial_children.append(branch(ser("3ble11BleSettings"), ser("5ColorE")))
    canonical = branch(canonical, *serial_children)
    guarded = any(
        "19visit_invalid_roles" in n or "19visit_stoichiometry" in n
        for n in functions
        if "dashboard_config" in n
    )
    if guarded:
        # Traced Oct12 ARM: visit_validation/visit_controls and callback bodies
        # inline into validate_ref. Colour/fuel visitors emit leaf siblings
        # (contrast is inlined in the colour leaf), not a nested visitor frame.
        # A newly outlined visitor is intentionally still an unknown helper.
        colour_validation = select("dashboard_config", "12WidgetColors19visit_invalid_roles")
        fuel_validation = select("dashboard_config", "4fuel19visit_stoichiometry")
    else:
        # Retain admission for frozen predecessor captures.
        colour_validation = select("dashboard_config", "12WidgetColors12invalid_role")
        fuel_validation = select("dashboard_config", "4fuel13stoichiometry")
    if ignition:
        # Oct27 ig27-d2 validate_ref312 directly calls compatible_widget8.
        # Gauge::valid_domain finite/range/span checks inline into validate_ref
        # at0x100476d0..0x1004772c. Count that exact method if later outlined;
        # unknown descendants still fail below. Frozen shapes retain Gauge::valid.
        gauge_validation = reviewed_method(
            "_RNvMs0_NtCs_16dashboard_config10appearanceNtB5_5Gauge12valid_domain"
        )
        readout_validation = select("dashboard_config", "7Readout17compatible_widget")
    else:
        gauge_validation = select("dashboard_config", "5Gauge5valid")
        readout_validation = set()
    validation = branch(
        validate,
        colour_validation,
        fuel_validation,
        gauge_validation,
        readout_validation,
        outlined("8controls17validate_controls"),
        outlined("8validate10text_valid"),
        *[
            select("dashboard_config", token)
            for token in (
                "9Placement11occupied_in" if orientation else "9Placement8occupied",
                "5Color9contrasts",
                "7catalog10definition",
                "6AccessNt",
            )
        ],
    )
    decoding = branch(outlined("19decode_current_into"), header, seed, canonical, validation)
    fallback = branch(
        outlined("19fill_default_config"),
        outlined("14default_widget"),
        select("dashboard_config", "7catalog10definition"),
    )
    # Validation follows decode/canonical or refill after their children return.
    result = branch(loader, decoding, validation, fallback)
    # Error Debug formatting is already covered by check()'s reviewed 1024 B
    # panic/entry allowance. Still require metadata and retain its observed
    # maximum single frame (80 B); changes require re-review of that allowance.
    debug = outlined("5Debug3fmt")
    # V4 Field diagnostics enlarge ValidationError Debug to112 B. Its reviewed
    # nested formatting path still fits the unchanged1024 B panic allowance.
    require(
        all(sizes[a] <= (112 if v4 else 80) for a in debug),
        "config panic formatting frame changed; review allowance",
    )
    # These three equality implementations serve dashboard cache comparisons,
    # not the boot tree. Access equality above IS a validation child. Do not
    # broadly exempt PartialEq, whole modules or names containing "validate".
    covered.update(
        n
        for n in functions
        if "dashboard_config" in n
        and "3cmp9PartialEq2eq" in n
        and any(t in n for t in ("14DisplayOptionsNt", "15WarningSettingsNt", "12WidgetColorsNt"))
    )
    # This one runtime snapshot contains dashboard_config in its generic pointee.
    # It is a required/countable publication boundary below, not a boot child.
    # Do not exempt other owner methods or unknown config helpers.
    covered.update(
        n
        for n in functions
        if all(
            token in n
            for token in (
                "14dashboard_core11acquisitionI",
                "16AcquisitionOwnerR",
                "16dashboard_config8validate15ValidatedConfigE8snapshot",
            )
        )
    )
    # CRC is a leaf; frame() only calls bounds/slice panic children. The two
    # unwrap adapters call unwrap_failed_inner. Panic descendants retain the
    # existing 1024 B allowance. Conservatively add all emitted frames even
    # though CRC/render/validation branches are not simultaneously resident.
    startup_names, startup_addresses = startup_frames(functions, sizes)
    covered.update(startup_names)
    result |= startup_addresses
    relevant = [n for n in functions if "dashboard_config" in n]
    require(
        all(n in covered for n in relevant),
        "unreviewed config stack helpers: " + str([n for n in relevant if n not in covered]),
    )
    return weight(result)


def radio_frames(functions: FunctionTable, sizes: FrameSizes) -> int | None:
    """Optional core0 radio poll bound, reviewed against the compact BLE ELF.

    Async future storage is already static RAM. These are synchronous frames
    resident while polling. Host RX/control, serial, security initiation and
    CYW initialization/receive branches are alternatives, not a sum of every
    radio function. Radio initialization runs inside this task, not main;
    PIO/State/Bluetooth initialization inline into its counted body, with the
    emitted CYW Runner::init future counted separately below.
    Aliases are deduplicated across each path; missing boundaries fail closed.

    LLVM23/nightly2026-10-16 disassembly: the deepest observed direct chain is
    radio poll/body -> host select -> RX -> security channel -> Pairing input
    -> peripheral transition -> dh_key -> P256 mul -> double -> fe_mul.
    Peripheral transition remains emitted in this central-only host build.
    The initiation branch conservatively retains the same pairing/crypto tree.

    4096 B covers other reviewed children: crypto sibling paths (CMAC/AES,
    decompression and RNG), bounded ATT/GATT/ELM and bus helpers, logging,
    memory builtins, executor/entry and panic formatting. Largest alternative
    crypto chain was below 2048 B; ordinary ELM/bus frames below 1024 B.
    IRQ/preemption remains within the separate unchanged 8192 B reserve.
    Dependency/source changes still require child-allowance review. This does
    not establish indirect-call closure, recursion absence or device high-water.
    """
    if not any("radio_task_task" in n or "pico25tasks3ble" in n for n in functions):
        return None
    task = task_frames(functions, sizes, "radio_task_task")

    def select(*tokens: str) -> set[int]:
        entries = {
            a & ~1
            for n, (a, _, _) in functions.items()
            if "drop_glue" not in n and all(t in n for t in tokens)
        }
        require(len(entries) == 1, "missing or ambiguous radio stack path: " + str(tokens))
        require(all(a in sizes for a in entries), "missing radio frame metadata: " + str(tokens))
        return entries

    def weight(path: set[int]) -> int:
        return sum(sizes[a] for a in path)

    def branch(parent: set[int], *children: set[int]) -> set[int]:
        return max((parent | child for child in children), key=weight)

    if any("3ble4host" in n and "Future4poll" in n for n in functions):
        host = select("embassy_futures", "3ble4host", "Future4poll")
    else:
        # Mutex capture: host select inlines into Join::poll(3080 B), called
        # from TaskStorage::poll(768 B). Both already counted by task_frames.
        # Require that exact alternative boundary rather than allowing an
        # arbitrary missing host frame. Re-emission above remains additive.
        select("embassy_futures", "4Join", "radio_task_task", "Future4poll")
        host = set()
    connected = select("embassy_futures", "3ble17connection_events", "Future4poll")
    crypto = (
        select("trouble_host", "9SecretKey6dh_key")
        | select("primeorder", "ProjectivePoint", "3Mul", "3mul")
        | select("primeorder", "ProjectivePoint", "6Double6double")
        | select("p256", "6fe_mul")
    )
    pairing = (
        select("trouble_host", "7Pairing12handle_input")
        | select("trouble_host", "10peripheral", "7Pairing10transition")
        | crypto
    )
    security = branch(
        select("trouble_host", "23handle_security_channel") | pairing,
        select("trouble_host", "8RxRunner", "16run_with_handler"),
        select("trouble_host", "13ControlRunner", "3run0"),
    )
    serial = branch(
        select("pico25tasks3ble", "6Serial7receive"),
        select("pico25tasks3ble", "6Serial8transact"),
        select("pico25tasks3ble", "6Serial3gap"),
    )
    application = branch(connected, serial, select("trouble_host", "16request_security") | pairing)
    bus = select("5cyw43", "6Runner", "2rx")
    initialize = select("5cyw43", "6Runner", "4init0")
    return task + weight(branch(set(), host | security, host | application, bus, initialize)) + 4096


def publication_frames(functions: FunctionTable, sizes: FrameSizes) -> int:
    # Nested path: firmware publication -> immutable owner -> private storage
    # -> sensor snapshot. Retain legacy capture support for baseline comparison.
    split = any("AcquisitionOwner" in n or "AcquisitionState" in n for n in functions)
    boundaries = [("pico2", "publish_snapshot")]
    boundaries += (
        [("AcquisitionOwner", "8snapshot"), ("AcquisitionState", "8snapshot")]
        if split
        else [("AcquisitionModel", "8snapshot")]
    )
    for tokens in boundaries:
        boundary = {
            address & ~1
            for name, (address, _, _) in functions.items()
            if all(token in name for token in tokens)
        }
        require(len(boundary) == 1, "missing or ambiguous publication boundary: " + str(tokens))
    matches = {
        address & ~1
        for name, (address, _, _) in functions.items()
        if ("pico2" in name and "publish_snapshot" in name)
        or (
            any(
                token in name
                for token in (
                    "AcquisitionModel",
                    "AcquisitionOwner",
                    "AcquisitionState",
                    "Session",
                    "Vehicle",
                    "SensorState",
                )
            )
            and any(
                token in name
                for token in ("8snapshot", "tracked_snapshot", "snapshot_with_identity")
            )
        )
    }
    require(all(address in sizes for address in matches), "missing publication frame metadata")
    return sum(sizes[address] for address in matches)


def constructor_frames(functions: FunctionTable, sizes: FrameSizes) -> int:
    # Constructors may inline. Every emitted owner/storage/policy frame must be
    # counted, including the extra owner boundary; aliases count once by address.
    matches = {
        address & ~1
        for name, (address, _, _) in functions.items()
        if "configured_vehicle" in name
        or "configured_model" in name
        or ("OriginIssuer" in name and ("5issue" in name or "_5issue" in name))
        or (
            all(token in name for token in ("dashboard_config", "7catalog"))
            and any(token in name for token in ("14resolve_source", "10definition"))
        )
        or ("PreparedSession" in name and "8assemble" in name)
        or ("Vehicle" in name and any(token in name for token in ("8assemble", "13prepare_inner")))
        or (
            any(
                token in name
                for token in (
                    "AcquisitionModel",
                    "AcquisitionOwner",
                    "AcquisitionState",
                    "Session",
                    "Vehicle",
                    "RuleEngine",
                    "RuleEngineState",
                )
            )
            and any(
                token in name
                for token in (
                    "new_inner",
                    "with_policy",
                    "assemble_variant",
                    "session_base",
                    "construction_base",
                    "initialize_for_session",
                    "initialize_bound_policy",
                    "initialize_measurements",
                    "4base",
                    "with_configuration",
                    "configured",
                    "legacy",
                    "3new",
                )
            )
        )
    }
    require(all(address in sizes for address in matches), "missing constructor frame metadata")
    return sum(sizes[address] for address in matches)


def check(elf: InspectableImage, headroom: int, source_context: object = None) -> FrameAdmission:
    if source_context is not None:
        from config_build_source import inspect_source_context

        inspect_source_context(source_context, elf)
    require(".stack_sizes" in elf.named, "build with -Z emit-stack-sizes")
    section = elf.named[".stack_sizes"]
    require(section[1] == 1 and not section[2] & 2, "stack metadata must not be allocated")
    for kind, offset, _, _, size, _, _, _ in elf.segments:
        if kind == 1 and size:
            require(
                section[4] + section[5] <= offset or offset + size <= section[4],
                "stack metadata overlaps a LOAD segment",
            )
    sizes = frames(elf.section_bytes(".stack_sizes"))

    def selected(predicate: Callable[[str], bool], required: bool = False) -> int:
        matches = {elf.functions[name][0] & ~1 for name in elf.functions if predicate(name)}
        if required:
            require(len(matches) == 1, "missing or ambiguous stack path: " + str(matches))
        values = []
        for address in matches:
            require(address in sizes, "missing frame metadata for " + hex(address))
            values.append(sizes[address])
        return sum(values)

    def task(name: str) -> int:
        return task_frames(elf.functions, sizes, name)

    boot_poll = task("embassy_main_task")
    dashboard = task("dashboard_task_task")
    acquisition = task("acquisition_task_task")
    initializer = selected(lambda n: "view_config" in n and "initialize" in n, True)
    configuration = configuration_frames(elf.functions, sizes)
    constructor = constructor_frames(elf.functions, sizes)
    snapshots = publication_frames(elf.functions, sizes)
    # 1024 includes resident executor/entry frames, ordinary constructors'
    # memcpy/memclr, boot validation/contrast and snapshot memory helpers.
    # Vehicle step/reset/telemetry paths have an independently reviewed 8KiB
    # child allowance. Runtime and construction are different poll branches.
    boot_bound = boot_poll + initializer + configuration + 1024
    constructor_bound = acquisition + constructor + 1024
    runtime_bound = acquisition + max(snapshots + 1024, 8192)
    dashboard_bound = dashboard + 16384  # drawing/formatting, flush, startup children
    radio_bound = radio_frames(elf.functions, sizes)
    budgets(boot_bound, constructor_bound, runtime_bound, dashboard_bound, headroom, radio_bound)
    print(
        f"Frame admission: boot {boot_bound} B, core1 construct {constructor_bound} B, "
        f"core1 runtime {runtime_bound} B, dashboard {dashboard_bound} B, radio {radio_bound} B; "
        "excludes unqualified hardware high-water use"
    )
    return FrameAdmission(
        boot_bound, constructor_bound, runtime_bound, dashboard_bound, radio_bound
    )
