"""Conditional Source publication lineage on the authored normal parent path.

This checks the package/argument/epoch relation behind the physical envelopes.
The actual private producer must additionally bind its entry, active source
guards and reached accesses. No supplied record creates an ordinary owner.
"""
from config_abstract import Span
from config_regions import Storage,borrow,require_disjoint,require_writes
from config_decode_effects import _object
from config_source_effects import SOURCE_EPOCHS
from config_source_parent_effects import PARENT_WRITES,POINTER_EPOCHS
from validation import require


# Fixed selected normal segments. The inherited entry and all child bodies
# have their own complete checked control relations. These segments retain
# the real noncontiguous parent order, rather than sorting instruction PCs.
_SEGMENTS=((4376,4394),(4892,4922),(4930,4960),(4968,4998),
    (5140,5170),(5240,5270),(5278,5308),(5316,5348),
    (5356,5518),(5762,5856),(4410,4534),(4534,4796))
_SOURCE_BRANCHES=((4390,4892,4896),(4920,4930,4934),(4958,4968,4972),
    (4996,5140,5144),(5168,5240,5244),(5268,5278,5282),
    (5306,5316,5320),(5346,5356,5360))
_CAPTURE_ORDER=(7,0,1,2,3,4,5,6)
_TAG_STORES=(4670,4682,4700,4714,4734,4748,4768,4782)


def _publication_lineage(objects,rows,parents,array,effects,frame,carriers,returns,envelopes):
    """Check one supplied conditional array; actual entry authority is separate.

    No data from a support file is loaded here. The closed current literals,
    actual child returns, eight source packages and exact effects are the
    inputs. In the real producer these all belong to the same private phase.
    """
    from config_decoded_sources import _parents,_CAPTURE,_TRANSPOSE,_CARRIER_COPY,_FINAL_LOCAL,_COMMIT
    from config_source_contracts import GRAMMARS
    V=parents[0]['function']-4376;B=array['old_array'].start-32
    require(parents==_parents(objects,rows,V,B) and frame.extent==1608 and
        array['sequence'] is not None and array['sequence'].count==Span(0,0) and
        len(array['packages'])==8 and effects is not None and envelopes['stores']==80,
        'Sources chronological publication lacks its current complete conditional array')
    packages=array['packages'];before=packages[0]['sequence_before'];call_records=[]
    high=tuple('r'+str(i) for i in range(4,12))
    call_map={site:(target,role,mask) for site,target,role,mask in returns['parent_calls']}
    require(len(call_map)==57 and all(mask==high for target,role,mask in call_map.values()),
        'Sources chronological carriers lose their actual child transfer')
    # The existing full literals already match every selected row and its
    # encoding. Keep the complete ordered parent instruction membership.
    selected={p['function']-V+off for p in parents
        for off,width,text in GRAMMARS['visitor_'+str(p['function']-V)]['instructions']}
    path=tuple(off for lo,hi in _SEGMENTS for off in sorted(selected) if lo<=off<hi)
    require(len(path)==len(set(path)) and path[0]==4376 and path[-1]==4794,
        'Sources chronological normal path changes its exact membership')
    positions={off:i for i,off in enumerate(path)}
    guarded={a:b for a,b,c in _SOURCE_BRANCHES}
    guarded.update({5516:5762,5854:4410})
    no_error={c for a,b,c in _SOURCE_BRANCHES}|{4530}
    for i,off in enumerate(path):
        row=rows[V+off];following=off+len(row.raw);next_pc=path[i+1] if i+1<len(path) else 4796
        op=row.mnemonic.split('.',1)[0]
        if op=='bl':
            require(V+off in call_map and next_pc==following,
                'Sources chronology bypasses an actual original child return')
        elif off in guarded:
            require(op in ('b','bne') and next_pc==guarded[off] and
                row.operands.split(' ',1)[0]==hex(V+guarded[off]),
                'Sources chronological normal guard/target differs')
        elif off in no_error:
            require(op=='beq' and row.operands.split(' ',1)[0]==hex(V+170) and next_pc==following,
                'Sources chronological None arm changes its actual stopped frontier')
        else:
            require(op not in ('b','beq','bne','bx','tbb','tbh','cbz','cbnz') and next_pc==following,
                'Sources chronological segment adds an unpaired parent successor')
    for index,(package,(call,out,tag),(branch,target,none)) in enumerate(zip(packages,SOURCE_EPOCHS,_SOURCE_BRANCHES)):
        result=array['invocations'][index]['result'];normal=result['normal']
        require(package['source'] is normal and normal is not None and normal['tag_domain']==(0,1) and
            normal['before']==before.decoder and package['sequence_before']==before and
            package['sequence_after'].count==Span(7-index,7-index) and
            package['sequence_after'].decoder==normal['after'] and normal['return_site']==V+call+4 and
            (package['output'].storage,package['output'].offset,package['output'].extent)==(frame,out,22) and
            package['captured_tag'].start==frame.start+tag and
            call_map[V+call][:2]==(result['proof']['function'],result['proof']['role']),
            'Sources chronology mixes a returned package/tag/source epoch')
        # The two parent predicates are justified by this actual child0/1
        # domain, not by assuming any value except ffff and2 must be valid.
        require(positions[call]<positions[branch]<positions[target]<=positions[none],
            'Sources parent tests precede their same-call tag capture')
        call_records.append(dict(index=index,package=package,call=V+call,
            returned_tag_domain=normal['tag_domain'],nonerror_edge=(V+branch,V+target),
            absent_None_edge=(V+none,V+170)))
        before=package['sequence_after']
    require(before==array['sequence'],'Sources chronology changes its final same-Q cursor')
    # Only these exact six writes can initialize/recycle the four scratch
    # pointer cells. Every other parent store, including later alternatives,
    # preserves them. The inherited cells retain their earlier checked epoch.
    expected_carriers=((216,1001),(224,688),(228,576),(232,1420),(236,1380),
        (240,1400),(244,1440),(248,1360))
    require(tuple((v.start-frame.start,target-frame.start) for v,target in carriers)==expected_carriers,
        'Sources chronology changed an inherited pointer producer/epoch')
    scratch=tuple(_object(borrow(frame,off,4,frame.phase),'Source chronological pointer '+str(off))
        for off in (200,204,208,212))
    stable=tuple(_object(v,'inherited Source pointer') for v,target in carriers)
    creators={at:cell for at,cell,target,read in POINTER_EPOCHS}
    for site,coordinate,offset,size,category in PARENT_WRITES:
        if coordinate!='V':continue
        view=borrow(frame,offset,size,frame.phase,True)
        relevant=stable+tuple(v for v in scratch if site not in creators or v.start!=frame.start+creators[site])
        require_writes((view,),((view,0,size),),relevant)
    for at,cell,target,read in POINTER_EPOCHS:
        require(positions[at]<positions[read] and (cell!=204 or at!=5468 or positions[5458]<positions[at]) and
            (cell!=208 or at!=5480 or positions[5470]<positions[at]),
            'Sources pointer cell is reused before its last read/copy return')
    # Every actual copy after pointer creation has a disjoint destination and
    # restored-tail child frame. Recheck these against the pointer cells which
    # are initialized here, rather than relying on a previous phase's guards.
    child=Storage(frame.space,'Sources physical copy descendants',frame.start-72,72,8,frame.phase,False)
    for guard in scratch+stable:require_disjoint((child,guard))
    chains=effects['transports'];require(len(chains)==4,'Sources chronology omits a staging phase')
    specs=(_CAPTURE,_TRANSPOSE,_CARRIER_COPY,_FINAL_LOCAL);lineages=[]
    for index,package in enumerate(packages):
        previous=borrow(frame,package['output'].offset+(3 if index==0 else 2),19 if index==0 else 20,frame.phase)
        stages=[]
        for stage,(chain,definitions) in enumerate(zip(chains,specs)):
            event=chain[index];site,origin,destination,count=definitions[index]
            require(event['index']==index and event['site']==V+site and event['source']==previous and
                event['source']==borrow(frame,origin,count,frame.phase) and
                event['destination']==borrow(frame,destination,count,frame.phase,True),
                'Sources chronological transport loses its same-package source/destination')
            if stage==0:
                require(V+site in call_map and event['effect']['physical_activation_paths']==((8,),(72,)) and
                    event['effect']['initialized_semantic_bytes_not_inferred'] is True,
                    'Sources chronological copy loses its restored original-return closure')
            require_writes((event['destination'],),event['effect']['possible_writes'],scratch+stable)
            stages.append((V+site,event['source'],event['destination']))
            previous=borrow(frame,destination,count,frame.phase)
        slot=effects['slots'][index];site,origin,destination,count=_COMMIT[index]
        require(slot['source_package'] is package and slot['index']==index and slot['tag_store']==V+_TAG_STORES[index] and
            slot['payload']['source']==previous and slot['payload']['site']==V+site and
            slot['payload']['destination'].start==B+destination and slot['payload']['destination'].extent==count and
            slot['complete_after']==V+site+len(rows[V+site].raw) and slot['prior_prefix_bytes']==22*index,
            'Sources chronological Config last store loses its package/tag/copy association')
        if index in (0,2,4,6):require(V+site in call_map,'Sources Config copy omits its original child return')
        require(positions[_TAG_STORES[index]]<positions[site] and
            (index==0 or positions[_COMMIT[index-1][0]]<positions[_TAG_STORES[index]]),
            'Sources chronological Config prefix advances before its prior slot completed')
        lineages.append(dict(index=index,package=package,stages=tuple(stages),slot=slot))
    require(tuple(i for i in sorted(range(8),key=lambda i:positions[_CAPTURE[i][0]]))==_CAPTURE_ORDER and
        positions[5410]<positions[5414]<positions[5426]<positions[5430] and
        positions[4664]<positions[4670]<positions[4676],
        'Sources split byte crosses its original package/copy/tag epoch')
    return dict(normal_parent_path=tuple(V+off for off in path),source_calls=tuple(call_records),
        payload_lineage=tuple(lineages),capture_order=_CAPTURE_ORDER,
        pointer_epochs=tuple((V+a,cell,target,V+b) for a,cell,target,b in POINTER_EPOCHS),
        split_byte=(packages[0],V+5414,V+5426,V+4664),
        source_nonpadding_bytes=168,copied_padding_is_initialized=False,
        complete_ordinary_Source_prefix_issued=False,
        requires_actual_private_entry_source_control_and_reached_accesses=True)
