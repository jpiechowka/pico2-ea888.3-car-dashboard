"""Private source/sequence interfaces for the selected validator invocation.

The caller must produce its context from the actual checksum/refill chain.
These internal facts are not an acquired Config, a validation result or a
normal-return proof. Complete child effects and contextual coverage remain
the responsibility of the enclosing validator-return consumer.
"""
import re
from dataclasses import replace
from config_entry import CONFIG,entry_object
from config_finite import _literal_body
from config_helpers import _helper_function
from config_refill_children import _readonly,child_coverage,definition_interface,period_interface
from config_regions import Storage,Region,Transfer,Argument,borrow,require_disjoint,require_writes,_control
from config_logger import logger_body
from config_roots import root_location
from config_stack import canonical_symbol,_typed_function_extent,transfer_family
from config_validator_traversal_contracts import TRAVERSAL,MATH,COLOR_CAPTURE,COLOR_CAPTURE_ROLE
import struct
from validation import require


# Semantic grouping follows visit_validation source order, not target struct
# declaration order. Error suffixes can be reached from several phases and
# retain their own first/valid/capture state instead of restarting a visitor.
SOURCE_PHASES=(
    ('version_catalogue_hold',0,88),('control_bindings',88,304),
    ('early_first_Result_restore',304,402),('control_edges_reachability',402,648),
    ('dashboard_acknowledgement',648,708),('history',708,772),
    ('LED_pair',772,928),('first_None_diagnostic_and_literals',928,964),
    ('views_and_boot_view',964,1178),('widget_geometry',1178,1630),
    ('widget_labels_colors',1630,2056),('widget_gauge',2056,2308),
    ('widget_advanced',2308,2452),('widget_bands',2452,2696),
    ('widget_warning',2696,3424),('shared_widget_first_errors',3424,3522),
    ('warning_page',3522,3552),('sources',3552,4008),
    ('shared_errors_and_fuel',4008,4988),('typed_literals',4988,5048))

# Both true iterations and backwards shared error suffixes have explicit
# source continuations. A backwards address alone is not a source loop proof.
# (site,target,interface); offsets are relative to the typed parent.
ITERATIONS=(
    (0xee,0xd6,'two tap/hold actions'),(0xfa,0xaa,'four buttons'),
    (0x118,0x7a,'three pages'),(0x11c,0x7a,'three pages'),
    (0x122,0x7a,'three pages'),(0x128,0x7a,'three pages'),
    (0x218,0x1f2,'two destination actions'),(0x21e,0x1e2,'four destination buttons'),
    (0x224,0x1b8,'three destination pages'),(0x24c,0x23c,'three edge indices'),
    (0x254,0x23c,'three edge indices'),(0x258,0x22a,'three closure passes'),
    (0x27a,0x26a,'three reachability pages'),(0x2a4,0x28a,'four acknowledgement bindings'),
    (0x2b0,0x2a2,'two acknowledgement actions per binding'),(0x2e6,0x2c8,'eight history channels'),
    (0x39a,0x366,'three copied LED records: disabled/normal/warning'),
    (0x412,0x404,'prior initialized View prefix'),(0x436,0x3e4,'one initialized View'),
    (0x4ca,0x4b8,'boot View lookup in initialized View prefix'),
    (0x53a,0x52a,'Widget View lookup'),(0x55a,0x54e,'count matching initialized Views'),
    (0x5cc,0x5ba,'prior initialized Widget prefix'),(0x63c,0x628,'bounded placement columns'),
    (0x640,0x61c,'bounded placement rows'),(0x7d4,0x734,'eleven constructed ColorRoles'),
    (0xa84,0x9a2,'paired current Widget bands, at most three'),
    (0xad8,0xac2,'four warning acknowledgement bindings'),(0xae2,0xad6,'bounded warning binding search'),
    (0xd40,0x508,'eight initialized Widgets with stride220'),
    (0xfa6,0xdf0,'eight paired Channel and initialized Source records'))
SUFFIXES=(
    (0x172,0x146),(0x190,0x146),(0x280,0x182),(0x286,0x182),
    (0x442,0x182),(0x47c,0x18c),(0x48e,0x18c),(0x498,0x188),
    (0xd54,0x48a),(0xd5c,0x482),(0xd7e,0x18c),(0xd8a,0x18c),
    (0xfc2,0x18c),(0xfee,0xd6a),(0x1002,0xd6a),(0x1020,0x18c),
    (0x103a,0xd7c),(0x1046,0xd6a),(0x1054,0xd86),(0x105a,0xd86),
    (0x1086,0xd6a),(0x1092,0xd6a),(0x1102,0x18c),(0x1136,0x18c),
    (0x1188,0x146),(0x1192,0x15a),(0x1310,0x18c),(0x1322,0x12fa),
    (0x1368,0x1148))
TABLES={272:('rotation',tuple(range(4))),
    1856:('constructed ColorRole subset',(2,3,4,5,6,8,9,10,11,12,13)),
    3090:('typed LimitMode',tuple(range(5))),3690:('guarded Access tag minus one',tuple(range(4)))}
TERMINAL_PADDING={942:(938,'pending_diagnostic_cut'),3750:(3748,'branch'),4914:(4912,'branch'),4986:(4982,'pending_diagnostic_cut')}


def _source_inputs(source,context):
    """Consume actual returned source views; no capacity-as-initialization."""
    prepared=source['widget']['prepared'];widgets=source['widget']['iterations'];sources=source['iterations']
    B=context.config.start;phase=context.phase;owner=context.config.storage
    require((owner.extent,context.config.offset,context.config.extent,context.config.writable)==
            (14604,0,14576,False),'validator input loses its readonly complete typed payload')
    require(prepared['config'].start==B and prepared['config'].storage.identity==owner.identity and
            prepared['config'].storage.space==owner.space,'validator borrow changes actual refill owner')
    view=prepared['view'];require((view['count'],view['element_offset'],view['name_capacity'])==(1,248,14),
            'validator views do not come from the completed initialized prefix')
    require(view['initialized_name'].view.start==B+252 and
            view['initialized_name'].length.lower==view['initialized_name'].length.upper==8,
            'validator View string loses its initialized length/payload pair')
    require(len(widgets)==len(sources)==8 and source['normal_exit_count']==8 and
            source['widget']['normal_exit_count']==8 and source['history_count']==8,
            'validator source counts widen into unused capacity or an incomplete source loop')
    widget_views=[];source_views=[]
    for i,(widget,item) in enumerate(zip(widgets,sources)):
        W=widget['widget'];S=item['source']
        require((widget['index'],W.start,W.extent,widget['initialized_capacity_elements'])==
                (i,B+444+220*i,220,i+1),'validator Widget loses paired initialized stride/index')
        require((item['index'],item['channel'],item['offset'],S.start,S.extent)==
                (i,i,32+22*i,B+32+22*i,22),'validator Source loses Channel/stride pairing')
        require(W.storage==prepared['config'].storage and S.storage==prepared['config'].storage,
                'validator traversal substitutes another source allocation')
        for key,offset,capacity in (('initialized_label',156,24),('initialized_warning',36,20)):
            text=widget[key]
            require(text.view.start==W.start+offset and text.view.extent==capacity and
                    text.length.lower==text.length.upper and 0<=text.length.lower<=capacity,
                    'validator string loses its initialized prefix and opaque capacity boundary')
        widget_views.append(borrow(owner,444+220*i,220,phase))
        source_views.append(borrow(owner,32+22*i,22,phase))
    # The complete refill proof establishes active field constructors, including
    # each source-paired enum. It does not initialize capacity slack or inactive
    # variants. These intervals describe what the visitor may borrow, not a new
    # mutable allocation or a proof that semantic validation succeeds.
    return dict(views=(borrow(owner,248,24,phase),),widgets=tuple(widget_views),sources=tuple(source_views),
        history=borrow(owner,14528,16,phase),led=borrow(owner,14544,26,phase),
        controls=borrow(owner,212,32,phase),warnings=borrow(owner,208,4,phase),
        warning_colors=borrow(owner,14524,4,phase),fuel=borrow(owner,0,4,phase),
        fuel_variant='EquivalentGasoline',fuel_physical_tag=2,
        initialized_counts=dict(views=1,widgets=8,sources=8),
        opaque=('unused vector/string capacity','inactive source/Widget/Fuel enum payload','padding'),
        source_semantically_valid_assumed=False)


def _object_consumer(objects,address,descriptor,B):
    kind=descriptor['kind']
    if kind=='config':
        require(address==B,'validator source phase loads another Config')
        return entry_object(objects,address,dict(kind='entry_static',role=CONFIG))
    if kind=='source_location':root_location(objects,address,descriptor['role']);return ()
    require(kind in ('range','first_message'),'unreviewed validator typed readonly role')
    data=bytes.fromhex(descriptor['hex']);alignment=descriptor.get('alignment',1)
    require(_readonly(objects,address,len(data),alignment)==data,
            'validator source range/message bytes differ')
    return ()


def _math_body(objects,name,rows,label):
    require(label in MATH and re.fullmatch(r'OUTLINED_FUNCTION_[0-9]+',name),
            'validator math outline borrows an ordinary identity')
    start=objects.elf.functions[name][0]&~1;contract=MATH[label]
    def function(objects,target,role):
        if target==start:
            extent,raw=_typed_function_extent(objects,target,role)
            require(extent==contract['extent'] and objects.sizes.get(target,0)==0,
                    'validator scalar outline changes complete extent/frame')
            return extent,0,raw
        return _helper_function(objects,target,role)
    def unexpected(*args):raise AssertionError('scalar validator outline gains memory/callee effect')
    proof=_literal_body(objects,name,rows,'@outline:'+label,label,contract,unexpected,unexpected,
                        function_consumer=function)
    require(not proof['ordinary_edges'] and not proof['indirect_sites'],
            'scalar validator outline gains another transfer')
    # The closed sequences above contain only register/FP comparisons, shifts,
    # condition transfer and BX LR. They neither read nor write memory, change
    # SP, nor change r4-r11/d8-d15. Flags are consumed at the exact parent site.
    proof.update(normal_preserves=tuple('r'+str(i) for i in range(4,12))+('sp','d8-d15'),
        reads=(),writes=(),clobbers=(('r1',) if label=='finite_shift_r1' else
            ('r0',) if label in ('f32_exponent','finite_bits_r0','finite_shift_r0') else ())+('flags',),
        condition_result='exact APSR/fpscr relation in the matched parent source phase')
    return proof


def _complete_sequence(objects,name,instructions,B,outline_consumer):
    """Exact current sequence; semantic/child/coverage composition is separate."""
    rows={a:replace(r,operands=r.operands.split('@',1)[0].split('<',1)[0].strip())
          for a,r in instructions.items()}
    return _literal_body(objects,name,rows,canonical_symbol(name),'typed validator traversal',TRAVERSAL,
        lambda o,a,d:_object_consumer(o,a,d,B),outline_consumer,function_consumer=_helper_function)


def _child_allocation(context,extent,label):
    require(type(extent) is int and extent>0 and extent%4==0,'invalid complete validator child frame')
    q=context.frame
    child=Storage(q.space,q.identity+':'+label,q.start-extent,extent,4 if extent%8 else 8,q.phase,True)
    # Space-tag checks are in addition to physical allocated section/LOAD/
    # OBJECT protection in the enclosing consumer, never a replacement for it.
    for live in (q,context.caller,context.config.storage)+context.protected:
        require_disjoint((child,live))
    return child


def _visitor_guards(context):
    q=context.frame
    return context.protected+tuple(Storage(q.space,q.identity+':'+label,q.start+lo,n,1,q.phase,False)
        for label,lo,n in (('Result spill',60,4),('first error',76,12),('captured first pointer',88,4),('visitor valid',95,1)))


def _color_capture(objects,name,instructions,parent,context,inputs):
    require(canonical_symbol(name)==COLOR_CAPTURE_ROLE,'ColorRole first-error closure changes raw context')
    def none(*args):raise AssertionError('ColorRole capture gains a child/object/indirect effect')
    proof=_literal_body(objects,name,instructions,COLOR_CAPTURE_ROLE,'current Widget ColorRole capture',
                        COLOR_CAPTURE,none,none,function_consumer=_helper_function)
    require(not proof['ordinary_edges'] and not proof['indirect_sites'],'ColorRole capture gains another transfer')
    q=context.frame;phase=q.phase
    first=borrow(q,76,12,phase,writable=True);valid=borrow(q,95,1,phase,writable=True)
    protected=context.protected+tuple(Storage(q.space,'Color capture retained '+str(o),q.start+o,n,1,phase,False)
                                     for o,n in ((60,4),(68,4),(88,4),(112,12),(180,4)))
    writes=((valid,0,1),(first,0,2),(first,2,2),(first,4,1),(first,5,1),
            (first,6,1),(first,7,1),(first,8,2),(first,10,1))
    require_writes((first,valid),writes,protected)
    # Exact parent phase stores Q112=Q95, Q116=Q180, Q120=Q88.
    # Q180's current Widget pointer and Q88's first pointer are read through
    # those actual reference cells, never supplied as a public capture record.
    require(inputs['initialized_counts']['widgets']==8,'Color capture reads outside initialized Widgets')
    fields=tuple((borrow(context.config.storage,W.offset+212,2,phase),
                  borrow(context.config.storage,W.offset+217,2,phase)) for W in inputs['widgets'])
    active=_child_allocation(context,16,'current Widget Color capture')
    return dict(proof=proof,possible_writes=writes,source_fields=fields,owned_frame=active,
        reference_cells=((112,95),(116,180),(120,88)),current_Widget_cell=180,first_cell=88,
        source_ColorRole_domain=(2,3,4,5,6,8,9,10,11,12,13,14),
        first_Some_tag=1,code=11,Field_base=42,inactive_or_padding=((11,12),),
        normal_preserves=tuple('r'+str(i) for i in range(4,12))+('sp','d8-d15'))


def _led_copies(objects,instructions,parent,context):
    """Only the four reviewed LED staging copies, not a generic copy API."""
    q=context.frame;guards=_visitor_guards(context);results={};children=[]
    edges={offset:(target,role) for offset,operation,target,role,frame in parent['ordinary_edges']}
    operations=((804,'Config',14544,181,10),(822,'Config',14554,193,10),
                (836,'Q',180,121,11),(848,'Q',192,133,11))
    for offset,source_kind,source_offset,destination_offset,count in operations:
        require(offset in edges and edges[offset][1]=='__aeabi_memcpy','LED staging loses its actual copy call')
        target=edges[offset][0];chain=[]
        for ordinal,role in enumerate(('__aeabi_memcpy','_RNvNtCs_17compiler_builtins3mem6memcpy')):
            extent,frame,name=_helper_function(objects,target,role)
            body=logger_body(objects,name,instructions)
            require((body['extent'],body['frame'])==(extent,frame) and frame==(8 if ordinal==0 else 72),
                    'LED copy loses the complete wrapper/builtin frame')
            active=_child_allocation(context,frame,'LED copy '+str(ordinal))
            source=borrow(context.config.storage,source_offset,count,q.phase) if source_kind=='Config' else borrow(q,source_offset,count,q.phase)
            destination=borrow(q,destination_offset,count,q.phase,writable=True)
            require(source.start+count<=destination.start or destination.start+count<=source.start,
                    'LED copy aliases its still-live source')
            live=Storage(source.storage.space,'LED copy live source',source.start,count,1,q.phase,False)
            saves=Storage(active.space,active.identity+':complete writes',active.start,active.extent,4,q.phase,False)
            require_writes((destination,),((destination,0,count),),guards+(live,saves))
            if ordinal==0:
                require(len(body['ordinary_edges'])==1 and body['ordinary_edges'][0][3]==
                    '_RNvNtCs_17compiler_builtins3mem6memcpy' and not body['indirect_sites'],
                    'LED copy wrapper loses its restored tail')
                target=body['ordinary_edges'][0][2]
            else:require(not body['ordinary_edges'] and not body['indirect_sites'],'LED copy builtin gains a callee')
            # Both fixed bodies are integer-only and preserve every original
            # callee-save/FPU carrier needed by the visitor's later phases.
            require(not any(r.mnemonic.startswith('v') for at,r in instructions.items()
                            if body['function']<=at<body['function']+body['extent']),
                    'LED copy changes an original floating carrier')
            chain.append(body)
        results[parent['function']+offset]=dict(source=source,destination=destination,count=count,
            bodies=tuple(chain),physical_activation_paths=((8,),(72,)),
            normal_preserves=tuple('r'+str(i) for i in range(4,12))+('sp','d8-d15'),
            possible_writes=((destination,0,count),),copied_padding_stays_opaque=True,
            source_epoch='normal/warning Pattern snapshots before the three-record validation loop')
        children.extend(chain)
    return results


def _source_catalogue(objects,instructions,parent,context,source):
    """Close actual Definition/period calls with paired initialized Sources."""
    edges={offset:(target,role) for offset,operation,target,role,frame in parent['ordinary_edges']}
    q=context.frame;guards=_visitor_guards(context);results={}
    for offset,label in ((0xe44,'definition'),(0xf3e,'period')):
        target,role=edges[offset];extent,frame,name=_helper_function(objects,target,role)
        proof=child_coverage(objects,name,instructions,(target,'validator source '+label,parent['function']+offset))
        require((proof['extent'],proof['frame'])==(extent,frame),'validator Source child changes its current ABI')
        active=_child_allocation(context,frame,'Source '+label)
        if label=='definition':
            domain=frozenset(item['id'] for item in source['iterations'])
            argument=Argument('DefinitionId',None,0,8,domain)
            output=borrow(q,112,36,q.phase,writable=True)
            result=definition_interface(objects,instructions,proof,argument,output,active,guards)
            alternatives={item['id']:item for item in result['outcomes']}
            require(set(alternatives)==domain,'validator catalogue omits an actual initialized DefinitionId')
            for item in source['iterations']:
                expected=item['typed_definition'];actual=alternatives[item['id']]
                require((actual['variant'],actual['channel'],actual['access']['variant'])==
                        (expected['variant'],item['channel'],expected['access']['variant']),
                        'validator catalogue loses the same-Source Definition/Channel/Access pairing')
        else:
            result=period_interface(objects,instructions,proof,Argument('Channel',None,0,8,frozenset(range(8))),active,guards)
            require(tuple(item['channel'] for item in result['outcomes'])==tuple(range(8)),
                    'validator period uses another Channel iteration domain')
        require(not any(r.mnemonic.startswith('v') for at,r in instructions.items()
                        if target<=at<target+extent),'validator Source child changes live floating carriers')
        results[parent['function']+offset]=result
    return results


def _phase_relations(objects,parent,context,source,inputs,controls,normal,children,instructions,copies,catalogue,color_capture):
    """Closed induction for these fixed typed source/transport phases.

    The complete sequence fixes updates, comparisons, source stores and all
    incoming edges. The source producer fixes initialized prefixes. Child
    relations fix every intervening write/clobber/return. The finite unions
    below therefore do not execute candidate scalars or infer new opcodes.
    Every guarded error is retained; a failing guard cannot authorize a later
    inactive read or the successful fuel result.
    """
    q=context.frame;owner=context.config.storage;B=owner.start;phase=q.phase
    require(inputs['initialized_counts']==dict(views=1,widgets=8,sources=8) and
            inputs['fuel_variant']=='EquivalentGasoline' and inputs['fuel_physical_tag']==2,
            'validator source induction loses the actual initialized prefixes/active Fuel constructor')
    require(controls['loops']==dict(pages=(0,1,2),buttons=(0,1,2,3),tap_hold=((0,False),(2,True)),rotation=(0,1,2,3)),
            'validator source controls lose their paired enum/index domains')
    expected={68:'range_u16',734:'range_u16',900:'range_u16',914:'range_u8',1060:'text_valid',1648:'text_valid',
        1996:'contrasts',2014:'contrasts',2032:'contrasts',2080:'channel_domain',2368:'range_u16',2388:'range_u16',
        2452:'bands',2590:'bands',2644:'bands',2682:'contrasts',2712:'range_u16',3332:'range_u16',3352:'range_u16',
        3370:'text_valid',3522:'range_u16',3548:'contrasts',3930:'range_u16',3992:'range_u16'}
    # Three exponent calls are exact pure-outline contracts, rather than bool
    # helpers: returned CMP flags feed the immediately fixed parent BHI. The
    # intervening domain call returns s0/s1 and exponent leaves both intact.
    require({item['site']-parent['function']:item['family'] for item in normal}==expected,
            'validator source phase omits or adds a normal caller/receiver family')
    for item in normal:
        require(item['proof']['external_writes']==() and 'd8-d15' in item['proof']['normal_preserves'],
                'validator child changes a source object or original FP save carrier')
    for offset in (0x804,0x828,0x968):
        child=children.get(parent['function']+offset)
        require(child is not None and child['role']=='@outline:f32_exponent' and not child['stops_at_cut'],
                'validator float/flags source pair loses its exact pure child')
    start=parent['function'];local=controls['pointer']['local'];pointer=controls['pointer']
    def point(offset,operation,operand):
        row=instructions.get(start+offset)
        require(row is not None and (row.mnemonic,row.operands)==(operation,operand),
                'validator phase anchor loses its exact current register/object transport')
        return (start+offset,operation,operand)
    def writers(lo,hi,begin,end):
        return tuple((at-start,a,b) for at,a,b in local['local_ranges']
                     if begin<=at-start<end and a<hi and lo<b)
    edge_reset=point(1256,'strd','r0, r0, [sp, #104]')
    require(writers(104,112,1178,3522)==((1256,104,112),),
            'occupied epoch has another local writer or lost its complete zero initialization')
    occupied=tuple((at-start,family,view.start,lo,hi) for at,family,view,lo,hi in pointer['pointer_writes']
                   if view.start<q.start+112 and q.start+104<view.start+hi)
    require(tuple((at,family) for at,family,*rest in occupied)==((530,'edges'),(1626,'occupied')),
            'edges/occupied epochs gain another pointer writer')
    widget_pointer=point(1438,'strd','r3, r2, [sp, #180]')
    require(writers(180,184,1178,3522)==((1438,180,188),),
            'current Widget pointer epoch is overwritten before the Source phase')
    # Constructed closure references are actual stack cells. Their consumers
    # read Q180 -> current Widget and Q88 -> first; they do not receive either
    # pointer from an exported description or from unrelated scalar data.
    color_references=(point(1808,'add','r0, sp, #0x58'),point(1810,'str','r0, [sp, #0x78]'),
        point(1812,'add','r0, sp, #0xb4'),point(1814,'str','r0, [sp, #0x74]'),
        point(1816,'sub.w','r0, r7, #0xd9'),point(1820,'str','r0, [sp, #0x70]'))
    require(color_capture['reference_cells']==((112,95),(116,180),(120,88)) and
            color_capture['current_Widget_cell']==180 and color_capture['first_cell']==88,
            'ColorRole callback loses its actual current Widget/first/valid references')
    # Table indices are typed source transports, not arbitrary byte-width
    # values. Complete current sequences and every child above retain these
    # cells/registers; the finite predecessor guards retain their source enum
    # domains even though candidate scalar values are not executed here.
    table_transports=dict(
        rotation=(point(252,'ldr','r6, [sp, #0x40]'),point(268,'ldrb','r0, [r6]'),
                  point(270,'ldr','r2, [sp, #0x38]')),
        ColorRole=(point(110,'add','r0, sp, #0xc0'),point(112,'add.w','r9, r0, #0x8'),
            point(1822,'movs','r0, #0xd'),point(1824,'strb.w','r0, [sp, #0xd2]'),
            point(1828,'movw','r0, #0xc0b'),point(1832,'strh.w','r0, [sp, #0xd0]'),
            point(1844,'cmp','r4, #0xb'),point(1848,'ldrb.w','r5, [r9, r4]')),
        LimitMode=(point(2788,'ldr','r0, [sp, #0x44]'),point(2790,'ldrb.w','r2, [r0, #0x54]'),
            point(2794,'cmp','r2, #0x2'),point(2948,'cmp','r2, #0x2')),
        Access=(point(3572,'cmp','r0, #0x8'),point(3582,'ldrb.w','r1, [r9, r0]'),
            point(3594,'str','r1, [sp, #0x48]'),point(3606,'ldrb.w','r11, [r0, #0x2a]'),
            point(3650,'mov','r1, r6'),point(3672,'ldrb.w','r0, [sp, #0x88]'),
            point(3676,'cmp','r11, r0'),point(3682,'sub.w','r0, r11, #0x1'),point(3686,'cmp','r0, #0x3')))
    # Every normal non-save write is an existing checked owned-write fact.
    # Their physical intervals must not modify the source Config or its active
    # Fuel tag while this readonly visitor executes. Child save writes are
    # separately below Q; copied padding remains opaque throughout.
    writes=list(local['local_writes'])+[(view,lo,hi-lo) for at,family,view,lo,hi in pointer['pointer_writes']]
    writes.extend(controls['children']['control_error_capture']['writes'])
    writes.extend(color_capture['possible_writes'])
    writes.extend(controls['pointer']['local']['boundary']['error_copy']['possible_writes'])
    for value in copies.values():writes.extend(value['possible_writes'])
    for value in catalogue.values():writes.extend(value['possible_writes']);writes.extend(value.get('local_writes',()))
    for label in ('first_capture','band_capture','placement_rect'):
        writes.extend(pointer['children'][label]['writes'])
    # The sole direct VSTR is the checked successful R+4 output.
    writes.append((context.result,4,4))
    for view,offset,n in writes:
        lo=view.start+offset;hi=lo+n
        require(hi<=B or B+14576<=lo,'normal validator child/parent mutates its readonly source Config')
    # The source terminal constructor is part of the privately recomputed
    # refill proof. Its r1=2 survives only loads/stores until the exact B store;
    # no image initial-value inference is used after refill mutation.
    source_start=source['proof']['function'];fuel_constructor=[]
    for offset,operation,operand in ((244,'movs','r1, #0x2'),(246,'ldr','r0, [pc, #0x5c]'),
                                    (248,'strd','r6, r5, [r10, #216]'),
                                    (252,'strd','r4, r3, [r10, #224]'),(256,'str.w','r1, [r10]')):
        row=instructions.get(source_start+offset)
        if offset==246:
            require(row is not None and row.mnemonic=='ldr' and row.operands.startswith('r0, [pc, #'),
                    'Fuel constructor loses its nonclobbering typed pool load')
        else:require(row is not None and (row.mnemonic,row.operands)==(operation,operand),
                     'actual source Fuel constructor loses r1=2 or the same-B store')
        fuel_constructor.append((row.address,row.mnemonic,row.operands))
    require(source['outgoing']['r10']==B,'Fuel constructor has another source owner')
    fuel_guard=(point(4266,'ldr','r3, [r0]'),point(4268,'subs','r1, r3, #0x2'),
                point(4276,'cmp','r1, #0x0'),point(4278,'beq',hex(start+4420)))
    float_source=point(4420,'vldr','s0, [pc, #612]')
    slot=((start+4420+4)&~3)+612
    require(struct.unpack('<I',objects.read_only(slot,4))[0]==0x416b3333,
            'EquivalentGasoline path loses actual14.7f32 source bits')
    ok_pair=(point(4482,'ldr','r5, [sp, #0x3c]'),point(4486,'cmp','r0, #0x0'),
             point(4488,'beq.w',hex(start+326)),point(4492,'movs','r4, #0x2'),
             point(4494,'vstr','s0, [r5, #4]'),point(4498,'b.w',hex(start+346)))
    # Object epochs are physical subintervals of the SAME Q allocation. Never
    # retag a live buffer to make another owner appear disjoint. Each new epoch
    # below begins after the old phase's last matched read or exact zeroing.
    epochs=(
        dict(interval=(104,107),owner='edges[ControlPage]',entry=402,exit=648,
             domain=(0,1,2),invariant='source page pairs; four bindings; two actions; three monotone closure passes'),
        dict(interval=(104,112),owner='occupied[View]',entry=1256,exit=3522,
             domain=tuple(range(len(inputs['views']))),
             invariant='STRD zero after edges retired; only matched Some rect cells and successful same-View lookup'),
        dict(interval=(120,123),owner='ControlPage::ALL',entry=560,exit=648,
             domain=(0,1,2),invariant='after copied binding staging retires'),
        dict(interval=(112,148),owner='LED tuple snapshots',entry=772,exit=928,
             invariant='disabled record then exact normal/warning snapshots; kind guard before active Pattern fields'),
        dict(interval=(112,122),owner='Placement rect Result',entry=1510,exit=1630,
             invariant='tag1 guard before any rect payload; tag0 reaches first-error constructor'),
        dict(interval=(112,124),owner='Color capture references',entry=1796,exit=2056,
             invariant='Q112=Q95, Q116=Q180 current Widget cell, Q120=Q88; no capture-cell substitution'),
        dict(interval=(180,184),owner='current Widget pointer',entry=1438,exit=3424,
             invariant='same Widget at Q68, packed source fields and Band/Color capture source; not dynamic range storage'),
        dict(interval=(180,186),owner='dynamic Source ranges',entry=3890,exit=4008,
             invariant='Widget pointer retired; same Channel period/cap; u16 endpoints, exhausted0, byte185 opaque'),
        dict(interval=(112,148),owner='Source Definition output',entry=3552,exit=4008,
             invariant='all Widget/LED/rect/color captures retired; paired source DefinitionId and Access result'))
    # Only initialized source prefixes feed pointer arithmetic. View iterates
    # 24*n and Widget 220*n, while all prior-prefix searches stop at current i.
    views=tuple((V.start,V.extent) for V in inputs['views'])
    widgets=tuple((W.start,W.extent) for W in inputs['widgets'])
    sources=tuple((S.start,S.extent) for S in inputs['sources'])
    require(views==((B+248,24),) and widgets==tuple((B+444+220*i,220) for i in range(8)) and
            sources==tuple((B+32+22*i,22) for i in range(8)),
            'validator source loop end/cursor is not the same initialized vector prefix')
    traversal=dict(views=dict(base=B+248,stride=24,count=1,prior_prefix_indices=tuple(range(1))),
        widgets=dict(base=B+444,stride=220,count=8,prior_prefix_bounds=tuple((0,i) for i in range(8)),
            view_indices=(0,),placement_rows=(0,1),placement_columns=(0,1,2,3),
            rect_payload_requires_Some1=True,missing_or_duplicate_view_stays_error=True),
        bands=dict(receiver_offsets=tuple(W.offset+88 for W in inputs['widgets']),stride=16,
            count_domain=(0,1,2,3),previous_indices=((1,0),(2,1)),
            bound_sites=(parent['function']+0xa26,parent['function']+0xa5a),
            distinct_Locations=('prior_band','current_band')),
        colors=dict(constructed_subset=(2,3,4,5,6,8,9,10,11,12,13),count=11,
            scratch=(200,211),all_same_Widget_scalar_pairs=True),
        sources=dict(base=B+32,stride=22,channels=tuple(range(8)),
            DefinitionId_pairs=tuple((i,item['id']) for i,item in enumerate(source['iterations'])),
            poll_payload_read_only_for_EveryMs=True,source_Off_payload_opaque=True,
            Access_table_domain=(1,2,3,4),route_payload_read_only_for_Physical=True),
        fuel=dict(active_constructor='EquivalentGasoline',representation_tag=2,active_bytes=(0,4),
            inactive_payload=(4,32),normal_s0_bits=0x416b3333,
            scalar_only_other_typed_variants_kept_in_body=True,
            Ok_requires_actual_valid_flag_and_finite_stoichiometric_guards=True))
    # All full-body first-error constructors write only declared local ranges,
    # pointer families, or the checked capture children. None2 reaches its
    # pending expect diagnostic. Valid=false retains the completed Some error
    # and reloads the original R from Q60; early errors keep original r5.
    first=dict(initial_None=2,Some_tags=(0,1),error_field_bytes=(76,88),valid=95,capture=(88,92),spill=(60,64),
        early_R='r5 before Q60 store',late_R='Q60 reload',
        shared_widget_Some1_and_root_None0_pairs_preserved=True,
        None2_is_diagnostic_not_Err=True,Err_payload_before_tag=True,Ok_s0_before_tag2=True)
    return dict(epochs=epochs,traversal=traversal,first=first,checked_table_transports=table_transports,
        checked_owned_write_intervals=tuple((v.start+o,v.start+o+n) for v,o,n in writes),
        checked_retirements=dict(edges_to_occupied=dict(zero=edge_reset,pointer_writers=occupied,
            old_domain=(0,1,2),new_domain=tuple(range(len(inputs['views'])))),
            Widget_to_dynamic_ranges=dict(pointer_store=widget_pointer,
                Widget_epoch_writers=writers(180,184,1178,3522),
                Source_epoch_writers=writers(180,186,3552,4008)),
            Color_reference_cells=color_references),
        checked_Fuel_to_Ok=dict(constructor=tuple(fuel_constructor),same_B=B,
            constructor_word=2,source_dispatch_guard=fuel_guard,subtracted_tag=2-2,
            selected_f32=(float_source,slot,0x416b3333),output_pair=ok_pair),
        initialized_prefix_counts=inputs['initialized_counts'],semantic_validation_success_assumed=False)


def _coverage(objects,name,instructions,proof,context,children):
    """Reconcile authored phases after their child/source effects are joined.

    `children` is internal output of complete child recognizers, keyed by the
    actual external site. It is not an input format and cannot be supplied to
    the public producer. Diagnostic frontiers stop this partial relation;
    neither machine nonreturn nor a continuation through them is inferred.
    """
    start=proof['function'];key=(start,'default checksum typed validator',context.frame.start)
    rows={a:r for a,r in instructions.items() if start<=a<start+5048}
    parent_edges={start+e[0]:e for e in proof['ordinary_edges']}
    require(set(children)==set(parent_edges),'complete validator omits a current child/diagnostic edge')
    backwards={a:(b,label) for a,b,label in ITERATIONS}
    backwards.update({a:(b,'typed first/Result or scalar-only finite join') for a,b in SUFFIXES})
    regions=[];all_backwards=set();tables=[];phase_at={}
    for ordinal,(label,lo,hi) in enumerate(SOURCE_PHASES):
        require(lo==(0 if ordinal==0 else SOURCE_PHASES[ordinal-1][2]) and lo<hi,
                'validator source phases overlap or leave an interval')
        for at in rows:
            if start+lo<=at<start+hi:phase_at[at]=ordinal
    require(SOURCE_PHASES[-1][2]==5048 and set(phase_at)==set(rows),
            'validator source phases omit current mapped instructions')
    data=[]
    for lo,hi,kind in TRAVERSAL['mapping']:
        if kind=='d':data.append((start+lo,objects.read_only(start+lo,hi-lo)))
    # These four exact pads belong to this validator family only. Generic
    # audit_coverage keeps its existing one-postlude policy unchanged.
    for ordinal,(label,lo,hi) in enumerate(SOURCE_PHASES):
        selected=tuple(sorted(a for a in rows if phase_at[a]==ordinal));transfers=[]
        for at in selected:
            row=rows[at];family,conditional=_control(row);stops=False
            if at-start in TERMINAL_PADDING:
                require(row.raw==b'\x00\xbf' and row.mnemonic=='nop' and not row.operands.strip(),
                        'validator terminal padding changes into code')
                continue
            if family is not None:
                operand=row.operands.split('@',1)[0].split('<',1)[0].strip()
                found=re.search(r'(?<![#\w])0x([0-9a-f]+)(?:\s|$)',operand)
                target=int(found[1],16) if found else None
                if at in parent_edges:
                    edge=parent_edges[at];child=children[at]
                    require(edge[2]==target and child['target']==target and child['role']==edge[3] and
                            type(child['stops_at_cut']) is bool,'validator child loses actual typed call/return binding')
                    kind='pending_diagnostic_cut' if child['stops_at_cut'] else 'call' if family=='bl' else 'tail'
                    transfers.append(Transfer(at,row.mnemonic,kind,target,child['interface'],edge[3]))
                    stops=kind in ('pending_diagnostic_cut','tail')
                elif family in ('tbb','tbh'):
                    offset=at-start;require(family=='tbb' and offset in TABLES,'validator gains an unsupported computed transfer')
                    role,domain=TABLES[offset];base=at+4
                    targets=tuple(base+2*objects.read_only(base+i,1)[0] for i in domain)
                    require(all(t in rows for t in targets),'typed validator table targets non-code')
                    transfers.append(Transfer(at,row.mnemonic,'table',targets,role));tables.append((at,role,domain,targets));stops=True
                elif family in ('b','cbz','cbnz'):
                    require(target in rows,'validator source continuation leaves current normal code')
                    if target<=at:
                        expected=backwards.get(at-start)
                        require(expected is not None and target==start+expected[0],
                                'validator backward edge loses its source invariant/typed suffix')
                        interface=expected[1];kind='loop';all_backwards.add(at-start)
                    else:interface='same invocation '+label;kind='branch'
                    transfers.append(Transfer(at,row.mnemonic,kind,target,interface));stops=family=='b' and not conditional
                else:raise AssertionError('validator gains an unclassified normal control transfer')
            following=at+len(row.raw)
            if not stops and following in rows and phase_at[following]!=ordinal:
                transfers.append(Transfer(at,row.mnemonic,'fallthrough',following,'preserved source/epoch boundary'))
        owned=[]
        for address,value in data:
            left=max(address,start+lo);right=min(address+len(value),start+hi)
            if left<right:owned.append((left,value[left-address:right-address]))
        # These pads follow exact pending cuts or unconditional branches.
        # The private audit excludes entry in this selected ordinary prefix.
        regions.append(Region(key,'data' if not selected else 'visitor',start+lo,start+hi,
                              selected,tuple(owned),tuple(transfers)))
    require(all_backwards==set(backwards),'validator omits an authored loop/shared-suffix edge')
    coverage=_audit_regions(objects,name,instructions,proof,tuple(regions),children)
    return dict(coverage=coverage,regions=tuple(regions),tables=tuple(tables),
                iterations=ITERATIONS,typed_shared_suffixes=SUFFIXES)


def _audit_regions(objects,name,instructions,proof,regions,children):
    """Closed validator-only coverage including its four terminal pads.

    Full _literal_body has already checked every instruction, mapped byte and
    typed pool. This additional reconciliation owns every byte exactly once,
    checks every actual control and cross-phase edge, and refuses executable
    fallthrough or direct/table/implicit-PC ingress into padding/data. It adds
    no generic padding exemption to the shared region verifier.
    """
    start=proof['function'];end=start+5048;rows={a:r for a,r in instructions.items() if start<=a<end}
    pads={start+a for a in TERMINAL_PADDING};machine={};falls={};owners={};data=set()
    for ordinal,region in enumerate(regions):
        require(region.context==regions[0].context and start<=region.start<region.end<=end,
                'validator region loses its actual invocation')
        points=set()
        for at in region.instructions:
            require(at in rows and region.start<=at<at+len(rows[at].raw)<=region.end,
                    'validator phase boundary splits a current instruction')
            covered=set(range(at,at+len(rows[at].raw)))
            require(not covered&set(owners),'validator phase code overlaps')
            for byte in covered:owners[byte]=ordinal
            points|=covered
        for at,value in region.data:
            covered=set(range(at,at+len(value)))
            require(bool(value) and region.start<=at<at+len(value)<=region.end and not covered&data and
                    objects.read_only(at,len(value))==value,'validator phase data ownership differs')
            data|=covered;points|=covered
        require(points==set(range(region.start,region.end)),'validator phase leaves an unowned byte')
        for edge in region.transfers:
            target=falls if edge.kind=='fallthrough' else machine
            require(edge.site in region.instructions and edge.operation==rows[edge.site].mnemonic and
                    edge.site not in target,'validator phase gains duplicate/unbound control')
            target[edge.site]=edge
    require(set(owners).isdisjoint(data) and set(owners)|data==set(range(start,end)),
            'validator current mapping has an uncovered/overlapping byte')
    direct_edges=[];consumed=set();crossings=set()
    for at,row in sorted(rows.items()):
        if at in pads:
            before,kind=TERMINAL_PADDING[at-start];previous=machine.get(start+before)
            require(previous is not None and previous.kind==kind and start+before+len(rows[start+before].raw)==at,
                    'validator padding has no exact terminal predecessor')
            require(at not in machine and at not in falls and at+2 in data,
                    'validator terminal padding gains a continuation')
            continue
        family,conditional=_control(row);edge=machine.get(at);stops=False
        if family is None:require(edge is None,'validator has a spurious machine control')
        else:
            require(edge is not None,'validator leaves an actual transfer unconsumed');consumed.add(at)
            operand=row.operands.split('@',1)[0].split('<',1)[0].strip()
            target=re.search(r'(?<![#\w])0x([0-9a-f]+)(?:\s|$)',operand)
            if family in ('b','bl','cbz','cbnz'):
                require(target is not None and edge.target==int(target[1],16),'validator direct edge changes target')
                direct_edges.append(edge.target)
                if at in children:
                    child=children[at]
                    require(child['target']==edge.target and child['role']==edge.role and
                            edge.kind==('pending_diagnostic_cut' if child['stops_at_cut'] else 'call' if family=='bl' else 'tail'),
                            'validator child effects do not match its live transfer')
                    stops=edge.kind in ('pending_diagnostic_cut','tail') and not conditional
                else:
                    require(edge.target in rows and edge.target not in pads and edge.role is None and
                            edge.kind in ('branch','loop'),'validator source branch enters data/padding/unbound code')
                    stops=family=='b' and not conditional
            elif family=='tbb':
                require(edge.kind=='table' and isinstance(edge.target,tuple) and bool(edge.target) and
                        all(t in rows and t not in pads for t in edge.target),'validator typed table enters data/padding')
                direct_edges.extend(edge.target);stops=True
            else:raise AssertionError('validator gains an implicit-PC/unknown normal transfer')
        next_at=at+len(row.raw);fall=falls.get(at)
        if stops:require(fall is None,'validator terminal edge has executable fallthrough')
        else:
            require(next_at in rows and next_at not in pads,
                    'validator executable fallthrough enters data/padding/end at '+hex(at-start))
            if owners[at]!=owners[next_at]:
                require(fall is not None and fall.target==next_at and fall.role is None,
                        'validator phase crossing loses its actual state continuation');crossings.add(at)
            else:require(fall is None,'validator gains a spurious phase fallthrough')
    require(not pads&set(direct_edges) and consumed==set(machine) and crossings==set(falls),
            'validator terminal padding is reachable or a control interface is unconsumed')
    for at,row in instructions.items():
        if start<=at<end:continue
        family=transfer_family(row.mnemonic.removesuffix('.w'))
        if family in ('b','bl','cbz','cbnz'):
            target=re.search(r'(?<![#\w])0x([0-9a-f]+)(?:\s|$)',row.operands.split('@',1)[0].strip())
            require(target is None or not start<int(target[1],16)<end,
                    'foreign direct ingress into a validator source phase')
    return dict(function=start,frame=320,instructions=len(rows),code_bytes=len(owners),data_bytes=len(data),
        regions=len(regions),machine_transfers=len(consumed),phase_fallthroughs=len(crossings),
        terminal_padding=tuple(sorted(pads)),all_current_bytes_owned_once=True)
