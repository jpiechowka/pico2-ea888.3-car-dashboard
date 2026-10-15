"""First-Boot CRC-success prefix with abstract u16 values and guarded stores.

This is a finite consumer of the existing source/header/helper contracts. It
stops before the shared error suffix or boot_view, never before a purported
whole visitor return. The public description cannot authorize a later phase.
Physical and source qualifications are inherited, not discharged by this API.
"""
import re
import struct
from config_abstract import Span,DecoderInterface,SequenceInterface
from config_decode_effects import _object
from config_headers import ROOT,VISITOR,NEXT_U16,root_visitor_headers,_u16_field_regions,_next_u16_interface,U16_FIELDS
from config_helpers import helper_body,HELPER_SEMANTIC_ROLES
from config_initial import _partition
from config_loader_boundary import first_boot_loader_boundary,_physical
from config_owner_entry import _direct_target
from config_regions import Storage,borrow,require_disjoint,require_writes,_control
from config_stack import canonical_symbol
from validation import require


def _name(objects,role,address=None):
    names=[n for n,e in objects.elf.functions.items() if canonical_symbol(n)==role and (address is None or e[0]&~1==address)]
    require(len(names)==1,'decoded prefix has no unique current typed target')
    return names[0]


def _geometry(objects,boundary):
    require(boundary['mode']=='sizez','decoded u16 prefix has no consuming variant for this mode')
    g=boundary['loader_geometry'];S=boundary['history']['preclaim']['startup']['copy']['initial_sp'];P,end=g['frame']
    require(g['root_frame_bytes']==456 and (P,end)==(S-544,S-88),'decoded prefix loses same restored loader SP')
    frame=Storage('stack:Reset:'+hex(S),'actual CRC-success loader',P,456,8,'decoded_u16_prefix',True)
    B,n=boundary['enclosing_allocation'];require(n==14604,'decoded prefix changes enclosing static extent')
    ledger={v['kind']:v['interval'] for v in boundary['physical_protected']}
    require(ledger['retained_callers']==(end,S) and end<=ledger['executor_P'][0]<ledger['executor_P'][1]<=S,
        'decoded prefix loses executor constituent under retained callers')
    guards=tuple(Storage('address' if k=='Boot_task' else frame.space,k,a,b-a,1,frame.phase,False)
        for k in ('Boot_task','retained_callers') for a,b in (ledger[k],))
    guards+=(Storage('address','all slot metadata/taken/padding',B+14576,28,4,frame.phase,False),)
    require_disjoint((frame,)+guards)
    owner=Storage('address','same private Config allocation',B,14604,4,frame.phase,True)
    return frame,guards,owner,S


def _helpers(objects,rows,outer):
    """Complete current helper family, including parent-bound zero-frame tails."""
    result={};pending=list(outer['proof']['ordinary_edges'])
    while pending:
        off,op,target,role,frame=pending.pop()
        if role in result:
            require(result[role]['function']==target,'decoded helper role names two targets');continue
        if role.startswith('@outline:'):
            names=[n for n,e in objects.elf.functions.items() if e[0]&~1==target]
            require(len(names)==1,'decoded helper outline is ambiguous');name=names[0]
            proof=helper_body(objects,name,rows,role)
        else:proof=helper_body(objects,_name(objects,role,target),rows)
        require(proof['frame']==frame and not proof['indirect_sites'],'decoded helper changes its direct ABI')
        result[role]=proof;pending.extend(proof['ordinary_edges'])
    expected=('seq_u16','seq_decrement_tail_u16','deserialize_u16','slice_pop','is_zero_r0','restore_r11_low20')
    require(set(result)=={HELPER_SEMANTIC_ROLES[s] for s in expected},'decoded prefix has an unclassified nested child')
    return {s:result[HELPER_SEMANTIC_ROLES[s]] for s in expected}


def _returns(outer,helpers,V,site):
    """Authored save/restore equations for the matched fixed six-helper family.

    BL overwrites architectural LR. Saved caller PCs and the Slice r2 shuttle
    survive it; the descriptor never claims that LR itself stays invariant.
    Seq's decrement outline tail keeps the Seq activation live. Deserialize's
    zero-frame restore consumes that parent's pending24-byte save map.
    """
    w=outer['proof']['function'];seq=helpers['seq_u16']['function'];d=helpers['deserialize_u16']['function']
    pop=helpers['slice_pop']['function'];zero=helpers['is_zero_r0']['function'];restore=helpers['restore_r11_low20']['function']
    require(outer['conservative_activation_frames']==(8,8,24,16) and
        {'r4','r8','sp'}<=set(outer['normal_preserves']),'decoded next loses its actual destination/source/save ABI')
    return dict(call=site,destination=site+4,entry_SP=V,return_SP=V,saved_PC=(V-4,V),
        wrapper=(w,54),inner=dict(call=w+4,destination=w+8,saved_PC=(V-12,V-8),SP=V-8,
            restores_LR_at=seq+14,returns_at=seq+48),
        decrement_tail=dict(call=seq+8,return_PC=seq+12,entry_SP=V-16,
            target=helpers['seq_decrement_tail_u16']['function'],tail_to=d),
        deserialize=dict(destination=seq+12,saved_PC=(V-20,V-16),SP=V-16,restore=restore,pending_frame=24),
        pop=dict(call=d+22,destination=d+26,saved_PC_on_nonempty=(V-44,V-40),entry_SP=V-40,
            restore_before_zero_call=pop+20,LR_shuttle=('r2',pop+30,pop+36),
            leaf_call=pop+32,leaf_target=zero,leaf_return=pop+36,return_at=pop+38),
        physical_active_saves=((V-8,V),(V-16,V-8),(V-40,V-16),(V-56,V-40)),
        architectural_LR_preserved_across_BL=False)


def _selected_control(objects,rows,boundary,headers,regions,outer,helpers):
    """Closed selected successors, with unvisited contexts explicitly pending.

    No computed transfer executes in this prefix/helper family. Unvisited
    same-parent PC-relative tables may be excluded only by their architectural
    forward envelope; unknown implicit PC/indirect forms fail closed. External
    contexts cannot authorize an interior entry, nor be reused as owners.
    """
    root=headers['root']['proof'];visitor=headers['visitor']['proof'];w=outer['proof']
    R=root['function'];V=visitor['function']
    proofs=[root,visitor,*regions,w,*helpers.values()]
    selected={}
    for p in proofs:
        # Shared error suffix bytes are bound by the reused grammar but are
        # not executed at this component's error frontier.
        if p['function']==V+54:continue
        for at,row in rows.items():
            if p['function']<=at<p['function']+p['extent']:
                require(at not in selected or selected[at]==row,'decoded prefix overlapping roles differ');selected[at]=row
    cuts={V+54,V+82,headers['root']['checksum_failure']['function']}
    explicit={}
    for p in proofs:
        for off,op,target,role,frame in p['ordinary_edges']:
            at=p['function']+off
            require(at in selected and rows[at].mnemonic==op,'decoded selected call inventory differs')
            explicit[at]=(target,role,op)
    require(explicit[R+126][0]==V and explicit[V+24][0]==w['function'] and explicit[V+46][0]==w['function'],
        'decoded prefix loses same-invocation nested call/return order')
    branch=[];returns=[]
    for at,row in selected.items():
        family,conditional=_control(row);following=at+len(row.raw)
        if at in explicit:
            target,role,op=explicit[at];require(family in ('b','bl') and not conditional and target in selected,
                'decoded selected child has an unclosed target')
            if family=='bl' and at!=R+126:require(following in selected,'decoded child returns outside the selected prefix')
            branch.append((at,op,target,'pending visitor return' if at==R+126 else following if family=='bl' else None));continue
        if family in ('b','cbz','cbnz'):
            target=_direct_target(row,family)
            require(target in selected or target in cuts,'decoded selected branch leaves its source frontier')
            if conditional:require(following in selected or following in cuts,'decoded guard falls outside its exact frontier')
            branch.append((at,row.mnemonic,target,following if conditional else None))
        elif family=='return' or family=='bx' and row.operands.strip()=='lr':returns.append(at)
        else:
            require(family is None,'decoded prefix has an unsupported computed/indirect PC effect')
            require(following in selected or following in cuts,'decoded prefix falls through unclassified bytes')
    # These are the only real returns in the complete current nested family.
    expected={w['function']+54,helpers['seq_u16']['function']+48,
        helpers['slice_pop']['function']+38,helpers['is_zero_r0']['function']+6,
        helpers['restore_r11_low20']['function']+4}
    require(set(returns)==expected,'decoded prefix return consumer differs')
    entrances={R:boundary['history']['preclaim']['startup']['entry']['once']['success']['tail'],V:R+126}
    ranges=[(p['function'],p['function']+p['extent']) for p in proofs if p['function']!=V+54]
    # A selected helper entry has legitimate other callers in unvisited phases;
    # their invocations remain pending. No such edge may skip a helper prologue.
    complete_entries={p['function'] for p in [w,*helpers.values()]}
    parents=[]
    for role in (ROOT,VISITOR):
        name=_name(objects,role);a=objects.elf.functions[name][0]&~1;extent=objects.elf.functions[name][1];parents.append((a,a+extent))
    for at,row in rows.items():
        if at in selected:continue
        within=any(a<=at<b for a,b in parents)
        if within:
            family,conditional=_control(row)
            if family in ('tbb','tbh'):
                match=re.fullmatch(r'\[pc, r([0-9]|1[0-2])'+(r', lsl #1' if family=='tbh' else '')+r'\]',row.operands.split('@',1)[0].strip())
                require(match is not None and not conditional and len(row.raw)==4 and
                    row.raw==struct.pack('<HH',0xe8df,(0xf010 if family=='tbh' else 0xf000)|int(match[1])),
                    'decoded prefix has an unresolved parent computed ingress')
                low=at+4;high=low+2*(255 if family=='tbb' else 65535)+2
                require(high<=0x100000000 and all(high<=a or b<=low for a,b in ranges),
                    'decoded parent computed target envelope overlaps prefix')
            elif family in ('bx','blx'):
                require(family=='bx' and not conditional and row.operands.strip()=='lr',
                    'decoded prefix has an unresolved parent indirect ingress')
            else:require(not row.mnemonic.startswith('rfe') and family in (None,'b','bl','cbz','cbnz','return'),
                'decoded parent has an unsupported implicit/exception PC effect')
        if _direct_family(row):
            family=_direct_family(row);target=_direct_target(row,family)
            if target in entrances:
                require(at==entrances[target] and not _control(row)[1],'foreign entry bypasses decoded source history')
            elif any(a<=target<b for a,b in ranges):
                require(target in complete_entries,'foreign direct ingress skips decoded state/prologue')
    return dict(selected_instruction_count=len(selected),local_edges=tuple(branch),nested_returns=tuple(sorted(returns)),
        stopped_frontiers=tuple(sorted(cuts)),unvisited_parent_and_other_invocations_pending=True,
        computed_transfers_executed=(),direct_foreign_interior_excluded=True)


def _direct_family(row):
    # transfer_family includes security/conditional indirect forms; the finite
    # classifier above rejects them when they occur in either selected parent.
    from config_stack import transfer_family
    f=transfer_family(row.mnemonic.removesuffix('.w'))
    return f if f in ('b','bl','cbz','cbnz') else None


def first_boot_decoded_u16_prefix(objects,instructions,source_context):
    boundary=first_boot_loader_boundary(objects,instructions,source_context)
    frame,guards,owner,S=_geometry(objects,boundary);B=owner.start
    name=_name(objects,ROOT,boundary['loader_geometry']['function'])
    headers=root_visitor_headers(objects,name,instructions,frame,guards)
    active=headers['visitor_frame'];visitor=headers['visitor'];V=visitor['proof']['function'];vf=active.start
    sequence=visitor['sequence_state'];D=sequence.decoder
    state=SequenceInterface(sequence.header,DecoderInterface(D.header,D.payload,Span(0,0)),Span(13,13))
    require((D.header.start,state.header.start)==(frame.start+236,vf+288) and state.decoder.header.extent==8 and
        state.header.extent==8 and headers['root']['call_site']==boundary['loader_geometry']['function']+126,
        'decoded constructor loses D/Q/source epoch')
    regions=_u16_field_regions(objects,_name(objects,VISITOR,V),instructions,active)
    require(regions[0]['objects']=={0:B},'decoded field destination is not same initialized B')
    edges=visitor['proof']['ordinary_edges'];require(len(edges)==1 and edges[0][0]==24,'decoded first wrapper edge differs')
    wname=_name(objects,NEXT_U16,edges[0][2]);nested=Storage(frame.space,'abstract next_u16',vf-8,8,8,frame.phase,True)
    protected=visitor['protected']+(_object(state.decoder.payload,'immutable-input'),)
    owner_payload=Storage('address','mutable Config payload',B,14576,4,frame.phase,True)
    require_disjoint((owner_payload,)+protected+(_object(state.header,'Q'),_object(state.decoder.header,'D')))
    outer=_next_u16_interface(objects,wname,instructions,state,nested,visitor['protected'])
    helpers=_helpers(objects,instructions,outer)
    control=_selected_control(objects,instructions,boundary,headers,regions,outer,helpers)
    returns=tuple(_returns(outer,helpers,vf,V+site) for site in (24,46))
    low=vf-56;bottom=boundary['history']['preclaim']['startup']['copy']['stack_bottom'];_physical(objects,low,S,bottom)
    # Physical before-space comparison protects Boot/B as well as section/load
    # aliases. Both nested invocations have the same deepest stack and saves.
    task=next(v['interval'] for v in boundary['physical_protected'] if v['kind']=='Boot_task')
    require(not (task[0]<S and low<task[1]) and B+14604<=low,'decoded child physically aliases Config/Boot')
    active_bytes,_=_partition();require(boundary['history']['preclaim']['Config_write_intersections']==((B+14600,1,'claim_transition'),),
        'decoded prefix changes initial-fact preservation before claim')
    initial=boundary['history']['preclaim']['startup']['entry']['initial']
    require((initial['address'],initial['extent'],initial['active_bytes'],initial['opaque_bytes'],
        initial['initial_collection_lengths'])==(B,14604,185,14419,(0,0)),
        'decoded prefix loses copied initialized active/opaque source state')
    initial_live=set(active_bytes)-{14600};outcomes=[];pending=[(state,(),(),outer)]
    for ordinal,(field,offset) in enumerate(U16_FIELDS):
        next_pending=[]
        for before,fields,retired,bound in pending:
            for alternative in bound['outcomes']:
                after=alternative['state'];error=alternative['error']
                require(after.count==Span(12-ordinal,12-ordinal) and error in (None,4,5),
                    'positive Config Q count cannot produce missing or another error')
                if error is None:
                    view=borrow(owner,offset,2,frame.phase,writable=True)
                    require_writes((view,),((view,0,2),),protected+(_object(state.header,'Q'),_object(state.decoder.header,'D')))
                    assigned=fields+((field,(0,65535)),);expired=retired+((offset,offset+2),)
                else:assigned=fields;expired=retired
                if error is not None or ordinal==1:
                    expired_bytes={i for a,b in expired for i in range(a,b)}
                    outcomes.append(dict(completed_fields=len(assigned),field_domains=assigned,error=error,
                        calls=tuple(V+off for off in (24,46)[:ordinal+1]),
                        packed_r0=alternative['packed_r0'],count=(after.count.lower,after.count.upper),
                        cursor=(after.decoder.cursor.lower,after.decoder.cursor.upper),frontier=V+(54 if error is not None else 82),
                        initial_facts_retired=expired,initial_active_bytes_preserved=len(initial_live-expired_bytes),
                        initialized_remainder_preserved=True,initial_collection_lengths=(0,0),
                        writes=tuple((B+o,2) for f,o in U16_FIELDS[:len(assigned)])))
                else:
                    second=_next_u16_interface(objects,wname,instructions,after,nested,visitor['protected'])
                    require('r4' in second['normal_preserves'],'second wrapper changes actual Config r4')
                    next_pending.append((after,assigned,expired,second))
        pending=next_pending
    return dict(phase='first_boot_decoded_u16_prefix_pending_remaining_visitor_canonical_and_loader_routes',mode='sizez',
        config=(B,14604),root_frame=(frame.start,456),visitor_frame=(vf,1608),decoder=(D.header.start,8),sequence=(state.header.start,8),
        guarded_CRC_success=(headers['root']['proof']['function']+114,'equal complement then fallthrough to D and visitor'),
        CRC_success_observed_taken=False,outcomes=tuple(outcomes),control=control,nested_return_sites=returns,
        deepest_child_SP=low,possible_field_writes=((B+14570,2),(B+14572,2)),initial_collection_lengths=(0,0),
        initial_facts_consumed_once=True,initial_facts_not_revalidated_after_mutation=True,
        D_end_and_Q_pointer_preserved=True,protected_static_suffix=(B+14576,B+14604),
        executor_constituent=boundary['physical_protected'][1],nested_frame_metadata=(456,1608,8,8,24,16),
        source_context=boundary['history']['source_context'],source_qualification=boundary['source_qualification'],
        requires_H8_consumed_input_closure=True,requires_M2_pre_init_source_reconciliation=True,
        external_qualifications=boundary['external_qualifications'],exception_graph_limit=boundary['exception_graph_limit'],
        exception_absence_established=False,hardware_qualified=False,
        pending=('shared error suffix/visitor restore','boot_view and all remaining visitor groups','finalize and complete canonical Compare',
            'decoded validator and all other loader alternatives','diagnostic/default Formatter/sink/PANICKED closure',
            'complete same-B loader before acquired owner','full graph and resource gates'))
