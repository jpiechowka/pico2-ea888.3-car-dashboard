"""Closed canonical helper unions with unknown scalar/string contents.

Facts describe bounded initialized source prefixes, not candidate bytes or
UTF-8 validation. Actual typed field/string construction, length/receiver
transport, current owner phase and complete parent coverage remain required.
No exact API accepts these interfaces; no arbitrary helper/ISA is evaluated.
"""

from dataclasses import dataclass, replace
from collections.abc import Sequence
from typing import NotRequired, TypedDict
from config_body_types import BodyRecord, InstructionMap
from config_helpers import HelperCoverageRecord, IndexResult
from config_logger import LoggerRecord
from config_stack import DiagnosticObjects
from config_abstract import Span, DecoderInterface, _policy
from config_platform import PlatformPolicy
from config_compare_effects import _memcmp_dependency
from config_decode_effects import _current, _object
from config_helpers import helper_frame, index_prefix
from config_serialize_effects import _child, _frame
from config_regions import View, Argument, Storage, borrow, require_disjoint, require_writes
from validation import require


@dataclass(frozen=True)
class CompareInterface:
    header: View
    payload: View
    cursor: Span
    platform_policy: PlatformPolicy | None = None

    def __post_init__(self) -> None:
        # Structural checks only: C.word4 is remaining LENGTH, never D.end.
        DecoderInterface(self.header, self.payload, self.cursor)
        require(
            self.platform_policy is None or isinstance(self.platform_policy, PlatformPolicy),
            "unresolved canonical platform prerequisite",
        )


@dataclass(frozen=True)
class InitializedPrefix:
    view: View
    length: Span

    def __post_init__(self) -> None:
        require(
            isinstance(self.view, View)
            and not self.view.writable
            and isinstance(self.length, Span)
            and self.length.upper <= self.view.extent,
            "source lacks its bounded initialized-prefix interface",
        )
        # The actual length selects its valid prefix; this does not initialize
        # capacity slack or assert all bytes up to the upper bound are live.


@dataclass(frozen=True)
class ScalarInterface:
    kind: str
    domain: Span

    def __post_init__(self) -> None:
        widths = {"u8": 8, "u16": 16, "u32": 32, "usize": 32, "f32_bits": 32}
        require(
            self.kind in widths
            and isinstance(self.domain, Span)
            and self.domain.upper < 1 << widths[self.kind],
            "unreviewed abstract scalar kind/width/domain",
        )


def _source(objects: DiagnosticObjects, source: InitializedPrefix, frame: Storage) -> Storage:
    require(isinstance(source, InitializedPrefix), "missing initialized source-prefix interface")
    view = source.view
    require(
        view.storage.phase == frame.phase
        and (view.storage.space == frame.space or view.storage.space == "address"),
        "canonical source belongs to another coordinate/lifetime",
    )
    if view.storage.space == "address" and not view.storage.writable:
        objects.read_only(view.start, view.extent)
    # Mutable Config/local storage is consumed through a readonly borrow. Its
    # initialization/exclusive phase must come from a matched source owner.
    return _object(view, "live-canonical-source")


class CompareOutcome(TypedDict):
    kind: str
    state: CompareInterface
    possible_writes: tuple[tuple[View, int, int], ...]
    status: int
    phase: NotRequired[str]


class CompareInterfaceResult(TypedDict):
    semantic_role: str
    proof: HelperCoverageRecord
    builtin: LoggerRecord
    outcomes: tuple[CompareOutcome, ...]
    source: InitializedPrefix
    possible_input_read_envelopes: tuple[View, InitializedPrefix]
    conservative_activation_frames: tuple[int, ...]
    platform_policy: PlatformPolicy | None
    requires_parent_preserve_dependent_platform_policy: bool
    normal_preserves: tuple[str, ...]
    requires_actual_stage_receiver_transport_and_complete_parent_graph: bool


def compare_interface(
    objects: DiagnosticObjects,
    instructions: InstructionMap,
    proof: BodyRecord,
    state: CompareInterface,
    source: InitializedPrefix,
    frame: Storage,
    ancestor_protected: Sequence[Storage],
) -> CompareInterfaceResult:
    proof = _current(objects, instructions, proof)
    require(
        proof["semantic_role"] == "compare_extend"
        and isinstance(state, CompareInterface)
        and state.header.storage.space == frame.space
        and state.header.storage.phase == frame.phase,
        "abstract Compare receiver ABI differs",
    )
    active = helper_frame(proof, frame, tuple(ancestor_protected))
    guard = _source(objects, source, frame)
    if state.platform_policy is not None:
        _policy(objects, state, state.platform_policy)
    objects.read_only(state.payload.start, state.payload.extent)
    protected = active["protected"] + (_object(state.payload, "immutable-input"), guard)
    require_writes((state.header,), ((state.header, 0, 8),), protected)
    require_disjoint((frame, guard))
    builtin, child_frame = _memcmp_dependency(objects, instructions, proof, frame)
    require_disjoint((child_frame,) + active["protected"])
    require_disjoint((child_frame, guard))
    require_writes((state.header,), ((state.header, 0, 8),), (child_frame,))
    outcomes: list[CompareOutcome] = []
    lo, hi = state.cursor.lower, state.cursor.upper
    n = state.payload.extent
    # No equality result is computed. Both byte-comparison outcomes are kept;
    # a short-prefix error is also included without consuming the current C.
    if lo + source.length.lower <= n:
        cursor = Span(lo + source.length.lower, min(n, hi + source.length.upper))
        outcomes.append(
            dict(
                kind="success",
                state=replace(state, cursor=cursor),
                possible_writes=((state.header, 0, 8),),
                status=255,
            )
        )
    outcomes.append(dict(kind="error2", state=state, possible_writes=(), status=2))
    return dict(
        semantic_role="compare_extend",
        proof=proof,
        builtin=builtin,
        outcomes=tuple(outcomes),
        source=source,
        possible_input_read_envelopes=(state.payload, source),
        conservative_activation_frames=(24, 32),
        platform_policy=state.platform_policy,
        requires_parent_preserve_dependent_platform_policy=True,
        normal_preserves=tuple("r" + str(i) for i in range(4, 12)) + ("sp",),
        requires_actual_stage_receiver_transport_and_complete_parent_graph=True,
    )


class SerializeInterfaceResult(TypedDict):
    semantic_role: str
    proof: HelperCoverageRecord
    outcomes: tuple[CompareOutcome, ...]
    children: tuple["SerializeInterfaceResult | CompareInterfaceResult", ...]
    index: IndexResult | None
    encoded_length: Span | None
    local_write_envelopes: tuple[tuple[View, int, int], ...]
    conservative_activation_paths: tuple[tuple[int, ...], ...]
    conservative_activation_frames: tuple[int, ...]
    requires_actual_stage_receiver_transport_and_complete_parent_graph: bool
    platform_policy: PlatformPolicy | None
    requires_parent_preserve_dependent_platform_policy: bool
    requires_hard_float_s0: bool
    requires_source_valid_string_reference: bool
    normal_preserves: tuple[str, ...]


def serialize_interface(
    objects: DiagnosticObjects,
    instructions: InstructionMap,
    proof: BodyRecord,
    state: CompareInterface,
    value: ScalarInterface | InitializedPrefix,
    frame: Storage,
    ancestor_protected: Sequence[Storage],
) -> SerializeInterfaceResult:
    proof = _current(objects, instructions, proof)
    semantic = proof["semantic_role"]
    kinds = {
        "serialize_u8": "u8",
        "serialize_u16": "u16",
        "varint_usize": "usize",
        "varint_u32_small": "u32",
        "serialize_f32": "f32_bits",
    }
    require(semantic in kinds or semantic == "serialize_str", "unreviewed abstract serializer")
    require(
        isinstance(state, CompareInterface)
        and state.header.storage.space == frame.space
        and state.header.storage.phase == frame.phase,
        "abstract serializer C has another owner/phase",
    )
    active = helper_frame(proof, frame, tuple(ancestor_protected))
    if state.platform_policy is not None:
        _policy(objects, state, state.platform_policy)
    prologue = Storage(
        frame.space,
        frame.identity + ":complete-prologue",
        frame.start,
        frame.extent,
        8,
        frame.phase,
        False,
    )
    require_writes(
        (state.header,),
        ((state.header, 0, 8),),
        active["protected"] + (prologue, _object(state.payload, "immutable-input")),
    )
    children: list[SerializeInterfaceResult | CompareInterfaceResult] = []
    outcomes: list[CompareOutcome] = []
    paths: list[tuple[int, ...]] = []
    index: IndexResult | None = None
    local_writes: tuple[tuple[View, int, int], ...] = ()
    if semantic == "serialize_str":
        if not isinstance(value, InitializedPrefix):
            raise AssertionError("missing initialized source-prefix interface")
        guard = _source(objects, value, frame)
        require_disjoint((frame, guard))
        require_writes((state.header,), ((state.header, 0, 8),), (guard,))
        child = _child(objects, instructions, proof, "varint_usize")
        prefix = serialize_interface(
            objects,
            instructions,
            child,
            state,
            ScalarInterface("usize", value.length),
            _frame(frame, "string-prefix", 24),
            active["protected"] + (guard,),
        )
        children.append(prefix)
        paths.append((frame.extent,) + prefix["conservative_activation_frames"])
        for alternative in prefix["outcomes"]:
            if alternative["kind"] == "error2":
                outcomes.append({**alternative, "phase": "prefix_failed"})
                continue
            child = _child(objects, instructions, proof, "compare_extend")
            body = compare_interface(
                objects,
                instructions,
                child,
                alternative["state"],
                value,
                _frame(frame, "string-body", 24),
                active["protected"],
            )
            children.append(body)
            paths.append((frame.extent,) + body["conservative_activation_frames"])
            for final in body["outcomes"]:
                outcomes.append(
                    {
                        **final,
                        "phase": "complete" if final["kind"] == "success" else "body_failed",
                        "possible_writes": alternative["possible_writes"]
                        + final["possible_writes"],
                    }
                )
        length = None
    else:
        if not (isinstance(value, ScalarInterface) and value.kind == kinds[semantic]):
            raise AssertionError("serializer lacks its source scalar type")
        require(
            semantic != "varint_u32_small" or value.domain.upper <= 127,
            "specialized abstract u32 lacks its guarded<=127 domain",
        )
        length = {
            "serialize_u8": Span(1, 1),
            "serialize_u16": Span(1, 3),
            "varint_usize": Span(1, 5),
            "varint_u32_small": Span(1, 1),
            "serialize_f32": Span(4, 4),
        }[semantic]
        scratch = active["scratch"]
        require(len(scratch) == 1, "serializer scratch interface differs")
        buffer = scratch[0]
        require(length.upper <= buffer.extent, "abstract prefix exceeds local buffer")
        require_writes(
            (buffer,),
            ((buffer, 0, buffer.extent),),
            active["protected"]
            + (_object(state.header, "C"), _object(state.payload, "immutable-input")),
        )
        local_writes = ((buffer, 0, buffer.extent),)
        if semantic in ("serialize_u16", "varint_usize", "varint_u32_small"):
            kind = (
                "range_to_inclusive_index_mut"
                if semantic == "serialize_u16"
                else "array5_index_mut"
            )
            child = _child(objects, instructions, proof, kind)
            location_role = {
                "serialize_u16": "postcard_serialize_u16",
                "varint_usize": "postcard_varint_usize",
                "varint_u32_small": "postcard_varint_u32_small",
            }[semantic]
            require(set(proof["objects"]) == {0}, "abstract serializer Location differs")
            index = index_prefix(
                objects,
                instructions,
                child,
                buffer,
                _frame(frame, "prefix-index", child["frame"]),
                active["protected"] + (_object(state.header, "C"),),
                proof["objects"][0],
                Argument("usize", None, 0, 32, frozenset(range(length.lower - 1, length.upper))),
                location_role=location_role,
            )
            require(
                all("prefix" in o for o in index["outcomes"]),
                "abstract scalar prefix admits an unproved index panic",
            )
            paths.append((frame.extent,) + index["conservative_frame_metadata"])
        child = _child(objects, instructions, proof, "compare_extend")
        source = InitializedPrefix(
            borrow(buffer.storage, buffer.offset, buffer.extent, buffer.storage.phase), length
        )
        compared = compare_interface(
            objects,
            instructions,
            child,
            state,
            source,
            _frame(frame, "comparison", 24),
            active["protected"],
        )
        children.append(compared)
        outcomes.extend(compared["outcomes"])
        paths.append((frame.extent,) + compared["conservative_activation_frames"])
    success = 255 if semantic in ("varint_usize", "varint_u32_small") else 0xFFFFFFFF
    outcomes = [{**o, "status": success if o["kind"] == "success" else 2} for o in outcomes]
    return dict(
        semantic_role=semantic,
        proof=proof,
        outcomes=tuple(outcomes),
        children=tuple(children),
        index=index,
        encoded_length=length,
        local_write_envelopes=local_writes,
        conservative_activation_paths=tuple(paths),
        conservative_activation_frames=max(paths, key=sum),
        requires_actual_stage_receiver_transport_and_complete_parent_graph=True,
        platform_policy=state.platform_policy,
        requires_parent_preserve_dependent_platform_policy=True,
        requires_hard_float_s0=semantic == "serialize_f32",
        requires_source_valid_string_reference=semantic == "serialize_str",
        normal_preserves=tuple("r" + str(i) for i in range(4, 12)) + ("sp",),
    )
