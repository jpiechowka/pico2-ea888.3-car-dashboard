"""Complete finite typed payloads; caller/source/default/sink closure is separate."""
import re
import struct
from config_payload_contracts import PAYLOAD_GRAMMARS,PAYLOAD_ROLES
from config_stack import canonical_symbol,typed_function
from llvm_stack import mapping_ranges
from validation import require


def payload_object(objects,address,descriptor):
    kind=descriptor['kind']
    if kind in ('literal','enum_lengths'):
        value=bytes.fromhex(descriptor['bytes']) if kind=='literal' else bytes(descriptor['lengths'])
        require(kind!='literal' or len(value)==descriptor['length'],'inconsistent payload literal contract')
        require(objects.read_only(address,len(value))==value,'payload label/ordered enum lengths differ')
        return ()
    if kind=='enum_pointers':
        require(address%4==0,'unaligned payload enum pointer table')
        labels=[v.encode() for v in descriptor['labels']]
        words=struct.unpack('<'+'I'*len(labels),objects.read_only(address,4*len(labels)))
        for pointer,label in zip(words,labels):
            require(objects.read_only(pointer,len(label))==label,'payload ordered enum label differs')
        return ()
    require(kind in ('debug_vtable','raw_debug_callback') and len(descriptor['roles'])==1,'unreviewed payload object')
    if kind=='debug_vtable':
        require(address%4==0,'unaligned payload Debug table')
        words=struct.unpack('<4I',objects.read_only(address,16))
        require(words[:3]==tuple(descriptor['layout']),'payload Debug table layout differs')
        pointer=words[3]
    else:pointer=address
    require(pointer&1,'payload callback is not Thumb')
    role=descriptor['roles'][0];extent,frame,_=typed_function(objects,pointer&~1,role)
    return ((pointer&~1,role,extent,frame),)


def _payload_shape(objects,name,instructions,role,shape,ancestry):
    contract=PAYLOAD_GRAMMARS[shape];start=objects.elf.functions[name][0]&~1
    extent,frame,_=typed_function(objects,start,canonical_symbol(name))
    require((extent,frame)==(contract['extent'],contract['frame']),'payload extent/frame differs')
    mapping=[(start+a,start+b,k) for a,b,k in contract['mapping']]
    require(mapping_ranges(objects.elf,name)==mapping,'payload code/data mapping differs')
    raw=objects.read_only(start,extent);rows=sorted((r for a,r in instructions.items() if start<=a<start+extent),key=lambda r:r.address)
    require(len(rows)==len(contract['instructions']),'payload instruction inventory differs')
    code={a for lo,hi,k in mapping if k=='t' for a in range(lo,hi)};covered=set();producers={};edges=[];indirect=[]
    for row,(offset,width,operation) in zip(rows,contract['instructions']):
        mnemonic,_,expected=operation.partition(' ');points=set(range(row.address,row.address+len(row.raw)))
        require(row.address==start+offset and len(row.raw)==width and points<=code and not points&covered and
                row.raw==raw[offset:offset+width],'payload current bytes/coverage differ')
        covered|=points;require(row.mnemonic==mnemonic,'payload source/return/selector instruction differs')
        actual=row.operands.split('@',1)[0].strip();producer=re.search(r'@object([0-9]+)\.(low|high)',expected)
        local=re.search(r'@self\+([0-9a-f]+)',expected)
        target_role=expected if expected.startswith(('@role:','@outline:')) else None
        if producer:
            match=re.fullmatch(r'(r[0-9]+|lr), #(0x[0-9a-f]+|[0-9]+)',actual)
            require(match is not None and int(match[2],0)<=65535,'invalid payload typed object producer')
            ordinal,part=int(producer[1]),producer[2]
            require(part not in producers.setdefault(ordinal,{}),'ambiguous payload object producer')
            producers[ordinal][part]=int(match[2],0);actual=match[1]+', '+producer[0]
        elif local:
            match=re.search(r'0x([0-9a-f]+)(?:\s|$)',actual)
            require(match is not None and int(match[1],16)==start+int(local[1],16),'payload case/predecessor destination differs')
            actual=actual[:match.start()]+local[0]
        elif target_role:
            match=re.match(r'0x([0-9a-f]+)(?:\s|$)',actual);require(match is not None,'unresolved payload ordinary target')
            target=int(match[1],16)
            if target_role.startswith('@outline:'):
                names=[n for n,e in objects.elf.functions.items() if e[0]&~1==target]
                require(names,'missing payload outline')
                child=payload_body(objects,names[0],instructions,target_role,ancestry+(start,));child_frame=child['frame']
                edge_role=target_role
            else:edge_role=target_role[6:];_,child_frame,_=typed_function(objects,target,edge_role)
            edges.append((offset,mnemonic,target,edge_role,child_frame));actual=target_role
        elif mnemonic in ('bx','blx') and actual!='lr':indirect.append((offset,mnemonic,actual))
        require(actual==expected,'payload reference/cell/ABI/default/return operands differ')
    require(covered==code,'incomplete payload mapped code')
    data={a for lo,hi,k in mapping if k=='d' for a in range(lo,hi)};covered=set();values={};pool_objects={}
    for offset,width,value in contract['data']:
        points=set(range(start+offset,start+offset+width))
        require(points<=data and not points&covered,'payload mapped data ownership differs');covered|=points
        if value.startswith('@object'):
            require(width==4 and (start+offset)%4==0,'invalid payload object pool extent')
            ordinal=int(value[7:]);require(ordinal not in values,'duplicate payload pool object')
            values[ordinal]=struct.unpack('<I',objects.read_only(start+offset,4))[0];pool_objects[start+offset]=ordinal
        else:require(raw[offset:offset+width].hex()==value,'payload fixed mapped bytes differ')
    require(covered==data,'unbound payload mapped data bytes')
    pool_reads=set()
    for row in rows:
        actual=row.operands.split('@',1)[0].strip();match=re.fullmatch(r'r[0-9]+, \[pc, #(0x[0-9a-f]+|[0-9]+)\]',actual)
        if match:
            pool=((row.address+4)&~3)+int(match[1],0)
            require(row.mnemonic.startswith('ldr') and pool in pool_objects,'unbound payload PC pool consumer')
            pool_reads.add(pool)
    require(pool_reads==set(pool_objects),'unconsumed payload object pool')
    for ordinal,pair in producers.items():
        require(set(pair)=={'low','high'} and ordinal not in values,'incomplete/duplicate payload object pair')
        values[ordinal]=pair['low']|(pair['high']<<16)
    require(set(values)==set(range(len(contract['objects']))),'missing/unreviewed payload typed object')
    callbacks=[]
    for ordinal,descriptor in enumerate(contract['objects']):callbacks.extend(payload_object(objects,values[ordinal],descriptor))
    return dict(shape=shape,role=role,function=start,extent=extent,frame=frame,objects=values,callbacks=tuple(callbacks),
                ordinary_edges=tuple(edges),indirect_sites=tuple(indirect),requires_value_source_and_live_sink=True,
                requires_options=(0x60000020,0,0),requires_pretty_inventory_exclusion=True,
                requires_parent_outline_abi=role if role.startswith('@outline:') else None)


def payload_body(objects,name,instructions,outline_role=None,_ancestry=()):
    """Select exactly one full typed body; never widen clones by demangled name."""
    require(name in objects.elf.functions,'missing payload body')
    start=objects.elf.functions[name][0]&~1;require(start not in _ancestry,'payload outline activation cycle')
    role=outline_role or canonical_symbol(name)
    require(not outline_role or re.fullmatch(r'OUTLINED_FUNCTION_[0-9]+',name),'payload outline origin differs')
    require(role in PAYLOAD_ROLES,'unreviewed exact payload receiver/context')
    results=[]
    for shape in PAYLOAD_ROLES[role]:
        try:results.append(_payload_shape(objects,name,instructions,role,shape,_ancestry))
        except AssertionError:continue
    require(len(results)==1,'missing, changed or ambiguous complete typed payload shape')
    return results[0]
