"""Complete finite core bridges for pinned rustc080cc2714/LLVM23.1.3.

Only typed readonly MOVW/MOVT pairs and exact direct target roles relocate.
This data does not establish caller options, payload or sink closure.

Maintenance: changed bodies fail closed. Review the complete mapped bytes,
source/ABI, every transfer and current frame before adding a finite variant;
retain exact register/condition/stack operands and typed object relationships.
Validate normal/-O mutations, independent code/data relocation and real varied
configuration links. Do not auto-learn bodies, normalize arbitrary immediates,
or weaken fixed budgets. Linked software evidence is not device high-water
proof. Initial full-byte review: task-2-format-bridges-report.md (ignored).
"""

FORMAT_GRAMMARS = {
    "_RNvNtCs_4core3fmt5write": (
        262,
        56,
        """
000 push	{r4, r5, r6, r7, lr}
002 add	r7, sp, #0xc
004 push.w	{r8, r9, r10, r11}
008 sub	sp, #0x14
00a mov	r6, r0
00c mov	r11, r3
00e mov	r10, r1
010 lsls	r0, r3, #0x1f
012 bne	@self+de
014 ldrb	r5, [r2]
016 movs	r0, #0x0
018 cmp	r5, #0x0
01a beq	@self+fe
01c ldr.w	r9, [r10, #0xc]
020 mov.w	r8, #0x0
024 b	@self+2c
026 ldrb	r5, [r2]
028 cmp	r5, #0x0
02a beq	@self+fc
02c adds	r4, r2, #0x1
02e sxtb	r0, r5
030 cmp.w	r0, #0xffffffff
034 ble	@self+46
036 mov	r0, r6
038 mov	r1, r4
03a mov	r2, r5
03c blx	r9
03e cmp	r0, #0x0
040 bne	@self+f8
042 adds	r2, r4, r5
044 b	@self+26
046 cmp	r5, #0x80
048 beq	@self+5e
04a cmp	r5, #0xc0
04c bne	@self+74
04e ldr.w	r0, [r11, r8, lsl #3]
052 movs	r1, #0x0
054 str	r1, [sp, #0x10]
056 movs	r1, #0x20
058 movt	r1, #0x6000
05c b	@self+c2
05e ldrh.w	r4, [r2, #0x1]
062 adds	r5, r2, #0x3
064 mov	r0, r6
066 mov	r1, r5
068 mov	r2, r4
06a blx	r9
06c cmp	r0, #0x0
06e bne	@self+f8
070 adds	r2, r5, r4
072 b	@self+26
074 lsls	r0, r5, #0x1f
076 bne	@self+80
078 movs	r1, #0x20
07a movt	r1, #0x6000
07e b	@self+86
080 ldr.w	r1, [r2, #0x1]
084 adds	r4, r2, #0x5
086 lsls	r0, r5, #0x1e
088 ite	mi
08a ldrhmi	r2, [r4], #2
08e movpl	r2, #0x0
090 lsls	r0, r5, #0x1d
092 ite	mi
094 ldrhmi	r3, [r4], #2
098 movpl	r3, #0x0
09a lsls	r0, r5, #0x1c
09c it	mi
09e ldrhmi	r8, [r4], #2
0a2 lsls	r0, r5, #0x1b
0a4 itt	mi
0a6 addmi.w	r0, r11, r2, lsl #3
0aa ldrhmi	r2, [r0, #0x4]
0ac lsls	r0, r5, #0x1a
0ae itt	mi
0b0 addmi.w	r0, r11, r3, lsl #3
0b4 ldrhmi	r3, [r0, #0x4]
0b6 ldr.w	r0, [r11, r8, lsl #3]
0ba strh.w	r3, [sp, #0x12]
0be strh.w	r2, [sp, #0x10]
0c2 str	r1, [sp, #0xc]
0c4 add.w	r1, r11, r8, lsl #3
0c8 str.w	r10, [sp, #0x8]
0cc ldr	r2, [r1, #0x4]
0ce add	r1, sp, #0x4
0d0 str	r6, [sp, #0x4]
0d2 blx	r2
0d4 cbnz	r0, @self+f8
0d6 add.w	r8, r8, #0x1
0da mov	r2, r4
0dc b	@self+26
0de lsr.w	r3, r11, #0x1
0e2 mov	r1, r2
0e4 ldr.w	r12, [r10, #0xc]
0e8 mov	r0, r6
0ea mov	r2, r3
0ec add	sp, #0x14
0ee pop.w	{r8, r9, r10, r11}
0f2 pop.w	{r4, r5, r6, r7, lr}
0f6 bx	r12
0f8 movs	r0, #0x1
0fa b	@self+fe
0fc movs	r0, #0x0
0fe add	sp, #0x14
100 pop.w	{r8, r9, r10, r11}
104 pop	{r4, r5, r6, r7, pc}
""".strip().splitlines(),
    ),
    "_RNvMs2_NtNtCs_4core3fmt8buildersNtB5_11DebugStruct5field": (
        282,
        72,
        """
000 push	{r4, r5, r6, r7, lr}
002 add	r7, sp, #0xc
004 push.w	{r8, r9, r10, r11}
008 sub	sp, #0x24
00a mov	r8, r3
00c mov	r3, r0
00e ldrb	r0, [r0, #0x4]
010 mov.w	r9, #0x1
014 cmp	r0, #0x0
016 mov.w	r0, #0x1
01a bne	@self+f6
01c ldr	r5, [r3]
01e mov	r10, r3
020 ldr.w	r11, [r7, #0x8]
024 ldrb	r3, [r3, #0x5]
026 ldrb	r0, [r5, #0xa]
028 lsls	r0, r0, #0x18
02a bmi	@self+88
02c mov	r4, r1
02e movw	r1, @addr0.low
032 mov	r11, r2
034 movw	r2, @addr1.low
038 movt	r1, @addr0.high
03c ldrd	r0, r6, [r5]
040 movt	r2, @addr1.high
044 cmp	r3, #0x0
046 it	ne
048 movne	r1, r2
04a ldr	r3, [r6, #0xc]
04c mov.w	r2, #0x3
050 it	ne
052 movne	r2, #0x2
054 blx	r3
056 cmp	r0, #0x0
058 bne	@self+f2
05a ldrd	r0, r1, [r5]
05e mov	r2, r11
060 ldr	r3, [r1, #0xc]
062 mov	r1, r4
064 blx	r3
066 cmp	r0, #0x0
068 bne	@self+f2
06a ldrd	r0, r1, [r5]
06e movs	r2, #0x2
070 ldr	r3, [r1, #0xc]
072 movw	r1, @addr2.low
076 movt	r1, @addr2.high
07a blx	r3
07c cbnz	r0, @self+f2
07e ldr	r2, [r7, #0x8]
080 mov	r0, r8
082 mov	r1, r5
084 blx	r2
086 b	@self+f4
088 cbnz	r3, @self+a8
08a ldrd	r0, r3, [r5]
08e movw	r6, @addr3.low
092 ldr	r3, [r3, #0xc]
094 movt	r6, @addr3.high
098 mov	r4, r1
09a mov	r1, r6
09c mov	r6, r2
09e movs	r2, #0x3
0a0 blx	r3
0a2 mov	r1, r4
0a4 mov	r2, r6
0a6 cbnz	r0, @self+f2
0a8 movs	r0, #0x1
0aa ldrd	r3, r6, [r5]
0ae strb	r0, [r7, #-45]
0b2 sub.w	r0, r7, #0x2d
0b6 str	r0, [sp, #0xc]
0b8 movw	r0, @addr4.low
0bc movt	r0, @addr4.high
0c0 ldrd	r4, r5, [r5, #8]
0c4 str	r0, [sp, #0x18]
0c6 add	r0, sp, #0x4
0c8 str	r0, [sp, #0x14]
0ca strd	r4, r5, [sp, #28]
0ce strd	r3, r6, [sp, #4]
0d2 bl	@role:_RNvXs0_NtNtCs_4core3fmt8buildersNtB5_10PadAdapterNtB7_5Write9write_str
0d6 cbnz	r0, @self+f2
0d8 movw	r1, @addr5.low
0dc add	r0, sp, #0x4
0de movt	r1, @addr5.high
0e2 movs	r2, #0x2
0e4 bl	@role:_RNvXs0_NtNtCs_4core3fmt8buildersNtB5_10PadAdapterNtB7_5Write9write_str
0e8 cbnz	r0, @self+f2
0ea add	r1, sp, #0x14
0ec mov	r0, r8
0ee blx	r11
0f0 cbz	r0, @self+106
0f2 movs	r0, #0x1
0f4 mov	r3, r10
0f6 strb	r0, [r3, #0x4]
0f8 mov	r0, r3
0fa strb.w	r9, [r3, #0x5]
0fe add	sp, #0x24
100 pop.w	{r8, r9, r10, r11}
104 pop	{r4, r5, r6, r7, pc}
106 ldrd	r0, r1, [sp, #20]
10a movs	r2, #0x2
10c ldr	r3, [r1, #0xc]
10e movw	r1, @addr6.low
112 movt	r1, @addr6.high
116 blx	r3
118 b	@self+f4
""".strip().splitlines(),
    ),
    "_RNvMsa_NtCs_4core3fmtNtB5_9Formatter26debug_struct_field5_finish": (
        198,
        40,
        """
000 push	{r4, r5, r6, r7, lr}
002 add	r7, sp, #0xc
004 str	r11, [sp, #-4]!
008 sub	sp, #0x10
00a mov	r5, r2
00c mov	r6, r0
00e ldrd	r0, r2, [r0]
012 mov	r4, r3
014 ldr	r3, [r2, #0xc]
016 movs	r2, #0xf
018 blx	r3
01a ldr	r1, [r7, #0x8]
01c movs	r2, #0x0
01e strb.w	r0, [sp, #0xc]
022 add	r0, sp, #0x8
024 strb.w	r2, [sp, #0xd]
028 movs	r2, #0x4
02a ldr	r1, [r1, #0xc]
02c mov	r3, r4
02e str	r1, [sp]
030 mov	r1, r5
032 str	r6, [sp, #0x8]
034 bl	@role:_RNvMs2_NtNtCs_4core3fmt8buildersNtB5_11DebugStruct5field
038 ldr	r2, [r7, #0x14]
03a ldr	r1, [r7, #0xc]
03c ldr	r3, [r7, #0x10]
03e ldr	r2, [r2, #0xc]
040 str	r2, [sp]
042 movs	r2, #0x5
044 bl	@role:_RNvMs2_NtNtCs_4core3fmt8buildersNtB5_11DebugStruct5field
048 ldr	r2, [r7, #0x20]
04a ldr	r1, [r7, #0x18]
04c ldr	r3, [r7, #0x1c]
04e ldr	r2, [r2, #0xc]
050 str	r2, [sp]
052 movs	r2, #0x4
054 bl	@role:_RNvMs2_NtNtCs_4core3fmt8buildersNtB5_11DebugStruct5field
058 ldr	r2, [r7, #0x2c]
05a ldr	r1, [r7, #0x24]
05c ldr	r3, [r7, #0x28]
05e ldr	r2, [r2, #0xc]
060 str	r2, [sp]
062 movs	r2, #0x6
064 bl	@role:_RNvMs2_NtNtCs_4core3fmt8buildersNtB5_11DebugStruct5field
068 ldr	r2, [r7, #0x38]
06a ldr	r1, [r7, #0x30]
06c ldr	r3, [r7, #0x34]
06e ldr	r2, [r2, #0xc]
070 str	r2, [sp]
072 movs	r2, #0x7
074 bl	@role:_RNvMs2_NtNtCs_4core3fmt8buildersNtB5_11DebugStruct5field
078 ldrb.w	r2, [sp, #0xd]
07c ldrb.w	r1, [sp, #0xc]
080 cmp	r2, #0x1
082 orr.w	r0, r2, r1
086 it	eq
088 lslseq.w	r1, r1, #0x1f
08c bne	@self+ba
08e ldr	r0, [sp, #0x8]
090 ldrb	r1, [r0, #0xa]
092 lsls	r1, r1, #0x18
094 bmi	@self+a8
096 ldrd	r0, r1, [r0]
09a movs	r2, #0x2
09c ldr	r3, [r1, #0xc]
09e movw	r1, @addr0.low
0a2 movt	r1, @addr0.high
0a6 b	@self+b8
0a8 ldrd	r0, r1, [r0]
0ac movs	r2, #0x1
0ae ldr	r3, [r1, #0xc]
0b0 movw	r1, @addr1.low
0b4 movt	r1, @addr1.high
0b8 blx	r3
0ba and	r0, r0, #0x1
0be add	sp, #0x10
0c0 ldr	r11, [sp], #4
0c4 pop	{r4, r5, r6, r7, pc}
""".strip().splitlines(),
    ),
    "_RNvMsa_NtCs_4core3fmtNtB5_9Formatter25debug_tuple_field1_finish": (
        206,
        64,
        """
000 push	{r4, r5, r6, r7, lr}
002 add	r7, sp, #0xc
004 push.w	{r8, r9, r10}
008 sub	sp, #0x20
00a ldrd	r4, r5, [r0]
00e mov	r6, r0
010 ldr.w	r10, [r5, #0xc]
014 mov	r8, r3
016 mov	r0, r4
018 blx	r10
01a cbz	r0, @self+26
01c movs	r0, #0x1
01e add	sp, #0x20
020 pop.w	{r8, r9, r10}
024 pop	{r4, r5, r6, r7, pc}
026 ldrb	r0, [r6, #0xa]
028 ldr.w	r9, [r7, #0x8]
02c lsls	r0, r0, #0x18
02e bmi	@self+60
030 movw	r1, @addr0.low
034 mov	r0, r4
036 movt	r1, @addr0.high
03a movs	r2, #0x1
03c movs	r5, #0x1
03e blx	r10
040 mov	r1, r0
042 mov	r0, r5
044 cmp	r1, #0x0
046 bne	@self+1e
048 ldr.w	r2, [r9, #0xc]
04c mov	r0, r8
04e mov	r1, r6
050 blx	r2
052 cmp	r0, #0x0
054 bne	@self+1c
056 ldrd	r4, r0, [r6]
05a ldr.w	r10, [r0, #0xc]
05e b	@self+be
060 movw	r1, @addr1.low
064 mov	r0, r4
066 movt	r1, @addr1.high
06a movs	r2, #0x2
06c blx	r10
06e mov	r1, r0
070 movs	r0, #0x1
072 cmp	r1, #0x0
074 bne	@self+1e
076 sub.w	r1, r7, #0x29
07a movw	r2, @addr2.low
07e movt	r2, @addr2.high
082 strb	r0, [r7, #-41]
086 strd	r5, r1, [sp, #4]
08a mov	r3, sp
08c ldrd	r0, r1, [r6, #8]
090 strd	r0, r1, [sp, #24]
094 add	r1, sp, #0x10
096 str	r2, [sp, #0x14]
098 mov	r0, r8
09a ldr.w	r2, [r9, #0xc]
09e str	r4, [sp]
0a0 str	r3, [sp, #0x10]
0a2 blx	r2
0a4 cmp	r0, #0x0
0a6 bne	@self+1c
0a8 ldrd	r0, r1, [sp, #16]
0ac movs	r2, #0x2
0ae ldr	r3, [r1, #0xc]
0b0 movw	r1, @addr3.low
0b4 movt	r1, @addr3.high
0b8 blx	r3
0ba cmp	r0, #0x0
0bc bne	@self+1c
0be movw	r1, @addr4.low
0c2 mov	r0, r4
0c4 movt	r1, @addr4.high
0c8 movs	r2, #0x1
0ca blx	r10
0cc b	@self+1e
""".strip().splitlines(),
    ),
}
