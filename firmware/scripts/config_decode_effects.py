"""Fixed source effects for the four reviewed postcard decoder bodies.

These consume caller-established D/header/input/cursor facts, not arbitrary
machine pointers. The complete current bodies and all required pop children
are rebound before effects are used. Construction, register transport, source
phase, all-ingress and the enclosing diagnostic/ordinary graph remain required.
Only the pinned emitted scalar/result ABIs below are supported. This is not an
instruction evaluator, general postcard implementation or Config validator.
"""
from dataclasses import dataclass,replace
import hashlib
import struct
from config_helpers import helper_coverage,helper_frame,HELPER_SEMANTIC_ROLES
from config_platform import PlatformPolicy
from config_regions import Storage,View,require_writes
from config_stack import canonical_symbol
from validation import require


@dataclass(frozen=True)
class DecoderState:
    header:View
    payload:View
    cursor:int

    def __post_init__(self):
        require(isinstance(self.header,View) and self.header.writable and self.header.extent==8 and
                self.header.start%4==0 and self.header.storage.alignment>=4 and self.header.storage.space.startswith('stack:'),
                'decoder header lacks its aligned mutable eight-byte source object')
        require(isinstance(self.payload,View) and self.payload.storage.space=='address' and
                self.payload.start>0 and not self.payload.writable and not self.payload.storage.writable,
                'decoder bytes lack their complete immutable input allocation')
        require(type(self.cursor) is int and 0<=self.cursor<=self.payload.extent,
                'decoder cursor/end ordering or same-allocation bound differs')


def _object(view,role):
    return Storage(view.storage.space,view.storage.identity+':'+role+':'+str(view.offset),
                   view.start,view.extent,1,view.storage.phase,False)


def _current(objects,instructions,proof):
    require(isinstance(proof,dict) and 'coverage' in proof,'decoder effects lack complete body/edge coverage')
    names=[n for n,e in objects.elf.functions.items() if e[0]&~1==proof['function'] and canonical_symbol(n)==proof['role']]
    require(len(names)==1,'decoder effects have no unique current typed body')
    current=helper_coverage(objects,names[0],instructions,proof['coverage']['context'])
    require(current==proof,'decoder effects use stale body/child facts')
    return current


def _pop_child(objects,instructions,proof,frame,state,protected):
    role=HELPER_SEMANTIC_ROLES['slice_pop']
    edges=[e for e in proof['ordinary_edges'] if e[3]==role]
    require(len(edges)==1,'decoder pop dependency differs')
    target=edges[0][2]
    names=[n for n,e in objects.elf.functions.items() if e[0]&~1==target and canonical_symbol(n)==role]
    require(len(names)==1,'decoder pop has no unique typed target')
    child=helper_coverage(objects,names[0],instructions,(target,'fixed decoder pop',proof['function']))
    child_frame=Storage(frame.space,frame.identity+':pop',frame.start-child['frame'],child['frame'],8,frame.phase,True)
    return decode_effect(objects,instructions,child,state,child_frame,protected)


def decode_effect(objects,instructions,proof,state,frame,ancestor_protected,*,output=None,platform_policy=None):
    """Return exact fixed helper effects for the supplied valid source state.

    Bytes are read only from the bound readonly ELF input. Cursor/header values
    are supplied by separately recognized constructors/predecessors; this does
    not read host stack bytes or infer that machine registers equal these facts.
    Error padding/value words remain uninitialized, not zero-filled facts.
    """
    proof=_current(objects,instructions,proof);semantic=proof['semantic_role']
    require(semantic in ('slice_pop','deserialize_u16','take_varint_u32','deserialize_seed_f32') and
            isinstance(state,DecoderState),'unsupported decoder/source state')
    require(state.header.storage.space==frame.space and state.header.storage.phase==frame.phase,
            'decoder header belongs to another caller coordinate/lifetime')
    payload=objects.read_only(state.payload.start,state.payload.extent)
    active=helper_frame(proof,frame,tuple(ancestor_protected))
    protected=active['protected']+(_object(state.payload,'immutable-input'),)
    # Check the possible cursor update, including on branches which do not write.
    # A typed D must never alias protected storage even on an empty stream.
    require_writes((state.header,),((state.header,0,4),),protected)
    if semantic in ('take_varint_u32','deserialize_seed_f32'):
        require(isinstance(output,View) and output.writable and output.extent==8 and output.start%4==0 and output.storage.alignment>=4 and
                output.storage.space==frame.space and output.storage.phase==frame.phase,'decoder Result output ABI differs')
        require_writes((output,),((output,0,8),),protected+(_object(state.header,'D'),))
        require_writes((state.header,),((state.header,0,4),),(_object(output,'Result'),))
        protected=protected+(_object(output,'Result'),)
    else:require(output is None,'register-result decoder has an unexpected outpointer')
    if semantic=='deserialize_seed_f32':
        require(isinstance(platform_policy,PlatformPolicy) and
                (platform_policy.input_start,platform_policy.input_end)==(state.payload.start,state.payload.start+state.payload.extent) and
                platform_policy.input_sha256==hashlib.sha256(payload).hexdigest() and
                platform_policy.elf_sha256==hashlib.sha256(objects.elf.data).hexdigest(),
                'f32 decoder lost its current conditional platform/input prerequisite')
    else:require(platform_policy is None,'non-f32 decoder acquired an unrelated platform prerequisite')
    original=state;reads=[];writes=[];children=[];value=None;error=None;registers={};fields=()
    if semantic=='slice_pop':
        if state.cursor==len(payload):error=4;registers={'r0':1,'r1':4}
        else:
            value=payload[state.cursor];reads.append((state.payload.start+state.cursor,1));state=replace(state,cursor=state.cursor+1)
            writes.append((state.header,0,struct.pack('<I',state.payload.start+state.cursor)))
            registers={'r0':0,'r1':value}
    elif semantic=='deserialize_seed_f32':
        if len(payload)-state.cursor<4:error=4
        else:
            platform_policy.for_f32_load(state.payload.start+state.cursor)
            reads.append((state.payload.start+state.cursor,4));value=int.from_bytes(payload[state.cursor:state.cursor+4],'little')
            state=replace(state,cursor=state.cursor+4)
            writes.append((state.header,0,struct.pack('<I',state.payload.start+state.cursor)))
    else:
        maximum,terminal_maximum=(3,3) if semantic=='deserialize_u16' else (5,15)
        value=0
        # Finite source varint invariant: k successful bytes, 7*k shift;
        # the maximum guard fires before a fourth/sixth pop. No rollback.
        for k in range(maximum):
            pop=_pop_child(objects,instructions,proof,frame,state,protected)
            children.append(pop);state=pop['state'];reads.extend(pop['input_reads']);writes.extend(pop['writes'])
            if pop['error'] is not None:error=pop['error'];break
            byte=pop['value'];value|=(byte&127)<<(7*k)
            if byte<128:
                if k==maximum-1 and byte>terminal_maximum:error=5
                break
        else:error=5
        if error is not None:value=None
        if semantic=='deserialize_u16':registers={'r0':value<<16 if error is None else (error<<8)|1}
    if output is not None:
        if error is None:
            writes.extend(((output,4,struct.pack('<I',value)),(output,0,b'\0')));fields=((0,1),(4,4))
        else:
            writes.extend(((output,1,bytes((error,))),(output,0,b'\1')));fields=((0,1),(1,1))
        # The Result's own object is protected from child writes, but its listed
        # parent-owned field stores are checked against all other live storage.
        require_writes((output,),((view,offset,len(data)) for view,offset,data in writes if view==output),
                       active['protected']+(_object(state.payload,'immutable-input'),_object(state.header,'D')))
    require(error in (None,4,5),'decoder emitted an unreviewed error domain')
    return dict(semantic_role=semantic,proof=proof,state=state,original_state=original,output=output,
                registers=registers,value=value,error=error,input_reads=tuple(reads),writes=tuple(writes),
                initialized_result_fields=fields,children=tuple(children),platform_policy=platform_policy,
                normal_preserves=tuple('r'+str(i) for i in range(4,12))+('sp',),
                conservative_activation_frames=(proof['frame'],16) if children else (proof['frame'],),
                requires_machine_constructor_arguments_lifetime_ingress_and_enclosing_graph=True)
