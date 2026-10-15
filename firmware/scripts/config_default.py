"""Grouped current default preparation, before the widget/source loops.

This binds source constructors to actual receiver/copy/scratch interfaces.
It does not expose a complete or validated Config. Incoming private-slot/fault
ownership, all remaining phases and whole-image indirect closure remain owed.
"""
import re
import struct
from config_abstract import Span
from config_abstract_compare import InitializedPrefix
from config_construction import string_interface
from config_entry import CONFIG,entry_object
from config_finite import _literal_body
from config_headers import ROOT,ROOT_ROWS
from config_helpers import helper_coverage,_helper_function,HELPER_SEMANTIC_ROLES
from config_regions import Storage,borrow,require_disjoint,require_writes,_control
from config_roots import root_window
from config_stack import canonical_symbol,typed_function,transfer_family
from llvm_stack import mapping_ranges
from validation import require

_TRY=HELPER_SEMANTIC_ROLES['string_try14']
_PREPARE=((0,4,'ldr.w r8, [pc, @object0.pc_slot]'),(4,4,'movw r0, #0x38ef'),(8,4,'mov.w r10, #0x1'),
    (12,2,'ldr r1, [pc, @object1.pc_slot]'),(14,4,'add.w r9, sp, #0x70'),(18,2,'movs r2, #0x8'),
    (20,4,'strb.w r10, [r8, r0]'),(24,4,'movw r0, #0x38ea'),(28,4,'str.w r1, [r8, r0]'),
    (32,4,'ldr.w r1, [pc, @object2.pc_slot]'),(36,2,'mov r0, r9'),(38,4,'bl @role:'+_TRY),
    (42,2,'ldr r0, [sp, #0x70]'),(44,2,'cmp r0, #0x1'),(46,4,'beq.w @capacity_failure'),
    (50,4,'strd r5, r4, [sp, #16]'),(54,4,'add.w r0, r9, #0x4'),(58,4,'add.w r12, sp, #0xec'),
    (62,4,'vldr s22, [pc, #384]'),(66,4,'vldr s20, [pc, #384]'),(70,4,'vldr s24, [pc, #384]'),
    (74,4,'vldr s26, [pc, #384]'),(78,4,'vldr s28, [pc, #384]'),(82,4,'vldr s17, [pc, #384]'),
    (86,4,'vldr s19, [pc, #384]'),(90,4,'vldr s25, [pc, #384]'),(94,4,'vldr s27, [pc, #384]'),
    (98,4,'vmov.f32 s30, #-2.000000e+01'),(102,4,'vmov.f32 s21, #1.600000e+01'),(106,4,'vmov.f32 s23, #8.000000e+00'),
    (110,4,'ldm.w r0, {r2, r3, r4, r5, r6}'),(114,2,'mov r1, r12'),(116,4,'add.w r0, r8, #0xf8'),
    (120,2,'stm r1!, {r2, r3, r4, r5, r6}'),(122,2,'mov r1, r12'),(124,4,'ldm.w r1, {r2, r3, r4, r5, r6}'),
    (128,2,'movs r1, #0x0'),(130,2,'stm r0!, {r2, r3, r4, r5, r6}'),(132,4,'str.w r10, [r8, #0xf4]'),
    (136,4,'strb.w r10, [r8, #0x10c]'),(140,4,'movw r5, #0xffff'),(144,4,'mov.w r10, #0x0'),
    (148,4,'str.w r1, [r8, #0x1b8]'),(152,2,'str r1, [sp, #0x50]'),
    (154,2,'ldr r0, [pc, @object3.pc_slot]'),(156,2,'str r0, [sp, #0x4c]'),
    (158,2,'ldr r0, [pc, @object4.pc_slot]'),(160,2,'str r0, [sp, #0x48]'),
    (162,4,'add.w r0, r9, #0x1'),(166,2,'str r0, [sp, #0x1c]'),(168,2,'add r0, sp, #0xb4'),
    (170,2,'adds r0, #0x2'),(172,2,'str r0, [sp, #0x18]'),(174,2,'add r0, sp, #0x40'),
    (176,2,'adds r0, #0x8'),(178,2,'str r0, [sp, #0x20]'),(180,2,'ldr r0, [pc, @object5.pc_slot]'))
_OBJECTS=({'kind':'entry_static','role':CONFIG},{'kind':'default_scalar','value':0x10004},
    {'kind':'default_bytes','bytes':b'Overview'},{'kind':'default_scalar','value':0x05040301},
    {'kind':'default_scalar','value':0x02060700},{'kind':'default_scalar','value':0xfffff920})


def _object(objects,address,descriptor):
    if descriptor['kind']=='entry_static':return entry_object(objects,address,descriptor)
    if descriptor['kind']=='default_scalar':
        require(address==descriptor['value'],'default typed scalar/paired channel constructor differs');return ()
    require(descriptor['kind']=='default_bytes' and objects.read_only(address,len(descriptor['bytes']))==descriptor['bytes'],
        'default source literal/paired channel constructor differs')
    return ()


def _no_outline(*args):require(False,'unreviewed default-preparation outline')


def _ingress(instructions,parent,extent,start,end):
    for at,row in instructions.items():
        family=transfer_family(row.mnemonic.removesuffix('.w'));operand=row.operands.split('@',1)[0].strip()
        if family in ('b','bl','cbz','cbnz'):
            target=re.search(r'(?<![#\w])0x([0-9a-f]+)(?:\s|$)',operand)
            require(target is not None,'default prefix has unresolved direct ingress')
            require(not (start<int(target[1],16)<end and not start<=at<end),'foreign default-preparation interior ingress')
        if not parent<=at<parent+extent:continue
        control,conditional=_control(row)
        if control=='tbb':
            index=re.fullmatch(r'\[pc, r([0-9]|1[0-2])\]',operand)
            require(index is not None and row.raw==struct.pack('<HH',0xe8df,0xf000|int(index[1])) and
                0<=at<=0xffffffff-514,'unreviewed default prefix table ingress')
            require(at+514<=start or at+4>=end,'table destination envelope overlaps default preparation')
        else:require(control in (None,'b','bl','cbz','cbnz','return') or control=='bx' and operand=='lr' and not conditional,
            'unresolved indirect/exception default-prefix ingress')


def default_preparation(objects,name,instructions,frame,ancestor_protected):
    """Current SizeZ prepared entry -> View -> widget-loop before-call state.

    No absolute capture address or body hash is a selector. Root shape, unique
    constructor anchor and independently matched capacity suffix bind this
    group. The original fault's incoming type/first slots still need the exact
    CRC/decode/canonical predecessor joins; this group does not invent them.
    """
    require(name in objects.elf.functions and canonical_symbol(name)==ROOT,'unreviewed default parent')
    parent=objects.elf.functions[name][0]&~1;extent,size,_=typed_function(objects,parent,ROOT)
    require(size==456 and isinstance(frame,Storage) and frame.space.startswith('stack:') and frame.extent==456 and
        frame.alignment>=8 and frame.writable,'unreviewed default activation')
    ancestor_protected=tuple(ancestor_protected);require_disjoint((frame,)+ancestor_protected)
    prologue=dict(extent=14,instructions=ROOT_ROWS[:5],objects=[])
    _literal_body(objects,name,instructions,ROOT,'default loader prologue',prologue,_no_outline,_no_outline,window=parent)
    rows={a:r for a,r in instructions.items() if parent<=a<parent+extent}
    candidates=[]
    for start,row in sorted(rows.items()):
        if row.mnemonic!='ldr.w' or not row.operands.startswith('r8, [pc,'):continue
        anchor=rows.get(start+4);branch=rows.get(start+46)
        if anchor is None or (anchor.mnemonic,anchor.operands)!=('movw','r0, #0x38ef') or branch is None or branch.mnemonic!='beq.w':continue
        target=re.match(r'0x([0-9a-f]+)(?:\s|$)',branch.operands)
        if target is None:continue
        destination=int(target[1],16);capacity=[]
        # This is structural discovery, not target acceptance. The complete
        # typed suffix below must start at that same current branch target.
        for at,current in rows.items():
            if not destination<=at<destination+64 or current.mnemonic!='bl':continue
            try:suffix=root_window(objects,name,instructions,at)
            except AssertionError:continue
            if suffix['location_role']=='view_capacity' and suffix['window']==destination:capacity.append(suffix)
        if len(capacity)!=1:continue
        failure=capacity[0]
        contract=dict(extent=182,instructions=[(o,w,t.replace('@capacity_failure','@self+'+format(failure['window']-start,'x')))
            for o,w,t in _PREPARE],objects=_OBJECTS)
        try:proof=_literal_body(objects,name,instructions,ROOT,'sizez default preparation',contract,_object,_no_outline,
            window=start,function_consumer=_helper_function)
        except AssertionError:continue
        candidates.append((proof,failure))
    require(len(candidates)==1,'missing, changed or ambiguous default preparation');proof,failure=candidates[0];start=proof['function'];end=start+182
    _ingress(instructions,parent,extent,start,end)
    saved=Storage(frame.space,frame.identity+':saved',frame.start+352,104,4,frame.phase,False)
    protected=ancestor_protected+(saved,)
    owner=Storage('address','private unpublished ConfigSlot',proof['objects'][0],14604,4,frame.phase,True)
    config=borrow(owner,0,14576,frame.phase,writable=True)
    require_disjoint((owner,)+protected)
    literal=Storage('address','default Overview',proof['objects'][2],8,1,frame.phase,False)
    source=InitializedPrefix(borrow(literal,0,8,frame.phase),Span(8,8))
    result_view=borrow(frame,112,24,frame.phase,writable=True)
    calls=[e for e in proof['ordinary_edges'] if e[3]==_TRY];require(len(calls)==1 and calls[0][0]==38,'default String call differs')
    target=calls[0][2];names=[n for n,e in objects.elf.functions.items() if e[0]&~1==target and canonical_symbol(n)==_TRY]
    require(len(names)==1,'default String target is ambiguous')
    child=helper_coverage(objects,names[0],instructions,(target,'default View construction',start))
    active=Storage(frame.space,frame.identity+':String14',frame.start-48,48,8,frame.phase,True)
    returned=string_interface(objects,instructions,child,source,result_view,active,protected)
    require(set(('r4','r5','r8','r9','r10'))<=set(returned['normal_preserves']),'View child loses live default/fault carriers')
    successes=[o for o in returned['outcomes'] if o['kind']=='success'];require(len(successes)==1,'View has no source-typed String success')
    # Direct and opaque-copy envelopes are distinct from initialized fields.
    local_writes=tuple((borrow(frame,o,n,frame.phase,writable=True),0,n) for o,n in
        ((16,8),(236,20),(80,4),(76,4),(72,4),(28,4),(24,4),(32,4)))
    require_writes(tuple(v for v,o,n in local_writes),local_writes,protected)
    config_writes=tuple((config,o,n) for o,n in ((14575,1),(14570,4),(248,20),(244,4),(268,1),(440,4)))
    require_writes((config,),config_writes,protected+(literal,))
    floats=[]
    for offset in range(62,98,4):
        row=rows[start+offset];pool=((row.address+4)&~3)+384
        require(any(k=='d' and lo<=pool<pool+4<=hi for lo,hi,k in mapping_ranges(objects.elf,name)),
            'default float is not caller-owned mapped readonly data')
        objects.read_only(pool,4);floats.append(pool)
    fault=Storage(frame.space,frame.identity+':retained original fault',frame.start+8,16,4,frame.phase,False)
    diagnostic_write=(borrow(frame,0,4,frame.phase,writable=True),0,4)
    require_writes((diagnostic_write[0],),(diagnostic_write,),protected+(fault,))
    name_view=InitializedPrefix(borrow(owner,252,14,frame.phase),Span(8,8))
    return dict(proof=proof,prologue_frame=size,config=config,view=dict(count=1,element_offset=248,name_capacity=14,
        initialized_name=name_view,opaque_string_transport=(248,20),id_offset=268),
        widget_handoff=dict(channels=(0,7,6,2,1,3,4,5),array=borrow(frame,72,8,frame.phase),
            signed_offset=-1760,count=0,cursor_cell=borrow(frame,32,4,frame.phase),
            required_affine_relation='W_i=B+0x1bc+220*i, count=i, offset=-1760+220*i; i=0..7'),
        fault=fault,protected=protected+(fault,),local_writes=local_writes,config_writes=config_writes,child=returned,
        capacity_failure=dict(root=failure,error_payload_bytes=0,child_result_tag=1,local_writes=(diagnostic_write,),
            argument_address=frame.start+351,argument_reference_levels=1,
            requires_nonreturn_diagnostic_closure=True),
        readonly_float_pools=tuple(floats),continuation=end,state_coordinate='before_widget_loop_guard',
        target_access_requirements=(dict(operation='single_word_store',site=start+28,
            address=owner.start+14570,width=4,unaligned=True,
            owner_role='Config.version/catalogue_version',status='unresolved_object_store_policy'),),
        conservative_activation_frames=(456,48,16,24),
        requires_incoming_SP_r7_frame_and_contextual_predecessor_transport=True,
        requires_incoming_typed_fault_exclusive_slot_and_all_remaining_phases=True,
        requires_whole_image_indirect_ingress_and_stage_coverage=True)
