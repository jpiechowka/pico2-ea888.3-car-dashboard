"""Finite normal main-to-once effects, with unresolved source/diagnostic joins.

The complete compatible entry grammars fix the actual producers, loops,
mode-specific initializer ABIs and restored children. This consumer binds
those effects to Reset-derived S, the live nonreturning executor and the same
boot object. It never accepts caller-selected addresses, source flags or
phase records. It is not an acquired Config owner or a whole-graph result.
"""

from collections.abc import Mapping
from typing import TypedDict
from config_body_types import InstructionMap
from config_entry import (
    EntryRecord,
    BOOT_POOL,
    DASHBOARD_POOL,
    CONFIG,
    ISSUER,
    STATICS,
    static_role,
    entry_object,
)
from config_entry_contracts import ENTRY_GRAMMARS
from config_owner_entry import OnceRetry, OnceTaken, _direct_target
from config_preclaim_contracts import WRITES
from config_stack import DiagnosticObjects, transfer_family
from config_startup import StartupRecord, startup_configuration_copy
from validation import require


class PreclaimWrite(TypedDict):
    phase: str
    shape: str
    instruction: int
    destination: str
    interval: tuple[int, int]
    scope: str
    applies_to_first_boot_invocation_only: bool


class PreclaimClaim(TypedDict):
    load: int
    store: int
    taken: int
    required_before_taken: int
    after_success_taken: int
    failed_exclusive_write_bytes: int
    conditions: tuple[str, str]
    retry: OnceRetry
    already_taken: OnceTaken
    loader_r0: int
    restored_before_loader: bool


class PendingDiagnostic(TypedDict):
    kind: str
    edges: tuple[tuple[str, int, int, str, int], ...]


class PreclaimRecord(TypedDict):
    mode: str
    phase: str
    startup: StartupRecord
    allocations: dict[str, tuple[int, int]]
    static_producer_bindings: tuple[tuple[str, int, str, int], ...]
    executor: tuple[int, int]
    normal_stack_interval: tuple[int, int]
    writes: tuple[PreclaimWrite, ...]
    Config_write_intersections: tuple[tuple[int, int, str], ...]
    publication_attempts: tuple[int, ...]
    detached_consume: tuple[int, ...]
    task: tuple[int, int]
    callback: tuple[int, int, int]
    future: tuple[int, int, int, int]
    callback_expiry: str
    claim: PreclaimClaim
    pending_diagnostics: tuple[PendingDiagnostic, ...]
    pending_other_shared_invocations: tuple[tuple[int, str, str, int], ...]
    alternatives: tuple[str, ...]
    source_qualification: str
    requires_M2_pre_init_source_reconciliation: bool
    hardware_qualified: bool
    requires_complete_source_alias_and_diagnostic_closure: bool
    requires_first_batch_and_all_implicit_computed_ingress_closure: bool
    requires_same_B_specialized_loader_and_all_later_source_epochs: bool
    must_not_reuse_after_decode_refill_or_publication: bool
    existing_conservative_frame_arithmetic_unchanged: bool


def _allocations(objects: DiagnosticObjects, startup: StartupRecord) -> dict[str, tuple[int, int]]:
    result = {}
    for role in (BOOT_POOL, DASHBOARD_POOL, CONFIG, ISSUER):
        values = [v for v in objects.elf.symbol_entries if static_role(v[0]) == role]
        require(len(values) == 1, "preclaim static is missing or ambiguous")
        name, address, size, info, index = values[0]
        require(
            all(type(v) is int for v in (address, size, info, index)),
            "preclaim allocation metadata is untyped",
        )
        entry_object(objects, address, dict(kind="entry_static", role=role))
        result[role] = (address, STATICS[role][0])
    z, ze = startup["copy"]["bss"]
    require(
        all(
            z <= result[role][0] < result[role][0] + result[role][1] <= ze
            for role in (BOOT_POOL, DASHBOARD_POOL)
        ),
        "complete task pool is outside the actual Reset zero interval",
    )
    require(
        result[CONFIG] == (startup["entry"]["initial"]["address"], 14604),
        "preclaim Config differs from the copied initial value",
    )
    return result


def _activation(mode: str, phase: str) -> tuple[int, int]:
    # E is this invocation's entry SP relative to the same actual Reset S.
    # Sequential initializer/spawn/push children are not retained with poll.
    fixed = dict(
        main=(0, 8),
        cortex_main=(-8, 16),
        run=(-24, 16),
        boot_initializer=(-40, 8),
        raw_spawn=(-40, 8),
        queue_push=(-48, 8),
        raw_poll=(-40, 16),
        callback_install=(-40, 0),
    )
    if phase in ("boot_poll", "configuration_initialize", "dashboard_initializer"):
        poll = -56 if mode == "sizez" else -40
        return (poll, 32) if phase == "boot_poll" else (poll - 32, 8)
    require(phase in fixed, "unreviewed preclaim activation")
    return fixed[phase]


def _stack(
    objects: DiagnosticObjects, startup: StartupRecord
) -> tuple[int, tuple[int, int], tuple[int, int]]:
    mode = startup["entry"]["mode"]
    sp = startup["copy"]["initial_sp"]
    extent = 96 if mode == "sizez" else 80
    lo = sp - extent
    require(
        startup["copy"]["stack_bottom"] <= lo < sp and lo % 8 == 0,
        "preclaim stack leaves the actual stack allocation",
    )
    for section in objects.elf.sections:
        if not section[2] & 2 or not section[5]:
            continue
        a, b = section[3], section[3] + section[5]
        require(
            0 <= a < b <= 0x100000000 and not (a < sp and lo < b),
            "preclaim stack intersects an allocated section",
        )
    for segment in objects.elf.segments:
        require(
            len(segment) == 8 and all(type(v) is int for v in segment),
            "preclaim stack has malformed load metadata",
        )
        if segment[0] != 1 or not segment[5]:
            continue
        a, b = segment[2], segment[2] + segment[5]
        require(
            0 <= a < b <= 0x100000000 and not (a < sp and lo < b),
            "preclaim stack intersects a LOAD memory allocation",
        )
    for name, a, size, info, index in objects.elf.symbol_entries:
        if info & 15 != 1 or not size or not 0 <= index < len(objects.elf.sections):
            continue
        if objects.elf.sections[index][2] & 2:
            require(
                0 <= a < a + size <= 0x100000000 and not (a < sp and lo < a + size),
                "preclaim stack intersects an allocated object",
            )
    return sp, (lo, sp), (sp - 24, 8)


def _static_bindings(
    phases: Mapping[str, EntryRecord], allocations: Mapping[str, tuple[int, int]]
) -> tuple[tuple[str, int, str, int], ...]:
    # Full-body matching binds each PC/MOV producer to its typed descriptor.
    # The same source role in another phase must name the same whole object.
    bindings = []
    for phase, proof in phases.items():
        grammar = ENTRY_GRAMMARS[proof["shape"]]
        for i, value in enumerate(grammar["objects"]):
            if value["kind"] != "entry_static":
                continue
            role = value["role"]
            address = proof["objects"][i]
            require(
                role in allocations and address == allocations[role][0],
                "preclaim producer selects another whole static",
            )
            bindings.append((phase, i, role, address))
    require(
        any(p == "run" and role == BOOT_POOL for p, i, role, a in bindings),
        "preclaim task has no actual run producer",
    )
    return tuple(bindings)


def _ingress(
    rows: InstructionMap, startup: StartupRecord, phases: Mapping[str, EntryRecord]
) -> tuple[tuple[int, str, str, int], ...]:
    allowed = {}
    for proof in phases.values():
        for offset, operation, target, role, frame in proof["ordinary_edges"]:
            allowed[(proof["function"] + offset, target)] = operation
    reset = startup["reset"]
    allowed[(reset["function"] + 56, phases["main"]["function"])] = "bl"
    shared = []
    for at, row in rows.items():
        family = transfer_family(row.mnemonic.removesuffix(".w"))
        if family not in ("b", "bl", "cbz", "cbnz"):
            continue
        target = _direct_target(row, family)
        for phase, proof in phases.items():
            begin = proof["function"]
            end = begin + proof["extent"]
            if not begin <= target < end or begin <= at < end:
                continue
            if (
                target == begin
                and phase in ("raw_spawn", "raw_poll", "queue_push", "callback_install")
                and (at, target) not in allowed
            ):
                # Shared code has other real invocation contexts (core1,
                # Dashboard and wake). Exact entry preserves its local ABI;
                # it supplies NO first-boot identity or preclaim predecessor.
                # Source lifetime/visibility must close these separately.
                shared.append((at, row.mnemonic, phase, target))
            else:
                require(
                    target == begin and allowed.get((at, target)) == row.mnemonic,
                    "foreign preclaim direct ingress bypasses a producer or phase",
                )
    # Indirect transfers elsewhere remain a separate whole-graph obligation.
    # Only the actual selected consume edge is contextualized below.
    return tuple(shared)


def _writes(
    phases: Mapping[str, EntryRecord],
    mode: str,
    sp: int,
    allocations: Mapping[str, tuple[int, int]],
    executor: tuple[int, int],
) -> tuple[tuple[PreclaimWrite, ...], tuple[tuple[int, int, str], ...]]:
    names = {"boot": BOOT_POOL, "dashboard": DASHBOARD_POOL, "Config": CONFIG}
    records: list[PreclaimWrite] = []
    intersections: list[tuple[int, int, str]] = []
    config, config_size = allocations[CONFIG]
    for phase, proof in phases.items():
        shape = proof["shape"]
        require(shape in WRITES, "unreviewed preclaim write shape")
        rows = WRITES[shape]
        grammar = ENTRY_GRAMMARS[shape]
        actual = {
            offset
            for offset, width, text in grammar["instructions"]
            if text.partition(" ")[0]
            .removesuffix(".w")
            .startswith(("push", "vpush", "str", "vstr", "stm", "vstm", "strex", "stlex"))
        }
        require(
            len({r[0] for r in rows}) == len(rows) and {r[0] for r in rows} == actual,
            "preclaim effect partition omits or duplicates a complete-body write",
        )
        entry, frame = _activation(mode, phase)
        entry += sp
        require(frame == proof["frame"], "preclaim activation frame differs")
        for offset, destination, displacement, width, scope in rows:
            require(
                scope
                in (
                    "before",
                    "after",
                    "diagnostic",
                    "after_diagnostic",
                    "claim",
                    "first_invocation_only",
                )
                and all(type(v) is int for v in (offset, displacement, width))
                and width > 0,
                "malformed authored preclaim effect",
            )
            if destination == "stack":
                base = entry
                low = entry - frame
                high = entry
            elif destination == "executor":
                base, extent = executor
                low = base
                high = base + extent
            else:
                require(destination in names, "unreviewed preclaim destination")
                base, extent = allocations[names[destination]]
                low = base
                high = base + extent
            a, b = base + displacement, base + displacement + width
            require(low <= a < b <= high, "preclaim write exceeds its actual live destination")
            after = scope in ("after", "after_diagnostic")
            if not after and a < config + config_size and config < b:
                require(
                    scope == "claim"
                    and phase == "configuration_initialize"
                    and destination == "Config"
                    and (displacement, width) == (14600, 1),
                    "preclaim application effect changes copied Config before the claim",
                )
                intersections.append((a, width, "claim_transition"))
            elif not after:
                require(destination != "Config", "preclaim Config write has lost its claim epoch")
            records.append(
                dict(
                    phase=phase,
                    shape=shape,
                    instruction=proof["function"] + offset,
                    destination=destination,
                    interval=(a, b),
                    scope=scope,
                    applies_to_first_boot_invocation_only=scope == "first_invocation_only",
                )
            )
    require(
        intersections == [(config + 14600, 1, "claim_transition")],
        "preclaim partition has a missing or additional Config claim",
    )
    return tuple(records), tuple(intersections)


def preclaim_configuration_preservation(
    objects: DiagnosticObjects, instructions: InstructionMap
) -> PreclaimRecord:
    """Bind normal physical effects; diagnostics/source/loader still fail closed.

    The compatible complete machine shapes retain the source-reviewed null
    executor construction, one-slot claim, mode-specific future initialization,
    publication CAS, detach/save/clear/callback ordering, boot guard and once
    retry/restore. No failure path is promoted to a successful claim. Internal
    private-source/alias and diagnostic closure are deliberately not replaced
    by a caller boolean or by the platform's conditional memory premise.
    """
    startup = startup_configuration_copy(objects, instructions)
    boundary = startup["entry"]
    entry = boundary["entry"]
    phases = entry["phases"]
    mode = boundary["mode"]
    allocations = _allocations(objects, startup)
    sp, stack, executor = _stack(objects, startup)
    bindings = _static_bindings(phases, allocations)
    shared = _ingress(instructions, startup, phases)
    writes, intersections = _writes(phases, mode, sp, allocations, executor)
    consumer = phases["raw_poll" if mode == "sizez" else "run"]
    boot = phases["boot_poll"]
    # These sites are fixed interfaces of the already matched complete bodies.
    publication = {
        "speed2": ("run", (66, 86, 106, 128)),
        "sizes": ("run", (96,)),
        "sizez": ("queue_push", (18,)),
    }[mode]
    poll_offsets = {
        "speed2": (208, 220, 222, 224, 226, 228),
        "sizes": (128, 140, 142, 144, 146, 148),
        "sizez": (12, 24, 26, 28, 30, 32),
    }[mode]
    attempts = tuple(phases[publication[0]]["function"] + i for i in publication[1])
    require(
        all(
            any(
                v["instruction"] == at
                and v["destination"] == "executor"
                and v["scope"] in ("before", "first_invocation_only")
                for v in writes
            )
            for at in attempts
        ),
        "preclaim publication loses an actual inherited-P store",
    )
    require(
        entry["callback_edge"]
        == dict(
            caller=consumer["function"],
            offset=poll_offsets[-1],
            target=boot["function"],
            scope="first detached boot batch",
        )
        and boundary["boot_call"] == boot["function"] + 22,
        "preclaim callback or unique synchronous claim seam differs",
    )
    boot_base = allocations[BOOT_POOL][0]
    once = boundary["once"]
    diagnostics: tuple[PendingDiagnostic, ...] = tuple(
        dict(
            kind=kind,
            edges=tuple(
                v
                for v in entry["external_edges"]
                if v[0] == phase
                and (
                    (kind == "resumed" and "panic_const_async_fn_resumed" in v[3])
                    or (kind == "Busy" and "unwrap_failed" in v[3])
                    or (
                        kind == "already_taken"
                        and ("panic_fmt" in v[3] or "initialize_clrex_panic" in v[3])
                    )
                )
            ),
        )
        for kind, phase in (
            ("Busy", "run"),
            ("resumed", "boot_poll"),
            ("already_taken", "configuration_initialize"),
        )
    )
    require(
        all(len(v["edges"]) == 1 for v in diagnostics),
        "preclaim diagnostic partition loses an actual descendant seam",
    )
    return dict(
        mode=mode,
        phase="normal_preclaim_preserved_pending_source_diagnostic_and_loader_closure",
        startup=startup,
        allocations=allocations,
        static_producer_bindings=bindings,
        executor=executor,
        normal_stack_interval=stack,
        writes=writes,
        Config_write_intersections=intersections,
        publication_attempts=attempts,
        detached_consume=tuple(consumer["function"] + i for i in poll_offsets),
        task=(boot_base, 48),
        callback=(consumer["function"] + poll_offsets[-1], boot["function"], boot_base),
        future=(boot_base + 40, executor[0], boot_base + 44, 0),
        callback_expiry="before next shared dequeue",
        claim=dict(
            load=once["success"]["load"],
            store=once["success"]["store"],
            taken=once["taken_address"],
            required_before_taken=0,
            after_success_taken=1,
            failed_exclusive_write_bytes=0,
            conditions=once["success"]["predicate"],
            retry=once["retry"],
            already_taken=once["already_taken"],
            loader_r0=boundary["loader"]["incoming_r0"],
            restored_before_loader=True,
        ),
        pending_diagnostics=diagnostics,
        pending_other_shared_invocations=shared,
        alternatives=(
            "successful_zero_claim_then_restored_loader_tail",
            "exclusive_failure_no_write_retry",
            "empty_or_wait_no_claim_successor",
            "Busy_diagnostic_pending",
            "resumed_diagnostic_pending",
            "already_taken_diagnostic_pending",
        ),
        source_qualification=startup["source_qualification"],
        requires_M2_pre_init_source_reconciliation=startup[
            "requires_M2_pre_init_source_reconciliation"
        ],
        hardware_qualified=False,
        requires_complete_source_alias_and_diagnostic_closure=True,
        requires_first_batch_and_all_implicit_computed_ingress_closure=True,
        requires_same_B_specialized_loader_and_all_later_source_epochs=True,
        must_not_reuse_after_decode_refill_or_publication=True,
        existing_conservative_frame_arithmetic_unchanged=True,
    )
