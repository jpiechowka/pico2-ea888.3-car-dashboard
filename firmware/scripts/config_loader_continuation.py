"""Finite fallback/decoded Result continuation and source publication interfaces.

This retains two caller epochs and every emitted failure arm. A matching
continuation does not prove validator output, incoming Config/fault ownership,
whole-caller coverage, boot visibility or the final qualified stack result.
"""

from collections.abc import Iterable, Mapping
from typing import Literal, Never, NotRequired, TypedDict
from config_body_types import BodyRecord, InstructionMap
from config_stack import DiagnosticObjects
from config_regions import Region, View
from config_default import Writes
from config_default_sources import SourceTerminal
from config_default_widget import TargetRequirement
from config_roots import WindowRecord
from config_upstream import IssuerRecord
from llvm_stack import Instruction
from dataclasses import replace
import re
from config_continuation_contract import CONTINUATIONS
from config_default import _ingress
from config_default_sources import source_terminal
from config_entry import entry_object
from config_finite import _literal_body
from config_regions import (
    Storage,
    Transfer,
    borrow,
    require_disjoint,
    require_writes,
    _control,
)
from config_roots import root_window
from config_stack import canonical_symbol, typed_function, transfer_family
from config_upstream import issuer_region
from validation import require


class FallbackContext(TypedDict):
    phase: Literal["fallback"]
    call: int
    result: View
    r8: int
    r4: int
    r10: int


class DecodedContext(TypedDict):
    phase: Literal["decoded"]
    call: int
    result: View
    r8: int
    requires_prior_Compare_retirement_and_complete_decode_finalize_canonical: bool


class ErrorPayload(TypedDict):
    view: View
    reference_levels: int
    opaque_padding: tuple[int, int]
    writes: Writes
    requires_actual_ValidationError_variant_and_callback_binding: bool


class RetainedFault(TypedDict):
    view: View
    first_word: str
    remaining_words: tuple[int, int]
    requires_source_typed_original_fault_and_preservation: bool


class PublicationPredecessor(TypedDict):
    phase: str
    fault: str
    stoich: str
    r10: NotRequired[int]
    r8: NotRequired[int]


class ReturnInterface(TypedDict):
    parent_frame: int
    outline: BodyRecord
    incoming_SP: int
    saved_intervals: tuple[tuple[int, int], ...]
    restored_bytes: int
    final_SP: int
    r0: str
    requires_actual_boot_discard_and_Dashboard_static_receiver: bool


class PayloadFailure(TypedDict):
    target: int
    requires_complete_source_Payload_constructor_and_refill_join: bool


class ValidationRefill(TypedDict):
    target: int
    third_word_to_P8: bool
    second_word_to_P12: bool
    first_word_register: str
    leading_tag_register: str
    requires_typed_fault_preparation_join: bool


class LoaderContinuation(TypedDict):
    proofs: dict[str, BodyRecord]
    source: SourceTerminal
    issuer: IssuerRecord
    default_error: WindowRecord
    regions: tuple[Region, ...]
    config: View
    validator_contexts: tuple[FallbackContext, DecodedContext]
    valid_result_tags: tuple[int, ...]
    excluded_non2_sentinel: int
    requires_source_valid_Result_not_arbitrary_non2: bool
    error_payload: ErrorPayload
    retained_fault: RetainedFault
    fallback_writes: Writes
    decoded_writes: Writes
    publication_writes: Writes
    decoded_local_writes: Writes
    publication_predecessors: tuple[PublicationPredecessor, ...]
    return_interface: ReturnInterface
    payload_failure: PayloadFailure
    validation_error_refill: ValidationRefill
    target_access_requirements: tuple[TargetRequirement, ...]
    requires_complete_validator_typed_fault_and_boot_graph: bool
    requires_full_parent_and_outline_ingress_coverage: bool
    requires_default_Formatter_live_sink_PANICKED_and_activation_graph: bool


def _no_outline(*args: object) -> Never:
    raise AssertionError("unreviewed continuation outline child")


def _destination(row: Instruction) -> int:
    match = re.search(r"(?<![#\w])0x([0-9a-f]+)(?:\s|$)", row.operands.split("@", 1)[0])
    if match is None:
        raise AssertionError("unresolved continuation transfer")
    return int(match[1], 16)


def _match(
    objects: DiagnosticObjects,
    name: str,
    instructions: InstructionMap,
    label: str,
    start: int,
    bindings: Mapping[str, int],
) -> BodyRecord:
    contract = CONTINUATIONS[label].copy()
    operations = []
    for offset, width, text in contract["instructions"]:
        for key, value in bindings.items():
            text = text.replace("@" + key, hex(value))
        operations.append((offset, width, text))
    contract["instructions"] = operations
    # Symbol display annotations are not instruction operands. Keep raw bytes,
    # addresses and every architectural operand; only the decoration is removed.
    rows = {
        a: replace(r, operands=r.operands.split("@", 1)[0].split("<", 1)[0].strip())
        for a, r in instructions.items()
    }
    return _literal_body(
        objects,
        name,
        rows,
        canonical_symbol(name),
        label,
        contract,
        entry_object,
        _no_outline,
        window=start,
    )


def _region(
    objects: DiagnosticObjects,
    name: str,
    instructions: InstructionMap,
    label: str,
    proof: BodyRecord,
) -> Region:
    start = proof["function"]
    end = start + proof["extent"]
    transfers = []
    for at, row in sorted(instructions.items()):
        if not start <= at < end:
            continue
        kind, conditional = _control(row)
        if kind in ("b", "bl", "cbz", "cbnz"):
            transfers.append(
                Transfer(
                    at, row.mnemonic, "call" if kind == "bl" else "branch", _destination(row), label
                )
            )
        else:
            require(
                kind is None or label == "restore" and kind == "return",
                "unreviewed continuation transfer",
            )
    return Region(
        (objects.elf.functions[name][0] & ~1, label),
        "source phase",
        start,
        end,
        tuple(sorted(a for a in instructions if start <= a < end)),
        (),
        tuple(transfers),
    )


def loader_continuation(
    objects: DiagnosticObjects,
    name: str,
    instructions: InstructionMap,
    frame: Storage,
    ancestor_protected: Iterable[Storage],
) -> LoaderContinuation:
    ancestor_protected = tuple(ancestor_protected)
    source = source_terminal(objects, name, instructions, frame, ancestor_protected)
    return _checked_continuation(objects, name, instructions, frame, ancestor_protected, source)


def _checked_continuation(
    objects: DiagnosticObjects,
    name: str,
    instructions: InstructionMap,
    frame: Storage,
    ancestor_protected: Iterable[Storage],
    source: SourceTerminal,
    _phase_protected: Iterable[Storage] | None = None,
) -> LoaderContinuation:
    # Only internally recomputed source enters this private shared matcher.
    ancestor_protected = tuple(ancestor_protected)
    prepared = source["widget"]["prepared"]
    base = prepared["config"]
    owner = base.storage
    parent = objects.elf.functions[name][0] & ~1
    extent = objects.elf.functions[name][1]
    start = source["proof"]["function"] + source["proof"]["extent"]
    issuer = issuer_region(objects, name, instructions)
    require(
        issuer["shape"] == "sizez" and issuer["frame"] == 456,
        "continuation issuer has another caller ABI",
    )
    # The fallback branch must target the independently typed concrete-error
    # suffix, including its LDM/STM/reference-level producer and Location.
    failure_start = _destination(instructions[start + 6])
    failure = root_window(objects, name, instructions, failure_start + 26)
    require(
        (failure["location_role"], failure["window"], failure["extent"])
        == ("reviewed_defaults", failure_start, 30),
        "fallback error loses its concrete Result copy/suffix",
    )
    validator = source["outgoing"]["call"][2]
    decoded_calls = [
        at
        for at, r in instructions.items()
        if parent <= at < parent + extent
        and r.mnemonic in ("bl", "bl.w")
        and _destination(r) == validator
    ]
    decoded_calls = [
        at
        for at in decoded_calls
        if at != source["proof"]["function"] + source["outgoing"]["call"][0]
    ]
    require(len(decoded_calls) == 1, "decoded validator caller is missing or ambiguous")
    decoded = decoded_calls[0] - 10
    publication = issuer["continuation"]
    restore = _destination(instructions[publication + 22])
    restore_names = [n for n, e in objects.elf.functions.items() if e[0] & ~1 == restore]
    require(
        len(restore_names) == 1 and re.fullmatch(r"OUTLINED_FUNCTION_[0-9]+", restore_names[0]),
        "loader return is not its exact owned outline",
    )
    restore_name = restore_names[0]
    rest_extent, rest_frame, _ = typed_function(objects, restore, canonical_symbol(restore_name))
    require((rest_extent, rest_frame) == (8, 0), "loader return outline extent/frame differs")
    refill = prepared["proof"]["function"]
    bindings = dict(
        default_error=failure_start,
        issuer=issuer["window"],
        restore=restore,
        refill=refill,
        payload=refill - 20,
    )
    proofs = dict(
        fallback=_match(objects, name, instructions, "fallback", start, bindings),
        publication=_match(objects, name, instructions, "publication", publication, bindings),
        decoded=_match(objects, name, instructions, "decoded", decoded, bindings),
        restore=_match(objects, restore_name, instructions, "restore", restore, {}),
    )
    require(
        start + 62 == issuer["window"] and proofs["publication"]["objects"][0] == base.start,
        "fallback/publication does not preserve its current source owner",
    )
    require(
        proofs["decoded"]["ordinary_edges"]
        == ((10, "bl", validator, "_RNvNtCs_16dashboard_config8validate12validate_ref", 320),),
        "decoded validator changes callee/Result boundary",
    )
    # Same-parent direct ingress cannot skip a phase's prerequisites. Computed
    # ingress is excluded here by the existing bounded parent classification;
    # the full source/entry/computed successor closure is still owed upstream.
    ranges = [
        (p["function"], p["function"] + p["extent"])
        for key, p in proofs.items()
        if key != "restore"
    ]
    identity_failure = issuer["failure_suffix"]
    zero_edges = [
        issuer["window"] + offset
        for offset, operation, target in issuer["branch_edges"]
        if target == identity_failure["window"] and operation in ("beq", "beq.w")
    ]
    require(
        len(zero_edges) == 1, "issuer expect loses its unique matched previous-zero predecessor"
    )
    ranges.extend(
        (
            (failure_start, failure_start + 30),
            (issuer["window"], issuer["continuation"]),
            (identity_failure["window"], identity_failure["window"] + identity_failure["extent"]),
        )
    )
    for lo, hi in ranges:
        _ingress(instructions, parent, extent, lo, hi)
    starts: dict[int, set[int]] = {
        start: set(),
        decoded: set(),
        publication: {issuer["window"] + 46},
        issuer["window"]: {decoded + 62},
        failure_start: {start + 6},
        identity_failure["window"]: {zero_edges[0]},
    }
    for at, row in instructions.items():
        if transfer_family(row.mnemonic.removesuffix(".w")) not in ("b", "bl", "cbz", "cbnz"):
            continue
        target = _destination(row)
        if target in starts:
            require(at in starts[target], "foreign continuation phase entry")
    issuer_owner = Storage(
        "address", "private cache identity issuer", issuer["objects"][0], 4, 4, frame.phase, True
    )
    require_disjoint((owner, issuer_owner, frame) + ancestor_protected)
    saved = prepared["protected"][:-1] if _phase_protected is None else tuple(_phase_protected)
    fault = prepared["fault"]
    protected = saved + (fault,)

    def writes(
        storage: Storage, items: Iterable[tuple[int, int]], guards: Iterable[Storage]
    ) -> Writes:
        pairs = tuple((borrow(storage, o, n, frame.phase, writable=True), 0, n) for o, n in items)
        require_writes(tuple(v for v, o, n in pairs), pairs, guards)
        return pairs

    fallback_writes = writes(owner, ((14596, 4), (14592, 4), (14576, 4), (14588, 4)), protected)
    decoded_writes = writes(owner, ((14588, 2), (14576, 4)), saved)
    publication_writes = writes(owner, ((14584, 1), (14580, 4)), protected)
    error_writes = writes(frame, ((112, 12), (0, 4)), protected)
    decoded_local_writes = writes(frame, ((12, 4), (8, 4)), saved)
    result_default = borrow(frame, 236, 12, frame.phase, writable=True)
    result_decoded = borrow(frame, 112, 12, frame.phase, writable=True)
    require(
        result_default.start % 4 == 0 and result_decoded.start % 4 == 0,
        "Result LDM/STM requires actual word alignment",
    )
    regions = tuple(
        _region(objects, restore_name if key == "restore" else name, instructions, key, p)
        for key, p in proofs.items()
    )
    return dict(
        proofs=proofs,
        source=source,
        issuer=issuer,
        default_error=failure,
        regions=regions,
        config=base,
        validator_contexts=(
            dict(
                phase="fallback",
                call=source["proof"]["function"] + source["outgoing"]["call"][0],
                result=result_default,
                r8=frame.start + 112,
                r4=frame.start + 236,
                r10=base.start,
            ),
            dict(
                phase="decoded",
                call=decoded + 10,
                result=result_decoded,
                r8=base.start,
                requires_prior_Compare_retirement_and_complete_decode_finalize_canonical=True,
            ),
        ),
        valid_result_tags=(0, 1, 2),
        excluded_non2_sentinel=65535,
        requires_source_valid_Result_not_arbitrary_non2=True,
        error_payload=dict(
            view=borrow(frame, 112, 12, frame.phase),
            reference_levels=0,
            opaque_padding=(11, 12),
            writes=error_writes,
            requires_actual_ValidationError_variant_and_callback_binding=True,
        ),
        retained_fault=dict(
            view=borrow(frame, 8, 16, frame.phase),
            first_word="(P16 & 0xffff0000)+P20",
            remaining_words=(12, 8),
            requires_source_typed_original_fault_and_preservation=True,
        ),
        fallback_writes=fallback_writes,
        decoded_writes=decoded_writes,
        publication_writes=publication_writes,
        decoded_local_writes=decoded_local_writes,
        publication_predecessors=(
            dict(
                phase="fallback",
                fault="Some(original fault)",
                stoich="default Result+4",
                r10=base.start,
            ),
            dict(
                phase="decoded", fault="None", stoich="decoded Result+4 through P12", r8=base.start
            ),
        ),
        return_interface=dict(
            parent_frame=456,
            outline=proofs["restore"],
            incoming_SP=frame.start + 416,
            saved_intervals=((352, 416), (416, 420), (420, 436), (436, 452), (452, 456)),
            restored_bytes=456,
            final_SP=frame.start + 456,
            r0="issued previous identity or zero; not Config pointer",
            requires_actual_boot_discard_and_Dashboard_static_receiver=True,
        ),
        payload_failure=dict(
            target=refill - 20, requires_complete_source_Payload_constructor_and_refill_join=True
        ),
        validation_error_refill=dict(
            target=refill,
            third_word_to_P8=True,
            second_word_to_P12=True,
            first_word_register="r5",
            leading_tag_register="r4",
            requires_typed_fault_preparation_join=True,
        ),
        target_access_requirements=source["target_access_requirements"],
        requires_complete_validator_typed_fault_and_boot_graph=True,
        requires_full_parent_and_outline_ingress_coverage=True,
        requires_default_Formatter_live_sink_PANICKED_and_activation_graph=True,
    )
