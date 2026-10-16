"""Selected SizeZ CRC-error -> normal refill -> before-validator relation.

All symbolic mutable borrows remain private to this check. This is one guarded
source alternative, not an acquired owner, whole loader or validation result.
"""
from config_loader_boundary import first_boot_loader_boundary,_physical
from config_headers import root_decoder_header,ROOT
from config_default_sources import source_terminal
from config_regions import Storage,borrow,require_disjoint,require_writes
from config_stack import canonical_symbol
from validation import require

_CHANNELS=(0,7,6,2,1,3,4,5)
_IDS=(5,3,1,14,2,15,4,13)
_GROUPS=(('fuel',0,4),('sources',32,176),('warnings',208,4),('controls',212,32),
    ('views',244,28),('widgets',440,4+8*220),('warning_colors',14524,4),('history',14528,16),
    ('led',14544,26),('version',14570,2),('catalogue_version',14572,2),('reduced_motion',14574,1),('boot_view',14575,1))


def _geometry(objects,boundary):
    require(boundary['mode']=='sizez','checksum refill has no authored consuming variant for this mode')
    geometry=boundary['loader_geometry'];P,end=geometry['frame'];S=boundary['history']['preclaim']['startup']['copy']['initial_sp']
    require(geometry['root_frame_bytes']==456 and end==S-88 and P==S-544,'checksum refill loses actual root SP')
    frame=Storage('stack:Reset:'+hex(S),'actual checksum loader',P,456,8,'default_refill',True)
    guards=[];ledger={v['kind']:v['interval'] for v in boundary['physical_protected']}
    require(ledger['retained_callers']==(end,S) and end<=ledger['executor_P'][0]<ledger['executor_P'][1]<=S,
        'executor constituent is not protected by retained caller envelope')
    # P is intentionally contained in the retained-callers guard. Preserve its
    # constituent record; do not put overlapping allocations in disjoint lists.
    for kind in ('Boot_task','retained_callers'):
        a,b=ledger[kind];guards.append(Storage('address' if kind=='Boot_task' else frame.space,kind,a,b-a,1,frame.phase,False))
    B,n=boundary['enclosing_allocation'];require(n==14604 and ledger['taken_and_enclosing_padding']==(B+14600,B+14604),
        'checksum refill loses exact taken/enclosing suffix')
    guards.append(Storage('address','unmutated slot metadata/taken/padding',B+14576,28,4,frame.phase,False))
    require_disjoint((frame,)+tuple(guards))
    return frame,tuple(guards),B,S


def _source_bridge(objects,rows,name,frame,guards,source,B):
    header=root_decoder_header(objects,name,rows,frame,guards)
    prepared=source['widget']['prepared'];failure=header['checksum_failure'];start=prepared['proof']['function']
    require(failure['function']+failure['extent']==start and failure['extent']==10,
        'checksum fault does not fall through its complete constructor to preparation')
    require(prepared['config'].start==B and prepared['config'].storage.extent==14604 and
        prepared['fault'].start==frame.start+8 and prepared['fault'].extent==16,
        'checksum preparation changes slot/fault owner')
    # Complete ROOT_ROWS prove two bounded readonly slices, descriptor0/8/16,
    # byte cursor/end, eight bit updates and stable SP. Neither branch is
    # erased by computing the expected checksum of the captured EAV here.
    require(header['proof']['function']==objects.elf.functions[name][0]&~1 and
        header['proof']['extent']==130 and header['call_site']==header['proof']['function']+126,
        'checksum source prefix loses its actual root entry')
    # The only selected fault entrance is the actual CRC comparison's BNE.
    from config_owner_entry import _direct_target
    from config_stack import transfer_family
    at=header['proof']['function']+114
    require(rows[at].mnemonic=='bne' and _direct_target(rows[at],'b')==failure['function'],
        'checksum alternative does not select its actual fault constructor')
    for pc,row in rows.items():
        family=transfer_family(row.mnemonic.removesuffix('.w'))
        if family not in ('b','bl','cbz','cbnz'):continue
        target=_direct_target(row,family)
        if failure['function']<=target<start and not failure['function']<=pc<start:
            require(pc==at and target==failure['function'],'foreign checksum-fault ingress bypasses its source state')
    require({'r4','r5','r8','r9','r10','sp'}<=set(prepared['child']['normal_preserves']),
        'View construction loses incoming checksum fault/base carriers')
    # P8 contains length; P12 is opaque in this source alternative. r5/r4 are
    # live until the matched STRD to P16/P20 after String14 returns. This maps
    # source meaning through CRC control, never numeric enum declaration order.
    early=Storage(frame.space,'incoming checksum fault words',frame.start+8,8,4,frame.phase,False)
    for view,o,n in prepared['local_writes']:
        if (view.start+o,n)!=(frame.start+16,8):require_writes((view,),((view,o,n),),(early,))
    return header,dict(source_variant='ConfigError::Checksum',guard=(at,'CRC accumulator differs from stored complement'),
        constructor=(failure['function'],10),fallthrough=start,registers_until_String_return=(('r5',0),('r4',5)),
        stored_words=((frame.start+8,'payload length; inactive for Checksum'),(frame.start+12,'opaque inactive word'),
            (frame.start+16,0),(frame.start+20,5)),retained=(frame.start+8,16),
        meaning_from_selected_CRC_source_constructor_not_enum_ordinal=True)


def _normal(result,frames,label):
    require(result['conservative_activation_frames']==frames and
        {'r4','r5','r6','r7','r8','r9','r10','r11','sp'}<=set(result['normal_preserves']),
        label+' loses its complete current normal-return/frame interface')
    outcomes=result.get('outcomes',())
    if outcomes and 'kind' in outcomes[0]:
        require(len([v for v in outcomes if v['kind']=='success'])==1 and
            all(v['kind'] in ('success','capacity_error') for v in outcomes),label+' has an unbound return alternative')
    return result


def _floating_carriers(source,rows):
    # Preparation loads s17..s30 for all Widget cases. Complete matched normal
    # children in this closed family contain no FP instruction at all, hence
    # cannot alter those live carriers. Child save/return proofs are separate.
    # Inspect only proofs already returned by the invoked checked APIs.
    checked=set()
    def walk(value):
        if isinstance(value,dict):
            if {'function','extent','ordinary_edges'}<=value.keys():
                lo=value['function'];hi=lo+value['extent']
                checked.add((lo,hi))
                require(not any(row.mnemonic.startswith('v') for at,row in rows.items() if lo<=at<hi),
                    'refill child changes live floating carriers')
            for key in ('proof','children','copy_children'):walk(value.get(key))
        elif isinstance(value,(list,tuple)):
            for child in value:walk(child)
    widget=source['widget'];walk(widget['prepared']['child'])
    for item in widget['iterations']:
        for child in item['child_history'].values():walk(child)
        walk(item['opaque_copies'])
    for item in source['iterations']:
        for child in item['child_history'].values():walk(child)
        walk(item['opaque_copy'])
    require(checked,'no current normally returning child bodies')
    return tuple(sorted(checked))


def _induction(source,frame,B):
    widget=source['widget'];prepared=widget['prepared'];handoff=prepared['widget_handoff'];Wstart=widget['proof']['function']
    require(prepared['continuation']==Wstart and handoff['channels']==_CHANNELS and
        (handoff['signed_offset'],handoff['count'])==(-1760,0) and handoff['array'].start==frame.start+72 and
        handoff['cursor_cell'].start==frame.start+32,'prepared Widget loop lacks its actual entry state')
    _normal(prepared['child'],(48,16,24),'View String14')
    paths={(48,16,24)};widgets=[];offset=-1760;count=0
    require(len(widget['iterations'])==8,'Widget source family lacks all eight elements')
    for item,channel in zip(widget['iterations'],_CHANNELS):
        W=item['widget'];require((item['index'],item['channel'],item['offset'],W.start,W.extent)==
            (count,channel,444+220*count,B+offset+2204,220),'Widget affine count/address/channel relation differs')
        require(item['initialized_capacity_elements']==count+1 and
            item['initialized_label'].view.start==W.start+156 and item['initialized_warning'].view.start==W.start+36 and
            item['initialized_label'].length.lower==item['initialized_label'].length.upper==item['source_case']['length'] and
            item['initialized_warning'].length==item['initialized_label'].length,
            'Widget semantic strings do not belong to this constructed element')
        history=item['child_history']
        _normal(history['label_append'],(16,24),'Widget label');_normal(history['warning_append'],(16,24),'Widget warning')
        advanced=_normal(history['advanced_constructor'],(72,8,72),'AdvancedDisplay')
        require(advanced['physical_activation_paths']==((72,8),(72,72)) and
            len(advanced['outcomes'])==1 and advanced['outcomes'][0]['channel']==channel and
            advanced['outcomes'][0]['afr']==(1 if channel==7 else 0),
            'AdvancedDisplay loses paired Channel/bool source argument or restored tail')
        paths.update(((16,24),)+advanced['physical_activation_paths'])
        require(len(item['opaque_copies'])==7,'Widget scratch epoch misses a bounded copy')
        for copy in item['opaque_copies']:
            _normal(copy,(8,72),'Widget opaque copy');require(copy['physical_activation_paths']==((8,),(72,)),
                'Widget copy loses restored-tail physical alternatives');paths.update(copy['physical_activation_paths'])
        widgets.append((count,channel,W.offset));offset+=220;count+=1
    require((offset,count,widget['normal_exit_count'],widget['continuation'])==(0,8,8,source['proof']['function']),
        'Widget loop does not reach actual source entry at the eight-element exit')
    sources=[];count=0;offset=0
    require(len(source['iterations'])==8 and source['normal_exit_count']==8,'Source loop is not the eight paired constructor family')
    for item,identifier in zip(source['iterations'],_IDS):
        require((item['index'],item['id'],item['channel'],item['offset'],item['source'].start,item['source'].extent)==
            (count,identifier,count,32+offset,B+32+offset,22),'Source ID/Channel/cursor/stride relation differs')
        history=item['child_history'];definition=_normal(history['definition'],(52,),'Definition')
        period=_normal(history['possible_period'],(8,),'period');copy=_normal(item['opaque_copy'],(8,72),'Source Access copy')
        require(len(definition['outcomes'])==1 and definition['outcomes'][0]['id']==identifier and
            len(period['outcomes'])==1 and period['outcomes'][0]['channel']==count and
            copy['physical_activation_paths']==((8,),(72,)), 'Source child argument/return alternative differs')
        paths.update(((52,),(8,),(72,)));sources.append((count,identifier,32+offset));count+=1;offset+=22
    require((offset,count,source['history_count'])==(176,8,8),'Source/history loop does not close its exact bound')
    return tuple(widgets),tuple(sources),tuple(sorted(paths))


def _groups(source,frame,B,guards):
    prepared=source['widget']['prepared'];widget=source['widget']
    writes=list(prepared['config_writes']+widget['possible_writes']+source['possible_writes'])
    for item in widget['iterations']:
        for copied in item['opaque_copies']:writes.extend(copied['possible_writes'])
    for item in source['iterations']:writes.extend(item['opaque_copy']['possible_writes'])
    owner=prepared['config'].storage;all_writes=[]
    for view,o,n in writes:
        require(view.storage in (owner,frame),'refill output names another object or epoch')
        if view.storage==owner:
            require(0<=view.start+o-B<view.start+o+n-B<=14576,'refill output escapes Config payload')
            all_writes.append((view,o,n))
        else:require_writes((view,),((view,o,n),),prepared['protected'])
    require_writes(tuple(view for view,o,n in all_writes),all_writes,guards)
    covered={a for view,o,n in all_writes for a in range(view.start+o-B,view.start+o+n-B)}
    # Full physical transport is necessary, not semantic initialization. The
    # source/loop constructor relations above give the active fields/prefixes;
    # capacity slack and inactive enum payload stay opaque.
    for label,lo,n in _GROUPS:
        if label=='sources':points={32+22*i+a for i in range(8) for a in range(21)}
        elif label=='views':points=set(range(244,260))|{268}
        elif label=='widgets':
            points=set(range(440,444))
            for item in widget['iterations']:
                length=item['source_case']['length'];ignored=set(range(4,12))|set(range(18,20))|set(range(30,32))|{87,219}|set(range(149,152))
                ignored|=set(range(36+length,56))|set(range(156+length,180))
                points.update(item['offset']+a for a in range(220) if a not in ignored)
        else:points=range(lo,lo+n)
        require(set(points)<=covered,'refill source group has an uncovered physical assignment: '+label)
    return tuple(label for label,lo,n in _GROUPS)


def _checksum_refill_state(objects,instructions,source_context):
    boundary=first_boot_loader_boundary(objects,instructions,source_context)
    frame,guards,B,S=_geometry(objects,boundary)
    names=[n for n,e in objects.elf.functions.items() if canonical_symbol(n)==ROOT and e[0]&~1==boundary['loader_geometry']['function']]
    require(len(names)==1,'checksum handoff has ambiguous actual loader');name=names[0]
    # Existing preparation/Widget/Source chain is consumed exactly once.
    source=source_terminal(objects,name,instructions,frame,guards)
    header,fault=_source_bridge(objects,instructions,name,frame,guards,source,B)
    widgets,sources,paths=_induction(source,frame,B);groups=_groups(source,frame,B,guards)
    floating_children=_floating_carriers(source,instructions)
    bottom=boundary['history']['preclaim']['startup']['copy']['stack_bottom']
    _physical(objects,frame.start-max(sum(v) for v in paths),S,bottom)
    outgoing=source['outgoing'];call=outgoing['call'];result=outgoing['result']
    require(source['proof']['function']+call[0]==source['proof']['function']+338 and call[3].endswith('validate12validate_ref') and call[4]==320 and
        outgoing['r8']==frame.start+112 and outgoing['r0']==result.start==frame.start+236 and result.extent==12 and
        outgoing['r10']==B and outgoing['r1']!=B,'completed refill loses its actual validator argument/Result epoch')
    # Bind only the future entry's exact prologue/R-carrier/static receiver
    # prefix; no visitor, first-error, Result or return effect is discharged.
    from config_validator_contract import CONTRACTS
    from config_validator_result import _object
    from config_finite import _literal_body
    from config_loader_boundary import VALIDATOR
    names=[n for n,e in objects.elf.functions.items() if e[0]&~1==call[2] and canonical_symbol(n)==VALIDATOR]
    require(len(names)==1,'before-validator target is not its unique specialized receiver')
    entry=CONTRACTS['entry']
    prefix=dict(extent=22,instructions=tuple(v for v in entry['instructions'] if v[0]<22),objects=entry['objects'][:1])
    def no_child(*args):raise AssertionError('unexpected child in validator receiver prefix')
    receiver=_literal_body(objects,names[0],instructions,VALIDATOR,'before-call receiver binding',prefix,_object,no_child,window=call[2])
    require(receiver['objects']=={0:B}, 'before-validator implicit static does not select the same B')
    # Source contract reloads B at each loop head BEFORE the equality guard,
    # resets r8 at initial entry and on every completed backedge, then writes
    # terminal fields without changing it. Thus Result replaces retired local
    # staging, and the error carrier survives the normal loop exit.
    future=Storage(frame.space,'upcoming validator',frame.start-320,320,8,frame.phase,True)
    _physical(objects,future.start,S,bottom)
    require_disjoint((future,Storage(frame.space,'new Result',result.start,12,4,frame.phase,False))+guards)
    require_writes((result,),((result,0,12),),source['widget']['prepared']['protected'])
    # Retire refill staging and form the next typed ABI epoch internally.
    # The context proves geometry/borrows only; it grants no source owner,
    # visitor outcome or whole-validator closure and is never exported.
    from dataclasses import replace
    from config_validator_access import ValidatorContext
    phase='default_validation';caller=replace(frame,phase=phase);active=replace(future,phase=phase)
    owner=replace(source['widget']['prepared']['config'].storage,phase=phase)
    protected=tuple(replace(v,phase=phase) for v in source['widget']['prepared']['protected'])
    saved=Storage(active.space,'future validator saves',active.start+216,104,4,phase,False)
    context=ValidatorContext('sizez',phase,caller,active,borrow(owner,0,14576,phase),
        borrow(caller,236,12,phase,writable=True),protected+(saved,))
    require(context.config.start==B and not context.config.writable and context.result.start==result.start,
        'before-validator epoch loses readonly Config or separate writable Result')
    diagnostics=(source['widget']['prepared']['capacity_failure'],)+tuple(source['widget']['diagnostics'].values())
    description=dict(phase='checksum_refill_pending_validator_and_diagnostic_closure',mode='sizez',
        source_context=boundary['history']['source_context'],fault=fault,widgets=widgets,sources=sources,source_groups=groups,
        checksum_alternative_observed_taken=False,other_loader_alternatives=('checksum success/visitor','other shared refill fault entries','decoded validation error/refill'),
        before_validator=dict(call=source['proof']['function']+call[0],target=call[2],frame=320,result=(result.start,12),
            r0=outgoing['r0'],r8=outgoing['r8'],r10=B,r1=outgoing['r1'],config=(B,14576),phase=context.phase,readonly_Config=True,receiver_prefix=(receiver['function'],receiver['extent']),source_typed_not_semantically_validated=True),
        floating_carrier_preserving_child_bodies=floating_children,
        physical_child_paths=paths,child_stack_low=frame.start-max(sum(v) for v in paths),
        executor_contained_under_caller_guard=boundary['physical_protected'][1],
        source_active_alternatives=('Fuel::EquivalentGasoline','GraphRange::Auto','GaugeKind::Numeric',
            'Applicability::DemoOnly','source-paired Poll/Route/Access','LED source template'),
        initial_image_facts_retired_at=source['widget']['prepared']['proof']['function']+20,
        retired_epochs=('CRC descriptors and D on selected checksum alternative','String14 Result before View copy',
            'Widget label/warning/Advanced staging at matched last reads','Widget Channel array before Source ID array',
            'Source Definition/Access scratch before new ResultP236'),
        opaque=('unused View/Widget/string capacity','inactive Fuel/Poll/Route payload','struct padding/opaque copied bytes'),
        pending_diagnostics=tuple((d['root']['location_role'],d['root']['window']) for d in diagnostics),diagnostic_closure_pending=True,
        target_access_requirements_unresolved=True,requires_complete_validator_Result_and_restore=True,
        requires_full_parent_other_paths_ingress_and_activation_graph=True,requires_complete_same_B_loader_before_owner=True,
        source_qualification=boundary['source_qualification'],requires_M2_pre_init_source_reconciliation=boundary['requires_M2_pre_init_source_reconciliation'],
        requires_H8_consumed_input_closure=True,external_qualifications=boundary['external_qualifications'],
        exception_graph_limit=boundary['exception_graph_limit'],exception_absence_established=False,hardware_qualified=False)

    return description,source,context,boundary


def checksum_refill_handoff(objects,instructions,source_context):
    """Readonly pending description; internal views never cross this API."""
    return _checksum_refill_state(objects,instructions,source_context)[0]
