"""Finite pinned numeric bodies; exact operands carry range/buffer proofs.

Unsigned scalar widths, reciprocal constants, bounded remainder/table accesses,
3/5/10-byte owned buffers and every transfer are exact. Changed bodies reject.
Only typed data and direct targets relocate; defaults require separately bound
flags 0x60000020, width 0 and precision 0 at each entry. No custom/hex admission.
"""

# Role, byte extent, stack frame, code/data spans, complete literal grammar.
NumericVariant = tuple[str, int, int, tuple[tuple[int, int, str], ...], list[str]]

NUMERIC_VARIANTS: dict[str, tuple[NumericVariant, ...]] = {
    "_RNvXsZ_NtNtCs_4core3fmt3numjNtB7_5Debug3fmt": (
        (
            "<usize as core::fmt::Debug>::fmt",
            278,
            40,
            ((0, 278, "t"),),
            """
000 push	{r4, r5, r6, r7, lr}
002 add	r7, sp, #0xc
004 str	r11, [sp, #-4]!
008 sub	sp, #0x10
00a ldr	r2, [r1, #0x8]
00c lsls	r3, r2, #0x6
00e bmi	@self+22
010 lsls	r2, r2, #0x5
012 bmi	@self+82
014 add	sp, #0x10
016 ldr	r11, [sp], #4
01a pop.w	{r4, r5, r6, r7, lr}
01e b.w	@role:_RNvXs8_NtNtNtCs_4core3fmt3num3impmNtB9_7Display3fmt
022 ldr	r3, [r0]
024 movw	lr, @lower_hex_digits.low
028 movs	r0, #0x8
02a movs	r2, #0x9
02c add.w	r12, sp, #0x8
030 movt	lr, @lower_hex_digits.high
034 and	r4, r3, #0xf
038 ldrb.w	r6, [lr, r4]
03c lsrs	r5, r3, #0x4
03e add.w	r4, r12, r2
042 strb	r6, [r4, #-2]
046 beq	@self+e2
048 and	r5, r5, #0xf
04c ldrb.w	r6, [lr, r5]
050 lsrs	r5, r3, #0x8
052 strb	r6, [r4, #-3]
056 beq	@self+e8
058 and	r5, r5, #0xf
05c ldrb.w	r6, [lr, r5]
060 lsrs	r5, r3, #0xc
062 strb	r6, [r4, #-4]
066 beq	@self+ee
068 and	r5, r5, #0xf
06c ldrb.w	r5, [lr, r5]
070 lsrs	r3, r3, #0x10
072 sub.w	r0, r0, #0x4
076 sub.w	r2, r2, #0x4
07a strb	r5, [r4, #-5]
07e bne	@self+34
080 b	@self+f2
082 ldr	r3, [r0]
084 movw	lr, @upper_hex_digits.low
088 movs	r0, #0x8
08a movs	r2, #0x9
08c add.w	r12, sp, #0x8
090 movt	lr, @upper_hex_digits.high
094 and	r4, r3, #0xf
098 ldrb.w	r6, [lr, r4]
09c lsrs	r5, r3, #0x4
09e add.w	r4, r12, r2
0a2 strb	r6, [r4, #-2]
0a6 beq	@self+e2
0a8 and	r5, r5, #0xf
0ac ldrb.w	r6, [lr, r5]
0b0 lsrs	r5, r3, #0x8
0b2 strb	r6, [r4, #-3]
0b6 beq	@self+e8
0b8 and	r5, r5, #0xf
0bc ldrb.w	r6, [lr, r5]
0c0 lsrs	r5, r3, #0xc
0c2 strb	r6, [r4, #-4]
0c6 beq	@self+ee
0c8 and	r5, r5, #0xf
0cc ldrb.w	r5, [lr, r5]
0d0 lsrs	r3, r3, #0x10
0d2 sub.w	r0, r0, #0x4
0d6 sub.w	r2, r2, #0x4
0da strb	r5, [r4, #-5]
0de bne	@self+94
0e0 b	@self+f2
0e2 subs	r0, r2, #0x2
0e4 subs	r2, #0x1
0e6 b	@self+f2
0e8 subs	r0, #0x2
0ea subs	r2, #0x2
0ec b	@self+f2
0ee subs	r0, #0x3
0f0 subs	r2, #0x3
0f2 rsb.w	r6, r2, #0x9
0f6 movw	r2, @hex_prefix.low
0fa movt	r2, @hex_prefix.high
0fe add.w	r3, r12, r0
102 mov	r0, r1
104 mov	r1, r2
106 movs	r2, #0x2
108 str	r6, [sp]
10a bl	@role:_RNvMsa_NtCs_4core3fmtNtB5_9Formatter12pad_integral
10e add	sp, #0x10
110 ldr	r11, [sp], #4
114 pop	{r4, r5, r6, r7, pc}
""".strip().splitlines(),
        ),
        (
            "<usize as core::fmt::Debug>::fmt",
            38,
            8,
            ((0, 38, "t"),),
            """
000 push	{r7, lr}
002 mov	r7, sp
004 ldr	r2, [r1, #0x8]
006 lsls	r3, r2, #0x6
008 bmi	@self+16
00a lsls	r2, r2, #0x5
00c bmi	@self+1e
00e pop.w	{r7, lr}
012 b.w	@role:_RNvXs8_NtNtNtCs_4core3fmt3num3impmNtB9_7Display3fmt
016 pop.w	{r7, lr}
01a b.w	@role:_RNvXs6_NtNtCs_4core3fmt3numjNtB7_8LowerHex3fmt
01e pop.w	{r7, lr}
022 b.w	@role:_RNvXs8_NtNtCs_4core3fmt3numjNtB7_8UpperHex3fmt
""".strip().splitlines(),
        ),
    ),
    "_RNvMsa_NtCs_4core3fmtNtB5_9Formatter3pad": (
        (
            "<core::fmt::Formatter>::pad",
            452,
            40,
            ((0, 266, "t"), (266, 270, "d"), (270, 452, "t")),
            """
000 push	{r4, r5, r6, r7, lr}
002 add	r7, sp, #0xc
004 push.w	{r8, r9, r10, r11}
008 sub	sp, #0x4
00a ldr.w	r10, [r0, #0x8]
00e mov	r11, r2
010 mov	r8, r1
012 tst.w	r10, #0x18000000
016 beq.w	@self+112
01a lsls.w	r1, r10, #0x3
01e bmi	@self+9c
020 cmp.w	r11, #0x10
024 bhs	@self+dc
026 cmp.w	r11, #0x0
02a beq.w	@self+186
02e and	r12, r11, #0x3
032 lsrs.w	r1, r11, #0x2
036 mov.w	r3, #0x0
03a beq.w	@self+18e
03e and	r1, r1, #0x3
042 add.w	lr, r8, #0x1
046 mvn	r5, #0x3
04a sub.w	r9, r3, r1, lsl #2
04e movs	r1, #0x0
050 mov	r4, r5
052 add	r5, lr
054 ldrsb.w	r6, [r5, #0x3]
058 ldrsb.w	r2, [r5, #0x6]
05c ldrsb.w	r3, [r5, #0x5]
060 cmn.w	r6, #0x41
064 ldrsb.w	r5, [r5, #0x4]
068 it	gt
06a addgt	r1, #0x1
06c cmn.w	r5, #0x41
070 it	gt
072 addgt	r1, #0x1
074 cmn.w	r3, #0x41
078 add.w	r5, r4, #0x4
07c it	gt
07e addgt	r1, #0x1
080 cmn.w	r2, #0x41
084 add.w	r2, r9, r5
088 it	gt
08a addgt	r1, #0x1
08c adds	r2, #0x4
08e bne	@self+50
090 cmp.w	r12, #0x0
094 beq	@self+f4
096 add.w	r3, r4, #0x8
09a b	@self+190
09c ldrh	r1, [r0, #0xe]
09e cbz	r1, @self+ec
0a0 add.w	r3, r8, r11
0a4 mov.w	r11, #0x0
0a8 mov	r5, r8
0aa mov	r2, r1
0ac b	@self+c2
0ae cmp	r5, #0xef
0b0 mov.w	r5, #0x3
0b4 it	hi
0b6 movhi	r5, #0x4
0b8 add	r5, r6
0ba subs	r6, r5, r6
0bc subs	r2, #0x1
0be add	r11, r6
0c0 beq	@self+f0
0c2 cmp	r5, r3
0c4 beq	@self+f2
0c6 mov	r6, r5
0c8 ldrsb	r4, [r5], #1
0cc cmp.w	r4, #0xffffffff
0d0 bgt	@self+ba
0d2 uxtb	r5, r4
0d4 cmp	r5, #0xe0
0d6 bhs	@self+ae
0d8 adds	r5, r6, #0x2
0da b	@self+ba
0dc mov	r4, r0
0de mov	r0, r8
0e0 mov	r1, r11
0e2 bl	@role:_RNvNtNtCs_4core3str5count14do_count_chars
0e6 mov	r1, r0
0e8 mov	r0, r4
0ea b	@self+f4
0ec mov.w	r11, #0x0
0f0 movs	r2, #0x0
0f2 subs	r1, r1, r2
0f4 ldrh	r2, [r0, #0xc]
0f6 cmp	r1, r2
0f8 bhs	@self+112
0fa subs	r3, r2, r1
0fc ubfx	r1, r10, #0x1d, #0x2
100 bfc	r10, #21, #11
104 movs	r6, #0x0
106 tbb	[pc, r1]
10e mov	r6, r3
110 b	@self+12c
112 ldrd	r0, r1, [r0]
116 mov	r2, r11
118 ldr	r3, [r1, #0xc]
11a mov	r1, r8
11c add	sp, #0x4
11e pop.w	{r8, r9, r10, r11}
122 pop.w	{r4, r5, r6, r7, lr}
126 bx	r3
128 uxth	r1, r3
12a lsrs	r6, r1, #0x1
12c ldrd	r5, r4, [r0]
130 mov.w	r9, #0x0
134 str	r3, [sp]
136 uxth	r0, r6
138 uxth.w	r1, r9
13c cmp	r1, r0
13e bhs	@self+152
140 ldr	r2, [r4, #0x10]
142 mov	r0, r5
144 mov	r1, r10
146 blx	r2
148 add.w	r9, r9, #0x1
14c cmp	r0, #0x0
14e beq	@self+136
150 b	@self+17c
152 ldr	r3, [r4, #0xc]
154 mov	r0, r5
156 mov	r1, r8
158 mov	r2, r11
15a blx	r3
15c cbnz	r0, @self+17c
15e ldr	r0, [sp]
160 subs	r0, r0, r6
162 movs	r6, #0x0
164 uxth.w	r8, r0
168 uxth	r0, r6
16a cmp	r0, r8
16c bhs	@self+18a
16e ldr	r2, [r4, #0x10]
170 mov	r0, r5
172 mov	r1, r10
174 blx	r2
176 adds	r6, #0x1
178 cmp	r0, #0x0
17a beq	@self+168
17c movs	r0, #0x1
17e add	sp, #0x4
180 pop.w	{r8, r9, r10, r11}
184 pop	{r4, r5, r6, r7, pc}
186 movs	r1, #0x0
188 b	@self+f4
18a movs	r0, #0x0
18c b	@self+17e
18e movs	r1, #0x0
190 ldrsb.w	r2, [r8, r3]
194 cmn.w	r2, #0x41
198 it	gt
19a addgt	r1, #0x1
19c cmp.w	r12, #0x1
1a0 beq	@self+f4
1a2 add	r3, r8
1a4 ldrsb.w	r2, [r3, #0x1]
1a8 cmn.w	r2, #0x41
1ac it	gt
1ae addgt	r1, #0x1
1b0 cmp.w	r12, #0x2
1b4 beq	@self+f4
1b6 ldrsb.w	r2, [r3, #0x2]
1ba cmn.w	r2, #0x41
1be it	gt
1c0 addgt	r1, #0x1
1c2 b	@self+f4
10a DATA 11020f11
""".strip().splitlines(),
        ),
        (
            "<core::fmt::Formatter>::pad",
            320,
            40,
            ((0, 182, "t"), (182, 186, "d"), (186, 320, "t")),
            """
000 push	{r4, r5, r6, r7, lr}
002 add	r7, sp, #0xc
004 push.w	{r8, r9, r10, r11}
008 sub	sp, #0x4
00a ldr.w	r10, [r0, #0x8]
00e mov	r6, r2
010 mov	r8, r1
012 tst.w	r10, #0x18000000
016 beq	@self+be
018 lsls.w	r1, r10, #0x3
01c bmi	@self+42
01e cmp	r6, #0x10
020 bhs	@self+86
022 cmp	r6, #0x0
024 beq.w	@self+138
028 sub.w	r2, r8, #0x1
02c movs	r1, #0x0
02e mov	r3, r6
030 ldrsb	r5, [r2, #1]!
034 cmn.w	r5, #0x41
038 it	gt
03a addgt	r1, #0x1
03c subs	r3, #0x1
03e bne	@self+30
040 b	@self+9e
042 ldrh.w	r12, [r0, #0xe]
046 cmp.w	r12, #0x0
04a beq	@self+96
04c add.w	r3, r8, r6
050 movs	r6, #0x0
052 mov	r4, r8
054 mov	r2, r12
056 b	@self+6c
058 cmp	r1, #0xef
05a mov.w	r1, #0x3
05e it	hi
060 movhi	r1, #0x4
062 adds	r4, r5, r1
064 subs	r1, r4, r5
066 subs	r2, #0x1
068 add	r6, r1
06a beq	@self+98
06c cmp	r4, r3
06e beq	@self+9a
070 mov	r5, r4
072 ldrsb	r1, [r4], #1
076 cmp.w	r1, #0xffffffff
07a bgt	@self+64
07c uxtb	r1, r1
07e cmp	r1, #0xe0
080 bhs	@self+58
082 adds	r4, r5, #0x2
084 b	@self+64
086 mov	r4, r0
088 mov	r0, r8
08a mov	r1, r6
08c bl	@role:_RNvNtNtCs_4core3str5count14do_count_chars
090 mov	r1, r0
092 mov	r0, r4
094 b	@self+9e
096 movs	r6, #0x0
098 movs	r2, #0x0
09a sub.w	r1, r12, r2
09e ldrh	r2, [r0, #0xc]
0a0 cmp	r1, r2
0a2 bhs	@self+be
0a4 subs	r3, r2, r1
0a6 ubfx	r1, r10, #0x1d, #0x2
0aa bfc	r10, #21, #11
0ae mov.w	r11, #0x0
0b2 tbb	[pc, r1]
0ba mov	r11, r3
0bc b	@self+da
0be ldrd	r0, r1, [r0]
0c2 mov	r2, r6
0c4 ldr	r3, [r1, #0xc]
0c6 mov	r1, r8
0c8 add	sp, #0x4
0ca pop.w	{r8, r9, r10, r11}
0ce pop.w	{r4, r5, r6, r7, lr}
0d2 bx	r3
0d4 uxth	r1, r3
0d6 lsr.w	r11, r1, #0x1
0da ldrd	r5, r4, [r0]
0de mov.w	r9, #0x0
0e2 str	r3, [sp]
0e4 uxth.w	r0, r11
0e8 uxth.w	r1, r9
0ec cmp	r1, r0
0ee bhs	@self+102
0f0 ldr	r2, [r4, #0x10]
0f2 mov	r0, r5
0f4 mov	r1, r10
0f6 blx	r2
0f8 add.w	r9, r9, #0x1
0fc cmp	r0, #0x0
0fe beq	@self+e4
100 b	@self+12e
102 ldr	r3, [r4, #0xc]
104 mov	r0, r5
106 mov	r1, r8
108 mov	r2, r6
10a blx	r3
10c cbnz	r0, @self+12e
10e ldr	r0, [sp]
110 movs	r6, #0x0
112 sub.w	r0, r0, r11
116 uxth.w	r8, r0
11a uxth	r0, r6
11c cmp	r0, r8
11e bhs	@self+13c
120 ldr	r2, [r4, #0x10]
122 mov	r0, r5
124 mov	r1, r10
126 blx	r2
128 adds	r6, #0x1
12a cmp	r0, #0x0
12c beq	@self+11a
12e movs	r0, #0x1
130 add	sp, #0x4
132 pop.w	{r8, r9, r10, r11}
136 pop	{r4, r5, r6, r7, pc}
138 movs	r1, #0x0
13a b	@self+9e
13c movs	r0, #0x0
13e b	@self+130
0b6 DATA 12020f12
""".strip().splitlines(),
        ),
    ),
    "_RNvMsa_NtCs_4core3fmtNtB5_9Formatter12pad_integral": (
        (
            "<core::fmt::Formatter>::pad_integral",
            412,
            64,
            ((0, 122, "t"), (122, 126, "d"), (126, 412, "t")),
            """
000 push	{r4, r5, r6, r7, lr}
002 add	r7, sp, #0xc
004 push.w	{r8, r9, r10, r11}
008 sub	sp, #0x1c
00a ldr	r6, [r0, #0x8]
00c mov	r4, r1
00e ldr.w	r10, [r7, #0x8]
012 mov	r11, r2
014 ands	r1, r6, #0x200000
018 mov.w	r9, #0x2b
01c it	eq
01e moveq.w	r9, #0xffffffff
022 add.w	r5, r10, r1, lsr #21
026 lsls	r1, r6, #0x8
028 bmi	@self+2e
02a movs	r4, #0x0
02c b	@self+56
02e movs	r1, #0x0
030 cmp.w	r11, #0x0
034 beq	@self+54
036 ldrsb.w	r2, [r4]
03a cmn.w	r2, #0x41
03e it	gt
040 movgt	r1, #0x1
042 cmp.w	r11, #0x1
046 beq	@self+54
048 ldrsb.w	r2, [r4, #0x1]
04c cmn.w	r2, #0x41
050 it	gt
052 addgt	r1, #0x1
054 add	r5, r1
056 ldrh.w	r8, [r0, #0xc]
05a cmp	r5, r8
05c bhs	@self+82
05e lsls	r1, r6, #0x7
060 strd	r10, r3, [sp, #20]
064 bmi	@self+b0
066 ubfx	r1, r6, #0x1d, #0x2
06a str	r4, [sp, #0x10]
06c sub.w	r12, r8, r5
070 bfc	r6, #21, #11
074 movs	r5, #0x0
076 tbb	[pc, r1]
07e mov	r5, r12
080 b	@self+122
082 mov	r5, r3
084 mov	r3, r4
086 ldrd	r4, r6, [r0]
08a mov	r2, r9
08c mov	r1, r6
08e str.w	r11, [sp]
092 mov	r0, r4
094 bl	@role:_RNvNvMsa_NtCs_4core3fmtNtB7_9Formatter12pad_integral12write_prefix
098 cmp	r0, #0x0
09a bne	@self+16c
09c ldr	r3, [r6, #0xc]
09e mov	r0, r4
0a0 mov	r1, r5
0a2 mov	r2, r10
0a4 add	sp, #0x1c
0a6 pop.w	{r8, r9, r10, r11}
0aa pop.w	{r4, r5, r6, r7, lr}
0ae bx	r3
0b0 ldrd	r2, r1, [r0, #8]
0b4 mov	r3, r4
0b6 ldrd	r10, r6, [r0]
0ba str	r1, [sp, #0x8]
0bc movs	r1, #0x0
0be movt	r1, #0x9fe0
0c2 str	r2, [sp, #0xc]
0c4 ands	r1, r2
0c6 str	r0, [sp, #0x10]
0c8 orr	r1, r1, #0x20000000
0cc mov	r2, r9
0ce orr	r1, r1, #0x30
0d2 str	r1, [r0, #0x8]
0d4 mov	r0, r10
0d6 mov	r1, r6
0d8 str.w	r11, [sp]
0dc bl	@role:_RNvNvMsa_NtCs_4core3fmtNtB7_9Formatter12pad_integral12write_prefix
0e0 cmp	r0, #0x0
0e2 bne	@self+16c
0e4 sub.w	r0, r8, r5
0e8 movs	r5, #0x0
0ea uxth	r4, r0
0ec uxth	r0, r5
0ee cmp	r0, r4
0f0 bhs	@self+102
0f2 ldr	r2, [r6, #0x10]
0f4 mov	r0, r10
0f6 movs	r1, #0x30
0f8 blx	r2
0fa adds	r5, #0x1
0fc cmp	r0, #0x0
0fe beq	@self+ec
100 b	@self+16c
102 ldrd	r2, r1, [sp, #20]
106 mov	r0, r10
108 ldr	r3, [r6, #0xc]
10a blx	r3
10c cbnz	r0, @self+16c
10e ldr	r0, [sp, #0x10]
110 ldrd	r2, r1, [sp, #8]
114 strd	r1, r2, [r0, #8]
118 movs	r0, #0x0
11a b	@self+16e
11c uxth.w	r1, r12
120 lsrs	r5, r1, #0x1
122 ldrd	r4, r8, [r0]
126 mov.w	r10, #0x0
12a str.w	r12, [sp, #0xc]
12e uxth	r0, r5
130 uxth.w	r1, r10
134 cmp	r1, r0
136 bhs	@self+14c
138 ldr.w	r2, [r8, #0x10]
13c mov	r0, r4
13e mov	r1, r6
140 blx	r2
142 add.w	r10, r10, #0x1
146 cmp	r0, #0x0
148 beq	@self+12e
14a b	@self+16c
14c str.w	r11, [sp]
150 mov	r0, r4
152 ldr	r3, [sp, #0x10]
154 mov	r1, r8
156 mov	r2, r9
158 bl	@role:_RNvNvMsa_NtCs_4core3fmtNtB7_9Formatter12pad_integral12write_prefix
15c cbnz	r0, @self+16c
15e ldrd	r2, r1, [sp, #20]
162 mov	r0, r4
164 ldr.w	r3, [r8, #0xc]
168 blx	r3
16a cbz	r0, @self+176
16c movs	r0, #0x1
16e add	sp, #0x1c
170 pop.w	{r8, r9, r10, r11}
174 pop	{r4, r5, r6, r7, pc}
176 ldr	r0, [sp, #0xc]
178 subs	r0, r0, r5
17a movs	r5, #0x0
17c uxth.w	r9, r0
180 uxth	r0, r5
182 cmp	r0, r9
184 bhs	@self+118
186 ldr.w	r2, [r8, #0x10]
18a mov	r0, r4
18c mov	r1, r6
18e blx	r2
190 adds	r5, #0x1
192 mov	r1, r0
194 movs	r0, #0x1
196 cmp	r1, #0x0
198 beq	@self+180
19a b	@self+16e
07a DATA 54025102
""".strip().splitlines(),
        ),
        (
            "<core::fmt::Formatter>::pad_integral",
            396,
            64,
            ((0, 112, "t"), (112, 116, "d"), (116, 396, "t")),
            """
000 push	{r4, r5, r6, r7, lr}
002 add	r7, sp, #0xc
004 push.w	{r8, r9, r10, r11}
008 sub	sp, #0x1c
00a ldr	r6, [r0, #0x8]
00c mov	r10, r3
00e mov	r3, r2
010 ldr.w	r12, [r7, #0x8]
014 ands	r2, r6, #0x200000
018 mov.w	r9, #0x2b
01c it	eq
01e moveq.w	r9, #0xffffffff
022 str	r3, [sp, #0x14]
024 add.w	r11, r12, r2, lsr #21
028 lsls	r2, r6, #0x8
02a bmi	@self+30
02c movs	r1, #0x0
02e b	@self+4c
030 cbz	r3, @self+48
032 subs	r2, r1, #0x1
034 movs	r5, #0x0
036 ldrsb	r4, [r2, #1]!
03a cmn.w	r4, #0x41
03e it	gt
040 addgt	r5, #0x1
042 subs	r3, #0x1
044 bne	@self+36
046 b	@self+4a
048 movs	r5, #0x0
04a add	r11, r5
04c str	r1, [sp, #0x18]
04e ldrh.w	r8, [r0, #0xc]
052 cmp	r11, r8
054 bhs	@self+78
056 lsls	r1, r6, #0x7
058 strd	r12, r10, [sp, #12]
05c bmi	@self+a6
05e ubfx	r1, r6, #0x1d, #0x2
062 sub.w	r11, r8, r11
066 bfc	r6, #21, #11
06a movs	r5, #0x0
06c tbb	[pc, r1]
074 mov	r5, r11
076 b	@self+116
078 ldrd	r4, r6, [r0]
07c mov	r2, r9
07e ldr	r0, [sp, #0x14]
080 mov	r1, r6
082 str	r0, [sp]
084 mov	r5, r12
086 ldr	r3, [sp, #0x18]
088 mov	r0, r4
08a bl	@role:_RNvNvMsa_NtCs_4core3fmtNtB7_9Formatter12pad_integral12write_prefix
08e cmp	r0, #0x0
090 bne	@self+15c
092 ldr	r3, [r6, #0xc]
094 mov	r0, r4
096 mov	r1, r10
098 mov	r2, r5
09a add	sp, #0x1c
09c pop.w	{r8, r9, r10, r11}
0a0 pop.w	{r4, r5, r6, r7, lr}
0a4 bx	r3
0a6 movs	r1, #0x0
0a8 ldrd	r4, r5, [r0, #8]
0ac movt	r1, #0x9fe0
0b0 str	r0, [sp, #0x8]
0b2 ands	r1, r4
0b4 ldrd	r10, r6, [r0]
0b8 orr	r1, r1, #0x20000000
0bc mov	r2, r9
0be orr	r1, r1, #0x30
0c2 str	r1, [r0, #0x8]
0c4 ldr	r0, [sp, #0x14]
0c6 mov	r1, r6
0c8 str	r0, [sp]
0ca mov	r0, r10
0cc ldr	r3, [sp, #0x18]
0ce bl	@role:_RNvNvMsa_NtCs_4core3fmtNtB7_9Formatter12pad_integral12write_prefix
0d2 cmp	r0, #0x0
0d4 bne	@self+15c
0d6 sub.w	r0, r8, r11
0da str	r5, [sp, #0x18]
0dc mov	r9, r4
0de movs	r5, #0x0
0e0 uxth	r4, r0
0e2 uxth	r0, r5
0e4 cmp	r0, r4
0e6 bhs	@self+f8
0e8 ldr	r2, [r6, #0x10]
0ea mov	r0, r10
0ec movs	r1, #0x30
0ee blx	r2
0f0 adds	r5, #0x1
0f2 cmp	r0, #0x0
0f4 beq	@self+e2
0f6 b	@self+15c
0f8 ldrd	r2, r1, [sp, #12]
0fc mov	r0, r10
0fe ldr	r3, [r6, #0xc]
100 blx	r3
102 cbnz	r0, @self+15c
104 ldr	r0, [sp, #0x8]
106 ldr	r1, [sp, #0x18]
108 strd	r9, r1, [r0, #8]
10c movs	r0, #0x0
10e b	@self+15e
110 uxth.w	r1, r11
114 lsrs	r5, r1, #0x1
116 ldrd	r4, r8, [r0]
11a mov.w	r10, #0x0
11e uxth	r0, r5
120 uxth.w	r1, r10
124 cmp	r1, r0
126 bhs	@self+13c
128 ldr.w	r2, [r8, #0x10]
12c mov	r0, r4
12e mov	r1, r6
130 blx	r2
132 add.w	r10, r10, #0x1
136 cmp	r0, #0x0
138 beq	@self+11e
13a b	@self+15c
13c ldr	r0, [sp, #0x14]
13e mov	r1, r8
140 str	r0, [sp]
142 mov	r0, r4
144 ldr	r3, [sp, #0x18]
146 mov	r2, r9
148 bl	@role:_RNvNvMsa_NtCs_4core3fmtNtB7_9Formatter12pad_integral12write_prefix
14c cbnz	r0, @self+15c
14e ldrd	r2, r1, [sp, #12]
152 mov	r0, r4
154 ldr.w	r3, [r8, #0xc]
158 blx	r3
15a cbz	r0, @self+166
15c movs	r0, #0x1
15e add	sp, #0x1c
160 pop.w	{r8, r9, r10, r11}
164 pop	{r4, r5, r6, r7, pc}
166 sub.w	r0, r11, r5
16a movs	r5, #0x0
16c uxth.w	r9, r0
170 uxth	r0, r5
172 cmp	r0, r9
174 bhs	@self+10c
176 ldr.w	r2, [r8, #0x10]
17a mov	r0, r4
17c mov	r1, r6
17e blx	r2
180 adds	r5, #0x1
182 mov	r1, r0
184 movs	r0, #0x1
186 cmp	r1, #0x0
188 beq	@self+170
18a b	@self+15e
070 DATA 53025002
""".strip().splitlines(),
        ),
    ),
    "_RNvNvMsa_NtCs_4core3fmtNtB7_9Formatter12pad_integral12write_prefix": (
        (
            "<core::fmt::Formatter>::pad_integral::write_prefix",
            68,
            24,
            ((0, 68, "t"),),
            """
000 push	{r4, r5, r6, r7, lr}
002 add	r7, sp, #0xc
004 str	r8, [sp, #-4]!
008 ldr.w	r8, [r7, #0x8]
00c mov	r6, r0
00e mov	r4, r3
010 mov	r5, r1
012 adds	r0, r2, #0x1
014 beq	@self+28
016 ldr	r3, [r5, #0x10]
018 mov	r0, r6
01a mov	r1, r2
01c blx	r3
01e cbz	r0, @self+28
020 movs	r0, #0x1
022 ldr	r8, [sp], #4
026 pop	{r4, r5, r6, r7, pc}
028 cbz	r4, @self+3c
02a ldr	r3, [r5, #0xc]
02c mov	r0, r6
02e mov	r1, r4
030 mov	r2, r8
032 ldr	r8, [sp], #4
036 pop.w	{r4, r5, r6, r7, lr}
03a bx	r3
03c movs	r0, #0x0
03e ldr	r8, [sp], #4
042 pop	{r4, r5, r6, r7, pc}
""".strip().splitlines(),
        ),
    ),
    "_RNvXs8_NtNtNtCs_4core3fmt3num3impmNtB9_7Display3fmt": (
        (
            "<u32 as core::fmt::Display>::fmt",
            230,
            64,
            ((0, 230, "t"),),
            """
000 push	{r4, r5, r6, r7, lr}
002 add	r7, sp, #0xc
004 push.w	{r8, r9, r10, r11}
008 sub	sp, #0x1c
00a ldr	r4, [r0]
00c movw	r0, @decimal_pairs.low
010 mov	r5, r1
012 movt	r0, @decimal_pairs.high
016 cmp.w	r4, #0x3e8
01a blo	@self+8e
01c movw	r8, #0x967f
020 sub.w	lr, r7, #0x26
024 movs	r3, #0x0
026 movw	r9, #0x2710
02a movw	r10, #0x147b
02e mov.w	r11, #0x64
032 movt	r8, #0x98
036 mov	r1, r4
038 str	r5, [sp, #0xc]
03a str	r4, [sp, #0x8]
03c mov	r2, r1
03e movw	r1, #0x1759
042 movt	r1, #0xd1b7
046 add.w	r4, lr, r3
04a umull	r1, r6, r2, r1
04e subs	r3, #0x4
050 cmp	r2, r8
052 lsr.w	r1, r6, #0xd
056 mls	r6, r1, r9, r2
05a uxth.w	r12, r6
05e lsr.w	r5, r12, #0x2
062 mul	r5, r5, r10
066 lsr.w	r5, r5, #0x11
06a mls	r6, r5, r11, r6
06e ldrh.w	r5, [r0, r5, lsl #1]
072 strh	r5, [r4, #0x6]
074 uxth	r6, r6
076 ldrh.w	r6, [r0, r6, lsl #1]
07a strh	r6, [r4, #0x8]
07c bhi	@self+3c
07e ldrd	r4, r5, [sp, #8]
082 adds	r3, #0xa
084 cmp	r1, #0x9
086 bhi	@self+96
088 mov	r2, r1
08a cbnz	r4, @self+b8
08c b	@self+ba
08e movs	r3, #0xa
090 mov	r1, r4
092 cmp	r1, #0x9
094 bls	@self+88
096 uxth	r2, r1
098 movw	r6, #0x147b
09c lsrs	r2, r2, #0x2
09e subs	r3, #0x2
0a0 muls	r2, r6, r2
0a2 movs	r6, #0x64
0a4 lsrs	r2, r2, #0x11
0a6 mls	r1, r2, r6, r1
0aa sub.w	r6, r7, #0x26
0ae uxth	r1, r1
0b0 ldrh.w	r1, [r0, r1, lsl #1]
0b4 strh	r1, [r6, r3]
0b6 cbz	r4, @self+ba
0b8 cbz	r2, @self+c8
0ba add.w	r0, r0, r2, lsl #1
0be subs	r3, #0x1
0c0 sub.w	r1, r7, #0x26
0c4 ldrb	r0, [r0, #0x1]
0c6 strb	r0, [r1, r3]
0c8 rsb.w	r0, r3, #0xa
0cc str	r0, [sp]
0ce sub.w	r0, r7, #0x26
0d2 movs	r1, #0x1
0d4 add	r3, r0
0d6 mov	r0, r5
0d8 movs	r2, #0x0
0da bl	@role:_RNvMsa_NtCs_4core3fmtNtB5_9Formatter12pad_integral
0de add	sp, #0x1c
0e0 pop.w	{r8, r9, r10, r11}
0e4 pop	{r4, r5, r6, r7, pc}
""".strip().splitlines(),
        ),
    ),
    "_RNvXNtNtNtCs_4core3fmt3num3imphNtB6_7Display3fmt": (
        (
            "<u8 as core::fmt::Display>::fmt",
            96,
            16,
            ((0, 96, "t"),),
            """
000 push	{r7, lr}
002 mov	r7, sp
004 sub	sp, #0x8
006 ldrb	r3, [r0]
008 movw	r12, @decimal_pairs.low
00c movt	r12, @decimal_pairs.high
010 cmp	r3, #0xa
012 blo	@self+30
014 movs	r0, #0x29
016 muls	r0, r3, r0
018 lsrs	r2, r0, #0xc
01a movs	r0, #0x64
01c mls	r0, r2, r0, r3
020 uxtb	r0, r0
022 ldrh.w	r0, [r12, r0, lsl #1]
026 strh	r0, [r7, #-2]
02a movs	r0, #0x1
02c cbnz	r3, @self+36
02e b	@self+38
030 movs	r0, #0x3
032 mov	r2, r3
034 cbz	r3, @self+38
036 cbz	r2, @self+48
038 and	r2, r2, #0x7f
03c subs	r0, #0x1
03e subs	r3, r7, #0x3
040 add.w	r2, r12, r2, lsl #1
044 ldrb	r2, [r2, #0x1]
046 strb	r2, [r3, r0]
048 eor	r2, r0, #0x3
04c str	r2, [sp]
04e subs	r2, r7, #0x3
050 adds	r3, r2, r0
052 mov	r0, r1
054 movs	r1, #0x1
056 movs	r2, #0x0
058 bl	@role:_RNvMsa_NtCs_4core3fmtNtB5_9Formatter12pad_integral
05c add	sp, #0x8
05e pop	{r7, pc}
""".strip().splitlines(),
        ),
    ),
    "_RNvXs3_NtNtNtCs_4core3fmt3num3imptNtB9_7Display3fmt": (
        (
            "<u16 as core::fmt::Display>::fmt",
            158,
            32,
            ((0, 158, "t"),),
            """
000 push	{r4, r6, r7, lr}
002 add	r7, sp, #0x8
004 sub	sp, #0x10
006 ldrh	r2, [r0]
008 movw	r12, @decimal_pairs.low
00c mov	lr, r1
00e movt	r12, @decimal_pairs.high
012 cmp.w	r2, #0x3e8
016 blo	@self+48
018 lsrs	r0, r2, #0x4
01a movw	r1, #0x347
01e muls	r0, r1, r0
020 movw	r1, #0x2710
024 movw	r4, #0x147b
028 lsrs	r0, r0, #0x13
02a mls	r1, r0, r1, r2
02e uxth	r3, r1
030 lsrs	r3, r3, #0x2
032 muls	r3, r4, r3
034 movs	r4, #0x64
036 lsrs	r3, r3, #0x11
038 mls	r1, r3, r4, r1
03c ldrh.w	r3, [r12, r3, lsl #1]
040 strh	r3, [r7, #-12]
044 movs	r3, #0x1
046 b	@self+5e
048 cmp	r2, #0xa
04a blo	@self+6c
04c lsrs	r0, r2, #0x2
04e movw	r1, #0x147b
052 muls	r0, r1, r0
054 movs	r1, #0x64
056 movs	r3, #0x3
058 lsrs	r0, r0, #0x11
05a mls	r1, r0, r1, r2
05e uxth	r1, r1
060 ldrh.w	r1, [r12, r1, lsl #1]
064 strh	r1, [r7, #-10]
068 cbnz	r2, @self+72
06a b	@self+74
06c movs	r3, #0x5
06e mov	r0, r2
070 cbz	r2, @self+74
072 cbz	r0, @self+84
074 lsls	r0, r0, #0x1
076 subs	r3, #0x1
078 uxth	r0, r0
07a sub.w	r1, r7, #0xd
07e add	r0, r12
080 ldrb	r0, [r0, #0x1]
082 strb	r0, [r1, r3]
084 rsb.w	r0, r3, #0x5
088 str	r0, [sp]
08a sub.w	r0, r7, #0xd
08e movs	r1, #0x1
090 add	r3, r0
092 mov	r0, lr
094 movs	r2, #0x0
096 bl	@role:_RNvMsa_NtCs_4core3fmtNtB5_9Formatter12pad_integral
09a add	sp, #0x10
09c pop	{r4, r6, r7, pc}
""".strip().splitlines(),
        ),
    ),
    "_RNvXs1g_NtCs_4core3fmtRtNtB6_5Debug3fmtCs_16dashboard_config": (
        (
            "<&u16 as core::fmt::Debug>::fmt",
            234,
            24,
            ((0, 234, "t"),),
            """
000 push	{r7, lr}
002 mov	r7, sp
004 sub	sp, #0x10
006 ldr	r2, [r1, #0x8]
008 ldr	r0, [r0]
00a lsls	r3, r2, #0x6
00c bmi	@self+1c
00e lsls	r2, r2, #0x5
010 bmi	@self+66
012 add	sp, #0x10
014 pop.w	{r7, lr}
018 b.w	@role:_RNvXs3_NtNtNtCs_4core3fmt3num3imptNtB9_7Display3fmt
01c ldrh	r3, [r0]
01e movw	lr, @lower_hex_digits.low
022 and	r0, r3, #0xf
026 movt	lr, @lower_hex_digits.high
02a ldrb.w	r0, [lr, r0]
02e lsrs	r2, r3, #0x4
030 add.w	r12, sp, #0x8
034 strb.w	r0, [sp, #0xb]
038 beq	@self+b4
03a and	r0, r2, #0xf
03e ldrb.w	r0, [lr, r0]
042 lsrs	r2, r3, #0x8
044 strb.w	r0, [sp, #0xa]
048 beq	@self+be
04a and	r0, r2, #0xf
04e ldrb.w	r0, [lr, r0]
052 lsrs	r2, r3, #0xc
054 strb.w	r0, [sp, #0x9]
058 beq	@self+c8
05a ldrb.w	r0, [lr, r2]
05e add	r3, sp, #0x8
060 strb.w	r0, [sp, #0x8]
064 b	@self+ae
066 ldrh	r3, [r0]
068 movw	lr, @upper_hex_digits.low
06c and	r0, r3, #0xf
070 movt	lr, @upper_hex_digits.high
074 ldrb.w	r0, [lr, r0]
078 lsrs	r2, r3, #0x4
07a add.w	r12, sp, #0xc
07e strb.w	r0, [sp, #0xf]
082 beq	@self+b4
084 and	r0, r2, #0xf
088 ldrb.w	r0, [lr, r0]
08c lsrs	r2, r3, #0x8
08e strb.w	r0, [sp, #0xe]
092 beq	@self+be
094 and	r0, r2, #0xf
098 ldrb.w	r0, [lr, r0]
09c lsrs	r2, r3, #0xc
09e strb.w	r0, [sp, #0xd]
0a2 beq	@self+c8
0a4 ldrb.w	r0, [lr, r2]
0a8 add	r3, sp, #0xc
0aa strb.w	r0, [sp, #0xc]
0ae mov.w	r12, #0x4
0b2 b	@self+d0
0b4 add.w	r3, r12, #0x3
0b8 mov.w	r12, #0x1
0bc b	@self+d0
0be add.w	r3, r12, #0x2
0c2 mov.w	r12, #0x2
0c6 b	@self+d0
0c8 add.w	r3, r12, #0x1
0cc mov.w	r12, #0x3
0d0 movw	r2, @hex_prefix.low
0d4 movt	r2, @hex_prefix.high
0d8 mov	r0, r1
0da mov	r1, r2
0dc movs	r2, #0x2
0de str.w	r12, [sp]
0e2 bl	@role:_RNvMsa_NtCs_4core3fmtNtB5_9Formatter12pad_integral
0e6 add	sp, #0x10
0e8 pop	{r7, pc}
""".strip().splitlines(),
        ),
    ),
    "_RNvXs1g_NtCs_4core3fmtRbNtB6_5Debug3fmtCs_16dashboard_config": (
        (
            "<&bool as core::fmt::Debug>::fmt",
            50,
            8,
            ((0, 50, "t"),),
            """
000 push	{r7, lr}
002 mov	r7, sp
004 ldr	r0, [r0]
006 ldrb	r0, [r0]
008 cmp	r0, #0x0
00a mov	r0, r1
00c beq	@self+20
00e movw	r1, @bool_true.low
012 movt	r1, @bool_true.high
016 movs	r2, #0x4
018 pop.w	{r7, lr}
01c b.w	@role:_RNvMsa_NtCs_4core3fmtNtB5_9Formatter3pad
020 movw	r1, @bool_false.low
024 movt	r1, @bool_false.high
028 movs	r2, #0x5
02a pop.w	{r7, lr}
02e b.w	@role:_RNvMsa_NtCs_4core3fmtNtB5_9Formatter3pad
""".strip().splitlines(),
        ),
    ),
    "_RNvXs1g_NtCs_4core3fmtRhNtB6_5Debug3fmtCs_14dashboard_core": (
        (
            "<&u8 as core::fmt::Debug>::fmt",
            212,
            24,
            ((0, 212, "t"),),
            """
000 push	{r7, lr}
002 mov	r7, sp
004 sub	sp, #0x10
006 mov	r2, r1
008 ldr	r1, [r1, #0x8]
00a ldr	r0, [r0]
00c lsls	r3, r1, #0x6
00e bmi	@self+40
010 lsls	r1, r1, #0x5
012 bmi	@self+62
014 ldrb	r3, [r0]
016 movw	r12, @decimal_pairs.low
01a cmp	r3, #0xa
01c movt	r12, @decimal_pairs.high
020 blo	@self+8c
022 movs	r0, #0x29
024 mul	r0, r3, r0
028 lsrs	r0, r0, #0xc
02a movs	r1, #0x64
02c mls	r1, r0, r1, r3
030 uxtb	r1, r1
032 ldrh.w	r1, [r12, r1, lsl #1]
036 strh	r1, [r7, #-4]
03a movs	r1, #0x1
03c cbnz	r3, @self+92
03e b	@self+94
040 ldrb	r1, [r0]
042 movw	r0, @lower_hex_digits.low
046 and	r3, r1, #0xf
04a movt	r0, @lower_hex_digits.high
04e ldrb	r3, [r0, r3]
050 lsrs	r1, r1, #0x4
052 strb	r3, [r7, #-6]
056 beq	@self+88
058 ldrb	r0, [r0, r1]
05a subs	r3, r7, #0x7
05c strb	r0, [r7, #-7]
060 b	@self+82
062 ldrb	r1, [r0]
064 movw	r0, @upper_hex_digits.low
068 and	r3, r1, #0xf
06c movt	r0, @upper_hex_digits.high
070 ldrb	r3, [r0, r3]
072 lsrs	r1, r1, #0x4
074 strb	r3, [r7, #-1]
078 beq	@self+b4
07a ldrb	r0, [r0, r1]
07c subs	r3, r7, #0x2
07e strb	r0, [r7, #-2]
082 mov.w	r12, #0x2
086 b	@self+bc
088 subs	r0, r7, #0x7
08a b	@self+b6
08c movs	r1, #0x3
08e mov	r0, r3
090 cbz	r3, @self+94
092 cbz	r0, @self+a4
094 and	r0, r0, #0x7f
098 add.w	r0, r12, r0, lsl #1
09c ldrb	r0, [r0, #0x1]
09e subs	r1, #0x1
0a0 subs	r3, r7, #0x5
0a2 strb	r0, [r3, r1]
0a4 subs	r0, r7, #0x5
0a6 eor	r12, r1, #0x3
0aa adds	r3, r0, r1
0ac mov	r0, r2
0ae movs	r1, #0x1
0b0 movs	r2, #0x0
0b2 b	@self+c8
0b4 subs	r0, r7, #0x2
0b6 adds	r3, r0, #0x1
0b8 mov.w	r12, #0x1
0bc movw	r1, @hex_prefix.low
0c0 movt	r1, @hex_prefix.high
0c4 mov	r0, r2
0c6 movs	r2, #0x2
0c8 str.w	r12, [sp]
0cc bl	@role:_RNvMsa_NtCs_4core3fmtNtB5_9Formatter12pad_integral
0d0 add	sp, #0x10
0d2 pop	{r7, pc}
""".strip().splitlines(),
        ),
    ),
    "_RNvXsg_NtCs_4core3fmtbNtB5_7Display3fmt": (
        (
            "<bool as core::fmt::Display>::fmt",
            50,
            8,
            ((0, 50, "t"),),
            """
000 push	{r7, lr}
002 mov	r7, sp
004 ldrb	r0, [r0]
006 mov	r2, r1
008 cbz	r0, @self+1e
00a movw	r1, @bool_true.low
00e mov	r0, r2
010 movt	r1, @bool_true.high
014 movs	r2, #0x4
016 pop.w	{r7, lr}
01a b.w	@role:_RNvMsa_NtCs_4core3fmtNtB5_9Formatter3pad
01e movw	r1, @bool_false.low
022 mov	r0, r2
024 movt	r1, @bool_false.high
028 movs	r2, #0x5
02a pop.w	{r7, lr}
02e b.w	@role:_RNvMsa_NtCs_4core3fmtNtB5_9Formatter3pad
""".strip().splitlines(),
        ),
    ),
    "_RNvXs1g_NtCs_4core3fmtRhNtB6_5Debug3fmtCs_16dashboard_config": (
        (
            "<&u8 as core::fmt::Debug>::fmt",
            40,
            8,
            ((0, 40, "t"),),
            """
000 push	{r7, lr}
002 mov	r7, sp
004 ldr	r2, [r1, #0x8]
006 ldr	r0, [r0]
008 lsls	r3, r2, #0x6
00a bmi	@self+18
00c lsls	r2, r2, #0x5
00e bmi	@self+20
010 pop.w	{r7, lr}
014 b.w	@role:_RNvXNtNtNtCs_4core3fmt3num3imphNtB6_7Display3fmt
018 pop.w	{r7, lr}
01c b.w	@role:_RNvXse_NtNtCs_4core3fmt3numhNtB7_8LowerHex3fmt
020 pop.w	{r7, lr}
024 b.w	@role:_RNvXsg_NtNtCs_4core3fmt3numhNtB7_8UpperHex3fmt
""".strip().splitlines(),
        ),
    ),
    "_RNvXs1g_NtCs_4core3fmtRbNtB6_5Debug3fmtCs_6rp_pac": (
        (
            "<&bool as core::fmt::Debug>::fmt",
            14,
            8,
            ((0, 14, "t"),),
            """
000 push	{r7, lr}
002 mov	r7, sp
004 ldr	r0, [r0]
006 pop.w	{r7, lr}
00a b.w	@role:_RNvXsg_NtCs_4core3fmtbNtB5_7Display3fmt
""".strip().splitlines(),
        ),
    ),
    "_RNvXs1g_NtCs_4core3fmtRtNtB6_5Debug3fmtCs_6rp_pac": (
        (
            "<&u16 as core::fmt::Debug>::fmt",
            40,
            8,
            ((0, 40, "t"),),
            """
000 push	{r7, lr}
002 mov	r7, sp
004 ldr	r2, [r1, #0x8]
006 ldr	r0, [r0]
008 lsls	r3, r2, #0x6
00a bmi	@self+18
00c lsls	r2, r2, #0x5
00e bmi	@self+20
010 pop.w	{r7, lr}
014 b.w	@role:_RNvXs3_NtNtNtCs_4core3fmt3num3imptNtB9_7Display3fmt
018 pop.w	{r7, lr}
01c b.w	@role:_RNvXsm_NtNtCs_4core3fmt3numtNtB7_8LowerHex3fmt
020 pop.w	{r7, lr}
024 b.w	@role:_RNvXso_NtNtCs_4core3fmt3numtNtB7_8UpperHex3fmt
""".strip().splitlines(),
        ),
    ),
    "_RNvXs1g_NtCs_4core3fmtRhNtB6_5Debug3fmtCs_20embassy_hal_internal": (
        (
            "<&u8 as core::fmt::Debug>::fmt",
            40,
            8,
            ((0, 40, "t"),),
            """
000 push	{r7, lr}
002 mov	r7, sp
004 ldr	r2, [r1, #0x8]
006 ldr	r0, [r0]
008 lsls	r3, r2, #0x6
00a bmi	@self+18
00c lsls	r2, r2, #0x5
00e bmi	@self+20
010 pop.w	{r7, lr}
014 b.w	@role:_RNvXNtNtNtCs_4core3fmt3num3imphNtB6_7Display3fmt
018 pop.w	{r7, lr}
01c b.w	@role:_RNvXse_NtNtCs_4core3fmt3numhNtB7_8LowerHex3fmt
020 pop.w	{r7, lr}
024 b.w	@role:_RNvXsg_NtNtCs_4core3fmt3numhNtB7_8UpperHex3fmt
""".strip().splitlines(),
        ),
    ),
}
