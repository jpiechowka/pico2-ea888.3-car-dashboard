"""Reviewed SizeZ Result/carrier phase sequences, not a validator grammar.

These source-role windows preserve the current private output/first/restore
ABI. Changed emission requires source and current-byte review. Uncovered
traversal, typed error construction, alias/child preservation and source
ownership remain the separate enclosing conservative graph's obligations.
"""
RANGE='_RINvMs8_NtNtCs_4core3ops5rangeINtB6_14RangeInclusivetE8containstECs_14dashboard_core'
FIRST='_RNCINvNtCs_16dashboard_config8validate16visit_validationNCNvB4_12validate_ref0E0B6_'
CONTRACTS={
 'default_call':dict(extent=14,instructions=((0,2,'add r4, sp, #0xec'),(2,2,'ldrh r0, [r1]'),
    (4,4,'strh.w r0, [r12]'),(8,2,'mov r0, r4'),(10,4,'bl @role:_RNvNtCs_16dashboard_config8validate12validate_ref')),objects=()),
 'decoded_call':dict(extent=6,instructions=((0,2,'add r0, sp, #0x70'),
    (2,4,'bl @role:_RNvNtCs_16dashboard_config8validate12validate_ref')),objects=()),
 'entry':dict(extent=88,instructions=(
    (0,2,'push {r4, r5, r6, r7, lr}'),(2,2,'add r7, sp, #0xc'),
    (4,4,'push.w {r7, r8, r9, r10, r11}'),(8,4,'vpush {d8, d9, d10, d11, d12, d13, d14, d15}'),
    (12,2,'sub sp, #0xd8'),(14,2,'mov r5, r0'),(16,2,'movs r0, #0x2'),
    (18,4,'ldr.w r4, [pc, @object0.pc_slot]'),(22,2,'add r6, sp, #0x4c'),
    (24,4,'strh.w r0, [sp, #0x4c]'),(28,2,'movs r0, #0x1'),(30,2,'str r6, [sp, #0x58]'),
    (32,4,'strb r0, [r7, #-217]'),(36,4,'movw r0, #0x38ea'),(40,2,'ldrh r0, [r4, r0]'),
    (42,2,'cmp r0, #0x4'),(44,4,'bne.w @version'),(48,4,'movw r0, #0x38ec'),
    (52,2,'ldrh r0, [r4, r0]'),(54,2,'cmp r0, #0x1'),(56,4,'bne.w @catalogue'),
    (60,4,'ldrh.w r1, [r4, #0xf2]'),(64,4,'ldr.w r0, [pc, @object1.pc_slot]'),
    (68,4,'bl @role:'+RANGE),(72,2,'cmp r0, #0x0'),(74,4,'beq.w @hold'),
    (78,2,'add r0, sp, #0x60'),(80,2,'movs r2, #0x0'),(82,4,'mov.w r10, #0x2'),
    (86,2,'str r5, [sp, #0x3c]')),
    objects=({'kind':'config'},{'kind':'hold_range'})),
 'early':dict(extent=22,instructions=(
    (0,2,'ldr r1, [pc, @object0.pc_slot]'),(2,2,'movs r0, #0x0'),(4,2,'b @self+a'),
    (6,2,'movs r0, #0x0'),(8,2,'ldr r1, [pc, @object1.pc_slot]'),
    (10,4,'bl @outline:first_tail'),(14,4,'strb.w r0, [sp, #0x50]'),
    (18,4,'strh.w r0, [sp, #0x4c]')),objects=({'kind':'first_literal','value':0x900ff},{'kind':'first_literal','value':0xa00ff})),
 'error':dict(extent=32,instructions=(
    (0,4,'ldrh.w r4, [sp, #0x4c]'),(4,2,'cmp r4, #0x2'),(6,4,'beq.w @none'),
    (10,2,'adds r0, r5, #0x2'),(12,2,'adds r1, r6, #0x2'),(14,2,'movs r2, #0xa'),
    (16,4,'bl @role:__aeabi_memcpy'),(20,2,'strh r4, [r5]'),(22,2,'add sp, #0xd8'),
    (24,4,'vpop {d8, d9, d10, d11, d12, d13, d14, d15}'),(28,4,'b.w @outline:validator_restore')),objects=()),
 'hold':dict(extent=14,instructions=(
    (0,4,'sub.w r0, r7, #0xd9'),(4,2,'add r1, sp, #0x58'),(6,2,'movs r2, #0x2'),
    (8,4,'bl @role:'+FIRST),(12,2,'b @error')),objects=()),
 'late_reset':dict(extent=6,instructions=((0,2,'ldr r5, [sp, #0x3c]'),(2,2,'add r6, sp, #0x4c'),
    (4,2,'b @error')),objects=()),
 'success':dict(extent=24,instructions=(
    (0,4,'ldrb r0, [r7, #-217]'),(4,2,'ldr r5, [sp, #0x3c]'),(6,2,'add r6, sp, #0x4c'),
    (8,2,'cmp r0, #0x0'),(10,4,'beq.w @error'),(14,2,'movs r4, #0x2'),
    (16,4,'vstr s0, [r5, #4]'),(20,4,'b.w @tag')),objects=()),
 'first_tail':dict(extent=10,frame=0,mapping=((0,10,'t'),),instructions=(
    (0,4,'strb.w r0, [sp, #0x56]'),(4,4,'str.w r1, [sp, #0x52]'),(8,2,'bx lr')),objects=()),
 'validator_restore':dict(extent=8,frame=0,mapping=((0,8,'t'),),instructions=(
    (0,2,'add sp, #0x4'),(2,4,'pop.w {r8, r9, r10, r11}'),(6,2,'pop {r4, r5, r6, r7, pc}')),objects=()),
}
