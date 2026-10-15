"""One actual first-Boot boot_view/Views group with bounded unknown values.

Uses private prefix state exactly once. Nothing accepts the public prefix
summary as a capability, reinitializes mutated Config, or completes later
visitor/canonical/loader/resource routes. All copies retain opaque padding.
"""
from dataclasses import replace
import re
from config_abstract import Span,SequenceInterface,sequence_interface,decoder_interface
from config_abstract_compare import InitializedPrefix
from config_construction import VecInterface,extend_interface
from config_decode_effects import _object
from config_decoded_prefix import _first_boot_decoded_u16_state,_name,_direct_family
from config_finite import _literal_body,_complete_body
from config_headers import VISITOR,ROOT
from config_helpers import helper_coverage,_helper_function,HELPER_SEMANTIC_ROLES
from config_logger import logger_body
from config_loader_boundary import _physical
from config_owner_entry import _direct_target
from config_refill_children import opaque_copy_interface
from config_regions import Storage,borrow,require_writes,require_disjoint,_control
from config_stack import canonical_symbol
from config_utf8 import UTF8Slice,from_utf8_interface,utf8_body,UTF8
from config_views_contracts import GRAMMARS,ROLES,SHARED_PENDING,PENDING_VARINT_TRANSPORT
from validation import require

_CUSTOM={v:k for k,v in ROLES.items() if v.startswith('@outline:')}


def _outline(objects,name,rows,role,ancestry=()):
    require(role in _CUSTOM and re.fullmatch(r'OUTLINED_FUNCTION_[0-9]+',name),'unreviewed decoded Views outline')
    contract=GRAMMARS[_CUSTOM[role]]
    def no_object(*args):require(False,'unexpected decoded outline object')
    proof=_complete_body(objects,name,rows,role,_CUSTOM[role],contract,no_object,_outline,ancestry,function_consumer=_helper_function)
    require(proof['frame']==0,'decoded Views outline grows an independent frame')
    return proof


def _parent(objects,rows,context):
    V=context.headers['visitor']['proof']['function'];name=_name(objects,VISITOR,V);B=context.owner.start
    def same_config(objects,value,descriptor):
        require(descriptor==dict(kind='same_Config') and value==B,'Views reload selects another Config');return ()
    return tuple(_literal_body(objects,name,rows,VISITOR,'decoded_views_'+str(off),GRAMMARS['visitor_'+str(off)],
        same_config,_outline,window=V+off,function_consumer=_helper_function) for off in (82,4188,4194,6848))


def _children(objects,rows,parents):
    proofs={};pending=[e for p in parents for e in p['ordinary_edges']]
    while pending:
        off,operation,target,role,frame=pending.pop();key=(target,role)
        if key in proofs:continue
        if role in _CUSTOM:
            names=[n for n,e in objects.elf.functions.items() if e[0]&~1==target];require(len(names)==1,'ambiguous View outline')
            proof=_outline(objects,names[0],rows,role)
        elif role==UTF8:proof=utf8_body(objects,rows);require(proof['function']==target,'Views UTF8 target differs')
        elif role in ('__aeabi_memcpy','_RNvNtCs_17compiler_builtins3mem6memcpy'):
            proof=logger_body(objects,_name(objects,role,target),rows)
        elif role in HELPER_SEMANTIC_ROLES.values():
            if role.startswith('@outline:'):
                names=[n for n,e in objects.elf.functions.items() if e[0]&~1==target];require(len(names)==1,'ambiguous View helper outline')
                proof=helper_coverage(objects,names[0],rows,(target,'decoded Views child'),role)
            else:proof=helper_coverage(objects,_name(objects,role,target),rows,(target,'decoded Views child'))
        else:require(False,'unclassified decoded Views child')
        require(proof['frame']==frame,'Views child activation differs');proofs[key]=proof;pending.extend(proof['ordinary_edges'])
    return proofs


def _edge(parents,offset):
    main=parents[0];edges=[v for v in main['ordinary_edges'] if v[0]==offset-82]
    require(len(edges)==1,'Views lacks its exact source call');return edges[0]


def _child(children,parents,offset):
    e=_edge(parents,offset);return children[e[2],e[3]]


def _frame(context,size,label):
    p=context.headers['visitor_frame'];return Storage(p.space,label,p.start-size,size,8,p.phase,True)


def _stable(context,state):
    q=state.header;d=state.decoder.header;p=context.frame.phase
    return (_object(borrow(q.storage,q.offset,4,p),'stable Q.D'),_object(borrow(d.storage,d.offset+4,4,p),'stable D.end'))


def _sequence(objects,rows,context,proof,state):
    top=context.headers['visitor']['sequence_state'].header
    guards=context.headers['visitor']['protected']+(() if state.header==top else (_object(top,'top Q'),))
    result=sequence_interface(objects,rows,proof,state,_frame(context,8,'Views seq_u8'),guards)
    for alternative in result['outcomes']:
        require_writes((state.header,state.decoder.header),alternative['possible_writes'],_stable(context,state))
    return result


def _length(objects,rows,context,proof,state,guards):
    parent=context.headers['visitor_frame'];out=borrow(parent,1064,8,parent.phase,True)
    result=decoder_interface(objects,rows,proof,state,_frame(context,32,'Views length Result8'),guards,output=out)
    stable=_object(borrow(state.header.storage,state.header.offset+4,4,parent.phase),'stable D.end')
    for alternative in result['outcomes']:
        require_writes((state.header,out),alternative['possible_writes'],(stable,))
    return result


def _local_effects(context):
    """Full live-object protection at each fixed parent scratch epoch."""
    frame=context.headers['visitor_frame'];phase=frame.phase
    state=context.headers['visitor']['sequence_state'];D=state.decoder.header;top=state.header
    protected=context.headers['visitor']['protected']+(_object(D,'full D'),
        _object(top,'full top Q'),_object(state.decoder.payload,'immutable input'))
    # These locals retain their meaning throughout the loop. S/A/C below are
    # deliberately separate retiring epochs; no union creates initialized data.
    slots=((150,272,4),(188,280,4),(198,276,4),(208,880,4),(210,884,4),(320,284,4))
    for site,offset,size in slots:
        view=borrow(frame,offset,size,phase,True);require_writes((view,),((view,0,size),),protected)
    count=borrow(top.storage,top.offset+4,4,phase,True)
    require_writes((count,),((count,0,4),),context.headers['visitor']['protected']+
        (_object(D,'full D'),_object(borrow(top.storage,top.offset,4,phase),'top Q pointer')))
    cursor=borrow(D.storage,D.offset,4,phase,True)
    require_writes((cursor,),((cursor,0,4),),context.headers['visitor']['protected']+
        (_object(top,'top Q'),_object(borrow(D.storage,D.offset+4,4,phase),'D.end')))
    stages=tuple(_object(borrow(frame,a,n,phase,True),label) for a,n,label in
        ((1064,20,'retiring Result8/Result12/String14'),(1000,19,'A19'),(940,19,'C19'),(880,8,'View Q')))
    retained=_object(borrow(frame,272,16,phase),'saved count/length/String pointer/id')
    require_disjoint(stages+protected+(retained,))
    return dict(fixed_local_writes=slots,D_cursor_write=272,top_Q_count_write=412,
        staging_intervals=tuple((s.start,s.start+s.extent,s.identity) for s in stages),
        Result_to_String_retirement=(296,'successful p/n captured before header reset'),
        String_to_A=(322,326),A_to_C=(334,356),C_last_read=374,
        loop_reuse_after_publication=(388,396),next_Result_write_only_after_C_last_read=True)


def _stage_effects(objects,rows,context,parents,children,index,decoder,string_before,string_length):
    """Bind one affine element's complete string/copy/publication effects.

    The source predicate is a paired p/n domain, not independent choices of
    a pointer and length. For existing Vec effects, all payload bytes supply
    the enclosing initialized read envelope; matched capture instructions
    retain the actual p/n pair and no pointer is chosen at the envelope base.
    """
    frame=context.headers['visitor_frame'];phase=frame.phase;B=context.owner.start;V=parents[0]['function']-82
    q=borrow(frame,880,8,phase,True);S=borrow(frame,1064,20,phase,True)
    guards=context.headers['visitor']['protected']+(_object(decoder.header,'D'),
        _object(decoder.payload,'immutable input'),_object(context.headers['visitor']['sequence_state'].header,'top Q'),_object(q,'View Q'))
    output=borrow(frame,1064,12,phase,True)
    source=UTF8Slice(decoder.payload,string_before,string_length)
    utf8=from_utf8_interface(objects,rows,source,output,_frame(context,24,'View UTF8'),guards)
    require(utf8['success']['source']==source and utf8['normal_preserves']==tuple('r'+str(i) for i in range(4,12))+('sp',),
        'UTF8 loses the same paired slice or caller carriers')
    # Successful UTF8 p/n is captured BEFORE outline222 replaces the tag with
    # empty Vec length. The old Result is now retired, never read as Result.
    vec=VecInterface(S,14,Span(0,0));envelope=InitializedPrefix(decoder.payload,string_length)
    extended=extend_interface(objects,rows,_child(children,parents,302),vec,envelope,
        _frame(context,16,'View String14 append'),guards)
    require(extended['conservative_activation_frames']==(16,24) and 'r5' in extended['normal_preserves'],
        'String append loses id/D/index carriers')
    appended=[v for v in extended['outcomes'] if v['kind']=='success']
    require(bool(appended) and appended[0]['length'].upper<=14,'View string lacks a bounded initialized prefix')
    A=borrow(frame,1000,19,phase,True);C=borrow(frame,940,19,phase,True)
    copytail=children[_edge(parents,322)[2],_edge(parents,322)[3]]
    require(len(copytail['ordinary_edges'])==1 and copytail['ordinary_edges'][0][1]=='b.w' and copytail['ordinary_edges'][0][3]=='__aeabi_memcpy',
        'View copy outline replaces the original BL return token')
    copy=opaque_copy_interface(objects,rows,copytail['ordinary_edges'][0][2],borrow(frame,1065,19,phase),A,19,frame,guards)
    # Exact non-flag-setting inline LDM/STM, LDRH/STRH and byte transport.
    require(A.start%4==C.start%4==0 and (A.start+16)%2==(C.start+16)%2==0,'View staging loses word/halfword alignment')
    require_writes((C,),((C,0,19),),guards+(_object(A,'live A bytes'),_object(S,'live String header/bytes')))
    require(index<=8,'View loop index escaped its bounded prefix')
    element_writes=();second=None
    if index<8:
        element=borrow(context.owner,248+24*index,21,phase,True)
        prefix=() if index==0 else (Storage('address','completed View prefix',B+248,24*index,1,phase,False),)
        require_writes((element,),((element,0,1),(element,1,19),(element,20,1)),guards+prefix)
        destination=borrow(context.owner,249+24*index,19,phase,True)
        second=opaque_copy_interface(objects,rows,_edge(parents,370)[2],borrow(frame,940,19,phase),destination,19,frame,guards+prefix)
        length=borrow(context.owner,244,4,phase,True);require_writes((length,),((length,0,4),),guards)
        element_writes=((element.start,1),(element.start+1,19),(element.start+20,1),(B+244,4))
    return dict(index=index,normal_string_length=(0,min(14,string_length.upper)),
        actual_source_pair='p=payload+cursor_before; n<=payload_end-p',
        cursor_advance_before_UTF8=True,byte_header_reset_after_capture=V+296,
        scratch_epochs=((1064,1072,'varint Result8'),(1064,1076,'UTF8 Result12'),(1064,1084,'String14 Vec')),
        stage_copies=((1065,1000,19),(1000,940,19)),id_domain=(0,255),
        initialized_prefix_before=index,initialized_prefix_after=index+1 if index<8 else index,
        Config_writes=element_writes,precommit_guard=(V+330,V+358,'i==8 excludes every element write'),
        publication=V+388 if index<8 else None,
        semantic_element=('String14 full header and UTF8 name prefix','u8 id at+20') if index<8 else (),
        opaque=('inactive String capacity and two padding bytes','View tail[21,24)'),
        physical_child_paths=((24,),(16,24),(8,),(72,)),return_after_copy=V+326,
        copy_preserves=copy['normal_preserves'],second_copy_preserves=None if second is None else second['normal_preserves'])


def _phase_outcomes(objects,rows,context,parents,children):
    V=parents[0]['function']-82;frame=context.headers['visitor_frame'];B=context.owner.start
    top=context.headers['visitor']['sequence_state'].header
    errors=[];successes=[];iterations=[]
    def error(stage,code,cursor,index,cleared,boot=True,guard=None):
        errors.append(dict(stage=stage,error=code,r5=code,frontier=V+56,cursor=(cursor.lower,cursor.upper),
            top_Q_physical_count=10,initialized_views=index,views_cleared=cleared,boot_view_assigned=boot,
            initial_active_bytes_preserved=175 if cleared else 179 if boot else 180,guard=guard,
            no_rollback=True,unfinished_element_not_published=True))
    for state,assigned,expired in context.success:
        require(state.count==Span(11,11) and len(assigned)==2,'Views did not consume an actual successful u16 prefix')
        boot=_sequence(objects,rows,context,_child(children,parents,82),state)
        for option in boot['outcomes']:
            require(option['count']==Span(10,10),'boot field loses decrement-before-child')
            if option['kind']!='some':
                require(option['kind']=='error4','actual positive boot count produced an unbound None/error')
                error('boot_view',4,option['cursor'],0,False,False);continue
            require(option['value_domain']==(0,255),'boot_view was restricted before semantic validation')
            boot_view=borrow(context.owner,14575,1,frame.phase,True)
            require_writes((boot_view,),((boot_view,0,1),),context.headers['visitor']['protected']+(_object(top,'top Q'),_object(state.decoder.header,'D')))
            decoder=option['state'].decoder
            guards=context.headers['visitor']['protected']+(_object(top,'top Q'),)
            length=_length(objects,rows,context,_child(children,parents,130),decoder,guards)
            for choice in length['outcomes']:
                if choice['kind']!='success':error('Views length',choice['error'],choice['cursor'],0,False);continue
                require(choice['value_domain']==(0,0xffffffff),'Views length lacks source usize domain')
                current=choice['state'];n=current.payload.extent;remaining=n-current.cursor.lower
                if remaining>=9:error('Views pre-clear capacity',14,current.cursor,0,False,guard='N>8 AND remaining>=N')
                clear=borrow(context.owner,244,4,frame.phase,True);require_writes((clear,),((clear,0,4),),guards+(_object(current.header,'D'),))
                for i in range(9):
                    # Completion is the paired branch N==i. N>i follows the
                    # element body only under N<=8 OR remaining_at_length<N.
                    successes.append(dict(initialized_views=i,desired_length=i,cursor=(current.cursor.lower,current.cursor.upper),
                        frontier=V+416,top_count=8,initial_active_bytes_preserved=175,
                        source_guard='N==i; preclear(N<=8 OR remaining<N)',
                        complete_UTF8_string_prefix=True,opaque_capacity_from=i,Widget_bytes_consumed=0))
                    q=borrow(frame,880,8,frame.phase,True)
                    require_writes((q,),((q,0,8),),guards+(_object(current.header,'D'),))
                    element_seq=SequenceInterface(q,current,Span(2,2))
                    ids=_sequence(objects,rows,context,_child(children,parents,214),element_seq)
                    id_success=[v for v in ids['outcomes'] if v['kind']=='some']
                    for v in ids['outcomes']:
                        if v['kind']!='some':error('View id',v['error'],v['cursor'],i,True)
                    if not id_success:break
                    id_state=id_success[0]['state'];require(id_state.count==Span(1,1),'View name lacks current positive count')
                    strings=_length(objects,rows,context,_child(children,parents,246),id_state.decoder,guards+(_object(q,'View Q'),))
                    next_cursor=None
                    for string in strings['outcomes']:
                        if string['kind']!='success':error('View string length',string['error'],string['cursor'],i,True);continue
                        before=string['state'];maximum=n-before.cursor.lower
                        error('View short string',4,before.cursor,i,True,guard='decoded n>remaining; no n-byte advance')
                        # After the exact remaining check, actual p/n stays
                        # paired; no representative pointer or value is chosen.
                        taken=replace(before,cursor=Span(before.cursor.lower,n))
                        stage=_stage_effects(objects,rows,context,parents,children,i,taken,before.cursor,Span(0,maximum))
                        iterations.append(stage)
                        if maximum:error('View UTF8',8,taken.cursor,i,True,guard='same advanced p/n UTF8 error')
                        if maximum>14:error('View String14 capacity',14,taken.cursor,i,True,guard='n>14 after UTF8 success')
                        if i==8:
                            error('View precommit capacity',14,Span(before.cursor.lower,min(n,before.cursor.upper+14)),i,True,
                                guard='N>i AND i==8 after element parsing/staging');continue
                        next_cursor=Span(before.cursor.lower,min(n,before.cursor.upper+14))
                    if next_cursor is None:break
                    current=replace(current,cursor=next_cursor)
    return tuple(errors),tuple(successes),tuple(iterations)


def _child_returns(rows,context,parents,children):
    """Finite SP/save equations for these matched families, per actual call.

    Only the reviewed stack instructions below change the save map. Scalar
    instructions are not executed. Every guarded successor is retained; a
    completed BL must consume its own saved return token and original SP.
    """
    builtin='_RNvNtCs_17compiler_builtins3mem6memcpy'
    stacks={
        '__aeabi_memcpy':{0,4},builtin:{0,4,8,634,636,640},UTF8:{0,4,356,360},
        'vec_extend14':{0,14},'vec_extend_inner':{0,4},
        'construction_restore_r11_low20':{0,4},'construction_prepare_slice':set(),
        'take_varint_u32':{0,4},'restore_high12_low20':{0,4},
        'seq_u8':{4,22},'seq_decrement_tail_pop':set(),'slice_pop':{8,20},'is_zero_r0':set(),
        '@outline:decoded_varint_tag':set(),'@outline:decoded_remaining_length':set(),
        '@outline:decoded_utf8_to_vec14':set(),'@outline:decoded_copy19':set()}
    completed={};nodes=[];active=[]
    def walk(proof,entry,token,expected_sp,cells=(),lr=None,shuttle=None):
        start=proof['function'];family=proof.get('semantic_role',proof['role'])
        require(family in stacks,'Views has no authored child return family')
        key=(start,entry,token,expected_sp,cells,lr,shuttle)
        if key in completed:return completed[key]
        require(start not in active,'Views child introduces an activation cycle');active.append(start)
        body={a:r for a,r in rows.items() if start<=a<start+proof['extent']}
        edges={start+e[0]:e for e in proof['ordinary_edges']}
        require(not proof['indirect_sites'],'Views child adds an executable callback')
        pending=[(start,entry,token if lr is None else lr,cells,shuttle)];seen=set();returns=[];minimum=entry
        while pending:
            at,sp,link,saved,reg2=pending.pop();state=(at,sp,link,saved,reg2)
            if state in seen:continue
            seen.add(state);require(at in body,'Views child successor enters data or outside its body')
            row=body[at];op=row.mnemonic.removesuffix('.w');following=at+len(row.raw)
            control,conditional=_control(row);memory=dict(saved);new_sp=sp;new_link=link;return_pc=None
            stack_step=op.startswith(('push','pop')) or row.operands.startswith('sp,') or bool(re.fullmatch(r'r(?:8|11), \[sp(?:, #-4\]!|\], #4)',row.operands))
            require(not stack_step or at-start in stacks[family],'Views child changes an unreviewed SP/save instruction')
            if at-start in stacks[family]:
                if op.startswith(('push','pop')):
                    regs=row.operands[1:-1].split(', ')
                    if op=='push':
                        new_sp=sp-4*len(regs)
                        for i,reg in enumerate(regs):
                            cell=new_sp+4*i;require(cell not in memory,'Views nested save overwrites a live caller slot')
                            memory[cell]=link if reg=='lr' else reg
                    else:
                        for i,reg in enumerate(regs):
                            cell=sp+4*i;require(cell in memory,'Views restore consumes an unowned save slot');value=memory.pop(cell)
                            if reg=='pc':return_pc=value
                            elif reg=='lr':new_link=value
                            elif reg in ('r2','r3'):
                                require(family=='vec_extend14','Views restores scratch into another register family')
                            else:require(value==reg,'Views restore consumes another caller register')
                        new_sp=sp+4*len(regs)
                elif op in ('add','sub'):
                    count=int(row.operands.split('#')[1],0);new_sp=sp+(count if op=='add' else -count)
                elif op=='str':
                    new_sp=sp-4;require(new_sp not in memory,'Views high save overwrites its caller');memory[new_sp]=row.operands.split(',')[0]
                else:
                    require(op=='ldr' and memory.get(sp)==row.operands.split(',')[0],'Views postindexed restore loses its high register')
                    del memory[sp];new_sp=sp+4
            if family=='slice_pop' and at-start==30:reg2=link
            elif family=='slice_pop' and at-start==36:new_link=reg2
            elif row.operands.startswith('lr,') and not stack_step:new_link=('scratch',at)
            minimum=min(minimum,new_sp);updated=tuple(sorted(memory.items()))
            if at in edges:
                off,operation,target,role,frame=edges[at]
                require(control in ('bl','b') and not conditional,'Views child edge changes call/tail family')
                child=children[target,role]
                child_token=following if control=='bl' else token
                expected=new_sp if control=='bl' else expected_sp
                result=walk(child,new_sp,child_token,expected,updated,child_token if control=='bl' else new_link,reg2)
                minimum=min(minimum,result['lowest_SP'])
                if control=='b':returns.extend(result['returns']);continue
                require(result['restored_cells']==updated,'Views child changes live ancestor save cells')
                new_link=following
            elif return_pc is not None or control=='bx':
                require(control=='return' if return_pc is not None else row.operands=='lr','Views child has an unreviewed return consumer')
                pc=return_pc if return_pc is not None else new_link
                require(pc==token and new_sp==expected_sp,'Views child loses its saved return PC or caller SP')
                returns.append((at,pc,new_sp));continue
            elif control in ('b','cbz','cbnz'):
                target=_direct_target(row,control);require(target in body,'Views child local branch escapes its matched body')
                pending.append((target,new_sp,new_link,updated,reg2))
                if not conditional:continue
            else:require(control is None,'Views child contains an unclassified PC effect')
            require(following in body,'Views child falls outside its complete body')
            pending.append((following,new_sp,new_link,updated,reg2))
        require(returns,'Views child has no completed saved-PC return')
        # A tail restore can consume its parent's cells. Its final map is the
        # caller's enclosing map, not necessarily the cells present at entry.
        final_cells=tuple((a,v) for a,v in cells if a>=expected_sp)
        result=dict(function=start,entry_SP=entry,return_PC=token,return_SP=expected_sp,
            lowest_SP=minimum,returns=tuple(sorted(set(returns))),restored_cells=final_cells,
            architectural_LR_preserved=False)
        nodes.append(result);completed[key]=result;active.pop();return result
    sp=context.headers['visitor_frame'].start
    for parent in parents:
        for off,op,target,role,frame in parent['ordinary_edges']:
            require(op=='bl','Views parent changes its call phase')
            walk(children[target,role],sp,parent['function']+off+4,sp)
    require(nodes and min(n['lowest_SP'] for n in nodes)==sp-72,'Views physical child tail maximum differs')
    return tuple(nodes)


def _closure(objects,rows,context,parents,children):
    """Closed selected invocation; later shared-error callers stay pending."""
    root=context.headers['root']['proof'];visitor=context.headers['visitor']['proof'];V=visitor['function'];R=root['function']
    proofs=[root,visitor,*context.regions,context.outer['proof'],*context.helpers.values(),*parents,*children.values()]
    proofs=list({(p['function'],p['extent']):p for p in proofs if p['function']!=V+54}.values())
    selected={at:row for p in proofs for at,row in rows.items() if p['function']<=at<p['function']+p['extent']}
    cuts={V+54,V+56,V+416,context.headers['root']['checksum_failure']['function']}
    edges={p['function']+off:(op,target,role) for p in proofs for off,op,target,role,frame in p['ordinary_edges']}
    returns=[];successors=[]
    for at,row in selected.items():
        family,conditional=_control(row);following=at+len(row.raw)
        if at in edges:
            op,target,role=edges[at]
            require(family in ('b','bl') and not conditional and target in selected,'Views selected child target is not closed')
            if family=='bl' and at!=R+126:require(following in selected,'Views child return skips its selected caller')
            successors.append((at,target,following if family=='bl' and at!=R+126 else None));continue
        if family in ('b','cbz','cbnz'):
            target=_direct_target(row,family)
            require(target in selected or target in cuts,'Views selected branch leaves its exact frontier')
            successors.append((at,target,following if conditional else None))
            if not conditional:continue
        elif family=='return' or family=='bx' and row.operands=='lr':returns.append(at);continue
        else:require(family is None,'Views selected invocation has an unresolved PC effect')
        require(following in selected or following in cuts,'Views selected fallthrough leaves its exact frontier')
    # Selected local successors plus every child return are closed before any
    # outside context is classified. Exact later entry sites grant no state.
    return_nodes=_child_returns(rows,context,parents,children)
    def pending_call(current,expected):
        """Current typed identity only; no later invocation is activated."""
        require(current.mnemonic=='bl','foreign Views shared-error call family differs')
        target=_direct_target(current,'bl')
        if expected in ('bl @child:tag','bl @child:varint_u32'):
            role=ROLES['outline92_current_varint_tag' if expected=='bl @child:tag' else 'varint_u32']
            matches=[p for p in children.values() if p['role']==role]
            require(len(matches)==1 and target==matches[0]['function'],
                'foreign Views shared-error ordinary target differs')
        elif expected=='bl @pending:f32_seed':
            role=HELPER_SEMANTIC_ROLES['deserialize_seed_f32'];name=_name(objects,role)
            wanted=objects.elf.functions[name][0]&~1
            _helper_function(objects,wanted,role)
            require(target==wanted,'foreign Views shared-error ordinary target differs')
        else:
            require(expected=='bl @pending:varint_from_r8','unreviewed pending Views call identity')
            names=[n for n,e in objects.elf.functions.items() if e[0]&~1==target]
            require(len(names)==1 and re.fullmatch(r'OUTLINED_FUNCTION_[0-9]+',names[0]),
                'pending Views varint transport lacks its exact outline origin')
            def no_object(*args):require(False,'unexpected pending Views transport object')
            proof=_complete_body(objects,names[0],rows,'@pending:varint_from_r8','pending_varint_from_r8',
                PENDING_VARINT_TRANSPORT,no_object,_outline,function_consumer=_helper_function)
            varint=[p for p in children.values() if p['role']==ROLES['varint_u32']]
            require(len(varint)==1 and len(proof['ordinary_edges'])==1 and
                proof['ordinary_edges'][0][2]==varint[0]['function'],
                'pending Views varint transport selects another current child')
        return expected
    intervals=tuple((p['function'],p['function']+p['extent']) for p in proofs)
    complete={p['function'] for p in [context.outer['proof'],*context.helpers.values(),*children.values()]}
    entry={R:context.boundary['history']['preclaim']['startup']['entry']['once']['success']['tail'],V:R+126}
    pending=[]
    for at,row in rows.items():
        if at in selected:continue
        family,conditional=_control(row);following=at+len(row.raw)
        if any(a<=following<b for a,b in intervals):
            require(len(row.raw) in (2,4) and objects.read_only(at,len(row.raw))==row.raw and
                not conditional and family in ('b','bx','return','tbb','tbh'),'unselected fallthrough enters Views selected window')
        direct=_direct_family(row)
        if direct:
            target=_direct_target(row,direct)
            if target in entry:require(at==entry[target] and not conditional,'foreign entry bypasses Views source history')
            elif any(a<=target<b for a,b in intervals):
                require(target in selected,'foreign Views ingress targets an instruction interior')
                if target in complete:continue
                offset=at-V
                require(offset in SHARED_PENDING,'foreign direct ingress skips Views state/prologue')
                witness=SHARED_PENDING[offset]
                require(target==V+witness['target'] and row.mnemonic==witness['operation'],'foreign Views shared-error context differs')
                for off,width,text in witness['context']:
                    current=rows.get(V+off)
                    require(current is not None and len(current.raw)==width and objects.read_only(current.address,width)==current.raw,
                        'foreign Views context has stale or missing bytes')
                    actual=current.mnemonic+' '+current.operands.split('@',1)[0].split('<',1)[0].strip()
                    if text.startswith(('bl @child:','bl @pending:')):actual=pending_call(current,text)
                    else:actual=re.sub(r'0x([0-9a-f]+)',lambda m:'@parent+'+format(int(m[1],16)-V,'x') if V<=int(m[1],16)<V+7128 else m[0],actual)
                    require(actual==text,'foreign Views shared-error predecessor differs')
                pending.append((at,target,'unvisited later invocation; no state/return/owner'))
    # Full ROOT/Visitor computed/implicit audit uses their actual declared
    # extents, not just these selected entry windows.
    for role in (ROOT,VISITOR):
        name=_name(objects,role);start,extent,_=objects.elf.functions[name];start&=~1
        for at,row in rows.items():
            if at in selected or not start<=at<start+extent:continue
            family,conditional=_control(row)
            if family in ('tbb','tbh'):
                match=re.fullmatch(r'\[pc, r([0-9]|1[0-2])'+(r', lsl #1' if family=='tbh' else '')+r'\]',row.operands)
                require(match is not None and not conditional,'Views has an unbound outside computed context')
                low=at+4;high=low+2*(255 if family=='tbb' else 65535)+2
                require(high<=0x100000000 and all(high<=a or b<=low for a,b in intervals),
                    'outside computed context overlaps Views selected windows')
            elif family in ('bx','blx'):require(family=='bx' and not conditional and row.operands=='lr','Views has an unresolved parent indirect ingress')
            else:require(not row.mnemonic.startswith('rfe') and family in (None,'b','bl','cbz','cbnz','return'),
                'Views parent has an unsupported implicit/exception PC effect')
    require({at-V for at,target,reason in pending}==set(SHARED_PENDING),'Views pending shared contexts are missing or retargeted')
    return dict(selected_instructions=len(selected),successors=tuple(successors),nested_returns=return_nodes,
        stopped_frontiers=tuple(sorted(cuts)),outside_shared_contexts=tuple(sorted(pending)),
        pending_contexts_confer_state=False,full_visitor_return_pending=True)


def first_boot_decoded_views(objects,instructions,source_context):
    prefix,context=_first_boot_decoded_u16_state(objects,instructions,source_context)
    parents=_parent(objects,instructions,context);children=_children(objects,instructions,parents)
    locals_=_local_effects(context)
    errors,successes,iterations=_phase_outcomes(objects,instructions,context,parents,children)
    # The enlarged invocation must be checked independently of the public
    # prefix control summary. Deferred diagnostics/restores never become cuts
    # that pretend to prove a nonreturning machine edge.
    closure=_closure(objects,instructions,context,parents,children)
    frame=context.headers['visitor_frame'];S=context.boundary['history']['preclaim']['startup']['copy']['initial_sp']
    low=frame.start-72;_physical(objects,low,S,context.boundary['history']['preclaim']['startup']['copy']['stack_bottom'])
    task=next(v['interval'] for v in context.boundary['physical_protected'] if v['kind']=='Boot_task')
    require(not(task[0]<S and low<task[1]) and context.owner.start+14604<=low,'Views child physically aliases Config/Boot')
    return dict(phase='first_boot_decoded_views_pending_widgets_remaining_visitor_canonical_and_loader_routes',mode='sizez',
        source_context=prefix['source_context'],prefix_errors=tuple(v for v in prefix['outcomes'] if v['error'] is not None),
        boot_view_domain=(0,255),group_errors=errors,successes=successes,iterations=iterations,control=closure,locals=locals_,
        Widget_ready=dict(frontier=context.headers['visitor']['proof']['function']+416,top_count=8,bytes_consumed=0),
        deepest_child_SP=low,physical_child_paths=((8,16),(32,16),(24,),(16,24),(8,),(72,)),
        logical_and_physical_counts=dict(after_u16=11,after_boot=10,Views_active_logical=9,Widget_entry_materialized=8),
        null_string_branch=dict(excluded_by='actual nonnull same-allocation XIP Slice with bounded cursor',
            island=context.headers['visitor']['proof']['function']+6848,machine_nonreturn_established=False),
        shared_error_suffix_pending=True,full_visitor_return_pending=True,initial_facts_reinitialized=False,
        target_policy_unchanged=True,source_qualification=prefix['source_qualification'],external_qualifications=prefix['external_qualifications'],
        requires_H8_consumed_input_closure=True,requires_M2_pre_init_source_reconciliation=True,
        exception_graph_limit=prefix['exception_graph_limit'],exception_absence_established=False,hardware_qualified=False,
        pending=('Widgets and remaining source groups','full visitor error/normal restore','D finalize and canonical Compare',
            'decoded validation and all other loader routes/modes','diagnostic/default sink/PANICKED graph',
            'acquired owner/publication/full resource and final H2 gates'))
