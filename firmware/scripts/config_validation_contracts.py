"""Finite ValidationError source-field assembly, reviewed Task2 contracts."""

VALIDATION_BUILDERS = {
    'wide': (
        158, 80, ((0, 158, 't'),),
        (((14, 22), ('debug', 'option_channel_ref')), ((18, 28), ('literal', b'channel')), ((32, 36), ('debug', 'option_u16')), ((46, 74), ('literal', b'view')), ((50, 78), ('debug', 'field')), ((70, 86), ('debug', 'option_u8')), ((82, 114), ('literal', b'widget')), ((90, 118), ('literal', b'field')), ((94, 122), ('debug', 'error_code')), ((106, 128), ('literal', b'ValidationError')), ((110, 132), ('literal', b'code'))),
        """
000 push	{r4, r5, r6, r7, lr}
002 add	r7, sp, #0xc
004 str	r8, [sp, #-4]!
008 sub	sp, #0x38
00a adds	r2, r0, #0x6
00c str	r2, [sp, #0x34]
00e movw	r2, @value0.low
012 movw	r5, @value1.low
016 movt	r2, @value0.high
01a add	r4, sp, #0x34
01c movt	r5, @value1.high
020 movw	r6, @value2.low
024 movt	r6, @value2.high
028 strd	r5, r4, [sp, #40]
02c str	r2, [sp, #0x30]
02e movw	r2, @value3.low
032 movw	r5, @value4.low
036 mov	r12, r1
038 adds	r3, r0, #0x7
03a add.w	r1, r0, #0x8
03e add.w	lr, r0, #0x4
042 strd	r0, r6, [sp, #32]
046 movw	r0, @value5.low
04a movt	r2, @value3.high
04e movt	r5, @value4.high
052 movw	r8, @value6.low
056 movt	r0, @value5.high
05a movw	r6, @value7.low
05e movw	r4, @value8.low
062 strd	r1, r5, [sp, #8]
066 strd	r2, lr, [sp, #16]
06a movw	r1, @value9.low
06e movw	r2, @value10.low
072 movt	r8, @value6.high
076 movt	r6, @value7.high
07a movt	r4, @value8.high
07e str	r0, [sp, #0x18]
080 movt	r1, @value9.high
084 movt	r2, @value10.high
088 mov	r0, r12
08a strd	r4, r6, [sp]
08e str.w	r8, [sp, #0x1c]
092 bl	@struct5
096 add	sp, #0x38
098 ldr	r8, [sp], #4
09c pop	{r4, r5, r6, r7, pc}
""".strip().splitlines()),
    'pool': (
        124, 72, ((0, 80, 't'), (80, 124, 'd')),
        (((10, 120), ('literal', b'code')), ((16, 80), ('debug', 'option_channel_ref')), ((24, 84), ('literal', b'channel')), ((28, 88), ('debug', 'option_u16')), ((34, 92), ('literal', b'widget')), ((38, 96), ('debug', 'option_u8')), ((46, 100), ('literal', b'view')), ((50, 104), ('debug', 'field')), ((62, 108), ('literal', b'field')), ((66, 112), ('debug', 'error_code')), ((70, 116), ('literal', b'ValidationError'))),
        """
000 push	{r4, r6, r7, lr}
002 add	r7, sp, #0x8
004 sub	sp, #0x38
006 mov	r4, r1
008 adds	r1, r0, #0x6
00a ldr	r2, [pc, #0x6c]
00c adds	r3, r0, #0x7
00e str	r1, [sp, #0x34]
010 ldr	r1, [pc, #0x3c]
012 str	r1, [sp, #0x30]
014 add	r1, sp, #0x34
016 str	r1, [sp, #0x2c]
018 ldr	r1, [pc, #0x38]
01a str	r1, [sp, #0x28]
01c ldr	r1, [pc, #0x38]
01e strd	r0, r1, [sp, #32]
022 ldr	r1, [pc, #0x38]
024 str	r1, [sp, #0x1c]
026 ldr	r1, [pc, #0x38]
028 str	r1, [sp, #0x18]
02a adds	r1, r0, #0x4
02c str	r1, [sp, #0x14]
02e ldr	r1, [pc, #0x34]
030 str	r1, [sp, #0x10]
032 ldr	r1, [pc, #0x34]
034 str	r1, [sp, #0xc]
036 add.w	r1, r0, #0x8
03a mov	r0, r4
03c str	r1, [sp, #0x8]
03e ldr	r1, [pc, #0x2c]
040 str	r1, [sp, #0x4]
042 ldr	r1, [pc, #0x2c]
044 str	r1, [sp]
046 ldr	r1, [pc, #0x2c]
048 bl	@struct5
04c add	sp, #0x38
04e pop	{r4, r6, r7, pc}
""".strip().splitlines()),
}
