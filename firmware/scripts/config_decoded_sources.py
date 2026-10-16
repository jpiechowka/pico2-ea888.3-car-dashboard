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
from dataclasses import dataclass,fields,is_dataclass,replace
from types import MappingProxyType
from config_abstract import SequenceInterface,Span
from config_regions import Storage,borrow,require_writes,require_disjoint
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
    loader=sequence.decoder.header.storage
    require(loader.extent==456 and sequence.decoder.header.offset==236 and loader.start==frame.start+1608,
        'Sources array loses its actual loader/Visitor/D geometry')
    # The old Source subarray is not the enclosing private allocation. Check
    # its full physical extent against the retained caller and deeper Source
    # chain; coordinate labels and an H72-only check cannot protect the suffix.
    config=Storage('address','complete Config reserved while parsing Sources',old_array.start-32,14604,4,phase,False)
    activation=Storage('address','Source deepest child and retained callers',frame.start-120,120+1608+456,8,phase,False)
    require_disjoint((activation,config))
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
    packages=[];errors=[];live=[];current=sequence;invocations=[]
    child_frame=Storage(frame.space,'actual Sources child',frame.start-72,72,8,phase,True)
    for index,((site,proof),(offset,out,tag)) in enumerate(zip(source_edges,SOURCE_EPOCHS)):
        require(site==V+offset and current.count==Span(8-index,8-index),
            'Sources call consumes another predecessor/count epoch')
        output=borrow(frame,out,22,phase,True)
        result=channel_source_interface(objects,rows,proof,current,output,child_frame,guard+tuple(live))
        invocations.append(dict(index=index,site=site,before=current,result=result))
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
    return dict(packages=tuple(packages),invocations=tuple(invocations),errors=tuple(errors),sequence=current,old_array=old_array,
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


def _array_effects(objects,rows,parents,array,owner,frame,protected,sequence,_descriptions=None):
    """Actual fixed-array transport and per-slot last-write envelopes.

    This proves the copy/preservation relation. An actual selected ordinary
    path and each reached source/access use remain separate prerequisites;
    this helper does not issue an ordinary completed prefix or mutable owner.
    """
    from config_refill_children import opaque_copy_interface
    from config_decoded_widgets import _edge_at
    from config_source_sharing import _CopyDescriptions
    if _descriptions is None:_descriptions=_CopyDescriptions(objects,rows)
    require(type(_descriptions) is _CopyDescriptions,'Sources copy description pool has another type')
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
            effect=_descriptions.checked(effect,objects,rows,target)
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


def _remaining_aggregates(objects,rows,parents,sequence,owner,protected):
    """Fresh source-typed Warning/Fuel/colors after the fixed Source array.

    The folded physical count store differs from conceptual field order.
    Keep each Fuel arm's paired decoder and emitted niche; do not multiply
    independent enum alternatives into complete Config histories.
    """
    from config_abstract import decoder_interface,sequence_interface
    from config_decoded_widgets import _at_child,_input_policy
    from config_widget_children import warning_colors_interface
    from config_widget_contracts import ROLES as WIDGET
    from config_source_handoff import _CARRIERS
    require(type(sequence) is SequenceInterface and sequence.count==Span(7,7) and
        sequence.header.offset==288 and sequence.header.storage.extent==1608 and
        owner.space=='address' and owner.extent==14604 and owner.writable and
        owner.phase==sequence.header.storage.phase,'Sources suffix has another actual top7/Config epoch')
    frame=sequence.header.storage;phase=frame.phase;V=parents[0]['function']-4376
    require(parents==_parents(objects,rows,V,owner.start),'Sources suffix uses stale parent/source effects')
    children=source_children(objects,rows,parents);errors=[];steps=[]
    # The actual predecessor already retains these same cell borrows. Keep
    # that original protection identity instead of inventing overlapping
    # allocations for the same caller bytes; supplied adapters add the cells.
    inherited=list(protected);carrier_guards=[]
    for cell,target,sites in _CARRIERS:
        original=next((v for v in inherited if v.space==frame.space and v.start==frame.start+cell and v.extent==4),None)
        if original is None:
            original=_object(borrow(frame,cell,4,phase),'reserved Source pointer '+str(cell));inherited.append(original)
        carrier_guards.append(original)
    inherited=tuple(inherited)
    guards=list(inherited)
    for g in inherited:
        for space in ('address',frame.space):
            if g.space!=space and not any(v.space==space and v.start<=g.start and g.start+g.extent<=v.start+v.extent for v in guards):
                guards.append(Storage(space,'physical '+g.identity,g.start,g.extent,1,phase,False))
    guards=tuple(guards)
    def base(active=None):
        return guards+(() if active==sequence.header else (_object(sequence.header,'stable physical top Q'),))
    def nested(proof,label):return Storage(frame.space,'Sources '+label,frame.start-proof['frame'],proof['frame'],8,phase,True)
    def warning_transport(stage,kind):
        # Exact current route anchors, including the packed intermediate r5.
        # Frontier56 copies the complete packed r5 into r0. Its low byte is
        # the conceptual error; narrowing by the later returned-error ABI
        # is separate. Neither whole register is inferred to be scalar14.
        require(stage in ('Warning scope','Warning mode','Warning page') and
            kind in ('domain','none','error'),'unreviewed packed Warning error route')
        if kind in ('domain','none'):prefix=(6446,6448)
        elif stage=='Warning page':prefix=(5210,)
        else:prefix=((4822 if stage=='Warning scope' else 4858),4862)
        return dict(kind='packed_Warning_lowbyte',
            path=tuple(V+off for off in prefix+(4866,4870,4872,4886,4888,4890,5024,5032,56)),
            mask=0x020000ff,whole_r5_is_scalar_error=False)
    def error(stage,option,completed,source_invocation=None):
        record=dict(stage=stage,error=14 if option['kind']=='none' else option['error'],
            cursor=option['cursor'],completed_aggregates=completed,frontier=V+56,
            partial_current_aggregate_not_promoted=True,child_outcome=option)
        if stage in ('Warning scope','Warning mode','Warning page'):
            record['error_transport']=warning_transport(stage,'none' if option['kind']=='none' else 'error')
        if source_invocation is not None:
            record.update(source_invocation=source_invocation,child_outcome=option)
        errors.append(record)
    def stable(result,state=None):
        fixed=(_object(borrow(sequence.decoder.header.storage,sequence.decoder.header.offset+4,4,phase),'stable D.end'),)
        if state is not None:fixed+=(_object(borrow(state.header.storage,state.header.offset,4,phase),'stable Q.D'),)
        for option in result['outcomes']:
            available=tuple(dict.fromkeys(v for v,offset,size in option['possible_writes']))
            require_writes(available,option['possible_writes'],fixed)
        return result
    def direct(label,site,decoder,maximum,completed):
        if decoder is None:return None
        proof=_at_child(parents,children,V+site,'take_varint_u32')
        result=stable(decoder_interface(objects,rows,proof,decoder,nested(proof,label),base(),output=borrow(frame,1064,8,phase,True)))
        normal=[]
        for option in result['outcomes']:
            if option['kind']=='success':
                normal.append(option['state']);record=dict(stage=label+'.domain',error=14,cursor=option['cursor'],
                    completed_aggregates=completed,frontier=V+(56 if label.startswith('Warning ') else 170),
                    source_guard='same parsed selector>'+str(maximum),child_outcome=option)
                if label in ('Warning scope','Warning mode'):record['error_transport']=warning_transport(label,'domain')
                errors.append(record)
            else:error(label,option,completed)
        after=normal[0] if normal else None
        steps.append(dict(field=label,site=V+site,before=decoder,after=after,domain=(0,maximum),child=result,
            guard='same parsed selector in source domain'))
        return after
    def scalar(label,site,state,semantic,completed):
        proof=_at_child(parents,children,V+site,semantic);kwargs={}
        if semantic=='seq_f32':kwargs=dict(output=borrow(frame,1064,8,phase,True),platform_policy=_input_policy(objects,state.decoder))
        result=stable(sequence_interface(objects,rows,proof,state,nested(proof,label),base(state.header),**kwargs),state)
        normal=[]
        for option in result['outcomes']:
            if option['kind']=='some':normal.append(option['state'])
            else:error(label,option,completed)
        require(len(normal)<=1,'Sources scalar has an unpaired normal cursor')
        steps.append(dict(field=label,site=V+site,before=state,after=normal[0] if normal else None,child=result))
        return normal[0] if normal else None
    def assignment(writes):
        available=tuple(dict.fromkeys(v for v,offset,size in writes))
        require_writes(available,writes,guards+(_object(sequence.header,'live top Q'),
            _object(sequence.decoder.header,'live D'),_object(sequence.decoder.payload,'immutable source')))
    completed=('Sources',);current=direct('Warning scope',4814,sequence.decoder,1,completed)
    current=direct('Warning mode',4850,current,1,completed)
    if current is not None:
        page=scalar('Warning page',5190,SequenceInterface(borrow(frame,1000,8,phase,True),current,Span(1,1)),'seq_u16',completed)
        current=None if page is None else page.decoder
    warning=None
    if current is not None:
        assignment(((borrow(frame,1000,8,phase,True),0,8),(borrow(frame,1064,3,phase,True),0,3),
            (borrow(owner,208,4,phase,True),0,4)))
        warning=dict(before=sequence.decoder,after=current,scope_domain=(0,1),mode_domain=(0,1),page_domain=(0,65535),
            packed_word='page_ms | (scope <<16) | (mode <<24)',scratch_read=(frame.start+1065,frame.start+1067),
            complete_store=V+5040,physical_top_before_fold=7,logical_remaining=6,
            packed_error_tag=0x02000000,packed_error_lowbyte_only=True)
    completed=('Sources','WarningPresentation')
    if current is not None:
        # The actual folded count5 store precedes the Fuel varint, including
        # its failures. Preserve Q.D while changing only this owned count.
        require_writes((sequence.header,),((sequence.header,4,4),),guards+
            (_object(borrow(frame,288,4,phase),'stable top Q.D'),
             _object(sequence.decoder.header,'live D'),_object(sequence.decoder.payload,'immutable source')))
    fuel_start=direct('Fuel',5064,current,2,completed)
    fuel=[]
    if fuel_start is not None:
        fuel.append((0,2,fuel_start,(),None))
        ratio=scalar('Stoich ratio',5110,SequenceInterface(borrow(frame,1000,8,phase,True),fuel_start,Span(1,1)),'seq_f32',completed)
        if ratio is not None:fuel.append((1,3,ratio.decoder,(('ratio','f32_bits',(0,0xffffffff)),),None))
        percent=scalar('Blend percent',5744,SequenceInterface(borrow(frame,1000,8,phase,True),fuel_start,Span(8,8)),'seq_f32',completed)
        blend=None if percent is None else direct('Blend basis',5890,percent.decoder,1,completed)
        blend=direct('Blend origin',5988,blend,1,completed)
        blend=direct('Blend density',6236,blend,1,completed)
        if blend is not None:
            density=[(0,blend)]
            proof=_at_child(parents,children,V+6266,'deserialize_seed_f32')
            extra=stable(decoder_interface(objects,rows,proof,blend,nested(proof,'density temperature'),base(),
                output=borrow(frame,1064,8,phase,True),platform_policy=_input_policy(objects,blend)))
            temperature=[]
            for option in extra['outcomes']:
                if option['kind']=='success':temperature.append(option['state']);density.append((1,option['state']))
                else:error('Blend density temperature',option,completed)
            require(len(temperature)<=1,'Sources density temperature has an unpaired normal decoder')
            steps.append(dict(field='Blend density temperature',site=V+6266,before=blend,
                after=temperature[0] if temperature else None,child=extra,source_guard=(2,1),
                sequence_count_before=Span(4,4),sequence_count_after=Span(4,4)))
            for tag,after in density:
                q=SequenceInterface(borrow(frame,1000,8,phase,True),after,Span(4,4))
                for field,site in (('gasoline_stoich',6288),('ethanol_stoich',6320),('gasoline_density',6352),('ethanol_density',6384)):
                    q=scalar('Blend '+field,site,q,'seq_f32',completed)
                    if q is None:break
                if q is not None:
                    require(q.count==Span(0,0),'Blend omitted a counted field')
                    fields=tuple((name,'f32_bits',(0,0xffffffff)) for name in
                        ('percent','gasoline_stoich','ethanol_stoich','gasoline_density','ethanol_density'))
                    if tag:fields+=(('temperature','f32_bits',(0,0xffffffff)),)
                    fuel.append((2,tag,q.decoder,fields,tag))
    normals=[];fuel_assignments=[];colors_invocations=[]
    for wire,tag,decoder,fields,density in fuel:
        writes=((0,4),(29,1),(7,1),(28,1),(8,4),(12,4),(16,4),(20,4),(24,4),(4,1),(5,2))
        possible=tuple((borrow(owner,0,32,phase,True),offset,size) for offset,size in writes)
        assignment(possible)
        record=dict(wire_tag=wire,emitted_tag=tag,active_fields=fields,padding=(30,32),
            complete_store=V+5570,early_tag_store=V+5568,pending_after_tag=((5,7),),
            inactive_payload_opaque=True,density_newtype_count_decrement=0,
            remaining_sequence_counts=(4,3,2,1,0) if wire==2 else (),folded_top_count=5)
        # The actual Fuel write precedes the global Colors call. Preserve its
        # paired input/variant/footprint even when that later call fails.
        fuel_assignments.append(dict(Fuel=record,before=fuel_start,after=decoder,
            source_guard=(wire,density),possible_writes=possible))
        from config_decoded_widgets import _edge_at
        edge=_edge_at(parents,V+5578)
        require(edge[3]==WIDGET['warning_colors'],'global WarningColors uses another current call')
        proof=children[edge[2],edge[3]]
        colors_before=replace(sequence,decoder=decoder,count=Span(5,5))
        colors=warning_colors_interface(objects,rows,proof,colors_before,
            borrow(frame,1064,6,phase,True),nested(proof,'global WarningColors'),base(sequence.header))
        stable(colors,sequence)
        invocation=dict(site=V+5578,before=colors_before,child=colors,Fuel_assignment=fuel_assignments[-1])
        colors_invocations.append(invocation)
        for option in colors['outcomes']:
            if option['kind']!='some':error('global WarningColors',option,completed+('Fuel',),invocation);continue
            assignment(((borrow(owner,14524,4,phase,True),2,2),(borrow(owner,14524,4,phase,True),0,2)))
            normals.append(dict(wire_tag=wire,emitted_tag=tag,Fuel=record,after=option['state'],
                colors=dict(source=colors,invocation=invocation,partial_background_store=V+5618,complete_store=V+5628,
                    same_source_order=('foreground','background'))))
    return dict(warning=warning,Fuel_assignments=tuple(fuel_assignments),Colors_invocations=tuple(colors_invocations),normal=tuple(normals),errors=tuple(errors),transitions=tuple(steps),carrier_guards=tuple(carrier_guards),
        before=sequence,complete_ordinary_group_issued=False,
        pending_owned_roles=('warning_presentation_local','Fuel_final_halfword'),
        requires_actual_Source_prefix_ordinary_control_and_reached_uses=True)


@dataclass(frozen=True)
class _SourcePhase:
    entry:object
    array:dict
    effects:object
    remaining:object
    parent_envelopes:dict


@dataclass(frozen=True)
class _SourcePhaseData:
    """Snapshot only local records, without copying the predecessor bridge."""
    entry:dict
    array:dict
    effects:object
    remaining:object
    parent_envelopes:dict


def _entry_data(entry,widgets):
    require(entry.predecessor is widgets and any(entry.exit is v for v in widgets.exits),
        'Sources private phase lost its actual predecessor/exit identity')
    return dict(exit_index=next(i for i,v in enumerate(widgets.exits) if v is entry.exit),
        sequence=entry.sequence,carriers=entry.carriers,protected=entry.protected,
        saved_top_count=entry.saved_top_count,
        old_array=entry.old_Source_array,call_site=entry.call_site,call_arguments=entry.call_arguments)


def _phase_checkpoint(widgets,entries,parents,children,returns,phases,payload_sha256,structural):
    from config_source_control import _structural_data
    require(len(entries)==len(phases) and all(p.entry is e for p,e in zip(phases,entries)),
        'Sources private phase changed its selected entry association')
    data=tuple(_SourcePhaseData(_entry_data(p.entry,widgets),p.array,p.effects,p.remaining,p.parent_envelopes) for p in phases)
    return _phase_graph(dict(parents=parents,children=children,returns=returns,phases=data,payload_sha256=payload_sha256,
        structural=_structural_data(structural,widgets,parents,children)))


def _phase_graph(value):
    """Independent immutable values and alias edges for one checked invocation.

    Array/effect records intentionally retain the same Source package. A tree
    copy expands that package at every transport occurrence. This snapshot
    instead records every supported container once and its exact references;
    it retains neither mutable input objects nor cached evaluated states.
    Rebuilding it checks values, types and shared associations again. Scalar
    identity is deliberately absent from this encoding: identical typed
    literal tokens share immutable storage only within this snapshot call.
    """
    nodes=[];memo={};active=set();literals={}
    def visit(item):
        kind=type(item)
        if kind in (str,bytes,int,bool,type(None)):
            key=(kind,item)
            if key not in literals:literals[key]=('value',kind.__name__,item)
            return literals[key]
        identity=id(item)
        require(identity not in active,'cyclic Sources phase snapshot')
        if identity in memo:return ('reference',memo[identity])
        index=len(nodes);memo[identity]=index;nodes.append(None);active.add(identity)
        if kind in (dict,MappingProxyType):
            node=('mapping',tuple((visit(k),visit(v)) for k,v in item.items()))
        elif kind in (tuple,list):
            node=(kind.__name__,tuple(visit(v) for v in item))
        elif is_dataclass(item) and not isinstance(item,type):
            node=('dataclass',kind.__module__,kind.__qualname__,tuple((f.name,visit(getattr(item,f.name))) for f in fields(item)))
        else:require(False,'unsupported Sources phase snapshot value')
        active.remove(identity);nodes[index]=node
        return ('reference',index)
    root=visit(value)
    return root,tuple(nodes)


@dataclass(frozen=True)
class _SourcePhasesState:
    widgets:object
    objects:object
    rows:object
    source_context:object
    parents:tuple
    children:dict
    returns:tuple
    structural:object
    entries:tuple
    phases:tuple
    checkpoint:object
    payload_sha256:str
    complete_ordinary_group_issued:bool=False
    ordinary_frontier_pending:int=5630

    def __post_init__(self):
        require(self.complete_ordinary_group_issued is False and type(self.ordinary_frontier_pending) is int and
            self.ordinary_frontier_pending==5630,'Sources private phase cannot mint ordinary completion')

    def current(self):
        from config_source_handoff import _source_entries
        from config_structural import _readonly_hash
        bridge=self.widgets.family_bridge.current()
        require(self.objects is bridge.objects and self.rows is bridge.rows and self.source_context is bridge.source_context,
            'Sources private phase uses another current input/source pair')
        c=self.widgets.views.context;V=c.headers['visitor']['proof']['function']
        require(self.parents==_parents(self.objects,self.rows,V,c.owner.start) and
            self.children==source_children(self.objects,self.rows,self.parents),
            'Sources private current child/body/effect family changed')
        from config_source_control import _source_structural_control,_structural_data
        fresh=_source_structural_control(self.objects,self.rows,self.widgets,self.parents,self.children)
        require(_structural_data(fresh,self.widgets,self.parents,self.children)==
            _structural_data(self.structural,self.widgets,self.parents,self.children),
            'Sources private current selected/foreign/computed/return structure changed')
        entries=_source_entries(self.objects,self.rows,self.widgets)
        require(tuple(_entry_data(e,self.widgets) for e in entries)==tuple(_entry_data(e,self.widgets) for e in self.entries),
            'Sources private current entry/source/commit association changed')
        payload=c.headers['visitor']['sequence_state'].decoder.payload
        require(_readonly_hash(self.objects,payload.start,payload.extent)==self.payload_sha256,
            'Sources private consumed source bytes changed')
        require(_phase_checkpoint(self.widgets,self.entries,self.parents,self.children,self.returns,self.phases,self.payload_sha256,self.structural)==self.checkpoint,
            'Sources private source/epoch/child/effect record changed')
        return self


def _first_boot_source_phases(objects,rows,source_context):
    """Derive the actual private predecessor once; ordinary closure is pending.

    This is deliberately not the public Sources boundary. No descriptor,
    receipt, copied previous report or caller-provided state enters the chain.
    """
    from config_decoded_widgets import _first_boot_decoded_widgets_state
    from config_source_handoff import _source_entries
    from config_source_control import _source_structural_control
    from config_source_parent_effects import _parent_envelopes
    from config_structural import _readonly_hash
    _,widgets=_first_boot_decoded_widgets_state(objects,rows,source_context)
    objects=widgets.family_bridge.objects;rows=widgets.family_bridge.rows
    c=widgets.views.context;frame=c.headers['visitor_frame'];V=c.headers['visitor']['proof']['function']
    parents=_parents(objects,rows,V,c.owner.start);children=source_children(objects,rows,parents)
    structural=_source_structural_control(objects,rows,widgets,parents,children);returns=structural.Source_returns
    require(returns and min(v['lowest_SP'] for v in returns)==frame.start-120,
        'Sources whole current child chain changes its physical/original-return bound')
    from config_source_sharing import _CopyDescriptions
    entries=_source_entries(objects,rows,widgets);phases=[];descriptions=_CopyDescriptions(objects,rows)
    for entry in entries:
        envelopes=_parent_envelopes(objects,rows,parents,entry.exit.sequence,c.owner,entry.protected)
        array=_source_array(objects,rows,parents,entry.sequence,entry.protected,_object(entry.old_Source_array,'actual old Source array'))
        effects=None;remaining=None
        if array['sequence'] is not None:
            effects=_array_effects(objects,rows,parents,array,c.owner,frame,entry.protected,entry.sequence,descriptions)
            # Whole Source array is now a protection envelope only after its
            # checked copy/last-store relation. Ordinary promotion is still
            # pending the separate actual selected-control/access-use proof.
            completed=_object(borrow(c.owner,32,176,frame.phase),'assigned Source array')
            top=replace(entry.exit.sequence,decoder=array['sequence'].decoder)
            remaining=_remaining_aggregates(objects,rows,parents,top,c.owner,entry.protected+(completed,))
        phases.append(_SourcePhase(entry,array,effects,remaining,envelopes))
    payload=c.headers['visitor']['sequence_state'].decoder.payload
    digest=_readonly_hash(objects,payload.start,payload.extent);phases=tuple(phases)
    checkpoint=_phase_checkpoint(widgets,entries,parents,children,returns,phases,digest,structural)
    return _SourcePhasesState(widgets,objects,rows,widgets.family_bridge.source_context,parents,children,returns,structural,entries,phases,checkpoint,digest)
