"""Private guard-stopped activation/return composition for the fixed visitor.

This consumes already matched complete bodies and typed argument/write proofs.
It does not evaluate scalars, learn instruction grammars, or select functions
from an external list. The closed stack transitions below belong only to these
reviewed families. A pending diagnostic ends this relation before its callee;
it is never a machine nonreturn or a reusable ownership capability.
"""
from dataclasses import dataclass
import re,struct
from config_regions import _control
from config_stack import transfer_family
from config_refill_contracts import CATALOG_DISPATCH
from config_validator_traversal import TERMINAL_PADDING
from validation import require


# Exact SP/save instructions from the complete matched families. This finite
# map is deliberately separate from general opcode classification. An added
# or moved stack operation cannot become an inferred frame/return rule.
_STACK={
 'validator':{0:'push {r4, r5, r6, r7, lr}',4:'push.w {r7, r8, r9, r10, r11}',
    8:'vpush {d8, d9, d10, d11, d12, d13, d14, d15}',12:'sub sp, #0xd8',
    348:'add sp, #0xd8',350:'vpop {d8, d9, d10, d11, d12, d13, d14, d15}'},
 'range_u16':{0:'push {r7, lr}',14:'pophi {r7, pc}',38:'pop {r7, pc}',46:'pop {r7, pc}'},
 'range_u8':{0:'push {r7, lr}',14:'pophi {r7, pc}',40:'pop {r7, pc}',48:'pop {r7, pc}'},
 'text_valid':{2:'push {r4, r5, r6, r7, lr}',6:'push.w {r8, r9, r11}'},
 'text_restore':{0:'pop.w {r8, r9, r11}',4:'pop {r4, r5, r6, r7, pc}'},
 'contrasts':{0:'push {r4, r5, r7, lr}',52:'pop {r4, r5, r7, pc}'},
 'luminance':{0:'push {r4, r6, r7, lr}',62:'pop {r4, r6, r7, pc}'},
 'channel_domain':{0:'push {r7, lr}',30:'pop {r7, pc}',40:'pop {r7, pc}',50:'pop {r7, pc}'},
 'bands':{10:'push {r7, lr}'},
 'catalog_definition_z':{0:'push {r4, r5, r6, r7, lr}',4:'push.w {r4, r5, r6, r7, r8, r9, r10, r11}',
    634:'pop.w {r0, r1, r2, r3, r8, r9, r10, r11}',638:'pop {r4, r5, r6, r7, pc}'},
 'minimum_period_z':{0:'push {r7, lr}',12:'pop {r7, pc}'},
 'action_allowed':{0:'push {r7, lr}',54:'pop {r7, pc}'},
 'action_destination':{0:'push {r7, lr}',18:'pophi {r7, pc}',40:'pop {r7, pc}',
    44:'pop {r7, pc}',48:'pop {r7, pc}',60:'pop {r7, pc}'},
 'placement_rect':{0:'push {r4, r5, r7, lr}',58:'pop {r4, r5, r7, pc}'},
 'control_error_capture':{0:'push {r4, r6, r7, lr}',48:'pop {r4, r6, r7, pc}'},
 'Color_capture':{0:'push {r4, r5, r7, lr}',54:'pop {r4, r5, r7, pc}'},
 'validator_restore':{0:'add sp, #0x4',2:'pop.w {r8, r9, r10, r11}',6:'pop {r4, r5, r6, r7, pc}'},
 'copy_wrapper':{0:'push {r7, lr}',4:'pop.w {r7, lr}'},
 'copy_builtin':{0:'push {r4, r5, r6, r7, lr}',4:'push.w {r8, r9, r10, r11}',
    8:'sub sp, #0x24',634:'add sp, #0x24',636:'pop.w {r8, r9, r10, r11}',640:'pop {r4, r5, r6, r7, pc}'}}
_ZERO={'first_capture','band_capture','first_tail','zero_to_bool','bounds_eight',
 'f32_exponent','compare_s0_s22','finite_bits_r0','compare_s0_s18','finite_shift_r0',
 'compare_s10_s8','compare_s0_s4','compare_s0_s2','finite_shift_r1'}


@dataclass(frozen=True)
class _Return:
    identity:str
    destination:int
    sp:int


def _target(row):
    text=row.operands.split('@',1)[0].split('<',1)[0].strip()
    found=re.fullmatch(r'(?:r(?:[0-9]|1[0-2]),\s*)?(0x[0-9a-f]+)',text)
    require(found is not None,'guarded activation has an unbound direct target')
    return int(found[1],16)


def _catalogue_table(objects,proof,source):
    dispatch=CATALOG_DISPATCH[proof['shape']];start=proof['function'];site=start+dispatch['site']
    require(proof['shape']=='catalog_definition_z' and 'coverage' in proof and
        (dispatch['count'],dispatch['width'],dispatch['input_domain'])==(16,2,list(range(16))) and
        all(type(v['id']) is int and 0<=v['id']<16 for v in source['iterations']),
        'active catalogue table lacks same-Source DefinitionId provenance')
    table=start+dispatch['table_offset'];require(table==site+4,'catalogue table loses its PC-relative base')
    targets=tuple(table+2*v for v in struct.unpack('<16H',objects.read_only(table,32)))
    require(targets==tuple(start+v for v in dispatch['target_offsets']),
            'active catalogue current target tuple differs')
    return site,targets


def guard_stopped_normal_activation_closure(objects,rows,parent,context,source,coverage,bindings,
                                           controls,normal,catalogue,description,relations):
    """Derive invocation nodes and PC/save transports, never accept a graph."""
    q=context.frame;call=description['before_validator']['call'];P=context.caller.start
    require(parent['frame']==320 and q.start+320==P and context.result.start==P+236,
            'guarded activation loses its privately derived default caller')
    invocations={};cuts=[];completed={};active=[];projection=set();computed={};states_checked=0
    invocation_ids={(None,call,'selected_call'):'v0'}
    writes=relations['checked_owned_write_intervals']
    require(writes and all(type(lo) is int and type(hi) is int and lo<hi for lo,hi in writes),
            'activation lacks the actual typed owned-write union')
    parent_tables={a:tuple(targets) for a,role,domain,targets in coverage['tables']}
    require(len(parent_tables)==4 and {role for a,role,domain,targets in coverage['tables']}==
        {'rotation','constructed ColorRole subset','typed LimitMode','guarded Access tag minus one'},
        'guarded activation loses a typed parent selector family')
    require(controls['loops']['rotation']==(0,1,2,3) and
        len(source['widget']['iterations'])==len(source['iterations'])==8,
        'parent tables lose the initialized source/paired loop transport')

    def dependencies(binding):
        proof=binding['proof'];family=binding['family'];result={};start=proof['function']
        if family in ('text_valid','contrasts'):
            for offset,op,target,role,frame in proof['ordinary_edges']:
                child=proof['children']['text_restore' if role=='@outline:text_restore' else offset]
                wanted='text_restore' if role=='@outline:text_restore' else 'range_u8' if family=='text_valid' else 'luminance'
                result[start+offset]=dict(proof=child,family=wanted,arguments=binding['arguments'],
                    effects=(),cut=False)
        elif family=='copy_wrapper':
            require(len(binding['copy_bodies'])==2,'copy activation loses its full restored tail')
            edge=proof['ordinary_edges'];require(len(edge)==1 and edge[0][0]==8,'copy tail moves before LR restoration')
            result[start+8]=dict(proof=binding['copy_bodies'][1],family='copy_builtin',
                arguments=binding['arguments'],effects=binding['effects'],cut=False)
        elif family=='action_allowed':
            child=controls['children']['zero_to_bool']
            result[start+50]=dict(proof=child,family='zero_to_bool',arguments=binding['arguments'],effects=(),cut=False)
        elif family in ('bands','bounds_eight'):
            require(len(proof['ordinary_edges'])==1,'diagnostic frontier gains an unknown normal callee')
            offset,op,target,role,frame=proof['ordinary_edges'][0]
            require((family,offset,role) in (
                ('bands',20,'_RNvNtNtCs_4core5slice5index16slice_index_fail'),
                ('bounds_eight',2,'_RNvNtCs_4core9panicking18panic_bounds_check')),
                'a normal child is relabelled as a diagnostic frontier')
            result[start+offset]=dict(cut=True,target=target,role=role,arguments=binding['arguments'])
        return result

    def table_for(binding,at):
        proof=binding['proof'];family=binding['family'];start=proof['function']
        if family=='validator':
            require(at in parent_tables,'active parent computed transfer lacks its source phase')
            return parent_tables[at]
        if family=='catalog_definition_z':
            site,targets=_catalogue_table(objects,proof,source);require(at==site,'catalogue gains another computed site')
            return targets
        if family=='action_allowed':
            require(at==start+8 and binding['arguments']['source']=='paired ControlAction/ControlPage loops' and
                controls['children']['action_allowed']['source_action_indices']==tuple(range(19)),
                'allowed_on table loses source action/page provenance')
            return tuple(at+4+2*v for v in objects.read_only(at+4,19))
        require(family=='action_destination' and at==start+20 and
            controls['pointer']['children']['action_destination']['guarded_table_indices']==(0,1,2,3,4) and
            binding['arguments']['source']=='paired ControlAction/ControlPage loops',
            'normal computed transfer has no typed source domain')
        return tuple(at+4+2*v for v in objects.read_only(at+4,5))

    def walk(binding,identity,parent_id,transfer,edge_site,entry_sp,token,cells,lr,expected_cells):
        nonlocal states_checked
        proof=binding['proof'];family=binding['family'];start=proof['function'];end=start+proof['extent']
        require(family in _STACK or family in _ZERO,'unreviewed normal return family')
        require(start not in active,'normal activation introduces a call/tail cycle')
        key=(identity,entry_sp,token,tuple(sorted(cells.items())),lr,tuple(sorted(expected_cells.items())))
        if key in completed:return completed[key]
        node=dict(identity=identity,parent=parent_id,family=family,function=start,extent=proof['extent'],
            transfer=transfer,calling_site=edge_site,entry_SP=entry_sp,return_SP=token.sp,return_to=token.destination,
            return_token=token.identity,source_receiver_phase=binding['arguments'],return_sites=[])
        if identity in invocations:require(invocations[identity]['entry_SP']==entry_sp,'same invocation gains another SP')
        else:invocations[identity]=node
        active.append(start);body={a:r for a,r in rows.items() if start<=a<end};steps=_STACK.get(family,{})
        require(bool(body) and not proof['indirect_sites'],'normal activation has no complete fixed body or gains a callback')
        for offset,text in steps.items():
            row=body.get(start+offset)
            require(row is not None and row.mnemonic+' '+row.operands==text,'normal return family changes exact SP/save step')
        for at,row in body.items():
            stack=row.mnemonic.startswith(('push','pop','vpush','vpop')) or re.match(r'sp(?:,|$)',row.operands)
            require(not stack or at-start in steps,'normal activation has an unreviewed SP/stack instruction')
        edges={start+v[0]:v for v in proof['ordinary_edges']}
        children=bindings if family=='validator' else dependencies(binding)
        require(set(children)==set(edges),'normal activation leaves a direct child unclassified')
        # Partition every current PC effect, including code not selected by a
        # source-valid table arm. Reachability below then assigns concrete
        # per-invocation SP/return state to each activated successor.
        for at,row in body.items():
            pc_family,conditional=_control(row)
            if pc_family in ('tbb','tbh'):
                targets=table_for(binding,at)
                require(targets and all(t in body for t in targets),'typed current table leaves its matched code')
            elif at in edges:
                require(pc_family in ('b','bl') and not conditional and _target(row)==edges[at][2],
                        'complete current activation has another direct child effect')
            elif pc_family in ('b','cbz','cbnz'):
                require(_target(row) in body,'complete normal body has an unbound outgoing successor')
            elif pc_family=='bx':require(row.operands=='lr','complete normal body has another executable address consumer')
            else:require(pc_family in (None,'return'),'complete normal body gains an implicit/security/exception PC effect')
        pending=[(start,entry_sp,lr,tuple(sorted(cells.items())))];seen=set();returns=[];used=set();min_sp=entry_sp
        while pending:
            at,sp,current_lr,stored=pending.pop();state=(at,sp,current_lr,stored)
            if state in seen:continue
            seen.add(state);require(at in body,'normal successor enters data/padding/end')
            states_checked+=1
            row=body[at];operation=row.mnemonic.removesuffix('.w');following=at+len(row.raw)
            if family=='validator' and at-start in TERMINAL_PADDING:
                raise AssertionError('guarded normal activation reaches terminal padding')
            projection.add(at);family_pc,conditional=_control(row);slot_map=dict(stored);new_sp=sp;new_lr=current_lr;pc=None;pc_slot=None
            if at-start in steps:
                if operation.startswith(('push','vpush')):
                    registers=re.fullmatch(r'\{([^}]+)\}',row.operands)
                    require(registers is not None,'save family loses its register list')
                    registers=registers[1].split(', ');width=8 if operation=='vpush' else 4
                    new_sp=sp-len(registers)*width
                    require(not any(new_sp<=v<sp for v in slot_map),'descendant save aliases a live ancestor slot')
                    for i,reg in enumerate(registers):
                        slot=new_sp+i*width
                        if reg=='lr':
                            require(not any(lo<slot+4 and slot<hi for lo,hi in writes),
                                    'typed descendant/parent output aliases a saved return slot')
                        slot_map[slot]=current_lr if reg=='lr' else reg
                elif operation.startswith(('pop','vpop')):
                    registers=re.fullmatch(r'\{([^}]+)\}',row.operands)
                    require(registers is not None,'restore family loses its register list')
                    registers=registers[1].split(', ');width=8 if operation=='vpop' else 4
                    for i,reg in enumerate(registers):
                        slot=sp+i*width;require(slot in slot_map,'restore reads a slot not owned by this invocation')
                        value=slot_map.pop(slot)
                        if reg=='pc':pc=value;pc_slot=slot
                        elif reg=='lr':new_lr=value
                        elif reg not in ('r0','r1','r2','r3'):
                            require(value==reg,'normal restore consumes another saved register cell')
                        else:
                            require(family=='catalog_definition_z' and value==('r4','r5','r6','r7')[i],
                                    'unreviewed scratch restore substitutes a saved caller register')
                    new_sp=sp+len(registers)*width
                else:
                    value=int(row.operands.split('#')[1],0);new_sp=sp+(value if operation=='add' else -value)
                    if new_sp<sp:require(not any(new_sp<=v<sp for v in slot_map),'local allocation aliases a saved ancestor')
                    else:
                        for slot in tuple(slot_map):
                            if sp<=slot<new_sp:
                                require(not isinstance(slot_map[slot],_Return),'SP discard destroys a live saved return')
                                slot_map.pop(slot)
                min_sp=min(min_sp,new_sp)
            if at in children:
                offset,actual_op,target,role,frame=edges[at];child=children[at];used.add(at)
                require(family_pc in ('bl','b') and not conditional and _target(row)==target and row.mnemonic==actual_op,
                        'activation child loses exact call/tail binding')
                if child['cut']:
                    require(target==child['target'] and role==child['role'],'diagnostic cut changes actual target')
                    cuts.append(dict(kind='pending_diagnostic_cut',invocation=identity,site=at,target=target,
                        role=role,SP=sp,retained_return_slots=tuple(v for v,x in stored if isinstance(x,_Return)),
                        retained_geometry=dict(Q=q.start,R=context.result.start,Config=context.config.start,
                            first=(q.start+76,12),capture=(q.start+88,4),valid=(q.start+95,1)),
                        source_receiver_phase=child['arguments'],return_token=None,machine_nonreturn_established=False))
                    continue
                require(child['proof']['function']==target and child['proof']['frame']==frame,
                        'normal child proof belongs to another call target/frame')
                is_call=family_pc=='bl';transfer_kind='call' if is_call else 'tail'
                child_key=(identity,at,transfer_kind)
                if child_key not in invocation_ids:invocation_ids[child_key]='v'+str(len(invocation_ids))
                child_id=invocation_ids[child_key]
                child_token=_Return(child_id,following,sp) if is_call else token
                child_cells=slot_map if is_call else expected_cells
                returned=walk(child,child_id,identity,transfer_kind,at,new_sp,child_token,
                    slot_map,child_token if is_call else new_lr,child_cells)
                if is_call:
                    if returned:
                        require(following in body,'normal child return falls outside its actual caller')
                        pending.append((following,new_sp,None,tuple(sorted(slot_map.items()))))
                else:returns.extend(returned)
                continue
            if family_pc=='return' or family_pc=='bx':
                require(family_pc=='return' or row.operands=='lr','normal activation gains another executable address consumer')
                value=pc if family_pc=='return' else current_lr
                require(value==token and new_sp==token.sp and slot_map==expected_cells,
                        'normal return loses original continuation/SP/protected saved cells')
                route=dict(site=at,PC=token.destination,SP=new_sp,
                    route='saved_PC' if family_pc=='return' else 'incoming_LR',saved_return_slot=pc_slot,
                    token=token.identity)
                returns.append(route)
                if route not in invocations[identity]['return_sites']:invocations[identity]['return_sites'].append(route)
                if conditional:pending.append((following,sp,current_lr,stored))
                continue
            if family_pc in ('tbb','tbh'):
                targets=table_for(binding,at)
                require(targets and all(v in body for v in targets),'active computed target leaves checked current body')
                computed[at]=targets
                pending.extend((v,new_sp,new_lr,tuple(sorted(slot_map.items()))) for v in targets);continue
            if family_pc in ('b','cbz','cbnz'):
                target=_target(row);require(target in body,'normal local successor leaves its matched family')
                pending.append((target,new_sp,new_lr,tuple(sorted(slot_map.items()))))
                if conditional:pending.append((following,new_sp,new_lr,tuple(sorted(slot_map.items()))))
                continue
            require(family_pc is None,'normal activation has an unknown PC/security/exception effect')
            if re.match(r'lr(?:,|$)',row.operands) or operation=='ldrd' and re.match(r'[^,]+, lr,',row.operands):new_lr=None
            require(following in body,'normal executable fallthrough enters data/padding/end')
            pending.append((following,new_sp,new_lr,tuple(sorted(slot_map.items()))))
        require(used==set(children),'normal activation has an unreachable/unbound declared child')
        require(min_sp>=entry_sp-proof['frame'] or family in ('text_restore','validator_restore'),
                'activation exceeds its reviewed physical frame')
        invocations[identity]['minimum_SP']=min_sp
        active.pop();completed[key]=tuple(returns);return tuple(returns)

    root_token=_Return('v0',call+4,P)
    root=dict(proof=parent,family='validator',arguments=dict(source='same private checksum/refill source',
        B=context.config.start,R=context.result.start,Q=q.start,phase=context.phase),effects=(),cut=False)
    returned=walk(root,root_token.identity,None,'selected_call',call,P,root_token,{},root_token,{})
    require(returned and all(v['PC']==call+4 and v['SP']==P for v in returned),
            'guard-stopped activation has no sole checked validator continuation')
    for frontier in description['pending_diagnostics']:
        cuts.append(dict(kind='pending_diagnostic_cut',invocation='inherited checksum/refill',retained_frontier=frontier,
            return_token=None,machine_nonreturn_established=False))
    # Projection follows the completed contextual relation; it is never an
    # acceptance set supplied by a caller or assembled from role names.
    unique=[]
    for cut in cuts:
        if cut not in unique:unique.append(cut)
    return dict(scope='selected ordinary execution before any pending diagnostic cut',
        return_to=call+4,invocations=tuple(invocations.values()),pending_diagnostic_cuts=tuple(unique),
        instruction_states=states_checked,closed_invocation_count=len(invocations),executable_site_count=len(projection),
        diagnostics_entered_or_nonreturn_proved=False,expires_at='first pending cut, exception or validator return'),projection,computed


def computed_context_dispositions(objects,rows,parent,projection,computed,closure):
    """Bind foreign envelopes only after complete internal forward closure."""
    start=parent['function'];end=start+parent['extent'];result=[]
    require(closure['return_to'] and closure['invocations'] and projection,
            'computed disposition has no closed guarded activation')
    for at,row in rows.items():
        family=transfer_family(row.mnemonic.removesuffix('.w'))
        if family not in ('tbb','tbh'):continue
        require(re.fullmatch(r'\[pc, r(?:[0-9]|1[0-2])'+(r', lsl #1' if family=='tbh' else '')+r'\]',row.operands),
                'computed disposition gains an unbound table base')
        low=at+4;high=low+2*(65535 if family=='tbh' else 255)+2
        require(0<=low<high<=0x100000000,'computed envelope wraps the address space')
        if at in projection:
            require(at in computed,'activated computed transfer has no source-valid current tuple')
            result.append(dict(site=at,status='selected_current_source_targets',targets=computed[at],raw=row.raw.hex()))
        elif low<end and start<high:
            owners=tuple((name,address&~1,n,index) for name,(address,n,index) in objects.elf.functions.items()
                         if address&~1<=at<at+len(row.raw)<=(address&~1)+n)
            require(owners,'outside computed site has no current declared function extent')
            require(objects.read_only(at,len(row.raw))==row.raw,'outside computed site bytes differ')
            result.append(dict(site=at,status='outside_selected_guard_stopped_prefix',raw=row.raw.hex(),
                architectural_envelope=(low,high),current_owners=owners,foreign_context_validated=False,
                reason='closed selected successors/children/paired returns and stopped diagnostic frontiers',
                diagnostic_or_other_invocation_context_remains_unresolved=True))
    return tuple(result)
