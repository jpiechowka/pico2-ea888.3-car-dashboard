"""Closed loader continuation contracts from slot/codec/cache_identity sources.

These finite phase sequences complement the independently matched issuer and
terminal roots. Target addresses bind current structural role/phase witnesses;
no runtime grammar generation, body-hash admission or generalized evaluator.
Changed source/compiler emission requires explicit review and covering tests.
"""

from config_body_types import BodyContract
from config_entry_contracts import EntryStaticObject


CONTINUATIONS: dict[str, BodyContract[EntryStaticObject]] = {
    "fallback": {
        "extent": 62,
        "instructions": [
            (0, 4, "ldrh.w r0, [sp, #0xec]"),
            (4, 2, "cmp r0, #0x2"),
            (6, 4, "bne.w @default_error"),
            (10, 2, "ldr r1, [sp, #0x8]"),
            (12, 4, "movw r0, #0x3904"),
            (16, 4, "str.w r1, [r10, r0]"),
            (20, 2, "ldr r1, [sp, #0xc]"),
            (22, 4, "mov.w r0, #0x3900"),
            (26, 4, "str.w r1, [r10, r0]"),
            (30, 2, "ldr r1, [sp, #0xf0]"),
            (32, 4, "movw r0, #0x38f0"),
            (36, 4, "str.w r1, [r10, r0]"),
            (40, 2, "ldr r1, [sp, #0x10]"),
            (42, 4, "movw r0, #0xffff"),
            (46, 4, "bic.w r0, r1, r0"),
            (50, 2, "ldr r1, [sp, #0x14]"),
            (52, 2, "add r0, r1"),
            (54, 4, "movw r1, #0x38fc"),
            (58, 4, "str.w r0, [r10, r1]"),
        ],
        "objects": [],
    },
    "publication": {
        "extent": 26,
        "instructions": [
            (0, 4, "ldr.w r3, [pc, @object0.pc_slot]"),
            (4, 4, "movw r2, #0x38f8"),
            (8, 2, "strb r1, [r3, r2]"),
            (10, 4, "movw r1, #0x38f4"),
            (14, 2, "str r0, [r3, r1]"),
            (16, 2, "add sp, #0x160"),
            (18, 4, "vpop {d8, d9, d10, d11, d12, d13, d14, d15}"),
            (22, 4, "b.w @restore"),
        ],
        "objects": [
            {"kind": "entry_static", "role": "_RNvNtCs_5pico211view_config13CONFIGURATION"}
        ],
    },
    "decoded": {
        "extent": 78,
        "instructions": [
            (0, 2, "ldr r0, [sp, #0x74]"),
            (2, 2, "cmp r0, #0x0"),
            (4, 4, "bne.w @payload"),
            (8, 2, "add r0, sp, #0x70"),
            (10, 4, "bl @role:_RNvNtCs_16dashboard_config8validate12validate_ref"),
            (14, 4, "ldrh.w r1, [sp, #0x70]"),
            (18, 4, "movw r0, #0xffff"),
            (22, 2, "cmp r1, #0x2"),
            (24, 2, "bne @self+20"),
            (26, 2, "ldr r1, [sp, #0x74]"),
            (28, 2, "str r1, [sp, #0xc]"),
            (30, 2, "b @self+2c"),
            (32, 4, "ldrd r2, r1, [sp, #112]"),
            (36, 2, "str r1, [sp, #0xc]"),
            (38, 2, "uxth r1, r2"),
            (40, 2, "cmp r1, r0"),
            (42, 2, "bne @self+42"),
            (44, 4, "movw r1, #0x38fc"),
            (48, 4, "strh.w r0, [r8, r1]"),
            (52, 2, "ldr r1, [sp, #0xc]"),
            (54, 4, "movw r0, #0x38f0"),
            (58, 4, "str.w r1, [r8, r0]"),
            (62, 4, "b.w @issuer"),
            (66, 2, "ldr r0, [sp, #0x78]"),
            (68, 2, "mov r5, r2"),
            (70, 2, "mov r4, r1"),
            (72, 2, "str r0, [sp, #0x8]"),
            (74, 4, "b.w @refill"),
        ],
        "objects": [],
    },
    "restore": {
        "extent": 8,
        "instructions": [
            (0, 2, "add sp, #0x4"),
            (2, 4, "pop.w {r8, r9, r10, r11}"),
            (6, 2, "pop {r4, r5, r6, r7, pc}"),
        ],
        "objects": [],
    },
}
