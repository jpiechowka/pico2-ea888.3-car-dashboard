"""Closed construction effects for the pinned default-refill helper family.

Unknown bytes stay unknown. Facts describe initialized length prefixes and
typed Result alternatives, never capacity slack/padding or UTF-8 validity.
Actual source constructor, receiver/length/SP transport and enclosing epochs,
all-ingress, source validity and whole graph remain parent obligations.
"""

from dataclasses import dataclass, replace
from config_abstract import Span
from config_abstract_compare import InitializedPrefix, _source
from config_decode_effects import _current, _object
from config_helpers import helper_coverage, helper_frame, HELPER_SEMANTIC_ROLES
from config_regions import Storage, View, borrow, require_disjoint, require_writes
from config_stack import canonical_symbol
from validation import require


@dataclass(frozen=True)
class VecInterface:
    view: View
    capacity: int
    length: Span

    def __post_init__(self):
        require(
            type(self.capacity) is int
            and self.capacity in (14, 20, 24)
            and isinstance(self.length, Span)
            and self.length.upper <= self.capacity,
            "unreviewed Vec capacity/length domain",
        )
        require(
            isinstance(self.view, View)
            and self.view.writable
            and self.view.start % 4 == 0
            and self.view.storage.alignment >= 4
            and self.view.extent == ((4 + self.capacity + 3) // 4) * 4,
            "Vec lacks its aligned full header/capacity owner",
        )


@dataclass(frozen=True)
class String5Interface:
    """The decoded Band's explicit small-string epoch; not a general Vec."""

    view: View
    length: Span

    def __post_init__(self):
        require(
            isinstance(self.length, Span)
            and self.length.upper <= 5
            and isinstance(self.view, View)
            and self.view.writable
            and self.view.start % 4 == 0
            and self.view.storage.alignment >= 4
            and self.view.extent == 12,
            "String5 loses its exact aligned length/header/capacity owner",
        )

    @property
    def capacity(self):
        return 5


def _frame(parent, semantic, size):
    return Storage(
        parent.space,
        parent.identity + ":" + semantic,
        parent.start - size,
        size,
        8,
        parent.phase,
        True,
    )


def _child(objects, instructions, proof, semantic):
    allowed = {
        "string_try14": ("vec_extend14",),
        "vec_extend14": ("vec_extend_inner",),
        "vec_extend20": ("vec_extend_inner",),
        "vec_extend24": ("vec_extend_inner",),
    }
    require(
        semantic in allowed.get(proof["semantic_role"], ()),
        "unreviewed construction child interface",
    )
    role = HELPER_SEMANTIC_ROLES[semantic]
    edges = [e for e in proof["ordinary_edges"] if e[3] == role]
    require(len(edges) == 1, "construction child edge differs")
    target = edges[0][2]
    names = [
        n
        for n, e in objects.elf.functions.items()
        if e[0] & ~1 == target and canonical_symbol(n) == role
    ]
    require(len(names) == 1, "construction child has no unique typed body")
    return helper_coverage(
        objects, names[0], instructions, (target, "construction child", proof["function"])
    )


def _return(proof, outcomes, children, frames, local_writes=()):
    return dict(
        proof=proof,
        outcomes=tuple(outcomes),
        children=tuple(children),
        conservative_activation_frames=frames,
        local_writes=local_writes,
        normal_preserves=tuple("r" + str(i) for i in range(4, 12)) + ("sp",),
        requires_actual_stage_receiver_transport_and_complete_parent_graph=True,
    )


def extend_interface(objects, instructions, proof, state, source, frame, ancestor_protected):
    """Vec append union, including all capacity errors and zero-count return.

    The existing prefix and source lengths are paired unknown domains. A
    success extends the prefix only, with no inferred contents. Capacity Err
    leaves the whole Vec unchanged; its ZST error has no readable payload.
    """
    require(
        isinstance(state, VecInterface),
        "construction Vec API received another finite string context",
    )
    return _extend_interface(objects, instructions, proof, state, source, frame, ancestor_protected)


def extend_string5_interface(
    objects, instructions, proof, state, source, frame, ancestor_protected
):
    """Actual inner24 call: caller r2=5 and S+0 is the paired source length."""
    require(
        isinstance(state, String5Interface)
        and isinstance(proof, dict)
        and proof.get("semantic_role") == "vec_extend_inner",
        "String5 requires the direct complete inner append family",
    )
    result = _extend_interface(
        objects, instructions, proof, state, source, frame, ancestor_protected
    )
    result.update(
        capacity_r2_argument=5,
        incoming_stack_source_length=source.length,
        requires_Band_UTF8_ASCII_nonempty_spaces_and_tail_zero_join=True,
    )
    return result


def _extend_interface(objects, instructions, proof, state, source, frame, ancestor_protected):
    # Called only after one of the two explicit typed entry interfaces above.
    # Their shared byte-copy/count equations do not widen VecInterface.
    proof = _current(objects, instructions, proof)
    semantic = proof["semantic_role"]
    require(
        semantic in ("vec_extend_inner", "vec_extend14", "vec_extend20", "vec_extend24")
        and (semantic == "vec_extend_inner" or semantic == "vec_extend" + str(state.capacity)),
        "construction wrapper and Vec capacity differ",
    )
    require(
        state.view.storage.space in (frame.space, "address")
        and state.view.storage.phase == frame.phase,
        "Vec belongs to another owner phase/coordinate",
    )
    active = helper_frame(proof, frame, tuple(ancestor_protected))
    guard = _source(objects, source, frame)
    require(
        state.length.upper + source.length.upper <= 0xFFFFFFFF,
        "Vec append sum may wrap before its capacity comparison",
    )
    # Full Vec includes header.end-equivalent/unused capacity/padding: none may
    # overlap the initial prologue, even when this branch writes no payload.
    require_disjoint((frame, _object(state.view, "Vec"), guard))
    require_writes(
        (state.view,), ((state.view, 0, state.view.extent),), active["protected"] + (guard,)
    )
    children = []
    locals = ()
    if semantic != "vec_extend_inner":
        child = _child(objects, instructions, proof, "vec_extend_inner")
        children.append(
            extend_interface(
                objects,
                instructions,
                child,
                state,
                source,
                _frame(frame, "inner", 24),
                active["protected"],
            )
        )
        # Parent P+0 is the incoming count, P+4 discarded push scratch. The
        # inner reads caller S+0 via its r7+8, not either child's current SP.
        locals = ((active["scratch"][0], 0, 4),)
        require_writes(
            active["scratch"], locals, active["protected"] + (_object(state.view, "Vec"), guard)
        )
    outcomes = []
    if state.length.lower + source.length.lower <= state.capacity:
        length = Span(
            state.length.lower + source.length.lower,
            min(state.capacity, state.length.upper + source.length.upper),
        )
        writes = ()
        if source.length.upper:
            # Union envelope; each concrete append starts at its own oldlen.
            end = length.upper
            start = state.length.lower
            if end > start:
                writes = ((state.view, 0, 4), (state.view, 4 + start, end - start))
        require_writes((state.view,), writes, active["protected"] + (guard,))
        outcomes.append(
            dict(
                kind="success",
                status=0,
                length=length,
                state=replace(state, length=length),
                possible_writes=writes,
                initialized_prefix=InitializedPrefix(
                    borrow(state.view.storage, state.view.offset + 4, state.capacity, frame.phase),
                    length,
                ),
            )
        )
    # Deliberately retain the guarded capacity failure even where the current
    # caller's short source would exclude it; this is a conservative union.
    outcomes.append(
        dict(
            kind="capacity_error",
            status=1,
            length=state.length,
            state=state,
            possible_writes=(),
            error_payload_bytes=0,
        )
    )
    return _return(
        proof, outcomes, children, (24,) if semantic == "vec_extend_inner" else (16, 24), locals
    )


def string_interface(objects, instructions, proof, source, output, frame, ancestor_protected):
    """String14::try_from Result24; opaque padding copy is not initialization."""
    proof = _current(objects, instructions, proof)
    require(proof["semantic_role"] == "string_try14", "unreviewed string constructor")
    require(
        isinstance(output, View)
        and output.writable
        and output.extent == 24
        and output.start % 4 == 0
        and output.storage.alignment >= 4
        and output.storage.space in (frame.space, "address")
        and output.storage.phase == frame.phase,
        "string Result has another owner/reference/extent ABI",
    )
    active = helper_frame(proof, frame, tuple(ancestor_protected))
    guard = _source(objects, source, frame)
    require_disjoint((frame, _object(output, "Result"), guard))
    require_writes((output,), ((output, 0, 24),), active["protected"] + (guard,))
    local = VecInterface(borrow(frame, 4, 20, frame.phase, writable=True), 14, Span(0, 0))
    # Complete local opaque transport is within P+[4,24), below save cells.
    require_writes(
        (local.view,),
        ((local.view, 0, 20),),
        active["protected"] + (_object(output, "Result"), guard),
    )
    child = _child(objects, instructions, proof, "vec_extend14")
    appended = extend_interface(
        objects,
        instructions,
        child,
        local,
        source,
        _frame(frame, "extend14", 16),
        active["protected"] + (_object(output, "Result"),),
    )
    outcomes = []
    for alternative in appended["outcomes"]:
        if alternative["kind"] == "success":
            prefix = InitializedPrefix(
                borrow(output.storage, output.offset + 8, 14, frame.phase), alternative["length"]
            )
            outcomes.append(
                dict(
                    kind="success",
                    tag=0,
                    possible_writes=((output, 0, 24),),
                    opaque_copy=(4, 20),
                    semantic_string_header=(4, 4),
                    initialized_prefix=prefix,
                )
            )
        else:
            outcomes.append(
                dict(
                    kind="capacity_error",
                    tag=1,
                    possible_writes=((output, 0, 4),),
                    error_payload_bytes=0,
                )
            )
    result = _return(proof, outcomes, (appended,), (48, 16, 24), ((local.view, 0, 4),))
    result["requires_source_valid_str"] = True
    return result
