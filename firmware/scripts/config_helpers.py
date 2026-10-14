"""Complete small helper bodies; source/owned effects and callers stay required.

Stable literal contracts retain every loop, save/return path and status flag.
Typed target metadata is a boundary requirement, never its transitive proof.
Specialized varint input, initialized Result fields and actual target alignment
policy must be established by contextual composition before gate admission.
"""
import re
from config_finite import _complete_body
from config_helper_contracts import HELPER_GRAMMARS,HELPER_ROLES,HELPER_SEMANTIC_ROLES
from config_logger import _logger_function
from config_roots import root_location
from config_regions import Region,Transfer,Storage,borrow,require_disjoint,require_writes,audit_coverage,_control
from config_stack import canonical_symbol,transfer_family
from validation import require

# (complete maximum frame, saved-cell start, exact optional scratch subview).
# Adding a body never manufactures a frame/effect contract by default.
FRAME_LAYOUTS={
    'compare_extend':(24,0,None),'serialize_str':(24,0,None),'slice_pop':(16,0,None),
    'take_varint_u32':(32,0,None),'seq_u8':(8,0,None),'seq_u16':(8,0,None),'seq_bool':(8,0,None),
    'serialize_u16':(24,8,(4,3)),'serialize_u8':(16,8,(7,1)),'varint_usize':(24,8,(0,5)),
    'varint_u32_small':(24,8,(3,5)),'serialize_f32':(16,8,(4,4)),'seq_f32':(24,8,(0,8)),
}


def _helper_function(objects,address,role):
    extent,frame,name=_logger_function(objects,address,role)
    if role in HELPER_ROLES:
        expected=HELPER_GRAMMARS[HELPER_ROLES[role]]
        require((extent,frame)==(expected['extent'],expected['frame']),'helper child current extent/frame differs')
    return extent,frame,name


def _helper_object(objects,address,descriptor):
    require(descriptor['kind']=='source_location' and descriptor['role'] in
            ('postcard_serialize_u16','postcard_varint_usize','postcard_varint_u32_small'),
            'unreviewed helper readonly object')
    root_location(objects,address,descriptor['role']);return ()


def helper_body(objects,name,instructions,outline_role=None,_ancestry=()):
    require(name in objects.elf.functions,'missing small helper body')
    start=objects.elf.functions[name][0]&~1;require(start not in _ancestry,'small helper outline cycle')
    role=canonical_symbol(name)
    if outline_role is not None:
        require(isinstance(outline_role,str) and outline_role.startswith('@outline:') and outline_role in HELPER_ROLES and
                re.fullmatch(r'OUTLINED_FUNCTION_[0-9]+',name),'unreviewed small helper outline role/origin')
        role=outline_role
    require(role in HELPER_ROLES,'unreviewed exact small helper receiver/outline')
    semantic=HELPER_ROLES[role]
    proof=_complete_body(objects,name,instructions,role,semantic,HELPER_GRAMMARS[semantic],
                         _helper_object,helper_body,_ancestry,function_consumer=_helper_function)
    proof.update(semantic_role=semantic,
                 transfers=tuple((r.address-start,r.mnemonic,r.operands.split('@',1)[0].strip())
                     for a,r in sorted(instructions.items()) if start<=a<start+proof['extent'] and
                     (transfer_family(r.mnemonic.removesuffix('.w')) or re.search(r'\bpc\b',r.operands))),
                 requires_source_inputs_owned_effects_and_child_closure=True,
                 requires_parent_frame_and_abi=semantic if outline_role else None,
                 requires_u32_domain_maximum=127 if semantic=='varint_u32_small' else None,
                 requires_hard_float_s0=semantic=='serialize_f32')
    return proof


def helper_coverage(objects,name,instructions,context,outline_role=None):
    """Reconcile complete matched helper inventories, retaining O5 obligations."""
    proof=helper_body(objects,name,instructions,outline_role);start=proof['function']
    contract=HELPER_GRAMMARS[proof['semantic_role']];rows=[];padding=[];edges=[]
    children={start+offset:(operation,target,role,frame) for offset,operation,target,role,frame in proof['ordinary_edges']}
    for offset,width,operation in contract['instructions']:
        at=start+offset;row=instructions[at]
        if operation=='nop':padding.append(at);continue
        rows.append(at);family,conditional=_control(row)
        if family is None:continue
        operands=row.operands.split('@',1)[0].strip()
        if family=='return' or family=='bx' and operands=='lr':
            edges.append(Transfer(at,row.mnemonic,'conditional_return' if conditional else 'return',None,'complete helper restore/return'))
        elif at in children:
            _,target,role,_=children[at]
            edges.append(Transfer(at,row.mnemonic,'call' if family=='bl' else 'tail',target,'complete helper typed child',role))
        else:
            target=re.search(r'0x([0-9a-f]+)(?:\s|$)',operands)
            require(target is not None and family in ('b','cbz','cbnz'),'unreviewed helper transfer inventory')
            target=int(target[1],16)
            edges.append(Transfer(at,row.mnemonic,'loop' if target<=at else 'branch',target,'matched helper local control; source invariant still required'))
    code_end=start+max(b for a,b,k in contract['mapping'] if k=='t')
    regions=[Region(context,'helper',start,min(padding) if padding else code_end,tuple(rows),(),tuple(edges))]
    if padding:regions.append(Region(context,'padding',min(padding),code_end,tuple(padding),(),()))
    for lo,hi,kind in contract['mapping']:
        if kind=='d':regions.append(Region(context,'data',start+lo,start+hi,(),((start+lo,objects.read_only(start+lo,hi-lo)),),()))
    def target_consumer(objects,target,role):
        if not role.startswith('@outline:'):return _helper_function(objects,target,role)
        names=[n for n,e in objects.elf.functions.items() if e[0]&~1==target]
        require(len(names)==1,'ambiguous helper outline coverage target')
        child=helper_body(objects,names[0],instructions,role)
        return child['extent'],child['frame'],names[0]
    coverage=audit_coverage(objects,name,instructions,context,regions,_function_consumer=target_consumer)
    proof.update(coverage=coverage)
    return proof


def helper_frame(proof,frame,ancestor_protected):
    """Bind closed local scratch/save maps to a caller-established actual SP.

    These are subobjects of one active frame, not independent allocations.
    Initial save writes are covered by the matched prologue; ordinary local
    writes and allowed child Result scratch cannot touch saved/ancestor cells.
    This does not establish the incoming SP, input pointers or initialized tags.
    Zero-frame outlines instead consume their parent's contextual pending map.
    """
    semantic=proof['semantic_role'];require(semantic in FRAME_LAYOUTS,'unknown helper frame contract')
    contract=HELPER_GRAMMARS[semantic];extent,saved_offset,scratch_layout=FRAME_LAYOUTS[semantic]
    require(extent>0 and proof['role']==HELPER_SEMANTIC_ROLES[semantic] and proof['shape']==semantic and
            (proof['extent'],proof['frame'],contract['frame'])==(contract['extent'],extent,extent),'helper body/frame proof differs')
    require(isinstance(frame,Storage) and frame.space.startswith('stack:') and frame.extent==extent and
            frame.alignment>=8 and frame.writable,'helper frame has no exact aligned mutable activation')
    ancestor_protected=tuple(ancestor_protected);require_disjoint((frame,)+ancestor_protected)
    saved=Storage(frame.space,frame.identity+':saved-cells',frame.start+saved_offset,extent-saved_offset,4,frame.phase,False)
    scratch=(borrow(frame,*scratch_layout,frame.phase,writable=True),) if scratch_layout is not None else ()
    protected=ancestor_protected+(saved,)
    local_writes=tuple((view,0,view.extent) for view in scratch) if semantic!='seq_f32' else ()
    require_writes(scratch,local_writes,protected)
    return dict(frame=frame,scratch=scratch,protected=protected,local_writes=local_writes,
                child_result_scratch=scratch if semantic=='seq_f32' else (),
                requires_caller_sp_inputs_child_effects_and_initialization=True)
