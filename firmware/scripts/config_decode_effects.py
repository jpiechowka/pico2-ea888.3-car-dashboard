"""Fixed source effects for the four reviewed postcard decoder bodies.

These consume caller-established D/header/input/cursor facts, not arbitrary
machine pointers. The complete current bodies and all required pop children
are rebound before effects are used. Construction, register transport, source
phase, all-ingress and the enclosing diagnostic/ordinary graph remain required.
Only the pinned emitted scalar/result ABIs below are supported. This is not an
instruction evaluator, general postcard implementation or Config validator.
"""

from dataclasses import dataclass, replace
import hashlib
from config_structural import _image_hash, _readonly_hash
import struct
from config_helpers import helper_body, helper_coverage, helper_frame, HELPER_SEMANTIC_ROLES
from config_platform import PlatformPolicy
from config_regions import Storage, View, require_writes
from config_stack import canonical_symbol
from validation import require


@dataclass(frozen=True)
class DecoderState:
    header: View
    payload: View
    cursor: int

    def __post_init__(self):
        require(
            isinstance(self.header, View)
            and self.header.writable
            and self.header.extent == 8
            and self.header.start % 4 == 0
            and self.header.storage.alignment >= 4
            and self.header.storage.space.startswith("stack:"),
            "decoder header lacks its aligned mutable eight-byte source object",
        )
        require(
            isinstance(self.payload, View)
            and self.payload.storage.space == "address"
            and self.payload.start > 0
            and not self.payload.writable
            and not self.payload.storage.writable,
            "decoder bytes lack their complete immutable input allocation",
        )
        require(
            type(self.cursor) is int and 0 <= self.cursor <= self.payload.extent,
            "decoder cursor/end ordering or same-allocation bound differs",
        )


def _object(view, role):
    return Storage(
        view.storage.space,
        view.storage.identity + ":" + role + ":" + str(view.offset),
        view.start,
        view.extent,
        1,
        view.storage.phase,
        False,
    )


def _with_input(protected, payload):
    # The SAME immutable allocation is propagated through nested decoders, not
    # reintroduced as a second owned object. Only exact equality can coalesce.
    owner = _object(payload, "immutable-input")
    return protected if owner in protected else protected + (owner,)


def _current(objects, instructions, proof):
    require(
        isinstance(proof, dict) and "coverage" in proof,
        "decoder effects lack complete body/edge coverage",
    )
    names = [
        n
        for n, e in objects.elf.functions.items()
        if e[0] & ~1 == proof["function"] and canonical_symbol(n) == proof["role"]
    ]
    require(len(names) == 1, "decoder effects have no unique current typed body")
    current = helper_coverage(objects, names[0], instructions, proof["coverage"]["context"])
    require(current == proof, "decoder effects use stale body/child facts")
    return current


def _decoder_dependency(objects, instructions, proof, frame):
    # Only these reviewed parent/outline/decoder edges may compose effects.
    interfaces = {
        "deserialize_u16": ("slice_pop", None),
        "take_varint_u32": ("slice_pop", None),
        "seq_u8": ("slice_pop", "seq_decrement_tail_pop"),
        "seq_bool": ("slice_pop", "seq_decrement_tail_pop"),
        "seq_u16": ("deserialize_u16", "seq_decrement_tail_u16"),
        "seq_f32": ("deserialize_seed_f32", None),
    }
    require(proof["semantic_role"] in interfaces, "unknown decoder parent interface")
    semantic, outline = interfaces[proof["semantic_role"]]
    edges = proof["ordinary_edges"]
    if outline:
        role = HELPER_SEMANTIC_ROLES[outline]
        forward = [e for e in edges if e[3] == role]
        require(len(forward) == 1, "sequence decrement/transport outline differs")
        names = [n for n, e in objects.elf.functions.items() if e[0] & ~1 == forward[0][2]]
        require(len(names) == 1, "sequence outline target is ambiguous")
        bound = helper_body(objects, names[0], instructions, role)
        require(bound["semantic_role"] == outline, "sequence outline ABI differs")
        edges = bound["ordinary_edges"]
    role = HELPER_SEMANTIC_ROLES[semantic]
    edges = [e for e in edges if e[3] == role]
    require(len(edges) == 1, "decoder pop dependency differs")
    target = edges[0][2]
    names = [
        n
        for n, e in objects.elf.functions.items()
        if e[0] & ~1 == target and canonical_symbol(n) == role
    ]
    require(len(names) == 1, "decoder pop has no unique typed target")
    child = helper_coverage(
        objects, names[0], instructions, (target, "fixed decoder pop", proof["function"])
    )
    child_frame = Storage(
        frame.space,
        frame.identity + ":pop",
        frame.start - child["frame"],
        child["frame"],
        8,
        frame.phase,
        True,
    )
    return child, child_frame


def _decoder_child(objects, instructions, proof, frame, state, protected, **kwargs):
    child, child_frame = _decoder_dependency(objects, instructions, proof, frame)
    return decode_effect(objects, instructions, child, state, child_frame, protected, **kwargs)


def decode_effect(
    objects,
    instructions,
    proof,
    state,
    frame,
    ancestor_protected,
    *,
    output=None,
    platform_policy=None,
):
    """Return exact fixed helper effects for the supplied valid source state.

    Bytes are read only from the bound readonly ELF input. Cursor/header values
    are supplied by separately recognized constructors/predecessors; this does
    not read host stack bytes or infer that machine registers equal these facts.
    Error padding/value words remain uninitialized, not zero-filled facts.
    """
    proof = _current(objects, instructions, proof)
    semantic = proof["semantic_role"]
    require(
        semantic in ("slice_pop", "deserialize_u16", "take_varint_u32", "deserialize_seed_f32")
        and isinstance(state, DecoderState),
        "unsupported decoder/source state",
    )
    require(
        state.header.storage.space == frame.space and state.header.storage.phase == frame.phase,
        "decoder header belongs to another caller coordinate/lifetime",
    )
    payload = objects.read_only(state.payload.start, state.payload.extent)
    active = helper_frame(proof, frame, tuple(ancestor_protected))
    protected = _with_input(active["protected"], state.payload)
    # Check the FULL live mutable header capability, including the read-only end
    # word: a prologue can overwrite that word before the equality/range read.
    # This is ownership validation, not a claim that the body stores all 8 bytes.
    require_writes((state.header,), ((state.header, 0, 8),), protected)
    if semantic in ("take_varint_u32", "deserialize_seed_f32"):
        require(
            isinstance(output, View)
            and output.writable
            and output.extent == 8
            and output.start % 4 == 0
            and output.storage.alignment >= 4
            and output.storage.space == frame.space
            and output.storage.phase == frame.phase,
            "decoder Result output ABI differs",
        )
        require_writes((output,), ((output, 0, 8),), protected + (_object(state.header, "D"),))
        require_writes((state.header,), ((state.header, 0, 4),), (_object(output, "Result"),))
        protected = protected + (_object(output, "Result"),)
    else:
        require(output is None, "register-result decoder has an unexpected outpointer")
    if semantic == "deserialize_seed_f32":
        require(
            isinstance(platform_policy, PlatformPolicy)
            and (platform_policy.input_start, platform_policy.input_end)
            == (state.payload.start, state.payload.start + state.payload.extent)
            and platform_policy.input_sha256 == hashlib.sha256(payload).hexdigest()
            and platform_policy.elf_sha256 == _image_hash(objects),
            "f32 decoder lost its current conditional platform/input prerequisite",
        )
    else:
        require(
            platform_policy is None, "non-f32 decoder acquired an unrelated platform prerequisite"
        )
    original = state
    reads = []
    writes = []
    children = []
    value = None
    error = None
    registers = {}
    fields = ()
    if semantic == "slice_pop":
        if state.cursor == len(payload):
            error = 4
            registers = {"r0": 1, "r1": 4}
        else:
            value = payload[state.cursor]
            reads.append((state.payload.start + state.cursor, 1))
            state = replace(state, cursor=state.cursor + 1)
            writes.append((state.header, 0, struct.pack("<I", state.payload.start + state.cursor)))
            registers = {"r0": 0, "r1": value}
    elif semantic == "deserialize_seed_f32":
        if len(payload) - state.cursor < 4:
            error = 4
        else:
            platform_policy.for_f32_load(state.payload.start + state.cursor)
            reads.append((state.payload.start + state.cursor, 4))
            value = int.from_bytes(payload[state.cursor : state.cursor + 4], "little")
            state = replace(state, cursor=state.cursor + 4)
            writes.append((state.header, 0, struct.pack("<I", state.payload.start + state.cursor)))
    else:
        maximum, terminal_maximum = (3, 3) if semantic == "deserialize_u16" else (5, 15)
        value = 0
        # Finite source varint invariant: k successful bytes, 7*k shift;
        # the maximum guard fires before a fourth/sixth pop. No rollback.
        for k in range(maximum):
            pop = _decoder_child(objects, instructions, proof, frame, state, protected)
            children.append(pop)
            state = pop["state"]
            reads.extend(pop["input_reads"])
            writes.extend(pop["writes"])
            if pop["error"] is not None:
                error = pop["error"]
                break
            byte = pop["value"]
            value |= (byte & 127) << (7 * k)
            if byte < 128:
                if k == maximum - 1 and byte > terminal_maximum:
                    error = 5
                break
        else:
            error = 5
        if error is not None:
            value = None
        if semantic == "deserialize_u16":
            registers = {"r0": value << 16 if error is None else (error << 8) | 1}
    if output is not None:
        if error is None:
            writes.extend(((output, 4, struct.pack("<I", value)), (output, 0, b"\0")))
            fields = ((0, 1), (4, 4))
        else:
            writes.extend(((output, 1, bytes((error,))), (output, 0, b"\1")))
            fields = ((0, 1), (1, 1))
        # The Result's own object is protected from child writes, but its listed
        # parent-owned field stores are checked against all other live storage.
        require_writes(
            (output,),
            ((view, offset, len(data)) for view, offset, data in writes if view == output),
            active["protected"]
            + (_object(state.payload, "immutable-input"), _object(state.header, "D")),
        )
    require(error in (None, 4, 5), "decoder emitted an unreviewed error domain")
    return dict(
        semantic_role=semantic,
        proof=proof,
        state=state,
        original_state=original,
        output=output,
        registers=registers,
        value=value,
        error=error,
        input_reads=tuple(reads),
        writes=tuple(writes),
        initialized_result_fields=fields,
        children=tuple(children),
        platform_policy=platform_policy,
        normal_preserves=tuple("r" + str(i) for i in range(4, 12)) + ("sp",),
        conservative_activation_frames=(proof["frame"], 16) if children else (proof["frame"],),
        requires_machine_constructor_arguments_lifetime_ingress_and_enclosing_graph=True,
    )


@dataclass(frozen=True)
class SequenceState:
    header: View
    decoder: DecoderState
    count: int

    def __post_init__(self):
        require(
            isinstance(self.header, View)
            and self.header.writable
            and self.header.extent == 8
            and self.header.start % 4 == 0
            and self.header.storage.alignment >= 4
            and isinstance(self.decoder, DecoderState)
            and self.header.storage.space == self.decoder.header.storage.space
            and self.header.storage.phase == self.decoder.header.storage.phase,
            "sequence has no typed aligned Q/D lifetime",
        )
        require(
            type(self.count) is int and 0 <= self.count <= 0xFFFFFFFF,
            "sequence count has no source usize domain",
        )


def sequence_effect(
    objects,
    instructions,
    proof,
    state,
    frame,
    ancestor_protected,
    *,
    output=None,
    platform_policy=None,
):
    """Four fixed SeqAccess ABIs; source Q.Dptr/count transport stays required."""
    proof = _current(objects, instructions, proof)
    semantic = proof["semantic_role"]
    require(
        semantic in ("seq_u8", "seq_u16", "seq_bool", "seq_f32")
        and isinstance(state, SequenceState),
        "unsupported sequence/source state",
    )
    require(
        state.header.storage.space == frame.space and state.header.storage.phase == frame.phase,
        "sequence header belongs to another activation/lifetime",
    )
    active = helper_frame(proof, frame, tuple(ancestor_protected))
    decoder = state.decoder
    objects.read_only(decoder.payload.start, decoder.payload.extent)
    protected = _with_input(active["protected"], decoder.payload)
    # PUSH initializes scratch slots too. An external live Q/D/RO8 must not
    # overlap ANY byte of this prologue, even a slot later owned as child Result.
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
        (state.header, decoder.header), ((state.header, 0, 8), (decoder.header, 0, 8)), (prologue,)
    )
    require_writes(
        (state.header,), ((state.header, 0, 8),), protected + (_object(decoder.header, "D"),)
    )
    require_writes(
        (decoder.header,), ((decoder.header, 0, 8),), protected + (_object(state.header, "Q"),)
    )
    if semantic == "seq_f32":
        require(
            isinstance(output, View)
            and output.writable
            and output.extent == 8
            and output.start % 4 == 0
            and output.storage.alignment >= 4
            and output.storage.space == frame.space
            and output.storage.phase == frame.phase,
            "sequence f32 output is not exact RO8",
        )
        require_writes(
            (output,),
            ((output, 0, 8),),
            protected + (_object(state.header, "Q"), _object(decoder.header, "D")),
        )
        require_writes((output,), ((output, 0, 8),), (prologue,))
        require_writes(
            (state.header, decoder.header),
            ((state.header, 0, 8), (decoder.header, 0, 8)),
            (_object(output, "RO8"),),
        )
        require(
            isinstance(platform_policy, PlatformPolicy)
            and (platform_policy.input_start, platform_policy.input_end)
            == (decoder.payload.start, decoder.payload.start + decoder.payload.extent)
            and platform_policy.input_sha256
            == _readonly_hash(objects, decoder.payload.start, decoder.payload.extent)
            and platform_policy.elf_sha256 == _image_hash(objects),
            "sequence lost its current conditional f32 prerequisite",
        )
        protected = protected + (_object(output, "RO8"),)
    else:
        require(
            output is None and platform_policy is None,
            "register sequence acquired another output/policy ABI",
        )
    writes = []
    children = []
    registers = {}
    fields = ()
    tag = 0
    value = None
    error = None
    original = state
    if state.count:
        state = replace(state, count=state.count - 1)
        writes.append((state.header, 4, struct.pack("<I", state.count)))
        child_kwargs = {}
        if semantic == "seq_f32":
            scratch = active["child_result_scratch"]
            require(
                len(scratch) == 1 and scratch[0].extent == 8,
                "sequence child Result scratch differs",
            )
            child_kwargs = dict(output=scratch[0], platform_policy=platform_policy)
        child = _decoder_child(
            objects,
            instructions,
            proof,
            frame,
            decoder,
            protected + (_object(state.header, "Q"),),
            **child_kwargs,
        )
        children.append(child)
        writes.extend(child["writes"])
        state = replace(state, decoder=child["state"])
        value, error = child["value"], child["error"]
        tag = 2 if error is not None else 1
        if semantic == "seq_u8":
            registers = {"r0": (value << 8) | 1 if error is None else (error << 8) | 2}
        elif semantic == "seq_u16":
            require(
                error in (None, 4, 5), "sequence u16 error exceeds actual four-bit child domain"
            )
            registers = {"r0": (value << 16) | 1 if error is None else (error << 16) | 2}
        elif semantic == "seq_bool":
            if error is None and value not in (0, 1):
                error = 6
                tag = 2
                value = None
            registers = {"r0": int(error is not None), "r1": error if error is not None else value}
    elif semantic in ("seq_u8", "seq_u16"):
        registers = {"r0": 0}
    elif semantic == "seq_bool":
        registers = {"r0": 0, "r1": 2}
    if semantic == "seq_f32":
        if tag == 0:
            writes.append((output, 0, bytes(4)))
            fields = ((0, 4),)
        elif tag == 1:
            writes.append((output, 0, struct.pack("<II", 1, value)))
            fields = ((0, 4), (4, 4))
        else:
            writes.extend(((output, 0, struct.pack("<I", 2)), (output, 4, bytes((error,)))))
            fields = ((0, 4), (4, 1))
        require_writes(
            (output,),
            ((view, offset, len(data)) for view, offset, data in writes if view == output),
            active["protected"]
            + (
                _object(state.header, "Q"),
                _object(decoder.header, "D"),
                _object(decoder.payload, "immutable-input"),
            ),
        )
    return dict(
        semantic_role=semantic,
        proof=proof,
        state=state,
        original_state=original,
        output=output,
        tag=tag,
        value=value,
        error=error,
        registers=registers,
        writes=tuple(writes),
        children=tuple(children),
        initialized_result_fields=fields,
        platform_policy=platform_policy,
        input_reads=tuple(read for child in children for read in child["input_reads"]),
        normal_preserves=tuple("r" + str(i) for i in range(4, 12)) + ("sp",),
        conservative_activation_frames={
            "seq_u8": (8, 16),
            "seq_bool": (8, 16),
            "seq_u16": (8, 24, 16),
            "seq_f32": (24, 8),
        }[semantic],
        requires_machine_constructor_arguments_lifetime_ingress_and_enclosing_graph=True,
    )
