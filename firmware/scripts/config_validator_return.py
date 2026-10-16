"""Actual selected checksum/refill -> guarded normal validator return.

All mutable source/frame state is recomputed privately. The public result is
only a readonly description pending diagnostics and loader continuation, and
never an owner, validated/publication capability, policy or resource result.
"""
from dataclasses import replace
import re
from config_refill_handoff import _checksum_refill_state
from config_validator_controls import validator_controls,_child as control_child
from config_validator_pointers import _leaf
from config_validator_result import _object as result_object
from config_validator_contract import CONTRACTS
from config_validator_access import VALIDATOR_ROLE
from config_validator_traversal import (_source_inputs,_complete_sequence,_math_body,MATH,
    _led_copies,_source_catalogue,_coverage,_child_allocation,_color_capture,_phase_relations)
from config_validator_activation import guard_stopped_normal_activation_closure,computed_context_dispositions
from config_validator_traversal_contracts import COLOR_CAPTURE_ROLE
from config_validator_children import _normal_body,_normal_storage
from config_validator_child_contracts import NORMAL_ROLES
from config_finite import _literal_body
from config_helpers import _helper_function
from config_loader_boundary import _physical
from config_roots import root_adapter,root_window
from config_regions import Storage,borrow,require_disjoint
from config_stack import canonical_symbol
from validation import require


def _outlines(objects,rows,context,checked):
    def consume(objects,name,records,role,ancestry):
        require(role.startswith('@outline:'),'validator outline has another origin')
        label=role[9:]
        if label in MATH:proof=_math_body(objects,name,records,label)
        elif label in ('first_capture','band_capture'):proof=_leaf(objects,name,records,label)
        elif label=='zero_to_bool':proof=control_child(objects,name,records,label,{})
        elif label in ('first_tail','validator_restore'):
            def none(*args):raise AssertionError('validator Result outline gains an activation')
            proof=_literal_body(objects,name,records,role,label,CONTRACTS[label],result_object,none,
                                function_consumer=_helper_function)
        elif label=='bounds_eight':
            proof=root_adapter(objects,name,records,'@outline:occupied_bounds8')
        else:raise AssertionError('validator gains an unreviewed parent-owned outline')
        start=objects.elf.functions[name][0]&~1
        require(proof['function']==start and proof['frame']==0,'validator outline grows or loses its actual body')
        checked[(start,role)]=proof;return proof
    return consume


def _source_arguments(family,offset,source,inputs,context,parent):
    """Finite actual caller families; no scalar ISA/transcript execution."""
    q=context.frame;owner=context.config.storage;phase=q.phase
    if family in ('range_u16','range_u8'):
        constants={68:1,734:3,900:4,914:5,2368:9,2388:10,2712:11,3332:12,3352:12,3522:13}
        if offset in (3930,3992):receiver=borrow(q,180,6,phase)
        else:
            require(offset in constants,'range child has no reviewed scalar/receiver phase')
            descriptor=parent['objects'][constants[offset]]
            receiver=borrow(Storage('address','validator source range '+str(descriptor),descriptor,
                3 if family=='range_u8' else 6,1 if family=='range_u8' else 2,phase,False),0,
                3 if family=='range_u8' else 6,phase)
        if offset==68:values=(borrow(owner,242,2,phase),)
        elif offset==734:values=tuple(borrow(owner,14528+2*i,2,phase) for i in range(8))
        elif offset in (900,914):values=(borrow(q,112,36,phase),)  # two active Pattern snapshots; guarded disabled record
        elif offset in (2368,2388,2712,3332,3352):
            field={2368:140,2388:142,2712:76,3332:80,3352:82}[offset]
            values=tuple(borrow(owner,W.offset+field,2,phase) for W in inputs['widgets'])
        elif offset==3522:values=(borrow(owner,208,2,phase),)
        else:
            field=2 if offset==3930 else 16
            values=tuple(borrow(owner,S.offset+field,2,phase) for S,item in zip(inputs['sources'],source['iterations'])
                         if offset!=3930 or item['poll'][0]=='EveryMs')
        require(bool(values),'range source union omits every actual invocation')
        return dict(kind='typed scalar u8' if family=='range_u8' else 'typed scalar u16',
            receiver=receiver,source_values=values,readonly=(receiver,)+values,
            dynamic_range='minimum_period of same Channel / same Channel freshness cap' if offset in (3930,3992) else None,
            unused_padding_excluded=((5,6),) if family=='range_u16' else (),
            exhausted_initialized_false=True)
    if family=='text_valid':
        if offset==1060:
            strings=(source['widget']['prepared']['view']['initialized_name'],);maxima=(14,)
        else:
            require(offset in (1648,3370),'text child has no reviewed source field')
            key='initialized_label' if offset==1648 else 'initialized_warning'
            strings=tuple(W[key] for W in source['widget']['iterations']);maxima=(12,24) if offset==1648 else (20,)
        prefixes=tuple(borrow(owner,item.view.start-owner.start,item.length.upper,phase) for item in strings)
        require(all(item.length.lower==item.length.upper and item.length.upper<=max(maxima) for item in strings),
                'text call widens initialized length into unused capacity')
        return dict(kind='same String logical prefix',readonly=prefixes,source_strings=strings,maxima=maxima,
            placement_and_current_Widget_paired=offset==1648)
    if family=='contrasts':
        require(offset in (1996,2014,2032,2682,3548),'contrast child loses its typed color pair')
        sources=(borrow(owner,14524,4,phase),) if offset==3548 else tuple(
            borrow(owner,W.offset+180,34,phase) for W in inputs['widgets'])
        if offset==2682:sources+=tuple(borrow(owner,W.offset+88,48,phase) for W in inputs['widgets'])
        return dict(kind='RGB565 scalar pair, not pointers',readonly=sources,
            color_role_subset=(2,3,4,5,6,8,9,10,11,12,13) if offset==1996 else None,
            same_current_Widget=offset!=3548)
    if family=='channel_domain':
        require(offset==2080,'Channel domain loses actual Widget scalar site')
        return dict(kind='Channel scalar',readonly=tuple(borrow(owner,W.offset+217,1,phase) for W in inputs['widgets']),
            domain=tuple(W['channel'] for W in source['widget']['iterations']),returns=('s0','s1'))
    require(family=='bands' and offset in (2452,2590,2644),'normal child has no complete source argument interface')
    return dict(kind='same current Widget AdvancedDisplay/count prefix',
        readonly=tuple(borrow(owner,W.offset+88,61,phase) for W in inputs['widgets']),
        count_domain=(0,1,2,3),stride=16,same_receiver_across_previous_index_calls=True,
        original_count4_diagnostic_pending=True)


def _checksum_refill_validator_state(objects,instructions,source_context):
    description,source,context,boundary=_checksum_refill_state(objects,instructions,source_context)
    fault_source=description['fault']
    names=[n for n in objects.elf.functions if canonical_symbol(n)==VALIDATOR_ROLE]
    require(len(names)==1,'selected checksum validator is missing or ambiguous');name=names[0]
    call=description['before_validator']['call'];q=context.frame
    # Consume the existing controls -> pointer -> local -> Result chain once.
    controls=validator_controls(objects,name,instructions,context,call)
    result=controls['pointer']['local']['boundary'];inputs=_source_inputs(source,context)
    rows={a:replace(r,operands=r.operands.split('@',1)[0].split('<',1)[0].strip())
          for a,r in instructions.items()}
    outlines={};parent=_complete_sequence(objects,name,rows,context.config.start,_outlines(objects,rows,context,outlines))
    require(not parent['indirect_sites'],'selected validator gains a callback')
    copies=_led_copies(objects,rows,parent,context);catalogue=_source_catalogue(objects,rows,parent,context,source)
    children={};normal=[];diagnostics=[];paths=[];color_capture=None
    known_controls={v['function']:(k,v) for k,v in controls['children'].items()}
    known_pointers={v['function']:(k,v) for k,v in controls['pointer']['children'].items()}
    for offset,operation,target,role,frame in parent['ordinary_edges']:
        at=parent['function']+offset;extent,actual,child_name=_helper_function(objects,target,
            canonical_symbol(next(n for n,e in objects.elf.functions.items() if e[0]&~1==target)))
        require(actual==frame,'validator child changes actual frame metadata')
        cut=False;stops_at_cut=False;interface='';family=NORMAL_ROLES.get(role);arguments={};bodies=None
        if family:
            proof=_normal_body(objects,child_name,rows,family)
            arguments=_source_arguments(family,offset,source,inputs,context,parent)
            saved=_normal_storage(proof,context,arguments['readonly']);paths.extend(proof['normal_physical_paths'])
            normal.append(dict(site=at,family=family,proof=proof,arguments=arguments,save_intervals=saved))
            interface='complete typed '+family+' source/loop/normal-return relation'
            arguments=dict(source='typed '+family,site=at,kind=arguments['kind'],
                readonly_ranges=tuple((v.start,v.extent) for v in arguments['readonly']))
        elif (target,role) in outlines:
            proof=outlines[target,role];family=role[9:]
            stops_at_cut=family=='bounds_eight';interface='matched current parent-owned '+role
            arguments=dict(source='current typed parent-owned outline',site=at,Q=q.start)
            if stops_at_cut:diagnostics.append(dict(site=at,kind='occupied bounds frontier',proof=proof))
        elif target in known_controls or target in known_pointers:
            family,proof=(known_controls if target in known_controls else known_pointers)[target]
            require(proof['extent']==extent,'visitor leaf has stale complete body')
            interface='typed parent argument/capture/placement relation';paths.append((frame,))
            arguments=dict(source='paired ControlAction/ControlPage loops' if family.startswith('action_') else
                'current typed first/valid/capture/placement phase',site=at,Q=q.start)
        elif at in copies:
            relation=copies[at];bodies=relation['bodies'];proof=bodies[0];family='copy_wrapper'
            interface='paired LED snapshot copy';paths.extend(relation['physical_activation_paths'])
            arguments=dict(source='same LED staging copy',site=at,read=relation['source'].start,
                write=relation['destination'].start,count=relation['count'])
        elif at in catalogue:
            relation=catalogue[at];proof=relation['proof'];family=proof['shape']
            interface='same Source Definition/Channel constructor';paths.append((frame,))
            arguments=dict(source='same initialized Source/DefinitionId/Channel',site=at,source_count=8)
        elif role==COLOR_CAPTURE_ROLE:
            require(offset==3462,'Color capture has another actual caller phase')
            proof=_color_capture(objects,child_name,rows,parent,context,inputs)
            color_capture=proof
            interface='same current Widget ColorRole first-error capture';paths.append((16,))
            proof=proof['proof'];family='Color_capture'
            arguments=dict(source='same current Widget/ColorRole/first/valid references',site=at,Q=q.start)
        elif role=='__aeabi_memcpy' and offset==342:
            relation=result['error_copy'];bodies=relation['children'];proof=bodies[0];family='copy_wrapper'
            interface='Err ten-byte payload before leading tag';paths.extend(relation['physical_activation_paths'])
            arguments=dict(source='same first Some payload before Result tag',site=at,read=q.start+78,
                write=context.result.start+2,count=10)
        else:
            require(role in ('_RNvNtCs_4core6option13expect_failed','_RNvNtNtCs_4core5slice5index16slice_index_fail',
                            '_RNvNtCs_4core9panicking18panic_bounds_check'),
                    'selected validator has an unclosed ordinary child')
            proof=root_window(objects,name,rows,at);cut=True;stops_at_cut=True;interface='typed pending diagnostic frontier'
            arguments=dict(source='same validator diagnostic frontier',site=at,Q=q.start,R=context.result.start)
            diagnostics.append(dict(site=at,kind=role,proof=proof))
        children[at]=dict(target=target,role=role,extent=extent,frame=frame,name=child_name,
            cut=cut,stops_at_cut=stops_at_cut,interface=interface,proof=proof,family=family,
            arguments=arguments,effects=(),copy_bodies=bodies)
    require(color_capture is not None,'validator omits its complete ColorRole first-error constructor')
    relations=_phase_relations(objects,parent,context,source,inputs,controls,normal,children,rows,copies,catalogue,color_capture)
    coverage=_coverage(objects,name,rows,parent,context,children)
    closure,projection,actual_tables=guard_stopped_normal_activation_closure(
        objects,rows,parent,context,source,coverage,children,controls,normal,catalogue,description,relations)
    computed=computed_context_dispositions(objects,rows,parent,projection,actual_tables,closure)
    require(paths and all(isinstance(path,tuple) and path and all(type(n) is int and n>=0 for n in path)
                         for path in paths),'normal validator child has no closed physical save path')
    maximum=max(sum(path) for path in paths)
    # Actual addresses precede every Storage-space distinction. Round only
    # the inspection envelope to eight bytes; e.g. catalogue owns52, not56.
    low=q.start-((maximum+7)//8)*8
    startup=boundary['history']['preclaim']['startup'];S=startup['copy']['initial_sp']
    _physical(objects,low,S,startup['copy']['stack_bottom'])
    for protected in context.protected:
        if protected.space==q.space:require(low<=protected.start<protected.start+protected.extent<=S,
                                           'normal validator protection leaves its actual stack envelope')
    restored=result['normal_return']
    require(restored['restored_bytes']==320 and restored['return_to']==call+4 and
            restored['final_sp']==context.caller.start and restored['saved_return']==q.start+316 and
            result['save_layout']['registers']['r8']==284,
            'normal validator loses its actual restore/original-r8/return-slot transport')
    # The readonly description exposes physical facts, never internal mutable
    # Views/ValidatorContext or an authority record acceptable to this API.
    alternatives=(dict(kind='Err',first_Some_tags=(0,1),writes=((context.result.start+2,10),(context.result.start,2)),
                       tag_after_payload=True,None2_diagnostic_pending=True),
                  dict(kind='Ok',tag=2,writes=((context.result.start+4,4),(context.result.start,2)),
                       tag_after_payload=True,requires_valid_and_guarded_stoichiometric_s0=True,
                       opaque=((context.result.start+2,2),(context.result.start+8,4))))
    description=dict(phase='checksum_refill_validator_return_pending_diagnostics_and_loader_continuation',mode='sizez',
        claim_scope='selected ordinary execution before a pending diagnostic, exception or other invocation',
        return_after_unclosed_diagnostic_not_covered=True,
        checksum_alternative_observed_taken=description['checksum_alternative_observed_taken'],
        source_context=description['source_context'],initialized_inputs=inputs['initialized_counts'],
        validator=dict(function=parent['function'],extent=5048,frame=(q.start,q.start+320),result=(context.result.start,12)),
        normal_return=dict(return_to=call+4,restored_SP=context.caller.start,original_r8=context.caller.start+112,
            saved_return_slot=q.start+316,preserved=restored['preserved'],architectural_LR_preservation_claimed=False,
            r0='not a Result pointer ABI'),normal_alternatives=alternatives,
        normal_physical_child_paths=tuple(sorted(set(paths))),normal_child_low=low,
        typed_phase_relations=relations,current_coverage=coverage['coverage'],computed_exclusions=computed,
        guard_stopped_normal_activation_closure=closure,
        normal_typed_call_count=len(normal)+3,
        pending_diagnostics=description['pending_diagnostics']+tuple((d['site'],d['kind']) for d in diagnostics),
        diagnostic_closure_pending=True,loader_continuation_pending=True,
        decoded_caller_and_other_routes_pending=True,other_optimization_variants_pending=True,
        source_qualification=description['source_qualification'],external_qualifications=description['external_qualifications'],
        requires_M2_pre_init_source_reconciliation=description['requires_M2_pre_init_source_reconciliation'],requires_H8_consumed_input_closure=True,
        exception_graph_limit=description['exception_graph_limit'],exception_absence_established=False,
        target_access_requirements_unresolved=True,requires_complete_same_B_loader_before_owner=True,
        requires_full_diagnostic_and_activation_graph_and_resource_gates=True,hardware_qualified=False)

    return description,source,context,boundary,dict(result=result,parent=parent,rows=rows,
        projection=projection,computed=actual_tables,closure=closure,relations=relations,children=children,fault_source=fault_source)


def checksum_refill_validator_return(objects,instructions,source_context):
    return _checksum_refill_validator_state(objects,instructions,source_context)[0]
