"""Closed scalar/string serializer effects for the pinned Compare helpers.

Concrete scalar/byte facts must come from recognized typed source fields and
actual machine arguments. This component produces their fixed local encoding;
it does not learn values from candidate registers, validate a Config, or prove
the enclosing caller/ingress/phase. Unknown scalar families/child roles reject.
"""
import struct
from config_compare_effects import CompareState,SourceBytes,compare_effect,_source_bytes
from config_decode_effects import _current,_object
from config_helpers import helper_coverage,helper_frame,index_prefix,HELPER_SEMANTIC_ROLES
from config_regions import Storage,Argument,borrow,require_disjoint,require_writes
from config_stack import canonical_symbol
from validation import require


def _child(objects,instructions,proof,semantic):
    interfaces={'serialize_u8':('compare_extend',),'serialize_u16':('range_to_inclusive_index_mut','compare_extend'),
                'serialize_f32':('compare_extend',),'varint_usize':('array5_index_mut','compare_extend'),
                'varint_u32_small':('array5_index_mut','compare_extend'),'serialize_str':('varint_usize','compare_extend')}
    require(semantic in interfaces.get(proof['semantic_role'],()),'unreviewed serializer child interface')
    role=HELPER_SEMANTIC_ROLES[semantic];edges=[e for e in proof['ordinary_edges'] if e[3]==role]
    require(len(edges)==1,'serializer child edge differs');target=edges[0][2]
    names=[n for n,e in objects.elf.functions.items() if e[0]&~1==target and canonical_symbol(n)==role]
    require(len(names)==1,'serializer child has no exact typed target')
    return helper_coverage(objects,names[0],instructions,(target,'fixed serializer child',proof['function']))


def _frame(parent,semantic,size):
    return Storage(parent.space,parent.identity+':'+semantic,parent.start-size,size,8,parent.phase,True)


def _varint(value):
    result=[]
    for group in range(5):
        byte=value&127;value>>=7;result.append(byte|(128 if value else 0))
        if not value:return bytes(result)
    raise AssertionError('scalar exceeded reviewed five-group usize width')


def serialize_effect(objects,instructions,proof,state,value,frame,ancestor_protected):
    """One fixed serialization operation, including its ordered child effects."""
    proof=_current(objects,instructions,proof);semantic=proof['semantic_role']
    scalar={'serialize_u8':('u8',8),'serialize_u16':('u16',16),'varint_usize':('usize',32),
            'varint_u32_small':('u32',32),'serialize_f32':('f32_bits',32)}
    require(semantic in scalar or semantic=='serialize_str','unreviewed serializer/source interface')
    require(isinstance(state,CompareState) and state.header.storage.space==frame.space and
            state.header.storage.phase==frame.phase,'serializer C belongs to another caller lifetime')
    active=helper_frame(proof,frame,tuple(ancestor_protected))
    prologue=Storage(frame.space,frame.identity+':complete-prologue',frame.start,frame.extent,8,frame.phase,False)
    require_writes((state.header,),((state.header,0,8),),active['protected']+(prologue,_object(state.payload,'immutable-input')))
    original=state;children=[];local_writes=();index=None;encoded=None;status=2;paths=[]
    if semantic=='serialize_str':
        data=_source_bytes(objects,value,frame)
        try:data.decode('utf-8')
        except UnicodeDecodeError:raise AssertionError('string source lacks its source-valid UTF-8 bytes') from None
        require(len(data)<=0xffffffff,'string length exceeds target usize')
        source_guard=_object(value.view,'live-string')
        require_disjoint((frame,source_guard))
        require_writes((state.header,),((state.header,0,8),),(source_guard,))
        child=_child(objects,instructions,proof,'varint_usize')
        prefix=serialize_effect(objects,instructions,child,state,Argument('usize',None,0,32,frozenset((len(data),))),
                                _frame(frame,'length-prefix',24),active['protected']+(source_guard,))
        children.append(prefix);state=prefix['state']
        paths.extend((frame.extent,)+p for p in prefix['conservative_activation_paths'])
        if prefix['error'] is None:
            child=_child(objects,instructions,proof,'compare_extend')
            comparison=compare_effect(objects,instructions,child,state,value,_frame(frame,'string-body',24),active['protected'])
            children.append(comparison);state=comparison['state'];status=0xffffffff if comparison['error'] is None else 2
            paths.append((frame.extent,24,32))
    else:
        kind,bits=scalar[semantic]
        require(isinstance(value,Argument) and (value.kind,value.view,value.levels,value.bits)==(kind,None,0,bits) and
                len(value.domain)==1,'serializer lacks one concrete typed source scalar/ABI')
        number=next(iter(value.domain))
        require(semantic!='varint_u32_small' or number<=127,'specialized u32 requires source-valid value<=127')
        encoded=bytes((number,)) if semantic=='serialize_u8' else struct.pack('<I',number) if semantic=='serialize_f32' else _varint(number)
        scratch=active['scratch'];require(len(scratch)==1,'serializer local buffer differs');buffer=scratch[0]
        require(len(encoded)<=buffer.extent,'encoded source scalar exceeds its fixed local buffer')
        require_writes((buffer,),((buffer,0,buffer.extent),),active['protected']+(_object(state.header,'C'),_object(state.payload,'immutable-input')))
        # The matched bodies initialize exactly these complete scratch buffers;
        # only the encoded prefix is borrowed by the later Compare call.
        local_writes=((buffer,0,buffer.extent),)
        if semantic in ('serialize_u16','varint_usize','varint_u32_small'):
            child_semantic='range_to_inclusive_index_mut' if semantic=='serialize_u16' else 'array5_index_mut'
            child=_child(objects,instructions,proof,child_semantic)
            location_role={'serialize_u16':'postcard_serialize_u16','varint_usize':'postcard_varint_usize','varint_u32_small':'postcard_varint_u32_small'}[semantic]
            require(set(proof['objects'])=={0},'serializer Location inventory differs')
            index=index_prefix(objects,instructions,child,buffer,_frame(frame,'encoded-prefix',child['frame']),
                active['protected']+(_object(state.header,'C'),),proof['objects'][0],
                Argument('usize',None,0,32,frozenset((len(encoded)-1,))),location_role=location_role)
            outcome=index['outcomes'];require(len(outcome)==1 and 'prefix' in outcome[0] and outcome[0]['prefix'].extent==len(encoded),
                                           'typed scalar failed its proved bounded index')
            paths.append((frame.extent,)+index['conservative_frame_metadata'])
        child=_child(objects,instructions,proof,'compare_extend')
        source=SourceBytes(borrow(buffer.storage,buffer.offset,buffer.extent,buffer.storage.phase),encoded)
        comparison=compare_effect(objects,instructions,child,state,source,_frame(frame,'comparison',24),active['protected'])
        children.append(comparison);state=comparison['state']
        status=(255 if semantic in ('varint_usize','varint_u32_small') else 0xffffffff) if comparison['error'] is None else 2
        paths.append((frame.extent,24,32))
    return dict(semantic_role=semantic,proof=proof,state=state,original_state=original,registers={'r0':status},
                error=None if status in (255,0xffffffff) else 2,encoded=encoded,local_write_envelopes=local_writes,
                writes=tuple(write for child in children for write in child['writes']),children=tuple(children),index=index,
                conservative_activation_paths=tuple(paths),normal_preserves=tuple('r'+str(i) for i in range(4,12))+('sp',),
                requires_machine_source_field_arguments_lifetime_ingress_and_enclosing_graph=True)
