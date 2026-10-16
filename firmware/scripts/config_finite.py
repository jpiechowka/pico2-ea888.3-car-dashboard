"""Shared literal finite body matching; every caller/typed policy stays explicit."""
import re
import struct
from config_stack import canonical_symbol,typed_function
from llvm_stack import mapping_ranges
from validation import require
from config_structural import _instruction_items


def _complete_body(objects,name,instructions,role,shape,contract,object_consumer,outline_consumer,ancestry=(),function_consumer=typed_function):
    return _literal_body(objects,name,instructions,role,shape,contract,object_consumer,outline_consumer,ancestry,function_consumer=function_consumer)


def _bounded_body(objects,name,instructions,role,shape,contract,object_consumer,outline_consumer,start):
    """A reviewed terminal window; upstream/whole-caller proof stays required."""
    proof=_literal_body(objects,name,instructions,role,shape,contract,object_consumer,outline_consumer,(),start)
    proof.update(function=objects.elf.functions[name][0]&~1,caller_extent=objects.elf.functions[name][1])
    return proof


def _literal_body(objects,name,instructions,role,shape,contract,object_consumer,outline_consumer,ancestry=(),window=None,function_consumer=typed_function):
    """Literal byte/operand matcher for closed, reviewed internal grammars.

    No instruction execution, inferred CFG, runtime learning or public plugin
    selection. Consumers supply fixed typed policies and retain caller proof.
    """
    parent=objects.elf.functions[name][0]&~1
    parent_extent,frame,_=function_consumer(objects,parent,canonical_symbol(name))
    start=parent if window is None else window;extent=contract['extent']
    caller_mapping=mapping_ranges(objects.elf,name)
    if window is None:
        require((parent_extent,frame)==(extent,contract['frame']),'payload extent/frame differs')
        mapping=[(start+a,start+b,k) for a,b,k in contract['mapping']]
        require(caller_mapping==mapping,'payload code/data mapping differs')
    else:
        require(parent<=start<start+extent<=parent+parent_extent,'root window exceeds declared caller')
        mapping=[(max(start,a),min(start+extent,b),k) for a,b,k in caller_mapping if a<start+extent and start<b]
        if 'mapping' in contract:
            require(mapping==[(start+a,start+b,k) for a,b,k in contract['mapping']],
                'bounded source code/data partition differs')
        else:
            require(all(k=='t' for a,b,k in mapping) and sum(b-a for a,b,k in mapping)==extent,
                    'root window is not wholly mapped Thumb code')
    raw=objects.read_only(start,extent);rows=sorted((r for a,r in _instruction_items(objects,instructions,start,extent)),key=lambda r:r.address)
    require(len(rows)==len(contract['instructions']),'payload instruction inventory differs')
    code={a for lo,hi,k in mapping if k=='t' for a in range(lo,hi)};covered=set();producers={};edges=[];indirect=[];external_values={}
    for row,(offset,width,operation) in zip(rows,contract['instructions']):
        mnemonic,_,expected=operation.partition(' ');points=set(range(row.address,row.address+len(row.raw)))
        require(row.address==start+offset and len(row.raw)==width and points<=code and not points&covered and
                row.raw==raw[offset:offset+width],'payload current bytes/coverage differ')
        covered|=points;require(row.mnemonic==mnemonic,'payload source/return/selector instruction differs')
        actual=row.operands.split('@',1)[0].strip();producer=re.search(r'@object([0-9]+)\.(low|high)',expected)
        parent_local=re.search(r'@parent\+([0-9a-f]+)',expected)
        local=re.search(r'@self\+([0-9a-f]+)',expected);pool_producer=re.search(r'@object([0-9]+)\.pc_slot',expected)
        target_role=expected if expected.startswith(('@role:','@outline:')) else None
        if producer:
            match=re.fullmatch(r'(r[0-9]+|lr), #(0x[0-9a-f]+|[0-9]+)',actual)
            require(match is not None and int(match[2],0)<=65535,'invalid payload typed object producer')
            ordinal,part=int(producer[1]),producer[2]
            require(part not in producers.setdefault(ordinal,{}),'ambiguous payload object producer')
            producers[ordinal][part]=int(match[2],0);actual=match[1]+', '+producer[0]
        elif pool_producer:
            require(window is not None,'external PC pool only belongs to bounded root contract')
            match=re.fullmatch(r'(r[0-9]+), \[pc, #(0x[0-9a-f]+|[0-9]+)\]',actual)
            require(match is not None and mnemonic in ('ldr','ldr.w'),'invalid root PC pool producer')
            pool=((row.address+4)&~3)+int(match[2],0);ordinal=int(pool_producer[1])
            require(any(k=='d' and a<=pool<pool+4<=b for a,b,k in caller_mapping) and pool%4==0,
                    'root PC pool is not owned mapped caller data')
            value=struct.unpack('<I',objects.read_only(pool,4))[0]
            require(ordinal not in external_values,'duplicate root PC object producer')
            external_values[ordinal]=value;actual=match[1]+', [pc, '+pool_producer[0]+']'
        elif parent_local:
            target=parent+int(parent_local[1],16);destination=instructions.get(target)
            require(window is not None and parent<=target<parent+parent_extent and destination is not None and
                destination.address==target and len(destination.raw) in (2,4) and
                any(k=='t' and a<=target<target+len(destination.raw)<=b for a,b,k in caller_mapping) and
                objects.read_only(target,len(destination.raw))==destination.raw,
                'payload parent-relative target is not a current mapped instruction in its typed parent')
            match=re.search(r'0x([0-9a-f]+)(?:\s|$)',actual)
            require(match is not None and int(match[1],16)==target,'payload parent-relative destination differs')
            actual=actual[:match.start()]+parent_local[0]
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
                child=outline_consumer(objects,names[0],instructions,target_role,ancestry+(start,));child_frame=child['frame']
                edge_role=target_role
            else:edge_role=target_role[6:];_,child_frame,_=function_consumer(objects,target,edge_role)
            edges.append((offset,mnemonic,target,edge_role,child_frame));actual=target_role
        elif mnemonic in ('bx','blx') and actual!='lr':indirect.append((offset,mnemonic,actual))
        require(actual==expected,'payload reference/cell/ABI/default/return operands differ')
    require(covered==code,'incomplete payload mapped code')
    data={a for lo,hi,k in mapping if k=='d' for a in range(lo,hi)};covered=set();values=dict(external_values);pool_objects={};fixed_data=set()
    for offset,width,value in contract.get('data',()):
        points=set(range(start+offset,start+offset+width))
        require(points<=data and not points&covered,'payload mapped data ownership differs');covered|=points
        if value.startswith('@object'):
            require(width==4 and (start+offset)%4==0,'invalid payload object pool extent')
            ordinal=int(value[7:]);require(ordinal not in values,'duplicate payload pool object')
            values[ordinal]=struct.unpack('<I',objects.read_only(start+offset,4))[0];pool_objects[start+offset]=ordinal
        else:
            require(raw[offset:offset+width].hex()==value,'payload fixed mapped bytes differ')
            fixed_data|=points
    require(covered==data,'unbound payload mapped data bytes')
    pool_reads=set()
    for row in rows:
        actual=row.operands.split('@',1)[0].strip();match=re.fullmatch(r'(?:r[0-9]+|lr), \[pc, #(0x[0-9a-f]+|[0-9]+)\]',actual)
        if match and window is None:
            pool=((row.address+4)&~3)+int(match[1],0)
            require(row.mnemonic.startswith('ldr') and
                    (pool in pool_objects or set(range(pool,pool+4))<=fixed_data),'unbound payload PC pool consumer')
            if pool in pool_objects:pool_reads.add(pool)
    require(pool_reads==set(pool_objects),'unconsumed payload object pool')
    for ordinal,pair in producers.items():
        require(set(pair)=={'low','high'} and ordinal not in values,'incomplete/duplicate payload object pair')
        values[ordinal]=pair['low']|(pair['high']<<16)
    require(set(values)==set(range(len(contract['objects']))),'missing/unreviewed payload typed object')
    callbacks=[]
    for ordinal,descriptor in enumerate(contract['objects']):callbacks.extend(object_consumer(objects,values[ordinal],descriptor))
    return dict(shape=shape,role=role,function=start,extent=extent,frame=frame,objects=values,callbacks=tuple(callbacks),
                ordinary_edges=tuple(edges),indirect_sites=tuple(indirect),
                requires_parent_outline_abi=role if role.startswith('@outline:') else None)
