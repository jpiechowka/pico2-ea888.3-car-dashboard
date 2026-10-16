"""Private complete Tail composition, before loader finalization.

Only the same actual private Sources successor can enter. Current literals,
source effects, protection, original returns and active ordinary control are
separate checked obligations. Public descriptions cannot initialize state.
"""
from dataclasses import dataclass
from config_abstract import SequenceInterface,Span
from config_decode_effects import _object
from config_headers import VISITOR
from config_regions import Storage,borrow,require_disjoint
from config_tail_children import tail_coverage
from config_tail_parent_contracts import GRAMMARS
from config_widget_children import _name
from validation import require


_WINDOWS=(170,5630,5922,6018,6412,6450,7000,56)


def _parents(objects,rows,V,B):
    """All current selected Tail windows, with the same private Config pool."""
    bundle=tail_coverage(objects,_name(objects,VISITOR,V),rows)
    require(bundle['kind']=='tail_parent_literal_windows_only' and bundle['role']==VISITOR and
        bundle['function']==V and (bundle['extent'],bundle['frame'])==(7128,1608),
        'Tail parent literals belong to another current source parent')
    windows=bundle['windows']
    require(tuple(p['function']-V for p in windows)==_WINDOWS and
        all(p['shape']=='decoded_tail_'+str(off) and p['role']==VISITOR and
            (p['extent'],p['frame'])==(GRAMMARS['visitor_'+str(off)]['extent'],1608) for off,p in zip(_WINDOWS,windows)),
        'Tail parent literals change the exact selected window family')
    actual=tuple((p['function']-V,ordinal,value) for p in windows for ordinal,value in p['objects'].items())
    require(bundle['pool_values']==actual and tuple((off,ordinal) for off,ordinal,value in actual)==
        ((6018,0),(6450,0),(6450,1),(7000,0)) and all(value==B for off,ordinal,value in actual),
        'Tail reload loses the same private Config')
    return windows


@dataclass(frozen=True)
class _TailEntry:
    predecessor:object
    exit:object
    sequence:SequenceInterface
    frontier:int
    protected:tuple
    r4_pointer:tuple
    second_screen_pointer:tuple
    retired_saved_count:tuple
    old_controls:object
    full_config:Storage
    Controls_sequence:object=None
    completed_ordinary_group_issued:bool=False

    def __post_init__(self):
        require(self.Controls_sequence is None and self.completed_ordinary_group_issued is False,
            'Tail entry cannot fabricate a local header or ordinary completion')


def _incoming_r4(objects,rows,phases,frame,V):
    """Actual producer and checked returned value, not ABI preservation alone."""
    from config_finite_returns import _high_definitions
    from config_regions import _control
    from config_widget_contracts import ROLES as WIDGET
    wanted={5538:('add.w','r4, sp, #0x428'),5574:('add','r1, sp, #0x120'),5576:('mov','r0, r4')}
    for off,pair in wanted.items():
        row=rows.get(V+off)
        require(row is not None and (row.mnemonic,row.operands)==pair and
            objects.read_only(row.address,len(row.raw))==row.raw,'Tail incoming r4 producer/receiver differs')
    calls=phases.structural.carrier_returns['parent_calls']
    color=tuple(v for v in calls if v[0]==V+5578)
    require(len(color)==1 and color[0][2]==WIDGET['warning_colors'] and
        color[0][3]==tuple('r'+str(i) for i in range(4,12)),
        'Tail incoming r4 lacks its actual WarningColors return transfer')
    for at,row in rows.items():
        if not V+5538<at<V+5630:continue
        control,conditional=_control(row)
        require('r4' not in _high_definitions(row,control),'Tail incoming r4 is redefined after its producer')
        if control=='bl':require(at==V+5578,'Tail incoming r4 crosses an unpaired caller')
    return (frame.start+1064,V+5538,V+5578)


def _tail_entries(objects,rows,state):
    """Fresh actual private predecessor; no public receipt or equal exit clone."""
    from config_decoded_sources import _SourcesState
    from config_tail_effects import _guards
    require(type(state) is _SourcesState,'Tail predecessor is not the private Sources producer')
    phases=state.phases
    require(objects is phases.objects and rows is phases.rows,'Tail predecessor uses another current input pair')
    state.current()
    if not state.exits:return ()
    c=phases.widgets.views.context;frame=c.headers['visitor_frame'];V=c.headers['visitor']['proof']['function'];owner=c.owner
    parents=_parents(objects,rows,V,owner.start)
    require(tuple(p['function']-V for p in parents)==_WINDOWS,'Tail entry has another current parent family')
    # Child activation cannot alias any physical Config byte, including the
    # enclosing suffix and currently inactive capacity. Labels do not separate
    # two views of the same RAM address.
    activation=Storage('address','Tail complete descendant/retained callers',frame.start-152,152+1608+456,8,frame.phase,False)
    full=Storage('address','same private Config protected during Tail parse',owner.start,14604,4,frame.phase,False)
    require_disjoint((activation,full))
    pointer=_incoming_r4(objects,rows,phases,frame,V);entries=[]
    for exit in state.exits:
        require(type(exit.phase_index) is int and 0<=exit.phase_index<len(phases.phases),'Tail exit has another Source phase')
        phase=phases.phases[exit.phase_index];source_entry=phases.entries[exit.phase_index]
        normal=exit.source_normal
        require(phase.entry is source_entry and phase.remaining is not None and
            any(normal is v for v in phase.remaining['normal']) and exit.sequence is normal['after'] and
            exit.sequence.count==Span(4,4) and normal['Fuel']['complete_store']==V+5570 and
            normal['colors']['complete_store']==V+5628,'Tail entry precedes its actual completed Source suffix')
        color=normal['colors'];invocation=color['invocation']
        require(any(invocation is v for v in phase.remaining['Colors_invocations']) and invocation['site']==V+5578 and
            color['source'] is invocation['child'] and any(v['kind']=='some' and v['state'] is exit.sequence
                for v in invocation['child']['outcomes']), 'Tail entry loses its same WarningColors normal source')
        carrier=tuple((cell,target) for cell,target in source_entry.carriers if cell.offset==216)
        require(len(carrier)==1 and carrier[0][0]==borrow(frame,216,4,frame.phase) and carrier[0][1]==frame.start+1001,
            'Tail entry loses its initialized/preserved second Screen pointer')
        cell,value,producer=source_entry.saved_top_count
        guard=_object(cell,'initialized saved top count')
        require((cell.storage,cell.offset,cell.extent,value,producer)==(frame,280,4,Span(7,7),V+4360) and
            guard in source_entry.protected,'Tail entry changes the initialized saved Source top-count epoch')
        # Normal suffix completion has consumed the source7 count at5052.
        # Tail takes current top4 at5630; V280 may now be reused by later LED
        # staging. This expires the old value rather than re-reading data.
        protected=tuple(v for v in source_entry.protected if v!=guard)
        previous=tuple(Storage(space,label,owner.start+off,n,1,frame.phase,False)
            for off,n,label in ((0,212,'completed Fuel Source and Warning'),(244,14284,'completed Views Widgets and global Colors'),
                (14570,4,'completed version fields'),(14575,29,'boot selector and enclosing Config suffix'))
            for space in ('address',frame.space))
        protected=_guards(frame,protected+previous)
        require(any(v.space==frame.space and v.start==frame.start+216 and v.extent==4 for v in protected),
            'Tail entry dropped its still-live Screen pointer cell')
        entries.append(_TailEntry(state,exit,exit.sequence,V+5630,protected,pointer,carrier[0],
            (cell,7,V+5052,V+5630),borrow(owner,212,32,frame.phase),full))
    return tuple(entries)


@dataclass(frozen=True)
class _TailPhase:
    entry:_TailEntry
    controls:dict
    control_effects:dict
    suffix:object
    suffix_effects:object
    parent_envelopes:dict
    completed_ordinary_group_issued:bool=False


def _entry_data(entry,predecessor):
    require(type(entry) is _TailEntry and entry.predecessor is predecessor and
        any(entry.exit is v for v in predecessor.exits) and entry.sequence is entry.exit.sequence,
        'Tail private entry lost its exact Source exit/sequence identity')
    return dict(exit_index=next(i for i,v in enumerate(predecessor.exits) if v is entry.exit),
        sequence=entry.sequence,frontier=entry.frontier,protected=entry.protected,
        r4_pointer=entry.r4_pointer,second_screen_pointer=entry.second_screen_pointer,
        retired_saved_count=entry.retired_saved_count,old_controls=entry.old_controls,full_config=entry.full_config,
        Controls_sequence=entry.Controls_sequence,completed_ordinary_group_issued=entry.completed_ordinary_group_issued)


def _phase_checkpoint(predecessor,entries,parents,children,phases,predecessor_description,payload_sha256):
    from config_decoded_sources import _phase_graph
    require(len(entries)==len(phases) and all(p.entry is e and not p.completed_ordinary_group_issued for p,e in zip(phases,entries)),
        'Tail private phase changed its selected entry association')
    data=tuple(dict(entry=_entry_data(p.entry,predecessor),controls=p.controls,control_effects=p.control_effects,
        suffix=p.suffix,suffix_effects=p.suffix_effects,parent_envelopes=p.parent_envelopes) for p in phases)
    return _phase_graph(dict(parents=parents,children=children,phases=data,
        predecessor_description=predecessor_description,payload_sha256=payload_sha256))


@dataclass(frozen=True)
class _TailPhasesState:
    predecessor:object
    objects:object
    rows:object
    source_context:object
    parents:tuple
    children:dict
    entries:tuple
    phases:tuple
    predecessor_description:dict
    payload_sha256:str
    checkpoint:object
    completed_ordinary_group_issued:bool=False

    def current(self):
        from config_tail_children import tail_children
        from config_structural import _readonly_hash
        require(not self.completed_ordinary_group_issued,'Tail phase alone cannot mint ordinary completion')
        base=self.predecessor.phases;c=base.widgets.views.context
        require(self.objects is base.objects and self.rows is base.rows and self.source_context is base.source_context,
            'Tail private phase uses another current input/source pair')
        entries=_tail_entries(self.objects,self.rows,self.predecessor)
        require(tuple(_entry_data(v,self.predecessor) for v in entries)==tuple(_entry_data(v,self.predecessor) for v in self.entries),
            'Tail private entry/source/epoch relation changed')
        require(self.parents==_parents(self.objects,self.rows,c.headers['visitor']['proof']['function'],c.owner.start) and
            self.children==tail_children(self.objects,self.rows,self.parents),'Tail private current parent/child body changed')
        payload=c.headers['visitor']['sequence_state'].decoder.payload
        require(_readonly_hash(self.objects,payload.start,payload.extent)==self.payload_sha256,
            'Tail private consumed source bytes changed')
        require(_phase_checkpoint(self.predecessor,self.entries,self.parents,self.children,self.phases,
            self.predecessor_description,self.payload_sha256)==self.checkpoint,
            'Tail private source/epoch/child/effect record changed')
        return self


def _first_boot_tail_phases(objects,rows,source_context):
    """Derive the actual private Sources chain once; no ordinary return yet."""
    from config_decoded_sources import _first_boot_decoded_sources_state
    from config_tail_source import _controls_source,_controls_effects,_history_led_source,_suffix_effects
    from config_tail_parent_effects import _parent_envelopes
    from config_tail_children import tail_children
    from config_structural import _readonly_hash
    description,predecessor=_first_boot_decoded_sources_state(objects,rows,source_context)
    base=predecessor.phases;objects=base.objects;rows=base.rows;c=base.widgets.views.context
    frame=c.headers['visitor_frame'];V=c.headers['visitor']['proof']['function'];owner=c.owner
    parents=_parents(objects,rows,V,owner.start);children=tail_children(objects,rows,parents)
    entries=_tail_entries(objects,rows,predecessor);phases=[]
    for entry in entries:
        envelopes=_parent_envelopes(objects,rows,parents,entry,owner)
        controls=_controls_source(objects,rows,parents,entry.sequence,frame,entry.protected+(entry.full_config,))
        effect=_controls_effects(objects,rows,parents,controls,owner,frame,entry.protected)
        suffix=suffix_effects=None
        if effect['complete_after'] is not None:
            suffix=_history_led_source(objects,rows,parents,effect,frame,entry.protected)
            suffix_effects=_suffix_effects(objects,rows,parents,suffix,frame,entry.protected)
        phases.append(_TailPhase(entry,controls,effect,suffix,suffix_effects,envelopes))
    phases=tuple(phases);payload=c.headers['visitor']['sequence_state'].decoder.payload
    digest=_readonly_hash(objects,payload.start,payload.extent)
    checkpoint=_phase_checkpoint(predecessor,entries,parents,children,phases,description,digest)
    return _TailPhasesState(predecessor,objects,rows,base.source_context,parents,children,entries,phases,description,digest,checkpoint)


@dataclass(frozen=True)
class _TailExit:
    phase_index:int
    decoder:object
    source_normal:dict
    original_return:tuple

def _tail_exits(state,bridge,uses):
    """Original normal Visitor returns after the closed reached-use batch.

    The outgoing carrier is the live caller D, never a Visitor-local Q or
    output borrow. Retained private histories are diagnostic relationships;
    they do not extend the machine lifetime of retired Visitor storage.
    """
    from config_tail_uses import _TailAccessBridge
    from config_tail_control import _TailActiveControl
    require(type(state) is _TailPhasesState and type(bridge) is _TailAccessBridge and
        type(bridge.control) is _TailActiveControl and bridge.control.state is state and
        uses is bridge.uses and len(bridge.control.guards)==len(state.phases),
        'Tail exit lacks its actual current source/control/access collection')
    c=state.predecessor.phases.widgets.views.context;V=c.headers['visitor']['proof']['function']
    frame=c.headers['visitor_frame'];out=[]
    for i,(phase,guard) in enumerate(zip(state.phases,bridge.control.guards)):
        if phase.suffix is None or phase.suffix['normal'] is None:continue
        normal=phase.suffix['normal'];decoder=normal['after'].decoder
        require(guard['entry_index']==i and guard['entry'] is phase.entry and
            phase.entry is state.entries[i] and guard['lineage']['normal'] is not None and
            normal['after'].count==Span(1,1) and normal['LEDQ'].count==Span(0,0) and
            normal['logical_remaining_fields']==0 and not normal['old_history_Q_still_live'] and
            phase.control_effects['complete_after']==V+6118 and phase.suffix_effects['complete_after']==V+7112 and
            phase.suffix_effects['LED'][-1]['site']==V+7108 and
            decoder.header is phase.entry.sequence.decoder.header and decoder.payload is phase.entry.sequence.decoder.payload and
            decoder.header.storage is c.frame and decoder.header.offset==236 and decoder.header.extent==8,
            'Tail normal return precedes its same-source late stores or retires the caller D')
        local=tuple(u for u in uses if u.phase_index==i)
        for screen in phase.controls['normal']['screens']:
            require(sum(u.role=='Screen_output_words' and u.source['invocation'] is screen['invocation'] for u in local)==1,
                'Tail original return omits a same-call Screen word epoch')
        first=phase.controls['normal']['screens'][0]
        for role in ('first_Screen_scratch_words','first_Config_Screen_words'):
            require(sum(u.role==role and u.source['Screen'] is first for u in local)==1,
                'Tail original return omits its first-only Screen words')
        for pattern in phase.suffix['LED']['patterns']:
            for role in ('result_words','pattern_words'):
                require(sum(u.role==role and u.source['invocation'] is pattern['invocation'] for u in local)==1,
                    'Tail original return omits its same-source Pattern word epoch')
        require(sum(u.role=='normal_config_words' and u.source['LED'] is phase.suffix['LED']['normal'] and
            u.continuation[-1]==V+7112 for u in local)==1,
            'Tail original return omits its late normal Config Pattern words')
        original=guard['original_return'];restore=bridge.control.structural.restore
        require(original[-1]==restore['original_return_PC'] and restore['return_SP']==frame.start+1608 and
            original[:5]==tuple(V+off for off in (56,58,62,66,68)),
            'Tail normal return changes its actual original saved-PC/SP relation')
        out.append(_TailExit(i,decoder,normal,original))
    return tuple(out)


@dataclass(frozen=True)
class _VisitorErrorReturn:
    group:str
    origin:object
    decoder:object
    error:int
    carrier:int
    transport:tuple
    original_return:tuple
    return_SP:int


def _error_parent_path(state,raw,segments,island):
    """Authored finite carrier path, including each current helper return.

    Source-derived predicates choose these paths; this checks their current
    instructions/edges without interpreting arbitrary scalar instructions.
    """
    from config_regions import _control
    from config_owner_entry import _direct_target
    c=raw.selection.context;V=c.headers['visitor']['proof']['function'];frame=c.headers['visitor_frame']
    rows=raw.selection.selected;path=[]
    for first,last in segments:
        require(V+first in rows and V+last in rows,'earlier error carrier lost a current segment endpoint')
        path.extend(at for at in sorted(rows) if V+first<=at<=V+last)
    suffix={56:(56,),140:(140,144,56),170:(170,172,56),4188:(4188,4190,56),
        4194:(4194,4196,56),4340:(4340,4342,4344,56)}
    if island not in suffix:
        require(island in (4200,4204,4210,4214,4218,4224,4228,4232,4236,4240,4244,4250,4254,
            4306,4310,4314,4318,4324,4330,4334) or island in range(4258,4303,4),
            'earlier error selects another authored carrier island')
        loader=rows.get(V+island)
        require(loader is not None,'earlier error lost its carrier loader')
        suffix[island]=(island,island+len(loader.raw),4340,4342,4344,56)
    path.extend(V+off for off in suffix[island])
    require(len(path)==len(set(path)),'earlier error carrier path repeats a parent instruction')
    for at,following in zip(path,path[1:]):
        row=rows.get(at)
        require(row is not None and state.objects.read_only(at,len(row.raw))==row.raw,
            'earlier error carrier has stale current bytes')
        family,conditional=_control(row);next_pc=at+len(row.raw)
        if family=='bl':
            edge=raw.selection.edges.get(at)
            require(edge is not None and edge[0]=='bl' and following==next_pc and
                any(n['function']==edge[1] and n['role']==edge[2] and
                    (n['entry_SP'],n['return_PC'],n['return_SP'])==(frame.start,next_pc,frame.start)
                    for n in raw.nested_returns),'earlier carrier helper lost its original same-call return')
        elif family in ('b','cbz','cbnz'):
            target=_direct_target(row,family)
            require(following==target or conditional and following==next_pc,
                'earlier error carrier changes its selected branch/flag successor')
        else:require(family is None and following==next_pc,'earlier carrier has an unclassified PC effect')
    return tuple(path)


def _widget_error_transport(state,raw,origin):
    from config_decoded_widgets import _WidgetCallOrigin,_checked_widget_origin
    c=raw.selection.context;V=c.headers['visitor']['proof']['function']
    origin=_checked_widget_origin(origin,c);call=origin.call;code=origin.error
    if type(call) is _WidgetCallOrigin:
        site=call.site-V
        roots=tuple(p for p in raw.selection.children.values() if
            tuple(p.get(k) for k in ('function','role','extent','frame'))==call.root_body)
        require(roots and any(n['function']==call.root_body[0] and n['role']==call.root_body[1] and
            (n['entry_SP'],n['return_PC'],n['return_SP'])==(c.headers['visitor_frame'].start,call.site+4,c.headers['visitor_frame'].start)
            for n in raw.nested_returns),'Widget error lost its current typed source child/original return')
        if origin.condition is not None:
            if origin.choice_index is None:
                require(site in (814,1558,2784) and code==4 and 'length>remaining' in origin.condition,
                    'Widget short input has another actual length/cursor predicate')
                branch={814:834,1558:1578,2784:2810}[site]
                return _error_parent_path(state,raw,((site+4,branch),),4224),('no n-byte advance',origin.condition)
            domains={656:(7,674,170),756:(2,774,4200),1094:(1,1120,4200),1284:(1,1306,4200),
                1336:(4,1356,4200),1786:(3,1806,4200),1910:(5,1930,4200),3244:(9,3264,4200),
                3278:(3,3298,4200),3312:(2,3332,4200),3346:(2,3364,4200)}
            require(site in domains and code==14,'Widget domain error selects another decoded enum')
            maximum,branch,island=domains[site]
            require(origin.condition=='same decoded u32>'+str(maximum),
                'Widget domain error changed its same decoded selector bound')
            return _error_parent_path(state,raw,((site+4,branch),),island),('successful child; out-of-domain selector',origin.condition)
        varints={416:140,656:140,756:4204,814:4204,1094:4204,1284:4204,1336:4204,
            1558:4204,1786:4204,1910:4204,2598:4204,2784:4318,3244:4204,3278:4204,3312:4204,3346:4204}
        if site in varints:
            require(code in (4,5) and call.body[2]=='take_varint_u32','Widget parser error has another Result8 ABI')
            return _error_parent_path(state,raw,((site+4,site+(10 if site==2784 else 8)),),varints[site]),('Result8 tag1; initialized error byte',code)
        if site in (606,696,722,948):
            require(code==4 and call.body[2]=='seq_u8','Widget u8 error has another packed ABI')
            return _error_parent_path(state,raw,((site+4,site+12),),{606:4214,696:4214,722:4232,948:4340}[site]),('packed u8',(code<<8)|2)
        if site in (972,994,1016,1038,1060,1242,1814,1996):
            require(code in (4,6) and call.body[2]=='seq_bool','Widget bool error has another register ABI')
            return _error_parent_path(state,raw,((site+4,site+(8 if site==1996 else 6)),),4254 if site==1996 else 4210),('bool r0=1; r1=error',code)
        if site in (1134,1166,1368,1402,1436,1666,1700,1940,1970,2676,2708,3132):
            require(code==4 and call.body[2]=='seq_f32','Widget float error has another Result8 ABI')
            if site in (1134,1166):
                selectors=tuple(v for v in origin.trace if type(v) is tuple and len(v)==3 and
                    v[0]=='GraphRange::Fixed guard' and type(v[1]) is _WidgetCallOrigin and
                    v[1].site==V+1094 and v[1].body[2]=='take_varint_u32' and v[2]=='same GraphRange==Fixed')
                require(len(selectors)==1,'Fixed float error lost its actual selector guard')
                selector=selectors[0][1]
                first=call if site==1134 else next((v[1] for v in origin.trace if type(v) is tuple and len(v)==4 and
                    v[0]=='GraphRange::Fixed.min' and type(v[1]) is _WidgetCallOrigin and
                    v[1].site==V+1134 and v[2]['kind']=='some' and v[2]['state'] is call.before),None)
                require(first is not None and any(v['kind']=='success' and v['state'] is first.before.decoder
                    for v in selector.choices) and call.before.count==Span(2 if site==1134 else 1,2 if site==1134 else 1),
                    'Fixed float error lost its actual selector/before-outcome relation')
            band=site in (2676,2708)
            return _error_parent_path(state,raw,((site+4,site+(10 if band else 8)),),4244 if band else 4218),('f32 Result tag2; initialized error byte',1444 if band else 1068)
        if site==2544:
            option=call.choices[origin.choice_index]
            require(code in (4,5) and 'nested_origin' in option and option['nested_origin'][0] in (0,1),
                'Widget WarningColors error lost its actual nested Color occurrence')
            ordinal,before,body,failed,previous=option['nested_origin']
            require(len(previous)==ordinal and before.count==Span(2-ordinal,2-ordinal) and
                failed['kind']=='error' and failed['state'].decoder is origin.after and
                (previous[-1][2]['state'] is before and previous[0][0].decoder is call.before.decoder
                    if previous else before.decoder is call.before.decoder) and
                all(choice['kind']=='some' and choice['state'].count==Span(1-i,1-i) and
                    first.count==Span(2-i,2-i) and p==body for i,(first,p,choice) in enumerate(previous)),
                'Widget WarningColors error lost its actual first-success/second-call predecessor')
            return _error_parent_path(state,raw,((2548,2554),),4324),('WarningColors tag2/error1002',option['nested_origin'])
        packed={574:4340,1466:4236,1502:4240,1730:4250,2736:4306,3162:4330,3196:4334,
            2470:4310,2506:4314,**{2038+36*k:4258+4*k for k in range(12)}}
        require(site in packed and code in (4,5),'Widget error has an unclassified actual scalar call')
        return _error_parent_path(state,raw,((site+4,site+(20 if site in (574,2736,3162,3196) else 22)),),packed[site]),(
            'packed u16/Color tag2; EQ skips ANDNE; extracted error16',(code<<16)|2)
    require(type(call) is tuple and call and type(call[0]) is int,'Widget conditional error lost its actual source operation')
    site=call[0]-V
    if site in (852,1598,2828):
        require(code==8 and 'invalid UTF8' in origin.condition,'Widget UTF8 error changes its paired source predicate')
        return _error_parent_path(state,raw,((site+4,site+10),),4228),('same advanced p/n; UTF8 Result tag1',call[1])
    if site in (884,1618,2862):
        require(code==14 and ('length>' in origin.condition or 'length>5' in origin.condition),
            'Widget append error lost its UTF8-success capacity guard')
        return _error_parent_path(state,raw,((site+4,site+6),),4200),('nonzero append; same advanced cursor',origin.condition)
    if site==2960:
        require(code==14 and origin.condition in ('1<=length<=5 AND some byte outside32..126',
            '1<=length<=5 AND all bytes==32'),'Band range error lost its active nonempty label predicate')
        branch=2970 if 'outside' in origin.condition else 2980
        return _error_parent_path(state,raw,((branch,branch),(3106,3110)),4200),('nonempty active Band label',origin.condition)
    if site==3042:
        require(code==14 and origin.condition=='N>3 AND fourth candidate decoded before j==3 guard',
            'Band capacity error precedes the fourth complete candidate')
        return _error_parent_path(state,raw,((3042,3044),(3106,3110)),4200),('parsed fourth Band; no publication',origin.condition)
    if site==432:
        require(code==14 and origin.condition=='same N>=65 AND actual post-length remaining>=N',
            'Widget pre-clear capacity lost its same length/remaining predicate')
        return _error_parent_path(state,raw,((432,446),),170),('old Widget array unchanged',origin.condition)
    require(site==3758 and code==14 and origin.condition=='N>64 AND complete candidate decoded before i==64 guard',
        'Widget capacity return precedes its actual complete candidate')
    return _error_parent_path(state,raw,((3758,3760),),170),('parsed candidate64; no spare borrow/publication',origin.condition)


def _earlier_error_returns(state,raw):
    """Join actual retained earlier origins within a fresh enclosing boundary.

    No diagnostic tuple or independent caller-supplied owner can enter. The
    caller must freshly validate the same private phase/current input graph.
    Source and Tail error groups compose separately with these occurrences.
    """
    from config_tail_control import _structural_data
    from config_decoded_widgets import _widget_decoder
    require(type(state) is _TailPhasesState,'earlier return lacks its actual private Tail producer')
    _structural_data(raw,state)
    widgets=state.predecessor.phases.widgets;c=widgets.views.context;widgets._checked_errors()
    V=c.headers['visitor']['proof']['function'];R=c.headers['root']['proof']['function'];frame=c.headers['visitor_frame']
    restore=raw.restore
    require(restore['saved_interval']==(frame.start+1512,frame.start+1608) and
        restore['return_SP']==c.frame.start and restore['original_return_PC']==R+130,
        'earlier error lost its original Visitor saved-PC/SP equation')
    original=(V+56,V+58,V+62,V+66,V+68,restore['restore']['function'],restore['restore']['function']+4,R+130)
    actual=c.headers['visitor']['sequence_state'].decoder;out=[]
    def emit(group,origin,decoder,error,carrier,path,saved,relation):
        require(type(error) is int and error in (4,5,6,8,14) and carrier&255==error and
            decoder.header is actual.header and decoder.payload is actual.payload and
            decoder.header.storage is c.frame and (decoder.header.offset,decoder.header.extent)==(236,8) and
            saved in c.headers['visitor']['protected'] and (saved.start,saved.extent)==(frame.start+1512,96),
            'earlier error return changes its actual caller decoder/source/carrier or original saves')
        out.append(_VisitorErrorReturn(group,origin,decoder,error,carrier,(path,relation),original,c.frame.start))
    for error in c.error_frontiers:
        code=error.outcome['error']
        require(error.packed_r0==(code<<8)|1,'prefix error lost its packed actual child result')
        path=_error_parent_path(state,raw,((54,54),),56)
        emit('prefix',error,error.after.decoder,code,error.packed_r0>>8,path,error.saved,
            ('prefix-only LSRS8',error.site,error.outcome,error.assigned_fields,error.original_child_return))
    for error in widgets.views.error_frontiers:
        cause=error.origin;code=error.summary['error']
        if type(cause[0]) is int:
            site=cause[0]-V;choice=cause[3]
            require(_widget_decoder(choice['state']) is error.after,'Views error changed its exact child decoder')
            require(site in (82,130,214,246),'Views error has another current source call')
            if site==82:path=_error_parent_path(state,raw,((86,110),),56);abi=('packed u8 error2',(code<<8)|2)
            elif site==214:path=_error_parent_path(state,raw,((218,226),),56);abi=('packed u8 error2',(code<<8)|2)
            else:
                path=_error_parent_path(state,raw,((134,138),) if site==130 else ((250,254),),140)
                abi=('Result8 tag1; initialized error1065',code)
        else:
            kind=cause[0]
            table={'preclear':(((154,168),),170,14),'short string':(((260,264),),4188,4),
                'UTF8':(((286,292),),4194,8),'String14 capacity':(((306,308),),170,14),
                'precommit':(((330,358),),170,14)}
            require(kind in table,'Views conditional error has another actual source guard')
            segments,island,wanted=table[kind];require(code==wanted,'Views conditional carrier changed')
            path=_error_parent_path(state,raw,segments,island);abi=(kind,cause,error.summary['guard'])
        emit('Views',error,error.after,code,code,path,error.saved,abi)
    for path in widgets.error_frontiers:
        origin=path.summary['_origin'];transport,abi=_widget_error_transport(state,raw,origin)
        emit('Widgets',path,origin.after,origin.error,origin.error,transport,origin.saved,abi)
    return tuple(out)


def _checked_error_exit(state,raw,group,origin,decoder,error,carrier,transport,protected):
    c=state.predecessor.phases.widgets.views.context;frame=c.headers['visitor_frame']
    V=c.headers['visitor']['proof']['function'];R=c.headers['root']['proof']['function']
    actual=c.headers['visitor']['sequence_state'].decoder;restore=raw.restore
    require(type(error) is int and error in (4,5,6,8,14) and type(carrier) is int and carrier&255==error and
        decoder.header is actual.header and decoder.payload is actual.payload and decoder.header.storage is c.frame and
        (decoder.header.offset,decoder.header.extent)==(236,8) and restore['original_return_PC']==R+130 and
        restore['return_SP']==c.frame.start and restore['saved_interval']==(frame.start+1512,frame.start+1608) and
        any(v.space==frame.space and v.start<=frame.start+1512 and frame.start+1608<=v.start+v.extent and
            not v.writable and v.phase==frame.phase for v in protected),
        'Source/Tail error changes its source/carrier or original protected return')
    original=(V+56,V+58,V+62,V+66,V+68,restore['restore']['function'],restore['restore']['function']+4,R+130)
    return _VisitorErrorReturn(group,origin,decoder,error,carrier,transport,original,c.frame.start)


def _source_error_returns(state,raw):
    """Actual eight-call errors and checked suffix carriers, occurrence by occurrence."""
    from config_tail_control import _structural_data
    from config_source_control import _SourceActiveControl
    from config_decoded_widgets import _widget_decoder
    _structural_data(raw,state);previous=state.predecessor;phases=previous.phases
    if not phases.entries:
        require(previous.control is None and not phases.phases,'unentered Sources fabricated an error invocation')
        return ()
    require(type(previous.control) is _SourceActiveControl and previous.control.state is phases and
        len(previous.control.source_guards)==len(phases.phases),'Source error return lost its actual guarded predecessor')
    c=phases.widgets.views.context;V=c.headers['visitor']['proof']['function'];frame=c.headers['visitor_frame'];out=[]
    # Authored actual Source call, error-byte load and B56. Every interposed
    # tag helper/capture/flag branch is retained by the complete parent span.
    sites=((4376,4398,1130),(4906,4926,518),(4944,4964,630),(4982,5002,746),
        (5154,5174,882),(5254,5274,942),(5292,5312,1002),(5332,5352,1066))
    for i,(phase,guard) in enumerate(zip(phases.phases,previous.control.source_guards)):
        require(phase.entry is phases.entries[i] and guard['entry_index']==i and
            len(guard['invocations'])==len(phase.array['invocations']),
            'Source error return changes its actual phase/call association')
        for error in phase.array['errors']:
            index=error['index'];require(type(index) is int and 0<=index<len(phase.array['invocations']),
                'Source error return changed its actual fixed-array index')
            call=phase.array['invocations'][index];checked=guard['invocations'][index];result=call['result']
            matches=tuple(v for v in result['errors'] if v['origin'] is error['origin'] and v['child_outcome'] is error['child_outcome'])
            require(len(matches)==1 and any(matches[0] is v for v in checked['error_alternatives']) and
                checked['before'] is call['before'] and call['before'].count==Span(8-index,8-index) and
                error['count_after']==Span(7-index,7-index) and error['tag']==65535 and result['none'] is None and
                error['old_array_unchanged'] and error['error']==matches[0]['error'] and error['cursor']==matches[0]['cursor'],
                'Source error return loses its positive count/actual failed primitive or array publication guard')
            site,last,cell=sites[index]
            require(call['site']==V+site and checked['site']==call['site'] and
                any(node['function']==result['proof']['function'] and
                    (node['entry_SP'],node['return_PC'],node['return_SP'])==(frame.start,V+site+4,frame.start)
                    for node in phases.returns),'Source error return lost its original ChannelSource call')
            output=borrow(frame,cell-2,22,frame.phase,True)
            require(any(view==output and off<=2<off+n for view,off,n in error['possible_writes']),
                'Source error return reads an uninitialized error byte')
            path=_error_parent_path(state,raw,((site+4,last),),56)
            decoder=_widget_decoder(error['child_outcome']['state'])
            require(decoder.cursor==error['cursor'],'Source error return changed its actual failed-child cursor')
            transport=(path,('tag65535; tag-helper EQ; captured STR preserves flags; BNE not taken',
                cell,error['origin'],error['child_outcome'],index))
            out.append(_checked_error_exit(state,raw,'Sources',error,decoder,error['error'],error['error'],transport,phase.entry.protected))
        suffix=phase.remaining;lineage=guard['suffix_lineage']
        require((suffix is None)==(lineage is None),'Source suffix error lost its publication/entry guard')
        if suffix is None:continue
        require(len(lineage['error_carriers'])==len(suffix['errors']), 'Source suffix error occurrence collection changed')
        for error,edge in zip(suffix['errors'],lineage['error_carriers']):
            option=error['child_outcome'];code=error['error']
            require(edge['error'] is error and edge['outcome'] is option and edge['stopped_before']==V+56 and
                edge['whole_register_narrowed'] is False and edge['path'] and
                error['cursor']==option['cursor'],'Source suffix error changed its actual origin/whole carrier')
            packed=edge['carrier']=='packed_0x02000000_or_error_u8'
            require(packed or edge['carrier']=='initialized_error_u8_or_constant14','Source suffix has another carrier ABI')
            carrier=(0x02000000|code) if packed else code
            transport=(edge['path']+(V+56,),('whole r5 preserved through MOV56',carrier,edge))
            out.append(_checked_error_exit(state,raw,'suffix',error,_widget_decoder(option['state']),code,carrier,transport,phase.entry.protected))
    return tuple(out)


def _tail_error_returns(state,bridge):
    """Consume already checked actual Tail caller paths after the fresh batch."""
    from config_tail_uses import _TailAccessBridge
    require(type(bridge) is _TailAccessBridge and bridge.control.state is state and
        len(bridge.control.guards)==len(state.phases),'Tail error return lost its actual current batch/control')
    raw=bridge.control.structural;out=[]
    for phase,guard in zip(state.phases,bridge.control.guards):
        require(guard['entry'] is phase.entry,'Tail error return changes its actual phase entry')
        errors=phase.controls['errors']+(() if phase.suffix is None else phase.suffix['errors'])
        require(len(errors)==len(guard['lineage']['errors']),'Tail error occurrence collection changed')
        for error,edge in zip(errors,guard['lineage']['errors']):
            require(edge['source_error'] is error and edge['call'] is error['origin'] and edge['origin'] is error['cause'] and
                edge['return_carrier']==error['error'] and edge['requires_full_original_restore'] and
                error['after'].decoder.cursor==error['cursor'],'Tail error return changed its actual origin/carrier/decoder')
            out.append(_checked_error_exit(state,raw,'Tail',error,error['after'].decoder,error['error'],edge['return_carrier'],
                (edge['path'],('actual caller path and partial Config groups',edge['completed_groups'],edge)),phase.entry.protected))
    return tuple(out)


def _tail_error_summary(prior,control):
    """Preserve every prior and new diagnostic occurrence exactly once."""
    from config_decoded_sources import _error_summary
    existing=prior['error_summary'];count=prior['error_occurrences']
    require(sum(v[-3] for v in existing)==count,'Tail predecessor diagnostic count changed')
    errors=[]
    if control is not None:
        for guard in control.guards:
            for edge in guard['lineage']['errors']:
                error=edge['source_error']
                errors.append(dict(stage=error['field'],error=error['error'],cursor=error['cursor'],
                    frontier=edge['path'][-1],completed_aggregates=edge['completed_groups']))
    rows,total=_error_summary((('Tail',tuple(errors)),))
    return tuple(existing)+rows,count+total


def _tail_description(state,bridge,uses,exits,raw):
    from collections import Counter
    from config_platform import _bounded_record
    prior=state.predecessor_description;c=state.predecessor.phases.widgets.views.context
    V=c.headers['visitor']['proof']['function'];R=c.headers['root']['proof']['function']
    control=None if bridge is None else bridge.control
    summaries,error_count=_tail_error_summary(prior,control)
    successes=tuple((e.phase_index,state.entries[e.phase_index].exit.source_normal['Fuel']['wire_tag'],
        state.entries[e.phase_index].exit.source_normal['Fuel']['emitted_tag'],e.decoder.cursor.lower,e.decoder.cursor.upper)
        for e in exits)
    result=dict(phase='first_boot_decoded_visitor_return_pending_finalize_canonical_validation_and_loader_routes',
        mode='sizez',successes=successes,
        success_columns=('Tail_phase','Fuel_wire','Fuel_emitted','cursor_lower','cursor_upper'),
        error_summary=summaries,error_occurrences=error_count,error_columns=prior['error_columns'],
        diagnostic_error_envelopes_are_not_paths=True,exact_error_relations_retained_privately=True,
        Visitor_return=dict(frontier=R+130,return_SP=c.frame.start,original_saved_PC=True,
            normal_returns=len(exits),error_returns=error_count,
            original_saved_bytes=96,retired_Visitor_bytes=1608,Visitor_local_borrows_retired=True,
            loader_successor_executed=False) if exits or error_count else None,
        declared_return_frontier=dict(frontier=R+130,reached=bool(exits or error_count)),
        physical_completion_sites=dict(Controls=V+6114,history=V+6732,reduced_motion=V+6768,LED=V+7108),
        conditional_Tail_accesses=dict(families=() if bridge is None else tuple(v.role for v in bridge.tail_families),
            descriptors_are_reached_uses=False,actual_inherited16_and_complete_policy_pending=True),
        source_guarded_accesses=dict(role_counts=tuple(sorted(Counter(v.role for v in uses).items())),
            completed_prefix_issued=False,runtime_observation=False),
        structural_control=dict(selected_instructions=len(raw.selection.selected),
            outside_shared_context_count=len(raw.outside_shared_contexts),
            separate_restore_context_count=len(raw.foreign_restore_contexts),
            pending_contexts_confer_state=False,unentered=not bool(state.entries)),
        ordinary_domain=prior['ordinary_domain'],execution_scope=prior['execution_scope'],
        source_context=prior['source_context'],source_qualification=prior['source_qualification'],
        external_qualifications=prior['external_qualifications'],exception_graph_limit=prior['exception_graph_limit'],
        exceptional_resume_established=False,exception_absence_established=False,all_observers_compatible=False,
        exception_safe_completion=False,unconditional_owner=False,hardware_qualified=False,initial_facts_reinitialized=False,
        requires_H8_consumed_input_closure=True,requires_M2_pre_init_source_reconciliation=True,
        complete_policy_and_resource_producer_pending=True,
        pending=('D finalize/canonical Compare','decoded validator and remaining loader routes/modes',
            'diagnostics/live sinks/PANICKED/full resource graph','actual complete policy/producer/max envelope',
            'final all-mode/max-input/native CI/H2 gates'))
    return _bounded_record(result)


def _visitor_error_returns(state,raw,bridge):
    errors=_earlier_error_returns(state,raw)+_source_error_returns(state,raw)
    if bridge is not None:errors+=_tail_error_returns(state,bridge)
    prior=state.predecessor_description
    _,expected=_tail_error_summary(prior,None if bridge is None else bridge.control)
    require(len(errors)==expected,'Tail actual error occurrence/original-return collection differs from diagnostics')
    return errors


@dataclass(frozen=True)
class _TailState:
    phases:_TailPhasesState
    access_bridge:object
    exits:tuple
    error_exits:tuple
    unentered_structure:object=None

    def current(self):
        from config_tail_uses import _check_tail_uses
        from config_tail_control import _tail_structural,_structural_data
        from config_decoded_sources import _phase_graph
        if self.phases.entries:
            require(self.unentered_structure is None and self.access_bridge is not None,
                'Active Tail cannot substitute an unentered structure')
            # The fresh batch already checks control -> phases -> same actual
            # Source predecessor. Do not traverse that boundary a second time.
            uses=_check_tail_uses(self.access_bridge);raw=self.access_bridge.control.structural
            normal=_tail_exits(self.phases,self.access_bridge,uses)
        else:
            require(self.access_bridge is None and not self.exits,'Unentered Tail fabricated an actual Tail normal return')
            self.phases.current();raw=_tail_structural(self.phases)
            require(_phase_graph(_structural_data(raw,self.phases))==self.unentered_structure,
                'Unentered Tail current structure changed')
            normal=()
        require(len(normal)==len(self.exits) and all(a.phase_index==b.phase_index and a.decoder is b.decoder and
            a.source_normal is b.source_normal and a.original_return==b.original_return for a,b in zip(normal,self.exits)),
            'Tail outgoing state changes its exact source/normal/original-return association')
        errors=_visitor_error_returns(self.phases,raw,self.access_bridge)
        require(len(errors)==len(self.error_exits) and all(type(b) is _VisitorErrorReturn and a.group==b.group and
            a.origin is b.origin and a.decoder is b.decoder and (a.error,a.carrier,a.transport,a.original_return,a.return_SP)==
            (b.error,b.carrier,b.transport,b.original_return,b.return_SP) for a,b in zip(errors,self.error_exits)),
            'Tail outgoing error changes its exact origin/carrier/original-return association')
        return self


def _first_boot_decoded_visitor_return_state(objects,instructions,source_context):
    """This component checks the admitted named ordinary/callback segment.

    Actual private predecessor/source/control/effects and original returns
    compose once. Neither this projection nor its receipt initializes state
    or executes the loader successor. Exceptional resume remains unresolved.
    """
    from config_tail_uses import _tail_access_group
    from config_tail_control import _tail_structural,_structural_data
    from config_decoded_sources import _phase_graph
    state=_first_boot_tail_phases(objects,instructions,source_context)
    bridge=None;uses=();exits=();unentered=None
    if state.entries:
        bridge,uses=_tail_access_group(state);raw=bridge.control.structural
        exits=_tail_exits(state,bridge,uses)
    else:
        state.current();raw=_tail_structural(state)
        unentered=_phase_graph(_structural_data(raw,state))
    errors=_visitor_error_returns(state,raw,bridge)
    description=_tail_description(state,bridge,uses,exits,raw)
    return description,_TailState(state,bridge,exits,errors,unentered)


def first_boot_decoded_visitor_return(objects,instructions,source_context):
    return _first_boot_decoded_visitor_return_state(objects,instructions,source_context)[0]
