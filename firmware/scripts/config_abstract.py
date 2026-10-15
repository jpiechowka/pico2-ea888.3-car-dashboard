"""Finite source-typed primitive interfaces for conservative stack unions.

No payload values are evaluated here. Full current helper bodies/coverage,
their exact child/outline ABIs and owned effects establish the closed success
and error alternatives below. These are caller interfaces, never a Config
validator, source selector, generic ISA evaluator or whole graph admission.
Existing exact helper APIs deliberately remain separate.
"""
from dataclasses import dataclass,replace
import hashlib
from config_decode_effects import DecoderState,SequenceState,_current,_object,_with_input,_decoder_dependency
from config_helpers import helper_frame
from config_platform import PlatformPolicy
from config_regions import Storage,View,require_writes
from validation import require


@dataclass(frozen=True)
class Span:
    lower:int
    upper:int

    def __post_init__(self):
        require(type(self.lower) is int and type(self.upper) is int and
                0<=self.lower<=self.upper<=0xffffffff,'unbounded abstract scalar/cursor interval')


@dataclass(frozen=True)
class DecoderInterface:
    header:View
    payload:View
    cursor:Span

    def __post_init__(self):
        require(isinstance(self.cursor,Span),'decoder has no bounded cursor interval')
        DecoderState(self.header,self.payload,self.cursor.lower)
        require(self.cursor.upper<=self.payload.extent,'abstract cursor exceeds its source allocation')


@dataclass(frozen=True)
class SequenceInterface:
    header:View
    decoder:DecoderInterface
    count:Span

    def __post_init__(self):
        require(isinstance(self.decoder,DecoderInterface) and isinstance(self.count,Span),'untyped abstract Q/D/count')
        SequenceState(self.header,DecoderState(self.decoder.header,self.decoder.payload,self.decoder.cursor.lower),self.count.lower)


def _policy(objects,state,policy):
    require(isinstance(policy,PlatformPolicy) and
        (policy.input_start,policy.input_end)==(state.payload.start,state.payload.start+state.payload.extent) and
        policy.input_sha256==hashlib.sha256(objects.read_only(state.payload.start,state.payload.extent)).hexdigest() and
        policy.elf_sha256==hashlib.sha256(objects.elf.data).hexdigest(),'abstract f32 interface lost current conditional input policy')


def _output(output,state,frame,protected):
    require(isinstance(output,View) and output.writable and output.extent==8 and output.start%4==0 and
        output.storage.alignment>=4 and output.storage.space==frame.space and output.storage.phase==frame.phase,
        'abstract Result output has another owner/reference/extent ABI')
    require_writes((output,),((output,0,8),),protected+(_object(state.header,'D'),))
    require_writes((state.header,),((state.header,0,8),),(_object(output,'Result'),))


def _result(semantic,proof,outcomes,children,policy,frames):
    require(bool(outcomes),'abstract primitive has no possible exit')
    return dict(semantic_role=semantic,proof=proof,outcomes=tuple(outcomes),children=tuple(children),platform_policy=policy,
        conservative_activation_frames=tuple(frames),normal_preserves=tuple('r'+str(i) for i in range(4,12))+('sp',),
        requires_actual_stage_receiver_transport_and_complete_parent_graph=True)


def decoder_interface(objects,instructions,proof,state,frame,ancestor_protected,*,output=None,platform_policy=None):
    """Closed four-decoder union; scalar successes remain unknown source types.

    Cursor intervals only constrain in-allocation guards. Complete varint loop
    bodies bound pops to3/5; sequential iterations do not nest another frame.
    The output envelope never claims padding or an Err value is initialized.
    """
    proof=_current(objects,instructions,proof);semantic=proof['semantic_role']
    require(semantic in ('slice_pop','deserialize_u16','take_varint_u32','deserialize_seed_f32') and
            isinstance(state,DecoderInterface),'unsupported abstract decoder/source interface')
    require(state.header.storage.space==frame.space and state.header.storage.phase==frame.phase,'abstract D lifetime differs')
    objects.read_only(state.payload.start,state.payload.extent)
    active=helper_frame(proof,frame,tuple(ancestor_protected));protected=_with_input(active['protected'],state.payload)
    require_writes((state.header,),((state.header,0,8),),protected)
    result_memory=semantic in ('take_varint_u32','deserialize_seed_f32')
    if result_memory:_output(output,state,frame,protected)
    else:require(output is None,'abstract register result received an outpointer')
    if semantic=='deserialize_seed_f32':_policy(objects,state,platform_policy)
    else:require(platform_policy is None,'abstract integer decoder acquired a platform prerequisite')
    children=[]
    if semantic in ('deserialize_u16','take_varint_u32'):
        child,child_frame=_decoder_dependency(objects,instructions,proof,frame)
        # Later iterations may consume a prefix. End remains the same checked
        # allocation and the pop body guards every read, including exhaustion.
        widened=replace(state,cursor=Span(state.cursor.lower,state.payload.extent))
        extra=(_object(output,'Result'),) if output is not None else ()
        children.append(decoder_interface(objects,instructions,child,widened,child_frame,protected+extra))
    outcomes=[];lo,hi=state.cursor.lower,state.cursor.upper;n=state.payload.extent
    def emit(kind,cursor,value_kind=None,domain=None,fields=(),write_cursor=False):
        writes=((state.header,0,4),) if write_cursor else ()
        if output is not None:writes+=tuple((output,a,b) for a,b in fields)
        require_writes((state.header,)+((output,) if output is not None else ()),writes,
                       active['protected']+(_object(state.payload,'immutable-input'),))
        outcomes.append(dict(kind=kind,state=replace(state,cursor=cursor),cursor=cursor,
            value_kind=value_kind,value_domain=domain,error=None if kind=='success' else int(kind[5:]),
            possible_writes=writes,initialized_result_fields=fields,
            possible_read_envelope=state.payload if write_cursor else None))
    if semantic=='deserialize_seed_f32':
        if lo<=n-4:
            last=min(hi,n-4);platform_policy.for_f32_load(state.payload.start+lo);platform_policy.for_f32_load(state.payload.start+last)
            emit('success',Span(lo+4,last+4),'f32_bits',(0,0xffffffff),((0,1),(4,4)),True)
        if hi>=max(0,n-3):emit('error4',Span(max(lo,n-3),hi),fields=((0,1),(1,1)))
    else:
        maximum={'slice_pop':1,'deserialize_u16':3,'take_varint_u32':5}[semantic]
        value_kind={'slice_pop':'u8','deserialize_u16':'u16','take_varint_u32':'u32'}[semantic]
        bits={'u8':8,'u16':16,'u32':32}[value_kind]
        if lo<n:emit('success',Span(lo+1,min(n,hi+maximum)),value_kind,(0,(1<<bits)-1),
            ((0,1),(4,4)) if result_memory else (),True)
        if hi>=max(0,n-maximum+1):emit('error4',Span(n,n),fields=((0,1),(1,1)) if result_memory else (),write_cursor=lo<n)
        if maximum>1 and lo+maximum<=n:emit('error5',Span(lo+maximum,min(n,hi+maximum)),
            fields=((0,1),(1,1)) if result_memory else (),write_cursor=True)
    frames=(proof['frame'],)+(children[0]['conservative_activation_frames'] if children else ())
    return _result(semantic,proof,outcomes,children,platform_policy,frames)


def sequence_interface(objects,instructions,proof,state,frame,ancestor_protected,*,output=None,platform_policy=None):
    """Closed Q/count union; None and decrement-before-child stay distinct."""
    proof=_current(objects,instructions,proof);semantic=proof['semantic_role']
    require(semantic in ('seq_u8','seq_u16','seq_bool','seq_f32') and isinstance(state,SequenceInterface),
            'unsupported abstract sequence/source interface')
    decoder=state.decoder
    require(state.header.storage.space==frame.space and state.header.storage.phase==frame.phase,'abstract Q lifetime differs')
    active=helper_frame(proof,frame,tuple(ancestor_protected));protected=_with_input(active['protected'],decoder.payload)
    prologue=Storage(frame.space,frame.identity+':complete-prologue',frame.start,frame.extent,8,frame.phase,False)
    require_writes((state.header,decoder.header),((state.header,0,8),(decoder.header,0,8)),(prologue,))
    require_writes((state.header,),((state.header,0,8),),protected+(_object(decoder.header,'D'),))
    require_writes((decoder.header,),((decoder.header,0,8),),protected+(_object(state.header,'Q'),))
    objects.read_only(decoder.payload.start,decoder.payload.extent)
    if semantic=='seq_f32':
        _output(output,decoder,frame,protected+(_object(state.header,'Q'),));_policy(objects,decoder,platform_policy)
        require_writes((output,),((output,0,8),),(prologue,))
        require_writes((state.header,),((state.header,0,8),),(_object(output,'RO8'),))
        protected+=(_object(output,'RO8'),)
    else:require(output is None and platform_policy is None,'abstract sequence received another result/policy ABI')
    child,child_frame=_decoder_dependency(objects,instructions,proof,frame);kwargs={}
    if semantic=='seq_f32':
        scratch=active['child_result_scratch'];require(len(scratch)==1,'abstract f32 scratch owner differs')
        kwargs=dict(output=scratch[0],platform_policy=platform_policy)
    bound=decoder_interface(objects,instructions,child,decoder,child_frame,protected+(_object(state.header,'Q'),),**kwargs)
    outcomes=[]
    if state.count.lower==0:
        fields=((0,4),) if output is not None else ()
        outcomes.append(dict(kind='none',count=Span(0,0),cursor=decoder.cursor,value_kind=None,value_domain=None,error=None,
            state=replace(state,count=Span(0,0)),possible_writes=tuple((output,a,b) for a,b in fields),initialized_result_fields=fields))
    if state.count.upper>0:
        count=Span(max(1,state.count.lower)-1,state.count.upper-1)
        for alternative in bound['outcomes']:
            kind='some' if alternative['kind']=='success' else alternative['kind']
            choices=(kind,'error6') if semantic=='seq_bool' and kind=='some' else (kind,)
            for choice in choices:
                value_kind=alternative['value_kind'] if choice=='some' else None
                domain=alternative['value_domain'] if choice=='some' else None
                if semantic=='seq_bool' and choice=='some':value_kind,domain='bool',(0,1)
                fields=(((0,4),(4,4)) if choice=='some' else ((0,4),(4,1))) if output is not None else ()
                writes=((state.header,4,4),)+alternative['possible_writes']+tuple((output,a,b) for a,b in fields)
                if output is not None:require_writes((output,),tuple((output,a,b) for a,b in fields),
                    active['protected']+(_object(state.header,'Q'),_object(decoder.header,'D'),_object(decoder.payload,'immutable-input')))
                outcomes.append(dict(kind=choice,count=count,cursor=alternative['cursor'],value_kind=value_kind,value_domain=domain,
                    state=replace(state,count=count,decoder=alternative['state']),
                    error=None if choice=='some' else int(choice[5:]),possible_writes=writes,initialized_result_fields=fields))
    return _result(semantic,proof,outcomes,(bound,),platform_policy,(proof['frame'],)+bound['conservative_activation_frames'])
