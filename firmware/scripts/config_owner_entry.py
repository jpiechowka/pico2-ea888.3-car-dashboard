"""Actual first-batch/once/loader boundary with pending startup ownership.

Complete entry bodies and the initial file value are composed here. Their
conditional successful claim is not a live-RAM preservation theorem. No
acquired/validated/published Config capability is emitted by this boundary.
"""

import re
from typing import TypedDict
from config_body_types import InstructionMap
from llvm_stack import Instruction
from config_entry import BootEntryRecord, boot_entry, CONFIG, INITIALIZE
from config_entry_contracts import ENTRY_GRAMMARS
from config_headers import ROOT
from config_initial import InitialConfiguration, initial_configuration
from config_stack import DiagnosticObjects, typed_function, transfer_family
from validation import require


# Already reviewed complete once shapes: load, nonzero branch, store, status
# compare, retry branch, normal restore, loader tail, already-taken successor.
_ONCE = {"speed2_6": (20, 24, 26, 30, 32, 34, 38, 42), "sizez_10": (14, 18, 20, 24, 26, 28, 32, 36)}
_MODES = {"speed2_2": ("speed2", 1288), "sizes_2": ("sizes", 696), "sizez_2": ("sizez", 456)}


class OnceSuccess(TypedDict):
    predicate: tuple[str, str]
    load: int
    store: int
    new_taken: int
    width: int
    restore: int
    tail: int


class OnceRetry(TypedDict):
    predicate: str
    compare: int
    branch: int
    target: int


class OnceTaken(TypedDict):
    predicate: str
    branch: int
    target: int
    requires_nonreturn_diagnostic_closure: bool


class OnceRecord(TypedDict):
    function: int
    extent: int
    frame: int
    taken_address: int
    success: OnceSuccess
    retry: OnceRetry
    already_taken: OnceTaken
    requires_actual_live_initial_state_and_exclusive_source_claim: bool


class LoaderRecord(TypedDict):
    function: int
    name: str
    extent: int
    frame: int
    incoming_r0: int
    initialize_frame_live_on_tail: int
    requires_same_B_reconstructed_inside_specialized_body: bool


class PhysicalPrefix(TypedDict):
    entry_chain: tuple[tuple[int, int], ...]
    boot_poll: tuple[int, int]
    once_frame: int
    loader_frame: int
    once_restores_before_loader: bool
    existing_conservative_boot_formula_unchanged: bool


class ConfigurationEntryRecord(TypedDict):
    mode: str
    phase: str
    initial: InitialConfiguration
    entry: BootEntryRecord
    actual_Config_references: tuple[tuple[str, int, int, int], ...]
    boot_call: int
    once: OnceRecord
    loader: LoaderRecord
    physical_prefix: PhysicalPrefix
    requires_actual_startup_preservation_and_no_prior_Config_alias: bool
    requires_specialized_loader_reconstruction_and_all_internal_effects: bool
    requires_frozen_executor_first_batch_source_ownership_and_all_indirect_ingress: bool
    requires_once_retry_and_nonreturn_diagnostic_default_sink_closure: bool
    must_not_reuse_initial_prewrite_epoch_after_acquisition_or_mutation: bool
    requires_completed_decode_or_default_validation_before_publication: bool


def _direct_target(row: Instruction, family: str) -> int:
    """One complete selected-LLVM operand, never a later hexadecimal token.

    B/BL start with their target; CBZ/CBNZ first name a low register. The only
    suffix is one nonempty LLVM <symbol> annotation, whose nested angle pairs
    must remain inside that annotation. Apply decode_listing's LLVM comment
    normalization to supplied records too. No additional transfer is authorized.
    """
    error = "unresolved direct configuration claim ingress"
    operand = row.operands
    require(isinstance(operand, str) and not any(c in operand for c in "\r\n"), error)
    prefix = r"r[0-7],[ \t]*" if family in ("cbz", "cbnz") else ""
    match = re.fullmatch(
        prefix + r"(0x[0-9a-f]{1,8})(?:[ \t]+(<.+>))?", operand.split("@", 1)[0].strip()
    )
    if match is None:
        raise AssertionError(error)
    annotation = match[2]
    if annotation is not None:
        depth = 0
        for index, char in enumerate(annotation):
            if char == "<":
                depth += 1
            elif char == ">":
                depth -= 1
            require(depth > 0 or index == len(annotation) - 1, error)
        require(depth == 0, error)
    return int(match[1], 16)


def configuration_entry_boundary(
    objects: DiagnosticObjects, instructions: InstructionMap
) -> ConfigurationEntryRecord:
    """Bind actual objects/calls and closed retry/success/failure alternatives.

    This function accepts no source-owner flag or caller-selected phase. The
    initial-value record is rechecked on this exact artifact. Runtime startup
    preservation and source exclusivity still must be established by the final
    graph before consuming the successful once alternative as an owner.
    """
    initial = initial_configuration(objects)
    entry = boot_entry(objects, instructions)
    mode, loader_frame = _MODES[entry["phases"]["run"]["shape"]]
    once = entry["phases"]["configuration_initialize"]
    start = once["function"]
    end = start + once["extent"]
    require(
        once["shape"] in _ONCE and once["frame"] == 8 and entry["initialize"] == start,
        "configuration claim loses its actual compatible initialize phase",
    )
    references = []
    for phase, proof in entry["phases"].items():
        contract = ENTRY_GRAMMARS[proof["shape"]]
        for ordinal, descriptor in enumerate(contract["objects"]):
            if descriptor == {"kind": "entry_static", "role": CONFIG}:
                require(
                    proof["objects"][ordinal] == initial["address"],
                    "boot/once/Dashboard name different private Config objects",
                )
                references.append((phase, proof["function"], ordinal, proof["objects"][ordinal]))
    require(
        tuple(sorted(v[0] for v in references))
        == ("configuration_initialize", "dashboard_initializer"),
        "entry has missing or additional Config receiver references",
    )
    boot = entry["phases"]["boot_poll"]
    callers = [e for e in boot["ordinary_edges"] if e[3] == INITIALIZE]
    require(
        len(callers) == 1
        and callers[0][1] == "bl"
        and callers[0][2] == start
        and callers[0][4] == 8,
        "configuration once claim has another boot call boundary",
    )
    caller = boot["function"] + callers[0][0]
    # Complete local grammars already retain the retry edges. No foreign direct
    # predecessor may bypass the load, claim, normal restore or failure prefix.
    for at, row in instructions.items():
        family = transfer_family(row.mnemonic.removesuffix(".w"))
        if family not in ("b", "bl", "cbz", "cbnz"):
            continue
        destination = _direct_target(row, family)
        if start <= destination < end and not start <= at < end:
            require(
                at == caller and row.mnemonic == "bl" and destination == start,
                "foreign direct ingress into configuration once boundary",
            )
    load, nonzero, store, compare, retry, restore, tail, failure = _ONCE[once["shape"]]
    tails = [e for e in once["ordinary_edges"] if e[3] == ROOT]
    require(
        len(tails) == 1 and (tails[0][0], tails[0][1], tails[0][4]) == (tail, "b.w", loader_frame),
        "configuration claim loses restored specialized loader tail/frame",
    )
    loader = tails[0][2]
    extent, frame, loader_name = typed_function(objects, loader, ROOT)
    require(frame == loader_frame, "configuration loader activation differs")
    # Current source specialization reconstructs B inside the loader/visitor.
    # R0 on this emitted tail is deliberately NOT the generic &mut ConfigSlot.
    taken = initial["address"] + 14600
    return dict(
        mode=mode,
        phase="once_loader_boundary_pending_startup",
        initial=initial,
        entry=entry,
        actual_Config_references=tuple(references),
        boot_call=caller,
        once=dict(
            function=start,
            extent=once["extent"],
            frame=8,
            taken_address=taken,
            success=dict(
                predicate=("loaded_taken_zero", "exclusive_store_succeeded"),
                load=start + load,
                store=start + store,
                new_taken=1,
                width=1,
                restore=start + restore,
                tail=start + tail,
            ),
            retry=dict(
                predicate="exclusive_store_failed",
                compare=start + compare,
                branch=start + retry,
                target=start + load,
            ),
            already_taken=dict(
                predicate="loaded_taken_nonzero",
                branch=start + nonzero,
                target=start + failure,
                requires_nonreturn_diagnostic_closure=True,
            ),
            requires_actual_live_initial_state_and_exclusive_source_claim=True,
        ),
        loader=dict(
            function=loader,
            name=loader_name,
            extent=extent,
            frame=frame,
            incoming_r0=taken,
            initialize_frame_live_on_tail=0,
            requires_same_B_reconstructed_inside_specialized_body=True,
        ),
        physical_prefix=dict(
            entry_chain=entry["entry_chain"],
            boot_poll=(boot["function"], boot["frame"]),
            once_frame=8,
            loader_frame=loader_frame,
            once_restores_before_loader=True,
            existing_conservative_boot_formula_unchanged=True,
        ),
        requires_actual_startup_preservation_and_no_prior_Config_alias=True,
        requires_specialized_loader_reconstruction_and_all_internal_effects=True,
        requires_frozen_executor_first_batch_source_ownership_and_all_indirect_ingress=True,
        requires_once_retry_and_nonreturn_diagnostic_default_sink_closure=True,
        must_not_reuse_initial_prewrite_epoch_after_acquisition_or_mutation=True,
        requires_completed_decode_or_default_validation_before_publication=True,
    )
