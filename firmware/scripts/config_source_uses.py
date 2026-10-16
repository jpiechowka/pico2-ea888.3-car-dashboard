"""Private same-allocation Source requirements and source-reached accesses.

Conditional families, read-only receipts and matching addresses cannot create
these relations. They consume the actual private Source/control predecessor.
Named ordinary source effects stay separate from mutable ownership, eventual
semantic validation, exceptional resume and the unavailable final producer.
"""
from dataclasses import dataclass,replace
from config_regions import Storage,View,borrow,require_disjoint,require_writes
from config_decode_effects import _object
from config_source_access import (SourceAccessContext,SourceFamily,SITE_SHAPES,
    SOURCE_EPOCHS,EXPECTED_SOURCE_ROLES,current_source_family,_PHASE)
from config_source_children import CHANNEL_SOURCE
from validation import require


def _family_data(control):
    from config_source_control import _SourceActiveControl
    from config_decoded_sources import _SourcePhasesState
    require(type(control) is _SourceActiveControl and type(control.state) is _SourcePhasesState,
        'Sources access bridge lacks its actual private active control')
    state=control.state;c=state.widgets.views.context;previous=state.widgets.family_bridge
    require(previous is state.widgets.family_bridge and state.objects is previous.objects and
        state.rows is previous.rows and state.source_context is previous.source_context and
        len(control.source_guards)==len(state.phases) and
        all(g['entry_index']==i and p.entry is state.entries[i] for i,(g,p) in
            enumerate(zip(control.source_guards,state.phases))),
        'Sources access bridge changes its actual entry/input/source association')
    # Reuse the actual startup/claim/specialized-receiver allocation lineage,
    # preserving every interval/identity/alignment. Only the finite source
    # phase changes. This is not a new allocation or initialization.
    loader=replace(previous.loader,phase=_PHASE);visitor=replace(previous.visitor,phase=_PHASE)
    config=replace(previous.config,phase=_PHASE)
    require((loader.start,loader.extent,visitor.start,visitor.extent,config.start,config.extent)==
        (c.frame.start,456,c.headers['visitor_frame'].start,1608,c.owner.start,14604) and
        previous.allocation_lineage[:3]==((c.frame,previous.loader),
            (c.headers['visitor_frame'],previous.visitor),(c.owner,previous.config)),
        'Sources access bridge loses the inherited claimed loader/Visitor/Config lineage')
    protected=tuple(replace(v,phase=_PHASE) for v in previous.protected)
    child=Storage(visitor.space,'actual Sources child',visitor.start-72,72,8,_PHASE,True)
    saved=Storage(child.space,'Source original saved cells',child.start+36,36,4,_PHASE,False)
    physical=Storage('address','physical Source original saved cells',saved.start,saved.extent,1,_PHASE,False)
    source=tuple(p for p in state.children.values() if p['role']==CHANNEL_SOURCE)
    require(len(source)==1 and source[0]['extent']==754 and source[0]['frame']==72,
        'Sources access bridge loses its current complete Source child')
    V=c.headers['visitor']['proof']['function'];families=[]
    for role in EXPECTED_SOURCE_ROLES:
        local=role in EXPECTED_SOURCE_ROLES[:2];parent=source[0]['function'] if local else V
        context=SourceAccessContext(loader,visitor,child if local else visitor,
            borrow(config,0,14576,_PHASE),protected+((saved,physical) if local else ()))
        sites=tuple((parent+off,*rest) for off,*rest in SITE_SHAPES[role])
        epochs=tuple((V+off,out,tag,8-i) for i,(off,out,tag) in enumerate(SOURCE_EPOCHS)) if local else ()
        families.append(current_source_family(state.objects,SourceFamily(role,'sizez',parent,V,sites,context,epochs)))
    lineage=((c.frame,loader),(c.headers['visitor_frame'],visitor),(c.owner,config),previous.allocation_lineage[3])
    return tuple(families),lineage


def _guards(state,phase,index=None):
    c=state.widgets.views.context;frame=c.headers['visitor_frame'];q=phase.entry.sequence
    guards=phase.entry.protected+(_object(q.decoder.header,'live complete Source D'),
        _object(q.decoder.payload,'immutable same Source input'),
        _object(phase.entry.exit.sequence.header,'live top Q'),_object(q.header,'live Source Q'))
    if index is not None:
        guards+=(_object(phase.entry.old_Source_array,'unchanged old Source array'),)
        for prior in phase.array['packages'][:index]:
            guards+=(_object(borrow(frame,prior['output'].offset,21,frame.phase),'prior parsed Source'),
                _object(prior['captured_tag'],'prior captured Source tag'))
        guards+=(Storage(frame.space,'Source original saved cells',frame.start-36,36,4,frame.phase,False),)
    else:
        require(phase.array['sequence'] is not None and phase.effects is not None,
            'Sources suffix use precedes the same completed Source array')
        guards+=(_object(borrow(c.owner,32,176,frame.phase),'completed same Source array'),)
    projected=[]
    for g in guards:
        other='address' if g.space!='address' else frame.space
        if not any(v.space==other and v.start<=g.start and g.start+g.extent<=v.start+v.extent for v in guards):
            projected.append(Storage(other,'physical Source use '+g.identity,g.start,g.extent,1,frame.phase,False))
    return guards+tuple(projected)


@dataclass(frozen=True)
class _SourceUse:
    role:str
    phase_index:int
    source_index:int
    before:object
    after:object
    output:View
    footprint:tuple
    protected:tuple
    source:dict
    continuation:tuple
    completed_prefix_issued:bool=False


def _member_graph(graph,root):
    """Canonical reachable projection of one immutable shared snapshot.

    No projected graph is retained per use. This preserves exact node values
    and alias edges while normalizing only the shared graph's node numbers.
    It neither evaluates source state nor supplies execution authority.
    """
    original=graph[1];nodes=[];memo={};active=set()
    def visit(ref):
        if ref[0]=='value':return ref
        require(ref[0]=='reference' and type(ref[1]) is int and 0<=ref[1]<len(original),
            'malformed Source member snapshot reference')
        old=ref[1];require(old not in active,'cyclic Source member snapshot')
        if old in memo:return ('reference',memo[old])
        new=len(nodes);memo[old]=new;nodes.append(None);active.add(old);node=original[old]
        if node[0]=='mapping':value=('mapping',tuple((visit(k),visit(v)) for k,v in node[1]))
        elif node[0] in ('tuple','list'):value=(node[0],tuple(visit(v) for v in node[1]))
        else:
            require(node[0]=='dataclass','unsupported Source member snapshot node')
            value=(*node[:3],tuple((name,visit(v)) for name,v in node[3]))
        nodes[new]=value;active.remove(old);return ('reference',new)
    return visit(root),tuple(nodes)


def _make_use(families,role,phase_index,index,before,after,output,guards,source,continuation):
    from config_abstract import DecoderInterface
    family=next(v for v in families if v.role==role);element=index if role in EXPECTED_SOURCE_ROLES[:2] else 0
    require(type(before) is DecoderInterface and type(after) is DecoderInterface and
        before.header==after.header and before.payload==after.payload and
        before.cursor.lower<=after.cursor.lower<=after.cursor.upper<=before.payload.extent,
        'Sources reached use changes its current source/header/cursor epoch')
    footprint=family.footprints(element)
    require(len(footprint)==1 and output.start==family.owner(element).start and
        output.extent==family.owner(element).extent and
        output.storage.space==family.owner(element).storage.space and
        output.writable==(family.sites[0][4]=='write'),
        'Sources reached use borrows another exact current family owner')
    if output.writable:
        require_writes((output,),((output,family.sites[0][2],family.sites[0][3]),),guards)
    else:
        read=Storage(output.storage.space,'exact active Sources read',* (footprint[0][0],footprint[0][1]-footprint[0][0]),
            1,output.storage.phase,False)
        for g in guards:require_disjoint((read,g))
    fields=(role,phase_index,index,before,after,output,footprint[0],guards,source,continuation)
    return _SourceUse(*fields)


def _reached_data(control,families):
    from config_abstract import Span
    state=control.state;c=state.widgets.views.context;frame=c.headers['visitor_frame'];V=c.headers['visitor']['proof']['function']
    child=Storage(frame.space,'actual Sources child',frame.start-72,72,8,frame.phase,True);uses=[]
    for phase_index,(phase,guard) in enumerate(zip(state.phases,control.source_guards)):
        require(guard['entry_index']==phase_index and len(guard['invocations'])==len(phase.array['invocations']),
            'Sources reached use loses its actual guarded invocation list')
        for index,invocation in enumerate(phase.array['invocations']):
            result=invocation['result'];proof=result['proof'];start=proof['function'];site=V+SOURCE_EPOCHS[index][0]
            require(invocation['index']==index and invocation['site']==site and
                guard['invocations'][index]['site']==site and invocation['before'].count==Span(8-index,8-index),
                'Sources reached use changes its paired call/count')
            protected=_guards(state,phase,index)
            for route in result['guarded_routes']:
                if route['selector']!=1:continue
                require(route['after'] is not None and route['payload']==(('request','u16',(0,65535)),('response','u16',(0,65535))) and
                    route['local_read']==(child.start+25,child.start+27) and
                    route['packed_word']=='request | (response <<16)' and len(route['children'])==2 and
                    tuple(v['proof']['semantic_role'] for v in route['children'])==('seq_u16','seq_u16'),
                    'Sources local read precedes its same-Route paired request/response')
                first,last=route['children']
                request=tuple(o['state'] for o in first['outcomes'] if o['kind']=='some')
                response=tuple(o['state'] for o in last['outcomes'] if o['kind']=='some')
                require(len(request)==len(response)==1 and request[0].count==Span(1,1) and
                    response[0].count==Span(0,0) and response[0].decoder==route['after'],
                    'Sources Route local read has an unpaired counted-field return')
                source=dict(source_phase=(phase_index,index,'Route Physical11'),Route=route,invocation=invocation)
                uses.append(_make_use(families,'source_route_local',phase_index,index,route['before'],route['after'],
                    borrow(child,24,4,frame.phase),protected,source,tuple(start+o for o in (474,478,482,486,490,494,506))))
            normal=result['normal']
            if normal is None:continue
            package=phase.array['packages'][index]
            require(package['source'] is normal and normal['tag_domain']==(0,1) and normal['return_site']==site+4 and
                result['last_output_store']==start+734 and result['returns'] and
                guard['invocations'][index]['output_store_reached'],
                'Sources Route output use precedes its full Source last-store/original-return relation')
            source=dict(source_phase=(phase_index,index,'complete Source output'),package=package,
                Route_alternatives=normal['route'],inactive_route_payload_opaque=(0,2),source_success_is_semantic_Config_validity=False)
            uses.append(_make_use(families,'source_route_output',phase_index,index,normal['before'],normal['after'],
                package['output'],protected,source,(start+724,start+730,start+734,site+4)))
        remaining=phase.remaining
        if remaining is None:continue
        relation=guard['suffix_lineage']
        require(guard['publication_lineage'] is not None and guard['complete_Source_array'] and relation is not None,
            'Sources suffix accesses precede actual array publication lineage')
        protected=_guards(state,phase);warning=remaining['warning']
        if warning is not None:
            steps=tuple(v for v in remaining['transitions'] if v['field'] in ('Warning scope','Warning mode','Warning page'))
            require(len(steps)==3 and tuple(v['site']-V for v in steps)==(4814,4850,5190) and
                steps[0]['before']==remaining['before'].decoder and steps[0]['after']==steps[1]['before'] and
                steps[2]['before'].decoder==steps[1]['after'] and steps[2]['after'].count==Span(0,0) and
                warning['after']==steps[2]['after'].decoder and
                (steps[0]['domain'],steps[1]['domain'],warning['page_domain'])==((0,1),(0,1),(0,65535)) and
                warning['scratch_read']==(frame.start+1065,frame.start+1067) and warning['complete_store']==V+5040,
                'Sources Warning read loses its same scope/mode/page source epoch')
            require(relation['warning'] is not None and
                len(relation['warning']['children'])==len(steps) and
                all(a is b for a,b in zip(relation['warning']['children'],steps)) and
                relation['warning']['complete_store']==warning['complete_store'],
                'Sources Warning use loses its checked same-parent argument/return relation')
            source=dict(source_phase=(phase_index,'WarningPresentation'),warning=warning,transitions=steps,
                actual_parent_relation=relation['warning'])
            uses.append(_make_use(families,'warning_presentation_local',phase_index,0,warning['before'],warning['after'],
                borrow(frame,1064,3,frame.phase),protected,source,(V+5220,V+5224,V+5228,V+5040)))
        for assignment in remaining['Fuel_assignments']:
            parent=tuple(v for v in relation['Fuel'] if v['assignment'] is assignment)
            require(len(parent)==1,'Sources Fuel use loses its checked same-parent source/physical relation')
            fuel=assignment['Fuel'];wire=fuel['wire_tag'];emitted=fuel['emitted_tag']
            require(warning is not None and assignment['source_guard']==(wire,emitted if wire==2 else None) and
                ((wire,emitted) in ((0,2),(1,3),(2,0),(2,1))) and fuel['complete_store']==V+5570 and
                fuel['early_tag_store']==V+5568 and fuel['pending_after_tag']==((5,7),) and
                tuple((o,n) for v,o,n in assignment['possible_writes'])==
                    ((0,4),(29,1),(7,1),(28,1),(8,4),(12,4),(16,4),(20,4),(24,4),(4,1),(5,2)),
                'Sources Fuel use loses its paired wire/density/late physical store')
            guards=protected+(_object(borrow(c.owner,208,4,frame.phase),'completed WarningPresentation'),)
            source=dict(source_phase=(phase_index,'Fuel',wire,emitted),assignment=assignment,
                actual_parent_relation=parent[0],
                inactive_payload_remains_opaque=True,later_Colors_success_required=False)
            uses.append(_make_use(families,'Fuel_final_halfword',phase_index,0,assignment['before'],assignment['after'],
                borrow(c.owner,0,32,frame.phase,True),guards,source,(V+5534,V+5566,V+5568,V+5570)))
    return tuple(uses)


@dataclass(frozen=True)
class _SourceAccessBridge:
    control:object
    families:tuple
    allocation_lineage:tuple
    uses:tuple
    checkpoint:object

    def current(self):
        from config_decoded_sources import _phase_graph
        _checked_bridge_graph(self)
        self.control.current();families,lineage=_family_data(self.control);uses=_reached_data(self.control,families)
        require(_phase_graph((families,lineage,uses,self.control.source_guards,self.control.record))==self.checkpoint,
            'Sources access bridge current allocation/source/epoch/effect relation differs')
        return self


def _checked_bridge_graph(bridge):
    from config_decoded_sources import _phase_graph
    require(_phase_graph((bridge.families,bridge.allocation_lineage,bridge.uses,
        bridge.control.source_guards,bridge.control.record))==bridge.checkpoint,
        'Sources access bridge/use record changed')


def _source_access_bridge(control):
    from config_source_control import _SourceActiveControl
    from config_decoded_sources import _phase_graph
    require(type(control) is _SourceActiveControl,'Sources access bridge lacks its actual private active control')
    control.current();families,lineage=_family_data(control);uses=_reached_data(control,families)
    return _SourceAccessBridge(control,families,lineage,uses,
        _phase_graph((families,lineage,uses,control.source_guards,control.record)))


def _check_source_use(bridge,use):
    """Check a retained member record, not fresh upstream source currentness.

    This private diagnostic check must not authorize a source-reached access
    or owner. Authoritative collection consumption uses _check_source_uses,
    whose non-yielding scope revalidates the actual full bridge first.
    """
    require(type(bridge) is _SourceAccessBridge and type(use) is _SourceUse and any(use is v for v in bridge.uses) and
        use.completed_prefix_issued is False,'Sources access is not the internally derived use')
    # A local projection cannot detect an equal-copy substitution whose other
    # reference is in another use or its control/package record. Retain the
    # complete cross-member/current-control association at this boundary too.
    _checked_bridge_graph(bridge)
    return _check_member(bridge,use,next(i for i,v in enumerate(bridge.uses) if v is use))


def _check_source_uses(bridge):
    """Consume the closed actual collection in one fresh, non-yielding scope.

    No reusable approval token or caller-selected index escapes. Whole bridge
    currentness and every member value/cross-use/control alias are checked
    together, then exact membership and footprint are consumed. No member
    graph is rebuilt after that complete comparison. The standalone record
    checker above deliberately grants no fresh source-use authority.
    """
    require(type(bridge) is _SourceAccessBridge,'Sources batch lacks its actual private bridge')
    bridge.current()
    for index,use in enumerate(bridge.uses):_check_member(bridge,use,index)
    return bridge.uses


def _check_member(bridge,use,index):
    require(type(use) is _SourceUse and type(index) is int and 0<=index<len(bridge.uses) and
        bridge.uses[index] is use and use.completed_prefix_issued is False,
        'Sources member is not in its internally checked collection')
    # Both internal callers first compare the complete live collection,
    # including this exact use and all source/control aliases. Rebuilding
    # each reachable projection here would repeat those same checks for
    # shared packages; membership never supplies a reusable approval token.
    family=next(v for v in bridge.families if v.role==use.role)
    element=use.source_index if use.role in EXPECTED_SOURCE_ROLES[:2] else 0
    require(family.footprints(element)==(use.footprint,),'Sources current use changes its family footprint')
    return use
