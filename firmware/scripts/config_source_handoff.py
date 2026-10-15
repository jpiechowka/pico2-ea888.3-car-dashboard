"""Private, current Widget-to-Sources pointer and prefix transport.

No Source is decoded here. Contents follow the exact matched constructors;
reserved-cell and completed-prefix protection survive ordinary descendants.
Neither this record nor a public predecessor description grants ownership.
"""
from dataclasses import dataclass
from config_abstract import Span,SequenceInterface
from config_regions import Storage,borrow,require_writes
from config_decode_effects import _object
from validation import require


# (cell, pointee, producer offsets relative to the actual ConfigSeed).
_CARRIERS=((216,1001,(472,476,478)),(224,688,(544,546)),
    (228,576,(540,542)),(232,1420,(528,532)),(236,1380,(534,538)),
    (240,1400,(522,526)),(244,1440,(492,500)),(248,1360,(466,474)))


def _source_carrier_cells(context):
    frame=context.headers['visitor_frame']
    return tuple(_object(borrow(frame,cell,4,frame.phase),'reserved Sources pointer '+str(cell))
        for cell,target,sites in _CARRIERS)


def _carrier_contents(objects,rows,state):
    """Initialization is one exact store; every subsequent write preserves it."""
    from config_decoded_widgets import _scratch_effects
    from config_widget_effect_contracts import LOCAL_STORES,INLINE_COPIES,FIXED_ALIAS_STORES
    c=state.views.context;frame=c.headers['visitor_frame'];V=c.headers['visitor']['proof']['function']
    expected={466:('sub.w','r4, r7, #0xf0'),472:('add','r0, sp, #0x3e8'),
        474:('str','r4, [sp, #0xf8]'),476:('adds','r0, #0x1'),478:('str','r0, [sp, #0xd8]'),
        492:('sub.w','r0, r7, #0xa0'),500:('str','r0, [sp, #0xf4]'),
        522:('sub.w','r0, r7, #0xc8'),526:('str','r0, [sp, #0xf0]'),
        528:('sub.w','r0, r7, #0xb4'),532:('str','r0, [sp, #0xe8]'),
        534:('sub.w','r0, r7, #0xdc'),538:('str','r0, [sp, #0xec]'),
        540:('add','r0, sp, #0x240'),542:('str','r0, [sp, #0xe4]'),
        544:('add','r0, sp, #0x2b0'),546:('str','r0, [sp, #0xe0]')}
    for off,pair in expected.items():
        row=rows.get(V+off)
        require(row is not None and (row.mnemonic,row.operands)==pair and
            objects.read_only(V+off,len(row.raw))==row.raw,'Sources carrier initialization or current bytes differ')
    # The complete parent literal and current structural selection establish
    # the actual r7=V+1600 prologue and normal predecessor of this constructor.
    guards=_source_carrier_cells(c);creations={sites[-1]:cell for cell,target,sites in _CARRIERS}
    for site,offset,size in LOCAL_STORES+FIXED_ALIAS_STORES:
        out=borrow(frame,offset,size,frame.phase,True)
        if site in creations:
            require((offset,size)==(creations[site],4),'Sources carrier has a different initializing store')
            relevant=tuple(v for v in guards if v.start!=frame.start+offset)
        else:relevant=guards
        require_writes((out,),((out,0,size),),relevant)
    for origin,destination,size,writes in INLINE_COPIES:
        out=borrow(frame,destination,size,frame.phase,True)
        require_writes((out,),tuple((out,off,n) for site,off,n in writes),guards)
    # This reruns the authored partition, including all child copies, while
    # fresh scalar/aggregate interfaces independently retain the same cells.
    effects=_scratch_effects(objects,rows,state.views,state.parents,state.children)
    require(effects['selected_store_count']==230,'Sources carrier preservation lost complete parent effects')
    return tuple((borrow(frame,cell,4,frame.phase),frame.start+target) for cell,target,sites in _CARRIERS),effects


@dataclass(frozen=True)
class _SourceEntry:
    predecessor:object
    exit:object
    sequence:SequenceInterface
    carriers:tuple
    protected:tuple
    old_Source_array:object
    call_site:int
    call_arguments:tuple
    completed_Source_issued:bool=False


def _source_entries(objects,rows,state):
    """Check each actual ordered predecessor; a receipt cannot replace it."""
    from config_decoded_widgets import (_WidgetsState,_WidgetActiveControl,_CommittedWidget,_iteration_state,
        _WidgetControlInput,_widget_source_prefix,_advanced_source,_widget_control)
    from config_widget_uses import _check_use
    from config_structural import _freeze
    require(type(state) is _WidgetsState,'Sources predecessor is not the private Widget producer')
    bridge=state.family_bridge.current()
    require(objects is bridge.objects and rows is bridge.rows,'Sources predecessor uses another current input pair')
    c=state.views.context;frame=c.headers['visitor_frame'];V=c.headers['visitor']['proof']['function']
    if not state.exits:return ()
    require(type(state.control) is _WidgetActiveControl and state.control.state is state.views and
        state.control.parents is state.parents and state.control.children is state.children and
        V+4376 in state.control['stopped_frontiers'],'Sources predecessor lacks its actual guarded ordinary frontier')
    receiver={4348:('ldr','r0, [sp, #0x124]'),4352:('subs','r0, #0x1'),
        4354:('movs','r1, #0x8'),4356:('add.w','r6, sp, #0x468'),4360:('str','r0, [sp, #0x118]'),
        4362:('str','r0, [sp, #0x124]'),4364:('ldr','r0, [sp, #0x120]'),
        4366:('strd','r0, r1, [r7, #-100]'),4370:('sub.w','r1, r7, #0x64'),4374:('mov','r0, r6')}
    require((V+4350,None,V+4352) in state.control['successors'] and
        all(e.sequence.count==Span(8,8) for e in bridge.entries),
        'Sources predecessor loses its actual positive top8 guarded successor')
    for off,expected in receiver.items():
        row=rows.get(V+off)
        require(row is not None and (row.mnemonic,row.operands)==expected and
            objects.read_only(V+off,len(row.raw))==row.raw,'Sources actual receiver/header construction differs')
    carriers,effects=_carrier_contents(objects,rows,state);entries=[]
    # The retained active record contains mutable descriptions. Recompose its
    # original first candidate using the already authored interfaces and the
    # same current predecessor; never authorize from a frontier label alone.
    first=bridge.entries[0].sequence.decoder
    source=_widget_source_prefix(objects,rows,state.views,state.parents,state.children,first)
    advanced=(dict(normal=(),errors=()) if source.decoder is None else
        _advanced_source(objects,rows,state.views,state.parents,state.children,source.decoder))
    value=_WidgetControlInput(state.views,state.parents,state.children,source,advanced,effects,bridge.returns)
    checked=_widget_control(objects,rows,value)
    require(checked.profile==state.control.profile and checked.record==state.control.record,
        'Sources predecessor ordinary control/return relation changed')
    for exit in state.exits:
        seq=exit.sequence;q=exit.Source_sequence
        require(type(seq) is SequenceInterface and type(q) is SequenceInterface and
            seq.count==Span(7,7) and q.count==Span(8,8) and q.decoder==seq.decoder and
            q.header==borrow(frame,1500,8,frame.phase,True) and
            seq.header==c.headers['visitor']['sequence_state'].header and
            seq.decoder.header==c.headers['visitor']['sequence_state'].decoder.header and
            seq.decoder.payload==c.headers['visitor']['sequence_state'].decoder.payload,
            'Sources predecessor changed its actual top-Q/Source-Q/D epoch')
        before=exit.committed[0].before if exit.committed else seq.decoder
        candidates=tuple(e for e in bridge.entries if e.initialized_views==exit.initialized_views and e.sequence.decoder==before)
        require(len(candidates)==1,'Sources predecessor lost its actual Views/length source pairing')
        for index,item in enumerate(exit.committed):
            require(type(item) is _CommittedWidget and item.index==index and item.before==before and
                item.commit==(V+4018,V+4158,V+4164,V+554),'Sources source/commit order differs')
            uses=tuple(u for u in item.uses if u.role=='widget_assembly')
            require(len(uses)==1,'Sources source/commit lacks its actual last-store use')
            use=_check_use(bridge,uses[0])
            require(any(use is u for u in state.reached_uses) and use.candidate==index and
                use.decoder==before and use.after==item.after and use.source_fields==_freeze(dict(item.source_fields)),
                'Sources source/commit or retained actual-use pairing changed')
            before=item.after
        require(before==seq.decoder,'Sources predecessor has an unpaired final cursor')
        active=_iteration_state(state.views,len(exit.committed));protected=active.context.headers['visitor']['protected']
        require(all(v in protected for v in c.headers['visitor']['protected']),
            'Sources predecessor dropped inherited protection')
        # Same physical completed prefix, including N==0 without inventing a
        # spare slot or an initialized Source. SourceQ remains its real borrow.
        # Preserve the already assigned u16s/boot selector, complete Views
        # vector (including inactive capacity) and Widget length as physical
        # bytes, separately from their source-typed logical prefixes. Later
        # Sources/Fuel/Warning fields lie outside these exact intervals.
        previous=tuple(Storage(space,label,c.owner.start+off,size,1,frame.phase,False)
            for off,size,label in ((244,200,'completed Views vector and Widget length'),
                (14570,4,'decoded version fields'),(14575,1,'decoded boot selector'))
            for space in ('address',frame.space))
        # This borrow retains the old array from the once-derived initial
        # history. Prefix/Views/Widget writes never intersect B[32,208);
        # nothing re-reads the initial template after actual mutations.
        initial=c.boundary['history']['preclaim']['startup']['entry']['initial']
        require((initial['address'],initial['extent'])==(c.owner.start,14604),
            'Sources predecessor changed its retained old-array allocation')
        entries.append(_SourceEntry(state,exit,q,carriers,protected+previous+_source_carrier_cells(c),
            borrow(c.owner,32,176,frame.phase),V+4376,(('r0',frame.start+1128),('r1',q.header.start))))
    return tuple(entries)
