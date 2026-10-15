"""Private actual-lineage bridge for the four conditional Widget families.

A conditional descriptor is never a reached access, initialized prefix or
mutable owner. Actual candidate/source/effect joins consume these separately.
"""
from dataclasses import dataclass,asdict,replace
from config_build_source import validate_source_context,inspect_source_context
from config_regions import Storage,borrow
from config_structural import _freeze,_FrozenObjects
from config_widget_access import WidgetContext,WidgetFamily,current_widget_family,SITE_SHAPES,EXPECTED_WIDGET_ROLES
from validation import require

_PHASE='decoded_widget_group'


def _checkpoint(state,parents,children,returns,structural,entries):
    # This is an immutable independent copy of already checked inputs, not a
    # body digest or inferred policy. Mutation requires rebinding the relation.
    return _freeze(dict(context=asdict(state.context),Views_parents=state.parents,
        Views_control=state.control,parents=parents,children=children,returns=returns,
        length_entries=tuple(asdict(v) for v in entries),selection=(structural.selection.proofs,
            structural.selection.selected,structural.selection.edges,tuple(sorted(structural.selection.cuts)),structural.selection.widget),
        structural=(structural.raw_successors,structural.raw_returns,
            structural.nested_returns,structural.outside_shared_contexts,structural.unentered_outgoing)))


def _input_checkpoint(objects,rows):
    if type(objects) is _FrozenObjects:
        require(rows is objects._rows,'Widget family lineage input rows leave the immutable package')
        return None
    # Unsupported providers gain no structural memoization. Retain a complete
    # independent value snapshot and recheck their current metadata/rows.
    elf=objects.elf
    return _freeze(dict(data=elf.data,sections=elf.sections,segments=elf.segments,
        functions=dict(elf.functions),symbol_entries=elf.symbol_entries,mapping_symbols=elf.mapping_symbols,
        symbols=dict(getattr(elf,'symbols',{})),named=dict(getattr(elf,'named',{})),sizes=dict(objects.sizes),rows=dict(rows)))


def _canonical(loader,visitor,config,protected,families,lineage):
    return _freeze(dict(loader=asdict(loader),visitor=asdict(visitor),config=asdict(config),
        protected=tuple(asdict(v) for v in protected),families=tuple(asdict(v) for v in families),
        lineage=tuple((asdict(a),asdict(b)) for a,b in lineage[:3])+(lineage[3],)))


@dataclass(frozen=True)
class _WidgetFamilyBridge:
    objects:object
    rows:object
    state:object
    parents:tuple
    children:dict
    returns:tuple
    structural:object
    source_context:object
    loader:Storage
    visitor:Storage
    config:Storage
    protected:tuple
    families:tuple
    allocation_lineage:tuple
    checkpoint:object
    canonical_checkpoint:object
    input_pair:tuple
    input_checkpoint:object
    entries:tuple

    def current(self):
        selection=self.structural.selection
        require(selection.context is self.state.context and selection.children is self.children,
            'Widget family lineage selection context/children binding changed')
        require(self.objects is self.input_pair[0] and self.rows is self.input_pair[1] and
            _input_checkpoint(self.objects,self.rows)==self.input_checkpoint,
            'Widget family lineage input package changed')
        require(_checkpoint(self.state,self.parents,self.children,self.returns,self.structural,self.entries)==self.checkpoint,
            'Widget family lineage or protected proof state changed')
        require(_canonical(self.loader,self.visitor,self.config,self.protected,self.families,self.allocation_lineage)==
            self.canonical_checkpoint,'Widget family lineage canonical allocation or family changed')
        if type(self.objects) is not _FrozenObjects:
            # A custom read_only implementation is not authenticated by the
            # ELF metadata snapshot. Re-run the already authored complete
            # source/body/child/return/ingress prerequisites on that provider;
            # never reuse its earlier structural result as current evidence.
            from config_decoded_widgets import _current_group,_widget_lengths,_child_returns
            from config_decoded_views import _widget_structural_control
            _,state,parents,children=_current_group(self.objects,self.rows,self.source_context)
            entries,_=_widget_lengths(self.objects,self.rows,state,parents,children)
            returns=_child_returns(self.rows,state,parents,children)
            structural=_widget_structural_control(self.objects,self.rows,state,parents,children,returns)
            require(_checkpoint(state,parents,children,returns,structural,entries)==self.checkpoint,
                'Widget family lineage custom provider current prerequisites changed')
        source=validate_source_context(self.source_context,self.objects.elf)
        require(source.binding()==self.state.context.boundary['history']['source_context'] and
            inspect_source_context(source,self.objects.elf)==self.state.context.boundary['history']['source_semantics'],
            'Widget family lineage source semantics changed')
        for value in self.families:current_widget_family(self.objects,value)
        return self


def _widget_family_bridge(objects,rows,state,parents,children,returns,structural,source_context,entries):
    """Map one checked actual first-Boot allocation into canonical metadata.

    Preserve actual geometry/identity and every stable ancestor guard. The one
    required Config label follows the recognized private static/single-take
    declaration and checked startup/claim/receiver/source chain below. Neither
    the label nor this mapping creates the still-unavailable mutable owner.
    """
    from config_decoded_views import _ViewsState,_StructuralControl
    from config_headers import ROOT,VISITOR
    from config_loader_boundary import _physical
    require(type(state) is _ViewsState and type(structural) is _StructuralControl and
        structural.selection.context is state.context and structural.selection.children is children and
        structural.nested_returns==state.control['nested_returns']+returns,
        'Widget family lineage lacks the same complete private structural predecessor')
    c=state.context;h=c.headers;boundary=c.boundary;history=boundary['history'];p=history['preclaim']
    source=validate_source_context(source_context,objects.elf);semantics=inspect_source_context(source,objects.elf)
    require(history['source_context']==source.binding() and semantics==history['source_semantics'] and
        semantics['Config_visibility']=='private_static_single_take_immutable_return' and
        semantics['ordered_boot_operations']==('initialize','enqueue_Dashboard') and
        semantics['take_source_expression']=='CONFIGURATION.take().load_at_boot(EMBEDDED)',
        'Widget family lineage source/private take/order binding differs')
    B=c.owner.start;frame=c.frame;visitor=h['visitor_frame'];geometry=boundary['loader_geometry']
    startup=p['startup'];entry=startup['entry'];S=startup['copy']['initial_sp'];P=frame.start
    require(boundary['mode']==history['mode']=='sizez' and boundary['enclosing_allocation']==(B,14604) and
        c.owner==Storage('address','same private Config allocation',B,14604,4,'decoded_u16_prefix',True) and
        frame==Storage('stack:Reset:'+hex(S),'actual CRC-success loader',P,456,8,c.owner.phase,True) and
        visitor==Storage(frame.space,'ConfigSeed',P-1608,1608,8,frame.phase,True),
        'Widget family lineage changes actual allocation/parent phase or geometry')
    require(geometry['frame']==(P,P+456) and P==S-544 and geometry['entry_SP']==P+456 and
        geometry['root_frame_bytes']==456 and geometry['function']==entry['loader']['function']==h['root']['proof']['function'] and
        entry['loader']['incoming_r0']==B+14600 and history['once_return_transport']['restored_SP']==P+456 and
        history['once_return_transport']['restored_before_specialized_tail'] and
        boundary['taken_state']==dict(address=B+14600,width=1,value=1,source='same successful once transition') and
        p['Config_write_intersections']==((B+14600,1,'claim_transition'),) and
        (p['claim']['taken'],p['claim']['loader_r0'],p['claim']['required_before_taken'],
            p['claim']['after_success_taken'],p['claim']['failed_exclusive_write_bytes'])==(B+14600,B+14600,0,1,0),
        'Widget family lineage loses actual same-B successful claim/restored loader')
    require(h['root']['proof']['role']==ROOT and h['visitor']['proof']['role']==VISITOR and
        h['root']['call_site']==geometry['function']+126 and
        tuple(e for e in h['root']['proof']['ordinary_edges'] if e[3]==VISITOR)==
            ((126,'bl',h['visitor']['proof']['function'],VISITOR,1608),) and
        c.regions[0]['objects']=={0:B} and
        h['visitor']['sequence_state'].decoder.header==h['root']['decoder_state'].header and
        h['visitor']['sequence_state'].decoder.payload==h['root']['decoder_state'].payload and
        h['visitor']['sequence_state'].decoder.header.start==P+236 and
        h['visitor']['sequence_state'].header.start==visitor.start+288,
        'Widget family lineage loses actual specialized receiver/header/visitor transport')
    require(tuple((v['function']-h['visitor']['proof']['function'],v['shape']) for v in parents)==
        ((416,'decoded_widgets_416'),(3516,'decoded_widgets_3516'),(6994,'decoded_widgets_6994')) and
        returns and min(v['lowest_SP'] for v in returns)==visitor.start-144,
        'Widget family lineage lacks complete selected Widget/child activation')
    low=visitor.start-144;_physical(objects,low,S,startup['copy']['stack_bottom'])
    task=next(v['interval'] for v in boundary['physical_protected'] if v['kind']=='Boot_task')
    require(B+14604<=low and not(task[0]<S and low<task[1]),
        'Widget family lineage physically aliases Config/Boot with nested children')
    loader_save=Storage(frame.space,frame.identity+':saved',P+352,104,4,frame.phase,False)
    visitor_save=Storage(frame.space,visitor.identity+':saved',visitor.start+1512,96,4,frame.phase,False)
    require(h['root']['protected']==c.guards+(loader_save,) and
        h['visitor']['protected']==c.guards+(loader_save,visitor_save) and
        any(v.space=='address' and (v.start,v.extent)==(B+14576,28) for v in c.guards),
        'Widget family lineage changes stable ancestor/save/metadata protection')
    # Derive the stable base before any iteration extends it. Do not filter an
    # active completed-prefix overlap from a whole-buffer family context.
    loader=replace(frame,phase=_PHASE);active=replace(visitor,phase=_PHASE)
    config=replace(c.owner,identity='private unpublished ConfigSlot',phase=_PHASE)
    payload=h['visitor']['sequence_state'].decoder.payload
    stable=h['visitor']['protected']+(Storage(payload.storage.space,'immutable Widget source',payload.start,payload.extent,
        1,frame.phase,False),)
    base=tuple(replace(v,phase=_PHASE) for v in stable)
    physical=tuple(Storage('address' if v.space!='address' else frame.space,'physical '+v.identity,v.start,v.extent,
        1,_PHASE,False) for v in base)
    protected=base+physical;V=h['visitor']['proof']['function'];families=[]
    advanced=next(e[2] for proof in parents for e in proof['ordinary_edges'] if proof['function']+e[0]==V+3416)
    for role in EXPECTED_WIDGET_ROLES:
        current=active;guards=protected;parent=V
        if role=='graph_payload':owner=borrow(active,940,8,_PHASE);loop=(0,0,0)
        elif role=='band_assembly':owner=borrow(active,1068,48,_PHASE,True);loop=(0,2,16)
        elif role=='widget_assembly':owner=borrow(config,444,14080,_PHASE,True);loop=(0,63,220)
        else:
            parent=advanced;current=Storage(frame.space,'decoded Widget Advanced initializer',active.start-72,72,8,_PHASE,True)
            save=Storage(frame.space,'Advanced initializer saved cells',current.start+48,24,4,_PHASE,False)
            guards+=(save,Storage('address','physical '+save.identity,save.start,save.extent,1,_PHASE,False))
            owner=borrow(active,1064,64,_PHASE,True);loop=(0,0,0)
        context=WidgetContext(loader,active,current,borrow(config,0,14576,_PHASE),owner,guards)
        sites=tuple((parent+offset,*rest) for offset,*rest in SITE_SHAPES[role])
        families.append(current_widget_family(objects,WidgetFamily(role,'sizez',parent,sites,context,loop)))
    lineage=((frame,loader),(visitor,active),(c.owner,config),
        ('same claimed source allocation',B,14604,history['boot_initialize_call'],geometry['function'],
            h['root']['call_site'],h['visitor']['proof']['function']))
    return _WidgetFamilyBridge(objects,rows,state,parents,children,returns,structural,source,loader,active,config,
        protected,tuple(families),lineage,_checkpoint(state,parents,children,returns,structural,entries),
        _canonical(loader,active,config,protected,tuple(families),lineage),(objects,rows),_input_checkpoint(objects,rows),entries).current()


@dataclass(frozen=True)
class _GuardedWidgetUse:
    bridge:_WidgetFamilyBridge
    role:str
    candidate:int
    element:int
    decoder:object
    after:object
    output:object
    footprints:tuple
    guards:tuple
    source_fields:object
    guard:str
    ordinary_continuation:tuple
    checked_values:object
    completed_prefix_issued:bool=False


def _use_checkpoint(role,candidate,element,before,after,output,guards,source_fields,guard,continuation):
    # Freeze the exact values after the caller/source/effect checks. Source
    # fields have already been recursively frozen by each closed factory.
    # This preserves those checked values; it does not replace any predicate.
    return (_freeze(dict(role=role,candidate=candidate,element=element,decoder=asdict(before),
        after=asdict(after),output=asdict(output),guards=tuple(asdict(v) for v in guards),
        guard=guard,ordinary_continuation=continuation)),source_fields)


def _active_candidate(bridge,active,value,entry,index,committed,control):
    from config_decoded_widgets import (_WidgetControlInput,_WidgetActiveControl,_iteration_state,
        _widget_control_profile,_CommittedWidget)
    from config_abstract import Span
    bridge.current()
    require(type(value) is _WidgetControlInput and type(control) is _WidgetActiveControl and
        value.state is bridge.state and value.parents is bridge.parents and value.children is bridge.children and
        control.state is bridge.state and control.parents is bridge.parents and control.children is bridge.children,
        'Widget guarded use lost the actual base/control/child lineage')
    require(type(index) is int and 0<=index<=64 and isinstance(committed,tuple) and len(committed)==index and
        any(entry is v for v in bridge.entries) and entry.sequence.count==Span(8,8),
        'Widget guarded use has another length predecessor or prefix count')
    expected=_iteration_state(bridge.state,index)
    require(active==expected and active.context.headers['visitor']['protected']==
        expected.context.headers['visitor']['protected'],'Widget guarded use omits actual address/physical prefix protection')
    V=bridge.state.context.headers['visitor']['proof']['function'];before=entry.sequence.decoder
    for ordinal,previous in enumerate(committed):
        require(type(previous) is _CommittedWidget and previous.index==ordinal and previous.before==before and
            previous.commit==(V+4018,V+4158,V+4164,V+554),'Widget guarded use has a stale/uncompleted previous prefix')
        uses=tuple(v for v in previous.uses if v.role=='widget_assembly')
        require(len(uses)==1 and type(uses[0]) is _GuardedWidgetUse and uses[0].bridge is bridge and
            uses[0].candidate==ordinal and uses[0].decoder==previous.before and uses[0].after==previous.after and
            uses[0].ordinary_continuation==(V+4158,V+4164) and
            uses[0].source_fields==_freeze(dict(previous.source_fields)),
            'Widget guarded use changed its prior source/late-store/epoch witness')
        before=previous.after
    require(value.source.incoming_decoder==before and
        value.source.incoming_decoder.header==entry.sequence.decoder.header and
        value.source.incoming_decoder.payload==entry.sequence.decoder.payload,
        'Widget guarded use splices a stale source cursor/header/payload epoch')
    profile=_widget_control_profile(value,value.state.context,value.state.parents+value.parents,value.children)
    require(profile==control.profile and value.returns==bridge.returns,
        'Widget guarded use changes current source guards or original child returns')
    return before


def _active_guards(active,extra=()):
    from config_decode_effects import _object
    c=active.context;frame=c.headers['visitor_frame'];sequence=c.headers['visitor']['sequence_state']
    guards=c.headers['visitor']['protected']+tuple(_object(v,label) for v,label in
        ((sequence.decoder.header,'live complete D'),(sequence.header,'live complete top Q'),
         (sequence.decoder.payload,'immutable same source'),(borrow(frame,1360,8,frame.phase),'live folded Widget Q')))+tuple(extra)
    physical=tuple(Storage('address' if v.space!='address' else frame.space,'physical use '+v.identity,
        v.start,v.extent,1,frame.phase,False) for v in guards)
    return guards+physical


def _family_use(bridge,role,candidate,element,before,after,output,guards,source_fields,guard,continuation):
    from config_regions import View,require_writes
    family=next(v for v in bridge.families if v.role==role)
    actual=family.footprints(element)
    require(isinstance(output,View) and output.storage.space==family.context.owner.storage.space and
        output.writable==(role!='graph_payload') and
        actual==tuple((output.start+offset,output.start+offset+width) for at,raw,offset,width,direction,phase in family.sites),
        'Widget guarded use borrows another receiver/field/iteration footprint')
    require(all(0<=offset<offset+width<=output.extent for at,raw,offset,width,direction,phase in family.sites),
        'Widget guarded use escapes the actual current owner')
    if role!='graph_payload':
        require_writes((output,),tuple((output,offset,width) for at,raw,offset,width,direction,phase in family.sites),guards)
    result=_GuardedWidgetUse(bridge,role,candidate,element,before,after,output,actual,tuple(guards),
        source_fields,guard,continuation,
        _use_checkpoint(role,candidate,element,before,after,output,guards,source_fields,guard,continuation))
    _check_use(bridge,result)
    return result


def _candidate_uses(bridge,active,value,entry,index,committed,control):
    """Only source-reached scratch alternatives; no spare Widget or prefix."""
    from config_decoded_widgets import _band_assembly_effects
    from config_decode_effects import _object
    from config_abstract import Span
    before=_active_candidate(bridge,active,value,entry,index,committed,control)
    frame=active.context.headers['visitor_frame'];phase=frame.phase;V=bridge.state.context.headers['visitor']['proof']['function']
    uses=[];fields=dict(value.source.fields);guards=_active_guards(active)
    outer='same N>i AND (N<=64 OR post-length remaining<N)'
    graph=tuple(v for label,v in value.source.children if label=='GraphRange paired variants')
    if 'display' in fields:
        require(len(graph)==1 and graph[0]['Auto_payload_opaque'] is True and
            fields['display']==dict(decimals=(0,255),bools=(0,1),graph_range=('Auto','Fixed'),f32_bits=(0,0xffffffff)),
            'Widget graph use loses its branch-paired source/opaque transport')
        pair=graph[0];auto=pair['Auto'];fixed=pair['Fixed']
        require(auto.header==before.header and auto.payload==before.payload and
            (fixed is None or fixed.header==auto.header and fixed.payload==auto.payload and pair['Fixed_counts']==(2,1,0)),
            'Widget graph use splices the Fixed cursor/domain into Auto')
        after=replace(auto,cursor=Span(auto.cursor.lower,max(auto.cursor.upper,auto.cursor.upper if fixed is None else fixed.cursor.upper)))
        source=_freeze(dict(fields=fields['display'],Auto=asdict(auto),Fixed=None if fixed is None else asdict(fixed),
            Auto_payload_opaque=True,packed_fragment_is_scalar=False))
        uses.append(_family_use(bridge,'graph_payload',index,0,before,after,borrow(frame,940,8,phase),guards,
            source,outer+' AND same decoded GraphRange Auto/Fixed branch',(V+1218,V+1230,V+1270)))
    if value.source.decoder is None:
        require(not value.advanced['normal'] and not value.advanced['errors'],'failed Widget prefix acquired Advanced uses')
        return tuple(uses)
    advanced=value.advanced
    history=advanced['children'];length=advanced['Band_length']
    length_normal=tuple(v['state'] for v in length['outcomes'] if v['kind']=='success')
    require(len(length_normal)<=1,'Band length has unpaired current success')
    band_records=tuple((int(label[4:]),candidate) for item in history if isinstance(item,tuple) and len(item)==2
        for label,candidate in (item,) if label.startswith('Band') and label[4:].isdigit())
    previous=length_normal[0] if length_normal else None
    for j,candidate in band_records:
        after=candidate['normal_decoder']
        if after is None:continue
        incoming=candidate['incoming_decoder']
        require(incoming.header==before.header and incoming.payload==before.payload and
            after.header==before.header and after.payload==before.payload and
            previous is not None and previous==incoming and candidate['count_after_primitive_children']==Span(1,1) and
            candidate['logical_source_fields_consumed']==4 and
            candidate['normal_label_guard']=='length==0 OR (all bytes in32..126 AND at least one byte!=32)',
            'Widget Band use loses its actual source/count/normalized-label epoch')
        previous=after
        if j==3:continue  # parsed fourth candidate rejects before a spare slot
        require(0<=j<3,'Widget Band use has an unbounded source index')
        effect=_band_assembly_effects(active,j,guards)
        require(effect['early_count']['logical']==j and effect['last_semantic_store']==V+3102 and
            effect['checked_continuation']==V+3104 and effect['outstanding_after_count']==((11,16),) and
            (V+3104,V+2650,None) in control['successors'],
            'Widget Band use promotes physical count before its last label/backedge')
        prior=() if j==0 else (_object(borrow(frame,1068,16*j,phase),'completed same-loop Band prefix'),)
        protected=_active_guards(active,prior+(_object(borrow(frame,1300,5,phase),'same normalized Band label'),))
        source=_freeze(dict(fields=candidate['source_fields'],label_guard=candidate['normal_label_guard'],
            tail_zero_is_initialized=True,early_count=j+1,logical_before_last_label=j))
        uses.append(_family_use(bridge,'band_assembly',index,j,incoming,after,borrow(frame,1068+16*j,16,phase,True),
            protected,source,outer+' AND same Band N>'+str(j)+' AND complete candidate before j<3 guard',(V+3102,V+3104)))
    for option in advanced['normal']:
        j=option['Band_count'];constructor=advanced['initializer']
        require(type(j) is int and 0<=j<=3 and advanced['initializer_arguments']==('Boost',False) and
            constructor['outcomes']==({'channel':0,'afr':0,'readout':2},) and
            constructor['physical_activation_paths']==((72,8),(72,72)) and
            constructor['possible_writes']==((borrow(frame,1064,64,phase,True),0,61),) and
            advanced['initializer_epoch']==(1064,1128) and advanced['final_epoch']==(1000,1064),
            'Widget Advanced use borrows default/retired initializer or decoded override epoch')
        child=Storage(frame.space,'actual Advanced initializer',frame.start-72,72,8,phase,True)
        saved=Storage(frame.space,'actual Advanced saved cells',child.start+48,24,4,phase,False)
        protected=_active_guards(active,(_object(borrow(frame,940,48,phase),'captured same Band source'),saved))
        source=_freeze(dict(fields=option['source_fields'],Band_count=j,initializer_arguments=('Boost',False),
            initializer_before_decoded_overrides=True,output_initialized_ranges=constructor['initialized_ranges']))
        uses.append(_family_use(bridge,'advanced_initializer',index,0,value.source.decoder,option['decoder'],
            borrow(frame,1064,64,phase,True),protected,source,outer+' AND same Band N=='+str(j)+' AND attributes succeed',
            (V+3416,V+3420,V+3424)))
    return tuple(uses)


def _widget_commit_use(bridge,active,value,entry,index,committed,control,evidence,after):
    from config_decoded_widgets import _OrdinaryCommitEvidence
    from config_decode_effects import _object
    from config_abstract import Span
    before=_active_candidate(bridge,active,value,entry,index,committed,control)
    require(type(index) is int and index<64 and type(evidence) is _OrdinaryCommitEvidence and evidence.source is value and
        value.source.decoder is not None and value.advanced['normal'] and
        after==replace(before,cursor=Span(min(v['decoder'].cursor.lower for v in value.advanced['normal']),
            max(v['decoder'].cursor.upper for v in value.advanced['normal']))),
        'Widget assembly use lacks its fresh complete source/capacity/commit epoch')
    V=bridge.state.context.headers['visitor']['proof']['function'];frame=active.context.headers['visitor_frame'];phase=frame.phase
    require(evidence.Widget==(V+4018,V+4158,V+4164,V+554) and
        (V+4164,V+554,None) in control['successors'] and all(v['Widget']['outstanding_after_count']==((180,219),) and
            v['Widget']['last_semantic_store']==V+4158 and v['Widget']['checked_continuation']==V+4164 for v in evidence.assembly),
        'Widget assembly use promotes the early physical count or loses late writes')
    protected=_active_guards(active,tuple(_object(borrow(frame,offset,size,phase),label) for offset,size,label in
        ((688,23,'captured Warning tail'),(628,58,'captured Advanced tail'),(600,27,'captured Widget label'))))
    output=borrow(active.context.owner,444+220*index,220,phase,True)
    return _family_use(bridge,'widget_assembly',index,index,before,after,output,protected,_freeze(dict(value.source.fields)),
        'same N>i, complete source decoded, i<64; prefix advances only after all late stores',(V+4158,V+4164))



def _check_use(bridge,use):
    """Recheck an immutable use; descriptors/receipts cannot replace it."""
    from config_abstract import DecoderInterface
    from config_regions import View,require_writes
    from config_decode_effects import _object
    require(type(use) is _GuardedWidgetUse and use.bridge is bridge and use.completed_prefix_issued is False and
        type(use.candidate) is int and 0<=use.candidate<=64 and type(use.element) is int,
        'Widget actual use is not its checked same-lineage relation')
    family=next((v for v in bridge.families if v.role==use.role),None)
    require(family is not None and use.footprints==family.footprints(use.element),
        'Widget actual use changed its bounded family/iteration')
    c=bridge.state.context;frame=c.headers['visitor_frame'];V=c.headers['visitor']['proof']['function']
    expected_storage=c.owner if use.role=='widget_assembly' else frame
    require(isinstance(use.output,View) and use.output.storage==expected_storage and use.output.storage.phase==frame.phase and
        use.output.writable==(use.role!='graph_payload') and
        use.footprints==tuple((use.output.start+offset,use.output.start+offset+width)
            for at,raw,offset,width,direction,phase in family.sites),
        'Widget actual use changes its current writable receiver/phase')
    if use.role=='widget_assembly':require(use.candidate==use.element<64,'Widget actual use creates spare slot64')
    else:require(use.element==0 if use.role!='band_assembly' else 0<=use.element<3,'Widget scratch use has another loop index')
    actual=c.headers['visitor']['sequence_state'].decoder
    require(type(use.decoder) is DecoderInterface and type(use.after) is DecoderInterface and
        use.decoder.header==use.after.header==actual.header and use.decoder.payload==use.after.payload==actual.payload and
        use.decoder.cursor.lower<=use.after.cursor.lower<=use.after.cursor.upper<=actual.payload.extent,
        'Widget actual use loses its paired current source/decoder epoch')
    require(isinstance(use.guards,tuple) and all(isinstance(v,Storage) and not v.writable and v.phase==frame.phase for v in use.guards),
        'Widget actual use drops typed live protection')
    required=c.headers['visitor']['protected']+tuple(_object(v,label) for v,label in
        ((actual.header,'live complete D'),(c.headers['visitor']['sequence_state'].header,'live complete top Q'),
         (actual.payload,'immutable same source'),(borrow(frame,1360,8,frame.phase),'live folded Widget Q')))
    if use.candidate:
        prefix=Storage('address','actual ordinary Widget prefix',c.owner.start+444,220*use.candidate,4,frame.phase,False)
        required+=(prefix,Storage(frame.space,'physical ordinary Widget prefix',prefix.start,prefix.extent,4,frame.phase,False))
    require(all(v in use.guards for v in required),'Widget actual use omits base/address/physical prefix protection')
    if use.role!='graph_payload':
        require_writes((use.output,),tuple((use.output,offset,width)
            for at,raw,offset,width,direction,phase in family.sites),use.guards)
    expected={'graph_payload':(V+1218,V+1230,V+1270),'band_assembly':(V+3102,V+3104),
        'advanced_initializer':(V+3416,V+3420,V+3424),'widget_assembly':(V+4158,V+4164)}
    require(use.ordinary_continuation==expected[use.role],'Widget actual use promotes an early count/foreign continuation')
    require(_use_checkpoint(use.role,use.candidate,use.element,use.decoder,use.after,use.output,use.guards,
        use.source_fields,use.guard,use.ordinary_continuation)==use.checked_values,
        'Widget actual use source/epoch/effect witness changed')
    return use
