"""Pinned heapless0.9.3 construction families for default refill.

Complete Thumb operands/control/save/return contracts use the existing finite
matcher. Closed capacity substitution is authored, never learned from bytes.
Changed compiler/source/ABI needs complete source/byte review and mutations.
Actual parent arguments/epochs and owned effects remain separate obligations.
"""

_INNER = "_RINvNvMs_NtCs_8heapless3vecINtB7_8VecInnerpppE17extend_from_slice23extend_from_slice_innerhjECs_16dashboard_config"
_VEC = "_RNvMs_NtCs_8heapless3vecINtB4_8VecInnerhjINtNtB4_7storage15VecStorageInnerAINtNtNtCs_4core3mem12maybe_uninit11MaybeUninithEj{capacity}_EE17extend_from_sliceCs_16dashboard_config"
_STRING = "_RNvXs4_NtCs_8heapless6stringINtB5_11StringInnerjINtNtNtB7_3vec7storage15VecStorageInnerAINtNtNtCs_4core3mem12maybe_uninit11MaybeUninithEje_EEINtNtB1G_7convert7TryFromReE8try_fromCs_16dashboard_config"
CONSTRUCTION_SEMANTIC_ROLES = {
    "vec_extend_inner": _INNER,
    "string_try14": _STRING,
    "construction_prepare_slice": "@outline:construction_prepare_slice",
    "construction_restore_r11_low20": "@outline:construction_restore_r11_low20",
    "construction_restore_r8_low20": "@outline:construction_restore_r8_low20",
}


def _contract(extent, frame, instructions):
    return dict(
        extent=extent,
        frame=frame,
        mapping=[(0, extent, "t")],
        objects=[],
        data=[],
        instructions=instructions,
    )


CONSTRUCTION_GRAMMARS = {
    "construction_prepare_slice": _contract(
        8,
        0,
        [
            (0, 2, "mov r3, r1"),
            (2, 2, "str r2, [sp]"),
            (4, 2, "adds r1, r0, #0x4"),
            (6, 2, "bx lr"),
        ],
    ),
    "construction_restore_r11_low20": _contract(
        6, 0, [(0, 4, "ldr r11, [sp], #4"), (4, 2, "pop {r4, r5, r6, r7, pc}")]
    ),
    "construction_restore_r8_low20": _contract(
        6, 0, [(0, 4, "ldr r8, [sp], #4"), (4, 2, "pop {r4, r5, r6, r7, pc}")]
    ),
    "vec_extend_inner": _contract(
        50,
        24,
        [
            (0, 2, "push {r4, r5, r6, r7, lr}"),
            (2, 2, "add r7, sp, #0xc"),
            (4, 4, "str r11, [sp, #-4]!"),
            (8, 2, "ldr r5, [r0]"),
            (10, 2, "ldr r6, [r7, #0x8]"),
            (12, 4, "add.w r12, r5, r6"),
            (16, 2, "cmp r12, r2"),
            (18, 2, "bls @self+20"),
            (20, 2, "movs r0, #0x0"),
            (22, 2, "cmp r12, r2"),
            (24, 2, "it hi"),
            (26, 2, "movhi r0, #0x1"),
            (28, 4, "b.w @outline:construction_restore_r11_low20"),
            (32, 2, "cmp r6, #0x0"),
            (34, 2, "beq @self+14"),
            (36, 4, "ldrb r4, [r3], #1"),
            (40, 2, "subs r6, #0x1"),
            (42, 2, "strb r4, [r1, r5]"),
            (44, 2, "adds r5, #0x1"),
            (46, 2, "str r5, [r0]"),
            (48, 2, "b @self+20"),
        ],
    ),
    "string_try14": _contract(
        52,
        48,
        [
            (0, 2, "push {r4, r5, r6, r7, lr}"),
            (2, 2, "add r7, sp, #0xc"),
            (4, 4, "str r8, [sp, #-4]!"),
            (8, 2, "sub sp, #0x18"),
            (10, 2, "mov r8, r0"),
            (12, 2, "movs r0, #0x0"),
            (14, 2, "add r5, sp, #0x4"),
            (16, 2, "str r0, [sp, #0x4]"),
            (18, 2, "mov r0, r5"),
            (20, 4, "bl @role:" + _VEC.format(capacity="e")),
            (24, 2, "cbz r0, @self+1e"),
            (26, 2, "movs r0, #0x1"),
            (28, 2, "b @self+2a"),
            (30, 4, "ldm.w r5, {r1, r2, r3, r4, r6}"),
            (34, 4, "add.w r0, r8, #0x4"),
            (38, 2, "stm r0!, {r1, r2, r3, r4, r6}"),
            (40, 2, "movs r0, #0x0"),
            (42, 4, "str.w r0, [r8]"),
            (46, 2, "add sp, #0x18"),
            (48, 4, "b.w @outline:construction_restore_r8_low20"),
        ],
    ),
}
for _capacity in (14, 20, 24):
    _semantic = "vec_extend" + str(_capacity)
    CONSTRUCTION_SEMANTIC_ROLES[_semantic] = _VEC.format(capacity=format(_capacity, "x"))
    CONSTRUCTION_GRAMMARS[_semantic] = _contract(
        16,
        16,
        [
            (0, 2, "push {r5, r6, r7, lr}"),
            (2, 2, "add r7, sp, #0x8"),
            (4, 4, "bl @outline:construction_prepare_slice"),
            (8, 2, "movs r2, #" + hex(_capacity)),
            (10, 4, "bl @role:" + _INNER),
            (14, 2, "pop {r2, r3, r7, pc}"),
        ],
    )
