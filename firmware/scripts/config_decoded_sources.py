"""Closed private Sources/Warning/Fuel continuation, before Controls.

Matched parent windows alone do not grant active state. Actual source,
dependent alternatives, private predecessor, effects and ordinary closure
must all be composed before the public pending boundary can be issued.
"""
from config_finite import _literal_body
from config_headers import VISITOR
from config_source_contracts import GRAMMARS
from config_source_children import _function,_outline
from config_widget_children import _name
from validation import require
from dataclasses import replace
from config_abstract import SequenceInterface,Span
from config_regions import Storage,borrow,require_writes
from config_decode_effects import _object
from config_source_children import source_children,CHANNEL_SOURCE
from config_source_effects import channel_source_interface,SOURCE_EPOCHS


_WINDOWS=(4376,4410,5732,5860,5974,6218,6446,6656)


def _parents(objects,rows,V,B):
    """Complete authored selected windows, with source-owned pool values."""
    name=_name(objects,VISITOR,V)
    def object_value(objects,value,descriptor):
        if descriptor==dict(kind='same_Config'):
            require(value==B,'Sources reload loses the same private Config');return ()
        # The LDR loads the scalar mask itself, not a pointer to mask bytes.
        require(descriptor==dict(kind='warning_error_mask',hex='ff000002') and value==0x020000ff,
            'Sources warning packed error mask differs')
        return ()
    return tuple(_literal_body(objects,name,rows,VISITOR,'decoded_sources_'+str(off),GRAMMARS['visitor_'+str(off)],
        object_value,_outline,window=V+off,function_consumer=_function) for off in _WINDOWS)


def _source_array(objects,rows,parents,sequence,protected,old_array):
    """Eight fresh same-Q invocations; this stage still cannot commit Config.

    Each package retains its own selected Source alternatives and cursor
    relation. Independent field alternatives are not multiplied into complete
    eight-Source configurations. The next call consumes the guarded normal
    postcursor, never a translated previous invocation or retired scratch.
    """
    require(type(sequence) is SequenceInterface and sequence.count==Span(8,8) and
        sequence.header.storage.extent==1608 and sequence.header.offset==1500 and
        isinstance(old_array,Storage) and not old_array.writable and old_array.extent==176,
        'Sources fixed array lost its actual eight-element input/old allocation')
    frame=sequence.header.storage;phase=frame.phase
    require(old_array.phase==phase,'Sources array changes its source/protected epoch')
    expected=tuple(p['function'] for p in parents)
    V=parents[0]['function']-4376
    require(expected==tuple(V+off for off in _WINDOWS) and parents==_parents(objects,rows,V,old_array.start-32),
        'Sources fixed array has another selected parent family')
    children=source_children(objects,rows,parents)
    source_edges=tuple(sorted((p['function']+off,children[target,role]) for p in parents
        for off,operation,target,role,extent in p['ordinary_edges'] if role==CHANNEL_SOURCE))
    require(tuple(site-V for site,proof in source_edges)==tuple(v[0] for v in SOURCE_EPOCHS),
        'Sources array omits or reorders an actual current invocation')
    top=borrow(frame,288,8,phase)
    guard=tuple(protected)+(old_array,_object(top,'stable top Q during Sources'))
    packages=[];errors=[];live=[];current=sequence
    child_frame=Storage(frame.space,'actual Sources child',frame.start-72,72,8,phase,True)
    for index,((site,proof),(offset,out,tag)) in enumerate(zip(source_edges,SOURCE_EPOCHS)):
        require(site==V+offset and current.count==Span(8-index,8-index),
            'Sources call consumes another predecessor/count epoch')
        output=borrow(frame,out,22,phase,True)
        result=channel_source_interface(objects,rows,proof,current,output,child_frame,guard+tuple(live))
        # Error descriptions preserve the exact parsed prefix, same input and
        # decrement-before-child cursor. None is an explicit missing field.
        errors.extend(dict(v,index=index,parsed_prefix=index,old_array_unchanged=True,
            ordinary_frontier=V+56) for v in result['errors'])
        if result['none'] is not None:
            errors.append(dict(result['none'],index=index,parsed_prefix=index,old_array_unchanged=True,
                ordinary_frontier=V+170))
        normal=result['normal']
        if normal is None:
            current=None;break
        require(normal['count_after']==Span(7-index,7-index) and normal['before']==current.decoder and
            normal['tag_domain']==(0,1) and normal['return_site']==site+4,
            'Sources normal result is not paired with its current selector/return')
        next_sequence=replace(current,decoder=normal['after'],count=normal['count_after'])
        captured=borrow(frame,tag,4,phase,True)
        require_writes((captured,),((captured,0,4),),guard+tuple(live)+(_object(output,'current Source output'),))
        packages.append(dict(index=index,site=site,source=normal,sequence_before=current,sequence_after=next_sequence,
            output=output,captured_tag=captured,original_returns=result['returns'],
            opaque_padding=result['output_padding'],pending_roles=result['pending_owned_roles']))
        live.extend((_object(borrow(frame,out,21,phase),'parsed Source '+str(index)),_object(captured,'captured Source tag '+str(index))))
        current=next_sequence
    require(current is None or len(packages)==8 and current.count==Span(0,0),
        'Sources array bypasses one of its eight complete normal invocations')
    return dict(packages=tuple(packages),errors=tuple(errors),sequence=current,old_array=old_array,
        protected_outputs=tuple(live),fixed_array_committed=False,
        requires_actual_predecessor_control_transports_and_owned_use=True)


# Each row is one authored transfer, not a policy reconstructed from new code.
# Opaque padding/inactive variant payload can be moved without being typed as
# an initialized semantic value. Index i always names the same parsed package.
_CAPTURE=((5430,1131,1480,19),(5444,518,828,20),(5458,630,712,20),(5470,746,600,20),
    (5482,882,488,20),(5496,942,1176,20),(5508,1002,856,20),(5410,1066,804,20))
_TRANSPOSE=((5768,1480,1340,19),(5786,828,1320,20),(5796,712,1300,20),(5808,600,1280,20),
    (5820,488,1260,20),(5830,1176,1240,20),(5840,856,1220,20),(5852,804,1200,20))
_CARRIER_COPY=((4422,1340,1460,19),(4444,1320,688,20),(4458,1300,576,20),(4472,1280,1440,20),
    (4486,1260,1420,20),(4500,1240,1400,20),(4514,1220,1380,20),(4524,1200,1360,20))
_FINAL_LOCAL=((4558,1460,436,19),(4582,688,416,20),(4594,576,396,20),(4606,1440,376,20),
    (4618,1420,356,20),(4630,1400,336,20),(4642,1380,316,20),(4654,1360,296,20))
_COMMIT=((4676,436,35,19),(4692,416,56,20),(4708,396,78,20),(4726,376,100,20),
    (4742,356,122,20),(4760,336,144,20),(4776,316,166,20),(4794,296,188,20))


def _array_effects(objects,rows,parents,array,owner,frame,protected,sequence):
    """Actual fixed-array transport and per-slot last-write envelopes.

    This proves the copy/preservation relation. An actual selected ordinary
    path and each reached source/access use remain separate prerequisites;
    this helper does not issue an ordinary completed prefix or mutable owner.
    """
    from config_refill_children import opaque_copy_interface
    from config_decoded_widgets import _edge_at
    require(isinstance(owner,Storage) and owner.writable and owner.extent==14604 and owner.alignment>=4 and
        frame.extent==1608 and owner.phase==frame.phase and array['old_array'].start==owner.start+32 and
        array['sequence'] is not None and array['sequence'].count==Span(0,0) and len(array['packages'])==8,
        'Sources publication lacks the actual complete candidate array/owner geometry')
    V=parents[0]['function']-4376;phase=frame.phase;packages=array['packages'];before=sequence
    require(parents==_parents(objects,rows,V,owner.start),'Sources publication uses stale current parent effects')
    for index,(package,(site,output,tag)) in enumerate(zip(packages,SOURCE_EPOCHS)):
        require(package['index']==index and package['site']==V+site and package['sequence_before']==before and
            package['source']['before']==before.decoder and package['source']['tag_domain']==(0,1) and
            package['source']['after']==package['sequence_after'].decoder and
            package['source']['return_site']==V+site+4 and
            (package['output'].storage,package['output'].offset,package['output'].extent)==(frame,output,22) and
            package['captured_tag']==borrow(frame,tag,4,phase,True),'Sources publication changes paired Source order/transport')
        before=package['sequence_after']
    require(before==array['sequence'],'Sources publication lost its final paired cursor')
    guards=list(protected)
    guards.extend((_object(sequence.header,'complete Source Q'),_object(sequence.decoder.header,'live D'),
        _object(sequence.decoder.payload,'immutable Source input'),_object(borrow(frame,288,8,phase),'stable top Q')))
    # Preserve physical meaning even where the caller uses different storage
    # coordinate names. No cross-space name alone can avoid the child check.
    for g in tuple(guards):
        for space in ('address',frame.space):
            if g.space!=space and not any(v.space==space and v.start<=g.start and g.start+g.extent<=v.start+v.extent for v in guards):
                guards.append(Storage(space,'physical '+g.identity,g.start,g.extent,1,phase,False))
    guards=tuple(guards)
    tag_cells=tuple(_object(p['captured_tag'],'captured Poll tag '+str(i)) for i,p in enumerate(packages))
    old=array['old_array'];live=tuple(_object(borrow(frame,p['output'].offset,22,phase),'parsed Source '+str(i)) for i,p in enumerate(packages))
    chains=[]
    def transfer(spec,index,protect,child=False,destination_owner=None):
        site,source,destination,size=spec;target_owner=frame if destination_owner is None else destination_owner
        inp=borrow(frame,source,size,phase);out=borrow(target_owner,destination,size,phase,True)
        if child:
            edge=_edge_at(parents,V+site);target=edge[2]
            if edge[3].startswith('@outline:'):
                bound=source_children(objects,rows,parents)[target,edge[3]]
                require(len(bound['ordinary_edges'])==1 and bound['ordinary_edges'][0][3]=='__aeabi_memcpy',
                    'Sources copied tail loses its actual memcpy child')
                target=bound['ordinary_edges'][0][2]
            else:require(edge[3]=='__aeabi_memcpy','Sources copy borrows another current child')
            effect=opaque_copy_interface(objects,rows,target,inp,out,size,frame,protect)
        else:
            writes=((out,0,16),(out,16,2),(out,18,1)) if size==19 else ((out,0,20),)
            expected_sites=(site,site+4,site+8) if site==5768 else ((site,site+8,site+6) if site==4422 else (site,site+6,site+16))
            if size==20:expected_sites=(site,)
            require(all(V+at in rows and rows[V+at].mnemonic.startswith(('str','stm')) for at in expected_sites),
                'Sources inline transfer loses a complete current store')
            require_writes((out,),writes,protect+(_object(inp,'current copied Source payload'),))
            effect=dict(possible_writes=writes,opaque_payload_not_promoted=True)
        return dict(index=index,site=V+site,source=inp,destination=out,effect=effect)
    for stage,child in ((_CAPTURE,True),(_TRANSPOSE,False),(_CARRIER_COPY,False),(_FINAL_LOCAL,False)):
        destinations=[];stage_chain=[]
        for index,spec in enumerate(stage):
            proof=transfer(spec,index,guards+(old,)+tag_cells+live+tuple(destinations),child)
            stage_chain.append(proof);destinations.append(_object(proof['destination'],'transported Source '+str(index)))
        chains.append(tuple(stage_chain));live=tuple(destinations)
    # The byte2 split off the first Source is captured before its payload
    # staging. It remains paired with Source0; unlike a niche/tag it is not a
    # validity selector and can include inactive bits for some Poll variants.
    first_byte=borrow(frame,220,4,phase,True)
    require_writes((first_byte,),((first_byte,0,4),),guards+(old,)+tag_cells+live)
    fixed_before=Storage('address','Config before Source array',owner.start,32,4,phase,False)
    fixed_after=Storage('address','Config after Source array',owner.start+208,14604-208,4,phase,False)
    slots=[]
    for index,spec in enumerate(_COMMIT):
        offset=32+22*index
        prior=() if not index else (Storage('address','completed ordinary Source prefix',owner.start+32,22*index,2,phase,False),)
        suffix=() if index==7 else (Storage('address','unchanged old Source suffix',owner.start+offset+22,22*(7-index),2,phase,False),)
        protect=guards+tag_cells+live+(fixed_before,fixed_after)+prior+suffix
        tag=borrow(owner,offset,2,phase,True)
        require_writes((tag,),((tag,0,2),),protect)
        if index==0:
            split=borrow(owner,34,1,phase,True);require_writes((split,),((split,0,1),),protect)
        moved=transfer(spec,index,protect,index in (0,2,4,6),owner)
        site=V+spec[0];width=len(rows[site].raw)
        slots.append(dict(index=index,source_package=packages[index],tag_store=V+(4670 if not index else (4682,4700,4714,4734,4748,4768,4782)[index-1]),
            payload=moved,complete_after=site+width,prior_prefix_bytes=22*index,
            outstanding_after_tag=((offset+2,offset+22),),opaque_padding=(offset+21,offset+22)))
    return dict(transports=tuple(chains),transport_execution_order=tuple(tuple(sorted(V+v[0] for v in stage))
        for stage in (_CAPTURE,_TRANSPOSE,_CARRIER_COPY,_FINAL_LOCAL)),
        slots=tuple(slots),nonpadding_written_bytes=168,
        opaque_Config_padding=tuple((53+22*i,54+22*i) for i in range(8)),
        inactive_variant_payload_remains_opaque=True,complete_ordinary_Source_prefix_issued=False,
        requires_actual_ordinary_source_guards_and_reached_access_uses=True)
