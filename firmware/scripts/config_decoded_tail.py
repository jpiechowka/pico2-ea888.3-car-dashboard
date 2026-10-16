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
