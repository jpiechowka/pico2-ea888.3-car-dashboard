"""Fixed Compare prefix effects; concrete byte facts still need caller proof.

The current complete Compare/restore and memcmp bodies are mandatory. A
SourceBytes record does not prove its machine producer, initialization,
transport, lifetime or a Config's validity. Authored serializer/caller regions
must establish those facts before using this finite summary. No generic memory
execution or candidate-selected builtin effect is supported.
"""
from dataclasses import dataclass,replace
import struct
from config_decode_effects import DecoderState,_current,_object
from config_helpers import helper_frame
from config_logger import logger_body
from config_regions import Storage,View,require_disjoint,require_writes
from validation import require


@dataclass(frozen=True)
class CompareState:
    header:View
    payload:View
    cursor:int

    def __post_init__(self):
        # Share the structural eight-byte header/input checks, not its ABI:
        # Compare word4 is remaining LENGTH; Decoder word4 is the END pointer.
        DecoderState(self.header,self.payload,self.cursor)


@dataclass(frozen=True)
class SourceBytes:
    view:View
    data:bytes

    def __post_init__(self):
        require(isinstance(self.view,View) and not self.view.writable and
                (self.view.storage.space!='address' or self.view.start>0) and
                isinstance(self.data,bytes) and len(self.data)<=self.view.extent,'source bytes lack initialized bounded readonly borrow')


def _source_bytes(objects,source,frame):
    require(isinstance(source,SourceBytes),'serialized bytes have no typed initialized source borrow')
    if source.view.storage.space=='address' and source.data:
        require(not source.view.storage.writable and objects.read_only(source.view.start,len(source.data))==source.data,
                'readonly source-byte fact differs from its current ELF allocation')
    else:
        require(source.view.storage.space==frame.space and source.view.storage.phase==frame.phase or
                source.view.storage.space=='address' and not source.view.storage.writable,
                'serialized source belongs to another coordinate/lifetime')
    return source.data


def _memcmp_dependency(objects,instructions,proof,frame):
    """Shared complete builtin binding; source/caller protection stays explicit."""
    dependencies=[e for e in proof['ordinary_edges'] if e[3]=='memcmp']
    require(len(dependencies)==1,'Compare memcmp dependency differs')
    target=dependencies[0][2]
    names=[n for n,e in objects.elf.functions.items() if n=='memcmp' and e[0]&~1==target]
    require(len(names)==1,'Compare has no exact memcmp target')
    builtin=logger_body(objects,names[0],instructions)
    require(builtin['role']=='memcmp' and builtin['frame']==32 and builtin['extent']==116 and
            not builtin['ordinary_edges'] and not builtin['indirect_sites'],'Compare builtin summary differs')
    child_frame=Storage(frame.space,frame.identity+':memcmp',frame.start-32,32,8,frame.phase,True)
    return builtin,child_frame


def compare_effect(objects,instructions,proof,state,source,frame,ancestor_protected):
    """One try_extend operation. A failed later operation never rolls back C."""
    proof=_current(objects,instructions,proof)
    require(proof['semantic_role']=='compare_extend' and isinstance(state,CompareState) and isinstance(source,SourceBytes),
            'Compare effects have another source/header ABI')
    require(state.header.storage.space==frame.space and state.header.storage.phase==frame.phase,
            'Compare header belongs to another activation/lifetime')
    active=helper_frame(proof,frame,tuple(ancestor_protected))
    payload=objects.read_only(state.payload.start,state.payload.extent);n=len(_source_bytes(objects,source,frame))
    protected=active['protected']+(_object(state.payload,'immutable-input'),_object(source.view,'serialized-input'))
    # Validate the full live C capability: initial prologue writes cannot touch
    # even a word which this particular operation would not update on Err.
    require_writes((state.header,),((state.header,0,8),),protected)
    require_disjoint((frame,_object(source.view,'serialized-input')))
    builtin,child_frame=_memcmp_dependency(objects,instructions,proof,frame)
    require_disjoint((child_frame,)+active['protected'])
    require_disjoint((child_frame,_object(source.view,'serialized-input')))
    require_writes((state.header,),((state.header,0,8),),(child_frame,))
    original=state;reads=();writes=();status=2
    if state.payload.extent-state.cursor>=n:
        if n:reads=((state.payload.start+state.cursor,n),(source.view.start,n))
        if payload[state.cursor:state.cursor+n]==source.data:
            state=replace(state,cursor=state.cursor+n);status=255
            writes=((state.header,0,struct.pack('<II',state.payload.start+state.cursor,state.payload.extent-state.cursor)),)
    return dict(semantic_role='compare_extend',proof=proof,builtin=builtin,state=state,original_state=original,
                registers={'r0':status},error=None if status==255 else 2,writes=writes,input_read_envelopes=reads,
                normal_preserves=tuple('r'+str(i) for i in range(4,12))+('sp',),
                conservative_activation_frames=(24,32),
                requires_machine_constructor_source_bytes_arguments_lifetime_ingress_and_enclosing_graph=True)
