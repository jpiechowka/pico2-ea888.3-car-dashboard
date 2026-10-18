"""Authored SizeZ D/Q constructor regions, not whole-parent permissions.

The captured calls bind the BEFORE-CALL state. Their return effects must be
composed before any successor may consume it. EAV data are checked source
parameters, never accepted body fingerprints. Other shapes fail closed until
their source/layout/phase families are implemented and reviewed. Parent entry,
all table/indirect ingress and complete ordinary coverage remain required.
"""

import re
import struct
import zlib
from config_decode_effects import DecoderState, SequenceState, sequence_effect, _object
from config_entry import CONFIG, entry_object
from config_finite import _literal_body, _complete_body
from config_helpers import helper_coverage, _helper_function, HELPER_SEMANTIC_ROLES
from config_regions import (
    Storage,
    Region,
    Transfer,
    borrow,
    require_disjoint,
    require_writes,
    audit_coverage,
)
from config_stack import canonical_symbol, typed_function, transfer_family
from llvm_stack import mapping_ranges
from validation import require

ROOT = "_RNvMNtNtCs_16dashboard_config8validate4slotNtB2_10ConfigSlot12load_at_boot"
VISITOR = "_RINvXs_NtCs_16dashboard_config15decode_in_placeNtB5_10ConfigSeedNtNtCs_10serde_core2de7Visitor9visit_seqINtNtNtCs_8postcard2de12deserializer9SeqAccessNtNtB25_7flavors5SliceEEB7_"
NEXT_U16 = "_RINvNtCs_16dashboard_config15decode_in_place4nexttINtNtNtCs_8postcard2de12deserializer9SeqAccessNtNtB12_7flavors5SliceEEB4_"
SEQ_U16 = HELPER_SEMANTIC_ROLES["seq_u16"]
NEXT_ROWS = (
    (0, 2, "push {r7, lr}"),
    (2, 2, "mov r7, sp"),
    (4, 4, "bl @role:" + SEQ_U16),
    (8, 4, "movw r1, #0xffff"),
    (12, 2, "uxth r3, r0"),
    (14, 4, "bic.w r2, r0, r1"),
    (18, 2, "cmp r3, #0x2"),
    (20, 2, "it eq"),
    (22, 4, "andeq r2, r0, #0xff0000"),
    (26, 2, "lsls r1, r0, #0x1f"),
    (28, 4, "movw r1, #0xe01"),
    (32, 2, "it ne"),
    (34, 4, "addne.w r1, r2, #0xe00"),
    (38, 4, "mov.w r2, #0xff00"),
    (42, 2, "cmp r3, #0x2"),
    (44, 4, "and.w r0, r2, r0, lsr #8"),
    (48, 2, "it eq"),
    (50, 2, "addeq r1, r0, #0x1"),
    (52, 2, "mov r0, r1"),
    (54, 2, "pop {r7, pc}"),
)

ROOT_ROWS = (
    (0, 2, "push {r4, r5, r6, r7, lr}"),
    (2, 2, "add r7, sp, #0xc"),
    (4, 4, "push.w {r7, r8, r9, r10, r11}"),
    (8, 4, "vpush {d8, d9, d10, d11, d12, d13, d14, d15}"),
    (12, 2, "sub sp, #0x160"),
    (14, 4, "ldr.w r12, [pc, @object0.pc_slot]"),
    (18, 4, "movw r0, @payload_length"),
    (22, 2, "ldr r5, [pc, @object1.pc_slot]"),
    (24, 4, "mov.w r3, #0xffffffff"),
    (28, 4, "mov.w r9, #0x0"),
    (32, 4, "add.w lr, sp, #0x70"),
    (36, 2, "str r0, [sp, #0x7c]"),
    (38, 2, "movs r0, #0x8"),
    (40, 4, "add.w r8, r12, #0xc"),
    (44, 2, "str r0, [sp, #0x74]"),
    (46, 4, "str.w r12, [sp, #0x70]"),
    (50, 4, "str.w r8, [sp, #0x78]"),
    (54, 4, "cmp.w r9, #0x10"),
    (58, 2, "beq @self+6e"),
    (60, 4, "add.w r0, lr, r9"),
    (64, 4, "ldr.w r6, [lr, r9]"),
    (68, 2, "ldr r0, [r0, #0x4]"),
    (70, 2, "add r0, r6"),
    (72, 2, "cmp r6, r0"),
    (74, 2, "beq @self+68"),
    (76, 2, "ldrb r4, [r6]"),
    (78, 2, "eors r3, r4"),
    (80, 2, "movs r4, #0x8"),
    (82, 2, "cbz r4, @self+64"),
    (84, 4, "eor.w r2, r5, r3, lsr #1"),
    (88, 2, "lsls r1, r3, #0x1f"),
    (90, 2, "it eq"),
    (92, 2, "lsreq r2, r3, #0x1"),
    (94, 2, "subs r4, #0x1"),
    (96, 2, "mov r3, r2"),
    (98, 2, "b @self+52"),
    (100, 2, "adds r6, #0x1"),
    (102, 2, "b @self+48"),
    (104, 4, "add.w r9, r9, #0x8"),
    (108, 2, "b @self+36"),
    (110, 2, "ldr r0, [pc, @object2.pc_slot]"),
    (112, 2, "cmp r3, r0"),
    (114, 2, "bne @checksum_failure"),
    (116, 4, "addw r0, r12, @file_length"),
    (120, 4, "strd r8, r0, [sp, #236]"),
    (124, 2, "add r0, sp, #0xec"),
    (126, 4, "bl @role:" + VISITOR),
)
VISITOR_ROWS = (
    (0, 2, "push {r4, r5, r6, r7, lr}"),
    (2, 2, "add r7, sp, #0xc"),
    (4, 4, "push.w {r7, r8, r9, r10, r11}"),
    (8, 4, "vpush {d8, d9, d10, d11, d12, d13, d14}"),
    (12, 4, "sub.w sp, sp, #0x5e8"),
    (16, 2, "movs r1, #0xd"),
    (18, 4, "strd r0, r1, [sp, #288]"),
    (22, 2, "add r0, sp, #0x120"),
    (24, 4, "bl @role:" + NEXT_U16),
)


def _no_object(*args):
    require(False, "unexpected constructor object/outline")


def _parent(objects, name, role, frame, size):
    require(
        name in objects.elf.functions and canonical_symbol(name) == role,
        "unreviewed typed constructor parent",
    )
    start = objects.elf.functions[name][0] & ~1
    extent, actual, _ = typed_function(objects, start, role)
    require(
        actual == size
        and isinstance(frame, Storage)
        and frame.space.startswith("stack:")
        and frame.extent == size
        and frame.alignment >= 8
        and frame.writable,
        "constructor activation/prologue metadata differs",
    )
    return start, extent


def _pc_word(objects, name, row, register):
    match = re.fullmatch(
        register + r", \[pc, #(0x[0-9a-f]+|[0-9]+)\]", row.operands.split("@", 1)[0].strip()
    )
    require(
        row.mnemonic in ("ldr", "ldr.w") and match is not None,
        "constructor readonly producer differs",
    )
    pool = ((row.address + 4) & ~3) + int(match[1], 0)
    require(
        pool % 4 == 0
        and any(
            k == "d" and lo <= pool < pool + 4 <= hi
            for lo, hi, k in mapping_ranges(objects.elf, name)
        ),
        "constructor pool is not mapped data owned by its parent",
    )
    return struct.unpack("<I", objects.read_only(pool, 4))[0]


def _direct_ingress(instructions, start, end):
    for at, row in instructions.items():
        if start <= at < end:
            continue
        if transfer_family(row.mnemonic.removesuffix(".w")) not in ("b", "bl", "cbz", "cbnz"):
            continue
        target = re.search(r"(?<![#\w])0x([0-9a-f]+)(?:\s|$)", row.operands)
        require(
            target is None or not start < int(target[1], 16) < end,
            "foreign direct ingress skips constructor state",
        )


def root_decoder_header(objects, name, instructions, frame, ancestor_protected):
    start, extent = _parent(objects, name, ROOT, frame, 456)
    ancestor_protected = tuple(ancestor_protected)
    require_disjoint((frame,) + ancestor_protected)
    require(start + 14 in instructions, "missing root source producer")
    address = _pc_word(objects, name, instructions[start + 14], "r12")
    header = objects.read_only(address, 12)
    require(
        header[:4] == b"EAV1" and struct.unpack_from("<H", header, 4)[0] == 4,
        "unreviewed embedded current-version envelope",
    )
    n = struct.unpack_from("<H", header, 6)[0]
    length = n + 12
    require(
        0 < n
        and length <= min(16384, 4095)
        and 0x10000000 <= address < address + length <= 0x101FE000,
        "embedded input does not fit this finite ADDW constructor/XIP family",
    )
    raw = objects.read_only(address, length)
    checksum = struct.unpack_from("<I", header, 8)[0]
    require(zlib.crc32(raw[:8] + raw[12:]) == checksum, "embedded envelope checksum differs")
    # A separate typed checksum-failure anchor binds the external branch. Its
    # default-fill successor/source fault semantics still need their own family.
    failure_rows = (
        (0, 4, "movw r0, #" + hex(n)),
        (4, 2, "movs r5, #0x0"),
        (6, 2, "movs r4, #0x5"),
        (8, 2, "str r0, [sp, #0x8]"),
    )
    failure_contract = dict(extent=10, instructions=failure_rows, objects=[], data=[])
    failures = []
    for at, row in sorted(instructions.items()):
        if not start + 130 <= at < start + extent - 10 or row.mnemonic != "movw":
            continue
        try:
            failure = _literal_body(
                objects,
                name,
                instructions,
                ROOT,
                "checksum_failure",
                failure_contract,
                _no_object,
                _no_object,
                window=at,
            )
        except AssertionError:
            continue
        failures.append(failure)
    require(len(failures) == 1, "checksum-failure source anchor is missing or ambiguous")
    failure = failures[0]

    def object_consumer(objects, value, descriptor):
        expected = {
            "embedded_eav": address,
            "crc_polynomial": 0xEDB88320,
            "complement_checksum": checksum ^ 0xFFFFFFFF,
        }
        require(
            descriptor["kind"] in expected and value == expected[descriptor["kind"]],
            "constructor source/polynomial/checksum binding differs",
        )
        return ()

    rows = [
        (
            off,
            width,
            text.replace("@payload_length", "#" + hex(n))
            .replace("@file_length", "#" + hex(length))
            .replace("@checksum_failure", "@self+" + format(failure["function"] - start, "x")),
        )
        for off, width, text in ROOT_ROWS
    ]
    contract = dict(
        extent=130,
        instructions=rows,
        objects=[dict(kind=k) for k in ("embedded_eav", "crc_polynomial", "complement_checksum")],
        data=[],
    )
    proof = _literal_body(
        objects,
        name,
        instructions,
        ROOT,
        "sizez_root_D",
        contract,
        object_consumer,
        _no_object,
        window=start,
    )
    _direct_ingress(instructions, start, start + 130)
    saved = Storage(
        frame.space, frame.identity + ":saved", frame.start + 352, 104, 4, frame.phase, False
    )
    protected = ancestor_protected + (saved,)
    D = borrow(frame, 236, 8, frame.phase, writable=True)
    crc = borrow(frame, 112, 16, frame.phase, writable=True)
    failure_length = borrow(frame, 8, 4, frame.phase, writable=True)
    require_writes(
        (D, crc, failure_length), ((D, 0, 8), (crc, 0, 16), (failure_length, 0, 4)), protected
    )
    owner = Storage("address", "embedded_eav", address, length, 1, frame.phase, False)
    payload = borrow(owner, 12, n, frame.phase)
    state = DecoderState(D, payload, 0)
    return dict(
        proof=proof,
        decoder_state=state,
        protected=protected,
        checksum_failure=failure,
        call_site=start + 126,
        call_role=VISITOR,
        state_coordinate="before_call",
        source_phase="embedded_envelope_checked",
        writes=((crc, 0, 16), (D, 0, 8)),
        checksum_failure_writes=((failure_length, 0, 4),),
        checksum_failure_requires_fault_mapping_and_default_successor=True,
        crc_loop_invariants=(
            "two source slices: header[0,8),payload[0,n)",
            "r9 descriptor offset0,8,16; byte cursor within selected slice",
            "r4 bit count8..0; reflected polynomial0xedb88320",
        ),
        requires_first_consumer_effect_before_exit=True,
        requires_parent_entry_all_ingress_source_effects_and_coverage=True,
    )


def visitor_sequence_header(objects, name, instructions, frame, decoder, ancestor_protected):
    start, extent = _parent(objects, name, VISITOR, frame, 1608)
    ancestor_protected = tuple(ancestor_protected)
    require(
        isinstance(decoder, DecoderState)
        and decoder.header.storage.space == frame.space
        and decoder.header.storage.phase == frame.phase,
        "visitor input has another D/phase ABI",
    )
    require_disjoint((frame,) + ancestor_protected)
    require_disjoint((frame, _object(decoder.header, "live-D")))
    contract = dict(extent=28, instructions=VISITOR_ROWS, objects=[], data=[])
    proof = _literal_body(
        objects,
        name,
        instructions,
        VISITOR,
        "sizez_Config_Q",
        contract,
        _no_object,
        _no_object,
        window=start,
    )
    _direct_ingress(instructions, start, start + 28)
    saved = Storage(
        frame.space, frame.identity + ":saved", frame.start + 1512, 96, 4, frame.phase, False
    )
    protected = ancestor_protected + (saved,)
    Q = borrow(frame, 288, 8, frame.phase, writable=True)
    require_writes(
        (Q,),
        ((Q, 0, 8),),
        protected
        + (_object(decoder.header, "live-D"), _object(decoder.payload, "immutable-input")),
    )
    return dict(
        proof=proof,
        sequence_state=SequenceState(Q, decoder, 13),
        protected=protected,
        call_site=start + 24,
        call_role=NEXT_U16,
        state_coordinate="before_call",
        source_phase="Config_thirteen_fields",
        writes=((Q, 0, 8),),
        requires_first_consumer_effect_before_exit=True,
        requires_parent_entry_all_ingress_source_effects_and_coverage=True,
    )


def root_visitor_headers(objects, name, instructions, frame, ancestor_protected):
    """One checked root→visitor construction edge, retaining both owners.

    This stops BEFORE the first next<u16> call. Neither the root's visitor
    return nor that wrapper's return ABI/effects are inferred from these facts.
    """
    root = root_decoder_header(objects, name, instructions, frame, ancestor_protected)
    edges = [e for e in root["proof"]["ordinary_edges"] if e[3] == VISITOR]
    require(
        len(edges) == 1 and edges[0][0] == 126 and edges[0][4] == 1608,
        "root visitor edge/activation differs",
    )
    target = edges[0][2]
    names = [
        n
        for n, e in objects.elf.functions.items()
        if e[0] & ~1 == target and canonical_symbol(n) == VISITOR
    ]
    require(len(names) == 1, "root visitor target is ambiguous")
    child = Storage(frame.space, "ConfigSeed", frame.start - 1608, 1608, 8, frame.phase, True)
    visitor = visitor_sequence_header(
        objects, names[0], instructions, child, root["decoder_state"], root["protected"]
    )
    return dict(
        root=root,
        visitor=visitor,
        visitor_frame=child,
        state_coordinate="before_first_next_call",
        retained_parent_frames=(456, 1608),
        requires_first_consumer_effect_before_exit=True,
        requires_parent_entry_all_ingress_source_effects_and_coverage=True,
    )


def _next_u16_binding(objects, name, instructions, frame, ancestor_protected):
    """One shared complete wrapper/inner binding for exact and abstract users."""
    start, extent = _parent(objects, name, NEXT_U16, frame, 8)
    ancestor_protected = tuple(ancestor_protected)
    require_disjoint((frame,) + ancestor_protected)
    contract = dict(
        extent=56, frame=8, mapping=[(0, 56, "t")], instructions=NEXT_ROWS, objects=[], data=[]
    )
    proof = _complete_body(
        objects,
        name,
        instructions,
        NEXT_U16,
        "next_u16",
        contract,
        _no_object,
        _no_object,
        function_consumer=_helper_function,
    )
    edges = proof["ordinary_edges"]
    require(
        len(edges) == 1 and edges[0][0] == 4 and edges[0][3] == SEQ_U16,
        "outer next child ABI differs",
    )
    target = edges[0][2]
    names = [
        n
        for n, e in objects.elf.functions.items()
        if e[0] & ~1 == target and canonical_symbol(n) == SEQ_U16
    ]
    require(len(names) == 1, "outer next has no exact inner sequence")
    context = (start, "complete outer next", "u16 source constructor still required")
    region = Region(
        context,
        "helper",
        start,
        start + 56,
        tuple(start + o for o, w, t in NEXT_ROWS),
        (),
        (
            Transfer(start + 4, "bl", "call", target, "typed inner sequence", SEQ_U16),
            Transfer(start + 54, "pop", "return", None, "restored outer next"),
        ),
    )
    coverage = audit_coverage(
        objects, name, instructions, context, (region,), _function_consumer=_helper_function
    )
    child = helper_coverage(objects, names[0], instructions, (target, "outer next", start))
    child_frame = Storage(
        frame.space, frame.identity + ":inner-seq", frame.start - 8, 8, 8, frame.phase, True
    )
    saved = Storage(frame.space, frame.identity + ":saved", frame.start, 8, 4, frame.phase, False)
    return proof, coverage, child, child_frame, ancestor_protected + (saved,)


def next_u16_effect(objects, name, instructions, state, frame, ancestor_protected):
    """Exact outer next<Result<u16>> ABI, distinct from SeqAccess<Option<u16>>."""
    proof, coverage, child, child_frame, protected = _next_u16_binding(
        objects, name, instructions, frame, ancestor_protected
    )
    sequence = sequence_effect(objects, instructions, child, state, child_frame, protected)
    if sequence["tag"] == 1:
        value = sequence["value"]
        error = None
        packed = (value << 16) | 0xE00
    else:
        value = None
        error = 14 if sequence["tag"] == 0 else sequence["error"]
        require(error in (4, 5, 14), "outer next error exceeds its exact child/source domain")
        packed = (error << 8) | 1
    return dict(
        proof=proof,
        coverage=coverage,
        state=sequence["state"],
        sequence=sequence,
        registers={"r0": packed},
        value=value,
        error=error,
        writes=sequence["writes"],
        input_reads=sequence["input_reads"],
        conservative_activation_frames=(8,) + sequence["conservative_activation_frames"],
        normal_preserves=sequence["normal_preserves"],
        requires_machine_source_phase_arguments_lifetime_and_parent_coverage=True,
    )


def _next_u16_interface(objects, name, instructions, state, frame, ancestor_protected):
    from config_abstract import sequence_interface

    proof, coverage, child, child_frame, protected = _next_u16_binding(
        objects, name, instructions, frame, ancestor_protected
    )
    sequence = sequence_interface(objects, instructions, child, state, child_frame, protected)
    stable = (
        _object(borrow(state.header.storage, state.header.offset, 4, frame.phase), "Q.D pointer"),
        _object(
            borrow(state.decoder.header.storage, state.decoder.header.offset + 4, 4, frame.phase),
            "D.end",
        ),
    )
    outcomes = []
    for inner in sequence["outcomes"]:
        require_writes((state.header, state.decoder.header), inner["possible_writes"], stable)
        require(
            all(
                (view.start + offset, n)
                in ((state.header.start + 4, 4), (state.decoder.header.start, 4))
                for view, offset, n in inner["possible_writes"]
            ),
            "outer abstract next exceeds Q.count/D.cursor cells",
        )
        if inner["kind"] == "some":
            require(
                inner["value_kind"] == "u16" and inner["value_domain"] == (0, 65535),
                "outer abstract u16 domain differs",
            )
            error = None
            packed = dict(tag=0, value_domain=(0, 65535), shift=16, constant=0xE00)
        else:
            error = 14 if inner["kind"] == "none" else inner["error"]
            require(error in (4, 5, 14), "outer abstract error domain differs")
            packed = (error << 8) | 1
        outcomes.append(
            dict(
                inner=inner,
                state=inner["state"],
                error=error,
                packed_r0=packed,
                value_domain=inner["value_domain"],
                possible_writes=inner["possible_writes"],
            )
        )
    return dict(
        proof=proof,
        coverage=coverage,
        sequence=sequence,
        outcomes=tuple(outcomes),
        normal_preserves=sequence["normal_preserves"],
        conservative_activation_frames=(8,) + sequence["conservative_activation_frames"],
    )


def root_first_u16(objects, name, instructions, frame, ancestor_protected):
    """Construct D/Q, then prove only the first next return to the visitor."""
    headers = root_visitor_headers(objects, name, instructions, frame, ancestor_protected)
    visitor = headers["visitor"]
    edges = visitor["proof"]["ordinary_edges"]
    require(
        len(edges) == 1 and edges[0][0] == 24 and edges[0][3] == NEXT_U16,
        "first field call/argument bridge differs",
    )
    target = edges[0][2]
    names = [
        n
        for n, e in objects.elf.functions.items()
        if e[0] & ~1 == target and canonical_symbol(n) == NEXT_U16
    ]
    require(len(names) == 1, "first next target is ambiguous")
    active = Storage(
        frame.space, "next_u16", headers["visitor_frame"].start - 8, 8, 8, frame.phase, True
    )
    returned = next_u16_effect(
        objects, names[0], instructions, visitor["sequence_state"], active, visitor["protected"]
    )
    return dict(
        headers=headers,
        first_return=returned,
        state_coordinate="after_first_next_return",
        continuation=visitor["call_site"] + 4,
        retained_activation_frames=(456, 1608) + returned["conservative_activation_frames"],
        requires_actual_Config_field_store_and_all_parent_success_error_continuations=True,
        requires_parent_entry_all_ingress_source_effects_and_coverage=True,
    )


# Source ConfigSeed::visit_seq assigns these two fields, in this order. These
# are field-store witnesses, not a Rust layout oracle or Config validity test.
U16_FIELDS = (("version", 14570), ("catalogue_version", 14572))


def _u16_field_regions(objects, name, instructions, frame):
    start, _ = _parent(objects, name, VISITOR, frame, 1608)
    first = (
        (0, 2, "lsls r1, r0, #0x1f"),
        (2, 2, "bne @self+1a"),
        (4, 4, "ldr.w r4, [pc, @object0.pc_slot]"),
        (8, 2, "lsrs r0, r0, #0x10"),
        (10, 4, "movw r1, #0x38ea"),
        (14, 2, "strh r0, [r4, r1]"),
        (16, 2, "add r0, sp, #0x120"),
        (18, 4, "bl @role:" + NEXT_U16),
    )
    guard = ((0, 2, "lsls r1, r0, #0x1f"), (2, 2, "beq @self+16"))
    second = (
        (0, 2, "lsrs r0, r0, #0x10"),
        (2, 4, "movw r1, #0x38ec"),
        (6, 2, "strh r0, [r4, r1]"),
        (8, 2, "add r0, sp, #0x120"),
    )
    error = ((0, 2, "lsrs r5, r0, #0x8"), (2, 2, "mov r0, r5"))
    proofs = []
    for offset, extent, rows, descriptors in (
        (28, 22, first, [dict(kind="entry_static", role=CONFIG)]),
        (50, 4, guard, []),
        (72, 10, second, []),
        (54, 4, error, []),
    ):
        contract = dict(extent=extent, instructions=rows, objects=descriptors, data=[])
        proof = _literal_body(
            objects,
            name,
            instructions,
            VISITOR,
            "two_u16_fields:" + str(offset),
            contract,
            entry_object,
            _no_object,
            window=start + offset,
        )
        # The shared error anchor has additional later-field predecessors to
        # MOV r0,r5. This proves its bytes/destination only, never their state.
        if offset != 54:
            _direct_ingress(instructions, start + offset, start + offset + extent)
        proofs.append(proof)
    return tuple(proofs)


def root_u16_fields(objects, name, instructions, frame, ancestor_protected):
    """Two unpublished source assignments; stop before boot_view executes.

    Initial static bytes/identity are checked, but exclusive take/initialized
    ConfigSlot ownership must still compose with entry and source construction.
    Child writes are only Q/D; their current preserved-register contract keeps
    the actual r4 destination across the second wrapper call. Errors enter the
    separately bound shared suffix; its remaining restore/default path is owed.
    """
    first = root_first_u16(objects, name, instructions, frame, ancestor_protected)
    headers = first["headers"]
    visitor = headers["visitor"]
    active = headers["visitor_frame"]
    start = visitor["proof"]["function"]
    visitor_names = [
        n
        for n, e in objects.elf.functions.items()
        if e[0] & ~1 == start and canonical_symbol(n) == VISITOR
    ]
    require(len(visitor_names) == 1, "scalar field visitor target is ambiguous")
    regions = _u16_field_regions(objects, visitor_names[0], instructions, active)
    address = regions[0]["objects"][0]
    owner = Storage("address", "partial_ConfigSlot", address, 14604, 4, frame.phase, True)
    state = first["first_return"]["state"]
    returned = first["first_return"]
    fields = []
    writes = []
    returns = [returned]
    # Full live destination must be separate from immutable source and the
    # caller's saved cells; each actual field write has its exact smaller view.
    protected = visitor["protected"] + (
        _object(state.header, "live-Q"),
        _object(state.decoder.header, "live-D"),
        _object(state.decoder.payload, "immutable-input"),
    )
    require_disjoint((owner,) + protected)
    for ordinal, (field, offset) in enumerate(U16_FIELDS):
        if returned["error"] is not None:
            break
        value = returned["value"]
        require(type(value) is int and 0 <= value <= 65535, "u16 source value differs")
        view = borrow(owner, offset, 2, frame.phase, writable=True)
        require_writes((view,), ((view, 0, 2),), protected)
        fields.append(
            dict(
                field=field,
                offset=offset,
                extent=2,
                value=value,
                phase="partial_decoded_unpublished",
                destination=view,
            )
        )
        writes.append((view, 0, struct.pack("<H", value)))
        if ordinal == 0:
            edge = regions[0]["ordinary_edges"]
            require(
                len(edge) == 1 and edge[0][0] == 18 and edge[0][3] == NEXT_U16,
                "second u16 call edge differs",
            )
            target = edge[0][2]
            names = [
                n
                for n, e in objects.elf.functions.items()
                if e[0] & ~1 == target and canonical_symbol(n) == NEXT_U16
            ]
            require(len(names) == 1, "second u16 wrapper target is ambiguous")
            child = Storage(
                frame.space, "next_catalogue_version", active.start - 8, 8, 8, frame.phase, True
            )
            returned = next_u16_effect(
                objects, names[0], instructions, state, child, visitor["protected"]
            )
            require(
                "r4" in returned["normal_preserves"],
                "second u16 call does not preserve Config destination",
            )
            state = returned["state"]
            returns.append(returned)
    failed = returned["error"] is not None
    return dict(
        first_bridge=first,
        regions=regions,
        field_returns=tuple(returns),
        fields=tuple(fields),
        writes=tuple(writes),
        state=state,
        error=returned["error"],
        continuation=start + (54 if failed else 82),
        state_coordinate="before_shared_error_suffix" if failed else "before_boot_view_call",
        source_field_order=tuple(f for f, o in U16_FIELDS),
        requires_shared_error_restore_default_and_remaining_success_continuations=True,
        requires_exclusive_initialized_Config_source_owner_and_all_remaining_phases=True,
        requires_parent_entry_all_ingress_source_effects_and_coverage=True,
    )
