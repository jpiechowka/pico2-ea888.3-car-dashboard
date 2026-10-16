"""Private complete Tail composition, before loader finalization.

Only the same actual private Sources successor can enter. Current literals,
source effects, protection, original returns and active ordinary control are
separate checked obligations. Public descriptions cannot initialize state.
"""
from dataclasses import dataclass
from config_abstract import SequenceInterface,Span
from config_decode_effects import _object
from config_finite import _literal_body
from config_headers import VISITOR
from config_regions import Storage,borrow,require_disjoint
from config_tail_children import _function,_outline
from config_tail_parent_contracts import GRAMMARS
from config_widget_children import _name
from validation import require


_WINDOWS=(170,5630,5922,6018,6412,6450,7000,56)


def _parents(objects,rows,V,B):
    """All current selected Tail windows, with the same private Config pool."""
    name=_name(objects,VISITOR,V)
    def object_value(objects,value,descriptor):
        require(descriptor==dict(kind='same_Config') and value==B,'Tail reload loses the same private Config')
        return ()
    return tuple(_literal_body(objects,name,rows,VISITOR,'decoded_tail_'+str(off),GRAMMARS['visitor_'+str(off)],
        object_value,_outline,window=V+off,function_consumer=_function) for off in _WINDOWS)


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
