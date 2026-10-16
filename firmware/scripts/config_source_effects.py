"""One source-typed ChannelSource aggregate, before parent publication.

Independent selectors remain finite guarded relations, not a Cartesian list
of complete Sources. Primitive transfers are fresh for the joined same-source
cursor. Current parent argument/owned-access use is still required separately.
"""
from dataclasses import replace
from config_abstract import Span,DecoderInterface,SequenceInterface,decoder_interface,sequence_interface
from config_source_children import source_coverage,source_children,source_return_profiles,CHANNEL_SOURCE
from config_widget_children import _name
from config_finite_returns import _closed_child_returns
from config_regions import Storage,View,borrow,require_disjoint,require_writes
from config_decode_effects import _object
from config_owner_entry import _direct_target
from config_headers import VISITOR
from validation import require
from config_source_contracts import GRAMMARS


SOURCE_EPOCHS=((4376,1128,284),(4906,516,276),(4944,628,272),(4982,744,268),
    (5154,880,264),(5254,940,260),(5292,1000,256),(5332,1064,252))
LOCAL_WRITES=((26,16,4),(88,28,8),(128,28,8),(184,28,8),(228,28,8),
    (256,12,4),(306,12,4),(410,0,4),(414,28,8),(474,26,2),(478,24,2),
    (506,8,4),(514,12,4),(522,8,4),(530,8,4),(534,4,4),(540,20,4),(640,0,4))
SUCCESS_WRITES=((696,20,1),(700,19,1),(704,18,1),(706,16,2),(708,11,1),
    (712,10,1),(714,2,1),(718,0,2),(720,12,4),(724,6,4),(730,4,2),(734,3,1))

# Exact authored Source-relative sites, with the local counted predecessor.
_ORIGINS={28:('definition','take_varint_u32',None),54:('access','take_varint_u32',None),
    92:('Access==1.pid','seq_u8',None),190:('Access==2.did','seq_u16',None),
    234:('Access==3.map_pid','seq_u8',None),270:('Access==3.ambient_pid','seq_u8',234),
    134:('Access==4.map_did','seq_u16',None),164:('Access==4.ambient_did','seq_u16',134),
    316:('decoder','take_varint_u32',None),338:('evidence','take_varint_u32',None),
    386:('route','take_varint_u32',None),420:('Route==1.request','seq_u16',None),
    448:('Route==1.response','seq_u16',420),544:('poll','take_varint_u32',None),
    582:('Poll==1.period','deserialize_u16',None),648:('freshness_ms.freshness_ms','seq_u16',None)}


def _source_error_origins(result):
    """Check retained conditional call/outcome identities; no active authority.

    Actual current parent argument, branch and original-return joins remain
    separate. Failed arm records retain references, not new evaluations.
    """
    origins=result['primitive_invocations'];steps=result['transitions']
    require(len({id(v) for v in origins})==len(origins),'Source error origin duplicates an invocation')
    retained=tuple(v for step in steps for v in step.get('primitive_invocations',()))
    require(len(retained)==len(origins) and all(a is b for a,b in zip(retained,origins)),
        'Source error origin leaves its ordered caller transition')
    for step in steps:
        local=step['primitive_invocations']
        if 'child' in step:
            require(len(local)==1 and local[0]['before'] is step['before'] and
                local[0]['result'] is step['child'] and local[0]['field']==step['field'],
                'Source error origin leaves its direct caller input/result')
        elif step['field']=='Poll payload':
            require(len(local)==1 and local[0]['before'] is step['before'] and
                local[0]['result'] is step['parsed_child'], 'Source error origin leaves its Poll caller input/result')
        else:
            choices=step.get('all_alternatives',(step,))
            for origin in local:
                matches=tuple(choice for choice in choices if
                    any(origin['result'] is child for child in choice['children']))
                require(len(matches)==1 and origin['arm_entry'] is matches[0]['before'],
                    'Source error origin leaves its retained same-arm caller result')
    for index,origin in enumerate(origins):
        spec=_ORIGINS.get(origin['site']);previous=origin['predecessor'];option=origin['predecessor_outcome']
        require(spec is not None and (origin['field'],origin['semantic_role'])==spec[:2] and
            origin['result']['semantic_role']==spec[1], 'Source error origin changes its call site/semantic')
        before=origin['before'];entry=origin['arm_entry']
        if spec[2] is None:
            require(previous is None and option is None and
                (before.decoder is entry if type(before) is SequenceInterface else before is entry),
                'Source error origin loses its initial caller input')
        else:
            require(any(previous is v for v in origins[:index]) and previous['site']==spec[2] and
                previous['arm_entry'] is entry and any(option is v for v in previous['result']['outcomes']) and
                option['kind']=='some' and before is option['state'],
                'Source error origin loses its same-arm predecessor outcome')
    for error in result['errors']:
        origin=error['origin'];option=error['child_outcome']
        require(any(origin is v for v in origins) and any(option is v for v in origin['result']['outcomes']) and
            error['cursor']==option['cursor'] and error['count_after']==result['count_after'] and
            error['tag']==65535 and error['stage']==origin['field']+('.invalid' if error['domain_error'] else ''),
            'Source error origin/outcome/cursor/count association differs')
        expected=14 if error['domain_error'] or option['kind']=='none' else option.get('error')
        require(error['error']==expected and
            (option['kind']=='success' if error['domain_error'] else option['kind'] not in ('success','some')),
            'Source error origin has another domain/error outcome')
    return origins


def _store_partition():
    """Every current Source store has one fixed owned-write classification."""
    stores={off for off,width,text in GRAMMARS['ChannelSource']['instructions']
        if text.split(' ',1)[0] in ('str','str.w','strh','strh.w','strb','strb.w','strd')}
    local={site for site,offset,size in LOCAL_WRITES}
    output={site for site,offset,size in SUCCESS_WRITES}|{356,360}
    require(not local&output and stores==local|output|{20},
        'Source store partition omits or double-classifies a current write')
    require({a for site,offset,size in SUCCESS_WRITES for a in range(offset,offset+size)}==set(range(21)),
        'Source successful output omits an active field or initializes padding')
    return dict(local=tuple(sorted(local)),output=tuple(sorted(output)),caller_count=(20,4,4))


def channel_source_interface(objects,rows,proof,state,output,frame,ancestor_protected):
    require(isinstance(proof,dict) and proof.get('role')==CHANNEL_SOURCE,
        'ChannelSource lacks its complete current body')
    current_proof=source_coverage(objects,_name(objects,CHANNEL_SOURCE,proof['function']),rows)
    require(proof==current_proof,'ChannelSource effects use stale current body/dispatch facts')
    partition=_store_partition()
    require(type(state) is SequenceInterface and isinstance(output,View) and output.writable and output.extent==22 and
        isinstance(frame,Storage) and frame.writable and frame.extent==72 and frame.alignment>=8 and
        state.header.storage.extent==1608 and state.header.offset==1500 and
        output.storage==state.header.storage and output.offset in tuple(v[1] for v in SOURCE_EPOCHS) and
        output.start%4==0 and frame.start+72==output.storage.start and
        frame.space==output.storage.space and frame.phase==output.storage.phase and
        0<=state.count.lower<=state.count.upper<=8,'ChannelSource has another H72/Q8/O22 caller ABI')
    decoder=state.decoder;visitor=output.storage;phase=frame.phase;start=proof['function']
    require(decoder.header.start==visitor.start+1608+236 and decoder.header.storage.phase==phase and
        decoder.header.storage.space==frame.space,'ChannelSource loses its same loader D')
    ancestors=tuple(ancestor_protected)
    # Preserve both coordinate meanings, retaining an existing containing
    # interval rather than adding a duplicate projection inside it.
    guards=list(ancestors)
    for g in ancestors:
        other='address' if g.space!='address' else frame.space
        if not any(v.space==other and v.start<=g.start and g.start+g.extent<=v.start+v.extent for v in guards):
            guards.append(Storage(other,'physical '+g.identity,g.start,g.extent,1,phase,False))
    guards=tuple(guards)
    require_disjoint((frame,)+guards)
    q=_object(state.header,'Source caller Q');d=_object(decoder.header,'Source D')
    out=_object(borrow(output.storage,output.offset,21,phase),'Source written envelope')
    require_disjoint((frame,q,d,out))
    saved=Storage(frame.space,'Source original saved cells',frame.start+36,36,4,phase,False)
    local=borrow(frame,0,36,phase,True)
    require_writes((local,),tuple((local,off,n) for site,off,n in LOCAL_WRITES),guards+(saved,q,d,out))
    require_writes((output,),tuple((output,off,n) for site,off,n in SUCCESS_WRITES),guards+(saved,q,d))
    require_writes((state.header,),((state.header,4,4),),guards+(saved,d,out))
    children=source_children(objects,rows,(proof,));edges={off:e for e in proof['ordinary_edges'] for off in (e[0],)}
    V=objects.elf.functions[_name(objects,VISITOR)][0]&~1
    epoch=next(v for v in SOURCE_EPOCHS if v[1]==output.offset);call=V+epoch[0];row=rows.get(call)
    require(row is not None and row.mnemonic=='bl' and _direct_target(row,'bl')==start and
        objects.read_only(call,len(row.raw))==row.raw,'Source original return has another actual caller')
    parent=dict(function=call,ordinary_edges=((0,'bl',start,CHANNEL_SOURCE,72),))
    returns=_closed_child_returns(rows,(parent,),{(start,CHANNEL_SOURCE):proof,**children},
        visitor.start,source_return_profiles(),(),objects=objects)
    require(returns and min(v['lowest_SP'] for v in returns)==visitor.start-120,
        'Source original-return or deepest child geometry differs')
    errors=[];steps=[];invocations=[];current=decoder;count=Span(max(1,state.count.lower)-1,state.count.upper-1) if state.count.upper else Span(0,0)
    def base(active=None):
        reserved=(_object(borrow(frame,0,16,phase),'Source captured fields'),
            _object(borrow(frame,24,4,phase),'Source Route byte pair'))
        if active is None or active.offset!=16:
            reserved+=(_object(borrow(frame,16,8,phase),'Source D/freshness header'),)
        return guards+(saved,q,out)+reserved
    def stable_writes(result,sequence=None):
        # Primitive interfaces reserve their complete D/Q headers while
        # establishing child frames. Check the exact cursor/count writes
        # against the stable cells after that interface has classified them.
        stable=(_object(borrow(decoder.header.storage,decoder.header.offset+4,4,phase),'stable D.end'),)
        if sequence is not None:
            stable+=(_object(borrow(sequence.header.storage,sequence.header.offset,4,phase),'stable Q.D'),)
        for option in result['outcomes']:
            outputs=tuple(dict.fromkeys(v for v,offset,size in option['possible_writes']))
            require_writes(outputs,option['possible_writes'],stable)
        return result
    def child(site,semantic):
        edge=edges[site];p=children[edge[2],edge[3]]
        if edge[3].startswith('@outline:'):
            require(p['frame']==0 and len(p['ordinary_edges'])==1 and p['ordinary_edges'][0][1]=='b.w',
                'Source primitive loses its zero-growth original-return transport')
            edge=p['ordinary_edges'][0];p=children[edge[2],edge[3]]
        require(p.get('semantic_role')==semantic,'Source field uses another current primitive')
        return p
    def invocation(label,site,before,result,entry,previous=None,previous_outcome=None):
        value=dict(field=label,site=site,semantic_role=result['semantic_role'],before=before,result=result,
            arm_entry=entry,predecessor=previous,predecessor_outcome=previous_outcome)
        invocations.append(value);return value
    def error(label,code,option,guard,origin,domain=False):
        errors.append(dict(stage=label,error=code,cursor=option['cursor'],count_after=count,tag=65535,
            source_guard=guard,origin=origin,child_outcome=option,domain_error=domain,
            possible_writes=((output,2,1),(output,0,2))))
    def nested(size,label):return Storage(frame.space,'Source '+label,frame.start-size,size,8,phase,True)
    def direct(label,site,maximum):
        nonlocal current
        if current is None:return
        before=current;p=child(site,'take_varint_u32')
        result=stable_writes(decoder_interface(objects,rows,p,before,nested(32,label),base(),output=borrow(frame,28,8,phase,True)))
        origin=invocation(label,site,before,result,before)
        success=[]
        for opt in result['outcomes']:
            if opt['kind']=='success':
                success.append(opt['state']);error(label+'.invalid',14,opt,'same decoded selector>'+str(maximum),origin,True)
            else:error(label,opt['error'],opt,'same '+label+' parse',origin)
        current=success[0] if success else None
        steps.append(dict(field=label,before=before,after=current,domain=(0,maximum),guard='same parsed enum in domain',child=result,
            primitive_invocations=(origin,)))
    def fields(label,before,items,header_offset=28):
        local_q=borrow(frame,header_offset,8,phase,True)
        seq=SequenceInterface(local_q,before,Span(len(items),len(items)));history=[];previous=None;previous_outcome=None
        for field,site,semantic in items:
            p=child(site,semantic)
            result=stable_writes(sequence_interface(objects,rows,p,seq,nested(p['frame'],field),base(local_q)),seq);history.append(result)
            origin=invocation(label+'.'+field,site,seq,result,before,previous,previous_outcome)
            successful=[]
            for opt in result['outcomes']:
                if opt['kind']=='some':successful.append(opt)
                else:error(label+'.'+field,14 if opt['kind']=='none' else opt['error'],opt,label,origin)
            if not successful:return None,tuple(history)
            require(len(successful)==1,'Source scalar has unpaired normal states')
            previous=origin;previous_outcome=successful[0];seq=previous_outcome['state']
        require(seq.count==Span(0,0),'Source payload omits a counted field')
        return seq.decoder,tuple(history)
    def join(label,before,alternatives,invocation_start,parsed_child=None):
        nonlocal current
        normals=tuple(v for v in alternatives if v['after'] is not None)
        current=None if not normals else replace(before,cursor=Span(
            min(v['after'].cursor.lower for v in normals),max(v['after'].cursor.upper for v in normals)))
        steps.append(dict(field=label,before=before,after=current,alternatives=normals,all_alternatives=tuple(alternatives),
            primitive_invocations=tuple(invocations[invocation_start:]),
            parsed_child=parsed_child,
            relation='same selected arm supplies its after-D; interval join retains selector/payload witnesses'))
        return normals
    none=None
    if state.count.lower==0:none=dict(tag=2,count_after=Span(0,0),cursor=decoder.cursor,possible_writes=((output,0,2),))
    if not state.count.upper:current=None
    direct('definition',28,15);direct('access',54,5);access=();route=();poll=()
    if current is not None:
        before=current;choices=[];invocation_start=len(invocations)
        for selector,items,payload in ((0,(),()),(1,(('pid',92,'seq_u8'),),(('pid','u8',(0,255)),)),
            (2,(('did',190,'seq_u16'),),(('did','u16',(0,65535)),)),
            (3,(('map_pid',234,'seq_u8'),('ambient_pid',270,'seq_u8')),(('map_pid','u8',(0,255)),('ambient_pid','u8',(0,255)))),
            (4,(('map_did',134,'seq_u16'),('ambient_did',164,'seq_u16')),(('map_did','u16',(0,65535)),('ambient_did','u16',(0,65535)))),(5,(),())):
            after,history=fields('Access=='+str(selector),before,items) if items else (before,())
            choices.append(dict(selector=selector,payload=payload,before=before,after=after,children=history))
        access=join('Access payload',before,choices,invocation_start)
    direct('decoder',316,11);direct('evidence',338,3);direct('route',386,2)
    if current is not None:
        before=current;choices=[];invocation_start=len(invocations)
        for selector in range(3):
            after,history=fields('Route==1',before,(('request',420,'seq_u16'),('response',448,'seq_u16'))) if selector==1 else (before,())
            choices.append(dict(selector=selector,before=before,after=after,children=history,
                payload=(('request','u16',(0,65535)),('response','u16',(0,65535))) if selector==1 else (),
                packed_word='request | (response <<16)' if selector==1 else None,
                local_read=(frame.start+25,frame.start+27) if selector==1 else None,inactive_payload_opaque=selector!=1))
        route=join('Route payload',before,choices,invocation_start)
    direct('poll',544,1)
    if current is not None:
        before=current;p=child(582,'deserialize_u16');invocation_start=len(invocations)
        result=stable_writes(decoder_interface(objects,rows,p,before,nested(24,'Poll EveryMs'),base()));choices=[dict(selector=0,payload=(),before=before,after=before,children=())]
        origin=invocation('Poll==1.period',582,before,result,before)
        for opt in result['outcomes']:
            if opt['kind']=='success':choices.append(dict(selector=1,payload=(('period','u16',(0,65535)),),before=before,after=opt['state'],children=(result,)))
            else:error('Poll==1.period',opt['error'],opt,'same Poll EveryMs direct u16',origin)
        poll=join('Poll payload',before,choices,invocation_start,result)
    if current is not None:
        before=current;invocation_start=len(invocations);current,history=fields('freshness_ms',before,(('freshness_ms',648,'seq_u16'),),16)
        steps.append(dict(field='freshness_ms',before=before,after=current,domain=(0,65535),children=history,
            primitive_invocations=tuple(invocations[invocation_start:])))
    normal=None if current is None else dict(source_order=('definition','access','decoder','evidence','route','poll','freshness_ms'),
        before=decoder,after=current,count_after=count,definition_domain=(0,15),access=access,decoder_domain=(0,11),
        evidence_domain=(0,3),route=route,poll=poll,freshness_domain=(0,65535),transitions=tuple(steps),
        output=output,tag_domain=(0,1),return_site=call+4)
    result=dict(proof=proof,normal=normal,none=none,errors=tuple(errors),returns=returns,store_partition=partition,physical_child_bytes=120,
        primitive_invocations=tuple(invocations),count_after=count,
        transitions=tuple(steps),guarded_access=access,guarded_routes=route,guarded_poll=poll,
        output_padding=(21,22),pending_after_early_tag=((12,16),(6,10),(4,6),(3,4)),
        last_output_store=start+734,completed_Source_issued=False,semantic_Config_validated=False,
        pending_owned_roles=('source_route_local','source_route_output'),requires_actual_parent_source_and_guarded_access_use=True)
    _source_error_origins(result)
    return result
