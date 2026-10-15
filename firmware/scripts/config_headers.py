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
from config_decode_effects import DecoderState,SequenceState,_object
from config_finite import _literal_body
from config_regions import Storage,borrow,require_disjoint,require_writes
from config_stack import canonical_symbol,typed_function,transfer_family
from llvm_stack import mapping_ranges
from validation import require

ROOT='_RNvMNtNtCs_16dashboard_config8validate4slotNtB2_10ConfigSlot12load_at_boot'
VISITOR='_RINvXs_NtCs_16dashboard_config15decode_in_placeNtB5_10ConfigSeedNtNtCs_10serde_core2de7Visitor9visit_seqINtNtNtCs_8postcard2de12deserializer9SeqAccessNtNtB25_7flavors5SliceEEB7_'
NEXT_U16='_RINvNtCs_16dashboard_config15decode_in_place4nexttINtNtNtCs_8postcard2de12deserializer9SeqAccessNtNtB12_7flavors5SliceEEB4_'

ROOT_ROWS=(
    (0,2,'push {r4, r5, r6, r7, lr}'),(2,2,'add r7, sp, #0xc'),(4,4,'push.w {r7, r8, r9, r10, r11}'),
    (8,4,'vpush {d8, d9, d10, d11, d12, d13, d14, d15}'),(12,2,'sub sp, #0x160'),
    (14,4,'ldr.w r12, [pc, @object0.pc_slot]'),(18,4,'movw r0, @payload_length'),(22,2,'ldr r5, [pc, @object1.pc_slot]'),
    (24,4,'mov.w r3, #0xffffffff'),(28,4,'mov.w r9, #0x0'),(32,4,'add.w lr, sp, #0x70'),(36,2,'str r0, [sp, #0x7c]'),
    (38,2,'movs r0, #0x8'),(40,4,'add.w r8, r12, #0xc'),(44,2,'str r0, [sp, #0x74]'),
    (46,4,'str.w r12, [sp, #0x70]'),(50,4,'str.w r8, [sp, #0x78]'),(54,4,'cmp.w r9, #0x10'),(58,2,'beq @self+6e'),
    (60,4,'add.w r0, lr, r9'),(64,4,'ldr.w r6, [lr, r9]'),(68,2,'ldr r0, [r0, #0x4]'),(70,2,'add r0, r6'),
    (72,2,'cmp r6, r0'),(74,2,'beq @self+68'),(76,2,'ldrb r4, [r6]'),(78,2,'eors r3, r4'),(80,2,'movs r4, #0x8'),
    (82,2,'cbz r4, @self+64'),(84,4,'eor.w r2, r5, r3, lsr #1'),(88,2,'lsls r1, r3, #0x1f'),(90,2,'it eq'),
    (92,2,'lsreq r2, r3, #0x1'),(94,2,'subs r4, #0x1'),(96,2,'mov r3, r2'),(98,2,'b @self+52'),
    (100,2,'adds r6, #0x1'),(102,2,'b @self+48'),(104,4,'add.w r9, r9, #0x8'),(108,2,'b @self+36'),
    (110,2,'ldr r0, [pc, @object2.pc_slot]'),(112,2,'cmp r3, r0'),(114,2,'bne @checksum_failure'),
    (116,4,'addw r0, r12, @file_length'),(120,4,'strd r8, r0, [sp, #236]'),(124,2,'add r0, sp, #0xec'),
    (126,4,'bl @role:'+VISITOR),
)
VISITOR_ROWS=((0,2,'push {r4, r5, r6, r7, lr}'),(2,2,'add r7, sp, #0xc'),
    (4,4,'push.w {r7, r8, r9, r10, r11}'),(8,4,'vpush {d8, d9, d10, d11, d12, d13, d14}'),
    (12,4,'sub.w sp, sp, #0x5e8'),(16,2,'movs r1, #0xd'),(18,4,'strd r0, r1, [sp, #288]'),
    (22,2,'add r0, sp, #0x120'),(24,4,'bl @role:'+NEXT_U16))


def _no_object(*args):require(False,'unexpected constructor object/outline')


def _parent(objects,name,role,frame,size):
    require(name in objects.elf.functions and canonical_symbol(name)==role,'unreviewed typed constructor parent')
    start=objects.elf.functions[name][0]&~1;extent,actual,_=typed_function(objects,start,role)
    require(actual==size and isinstance(frame,Storage) and frame.space.startswith('stack:') and frame.extent==size and
            frame.alignment>=8 and frame.writable,'constructor activation/prologue metadata differs')
    return start,extent


def _pc_word(objects,name,row,register):
    match=re.fullmatch(register+r', \[pc, #(0x[0-9a-f]+|[0-9]+)\]',row.operands.split('@',1)[0].strip())
    require(row.mnemonic in ('ldr','ldr.w') and match is not None,'constructor readonly producer differs')
    pool=((row.address+4)&~3)+int(match[1],0)
    require(pool%4==0 and any(k=='d' and lo<=pool<pool+4<=hi for lo,hi,k in mapping_ranges(objects.elf,name)),
            'constructor pool is not mapped data owned by its parent')
    return struct.unpack('<I',objects.read_only(pool,4))[0]


def _direct_ingress(instructions,start,end):
    for at,row in instructions.items():
        if start<=at<end:continue
        if transfer_family(row.mnemonic.removesuffix('.w')) not in ('b','bl','cbz','cbnz'):continue
        target=re.search(r'(?<![#\w])0x([0-9a-f]+)(?:\s|$)',row.operands)
        require(target is None or not start<int(target[1],16)<end,'foreign direct ingress skips constructor state')


def root_decoder_header(objects,name,instructions,frame,ancestor_protected):
    start,extent=_parent(objects,name,ROOT,frame,456);ancestor_protected=tuple(ancestor_protected)
    require_disjoint((frame,)+ancestor_protected)
    require(start+14 in instructions,'missing root source producer')
    address=_pc_word(objects,name,instructions[start+14],'r12');header=objects.read_only(address,12)
    require(header[:4]==b'EAV1' and struct.unpack_from('<H',header,4)[0]==4,'unreviewed embedded current-version envelope')
    n=struct.unpack_from('<H',header,6)[0];length=n+12
    require(0<n and length<=min(16384,4095) and 0x10000000<=address<address+length<=0x101fe000,
            'embedded input does not fit this finite ADDW constructor/XIP family')
    raw=objects.read_only(address,length);checksum=struct.unpack_from('<I',header,8)[0]
    require(zlib.crc32(raw[:8]+raw[12:])==checksum,'embedded envelope checksum differs')
    # A separate typed checksum-failure anchor binds the external branch. Its
    # default-fill successor/source fault semantics still need their own family.
    failure_rows=((0,4,'movw r0, #'+hex(n)),(4,2,'movs r5, #0x0'),(6,2,'movs r4, #0x5'),(8,2,'str r0, [sp, #0x8]'))
    failure_contract=dict(extent=10,instructions=failure_rows,objects=[],data=[])
    failures=[]
    for at,row in sorted(instructions.items()):
        if not start+130<=at<start+extent-10 or row.mnemonic!='movw':continue
        try:failure=_literal_body(objects,name,instructions,ROOT,'checksum_failure',failure_contract,_no_object,_no_object,window=at)
        except AssertionError:continue
        failures.append(failure)
    require(len(failures)==1,'checksum-failure source anchor is missing or ambiguous');failure=failures[0]
    def object_consumer(objects,value,descriptor):
        expected={'embedded_eav':address,'crc_polynomial':0xedb88320,'complement_checksum':checksum^0xffffffff}
        require(descriptor['kind'] in expected and value==expected[descriptor['kind']],'constructor source/polynomial/checksum binding differs')
        return ()
    rows=[(off,width,text.replace('@payload_length','#'+hex(n)).replace('@file_length','#'+hex(length)).replace(
           '@checksum_failure','@self+'+format(failure['function']-start,'x'))) for off,width,text in ROOT_ROWS]
    contract=dict(extent=130,instructions=rows,objects=[dict(kind=k) for k in ('embedded_eav','crc_polynomial','complement_checksum')],data=[])
    proof=_literal_body(objects,name,instructions,ROOT,'sizez_root_D',contract,object_consumer,_no_object,window=start)
    _direct_ingress(instructions,start,start+130)
    saved=Storage(frame.space,frame.identity+':saved',frame.start+352,104,4,frame.phase,False);protected=ancestor_protected+(saved,)
    D=borrow(frame,236,8,frame.phase,writable=True);crc=borrow(frame,112,16,frame.phase,writable=True)
    failure_length=borrow(frame,8,4,frame.phase,writable=True)
    require_writes((D,crc,failure_length),((D,0,8),(crc,0,16),(failure_length,0,4)),protected)
    owner=Storage('address','embedded_eav',address,length,1,frame.phase,False);payload=borrow(owner,12,n,frame.phase)
    state=DecoderState(D,payload,0)
    return dict(proof=proof,decoder_state=state,protected=protected,checksum_failure=failure,
                call_site=start+126,call_role=VISITOR,state_coordinate='before_call',source_phase='embedded_envelope_checked',
                writes=((crc,0,16),(D,0,8)),checksum_failure_writes=((failure_length,0,4),),
                checksum_failure_requires_fault_mapping_and_default_successor=True,
                crc_loop_invariants=('two source slices: header[0,8),payload[0,n)',
                'r9 descriptor offset0,8,16; byte cursor within selected slice','r4 bit count8..0; reflected polynomial0xedb88320'),
                requires_first_consumer_effect_before_exit=True,requires_parent_entry_all_ingress_source_effects_and_coverage=True)


def visitor_sequence_header(objects,name,instructions,frame,decoder,ancestor_protected):
    start,extent=_parent(objects,name,VISITOR,frame,1608);ancestor_protected=tuple(ancestor_protected)
    require(isinstance(decoder,DecoderState) and decoder.header.storage.space==frame.space and
            decoder.header.storage.phase==frame.phase,'visitor input has another D/phase ABI')
    require_disjoint((frame,)+ancestor_protected)
    require_disjoint((frame,_object(decoder.header,'live-D')))
    contract=dict(extent=28,instructions=VISITOR_ROWS,objects=[],data=[])
    proof=_literal_body(objects,name,instructions,VISITOR,'sizez_Config_Q',contract,_no_object,_no_object,window=start)
    _direct_ingress(instructions,start,start+28)
    saved=Storage(frame.space,frame.identity+':saved',frame.start+1512,96,4,frame.phase,False);protected=ancestor_protected+(saved,)
    Q=borrow(frame,288,8,frame.phase,writable=True)
    require_writes((Q,),((Q,0,8),),protected+(_object(decoder.header,'live-D'),_object(decoder.payload,'immutable-input')))
    return dict(proof=proof,sequence_state=SequenceState(Q,decoder,13),protected=protected,
                call_site=start+24,call_role=NEXT_U16,state_coordinate='before_call',source_phase='Config_thirteen_fields',writes=((Q,0,8),),
                requires_first_consumer_effect_before_exit=True,requires_parent_entry_all_ingress_source_effects_and_coverage=True)


def root_visitor_headers(objects,name,instructions,frame,ancestor_protected):
    """One checked root→visitor construction edge, retaining both owners.

    This stops BEFORE the first next<u16> call. Neither the root's visitor
    return nor that wrapper's return ABI/effects are inferred from these facts.
    """
    root=root_decoder_header(objects,name,instructions,frame,ancestor_protected)
    edges=[e for e in root['proof']['ordinary_edges'] if e[3]==VISITOR]
    require(len(edges)==1 and edges[0][0]==126 and edges[0][4]==1608,'root visitor edge/activation differs')
    target=edges[0][2];names=[n for n,e in objects.elf.functions.items() if e[0]&~1==target and canonical_symbol(n)==VISITOR]
    require(len(names)==1,'root visitor target is ambiguous')
    child=Storage(frame.space,'ConfigSeed',frame.start-1608,1608,8,frame.phase,True)
    visitor=visitor_sequence_header(objects,names[0],instructions,child,root['decoder_state'],root['protected'])
    return dict(root=root,visitor=visitor,visitor_frame=child,state_coordinate='before_first_next_call',
                retained_parent_frames=(456,1608),requires_first_consumer_effect_before_exit=True,
                requires_parent_entry_all_ingress_source_effects_and_coverage=True)
