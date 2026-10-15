"""Same-invocation loader geometry/static points, with effects still pending.

The tables are authored from the admitted ordinary support's current source
families. They bind individual typed static producers, not their surrounding
dataflow, paths or complete consuming effects. No mutable owner is emitted.
"""
import re
import struct
from config_boot_history import first_boot_history
from config_finite import _literal_body
from config_headers import ROOT,VISITOR
from config_stack import canonical_symbol,typed_function
from llvm_stack import mapping_ranges
from validation import require

VALIDATOR='_RNvNtCs_16dashboard_config8validate12validate_ref'
_FRAMES={'speed2':(1288,296),'sizes':(696,288),'sizez':(456,320)}
_PREFIX=((0,2,'push {r4, r5, r6, r7, lr}'),(2,2,'add r7, sp, #0xc'))
_PROLOGUES={
 'speed2':_PREFIX+((4,4,'push.w {r8, r9, r10, r11}'),(8,2,'sub sp, #0x4'),
     (10,4,'vpush {d8, d9, d10, d11, d12, d13, d14, d15}'),(14,4,'sub.w sp, sp, #0x4a0')),
 'sizes':_PREFIX+((4,4,'push.w {r8, r9, r10, r11}'),(8,2,'sub sp, #0x4'),
     (10,4,'vpush {d8, d9, d10, d11, d12, d13, d14, d15}'),(14,4,'sub.w sp, sp, #0x250')),
 'sizez':_PREFIX+((4,4,'push.w {r7, r8, r9, r10, r11}'),
     (8,4,'vpush {d8, d9, d10, d11, d12, d13, d14, d15}'),(12,2,'sub sp, #0x160'))}

# MOVW/MOVT instruction offsets/registers; intervening transport is NOT proved
# here. PC rows bind offset/width/register but derive the actual mapped pool,
# permitting independent pool placement without accepting an untyped value.
_MOV={
 ('speed2',ROOT):((246,254,'r8'),(344,358,'r0'),(612,620,'r8'),(680,684,'r8'),(1118,1124,'r8'),
     (1752,1770,'r8'),(3708,3714,'r8'),(3728,3736,'r8'),(4304,4308,'r2'),(4710,4714,'r8'),
     (4726,4730,'r8'),(5282,5288,'r8'),(5302,5310,'r2'),(5338,5342,'r8'),(5362,5370,'r2')),
 ('speed2',VALIDATOR):((16,22,'r3'),(248,256,'r3'),(790,812,'r12'),(1868,1876,'r5'),(1892,1900,'r3'),
     (2024,2032,'r6'),(2058,2066,'r4'),(2120,2128,'r3'),(2142,2146,'r0'),(2174,2178,'r3'),
     (2522,2530,'r3'),(2630,2634,'r0'),(2658,2666,'r5'),(2866,2870,'r2'),(3000,3004,'r0'),
     (3076,3080,'r3'),(5426,5430,'r0'),(5452,5460,'r2'),(5524,5528,'r0')),
 ('sizes',ROOT):((134,138,'r4'),(506,522,'r2'),(532,536,'r4'),(1458,1466,'r4'),(1796,1800,'r2'),
     (1862,1868,'r8'),(2282,2292,'r0'),(2522,2530,'lr'),(2668,2672,'r0'),(2974,2978,'r4'),
     (2994,3002,'r1'),(3046,3050,'r0'),(3098,3110,'r2'),(3160,3164,'r0'),(3236,3244,'r1'),
     (3286,3290,'r0'),(3338,3346,'r4'),(3416,3426,'r8'),(3450,3454,'r4')),
 ('sizes',VALIDATOR):((20,28,'r2'),(282,298,'r2'),(388,396,'r1'),(570,574,'r1'),(626,648,'r4'),
     (824,828,'r0'),(858,862,'r1'),(990,1000,'r1'),(1028,1032,'r0'),(1056,1064,'r1'),
     (2322,2326,'r1'),(3074,3078,'r0'),(3100,3108,'r2'),(3172,3176,'r0'))}
_PC={
 ROOT:((158,4,'r8'),(752,4,'r10'),(1050,4,'r1'),(1106,4,'r10'),(1528,4,'r3'),(1574,4,'r11'),(3816,4,'r8')),
 VALIDATOR:((18,4,'r4'),(242,4,'r4'),(454,4,'r1'),(656,4,'r1'),(720,4,'r0'),(776,4,'r10'),
     (1182,4,'r1'),(1260,4,'r0'),(1344,4,'r2'),(2760,4,'r1'),(3510,4,'r0'),(3532,4,'r2'),
     (3586,4,'r0'),(4264,2,'r0'),(4284,2,'r1'),(4502,2,'r1'),(4588,2,'r1'),(4642,2,'r1'),
     (4688,2,'r1'),(4740,2,'r1'),(4794,2,'r1'),(4802,2,'r1'),(4888,2,'r1')),
 VISITOR:((32,4,'r4'),(342,4,'r4'),(3748,4,'r9'),(4656,4,'r6'),(5036,4,'r0'),(5526,4,'r1'),
     (5610,4,'r3'),(6052,4,'r6'),(6666,2,'r2'),(6762,2,'r1'),(7014,2,'r2'))}


def _parent(objects,role,frame):
    candidates={e[0]&~1 for n,e in objects.elf.functions.items() if canonical_symbol(n)==role}
    require(len(candidates)==1,'loader static parent is missing or ambiguous')
    start=next(iter(candidates));extent,actual,name=typed_function(objects,start,role)
    require(actual==frame,'loader/static parent has another current frame')
    return name,start,extent


def _point(objects,name,rows,at,width):
    row=rows.get(at);mapping=mapping_ranges(objects.elf,name)
    require(row is not None and row.address==at and len(row.raw)==width and
        any(k=='t' and a<=at<at+width<=b for a,b,k in mapping) and
        objects.read_only(at,width)==row.raw,'loader producer current bytes/mapping differ')
    return row


def _static_points(objects,rows,mode,B):
    result=[];parents={ROOT:_FRAMES[mode][0],VALIDATOR:_FRAMES[mode][1]}
    if mode=='sizez':parents[VISITOR]=1608
    for role,frame in parents.items():
        name,start,extent=_parent(objects,role,frame)
        if mode!='sizez':
            for lo,hi,register in _MOV[mode,role]:
                require(0<=lo<hi and hi+4<=extent,'loader MOV static points exceed parent')
                rd=14 if register=='lr' else int(register[1:]);sites=[]
                for offset,operation,value in ((lo,'movw',B&0xffff),(hi,'movt',B>>16)):
                    row=_point(objects,name,rows,start+offset,4)
                    actual=row.operands.split('@',1)[0].strip()
                    m=re.fullmatch(re.escape(register)+r', #(0x[0-9a-f]+|[0-9]+)',actual)
                    encoded=struct.pack('<HH',(0xf240 if operation=='movw' else 0xf2c0)|((value>>12)&15)|(((value>>11)&1)<<10),
                        (((value>>8)&7)<<12)|(rd<<8)|(value&255))
                    require(row.mnemonic==operation and m is not None and int(m[1],0)==value and row.raw==encoded,
                        'loader static producer selects another Config or opcode')
                    sites.append(row.address)
                result.append(dict(parent=start,role=role,sites=tuple(sites),register=register,value=B,
                    kind='MOVW_MOVT_points',requires_intervening_and_consuming_transport=True))
        else:
            for offset,width,register in _PC[role]:
                row=_point(objects,name,rows,start+offset,width);actual=row.operands.split('@',1)[0].strip()
                m=re.fullmatch(re.escape(register)+r', \[pc, #(0x[0-9a-f]+|[0-9]+)\]',actual)
                require(m is not None and row.mnemonic==('ldr' if width==2 else 'ldr.w'),'loader static PC producer differs')
                immediate=int(m[1],0);rd=int(register[1:]);pool=((row.address+4)&~3)+immediate
                require(pool%4==0 and any(k=='d' and a<=pool<pool+4<=b for a,b,k in mapping_ranges(objects.elf,name)),
                    'loader static pool leaves its actual mapped parent data')
                require((width==2 and rd<=7 and immediate<=1020 and immediate%4==0 and
                    row.raw==struct.pack('<H',0x4800|(rd<<8)|(immediate//4))) or
                    (width==4 and immediate<=4095 and row.raw==struct.pack('<HH',0xf8df,(rd<<12)|immediate)),
                    'loader static PC opcode/register/displacement differs')
                require(struct.unpack('<I',objects.read_only(pool,4))[0]==B,'loader static pool selects another Config')
                result.append(dict(parent=start,role=role,sites=(row.address,),pool=pool,register=register,value=B,
                    kind='PC_literal_point',requires_consuming_transport=True))
    return tuple(result)


def _physical(objects,low,high,bottom):
    """Compare actual addresses BEFORE any separate Storage space is used."""
    require(all(type(v) is int for v in (low,high,bottom)) and
        0x20000000<=bottom<=low<high<=0x20080000 and low%8==high%8==0,
        'loader frame leaves its actual Reset-derived stack allocation')
    def interval(a,n,label):
        require(type(a) is int and type(n) is int and n>0 and 0<=a<a+n<=0x100000000,
            'loader physical '+label+' extent is invalid')
        require(not (a<high and low<a+n),'loader physical frame intersects '+label)
    for section in objects.elf.sections:
        require(len(section)==10 and all(type(v) is int for v in section),'untyped loader section metadata')
        if section[2]&2 and section[5]:interval(section[3],section[5],'allocated section')
    for segment in objects.elf.segments:
        require(len(segment)==8 and all(type(v) is int for v in segment),'untyped loader LOAD metadata')
        if segment[0]!=1:continue
        require(0<=segment[4]<=segment[5] and 0<=segment[1]<=segment[1]+segment[4]<=len(objects.elf.data) and
            0<=segment[3]<=segment[3]+segment[4]<=0x100000000,'loader LOAD file/memory extent differs')
        if segment[5]:interval(segment[2],segment[5],'LOAD memory')
    for name,a,n,info,index in objects.elf.symbol_entries:
        require(all(type(v) is int for v in (a,n,info,index)),'untyped loader symbol metadata')
        if info&15==1 and n and 0<=index<len(objects.elf.sections) and objects.elf.sections[index][2]&2:
            interval(a,n,'allocated OBJECT')


def first_boot_loader_boundary(objects,instructions,source_context):
    """Derive geometry and same-B static points; never return a mutable View."""
    history=first_boot_history(objects,instructions,source_context);p=history['preclaim'];startup=p['startup']
    mode=history['mode'];entry=startup['entry'];loader=entry['loader'];B=entry['initial']['address']
    sp=startup['copy']['initial_sp'];incoming=history['once_return_transport']['restored_SP'];frame=_FRAMES[mode][0]
    name,start,extent=_parent(objects,ROOT,frame)
    require(start==loader['function'] and frame==loader['frame'] and loader['incoming_r0']==B+14600 and
        history['once_return_transport']['restored_before_specialized_tail'] and
        incoming==sp-(88 if mode=='sizez' else 72),'loader loses its actual restored once tail/SP')
    _physical(objects,incoming-frame,sp,startup['copy']['stack_bottom'])
    def unexpected(*args):raise AssertionError('unexpected loader prologue dependency')
    prologue=_PROLOGUES[mode];length=prologue[-1][0]+prologue[-1][1]
    proof=_literal_body(objects,name,instructions,ROOT,mode+'_loader_prologue',
        dict(extent=length,instructions=prologue,objects=()),unexpected,unexpected,window=start)
    points=_static_points(objects,instructions,mode,B)
    task=p['task'];P=p['executor'];require(task[1]==48 and P[1]==8,'loader protects another task/executor extent')
    require(0x20000000<=B<B+14604<=0x20080000 and
        all(not (a<incoming and incoming-frame<a+n) for a,n in ((B,14604),task,P)),
        'loader physical root frame aliases live Config/task/executor')
    require(incoming<=P[0]<P[0]+8<=sp,'executor is not the retained actual cortex local')
    retained=('main','cortex_main','run')+(('raw_poll',) if mode=='sizez' else ())+('boot_poll',)
    constituent=tuple((k,history['protected_save_intervals'][k]) for k in retained)
    require(all(incoming<=a<b<=sp for k,(a,b) in constituent),'loader caller envelope loses an actual saved carrier')
    protected=(dict(kind='Boot_task',interval=(task[0],task[0]+48)),
        dict(kind='executor_P',interval=(P[0],P[0]+8)),dict(kind='retained_callers',interval=(incoming,sp)),
        dict(kind='taken_and_enclosing_padding',interval=(B+14600,B+14604)))
    return dict(phase='first_boot_loader_pending_consuming_phase_closure',mode=mode,history=history,
        loader_geometry=dict(function=start,extent=extent,entry_SP=incoming,frame=(incoming-frame,incoming),
            root_frame_bytes=frame,saves=(incoming-104,incoming),locals=(incoming-frame,incoming-104),
            LR_value=history['once_return_transport']['LR_value'],once_frame_retained_bytes=0),
        prologue=proof,static_producers=points,static_point_scope='authored current producers only; consuming transport/effects unclosed',
        physical_protected=protected,retained_save_constituents=constituent,
        protection_requires_complete_consuming_write_and_child_checks=True,
        enclosing_allocation=(B,14604),confined_slot_extent=14600,config_extent=14576,
        taken_state=dict(address=B+14600,width=1,value=1,source='same successful once transition'),
        initial_file_facts_not_live_reinitialization=True,initial_fact_expiry='first relevant decode/refill/publication mutation',
        consuming_phase_closure=False,unclosed_phases=('decode/finalize/canonical','complete refill/source loops',
            'validator source/children/Result/restore','fault/issuer/publication/normal return',
            'all parent/child ingress and code/data coverage','diagnostic default Formatter/sink/PANICKED activation graph'),
        mode_specific_consuming_variants_not_borrowed=True,requires_complete_same_B_loader_before_owner=True,
        requires_H8_consumed_input_closure=history['requires_H8_consumed_input_closure'],
        requires_M2_pre_init_source_reconciliation=history['requires_M2_pre_init_source_reconciliation'],
        source_qualification=history['source_qualification'],external_qualifications=history['external_qualifications'],
        exception_graph_limit=history['exception_graph_limit'],exception_absence_established=False,
        hardware_qualified=False,existing_conservative_frame_arithmetic_unchanged=True)
