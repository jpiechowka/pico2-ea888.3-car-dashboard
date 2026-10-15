"""Finite catalogue/display children for grouped source-stage composition.

Bodies and source literals are closed current families. Typed enum arguments
must come from actual source constructors/decoded-success phases, never raw
bytes or UXTB alone. Owned effects do not establish those caller transports.
Padding/inactive payload writes are distinguished from initialized fields.
"""
import re
import struct
from config_finite import _complete_body
from config_logger import _logger_function,logger_body
from config_refill_contracts import REFILL_GRAMMARS,REFILL_ROLES,CATALOG_DISPATCH,CATALOG_ALTERNATIVES
from config_regions import Storage,View,Argument,Region,Transfer,borrow,require_disjoint,require_writes,audit_coverage,_control
from config_stack import canonical_symbol
from validation import require


def _readonly(objects,address,extent,alignment):
    require(type(address) is int and type(extent) is int and extent>0 and address%alignment==0 and
        0<address<address+extent<=0x100000000,'invalid refill readonly interval/alignment')
    owners=[]
    for section in objects.elf.sections:
        if not section[2]&2 or not section[5]:continue
        lo,end=section[3],section[3]+section[5]
        require(0<=lo<end<=0x100000000,'refill allocated section extent wraps')
        if lo<address+extent and address<end:owners.append(section)
    require(len(owners)==1 and owners[0][1]==1 and owners[0][2]&3==2 and
        owners[0][3]<=address<address+extent<=owners[0][3]+owners[0][5],
        'refill source is not uniquely file-backed readonly data')
    return objects.read_only(address,extent)


def _object(objects,address,descriptor):
    kind=descriptor['kind']
    if kind=='source_static_str':
        expected=bytes.fromhex(descriptor['utf8_hex'])
        require(descriptor['alignment']==1 and descriptor['extent']==len(expected) and bool(expected),
            'invalid source-static string descriptor')
        expected.decode('utf-8')
    else:
        require(kind=='channel_indexed_u16' and descriptor['alignment']==2 and descriptor['extent']==16 and
            descriptor['values']==[500,1500,1000,1000,1000,1000,500,500],'unreviewed typed Channel period table')
        expected=struct.pack('<8H',*descriptor['values'])
    require(_readonly(objects,address,descriptor['extent'],descriptor['alignment'])==expected,
        'refill source string/paired period values differ')
    return ()


def _function(objects,address,role):
    extent,frame,name=_logger_function(objects,address,role)
    if role in REFILL_ROLES:
        require((extent,frame) in {(REFILL_GRAMMARS[k]['extent'],REFILL_GRAMMARS[k]['frame']) for k in REFILL_ROLES[role]},
            'refill child current extent/frame differs')
    return extent,frame,name


def child_body(objects,name,instructions,outline_role=None,_ancestry=()):
    require(name in objects.elf.functions,'missing refill child')
    start=objects.elf.functions[name][0]&~1;require(start not in _ancestry,'refill outline cycle')
    role=canonical_symbol(name)
    if outline_role is not None:
        require(isinstance(outline_role,str) and outline_role.startswith('@outline:') and outline_role in REFILL_ROLES and
            re.fullmatch(r'OUTLINED_FUNCTION_[0-9]+',name),'unreviewed refill outline role/origin')
        role=outline_role
    require(role in REFILL_ROLES,'unreviewed refill child identity');results=[]
    for shape in REFILL_ROLES[role]:
        try:proof=_complete_body(objects,name,instructions,role,shape,REFILL_GRAMMARS[shape],_object,child_body,_ancestry,function_consumer=_function)
        except AssertionError:continue
        proof.update(requires_parent_frame_and_abi=shape if outline_role is not None else None,
            requires_typed_origin_owned_effects_and_child_closure=True)
        results.append(proof)
    require(len(results)==1,'missing, changed or ambiguous complete refill family')
    return results[0]


def child_coverage(objects,name,instructions,context,outline_role=None):
    proof=child_body(objects,name,instructions,outline_role);start=proof['function'];contract=REFILL_GRAMMARS[proof['shape']]
    children={start+e[0]:e for e in proof['ordinary_edges']};rows=[];padding=[];edges=[]
    for offset,width,text in contract['instructions']:
        at=start+offset;row=instructions[at]
        if text=='nop':padding.append(at);continue
        rows.append(at);family,conditional=_control(row)
        if family is None:continue
        operand=row.operands.split('@',1)[0].strip()
        if family=='return' or family=='bx' and operand=='lr':
            edges.append(Transfer(at,row.mnemonic,'conditional_return' if conditional else 'return',None,'matched refill restoration'))
        elif at in children:
            _,_,target,role,_=children[at];edges.append(Transfer(at,row.mnemonic,'call' if family=='bl' else 'tail',target,'closed refill child',role))
        elif family=='tbh':
            require(proof['shape'] in CATALOG_DISPATCH,'unreviewed refill table');dispatch=CATALOG_DISPATCH[proof['shape']]
            require(offset==dispatch['site'] and dispatch['table_offset']==offset+4 and
                (dispatch['width'],dispatch['count'],dispatch['input_domain'])==(2,16,list(range(16))),
                'refill table site/width/source-domain differs')
            halves=struct.unpack('<16H',objects.read_only(start+dispatch['table_offset'],32))
            targets=tuple(at+4+2*v for v in halves)
            require(targets==tuple(start+v for v in dispatch['target_offsets']),'refill table bytes and typed target vector differ')
            edges.append(Transfer(at,row.mnemonic,'table',targets,
                'all16 source-valid DefinitionId arms; caller origin still required'))
        else:
            target=re.search(r'0x([0-9a-f]+)(?:\s|$)',operand)
            require(target is not None and family in ('b','cbz','cbnz'),'unreviewed refill transfer')
            target=int(target[1],16);edges.append(Transfer(at,row.mnemonic,'loop' if target<=at else 'branch',target,'closed local refill branch/invariant'))
    # Catalog has an internal mapped TBH table, so preserve its two code
    # regions and the exact boundary fallthrough (the TBH itself terminates).
    regions=[]
    for lo,hi,kind in contract['mapping']:
        if kind=='d':regions.append(Region(context,'data',start+lo,start+hi,(),((start+lo,objects.read_only(start+lo,hi-lo)),),()))
        else:
            upper=min(padding) if padding and start+lo<=min(padding)<start+hi else start+hi
            local=tuple(a for a in rows if start+lo<=a<upper);transfers=tuple(e for e in edges if start+lo<=e.site<upper)
            regions.append(Region(context,'helper',start+lo,upper,local,(),transfers))
            if upper<start+hi:regions.append(Region(context,'padding',upper,start+hi,tuple(padding),(),()))
    def target_consumer(objects,target,role):
        if not role.startswith('@outline:'):return _function(objects,target,role)
        names=[n for n,e in objects.elf.functions.items() if e[0]&~1==target]
        require(len(names)==1,'ambiguous refill outline target');child=child_body(objects,names[0],instructions,role)
        return child['extent'],child['frame'],names[0]
    proof['coverage']=audit_coverage(objects,name,instructions,context,regions,_function_consumer=target_consumer)
    return proof


def _current(objects,instructions,proof):
    require(isinstance(proof,dict) and 'coverage' in proof,'refill effects lack complete body coverage')
    names=[n for n,e in objects.elf.functions.items() if e[0]&~1==proof['function'] and canonical_symbol(n)==proof['role']]
    require(len(names)==1,'refill effects have no unique typed body')
    current=child_coverage(objects,names[0],instructions,proof['coverage']['context'])
    require(current==proof,'refill effects use stale code/source/child facts')
    return current


def _domain(argument,kind,maximum):
    require(isinstance(argument,Argument) and (argument.kind,argument.view,argument.levels,argument.bits)==(kind,None,0,8) and
        argument.domain<=frozenset(range(maximum+1)),'refill enum/bool lacks its source-typed bounded domain')
    return tuple(sorted(argument.domain))


def _layout(proof,frame,ancestors):
    size,saved_offset={'catalog_definition_s':(52,16),'catalog_definition_z':(52,16),
        'minimum_period_z':(8,0),'advanced_for_channel_z':(72,48)}[proof['shape']]
    require(isinstance(frame,Storage) and frame.space.startswith('stack:') and frame.writable and frame.extent==size and
        frame.alignment>=4 and (frame.start+size)%8==0,'refill frame/current SP or incoming AAPCS alignment differs')
    ancestors=tuple(ancestors);require_disjoint((frame,)+ancestors)
    saved=Storage(frame.space,frame.identity+':saved',frame.start+saved_offset,size-saved_offset,4,frame.phase,False)
    return ancestors+(saved,)


def _output(proof,frame,output,extent,protected):
    require(isinstance(output,View) and output.writable and output.extent==extent and output.start%4==0 and
        output.storage.alignment>=4 and output.storage.space in (frame.space,'address') and output.storage.phase==frame.phase,
        'refill output has another owner/reference/phase/extent')
    own=Storage(output.storage.space,output.storage.identity+':output',output.start,output.extent,1,frame.phase,False)
    require_disjoint((frame,own))
    guards=[]
    for ordinal,descriptor in enumerate(REFILL_GRAMMARS[proof['shape']]['objects']):
        address=proof['objects'][ordinal];guards.append(Storage('address','refill source:'+str(address),address,descriptor['extent'],1,frame.phase,False))
    require_writes((output,),((output,0,extent),),protected+tuple(guards))


def _result(proof,frames,**fields):
    return dict(proof=proof,conservative_activation_frames=frames,
        normal_preserves=tuple('r'+str(i) for i in range(4,12))+('sp',),
        requires_actual_typed_origin_receiver_transport_and_parent_graph=True,**fields)


def definition_interface(objects,instructions,proof,identifier,output,frame,ancestor_protected):
    proof=_current(objects,instructions,proof);require(proof['shape'] in CATALOG_DISPATCH,'not a source catalogue constructor')
    domain=_domain(identifier,'DefinitionId',15);protected=_layout(proof,frame,ancestor_protected);_output(proof,frame,output,36,protected)
    scratch=borrow(frame,0,16,frame.phase,writable=True)
    require_writes((scratch,),((scratch,0,16),),protected+(Storage(output.storage.space,'live Definition',output.start,36,1,frame.phase,False),))
    outcomes=[];contract=REFILL_GRAMMARS[proof['shape']]
    for alternative in CATALOG_ALTERNATIVES:
        if alternative['id'] not in domain:continue
        strings=[]
        for source in alternative['strings']:
            if source['length']==0:pointers=(source['empty_pointer'],)
            else:
                pointers=tuple(sorted({proof['objects'][i] for i,d in enumerate(contract['objects']) if d['utf8_hex']==source['utf8_hex']}))
                require(pointers,'source Definition field lost its paired static string')
            strings.append(dict(source,pointers=pointers))
        initialized=((0,25),(30,5))+tuple((a,b-a) for a,b in alternative['access']['active_payload_ranges'])
        outcomes.append(dict(alternative,strings=tuple(strings),initialized_ranges=initialized))
    return _result(proof,(52,),outcomes=tuple(outcomes),possible_writes=((output,0,35),),local_writes=((scratch,0,16),))


def period_interface(objects,instructions,proof,channel,frame,ancestor_protected):
    proof=_current(objects,instructions,proof);require(proof['shape']=='minimum_period_z','not the typed period helper')
    domain=_domain(channel,'Channel',7);_layout(proof,frame,ancestor_protected)
    values=REFILL_GRAMMARS[proof['shape']]['objects'][0]['values']
    return _result(proof,(8,),outcomes=tuple(dict(channel=c,period_ms=values[c]) for c in domain),possible_writes=())


def opaque_copy_interface(objects,instructions,target,source,destination,count,frame,protected):
    """Reviewed restored-tail copy; copied padding remains opaque bytes."""
    require(isinstance(source,View) and not source.writable and isinstance(destination,View) and destination.writable and
        type(count) is int and 0<count==source.extent<=destination.extent and isinstance(frame,Storage) and
        frame.space.startswith('stack:') and frame.writable and frame.alignment>=8 and frame.start%8==0 and
        source.storage==frame and source.storage.phase==destination.storage.phase==frame.phase,
        'opaque copy lacks paired current caller-local source/views/count')
    protected=tuple(protected)
    live_source=Storage(source.storage.space,'opaque source',source.start,count,1,frame.phase,False)
    require_writes((destination,),((destination,0,count),),protected+(live_source,))
    copies=[];roles=('__aeabi_memcpy','_RNvNtCs_17compiler_builtins3mem6memcpy')
    for ordinal,role in enumerate(roles):
        names=[n for n,e in objects.elf.functions.items() if e[0]&~1==target and canonical_symbol(n)==role]
        require(len(names)==1,'memcpy child is not uniquely typed');bound=logger_body(objects,names[0],instructions);copies.append(bound)
        require(bound['frame']==(8 if ordinal==0 else 72),'memcpy conservative frame differs')
        child=Storage(frame.space,frame.identity+':copy'+str(ordinal),frame.start-bound['frame'],bound['frame'],4,frame.phase,True)
        for retained in protected+(live_source,):require_disjoint((child,retained))
        saves=Storage(child.space,child.identity+':complete child frame',child.start,child.extent,4,frame.phase,False)
        require_writes((destination,),((destination,0,destination.extent),),protected+(saves,))
        if ordinal==0:
            edges=bound['ordinary_edges'];require(len(edges)==1 and edges[0][3]==roles[1],'memcpy restored tail differs');target=edges[0][2]
        else:require(not bound['ordinary_edges'] and not bound['indirect_sites'],'memcpy has unresolved child/callback')
    return dict(children=tuple(copies),possible_writes=((destination,0,count),),
        conservative_activation_frames=(8,72),physical_activation_paths=((8,),(72,)),restored_tail_storage=True,
        normal_preserves=tuple('r'+str(i) for i in range(4,12))+('sp',),
        initialized_semantic_bytes_not_inferred=True,requires_actual_parent_pointer_count_transport=True)


def advanced_interface(objects,instructions,proof,channel,afr,output,frame,ancestor_protected):
    proof=_current(objects,instructions,proof);require(proof['shape']=='advanced_for_channel_z','not the AdvancedDisplay constructor')
    channels=_domain(channel,'Channel',7);flags=_domain(afr,'bool',1);protected=_layout(proof,frame,ancestor_protected)
    _output(proof,frame,output,64,protected)
    bands=borrow(frame,0,48,frame.phase,writable=True);require_writes((bands,),((bands,0,48),),protected)
    # Complete wrapper and builtin bodies are required, including the wrapper's
    # restored tail. Conservative accounting retains both8 and72 metadata.
    copy_role='__aeabi_memcpy'
    outline=proof['ordinary_edges'];require(len(outline)==1 and outline[0][0]==72,'AdvancedDisplay copy interface differs')
    names=[n for n,e in objects.elf.functions.items() if e[0]&~1==outline[0][2]];require(len(names)==1,'ambiguous bands48 outline')
    copy=child_body(objects,names[0],instructions,'@outline:advanced_copy_bands48')
    edges=copy['ordinary_edges'];require(len(edges)==1 and edges[0][3]==copy_role,'bands48 copy tail differs')
    copied=opaque_copy_interface(objects,instructions,edges[0][2],borrow(frame,0,48,frame.phase),output,48,frame,protected)
    outcomes=tuple(dict(channel=c,afr=a,readout=9 if c==7 and a else 7 if c==7 else 6 if c==6 else 2 if c==0 else 0)
        for c in channels for a in flags)
    return _result(proof,(72,8,72),outcomes=outcomes,possible_writes=((output,0,61),),initialized_ranges=((0,61),),
        physical_activation_paths=((72,8),(72,72)),restored_tail_storage=True,
        local_band_ranges=((0,16),(16,16),(32,16)),copy_children=copied['children'],
        target_access_requirements=(dict(operation='single_word_store',site=proof['function']+84,
            owner=output,coordinate_space=output.storage.space,phase=frame.phase,address=output.start+57,width=4,unaligned=True,
            owner_role='AdvancedDisplay.secondary/peak/reference/band_count',status='unresolved_object_store_policy'),))
