"""Private source/Views continuation for the closed decoded Widget group.

Literal bodies are structural witnesses. The source, paired loops, partial
completion, child effects, observer and protected-return joins are separately
required before the public pending boundary can be emitted. This module never
accepts a caller summary or creates an acquired mutable owner.
"""
import struct
import re
from dataclasses import dataclass,replace
from collections.abc import Mapping
from config_abstract import Span,SequenceInterface,sequence_interface,decoder_interface
from config_abstract_compare import InitializedPrefix
from config_construction import String5Interface,extend_string5_interface,VecInterface,extend_interface
from config_platform import reviewed_eav_policy,TARGET,BOOT_CONTEXT
from config_utf8 import UTF8Slice,from_utf8_interface
from config_widget_children import color_interface,warning_colors_interface,band_tail_clear_interface,label_copy_interface
from config_decode_effects import _object
from config_regions import Storage,borrow,require_writes,Argument
from config_refill_children import opaque_copy_interface,advanced_interface
from config_decoded_views import _first_boot_decoded_views_state,_views_return_profiles
from config_widget_children import _name,_outline,_function,widget_body,_CUSTOM
from config_widget_contracts import GRAMMARS,ROLES
from config_widget_effect_contracts import (LOCAL_STORES,WIDGET_STORES,BAND_STORES,INLINE_COPIES,
    FIXED_ALIAS_STORES,DECODER_CURSOR_STORES,VECTOR_COUNT_STORES,ZERO_BAND_STORES)
from config_finite import _literal_body
from config_helpers import HELPER_SEMANTIC_ROLES,helper_coverage
from config_headers import VISITOR
from config_logger import logger_body
from config_refill_children import REFILL_ROLES,child_coverage
from config_utf8 import UTF8,utf8_body
from config_validator_children import _normal_body
from config_validator_child_contracts import NORMAL_ROLES
from llvm_stack import mapping_ranges
from config_finite_returns import _closed_child_returns
from validation import require


def _parent(objects,rows,state):
    context=state.context;V=context.headers['visitor']['proof']['function'];B=context.owner.start
    name=_name(objects,VISITOR,V)
    def object_value(objects,value,descriptor):
        if descriptor==dict(kind='same_Config'):
            require(value==B,'Widget reload selects another private Config');return ()
        require(descriptor==dict(kind='printable_ascii_range',extent=3) and
            objects.read_only(value,3)==bytes((0,32,126)),'Widget printable ASCII source range differs')
        return ()
    proofs=tuple(_literal_body(objects,name,rows,VISITOR,'decoded_widgets_'+str(off),GRAMMARS['visitor_'+str(off)],
        object_value,_outline,window=V+off,function_consumer=_function) for off in (416,3516,6994))
    # The two words between selected code regions remain typed, owned data.
    # Earlier Views and current Widget reloads must name the same B. Never
    # treat the numerically equal words as interchangeable producer sites.
    require(any(k=='d' and lo<=V+3508<V+3516<=hi for lo,hi,k in mapping_ranges(objects.elf,name)),
        'Widget shared pool changes its current code/data ownership')
    config,ascii_range=struct.unpack('<II',objects.read_only(V+3508,8))
    require(config==B and ascii_range==proofs[0]['objects'][0],
        'Widget shared pool loses Views Config/printable source pairing')
    return proofs


def _children(objects,rows,state,parents):
    proofs=dict(state.children);pending=[edge for p in parents for edge in p['ordinary_edges']]
    while pending:
        offset,operation,target,role,frame=pending.pop();key=(target,role)
        if key in proofs:continue
        if role.startswith('@outline:'):
            names=[n for n,e in objects.elf.functions.items() if e[0]&~1==target]
            require(len(names)==1,'Widget outline target is missing or ambiguous')
            proof=_outline(objects,names[0],rows,role)
        elif role in (ROLES['color'],ROLES['warning_colors']):proof=widget_body(objects,_name(objects,role,target),rows)
        elif role==UTF8:
            proof=utf8_body(objects,rows);require(proof['function']==target,'Widget UTF8 uses another current child')
        elif role in HELPER_SEMANTIC_ROLES.values():
            proof=helper_coverage(objects,_name(objects,role,target),rows,(target,'decoded Widget scalar/construction'))
        elif role in REFILL_ROLES:
            require(tuple(REFILL_ROLES[role])==('advanced_for_channel_z',),'Widget child borrows an unrelated refill constructor')
            proof=child_coverage(objects,_name(objects,role,target),rows,(target,'decoded Widget initializer'))
        elif role in NORMAL_ROLES:
            require(NORMAL_ROLES[role]=='range_u8','Widget borrows an unrelated validator child')
            proof=_normal_body(objects,_name(objects,role,target),rows,'range_u8')
        elif role in ('__aeabi_memclr','__aeabi_memcpy4','__aeabi_memcpy','_RNvNtCs_17compiler_builtins3mem6memcpy'):
            proof=logger_body(objects,_name(objects,role,target),rows)
        else:require(False,'Widget group contains an unclassified child')
        require(proof['frame']==frame,'Widget child frame differs from current call edge')
        proofs[key]=proof;pending.extend(proof['ordinary_edges'])
    return proofs


def _current_group(objects,rows,source_context):
    description,state=_first_boot_decoded_views_state(objects,rows,source_context)
    parents=_parent(objects,rows,state);children=_children(objects,rows,state,parents)
    return description,state,parents,children


def _child_returns(rows,state,parents,children):
    profiles=_views_return_profiles()
    advanced='_RNvMs2_NtCs_16dashboard_config8advancedNtB5_15AdvancedDisplay11for_channel'
    ascii_range=next(role for role,family in NORMAL_ROLES.items() if family=='range_u8')
    profiles.update({
        'seq_u16':{4,14},'seq_bool':{4,14},'seq_f32':{0,40,46,56},
        'deserialize_u16':{0,4},'deserialize_seed_f32':{16,26},
        'restore_r11_low20':{0,4},'seq_decrement_tail_u16':set(),'seq_f32_setup_flags':set(),
        'vec_extend20':{0,14},'vec_extend24':{0,14},
        ROLES['color']:{4,14},ROLES['warning_colors']:{0,98},
        advanced:{0,4,8,92,94,98},ascii_range:{0,14,40,48},
        '__aeabi_memclr':{0,190,206},'__aeabi_memcpy4':{4,152},
        '@outline:advanced_copy_bands48':set(),
    })
    profiles.update({role:set() for role in _CUSTOM})
    nodes=_closed_child_returns(rows,parents,children,state.context.headers['visitor_frame'].start,profiles,
        ('vec_extend14','vec_extend20','vec_extend24','seq_f32',ROLES['warning_colors']))
    require(nodes and min(v['lowest_SP'] for v in nodes)==state.context.headers['visitor_frame'].start-144,
        'Widget complete restored-tail physical child maximum differs')
    return nodes


def _edge_at(parents,site):
    edges=[e for p in parents for e in p['ordinary_edges'] if p['function']+e[0]==site]
    require(len(edges)==1,'Widget phase lacks its actual current call edge')
    return edges[0]


def _at_child(parents,children,site,semantic):
    edge=_edge_at(parents,site);proof=children[edge[2],edge[3]]
    if edge[3].startswith('@outline:'):
        require(proof['frame']==0 and len(proof['ordinary_edges'])==1,
            'Widget scalar transport is not its matched zero-growth tail')
        tail=proof['ordinary_edges'][0]
        require(tail[1]=='b.w','Widget scalar transport changes the original return token')
        proof=children[tail[2],tail[3]]
    require(proof.get('semantic_role')==semantic,'Widget source field selects another primitive')
    return proof


def _nested_frame(context,size,label):
    frame=context.headers['visitor_frame']
    return Storage(frame.space,'decoded Widget '+label,frame.start-size,size,8,frame.phase,True)


def _primitive_guards(context,q):
    top=context.headers['visitor']['sequence_state'].header
    frame=context.headers['visitor_frame'];widget=borrow(frame,1360,8,frame.phase,True)
    return context.headers['visitor']['protected']+tuple(_object(v,label) for v,label in
        ((top,'live top Q'),(widget,'live folded Widget Q')) if q!=v)


def _input_policy(objects,decoder):
    # The private first-Boot context establishes the selected source allocation;
    # this is the existing explicit conditional hardware prerequisite only.
    return reviewed_eav_policy(objects,decoder.payload,dict(target=TARGET,core=0,security='secure',boot_context=BOOT_CONTEXT),
        contradictory_effects=())


def _source_sequence(objects,rows,context,parents,children,site,semantic,sequence,output_offset=None):
    proof=_at_child(parents,children,site,semantic);frame=context.headers['visitor_frame']
    args={}
    if semantic=='seq_f32':
        require(output_offset in (1064,1440),'Widget f32 Result uses an unreviewed scratch epoch')
        args=dict(output=borrow(frame,output_offset,8,frame.phase,True),platform_policy=_input_policy(objects,sequence.decoder))
    else:require(output_offset is None,'Widget register scalar acquired an outpointer')
    result=sequence_interface(objects,rows,proof,sequence,_nested_frame(context,proof['frame'],semantic),
        _primitive_guards(context,sequence.header),**args)
    stable=(_object(borrow(sequence.header.storage,sequence.header.offset,4,frame.phase),'stable Q.D'),
        _object(borrow(sequence.decoder.header.storage,sequence.decoder.header.offset+4,4,frame.phase),'stable D.end'))
    for option in result['outcomes']:
        # The checked sequence interface includes its own child's owned Result
        # scratch writes. Retain those proven borrows when checking the two
        # stable caller cells; do not recast them as caller-owned output.
        outputs=tuple(dict.fromkeys(v for v,offset,size in option['possible_writes']))
        require_writes(outputs,option['possible_writes'],stable)
    return result


def _band_candidate(objects,rows,state,parents,children,sequence):
    """One actual four-field Band candidate, before the Vec push guard.

    Unknown scalar values stay unknown. UTF8 and ASCII success predicates are
    paired with the same decoded slice; their error alternatives retain the
    advanced cursor. This private adapter cannot publish a Band/Widget slot.
    """
    context=state.context;frame=context.headers['visitor_frame'];phase=frame.phase
    V=context.headers['visitor']['proof']['function'];q=borrow(frame,1400,8,phase,True)
    actual=context.headers['visitor']['sequence_state'].decoder
    require(isinstance(sequence,SequenceInterface) and sequence.header==q and sequence.count==Span(4,4) and
        sequence.decoder.header==actual.header and sequence.decoder.payload==actual.payload,
        'Band loses its actual four-field Q/same-D/source epoch')
    errors=[];history=[];current=sequence
    def collect(stage,result):
        normal=[]
        for option in result['outcomes']:
            if option['kind']=='some':normal.append(option['state'])
            else:errors.append(dict(stage=stage,error=14 if option['kind']=='none' else option['error'],
                cursor=option['cursor'],frontier=V+56,source_field_consumed_before_error=option['kind']!='none'))
        require(len(normal)<=1,'Band primitive has multiple unpaired normal states')
        history.append((stage,result))
        return normal[0] if normal else None
    for label,site in (('min',2676),('max',2708)):
        result=_source_sequence(objects,rows,context,parents,children,V+site,'seq_f32',current,1440)
        current=collect(label,result)
        if current is None:return dict(normal_decoder=None,errors=tuple(errors),children=tuple(history),completed_Band_slot_issued=False)
    edge=_edge_at(parents,V+2736);proof=children[edge[2],edge[3]]
    require(proof['role']==ROLES['color'],'Band color does not use binary u16 Color')
    current=collect('color',color_interface(objects,rows,proof,current,_nested_frame(context,8,'Band Color'),
        _primitive_guards(context,q)))
    if current is None:return dict(normal_decoder=None,errors=tuple(errors),children=tuple(history),completed_Band_slot_issued=False)
    require(current.count==Span(1,1),'Band label guard lacks the remaining fourth source field')
    length_proof=_at_child(parents,children,V+2784,'take_varint_u32')
    output=borrow(frame,1440,8,phase,True);guards=_primitive_guards(context,q)+(_object(q,'Band Q'),)
    length=decoder_interface(objects,rows,length_proof,current.decoder,_nested_frame(context,32,'Band length'),guards,output=output)
    history.append(('label length',length));normal=[v for v in length['outcomes'] if v['kind']=='success']
    for option in length['outcomes']:
        if option['kind']!='success':errors.append(dict(stage='label length',error=option['error'],cursor=option['cursor'],frontier=V+56))
    if not normal:return dict(normal_decoder=None,errors=tuple(errors),children=tuple(history),completed_Band_slot_issued=False)
    require(len(normal)==1,'Band length loses its paired same-source outcome')
    before=normal[0]['state'];n=before.payload.extent;maximum=n-before.cursor.lower
    errors.append(dict(stage='label short input',error=4,cursor=before.cursor,frontier=V+56,
        guard='decoded length>remaining; no label-byte advance'))
    source=UTF8Slice(before.payload,before.cursor,Span(0,maximum))
    taken=replace(before,cursor=Span(before.cursor.lower,n))
    utf8=from_utf8_interface(objects,rows,source,borrow(frame,1440,12,phase,True),_nested_frame(context,24,'Band UTF8'),guards)
    if utf8['errors']:errors.append(dict(stage='label UTF8',error=8,cursor=taken.cursor,frontier=V+56,
        guard='the same advanced p/n pair is invalid UTF8'))
    extended=extend_string5_interface(objects,rows,_at_child(parents,children,V+2862,'vec_extend_inner'),
        String5Interface(borrow(frame,1440,12,phase,True),Span(0,0)),InitializedPrefix(before.payload,Span(0,maximum)),
        _nested_frame(context,24,'Band String5'),guards)
    if maximum>5:errors.append(dict(stage='String5 capacity',error=14,cursor=taken.cursor,frontier=V+56,
        guard='same UTF8 length>5; append does not write'))
    copies=[]
    for site,origin,destination in ((2886,1441,1420),(2896,1420,1185)):
        edge=_edge_at(parents,V+site);require(edge[3]=='__aeabi_memcpy','Band String5 staging copy target differs')
        copies.append(opaque_copy_interface(objects,rows,edge[2],borrow(frame,origin,11,phase),
            borrow(frame,destination,11,phase,True),11,frame,guards))
    # Only the captured label bytes survive String5 Result retirement. The
    # ordinary range child and loop operate on this source, not on the reused
    # normalized output or an arbitrary byte chosen independently of length.
    range_edge=_edge_at(parents,V+2960);range_proof=children[range_edge[2],range_edge[3]]
    require(range_proof.get('shape')=='range_u8' and parents[0]['objects'][0] and
        objects.read_only(parents[0]['objects'][0],3)==bytes((0,32,126)),
        'Band label loses its current inclusive printable byte range')
    label=borrow(frame,1440,5,phase,True);max_length=min(5,maximum)
    normalized_source=borrow(frame,1188,5,phase)
    clear_edge=_edge_at(parents,V+3000);copy_edge=_edge_at(parents,V+3010)
    clear=band_tail_clear_interface(objects,rows,clear_edge[2],label,Span(0,max_length),frame,guards)
    copied=None
    if max_length:
        copied=label_copy_interface(objects,rows,copy_edge[2],InitializedPrefix(normalized_source,Span(1,max_length)),label,frame,guards)
        errors.extend(dict(stage=stage,error=14,cursor=taken.cursor,frontier=V+56,guard=guard) for stage,guard in
            (('Band ASCII','1<=length<=5 AND some byte outside32..126'),('Band all spaces','1<=length<=5 AND all bytes==32')))
    after=replace(before,cursor=Span(before.cursor.lower,min(n,before.cursor.upper+5)))
    return dict(normal_decoder=after,incoming_decoder=sequence.decoder,errors=tuple(errors),children=tuple(history),utf8=utf8,extend=extended,
        staging_copy_effects=tuple(copies),clear=clear,normalized_copy=copied,range_proof=range_proof,
        source_fields=(('min','f32_bits',(0,0xffffffff)),('max','f32_bits',(0,0xffffffff)),
            ('color','u16',(0,65535)),('label','UTF8_String5',(0,max_length))),
        count_after_primitive_children=current.count,logical_source_fields_consumed=4,
        normal_label_guard='length==0 OR (all bytes in32..126 AND at least one byte!=32)',
        label_transport=((1441,1420,11),(1420,1185,11)),String5_Result_retired_after=V+2896,
        normalized_label_epoch=(V+2932,V+3036),source_validity_before_config_validation=True,
        completed_Band_slot_issued=False)


@dataclass(frozen=True)
class _WidgetSourcePrefix:
    decoder:object
    fields:tuple
    errors:tuple
    children:tuple
    physical_counts:tuple
    incoming_decoder:object
    completed_source_Widget:bool=False


def _widget_source_prefix(objects,rows,state,parents,children,decoder):
    """The fixed nine source groups preceding AdvancedWire, as one union.

    This is not a payload parser. Complete current child interfaces supply
    bounded unknown successes and all parse exits. The authored source order
    below supplies the exact Q epochs and folded manual decrements. No public
    summary or separately supplied value may be substituted for this join.
    """
    context=state.context;frame=context.headers['visitor_frame'];phase=frame.phase
    V=context.headers['visitor']['proof']['function'];actual=context.headers['visitor']['sequence_state'].decoder
    require(decoder.header==actual.header and decoder.payload==actual.payload,
        'Widget source prefix has another actual D/source allocation')
    errors=[];history=[];fields=[];counts=[]
    def error(label,code,cursor,guard=None):
        errors.append(dict(stage=label,error=code,cursor=cursor,guard=guard,frontier=V+56))
    def select(label,result):
        normal=[v for v in result['outcomes'] if v['kind'] in ('some','success')]
        for option in result['outcomes']:
            if option['kind'] not in ('some','success'):
                error(label,14 if option['kind']=='none' else option['error'],option['cursor'])
        history.append((label,result));require(len(normal)<=1,'Widget source child has unpaired successful states')
        return normal[0]['state'] if normal else None
    def sequence(label,offset,count,steps,before):
        q=SequenceInterface(borrow(frame,offset,8,phase,True),before,Span(count,count));physical=[count]
        for field,site,semantic in steps:
            result=_source_sequence(objects,rows,context,parents,children,V+site,semantic,q,
                1064 if semantic=='seq_f32' else None)
            q=select(label+'.'+field,result)
            if q is None:return None,tuple(physical)
            require(q.count.lower==q.count.upper and q.count.lower==physical[-1]-1,
                'Widget nested sequence loses decrement-before-child')
            physical.append(q.count.lower)
        return q,tuple(physical)
    def varint(label,site,before,q_offset,maximum=None):
        proof=_at_child(parents,children,V+site,'take_varint_u32')
        q=borrow(frame,q_offset,8,phase,True)
        result=decoder_interface(objects,rows,proof,before,_nested_frame(context,32,label),
            _primitive_guards(context,q)+(_object(q,'live '+label+' Q'),),output=borrow(frame,1064,8,phase,True))
        after=select(label,result)
        if after is not None and maximum is not None:
            error(label+' invalid enum',14,after.cursor,'same decoded u32>'+str(maximum))
        return after
    def manual_count(label,q,count_before,count_after,site):
        require(q.count==Span(count_before,count_before) and count_after>=0,
            'Widget manual source fields lack the actual positive Q residual')
        cell=borrow(q.header.storage,q.header.offset+4,4,phase,True)
        require_writes((cell,),((cell,0,4),),_primitive_guards(context,q.header)+(_object(q.decoder.header,'live D'),
            _object(borrow(q.header.storage,q.header.offset,4,phase),'stable Q.D')))
        history.append((label+' physical count',dict(site=V+site,before=count_before,after=count_after)))
        return replace(q,count=Span(count_after,count_after))
    def string(label,capacity,length_site,utf8_site,extend_site,copy_site,before,q_offset):
        after=varint(label+' length',length_site,before,q_offset)
        if after is None:return None
        n=after.payload.extent;maximum=n-after.cursor.lower
        error(label+' short input',4,after.cursor,'length>remaining; no string-byte advance')
        taken=replace(after,cursor=Span(after.cursor.lower,n))
        guards=_primitive_guards(context,borrow(frame,q_offset,8,phase,True))+(_object(borrow(frame,q_offset,8,phase),'live string Q'),)
        pair=UTF8Slice(after.payload,after.cursor,Span(0,maximum))
        utf8=from_utf8_interface(objects,rows,pair,borrow(frame,1064,12,phase,True),_nested_frame(context,24,label+' UTF8'),guards)
        require(_edge_at(parents,V+utf8_site)[2]==utf8['proof']['function'],'Widget string uses another UTF8 result')
        history.append((label+' UTF8',utf8))
        if utf8['errors']:error(label+' UTF8',8,taken.cursor,'the same advanced p/n pair is invalid UTF8')
        proof=_at_child(parents,children,V+extend_site,'vec_extend'+str(capacity))
        appended=extend_interface(objects,rows,proof,VecInterface(borrow(frame,1064,capacity+4,phase,True),capacity,Span(0,0)),
            InitializedPrefix(after.payload,Span(0,maximum)),_nested_frame(context,16,label+' append'),guards)
        history.append((label+' append',appended))
        if maximum>capacity:error(label+' capacity',14,taken.cursor,'length>'+str(capacity)+' after UTF8 success')
        edge=_edge_at(parents,V+copy_site);transport=children[edge[2],edge[3]]
        require(len(transport['ordinary_edges'])==1 and transport['ordinary_edges'][0][3]=='__aeabi_memcpy',
            'Widget successful string loses its captured byte-preserving tail')
        copied=opaque_copy_interface(objects,rows,transport['ordinary_edges'][0][2],borrow(frame,1065,capacity+3,phase),
            borrow(frame,1000,capacity+3,phase,True),capacity+3,frame,guards)
        history.append((label+' captured string tail',copied))
        return replace(after,cursor=Span(after.cursor.lower,min(n,after.cursor.upper+capacity)))
    def finish(before):
        return _WidgetSourcePrefix(before,tuple(fields),tuple(errors),tuple(history),tuple(counts),decoder)
    q,physical=sequence('Widget',1360,10,(('id',574,'seq_u16'),('view',606,'seq_u8')),decoder)
    counts.append(('Widget',physical))
    if q is None:return finish(None)
    fields.extend((('id',('u16',(0,65535))),('view',('u8',(0,255)))))
    require(q.count==Span(8,8),'Widget folded r10 residual is not the actual count after id/view')
    current=varint('Channel',656,q.decoder,1360,7)
    if current is None:return finish(None)
    fields.append(('channel',('Channel',(0,7))))
    placement,physical=sequence('Placement',1000,3,(('col',696,'seq_u8'),('row',722,'seq_u8')),current)
    counts.append(('Placement',physical))
    if placement is None:return finish(None)
    current=varint('Span',756,placement.decoder,1000,2)
    if current is None:return finish(None)
    fields.append(('placement',(('col',(0,255)),('row',(0,255)),('span',(0,2)))))
    current=string('Widget label',24,814,852,884,906,current,1360)
    if current is None:return finish(None)
    fields.append(('label',('UTF8_String24',(0,24))))
    display,physical=sequence('Display',880,7,(('decimals',948,'seq_u8'),)+tuple(
        (field,site,'seq_bool') for field,site in zip(('graph','average','extrema','trend','afr'),(972,994,1016,1038,1060))),current)
    counts.append(('Display',physical))
    if display is None:return finish(None)
    current=varint('GraphRange',1094,display.decoder,880,1)
    if current is None:return finish(None)
    fixed,fixed_counts=sequence('GraphRange::Fixed',1000,2,(('min',1134,'seq_f32'),('max',1166,'seq_f32')),current)
    # Auto takes no float payload; Fixed consumes both. Keep the branch source
    # relation beside the conservative joined cursor, never initialize Auto's
    # inactive payload from the Fixed alternative.
    history.append(('GraphRange paired variants',dict(Auto=current,Fixed=None if fixed is None else fixed.decoder,
        Auto_payload_opaque=True,Fixed_counts=fixed_counts)))
    current=replace(current,cursor=Span(current.cursor.lower,max(current.cursor.upper,
        current.cursor.upper if fixed is None else fixed.decoder.cursor.upper)))
    fields.append(('display',dict(decimals=(0,255),bools=(0,1),graph_range=('Auto','Fixed'),f32_bits=(0,0xffffffff))))
    warning,physical=sequence('Warning',1176,14,(('enabled',1242,'seq_bool'),),current)
    warning_counts=list(physical)
    if warning is None:return finish(None)
    current=varint('Applicability',1284,warning.decoder,1176,1)
    if current is None:return finish(None)
    warning=manual_count('Warning two enum fields',replace(warning,decoder=current),13,11,1328);warning_counts.append(11)
    current=varint('WarningMode',1336,warning.decoder,1176,4)
    if current is None:return finish(None)
    warning=replace(warning,decoder=current)
    for field,site,semantic in (('trigger',1368,'seq_f32'),('clear',1402,'seq_f32'),('min_load',1436,'seq_f32'),
        ('on_ms',1466,'seq_u16'),('off_ms',1502,'seq_u16')):
        warning=select('Warning.'+field,_source_sequence(objects,rows,context,parents,children,V+site,semantic,warning,
            1064 if semantic=='seq_f32' else None))
        if warning is None:return finish(None)
        warning_counts.append(warning.count.lower)
    warning=manual_count('Warning message',warning,6,5,1550);warning_counts.append(5)
    current=string('Warning message',20,1558,1598,1618,1640,warning.decoder,1176)
    if current is None:return finish(None)
    warning=replace(warning,decoder=current)
    for field,site,semantic in (('trigger_high',1666,'seq_f32'),('clear_high',1700,'seq_f32'),('snooze',1730,'seq_u16')):
        warning=select('Warning.'+field,_source_sequence(objects,rows,context,parents,children,V+site,semantic,warning,
            1064 if semantic=='seq_f32' else None))
        if warning is None:return finish(None)
        warning_counts.append(warning.count.lower)
    warning=manual_count('Warning priority',warning,2,1,1778);warning_counts.append(1)
    current=varint('WarningPriority',1786,warning.decoder,1176,3)
    if current is None:return finish(None)
    warning=select('Warning.latch',_source_sequence(objects,rows,context,parents,children,V+1814,'seq_bool',
        replace(warning,decoder=current)))
    if warning is None:return finish(None)
    warning_counts.append(warning.count.lower);counts.append(('Warning',tuple(warning_counts)))
    fields.append(('warning',dict(source_Wire_fields=14,f32_bits=(0,0xffffffff),u16=(0,65535),
        applicability=(0,1),mode=(0,4),priority=(0,3),message=('UTF8_String20',(0,20)),bools=(0,1))))
    current=varint('GaugeKind',1910,warning.decoder,1000,5)
    if current is None:return finish(None)
    gauge,physical=sequence('Gauge',1000,3,(('min',1940,'seq_f32'),('max',1970,'seq_f32'),('expand',1996,'seq_bool')),current)
    counts.append(('Gauge',physical))
    if gauge is None:return finish(None)
    fields.append(('gauge',dict(kind=(0,5),f32_bits=(0,0xffffffff),expand=(0,1))))
    colors=SequenceInterface(borrow(frame,1064,8,phase,True),gauge.decoder,Span(15,15));color_counts=[15]
    for ordinal in range(14):
        site=V+2038+36*ordinal;edge=_edge_at(parents,site);proof=children[edge[2],edge[3]]
        colors=select('WidgetColors.'+str(ordinal),color_interface(objects,rows,proof,colors,
            _nested_frame(context,8,'Widget Color'),_primitive_guards(context,colors.header)))
        if colors is None:return finish(None)
        color_counts.append(colors.count.lower)
    edge=_edge_at(parents,V+2544);proof=children[edge[2],edge[3]]
    colors=select('WidgetColors.warning',warning_colors_interface(objects,rows,proof,colors,
        borrow(frame,1000,6,phase,True),_nested_frame(context,24,'Widget WarningColors'),_primitive_guards(context,colors.header)))
    if colors is None:return finish(None)
    color_counts.append(colors.count.lower);counts.append(('WidgetColors',tuple(color_counts)))
    fields.append(('colors',('Color16',16,(0,65535))))
    require(colors.count==Span(0,0),'WidgetColors does not consume all15 source fields')
    return finish(colors.decoder)


def _source_fp_preservation(rows,children):
    """All matched normal descendants preserve the parent's s16..s31.

    Integer AAPCS summaries alone do not preserve the decoded float package.
    These complete finite children never define a high float destination. Low
    s0/s1 decoder results remain caller scratch; VSTR reads are not definitions.
    Saved return-PC restoration is checked separately by the return relation.
    """
    checked=set();high=re.compile(r'\b(?:s(?:1[6-9]|2[0-9]|3[01])|d(?:[89]|1[0-5]))\b')
    for proof in children.values():
        start=proof['function'];extent=proof['extent'];checked.add((start,extent))
        for at,row in rows.items():
            if not start<=at<start+extent or not row.mnemonic.startswith('v'):continue
            operation=row.mnemonic.removesuffix('.w');parts=row.operands.split(',',1)
            require(operation.startswith(('vstr','vstm','vpush','vcmp')) or
                not high.search(parts[0]) and not(operation.startswith(('vldm','vpop')) and high.search(row.operands)),
                'Widget normal child changes a live float source carrier')
    require(checked,'Widget source has no complete current normal children')
    return tuple(sorted(checked))


def _advanced_source(objects,rows,state,parents,children,decoder):
    """AdvancedWire8 with guarded Band source candidates and actual defaults.

    Band source values and their stored prefix are distinct. This produces no
    ordinary completion token: full selected control/commit/epoch composition
    must consume these effects before a stored Band or Widget can be promoted.
    """
    context=state.context;frame=context.headers['visitor_frame'];phase=frame.phase
    V=context.headers['visitor']['proof']['function'];actual=context.headers['visitor']['sequence_state'].decoder
    require(decoder.header==actual.header and decoder.payload==actual.payload,'Advanced has another same-invocation D/source')
    high_fp=_source_fp_preservation(rows,children);errors=[];history=[]
    q=borrow(frame,1380,8,phase,True);guards=_primitive_guards(context,q)+(_object(q,'Advanced Q'),)
    proof=_at_child(parents,children,V+2598,'take_varint_u32')
    length=decoder_interface(objects,rows,proof,decoder,_nested_frame(context,32,'Bands length'),guards,
        output=borrow(frame,1064,8,phase,True))
    normal=[v for v in length['outcomes'] if v['kind']=='success']
    errors.extend(dict(stage='Bands length',error=v['error'],cursor=v['cursor'],frontier=V+56) for v in length['outcomes'] if v['kind']!='success')
    if not normal:return dict(normal=(),errors=tuple(errors),children=(length,),Band_length=length,completed_Advanced_slot_issued=False)
    require(len(normal)==1,'Band Vec length has unpaired normal states')
    current=normal[0]['state'];bands=[]
    for j in range(4):
        # Completion N==j consumes no next candidate. N>j must parse a complete
        # candidate BEFORE the j==3 capacity check, including every parse exit.
        bands.append((j,current))
        candidate=_band_candidate(objects,rows,state,parents,children,
            SequenceInterface(borrow(frame,1400,8,phase,True),current,Span(4,4)))
        history.append(('Band'+str(j),candidate))
        errors.extend(dict(v,stage='Band'+str(j)+'.'+v['stage'],completed_Band_source_prefix=j) for v in candidate['errors'])
        after=candidate['normal_decoder']
        if after is None:break
        if j==3:
            errors.append(dict(stage='Band capacity',error=14,cursor=after.cursor,frontier=V+56,
                completed_Band_source_prefix=3,guard='N>3 AND fourth candidate decoded before j==3 guard'))
        else:current=after
    outcomes=[];attribute_histories=[]
    for j,before in bands:
        seq=SequenceInterface(q,before,Span(7,7));physical=[7];attributes=[]
        for label,site,semantic in (('reference',3132,'seq_f32'),('smoothing',3162,'seq_u16'),('peak_hold',3196,'seq_u16')):
            result=_source_sequence(objects,rows,context,parents,children,V+site,semantic,seq,1064 if semantic=='seq_f32' else None)
            attributes.append((label,result));success=[v for v in result['outcomes'] if v['kind']=='some']
            for v in result['outcomes']:
                if v['kind']!='some':errors.append(dict(stage='Advanced.'+label,error=14 if v['kind']=='none' else v['error'],
                    cursor=v['cursor'],frontier=V+56,Band_count=j))
            if not success:seq=None;break
            require(len(success)==1,'Advanced source scalar has unpaired normal states')
            seq=success[0]['state'];physical.append(seq.count.lower)
        if seq is None:continue
        require(tuple(physical)==(7,6,5,4),'Advanced folded enum residual differs')
        after=seq.decoder
        for label,site,maximum in (('readout',3244,9),('secondary',3278,3),('peak_mode',3312,2),('reference_mode',3346,2)):
            proof=_at_child(parents,children,V+site,'take_varint_u32')
            result=decoder_interface(objects,rows,proof,after,_nested_frame(context,32,'Advanced '+label),guards,
                output=borrow(frame,1064,8,phase,True));attributes.append((label,result))
            success=[v for v in result['outcomes'] if v['kind']=='success']
            for v in result['outcomes']:
                if v['kind']!='success':errors.append(dict(stage='Advanced.'+label,error=v['error'],cursor=v['cursor'],frontier=V+56,Band_count=j))
            if not success:after=None;break
            require(len(success)==1,'Advanced enum has unpaired normal states')
            after=success[0]['state']
            errors.append(dict(stage='Advanced.'+label+' invalid enum',error=14,cursor=after.cursor,frontier=V+56,
                Band_count=j,guard='same decoded u32>'+str(maximum)))
        if after is not None:outcomes.append(dict(Band_count=j,decoder=after,guard='decoded Band length=='+str(j),
            source_fields=(('reference','f32_bits',(0,0xffffffff)),('smoothing','u16',(0,65535)),('peak_hold','u16',(0,65535)),
                ('readout','enum',(0,9)),('secondary','enum',(0,3)),('peak_mode','enum',(0,2)),('reference_mode','enum',(0,2)))))
        attribute_histories.append((j,tuple(attributes)))
    # Exact 48-byte staging chain, including zero-capacity/opaque bytes. A byte
    # copy does not promote unused Band slots into typed values.
    copy_effects=[]
    for site,origin,destination in ((4172,1068,1000),(3120,1000,1128),(3374,1128,940)):
        edge=_edge_at(parents,V+site);copy=children[edge[2],edge[3]]
        require(edge[3]=='@outline:advanced_copy_bands48' and len(copy['ordinary_edges'])==1,
            'Advanced Band capture loses its exact 48-byte source transport')
        tail=copy['ordinary_edges'][0]
        copy_effects.append(opaque_copy_interface(objects,rows,tail[2],borrow(frame,origin,48,phase),
            borrow(frame,destination,48,phase,True),48,frame,guards))
    edge=_edge_at(parents,V+3416);proof=children[edge[2],edge[3]]
    constructor=advanced_interface(objects,rows,proof,Argument('Channel',None,0,8,frozenset((0,))),
        Argument('bool',None,0,8,frozenset((0,))),borrow(frame,1064,64,phase,True),
        _nested_frame(context,72,'Advanced initializer'),guards+(_object(borrow(frame,940,48,phase),'captured Band source'),))
    require(constructor['physical_activation_paths']==((72,8),(72,72)) and constructor['outcomes']==({'channel':0,'afr':0,'readout':2},),
        'Advanced source does not use the actual Boost/false initializer and restored copy tail')
    # Decoded overrides define the final V1000 object, not the retired V1064
    # initializer. All three16-byte band slots are zeroed before copying16*j.
    final=borrow(frame,1000,64,phase,True)
    override_writes=((final,59,1),(final,58,1),(final,57,1),(final,56,1),(final,54,2),(final,52,2),
        (final,48,4),(final,60,1))
    require_writes((final,),override_writes,guards+(_object(borrow(frame,940,48,phase),'captured Band source'),))
    require_writes((final,),tuple((final,16*i,16) for i in range(3)),guards+(_object(borrow(frame,940,48,phase),'captured Band source'),))
    copied=[];copy_edge=_edge_at(parents,V+3524)
    from config_widget_children import band_copy_interface
    for option in outcomes:
        count=option['Band_count'];copied.append(band_copy_interface(objects,rows,copy_edge[2],
            InitializedPrefix(borrow(frame,940,48,phase),Span(16*count,16*count)),borrow(frame,1000,48,phase,True),
            Span(count,count),frame,guards))
    return dict(normal=tuple(outcomes),errors=tuple(errors),children=tuple(history),Band_length=length,attribute_children=tuple(attribute_histories),
        physical_counts=(7,6,5,4),logical_remaining_enum_fields=4,source_enum_domains=(('readout',0,9),('secondary',0,3),
            ('peak_mode',0,2),('reference_mode',0,2)),initializer_arguments=('Boost',False),initializer=constructor,
        Band_capture_copies=((1068,1000,48),(1000,1128,48),(1128,940,48)),opaque_capture_effects=tuple(copy_effects),
        initializer_epoch=(1064,1128),final_epoch=(1000,1064),decoded_override_writes=override_writes,
        set_bands=copied,high_float_carrier_preservation=high_fp,completed_Advanced_slot_issued=False)


def _scratch_effects(objects,rows,state,parents,children):
    """Complete230-store ownership partition for this fixed parent family.

    Contents and retirement are supplied by the paired source/transport
    relations, never by this spatial envelope. In particular copying capacity
    and padding bytes cannot turn them into initialized source values.
    """
    context=state.context;frame=context.headers['visitor_frame'];phase=frame.phase
    V=context.headers['visitor']['proof']['function'];top=context.headers['visitor']['sequence_state'].header
    decoder=context.headers['visitor']['sequence_state'].decoder;D=decoder.header
    guards=context.headers['visitor']['protected']+(_object(D,'live complete D'),_object(decoder.payload,'immutable source'))
    widget_q=_object(borrow(frame,1360,8,phase),'folded Widget Q')
    fixed=[]
    for site,offset,size in LOCAL_STORES+FIXED_ALIAS_STORES:
        output=borrow(frame,offset,size,phase,True)
        stable_top=_object(borrow(top.storage,top.offset,4,phase),'stable top Q.D') if site==4362 else _object(top,'live complete top Q')
        extra=() if site in (564,570) else (widget_q,)
        require_writes((output,),((output,0,size),),guards+(stable_top,)+extra)
        fixed.append((V+site,offset,size))
    copied=[]
    for origin,destination,size,writes in INLINE_COPIES:
        source=borrow(frame,origin,size,phase);target=borrow(frame,destination,size,phase,True)
        require({b for site,offset,n in writes for b in range(offset,offset+n)}==set(range(size)),
            'Widget inline transport omits bytes from its captured source tail')
        require_writes((target,),tuple((target,offset,n) for site,offset,n in writes),
            guards+(_object(top,'top Q'),widget_q,_object(source,'live same source tail')))
        copied.append((origin,destination,size,tuple(V+site for site,offset,n in writes)))
    # Four byte-preserving Advanced copies lie between the initializer/source
    # package and the final destination tail. Full wrapper/builtin normal
    # effects protect the original return and all current live source objects.
    child_copies=[]
    for site,origin,destination,size in ((3424,1064,1000,48),(3548,1001,880,58),
        (3574,880,744,58),(3652,744,516,58),(3710,516,628,58)):
        edge=_edge_at(parents,V+site);proof=children[edge[2],edge[3]];target=edge[2]
        if edge[3].startswith('@outline:'):
            require(len(proof['ordinary_edges'])==1,'Widget tail transport contains another child')
            tail=proof['ordinary_edges'][0];require(tail[1]=='b.w' and tail[3]=='__aeabi_memcpy','Widget copy changes restored tail identity')
            target=tail[2]
        else:require(edge[3]=='__aeabi_memcpy','Widget captured tail uses another primitive')
        child_copies.append((site,origin,destination,size,opaque_copy_interface(objects,rows,target,
            borrow(frame,origin,size,phase),borrow(frame,destination,size,phase,True),size,frame,
            guards+(_object(top,'top Q'),widget_q))))
    # The actual fixed3-slot loop writes0/4/12 then advances by16 until48;
    # this clears complete EMPTY bands. Its copied source-prefix is separate.
    final=borrow(frame,1000,48,phase,True)
    require_writes((final,),tuple((final,i*16,16) for i in range(3)),guards+(_object(top,'top Q'),widget_q))
    cursor=borrow(D.storage,D.offset,4,phase,True)
    require_writes((cursor,),((cursor,0,4),),context.headers['visitor']['protected']+(_object(top,'top Q'),widget_q,
        _object(borrow(D.storage,D.offset+4,4,phase),'stable D.end'),_object(decoder.payload,'immutable source')))
    # Reconcile every parent store exactly once. This is a closed authored
    # partition after literal matching, not a scanner-generated write policy.
    partitions=(tuple(tuple(row[0] for row in table) for table in
        (LOCAL_STORES,FIXED_ALIAS_STORES,WIDGET_STORES,BAND_STORES))+
        (tuple(site for origin,destination,size,writes in INLINE_COPIES for site,offset,n in writes),
         DECODER_CURSOR_STORES,VECTOR_COUNT_STORES,ZERO_BAND_STORES))
    sites=tuple(s for group in partitions for s in group)
    actual={at-V for proof in parents for at,row in rows.items() if proof['function']<=at<proof['function']+proof['extent'] and
        row.mnemonic.startswith(('str','stm','vstr'))}
    require(len(sites)==len(set(sites)) and set(sites)==actual and len(sites)==230,
        'Widget current store has missing, duplicate or changed owned-family attribution')
    # Last reads are enforced by the complete matched load/store/call sequences:
    # each same tail is captured before its predecessor scratch is reused.
    chains=(('label',((1065,1000,27),(1000,828,27),(828,712,27),(712,488,27),(488,600,27)),600,27),
        ('warning',((1065,1000,23),(1000,940,23),(940,880,23),(880,1128,23),(1128,856,23),
            (856,804,23),(804,576,23),(576,688,23)),688,23),
        ('advanced',((1001,880,58),(880,744,58),(744,516,58),(516,628,58)),628,58))
    return dict(selected_store_count=len(sites),fixed_local_writes=tuple(fixed),inline_copies=tuple(copied),
        child_copies=tuple(child_copies),paired_source_transport_chains=chains,
        final_tail_origins=tuple((label,last,size) for label,chain,last,size in chains),
        top_count_write=(V+4362,292,4),next_Source_header_write=(V+4366,1500,8),stable_decoder_end=D.start+4,
        copied_opaque_bytes_claim_initialized=False,completed_prefix_issued=False)


@dataclass(frozen=True)
class _WidgetControlInput:
    state:object
    parents:tuple
    children:dict
    source:_WidgetSourcePrefix
    advanced:dict
    scratch:dict
    returns:tuple


def _widget_control_profile(value,context,parents,children):
    """The sole closed extension to the existing Views control walk.

    The public group derives this object itself. This function accepts no
    approval/cut dictionary and provides no continuation from a public report.
    The three impossible taken edges and one impossible fallthrough follow
    the matched enum/positive-count/error-union predecessor relations.
    """
    from config_decoded_views import _ViewsState
    require(type(value) is _WidgetControlInput and type(value.state) is _ViewsState and value.state.context is context and
        value.children is children and tuple(parents)==value.state.parents+value.parents and
        type(value.source) is _WidgetSourcePrefix,'Widget control extension is not its actual private phase state')
    V=context.headers['visitor']['proof']['function'];frame=context.headers['visitor_frame']
    require(tuple((p['function']-V,p['shape']) for p in value.parents)==
        ((416,'decoded_widgets_416'),(3516,'decoded_widgets_3516'),(6994,'decoded_widgets_6994')),
        'Widget control profile has unreviewed selected windows')
    require(value.state.exits and all(v.sequence.count==Span(8,8) and
        v.sequence.header==context.headers['visitor']['sequence_state'].header for v in value.state.exits),
        'Widget Sources frontier loses the actual inherited top8 alternatives')
    require(value.scratch['top_count_write']==(V+4362,292,4) and value.scratch['selected_store_count']==230,
        'Widget Sources frontier lacks its complete protected top-count effect')
    codes={v['error'] for v in value.source.errors}|{v['error'] for v in value.advanced['errors']}|{4,5,14}
    require(codes<={4,5,6,8,14} and 255 not in codes,
        'Widget selected error carriers can masquerade as the success sentinel')
    require(value.returns and all(v['returns'] and all(pc==v['return_PC'] and sp==v['return_SP']
        for at,pc,sp in v['returns']) and not v['architectural_LR_preserved'] for v in value.returns),
        'Widget control extension lacks actual saved-PC normal returns')
    return dict(excluded_taken={
        V+796:(V+6994,'same parsed Span<3; UXTB and FE/FF mux cannot produceFE'),
        V+3556:(V+4338,'same parsed reference_mode0..2 captured/overwritten at3434/3534 cannot equalFF'),
        V+4350:(V+4402,'the actual unchanged top8 is loaded at4348 before consuming Sources')},
        excluded_fallthrough={V+4344:(V+56,'all selected source/parse/capacity errors have lowbyte in4,5,6,8,14; neverFF')},
        nested_returns=value.state.control['nested_returns']+value.returns)


@dataclass(frozen=True)
class _WidgetActiveControl(Mapping):
    state:object
    parents:tuple
    children:dict
    profile:dict
    record:dict

    def __getitem__(self,key):return self.record[key]
    def __iter__(self):return iter(self.record)
    def __len__(self):return len(self.record)


def _widget_control(objects,rows,value):
    from config_decoded_views import _closure
    profile=_widget_control_profile(value,value.state.context,value.state.parents+value.parents,value.children)
    record=_closure(objects,rows,value.state.context,value.state.parents+value.parents,value.children,_widget=value)
    return _WidgetActiveControl(value.state,value.parents,value.children,profile,record)


@dataclass(frozen=True)
class _WidgetLengthEntry:
    sequence:SequenceInterface
    initialized_views:int
    length_result:dict


def _widget_lengths(objects,rows,state,parents,children):
    """Actual length416, before any clear or per-element source consumption.

    The length remains an unknown u32 paired with this same decoder. Its
    size-hint predicate is retained symbolically, rather than deciding a
    particular input length or treating all N>=65 as a pre-clear rejection.
    """
    context=state.context;frame=context.headers['visitor_frame'];phase=frame.phase
    V=context.headers['visitor']['proof']['function'];top=context.headers['visitor']['sequence_state'].header
    proof=_at_child(parents,children,V+416,'take_varint_u32');entries=[];errors=[]
    guards=context.headers['visitor']['protected']+(_object(top,'live top Q8'),)
    output=borrow(frame,1064,8,phase,True)
    for incoming in state.exits:
        require(incoming.sequence.header==top and incoming.sequence.count==Span(8,8),
            'Widget length has another inherited Views/top-Q epoch')
        decoder=incoming.sequence.decoder
        result=decoder_interface(objects,rows,proof,decoder,_nested_frame(context,32,'Widgets length'),guards,output=output)
        for option in result['outcomes']:
            require_writes((decoder.header,output),option['possible_writes'],
                (_object(borrow(decoder.header.storage,decoder.header.offset+4,4,phase),'stable D.end'),))
            if option['kind']=='success':
                require(option['value_domain']==(0,0xffffffff),'Widget Vec length lost its source u32 domain')
                entries.append(_WidgetLengthEntry(replace(incoming.sequence,decoder=option['state']),
                    incoming.initialized_views,result))
            else:
                errors.append(dict(stage='Widgets length',error=option['error'],cursor=option['cursor'],
                    initialized_views=incoming.initialized_views,initialized_widgets=0,widgets_cleared=False,frontier=V+56))
    # The clear's spatial permission is not an unconditional early write. The
    # guarded loop consumer below applies it only to successful length paths
    # satisfying N<=64 OR the actual remaining input R<N.
    length=borrow(context.owner,440,4,phase,True)
    require_writes((length,),((length,0,4),),guards+(_object(context.headers['visitor']['sequence_state'].decoder.header,'D'),))
    return tuple(entries),tuple(errors)


@dataclass(frozen=True)
class _OrdinaryCommitEvidence:
    Widget:tuple
    Band:tuple
    minimum_source_progress:int
    domain:str
    exception_safe_completion:bool
    source:object
    assembly:tuple
    physical_stack:tuple


def _ordinary_commit_evidence(objects,rows,value,control,index=0):
    """Join complete source/effect/control witnesses, without a prefix token.

    This finite relation authorizes the guarded last-store transition only.
    A later loop fold must supply its own fresh same-source candidate, index
    and remaining-count state. It cannot reuse an evaluated cursor interval
    from another iteration or treat this record as mutable ownership.
    """
    require(type(value) is _WidgetControlInput and type(control) is _WidgetActiveControl and
        control.state is value.state and control.parents is value.parents and control.children is value.children,
        'Widget completion requires the active same-group source/control relation, not structural evidence')
    profile=_widget_control_profile(value,value.state.context,value.state.parents+value.parents,value.children)
    require(profile==control.profile,'Widget completion changed its actual source/count/epoch control profile')
    context=value.state.context;frame=context.headers['visitor_frame'];V=context.headers['visitor']['proof']['function']
    source=value.source;advanced=value.advanced
    require(source.decoder is not None and tuple(k for k,v in source.fields)==
        ('id','view','channel','placement','label','display','warning','gauge','colors') and
        source.decoder.header==source.incoming_decoder.header and source.decoder.payload==source.incoming_decoder.payload and
        source.decoder.cursor.lower==source.incoming_decoder.cursor.lower+69,
        'Widget completion lacks its complete paired source prefix')
    require(advanced['normal'] and all(v['decoder'].header==source.decoder.header and
        v['decoder'].payload==source.decoder.payload and
        v['decoder'].cursor.lower==source.decoder.cursor.lower+11+10*v['Band_count'] for v in advanced['normal']),
        'Widget completion loses the same Advanced/Band source transition')
    successors=set(control['successors'])
    require((V+4164,V+554,None) in successors and (V+3104,V+2650,None) in successors and
        V+4376 in control['stopped_frontiers'] and V+416 not in control['stopped_frontiers'] and
        control['nested_returns']==value.state.control['nested_returns']+value.returns,
        'Widget/Band completion backedge or current child return is missing')
    # This is the same full physical activation test as the inherited path,
    # enlarged for Advanced72 + builtin72. The restored wrapper8 is a separate
    # alternative, never subtracted from that simultaneous144-byte path.
    startup=context.boundary['history']['preclaim']['startup']['copy'];S=startup['initial_sp'];low=frame.start-144
    from config_loader_boundary import _physical
    _physical(objects,low,S,startup['stack_bottom'])
    protected=context.boundary['physical_protected']
    task=next(g['interval'] for g in protected if g['kind']=='Boot_task')
    require(context.owner.start+14604<=low and not(task[0]<S and low<task[1]),
        'Widget deepest physical child aliases Config/Boot')
    # The whole affine buffer is a protection envelope, not an initialization
    # claim. It detects even a middle ancestor overlap; endpoint checks alone
    # would miss it. Actual writes and opaque padding remain in assembly.
    guards=context.headers['visitor']['protected']
    physical=tuple(Storage('address','physical Widget '+g.identity,g.start,g.extent,1,frame.phase,False)
        for g in guards if g.space!='address')
    buffer=borrow(context.owner,444,14080,frame.phase,True)
    require_writes((buffer,),((buffer,0,14080),),guards+physical)
    require(type(index) is int and 0<=index<64,'Widget completion skips the current capacity guard')
    assembly=tuple(_assembly_effects(objects,rows,value.state,value.parents,value.children,index,j) for j in (0,2))
    for proof in assembly:
        require(proof['Widget']['last_semantic_store']==V+4158 and proof['Widget']['checked_continuation']==V+4164 and
            proof['Band']['last_semantic_store']==V+3102 and proof['Band']['checked_continuation']==V+3104 and
            proof['Widget']['outstanding_after_count']==((180,219),) and proof['Band']['outstanding_after_count']==((11,16),),
            'Widget completion is earlier than its outstanding semantic writes')
    return _OrdinaryCommitEvidence((V+4018,V+4158,V+4164,V+554),(V+3092,V+3102,V+3104,V+2650),
        80,'named_ordinary_and_callback_paths',False,value,assembly,(low,S))


def _assembly_effects(objects,rows,state,parents,children,index,band_index):
    """Owned last-store relations, not a completed source package or prefix.

    The caller must separately establish the decoded package and the guarded
    ordinary path to these stores. Neither the early physical count nor this
    effect envelope can supply that prerequisite. All offsets below belong to
    the already matched finite source family; no instructions are interpreted
    to learn an output policy from a new binary.
    """
    require(type(index) is int and 0<=index<64 and type(band_index) is int and 0<=band_index<3,
        'Widget/Band capacity guard does not authorize this spare slot')
    context=state.context;frame=context.headers['visitor_frame'];phase=frame.phase
    V=context.headers['visitor']['proof']['function'];B=context.owner.start
    require(tuple(p['function'] for p in parents)==tuple(V+n for n in (416,3516,6994)),
        'Widget assembly uses another current parent/window')
    guards=tuple(context.headers['visitor']['protected'])+tuple(_object(v,label) for v,label in
        ((context.headers['visitor']['sequence_state'].header,'live top Q'),
         (context.headers['visitor']['sequence_state'].decoder.header,'live D'),
         (context.headers['visitor']['sequence_state'].decoder.payload,'immutable input')))
    # Storage coordinate labels do not prove physical separation. Project
    # every current ancestor into each consumed address space before checking
    # Config and local outputs, preserving all original guards as well.
    physical=tuple(Storage(space,'physical '+g.identity,g.start,g.extent,1,phase,False)
        for g in guards for space in ('address',frame.space) if space!=g.space)
    guards+=physical
    W=borrow(context.owner,444+220*index,220,phase,True)
    prior=() if not index else (Storage('address','ordinary prior Widget prefix',B+444,220*index,4,phase,False),)
    previous=(Storage('address','Config preceding Widget vector',B,440,4,phase,False),)
    after=(Storage('address','Config following Widget buffer',B+14524,52,4,phase,False),)
    protected=guards+prior+previous+after
    # These are the actual r5=B+220*i stores, expressed relative to W=r5+444.
    stores=WIDGET_STORES
    require_writes((W,),tuple((W,offset,size) for site,offset,size in stores),protected)
    copied=[]
    for site,source,destination,size in ((3888,688,33,23),(3974,628,89,58),(4002,600,153,27)):
        edge=_edge_at(parents,V+site)
        require(edge[3]=='__aeabi_memcpy' and (edge[2],edge[3]) in children,
            'Widget semantic staging copy loses its actual complete child')
        result=opaque_copy_interface(objects,rows,edge[2],borrow(frame,source,size,phase),
            borrow(context.owner,444+220*index+destination,size,phase,True),size,frame,protected)
        require(result['normal_preserves']==tuple('r'+str(i) for i in range(4,12))+('sp',),
            'Widget copy changes the live affine destination/source carriers')
        copied.append((site,source,destination,size,result))
    coverage={a for site,offset,size in stores for a in range(offset,offset+size)}
    coverage|={a for site,source,destination,size,result in copied for a in range(destination,destination+size)}
    late={a for site,offset,size in stores if site>4018 for a in range(offset,offset+size)}
    padding={18,19,30,31,87,219}
    require(coverage==set(range(220))-padding and late==set(range(180,219)),
        'Widget last-store envelope drops semantic fields or initializes padding')
    length=borrow(context.owner,440,4,phase,True)
    require_writes((length,),((length,0,4),),protected)
    band=_band_assembly_effects(state,band_index,guards)
    return dict(Widget=dict(slot_interval=(W.start,W.start+220),stores=stores,copies=tuple(copied),
        early_count=dict(site=V+4018,physical=index+1,logical=index),outstanding_after_count=((180,219),),
        last_semantic_store=V+4158,checked_continuation=V+4164,
        opaque_padding=((18,20),(30,32),(87,88),(219,220))),
        Band=band,
        completed_prefix_issued=False,requires_same_decoded_source_package_and_ordinary_path=True)


def _band_assembly_effects(state,band_index,guards):
    """Only the reached local Band; never borrow a spare Widget on error/i64."""
    require(type(band_index) is int and 0<=band_index<3,'Band capacity guard does not authorize this slot')
    frame=state.context.headers['visitor_frame'];phase=frame.phase
    V=state.context.headers['visitor']['proof']['function'];guards=tuple(guards)
    T=borrow(frame,1068+16*band_index,16,phase,True)
    band_stores=BAND_STORES
    band_prior=() if not band_index else (_object(borrow(frame,1068,16*band_index,phase),'prior Band prefix'),)
    label=_object(borrow(frame,1300,5,phase),'captured normalized Band label')
    require_writes((T,),tuple((T,offset,size) for site,offset,size in band_stores),guards+band_prior+(label,))
    spill=borrow(frame,148,4,phase,True)
    require_writes((spill,),((spill,0,4),),guards+band_prior+(label,_object(T,'in-progress Band')))
    require({a for site,offset,size in band_stores for a in range(offset,offset+size)}==set(range(16)),
        'Band source fields are not fully covered by the last-store envelope')
    return dict(slot_interval=(T.start,T.start+16),stores=band_stores,
        early_count=dict(site=V+3092,physical=band_index+1,logical=band_index),outstanding_after_count=((11,16),),
        last_semantic_store=V+3102,checked_continuation=V+3104,opaque_padding=())


@dataclass(frozen=True)
class _CommittedWidget:
    index:int
    before:object
    after:object
    source_fields:tuple
    Band_alternatives:tuple
    commit:tuple
    uses:tuple=()


@dataclass(frozen=True)
class _WidgetExit:
    sequence:SequenceInterface
    Source_sequence:SequenceInterface
    initialized_views:int
    committed:tuple


@dataclass(frozen=True)
class _WidgetsState:
    views:object
    parents:tuple
    children:dict
    control:object
    exits:tuple
    errors:tuple
    structural:object
    execution:object
    family_bridge:object
    reached_uses:tuple

    def __post_init__(self):
        from config_decoded_views import _ViewsState,_StructuralControl
        from config_platform import ExecutionReceipt
        from config_widget_uses import _WidgetFamilyBridge
        require(type(self.family_bridge) is _WidgetFamilyBridge and self.family_bridge.state is self.views and
            self.family_bridge.parents is self.parents and self.family_bridge.children is self.children and
            self.family_bridge.structural is self.structural,
            'Widget private state lost its current family lineage bridge')
        self.family_bridge.current()
        from config_widget_uses import _check_use
        require(type(self.reached_uses) is tuple,'Widget actual-use relation is not immutable')
        for use in self.reached_uses:_check_use(self.family_bridge,use)
        require(type(self.views) is _ViewsState and type(self.structural) is _StructuralControl and
            self.structural.selection.context is self.views.context and self.structural.selection.children is self.children and
            type(self.execution) is ExecutionReceipt,'Widget private state lost its derived predecessor/structure/domain binding')
        require((type(self.control) is _WidgetActiveControl and self.control.state is self.views) if self.exits else
            self.control is self.views.control,'Widget private control cannot be manufactured from a receipt or dormant structure')


def _iteration_state(state,index):
    """Carry the actual prior prefix through *all* fresh child effects."""
    require(type(index) is int and 0<=index<=64,'Widget head has an unbounded index')
    if index==0:return state
    context=state.context;frame=context.headers['visitor_frame'];B=context.owner.start
    prefix=Storage('address','actual ordinary Widget prefix',B+444,220*index,4,frame.phase,False)
    projected=Storage(frame.space,'physical ordinary Widget prefix',prefix.start,prefix.extent,4,frame.phase,False)
    headers=dict(context.headers);visitor=dict(headers['visitor'])
    visitor['protected']=visitor['protected']+(prefix,projected);headers['visitor']=visitor
    return replace(state,context=replace(context,headers=headers))


def _widget_capacity_decision(value,active,before,index,control):
    """The shared post-parse capacity boundary, never a slot/prefix grant.

    Public use follows the private lineage/reached-use checks. Explicit
    supplied abstract adapters can exercise this boundary at i64, without
    fabricating a larger first-Boot constructor or earlier completed Widgets.
    """
    require(type(value) is _WidgetControlInput and type(control) is _WidgetActiveControl and
        control.state is value.state and control.parents is value.parents and control.children is value.children,
        'Widget capacity lacks its actual guarded source/control relation')
    require(type(index) is int and 0<=index<=64 and active==_iteration_state(value.state,index),
        'Widget capacity loses actual address/physical prior-prefix protection')
    require(_widget_control_profile(value,value.state.context,value.state.parents+value.parents,value.children)==control.profile,
        'Widget capacity changed its current source/child/return profile')
    actual=active.context.headers['visitor']['sequence_state'].decoder
    require(value.source.incoming_decoder==before and before.header==actual.header and before.payload==actual.payload,
        'Widget capacity splices another current decoder/source epoch')
    normal=value.advanced['normal']
    if not normal:
        return dict(kind='source_error',after=None,spare_Widget_borrow_created=False,completed_prefix_issued=False)
    require(value.source.decoder is not None and tuple(k for k,v in value.source.fields)==
        ('id','view','channel','placement','label','display','warning','gauge','colors') and
        value.source.decoder.header==before.header and value.source.decoder.payload==before.payload and
        value.source.decoder.cursor.lower==before.cursor.lower+69 and
        all(v['decoder'].header==before.header and v['decoder'].payload==before.payload and
            type(v['Band_count']) is int and 0<=v['Band_count']<=3 and
            v['decoder'].cursor.lower==value.source.decoder.cursor.lower+11+10*v['Band_count'] and
            before.cursor.lower+80<=v['decoder'].cursor.lower<=v['decoder'].cursor.upper<=before.payload.extent
            for v in normal),
        'Widget capacity precedes complete fresh Widget/Advanced/Band parsing')
    after=replace(before,cursor=Span(min(v['decoder'].cursor.lower for v in normal),max(v['decoder'].cursor.upper for v in normal)))
    return dict(kind='capacity_error' if index==64 else 'candidate_ready',after=after,
        **({'error':14} if index==64 else {}),spare_Widget_borrow_created=False,completed_prefix_issued=False)


def _first_boot_decoded_widgets_state(objects,rows,source_context):
    """Fresh finite candidate evaluation on each actual private Views exit.

    No evaluated cursor/source dictionary is translated into another epoch.
    A normal abstract join can lose scalar precision, but every next source
    adapter runs again with that new same-source input. Each Views predecessor
    stays separate. Full source histories are checked then released; private
    transitions retain the actual before/after/source/commit relation.
    """
    from config_structural import _detached_inputs
    objects,rows=_detached_inputs(objects,rows)
    views,state,parents,children=_current_group(objects,rows,source_context)
    entries,length_errors=_widget_lengths(objects,rows,state,parents,children)
    context=state.context;frame=context.headers['visitor_frame'];phase=frame.phase
    V=context.headers['visitor']['proof']['function'];top=context.headers['visitor']['sequence_state'].header
    returns=_child_returns(rows,state,parents,children)
    from config_decoded_views import _widget_structural_control
    structural=_widget_structural_control(objects,rows,state,parents,children,returns)
    from config_widget_uses import _widget_family_bridge
    family_bridge=_widget_family_bridge(objects,rows,state,parents,children,returns,structural,source_context,entries)
    errors=list(length_errors);exits=[];control=None;iterations=[];reached_uses=[]
    def record_errors(options,entry,index):
        for option in options:
            errors.append(dict(option,initialized_views=entry.initialized_views,initialized_widgets=index,
                widgets_cleared=True,ordinary_current_candidate_unpublished=True))
    for entry in entries:
        current=entry.sequence.decoder;committed=();n=current.payload.extent
        if n-current.cursor.lower>=65:
            errors.append(dict(stage='Widgets pre-clear capacity',error=14,cursor=current.cursor,
                initialized_views=entry.initialized_views,initialized_widgets=0,widgets_cleared=False,frontier=V+56,
                guard='same N>=65 AND actual post-length remaining>=N'))
        # Every success below implies N==i<=64, hence satisfies the complementary
        # preclear guard. Attempted candidates also retain N>i AND (N<=64 OR R<N).
        # The latter includes oversized short-input sequences; never force N<=64.
        for index in range(65):
            require(len(committed)==index and current.header==entry.sequence.decoder.header and
                current.payload==entry.sequence.decoder.payload and current.cursor.upper<=n,
                'Widget head lost its paired predecessor/source/prefix epoch')
            next_top=replace(entry.sequence,count=Span(7,7),decoder=current)
            source_q=SequenceInterface(borrow(frame,1500,8,phase,True),current,Span(8,8))
            exits.append(_WidgetExit(next_top,source_q,entry.initialized_views,committed))
            # N>i must still attempt parsing even if fewer than80 bytes remain.
            # Primitive errors, not the minimum-byte estimate, stop that path.
            active=_iteration_state(state,index)
            source=_widget_source_prefix(objects,rows,active,parents,children,current)
            record_errors(source.errors,entry,index)
            advanced=(dict(normal=(),errors=()) if source.decoder is None else
                _advanced_source(objects,rows,active,parents,children,source.decoder))
            record_errors(advanced['errors'],entry,index)
            scratch=_scratch_effects(objects,rows,active,parents,children)
            value=_WidgetControlInput(state,parents,children,source,advanced,scratch,returns)
            if control is None:control=_widget_control(objects,rows,value)
            else:_widget_control_profile(value,context,state.parents+parents,children)
            from config_widget_uses import _candidate_uses,_widget_commit_use
            uses=_candidate_uses(family_bridge,active,value,entry,index,committed,control)
            reached_uses.extend(uses)
            decision=_widget_capacity_decision(value,active,current,index,control)
            if decision['kind']=='source_error':break
            normal=advanced['normal'];after=decision['after']
            if decision['kind']=='capacity_error':
                errors.append(dict(stage='Widget precommit capacity',error=14,cursor=after.cursor,
                    initialized_views=entry.initialized_views,initialized_widgets=64,widgets_cleared=True,frontier=V+56,
                    guard='N>64 AND complete candidate decoded before i==64 guard',spare_Widget_borrow_created=False))
                break
            evidence=_ordinary_commit_evidence(objects,rows,value,control,index)
            require(evidence.domain=='named_ordinary_and_callback_paths' and evidence.exception_safe_completion is False,
                'Widget candidate changed its ordinary execution domain')
            widget_use=_widget_commit_use(family_bridge,active,value,entry,index,committed,control,evidence,after)
            reached_uses.append(widget_use);uses+=(widget_use,)
            committed+=(_CommittedWidget(index,current,after,source.fields,
                tuple((v['Band_count'],v['decoder'].cursor) for v in normal),evidence.Widget,uses),)
            iterations.append((entry.initialized_views,index,current.cursor,after.cursor))
            current=after
            # No old source result or child history is used by the next loop.
            del source,advanced,value,evidence
    # No successful Widget length means no candidate/clear/Source-Q successor.
    # Structural coverage above does not activate any dormant outgoing edge.
    if control is None:
        require(not entries,'Widget entries lost their actual guarded active control')
        control=state.control
    from config_platform import _execution_receipt
    receipt=_execution_receipt(_input_policy(objects,context.headers['visitor']['sequence_state'].decoder),source_context,objects.elf)
    # Public diagnostics deliberately omit the heavyweight checked histories;
    # neither the description nor its copied execution receipt is a capability.
    description=dict(phase='first_boot_decoded_widgets_pending_remaining_visitor_canonical_and_loader_routes',mode='sizez',
        Sources_ready=dict(frontier=V+4376,top_count=7,source_count=8,Source_bytes_consumed=0) if exits else None,
        declared_Source_frontier=dict(frontier=V+4376,reached=bool(exits)),
        prefix_errors=views['prefix_errors'],Views_errors=views['group_errors'],
        successes=tuple(dict(initialized_views=e.initialized_views,initialized_widgets=len(e.committed),
            cursor=(e.sequence.decoder.cursor.lower,e.sequence.decoder.cursor.upper),top_count=7,source_count=8,
            guard='same decoded N==initialized_widgets; all prior fresh candidates committed') for e in exits),
        group_errors=tuple(dict(v,cursor=(v['cursor'].lower,v['cursor'].upper)) for v in errors),
        iterations=tuple((views,index,(before.lower,before.upper),(after.lower,after.upper))
            for views,index,before,after in iterations),control=dict(selected_instructions=control['selected_instructions'],
            stopped_frontiers=control['stopped_frontiers'],outside_shared_context_count=len(control['outside_shared_contexts']),
            pending_contexts_confer_state=False),
        structural_control=dict(selected_instructions=len(structural.selection.selected),
            raw_successor_count=len(structural.raw_successors),unentered_outgoing=structural.unentered_outgoing,
            outside_shared_context_count=len(structural.outside_shared_contexts),confers_active_state=False),
        conditional_Widget_accesses=dict(families=tuple(v.role for v in family_bridge.families),
            sites=sum(len(v.sites) for v in family_bridge.families),descriptors_are_reached_uses=False,actual_inherited16_pending=True),
        source_guarded_accesses=dict(alternatives=len(reached_uses),roles=tuple(sorted({v.role for v in reached_uses})),
            runtime_observation=False,completed_prefix_issued=False),
        ordinary_domain=receipt.to_record()['domain'],execution_scope=receipt.to_record(),
        logical_prefix_promotion=dict(Widget=(V+4158,V+4164),Band=(V+3102,V+3104)),
        exceptional_resume_established=False,all_observers_compatible=False,exception_safe_completion=False,unconditional_owner=False,
        source_context=views['source_context'],source_qualification=views['source_qualification'],
        external_qualifications=views['external_qualifications'],exception_graph_limit=views['exception_graph_limit'],
        exception_absence_established=False,hardware_qualified=False,initial_facts_reinitialized=False,
        requires_H8_consumed_input_closure=True,requires_M2_pre_init_source_reconciliation=True,
        full_visitor_return_pending=True,shared_error_suffix_pending=True,
        pending=('Sources and remaining visitor groups','full visitor errors and restore','D finalize/canonical Compare',
            'decoded validation and other loader modes/routes','diagnostic/sink/PANICKED/full resource graph',
            'complete execution/result consumers and actual full producer','final all-mode/max-input/native CI/H2 gates'))
    return description,_WidgetsState(state,parents,children,control,tuple(exits),tuple(errors),structural,receipt,family_bridge,tuple(reached_uses))


def first_boot_decoded_widgets(objects,instructions,source_context):
    return _first_boot_decoded_widgets_state(objects,instructions,source_context)[0]
